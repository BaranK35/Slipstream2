#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod apps;
mod dns;
mod engine;
mod isp;
mod profiles;
mod proxy;

use engine::{Config, Engine};
use serde_json::json;
use std::{
    error::Error,
    net::{IpAddr, SocketAddr},
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, AtomicI64, Ordering},
        Mutex,
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Emitter, Manager, State, WebviewWindow, WindowEvent,
};

static HIDE_ON_BLUR: AtomicBool = AtomicBool::new(true);
static MINIMIZE_TO_TRAY: AtomicBool = AtomicBool::new(true);
static LAST_BLUR_HIDE_MS: AtomicI64 = AtomicI64::new(0);
static LAUNCH_TIME_MS: AtomicI64 = AtomicI64::new(0);
static FILE_DIALOG_ACTIVE: AtomicBool = AtomicBool::new(false);
static TRAY_CLICK_STARTED_OPEN: AtomicBool = AtomicBool::new(false);
static SINGLE_INSTANCE_MUTEX: std::sync::atomic::AtomicIsize = std::sync::atomic::AtomicIsize::new(0);

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

struct AppState {
    engine: Mutex<Engine>,
}

#[tauri::command]
fn catalog() -> serde_json::Value {
    json!({ "isps": isp::ISPS, "apps": profiles::APPS, "strategies": isp::STRATEGIES })
}

#[tauri::command]
fn load_config(s: State<AppState>) -> Config {
    let cfg = Config::load(&s.engine.lock().unwrap().dir);
    HIDE_ON_BLUR.store(cfg.hide_on_blur, Ordering::SeqCst);
    MINIMIZE_TO_TRAY.store(cfg.minimize_to_tray, Ordering::SeqCst);
    cfg
}

#[tauri::command]
fn get_config(s: State<AppState>) -> Config {
    let cfg = Config::load(&s.engine.lock().unwrap().dir);
    HIDE_ON_BLUR.store(cfg.hide_on_blur, Ordering::SeqCst);
    MINIMIZE_TO_TRAY.store(cfg.minimize_to_tray, Ordering::SeqCst);
    cfg
}

#[tauri::command]
fn save_config(s: State<AppState>, cfg: Config) {
    HIDE_ON_BLUR.store(cfg.hide_on_blur, Ordering::SeqCst);
    MINIMIZE_TO_TRAY.store(cfg.minimize_to_tray, Ordering::SeqCst);
    let is_on = s.engine.lock().unwrap().is_on();
    if is_on {
        engine::apply_gaming_network_tweaks(cfg.gaming_mode);
    }
    if !cfg.game_priority_boost {
        restore_all_boosted_game_priorities();
    }
    cfg.save(&s.engine.lock().unwrap().dir);
}

#[tauri::command]
fn set_hide_on_blur(enabled: bool) {
    HIDE_ON_BLUR.store(enabled, Ordering::SeqCst);
}

#[tauri::command]
fn is_engine_on(s: State<AppState>) -> bool {
    s.engine.lock().unwrap().is_on()
}

#[tauri::command]
async fn start(app: tauri::AppHandle, s: State<'_, AppState>, cfg: Config) -> Result<(), String> {
    let res = s.engine.lock().unwrap().start(&cfg);
    if res.is_ok() {
        let _ = app.emit("engine_state_changed", true);
    }
    res
}

#[tauri::command]
async fn stop(app: tauri::AppHandle, s: State<'_, AppState>) -> Result<(), String> {
    s.engine.lock().unwrap().stop();
    let _ = app.emit("engine_state_changed", false);
    Ok(())
}

#[tauri::command]
async fn bench_doh_providers() -> Vec<proxy::DohBenchmarkResult> {
    proxy::benchmark_doh_providers().await
}

#[tauri::command]
async fn diagnose_discord() -> proxy::DiscordDiagnosis {
    proxy::diagnose_discord().await
}

#[tauri::command]
fn repair_discord() -> Result<(), String> {
    proxy::repair_discord()
}

#[tauri::command]
async fn check_network_health(provider: String) -> proxy::NetworkHealth {
    proxy::check_network_health(&provider).await
}

#[tauri::command]
fn hide_window(window: WebviewWindow) {
    LAST_BLUR_HIDE_MS.store(now_ms(), std::sync::atomic::Ordering::SeqCst);
    let _ = window.hide();
}

#[tauri::command]
fn set_file_dialog_active(active: bool) {
    FILE_DIALOG_ACTIVE.store(active, std::sync::atomic::Ordering::SeqCst);
}

#[tauri::command]
fn set_window_expanded(app: tauri::AppHandle, expanded: bool) -> Result<(), String> {
    if let Some(win) = app.get_webview_window("main") {
        let scale = win.scale_factor().unwrap_or(1.0);
        let cur_pos = win.outer_position().unwrap_or(tauri::PhysicalPosition::new(0, 0));
        let cur_size = win.outer_size().unwrap_or(tauri::PhysicalSize::new(
            (380.0 * scale).round() as u32,
            (620.0 * scale).round() as u32,
        ));

        let target_logical_w = if expanded { 820.0 } else { 380.0 };
        let target_phys_w = (target_logical_w * scale).round() as i32;
        let cur_phys_w = cur_size.width as i32;

        let delta_phys_w = target_phys_w - cur_phys_w;

        if delta_phys_w.abs() > 5 {
            let mut new_x = cur_pos.x - delta_phys_w;
            if let Ok(Some(mon)) = win.current_monitor() {
                let min_x = mon.work_area().position.x + 10;
                if new_x < min_x {
                    new_x = min_x;
                }
            } else if new_x < 10 {
                new_x = 10;
            }

            if expanded {
                let _ = win.set_position(tauri::Position::Physical(tauri::PhysicalPosition::new(new_x, cur_pos.y)));
                let _ = win.set_size(tauri::Size::Logical(tauri::LogicalSize::new(target_logical_w, 620.0)));
            } else {
                let _ = win.set_size(tauri::Size::Logical(tauri::LogicalSize::new(target_logical_w, 620.0)));
                let _ = win.set_position(tauri::Position::Physical(tauri::PhysicalPosition::new(new_x, cur_pos.y)));
            }
        }
    }
    Ok(())
}

#[tauri::command]
fn close_app(app: tauri::AppHandle, s: State<AppState>) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.hide();
    }
    if let Some(t) = app.get_webview_window("toast") {
        let _ = t.hide();
    }
    restore_all_boosted_game_priorities();
    engine::apply_gaming_network_tweaks(false);
    {
        if let Ok(mut eng) = s.engine.lock() {
            eng.ensure_relay(false);
            eng.stop();
        }
    }
    Engine::kill_all_winws();
    app.exit(0);
    std::process::exit(0);
}

#[tauri::command]
fn scan_apps() -> Vec<apps::ScannedApp> {
    let procs = get_running_process_names();
    apps::scan_installed_and_running_apps(&procs)
}

#[tauri::command]
fn pick_exe_file() -> Option<String> {
    FILE_DIALOG_ACTIVE.store(true, Ordering::SeqCst);
    let picked = apps::pick_exe_dialog();
    FILE_DIALOG_ACTIVE.store(false, Ordering::SeqCst);
    picked
}

#[tauri::command]
fn reset_dns() {
    dns::reset_all_adapters();
}

#[tauri::command]
fn flush_dns_cache() {
    proxy::flush_dns_cache();
}

#[tauri::command]
fn restart_discord() {
    proxy::restart_discord();
}

#[tauri::command]
fn is_discord_running() -> bool {
    proxy::is_discord_running()
}

#[tauri::command]
fn expand_url(raw: String) -> Vec<String> {
    profiles::expand_domains(&raw)
}

#[tauri::command]
fn set_autostart(enable: bool) -> Result<(), String> {
    engine::set_windows_autostart(enable)
}

#[tauri::command]
fn is_autostart() -> bool {
    engine::is_windows_autostart()
}

fn is_portable_app() -> bool {
    if let Ok(exe_path) = std::env::current_exe() {
        let exe_str = exe_path.to_string_lossy().to_lowercase();
        if exe_str.contains("portable") {
            return true;
        }
        if let Ok(local) = std::env::var("LOCALAPPDATA") {
            let install_dir = std::path::Path::new(&local).join("Programs").join("Slipstream");
            let install_dir_str = install_dir.to_string_lossy().to_lowercase();
            if !exe_str.starts_with(&install_dir_str)
                && !exe_str.contains("program files")
                && !exe_str.contains("target\\debug")
                && !exe_str.contains("target\\release")
            {
                return true;
            }
        }
    }
    false
}

#[tauri::command]
fn is_portable() -> bool {
    is_portable_app()
}

#[derive(Clone, Debug)]
pub struct ProcessItem {
    pub pid: u32,
    pub name: String,
}

#[cfg(target_os = "windows")]
fn get_running_processes() -> Vec<ProcessItem> {
    use std::mem;
    #[allow(non_snake_case)]
    #[repr(C)]
    struct PROCESSENTRY32W {
        dwSize: u32,
        cntUsage: u32,
        th32ProcessID: u32,
        th32DefaultHeapID: usize,
        dwFlags: u32,
        cntThreads: u32,
        th32ParentProcessID: u32,
        pcPriClassBase: i32,
        dwFlags2: u32,
        szExeFile: [u16; 260],
    }
    #[link(name = "kernel32")]
    extern "system" {
        fn CreateToolhelp32Snapshot(dwFlags: u32, th32ProcessID: u32) -> isize;
        fn Process32FirstW(hSnapshot: isize, lppe: *mut PROCESSENTRY32W) -> i32;
        fn Process32NextW(hSnapshot: isize, lppe: *mut PROCESSENTRY32W) -> i32;
        fn CloseHandle(hObject: isize) -> i32;
    }
    const TH32CS_SNAPPROCESS: u32 = 0x00000002;
    let mut procs = Vec::new();
    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
    if snapshot == -1 || snapshot == 0 {
        return procs;
    }
    let mut entry: PROCESSENTRY32W = unsafe { mem::zeroed() };
    entry.dwSize = mem::size_of::<PROCESSENTRY32W>() as u32;

    if unsafe { Process32FirstW(snapshot, &mut entry) } != 0 {
        loop {
            let len = entry.szExeFile.iter().position(|&c| c == 0).unwrap_or(entry.szExeFile.len());
            let exe_name = String::from_utf16_lossy(&entry.szExeFile[..len]);
            procs.push(ProcessItem {
                pid: entry.th32ProcessID,
                name: exe_name,
            });
            if unsafe { Process32NextW(snapshot, &mut entry) } == 0 {
                break;
            }
        }
    }
    unsafe { CloseHandle(snapshot) };
    procs
}

#[cfg(not(target_os = "windows"))]
fn get_running_processes() -> Vec<ProcessItem> {
    vec![]
}

fn get_running_process_names() -> Vec<String> {
    get_running_processes().into_iter().map(|p| p.name).collect()
}

#[allow(dead_code)]
#[cfg(target_os = "windows")]
fn trim_working_set() {
    #[link(name = "kernel32")]
    extern "system" {
        fn GetCurrentProcess() -> isize;
        fn SetProcessWorkingSetSize(hProcess: isize, dwMinimumWorkingSetSize: usize, dwMaximumWorkingSetSize: usize) -> i32;
    }
    unsafe {
        let h_proc = GetCurrentProcess();
        SetProcessWorkingSetSize(h_proc, usize::MAX, usize::MAX);
    }
}

#[allow(dead_code)]
#[cfg(not(target_os = "windows"))]
fn trim_working_set() {}

#[cfg(target_os = "windows")]
fn set_process_priority(pid: u32, priority_class: u32) -> bool {
    #[link(name = "kernel32")]
    extern "system" {
        fn OpenProcess(dwDesiredAccess: u32, bInheritHandle: i32, dwProcessId: u32) -> isize;
        fn SetPriorityClass(hProcess: isize, dwPriorityClass: u32) -> i32;
        fn CloseHandle(hObject: isize) -> i32;
    }
    const PROCESS_SET_INFORMATION: u32 = 0x0200;
    let handle = unsafe { OpenProcess(PROCESS_SET_INFORMATION, 0, pid) };
    if handle != 0 {
        let res = unsafe { SetPriorityClass(handle, priority_class) };
        unsafe { CloseHandle(handle) };
        res != 0
    } else {
        false
    }
}

#[cfg(not(target_os = "windows"))]
fn set_process_priority(_pid: u32, _priority_class: u32) -> bool {
    true
}

static BOOSTED_GAME_PIDS: Mutex<Option<std::collections::HashSet<u32>>> = Mutex::new(None);

fn restore_all_boosted_game_priorities() {
    if let Ok(mut lock) = BOOSTED_GAME_PIDS.lock() {
        if let Some(set) = lock.take() {
            for pid in set {
                set_process_priority(pid, 0x0000_0020); // NORMAL_PRIORITY_CLASS
            }
        }
    }
}

fn manage_game_process_priorities(cfg: &Config, running_procs: &[ProcessItem]) {
    if !cfg.game_priority_boost {
        restore_all_boosted_game_priorities();
        return;
    }

    // 1) Collect game executables ONLY
    let mut game_exes = std::collections::HashSet::new();
    for profile in profiles::APPS {
        if profile.category == "game" {
            for exe in profile.exes {
                game_exes.insert(exe.to_lowercase());
            }
        }
    }
    for plat in &cfg.custom_platforms {
        if plat.category == "game" {
            for exe in &cfg.custom_exes {
                game_exes.insert(exe.to_lowercase());
            }
        }
    }

    // 2) Explicitly forbidden non-game applications (never boost, avoid CPU contention)
    let mut forbidden_exes = std::collections::HashSet::new();
    for profile in profiles::APPS {
        if profile.category == "app" {
            for exe in profile.exes {
                forbidden_exes.insert(exe.to_lowercase());
            }
        }
    }
    forbidden_exes.insert("discord.exe".into());
    forbidden_exes.insert("discordptb.exe".into());
    forbidden_exes.insert("discordcanary.exe".into());
    forbidden_exes.insert("update.exe".into());
    forbidden_exes.insert("spotify.exe".into());
    forbidden_exes.insert("telegram.exe".into());
    forbidden_exes.insert("chrome.exe".into());
    forbidden_exes.insert("firefox.exe".into());
    forbidden_exes.insert("msedge.exe".into());
    forbidden_exes.insert("brave.exe".into());
    forbidden_exes.insert("opera.exe".into());
    forbidden_exes.insert("slipstream.exe".into());
    forbidden_exes.insert("winws.exe".into());
    // Launchers stay normal so they don't fight with active games:
    forbidden_exes.insert("steam.exe".into());
    forbidden_exes.insert("epicgameslauncher.exe".into());
    forbidden_exes.insert("riotclientservices.exe".into());
    forbidden_exes.insert("battle.net.exe".into());
    forbidden_exes.insert("eadesktop.exe".into());
    forbidden_exes.insert("rockstargameslauncher.exe".into());

    let mut lock = match BOOSTED_GAME_PIDS.lock() {
        Ok(l) => l,
        Err(_) => return,
    };
    let boosted = lock.get_or_insert_with(std::collections::HashSet::new);

    let running_pids: std::collections::HashSet<u32> = running_procs.iter().map(|p| p.pid).collect();
    // Prune terminated game PIDs
    boosted.retain(|pid| running_pids.contains(pid));

    // Boost running game processes
    for proc in running_procs {
        let lower = proc.name.to_lowercase();
        if game_exes.contains(&lower) && !forbidden_exes.contains(&lower) {
            if !boosted.contains(&proc.pid) {
                // Apply HIGH_PRIORITY_CLASS (0x00000080)
                if set_process_priority(proc.pid, 0x0000_0080) {
                    boosted.insert(proc.pid);
                }
            }
        }
    }
}

#[allow(dead_code)]
fn is_game_process_running(cfg: &Config, running_procs: &[ProcessItem]) -> bool {
    let mut game_exes = std::collections::HashSet::new();
    for profile in profiles::APPS {
        if profile.category == "game" {
            for exe in profile.exes {
                game_exes.insert(exe.to_lowercase());
            }
        }
    }
    for plat in &cfg.custom_platforms {
        if plat.category == "game" {
            if let Some(ref exe) = plat.exe_name {
                game_exes.insert(exe.to_lowercase());
            }
            for exe in &cfg.custom_exes {
                game_exes.insert(exe.to_lowercase());
            }
        }
    }
    running_procs.iter().any(|p| game_exes.contains(&p.name.to_lowercase()))
}

#[tauri::command]
fn get_detected_active_apps(cfg: Config) -> Vec<String> {
    let running_exes = get_running_process_names();
    let running_lower: std::collections::HashSet<String> = running_exes.iter().map(|e| e.to_lowercase()).collect();
    let mut detected = Vec::new();

    for profile in profiles::APPS {
        for exe in profile.exes {
            if running_lower.contains(&exe.to_lowercase()) {
                detected.push(profile.id.to_string());
                break;
            }
        }
    }
    for plat in &cfg.custom_platforms {
        if let Some(ref exe) = plat.exe_name {
            if running_lower.contains(&exe.to_lowercase()) {
                detected.push(plat.id.clone());
                continue;
            }
        }
        for exe in &cfg.custom_exes {
            if running_lower.contains(&exe.to_lowercase()) {
                detected.push(plat.id.clone());
                break;
            }
        }
    }
    detected
}

#[tauri::command]
fn suggest_related_domains(domain: String) -> profiles::DomainSuggestionResult {
    profiles::get_domain_suggestions(&domain)
}

#[tauri::command]
async fn test_platform_latency(host: String) -> Option<u64> {
    let clean = host
        .trim()
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .split('/')
        .next()
        .unwrap_or(&host)
        .split(':')
        .next()
        .unwrap_or(&host)
        .to_string();
    proxy::measure_latency(&clean, 443).await
}

#[tauri::command]
async fn check_latency() -> serde_json::Value {
    let discord = proxy::measure_latency("updates.discord.com", 443).await;
    let cloudflare = proxy::measure_latency("1.1.1.1", 443).await;
    let google = proxy::measure_latency("dns.google", 443).await;
    json!({
        "discord": discord,
        "cloudflare": cloudflare,
        "google": google,
    })
}

/// "outer -> inner -> innermost" text of an error, so the real cause is visible.
fn chain(e: &(dyn Error + 'static)) -> String {
    let (mut s, mut cur) = (e.to_string(), e.source());
    while let Some(x) = cur {
        s.push_str(" -> ");
        s.push_str(&x.to_string());
        cur = x.source();
    }
    s
}

/// Tests one service layer by layer:
/// 1) DNS through DoH (Cloudflare / Google / Quad9)
/// 2) TCP connect to resolved IP
/// 3) TLS + HTTPS through the desynced connection.
#[allow(dead_code)]
async fn probe_one(url: &str) -> Option<String> {
    let host = url
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .split('/')
        .next()
        .unwrap_or("")
        .to_string();

    let clean_host = host.split(':').next().unwrap_or(&host).to_string();

    // 1) Resolve via DoH
    let ip = match proxy::resolve_doh(&clean_host).await {
        Ok(ip) => ip,
        Err(e) => return Some(format!("DoH DNS hatası ({clean_host}): {e}")),
    };
    let addr = SocketAddr::new(IpAddr::V4(ip), 443);

    // 2) TCP connect
    match tokio::time::timeout(Duration::from_millis(3000), tokio::net::TcpStream::connect(addr)).await {
        Ok(Ok(_)) => {}
        Ok(Err(e)) => return Some(format!("TCP bağlantı hatası ({addr}): {e}")),
        Err(_) => return Some(format!("TCP zaman aşımı ({addr})")),
    }

    // 3) TLS + HTTP GET
    let client = match reqwest::Client::builder()
        .timeout(Duration::from_millis(3500))
        .no_proxy()
        .pool_max_idle_per_host(0)
        .resolve(&clean_host, addr)
        .build()
    {
        Ok(c) => c,
        Err(e) => return Some(format!("Test istemcisi oluşturulamadı: {e}")),
    };

    match client.get(url).send().await {
        Ok(resp) => {
            let status = resp.status().as_u16();
            if (200..=399).contains(&status) {
                None // Reached successfully!
            } else {
                Some(format!("HTTP Yanıt Kodu: {status}"))
            }
        }
        Err(e) => Some(format!("TLS/DPI engeli ({addr}): {}", chain(&e))),
    }
}

/// None = every service reachable, otherwise the first failure reason.
#[allow(dead_code)]
async fn probe(urls: &[String]) -> Option<String> {
    for u in urls {
        if let Some(why) = probe_one(u).await {
            return Some(why);
        }
    }
    None
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct UpdateInfo {
    pub has_update: bool,
    pub current_version: String,
    pub latest_version: String,
    pub release_notes: String,
    pub download_url: String,
}

fn is_newer_version(latest: &str, current: &str) -> bool {
    let clean_l = latest.trim().trim_start_matches('v');
    let clean_c = current.trim().trim_start_matches('v');
    let norm_l = clean_l.replace(['+', '-'], ".");
    let norm_c = clean_c.replace(['+', '-'], ".");
    let mut l_parts: Vec<u64> = norm_l.split('.').filter_map(|s| s.parse().ok()).collect();
    let mut c_parts: Vec<u64> = norm_c.split('.').filter_map(|s| s.parse().ok()).collect();
    let max_len = l_parts.len().max(c_parts.len()).max(3);
    while l_parts.len() < max_len {
        l_parts.push(0);
    }
    while c_parts.len() < max_len {
        c_parts.push(0);
    }
    if !l_parts.is_empty() && !c_parts.is_empty() {
        return l_parts > c_parts;
    }
    clean_l != clean_c
}

#[tauri::command]
fn open_url(url: String) -> Result<(), String> {
    if url.starts_with("https://") || url.starts_with("http://") {
        #[cfg(target_os = "windows")]
        {
            use std::os::windows::process::CommandExt;
            let _ = std::process::Command::new("rundll32.exe")
                .args(["url.dll,FileProtocolHandler", &url])
                .creation_flags(0x0800_0000)
                .spawn();
        }
        Ok(())
    } else {
        Err("Geçersiz URL formatı".into())
    }
}

fn base64_encode(bytes: &[u8]) -> String {
    const CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    let mut i = 0;
    while i < bytes.len() {
        let b0 = bytes[i];
        let b1 = if i + 1 < bytes.len() { bytes[i + 1] } else { 0 };
        let b2 = if i + 2 < bytes.len() { bytes[i + 2] } else { 0 };

        out.push(CHARSET[(b0 >> 2) as usize] as char);
        out.push(CHARSET[(((b0 & 0x03) << 4) | (b1 >> 4)) as usize] as char);
        if i + 1 < bytes.len() {
            out.push(CHARSET[(((b1 & 0x0F) << 2) | (b2 >> 6)) as usize] as char);
        } else {
            out.push('=');
        }
        if i + 2 < bytes.len() {
            out.push(CHARSET[(b2 & 0x3F) as usize] as char);
        } else {
            out.push('=');
        }
        i += 3;
    }
    out
}

fn write_utf16_file(path: &std::path::Path, content: &str) -> std::io::Result<()> {
    let mut bytes = vec![0xFF, 0xFE]; // UTF-16 LE BOM
    for u in content.encode_utf16() {
        bytes.extend_from_slice(&u.to_le_bytes());
    }
    std::fs::write(path, bytes)
}

#[cfg(target_os = "windows")]
fn query_registry_string(hkey: isize, subkey: &str, value_name: &str) -> Option<String> {
    #[link(name = "advapi32")]
    extern "system" {
        fn RegOpenKeyExW(
            hKey: isize,
            lpSubKey: *const u16,
            ulOptions: u32,
            samDesired: u32,
            phkResult: *mut isize,
        ) -> i32;
        fn RegQueryValueExW(
            hKey: isize,
            lpValueName: *const u16,
            lpReserved: *mut u32,
            lpType: *mut u32,
            lpData: *mut u8,
            lpcbData: *mut u32,
        ) -> i32;
        fn RegCloseKey(hKey: isize) -> i32;
    }

    let subkey_wide: Vec<u16> = subkey.encode_utf16().chain(std::iter::once(0)).collect();
    let value_wide: Vec<u16> = if value_name.is_empty() {
        vec![0]
    } else {
        value_name.encode_utf16().chain(std::iter::once(0)).collect()
    };

    let mut key_handle: isize = 0;
    const KEY_READ: u32 = 0x20019;
    unsafe {
        if RegOpenKeyExW(hkey, subkey_wide.as_ptr(), 0, KEY_READ, &mut key_handle) != 0 {
            return None;
        }
        let mut data_type: u32 = 0;
        let mut byte_len: u32 = 0;
        if RegQueryValueExW(key_handle, value_wide.as_ptr(), std::ptr::null_mut(), &mut data_type, std::ptr::null_mut(), &mut byte_len) != 0 || byte_len == 0 {
            RegCloseKey(key_handle);
            return None;
        }
        let mut buffer = vec![0u8; byte_len as usize];
        if RegQueryValueExW(key_handle, value_wide.as_ptr(), std::ptr::null_mut(), &mut data_type, buffer.as_mut_ptr(), &mut byte_len) != 0 {
            RegCloseKey(key_handle);
            return None;
        }
        RegCloseKey(key_handle);

        let u16_len = (byte_len as usize) / 2;
        let u16_slice: &[u16] = std::slice::from_raw_parts(buffer.as_ptr() as *const u16, u16_len);
        let s = String::from_utf16_lossy(u16_slice);
        Some(s.trim_matches('\0').trim().to_string())
    }
}

fn resolve_install_paths() -> (PathBuf, PathBuf) {
    let mut target_exe = std::env::current_exe().unwrap_or_default();
    let mut install_dir = target_exe.parent().map(|p| p.to_path_buf()).unwrap_or_default();

    let exe_str = target_exe.to_string_lossy().to_lowercase();
    let is_dev_path = exe_str.contains("target\\release") || exe_str.contains("target\\debug");

    if is_dev_path || !target_exe.exists() {
        #[cfg(target_os = "windows")]
        {
            const HKEY_CURRENT_USER: isize = 0x80000001u32 as i32 as isize;
            // 1) Query HKCU\Software\slipstream\Slipstream (Default install dir)
            if let Some(reg_dir) = query_registry_string(HKEY_CURRENT_USER, r"Software\slipstream\Slipstream", "") {
                let p = PathBuf::from(reg_dir.trim_matches('"'));
                if p.exists() {
                    install_dir = p.clone();
                    let candidate = p.join("slipstream.exe");
                    if candidate.exists() {
                        target_exe = candidate;
                    } else {
                        target_exe = p.join("Slipstream.exe");
                    }
                }
            }
            // 2) Query HKCU\Software\Microsoft\Windows\CurrentVersion\Uninstall\Slipstream -> InstallLocation
            if (!target_exe.exists() || is_dev_path) && !install_dir.exists() {
                if let Some(reg_dir) = query_registry_string(
                    HKEY_CURRENT_USER,
                    r"Software\Microsoft\Windows\CurrentVersion\Uninstall\Slipstream",
                    "InstallLocation",
                ) {
                    let p = PathBuf::from(reg_dir.trim_matches('"'));
                    if p.exists() {
                        install_dir = p.clone();
                        let candidate = p.join("slipstream.exe");
                        if candidate.exists() {
                            target_exe = candidate;
                        } else {
                            target_exe = p.join("Slipstream.exe");
                        }
                    }
                }
            }
        }
        // 3) Fallback to %LOCALAPPDATA%\Slipstream
        if !target_exe.exists() {
            if let Ok(local) = std::env::var("LOCALAPPDATA") {
                let p = PathBuf::from(local).join("Slipstream");
                install_dir = p.clone();
                target_exe = p.join("slipstream.exe");
            }
        }
    }

    (target_exe, install_dir)
}

const APP_VERSION: &str = "0.1.8";

#[tauri::command]
async fn check_for_updates() -> UpdateInfo {
    let current_version = APP_VERSION.to_string();
    let client = match reqwest::Client::builder()
        .user_agent("Slipstream-Updater/1.0")
        .timeout(Duration::from_secs(12))
        .build()
    {
        Ok(c) => c,
        Err(_) => return UpdateInfo {
            has_update: false,
            current_version: current_version.clone(),
            latest_version: current_version,
            release_notes: "".into(),
            download_url: "".into(),
        },
    };

    // 1) Try raw version.json from raw.githubusercontent.com and github.com/raw with cache-busting timestamp
    let now_ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let version_urls = [
        format!("https://raw.githubusercontent.com/BaranK35/Slipstream2/main/version.json?t={now_ts}"),
        format!("https://github.com/BaranK35/Slipstream2/raw/main/version.json?t={now_ts}"),
        format!("https://raw.githubusercontent.com/BaranK35/Slipstreamtest2/main/version.json?t={now_ts}"),
        format!("https://github.com/BaranK35/Slipstreamtest2/raw/main/version.json?t={now_ts}"),
    ];

    for u in version_urls {
        if let Ok(resp) = client
            .get(&u)
            .header("Cache-Control", "no-cache")
            .header("Pragma", "no-cache")
            .send()
            .await
        {
            if resp.status().is_success() {
                if let Ok(text) = resp.text().await {
                    if let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) {
                        if let Some(latest) = json.get("version").and_then(|v| v.as_str()) {
                            let has = is_newer_version(latest, &current_version);
                            let notes = json.get("notes").and_then(|v| v.as_str()).unwrap_or("Hata düzeltmeleri ve performans iyileştirmeleri.");
                            let url = json.get("url").and_then(|v| v.as_str()).unwrap_or("");
                            if has {
                                let mut valid_asset = false;
                                if !url.is_empty() {
                                    if let Ok(head) = client.head(url).send().await {
                                        if head.status().is_success() {
                                            let cl = head.headers()
                                                .get("content-length")
                                                .and_then(|v| v.to_str().ok())
                                                .and_then(|s| s.parse::<u64>().ok())
                                                .unwrap_or(0);
                                            if cl >= 2_000_000 {
                                                valid_asset = true;
                                            }
                                        }
                                    }
                                }
                                if valid_asset {
                                    return UpdateInfo {
                                        has_update: true,
                                        current_version: current_version.clone(),
                                        latest_version: latest.to_string(),
                                        release_notes: notes.to_string(),
                                        download_url: url.to_string(),
                                    };
                                } else {
                                    // Asset is still being uploaded, prevent update loop
                                    return UpdateInfo {
                                        has_update: false,
                                        current_version: current_version.clone(),
                                        latest_version: current_version,
                                        release_notes: "".into(),
                                        download_url: "".into(),
                                    };
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // 2) Fallback to GitHub Releases API (releases/latest)
    let api_urls = [
        "https://api.github.com/repos/BaranK35/Slipstream2/releases/latest",
        "https://api.github.com/repos/BaranK35/Slipstreamtest2/releases/latest",
    ];
    for api_url in api_urls {
        if let Ok(resp) = client
            .get(api_url)
            .header("Cache-Control", "no-cache")
            .header("Pragma", "no-cache")
            .send()
            .await
        {
            if resp.status().is_success() {
                if let Ok(text) = resp.text().await {
                    if let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) {
                        if let Some(tag) = json.get("tag_name").and_then(|v| v.as_str()) {
                            let latest = tag.trim_start_matches('v').to_string();
                            let has = is_newer_version(&latest, &current_version);
                            let notes = json.get("body").and_then(|v| v.as_str()).unwrap_or("Yeni sürüm GitHub üzerinde yayınlandı.");
                            let mut dl_url = String::new();
                            let mut asset_ready = false;
                            if let Some(assets) = json.get("assets").and_then(|a| a.as_array()) {
                                for asset in assets {
                                    if let (Some(name), Some(browser_url), Some(state), Some(size)) = (
                                        asset.get("name").and_then(|n| n.as_str()),
                                        asset.get("browser_download_url").and_then(|u| u.as_str()),
                                        asset.get("state").and_then(|s| s.as_str()),
                                        asset.get("size").and_then(|s| s.as_u64()),
                                    ) {
                                        if name.ends_with(".exe") && (name.contains("setup") || name.contains("Setup")) && state == "uploaded" && size >= 2_000_000 {
                                            dl_url = browser_url.to_string();
                                            asset_ready = true;
                                            break;
                                        }
                                    }
                                }
                            }
                            if has && asset_ready {
                                return UpdateInfo {
                                    has_update: true,
                                    current_version: current_version.clone(),
                                    latest_version: latest,
                                    release_notes: notes.to_string(),
                                    download_url: dl_url,
                                };
                            }
                        }
                    }
                }
            }
        }
    }

    UpdateInfo {
        has_update: false,
        current_version: current_version.clone(),
        latest_version: current_version,
        release_notes: "".into(),
        download_url: "".into(),
    }
}

#[tauri::command]
async fn apply_update(app: tauri::AppHandle, s: State<'_, AppState>, download_url: String) -> Result<(), String> {
    if download_url.is_empty() {
        return Err("Geçerli bir indirme bağlantısı bulunamadı.".into());
    }

    let client = reqwest::Client::builder()
        .user_agent("Slipstream-Updater/1.0")
        .timeout(Duration::from_secs(120))
        .build()
        .map_err(|e| e.to_string())?;

    let resp = match client.get(&download_url).send().await {
        Ok(r) if r.status().is_success() => Ok(r),
        _ => {
            let alt_url = if download_url.contains("Slipstreamtest2") {
                download_url.replace("Slipstreamtest2", "Slipstream2")
            } else if download_url.contains("Slipstream2") {
                download_url.replace("Slipstream2", "Slipstreamtest2")
            } else {
                download_url.clone()
            };
            client.get(&alt_url).send().await
        }
    };

    let bytes = resp
        .map_err(|e| e.to_string())?
        .bytes()
        .await
        .map_err(|e| e.to_string())?;

    if bytes.len() < 2_000_000 {
        return Err("İndirilen güncelleme paketi eksik veya henüz GitHub üzerinde tamamlanmadı.".into());
    }

    let temp_dir = std::env::temp_dir();
    let installer_path = temp_dir.join("Slipstream-Update.exe");
    std::fs::write(&installer_path, bytes).map_err(|e| e.to_string())?;

    let (target_exe, install_dir) = resolve_install_paths();

    // 1) Stop DPI engine and Discord relay cleanly in this process before spawning updater
    {
        let mut eng = s.engine.lock().unwrap();
        eng.ensure_relay(false);
        eng.stop();
    }

    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;

        let current_pid = std::process::id();
        let target_str = target_exe.to_string_lossy();
        let install_str = install_dir.to_string_lossy();
        let installer_str = installer_path.to_string_lossy();

        // 100% silent background update script:
        // - No console window creation (CREATE_NO_WINDOW)
        // - Zero focus stealing (processes run Hidden; restarts with --minimized)
        // - Waits for current process PID to exit cleanly
        // - Terminates lingering processes and verifies target executable lock release
        // - Safety rename fallback (.old) if target file is temporarily held by external handles
        // - Executes NSIS silently with target directory: /S /UPDATE /D=...
        // - Restarts the updated application minimized to system tray without stealing game focus
        // - Cleans up installer binary and temporary files
        let ps_code = format!(
            "$targetExe = '{target}';\
            $installDir = '{dir}';\
            $installer = '{inst}';\
            $parentPid = {pid};\
            if ($parentPid -gt 0) {{ Wait-Process -Id $parentPid -Timeout 6 -ErrorAction SilentlyContinue; }}\
            Stop-Process -Name 'slipstream', 'winws' -Force -ErrorAction SilentlyContinue;\
            $unlocked = $false;\
            for ($i = 0; $i -lt 30; $i++) {{\
                try {{\
                    if (Test-Path -LiteralPath $targetExe) {{\
                        $s = [System.IO.File]::Open($targetExe, [System.IO.FileMode]::Open, [System.IO.FileAccess]::ReadWrite, [System.IO.FileShare]::None);\
                        $s.Close();\
                        $s.Dispose();\
                    }}\
                    $unlocked = $true;\
                    break;\
                }} catch {{\
                    Start-Sleep -Milliseconds 250;\
                }}\
            }};\
            if (-not $unlocked -and (Test-Path -LiteralPath $targetExe)) {{\
                $old = \"$targetExe.old\";\
                Remove-Item -LiteralPath $old -Force -ErrorAction SilentlyContinue;\
                Rename-Item -LiteralPath $targetExe -NewName \"$targetExe.old\" -Force -ErrorAction SilentlyContinue;\
            }};\
            $instArgs = if ($installDir -ne '') {{ \"/S /UPDATE /D=$installDir\" }} else {{ \"/S /UPDATE\" }};\
            Start-Process -FilePath $installer -ArgumentList $instArgs -Wait -WindowStyle Hidden;\
            Remove-Item -LiteralPath \"$targetExe.old\" -Force -ErrorAction SilentlyContinue;\
            Remove-Item -LiteralPath $installer -Force -ErrorAction SilentlyContinue;\
            if (Test-Path -LiteralPath $targetExe) {{\
                Start-Process -FilePath $targetExe -ArgumentList '--minimized' -WindowStyle Hidden;\
            }} elseif (Test-Path -LiteralPath \"$installDir\\slipstream.exe\") {{\
                Start-Process -FilePath \"$installDir\\slipstream.exe\" -ArgumentList '--minimized' -WindowStyle Hidden;\
            }}",
            target = target_str.replace('\'', "''"),
            dir = install_str.replace('\'', "''"),
            inst = installer_str.replace('\'', "''"),
            pid = current_pid
        );

        let utf16_bytes: Vec<u8> = ps_code.encode_utf16().flat_map(|u| u.to_le_bytes()).collect();
        let encoded_cmd = base64_encode(&utf16_bytes);

        let mut ps_cmd = std::process::Command::new("powershell.exe");
        ps_cmd.args([
            "-WindowStyle", "Hidden",
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy", "Bypass",
            "-EncodedCommand", &encoded_cmd,
        ]);
        ps_cmd.creation_flags(CREATE_NO_WINDOW);

        if ps_cmd.spawn().is_err() {
            // Secondary VBScript fallback executed via wscript.exe
            let vbs_script = temp_dir.join("slipstream_finish_update.vbs");
            let vbs_content = format!(
                "Option Explicit\r\n\
                On Error Resume Next\r\n\
                Dim WshShell, fso, targetExe, installDir, installerPath, exeToLaunch, i, fileTest, instCmd\r\n\
                Set WshShell = CreateObject(\"WScript.Shell\")\r\n\
                Set fso = CreateObject(\"Scripting.FileSystemObject\")\r\n\
                targetExe = \"{}\"\r\n\
                installDir = \"{}\"\r\n\
                installerPath = \"{}\"\r\n\
                WScript.Sleep 1500\r\n\
                WshShell.Run \"taskkill /F /IM slipstream.exe /T\", 0, True\r\n\
                WshShell.Run \"taskkill /F /IM winws.exe /T\", 0, True\r\n\
                If targetExe <> \"\" Then\r\n\
                    For i = 1 To 20\r\n\
                        Err.Clear\r\n\
                        If fso.FileExists(targetExe) Then\r\n\
                            Set fileTest = fso.OpenTextFile(targetExe, 8, False)\r\n\
                            If Err.Number = 0 Then\r\n\
                                fileTest.Close\r\n\
                                Exit For\r\n\
                            End If\r\n\
                        Else\r\n\
                            Exit For\r\n\
                        End If\r\n\
                        WScript.Sleep 500\r\n\
                    Next\r\n\
                End If\r\n\
                If installDir <> \"\" Then\r\n\
                    instCmd = \"\"\"\" & installerPath & \"\"\" /S /UPDATE /D=\" & installDir\r\n\
                Else\r\n\
                    instCmd = \"\"\"\" & installerPath & \"\"\" /S /UPDATE\"\r\n\
                End If\r\n\
                WshShell.Run instCmd, 0, True\r\n\
                WScript.Sleep 1000\r\n\
                exeToLaunch = \"\"\r\n\
                If targetExe <> \"\" And fso.FileExists(targetExe) Then\r\n\
                    exeToLaunch = targetExe\r\n\
                ElseIf installDir <> \"\" And fso.FileExists(installDir & \"\\slipstream.exe\") Then\r\n\
                    exeToLaunch = installDir & \"\\slipstream.exe\"\r\n\
                End If\r\n\
                If exeToLaunch <> \"\" Then\r\n\
                    WshShell.Run \"\"\"\" & exeToLaunch & \"\"\" --minimized\", 0, False\r\n\
                End If\r\n\
                WScript.Sleep 2000\r\n\
                fso.DeleteFile installerPath, True\r\n\
                fso.DeleteFile WScript.ScriptFullName, True\r\n",
                target_str,
                install_str,
                installer_str
            );
            let _ = write_utf16_file(&vbs_script, &vbs_content);
            let mut vbs_cmd = std::process::Command::new("wscript.exe");
            vbs_cmd.arg(&vbs_script);
            vbs_cmd.creation_flags(CREATE_NO_WINDOW);
            let _ = vbs_cmd.spawn();
        }

        // Clean any leftover cmd file from older versions
        let old_cmd = temp_dir.join("slipstream_finish_update.cmd");
        if old_cmd.exists() {
            let _ = std::fs::remove_file(old_cmd);
        }
    }

    app.exit(0);
    std::process::exit(0);
}

/// Comprehensive Auto-Tune: Tests major services live against each candidate strategy,
/// measuring packet bypass success and ping, then picks and applies the best strategy.
#[tauri::command]
async fn autotune(
    app: tauri::AppHandle,
    s: State<'_, AppState>,
    mut cfg: Config,
) -> Result<Option<String>, String> {
    // Benchmark targets covering Discord, YouTube, Roblox, X, and user targets
    let mut targets = vec![
        "https://discord.com".to_string(),
        "https://gateway.discord.gg".to_string(),
        "https://www.youtube.com".to_string(),
        "https://www.roblox.com".to_string(),
        "https://x.com".to_string(),
    ];
    for u in profiles::probe_urls(&cfg) {
        if !targets.contains(&u) {
            targets.push(u);
        }
    }

    let candidate_ids = isp::order(&cfg.isp);
    let mut best_id: Option<String> = None;
    let mut best_score = -1;
    let mut best_latency = u64::MAX;
    let mut best_name = String::new();

    let total = targets.len();
    let _ = app.emit("autotune", format!("🚀 En iyi ayar testi başlatıldı: {total} kritik servis canlı test edilecek."));

    for id in candidate_ids {
        let name = isp::strategy(id).name;
        let _ = app.emit("autotune", format!("⚡ Deneniyor: {name}..."));
        cfg.strategy = Some(id.to_string());
        {
            s.engine.lock().unwrap().start(&cfg)?;
        }
        tokio::time::sleep(Duration::from_millis(600)).await;

        let mut success_count = 0;
        let mut total_latency: u64 = 0;

        for target in &targets {
            let start_t = std::time::Instant::now();
            let res = probe_one(target).await;
            let elapsed = start_t.elapsed().as_millis() as u64;

            if res.is_none() {
                success_count += 1;
                total_latency += elapsed;
            }
        }

        let avg_latency = if success_count > 0 {
            total_latency / (success_count as u64)
        } else {
            9999
        };

        let _ = app.emit(
            "autotune",
            format!("📊 {name}: {success_count}/{total} servis başarılı (Gecikme: {avg_latency}ms)"),
        );

        if success_count > best_score || (success_count == best_score && avg_latency < best_latency) {
            best_score = success_count;
            best_latency = avg_latency;
            best_id = Some(id.to_string());
            best_name = name.to_string();
        }

        // If 100% of sites succeeded with low ping, we found our optimal winner
        if success_count == total as i32 && avg_latency < 80 {
            break;
        }
    }

    if let Some(winner) = best_id {
        cfg.strategy = Some(winner.clone());
        s.engine.lock().unwrap().start(&cfg)?;
        cfg.save(&s.engine.lock().unwrap().dir);
        let _ = app.emit(
            "autotune",
            format!("🏆 Mükemmel strateji bulundu ve aktif edildi: {best_name} ({best_score}/{total} servis, {best_latency}ms)"),
        );
        Ok(Some(winner))
    } else {
        s.engine.lock().unwrap().stop();
        Ok(None)
    }
}

/// Positions window in bottom-right corner of current work area (cleanly above taskbar).
fn position_bottom_right(w: &WebviewWindow) -> Result<(), Box<dyn std::error::Error>> {
    let monitor = w
        .current_monitor()
        .ok()
        .flatten()
        .or_else(|| w.primary_monitor().ok().flatten())
        .or_else(|| w.available_monitors().ok().and_then(|v| v.into_iter().next()));

    if let Some(m) = monitor {
        let work_area = m.work_area();
        let scale = m.scale_factor();
        
        let _ = w.set_size(tauri::Size::Logical(tauri::LogicalSize::new(380.0, 620.0)));
        let win_width = (380.0 * scale).round() as i32;
        let win_height = (620.0 * scale).round() as i32;

        let margin_x = (16.0 * scale).round() as i32;
        let margin_y = (16.0 * scale).round() as i32;

        let mut x = work_area.position.x + work_area.size.width as i32 - win_width - margin_x;
        let mut y = work_area.position.y + work_area.size.height as i32 - win_height - margin_y;

        if x < work_area.position.x {
            x = work_area.position.x;
        }
        if y < work_area.position.y {
            y = work_area.position.y;
        }

        let _ = w.set_position(tauri::Position::Physical(tauri::PhysicalPosition::new(x, y)));
    }
    Ok(())
}

/// Positions the toast notification window in the bottom-right corner directly above taskbar.
fn position_toast_bottom_right(w: &WebviewWindow) -> Result<(), Box<dyn std::error::Error>> {
    let monitor = w
        .current_monitor()
        .ok()
        .flatten()
        .or_else(|| w.primary_monitor().ok().flatten())
        .or_else(|| w.available_monitors().ok().and_then(|v| v.into_iter().next()));

    if let Some(m) = monitor {
        let work_area = m.work_area();
        let scale = m.scale_factor();

        let (win_width, win_height) = match w.outer_size() {
            Ok(s) if s.width > 50 && s.height > 20 => (s.width as i32, s.height as i32),
            _ => ((360.0 * scale).round() as i32, (92.0 * scale).round() as i32),
        };

        let margin_x = (16.0 * scale).round() as i32;
        let margin_y = (16.0 * scale).round() as i32;

        let mut x = work_area.position.x + work_area.size.width as i32 - win_width - margin_x;
        let mut y = work_area.position.y + work_area.size.height as i32 - win_height - margin_y;

        if x < work_area.position.x {
            x = work_area.position.x;
        }
        if y < work_area.position.y {
            y = work_area.position.y;
        }

        let _ = w.set_position(tauri::Position::Physical(tauri::PhysicalPosition::new(x, y)));
    }
    Ok(())
}

#[tauri::command]
fn open_main_from_toast(app: tauri::AppHandle) {
    if let Some(toast_w) = app.get_webview_window("toast") {
        let _ = toast_w.hide();
    }
    if let Some(main_w) = app.get_webview_window("main") {
        let _ = position_bottom_right(&main_w);
        let _ = main_w.show();
        let _ = main_w.set_focus();
    }
}

#[tauri::command]
fn hide_toast_notification(app: tauri::AppHandle) {
    if let Some(toast_w) = app.get_webview_window("toast") {
        let _ = toast_w.hide();
    }
}

#[tauri::command]
fn show_toast_notification(app: tauri::AppHandle, message: Option<String>) {
    if let Some(toast_w) = app.get_webview_window("toast") {
        let _ = position_toast_bottom_right(&toast_w);
        if let Some(ref msg) = message {
            let _ = toast_w.emit("toast_message", msg.clone());
        }
        let _ = toast_w.show();
    }
}

fn toggle_window(w: &WebviewWindow) {
    if let Ok(vis) = w.is_visible() {
        if vis {
            LAST_BLUR_HIDE_MS.store(now_ms(), Ordering::SeqCst);
            let _ = w.hide();
        } else {
            let _ = position_bottom_right(w);
            let _ = w.show();
            let _ = w.set_focus();
        }
    }
}

fn main() {
    #[cfg(target_os = "windows")]
    {
        use std::ptr;
        #[link(name = "kernel32")]
        extern "system" {
            fn CreateMutexW(lpMutexAttributes: *mut std::ffi::c_void, bInitialOwner: i32, lpName: *const u16) -> isize;
            fn GetLastError() -> u32;
            fn CreateEventW(lpEventAttributes: *mut std::ffi::c_void, bManualReset: i32, bInitialState: i32, lpName: *const u16) -> isize;
            fn SetEvent(hEvent: isize) -> i32;
            fn CloseHandle(hObject: isize) -> i32;
        }
        #[link(name = "user32")]
        extern "system" {
            fn FindWindowW(lpClassName: *const u16, lpWindowName: *const u16) -> isize;
            fn SetForegroundWindow(hWnd: isize) -> i32;
            fn ShowWindow(hWnd: isize, nCmdShow: i32) -> i32;
        }

        let mutex_name: Vec<u16> = "Local\\Slipstream_SingleInstance_Mutex\0".encode_utf16().collect();
        let event_name: Vec<u16> = "Local\\Slipstream_Wakeup_Event\0".encode_utf16().collect();

        let mutex_handle = unsafe { CreateMutexW(ptr::null_mut(), 1, mutex_name.as_ptr()) };
        const ERROR_ALREADY_EXISTS: u32 = 183;
        if mutex_handle != 0 && unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
            // Another instance is already running!
            let ev_handle = unsafe { CreateEventW(ptr::null_mut(), 0, 0, event_name.as_ptr()) };
            if ev_handle != 0 {
                unsafe {
                    SetEvent(ev_handle);
                    CloseHandle(ev_handle);
                }
            }
            let win_name: Vec<u16> = "Slipstream\0".encode_utf16().collect();
            let hwnd = unsafe { FindWindowW(ptr::null_mut(), win_name.as_ptr()) };
            if hwnd != 0 {
                unsafe {
                    ShowWindow(hwnd, 9); // SW_RESTORE
                    SetForegroundWindow(hwnd);
                }
            }
            std::process::exit(0);
        }
        SINGLE_INSTANCE_MUTEX.store(mutex_handle, Ordering::SeqCst);
    }

    tauri::Builder::default()
        .setup(|app| {
            #[cfg(target_os = "windows")]
            {
                use std::ptr;
                #[link(name = "kernel32")]
                extern "system" {
                    fn CreateEventW(lpEventAttributes: *mut std::ffi::c_void, bManualReset: i32, bInitialState: i32, lpName: *const u16) -> isize;
                    fn WaitForSingleObject(hHandle: isize, dwMilliseconds: u32) -> u32;
                    fn CloseHandle(hObject: isize) -> i32;
                }

                let app_handle = app.handle().clone();
                std::thread::spawn(move || {
                    let event_name: Vec<u16> = "Local\\Slipstream_Wakeup_Event\0".encode_utf16().collect();
                    let ev_handle = unsafe { CreateEventW(ptr::null_mut(), 0, 0, event_name.as_ptr()) };
                    if ev_handle != 0 {
                        const INFINITE: u32 = 0xFFFF_FFFF;
                        loop {
                            let wait_res = unsafe { WaitForSingleObject(ev_handle, INFINITE) };
                            if wait_res == 0 {
                                if let Some(w) = app_handle.get_webview_window("main") {
                                    let _ = position_bottom_right(&w);
                                    let _ = w.show();
                                    let _ = w.unminimize();
                                    let _ = w.set_focus();
                                }
                            } else {
                                break;
                            }
                        }
                        unsafe { CloseHandle(ev_handle) };
                    }
                });
            }

            let dir = app
                .path()
                .app_data_dir()
                .unwrap_or_else(|_| PathBuf::from("data"));
            
            // Clean any stale adapter DNS & proxy configuration on boot
            dns::recover(&dir);
            proxy::clean_stale_proxy();

            let exe_dir = std::env::current_exe()
                .ok()
                .and_then(|p| p.parent().map(|d| d.to_path_buf()))
                .unwrap_or_else(|| PathBuf::from("."));

            let is_portable_mode = is_portable_app();

            // Resolve engine directory:
            // For portable mode or when local engine folder is missing next to executable,
            // use %LOCALAPPDATA%\Slipstream\engine (or %TEMP%\Slipstream\engine)
            // and unpack embedded engine files so that NO files are created in the portable folder!
            let engine_dir = if is_portable_mode || !exe_dir.join("engine").join("winws.exe").exists() {
                let local_dir = std::env::var("LOCALAPPDATA")
                    .map(|l| PathBuf::from(l).join("Slipstream").join("engine"))
                    .unwrap_or_else(|_| std::env::temp_dir().join("Slipstream").join("engine"));
                let _ = engine::ensure_engine_files(&local_dir);
                local_dir
            } else {
                let p = exe_dir.join("engine");
                let _ = engine::ensure_engine_files(&p);
                p
            };

            let bin = engine_dir.join("winws.exe");

            let boot_cfg = Config::load(&dir);
            HIDE_ON_BLUR.store(boot_cfg.hide_on_blur, Ordering::SeqCst);
            MINIMIZE_TO_TRAY.store(boot_cfg.minimize_to_tray, Ordering::SeqCst);
            let has_discord = boot_cfg.enabled_apps.contains(&"discord".to_string());

            Engine::kill_all_winws();
            let mut engine = Engine::new(dir, bin);
            engine.ensure_relay(has_discord);

            // Auto-connect bypass on launch if configured
            if boot_cfg.auto_connect {
                let _ = engine.start(&boot_cfg);
            }

            app.manage(AppState {
                engine: Mutex::new(engine),
            });

            // Background self-healing watchdog for winws, adaptive bypass, and 3. Yöntem process detector
            let app_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                let mut last_detected_apps: Vec<String> = Vec::new();
                let mut auto_probe_cycle: u32 = 0;
                loop {
                    tokio::time::sleep(Duration::from_secs(3)).await;
                    if let Some(state) = app_handle.try_state::<AppState>() {
                        let (should_probe, eng_dir, current_isp, current_strat) = {
                            let mut eng = state.engine.lock().unwrap();
                            let cfg = Config::load(&eng.dir);

                            // 1) Auto-healing watchdog
                            if cfg.auto_healing && eng.is_on() {
                                if eng.check_health(&cfg) {
                                    let _ = app_handle.emit("watchdog_recovery", "Motor arka planda otomatik onarıldı.");
                                }
                            }

                            let running_procs = get_running_processes();

                            // 2) Oyun Süreçlerine Yüksek İşlemci Önceliği (Sadece oyunları High Priority yapar, uygulamalara dokunmaz)
                            manage_game_process_priorities(&cfg, &running_procs);

                            // 3) Canlı Oyun & Uygulama Algılayıcı
                            if cfg.process_detection_mode {
                                let running_lower: std::collections::HashSet<String> = running_procs.iter().map(|p| p.name.to_lowercase()).collect();
                                let mut detected: Vec<String> = Vec::new();

                                for profile in profiles::APPS {
                                    for exe in profile.exes {
                                        if running_lower.contains(&exe.to_lowercase()) {
                                            detected.push(profile.id.to_string());
                                            break;
                                        }
                                    }
                                }
                                for plat in &cfg.custom_platforms {
                                    if let Some(ref exe) = plat.exe_name {
                                        if running_lower.contains(&exe.to_lowercase()) {
                                            detected.push(plat.id.clone());
                                            continue;
                                        }
                                    }
                                    for exe in &cfg.custom_exes {
                                        if running_lower.contains(&exe.to_lowercase()) {
                                            detected.push(plat.id.clone());
                                            break;
                                        }
                                    }
                                }

                                if detected != last_detected_apps {
                                    last_detected_apps = detected.clone();
                                    let _ = app_handle.emit("detected_processes_changed", detected);
                                }
                            }

                            let is_on = eng.is_on();
                            (cfg.auto_healing && is_on, eng.dir.clone(), cfg.isp.clone(), cfg.strategy.clone())
                        };

                        // 5) Akıllı Kendi Kendini İyileştiren Sistem (Self-Healing Auto-Bypass)
                        auto_probe_cycle += 1;
                        if auto_probe_cycle >= 10 {
                            auto_probe_cycle = 0;
                            if should_probe {
                                let probe_targets = ["https://discord.com", "https://www.roblox.com"];
                                let mut probe_failed = false;
                                for target in probe_targets {
                                    if probe_one(target).await.is_some() {
                                        probe_failed = true;
                                        break;
                                    }
                                }

                                if probe_failed {
                                    let candidates = isp::order(&current_isp);
                                    for cand in candidates {
                                        if current_strat.as_deref() == Some(cand) {
                                            continue;
                                        }
                                        let started = if let Some(state_inner) = app_handle.try_state::<AppState>() {
                                            let mut eng_inner = state_inner.engine.lock().unwrap();
                                            let mut test_cfg = Config::load(&eng_dir);
                                            test_cfg.strategy = Some(cand.to_string());
                                            eng_inner.start(&test_cfg).is_ok()
                                        } else {
                                            false
                                        };

                                        if started {
                                            tokio::time::sleep(Duration::from_millis(600)).await;
                                            if probe_one("https://discord.com").await.is_none() {
                                                let mut test_cfg = Config::load(&eng_dir);
                                                test_cfg.strategy = Some(cand.to_string());
                                                test_cfg.save(&eng_dir);
                                                let s_name = isp::strategy(cand).name;
                                                let _ = app_handle.emit("auto_bypass_recovered", format!("⚡ İnternet sağlayıcısı engeli aşıldı! Otomatik olarak '{}' stratejisine geçildi.", s_name));
                                                let _ = app_handle.emit("strategy_changed", cand.to_string());
                                                break;
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            });

            // Tray setup
            let show_i = MenuItem::with_id(app, "toggle", "Aç / Gizle", true, None::<&str>)?;
            let toggle_bypass_i = MenuItem::with_id(app, "toggle_bypass", "Korumayı Aç / Kapat", true, None::<&str>)?;
            let flush_dns_i = MenuItem::with_id(app, "flush_dns", "DNS Önbelleğini Temizle", true, None::<&str>)?;
            let quit_i = MenuItem::with_id(app, "quit", "Çıkış", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&show_i, &toggle_bypass_i, &flush_dns_i, &quit_i])?;

            let default_icon = app.default_window_icon().cloned();
            let mut tray_builder = TrayIconBuilder::new()
                .tooltip("Slipstream DPI Bypass")
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "toggle" => {
                        if let Some(w) = app.get_webview_window("main") {
                            if let Some(toast_w) = app.get_webview_window("toast") {
                                let _ = toast_w.hide();
                            }
                            TRAY_CLICK_STARTED_OPEN.store(false, Ordering::SeqCst);
                            LAST_BLUR_HIDE_MS.store(0, Ordering::SeqCst);
                            toggle_window(&w);
                        }
                    }
                    "toggle_bypass" => {
                        if let Some(state) = app.try_state::<AppState>() {
                            let mut eng = state.engine.lock().unwrap();
                            let cfg = Config::load(&eng.dir);
                            let now_on = if eng.is_on() {
                                eng.stop();
                                false
                            } else {
                                let _ = eng.start(&cfg);
                                true
                            };
                            let _ = app.emit("engine_state_changed", now_on);
                        }
                    }
                    "flush_dns" => {
                        proxy::flush_dns_cache();
                        let _ = app.emit("dns_flushed", ());
                    }
                    "quit" => {
                        if let Some(w) = app.get_webview_window("main") {
                            let _ = w.hide();
                        }
                        if let Some(t) = app.get_webview_window("toast") {
                            let _ = t.hide();
                        }
                        restore_all_boosted_game_priorities();
                        engine::apply_gaming_network_tweaks(false);
                        if let Some(state) = app.try_state::<AppState>() {
                            if let Ok(mut eng) = state.engine.lock() {
                                eng.ensure_relay(false);
                                eng.stop();
                            }
                        }
                        Engine::kill_all_winws();
                        app.exit(0);
                        std::process::exit(0);
                    }
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state,
                        ..
                    } = event
                    {
                        let app = tray.app_handle();
                        if let Some(w) = app.get_webview_window("main") {
                            match button_state {
                                MouseButtonState::Down => {
                                    let now = now_ms();
                                    let last_blur = LAST_BLUR_HIDE_MS.load(Ordering::SeqCst);
                                    let vis = w.is_visible().unwrap_or(false);
                                    if vis || (now - last_blur < 100) {
                                        TRAY_CLICK_STARTED_OPEN.store(true, Ordering::SeqCst);
                                    } else {
                                        TRAY_CLICK_STARTED_OPEN.store(false, Ordering::SeqCst);
                                    }
                                }
                                MouseButtonState::Up => {
                                    let now = now_ms();
                                    let last_blur = LAST_BLUR_HIDE_MS.load(Ordering::SeqCst);
                                    let is_vis = w.is_visible().unwrap_or(false);
                                    if is_vis {
                                        // Window is open; clicking tray icon toggles it to hidden
                                        LAST_BLUR_HIDE_MS.store(now, Ordering::SeqCst);
                                        let _ = w.hide();
                                    } else {
                                        // Window is hidden; only reopen if the click gesture did not start when open
                                        let started_open = TRAY_CLICK_STARTED_OPEN.swap(false, Ordering::SeqCst);
                                        if started_open || (now - last_blur < 100) {
                                            return;
                                        }
                                        if let Some(toast_w) = app.get_webview_window("toast") {
                                            let _ = toast_w.hide();
                                        }
                                        let _ = position_bottom_right(&w);
                                        let _ = w.show();
                                        let _ = w.set_focus();
                                    }
                                }
                            }
                        }
                    }
                });

            if let Some(icon) = default_icon {
                tray_builder = tray_builder.icon(icon);
            }

            let _tray = tray_builder.build(app)?;

            // Check if application was started with --minimized / --autostart flag or start_minimized setting
            let args: Vec<String> = std::env::args().collect();
            let is_minimized_arg = args.iter().any(|a| a == "--minimized" || a == "--autostart" || a == "-silent");
            let should_start_minimized = is_minimized_arg || boot_cfg.start_minimized;

            // Pre-position toast window while hidden
            if let Some(toast_w) = app.get_webview_window("toast") {
                let _ = position_toast_bottom_right(&toast_w);
                let _ = toast_w.hide();
            }

            // Manage initial window display according to start_minimized configuration
            if let Some(w) = app.get_webview_window("main") {
                // Pre-position main window at bottom-right immediately so it NEVER appears on the left
                let _ = position_bottom_right(&w);

                if should_start_minimized {
                    let _ = w.hide();

                    // Show animated background notification in bottom-right
                    let app_handle = app.handle().clone();
                    tauri::async_runtime::spawn(async move {
                        tokio::time::sleep(Duration::from_millis(900)).await;
                        show_toast_notification(app_handle, None);
                    });
                } else {
                    let _ = w.show();
                    let _ = w.set_focus();
                }
            }

            LAUNCH_TIME_MS.store(now_ms(), Ordering::SeqCst);

            Ok(())
        })
        .on_window_event(|window, event| match event {
            WindowEvent::Focused(focused) => {
                if !focused {
                    let now = now_ms();
                    let launch_time = LAUNCH_TIME_MS.load(Ordering::SeqCst);
                    if launch_time > 0 && (now - launch_time) < 1200 {
                        return;
                    }
                    if FILE_DIALOG_ACTIVE.load(Ordering::SeqCst) {
                        return;
                    }
                    if HIDE_ON_BLUR.load(Ordering::SeqCst) {
                        if window.label() == "main" {
                            if let Ok(true) = window.is_visible() {
                                LAST_BLUR_HIDE_MS.store(now, Ordering::SeqCst);
                                let _ = window.hide();
                            }
                        }
                    }
                }
            }
            WindowEvent::CloseRequested { api, .. } => {
                if MINIMIZE_TO_TRAY.load(Ordering::SeqCst) {
                    api.prevent_close();
                    let _ = window.hide();
                } else {
                    restore_all_boosted_game_priorities();
                    engine::apply_gaming_network_tweaks(false);
                    if let Some(state) = window.try_state::<AppState>() {
                        if let Ok(mut eng) = state.engine.lock() {
                            eng.ensure_relay(false);
                            eng.stop();
                        }
                    }
                    Engine::kill_all_winws();
                    std::process::exit(0);
                }
            }
            WindowEvent::Destroyed => {
                restore_all_boosted_game_priorities();
                engine::apply_gaming_network_tweaks(false);
                if let Some(state) = window.try_state::<AppState>() {
                    if let Ok(mut eng) = state.engine.lock() {
                        eng.ensure_relay(false);
                        eng.stop();
                    }
                }
                Engine::kill_all_winws();
            }
            _ => {}
        })
        .invoke_handler(tauri::generate_handler![
            catalog,
            load_config,
            get_config,
            save_config,
            set_hide_on_blur,
            is_engine_on,
            start,
            stop,
            autotune,
            hide_window,
            set_file_dialog_active,
            close_app,
            reset_dns,
            expand_url,
            restart_discord,
            is_discord_running,
            check_latency,
            set_autostart,
            is_autostart,
            flush_dns_cache,
            test_platform_latency,
            bench_doh_providers,
            diagnose_discord,
            repair_discord,
            check_network_health,
            check_for_updates,
            apply_update,
            open_url,
            open_main_from_toast,
            hide_toast_notification,
            show_toast_notification,
            is_portable,
            get_detected_active_apps,
            suggest_related_domains,
            scan_apps,
            pick_exe_file,
            set_window_expanded
        ])
        .build(tauri::generate_context!())
        .expect("error while building Slipstream")
        .run(|app_handle, event| {
            if let tauri::RunEvent::ExitRequested { .. } | tauri::RunEvent::Exit = event {
                restore_all_boosted_game_priorities();
                engine::apply_gaming_network_tweaks(false);
                if let Some(state) = app_handle.try_state::<AppState>() {
                    if let Ok(mut eng) = state.engine.lock() {
                        eng.ensure_relay(false);
                        eng.stop();
                    }
                }
                Engine::kill_all_winws();
            }
        });
}

#[cfg(test)]
mod tests {
    use super::is_newer_version;

    #[test]
    fn test_is_newer_version_hotfix() {
        assert!(is_newer_version("0.1.6.1", "0.1.6"));
        assert!(is_newer_version("0.1.6+1", "0.1.6"));
        assert!(is_newer_version("v0.1.6.1", "v0.1.6"));
        assert!(is_newer_version("0.1.7", "0.1.6.1"));
        assert!(!is_newer_version("0.1.6", "0.1.6.1"));
        assert!(!is_newer_version("0.1.6.1", "0.1.6.1"));
        assert!(!is_newer_version("0.1.6", "0.1.6"));
    }
}
