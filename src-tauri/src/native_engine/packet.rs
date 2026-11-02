//! Safe, fast, zero-copy packet parser and builder for IPv4, IPv6, and TCP.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

pub const IPPROTO_TCP: u8 = 6;
pub const IPPROTO_UDP: u8 = 17;

pub const TCP_FIN: u8 = 0x01;
pub const TCP_SYN: u8 = 0x02;
pub const TCP_RST: u8 = 0x04;
pub const TCP_PSH: u8 = 0x08;
pub const TCP_ACK: u8 = 0x10;
pub const TCP_URG: u8 = 0x20;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Ipv4Header {
    pub ihl: u8, // in bytes (typically 20..60)
    pub dscp_ecn: u8,
    pub total_len: u16,
    pub id: u16,
    pub flags_fragment: u16,
    pub ttl: u8,
    pub protocol: u8,
    pub checksum: u16,
    pub src_ip: [u8; 4],
    pub dst_ip: [u8; 4],
    pub options: Vec<u8>,
}

impl Ipv4Header {
    pub fn parse(buf: &[u8]) -> Option<(Self, usize)> {
        if buf.len() < 20 {
            return None;
        }
        let ver_ihl = buf[0];
        let version = ver_ihl >> 4;
        if version != 4 {
            return None;
        }
        let ihl_words = ver_ihl & 0x0F;
        let ihl = (ihl_words as usize) * 4;
        if ihl < 20 || buf.len() < ihl {
            return None;
        }

        let total_len = u16::from_be_bytes([buf[2], buf[3]]);
        let id = u16::from_be_bytes([buf[4], buf[5]]);
        let flags_fragment = u16::from_be_bytes([buf[6], buf[7]]);
        let ttl = buf[8];
        let protocol = buf[9];
        let checksum = u16::from_be_bytes([buf[10], buf[11]]);
        let mut src_ip = [0u8; 4];
        let mut dst_ip = [0u8; 4];
        src_ip.copy_from_slice(&buf[12..16]);
        dst_ip.copy_from_slice(&buf[16..20]);

        let options = if ihl > 20 {
            buf[20..ihl].to_vec()
        } else {
            Vec::new()
        };

        Some((
            Self {
                ihl: ihl as u8,
                dscp_ecn: buf[1],
                total_len,
                id,
                flags_fragment,
                ttl,
                protocol,
                checksum,
                src_ip,
                dst_ip,
                options,
            },
            ihl,
        ))
    }

    pub fn serialize_into(&self, out: &mut [u8]) {
        assert!(out.len() >= self.ihl as usize);
        let ihl_words = (self.ihl / 4) & 0x0F;
        out[0] = (4 << 4) | ihl_words;
        out[1] = self.dscp_ecn;
        out[2..4].copy_from_slice(&self.total_len.to_be_bytes());
        out[4..6].copy_from_slice(&self.id.to_be_bytes());
        out[6..8].copy_from_slice(&self.flags_fragment.to_be_bytes());
        out[8] = self.ttl;
        out[9] = self.protocol;
        out[10..12].copy_from_slice(&[0, 0]); // Zero before checksum
        out[12..16].copy_from_slice(&self.src_ip);
        out[16..20].copy_from_slice(&self.dst_ip);
        if !self.options.is_empty() {
            let opt_len = self.options.len().min(out.len() - 20);
            out[20..20 + opt_len].copy_from_slice(&self.options[..opt_len]);
        }
        let csum = compute_checksum(&out[..self.ihl as usize]);
        out[10..12].copy_from_slice(&csum.to_be_bytes());
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Ipv6Header {
    pub version_tc_fl: u32,
    pub payload_len: u16,
    pub next_header: u8,
    pub hop_limit: u8,
    pub src_ip: [u8; 16],
    pub dst_ip: [u8; 16],
}

impl Ipv6Header {
    pub const FIXED_HEADER_LEN: usize = 40;

    pub fn parse(buf: &[u8]) -> Option<(Self, usize)> {
        if buf.len() < Self::FIXED_HEADER_LEN {
            return None;
        }
        let version_tc_fl = u32::from_be_bytes([buf[0], buf[1], buf[2], buf[3]]);
        let version = (version_tc_fl >> 28) & 0x0F;
        if version != 6 {
            return None;
        }
        let payload_len = u16::from_be_bytes([buf[4], buf[5]]);
        let next_header = buf[6];
        let hop_limit = buf[7];
        let mut src_ip = [0u8; 16];
        let mut dst_ip = [0u8; 16];
        src_ip.copy_from_slice(&buf[8..24]);
        dst_ip.copy_from_slice(&buf[24..40]);

        Some((
            Self {
                version_tc_fl,
                payload_len,
                next_header,
                hop_limit,
                src_ip,
                dst_ip,
            },
            Self::FIXED_HEADER_LEN,
        ))
    }

    pub fn serialize_into(&self, out: &mut [u8]) {
        assert!(out.len() >= Self::FIXED_HEADER_LEN);
        out[0..4].copy_from_slice(&self.version_tc_fl.to_be_bytes());
        out[4..6].copy_from_slice(&self.payload_len.to_be_bytes());
        out[6] = self.next_header;
        out[7] = self.hop_limit;
        out[8..24].copy_from_slice(&self.src_ip);
        out[24..40].copy_from_slice(&self.dst_ip);
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum IpHeader {
    V4(Ipv4Header),
    V6(Ipv6Header),
}

impl IpHeader {
    pub fn src_ip(&self) -> IpAddr {
        match self {
            Self::V4(v4) => IpAddr::V4(Ipv4Addr::from(v4.src_ip)),
            Self::V6(v6) => IpAddr::V6(Ipv6Addr::from(v6.src_ip)),
        }
    }

    pub fn dst_ip(&self) -> IpAddr {
        match self {
            Self::V4(v4) => IpAddr::V4(Ipv4Addr::from(v4.dst_ip)),
            Self::V6(v6) => IpAddr::V6(Ipv6Addr::from(v6.dst_ip)),
        }
    }

    pub fn ttl(&self) -> u8 {
        match self {
            Self::V4(v4) => v4.ttl,
            Self::V6(v6) => v6.hop_limit,
        }
    }

    pub fn set_ttl(&mut self, ttl: u8) {
        match self {
            Self::V4(v4) => v4.ttl = ttl,
            Self::V6(v6) => v6.hop_limit = ttl,
        }
    }

    pub fn protocol(&self) -> u8 {
        match self {
            Self::V4(v4) => v4.protocol,
            Self::V6(v6) => v6.next_header,
        }
    }

    pub fn header_len(&self) -> usize {
        match self {
            Self::V4(v4) => v4.ihl as usize,
            Self::V6(_) => Ipv6Header::FIXED_HEADER_LEN,
        }
    }

    pub fn set_total_length(&mut self, l4_and_payload_len: usize) {
        match self {
            Self::V4(v4) => {
                v4.total_len = (v4.ihl as usize + l4_and_payload_len) as u16;
            }
            Self::V6(v6) => {
                v6.payload_len = l4_and_payload_len as u16;
            }
        }
    }

    pub fn serialize_into(&self, out: &mut [u8]) {
        match self {
            Self::V4(v4) => v4.serialize_into(out),
            Self::V6(v6) => v6.serialize_into(out),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TcpHeader {
    pub src_port: u16,
    pub dst_port: u16,
    pub seq: u32,
    pub ack: u32,
    pub data_offset: u8, // In bytes (min 20)
    pub flags: u8,
    pub window_size: u16,
    pub checksum: u16,
    pub urgent_ptr: u16,
    pub options: Vec<u8>,
}

impl TcpHeader {
    pub fn parse(buf: &[u8]) -> Option<(Self, usize)> {
        if buf.len() < 20 {
            return None;
        }
        let src_port = u16::from_be_bytes([buf[0], buf[1]]);
        let dst_port = u16::from_be_bytes([buf[2], buf[3]]);
        let seq = u32::from_be_bytes([buf[4], buf[5], buf[6], buf[7]]);
        let ack = u32::from_be_bytes([buf[8], buf[9], buf[10], buf[11]]);
        let data_offset_words = buf[12] >> 4;
        let data_offset = (data_offset_words as usize) * 4;
        if data_offset < 20 || buf.len() < data_offset {
            return None;
        }
        let flags = buf[13];
        let window_size = u16::from_be_bytes([buf[14], buf[15]]);
        let checksum = u16::from_be_bytes([buf[16], buf[17]]);
        let urgent_ptr = u16::from_be_bytes([buf[18], buf[19]]);

        let options = if data_offset > 20 {
            buf[20..data_offset].to_vec()
        } else {
            Vec::new()
        };

        Some((
            Self {
                src_port,
                dst_port,
                seq,
                ack,
                data_offset: data_offset as u8,
                flags,
                window_size,
                checksum,
                urgent_ptr,
                options,
            },
            data_offset,
        ))
    }

    pub fn serialize_into(&self, out: &mut [u8]) {
        assert!(out.len() >= self.data_offset as usize);
        out[0..2].copy_from_slice(&self.src_port.to_be_bytes());
        out[2..4].copy_from_slice(&self.dst_port.to_be_bytes());
        out[4..8].copy_from_slice(&self.seq.to_be_bytes());
        out[8..12].copy_from_slice(&self.ack.to_be_bytes());
        let words = (self.data_offset / 4) & 0x0F;
        out[12] = words << 4;
        out[13] = self.flags;
        out[14..16].copy_from_slice(&self.window_size.to_be_bytes());
        out[16..18].copy_from_slice(&self.checksum.to_be_bytes());
        out[18..20].copy_from_slice(&self.urgent_ptr.to_be_bytes());
        if !self.options.is_empty() {
            let opt_len = self.options.len().min(out.len() - 20);
            out[20..20 + opt_len].copy_from_slice(&self.options[..opt_len]);
        }
    }

    /// Adds RFC 2385 TCP MD5 signature option (Kind 19, Length 18) for DPI fooling.
    pub fn append_md5_signature_option(&mut self) {
        // TCP Option Kind 19 = MD5 Signature, length 18
        let mut opt = vec![19u8, 18u8];
        opt.extend_from_slice(&[0x42; 16]); // 16 bytes MD5 digest
        // Pad with NOPs to 4-byte boundary (18 + 2 = 20 bytes total)
        opt.push(1);
        opt.push(1);

        // Ensure total options never exceed 40 bytes (60 bytes max TCP header)
        if self.options.len() > 20 {
            self.options.truncate(20);
        }
        self.options.extend(opt);
        self.data_offset = (20 + self.options.len().min(40)) as u8;
    }

    #[inline]
    pub fn is_syn(&self) -> bool {
        (self.flags & TCP_SYN) != 0
    }

    #[inline]
    pub fn is_ack(&self) -> bool {
        (self.flags & TCP_ACK) != 0
    }

    #[inline]
    pub fn is_psh(&self) -> bool {
        (self.flags & TCP_PSH) != 0
    }

    #[inline]
    pub fn is_fin(&self) -> bool {
        (self.flags & TCP_FIN) != 0
    }

    #[inline]
    pub fn is_rst(&self) -> bool {
        (self.flags & TCP_RST) != 0
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UdpHeader {
    pub src_port: u16,
    pub dst_port: u16,
    pub length: u16,
    pub checksum: u16,
}

impl UdpHeader {
    pub const FIXED_HEADER_LEN: usize = 8;

    pub fn parse(buf: &[u8]) -> Option<(Self, usize)> {
        if buf.len() < Self::FIXED_HEADER_LEN {
            return None;
        }
        let src_port = u16::from_be_bytes([buf[0], buf[1]]);
        let dst_port = u16::from_be_bytes([buf[2], buf[3]]);
        let length = u16::from_be_bytes([buf[4], buf[5]]);
        let checksum = u16::from_be_bytes([buf[6], buf[7]]);
        Some((
            Self {
                src_port,
                dst_port,
                length,
                checksum,
            },
            Self::FIXED_HEADER_LEN,
        ))
    }
}

pub struct ParsedUdpPacket<'a> {
    pub ip: IpHeader,
    pub udp: UdpHeader,
    pub payload: &'a [u8],
    pub ip_header_len: usize,
    pub udp_header_len: usize,
}

pub fn parse_udp_packet(buf: &[u8]) -> Option<ParsedUdpPacket<'_>> {
    if buf.is_empty() {
        return None;
    }
    let ver = buf[0] >> 4;
    let (ip, ip_len) = match ver {
        4 => {
            let (v4, len) = Ipv4Header::parse(buf)?;
            (IpHeader::V4(v4), len)
        }
        6 => {
            let (v6, len) = Ipv6Header::parse(buf)?;
            (IpHeader::V6(v6), len)
        }
        _ => return None,
    };

    if ip.protocol() != IPPROTO_UDP {
        return None;
    }

    let udp_buf = &buf[ip_len..];
    let (udp, udp_len) = UdpHeader::parse(udp_buf)?;
    let total_header_len = ip_len + udp_len;
    if buf.len() < total_header_len {
        return None;
    }
    let payload = &buf[total_header_len..];

    Some(ParsedUdpPacket {
        ip,
        udp,
        payload,
        ip_header_len: ip_len,
        udp_header_len: udp_len,
    })
}

pub struct ParsedPacket<'a> {
    pub ip: IpHeader,
    pub tcp: TcpHeader,
    pub payload: &'a [u8],
    pub ip_header_len: usize,
    pub tcp_header_len: usize,
}

pub fn parse_packet(buf: &[u8]) -> Option<ParsedPacket<'_>> {
    if buf.is_empty() {
        return None;
    }
    let ver = buf[0] >> 4;
    let (ip, ip_len) = match ver {
        4 => {
            let (v4, len) = Ipv4Header::parse(buf)?;
            (IpHeader::V4(v4), len)
        }
        6 => {
            let (v6, len) = Ipv6Header::parse(buf)?;
            (IpHeader::V6(v6), len)
        }
        _ => return None,
    };

    if ip.protocol() != IPPROTO_TCP {
        return None;
    }

    let tcp_buf = &buf[ip_len..];
    let (tcp, tcp_len) = TcpHeader::parse(tcp_buf)?;
    let total_header_len = ip_len + tcp_len;
    if buf.len() < total_header_len {
        return None;
    }
    let payload = &buf[total_header_len..];

    Some(ParsedPacket {
        ip,
        tcp,
        payload,
        ip_header_len: ip_len,
        tcp_header_len: tcp_len,
    })
}

/// Computes standard Internet Checksum (RFC 1071) over a slice.
pub fn compute_checksum(data: &[u8]) -> u16 {
    let mut sum: u32 = 0;
    let mut chunks = data.chunks_exact(2);
    for chunk in &mut chunks {
        sum += u16::from_be_bytes([chunk[0], chunk[1]]) as u32;
    }
    let rem = chunks.remainder();
    if !rem.is_empty() {
        sum += (rem[0] as u32) << 8;
    }
    while (sum >> 16) > 0 {
        sum = (sum & 0xFFFF) + (sum >> 16);
    }
    !(sum as u16)
}

/// Computes TCP checksum over pseudo-header + TCP header + TCP payload.
pub fn compute_tcp_checksum(ip: &IpHeader, tcp_header: &[u8], payload: &[u8]) -> u16 {
    let mut sum: u32 = 0;
    let tcp_len = (tcp_header.len() + payload.len()) as u32;

    match ip {
        IpHeader::V4(v4) => {
            // IPv4 pseudo header: SrcIP (4), DstIP (4), Zero (1), Protocol (1), TCPLen (2)
            sum += u16::from_be_bytes([v4.src_ip[0], v4.src_ip[1]]) as u32;
            sum += u16::from_be_bytes([v4.src_ip[2], v4.src_ip[3]]) as u32;
            sum += u16::from_be_bytes([v4.dst_ip[0], v4.dst_ip[1]]) as u32;
            sum += u16::from_be_bytes([v4.dst_ip[2], v4.dst_ip[3]]) as u32;
            sum += v4.protocol as u32;
            sum += tcp_len;
        }
        IpHeader::V6(v6) => {
            // IPv6 pseudo header: SrcIP (16), DstIP (16), TCPLen (4), Zero (3), NextHeader (1)
            for chunk in v6.src_ip.chunks_exact(2) {
                sum += u16::from_be_bytes([chunk[0], chunk[1]]) as u32;
            }
            for chunk in v6.dst_ip.chunks_exact(2) {
                sum += u16::from_be_bytes([chunk[0], chunk[1]]) as u32;
            }
            sum += tcp_len >> 16;
            sum += tcp_len & 0xFFFF;
            sum += v6.next_header as u32;
        }
    }

    // TCP Header without checksum field (bytes 16..18 treated as 0)
    let mut chunks = tcp_header.chunks_exact(2);
    let mut idx = 0;
    for chunk in &mut chunks {
        if idx != 8 {
            sum += u16::from_be_bytes([chunk[0], chunk[1]]) as u32;
        }
        idx += 1;
    }
    let rem = chunks.remainder();
    if !rem.is_empty() {
        sum += (rem[0] as u32) << 8;
    }

    // Payload
    let mut p_chunks = payload.chunks_exact(2);
    for chunk in &mut p_chunks {
        sum += u16::from_be_bytes([chunk[0], chunk[1]]) as u32;
    }
    let p_rem = p_chunks.remainder();
    if !p_rem.is_empty() {
        sum += (p_rem[0] as u32) << 8;
    }

    while (sum >> 16) > 0 {
        sum = (sum & 0xFFFF) + (sum >> 16);
    }
    let res = !(sum as u16);
    if res == 0 {
        0xFFFF
    } else {
        res
    }
}

/// Constructs a full serialized IP + TCP packet with valid lengths and checksums.
pub fn build_packet(
    mut ip: IpHeader,
    mut tcp: TcpHeader,
    payload: &[u8],
    corrupt_tcp_checksum: bool,
) -> Vec<u8> {
    let ip_len = ip.header_len();
    let tcp_len = tcp.data_offset as usize;
    let total_len = ip_len + tcp_len + payload.len();

    ip.set_total_length(tcp_len + payload.len());

    let mut out = vec![0u8; total_len];
    ip.serialize_into(&mut out[..ip_len]);

    // Zero out checksum before calculating
    tcp.checksum = 0;
    tcp.serialize_into(&mut out[ip_len..ip_len + tcp_len]);
    out[ip_len + tcp_len..].copy_from_slice(payload);

    let mut csum = compute_tcp_checksum(&ip, &out[ip_len..ip_len + tcp_len], payload);
    if corrupt_tcp_checksum {
        csum ^= 0xFFFF; // Invert to guarantee invalid checksum
        if csum == 0 {
            csum = 0xDEAD;
        }
    }
    out[ip_len + 16..ip_len + 18].copy_from_slice(&csum.to_be_bytes());

    out
}

/// Builds an ICMPv4 Destination Unreachable (Port Unreachable - Type 3, Code 3) packet
/// in response to a UDP packet. Injected in the inbound direction, this causes the Windows
/// networking stack to signal `WSAECONNRESET` immediately, forcing Chromium/Brave/Edge/Firefox
/// to fall back from QUIC (HTTP/3) to TCP (HTTP/2 / TLS 1.3) within 0.1ms without stalling.
pub fn build_icmp_port_unreachable_ipv4(orig_pkt: &[u8], orig_ihl: usize) -> Option<Vec<u8>> {
    if orig_pkt.len() < orig_ihl + 8 {
        return None;
    }

    let icmp_data_len = orig_ihl + 8;
    let total_len = 20 + 8 + icmp_data_len;
    let mut out = vec![0u8; total_len];

    // 1. IPv4 Header (20 bytes)
    out[0] = 0x45; // Version 4, IHL 5
    out[1] = 0;    // DSCP/ECN
    out[2..4].copy_from_slice(&(total_len as u16).to_be_bytes());
    out[4..6].copy_from_slice(&0u16.to_be_bytes()); // ID
    out[6..8].copy_from_slice(&0x4000u16.to_be_bytes()); // DF flag
    out[8] = 64;   // TTL
    out[9] = 1;    // Protocol = ICMP (1)
    out[10..12].copy_from_slice(&[0, 0]); // Zero checksum before calculation

    // Invert endpoints: Source is original destination, Destination is original source (our PC)
    out[12..16].copy_from_slice(&orig_pkt[16..20]); // src = orig dst
    out[16..20].copy_from_slice(&orig_pkt[12..16]); // dst = orig src

    let ip_csum = compute_checksum(&out[..20]);
    out[10..12].copy_from_slice(&ip_csum.to_be_bytes());

    // 2. ICMP Header (8 bytes)
    out[20] = 3; // Type 3: Destination Unreachable
    out[21] = 3; // Code 3: Port Unreachable
    out[22..24].copy_from_slice(&[0, 0]); // ICMP Checksum placeholder
    out[24..28].copy_from_slice(&[0, 0, 0, 0]); // Unused 4 bytes

    // 3. ICMP Payload: Original IPv4 header + first 8 bytes of original UDP
    out[28..28 + icmp_data_len].copy_from_slice(&orig_pkt[..icmp_data_len]);

    let icmp_csum = compute_checksum(&out[20..total_len]);
    out[22..24].copy_from_slice(&icmp_csum.to_be_bytes());

    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ipv4_checksum() {
        // RFC 1071 Header example
        let header = [
            0x45, 0x00, 0x00, 0x73, 0x00, 0x00, 0x40, 0x00, 0x40, 0x11, 0x00, 0x00, 0xc0, 0xa8,
            0x00, 0x01, 0xc0, 0xa8, 0x00, 0xc7,
        ];
        let csum = compute_checksum(&header);
        assert_eq!(csum, 0xb861);
    }

    #[test]
    fn test_build_and_parse_ipv4_tcp_packet() {
        let ip = IpHeader::V4(Ipv4Header {
            ihl: 20,
            dscp_ecn: 0,
            total_len: 0,
            id: 1234,
            flags_fragment: 0x4000, // DF
            ttl: 64,
            protocol: IPPROTO_TCP,
            checksum: 0,
            src_ip: [192, 168, 1, 100],
            dst_ip: [104, 21, 5, 200],
            options: Vec::new(),
        });

        let tcp = TcpHeader {
            src_port: 54321,
            dst_port: 443,
            seq: 1000,
            ack: 5000,
            data_offset: 20,
            flags: TCP_ACK | TCP_PSH,
            window_size: 65535,
            checksum: 0,
            urgent_ptr: 0,
            options: Vec::new(),
        };

        let payload = b"Hello, Slipstream Native Engine!";
        let raw = build_packet(ip, tcp, payload, false);

        let parsed = parse_packet(&raw).expect("Must parse successfully");
        assert_eq!(parsed.ip.src_ip(), IpAddr::V4(Ipv4Addr::new(192, 168, 1, 100)));
        assert_eq!(parsed.ip.dst_ip(), IpAddr::V4(Ipv4Addr::new(104, 21, 5, 200)));
        assert_eq!(parsed.ip.ttl(), 64);
        assert_eq!(parsed.tcp.src_port, 54321);
        assert_eq!(parsed.tcp.dst_port, 443);
        assert_eq!(parsed.tcp.seq, 1000);
        assert_eq!(parsed.tcp.ack, 5000);
        assert!(parsed.tcp.is_ack());
        assert!(parsed.tcp.is_psh());
        assert_eq!(parsed.payload, payload);

        // Verify TCP checksum matches
        let tcp_start = parsed.ip_header_len;
        let tcp_end = tcp_start + parsed.tcp_header_len;
        let expected_csum = compute_tcp_checksum(&parsed.ip, &raw[tcp_start..tcp_end], parsed.payload);
        assert_eq!(parsed.tcp.checksum, expected_csum);
    }

    #[test]
    fn test_build_and_parse_ipv6_tcp_packet() {
        let ip = IpHeader::V6(Ipv6Header {
            version_tc_fl: 6 << 28,
            payload_len: 0,
            next_header: IPPROTO_TCP,
            hop_limit: 128,
            src_ip: [0x20, 0x01, 0x0d, 0xb8, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1],
            dst_ip: [0x26, 0x06, 0x47, 0x00, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 2],
        });

        let tcp = TcpHeader {
            src_port: 48999,
            dst_port: 80,
            seq: 2000,
            ack: 0,
            data_offset: 20,
            flags: TCP_ACK,
            window_size: 32768,
            checksum: 0,
            urgent_ptr: 0,
            options: Vec::new(),
        };

        let payload = b"GET / HTTP/1.1\r\nHost: example.com\r\n\r\n";
        let raw = build_packet(ip, tcp, payload, false);

        let parsed = parse_packet(&raw).expect("Must parse IPv6 TCP packet");
        assert_eq!(parsed.ip.ttl(), 128);
        assert_eq!(parsed.tcp.dst_port, 80);
        assert_eq!(parsed.payload, payload);
    }

    #[test]
    fn test_build_icmp_port_unreachable_ipv4() {
        // Construct a sample IPv4 UDP packet (20 byte IPv4 + 8 byte UDP + 12 byte payload)
        let mut udp_pkt = vec![0u8; 40];
        udp_pkt[0] = 0x45; // Version 4, IHL 5 (20 bytes)
        udp_pkt[2..4].copy_from_slice(&40u16.to_be_bytes());
        udp_pkt[9] = 17; // UDP
        udp_pkt[12..16].copy_from_slice(&[192, 168, 1, 50]); // Client IP
        udp_pkt[16..20].copy_from_slice(&[142, 250, 180, 206]); // Remote IP (e.g. Google QUIC)
        udp_pkt[20..22].copy_from_slice(&55555u16.to_be_bytes()); // Client UDP port
        udp_pkt[22..24].copy_from_slice(&443u16.to_be_bytes()); // Remote UDP port (443)

        let icmp = build_icmp_port_unreachable_ipv4(&udp_pkt, 20).expect("Must build ICMP");
        // Verify total length: 20 (IP) + 8 (ICMP) + 20 (orig IP) + 8 (orig UDP) = 56 bytes
        assert_eq!(icmp.len(), 56);
        // Verify IP endpoints inverted
        assert_eq!(&icmp[12..16], &[142, 250, 180, 206]); // src = remote IP
        assert_eq!(&icmp[16..20], &[192, 168, 1, 50]); // dst = client IP
        assert_eq!(icmp[9], 1); // Protocol = ICMP

        // Verify ICMP Type 3, Code 3
        assert_eq!(icmp[20], 3); // Type 3 (Destination Unreachable)
        assert_eq!(icmp[21], 3); // Code 3 (Port Unreachable)

        // Verify ICMP checksum
        let csum = compute_checksum(&icmp[20..56]);
        assert_eq!(csum, 0); // Checksum of packet with calculated csum should be 0

        // Verify IP checksum
        let ip_csum = compute_checksum(&icmp[..20]);
        assert_eq!(ip_csum, 0);
    }
}
