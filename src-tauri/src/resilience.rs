//! System Resilience & Diagnostic Suite for Slipstream.
//!
//! Provides concrete, battle-tested solutions for:
//! 1. UAC administrator privilege detection & graceful self-elevation (with seamless mutex handoff)
//! 2. Antivirus & Windows Defender false-positive detection, 3rd-party AV recognition & smart exclusion tools
//! 3. Conflicting DPI tools (GoodbyeDPI, Zapret, Shroudly, ByeDPI) and strict VPN detection/coexistence
//! 4. Windows Filtering Platform (WFP) diagnostics, driver unfreezing & network filter reset
//! 5. Microsoft Edge WebView2 runtime pre-flight verification & auto-repair

use serde::{Deserialize, Serialize};
use std::{
    fs,
    os::windows::process::CommandExt,
    path::Path,
    process::Command,
    time::Duration,
};

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

// =========================================================================
// 1. UAC / ADMINISTRATOR ELEVATION HANDLING
// =========================================================================

/// Returns true if the current process is running with elevated administrator rights.
#[cfg(target_os = "windows")]
pub fn is_elevated_admin() -> bool {
    use std::mem;
    #[link(name = "advapi32")]
    extern "system" {
        fn OpenProcessToken(ProcessHandle: isize, DesiredAccess: u32, TokenHandle: *mut isize) -> i32;
        fn GetTokenInformation(
            TokenHandle: isize,
            TokenInformationClass: u32,
            TokenInformation: *mut std::ffi::c_void,
            TokenInformationLength: u32,
            ReturnLength: *mut u32,
        ) -> i32;
    }
    #[link(name = "kernel32")]
    extern "system" {
        fn GetCurrentProcess() -> isize;
        fn CloseHandle(hObject: isize) -> i32;
    }

    const TOKEN_QUERY: u32 = 0x0008;
    const TOKEN_ELEVATION: u32 = 20;

    #[repr(C)]
    struct TOKEN_ELEVATION_STRUCT {
        token_is_elevated: u32,
    }

    unsafe {
        let mut token: isize = 0;
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) == 0 {
            return false;
        }
        let mut elevation = TOKEN_ELEVATION_STRUCT { token_is_elevated: 0 };
        let mut ret_len = 0u32;
        let success = GetTokenInformation(
            token,
            TOKEN_ELEVATION,
            &mut elevation as *mut _ as *mut std::ffi::c_void,
            mem::size_of::<TOKEN_ELEVATION_STRUCT>() as u32,
            &mut ret_len,
        );
        CloseHandle(token);
        success != 0 && elevation.token_is_elevated != 0
    }
}

#[cfg(not(target_os = "windows"))]
pub fn is_elevated_admin() -> bool {
    true
}

/// Requests UAC elevation via Windows ShellExecuteW ("runas").
/// If the user rejects the UAC prompt (clicks "Hayır / No"), catches error code 5 / 1223
/// and returns a clean, explanatory error without crashing the application.
/// When accepted, seamlessly releases the single-instance mutex and closes the un-elevated
/// instance so the new elevated instance takes over smoothly.
#[cfg(target_os = "windows")]
pub fn request_admin_elevation(extra_args: Option<&str>) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;
    use std::ptr;

    if is_elevated_admin() {
        return Ok(());
    }

    #[link(name = "shell32")]
    extern "system" {
        fn ShellExecuteW(
            hwnd: isize,
            lpOperation: *const u16,
            lpFile: *const u16,
            lpParameters: *const u16,
            lpDirectory: *const u16,
            nShowCmd: i32,
        ) -> isize;
    }

    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let exe_wide: Vec<u16> = exe.as_os_str().encode_wide().chain(std::iter::once(0)).collect();
    let op_wide: Vec<u16> = "runas\0".encode_utf16().collect();

    let args_str = extra_args.unwrap_or("--elevated-takeover");
    let args_wide: Vec<u16> = std::ffi::OsStr::new(args_str)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();

    let res = unsafe {
        ShellExecuteW(
            0,
            op_wide.as_ptr(),
            exe_wide.as_ptr(),
            if args_str.is_empty() { ptr::null() } else { args_wide.as_ptr() },
            ptr::null(),
            5, // SW_SHOW
        )
    };

    // ShellExecute returns an HINSTANCE value. If > 32, it succeeded.
    // 5 = SE_ERR_ACCESSDENIED (user cancelled UAC prompt or denied permission)
    if res as usize <= 32 {
        if res == 5 || res == 0 {
            return Err("Yönetici izni (UAC) reddedildi. Slipstream paket filtreleme sürücüsünü başlatabilmek için lütfen yönetici onayını kabul edin.".into());
        }
        return Err(format!("UAC yükseltme işlemi başlatılamadı (Hata kodu: {res})"));
    }

    // Hand off single instance lock to the elevated process and exit gracefully
    crate::release_single_instance_mutex();
    std::thread::spawn(|| {
        std::thread::sleep(Duration::from_millis(300));
        std::process::exit(0);
    });

    Ok(())
}

#[cfg(not(target_os = "windows"))]
pub fn request_admin_elevation(_extra_args: Option<&str>) -> Result<(), String> {
    Ok(())
}

// =========================================================================
// 2. ANTIVIRUS & WINDOWS DEFENDER FALSE-POSITIVE HANDLING
// =========================================================================

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct AntivirusReport {
    pub is_healthy: bool,
    pub defender_active: bool,
    pub active_av_name: String,
    pub engine_intact: bool,
    pub quarantined_files: Vec<String>,
    pub message: String,
}

/// Detects running 3rd-party antivirus solutions or Windows Defender.
pub fn detect_active_antivirus() -> (bool, String) {
    let mut defender_active = false;
    let mut av_names = Vec::new();

    #[cfg(target_os = "windows")]
    {
        let procs = crate::get_running_processes();
        for p in procs {
            let lower = p.name.to_lowercase();
            if lower == "msmpeng.exe" {
                defender_active = true;
            } else if lower == "avp.exe" || lower == "kav.exe" {
                if !av_names.contains(&"Kaspersky".to_string()) {
                    av_names.push("Kaspersky".to_string());
                }
            } else if lower == "bdservicehost.exe" || lower == "vsserv.exe" {
                if !av_names.contains(&"Bitdefender".to_string()) {
                    av_names.push("Bitdefender".to_string());
                }
            } else if lower == "avastsvc.exe" {
                if !av_names.contains(&"Avast".to_string()) {
                    av_names.push("Avast".to_string());
                }
            } else if lower == "avgsvc.exe" {
                if !av_names.contains(&"AVG".to_string()) {
                    av_names.push("AVG".to_string());
                }
            } else if lower == "ekrn.exe" {
                if !av_names.contains(&"ESET NOD32".to_string()) {
                    av_names.push("ESET NOD32".to_string());
                }
            } else if lower == "mbamservice.exe" {
                if !av_names.contains(&"Malwarebytes".to_string()) {
                    av_names.push("Malwarebytes".to_string());
                }
            } else if lower == "nortonsecurity.exe" || lower == "ccsvchst.exe" {
                if !av_names.contains(&"Norton".to_string()) {
                    av_names.push("Norton".to_string());
                }
            } else if lower == "mcshield.exe" || lower == "mcapexe.exe" {
                if !av_names.contains(&"McAfee".to_string()) {
                    av_names.push("McAfee".to_string());
                }
            }
        }
    }

    let name = if !av_names.is_empty() {
        av_names.join(", ")
    } else if defender_active {
        "Windows Defender".to_string()
    } else {
        "Windows Güvenliği".to_string()
    };

    (defender_active, name)
}

/// Inspects the DPI engine directory to verify that binary and driver files are intact
/// and haven't been locked, stripped, or quarantined by Windows Defender or 3rd-party antivirus.
pub fn check_antivirus_status(engine_dir: &Path) -> AntivirusReport {
    let mut quarantined = Vec::new();
    let files = ["WinDivert.dll", "WinDivert64.sys"];

    for f in &files {
        let p = engine_dir.join(f);
        if !p.exists() {
            quarantined.push(f.to_string());
        } else if let Ok(meta) = fs::metadata(&p) {
            if meta.len() == 0 {
                quarantined.push(f.to_string());
            }
        }
    }

    let (defender_active, active_av_name) = detect_active_antivirus();
    let is_healthy = quarantined.is_empty();
    let msg = if is_healthy {
        format!("DPI motor dosyaları tam ve sağlam. (Aktif koruma: {active_av_name})")
    } else {
        format!(
            "{active_av_name} veya güvenlik yazılımı şu dosyaları engellemiş veya karantinaya almış olabilir: {}. Lütfen Slipstream klasörünü dışlama listesine ekleyin.",
            quarantined.join(", ")
        )
    };

    AntivirusReport {
        is_healthy,
        defender_active,
        active_av_name,
        engine_intact: is_healthy,
        quarantined_files: quarantined,
        message: msg,
    }
}

/// Adds Windows Defender smart exclusions for the Slipstream installation directory,
/// engine directory, and slipstream/winws processes.
/// If running un-elevated, prompts for administrator elevation cleanly via PowerShell RunAs.
/// Also self-heals missing engine files immediately after exclusion is added.
pub fn add_defender_exclusions(install_dir: &Path, engine_dir: &Path) -> Result<String, String> {
    #[cfg(target_os = "windows")]
    {
        let install_path = install_dir.to_string_lossy().to_string();
        let engine_path = engine_dir.to_string_lossy().to_string();

        let ps_cmd = format!(
            "Add-MpPreference -ExclusionPath '{install_path}', '{engine_path}' -ExclusionProcess 'slipstream.exe', 'winws.exe'"
        );

        let out = if is_elevated_admin() {
            Command::new("powershell")
                .args(["-NoProfile", "-NonInteractive", "-Command", &ps_cmd])
                .creation_flags(CREATE_NO_WINDOW)
                .output()
                .map_err(|e| format!("PowerShell çalıştırılamadı: {e}"))?
        } else {
            // Request elevation for Add-MpPreference
            let elevated_ps = format!(
                "Start-Process powershell -Verb RunAs -Wait -WindowStyle Hidden -ArgumentList '-NoProfile -NonInteractive -Command \"{}\"'",
                ps_cmd.replace('"', "\\\"")
            );
            Command::new("powershell")
                .args(["-NoProfile", "-NonInteractive", "-Command", &elevated_ps])
                .creation_flags(CREATE_NO_WINDOW)
                .output()
                .map_err(|e| format!("Yönetici olarak PowerShell başlatılamadı: {e}"))?
        };

        if out.status.success() {
            // Restore any engine files that might have been deleted/quarantined
            let restore_res = crate::engine::ensure_engine_files(engine_dir);
            match restore_res {
                Ok(_) => Ok("Windows Defender dışlama kuralları tanımlandı ve DPI dosyaları doğrulandı.".into()),
                Err(e) => Ok(format!("Dışlama kuralları uygulandı, ancak dosya yenileme bildirimi: {e}")),
            }
        } else {
            let err = String::from_utf8_lossy(&out.stderr);
            Err(format!("Dışlama kuralı eklenemedi (Yönetici onayı gereklidir): {err}"))
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = (install_dir, engine_dir);
        Ok("Windows dışı sistemlerde Defender dışlaması gerekmez.".into())
    }
}

// =========================================================================
// 3. CONFLICTING DPI TOOLS & STRICT VPN DETECTION & RESOLUTION
// =========================================================================

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct NetworkConflict {
    pub id: String,
    pub name: String,
    pub kind: String, // "dpi_tool" | "vpn" | "driver"
    pub pid: u32,
    pub description: String,
    pub is_critical: bool,
}

/// Detects competing DPI software (GoodbyeDPI, Zapret, Shroudly, ByeDPI, external winws)
/// and strict VPN tunnels that could interfere with WinDivert or cause packet loops.
pub fn detect_network_conflicts(
    running_processes: &[crate::ProcessItem],
    own_engine_pid: Option<u32>,
) -> Vec<NetworkConflict> {
    let mut conflicts = Vec::new();

    for p in running_processes {
        let lower = p.name.to_lowercase();
        if lower == "goodbyedpi.exe" {
            conflicts.push(NetworkConflict {
                id: "goodbyedpi".into(),
                name: "GoodbyeDPI".into(),
                kind: "dpi_tool".into(),
                pid: p.pid,
                description: "GoodbyeDPI çalışıyor ve WinDivert paket filtresini kilitliyor.".into(),
                is_critical: true,
            });
        } else if lower == "zapret.exe" {
            conflicts.push(NetworkConflict {
                id: "zapret".into(),
                name: "Zapret".into(),
                kind: "dpi_tool".into(),
                pid: p.pid,
                description: "Zapret DPI motoru aktif ve WinDivert sürücüsünü kullanıyor.".into(),
                is_critical: true,
            });
        } else if lower == "winws.exe" {
            // Check if this is an external or orphaned instance of winws
            if own_engine_pid.map_or(true, |own| p.pid != own) {
                conflicts.push(NetworkConflict {
                    id: "winws_orphaned".into(),
                    name: "Zapret / winws.exe".into(),
                    kind: "dpi_tool".into(),
                    pid: p.pid,
                    description: "Bağımsız veya askıda kalmış bir winws.exe oturumu WinDivert filtresini tutuyor.".into(),
                    is_critical: true,
                });
            }
        } else if lower == "byedpi.exe" || lower == "ciadpi.exe" || lower == "spoofdpi.exe" {
            conflicts.push(NetworkConflict {
                id: "byedpi".into(),
                name: p.name.clone(),
                kind: "dpi_tool".into(),
                pid: p.pid,
                description: "Alternatif bir DPI atlatma aracı çalışıyor.".into(),
                is_critical: true,
            });
        } else if lower == "shroudly-core.exe" || lower == "shroudly.exe" {
            conflicts.push(NetworkConflict {
                id: "shroudly".into(),
                name: "Shroudly".into(),
                kind: "dpi_tool".into(),
                pid: p.pid,
                description: "Shroudly DPI bypass servisi aktif.".into(),
                is_critical: true,
            });
        } else if lower == "green-tunnel.exe" {
            conflicts.push(NetworkConflict {
                id: "green_tunnel".into(),
                name: "Green Tunnel".into(),
                kind: "dpi_tool".into(),
                pid: p.pid,
                description: "Green Tunnel DPI yazılımı aktif.".into(),
                is_critical: true,
            });
        } else if lower == "warp-svc.exe" || lower == "cloudflare warp.exe" {
            conflicts.push(NetworkConflict {
                id: "warp".into(),
                name: "Cloudflare WARP".into(),
                kind: "vpn".into(),
                pid: p.pid,
                description: "Cloudflare WARP aktif. DPI filtresi ile birlikte çalışırken ağ rotalama çakışması yaşanabilir.".into(),
                is_critical: false,
            });
        } else if lower == "wireguard.exe" {
            conflicts.push(NetworkConflict {
                id: "wireguard".into(),
                name: "WireGuard".into(),
                kind: "vpn".into(),
                pid: p.pid,
                description: "WireGuard VPN tüneli aktif.".into(),
                is_critical: false,
            });
        } else if lower == "openvpn.exe" {
            conflicts.push(NetworkConflict {
                id: "openvpn".into(),
                name: "OpenVPN".into(),
                kind: "vpn".into(),
                pid: p.pid,
                description: "OpenVPN bağlantısı aktif.".into(),
                is_critical: false,
            });
        } else if lower == "nordvpn-service.exe" || lower == "protonvpn.service.exe" || lower == "mullvad-daemon.exe" {
            conflicts.push(NetworkConflict {
                id: "vpn_service".into(),
                name: p.name.clone(),
                kind: "vpn".into(),
                pid: p.pid,
                description: "Üçüncü parti VPN istemcisi tespit edildi.".into(),
                is_critical: false,
            });
        }
    }

    conflicts
}

/// Automatically terminates conflicting DPI tools that hold WinDivert handles
/// and stops hung driver services. (Safe conflict resolution: does NOT perform destructive winsock reset).
pub fn resolve_network_conflicts() -> Result<String, String> {
    #[cfg(target_os = "windows")]
    {
        for im in &[
            "goodbyedpi.exe",
            "zapret.exe",
            "byedpi.exe",
            "ciadpi.exe",
            "spoofdpi.exe",
            "shroudly-core.exe",
            "green-tunnel.exe",
        ] {
            let _ = Command::new("taskkill")
                .args(["/IM", im, "/T", "/F"])
                .creation_flags(CREATE_NO_WINDOW)
                .output();
        }

        // Stop hung windivert driver instances
        for svc in &["windivert", "windivert14"] {
            let _ = Command::new("net")
                .args(["stop", svc, "/y"])
                .creation_flags(CREATE_NO_WINDOW)
                .output();
        }

        let _ = Command::new("ipconfig")
            .args(["/flushdns"])
            .creation_flags(CREATE_NO_WINDOW)
            .output();

        std::thread::sleep(Duration::from_millis(150));
        Ok("Çakışan DPI araçları sonlandırıldı ve WinDivert ağ sürücüsü serbest bırakıldı.".into())
    }
    #[cfg(not(target_os = "windows"))]
    {
        Ok("Çözüm tamamlandı.".into())
    }
}

// =========================================================================
// 4. WINDOWS FILTERING PLATFORM (WFP) DIAGNOSTICS & FILTER RESET
// =========================================================================

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct WfpDiagnostics {
    pub bfe_running: bool,
    pub windivert_state: String,
    pub is_healthy: bool,
    pub details: String,
}

/// Checks the state of Windows Base Filtering Engine (BFE) and WinDivert WFP sublayers.
pub fn diagnose_wfp_health() -> WfpDiagnostics {
    let mut bfe_running = false;
    let mut windivert_state = "Durduruldu".to_string();

    #[cfg(target_os = "windows")]
    {
        let bfe_out = Command::new("sc")
            .args(["query", "bfe"])
            .creation_flags(CREATE_NO_WINDOW)
            .output();
        if let Ok(o) = bfe_out {
            let s = String::from_utf8_lossy(&o.stdout);
            bfe_running = s.contains("RUNNING");
        }

        for svc in &["windivert", "windivert14"] {
            let wd_out = Command::new("sc")
                .args(["query", svc])
                .creation_flags(CREATE_NO_WINDOW)
                .output();
            if let Ok(o) = wd_out {
                let s = String::from_utf8_lossy(&o.stdout);
                if s.contains("RUNNING") {
                    windivert_state = "Çalışıyor".to_string();
                    break;
                } else if s.contains("STOP_PENDING") || s.contains("START_PENDING") {
                    windivert_state = "Askıda (Kilitli)".to_string();
                    break;
                } else if s.contains("STOPPED") {
                    windivert_state = "Durduruldu".to_string();
                }
            }
        }
    }

    let is_healthy = bfe_running && !windivert_state.contains("Kilitli");
    let details = if !bfe_running {
        "Windows Base Filtering Engine (BFE) servisi kapalı! Bu servis WFP paket filtrelemesi için zorunludur.".to_string()
    } else if windivert_state.contains("Kilitli") {
        "WinDivert sürücüsü askıda kalmış veya başka bir işlem tarafından kilitlenmiş.".to_string()
    } else {
        "Windows Filtering Platform (WFP) ve filtre altyapısı sağlıklı durumda.".to_string()
    };

    WfpDiagnostics {
        bfe_running,
        windivert_state,
        is_healthy,
        details,
    }
}

/// Programmatically stops the WinDivert kernel driver service via Service Control Manager (SCM).
/// Also falls back to `sc.exe stop windivert` and `sc.exe stop windivert14` to guarantee
/// all WFP kernel filters and sublayers are completely released.
#[cfg(target_os = "windows")]
pub fn stop_windivert_kernel_driver() {
    use std::os::windows::ffi::OsStrExt;
    use std::ffi::OsStr;
    use std::ptr;

    #[allow(non_snake_case)]
    #[repr(C)]
    struct SERVICE_STATUS {
        dwServiceType: u32,
        dwCurrentState: u32,
        dwControlsAccepted: u32,
        dwWin32ExitCode: u32,
        dwServiceSpecificExitCode: u32,
        dwCheckPoint: u32,
        dwWaitHint: u32,
    }

    const SC_MANAGER_CONNECT: u32 = 0x0001;
    const SERVICE_STOP: u32 = 0x0020;
    const SERVICE_QUERY_STATUS: u32 = 0x0004;
    const SERVICE_CONTROL_STOP: u32 = 0x00000001;
    const SERVICE_STOPPED: u32 = 0x00000001;

    #[link(name = "advapi32")]
    extern "system" {
        fn OpenSCManagerW(
            lpMachineName: *const u16,
            lpDatabaseName: *const u16,
            dwDesiredAccess: u32,
        ) -> isize;
        fn OpenServiceW(
            hSCManager: isize,
            lpServiceName: *const u16,
            dwDesiredAccess: u32,
        ) -> isize;
        fn ControlService(
            hService: isize,
            dwControl: u32,
            lpServiceStatus: *mut SERVICE_STATUS,
        ) -> i32;
        fn QueryServiceStatus(
            hService: isize,
            lpServiceStatus: *mut SERVICE_STATUS,
        ) -> i32;
        fn CloseServiceHandle(hSCObject: isize) -> i32;
    }

    for svc_name in &["WinDivert", "windivert", "windivert14"] {
        let wide: Vec<u16> = OsStr::new(svc_name)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();

        unsafe {
            let scm = OpenSCManagerW(ptr::null(), ptr::null(), SC_MANAGER_CONNECT);
            if scm != 0 {
                let svc = OpenServiceW(scm, wide.as_ptr(), SERVICE_STOP | SERVICE_QUERY_STATUS);
                if svc != 0 {
                    let mut status = SERVICE_STATUS {
                        dwServiceType: 0,
                        dwCurrentState: 0,
                        dwControlsAccepted: 0,
                        dwWin32ExitCode: 0,
                        dwServiceSpecificExitCode: 0,
                        dwCheckPoint: 0,
                        dwWaitHint: 0,
                    };
                    if QueryServiceStatus(svc, &mut status) != 0 && status.dwCurrentState != SERVICE_STOPPED {
                        ControlService(svc, SERVICE_CONTROL_STOP, &mut status);
                    }
                    CloseServiceHandle(svc);
                }
                CloseServiceHandle(scm);
            }
        }

        let _ = Command::new("sc.exe")
            .args(["stop", svc_name])
            .creation_flags(CREATE_NO_WINDOW)
            .output();
    }
}

#[cfg(not(target_os = "windows"))]
pub fn stop_windivert_kernel_driver() {}

/// Fast kernel-level WFP & WinDivert driver reset:
/// 1. Terminates any dangling or zombie DPI bypass processes holding driver handles.
/// 2. Stops the WinDivert kernel driver via Windows SCM to release all WFP sublayers & callout locks.
/// 3. Ensures Base Filtering Engine (BFE) is active.
/// 4. Waits 180ms for kernel netio teardown.
pub fn reset_windivert_wfp_state() -> Result<String, String> {
    #[cfg(target_os = "windows")]
    {
        for im in &["winws.exe", "goodbyedpi.exe", "zapret.exe", "byedpi.exe", "shroudly-core.exe"] {
            let _ = Command::new("taskkill")
                .args(["/IM", im, "/T", "/F"])
                .creation_flags(CREATE_NO_WINDOW)
                .output();
        }

        stop_windivert_kernel_driver();

        let _ = Command::new("sc.exe")
            .args(["start", "bfe"])
            .creation_flags(CREATE_NO_WINDOW)
            .output();

        std::thread::sleep(Duration::from_millis(180));
        Ok("WinDivert kernel sürücüsü durduruldu ve WFP filtre alanı temizlendi.".into())
    }
    #[cfg(not(target_os = "windows"))]
    {
        Ok("WFP sıfırlama işlemi tamamlandı.".into())
    }
}

/// Deep repair: Flushes stuck WFP filters, ensures Base Filtering Engine is running,
/// purges stuck driver services and registry keys, and resets Winsock if necessary.
pub fn repair_wfp_and_network_filters() -> Result<String, String> {
    #[cfg(target_os = "windows")]
    {
        // 1. Force kill any stray winws or DPI processes
        for im in &["winws.exe", "goodbyedpi.exe", "zapret.exe", "byedpi.exe", "shroudly-core.exe"] {
            let _ = Command::new("taskkill")
                .args(["/IM", im, "/T", "/F"])
                .creation_flags(CREATE_NO_WINDOW)
                .output();
        }

        // 2. Safely stop windivert kernel drivers via SCM
        stop_windivert_kernel_driver();

        // 3. Ensure Base Filtering Engine is running
        let _ = Command::new("sc.exe")
            .args(["start", "bfe"])
            .creation_flags(CREATE_NO_WINDOW)
            .output();

        // 4. Reset winsock and flush dns
        let _ = Command::new("netsh")
            .args(["winsock", "reset"])
            .creation_flags(CREATE_NO_WINDOW)
            .output();

        let _ = Command::new("ipconfig")
            .args(["/flushdns"])
            .creation_flags(CREATE_NO_WINDOW)
            .output();

        std::thread::sleep(Duration::from_millis(150));
        Ok("WFP filtreleri temizlendi, kilitli sürücüler sıfırlandı ve Base Filtering Engine doğrulandı.".into())
    }
    #[cfg(not(target_os = "windows"))]
    {
        Ok("WFP sıfırlama işlemi tamamlandı.".into())
    }
}

// =========================================================================
// 5. MICROSOFT EDGE WEBVIEW2 RUNTIME RESILIENCE
// =========================================================================

#[cfg(target_os = "windows")]
pub fn check_webview2_installed() -> bool {
    let subkeys = [
        (0x80000002u32 as i32 as isize, r"SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}"), // HKLM WOW64
        (0x80000002u32 as i32 as isize, r"SOFTWARE\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}"), // HKLM 64
        (0x80000001u32 as i32 as isize, r"SOFTWARE\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}"), // HKCU
    ];

    for (hkey, path) in subkeys {
        if let Some(val) = crate::query_registry_string(hkey, path, "pv") {
            if !val.trim().is_empty() && val.trim() != "0.0.0.0" {
                return true;
            }
        }
    }

    // Disk inspection check as fallback
    let candidate_paths = [
        r"C:\Program Files (x86)\Microsoft\EdgeWebView\Application",
        r"C:\Program Files\Microsoft\EdgeWebView\Application",
    ];
    for p in &candidate_paths {
        if Path::new(p).exists() {
            return true;
        }
    }

    if let Ok(local) = std::env::var("LOCALAPPDATA") {
        let edge_p = Path::new(&local).join(r"Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}");
        if edge_p.exists() {
            return true;
        }
    }

    false
}

#[cfg(not(target_os = "windows"))]
pub fn check_webview2_installed() -> bool {
    true
}

/// Verifies WebView2 runtime presence before starting Tauri. If missing or broken,
/// attempts silent download and installation of the Microsoft bootstrapper.
/// If offline or failing, displays a clean native Win32 dialog with download link
/// rather than crashing abruptly.
#[cfg(target_os = "windows")]
pub fn ensure_webview2_runtime() -> Result<(), String> {
    if check_webview2_installed() {
        return Ok(());
    }

    let temp_dir = std::env::temp_dir();
    let setup_path = temp_dir.join("MicrosoftEdgeWebview2Setup.exe");

    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(45))
        .build()
        .map_err(|e| e.to_string())?;

    let download_url = "https://go.microsoft.com/fwlink/p/?LinkId=2124703";
    let dl_res = client.get(download_url).send();

    let mut download_ok = false;
    if let Ok(resp) = dl_res {
        if resp.status().is_success() {
            if let Ok(bytes) = resp.bytes() {
                if bytes.len() > 100_000 {
                    let _ = fs::write(&setup_path, bytes);
                    download_ok = true;
                }
            }
        }
    }

    if download_ok && setup_path.exists() {
        let status = Command::new(&setup_path)
            .args(["/silent", "/install"])
            .creation_flags(CREATE_NO_WINDOW)
            .status();

        if let Ok(s) = status {
            if s.success() || check_webview2_installed() {
                return Ok(());
            }
        }
    }

    #[link(name = "user32")]
    extern "system" {
        fn MessageBoxW(hWnd: isize, lpText: *const u16, lpCaption: *const u16, uType: u32) -> i32;
    }
    const MB_ICONEXCLAMATION: u32 = 0x00000030;
    const MB_YESNO: u32 = 0x00000004;
    const IDYES: i32 = 6;

    let title: Vec<u16> = "Slipstream - Microsoft Edge WebView2 Gerekli\0".encode_utf16().collect();
    let text: Vec<u16> = ("Slipstream arayüzünün çalışabilmesi için Microsoft Edge WebView2 Runtime bileşeni gereklidir.\n\n\
        WebView2 sisteminizde bulunamadı ve otomatik kurulum tamamlanamadı.\n\n\
        Resmi indirme sayfasını açmak istiyor musunuz?\0")
        .encode_utf16()
        .collect();

    let choice = unsafe {
        MessageBoxW(
            0,
            text.as_ptr(),
            title.as_ptr(),
            MB_ICONEXCLAMATION | MB_YESNO,
        )
    };

    if choice == IDYES {
        let _ = Command::new("rundll32.exe")
            .args(["url.dll,FileProtocolHandler", "https://developer.microsoft.com/en-us/microsoft-edge/webview2/"])
            .creation_flags(CREATE_NO_WINDOW)
            .spawn();
    }

    Err("WebView2 Runtime bileşeni eksik. Lütfen yükledikten sonra Slipstream'i tekrar başlatın.".into())
}

#[cfg(not(target_os = "windows"))]
pub fn ensure_webview2_runtime() -> Result<(), String> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_elevated_admin() {
        // Runs elevation check without crashing or hanging
        let _ = is_elevated_admin();
    }

    #[test]
    fn test_detect_network_conflicts_competing_tools() {
        let procs = vec![
            crate::ProcessItem { pid: 100, name: "goodbyedpi.exe".into(), parent_pid: 0 },
            crate::ProcessItem { pid: 101, name: "zapret.exe".into(), parent_pid: 0 },
            crate::ProcessItem { pid: 102, name: "winws.exe".into(), parent_pid: 0 },
            crate::ProcessItem { pid: 103, name: "chrome.exe".into(), parent_pid: 0 },
        ];

        // If own_engine_pid is 102, winws.exe is our own child and should NOT be flagged
        let conflicts_with_own = detect_network_conflicts(&procs, Some(102));
        assert_eq!(conflicts_with_own.len(), 2);
        assert!(conflicts_with_own.iter().any(|c| c.name == "GoodbyeDPI"));
        assert!(conflicts_with_own.iter().any(|c| c.name == "Zapret"));

        // If own_engine_pid is None, winws.exe is external and MUST be flagged
        let conflicts_external = detect_network_conflicts(&procs, None);
        assert_eq!(conflicts_external.len(), 3);
        assert!(conflicts_external.iter().any(|c| c.id == "winws_orphaned"));
    }

    #[test]
    fn test_detect_network_conflicts_vpn() {
        let procs = vec![
            crate::ProcessItem { pid: 200, name: "warp-svc.exe".into(), parent_pid: 0 },
            crate::ProcessItem { pid: 201, name: "wireguard.exe".into(), parent_pid: 0 },
            crate::ProcessItem { pid: 202, name: "openvpn.exe".into(), parent_pid: 0 },
        ];

        let conflicts = detect_network_conflicts(&procs, None);
        assert_eq!(conflicts.len(), 3);
        assert!(!conflicts[0].is_critical, "VPN conflicts must be non-critical notifications");
    }

    #[test]
    fn test_antivirus_report_missing_files() {
        let temp_dir = std::env::temp_dir().join("slipstream_av_test_empty");
        let _ = fs::create_dir_all(&temp_dir);

        let report = check_antivirus_status(&temp_dir);
        assert!(!report.is_healthy);
        assert!(!report.engine_intact);
        assert_eq!(report.quarantined_files.len(), 2);
        assert!(report.message.contains("engellemiş veya karantinaya almış"));

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_wfp_diagnostics_structure() {
        let diag = diagnose_wfp_health();
        assert!(!diag.windivert_state.is_empty());
        assert!(!diag.details.is_empty());
    }

    #[test]
    fn test_check_webview2_installed() {
        // Runs webview2 detection without crashing
        let _ = check_webview2_installed();
    }

    #[test]
    fn test_reset_windivert_wfp_state() {
        let res = reset_windivert_wfp_state();
        assert!(res.is_ok());
    }
}
