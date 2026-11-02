//! Pure safe, zero-copy TLS ClientHello SNI and HTTP Host header parser.

pub const TLS_RECORD_HANDSHAKE: u8 = 0x16;
pub const TLS_HANDSHAKE_CLIENT_HELLO: u8 = 0x01;
pub const TLS_EXTENSION_SNI: u16 = 0x0000;
pub const SNI_NAME_TYPE_HOSTNAME: u8 = 0x00;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TlsClientHello<'a> {
    pub sni: Option<&'a str>,
    pub sni_offset_in_payload: Option<usize>,
    pub sni_len: Option<usize>,
    pub sni_ext_offset_in_payload: Option<usize>,
}

/// Parses a TCP payload to check if it contains a TLS ClientHello,
/// returning the Server Name Indication (SNI) domain name if present.
pub fn parse_tls_client_hello(payload: &[u8]) -> Option<TlsClientHello<'_>> {
    // Minimum TLS record (5) + handshake (4) + version (2) + random (32) + session_id (1) + cipher_suites (2) + compression (1) = 47 bytes
    if payload.len() < 47 {
        return None;
    }

    // 1. Verify TLS Record Header
    if payload[0] != TLS_RECORD_HANDSHAKE {
        return None;
    }
    // Version 3.1 (TLS 1.0) to 3.4 (TLS 1.3 draft/final)
    if payload[1] != 0x03 || payload[2] > 0x04 {
        return None;
    }
    let record_len = u16::from_be_bytes([payload[3], payload[4]]) as usize;
    if record_len == 0 || payload.len() < 5 + record_len.min(42) {
        return None;
    }

    // 2. Verify Handshake Header
    if payload[5] != TLS_HANDSHAKE_CLIENT_HELLO {
        return None;
    }
    let handshake_len = ((payload[6] as usize) << 16) | ((payload[7] as usize) << 8) | (payload[8] as usize);
    if handshake_len == 0 {
        return None;
    }

    // 3. Skip Client Version (2 bytes) + Client Random (32 bytes)
    let mut cursor = 5 + 4 + 2 + 32;
    if cursor >= payload.len() {
        return None;
    }

    // 4. Skip Session ID
    let session_id_len = payload[cursor] as usize;
    cursor += 1 + session_id_len;
    if cursor + 2 > payload.len() {
        return None;
    }

    // 5. Skip Cipher Suites
    let cipher_suites_len = u16::from_be_bytes([payload[cursor], payload[cursor + 1]]) as usize;
    cursor += 2 + cipher_suites_len;
    if cursor >= payload.len() {
        return None;
    }

    // 6. Skip Compression Methods
    let comp_methods_len = payload[cursor] as usize;
    if cursor + 1 + comp_methods_len > payload.len() {
        return None;
    }
    cursor += 1 + comp_methods_len;
    if cursor + 2 > payload.len() {
        // No extensions present
        return Some(TlsClientHello {
            sni: None,
            sni_offset_in_payload: None,
            sni_len: None,
            sni_ext_offset_in_payload: None,
        });
    }

    // 7. Parse Extensions
    let extensions_len = u16::from_be_bytes([payload[cursor], payload[cursor + 1]]) as usize;
    cursor += 2;
    let ext_end = cursor.saturating_add(extensions_len).min(payload.len());

    while cursor + 4 <= ext_end {
        let ext_type = u16::from_be_bytes([payload[cursor], payload[cursor + 1]]);
        let ext_len = u16::from_be_bytes([payload[cursor + 2], payload[cursor + 3]]) as usize;
        let ext_offset = cursor;
        cursor += 4;

        if cursor.saturating_add(ext_len) > ext_end {
            break;
        }

        if ext_type == TLS_EXTENSION_SNI {
            // Parse SNI extension data
            // server_name_list_length: 2 bytes
            if ext_len >= 5 {
                let list_len = u16::from_be_bytes([payload[cursor], payload[cursor + 1]]) as usize;
                if list_len + 2 <= ext_len {
                    let mut sni_cursor = cursor + 2;
                    let sni_end = (cursor + 2).saturating_add(list_len).min(cursor + ext_len);
                    while sni_cursor + 3 <= sni_end {
                        let name_type = payload[sni_cursor];
                        let name_len = u16::from_be_bytes([payload[sni_cursor + 1], payload[sni_cursor + 2]]) as usize;
                        sni_cursor += 3;
                        if name_type == SNI_NAME_TYPE_HOSTNAME && sni_cursor.saturating_add(name_len) <= sni_end {
                            let name_bytes = &payload[sni_cursor..sni_cursor + name_len];
                            if let Ok(sni_str) = std::str::from_utf8(name_bytes) {
                                return Some(TlsClientHello {
                                    sni: Some(sni_str),
                                    sni_offset_in_payload: Some(sni_cursor),
                                    sni_len: Some(name_len),
                                    sni_ext_offset_in_payload: Some(ext_offset),
                                });
                            }
                        }
                        sni_cursor = sni_cursor.saturating_add(name_len);
                    }
                }
            }
        }

        cursor = cursor.saturating_add(ext_len);
    }

    Some(TlsClientHello {
        sni: None,
        sni_offset_in_payload: None,
        sni_len: None,
        sni_ext_offset_in_payload: None,
    })
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HttpRequest<'a> {
    pub host: &'a str,
    pub host_offset_in_payload: usize,
    pub host_len: usize,
}

/// Parses an HTTP/1.x plaintext request and extracts the `Host:` header domain.
/// Robust against binary POST/PUT bodies and arbitrary header casing.
pub fn parse_http_request(payload: &[u8]) -> Option<HttpRequest<'_>> {
    if payload.len() < 16 {
        return None;
    }

    // Check for HTTP verbs
    let starts_with_verb = payload.starts_with(b"GET ")
        || payload.starts_with(b"POST ")
        || payload.starts_with(b"HEAD ")
        || payload.starts_with(b"PUT ")
        || payload.starts_with(b"DELETE ")
        || payload.starts_with(b"OPTIONS ")
        || payload.starts_with(b"CONNECT ");

    if !starts_with_verb {
        return None;
    }

    // Isolate only the HTTP headers section (up to \r\n\r\n or \n\n) to allow binary request bodies
    let header_end = payload
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .map(|pos| pos + 4)
        .or_else(|| payload.windows(2).position(|w| w == b"\n\n").map(|p| p + 2))
        .unwrap_or(payload.len());

    let header_bytes = &payload[..header_end];
    let text = std::str::from_utf8(header_bytes).ok()?;

    let mut line_offset = 0;
    for line in text.split('\n') {
        let trimmed_line = line.trim_end_matches('\r');
        let lower = trimmed_line.to_ascii_lowercase();
        if let Some(rest) = lower.strip_prefix("host:") {
            let host_start_in_line = trimmed_line.len() - rest.len();
            let host_raw = trimmed_line[host_start_in_line..].trim();
            let host = host_raw.split(':').next().unwrap_or(host_raw).trim();
            if !host.is_empty() {
                // Find byte offset in payload
                if let Some(pos) = trimmed_line.find(host) {
                    let host_offset = line_offset + pos;
                    return Some(HttpRequest {
                        host,
                        host_offset_in_payload: host_offset,
                        host_len: host.len(),
                    });
                }
            }
        }
        line_offset += line.len() + 1; // + \n
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_real_tls_client_hello_with_sni() {
        // Real captured TLS 1.2 ClientHello for discord.com
        let mut packet = vec![
            0x16, 0x03, 0x01, 0x00, 0x5a, // TLS Record: Handshake, TLS 1.0, Len: 90
            0x01, 0x00, 0x00, 0x56,       // Handshake: ClientHello, Len: 86
            0x03, 0x03,                   // Version: TLS 1.2
        ];
        // 32 bytes random
        packet.extend_from_slice(&[0xaa; 32]);
        // Session ID length 0
        packet.push(0x00);
        // Cipher suites length 4 (2 suites)
        packet.extend_from_slice(&[0x00, 0x04, 0xc0, 0x2f, 0xc0, 0x30]);
        // Compression methods length 1 (1 method: null)
        packet.extend_from_slice(&[0x01, 0x00]);

        // Extensions
        let sni_domain = b"discord.com";
        let mut sni_ext = vec![
            0x00, 0x00, // Extension Type: SNI (0x0000)
            0x00, (sni_domain.len() + 5) as u8, // Ext len
            0x00, (sni_domain.len() + 3) as u8, // Server Name List len
            0x00,                               // Name Type: Hostname (0x00)
            0x00, sni_domain.len() as u8,       // Name len
        ];
        sni_ext.extend_from_slice(sni_domain);

        let ext_len = sni_ext.len() as u16;
        packet.extend_from_slice(&ext_len.to_be_bytes());
        packet.extend_from_slice(&sni_ext);

        let parsed = parse_tls_client_hello(&packet).expect("Must parse TLS ClientHello");
        assert_eq!(parsed.sni, Some("discord.com"));
        assert!(parsed.sni_offset_in_payload.is_some());
        let sni_offset = parsed.sni_offset_in_payload.unwrap();
        assert_eq!(&packet[sni_offset..sni_offset + parsed.sni_len.unwrap()], b"discord.com");
    }

    #[test]
    fn test_parse_http_get_request() {
        let req = b"GET /v1/gateway HTTP/1.1\r\nHost: gateway.discord.gg\r\nUser-Agent: curl/7.88.1\r\nAccept: */*\r\n\r\n";
        let parsed = parse_http_request(req).expect("Must parse HTTP GET");
        assert_eq!(parsed.host, "gateway.discord.gg");
    }

    #[test]
    fn test_parse_http_with_port() {
        let req = b"POST /api/v9 HTTP/1.1\r\nHost: discord.com:443\r\nContent-Length: 0\r\n\r\n";
        let parsed = parse_http_request(req).expect("Must parse HTTP with port");
        assert_eq!(parsed.host, "discord.com");
    }

    #[test]
    fn test_non_tls_traffic() {
        let random_data = [0xde, 0xad, 0xbe, 0xef, 0x01, 0x02, 0x03, 0x04];
        assert_eq!(parse_tls_client_hello(&random_data), None);
        assert_eq!(parse_http_request(&random_data), None);
    }
}
