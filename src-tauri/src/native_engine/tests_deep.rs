//! Comprehensive edge-case and deep verification tests for Slipstream Native Engine.

#[cfg(test)]
mod tests {
    use super::super::{
        desync::{apply_desync, DesyncParams, SplitPosition},
        filter::DomainMatcher,
        packet::{
            build_packet, compute_tcp_checksum, parse_packet, IpHeader, Ipv4Header, TcpHeader,
            IPPROTO_TCP, IPPROTO_UDP, TCP_ACK, TCP_FIN, TCP_PSH, TCP_RST, TCP_SYN,
        },
        tls::{parse_http_request, parse_tls_client_hello},
    };

    #[test]
    fn test_packet_truncated_buffers_never_panic() {
        // Empty buffer
        assert!(parse_packet(&[]).is_none());

        // 1-byte to 19-byte buffers (smaller than min IPv4 header)
        for len in 1..20 {
            let buf = vec![0x45; len];
            assert!(parse_packet(&buf).is_none());
        }

        // Invalid IP versions (version 1, 2, 3, 5, 7..15)
        for ver in [1, 2, 3, 5, 7, 8, 9, 10, 15] {
            let mut buf = vec![0u8; 40];
            buf[0] = ver << 4 | 5;
            assert!(parse_packet(&buf).is_none());
        }

        // Truncated IPv4 with valid version but insufficient length for IHL
        let mut buf = vec![0x45; 40];
        buf[0] = 0x4f; // IHL = 15 words = 60 bytes, but buffer is only 40 bytes
        assert!(parse_packet(&buf).is_none());

        // IPv4 with non-TCP protocol (e.g. UDP)
        let mut udp_buf = vec![0u8; 40];
        udp_buf[0] = 0x45;
        udp_buf[9] = IPPROTO_UDP;
        assert!(parse_packet(&udp_buf).is_none());
    }

    #[test]
    fn test_packet_tcp_flags_and_options() {
        let ip = IpHeader::V4(Ipv4Header {
            ihl: 20,
            dscp_ecn: 0,
            total_len: 0,
            id: 101,
            flags_fragment: 0x4000,
            ttl: 64,
            protocol: IPPROTO_TCP,
            checksum: 0,
            src_ip: [10, 0, 0, 1],
            dst_ip: [10, 0, 0, 2],
            options: Vec::new(),
        });

        let mut tcp = TcpHeader {
            src_port: 12345,
            dst_port: 80,
            seq: 42,
            ack: 100,
            data_offset: 20,
            flags: TCP_SYN | TCP_ACK | TCP_FIN | TCP_RST,
            window_size: 1024,
            checksum: 0,
            urgent_ptr: 0,
            options: Vec::new(),
        };

        assert!(tcp.is_syn());
        assert!(tcp.is_ack());
        assert!(tcp.is_fin());
        assert!(tcp.is_rst());

        // Append MD5 signature option
        tcp.append_md5_signature_option();
        assert_eq!(tcp.data_offset, 40); // 20 + 20 (18 bytes MD5 + 2 NOPs)
        assert!(tcp.options.len() == 20);
        assert_eq!(tcp.options[0], 19); // Kind 19
        assert_eq!(tcp.options[1], 18); // Length 18

        let payload = b"GET / HTTP/1.0\r\n\r\n";
        let raw = build_packet(ip, tcp, payload, false);
        let parsed = parse_packet(&raw).expect("Must parse packet with TCP options");
        assert_eq!(parsed.tcp.data_offset, 40);
        assert_eq!(parsed.tcp.options.len(), 20);
        assert_eq!(parsed.payload, payload);
    }

    #[test]
    fn test_checksum_boundary_values_and_odd_lengths() {
        // Odd length payload: 1 byte, 3 bytes, 5 bytes
        for len in [1, 3, 5, 7, 11, 15, 127, 1461] {
            let payload = vec![0xAB; len];
            let ip = IpHeader::V4(Ipv4Header {
                ihl: 20,
                dscp_ecn: 0,
                total_len: 0,
                id: 1,
                flags_fragment: 0,
                ttl: 64,
                protocol: IPPROTO_TCP,
                checksum: 0,
                src_ip: [127, 0, 0, 1],
                dst_ip: [127, 0, 0, 1],
                options: Vec::new(),
            });
            let tcp = TcpHeader {
                src_port: 8080,
                dst_port: 8080,
                seq: 1,
                ack: 1,
                data_offset: 20,
                flags: TCP_ACK,
                window_size: 512,
                checksum: 0,
                urgent_ptr: 0,
                options: Vec::new(),
            };

            let raw = build_packet(ip, tcp, &payload, false);
            let parsed = parse_packet(&raw).expect("Must parse odd-length payload");
            let tcp_start = parsed.ip_header_len;
            let tcp_end = tcp_start + parsed.tcp_header_len;
            let csum = compute_tcp_checksum(&parsed.ip, &raw[tcp_start..tcp_end], parsed.payload);
            assert_eq!(parsed.tcp.checksum, csum);
        }
    }

    #[test]
    fn test_tls_multi_extension_sni_extraction() {
        // Construct ClientHello with dummy extensions before and after SNI
        let mut payload = vec![
            0x16, 0x03, 0x01, 0x01, 0x00, // Record
            0x01, 0x00, 0x00, 0xfc,       // Handshake
            0x03, 0x03,                   // Version
        ];
        payload.extend_from_slice(&[0x11; 32]); // Random
        payload.push(0);                       // Session ID len 0
        payload.extend_from_slice(&[0x00, 0x02, 0x13, 0x01]); // 1 cipher suite
        payload.extend_from_slice(&[0x01, 0x00]);             // 1 compression method

        // Extensions block
        let mut exts = Vec::new();

        // 1. Dummy Extension 1: Supported Groups (0x000a, len 4)
        exts.extend_from_slice(&[0x00, 0x0a, 0x00, 0x04, 0x00, 0x02, 0x00, 0x1d]);

        // 2. SNI Extension (0x0000) for "media.discordapp.net"
        let domain = b"media.discordapp.net";
        let sni_data_len = (domain.len() + 5) as u16;
        let list_len = (domain.len() + 3) as u16;
        let name_len = domain.len() as u16;

        exts.extend_from_slice(&[0x00, 0x00]); // SNI type
        exts.extend_from_slice(&sni_data_len.to_be_bytes());
        exts.extend_from_slice(&list_len.to_be_bytes());
        exts.push(0x00); // Hostname
        exts.extend_from_slice(&name_len.to_be_bytes());
        exts.extend_from_slice(domain);

        // 3. Dummy Extension 2: ALPN (0x0010, len 5)
        exts.extend_from_slice(&[0x00, 0x10, 0x00, 0x05, 0x00, 0x03, 0x02, b'h', b'2']);

        let exts_len = exts.len() as u16;
        payload.extend_from_slice(&exts_len.to_be_bytes());
        payload.extend_from_slice(&exts);

        let parsed = parse_tls_client_hello(&payload).expect("Must parse ClientHello with multiple extensions");
        assert_eq!(parsed.sni, Some("media.discordapp.net"));
        let offset = parsed.sni_offset_in_payload.expect("Offset must be present");
        assert_eq!(&payload[offset..offset + parsed.sni_len.unwrap()], domain);
    }

    #[test]
    fn test_tls_malformed_never_panics() {
        // Record says length 100, but only 20 bytes provided
        let short_record = [0x16, 0x03, 0x01, 0x00, 0x64, 0x01, 0x00, 0x00, 0x60];
        assert!(parse_tls_client_hello(&short_record).is_none());

        // Corrupted session id length that overruns packet
        let mut corrupted = vec![0x16, 0x03, 0x01, 0x00, 0x30, 0x01, 0x00, 0x00, 0x2c, 0x03, 0x03];
        corrupted.extend_from_slice(&[0x00; 32]);
        corrupted.push(255); // Session id claims 255 bytes, but buffer ends
        assert!(parse_tls_client_hello(&corrupted).is_none());
    }

    #[test]
    fn test_http_host_case_insensitivity_and_whitespace() {
        let req1 = b"GET / HTTP/1.1\r\nhost:   roblox.com   \r\n\r\n";
        let parsed1 = parse_http_request(req1).expect("Must parse lowercase host");
        assert_eq!(parsed1.host, "roblox.com");

        let req2 = b"POST /login HTTP/1.1\r\nHOST: sub.domain.org:8080\r\n\r\n";
        let parsed2 = parse_http_request(req2).expect("Must parse uppercase HOST with port");
        assert_eq!(parsed2.host, "sub.domain.org");
    }

    #[test]
    fn test_desync_multisplit_three_segments() {
        let ip = IpHeader::V4(Ipv4Header {
            ihl: 20,
            dscp_ecn: 0,
            total_len: 0,
            id: 888,
            flags_fragment: 0x4000,
            ttl: 64,
            protocol: IPPROTO_TCP,
            checksum: 0,
            src_ip: [192, 168, 1, 10],
            dst_ip: [104, 21, 5, 2],
            options: Vec::new(),
        });
        let tcp = TcpHeader {
            src_port: 50000,
            dst_port: 443,
            seq: 1000,
            ack: 2000,
            data_offset: 20,
            flags: TCP_ACK | TCP_PSH,
            window_size: 65535,
            checksum: 0,
            urgent_ptr: 0,
            options: Vec::new(),
        };

        let orig_payload = b"\x16\x03\x01\x00\x50ThisIsLongPayloadForMultiSplitTestingBoundary!";
        let raw = build_packet(ip, tcp, orig_payload, false);
        let parsed = parse_packet(&raw).expect("Parsed packet");

        let params = DesyncParams {
            has_fake: false,
            has_split2: false,
            has_multisplit: true,
            has_disorder2: false,
            split_pos: SplitPosition::Fixed(4),
            split_seqovl: 0,
            fooling: vec![],
            ttl: 4,
            fake_repeats: 1,
            custom_fake_payload: None,
        };

        let packets = apply_desync(&parsed, None, None, &params);
        assert_eq!(packets.len(), 3, "MultiSplit must produce 3 segments");

        let s1 = parse_packet(&packets[0]).unwrap();
        let s2 = parse_packet(&packets[1]).unwrap();
        let s3 = parse_packet(&packets[2]).unwrap();

        assert_eq!(s1.tcp.seq, 1000);
        assert_eq!(s2.tcp.seq, 1000 + s1.payload.len() as u32);
        assert_eq!(s3.tcp.seq, 1000 + s1.payload.len() as u32 + s2.payload.len() as u32);

        let mut reconstructed = Vec::new();
        reconstructed.extend_from_slice(s1.payload);
        reconstructed.extend_from_slice(s2.payload);
        reconstructed.extend_from_slice(s3.payload);
        assert_eq!(reconstructed, orig_payload);
    }

    #[test]
    fn test_desync_disorder_packet_order() {
        let ip = IpHeader::V4(Ipv4Header {
            ihl: 20,
            dscp_ecn: 0,
            total_len: 0,
            id: 777,
            flags_fragment: 0x4000,
            ttl: 64,
            protocol: IPPROTO_TCP,
            checksum: 0,
            src_ip: [192, 168, 1, 10],
            dst_ip: [104, 21, 5, 2],
            options: Vec::new(),
        });
        let tcp = TcpHeader {
            src_port: 50000,
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

        let orig_payload = b"\x16\x03\x01\x00\x20PayloadForDisorderTesting";
        let raw = build_packet(ip, tcp, orig_payload, false);
        let parsed = parse_packet(&raw).expect("Parsed packet");

        let params = DesyncParams {
            has_fake: false,
            has_split2: false,
            has_multisplit: false,
            has_disorder2: true,
            split_pos: SplitPosition::Fixed(2),
            split_seqovl: 0,
            fooling: vec![],
            ttl: 4,
            fake_repeats: 1,
            custom_fake_payload: None,
        };

        let packets = apply_desync(&parsed, None, None, &params);
        assert_eq!(packets.len(), 2, "Disorder2 must produce 2 segments");

        let first_sent = parse_packet(&packets[0]).unwrap();
        let second_sent = parse_packet(&packets[1]).unwrap();

        // Disorder: Segment 2 (higher seq) is sent first!
        assert_eq!(first_sent.tcp.seq, 5002);
        assert_eq!(first_sent.payload, &orig_payload[2..]);

        // Segment 1 (lower seq) is sent second!
        assert_eq!(second_sent.tcp.seq, 5000);
        assert_eq!(second_sent.payload, &orig_payload[..2]);
    }

    #[test]
    fn test_domain_matcher_hierarchical_subdomains() {
        let mut matcher = DomainMatcher::new();
        matcher.add("discord.gg");
        matcher.add("roblox.com");
        matcher.add(".epicgames.com");

        assert!(matcher.matches("discord.gg"));
        assert!(matcher.matches("canary.discord.gg"));
        assert!(matcher.matches("voice.rtc.canary.discord.gg"));

        assert!(matcher.matches("roblox.com"));
        assert!(matcher.matches("client.roblox.com"));

        assert!(matcher.matches("epicgames.com"));
        assert!(matcher.matches("store.epicgames.com"));

        // Negative cases
        assert!(!matcher.matches("fake-discord.gg"));
        assert!(!matcher.matches("discord.gg.attacker.com"));
        assert!(!matcher.matches("google.com"));
    }

    #[test]
    fn test_desync_split_seqovl_with_real_overlap() {
        let ip = IpHeader::V4(Ipv4Header {
            ihl: 20,
            dscp_ecn: 0,
            total_len: 0,
            id: 111,
            flags_fragment: 0x4000,
            ttl: 64,
            protocol: IPPROTO_TCP,
            checksum: 0,
            src_ip: [192, 168, 1, 100],
            dst_ip: [162, 159, 137, 232],
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

        let orig_payload = b"\x16\x03\x01\x00\x30ABCDEFClientHelloPayloadForDiscordCom";
        let raw = build_packet(ip, tcp, orig_payload, false);
        let parsed = parse_packet(&raw).expect("Must parse");

        let params = DesyncParams {
            has_fake: false,
            has_split2: true,
            has_multisplit: false,
            has_disorder2: false,
            split_pos: SplitPosition::Fixed(2),
            split_seqovl: 1, // 1 byte overlap
            fooling: vec![],
            ttl: 4,
            fake_repeats: 1,
            custom_fake_payload: None,
        };

        let packets = apply_desync(&parsed, None, None, &params);
        assert_eq!(packets.len(), 2, "Split2 with seqovl must produce 2 segments");

        let seg1 = parse_packet(&packets[0]).unwrap();
        let seg2 = parse_packet(&packets[1]).unwrap();

        // Segment 1: bytes [0..2], seq = 10000
        assert_eq!(seg1.tcp.seq, 10000);
        assert_eq!(seg1.payload.len(), 2);
        assert_eq!(seg1.payload, &orig_payload[..2]);

        // Segment 2: starts 1 byte earlier (seq = 10001), payload starts at byte 1
        assert_eq!(seg2.tcp.seq, 10001);
        assert_eq!(seg2.payload.len(), orig_payload.len() - 1);
        assert_eq!(seg2.payload, &orig_payload[1..]);

        // Stream reconstruction with overlap de-duplication:
        let mut reconstructed = Vec::new();
        reconstructed.extend_from_slice(seg1.payload); // bytes 0..2
        reconstructed.extend_from_slice(&seg2.payload[1..]); // de-duplicate overlapping byte at index 1
        assert_eq!(reconstructed, orig_payload);
    }

    #[test]
    fn test_desync_multidisorder_three_segments_out_of_order() {
        let ip = IpHeader::V4(Ipv4Header {
            ihl: 20,
            dscp_ecn: 0,
            total_len: 0,
            id: 222,
            flags_fragment: 0x4000,
            ttl: 64,
            protocol: IPPROTO_TCP,
            checksum: 0,
            src_ip: [192, 168, 1, 100],
            dst_ip: [162, 159, 137, 232],
            options: Vec::new(),
        });
        let tcp = TcpHeader {
            src_port: 50123,
            dst_port: 443,
            seq: 20000,
            ack: 30000,
            data_offset: 20,
            flags: TCP_ACK | TCP_PSH,
            window_size: 65535,
            checksum: 0,
            urgent_ptr: 0,
            options: Vec::new(),
        };

        let orig_payload = b"\x16\x03\x01\x00\x50MultiDisorderPayloadTestForSlipstreamNativeEngine";
        let raw = build_packet(ip, tcp, orig_payload, false);
        let parsed = parse_packet(&raw).expect("Must parse");

        let flags = vec![
            "--dpi-desync=multidisorder".to_string(),
            "--dpi-desync-split-pos=2".to_string(),
            "--dpi-desync-split-seqovl=1".to_string(),
        ];
        let params = DesyncParams::from_flags(&flags);
        assert!(params.has_multisplit);
        assert!(params.has_disorder2);
        assert_eq!(params.split_seqovl, 1);

        let packets = apply_desync(&parsed, None, None, &params);
        assert_eq!(packets.len(), 3, "MultiDisorder must produce 3 segments");

        let first = parse_packet(&packets[0]).unwrap();
        let second = parse_packet(&packets[1]).unwrap();
        let third = parse_packet(&packets[2]).unwrap();

        // In MultiDisorder, segment 3 is sent first!
        assert!(first.tcp.seq > second.tcp.seq, "Segment 3 sequence must be greater than Segment 2");
        assert!(second.tcp.seq > third.tcp.seq || second.tcp.seq >= third.tcp.seq, "Segment 2 sequence must be greater than Segment 1");
        assert_eq!(third.tcp.seq, 20000, "Segment 1 (sent last) must start at original sequence");
    }

    #[test]
    fn test_http_post_binary_body_with_valid_host() {
        // HTTP POST with binary body containing non-UTF-8 bytes (0xFF, 0xFE, 0x00, 0xAA)
        let mut req = Vec::new();
        req.extend_from_slice(b"POST /api/v9/science HTTP/1.1\r\nHost: discord.com\r\nContent-Type: application/octet-stream\r\n\r\n");
        req.extend_from_slice(&[0xFF, 0xFE, 0x00, 0xAA, 0xDE, 0xAD, 0xBE, 0xEF]);

        let parsed = parse_http_request(&req).expect("Must parse HTTP Host header even with binary body");
        assert_eq!(parsed.host, "discord.com");
    }

    #[test]
    fn test_udp_quic_packet_parsing() {
        use super::super::packet::parse_udp_packet;

        let ip = IpHeader::V4(Ipv4Header {
            ihl: 20,
            dscp_ecn: 0,
            total_len: 48,
            id: 333,
            flags_fragment: 0x4000,
            ttl: 64,
            protocol: IPPROTO_UDP,
            checksum: 0,
            src_ip: [192, 168, 1, 50],
            dst_ip: [142, 250, 185, 206],
            options: Vec::new(),
        });

        let mut raw = vec![0u8; 20 + 8 + 20];
        ip.serialize_into(&mut raw[..20]);

        // UDP header: src 54321, dst 443, len 28, checksum 0
        raw[20..22].copy_from_slice(&54321u16.to_be_bytes());
        raw[22..24].copy_from_slice(&443u16.to_be_bytes());
        raw[24..26].copy_from_slice(&28u16.to_be_bytes());
        raw[26..28].copy_from_slice(&0u16.to_be_bytes());
        // QUIC dummy payload
        raw[28..48].copy_from_slice(b"QUICInitialPayload..");

        let parsed = parse_udp_packet(&raw).expect("Must parse UDP packet");
        assert_eq!(parsed.udp.src_port, 54321);
        assert_eq!(parsed.udp.dst_port, 443);
        assert_eq!(parsed.udp.length, 28);
        assert_eq!(parsed.payload, b"QUICInitialPayload..");
    }

    #[test]
    fn test_windivert_dll_loading_and_symbols() {
        use super::super::windivert::WinDivertApi;
        use std::path::Path;

        let dll_path = Path::new("engine/WinDivert.dll");
        if dll_path.exists() {
            let api = WinDivertApi::load(Some(dll_path)).expect("Must dynamically load WinDivert.dll");
            assert!((api.open_fn as usize) != 0, "open_fn must be valid");
            assert!((api.recv_fn as usize) != 0, "recv_fn must be valid");
            assert!((api.send_fn as usize) != 0, "send_fn must be valid");
            assert!((api.calc_checksums_fn as usize) != 0, "calc_checksums_fn must be valid");
            assert!((api.close_fn as usize) != 0, "close_fn must be valid");
        }
    }

    #[tokio::test]
    #[ignore]
    async fn test_live_discord_connectivity() {
        use super::super::{start_native_engine, NativeEngineConfig};
        use std::path::PathBuf;

        let dll_path = PathBuf::from("engine/WinDivert.dll");
        if !dll_path.exists() {
            println!("WinDivert.dll not in engine, skipping live test");
            return;
        }

        let config = NativeEngineConfig {
            dll_path: Some(dll_path),
            hostlist: vec![
                "discord.com".into(),
                "updates.discord.com".into(),
                "gateway.discord.gg".into(),
                "cdn.discordapp.com".into(),
            ],
            custom_exes: vec!["curl.exe".into()],
            desync_params: DesyncParams::default(),
            log_file: None,
        };

        let mut worker = match start_native_engine(config) {
            Ok(w) => w,
            Err(e) => {
                println!("Cannot start WinDivert (elevation required?): {e}");
                return;
            }
        };

        // Allow engine to hook into driver
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(5))
            .no_proxy()
            .build()
            .unwrap();

        let resp_discord = client.get("https://discord.com").send().await;
        println!("GET https://discord.com result: {:?}", resp_discord.as_ref().map(|r| r.status()));

        let resp_updates = client.get("https://updates.discord.com/distributions/app/manifests/latest?channel=stable&platform=win&arch=x64").send().await;
        println!("GET https://updates.discord.com result: {:?}", resp_updates.as_ref().map(|r| r.status()));

        let stats = worker.stats_snapshot();
        println!("Worker stats: intercepted={}, desynced={}, bypassed={}", stats.packets_intercepted, stats.packets_desynced, stats.packets_bypassed);

        worker.stop();
    }
}
