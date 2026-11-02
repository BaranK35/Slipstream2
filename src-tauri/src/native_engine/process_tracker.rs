//! Ultra-high performance process and port tracking for Custom EXE DPI bypass.
//! Maps TCP local ports to process executable names using atomic bitsets and
//! Windows Extended TCP Table (IPv4 + IPv6) with sub-nanosecond lookups.

use std::{
    collections::{HashMap, HashSet},
    ffi::c_void,
    path::Path,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Mutex,
    },
    thread,
    time::{Duration, Instant},
};

#[repr(C)]
#[derive(Copy, Clone)]
struct MIB_TCPROW_OWNER_PID {
    dw_state: u32,
    dw_local_addr: u32,
    dw_local_port: u32, // Network byte order in lower 16 bits
    dw_remote_addr: u32,
    dw_remote_port: u32,
    dw_owning_pid: u32,
}

#[repr(C)]
struct MIB_TCPTABLE_OWNER_PID {
    dw_num_entries: u32,
    table: [MIB_TCPROW_OWNER_PID; 1],
}

#[repr(C)]
#[derive(Copy, Clone)]
struct MIB_TCP6ROW_OWNER_PID {
    uc_local_addr: [u8; 16],
    dw_local_scope_id: u32,
    dw_local_port: u32, // Network byte order in lower 16 bits
    uc_remote_addr: [u8; 16],
    dw_remote_scope_id: u32,
    dw_remote_port: u32,
    dw_state: u32,
    dw_owning_pid: u32,
}

#[repr(C)]
struct MIB_TCP6TABLE_OWNER_PID {
    dw_num_entries: u32,
    table: [MIB_TCP6ROW_OWNER_PID; 1],
}

const AF_INET: u32 = 2;
const AF_INET6: u32 = 23;
const TCP_TABLE_OWNER_PID_ALL: u32 = 5;
const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;

#[link(name = "iphlpapi")]
extern "system" {
    fn GetExtendedTcpTable(
        pTcpTable: *mut c_void,
        pdwSize: *mut u32,
        bOrder: i32,
        ulAf: u32,
        TableClass: u32,
        Reserved: u32,
    ) -> u32;
}

#[link(name = "kernel32")]
extern "system" {
    fn OpenProcess(dwDesiredAccess: u32, bInheritHandle: i32, dwProcessId: u32) -> isize;
    fn QueryFullProcessImageNameW(
        hProcess: isize,
        dwFlags: u32,
        lpExeName: *mut u16,
        lpdwSize: *mut u32,
    ) -> i32;
    fn CloseHandle(hObject: isize) -> i32;
}

/// Tracks target custom executables and checks if a local TCP port belongs to one of them.
/// Uses an atomic bitset for 65536 ports to achieve sub-nanosecond check times.
pub struct ProcessTracker {
    target_exes: Vec<String>,
    // 65536 ports mapped to 1024 * 64 bits. If bit `port` is 1, it belongs to a target custom exe.
    port_bits: Arc<[AtomicU64; 1024]>,
    // Cache for pid -> (exe_name, expiry)
    pid_cache: Arc<Mutex<HashMap<u32, (String, Instant)>>>,
    // Known target PIDs cache
    target_pids: Arc<Mutex<HashSet<u32>>>,
    last_scan: Arc<Mutex<Instant>>,
    stop_signal: Arc<AtomicBool>,
    _worker_thread: Option<thread::JoinHandle<()>>,
}

impl ProcessTracker {
    pub fn new(custom_exes: &[String]) -> Self {
        let targets: Vec<String> = custom_exes
            .iter()
            .map(|s| {
                let trimmed = s.trim().to_lowercase();
                Path::new(&trimmed)
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or(&trimmed)
                    .to_string()
            })
            .filter(|s| !s.is_empty())
            .collect();

        // Safe allocation of 1024 AtomicU64 elements in Arc
        let port_bits: Arc<[AtomicU64; 1024]> = Arc::new(std::array::from_fn(|_| AtomicU64::new(0)));

        let pid_cache = Arc::new(Mutex::new(HashMap::new()));
        let target_pids = Arc::new(Mutex::new(HashSet::new()));
        let last_scan = Arc::new(Mutex::new(Instant::now().checked_sub(Duration::from_secs(10)).unwrap_or_else(Instant::now)));
        let stop_signal = Arc::new(AtomicBool::new(false));

        let mut tracker = Self {
            target_exes: targets,
            port_bits,
            pid_cache,
            target_pids,
            last_scan,
            stop_signal,
            _worker_thread: None,
        };

        if !tracker.is_empty() {
            // Initial synchronous scan
            tracker.refresh_ports_internal();

            // Background updater thread every 75ms
            let bg_targets = tracker.target_exes.clone();
            let bg_bits = Arc::clone(&tracker.port_bits);
            let bg_pids = Arc::clone(&tracker.pid_cache);
            let bg_target_pids = Arc::clone(&tracker.target_pids);
            let bg_last_scan = Arc::clone(&tracker.last_scan);
            let bg_stop = Arc::clone(&tracker.stop_signal);

            let handle = thread::Builder::new()
                .name("slipstream-proc-tracker".into())
                .spawn(move || {
                    while !bg_stop.load(Ordering::Relaxed) {
                        thread::sleep(Duration::from_millis(75));
                        if bg_stop.load(Ordering::Relaxed) {
                            break;
                        }
                        Self::scan_and_update(
                            &bg_targets,
                            &bg_bits,
                            &bg_pids,
                            &bg_target_pids,
                            &bg_last_scan,
                        );
                    }
                })
                .ok();

            tracker._worker_thread = handle;
        }

        tracker
    }

    pub fn is_empty(&self) -> bool {
        self.target_exes.is_empty()
    }

    /// Sub-nanosecond check if the local TCP port belongs to an executable in `custom_exes`.
    #[inline]
    pub fn is_custom_exe_port(&self, local_port: u16) -> bool {
        if self.target_exes.is_empty() {
            return false;
        }

        let idx = (local_port as usize) / 64;
        let bit = 1u64 << ((local_port as usize) % 64);

        // 1. Ultra-fast sub-nanosecond atomic bitset check (covers 99.9% of recurring packets)
        let word = self.port_bits[idx].load(Ordering::Relaxed);
        if (word & bit) != 0 {
            return true;
        }

        // 2. Port not yet in bitset (newly opened socket or parallel burst from browser).
        // Check TCP table directly without dropping parallel bursts, register port and sibling ports.
        self.check_and_register_port(local_port, idx, bit)
    }

    fn check_and_register_port(&self, local_port: u16, idx: usize, bit: u64) -> bool {
        let owning_pid = match self.lookup_pid_for_port(local_port) {
            Some(pid) => pid,
            None => return false,
        };

        let is_target = {
            if let Ok(guard) = self.target_pids.lock() {
                guard.contains(&owning_pid)
            } else {
                false
            }
        };

        let matches = if is_target {
            true
        } else {
            let m = Self::resolve_pid_match(owning_pid, &self.target_exes, &self.pid_cache);
            if m {
                if let Ok(mut guard) = self.target_pids.lock() {
                    guard.insert(owning_pid);
                }
            }
            m
        };

        if matches {
            // Set bit immediately for this newly confirmed custom exe port
            self.port_bits[idx].fetch_or(bit, Ordering::Relaxed);

            // Opportunistically trigger refresh of other sibling ports if last full scan was > 15ms ago
            let now = Instant::now();
            if let Ok(mut last_scan) = self.last_scan.try_lock() {
                if now.duration_since(*last_scan) > Duration::from_millis(15) {
                    *last_scan = now;
                    self.refresh_ports_internal();
                }
            }

            true
        } else {
            false
        }
    }

    fn refresh_ports_internal(&self) {
        Self::scan_and_update(
            &self.target_exes,
            &self.port_bits,
            &self.pid_cache,
            &self.target_pids,
            &self.last_scan,
        );
    }

    fn scan_and_update(
        targets: &[String],
        bits: &[AtomicU64; 1024],
        pid_cache: &Mutex<HashMap<u32, (String, Instant)>>,
        target_pids: &Mutex<HashSet<u32>>,
        last_scan: &Mutex<Instant>,
    ) {
        if targets.is_empty() {
            return;
        }

        if let Ok(mut ls) = last_scan.lock() {
            *ls = Instant::now();
        }

        let mut local_target_pids: HashSet<u32> = {
            if let Ok(guard) = target_pids.lock() {
                guard.clone()
            } else {
                HashSet::new()
            }
        };

        // Prune dead target PIDs to prevent PID recycling false-positives
        local_target_pids.retain(|&pid| {
            let h = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
            if h != 0 && h != -1 {
                unsafe { CloseHandle(h) };
                true
            } else {
                false
            }
        });

        // Temporary bitset array for new scan state
        let mut new_bits = [0u64; 1024];

        // 1. Scan IPv4 Extended TCP Table
        Self::scan_table_v4(targets, &mut local_target_pids, pid_cache, &mut new_bits);

        // 2. Scan IPv6 Extended TCP Table
        Self::scan_table_v6(targets, &mut local_target_pids, pid_cache, &mut new_bits);

        // Update target PIDs cache
        if let Ok(mut guard) = target_pids.lock() {
            *guard = local_target_pids;
        }

        // Atomically update bits (only modify changed words for cache coherence)
        for i in 0..1024 {
            let cur = bits[i].load(Ordering::Relaxed);
            let next = new_bits[i];
            if cur != next {
                bits[i].store(next, Ordering::Relaxed);
            }
        }
    }

    fn scan_table_v4(
        targets: &[String],
        target_pids: &mut HashSet<u32>,
        pid_cache: &Mutex<HashMap<u32, (String, Instant)>>,
        new_bits: &mut [u64; 1024],
    ) {
        let mut size = 0u32;
        unsafe {
            GetExtendedTcpTable(
                std::ptr::null_mut(),
                &mut size,
                0,
                AF_INET,
                TCP_TABLE_OWNER_PID_ALL,
                0,
            );
        }

        if size == 0 {
            return;
        }

        let mut buf = vec![0u8; size as usize];
        let ret = unsafe {
            GetExtendedTcpTable(
                buf.as_mut_ptr() as *mut c_void,
                &mut size,
                0,
                AF_INET,
                TCP_TABLE_OWNER_PID_ALL,
                0,
            )
        };

        if ret != 0 {
            return;
        }

        let table_ptr = buf.as_ptr() as *const MIB_TCPTABLE_OWNER_PID;
        let num_entries = unsafe { (*table_ptr).dw_num_entries as usize };
        let entries_ptr =
            unsafe { std::ptr::addr_of!((*table_ptr).table) as *const MIB_TCPROW_OWNER_PID };

        for i in 0..num_entries {
            let entry = unsafe { *entries_ptr.add(i) };
            let pid = entry.dw_owning_pid;
            if pid == 0 || pid == 4 {
                continue;
            }

            let is_match = if target_pids.contains(&pid) {
                true
            } else {
                let is_m = Self::resolve_pid_match(pid, targets, pid_cache);
                if is_m {
                    target_pids.insert(pid);
                }
                is_m
            };

            if is_match {
                let port = u16::from_be((entry.dw_local_port & 0xFFFF) as u16);
                let idx = (port as usize) / 64;
                let bit = 1u64 << ((port as usize) % 64);
                new_bits[idx] |= bit;
            }
        }
    }

    fn scan_table_v6(
        targets: &[String],
        target_pids: &mut HashSet<u32>,
        pid_cache: &Mutex<HashMap<u32, (String, Instant)>>,
        new_bits: &mut [u64; 1024],
    ) {
        let mut size = 0u32;
        unsafe {
            GetExtendedTcpTable(
                std::ptr::null_mut(),
                &mut size,
                0,
                AF_INET6,
                TCP_TABLE_OWNER_PID_ALL,
                0,
            );
        }

        if size == 0 {
            return;
        }

        let mut buf = vec![0u8; size as usize];
        let ret = unsafe {
            GetExtendedTcpTable(
                buf.as_mut_ptr() as *mut c_void,
                &mut size,
                0,
                AF_INET6,
                TCP_TABLE_OWNER_PID_ALL,
                0,
            )
        };

        if ret != 0 {
            return;
        }

        let table_ptr = buf.as_ptr() as *const MIB_TCP6TABLE_OWNER_PID;
        let num_entries = unsafe { (*table_ptr).dw_num_entries as usize };
        let entries_ptr =
            unsafe { std::ptr::addr_of!((*table_ptr).table) as *const MIB_TCP6ROW_OWNER_PID };

        for i in 0..num_entries {
            let entry = unsafe { *entries_ptr.add(i) };
            let pid = entry.dw_owning_pid;
            if pid == 0 || pid == 4 {
                continue;
            }

            let is_match = if target_pids.contains(&pid) {
                true
            } else {
                let is_m = Self::resolve_pid_match(pid, targets, pid_cache);
                if is_m {
                    target_pids.insert(pid);
                }
                is_m
            };

            if is_match {
                let port = u16::from_be((entry.dw_local_port & 0xFFFF) as u16);
                let idx = (port as usize) / 64;
                let bit = 1u64 << ((port as usize) % 64);
                new_bits[idx] |= bit;
            }
        }
    }

    fn resolve_pid_match(
        pid: u32,
        targets: &[String],
        pid_cache: &Mutex<HashMap<u32, (String, Instant)>>,
    ) -> bool {
        let exe_name = Self::query_exe_name_for_pid(pid, pid_cache);
        if let Some(ref name) = exe_name {
            let lower = name.to_lowercase();
            targets.iter().any(|target| {
                lower == *target
                    || lower == format!("{target}.exe")
                    || target == &format!("{lower}.exe")
            })
        } else {
            false
        }
    }

    pub fn get_exe_name_for_pid(&self, pid: u32) -> Option<String> {
        Self::query_exe_name_for_pid(pid, &self.pid_cache)
    }

    fn query_exe_name_for_pid(
        pid: u32,
        pid_cache: &Mutex<HashMap<u32, (String, Instant)>>,
    ) -> Option<String> {
        let now = Instant::now();

        if let Ok(guard) = pid_cache.lock() {
            if let Some((name, expiry)) = guard.get(&pid) {
                if now < *expiry {
                    return Some(name.clone());
                }
            }
        }

        let h_proc = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
        if h_proc == 0 || h_proc == -1 {
            return None;
        }

        let mut buf = [0u16; 1024];
        let mut size = buf.len() as u32;
        let success = unsafe { QueryFullProcessImageNameW(h_proc, 0, buf.as_mut_ptr(), &mut size) };
        unsafe { CloseHandle(h_proc) };

        if success == 0 || size == 0 {
            return None;
        }

        let full_path = String::from_utf16_lossy(&buf[..size as usize]);
        let exe_name = Path::new(&full_path)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(&full_path)
            .to_string();

        if let Ok(mut guard) = pid_cache.lock() {
            if guard.len() > 1024 {
                guard.clear();
            }
            guard.insert(pid, (exe_name.clone(), now + Duration::from_secs(15)));
        }

        Some(exe_name)
    }

    pub fn lookup_pid_for_port(&self, local_port: u16) -> Option<u32> {
        // 1. Try IPv4 TCP table
        if let Some(pid) = Self::lookup_pid_ipv4(local_port) {
            return Some(pid);
        }
        // 2. Fallback to IPv6 TCP table
        Self::lookup_pid_ipv6(local_port)
    }

    fn lookup_pid_ipv4(local_port: u16) -> Option<u32> {
        let mut size = 0u32;
        unsafe {
            GetExtendedTcpTable(
                std::ptr::null_mut(),
                &mut size,
                0,
                AF_INET,
                TCP_TABLE_OWNER_PID_ALL,
                0,
            );
        }

        if size == 0 {
            return None;
        }

        let mut buf = vec![0u8; size as usize];
        let ret = unsafe {
            GetExtendedTcpTable(
                buf.as_mut_ptr() as *mut c_void,
                &mut size,
                0,
                AF_INET,
                TCP_TABLE_OWNER_PID_ALL,
                0,
            )
        };

        if ret != 0 {
            return None;
        }

        let table_ptr = buf.as_ptr() as *const MIB_TCPTABLE_OWNER_PID;
        let num_entries = unsafe { (*table_ptr).dw_num_entries as usize };
        let entries_ptr =
            unsafe { std::ptr::addr_of!((*table_ptr).table) as *const MIB_TCPROW_OWNER_PID };

        for i in 0..num_entries {
            let entry = unsafe { *entries_ptr.add(i) };
            let port = u16::from_be((entry.dw_local_port & 0xFFFF) as u16);
            if port == local_port {
                return Some(entry.dw_owning_pid);
            }
        }

        None
    }

    fn lookup_pid_ipv6(local_port: u16) -> Option<u32> {
        let mut size = 0u32;
        unsafe {
            GetExtendedTcpTable(
                std::ptr::null_mut(),
                &mut size,
                0,
                AF_INET6,
                TCP_TABLE_OWNER_PID_ALL,
                0,
            );
        }

        if size == 0 {
            return None;
        }

        let mut buf = vec![0u8; size as usize];
        let ret = unsafe {
            GetExtendedTcpTable(
                buf.as_mut_ptr() as *mut c_void,
                &mut size,
                0,
                AF_INET6,
                TCP_TABLE_OWNER_PID_ALL,
                0,
            )
        };

        if ret != 0 {
            return None;
        }

        let table_ptr = buf.as_ptr() as *const MIB_TCP6TABLE_OWNER_PID;
        let num_entries = unsafe { (*table_ptr).dw_num_entries as usize };
        let entries_ptr =
            unsafe { std::ptr::addr_of!((*table_ptr).table) as *const MIB_TCP6ROW_OWNER_PID };

        for i in 0..num_entries {
            let entry = unsafe { *entries_ptr.add(i) };
            let port = u16::from_be((entry.dw_local_port & 0xFFFF) as u16);
            if port == local_port {
                return Some(entry.dw_owning_pid);
            }
        }

        None
    }
}

impl Drop for ProcessTracker {
    fn drop(&mut self) {
        self.stop_signal.store(true, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_process_tracker_empty_returns_false() {
        let tracker = ProcessTracker::new(&[]);
        assert!(tracker.is_empty());
        assert!(!tracker.is_custom_exe_port(80));
        assert!(!tracker.is_custom_exe_port(443));
    }

    #[test]
    fn test_process_tracker_normalizes_names() {
        let tracker = ProcessTracker::new(&[
            r"C:\Program Files\BraveSoftware\Brave-Browser\Application\brave.exe".to_string(),
            "discord".to_string(),
        ]);
        assert_eq!(tracker.target_exes, vec!["brave.exe", "discord"]);
    }

    #[test]
    fn test_process_tracker_current_pid_query() {
        let current_pid = std::process::id();
        let tracker = ProcessTracker::new(&[]);
        let exe_name = tracker.get_exe_name_for_pid(current_pid);
        assert!(exe_name.is_some(), "Must be able to query current process name");
        let name = exe_name.unwrap().to_lowercase();
        assert!(name.contains("slipstream") || name.contains("cargo") || name.ends_with(".exe"));
    }

    #[test]
    fn test_lookup_pid_for_bound_socket() {
        use std::net::TcpListener;
        let listener = TcpListener::bind("127.0.0.1:0").expect("Failed to bind");
        let port = listener.local_addr().unwrap().port();
        let tracker = ProcessTracker::new(&[]);
        let pid = tracker.lookup_pid_for_port(port);
        assert_eq!(pid, Some(std::process::id()));
    }

    #[test]
    fn test_process_tracker_with_active_socket() {
        use std::net::TcpListener;
        let listener = TcpListener::bind("127.0.0.1:0").expect("Failed to bind");
        let port = listener.local_addr().unwrap().port();

        // Get own executable name
        let tracker_tmp = ProcessTracker::new(&[]);
        let current_pid = std::process::id();
        let exe_name = tracker_tmp.get_exe_name_for_pid(current_pid).unwrap();

        // Create tracker targeting our own process
        let tracker = ProcessTracker::new(&[exe_name]);
        // Fast check should find our port
        let is_match = tracker.is_custom_exe_port(port);
        assert!(is_match, "Port belonging to target exe must match");
    }

    #[test]
    fn test_check_and_register_subsequent_socket() {
        use std::net::TcpListener;
        let tracker_tmp = ProcessTracker::new(&[]);
        let current_pid = std::process::id();
        let exe_name = tracker_tmp.get_exe_name_for_pid(current_pid).unwrap();

        let tracker = ProcessTracker::new(&[exe_name]);

        // Bind a new socket AFTER tracker was created
        let new_listener = TcpListener::bind("127.0.0.1:0").expect("Failed to bind new socket");
        let new_port = new_listener.local_addr().unwrap().port();

        // Must dynamically detect and register the new socket
        assert!(tracker.is_custom_exe_port(new_port), "Newly bound socket must be dynamically matched");
        // Second call should hit the atomic bitset cache in sub-nanoseconds
        assert!(tracker.is_custom_exe_port(new_port), "Cached bit must remain true");
    }
}

