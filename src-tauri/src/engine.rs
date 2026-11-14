//! Owns the winws.exe child process and Discord updater relay.
//! Zero proxy overhead: traffic is handled at the kernel packet layer via WinDivert.
//! System proxy and network adapter DNS are NEVER modified.

use crate::{dns, isp, profiles, proxy};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    os::windows::process::CommandExt,
    path::{Path, PathBuf},
    process::{Child, Command},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
    time::Duration,
};

fn default_true() -> bool {
    true
}

fn default_doh_provider() -> String {
    "cloudflare".into()
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct CustomPlatform {
    pub id: String,
    pub name: String,
    pub category: String, // "app" or "game"
    pub icon: String,
    pub domains: Vec<String>,
    #[serde(default)]
    pub exe_name: Option<String>,
    #[serde(default)]
    pub exe_path: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Config {
    pub isp: String,
    pub strategy: Option<String>,
    pub enabled_apps: Vec<String>,
    #[serde(default)]
    pub custom_platforms: Vec<CustomPlatform>,
    pub custom_domains: Vec<String>,
    pub custom_exes: Vec<String>,
    #[serde(default = "default_true")]
    pub manage_dns: bool,

    // === Sistem ve Başlangıç Ayarları ===
    #[serde(default = "default_true")]
    pub auto_start_pc: bool,
    #[serde(default = "default_true")]
    pub start_minimized: bool,
    #[serde(default = "default_true")]
    pub auto_connect: bool,
    #[serde(default = "default_true")]
    pub minimize_to_tray: bool,
    #[serde(default = "default_true")]
    pub hide_on_blur: bool,
    #[serde(default)]
    pub sound_enabled: bool,

    // === Gelişmiş Ağ & Performans Ayarları ===
    #[serde(default = "default_doh_provider")]
    pub doh_provider: String,
    #[serde(default)]
    pub gaming_mode: bool,
    #[serde(default)]
    pub game_priority_boost: bool,
    #[serde(default = "default_true")]
    pub auto_healing: bool,
    #[serde(default = "default_true")]
    pub auto_update: bool,

    // === Canlı Oyun & Uygulama Algılayıcı ===
    #[serde(default)]
    pub process_detection_mode: bool,

    // === Geri Bildirim & Topluluk İletişimi ===
    #[serde(default)]
    pub feedback_webhook: String,

    // === Yapılandırma ve Varsayılan Sürümü (Tek seferlik geçişler için) ===
    #[serde(default)]
    pub app_defaults_version: u32,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            isp: "turk-telekom".into(),
            strategy: Some("fake-split-md5sig".into()),
            enabled_apps: vec!["discord".into(), "roblox".into()],
            custom_platforms: vec![],
            custom_domains: vec![],
            custom_exes: vec![],
            manage_dns: true,
            auto_start_pc: true,
            start_minimized: true,
            auto_connect: true,
            minimize_to_tray: true,
            hide_on_blur: true,
            sound_enabled: false,
            doh_provider: "cloudflare".into(),
            gaming_mode: false,
            game_priority_boost: false,
            auto_healing: true,
            auto_update: true,
            process_detection_mode: false,
            feedback_webhook: String::new(),
            app_defaults_version: 1,
        }
    }
}

impl Config {
    pub fn load(dir: &PathBuf) -> Self {
        let config_path = dir.join("config.json");
        if !config_path.exists() {
            // Fresh installation: initialize default configuration and register Windows autostart
            let cfg = Self::default();
            let _ = fs::create_dir_all(dir);
            let _ = fs::write(
                &config_path,
                serde_json::to_string_pretty(&cfg).unwrap_or_default(),
            );
            if cfg.auto_start_pc {
                let _ = set_windows_autostart(true, cfg.start_minimized);
            }
            return cfg;
        }

        // Existing configuration: preserve user settings strictly without resetting to defaults
        let mut cfg: Self = fs::read_to_string(&config_path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();

        // One-time migration for existing users:
        // Set default enabled apps to strictly ["discord", "roblox"] + preserve any custom platforms
        if cfg.app_defaults_version < 1 {
            let custom_ids: Vec<String> = cfg.custom_platforms.iter().map(|p| p.id.clone()).collect();
            let mut new_enabled = vec!["discord".to_string(), "roblox".to_string()];
            for cid in custom_ids {
                if cfg.enabled_apps.contains(&cid) && !new_enabled.contains(&cid) {
                    new_enabled.push(cid);
                }
            }
            cfg.enabled_apps = new_enabled;
            cfg.app_defaults_version = 1;
            let _ = fs::write(
                &config_path,
                serde_json::to_string_pretty(&cfg).unwrap_or_default(),
            );
        }

        cfg
    }

    pub fn save(&self, dir: &PathBuf) {
        let _ = fs::create_dir_all(dir);
        let _ = fs::write(
            dir.join("config.json"),
            serde_json::to_string_pretty(self).unwrap_or_default(),
        );
        // Synchronize Windows startup registry setting
        let _ = set_windows_autostart(self.auto_start_pc, self.start_minimized);
    }
}

/// Returns the canonical path to Slipstream executable.
/// Prioritizes official installed locations and avoids test runners or ephemeral build targets.
#[allow(dead_code)]
pub fn get_canonical_app_exe() -> String {
    // 1. Check standard %LOCALAPPDATA%\Slipstream\slipstream.exe
    if let Ok(local) = std::env::var("LOCALAPPDATA") {
        let installed_exe = std::path::Path::new(&local).join("Slipstream").join("slipstream.exe");
        if installed_exe.exists() {
            return installed_exe.to_string_lossy().to_string();
        }
        let installed_exe_prog = std::path::Path::new(&local).join("Programs").join("Slipstream").join("slipstream.exe");
        if installed_exe_prog.exists() {
            return installed_exe_prog.to_string_lossy().to_string();
        }
    }

    // 2. Check Windows Registry Uninstall InstallLocation
    let uninst_query = Command::new("reg.exe")
        .args([
            "query",
            r"HKCU\Software\Microsoft\Windows\CurrentVersion\Uninstall\Slipstream",
            "/v",
            "InstallLocation",
        ])
        .creation_flags(0x0800_0000)
        .output();
    if let Ok(out) = uninst_query {
        let text = String::from_utf8_lossy(&out.stdout);
        let path_str = extract_exe_path_from_cmd(&text);
        if !path_str.is_empty() {
            let p = std::path::Path::new(&path_str);
            let candidate = if p.is_dir() {
                p.join("slipstream.exe")
            } else {
                p.to_path_buf()
            };
            if candidate.exists() {
                return candidate.to_string_lossy().to_string();
            }
        }
    }

    // 3. Check ProgramFiles
    if let Ok(pf) = std::env::var("ProgramFiles") {
        let pf_exe = std::path::Path::new(&pf).join("Slipstream").join("slipstream.exe");
        if pf_exe.exists() {
            return pf_exe.to_string_lossy().to_string();
        }
    }

    // 4. Check current_exe(), but filter out test runners, debug deps, target folders
    if let Ok(exe) = std::env::current_exe() {
        let path_str = exe.to_string_lossy().to_string();
        let lower = path_str.to_lowercase();
        // Never allow cargo test runners or target/deps binaries to become the canonical startup executable
        if !lower.contains(r"\target\debug")
            && !lower.contains(r"\target\release\deps")
            && !lower.contains(r"\deps\")
            && lower.ends_with("slipstream.exe")
            && exe.exists()
        {
            return path_str;
        }
    }

    // 5. Default fallback to standard local appdata installation path
    if let Ok(local) = std::env::var("LOCALAPPDATA") {
        return std::path::Path::new(&local)
            .join("Slipstream")
            .join("slipstream.exe")
            .to_string_lossy()
            .to_string();
    }

    "slipstream.exe".to_string()
}

/// Helper function to parse executable path out of a Windows command line string.
pub fn extract_exe_path_from_cmd(cmd: &str) -> String {
    let mut text = cmd.trim();
    if let Some((_, after)) = text.split_once("REG_SZ") {
        text = after.trim();
    } else if let Some((_, after)) = text.split_once("Task To Run:") {
        text = after.trim();
    }
    if let Some(first_quote) = text.find('"') {
        if let Some(second_quote) = text[first_quote + 1..].find('"') {
            return text[first_quote + 1..first_quote + 1 + second_quote].to_string();
        }
    }
    text.split_whitespace().next().unwrap_or(text).to_string()
}

#[cfg(test)]
pub fn set_windows_autostart(_enable: bool, _start_minimized: bool) -> Result<(), String> {
    Ok(())
}

/// Registers or unregisters Slipstream in Windows Task Scheduler and HKCU Run key for boot startup.
/// High-privilege Task Scheduler (/RL HIGHEST /SC ONLOGON) is used as the primary startup mechanism
/// because applications requiring administrator privileges (requireAdministrator manifest) are silently
/// suppressed and blocked from starting at user logon by Windows Explorer if only registered in HKCU Run.
#[cfg(not(test))]
pub fn set_windows_autostart(enable: bool, start_minimized: bool) -> Result<(), String> {
    // Purge legacy conflicting keys first
    for legacy_key in &["Slipstream-Portable", "slipstream", "SlipstreamDPI"] {
        let _ = Command::new("reg.exe")
            .args([
                "delete",
                r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run",
                "/v",
                legacy_key,
                "/f",
            ])
            .creation_flags(0x0800_0000)
            .output();
    }

    if enable {
        let exe_str = get_canonical_app_exe();
        let cmd_val = if start_minimized {
            format!("\"{exe_str}\" --minimized")
        } else {
            format!("\"{exe_str}\" --autostart")
        };

        // 1. Task Scheduler registration with HIGHEST privileges (UAC bypass for autostart)
        let sch_status = Command::new("schtasks.exe")
            .args([
                "/create",
                "/tn",
                "Slipstream",
                "/tr",
                &cmd_val,
                "/sc",
                "onlogon",
                "/rl",
                "highest",
                "/f",
            ])
            .creation_flags(0x0800_0000)
            .status();

        let sch_ok = sch_status.map(|s| s.success()).unwrap_or(false);

        // 2. Also register HKCU Run key for Windows Task Manager Startup tab visibility
        let reg_status = Command::new("reg.exe")
            .args([
                "add",
                r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run",
                "/v",
                "Slipstream",
                "/t",
                "REG_SZ",
                "/d",
                &cmd_val,
                "/f",
            ])
            .creation_flags(0x0800_0000)
            .status()
            .map_err(|e| e.to_string())?;

        if !sch_ok && !reg_status.success() {
            return Err("Windows başlangıç kaydı oluşturulamadı.".into());
        }
    } else {
        // 1. Delete Task Scheduler task
        let _ = Command::new("schtasks.exe")
            .args(["/delete", "/tn", "Slipstream", "/f"])
            .creation_flags(0x0800_0000)
            .output();

        // 2. Delete HKCU Run entry
        let _ = Command::new("reg.exe")
            .args([
                "delete",
                r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run",
                "/v",
                "Slipstream",
                "/f",
            ])
            .creation_flags(0x0800_0000)
            .output();
    }
    Ok(())
}

#[cfg(test)]
pub fn repair_and_sync_autostart(_auto_start_pc: bool, _start_minimized: bool) {}

/// Inspects Windows startup configuration, cleans up legacy/conflicting entries,
/// and heals any stale or broken paths that point to deleted or moved executables.
#[cfg(not(test))]
pub fn repair_and_sync_autostart(auto_start_pc: bool, start_minimized: bool) {
    // 1. Purge legacy conflicting registry keys
    for legacy_key in &["Slipstream-Portable", "slipstream", "SlipstreamDPI"] {
        let _ = Command::new("reg.exe")
            .args([
                "delete",
                r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run",
                "/v",
                legacy_key,
                "/f",
            ])
            .creation_flags(0x0800_0000)
            .output();
    }

    // 2. Clean up any stale shortcuts in the Windows Startup folder
    if let Ok(appdata) = std::env::var("APPDATA") {
        let startup_dir = std::path::Path::new(&appdata)
            .join("Microsoft")
            .join("Windows")
            .join("Start Menu")
            .join("Programs")
            .join("Startup");

        for name in &["Slipstream.lnk", "Slipstream-Portable.lnk", "slipstream.lnk"] {
            let lnk = startup_dir.join(name);
            if lnk.exists() {
                let _ = std::fs::remove_file(lnk);
            }
        }
    }

    let canonical_exe = get_canonical_app_exe();

    if auto_start_pc {
        // Check Task Scheduler task
        let sch_query = Command::new("schtasks.exe")
            .args(["/query", "/tn", "Slipstream", "/fo", "list", "/v"])
            .creation_flags(0x0800_0000)
            .output();

        let sch_needs_repair = match sch_query {
            Ok(ref o) if o.status.success() => {
                let out = String::from_utf8_lossy(&o.stdout);
                !out.to_lowercase().contains(&canonical_exe.to_lowercase())
            }
            _ => true,
        };

        // Check HKCU Run entry
        let query_out = Command::new("reg.exe")
            .args([
                "query",
                r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run",
                "/v",
                "Slipstream",
            ])
            .creation_flags(0x0800_0000)
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
            .unwrap_or_default();

        let reg_needs_repair = if query_out.contains("Slipstream") {
            let is_target_valid = if let Some(line) = query_out.lines().find(|l| l.contains("Slipstream")) {
                let path_str = extract_exe_path_from_cmd(line);
                std::path::Path::new(&path_str).exists()
            } else {
                false
            };
            !is_target_valid || !query_out.to_lowercase().contains(&canonical_exe.to_lowercase())
        } else {
            true
        };

        if sch_needs_repair || reg_needs_repair {
            let _ = set_windows_autostart(true, start_minimized);
        }
    } else {
        let _ = set_windows_autostart(false, start_minimized);
    }

    // 3. Ensure Windows Search ("Slipstream" and "ss") and Start Menu shortcuts are always registered
    ensure_windows_search_integration();
}

/// Guarantees that Slipstream is registered in Windows Search and Start Menu,
/// responding to both "Slipstream" and the short alias "ss".
pub fn ensure_windows_search_integration() {
    let canonical_exe = get_canonical_app_exe();
    if canonical_exe.is_empty() {
        return;
    }
    let exe_path = Path::new(&canonical_exe);
    if !exe_path.exists() {
        return;
    }

    let exe_dir = exe_path
        .parent()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default();

    // 1. Gather all Start Menu directories (User + All Users/System)
    let mut menu_dirs = Vec::new();
    if let Ok(appdata) = std::env::var("APPDATA") {
        menu_dirs.push(Path::new(&appdata).join("Microsoft").join("Windows").join("Start Menu").join("Programs"));
    }
    if let Ok(programdata) = std::env::var("PROGRAMDATA") {
        menu_dirs.push(Path::new(&programdata).join("Microsoft").join("Windows").join("Start Menu").join("Programs"));
    }

    // 2. Build and run fast VBScript to create/update shortcuts
    let mut targets_to_create = Vec::new();
    for dir in &menu_dirs {
        if dir.exists() {
            for name in &["Slipstream.lnk", "ss.lnk", "Slipstream (ss).lnk"] {
                targets_to_create.push(dir.join(name));
            }
        }
    }

    if !targets_to_create.is_empty() {
        let vbs_lines: Vec<String> = targets_to_create
            .iter()
            .map(|lnk| {
                format!(
                    "Set sc = sh.CreateShortcut(\"{path}\")\n\
                     sc.TargetPath = \"{target}\"\n\
                     sc.WorkingDirectory = \"{workdir}\"\n\
                     sc.Description = \"Slipstream (ss) - Ultra-Fast Zero-Latency DPI Bypass\"\n\
                     sc.IconLocation = \"{target},0\"\n\
                     sc.Save",
                    path = lnk.to_string_lossy().replace('"', "\"\""),
                    target = canonical_exe.replace('"', "\"\""),
                    workdir = exe_dir.replace('"', "\"\""),
                )
            })
            .collect();

        let vbs_content = format!(
            "On Error Resume Next\nSet sh = CreateObject(\"WScript.Shell\")\n{}\n",
            vbs_lines.join("\n")
        );

        let temp_vbs = std::env::temp_dir().join("slipstream_shortcuts.vbs");
        if let Ok(()) = std::fs::write(&temp_vbs, vbs_content) {
            let _ = Command::new("cscript.exe")
                .args(["//nologo", &temp_vbs.to_string_lossy()])
                .creation_flags(0x0800_0000)
                .output();
            let _ = std::fs::remove_file(temp_vbs);
        }
    }

    // 3. Create ss.cmd wrapper script in app directory for instant cmd/run launching
    if !exe_dir.is_empty() {
        let ss_cmd = Path::new(&exe_dir).join("ss.cmd");
        if !ss_cmd.exists() {
            let _ = std::fs::write(&ss_cmd, "@start \"\" \"%~dp0slipstream.exe\" %*\r\n");
        }
    }

    // 4. Windows App Paths Registry (both HKCU and HKLM so "ss" or "slipstream" in Run/Search works everywhere)
    for root in &["HKCU", "HKLM"] {
        for alias in &["slipstream.exe", "ss.exe", "ss"] {
            let key = format!(r"{root}\Software\Microsoft\Windows\CurrentVersion\App Paths\{alias}");
            let _ = Command::new("reg.exe")
                .args(["add", &key, "/ve", "/d", &canonical_exe, "/f"])
                .creation_flags(0x0800_0000)
                .output();
            if !exe_dir.is_empty() {
                let _ = Command::new("reg.exe")
                    .args(["add", &key, "/v", "Path", "/d", &exe_dir, "/f"])
                    .creation_flags(0x0800_0000)
                    .output();
            }
        }
    }
}

#[cfg(test)]
pub fn is_windows_autostart() -> bool {
    true
}

/// Checks if Windows autostart is active in Task Scheduler or HKCU Run and points to a valid existing executable.
#[cfg(not(test))]
pub fn is_windows_autostart() -> bool {
    // 1. Task Scheduler check
    let sch_out = Command::new("schtasks.exe")
        .args(["/query", "/tn", "Slipstream", "/fo", "list", "/v"])
        .creation_flags(0x0800_0000)
        .output();

    if let Ok(o) = sch_out {
        if o.status.success() {
            let text = String::from_utf8_lossy(&o.stdout);
            if let Some(line) = text.lines().find(|l| l.to_lowercase().contains("slipstream.exe")) {
                let path_str = extract_exe_path_from_cmd(line);
                if std::path::Path::new(&path_str).exists() {
                    return true;
                }
            }
            return true;
        }
    }

    // 2. Fallback HKCU Run check
    let out = Command::new("reg.exe")
        .args([
            "query",
            r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run",
            "/v",
            "Slipstream",
        ])
        .creation_flags(0x0800_0000)
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
        .unwrap_or_default();

    if out.contains("Slipstream") {
        if let Some(line) = out.lines().find(|l| l.contains("Slipstream")) {
            let path_str = extract_exe_path_from_cmd(line);
            std::path::Path::new(&path_str).exists()
        } else {
            false
        }
    } else {
        false
    }
}

/// Applies or reverts Windows TCP/IP registry and network stack tweaks for low latency gaming.
///
/// When enabled:
/// - Disables Nagle's algorithm (TcpAckFrequency = 1, TCPNoDelay = 1 on network interfaces)
/// - Disables Windows Multimedia Network Throttling (NetworkThrottlingIndex = 0xFFFFFFFF)
/// - Sets SystemResponsiveness = 0 (100% responsiveness for gaming and foreground processes)
/// - Configures TCP auto-tuning and Receive Side Scaling (RSS) for multi-core packet processing
///
/// When disabled:
/// - Restores Windows defaults cleanly: NetworkThrottlingIndex = 10, SystemResponsiveness = 20,
///   removes interface registry overrides.
pub fn apply_gaming_network_tweaks(enable: bool) {
    #[cfg(target_os = "windows")]
    {
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;

        let sys_profile_key = r"HKLM\SOFTWARE\Microsoft\Windows NT\CurrentVersion\Multimedia\SystemProfile";
        let interfaces_key = r"HKLM\SYSTEM\CurrentControlSet\Services\Tcpip\Parameters\Interfaces";

        if enable {
            // 1) Multimedia Network Throttling & Responsiveness
            let _ = Command::new("reg.exe")
                .args(["add", sys_profile_key, "/v", "NetworkThrottlingIndex", "/t", "REG_DWORD", "/d", "0xFFFFFFFF", "/f"])
                .creation_flags(CREATE_NO_WINDOW)
                .output();

            let _ = Command::new("reg.exe")
                .args(["add", sys_profile_key, "/v", "SystemResponsiveness", "/t", "REG_DWORD", "/d", "0", "/f"])
                .creation_flags(CREATE_NO_WINDOW)
                .output();

            // 2) Query network interface subkeys and apply TcpAckFrequency = 1 and TCPNoDelay = 1
            if let Ok(out) = Command::new("reg.exe")
                .args(["query", interfaces_key])
                .creation_flags(CREATE_NO_WINDOW)
                .output()
            {
                let text = String::from_utf8_lossy(&out.stdout);
                for line in text.lines() {
                    let trimmed = line.trim();
                    if trimmed.starts_with("HKEY_LOCAL_MACHINE\\") {
                        let subkey = trimmed.trim_start_matches("HKEY_LOCAL_MACHINE\\");
                        let full_sub = format!("HKLM\\{}", subkey);
                        let _ = Command::new("reg.exe")
                            .args(["add", &full_sub, "/v", "TcpAckFrequency", "/t", "REG_DWORD", "/d", "1", "/f"])
                            .creation_flags(CREATE_NO_WINDOW)
                            .output();
                        let _ = Command::new("reg.exe")
                            .args(["add", &full_sub, "/v", "TCPNoDelay", "/t", "REG_DWORD", "/d", "1", "/f"])
                            .creation_flags(CREATE_NO_WINDOW)
                            .output();
                    }
                }
            }

            // 3) Global TCP autotuning & RSS
            let _ = Command::new("netsh")
                .args(["int", "tcp", "set", "global", "autotuninglevel=normal"])
                .creation_flags(CREATE_NO_WINDOW)
                .output();
            let _ = Command::new("netsh")
                .args(["int", "tcp", "set", "global", "rss=enabled"])
                .creation_flags(CREATE_NO_WINDOW)
                .output();
        } else {
            // Restore clean Windows defaults
            let _ = Command::new("reg.exe")
                .args(["add", sys_profile_key, "/v", "NetworkThrottlingIndex", "/t", "REG_DWORD", "/d", "10", "/f"])
                .creation_flags(CREATE_NO_WINDOW)
                .output();

            let _ = Command::new("reg.exe")
                .args(["add", sys_profile_key, "/v", "SystemResponsiveness", "/t", "REG_DWORD", "/d", "20", "/f"])
                .creation_flags(CREATE_NO_WINDOW)
                .output();

            if let Ok(out) = Command::new("reg.exe")
                .args(["query", interfaces_key])
                .creation_flags(CREATE_NO_WINDOW)
                .output()
            {
                let text = String::from_utf8_lossy(&out.stdout);
                for line in text.lines() {
                    let trimmed = line.trim();
                    if trimmed.starts_with("HKEY_LOCAL_MACHINE\\") {
                        let subkey = trimmed.trim_start_matches("HKEY_LOCAL_MACHINE\\");
                        let full_sub = format!("HKLM\\{}", subkey);
                        let _ = Command::new("reg.exe")
                            .args(["delete", &full_sub, "/v", "TcpAckFrequency", "/f"])
                            .creation_flags(CREATE_NO_WINDOW)
                            .output();
                        let _ = Command::new("reg.exe")
                            .args(["delete", &full_sub, "/v", "TCPNoDelay", "/f"])
                            .creation_flags(CREATE_NO_WINDOW)
                            .output();
                    }
                }
            }
        }
    }
}

pub const WINWS_EXE_BYTES: &[u8] = include_bytes!("../../engine/winws.exe");
pub const CYGWIN_DLL_BYTES: &[u8] = include_bytes!("../../engine/cygwin1.dll");
pub const WINDIVERT_DLL_BYTES: &[u8] = include_bytes!("../../engine/WinDivert.dll");
pub const WINDIVERT_SYS_BYTES: &[u8] = include_bytes!("../../engine/WinDivert64.sys");
pub const LICENSE_BYTES: &[u8] = include_bytes!("../../engine/WinDivert-LICENSE.txt");

/// Ensures engine driver and winws.exe files exist in target directory, extracting embedded copies if needed.
pub fn ensure_engine_files(target_dir: &std::path::Path) -> Result<PathBuf, String> {
    let winws_path = target_dir.join("winws.exe");
    let cygwin_path = target_dir.join("cygwin1.dll");
    let dll_path = target_dir.join("WinDivert.dll");
    let sys_path = target_dir.join("WinDivert64.sys");
    let lic_path = target_dir.join("WinDivert-LICENSE.txt");

    fs::create_dir_all(target_dir).map_err(|e| e.to_string())?;

    let write_if_needed = |path: &PathBuf, data: &[u8]| -> Result<(), String> {
        let need_write = match fs::metadata(path) {
            Ok(meta) if meta.len() == data.len() as u64 => match fs::read(path) {
                Ok(existing) => existing != data,
                Err(_) => true,
            },
            _ => true,
        };
        if need_write {
            match fs::write(path, data) {
                Ok(_) => Ok(()),
                Err(e) => {
                    let raw_err = e.raw_os_error();
                    if e.kind() == std::io::ErrorKind::PermissionDenied || raw_err == Some(5) || raw_err == Some(225) {
                        Err(format!(
                            "DPI motor dosyası yazılamadı (Antivirüs / Yetki Engeli): {}. Windows Defender veya güvenlik yazılımınız bu dosyayı engellemiş olabilir. Lütfen 'Windows Defender Dışlama Ekle' seçeneğini kullanın.",
                            path.display()
                        ))
                    } else {
                        Err(format!("Dosya yazılamadı ({}): {e}", path.display()))
                    }
                }
            }
        } else {
            Ok(())
        }
    };

    write_if_needed(&winws_path, WINWS_EXE_BYTES)?;
    write_if_needed(&cygwin_path, CYGWIN_DLL_BYTES)?;
    write_if_needed(&dll_path, WINDIVERT_DLL_BYTES)?;
    write_if_needed(&sys_path, WINDIVERT_SYS_BYTES)?;
    write_if_needed(&lic_path, LICENSE_BYTES)?;

    Ok(winws_path)
}

#[repr(C)]
struct IO_COUNTERS {
    read_operation_count: u64,
    write_operation_count: u64,
    other_operation_count: u64,
    read_transfer_count: u64,
    write_transfer_count: u64,
    other_transfer_count: u64,
}

#[repr(C)]
struct JOBOBJECT_BASIC_LIMIT_INFORMATION {
    per_process_user_time_limit: i64,
    per_job_user_time_limit: i64,
    limit_flags: u32,
    minimum_working_set_size: usize,
    maximum_working_set_size: usize,
    active_process_limit: u32,
    affinity: usize,
    priority_class: u32,
    scheduling_class: u32,
}

#[repr(C)]
struct JOBOBJECT_EXTENDED_LIMIT_INFORMATION {
    basic_limit_information: JOBOBJECT_BASIC_LIMIT_INFORMATION,
    io_info: IO_COUNTERS,
    process_memory_limit: usize,
    job_memory_limit: usize,
    peak_process_memory_limit: usize,
    peak_job_memory_limit: usize,
}

const JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE: u32 = 0x0000_2000;
const JOB_OBJECT_EXTENDED_LIMIT_INFORMATION: u32 = 9;

#[link(name = "kernel32")]
extern "system" {
    fn CreateJobObjectW(lp_job_attributes: *mut std::ffi::c_void, lp_name: *const u16) -> isize;
    fn SetInformationJobObject(
        h_job: isize,
        job_object_information_class: u32,
        lp_job_information: *const std::ffi::c_void,
        cb_job_information_length: u32,
    ) -> i32;
    #[allow(dead_code)]
    fn AssignProcessToJobObject(h_job: isize, h_process: isize) -> i32;
    fn CloseHandle(h_object: isize) -> i32;
}

/// Creates a Windows Job Object configured with JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE.
/// Any child process assigned to this job object will be immediately and unconditionally
/// terminated by the Windows NT kernel if the parent process (Slipstream) terminates, crashes,
/// or is closed for any reason.
pub fn create_kill_on_close_job() -> Option<isize> {
    unsafe {
        let job = CreateJobObjectW(std::ptr::null_mut(), std::ptr::null());
        if job == 0 || job == -1 {
            return None;
        }

        let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
        info.basic_limit_information.limit_flags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;

        let ret = SetInformationJobObject(
            job,
            JOB_OBJECT_EXTENDED_LIMIT_INFORMATION,
            &info as *const _ as *const std::ffi::c_void,
            std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
        );

        if ret == 0 {
            CloseHandle(job);
            return None;
        }

        Some(job)
    }
}

#[allow(dead_code)]
pub fn assign_process_to_job(job: isize, process_handle: isize) -> bool {
    if job == 0 || job == -1 || process_handle == 0 || process_handle == -1 {
        return false;
    }
    unsafe { AssignProcessToJobObject(job, process_handle) != 0 }
}

pub struct Engine {
    pub dir: PathBuf,
    pub bin: PathBuf,
    pub native_worker: Option<crate::native_engine::NativeWorkerHandle>,
    child: Option<Child>,
    relay_stop: Option<Arc<AtomicBool>>,
    job_handle: Option<isize>,
}

#[allow(dead_code)]
fn push(a: &mut Vec<String>, xs: &[&str]) {
    a.extend(xs.iter().map(|s| s.to_string()));
}

/// Builds the winws argument list. Traffic matching TCP 80/443 and Discord voice
/// UDP is desynced transparently at the kernel packet level via WinDivert.
/// Hostlist filtering is STRICTLY enforced so unblocked sites (Steam, banking, local networks)
/// pass directly through the network stack with ZERO packet modification and ZERO latency!
#[allow(dead_code)]
fn build_args(cfg: &Config, hosts: &Path) -> Vec<String> {
    let dir = hosts.parent();
    let strat_id = match cfg.strategy.as_deref() {
        Some(s) => s.to_string(),
        None => {
            let default_order = isp::order_with_updates(&cfg.isp, dir);
            default_order.into_iter().next().unwrap_or_else(|| "fake-split-md5sig".to_string())
        }
    };
    let tls_flags = isp::resolve_strategy_tls(&strat_id, dir);

    let mut a = vec![];
    push(&mut a, &["--wf-tcp=80,443"]);
    // Capture only UDP 443 for QUIC desync. Discord voice UDP (ports 19294-19344 & 50000-65535)
    // passes directly through kernel with zero WinDivert overhead, zero STUN corruption, and 30-40ms native ping.
    push(&mut a, &["--wf-udp=443"]);

    let hostlist_arg = format!("--hostlist={}", hosts.display());

    // QUIC (HTTP/3) - strictly targeted by active hostlist
    push(
        &mut a,
        &[
            "--filter-udp=443",
            &hostlist_arg,
            "--dpi-desync=fake",
            "--dpi-desync-repeats=1",
            "--new",
        ],
    );

    // Plain HTTP - strictly targeted by active hostlist
    push(
        &mut a,
        &[
            "--filter-tcp=80",
            &hostlist_arg,
            "--dpi-desync=fake,split2",
            "--dpi-desync-autottl=2",
            "--dpi-desync-fooling=md5sig",
            "--new",
        ],
    );

    // HTTPS: applies desync ONLY to domains matching the active hostlist (subdomains auto apply).
    // All non-listed sites (Steam, Epic, Riot, banking, local networks) pass through with ZERO overhead (0 ping)!
    push(
        &mut a,
        &[
            "--filter-tcp=443",
            &hostlist_arg,
        ],
    );
    let tls_refs: Vec<&str> = tls_flags.iter().map(|s| s.as_str()).collect();
    push(&mut a, &tls_refs);
    a
}

#[derive(Clone, Debug)]
pub struct EngineWorkerConfig {
    pub bin: PathBuf,
    pub args: Vec<String>,
    pub work_dir: PathBuf,
}

impl Engine {
    pub fn new(dir: PathBuf, bin: PathBuf) -> Self {
        Self {
            dir,
            bin,
            native_worker: None,
            child: None,
            relay_stop: None,
            job_handle: create_kill_on_close_job(),
        }
    }

    /// True while the engine is running.
    pub fn is_on(&self) -> bool {
        if let Some(ref c) = self.child {
            let pid = c.id();
            crate::get_running_processes().iter().any(|p| p.pid == pid)
        } else {
            self.native_worker.as_ref().map(|w| w.is_running()).unwrap_or(false)
        }
    }

    /// Returns the PID of the active engine, if running.
    #[allow(dead_code)]
    pub fn child_pid(&self) -> Option<u32> {
        self.child.as_ref().map(|c| c.id())
    }

    /// Returns current packet and desync statistics from the engine.
    #[allow(dead_code)]
    pub fn stats(&self) -> Option<crate::native_engine::EngineStatsSnapshot> {
        None
    }

    /// Ensures the Discord updater relay is running if enabled, or stopped if not.
    pub fn ensure_relay(&mut self, enabled: bool) {
        if enabled {
            if self.relay_stop.is_none() {
                let r_stop = Arc::new(AtomicBool::new(false));
                self.relay_stop = Some(r_stop.clone());
                tauri::async_runtime::spawn(proxy::run_discord_relay(r_stop, proxy::DISCORD_RELAY_PORT));
            }
        } else if let Some(s) = self.relay_stop.take() {
            s.store(true, Ordering::Relaxed);
        }
    }

    /// Starts (or re-applies) everything. On error, retries once and stops cleanly on fatal error.
    pub fn start(&mut self, cfg: &Config) -> Result<(), String> {
        let worker_cfg = Self::prepare_start(&self.dir, &self.bin, cfg)?;
        let mut r = self.start_worker(worker_cfg.clone());
        if let Err(ref err) = r {
            if err.contains("kilitli") || err.contains("10") || err.contains("1072") {
                let _ = crate::resilience::reset_windivert_wfp_state();
                thread::sleep(Duration::from_millis(300));
                r = self.start_worker(worker_cfg);
            }
        }
        if r.is_err() {
            self.stop();
            return r;
        }
        Self::apply_post_start(&self.dir, cfg);
        Ok(())
    }

    /// Prepares disk files, hostlist, proxy cleanup and resolves winws engine arguments.
    /// Runs OUTSIDE any Engine mutex so shell and file I/O operations never block other commands.
    pub fn prepare_start(
        dir: &Path,
        bin: &Path,
        cfg: &Config,
    ) -> Result<EngineWorkerConfig, String> {
        Self::kill_all_winws();
        let _ = crate::resilience::resolve_network_conflicts();

        let eng_dir = bin.parent().unwrap_or(dir);
        let winws_path = eng_dir.join("winws.exe");
        let cygwin_path = eng_dir.join("cygwin1.dll");
        let dll_path = eng_dir.join("WinDivert.dll");
        let sys_path = eng_dir.join("WinDivert64.sys");

        let files_incomplete = !winws_path.exists() || !cygwin_path.exists() || !dll_path.exists() || !sys_path.exists();
        if files_incomplete {
            ensure_engine_files(eng_dir)?;
            if !winws_path.exists() || !cygwin_path.exists() || !dll_path.exists() || !sys_path.exists() {
                return Err(
                    "winws.exe veya WinDivert sürücü dosyaları eksik veya güvenlik yazılımı/Windows Defender tarafından karantinaya alınmış. Lütfen 'Windows Defender Dışlama Ekle' seçeneğini kullanın.".into()
                );
            }
        }
        fs::create_dir_all(dir).map_err(|e| e.to_string())?;

        // 1) Ensure network adapters have clean DHCP and proxy settings are clean
        dns::recover(dir);
        proxy::clean_stale_proxy();

        // 2) Write hostlist
        let domains = profiles::collect_domains(cfg);
        let hosts = dir.join("hosts.txt");
        fs::write(&hosts, domains.join("\r\n")).map_err(|e| e.to_string())?;

        // 3) Build winws argument list
        let args = build_args(cfg, &hosts);

        Ok(EngineWorkerConfig {
            bin: winws_path,
            args,
            work_dir: eng_dir.to_path_buf(),
        })
    }

    /// Spawns the winws.exe child process with WinDivert kernel desync arguments.
    /// Incorporates autonomous kernel-level self-healing for exit code 10 (FWP_E_IN_USE)
    /// and WFP filter locks so the engine restarts cleanly without error popups.
    pub fn start_worker(
        &mut self,
        cfg: EngineWorkerConfig,
    ) -> Result<(), String> {
        let max_attempts = 3;
        let mut last_err = String::new();

        for attempt in 1..=max_attempts {
            if let Some(mut w) = self.native_worker.take() {
                w.stop();
            }
            if let Some(mut c) = self.child.take() {
                let _ = c.kill();
                let _ = c.wait();
            }

            Self::kill_all_winws();

            let mut cmd = Command::new(&cfg.bin);
            cmd.args(&cfg.args)
                .current_dir(&cfg.work_dir)
                .creation_flags(0x0800_0000); // CREATE_NO_WINDOW

            let child = match cmd.spawn() {
                Ok(c) => c,
                Err(e) => {
                    return Err(format!("winws.exe başlatılamadı ({}): {e}", cfg.bin.display()));
                }
            };

            // Assign to job object so child dies unconditionally if parent process exits
            if let Some(job) = self.job_handle {
                use std::os::windows::io::AsRawHandle;
                assign_process_to_job(job, child.as_raw_handle() as isize);
            }

            self.child = Some(child);

            // Verification check: give winws.exe 180ms to verify driver load and bind
            thread::sleep(Duration::from_millis(180));
            if let Some(ref mut c) = self.child {
                if let Ok(Some(status)) = c.try_wait() {
                    self.child = None;
                    let status_str = status.to_string();
                    let is_recoverable = status_str.contains("10")
                        || status_str.contains("1072")
                        || status_str.contains("7")
                        || status_str.contains("5");

                    last_err = format!("winws.exe başlatıldıktan hemen sonra kapandı ({status}). WinDivert sürücüsü yüklenememiş veya başka bir DPI aracı portu tutuyor olabilir.");

                    if is_recoverable && attempt < max_attempts {
                        // Autonomous kernel-level self-healing: purge driver locks & sublayers
                        let _ = crate::resilience::reset_windivert_wfp_state();
                        thread::sleep(Duration::from_millis(250));
                        continue;
                    }

                    return Err(last_err);
                }
            }

            // Successfully started and running stably!
            return Ok(());
        }

        Err(last_err)
    }

    /// Applies post-start network, DNS and firewall rules.
    /// Runs OUTSIDE any Engine mutex so shell commands never block UI threads.
    pub fn apply_post_start(dir: &Path, cfg: &Config) {
        if cfg.manage_dns {
            dns::apply_dns(cfg, dir);
        }
        dns::apply_bypass_hosts(cfg);
        proxy::patch_discord_settings(None);
        apply_gaming_network_tweaks(cfg.gaming_mode);
        apply_quic_firewall_rule(true);
    }

    /// Forcefully terminates any orphan winws.exe or conflicting DPI bypass processes to free WinDivert driver handles.
    pub fn kill_all_winws() {
        #[cfg(target_os = "windows")]
        {
            let procs = crate::get_running_process_names();
            let targets = [
                "winws.exe", "goodbyedpi.exe", "zapret.exe",
                "shroudly.exe", "shroudly-core.exe", "shroudly-core-adv.exe",
                "byedpi.exe", "ciadpi.exe", "spoofdpi.exe", "green-tunnel.exe"
            ];
            let mut found = false;
            for p in &procs {
                let p_lower = p.to_lowercase();
                if targets.iter().any(|t| p_lower == *t) {
                    found = true;
                    let _ = Command::new("taskkill")
                        .args(["/IM", p, "/T", "/F"])
                        .creation_flags(0x0800_0000)
                        .output();
                }
            }
            if found {
                // If any conflicting or zombie processes were terminated, stop the driver so handles unbind cleanly
                crate::resilience::stop_windivert_kernel_driver();
                std::thread::sleep(Duration::from_millis(100));
            }
        }
    }

    /// Forcefully terminates any orphan Slipstream processes (other than the current PID)
    /// to guarantee clean startup and prevent background zombie locks.
    pub fn kill_orphan_slipstream_instances() {
        #[cfg(target_os = "windows")]
        {
            let current_pid = std::process::id();
            let procs = crate::get_running_processes();
            let mut found = false;
            for p in procs {
                let name = p.name.to_lowercase();
                if (name == "slipstream.exe" || name.starts_with("slipstream")) && p.pid != current_pid {
                    found = true;
                    let _ = Command::new("taskkill")
                        .args(["/PID", &p.pid.to_string(), "/F"])
                        .creation_flags(0x0800_0000)
                        .output();
                }
            }
            if found {
                std::thread::sleep(Duration::from_millis(50));
            }
        }
    }

    /// Health watchdog: checks if native packet engine is still alive, restarts if needed.
    #[allow(dead_code)]
    pub fn check_health(&mut self, cfg: &Config) -> bool {
        if let Some(ref w) = self.native_worker {
            if !w.is_running() {
                self.native_worker = None;
                if cfg.auto_healing {
                    let _ = self.start(cfg);
                    return true;
                }
            }
        }
        if let Some(ref mut c) = self.child {
            if let Ok(Some(_)) = c.try_wait() {
                self.child = None;
                if cfg.auto_healing {
                    let _ = self.start(cfg);
                    return true;
                }
            }
        }
        false
    }

    /// Stops the native worker and Discord relay cleanly in memory (~2-5ms).
    pub fn stop_worker(&mut self) {
        if let Some(mut w) = self.native_worker.take() {
            w.stop();
        }
        if let Some(mut c) = self.child.take() {
            let _ = c.kill();
            let _ = c.wait();
        }
        self.ensure_relay(false);
    }

    /// Restores network configuration, DNS, proxy, gaming tweaks and stops driver.
    /// Runs OUTSIDE any Engine mutex so shell commands never block UI threads.
    pub fn cleanup_post_stop(dir: &Path) {
        Self::kill_all_winws();
        dns::clear_bypass_hosts();
        dns::restore_dns(dir);
        proxy::clean_stale_proxy();
        dns::recover(dir);
        apply_gaming_network_tweaks(false);
        apply_quic_firewall_rule(false);

        // Safely STOP the driver service via Windows SCM so kernel handles are released cleanly.
        crate::resilience::stop_windivert_kernel_driver();
    }

    /// Full stop: terminates native packet engine, stops relay, restores Discord settings, cleans hosts and proxy state.
    pub fn stop(&mut self) {
        self.stop_worker();
        Self::cleanup_post_stop(&self.dir);
    }
}

/// Manages outbound UDP port 443 block rule in Windows Firewall.
/// When active, Windows Firewall rejects outbound UDP 443 instantly at the socket layer (WSAEACCES).
/// This forces Chromium, Brave, and Discord to immediately fall back to TCP TLS (HTTP/2 / HTTP/1.1) in 0.0ms,
/// eliminating the 3-7 second QUIC handshake stalls caused by ISP packet drops without touching UDP voice or gaming ports.
pub fn apply_quic_firewall_rule(block: bool) {
    let _ = Command::new("netsh")
        .args(["advfirewall", "firewall", "delete", "rule", "name=Slipstream-Block-QUIC"])
        .creation_flags(0x0800_0000)
        .output();
    if block {
        let _ = Command::new("netsh")
            .args([
                "advfirewall",
                "firewall",
                "add",
                "rule",
                "name=Slipstream-Block-QUIC",
                "dir=out",
                "action=block",
                "protocol=UDP",
                "remoteport=443",
            ])
            .creation_flags(0x0800_0000)
            .output();
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        self.stop();
        if let Some(job) = self.job_handle.take() {
            unsafe { CloseHandle(job) };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_fresh_install_defaults() {
        let cfg = Config::default();
        assert_eq!(cfg.enabled_apps, vec!["discord".to_string(), "roblox".to_string()], "Default enabled apps must only be discord and roblox");
        assert_eq!(cfg.app_defaults_version, 1, "Fresh install must set app_defaults_version to 1");
        assert!(cfg.auto_update, "auto_update must be true on fresh install");
        assert!(cfg.auto_start_pc, "auto_start_pc must be true on fresh install");
        assert!(cfg.hide_on_blur, "hide_on_blur must be true on fresh install");
        assert!(cfg.auto_connect, "auto_connect must be true on fresh install");
        assert!(cfg.start_minimized, "start_minimized must be true on fresh install");
        assert!(cfg.minimize_to_tray, "minimize_to_tray must be true on fresh install");
        assert!(cfg.manage_dns, "manage_dns must be true on fresh install");
        assert!(!cfg.gaming_mode, "gaming_mode must default to false");
        assert!(!cfg.game_priority_boost, "game_priority_boost must default to false");
        assert!(!cfg.sound_enabled, "sound_enabled must default to false");
    }

    #[test]
    fn test_one_time_enabled_apps_migration() {
        let temp_dir = std::env::temp_dir().join(format!("slipstream_test_mig_{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        let _ = fs::create_dir_all(&temp_dir);

        // 1) Write legacy config with version 0 and old defaults + a custom platform
        let legacy_json = r#"{
            "isp": "turk-telekom",
            "strategy": "fake-split-md5sig",
            "enabled_apps": ["discord", "youtube", "roblox", "custom_testapp"],
            "custom_platforms": [
                {
                    "id": "custom_testapp",
                    "name": "Test App",
                    "category": "app",
                    "icon": "📱",
                    "domains": ["testapp.com"]
                }
            ],
            "custom_domains": [],
            "custom_exes": []
        }"#;
        fs::write(temp_dir.join("config.json"), legacy_json).unwrap();

        // 2) Load should trigger migration: resets enabled_apps to discord, roblox + custom_testapp
        let loaded = Config::load(&temp_dir);
        assert_eq!(loaded.app_defaults_version, 1);
        assert!(loaded.enabled_apps.contains(&"discord".to_string()));
        assert!(loaded.enabled_apps.contains(&"roblox".to_string()));
        assert!(loaded.enabled_apps.contains(&"custom_testapp".to_string()));
        assert!(!loaded.enabled_apps.contains(&"youtube".to_string()), "youtube should be pruned in one-time migration");

        // 3) User manually turns on youtube and turns off roblox
        let mut user_modified = loaded;
        user_modified.enabled_apps = vec!["discord".to_string(), "youtube".to_string()];
        user_modified.save(&temp_dir);

        // 4) Subsequent load must NEVER touch user choices!
        let reloaded = Config::load(&temp_dir);
        assert_eq!(reloaded.app_defaults_version, 1);
        assert_eq!(reloaded.enabled_apps, vec!["discord".to_string(), "youtube".to_string()], "User preferences must be strictly preserved");

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_config_hide_on_blur_default() {
        let cfg = Config::default();
        assert!(cfg.hide_on_blur, "Config default hide_on_blur must be true");
    }

    #[test]
    fn test_config_deserialize_missing_fields() {
        let json_without_field = r#"{
            "isp": "turk-telekom",
            "strategy": "fake-split-md5sig",
            "enabled_apps": ["discord"],
            "custom_domains": [],
            "custom_exes": []
        }"#;
        let cfg: Config = serde_json::from_str(json_without_field).expect("Deserialization should succeed");
        assert!(cfg.hide_on_blur, "Missing hide_on_blur should default to true via serde");
        assert!(cfg.auto_update, "Missing auto_update should default to true via serde");
        assert!(cfg.auto_start_pc, "Missing auto_start_pc should default to true via serde");
        assert!(cfg.auto_connect, "Missing auto_connect should default to true via serde");
        assert!(cfg.start_minimized, "Missing start_minimized should default to true via serde");
        assert!(cfg.manage_dns, "Missing manage_dns should default to true via serde");
        assert!(!cfg.sound_enabled, "Missing sound_enabled should default to false via serde");
    }

    #[test]
    fn test_config_deserialize_explicit_false() {
        let json_with_false = r#"{
            "isp": "turk-telekom",
            "strategy": "fake-split-md5sig",
            "enabled_apps": ["discord"],
            "custom_domains": [],
            "custom_exes": [],
            "hide_on_blur": false,
            "auto_update": false,
            "auto_start_pc": false,
            "manage_dns": false
        }"#;
        let cfg: Config = serde_json::from_str(json_with_false).expect("Deserialization should succeed");
        assert!(!cfg.hide_on_blur, "Explicit false should be deserialized as false");
        assert!(!cfg.auto_update, "Explicit false should be deserialized as false");
        assert!(!cfg.auto_start_pc, "Explicit false should be deserialized as false");
        assert!(!cfg.manage_dns, "Explicit false should be deserialized as false");
    }

    #[test]
    fn test_config_serialization_roundtrip() {
        let mut cfg = Config::default();
        cfg.hide_on_blur = false;
        let serialized = serde_json::to_string(&cfg).expect("Serialization should succeed");
        assert!(serialized.contains("\"hide_on_blur\":false"));
        let deserialized: Config = serde_json::from_str(&serialized).expect("Deserialization should succeed");
        assert!(!deserialized.hide_on_blur);
    }

    #[test]
    fn test_discord_voice_args() {
        let cfg = Config::default();
        let hosts = PathBuf::from("hosts.txt");
        let args = build_args(&cfg, &hosts);
        assert!(args.contains(&"--wf-tcp=80,443".to_string()));
        assert!(args.contains(&"--wf-udp=443".to_string()));
        assert!(args.contains(&"--filter-tcp=80".to_string()));
        assert!(args.contains(&"--filter-tcp=443".to_string()));
        // Ensure voice UDP is NOT captured or mangled by winws so STUN succeeds with 30-40ms direct ping
        assert!(!args.iter().any(|a| a.contains("19294")));
        assert!(!args.iter().any(|a| a.contains("50000-65535")));
        assert!(!args.iter().any(|a| a.contains("--filter-l7=discord")));
    }

    #[test]
    fn test_strict_targeted_hostlist_enforcement() {
        let mut cfg = Config::default();
        cfg.custom_exes = vec!["thorium.exe".to_string()];
        let hosts = PathBuf::from("hosts.txt");
        let args = build_args(&cfg, &hosts);
        let hostlist_flag = format!("--hostlist={}", hosts.display());
        assert!(args.contains(&hostlist_flag), "Strict targeted mode must ALWAYS include --hostlist to protect unblocked traffic (Steam, banking, etc.)");
    }

    #[test]
    fn test_roblox_domains_and_host_map() {
        let mut cfg = Config::default();
        cfg.enabled_apps = vec!["roblox".into()];
        let domains = profiles::collect_domains(&cfg);
        assert!(domains.contains(&"roblox.com".to_string()));
        assert!(domains.contains(&"gamejoin.roblox.com".to_string()));
        assert!(domains.contains(&"games.roblox.com".to_string()));
        assert!(domains.contains(&"clientsettings.roblox.com".to_string()));
        assert!(domains.contains(&"clientsettingscdn.roblox.com".to_string()));
        assert!(domains.contains(&"assetdelivery.roblox.com".to_string()));
        assert!(domains.contains(&"setup.rbxcdn.com".to_string()));
        assert!(domains.contains(&"rbxcdn.com".to_string()));
        assert!(domains.contains(&"rblx.co".to_string()));

        // Also verify dns::ROBLOX_HOST_MAP includes gamejoin.roblox.com
        let map_contains_gamejoin = dns::ROBLOX_HOST_MAP.iter().any(|(d, _)| *d == "gamejoin.roblox.com");
        assert!(map_contains_gamejoin, "ROBLOX_HOST_MAP must include gamejoin.roblox.com to defeat ISP DNS poisoning");
    }

    #[test]
    fn test_gaming_mode_and_priority_boost_serialization() {
        let json = r#"{
            "isp": "turk-telekom",
            "strategy": "fake-split-md5sig",
            "enabled_apps": ["roblox"],
            "custom_domains": [],
            "custom_exes": [],
            "gaming_mode": true,
            "game_priority_boost": true
        }"#;
        let cfg: Config = serde_json::from_str(json).expect("Deserialization should succeed");
        assert!(cfg.gaming_mode, "gaming_mode should deserialize as true");
        assert!(cfg.game_priority_boost, "game_priority_boost should deserialize as true");
        let serialized = serde_json::to_string(&cfg).expect("Serialization should succeed");
        assert!(serialized.contains("\"gaming_mode\":true"));
        assert!(serialized.contains("\"game_priority_boost\":true"));
    }

    #[test]
    fn test_extract_exe_path_from_cmd() {
        let quoted = r#"    Slipstream    REG_SZ    "C:\Program Files\Slipstream\slipstream.exe" --minimized"#;
        assert_eq!(extract_exe_path_from_cmd(quoted), r#"C:\Program Files\Slipstream\slipstream.exe"#);

        let unquoted = r#"    Slipstream    REG_SZ    C:\Slipstream\slipstream.exe --minimized"#;
        assert_eq!(extract_exe_path_from_cmd(unquoted), r#"C:\Slipstream\slipstream.exe"#);

        let raw_quoted = r#""C:\Users\User\Downloads\Slipstream.exe" --minimized"#;
        assert_eq!(extract_exe_path_from_cmd(raw_quoted), r#"C:\Users\User\Downloads\Slipstream.exe"#);

        let sch_quoted = r#"Task To Run:                          "C:\Program Files\Slipstream\slipstream.exe" --minimized"#;
        assert_eq!(extract_exe_path_from_cmd(sch_quoted), r#"C:\Program Files\Slipstream\slipstream.exe"#);
    }

    #[test]
    fn test_autostart_cmd_generation() {
        let exe = "C:\\Program Files\\Slipstream\\slipstream.exe";
        let cmd_minimized = if true {
            format!("\"{exe}\" --minimized")
        } else {
            format!("\"{exe}\" --autostart")
        };
        assert_eq!(cmd_minimized, "\"C:\\Program Files\\Slipstream\\slipstream.exe\" --minimized");

        let cmd_normal = if false {
            format!("\"{exe}\" --minimized")
        } else {
            format!("\"{exe}\" --autostart")
        };
        assert_eq!(cmd_normal, "\"C:\\Program Files\\Slipstream\\slipstream.exe\" --autostart");
    }

    #[test]
    fn test_canonical_app_exe_never_empty_or_test_dep() {
        let exe = get_canonical_app_exe();
        assert!(!exe.is_empty(), "Canonical exe path must never be empty");
        let lower = exe.to_lowercase();
        assert!(!lower.contains(r"\target\debug"), "Canonical exe must not point to debug build folder: {}", exe);
        assert!(!lower.contains(r"\target\release\deps"), "Canonical exe must not point to test runner deps: {}", exe);
        assert!(!lower.contains(r"\deps\"), "Canonical exe must not point to deps folder: {}", exe);
    }

    #[test]
    fn test_ensure_windows_search_integration() {
        ensure_windows_search_integration();
        // Verify Start Menu shortcuts were created in AppData
        if let Ok(appdata) = std::env::var("APPDATA") {
            let user_programs = Path::new(&appdata).join("Microsoft").join("Windows").join("Start Menu").join("Programs");
            if user_programs.exists() {
                assert!(user_programs.join("Slipstream.lnk").exists(), "Slipstream.lnk must exist");
                assert!(user_programs.join("ss.lnk").exists(), "ss.lnk must exist in user start menu");
            }
        }
    }
}

