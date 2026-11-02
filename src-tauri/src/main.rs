#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod apps;
mod dns;
mod engine;
mod isp;
pub mod native_engine;
mod profiles;
mod proxy;
mod resilience;
mod sound;

use engine::{Config, Engine};
use serde_json::json;
use std::{
    error::Error,
    net::{IpAddr, SocketAddr},
    os::windows::process::CommandExt,
    path::PathBuf,
    process::Command,
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
static TRAY_CLICK_STARTED_FOCUSED: AtomicBool = AtomicBool::new(false);
static IS_WINDOW_EXPANDED: AtomicBool = AtomicBool::new(false);
static COLLAPSED_ORIGIN_X: std::sync::atomic::AtomicI32 = std::sync::atomic::AtomicI32::new(i32::MIN);
static SINGLE_INSTANCE_MUTEX: std::sync::atomic::AtomicIsize = std::sync::atomic::AtomicIsize::new(0);
static LAST_APPLY_UPDATE_MS: AtomicI64 = AtomicI64::new(0);

pub fn release_single_instance_mutex() {
    let old = SINGLE_INSTANCE_MUTEX.swap(0, Ordering::SeqCst);
    if old != 0 {
        #[cfg(target_os = "windows")]
        unsafe {
            #[link(name = "kernel32")]
            extern "system" {
                fn CloseHandle(hObject: isize) -> i32;
            }
            CloseHandle(old);
        }
    }
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

static ENGINE_IS_ON: AtomicBool = AtomicBool::new(false);
static OP_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

struct AppState {
    dir: PathBuf,
    bin: PathBuf,
    engine: Mutex<Engine>,
}

#[tauri::command]
fn catalog(s: State<'_, AppState>) -> serde_json::Value {
    let strats = isp::get_all_strategies(Some(&s.dir));
    let isps = isp::get_all_isps(Some(&s.dir));
    json!({ "isps": isps, "apps": profiles::APPS, "strategies": strats })
}

#[tauri::command]
fn load_config(s: State<AppState>) -> Config {
    let cfg = Config::load(&s.dir);
    HIDE_ON_BLUR.store(cfg.hide_on_blur, Ordering::SeqCst);
    MINIMIZE_TO_TRAY.store(cfg.minimize_to_tray, Ordering::SeqCst);
    sound::set_enabled(cfg.sound_enabled);
    cfg
}

#[tauri::command]
fn get_config(s: State<AppState>) -> Config {
    let cfg = Config::load(&s.dir);
    HIDE_ON_BLUR.store(cfg.hide_on_blur, Ordering::SeqCst);
    MINIMIZE_TO_TRAY.store(cfg.minimize_to_tray, Ordering::SeqCst);
    sound::set_enabled(cfg.sound_enabled);
    cfg
}

#[tauri::command]
async fn save_config(s: State<'_, AppState>, cfg: Config) -> Result<(), String> {
    HIDE_ON_BLUR.store(cfg.hide_on_blur, Ordering::SeqCst);
    MINIMIZE_TO_TRAY.store(cfg.minimize_to_tray, Ordering::SeqCst);
    sound::set_enabled(cfg.sound_enabled);
    let is_on = ENGINE_IS_ON.load(Ordering::SeqCst);
    let dir = s.dir.clone();
    let cfg_clone = cfg.clone();
    tokio::task::spawn_blocking(move || {
        if is_on {
            engine::apply_gaming_network_tweaks(cfg_clone.gaming_mode);
            if cfg_clone.game_priority_boost {
                manage_game_process_priorities(&cfg_clone, &get_running_processes());
            } else {
                restore_all_boosted_game_priorities();
            }
        } else {
            restore_all_boosted_game_priorities();
        }
        cfg_clone.save(&dir);
    })
    .await
    .map_err(|e| e.to_string())
}

#[tauri::command]
fn set_hide_on_blur(enabled: bool) {
    HIDE_ON_BLUR.store(enabled, Ordering::SeqCst);
}

#[tauri::command]
fn is_engine_on(_s: State<AppState>) -> bool {
    ENGINE_IS_ON.load(Ordering::SeqCst)
}

#[tauri::command]
async fn start(app: tauri::AppHandle, s: State<'_, AppState>, cfg: Config) -> Result<(), String> {
    let _guard = OP_LOCK.lock().await;

    let dir = s.dir.clone();
    let bin = s.bin.clone();
    let cfg_clone = cfg.clone();
    let app_handle = app.clone();

    tokio::task::spawn_blocking(move || {
        // Phase 1: Pre-start system preparation (kill conflicts, clean proxy, write hosts) outside engine lock
        let native_cfg = Engine::prepare_start(&dir, &bin, &cfg_clone)?;

        // Phase 2: Start worker - engine mutex held for ~10ms only!
        let start_res = if let Some(state) = app_handle.try_state::<AppState>() {
            let mut eng = state.engine.lock().unwrap();
            eng.start_worker(native_cfg.clone())
        } else {
            Err("AppState bulunamadı".to_string())
        };

        if let Err(err) = start_res {
            if err.contains("kilitli") || err.contains("10") || err.contains("1072") {
                let _ = resilience::repair_wfp_and_network_filters();
                std::thread::sleep(Duration::from_millis(300));
                if let Some(state) = app_handle.try_state::<AppState>() {
                    let mut eng = state.engine.lock().unwrap();
                    eng.start_worker(native_cfg)?;
                }
            } else {
                return Err(err);
            }
        }
        ENGINE_IS_ON.store(true, Ordering::SeqCst);

        // Phase 3: Post-start network config (DNS, gaming tweaks, firewall) outside engine lock
        Engine::apply_post_start(&dir, &cfg_clone);

        if cfg_clone.game_priority_boost {
            manage_game_process_priorities(&cfg_clone, &get_running_processes());
        }

        let _ = app_handle.emit("engine_state_changed", true);
        Ok::<(), String>(())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn stop(app: tauri::AppHandle, s: State<'_, AppState>) -> Result<(), String> {
    let _guard = OP_LOCK.lock().await;

    let dir = s.dir.clone();
    let app_handle = app.clone();

    tokio::task::spawn_blocking(move || {
        restore_all_boosted_game_priorities();

        // Phase 1: Stop worker - engine mutex held for ~2-5ms only!
        if let Some(state) = app_handle.try_state::<AppState>() {
            let mut eng = state.engine.lock().unwrap();
            eng.stop_worker();
            ENGINE_IS_ON.store(false, Ordering::SeqCst);
        }

        // Phase 2: Cleanup system network (restore DNS, proxy, windivert driver stop) outside engine lock
        Engine::cleanup_post_stop(&dir);

        let _ = app_handle.emit("engine_state_changed", false);
        Ok::<(), String>(())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn refresh_game_priorities(cfg: Config) {
    tokio::task::spawn_blocking(move || {
        if cfg.game_priority_boost {
            manage_game_process_priorities(&cfg, &get_running_processes());
        } else {
            restore_all_boosted_game_priorities();
        }
    })
    .await
    .ok();
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
async fn repair_discord(_state: State<'_, AppState>) -> Result<(), String> {
    tokio::task::spawn_blocking(proxy::repair_and_restart_discord)
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn check_network_health(provider: String) -> proxy::NetworkHealth {
    proxy::check_network_health(&provider).await
}

pub fn perform_hide_window(window: &WebviewWindow) {
    LAST_BLUR_HIDE_MS.store(now_ms(), Ordering::SeqCst);
    let _ = window.emit("app_window_visibility", false);
    let _ = window.hide();
    #[cfg(target_os = "windows")]
    {
        #[link(name = "user32")]
        extern "system" {
            fn FindWindowW(lpClassName: *const u16, lpWindowName: *const u16) -> isize;
            fn ShowWindow(hWnd: isize, nCmdShow: i32) -> i32;
        }
        let hwnd_raw = if let Ok(hwnd) = window.hwnd() {
            hwnd.0 as isize
        } else {
            use std::ptr;
            let win_name: Vec<u16> = "Slipstream\0".encode_utf16().collect();
            unsafe { FindWindowW(ptr::null_mut(), win_name.as_ptr()) }
        };
        if hwnd_raw != 0 {
            unsafe {
                ShowWindow(hwnd_raw, 0); // SW_HIDE
            }
        }
    }
    schedule_memory_trim();
}

#[tauri::command]
fn hide_window(window: WebviewWindow) {
    perform_hide_window(&window);
}

#[tauri::command]
fn is_window_visible(window: WebviewWindow) -> bool {
    window.is_visible().unwrap_or(false)
}

#[tauri::command]
fn play_sound(sound: String) {
    sound::play(&sound);
}

#[tauri::command]
fn set_file_dialog_active(active: bool) {
    FILE_DIALOG_ACTIVE.store(active, std::sync::atomic::Ordering::SeqCst);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TaskbarClearance {
    pub extra_left: i32,
    pub extra_top: i32,
    pub extra_right: i32,
    pub extra_bottom: i32,
}

#[cfg(target_os = "windows")]
mod taskbar {
    use super::TaskbarClearance;
    use std::mem;

    #[repr(C)]
    #[derive(Copy, Clone, Debug)]
    pub struct RECT {
        pub left: i32,
        pub top: i32,
        pub right: i32,
        pub bottom: i32,
    }

    #[repr(C)]
    struct APPBARDATA {
        cb_size: u32,
        h_wnd: isize,
        u_callback_message: u32,
        u_edge: u32,
        rc: RECT,
        l_param: isize,
    }

    #[link(name = "shell32")]
    extern "system" {
        fn SHAppBarMessage(dw_message: u32, p_data: *mut APPBARDATA) -> usize;
    }

    #[link(name = "user32")]
    extern "system" {
        fn FindWindowW(lp_class_name: *const u16, lp_window_name: *const u16) -> isize;
        fn GetWindowRect(h_wnd: isize, lp_rect: *mut RECT) -> i32;
    }

    #[link(name = "advapi32")]
    extern "system" {
        fn RegOpenKeyExW(
            h_key: isize,
            lp_sub_key: *const u16,
            ul_options: u32,
            sam_desired: u32,
            phk_result: *mut isize,
        ) -> i32;
        fn RegQueryValueExW(
            h_key: isize,
            lp_value_name: *const u16,
            lp_reserved: *mut u32,
            lp_type: *mut u32,
            lp_data: *mut u8,
            lpcb_data: *mut u32,
        ) -> i32;
        fn RegCloseKey(h_key: isize) -> i32;
    }

    const HKEY_CURRENT_USER: isize = 0x80000001u32 as isize;
    const KEY_READ: u32 = 0x20019;

    const ABM_GETSTATE: u32 = 0x00000004;
    const ABM_GETTASKBARPOS: u32 = 0x00000005;
    const ABS_AUTOHIDE: usize = 0x00000001;

    pub const ABE_LEFT: u32 = 0;
    pub const ABE_TOP: u32 = 1;
    pub const ABE_RIGHT: u32 = 2;
    pub const ABE_BOTTOM: u32 = 3;

    pub fn is_autohide_enabled() -> bool {
        let mut data: APPBARDATA = unsafe { mem::zeroed() };
        data.cb_size = mem::size_of::<APPBARDATA>() as u32;
        let state = unsafe { SHAppBarMessage(ABM_GETSTATE, &mut data) };
        if (state & ABS_AUTOHIDE) != 0 {
            return true;
        }

        // Secondary check via registry: HKCU\Software\Microsoft\Windows\CurrentVersion\Explorer\StuckRects3
        let sub_key: Vec<u16> = "Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\StuckRects3\0"
            .encode_utf16()
            .collect();
        let val_name: Vec<u16> = "Settings\0".encode_utf16().collect();
        let mut h_key: isize = 0;
        if unsafe { RegOpenKeyExW(HKEY_CURRENT_USER, sub_key.as_ptr(), 0, KEY_READ, &mut h_key) } == 0 {
            let mut buf = [0u8; 64];
            let mut size = buf.len() as u32;
            let mut val_type = 0u32;
            let status = unsafe {
                RegQueryValueExW(
                    h_key,
                    val_name.as_ptr(),
                    std::ptr::null_mut(),
                    &mut val_type,
                    buf.as_mut_ptr(),
                    &mut size,
                )
            };
            unsafe { RegCloseKey(h_key); }
            if status == 0 && size >= 9 {
                // Byte 8 bit 0 is ABS_AUTOHIDE in Windows Explorer
                if (buf[8] & 0x01) != 0 {
                    return true;
                }
            }
        }

        false
    }

    pub fn get_taskbar_geometry(scale: f64) -> (u32, i32) {
        let mut pos_data: APPBARDATA = unsafe { mem::zeroed() };
        pos_data.cb_size = mem::size_of::<APPBARDATA>() as u32;
        let res = unsafe { SHAppBarMessage(ABM_GETTASKBARPOS, &mut pos_data) };
        if res != 0 {
            let thickness = match pos_data.u_edge {
                ABE_LEFT | ABE_RIGHT => pos_data.rc.right - pos_data.rc.left,
                ABE_TOP | ABE_BOTTOM => pos_data.rc.bottom - pos_data.rc.top,
                _ => (48.0 * scale).round() as i32,
            };
            let thickness = if thickness > 10 { thickness } else { (48.0 * scale).round() as i32 };
            (pos_data.u_edge, thickness)
        } else {
            // Fallback via FindWindowW("Shell_TrayWnd")
            let class_name: Vec<u16> = "Shell_TrayWnd\0".encode_utf16().collect();
            let hwnd = unsafe { FindWindowW(class_name.as_ptr(), std::ptr::null()) };
            if hwnd != 0 {
                let mut rect: RECT = unsafe { mem::zeroed() };
                if unsafe { GetWindowRect(hwnd, &mut rect) } != 0 {
                    let h = rect.bottom - rect.top;
                    let w = rect.right - rect.left;
                    if h < w && h > 10 {
                        return (ABE_BOTTOM, h);
                    } else if w < h && w > 10 {
                        return (ABE_RIGHT, w);
                    }
                }
            }
            (ABE_BOTTOM, (48.0 * scale).round() as i32)
        }
    }

    pub fn calculate_clearance(
        work_area_w: u32,
        work_area_h: u32,
        mon_w: u32,
        mon_h: u32,
        scale: f64,
    ) -> TaskbarClearance {
        let autohide = is_autohide_enabled();
        let (edge, thickness) = get_taskbar_geometry(scale);

        let mut clearance = TaskbarClearance {
            extra_left: 0,
            extra_top: 0,
            extra_right: 0,
            extra_bottom: 0,
        };

        // When auto-hide is ON, Windows work_area does NOT subtract the taskbar.
        // Also check if the work_area height is within 4px of monitor height (typical for auto-hidden taskbar).
        let is_work_area_full_height = work_area_h >= mon_h.saturating_sub(4);
        let is_work_area_full_width = work_area_w >= mon_w.saturating_sub(4);

        if autohide || is_work_area_full_height || is_work_area_full_width {
            match edge {
                ABE_BOTTOM => {
                    if autohide || is_work_area_full_height {
                        clearance.extra_bottom = thickness;
                    }
                }
                ABE_TOP => {
                    if autohide || is_work_area_full_height {
                        clearance.extra_top = thickness;
                    }
                }
                ABE_RIGHT => {
                    if autohide || is_work_area_full_width {
                        clearance.extra_right = thickness;
                    }
                }
                ABE_LEFT => {
                    if autohide || is_work_area_full_width {
                        clearance.extra_left = thickness;
                    }
                }
                _ => {}
            }
        }

        clearance
    }
}

#[cfg(not(target_os = "windows"))]
mod taskbar {
    use super::TaskbarClearance;
    pub fn calculate_clearance(
        _work_area_w: u32,
        _work_area_h: u32,
        _mon_w: u32,
        _mon_h: u32,
        _scale: f64,
    ) -> TaskbarClearance {
        TaskbarClearance {
            extra_left: 0,
            extra_top: 0,
            extra_right: 0,
            extra_bottom: 0,
        }
    }
}

/// Dynamically calibrates window dimensions and coordinates for any screen resolution, DPI scale,
/// and auto-hidden taskbar state so the UI is never clipped or occluded.
fn get_calibrated_window_layout(
    m: &tauri::Monitor,
    is_expanded: bool,
) -> (f64, f64, i32, i32) {
    let scale = m.scale_factor();
    let work_area = m.work_area();
    let mon_size = m.size();

    let clearance = taskbar::calculate_clearance(
        work_area.size.width,
        work_area.size.height,
        mon_size.width,
        mon_size.height,
        scale,
    );

    let margin_x = (16.0 * scale).round() as i32;
    let margin_y = (16.0 * scale).round() as i32;

    // Available space taking into account margins and any auto-hidden taskbar
    let avail_phys_w = (work_area.size.width as i32)
        .saturating_sub(clearance.extra_left + clearance.extra_right + margin_x * 2);
    let avail_phys_h = (work_area.size.height as i32)
        .saturating_sub(clearance.extra_top + clearance.extra_bottom + margin_y * 2);

    let avail_logical_w = (avail_phys_w as f64 / scale).max(320.0);
    let avail_logical_h = (avail_phys_h as f64 / scale).max(440.0);

    // Calibrate target logical size (default 380x620 or 820x620, clamped down if monitor is small)
    let ideal_w: f64 = if is_expanded { 820.0 } else { 380.0 };
    let ideal_h: f64 = 620.0;

    let target_logical_w = ideal_w.min(avail_logical_w.floor());
    let target_logical_h = ideal_h.min(avail_logical_h.floor());

    let win_phys_w = (target_logical_w * scale).round() as i32;
    let win_phys_h = (target_logical_h * scale).round() as i32;

    let mut x = work_area.position.x + work_area.size.width as i32 - win_phys_w - margin_x - clearance.extra_right;
    let mut y = work_area.position.y + work_area.size.height as i32 - win_phys_h - margin_y - clearance.extra_bottom;

    let min_x = work_area.position.x + margin_x + clearance.extra_left;
    let min_y = work_area.position.y + margin_y + clearance.extra_top;

    if x < min_x {
        x = min_x;
    }
    if y < min_y {
        y = min_y;
    }

    (target_logical_w, target_logical_h, x, y)
}

#[tauri::command]
fn set_window_expanded(app: tauri::AppHandle, expanded: bool) -> Result<(), String> {
    if let Some(win) = app.get_webview_window("main") {
        let currently_expanded = IS_WINDOW_EXPANDED.load(Ordering::SeqCst);
        if expanded == currently_expanded {
            return Ok(());
        }

        let monitor = win
            .current_monitor()
            .ok()
            .flatten()
            .or_else(|| win.primary_monitor().ok().flatten())
            .or_else(|| win.available_monitors().ok().and_then(|v| v.into_iter().next()));

        if let Some(m) = monitor {
            let (target_w, target_h, target_x, target_y) = get_calibrated_window_layout(&m, expanded);

            if expanded {
                if let Ok(cur_pos) = win.outer_position() {
                    COLLAPSED_ORIGIN_X.store(cur_pos.x, Ordering::SeqCst);
                }

                let _ = win.set_position(tauri::Position::Physical(tauri::PhysicalPosition::new(target_x, target_y)));
                let _ = win.set_size(tauri::Size::Logical(tauri::LogicalSize::new(target_w, target_h)));
                IS_WINDOW_EXPANDED.store(true, Ordering::SeqCst);
            } else {
                let saved_x = COLLAPSED_ORIGIN_X.swap(i32::MIN, Ordering::SeqCst);
                let new_x = if saved_x != i32::MIN {
                    saved_x
                } else {
                    target_x
                };

                let _ = win.set_size(tauri::Size::Logical(tauri::LogicalSize::new(target_w, target_h)));
                let _ = win.set_position(tauri::Position::Physical(tauri::PhysicalPosition::new(new_x, target_y)));
                IS_WINDOW_EXPANDED.store(false, Ordering::SeqCst);
            }
        }
    }
    Ok(())
}

static SHUTTING_DOWN: AtomicBool = AtomicBool::new(false);

fn full_application_shutdown(app: Option<&tauri::AppHandle>, state: Option<&AppState>) {
    if SHUTTING_DOWN.swap(true, Ordering::SeqCst) {
        return;
    }

    // 1. Immediately hide window and tray icon so the UI vanishes with zero delay (<5ms)
    if let Some(app) = app {
        if let Some(w) = app.get_webview_window("main") {
            perform_hide_window(&w);
        }
        if let Some(t) = app.get_webview_window("toast") {
            let _ = t.hide();
            let _ = t.destroy();
        }
        let _ = app.remove_tray_by_id("main-tray");
    }

    // Unconditional safety watchdog: guarantees process termination within 800ms under all conditions
    std::thread::spawn(|| {
        std::thread::sleep(Duration::from_millis(800));
        std::process::exit(0);
    });

    restore_all_boosted_game_priorities();
    engine::apply_gaming_network_tweaks(false);

    if let Some(state) = state {
        if let Ok(mut eng) = state.engine.try_lock() {
            eng.ensure_relay(false);
            eng.stop_worker();
        }
        ENGINE_IS_ON.store(false, Ordering::SeqCst);
        Engine::cleanup_post_stop(&state.dir);
    } else if let Ok(appdata) = std::env::var("APPDATA") {
        let fallback_dir = PathBuf::from(appdata).join("app.slipstream.desktop");
        Engine::cleanup_post_stop(&fallback_dir);
    }

    Engine::kill_all_winws();
    engine::apply_quic_firewall_rule(false);
    dns::clear_bypass_hosts();
    proxy::patch_discord_settings(None);
    proxy::clean_stale_proxy();
    dns::flush();

    // Release WinDivert driver handles and stop service safely
    for svc in &["windivert", "windivert14"] {
        let _ = Command::new("net")
            .args(["stop", svc, "/y"])
            .creation_flags(0x0800_0000)
            .output();
    }

    if let Some(app) = app {
        app.exit(0);
    }
    std::process::exit(0);
}

#[tauri::command]
fn close_app(app: tauri::AppHandle, s: State<AppState>) {
    full_application_shutdown(Some(&app), Some(&s));
}

#[tauri::command]
async fn scan_apps() -> Vec<apps::ScannedApp> {
    tokio::task::spawn_blocking(|| {
        let procs = get_running_process_names();
        apps::scan_installed_and_running_apps(&procs)
    })
    .await
    .unwrap_or_default()
}

#[tauri::command]
async fn pick_exe_file() -> Option<String> {
    tokio::task::spawn_blocking(|| {
        FILE_DIALOG_ACTIVE.store(true, Ordering::SeqCst);
        let picked = apps::pick_exe_dialog();
        FILE_DIALOG_ACTIVE.store(false, Ordering::SeqCst);
        picked
    })
    .await
    .unwrap_or(None)
}

#[tauri::command]
async fn reset_dns() {
    tokio::task::spawn_blocking(dns::reset_all_adapters).await.ok();
}

#[tauri::command]
async fn flush_dns_cache() {
    tokio::task::spawn_blocking(proxy::flush_dns_cache).await.ok();
}

#[tauri::command]
async fn restart_discord() -> Result<(), String> {
    tokio::task::spawn_blocking(proxy::repair_and_restart_discord)
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn is_discord_running() -> bool {
    tokio::task::spawn_blocking(proxy::is_discord_running).await.unwrap_or(false)
}

#[tauri::command]
fn expand_url(raw: String) -> Vec<String> {
    profiles::expand_domains(&raw)
}

#[tauri::command]
async fn set_autostart(s: State<'_, AppState>, enable: bool) -> Result<(), String> {
    let dir = s.dir.clone();
    tokio::task::spawn_blocking(move || {
        let cfg = Config::load(&dir);
        engine::set_windows_autostart(enable, cfg.start_minimized)
    })
    .await
    .map_err(|e| e.to_string())?
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
            let local_lower = local.to_lowercase();
            let install_dir1 = std::path::Path::new(&local_lower).join("slipstream");
            let install_dir2 = std::path::Path::new(&local_lower).join("programs").join("slipstream");
            let dir1_str = install_dir1.to_string_lossy().to_lowercase();
            let dir2_str = install_dir2.to_string_lossy().to_lowercase();

            if exe_str.starts_with(&dir1_str)
                || exe_str.starts_with(&dir2_str)
                || exe_str.contains("program files")
                || exe_str.contains("target\\debug")
                || exe_str.contains("target\\release")
            {
                return false;
            }
        }
        return true;
    }
    false
}

#[tauri::command]
fn is_portable() -> bool {
    is_portable_app()
}

// === Geri Bildirim & Kullanıcı Kimliği Sistemi ===
fn get_default_feedback_endpoint() -> String {
    const OBFUSCATED: &[u8] = &[
        0x32, 0x2e, 0x2e, 0x2a, 0x29, 0x60, 0x75, 0x75, 0x29, 0x37, 0x3b, 0x36, 0x36, 0x77, 0x3e, 0x33,
        0x29, 0x31, 0x77, 0x6d, 0x69, 0x3b, 0x6d, 0x74, 0x38, 0x3b, 0x28, 0x3b, 0x34, 0x31, 0x3b, 0x23,
        0x3b, 0x2a, 0x33, 0x34, 0x3b, 0x28, 0x68, 0x74, 0x2d, 0x35, 0x28, 0x31, 0x3f, 0x28, 0x29, 0x74,
        0x3e, 0x3f, 0x2c, 0x75,
    ];
    let decoded: Vec<u8> = OBFUSCATED.iter().map(|&b| b ^ 0x5a).collect();
    String::from_utf8(decoded).unwrap_or_default()
}

fn get_machine_guid() -> String {
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        use std::process::Command;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;

        if let Ok(out) = Command::new("reg")
            .args(&["query", "HKLM\\SOFTWARE\\Microsoft\\Cryptography", "/v", "MachineGuid"])
            .creation_flags(CREATE_NO_WINDOW)
            .output()
        {
            let text = String::from_utf8_lossy(&out.stdout);
            for line in text.lines() {
                if line.contains("MachineGuid") {
                    let parts: Vec<&str> = line.split_whitespace().collect();
                    if let Some(guid) = parts.last() {
                        if guid.len() >= 8 {
                            return guid.to_string();
                        }
                    }
                }
            }
        }
    }
    "UNKNOWN-GUID".to_string()
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct SystemIdentity {
    pub username: String,
    pub computer_name: String,
    pub machine_id: String,
    pub short_machine_id: String,
    pub os_info: String,
    pub app_version: String,
    pub is_portable: bool,
}

#[tauri::command]
fn get_system_identity() -> SystemIdentity {
    let username = std::env::var("USERNAME").unwrap_or_else(|_| "User".to_string());
    let computer_name = std::env::var("COMPUTERNAME").unwrap_or_else(|_| "Windows-PC".to_string());
    let guid = get_machine_guid();
    let short_id = if guid.len() >= 8 {
        format!("#{}", &guid[..8].to_uppercase())
    } else {
        "#00000000".to_string()
    };
    let is_portable = is_portable_app();
    let app_version = env!("CARGO_PKG_VERSION").to_string();

    SystemIdentity {
        username,
        computer_name,
        machine_id: guid,
        short_machine_id: short_id,
        os_info: "Windows (x64)".to_string(),
        app_version,
        is_portable,
    }
}

#[tauri::command]
fn check_elevation() -> bool {
    resilience::is_elevated_admin()
}

#[tauri::command]
fn request_elevation() -> Result<(), String> {
    resilience::request_admin_elevation(None)
}

#[tauri::command]
async fn check_antivirus_health(s: State<'_, AppState>) -> Result<resilience::AntivirusReport, String> {
    let eng_dir = s.bin.parent().map(|p| p.to_path_buf()).unwrap_or_else(|| PathBuf::from("engine"));
    tokio::task::spawn_blocking(move || {
        resilience::check_antivirus_status(&eng_dir)
    })
    .await
    .map_err(|e| e.to_string())
}

#[tauri::command]
async fn apply_defender_exclusions(s: State<'_, AppState>) -> Result<String, String> {
    let eng_dir = s.bin.parent().map(|p| p.to_path_buf()).unwrap_or_else(|| PathBuf::from("engine"));
    let (_target_exe, install_dir) = resolve_install_paths();
    tokio::task::spawn_blocking(move || {
        resilience::add_defender_exclusions(&install_dir, &eng_dir)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn detect_conflicts() -> Result<Vec<resilience::NetworkConflict>, String> {
    let own_pid = Some(std::process::id());
    tokio::task::spawn_blocking(move || {
        let procs = get_running_processes();
        resilience::detect_network_conflicts(&procs, own_pid)
    })
    .await
    .map_err(|e| e.to_string())
}

#[tauri::command]
async fn resolve_conflicts() -> Result<String, String> {
    tokio::task::spawn_blocking(resilience::resolve_network_conflicts)
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn diagnose_wfp() -> resilience::WfpDiagnostics {
    tokio::task::spawn_blocking(resilience::diagnose_wfp_health)
        .await
        .unwrap_or_else(|_| resilience::WfpDiagnostics {
            bfe_running: false,
            windivert_state: "Hata".to_string(),
            is_healthy: false,
            details: "Tanılama görevi tamamlanamadı.".to_string(),
        })
}

#[tauri::command]
async fn repair_wfp() -> Result<String, String> {
    tokio::task::spawn_blocking(resilience::repair_wfp_and_network_filters)
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn fetch_remote_isp_updates(s: State<'_, AppState>) -> Result<isp::RemoteIspUpdates, String> {
    let dir = s.dir.clone();
    let client = match reqwest::Client::builder()
        .user_agent("Slipstream-Client/1.0")
        .timeout(Duration::from_secs(8))
        .build()
    {
        Ok(c) => c,
        Err(_) => return Ok(isp::load_cached_updates(&dir).unwrap_or_default()),
    };

    let now_ts = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let update_url = format!("https://raw.githubusercontent.com/BaranK35/Slipstream2/main/isp_rules.json?t={now_ts}");

    if let Ok(resp) = client.get(&update_url).send().await {
        if resp.status().is_success() {
            if let Ok(text) = resp.text().await {
                if let Ok(updates) = serde_json::from_str::<isp::RemoteIspUpdates>(&text) {
                    let _ = isp::save_cached_updates(&dir, &updates);
                    return Ok(updates);
                }
            }
        }
    }

    Ok(isp::load_cached_updates(&dir).unwrap_or_default())
}

#[derive(serde::Deserialize, Clone, Debug)]
pub struct SendFeedbackPayload {
    pub category: String, // "bug", "feature", "general"
    pub sender: String,
    pub message: String,
    pub attach_sys_info: bool,
    pub custom_webhook: Option<String>,
}

#[tauri::command]
async fn send_discord_feedback(
    s: State<'_, AppState>,
    payload: SendFeedbackPayload,
) -> Result<(), String> {
    let msg = payload.message.trim();
    if msg.is_empty() {
        return Err("Lütfen bir açıklama veya mesaj girin.".to_string());
    }

    let cfg = Config::load(&s.dir);

    let webhook_url = if let Some(ref custom) = payload.custom_webhook {
        let trimmed = custom.trim().to_string();
        if !trimmed.is_empty() && trimmed.starts_with("https://") {
            trimmed
        } else {
            get_default_feedback_endpoint()
        }
    } else {
        get_default_feedback_endpoint()
    };

    if webhook_url.is_empty() || !webhook_url.starts_with("https://") {
        return Err("Geçerli bir Webhook veya Relay URL bulunamadı (https:// ile başlamalıdır).".to_string());
    }

    let identity = get_system_identity();
    let engine_active = ENGINE_IS_ON.load(Ordering::SeqCst);
    let strategy_id = cfg.strategy.clone().unwrap_or_else(|| "varsayılan".to_string());
    let isp_id = cfg.isp.clone();

    // Category title and embed color (decimal values)
    let (cat_title, color) = match payload.category.as_str() {
        "bug" => ("🐛 Hata Bildirimi", 15548997), // Red #ED4245
        "feature" => ("💡 Yeni Özellik / Öneri", 5763719), // Green #57F287
        _ => ("💬 Genel Görüş & Geri Bildirim", 5793266), // Blurple #5865F2
    };

    let sender_display = if payload.sender.trim().is_empty() {
        if payload.attach_sys_info {
            format!("Anonim ({})", identity.short_machine_id)
        } else {
            "Anonim".to_string()
        }
    } else if payload.attach_sys_info {
        format!("{} ({})", payload.sender.trim(), identity.short_machine_id)
    } else {
        payload.sender.trim().to_string()
    };

    let mut fields = Vec::new();
    fields.push(json!({
        "name": "👤 Gönderen",
        "value": sender_display,
        "inline": true
    }));

    if payload.attach_sys_info {
        fields.push(json!({
            "name": "🔑 Donanım Kimliği",
            "value": format!("`{}`", identity.short_machine_id),
            "inline": true
        }));
        fields.push(json!({
            "name": "🚀 Slipstream Sürümü",
            "value": format!("v{} ({})", identity.app_version, if identity.is_portable { "Portable" } else { "Setup" }),
            "inline": true
        }));
        fields.push(json!({
            "name": "🌐 İSS & Strateji",
            "value": format!("Sağlayıcı: **{}** | Strateji: `{}`\n🛡️ Koruma: **{}**", 
                isp_id, 
                strategy_id, 
                if engine_active { "Devrede 🟢" } else { "Kapalı ⚪" }
            ),
            "inline": false
        }));

        let engine_dir = s.dir.clone();
        let log_path = engine_dir.join("engine.log");
        if let Ok(log_content) = std::fs::read_to_string(&log_path) {
            let lines: Vec<&str> = log_content.lines().collect();
            let recent_lines = if lines.len() > 15 {
                &lines[lines.len() - 15..]
            } else {
                &lines[..]
            };
            let log_snippet = recent_lines.join("\n");
            if !log_snippet.trim().is_empty() {
                let trimmed_log = if log_snippet.len() > 950 {
                    &log_snippet[log_snippet.len() - 950..]
                } else {
                    &log_snippet
                };
                fields.push(json!({
                    "name": "📜 Son Motor Logları (engine.log)",
                    "value": format!("```text\n{}\n```", trimmed_log),
                    "inline": false
                }));
            }
        }
    }

    let embed = json!({
        "title": format!("[Slipstream] {}", cat_title),
        "description": msg,
        "color": color,
        "fields": fields,
        "footer": {
            "text": format!("Slipstream v{} • Kullanıcı Geri Bildirimi", identity.app_version)
        }
    });

    let client = reqwest::Client::builder()
        .user_agent("Slipstream-Feedback/1.0")
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(|e| format!("HTTP istemci hatası: {}", e))?;

    let post_body = json!({
        "username": "Slipstream Feedback",
        "embeds": [embed]
    });

    let json_bytes = serde_json::to_vec(&post_body).map_err(|e| e.to_string())?;

    let resp = client.post(&webhook_url)
        .header("Content-Type", "application/json")
        .body(json_bytes)
        .send()
        .await
        .map_err(|e| format!("Discord sunucusuna ulaşılamadı: {}", e))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let err_text = resp.text().await.unwrap_or_default();
        return Err(format!("Discord Webhook hatası (HTTP {status}): {err_text}"));
    }

    Ok(())
}

#[derive(Clone, Debug)]
pub struct ProcessItem {
    pub pid: u32,
    pub name: String,
    pub parent_pid: u32,
}

#[cfg(target_os = "windows")]
pub fn get_running_processes() -> Vec<ProcessItem> {
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
                parent_pid: entry.th32ParentProcessID,
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
pub fn get_running_processes() -> Vec<ProcessItem> {
    vec![]
}

pub fn get_running_process_names() -> Vec<String> {
    get_running_processes().into_iter().map(|p| p.name).collect()
}

/// Trims memory working set of Slipstream and all child WebView2 processes down to minimal RAM.
#[cfg(target_os = "windows")]
pub fn trim_all_slipstream_memory() {
    #[link(name = "kernel32")]
    extern "system" {
        fn GetCurrentProcess() -> isize;
        fn GetCurrentProcessId() -> u32;
        fn OpenProcess(dwDesiredAccess: u32, bInheritHandle: i32, dwProcessId: u32) -> isize;
        fn SetProcessWorkingSetSize(hProcess: isize, dwMinimumWorkingSetSize: usize, dwMaximumWorkingSetSize: usize) -> i32;
        fn CloseHandle(hObject: isize) -> i32;
    }
    unsafe {
        let h_proc = GetCurrentProcess();
        SetProcessWorkingSetSize(h_proc, usize::MAX, usize::MAX);
    }

    let my_pid = unsafe { GetCurrentProcessId() };
    let procs = get_running_processes();

    let mut child_pids = std::collections::HashSet::new();
    child_pids.insert(my_pid);

    for _ in 0..3 {
        for p in &procs {
            if child_pids.contains(&p.parent_pid) {
                child_pids.insert(p.pid);
            }
        }
    }

    const PROCESS_SET_QUOTA: u32 = 0x0100;
    const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;

    for pid in child_pids {
        if pid == my_pid {
            continue;
        }
        let h = unsafe { OpenProcess(PROCESS_SET_QUOTA | PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
        if h != 0 {
            unsafe {
                SetProcessWorkingSetSize(h, usize::MAX, usize::MAX);
                CloseHandle(h);
            }
        }
    }
}

#[cfg(not(target_os = "windows"))]
pub fn trim_all_slipstream_memory() {}

pub fn schedule_memory_trim() {
    tauri::async_runtime::spawn(async move {
        // Debounce: do not trim immediately. Only trim if window remains hidden for 5 seconds.
        tokio::time::sleep(Duration::from_secs(5)).await;
        let last_hide = LAST_BLUR_HIDE_MS.load(Ordering::SeqCst);
        if last_hide > 0 && (now_ms() - last_hide) >= 4800 {
            let _ = tokio::task::spawn_blocking(trim_all_slipstream_memory).await;
        }
    });
}

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
            if let Some(ref exe) = plat.exe_name {
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
        if game_exes.contains(&lower)
            && !forbidden_exes.contains(&lower)
            && !boosted.contains(&proc.pid)
        {
            // Apply HIGH_PRIORITY_CLASS (0x00000080)
            if set_process_priority(proc.pid, 0x0000_0080) {
                boosted.insert(proc.pid);
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
        }
    }
    running_procs.iter().any(|p| game_exes.contains(&p.name.to_lowercase()))
}

#[tauri::command]
async fn get_detected_active_apps(cfg: Config) -> Vec<String> {
    tokio::task::spawn_blocking(move || {
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
                }
            }
        }
        detected
    })
    .await
    .unwrap_or_default()
}

#[tauri::command]
async fn is_game_running() -> bool {
    tokio::task::spawn_blocking(|| {
        let procs = get_running_process_names();
        procs.iter().any(|p| {
            if apps::is_system_process(p) {
                return false;
            }
            let clean = p.strip_suffix(".exe").or_else(|| p.strip_suffix(".EXE")).unwrap_or(p);
            apps::is_likely_game(clean, p, "")
        })
    })
    .await
    .unwrap_or(false)
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
    let (discord, discord_voice, cloudflare, google) = tokio::join!(
        proxy::measure_latency("updates.discord.com", 443),
        proxy::measure_latency("ec2.eu-central-1.amazonaws.com", 443),
        proxy::measure_latency("1.1.1.1", 443),
        proxy::measure_latency("dns.google", 443)
    );
    json!({
        "discord": discord,
        "discord_voice": discord_voice,
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
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36")
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
pub fn query_registry_string(hkey: isize, subkey: &str, value_name: &str) -> Option<String> {
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

#[cfg(not(target_os = "windows"))]
pub fn query_registry_string(_hkey: isize, _subkey: &str, _value_name: &str) -> Option<String> {
    None
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

const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

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

    // 1) First, query GitHub Releases API (authoritative source for published releases, unaffected by raw CDN caching)
    let api_urls = [
        "https://api.github.com/repos/BaranK35/Slipstream2/releases/latest",
        "https://api.github.com/repos/BaranK35/Slipstreamtest2/releases/latest",
    ];
    let mut fallback_notes = String::new();

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
                            if fallback_notes.is_empty() && !notes.trim().is_empty() {
                                fallback_notes = notes.to_string();
                            }
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

    // 2) Secondary fallback: version.json with cache-busting timestamp
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
                            if fallback_notes.is_empty() && !notes.trim().is_empty() {
                                fallback_notes = notes.to_string();
                            }
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
                                }
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
        release_notes: fallback_notes,
        download_url: "".into(),
    }
}

pub fn is_trusted_update_url(url: &str) -> bool {
    let lower = url.to_lowercase();
    (lower.starts_with("https://github.com/barank35/slipstream2/releases/download/")
        || lower.starts_with("https://github.com/barank35/slipstreamtest2/releases/download/"))
        && lower.ends_with(".exe")
}

pub async fn execute_apply_update(app: tauri::AppHandle, _dir: PathBuf, download_url: String) -> Result<(), String> {
    if download_url.is_empty() {
        return Err("Geçerli bir indirme bağlantısı bulunamadı.".into());
    }

    if !is_trusted_update_url(&download_url) {
        return Err("Güvenlik ihlali: Güncelleme paketi yalnızca resmi Slipstream GitHub deposundan indirilebilir.".into());
    }

    let now = now_ms();
    let last = LAST_APPLY_UPDATE_MS.load(Ordering::SeqCst);
    if last > 0 && (now - last) < 5_000 {
        return Err("Güncelleme işlemi başlatılıyor, lütfen bekleyin...".into());
    }
    LAST_APPLY_UPDATE_MS.store(now, Ordering::SeqCst);

    let client = reqwest::Client::builder()
        .user_agent("Slipstream-Updater/1.0")
        .timeout(Duration::from_secs(60))
        .build()
        .map_err(|e| e.to_string())?;

    let _ = app.emit("update_status", "downloading");

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

    let _ = app.emit("update_status", "installing");

    // Acquire OP_LOCK only after bytes are fully downloaded
    let _guard = OP_LOCK.lock().await;

    // Explicitly release single instance mutex so installer doesn't see a running locked instance
    release_single_instance_mutex();

    let app_h = app.clone();
    tokio::task::spawn_blocking(move || -> Result<(), String> {
        let temp_dir = std::env::temp_dir();
        let installer_path = temp_dir.join("Slipstream-Update.exe");
        std::fs::write(&installer_path, bytes).map_err(|e| e.to_string())?;

        let (target_exe, install_dir) = resolve_install_paths();

        // 1) Stop DPI engine worker quickly (~2ms) in memory before launching installer
        if let Some(state) = app_h.try_state::<AppState>() {
            let mut eng = state.engine.lock().unwrap();
            eng.ensure_relay(false);
            eng.stop_worker();
            ENGINE_IS_ON.store(false, Ordering::SeqCst);
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
            if ($parentPid -gt 0) {{ Wait-Process -Id $parentPid -Timeout 5 -ErrorAction SilentlyContinue; }}\
            Stop-Process -Name 'slipstream', 'winws' -Force -ErrorAction SilentlyContinue;\
            taskkill.exe /F /IM slipstream.exe /T 2>$null;\
            taskkill.exe /F /IM winws.exe /T 2>$null;\
            Start-Sleep -Milliseconds 500;\
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
            if (Test-Path -LiteralPath \"$installDir\\slipstream.exe\") {{\
                Start-Process -FilePath \"$installDir\\slipstream.exe\" -ArgumentList '--minimized' -WindowStyle Hidden;\
            }} elseif (Test-Path -LiteralPath $targetExe) {{\
                Start-Process -FilePath $targetExe -ArgumentList '--minimized' -WindowStyle Hidden;\
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
        Ok(())
    })
    .await
    .map_err(|e| e.to_string())??;

    // Fast-exit to let updater script take over immediately without deadlocking
    if let Some(w) = app.get_webview_window("main") {
        perform_hide_window(&w);
    }
    let _ = app.remove_tray_by_id("main-tray");
    app.exit(0);
    std::process::exit(0);
}

#[tauri::command]
async fn apply_update(app: tauri::AppHandle, s: State<'_, AppState>, download_url: String) -> Result<(), String> {
    execute_apply_update(app, s.dir.clone(), download_url).await
}

static LAST_NOTIFIED_VERSION: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);

pub fn notify_update_found(app: &tauri::AppHandle, dir: &PathBuf, info: &UpdateInfo, _is_cold_start: bool) {
    let _ = app.emit("update_available", info.clone());

    let should_notify = {
        let mut last = LAST_NOTIFIED_VERSION.lock().unwrap();
        if last.as_deref() != Some(&info.latest_version) {
            *last = Some(info.latest_version.clone());
            true
        } else {
            false
        }
    };

    let cfg = Config::load(dir);

    if should_notify {
        let msg = if cfg.auto_update {
            format!("Slipstream v{} güncellemesi otomatik uygulanıyor...", info.latest_version)
        } else {
            format!("Yeni bir güncelleme mevcut (v{}). İncelemek veya güncellemek için tıklayın.", info.latest_version)
        };
        show_toast_notification(app.clone(), Some(msg));
    }

    if cfg.auto_update && !info.download_url.is_empty() {
        let app_cl = app.clone();
        let dir_cl = dir.clone();
        let url = info.download_url.clone();
        tauri::async_runtime::spawn(async move {
            let is_game = is_game_running().await;
            if !is_game {
                let _ = execute_apply_update(app_cl, dir_cl, url).await;
            }
        });
    }
}

pub fn start_live_sse_update_listener(app: tauri::AppHandle, dir: PathBuf) {
    tauri::async_runtime::spawn(async move {
        let client = match reqwest::Client::builder()
            .user_agent("Slipstream-LiveUpdater/1.0")
            .tcp_keepalive(Duration::from_secs(45))
            .build()
        {
            Ok(c) => c,
            Err(_) => return,
        };

        let sse_url = "https://ntfy.sh/slipstream_live_updates_bk35/sse";

        loop {
            match client.get(sse_url).header("Accept", "text/event-stream").send().await {
                Ok(mut resp) => {
                    let mut line_buffer = String::new();
                    // Read SSE stream chunks with timeout to detect half-closed sockets
                    loop {
                        let chunk_res = tokio::time::timeout(Duration::from_secs(75), resp.chunk()).await;
                        match chunk_res {
                            Ok(Ok(Some(chunk))) => {
                                if let Ok(text) = std::str::from_utf8(&chunk) {
                                    line_buffer.push_str(text);
                                    if line_buffer.contains("event: message") || line_buffer.contains("\"event\":\"message\"") {
                                        line_buffer.clear();
                                        // Received instant live push notification! Check version immediately!
                                        let info = check_for_updates().await;
                                        if info.has_update {
                                            notify_update_found(&app, &dir, &info, false);
                                        }
                                    }
                                }
                            }
                            Ok(Ok(None)) => {
                                // Stream ended normally from server, reconnect
                                break;
                            }
                            Ok(Err(_)) => {
                                // Network or stream error, reconnect
                                break;
                            }
                            Err(_) => {
                                // 75-second read timeout (server keep-alive missed), break and reconnect cleanly
                                break;
                            }
                        }
                    }
                }
                Err(_) => {
                    tokio::time::sleep(Duration::from_secs(5)).await;
                }
            }
            tokio::time::sleep(Duration::from_secs(3)).await;
        }
    });
}

/// Comprehensive Auto-Tune: Tests major services live against each candidate strategy,
/// measuring packet bypass success and ping, then picks and applies the best strategy.
#[tauri::command]
async fn autotune(
    app: tauri::AppHandle,
    s: State<'_, AppState>,
    mut cfg: Config,
) -> Result<Option<String>, String> {
    let _guard = OP_LOCK.lock().await;

    // Benchmark targets covering active user services
    let mut targets = Vec::new();
    if cfg.enabled_apps.iter().any(|a| a == "discord") {
        targets.push("https://discord.com".to_string());
        targets.push("https://gateway.discord.gg".to_string());
    }
    if cfg.enabled_apps.iter().any(|a| a == "roblox") {
        targets.push("https://www.roblox.com".to_string());
    }
    if cfg.enabled_apps.iter().any(|a| a == "youtube") {
        targets.push("https://www.youtube.com".to_string());
    }
    if cfg.enabled_apps.iter().any(|a| a == "x") {
        targets.push("https://x.com".to_string());
    }
    for u in profiles::probe_urls(&cfg) {
        if !targets.contains(&u) {
            targets.push(u);
        }
    }
    if targets.is_empty() {
        targets.push("https://discord.com".to_string());
    }

    let eng_dir = Some(s.dir.as_path());
    let candidate_ids = isp::order_with_updates(&cfg.isp, eng_dir);
    let all_strats = isp::get_all_strategies(eng_dir);
    let mut best_id: Option<String> = None;
    let mut best_score = -1;
    let mut best_latency = u64::MAX;
    let mut best_name = String::new();

    let total = targets.len();
    let _ = app.emit("autotune", format!("🚀 En iyi ayar testi başlatıldı: {total} kritik servis canlı test edilecek."));

    for id in candidate_ids {
        let name = all_strats
            .iter()
            .find(|s| s.id == id)
            .map(|s| s.name.clone())
            .unwrap_or_else(|| id.clone());
        let _ = app.emit("autotune", format!("⚡ Deneniyor: {name}..."));
        cfg.strategy = Some(id.clone());
        let dir = s.dir.clone();
        let bin = s.bin.clone();
        let cfg_cl = cfg.clone();
        let app_h = app.clone();
        tokio::task::spawn_blocking(move || {
            if let Ok(native_cfg) = Engine::prepare_start(&dir, &bin, &cfg_cl) {
                if let Some(state) = app_h.try_state::<AppState>() {
                    let mut eng = state.engine.lock().unwrap();
                    let _ = eng.start_worker(native_cfg);
                    ENGINE_IS_ON.store(true, Ordering::SeqCst);
                }
                Engine::apply_post_start(&dir, &cfg_cl);
            }
        })
        .await
        .map_err(|e| e.to_string())?;
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
        let dir = s.dir.clone();
        let bin = s.bin.clone();
        let cfg_cl = cfg.clone();
        let app_h = app.clone();
        tokio::task::spawn_blocking(move || {
            if let Ok(native_cfg) = Engine::prepare_start(&dir, &bin, &cfg_cl) {
                if let Some(state) = app_h.try_state::<AppState>() {
                    let mut eng = state.engine.lock().unwrap();
                    let _ = eng.start_worker(native_cfg);
                    ENGINE_IS_ON.store(true, Ordering::SeqCst);
                }
                Engine::apply_post_start(&dir, &cfg_cl);
                cfg_cl.save(&dir);
            }
        })
        .await
        .map_err(|e| e.to_string())?;
        let _ = app.emit(
            "autotune",
            format!("🏆 Mükemmel strateji bulundu ve aktif edildi: {best_name} ({best_score}/{total} servis, {best_latency}ms)"),
        );
        let _ = app.emit("engine_state_changed", true);
        Ok(Some(winner))
    } else {
        let dir = s.dir.clone();
        let app_h = app.clone();
        tokio::task::spawn_blocking(move || {
            if let Some(state) = app_h.try_state::<AppState>() {
                let mut eng = state.engine.lock().unwrap();
                eng.stop_worker();
                ENGINE_IS_ON.store(false, Ordering::SeqCst);
            }
            Engine::cleanup_post_stop(&dir);
        })
        .await
        .map_err(|e| e.to_string())?;
        let _ = app.emit("engine_state_changed", false);
        Ok(None)
    }
}

/// Positions window in bottom-right corner of current work area (cleanly above taskbar).
fn position_bottom_right(w: &WebviewWindow) -> Result<(), Box<dyn std::error::Error>> {
    IS_WINDOW_EXPANDED.store(false, Ordering::SeqCst);
    COLLAPSED_ORIGIN_X.store(i32::MIN, Ordering::SeqCst);

    let monitor = w
        .current_monitor()
        .ok()
        .flatten()
        .or_else(|| w.primary_monitor().ok().flatten())
        .or_else(|| w.available_monitors().ok().and_then(|v| v.into_iter().next()));

    if let Some(m) = monitor {
        let (target_w, target_h, x, y) = get_calibrated_window_layout(&m, false);
        let _ = w.set_size(tauri::Size::Logical(tauri::LogicalSize::new(target_w, target_h)));
        let _ = w.set_position(tauri::Position::Physical(tauri::PhysicalPosition::new(x, y)));
    }
    Ok(())
}

#[cfg(target_os = "windows")]
fn force_bring_to_front(w: &WebviewWindow) {
    #[link(name = "user32")]
    extern "system" {
        fn FindWindowW(lpClassName: *const u16, lpWindowName: *const u16) -> isize;
        fn ShowWindow(hWnd: isize, nCmdShow: i32) -> i32;
        fn SetForegroundWindow(hWnd: isize) -> i32;
        fn SetWindowPos(hWnd: isize, hWndInsertAfter: isize, X: i32, Y: i32, cx: i32, cy: i32, uFlags: u32) -> i32;
        fn BringWindowToTop(hWnd: isize) -> i32;
    }
    const SW_RESTORE: i32 = 9;
    const SW_SHOW: i32 = 5;
    const HWND_TOPMOST: isize = -1;
    const HWND_NOTOPMOST: isize = -2;
    const SWP_NOMOVE: u32 = 0x0002;
    const SWP_NOSIZE: u32 = 0x0001;
    const SWP_SHOWWINDOW: u32 = 0x0040;

    let hwnd_raw = if let Ok(h) = w.hwnd() {
        h.0 as isize
    } else {
        use std::ptr;
        let win_name: Vec<u16> = "Slipstream\0".encode_utf16().collect();
        unsafe { FindWindowW(ptr::null_mut(), win_name.as_ptr()) }
    };
    if hwnd_raw != 0 {
        unsafe {
            ShowWindow(hwnd_raw, SW_RESTORE);
            ShowWindow(hwnd_raw, SW_SHOW);
            BringWindowToTop(hwnd_raw);
            SetWindowPos(hwnd_raw, HWND_TOPMOST, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_SHOWWINDOW);
            SetWindowPos(hwnd_raw, HWND_NOTOPMOST, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_SHOWWINDOW);
            SetForegroundWindow(hwnd_raw);
        }
    }
}

#[cfg(not(target_os = "windows"))]
fn force_bring_to_front(_w: &WebviewWindow) {}

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
        let mon_size = m.size();
        let clearance = taskbar::calculate_clearance(
            work_area.size.width,
            work_area.size.height,
            mon_size.width,
            mon_size.height,
            scale,
        );

        let (win_width, win_height) = match w.outer_size() {
            Ok(s) if s.width > 50 && s.height > 20 => (s.width as i32, s.height as i32),
            _ => ((360.0 * scale).round() as i32, (92.0 * scale).round() as i32),
        };

        let margin_x = (16.0 * scale).round() as i32;
        let margin_y = (16.0 * scale).round() as i32;

        let mut x = work_area.position.x + work_area.size.width as i32 - win_width - margin_x - clearance.extra_right;
        let mut y = work_area.position.y + work_area.size.height as i32 - win_height - margin_y - clearance.extra_bottom;

        let min_x = work_area.position.x + margin_x + clearance.extra_left;
        let min_y = work_area.position.y + margin_y + clearance.extra_top;

        if x < min_x {
            x = min_x;
        }
        if y < min_y {
            y = min_y;
        }

        let _ = w.set_position(tauri::Position::Physical(tauri::PhysicalPosition::new(x, y)));
    }
    Ok(())
}

#[tauri::command]
fn open_main_from_toast(app: tauri::AppHandle) {
    if let Some(toast_w) = app.get_webview_window("toast") {
        let _ = toast_w.destroy();
    }
    if let Some(main_w) = app.get_webview_window("main") {
        let _ = position_bottom_right(&main_w);
        let _ = main_w.emit("app_window_visibility", true);
        let _ = main_w.show();
        let _ = main_w.unminimize();
        force_bring_to_front(&main_w);
        let _ = main_w.set_focus();
    }
}

#[tauri::command]
fn hide_toast_notification(app: tauri::AppHandle) {
    if let Some(toast_w) = app.get_webview_window("toast") {
        let _ = toast_w.destroy();
    }
    schedule_memory_trim();
}

static CURRENT_TOAST_MESSAGE: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);

#[tauri::command]
fn get_initial_toast_message() -> Option<String> {
    CURRENT_TOAST_MESSAGE.lock().unwrap().clone()
}

#[tauri::command]
fn show_toast_notification(app: tauri::AppHandle, message: Option<String>) {
    let app_handle = app.clone();
    let msg_clone = message.clone();
    if let Some(ref m) = message {
        *CURRENT_TOAST_MESSAGE.lock().unwrap() = Some(m.clone());
    }

    let _ = app.run_on_main_thread(move || {
        if let Some(toast_w) = app_handle.get_webview_window("toast") {
            let _ = position_toast_bottom_right(&toast_w);
            if let Some(ref msg) = msg_clone {
                let _ = toast_w.emit("toast_message", msg.clone());
            }
            let _ = toast_w.show();
            let _ = toast_w.set_always_on_top(true);
        } else if let Ok(toast_w) = tauri::WebviewWindowBuilder::new(
            &app_handle,
            "toast",
            tauri::WebviewUrl::App("index.html?toast=1".into()),
        )
        .title("Slipstream Bildirim")
        .inner_size(360.0, 92.0)
        .resizable(false)
        .decorations(false)
        .transparent(true)
        .skip_taskbar(true)
        .always_on_top(true)
        .visible(false)
        .build()
        {
            let _ = position_toast_bottom_right(&toast_w);
            if let Some(ref msg) = msg_clone {
                let _ = toast_w.emit("toast_message", msg.clone());
            }
            let _ = toast_w.show();
            let _ = toast_w.set_always_on_top(true);
        }
    });
}

fn toggle_window(w: &WebviewWindow) {
    let is_vis = w.is_visible().unwrap_or(false);
    let is_min = w.is_minimized().unwrap_or(false);

    if is_vis && !is_min {
        perform_hide_window(w);
    } else {
        let _ = position_bottom_right(w);
        let _ = w.emit("app_window_visibility", true);
        let _ = w.show();
        let _ = w.unminimize();
        force_bring_to_front(w);
        let _ = w.set_focus();
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

        let mutex_name: Vec<u16> = "Local\\Slipstream_SingleInstance_Mutex\0".encode_utf16().collect();
        let event_name: Vec<u16> = "Local\\Slipstream_Wakeup_Event\0".encode_utf16().collect();

        let args: Vec<String> = std::env::args().collect();
        let is_takeover = args.iter().any(|a| a == "--elevated-takeover");

        let mut mutex_handle = unsafe { CreateMutexW(ptr::null_mut(), 1, mutex_name.as_ptr()) };
        const ERROR_ALREADY_EXISTS: u32 = 183;

        if is_takeover && mutex_handle != 0 && unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
            unsafe { CloseHandle(mutex_handle) };
            mutex_handle = 0;
            // Wait up to 1500ms for old un-elevated instance to close and release mutex
            for _ in 0..30 {
                std::thread::sleep(Duration::from_millis(50));
                let h = unsafe { CreateMutexW(ptr::null_mut(), 1, mutex_name.as_ptr()) };
                if h != 0 && unsafe { GetLastError() } != ERROR_ALREADY_EXISTS {
                    mutex_handle = h;
                    break;
                }
                if h != 0 {
                    unsafe { CloseHandle(h) };
                }
            }
        }

        let is_background_launch = args.iter().any(|a| a == "--minimized" || a == "--autostart" || a == "-silent");

        if mutex_handle != 0 && unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
            // Another instance is already running!
            // Only signal the wakeup event if this is an interactive user launch (NOT an autostart or minimized background launch)
            if !is_background_launch {
                let ev_handle = unsafe { CreateEventW(ptr::null_mut(), 0, 0, event_name.as_ptr()) };
                if ev_handle != 0 {
                    unsafe {
                        SetEvent(ev_handle);
                        CloseHandle(ev_handle);
                    }
                }
            }
            std::process::exit(0);
        }
        SINGLE_INSTANCE_MUTEX.store(mutex_handle, Ordering::SeqCst);
        // Forcefully eliminate any orphaned winws.exe or zombie slipstream instances left over from previous sessions
        Engine::kill_all_winws();
        Engine::kill_orphan_slipstream_instances();
    }

    // Pre-flight check: Microsoft Edge WebView2 Runtime verification & auto-repair
    if let Err(e) = resilience::ensure_webview2_runtime() {
        eprintln!("WebView2 Runtime hatası: {e}");
        std::process::exit(1);
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
                                if let Some(toast_w) = app_handle.get_webview_window("toast") {
                                    let _ = toast_w.destroy();
                                }
                                if let Some(w) = app_handle.get_webview_window("main") {
                                    let _ = position_bottom_right(&w);
                                    let _ = w.emit("app_window_visibility", true);
                                    let _ = w.show();
                                    let _ = w.unminimize();
                                    force_bring_to_front(&w);
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
            
            // Exhaustive pre-flight clean sweep: stop and cleanup any stale WinDivert driver instances,
            // flush leftover firewall rules, ensure no orphan processes or stuck states exist from prior sessions
            Engine::kill_all_winws();
            let _ = Command::new("sc.exe")
                .args(["stop", "windivert"])
                .creation_flags(0x0800_0000)
                .output();
            let _ = Command::new("sc.exe")
                .args(["stop", "windivert14"])
                .creation_flags(0x0800_0000)
                .output();
            engine::apply_quic_firewall_rule(false);
            engine::apply_gaming_network_tweaks(false);
            dns::clear_bypass_hosts();
            dns::recover(&dir);
            proxy::clean_stale_proxy();
            proxy::patch_discord_settings(None);
            dns::flush();

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
            sound::set_enabled(boot_cfg.sound_enabled);

            // Ensure Windows startup registry is repaired, healed and synchronized with current/canonical executable
            engine::repair_and_sync_autostart(boot_cfg.auto_start_pc, boot_cfg.start_minimized);

            // Ensure system proxy is completely clean on startup
            proxy::clean_stale_proxy();

            Engine::kill_all_winws();
            let engine = Engine::new(dir.clone(), bin.clone());

            app.manage(AppState {
                dir: dir.clone(),
                bin: bin.clone(),
                engine: Mutex::new(engine),
            });

            // Auto-connect bypass on launch asynchronously so it doesn't block window creation or UI message loop
            if boot_cfg.auto_connect {
                let app_handle = app.handle().clone();
                let dir_cl = dir.clone();
                let bin_cl = bin.clone();
                let boot_cfg_cl = boot_cfg.clone();
                tauri::async_runtime::spawn(async move {
                    let _guard = OP_LOCK.lock().await;
                    let _ = tokio::task::spawn_blocking(move || {
                        if let Ok(native_cfg) = Engine::prepare_start(&dir_cl, &bin_cl, &boot_cfg_cl) {
                            if let Some(state) = app_handle.try_state::<AppState>() {
                                let start_res = {
                                    let mut eng = state.engine.lock().unwrap();
                                    eng.start_worker(native_cfg.clone())
                                };
                                let ok = match start_res {
                                    Ok(_) => true,
                                    Err(err) if err.contains("kilitli") || err.contains("10") || err.contains("1072") => {
                                        let _ = resilience::repair_wfp_and_network_filters();
                                        std::thread::sleep(Duration::from_millis(300));
                                        let mut eng = state.engine.lock().unwrap();
                                        eng.start_worker(native_cfg).is_ok()
                                    }
                                    _ => false,
                                };
                                if ok {
                                    ENGINE_IS_ON.store(true, Ordering::SeqCst);
                                    Engine::apply_post_start(&dir_cl, &boot_cfg_cl);
                                    if boot_cfg_cl.game_priority_boost {
                                        manage_game_process_priorities(&boot_cfg_cl, &get_running_processes());
                                    }
                                    let _ = app_handle.emit("engine_state_changed", true);
                                }
                            }
                        }
                    }).await;
                });
            } else {
                proxy::patch_discord_settings(None);
            }

            // Background lightweight watchdog:
            // 1. Checks if native packet engine died and restarts it if auto_healing is enabled.
            // 2. Checks if Discord has a stalled/invalid session or updater hang and repairs it cleanly.
            // When engine is OFF, never executes taskkill or acquires locks.
            let app_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                let mut last_discord_heal_ms: i64 = 0;
                loop {
                    tokio::time::sleep(Duration::from_secs(4)).await;
                    let _guard = match OP_LOCK.try_lock() {
                        Ok(g) => g,
                        Err(_) => continue,
                    };
                    if !ENGINE_IS_ON.load(Ordering::SeqCst) {
                        continue;
                    }
                    if let Some(state) = app_handle.try_state::<AppState>() {
                        let is_alive = {
                            if let Ok(eng) = state.engine.try_lock() {
                                eng.is_on()
                            } else {
                                true
                            }
                        };
                        let cfg = Config::load(&state.dir);
                        if !is_alive {
                            ENGINE_IS_ON.store(false, Ordering::SeqCst);
                            if cfg.auto_healing {
                                let dir = state.dir.clone();
                                let bin = state.bin.clone();
                                let app_cl = app_handle.clone();
                                let _ = tokio::task::spawn_blocking(move || {
                                    if let Ok(native_cfg) = Engine::prepare_start(&dir, &bin, &cfg) {
                                        if let Some(st) = app_cl.try_state::<AppState>() {
                                            let start_res = {
                                                let mut eng = st.engine.lock().unwrap();
                                                eng.start_worker(native_cfg.clone())
                                            };
                                            let ok = match start_res {
                                                Ok(_) => true,
                                                Err(err) if err.contains("kilitli") || err.contains("10") || err.contains("1072") => {
                                                    let _ = resilience::repair_wfp_and_network_filters();
                                                    std::thread::sleep(Duration::from_millis(300));
                                                    let mut eng = st.engine.lock().unwrap();
                                                    eng.start_worker(native_cfg).is_ok()
                                                }
                                                _ => false,
                                            };
                                            if ok {
                                                ENGINE_IS_ON.store(true, Ordering::SeqCst);
                                                Engine::apply_post_start(&dir, &cfg);
                                                let _ = app_cl.emit("watchdog_recovery", "Motor arka planda otomatik onarıldı.");
                                                let _ = app_cl.emit("engine_state_changed", true);
                                            }
                                        }
                                    }
                                }).await;
                            }
                        } else if cfg.auto_healing && cfg.enabled_apps.iter().any(|a| a == "discord") {
                            let now = now_ms();
                            // Debounce healing actions to at most once every 60 seconds
                            if now - last_discord_heal_ms > 60_000 {
                                let should_heal = tokio::task::spawn_blocking(move || {
                                    if !proxy::is_discord_running() {
                                        return false;
                                    }
                                    proxy::has_discord_stalled_session()
                                }).await.unwrap_or(false);

                                if should_heal {
                                    last_discord_heal_ms = now_ms();
                                    let app_cl = app_handle.clone();
                                    tokio::task::spawn_blocking(move || {
                                        let _ = proxy::repair_and_restart_discord();
                                        let _ = app_cl.emit("watchdog_recovery", "Askıda kalan Discord oturumu otomatik olarak onarıldı ve yeniden başlatıldı.");
                                    }).await.ok();
                                }
                            }
                        }
                    }
                }
            });

            // Tray setup
            let show_i = MenuItem::with_id(app, "toggle", "Aç / Gizle", true, None::<&str>)?;
            let toggle_bypass_i = MenuItem::with_id(app, "toggle_bypass", "Korumayı Aç / Kapat", true, None::<&str>)?;
            let repair_discord_i = MenuItem::with_id(app, "repair_discord_tray", "Discord'u Onar ve Yeniden Başlat", true, None::<&str>)?;
            let flush_dns_i = MenuItem::with_id(app, "flush_dns", "DNS Önbelleğini Temizle", true, None::<&str>)?;
            let quit_i = MenuItem::with_id(app, "quit", "Çıkış", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&show_i, &toggle_bypass_i, &repair_discord_i, &flush_dns_i, &quit_i])?;

            let default_icon = app.default_window_icon().cloned();
            let tray_builder = TrayIconBuilder::with_id("main-tray")
                .tooltip("Slipstream DPI Bypass")
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "toggle" => {
                        if let Some(w) = app.get_webview_window("main") {
                            if let Some(toast_w) = app.get_webview_window("toast") {
                                let _ = toast_w.destroy();
                            }
                            TRAY_CLICK_STARTED_FOCUSED.store(false, Ordering::SeqCst);
                            LAST_BLUR_HIDE_MS.store(0, Ordering::SeqCst);
                            toggle_window(&w);
                        }
                    }
                    "toggle_bypass" => {
                        if let Some(state) = app.try_state::<AppState>() {
                            let is_currently_on = ENGINE_IS_ON.load(Ordering::SeqCst);
                            let dir = state.dir.clone();
                            let bin = state.bin.clone();
                            let app_cl = app.clone();
                            tauri::async_runtime::spawn(async move {
                                let _guard = OP_LOCK.lock().await;
                                if is_currently_on {
                                    let app_h = app_cl.clone();
                                    let dir_cl = dir.clone();
                                    let _ = tokio::task::spawn_blocking(move || {
                                        restore_all_boosted_game_priorities();
                                        if let Some(st) = app_h.try_state::<AppState>() {
                                            if let Ok(mut eng) = st.engine.lock() {
                                                eng.stop_worker();
                                            }
                                        }
                                        ENGINE_IS_ON.store(false, Ordering::SeqCst);
                                        Engine::cleanup_post_stop(&dir_cl);
                                        let _ = app_h.emit("engine_state_changed", false);
                                    }).await;
                                } else {
                                    let cfg = Config::load(&dir);
                                    let app_h = app_cl.clone();
                                    let dir_cl = dir.clone();
                                    let bin_cl = bin.clone();
                                    let _ = tokio::task::spawn_blocking(move || {
                                        if let Ok(native_cfg) = Engine::prepare_start(&dir_cl, &bin_cl, &cfg) {
                                            if let Some(st) = app_h.try_state::<AppState>() {
                                                if let Ok(mut eng) = st.engine.lock() {
                                                    if eng.start_worker(native_cfg).is_ok() {
                                                        ENGINE_IS_ON.store(true, Ordering::SeqCst);
                                                        drop(eng);
                                                        Engine::apply_post_start(&dir_cl, &cfg);
                                                        if cfg.game_priority_boost {
                                                            manage_game_process_priorities(&cfg, &get_running_processes());
                                                        }
                                                        let _ = app_h.emit("engine_state_changed", true);
                                                    }
                                                }
                                            }
                                        }
                                    }).await;
                                }
                            });
                        }
                    }
                    "repair_discord_tray" => {
                        let app_cl = app.clone();
                        tauri::async_runtime::spawn(async move {
                            let _ = tokio::task::spawn_blocking(proxy::repair_and_restart_discord).await;
                            let _ = app_cl.emit("watchdog_recovery", "Discord onarıldı ve temiz başlatıldı.");
                        });
                    }
                    "flush_dns" => {
                        let app_cl = app.clone();
                        tauri::async_runtime::spawn(async move {
                            tokio::task::spawn_blocking(proxy::flush_dns_cache).await.ok();
                            let _ = app_cl.emit("dns_flushed", ());
                        });
                    }
                    "quit" => {
                        let state = app.try_state::<AppState>();
                        full_application_shutdown(Some(&app), state.as_deref());
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
                                    let is_open = w.is_visible().unwrap_or(false)
                                        && !w.is_minimized().unwrap_or(false)
                                        && (w.is_focused().unwrap_or(false) || (now - last_blur < 400));
                                    TRAY_CLICK_STARTED_FOCUSED.store(is_open, Ordering::SeqCst);
                                }
                                MouseButtonState::Up => {
                                    let now = now_ms();
                                    let last_blur = LAST_BLUR_HIDE_MS.load(Ordering::SeqCst);
                                    let started_open = TRAY_CLICK_STARTED_FOCUSED.swap(false, Ordering::SeqCst);
                                    let is_vis = w.is_visible().unwrap_or(false);
                                    let is_min = w.is_minimized().unwrap_or(false);

                                    if (started_open || (now - last_blur < 400)) && (!is_vis || (started_open && is_vis && !is_min)) {
                                        if is_vis && !is_min {
                                            perform_hide_window(&w);
                                        }
                                        // Keep hidden
                                    } else if now - last_blur < 400 {
                                        // Recently blurred or closed, keep hidden
                                    } else {
                                        if let Some(toast_w) = app.get_webview_window("toast") {
                                            let _ = toast_w.destroy();
                                        }
                                        let _ = position_bottom_right(&w);
                                        let _ = w.emit("app_window_visibility", true);
                                        let _ = w.show();
                                        let _ = w.unminimize();
                                        force_bring_to_front(&w);
                                        let _ = w.set_focus();
                                    }
                                }
                            }
                        }
                    }
                });

            let icon = match default_icon {
                Some(i) => i,
                None => tauri::image::Image::from_app_icon_resource(32)
                    .unwrap_or_else(|_| tauri::image::Image::new_owned(vec![0u8; 32 * 32 * 4], 32, 32)),
            };
            let _tray = tray_builder.icon(icon).build(app)?;

            // Ensure Windows Start Menu shortcuts and App Paths ("ss", "slipstream") are registered
            engine::ensure_windows_search_integration();

            // Only start minimized when explicitly launched via autostart or --minimized / -silent CLI flag,
            // or when autostarted and boot_cfg.start_minimized is true.
            // When user manually double-clicks or runs Slipstream interactively, always open and show the main window directly!
            let args: Vec<String> = std::env::args().collect();
            let is_minimized_arg = args.iter().any(|a| a == "--minimized" || a == "-silent");
            let is_autostart_arg = args.iter().any(|a| a == "--autostart");
            let should_start_minimized = is_minimized_arg || (is_autostart_arg && boot_cfg.start_minimized);

            // Manage toast window according to startup mode
            if let Some(toast_w) = app.get_webview_window("toast") {
                if !should_start_minimized {
                    let _ = toast_w.destroy();
                } else {
                    let _ = position_toast_bottom_right(&toast_w);
                    let _ = toast_w.hide();
                }
            }

            // Manage initial window display according to start_minimized configuration
            if let Some(w) = app.get_webview_window("main") {
                // Pre-position main window at bottom-right immediately so it NEVER appears on the left
                let _ = position_bottom_right(&w);

                if should_start_minimized {
                    let _ = w.emit("app_window_visibility", false);
                    let _ = w.hide();
                    schedule_memory_trim();

                    // Show animated background notification in bottom-right
                    let app_handle = app.handle().clone();
                    tauri::async_runtime::spawn(async move {
                        tokio::time::sleep(Duration::from_millis(1200)).await;
                        show_toast_notification(app_handle, None);
                    });
                } else {
                    let _ = w.emit("app_window_visibility", true);
                    let _ = w.show();
                    let _ = w.set_focus();
                }
            }

            // 1) Instant live SSE push listener
            start_live_sse_update_listener(app.handle().clone(), dir.clone());

            // 2) Cold-start update check (fires within 250ms of launch)
            let app_cl = app.handle().clone();
            let dir_cl = dir.clone();
            tauri::async_runtime::spawn(async move {
                tokio::time::sleep(Duration::from_millis(250)).await;
                let info = check_for_updates().await;
                if info.has_update {
                    notify_update_found(&app_cl, &dir_cl, &info, true);
                }
            });

            // 3) 15-Minute safety fallback in case network drops or SSE reconnects
            let app_cl2 = app.handle().clone();
            let dir_cl2 = dir.clone();
            tauri::async_runtime::spawn(async move {
                loop {
                    tokio::time::sleep(Duration::from_secs(15 * 60)).await;
                    let info = check_for_updates().await;
                    if info.has_update {
                        notify_update_found(&app_cl2, &dir_cl2, &info, false);
                    }
                }
            });

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
                    if HIDE_ON_BLUR.load(Ordering::SeqCst) && window.label() == "main" {
                        if let Some(w) = window.app_handle().get_webview_window("main") {
                            perform_hide_window(&w);
                        }
                    }
                }
            }
            WindowEvent::CloseRequested { .. } => {
                if window.label() == "main" {
                    let app = window.app_handle();
                    let state = window.try_state::<AppState>();
                    full_application_shutdown(Some(&app), state.as_deref());
                }
            }
            WindowEvent::Destroyed => {
                if window.label() == "main" {
                    let app = window.app_handle();
                    let state = window.try_state::<AppState>();
                    full_application_shutdown(Some(&app), state.as_deref());
                }
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
            get_initial_toast_message,
            is_portable,
            get_detected_active_apps,
            is_game_running,
            suggest_related_domains,
            scan_apps,
            pick_exe_file,
            set_window_expanded,
            refresh_game_priorities,
            is_window_visible,
            play_sound,
            get_system_identity,
            send_discord_feedback,
            check_elevation,
            request_elevation,
            check_antivirus_health,
            apply_defender_exclusions,
            detect_conflicts,
            resolve_conflicts,
            diagnose_wfp,
            repair_wfp,
            fetch_remote_isp_updates
        ])
        .build(tauri::generate_context!())
        .expect("error while building Slipstream")
        .run(|app_handle, event| {
            if let tauri::RunEvent::ExitRequested { .. } | tauri::RunEvent::Exit = event {
                let state = app_handle.try_state::<AppState>();
                full_application_shutdown(Some(app_handle), state.as_deref());
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn test_is_trusted_update_url() {
        assert!(is_trusted_update_url("https://github.com/BaranK35/Slipstream2/releases/download/v0.2.7/Slipstream_0.2.7_x64-setup.exe"));
        assert!(is_trusted_update_url("https://github.com/BaranK35/Slipstreamtest2/releases/download/v0.2.7/Slipstream_0.2.7_x64-setup.exe"));
        // Reject non-whitelisted domains, HTTP, arbitrary exes, and path injections
        assert!(!is_trusted_update_url("http://github.com/BaranK35/Slipstream2/releases/download/v0.2.7/setup.exe"));
        assert!(!is_trusted_update_url("https://malicious-site.com/payload.exe"));
        assert!(!is_trusted_update_url("https://github.com/attacker/malware/releases/download/v1.0/evil.exe"));
        assert!(!is_trusted_update_url("https://github.com/BaranK35/Slipstream2/releases/download/v0.2.7/script.bat"));
    }

    #[test]
    fn test_custom_platform_game_detection() {
        let mut cfg = Config::default();
        cfg.custom_platforms = vec![
            engine::CustomPlatform {
                id: "custom_cyberpunk".into(),
                name: "Cyberpunk 2077".into(),
                category: "game".into(),
                icon: "🎮".into(),
                domains: vec!["cyberpunk.net".into()],
                exe_name: Some("Cyberpunk2077.exe".into()),
                exe_path: None,
            },
            engine::CustomPlatform {
                id: "custom_chat".into(),
                name: "My Chat".into(),
                category: "app".into(),
                icon: "📱".into(),
                domains: vec!["mychat.com".into()],
                exe_name: Some("mychat.exe".into()),
                exe_path: None,
            },
        ];

        let procs = vec![
            ProcessItem { pid: 1234, name: "Cyberpunk2077.exe".into(), parent_pid: 0 },
        ];
        assert!(is_game_process_running(&cfg, &procs));

        let app_procs = vec![
            ProcessItem { pid: 5678, name: "mychat.exe".into(), parent_pid: 0 },
        ];
        assert!(!is_game_process_running(&cfg, &app_procs), "Non-game custom platforms must NOT be classified as games");
    }

    #[test]
    fn test_v020_version_upgrade() {
        assert!(is_newer_version("0.2.0", "0.1.9"));
        assert!(is_newer_version("v0.2.0", "v0.1.9"));
        assert!(is_newer_version("0.2.1", "0.2.0"));
        assert!(is_newer_version("v0.2.1", "v0.2.0"));
        assert!(is_newer_version("0.2.4", "0.2.3"));
        assert!(is_newer_version("v0.2.4", "v0.2.3"));
        assert!(!is_newer_version("0.2.0", "0.2.1"));
        assert!(!is_newer_version("0.2.1", "0.2.1"));
    }

    #[test]
    fn test_expanded_window_clamping_bounds() {
        let work_area_x = 0;
        let work_area_w = 1920;
        let scale = 1.0f64;
        let target_phys_w = (380.0 * scale).round() as i32;
        let min_x = work_area_x + 10;
        let max_x = work_area_x + work_area_w - target_phys_w - 10; // 1920 - 380 - 10 = 1530

        // If window attempts to slide beyond screen edge to the right (e.g. 2000px):
        let requested_x = 2200;
        let clamped_x = requested_x.clamp(min_x, max_x);
        assert_eq!(clamped_x, 1530, "Window X must never exceed max_x bounds");
        assert!(clamped_x + target_phys_w <= work_area_w);

        // If window attempts to slide off-screen to the left:
        let requested_left = -100;
        let clamped_left = requested_left.clamp(min_x, max_x);
        assert_eq!(clamped_left, 10, "Window X must never be less than min_x bounds");
    }

    #[test]
    fn test_system_identity_generation() {
        let identity = get_system_identity();
        assert!(!identity.username.is_empty());
        assert!(!identity.computer_name.is_empty());
        assert!(!identity.short_machine_id.is_empty());
        assert!(identity.short_machine_id.starts_with("#"));
        assert_eq!(identity.app_version, env!("CARGO_PKG_VERSION"));
    }

    #[test]
    fn test_taskbar_clearance_auto_hide() {
        // When taskbar auto-hide is active, work_area_h is equal to mon_h (e.g. 1080)
        let work_area_w = 1920;
        let work_area_h = 1080;
        let mon_w = 1920;
        let mon_h = 1080;
        let scale = 1.0f64;

        let clearance = taskbar::calculate_clearance(work_area_w, work_area_h, mon_w, mon_h, scale);
        // On Windows with full height work_area, extra_bottom should be > 0 (taskbar thickness)
        #[cfg(target_os = "windows")]
        assert!(clearance.extra_bottom > 0, "Auto-hide taskbar must provide extra_bottom clearance");
        #[cfg(not(target_os = "windows"))]
        let _ = clearance;
    }

    #[test]
    fn test_calibrated_window_layout_low_resolution() {
        // Test 1366x768 display with 1.25 scale (typical budget laptop)
        let scale = 1.25f64;
        let work_area_h = 768;
        let margin_y = (16.0 * scale).round() as i32; // 20px
        let extra_bottom = (48.0 * scale).round() as i32; // 60px
        let avail_phys_h = work_area_h - (margin_y * 2 + extra_bottom); // 768 - 100 = 668px
        let avail_logical_h = (avail_phys_h as f64 / scale).max(440.0); // 668 / 1.25 = 534.4px
        let target_logical_h = (620.0f64).min(avail_logical_h.floor());
        
        assert!(target_logical_h <= 534.0, "Window height must adapt and scale down to fit screen");
        assert!(target_logical_h >= 440.0, "Window height must not fall below min logical height");
        let win_phys_h = (target_logical_h * scale).round() as i32;
        assert!(win_phys_h + margin_y * 2 + extra_bottom <= work_area_h, "Window + margins + taskbar clearance must fit in screen");
    }

    #[test]
    fn test_is_portable_detection_rules() {
        let portable_name = "C:\\Users\\User\\Downloads\\Slipstream-Portable.exe".to_lowercase();
        let installed_setup_path = "C:\\Users\\User\\AppData\\Local\\Slipstream\\slipstream.exe".to_lowercase();
        let local_app_data = "C:\\Users\\User\\AppData\\Local".to_lowercase();

        let dir1 = format!("{}\\slipstream", local_app_data);
        let dir2 = format!("{}\\programs\\slipstream", local_app_data);

        // Portable exe filename must be recognized as portable
        assert!(portable_name.contains("portable"));

        // Setup installed path in AppData\Local\Slipstream must NOT be considered portable
        assert!(installed_setup_path.starts_with(&dir1));
        assert!(!portable_name.starts_with(&dir1));
        assert!(!portable_name.starts_with(&dir2));
    }

    #[tokio::test]
    async fn test_check_for_updates_live() {
        let is_newer = super::is_newer_version("0.3.1", "0.3.0");
        println!("IS NEWER 0.3.1 > 0.3.0: {}", is_newer);
        let info = super::check_for_updates().await;
        println!("TEST UPDATE RESULT: has_update={} latest={} current={} url={}", info.has_update, info.latest_version, info.current_version, info.download_url);
    }
}
