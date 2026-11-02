//! Native Rust DPI desync mechanics: Split2, MultiSplit, Fake packet injection,
//! TTL manipulation, bad checksum evasion, MD5 signature fooling, and SeqOvl.

use super::{
    packet::{build_packet, ParsedPacket, TCP_ACK, TCP_PSH},
    tls::{HttpRequest, TlsClientHello},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SplitPosition {
    Fixed(usize),
    Sni,
    SniMid,
    SniExt,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FoolingMethod {
    Md5Sig,
    BadSum,
    BadSeq,
    LowTtl(u8),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DesyncParams {
    pub has_fake: bool,
    pub has_split2: bool,
    pub has_multisplit: bool,
    pub has_disorder2: bool,
    pub split_pos: SplitPosition,
    pub split_seqovl: usize,
    pub fooling: Vec<FoolingMethod>,
    pub ttl: u8,
    pub fake_repeats: usize,
    pub custom_fake_payload: Option<Vec<u8>>,
}

impl Default for DesyncParams {
    fn default() -> Self {
        Self {
            has_fake: true,
            has_split2: true,
            has_multisplit: false,
            has_disorder2: false,
            split_pos: SplitPosition::Fixed(2),
            split_seqovl: 0,
            fooling: vec![FoolingMethod::Md5Sig],
            ttl: 4,
            fake_repeats: 1,
            custom_fake_payload: None,
        }
    }
}

impl DesyncParams {
    /// Parses winws/zapret-style TLS arguments into native Rust DesyncParams.
    pub fn from_flags<S: AsRef<str>>(flags: &[S]) -> Self {
        let mut params = Self::default();
        params.fooling.clear();

        for f in flags {
            let s = f.as_ref();
            if let Some(val) = s.strip_prefix("--dpi-desync=") {
                let parts: Vec<&str> = val.split(',').collect();
                params.has_fake = parts.contains(&"fake");
                params.has_split2 = parts.contains(&"split2") || parts.contains(&"split");
                params.has_multisplit = parts.contains(&"multisplit") || parts.contains(&"multidisorder");
                params.has_disorder2 = parts.contains(&"disorder2") || parts.contains(&"multidisorder");
            } else if let Some(val) = s.strip_prefix("--dpi-desync-split-pos=") {
                match val {
                    "sni" => params.split_pos = SplitPosition::Sni,
                    "snimid" => params.split_pos = SplitPosition::SniMid,
                    "sniext" => params.split_pos = SplitPosition::SniExt,
                    num => {
                        if let Ok(n) = num.parse::<usize>() {
                            params.split_pos = SplitPosition::Fixed(n);
                        }
                    }
                }
            } else if let Some(val) = s.strip_prefix("--dpi-desync-split-seqovl=") {
                if let Ok(n) = val.parse::<usize>() {
                    params.split_seqovl = n;
                }
            } else if let Some(val) = s.strip_prefix("--dpi-desync-fooling=") {
                for m in val.split(',') {
                    match m {
                        "md5sig" => params.fooling.push(FoolingMethod::Md5Sig),
                        "badsum" => params.fooling.push(FoolingMethod::BadSum),
                        "badseq" => params.fooling.push(FoolingMethod::BadSeq),
                        _ => {}
                    }
                }
            } else if let Some(val) = s.strip_prefix("--dpi-desync-ttl=") {
                if let Ok(t) = val.parse::<u8>() {
                    params.ttl = t;
                    params.fooling.push(FoolingMethod::LowTtl(t));
                }
            } else if let Some(val) = s.strip_prefix("--dpi-desync-autottl=") {
                if let Ok(t) = val.parse::<u8>() {
                    params.ttl = t.max(1).min(8);
                    params.fooling.push(FoolingMethod::LowTtl(params.ttl));
                }
            } else if let Some(val) = s.strip_prefix("--dpi-desync-repeats=") {
                if let Ok(r) = val.parse::<usize>() {
                    params.fake_repeats = r.max(1);
                }
            }
        }

        // Default fooling if none was explicitly configured
        if params.fooling.is_empty() {
            params.fooling.push(FoolingMethod::Md5Sig);
        }

        params
    }
}

/// Generates a realistic decoy payload for fake packet injection.
pub fn generate_default_fake_payload(is_tls: bool) -> &'static [u8] {
    if is_tls {
        // Realistic dummy TLS 1.2 ClientHello header
        b"\x16\x03\x01\x00\x30\x01\x00\x00\x2c\x03\x03\x00\x01\x02\x03\x04\x05\x06\x07\x08\x09\x0a\x0b\x0c\x0d\x0e\x0f\x10\x11\x12\x13\x14\x15\x16\x17\x18\x19\x1a\x1b\x1c\x1d\x1e\x1f\x00\x00\x04\xc0\x2f\xc0\x30\x01\x00"
    } else {
        // Decoy HTTP GET request
        b"GET / HTTP/1.1\r\nHost: default.org\r\nUser-Agent: Mozilla/5.0\r\nAccept: */*\r\n\r\n"
    }
}

/// Determines the byte split position inside the TCP payload.
fn resolve_split_index(
    payload_len: usize,
    split_pos: SplitPosition,
    tls_info: Option<&TlsClientHello>,
    http_info: Option<&HttpRequest>,
) -> usize {
    if payload_len <= 1 {
        return 0;
    }

    let raw_split = match split_pos {
        SplitPosition::Fixed(pos) => pos,
        SplitPosition::Sni => {
            if let Some(t) = tls_info {
                t.sni_offset_in_payload.unwrap_or(2)
            } else if let Some(h) = http_info {
                h.host_offset_in_payload
            } else {
                2
            }
        }
        SplitPosition::SniMid => {
            if let Some(t) = tls_info {
                let off = t.sni_offset_in_payload.unwrap_or(2);
                let len = t.sni_len.unwrap_or(4);
                off + (len / 2).max(1)
            } else if let Some(h) = http_info {
                h.host_offset_in_payload + (h.host_len / 2).max(1)
            } else {
                2
            }
        }
        SplitPosition::SniExt => {
            if let Some(t) = tls_info {
                t.sni_ext_offset_in_payload.unwrap_or(2)
            } else {
                2
            }
        }
    };

    raw_split.clamp(1, payload_len - 1)
}

/// Executes desynchronization maneuvers on a captured TCP packet.
/// Returns a list of constructed raw IP+TCP packets to inject in sequence.
pub fn apply_desync(
    parsed: &ParsedPacket,
    tls_info: Option<&TlsClientHello>,
    http_info: Option<&HttpRequest>,
    params: &DesyncParams,
) -> Vec<Vec<u8>> {
    let payload = parsed.payload;
    if payload.is_empty() {
        return Vec::new();
    }

    let mut result = Vec::new();
    let is_tls = tls_info.is_some() || parsed.tcp.dst_port == 443;

    // 1. Fake Packet Injection
    if params.has_fake {
        let fake_payload_bytes = params
            .custom_fake_payload
            .as_deref()
            .unwrap_or_else(|| generate_default_fake_payload(is_tls));

        let mut fake_ip = parsed.ip.clone();
        let mut fake_tcp = parsed.tcp.clone();

        // Apply TTL fooling only if LowTtl was explicitly requested in fooling methods
        for f in &params.fooling {
            if let FoolingMethod::LowTtl(t) = f {
                fake_ip.set_ttl(*t);
                break;
            }
        }

        // Apply TCP MD5 signature option if requested
        if params.fooling.contains(&FoolingMethod::Md5Sig) {
            fake_tcp.append_md5_signature_option();
        }

        // Sequence number for fake packet
        if params.split_seqovl > 0 {
            fake_tcp.seq = fake_tcp.seq.wrapping_sub(params.split_seqovl as u32);
        }

        if params.fooling.contains(&FoolingMethod::BadSeq) {
            fake_tcp.seq = fake_tcp.seq.wrapping_sub(10000);
        }

        fake_tcp.flags = TCP_ACK | TCP_PSH;

        let corrupt_checksum = params.fooling.contains(&FoolingMethod::BadSum);
        let fake_raw = build_packet(fake_ip, fake_tcp, fake_payload_bytes, corrupt_checksum);

        for _ in 0..params.fake_repeats {
            result.push(fake_raw.clone());
        }
    }

    // 2. Real Segmenting: Split2 / MultiSplit / MultiDisorder / Disorder
    if params.has_multisplit && payload.len() >= 6 {
        // Multi-Split: 3 segments
        let split1 = resolve_split_index(payload.len(), SplitPosition::Fixed(1), tls_info, http_info);
        let split2 = resolve_split_index(payload.len(), params.split_pos, tls_info, http_info)
            .max(split1 + 1)
            .min(payload.len() - 1);

        let overlap = if params.split_seqovl > 0 {
            params.split_seqovl.min(split1).min(split2 - split1)
        } else {
            0
        };

        let seg1_data = &payload[..split1];
        let seg2_data = if overlap > 0 {
            &payload[split1 - overlap..split2]
        } else {
            &payload[split1..split2]
        };
        let seg3_data = &payload[split2..];

        let mut tcp1 = parsed.tcp.clone();
        tcp1.flags = TCP_ACK;
        let p1 = build_packet(parsed.ip.clone(), tcp1, seg1_data, false);

        let mut tcp2 = parsed.tcp.clone();
        tcp2.seq = parsed.tcp.seq.wrapping_add((split1 - overlap) as u32);
        tcp2.flags = TCP_ACK;
        let p2 = build_packet(parsed.ip.clone(), tcp2, seg2_data, false);

        let mut tcp3 = parsed.tcp.clone();
        tcp3.seq = parsed.tcp.seq.wrapping_add(split2 as u32);
        tcp3.flags = parsed.tcp.flags;
        let p3 = build_packet(parsed.ip.clone(), tcp3, seg3_data, false);

        if params.has_disorder2 {
            // Multi-Disorder: send segment 3, then segment 2, then segment 1
            result.push(p3);
            result.push(p2);
            result.push(p1);
        } else {
            result.push(p1);
            result.push(p2);
            result.push(p3);
        }
    } else if params.has_split2 || params.has_disorder2 {
        let split_idx = resolve_split_index(payload.len(), params.split_pos, tls_info, http_info);
        let overlap = if params.split_seqovl > 0 {
            params.split_seqovl.min(split_idx).min(payload.len() - split_idx)
        } else {
            0
        };

        let seg1_data = &payload[..split_idx];
        let seg2_data = if overlap > 0 {
            &payload[split_idx - overlap..]
        } else {
            &payload[split_idx..]
        };

        let mut tcp1 = parsed.tcp.clone();
        tcp1.flags = TCP_ACK;
        let p1 = build_packet(parsed.ip.clone(), tcp1, seg1_data, false);

        let mut tcp2 = parsed.tcp.clone();
        tcp2.seq = parsed.tcp.seq.wrapping_add((split_idx - overlap) as u32);
        tcp2.flags = parsed.tcp.flags;
        let p2 = build_packet(parsed.ip.clone(), tcp2, seg2_data, false);

        if params.has_disorder2 {
            // Out-of-order segment injection: send Segment 2 first, then Segment 1
            result.push(p2);
            result.push(p1);
        } else {
            result.push(p1);
            result.push(p2);
        }
    } else {
        // Just fake packet + original intact real packet
        let p_real = build_packet(parsed.ip.clone(), parsed.tcp.clone(), payload, false);
        result.push(p_real);
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::native_engine::packet::{parse_packet, IpHeader, Ipv4Header, TcpHeader, IPPROTO_TCP};

    #[test]
    fn test_desync_params_from_flags() {
        let flags = vec![
            "--dpi-desync=fake,split2".to_string(),
            "--dpi-desync-split-pos=2".to_string(),
            "--dpi-desync-split-seqovl=1".to_string(),
            "--dpi-desync-fooling=md5sig,badsum".to_string(),
            "--dpi-desync-autottl=3".to_string(),
        ];
        let params = DesyncParams::from_flags(&flags);
        assert!(params.has_fake);
        assert!(params.has_split2);
        assert!(!params.has_multisplit);
        assert_eq!(params.split_pos, SplitPosition::Fixed(2));
        assert_eq!(params.split_seqovl, 1);
        assert!(params.fooling.contains(&FoolingMethod::Md5Sig));
        assert!(params.fooling.contains(&FoolingMethod::BadSum));
        assert_eq!(params.ttl, 3);
    }

    #[test]
    fn test_split2_reconstruction_integrity() {
        let ip = IpHeader::V4(Ipv4Header {
            ihl: 20,
            dscp_ecn: 0,
            total_len: 0,
            id: 555,
            flags_fragment: 0x4000,
            ttl: 64,
            protocol: IPPROTO_TCP,
            checksum: 0,
            src_ip: [192, 168, 1, 50],
            dst_ip: [104, 26, 10, 1],
            options: Vec::new(),
        });
        let tcp = TcpHeader {
            src_port: 50123,
            dst_port: 443,
            seq: 10000,
            ack: 20000,
            data_offset: 20,
            flags: TCP_ACK | TCP_PSH,
            window_size: 65535,
            checksum: 0,
            urgent_ptr: 0,
            options: Vec::new(),
        };

        let orig_payload = b"\x16\x03\x01\x00\x20ClientHelloPayloadTestDomainDiscordCom";
        let raw = build_packet(ip, tcp, orig_payload, false);
        let parsed = parse_packet(&raw).expect("Must parse");

        let params = DesyncParams {
            has_fake: false,
            has_split2: true,
            has_multisplit: false,
            has_disorder2: false,
            split_pos: SplitPosition::Fixed(2),
            split_seqovl: 0,
            fooling: vec![],
            ttl: 4,
            fake_repeats: 1,
            custom_fake_payload: None,
        };

        let packets = apply_desync(&parsed, None, None, &params);
        assert_eq!(packets.len(), 2, "Split2 must produce exactly 2 segments");

        let parsed1 = parse_packet(&packets[0]).expect("Seg 1 parse");
        let parsed2 = parse_packet(&packets[1]).expect("Seg 2 parse");

        assert_eq!(parsed1.payload.len(), 2);
        assert_eq!(parsed1.tcp.seq, 10000);
        assert_eq!(parsed1.payload, &orig_payload[..2]);

        assert_eq!(parsed2.payload.len(), orig_payload.len() - 2);
        assert_eq!(parsed2.tcp.seq, 10002);
        assert_eq!(parsed2.payload, &orig_payload[2..]);

        // Reassembled payload check
        let mut reassembled = Vec::new();
        reassembled.extend_from_slice(parsed1.payload);
        reassembled.extend_from_slice(parsed2.payload);
        assert_eq!(reassembled, orig_payload);
    }

    #[test]
    fn test_fake_packet_injection_with_md5sig_and_low_ttl() {
        let ip = IpHeader::V4(Ipv4Header {
            ihl: 20,
            dscp_ecn: 0,
            total_len: 0,
            id: 999,
            flags_fragment: 0x4000,
            ttl: 64,
            protocol: IPPROTO_TCP,
            checksum: 0,
            src_ip: [192, 168, 1, 50],
            dst_ip: [104, 26, 10, 1],
            options: Vec::new(),
        });
        let tcp = TcpHeader {
            src_port: 50123,
            dst_port: 443,
            seq: 5000,
            ack: 6000,
            data_offset: 20,
            flags: TCP_ACK | TCP_PSH,
            window_size: 65535,
            checksum: 0,
            urgent_ptr: 0,
            options: Vec::new(),
        };

        let orig_payload = b"\x16\x03\x01\x00\x10RealData";
        let raw = build_packet(ip, tcp, orig_payload, false);
        let parsed = parse_packet(&raw).expect("Must parse");

        let params = DesyncParams {
            has_fake: true,
            has_split2: true,
            has_multisplit: false,
            has_disorder2: false,
            split_pos: SplitPosition::Fixed(2),
            split_seqovl: 0,
            fooling: vec![FoolingMethod::Md5Sig, FoolingMethod::LowTtl(3)],
            ttl: 3,
            fake_repeats: 1,
            custom_fake_payload: None,
        };

        let packets = apply_desync(&parsed, None, None, &params);
        assert_eq!(packets.len(), 3, "Fake + Split2 must produce 3 packets (1 fake + 2 segments)");

        let fake_parsed = parse_packet(&packets[0]).expect("Fake packet parse");
        assert_eq!(fake_parsed.ip.ttl(), 3, "Fake packet TTL must be set to low TTL");
        // Verify MD5 signature option exists in TCP options
        assert!(
            fake_parsed.tcp.options.contains(&19),
            "Fake packet must contain TCP Option 19 (MD5 Signature)"
        );
    }

    #[test]
    fn test_fake_packet_injection_with_md5sig_preserves_full_ttl() {
        let ip = IpHeader::V4(Ipv4Header {
            ihl: 20,
            dscp_ecn: 0,
            total_len: 0,
            id: 999,
            flags_fragment: 0x4000,
            ttl: 64,
            protocol: IPPROTO_TCP,
            checksum: 0,
            src_ip: [192, 168, 1, 50],
            dst_ip: [162, 159, 138, 232],
            options: Vec::new(),
        });
        let tcp = TcpHeader {
            src_port: 50123,
            dst_port: 443,
            seq: 5000,
            ack: 6000,
            data_offset: 20,
            flags: TCP_ACK | TCP_PSH,
            window_size: 65535,
            checksum: 0,
            urgent_ptr: 0,
            options: Vec::new(),
        };

        let orig_payload = b"\x16\x03\x01\x00\x10RealData";
        let raw = build_packet(ip, tcp, orig_payload, false);
        let parsed = parse_packet(&raw).expect("Must parse");

        let params = DesyncParams {
            has_fake: true,
            has_split2: true,
            has_multisplit: false,
            has_disorder2: false,
            split_pos: SplitPosition::Fixed(2),
            split_seqovl: 0,
            fooling: vec![FoolingMethod::Md5Sig], // No LowTtl!
            ttl: 4,
            fake_repeats: 1,
            custom_fake_payload: None,
        };

        let packets = apply_desync(&parsed, None, None, &params);
        let fake_parsed = parse_packet(&packets[0]).expect("Fake packet parse");
        assert_eq!(fake_parsed.ip.ttl(), 64, "Fake packet must retain original TTL (64) so it reaches DPI and server");
        assert!(fake_parsed.tcp.options.contains(&19), "Fake packet must have MD5 signature option");
    }
}
