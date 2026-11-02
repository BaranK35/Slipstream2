//! Slipstream Native DPI Packet Engine (`slipstream-core` / `native_engine`).
//! Provides in-process, high-performance packet interception, TLS/HTTP zero-copy parsing,
//! and advanced desync manipulation using direct WinDivert FFI bindings.

pub mod desync;
pub mod filter;
pub mod packet;
pub mod process_tracker;
pub mod tls;
pub mod windivert;

use std::{
    fs::OpenOptions,
    io::Write,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Mutex,
    },
    thread,
    time::Duration,
};

use desync::{apply_desync, DesyncParams};
use filter::DomainMatcher;
use packet::parse_packet;
use process_tracker::ProcessTracker;
use tls::{parse_http_request, parse_tls_client_hello};
use windivert::{
    WinDivertAddress, WinDivertApi, WinDivertHandle, WINDIVERT_LAYER_NETWORK,
    WINDIVERT_PARAM_QUEUE_LENGTH, WINDIVERT_PARAM_QUEUE_SIZE, WINDIVERT_PARAM_QUEUE_TIME,
};

#[derive(Debug, Default)]
pub struct EngineStats {
    pub packets_intercepted: AtomicU64,
    pub packets_desynced: AtomicU64,
    pub packets_bypassed: AtomicU64,
    pub bytes_processed: AtomicU64,
    pub errors: AtomicU64,
}

#[derive(Clone, Debug, Default, serde::Serialize)]
pub struct EngineStatsSnapshot {
    pub packets_intercepted: u64,
    pub packets_desynced: u64,
    pub packets_bypassed: u64,
    pub bytes_processed: u64,
    pub errors: u64,
}

#[derive(Clone, Debug)]
pub struct NativeEngineConfig {
    pub dll_path: Option<PathBuf>,
    pub hostlist: Vec<String>,
    pub custom_exes: Vec<String>,
    pub desync_params: DesyncParams,
    pub log_file: Option<PathBuf>,
}

pub struct NativeWorkerHandle {
    stop_signal: Arc<AtomicBool>,
    join_handle: Option<thread::JoinHandle<()>>,
    active_handle: Arc<Mutex<Option<Arc<WinDivertHandle>>>>,
    pub stats: Arc<EngineStats>,
}

impl NativeWorkerHandle {
    pub fn is_running(&self) -> bool {
        if self.stop_signal.load(Ordering::Relaxed) {
            return false;
        }
        if let Some(ref h) = self.join_handle {
            !h.is_finished()
        } else {
            false
        }
    }

    pub fn stats_snapshot(&self) -> EngineStatsSnapshot {
        EngineStatsSnapshot {
            packets_intercepted: self.stats.packets_intercepted.load(Ordering::Relaxed),
            packets_desynced: self.stats.packets_desynced.load(Ordering::Relaxed),
            packets_bypassed: self.stats.packets_bypassed.load(Ordering::Relaxed),
            bytes_processed: self.stats.bytes_processed.load(Ordering::Relaxed),
            errors: self.stats.errors.load(Ordering::Relaxed),
        }
    }

    /// Stops the worker thread cleanly, immediately unblocking any pending WinDivert recv.
    pub fn stop(&mut self) {
        self.stop_signal.store(true, Ordering::SeqCst);
        if let Ok(guard) = self.active_handle.lock() {
            if let Some(ref handle) = *guard {
                handle.close();
            }
        }
        if let Some(jh) = self.join_handle.take() {
            let _ = jh.join();
        }
    }
}

impl Drop for NativeWorkerHandle {
    fn drop(&mut self) {
        self.stop();
    }
}

#[link(name = "kernel32")]
extern "system" {
    fn SetThreadPriority(hThread: isize, nPriority: i32) -> i32;
    fn GetCurrentThread() -> isize;
}

/// Spawns the native packet engine worker on a high-priority background OS thread.
pub fn start_native_engine(config: NativeEngineConfig) -> Result<NativeWorkerHandle, String> {
    let dll_p = config.dll_path.clone();
    let api = WinDivertApi::load(dll_p.as_deref())?;

    // WinDivert filter: capture TCP 80 and 443 outbound non-loopback non-impostor.
    // Focusing purely on TCP guarantees zero interference with UDP gaming, Discord voice, and browser QUIC.
    let filter = "outbound && !loopback && !impostor && (tcp.DstPort == 80 || tcp.DstPort == 443)";
    let handle = Arc::new(api.open(filter, WINDIVERT_LAYER_NETWORK, 1000, 0)?);

    // Maximize WinDivert driver queue capacity and timeout to prevent dropped packets under heavy traffic
    handle.set_param(WINDIVERT_PARAM_QUEUE_LENGTH, 16384);
    handle.set_param(WINDIVERT_PARAM_QUEUE_TIME, 8000);
    handle.set_param(WINDIVERT_PARAM_QUEUE_SIZE, 33554432); // 32 MB buffer

    let stop_signal = Arc::new(AtomicBool::new(false));
    let stats = Arc::new(EngineStats::default());
    let active_handle = Arc::new(Mutex::new(Some(Arc::clone(&handle))));

    let thread_stop = Arc::clone(&stop_signal);
    let thread_stats = Arc::clone(&stats);
    let thread_handle = Arc::clone(&handle);
    let thread_active_slot = Arc::clone(&active_handle);

    let join_handle = thread::Builder::new()
        .name("slipstream-native-engine".into())
        .spawn(move || {
            // Set thread priority to THREAD_PRIORITY_HIGHEST (2) for sub-millisecond packet latency
            unsafe {
                SetThreadPriority(GetCurrentThread(), 2);
            }

            let matcher = DomainMatcher::from_domains(&config.hostlist);
            let process_tracker = ProcessTracker::new(&config.custom_exes);
            let mut log_writer = config.log_file.as_ref().and_then(|p| {
                OpenOptions::new().create(true).append(true).open(p).ok()
            });

            if let Some(ref mut w) = log_writer {
                let _ = writeln!(
                    w,
                    "[Slipstream Core] Native Rust DPI Engine started (Hostlist domains: {}, Custom EXEs: {}, Desync: {:?})",
                    matcher.len(),
                    config.custom_exes.len(),
                    config.desync_params
                );
            }

            let mut packet_buf = vec![0u8; 65536];
            let mut addr = WinDivertAddress::default();

            while !thread_stop.load(Ordering::Relaxed) {
                match thread_handle.recv(&mut packet_buf, &mut addr) {
                    Ok(len) => {
                        thread_stats.packets_intercepted.fetch_add(1, Ordering::Relaxed);
                        thread_stats.bytes_processed.fetch_add(len as u64, Ordering::Relaxed);

                        let raw = &packet_buf[..len];
                        if len < 20 {
                            let _ = thread_handle.send(raw, &addr);
                            thread_stats.packets_bypassed.fetch_add(1, Ordering::Relaxed);
                            continue;
                        }

                        let version = raw[0] >> 4;
                        let (proto, ihl, total_len) = if version == 4 {
                            let ihl = ((raw[0] & 0x0f) * 4) as usize;
                            if len < ihl {
                                let _ = thread_handle.send(raw, &addr);
                                thread_stats.packets_bypassed.fetch_add(1, Ordering::Relaxed);
                                continue;
                            }
                            let proto = raw[9];
                            let total_len = u16::from_be_bytes([raw[2], raw[3]]) as usize;
                            (proto, ihl, total_len)
                        } else if version == 6 {
                            if len < 40 {
                                let _ = thread_handle.send(raw, &addr);
                                thread_stats.packets_bypassed.fetch_add(1, Ordering::Relaxed);
                                continue;
                            }
                            let proto = raw[6];
                            let payload_len = u16::from_be_bytes([raw[4], raw[5]]) as usize;
                            (proto, 40, 40 + payload_len)
                        } else {
                            let _ = thread_handle.send(raw, &addr);
                            thread_stats.packets_bypassed.fetch_add(1, Ordering::Relaxed);
                            continue;
                        };

                        // Non-TCP protocols: pass through untouched in nanoseconds
                        if proto != 6 {
                            let _ = thread_handle.send(raw, &addr);
                            thread_stats.packets_bypassed.fetch_add(1, Ordering::Relaxed);
                            continue;
                        }

                        // === FAST PATH: TCP Traffic (Protocol 6) ===
                        if proto == 6 {
                            if len < ihl + 20 {
                                let _ = thread_handle.send(raw, &addr);
                                thread_stats.packets_bypassed.fetch_add(1, Ordering::Relaxed);
                                continue;
                            }

                            let tcp_data_offset = ((raw[ihl + 12] >> 4) * 4) as usize;
                            let header_len = ihl + tcp_data_offset;
                            let payload_len = total_len
                                .saturating_sub(header_len)
                                .min(len.saturating_sub(header_len));

                            // Pure TCP ACKs, SYN, FIN, RST, Window probes: zero payload.
                            // 90% of all TCP traffic has zero payload. Re-inject in 5ns without heap allocations!
                            if payload_len == 0 {
                                let _ = thread_handle.send(raw, &addr);
                                thread_stats.packets_bypassed.fetch_add(1, Ordering::Relaxed);
                                continue;
                            }

                            let src_port = u16::from_be_bytes([raw[ihl], raw[ihl + 1]]);
                            let dst_port = u16::from_be_bytes([raw[ihl + 2], raw[ihl + 3]]);
                            let payload = &raw[header_len..header_len + payload_len];

                            // Fast bypass for non-target ports
                            if dst_port != 80 && dst_port != 443 {
                                let _ = thread_handle.send(raw, &addr);
                                thread_stats.packets_bypassed.fetch_add(1, Ordering::Relaxed);
                                continue;
                            }

                            // Fast bypass for TLS Application Data (0x17) and non-ClientHello records
                            // (99.9% of TLS traffic is 0x17 application data)
                            if dst_port == 443
                                && (payload.len() < 47
                                    || payload[0] != 0x16
                                    || payload[1] != 0x03
                                    || payload[5] != 0x01)
                            {
                                let _ = thread_handle.send(raw, &addr);
                                thread_stats.packets_bypassed.fetch_add(1, Ordering::Relaxed);
                                continue;
                            }

                            // Fast bypass for non-HTTP request verbs on port 80
                            if dst_port == 80
                                && !(payload.starts_with(b"GET ")
                                    || payload.starts_with(b"POST ")
                                    || payload.starts_with(b"HEAD ")
                                    || payload.starts_with(b"CONNECT ")
                                    || payload.starts_with(b"PUT ")
                                    || payload.starts_with(b"OPTIONS "))
                            {
                                let _ = thread_handle.send(raw, &addr);
                                thread_stats.packets_bypassed.fetch_add(1, Ordering::Relaxed);
                                continue;
                            }

                            // === CANDIDATE PACKET: TLS ClientHello or HTTP Request ===
                            let is_custom_exe = process_tracker.is_custom_exe_port(src_port);
                            let mut matched_domain: Option<String> = None;
                            let mut is_tls = false;
                            let mut tls_info_holder = None;
                            let mut http_info_holder = None;

                            if dst_port == 443 {
                                if let Some(tls) = parse_tls_client_hello(payload) {
                                    let sni_str = tls.sni.unwrap_or("custom_exe_app");
                                    if is_custom_exe || matcher.matches(sni_str) {
                                        matched_domain = Some(sni_str.to_string());
                                        is_tls = true;
                                        tls_info_holder = Some(tls);
                                    }
                                } else if is_custom_exe {
                                    matched_domain = Some("custom_exe_app".to_string());
                                    is_tls = true;
                                }
                            } else if dst_port == 80 {
                                if let Some(http) = parse_http_request(payload) {
                                    if is_custom_exe || matcher.matches(http.host) {
                                        matched_domain = Some(http.host.to_string());
                                        http_info_holder = Some(http);
                                    }
                                } else if is_custom_exe {
                                    matched_domain = Some("custom_exe_app".to_string());
                                }
                            }

                            if let Some(domain) = matched_domain {
                                if let Some(parsed) = parse_packet(raw) {
                                    let injected = apply_desync(
                                        &parsed,
                                        tls_info_holder.as_ref(),
                                        http_info_holder.as_ref(),
                                        &config.desync_params,
                                    );

                                    if !injected.is_empty() {
                                        let mut send_addr = addr;
                                        send_addr.set_impostor(true);
                                        // Clear offload flags so network driver transmits corrupted/fake checksums as-is
                                        send_addr.set_ip_checksum(false);
                                        send_addr.set_tcp_checksum(false);
                                        send_addr.set_udp_checksum(false);

                                        for pkt in injected {
                                            let _ = thread_handle.send(&pkt, &send_addr);
                                        }

                                        thread_stats.packets_desynced.fetch_add(1, Ordering::Relaxed);

                                        if let Some(ref mut w) = log_writer {
                                            let proto_tag = if is_tls { "TLS SNI" } else { "HTTP Host" };
                                            let exe_tag = if is_custom_exe { " [Custom EXE Tunnel]" } else { "" };
                                            let _ = writeln!(
                                                w,
                                                "[Slipstream Core] Desync applied -> {proto_tag}: {domain}{exe_tag} (Dst: {}:{})",
                                                parsed.ip.dst_ip(),
                                                parsed.tcp.dst_port
                                            );
                                        }
                                        continue;
                                    }
                                }
                            }

                            // Candidate packet not matched or desync empty: reinject untouched
                            let _ = thread_handle.send(raw, &addr);
                            thread_stats.packets_bypassed.fetch_add(1, Ordering::Relaxed);
                            continue;
                        }

                        // Other protocols: reinject untouched
                        let _ = thread_handle.send(raw, &addr);
                        thread_stats.packets_bypassed.fetch_add(1, Ordering::Relaxed);
                    }
                    Err(code) => {
                        if thread_stop.load(Ordering::Relaxed) || code == 995 || code == 6 || code == 232 {
                            // 995 = ERROR_OPERATION_ABORTED, 6 = ERROR_INVALID_HANDLE, 232 = ERROR_NO_DATA
                            break;
                        }
                        thread_stats.errors.fetch_add(1, Ordering::Relaxed);
                        thread::sleep(Duration::from_millis(1));
                    }
                }
            }

            if let Some(ref mut w) = log_writer {
                let _ = writeln!(w, "[Slipstream Core] Native Rust DPI Engine stopped cleanly.");
            }

            // Cleanup handle on thread exit
            if let Ok(mut guard) = thread_active_slot.lock() {
                if let Some(h) = guard.take() {
                    h.close();
                }
            }
        })
        .map_err(|e| format!("İş parçacığı başlatılamadı: {e}"))?;

    Ok(NativeWorkerHandle {
        stop_signal,
        join_handle: Some(join_handle),
        active_handle,
        stats,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_native_engine_stats_defaults() {
        let stats = EngineStats::default();
        assert_eq!(stats.packets_intercepted.load(Ordering::Relaxed), 0);
        assert_eq!(stats.packets_desynced.load(Ordering::Relaxed), 0);
        assert_eq!(stats.packets_bypassed.load(Ordering::Relaxed), 0);
    }
}

#[cfg(test)]
mod tests_deep;
