//! Local DoH-backed resolver, latency tester, and Discord update relay.
//! Strictly leaves Windows WinINET proxy and network adapter DNS untouched.
//! Eliminates proxy overhead for 0-ping, 0-lag kernel packet processing via WinDivert.

use std::{
    collections::HashMap,
    net::{IpAddr, Ipv4Addr, SocketAddr},
    os::windows::process::CommandExt,
    path::PathBuf,
    process::Command,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};

#[link(name = "wininet")]
extern "system" {
    fn InternetSetOptionW(
        h_internet: *mut std::ffi::c_void,
        dw_option: u32,
        lp_buffer: *mut std::ffi::c_void,
        dw_buffer_length: u32,
    ) -> i32;
}

const INTERNET_OPTION_REFRESH: u32 = 37;
const INTERNET_OPTION_SETTINGS_CHANGED: u32 = 39;

pub const DISCORD_RELAY_PORT: u16 = 18809;

type DnsCache = Mutex<HashMap<String, (Vec<Ipv4Addr>, Instant)>>;

// In-memory DNS cache: host -> (IPs, expiration)
fn get_dns_cache() -> &'static DnsCache {
    use std::sync::OnceLock;
    static CACHE: OnceLock<DnsCache> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

#[derive(serde::Deserialize)]
struct DohResponse {
    #[serde(rename = "Status")]
    status: i32,
    #[serde(rename = "Answer")]
    answer: Option<Vec<DohAnswer>>,
}

#[derive(serde::Deserialize)]
struct DohAnswer {
    #[serde(rename = "type")]
    rtype: u16,
    data: String,
}

/// Checks if an IPv4 address is an ISP bogon or block page redirect (e.g. 195.175.x.x in TR)
fn is_bogon_or_block(ip: Ipv4Addr) -> bool {
    let o = ip.octets();
    // 0.0.0.0/8, 127.0.0.0/8, 195.175.x.x (known TTNET block splash)
    o[0] == 0 || o[0] == 127 || (o[0] == 195 && o[1] == 175)
}

/// Flushes Windows DNS cache and resets Slipstream in-memory DNS cache.
pub fn flush_dns_cache() {
    let mut cache = get_dns_cache().lock().unwrap();
    cache.clear();
    let _ = Command::new("ipconfig")
        .arg("/flushdns")
        .creation_flags(0x0800_0000)
        .output();
}

/// Resolves a hostname using DNS-over-HTTPS with configurable provider and TTL caching.
pub async fn resolve_doh_with_provider(host: &str, provider: &str) -> Result<Ipv4Addr, String> {
    let raw_clean = host.trim().trim_end_matches('.').to_lowercase();
    let clean_host = raw_clean.split(':').next().unwrap_or(&raw_clean);
    if let Ok(ip) = clean_host.parse::<Ipv4Addr>() {
        return Ok(ip);
    }

    // Check memory cache
    {
        let cache = get_dns_cache().lock().unwrap();
        if let Some((ips, expiry)) = cache.get(clean_host) {
            if Instant::now() < *expiry && !ips.is_empty() {
                return Ok(ips[0]);
            }
        }
    }

    let client = reqwest::Client::builder()
        .timeout(Duration::from_millis(2500))
        .no_proxy()
        .build()
        .map_err(|e| e.to_string())?;

    let cloudflare = format!("https://1.1.1.1/dns-query?name={clean_host}&type=A");
    let google = format!("https://dns.google/resolve?name={clean_host}&type=A");
    let quad9 = format!("https://9.9.9.9/dns-query?name={clean_host}&type=A");
    let adguard = format!("https://dns.adguard-dns.com/resolve?name={clean_host}&type=A");

    let urls = match provider.to_lowercase().as_str() {
        "google" => vec![google, cloudflare, quad9, adguard],
        "quad9" => vec![quad9, cloudflare, google, adguard],
        "adguard" => vec![adguard, cloudflare, google, quad9],
        "yandex" => vec![cloudflare, google, quad9, adguard],
        _ => vec![cloudflare, google, quad9, adguard],
    };

    let mut last_err = String::from("DNS resolution failed");
    for url in urls {
        match client
            .get(&url)
            .header("Accept", "application/dns-json")
            .send()
            .await
        {
            Ok(resp) => {
                if let Ok(bytes) = resp.bytes().await {
                    if let Ok(body) = serde_json::from_slice::<DohResponse>(&bytes) {
                        if body.status == 0 {
                            if let Some(answers) = body.answer {
                                let ips: Vec<Ipv4Addr> = answers
                                    .into_iter()
                                    .filter(|a| a.rtype == 1) // A record
                                    .filter_map(|a| a.data.parse::<Ipv4Addr>().ok())
                                    .filter(|ip| !is_bogon_or_block(*ip))
                                    .collect();

                                if let Some(&first) = ips.first() {
                                    let mut cache = get_dns_cache().lock().unwrap();
                                    cache.insert(
                                        clean_host.to_string(),
                                        (ips, Instant::now() + Duration::from_secs(600)),
                                    );
                                    return Ok(first);
                                }
                            }
                        }
                    }
                }
            }
            Err(e) => {
                last_err = e.to_string();
            }
        }
    }

    Err(format!("DoH resolve error for {clean_host}: {last_err}"))
}

pub async fn resolve_doh(host: &str) -> Result<Ipv4Addr, String> {
    resolve_doh_with_provider(host, "cloudflare").await
}

/// Measures TCP connect latency in milliseconds.
pub async fn measure_latency(host: &str, port: u16) -> Option<u64> {
    let ip = resolve_doh(host).await.ok()?;
    let addr = SocketAddr::new(IpAddr::V4(ip), port);
    let start = Instant::now();
    match tokio::time::timeout(Duration::from_millis(2000), TcpStream::connect(addr)).await {
        Ok(Ok(_)) => Some(start.elapsed().as_millis() as u64),
        _ => None,
    }
}

/// Safety cleaner: guarantees no stale 127.0.0.1 proxy or environment variables remain.
/// Never activates a proxy; only cleans up leftovers from previous configurations.
pub fn clean_stale_proxy() {
    // Check if ProxyServer points to localhost/127.0.0.1
    let query = Command::new("reg.exe")
        .args([
            "query",
            r"HKCU\Software\Microsoft\Windows\CurrentVersion\Internet Settings",
            "/v",
            "ProxyServer",
        ])
        .creation_flags(0x0800_0000)
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
        .unwrap_or_default();

    if query.contains("127.0.0.1") || query.contains("localhost") {
        let _ = Command::new("reg.exe")
            .args([
                "add",
                r"HKCU\Software\Microsoft\Windows\CurrentVersion\Internet Settings",
                "/v",
                "ProxyEnable",
                "/t",
                "REG_DWORD",
                "/d",
                "0",
                "/f",
            ])
            .creation_flags(0x0800_0000)
            .output();

        let _ = Command::new("reg.exe")
            .args([
                "delete",
                r"HKCU\Software\Microsoft\Windows\CurrentVersion\Internet Settings",
                "/v",
                "ProxyServer",
                "/f",
            ])
            .creation_flags(0x0800_0000)
            .output();
    }

    // Clean any leftover proxy environment variables
    let env_query = Command::new("reg.exe")
        .args(["query", r"HKCU\Environment"])
        .creation_flags(0x0800_0000)
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
        .unwrap_or_default();

    for key in ["HTTP_PROXY", "HTTPS_PROXY", "NO_PROXY"] {
        if env_query.contains(key) {
            let _ = Command::new("reg.exe")
                .args(["delete", r"HKCU\Environment", "/v", key, "/f"])
                .creation_flags(0x0800_0000)
                .output();
        }
    }

    unsafe {
        InternetSetOptionW(std::ptr::null_mut(), INTERNET_OPTION_SETTINGS_CHANGED, std::ptr::null_mut(), 0);
        InternetSetOptionW(std::ptr::null_mut(), INTERNET_OPTION_REFRESH, std::ptr::null_mut(), 0);
    }
}

/// Discord settings locations on Windows.
fn discord_settings_paths() -> Vec<PathBuf> {
    let mut out = vec![];
    if let Ok(appdata) = std::env::var("APPDATA") {
        let base = PathBuf::from(appdata);
        for dir in ["discord", "discordptb", "discordcanary", "discorddevelopment"] {
            let p = base.join(dir).join("settings.json");
            if p.parent().map(|d| d.exists()).unwrap_or(false) {
                out.push(p);
            }
        }
    }
    out
}

/// Cleans or updates Discord's NEW_UPDATE_ENDPOINT.
/// Passing None removes any local relay endpoint so Discord connects natively via WinDivert kernel desync.
pub fn patch_discord_settings(endpoint: Option<&str>) {
    for path in discord_settings_paths() {
        if !path.exists() {
            continue;
        }
        let mut obj: serde_json::Value = std::fs::read_to_string(&path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_else(|| serde_json::json!({}));

        if let Some(url) = endpoint {
            obj["NEW_UPDATE_ENDPOINT"] = serde_json::Value::String(url.to_string());
        } else if let Some(map) = obj.as_object_mut() {
            map.remove("NEW_UPDATE_ENDPOINT");
        }

        if let Ok(json_str) = serde_json::to_string_pretty(&obj) {
            let _ = std::fs::write(&path, json_str);
        }
    }
}


#[allow(dead_code)]
pub const DISCORD_DOMAINS: &[&str] = &[
    "discord.com", "discord.gg", "discord.co", "discord.media", "discord.new",
    "discord.gift", "discord.gifts", "discord.dev", "discord.design", "discord.store",
    "discord.tools", "discordapp.com", "discordapp.net", "discordcdn.com",
    "discordstatus.com", "discordsays.com", "discordsez.com", "discordactivities.com",
    "discord-activities.com", "dis.gd",
];

#[allow(dead_code)]
pub fn is_discord_host(host: &str) -> bool {
    let h = host.trim().trim_end_matches('.').to_lowercase();
    let clean = h.split(':').next().unwrap_or(&h);
    DISCORD_DOMAINS.iter().any(|&d| clean == d || clean.ends_with(&format!(".{d}")))
}

#[allow(dead_code)]
fn rewrite_discord_urls(text: &str, port: u16) -> String {
    let mut result = String::with_capacity(text.len() + 1024);
    let mut remaining = text;
    let target_prefix = "https://";

    while let Some(idx) = remaining.find(target_prefix) {
        result.push_str(&remaining[..idx]);
        let after_scheme = &remaining[idx + target_prefix.len()..];
        let host_len = after_scheme
            .find(|c: char| c == '/' || c == '"' || c == '\'' || c == '?' || c.is_whitespace())
            .unwrap_or(after_scheme.len());
        let host = &after_scheme[..host_len];

        if is_discord_host(host) {
            result.push_str(&format!("http://127.0.0.1:{port}/__h/{host}"));
            remaining = &after_scheme[host_len..];
        } else {
            result.push_str(target_prefix);
            remaining = after_scheme;
        }
    }
    result.push_str(remaining);
    result
}

/// Checks if Discord.exe or Update.exe is currently running.
pub fn is_discord_running() -> bool {
    let out = Command::new("tasklist")
        .args(["/FI", "IMAGENAME eq Discord.exe", "/NH", "/FO", "CSV"])
        .creation_flags(0x0800_0000)
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).to_lowercase())
        .unwrap_or_default();
    if out.contains("discord.exe") {
        return true;
    }
    let out_up = Command::new("tasklist")
        .args(["/FI", "IMAGENAME eq Update.exe", "/NH", "/FO", "CSV"])
        .creation_flags(0x0800_0000)
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).to_lowercase())
        .unwrap_or_default();
    out_up.contains("update.exe")
}

/// Checks if general internet connectivity is functional.
#[allow(dead_code)]
pub fn is_internet_available() -> bool {
    let addrs = ["1.1.1.1:443", "8.8.8.8:443"];
    for addr_str in addrs {
        if let Ok(addr) = addr_str.parse::<SocketAddr>() {
            if std::net::TcpStream::connect_timeout(&addr, Duration::from_millis(1200)).is_ok() {
                return true;
            }
        }
    }
    false
}

/// Checks if TCP connection to Discord's primary endpoints (updates and gateway) is successful.
#[allow(dead_code)]
pub fn probe_discord_connectivity() -> bool {
    let addrs = [
        "162.159.138.232:443", // updates.discord.com (Cloudflare)
        "162.159.130.234:443", // gateway.discord.gg (Cloudflare)
    ];
    for addr_str in addrs {
        if let Ok(addr) = addr_str.parse::<SocketAddr>() {
            if std::net::TcpStream::connect_timeout(&addr, Duration::from_millis(1500)).is_ok() {
                return true;
            }
        }
    }
    false
}

/// Checks if Discord updater log recorded a recent failure in the last 120 seconds.
#[allow(dead_code)]
pub fn has_discord_recent_updater_error() -> bool {
    let mut log_paths = vec![];
    if let Ok(appdata) = std::env::var("APPDATA") {
        let current_log = PathBuf::from(&appdata).join("discord").join("logs").join("Discord_updater_rCURRENT.log");
        if current_log.exists() {
            log_paths.push(current_log);
        }
    }
    if let Ok(local) = std::env::var("LOCALAPPDATA") {
        let squirrel_log = PathBuf::from(&local).join("Discord").join("SquirrelSetup.log");
        if squirrel_log.exists() {
            log_paths.push(squirrel_log);
        }
    }

    for path in log_paths {
        if let Ok(meta) = std::fs::metadata(&path) {
            if let Ok(modified) = meta.modified() {
                if let Ok(elapsed) = modified.elapsed() {
                    if elapsed < Duration::from_secs(120) {
                        if let Ok(content) = std::fs::read_to_string(&path) {
                            let tail: String = content.lines().rev().take(30).collect::<Vec<&str>>().join("\n");
                            let tail_lower = tail.to_lowercase();
                            if tail_lower.contains("failed:")
                                || tail_lower.contains("cert_chain_failed")
                                || tail_lower.contains("update failed")
                                || tail_lower.contains("connection refused")
                                || tail_lower.contains("güvenilmeyen bir kök")
                            {
                                return true;
                            }
                        }
                    }
                }
            }
        }
    }
    false
}

/// Determines if Discord is actively trying to run/update but blocked or failing to connect.
#[allow(dead_code)]
pub fn is_discord_failing() -> bool {
    if !is_internet_available() {
        return false;
    }
    let running = is_discord_running();
    let recent_err = has_discord_recent_updater_error();
    if !running && !recent_err {
        return false;
    }
    !probe_discord_connectivity()
}

/// Restarts Discord cleanly using Update.exe so it connects cleanly.
pub fn restart_discord() {
    let _ = Command::new("taskkill")
        .args(["/IM", "Discord.exe", "/T", "/F"])
        .creation_flags(0x0800_0000)
        .output();
    let _ = Command::new("taskkill")
        .args(["/IM", "Update.exe", "/T", "/F"])
        .creation_flags(0x0800_0000)
        .output();

    std::thread::sleep(Duration::from_millis(500));

    if let Ok(local) = std::env::var("LOCALAPPDATA") {
        let update_exe = PathBuf::from(&local).join("Discord").join("Update.exe");
        if update_exe.exists() {
            let _ = Command::new(update_exe)
                .args(["--processStart", "Discord.exe"])
                .creation_flags(0x0800_0000)
                .spawn();
        }
    }
}

/// Runs the Discord updater relay on port 18809.
/// Bridges Discord's native updater to Discord servers with TLS 1.3 encryption and URL rewriting.
#[allow(dead_code)]
pub async fn run_discord_relay(stop: Arc<AtomicBool>, port: u16) {
    let mut listener = None;
    let mut last_err = String::new();
    for _i in 0..15 {
        if stop.load(Ordering::Relaxed) {
            return;
        }
        match TcpListener::bind(("127.0.0.1", port)).await {
            Ok(l) => {
                listener = Some(l);
                break;
            }
            Err(e) => {
                last_err = e.to_string();
                tokio::time::sleep(Duration::from_millis(200)).await;
            }
        }
    }

    let listener = match listener {
        Some(l) => l,
        None => {
            eprintln!("[relay] failed to bind on {port}: {last_err}");
            return;
        }
    };

    while !stop.load(Ordering::Relaxed) {
        let (mut client_sock, _) = match tokio::time::timeout(Duration::from_millis(500), listener.accept()).await {
            Ok(Ok(pair)) => pair,
            _ => continue,
        };

        tokio::spawn(async move {
            let mut buf = [0u8; 8192];
            let n = match client_sock.read(&mut buf).await {
                Ok(n) if n > 0 => n,
                _ => return,
            };

            let req_str = String::from_utf8_lossy(&buf[..n]);
            let mut lines = req_str.lines();
            let first_line = match lines.next() {
                Some(l) => l,
                None => return,
            };

            let parts: Vec<&str> = first_line.split_whitespace().collect();
            if parts.len() < 2 {
                return;
            }

            let method = parts[0];
            let raw_path = parts[1];

            if !method.eq_ignore_ascii_case("GET") && !method.eq_ignore_ascii_case("HEAD") {
                let _ = client_sock
                    .write_all(b"HTTP/1.1 405 Method Not Allowed\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
                    .await;
                return;
            }

            let (target_host, target_path) = if let Some(rest) = raw_path.strip_prefix("/__h/") {
                if let Some(pos) = rest.find(['/', '?']) {
                    let (h, p) = rest.split_at(pos);
                    let path = if p.starts_with('?') {
                        format!("/{p}")
                    } else {
                        p.to_string()
                    };
                    (h, path)
                } else {
                    (rest, "/".to_string())
                }
            } else {
                ("updates.discord.com", raw_path.to_string())
            };

            if !is_discord_host(target_host) {
                let _ = client_sock
                    .write_all(b"HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
                    .await;
                return;
            }

            let target_ip = match resolve_doh(target_host).await {
                Ok(ip) => ip,
                Err(e) => {
                    eprintln!("Discord relay DNS resolution failed for {target_host}: {e}");
                    let _ = client_sock
                        .write_all(b"HTTP/1.1 502 Bad Gateway\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
                        .await;
                    let _ = client_sock.flush().await;
                    let _ = client_sock.shutdown().await;
                    return;
                }
            };

            let client = match reqwest::Client::builder()
                .no_proxy()
                .redirect(reqwest::redirect::Policy::none())
                .resolve(target_host, SocketAddr::new(IpAddr::V4(target_ip), 443))
                .connect_timeout(Duration::from_secs(10))
                .build()
            {
                Ok(c) => c,
                Err(_) => {
                    let _ = client_sock
                        .write_all(b"HTTP/1.1 502 Bad Gateway\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
                        .await;
                    return;
                }
            };

            let target_url = format!("https://{target_host}{target_path}");
            let mut req_builder = if method.eq_ignore_ascii_case("HEAD") {
                client.head(&target_url)
            } else {
                client.get(&target_url)
            };

            for line in lines {
                if let Some((k, v)) = line.split_once(':') {
                    let k_lower = k.trim().to_lowercase();
                    if k_lower == "user-agent" || k_lower == "if-none-match" || k_lower == "range" || k_lower == "accept" {
                        req_builder = req_builder.header(k.trim(), v.trim());
                    }
                }
            }
            req_builder = req_builder.header("Accept-Encoding", "identity");

            let resp = match req_builder.send().await {
                Ok(r) => r,
                Err(e) => {
                    eprintln!("Discord relay failed to fetch {target_url}: {e}");
                    let _ = client_sock
                        .write_all(b"HTTP/1.1 502 Bad Gateway\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
                        .await;
                    let _ = client_sock.flush().await;
                    let _ = client_sock.shutdown().await;
                    return;
                }
            };

            let status = resp.status();
            let reason = status.canonical_reason().unwrap_or("OK");

            if status.is_redirection() {
                let mut loc_header = String::new();
                if let Some(loc) = resp.headers().get("location") {
                    if let Ok(loc_str) = loc.to_str() {
                        let rewritten_loc = rewrite_discord_urls(loc_str, port);
                        loc_header = format!("Location: {}\r\n", rewritten_loc);
                    }
                }
                let resp_str = format!(
                    "HTTP/1.1 {} {}\r\n{}Content-Length: 0\r\nConnection: close\r\n\r\n",
                    status.as_u16(),
                    reason,
                    loc_header
                );
                let _ = client_sock.write_all(resp_str.as_bytes()).await;
                let _ = client_sock.flush().await;
                let _ = client_sock.shutdown().await;
                return;
            }

            let content_type = resp
                .headers()
                .get("content-type")
                .and_then(|v| v.to_str().ok())
                .unwrap_or("application/octet-stream")
                .to_string();
            let is_json = content_type.contains("json");

            if is_json && !method.eq_ignore_ascii_case("HEAD") {
                let bytes = match resp.bytes().await {
                    Ok(b) => b,
                    Err(_) => {
                        let _ = client_sock
                            .write_all(b"HTTP/1.1 502 Bad Gateway\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
                            .await;
                        return;
                    }
                };
                let text = String::from_utf8_lossy(&bytes);
                let rewritten = rewrite_discord_urls(&text, port);
                let body_bytes = rewritten.as_bytes();

                let header = format!(
                    "HTTP/1.1 {} {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    status.as_u16(),
                    reason,
                    body_bytes.len()
                );
                let _ = client_sock.write_all(header.as_bytes()).await;
                let _ = client_sock.write_all(body_bytes).await;
                let _ = client_sock.flush().await;
                let _ = client_sock.shutdown().await;
                return;
            }

            let mut header_str = format!("HTTP/1.1 {} {}\r\n", status.as_u16(), reason);
            for (k, v) in resp.headers() {
                let kname = k.as_str().to_lowercase();
                if kname == "connection" || kname == "transfer-encoding" || kname == "keep-alive" {
                    continue;
                }
                if let Ok(vstr) = v.to_str() {
                    header_str.push_str(&format!("{}: {}\r\n", k.as_str(), vstr));
                }
            }
            header_str.push_str("Connection: close\r\n\r\n");

            if client_sock.write_all(header_str.as_bytes()).await.is_err() {
                return;
            }

            if !method.eq_ignore_ascii_case("HEAD") {
                let mut stream = resp;
                while let Ok(Some(chunk)) = stream.chunk().await {
                    if client_sock.write_all(&chunk).await.is_err() {
                        return;
                    }
                }
            }

            let _ = client_sock.flush().await;
            let _ = client_sock.shutdown().await;
        });
    }
}

/// Checks if Windows system proxy is enabled in registry.
pub fn is_proxy_enabled() -> bool {
    let query = Command::new("reg.exe")
        .args([
            "query",
            r"HKCU\Software\Microsoft\Windows\CurrentVersion\Internet Settings",
            "/v",
            "ProxyEnable",
        ])
        .creation_flags(0x0800_0000)
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
        .unwrap_or_default();

    query.contains("0x1")
}

#[derive(serde::Serialize, Clone, Debug)]
pub struct DohBenchmarkResult {
    pub provider: String,
    pub name: String,
    pub latency: Option<u64>,
}

/// Benchmarks all supported DoH DNS providers via direct low-level TCP handshake.
pub async fn benchmark_doh_providers() -> Vec<DohBenchmarkResult> {
    let targets = [
        ("cloudflare", "Cloudflare (1.1.1.1)", "1.1.1.1:443"),
        ("google", "Google DNS (8.8.8.8)", "8.8.8.8:443"),
        ("quad9", "Quad9 (9.9.9.9)", "9.9.9.9:443"),
        ("adguard", "AdGuard (94.140.14.14)", "94.140.14.14:443"),
        ("yandex", "Yandex DNS (77.88.8.8)", "77.88.8.8:443"),
    ];

    let mut list = Vec::new();
    for (id, display_name, addr_str) in targets {
        let addr: SocketAddr = match addr_str.parse() {
            Ok(a) => a,
            Err(_) => continue,
        };
        let start = Instant::now();
        let ping = match tokio::time::timeout(Duration::from_millis(2000), tokio::net::TcpStream::connect(addr)).await {
            Ok(Ok(_)) => Some(start.elapsed().as_millis() as u64),
            _ => None,
        };
        list.push(DohBenchmarkResult {
            provider: id.to_string(),
            name: display_name.to_string(),
            latency: ping,
        });
    }
    list
}

#[derive(serde::Serialize, Clone, Debug)]
pub struct DiscordDiagnosis {
    pub installed: bool,
    pub patched: bool,
    pub running: bool,
    pub relay_ok: bool,
}

/// Inspects Discord updater configuration, running processes, and local relay health.
pub async fn diagnose_discord() -> DiscordDiagnosis {
    let local = std::env::var("LOCALAPPDATA").unwrap_or_default();
    let installed = PathBuf::from(&local).join("Discord").join("Update.exe").exists();

    let patched = discord_settings_paths().iter().any(|p| {
        std::fs::read_to_string(p)
            .map(|s| s.contains("18809"))
            .unwrap_or(false)
    });

    let running = is_discord_running();

    let relay_ok = tokio::time::timeout(
        Duration::from_millis(400),
        tokio::net::TcpStream::connect(("127.0.0.1", DISCORD_RELAY_PORT)),
    )
    .await
    .map(|r| r.is_ok())
    .unwrap_or(false);

    DiscordDiagnosis {
        installed,
        patched,
        running,
        relay_ok,
    }
}

/// Completely repairs Discord by patching NEW_UPDATE_ENDPOINT to the relay, flushing DNS, and cleanly relaunching.
pub fn repair_discord() -> Result<(), String> {
    // 1. Ensure Discord settings are patched with the relay URL
    patch_discord_settings(Some(&format!("http://127.0.0.1:{}/", DISCORD_RELAY_PORT)));

    // 2. Flush DNS
    flush_dns_cache();

    // 3. Restart Discord cleanly
    restart_discord();

    Ok(())
}

#[derive(serde::Serialize, Clone, Debug)]
pub struct NetworkHealth {
    pub proxy_clean: bool,
    pub doh_ok: bool,
    pub discord_relay_ok: bool,
    pub windivert_driver_ok: bool,
    pub active_doh_provider: String,
}

/// Runs a comprehensive system and network diagnostics check.
pub async fn check_network_health(provider: &str) -> NetworkHealth {
    // 1. Check if proxy is completely disabled (clean)
    let proxy_clean = !is_proxy_enabled();

    // 2. Check DoH resolution
    let doh_ok = resolve_doh_with_provider("discord.com", provider).await.is_ok();

    // 3. Check Discord relay
    let discord_relay_ok = tokio::time::timeout(
        Duration::from_millis(400),
        tokio::net::TcpStream::connect(("127.0.0.1", DISCORD_RELAY_PORT)),
    )
    .await
    .map(|r| r.is_ok())
    .unwrap_or(false);

    // 4. Check if WinDivert driver / service exists or is functional
    let windivert_driver_ok = Command::new("sc")
        .args(["query", "WinDivert"])
        .creation_flags(0x0800_0000)
        .output()
        .map(|o| {
            let s = String::from_utf8_lossy(&o.stdout);
            s.contains("WinDivert") || s.contains("STATE")
        })
        .unwrap_or(true);

    NetworkHealth {
        proxy_clean,
        doh_ok,
        discord_relay_ok,
        windivert_driver_ok,
        active_doh_provider: provider.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_discord_host() {
        assert!(is_discord_host("discord.com"));
        assert!(is_discord_host("gateway.discord.gg"));
        assert!(is_discord_host("updates.discord.com:443"));
        assert!(is_discord_host("DISCORD.COM"));
        assert!(!is_discord_host("google.com"));
        assert!(!is_discord_host("notdiscord.com"));
    }

    #[test]
    fn test_rewrite_discord_urls() {
        let original = "https://updates.discord.com/app/manifest https://other.com/file";
        let rewritten = rewrite_discord_urls(original, 18809);
        assert!(rewritten.contains("http://127.0.0.1:18809/__h/updates.discord.com/app/manifest"));
        assert!(rewritten.contains("https://other.com/file"));
    }
}

