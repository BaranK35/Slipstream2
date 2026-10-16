//! Owns the winws.exe child process and Discord updater relay.
//! Zero proxy overhead: traffic is handled at the kernel packet layer via WinDivert.
//! System proxy and network adapter DNS are NEVER modified.

use crate::{dns, isp, profiles, proxy};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    os::windows::process::CommandExt,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
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
            doh_provider: "cloudflare".into(),
            gaming_mode: false,
            game_priority_boost: false,
            auto_healing: true,
            auto_update: true,
            process_detection_mode: false,
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
                let _ = set_windows_autostart(true);
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
        let _ = set_windows_autostart(self.auto_start_pc);
    }
}

/// Registers or unregisters Slipstream in Windows HKCU Run key for boot startup.
pub fn set_windows_autostart(enable: bool) -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let exe_str = exe.to_string_lossy();

    if enable {
        let cmd_val = format!("\"{exe_str}\" --minimized");
        let status = Command::new("reg.exe")
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

        if !status.success() {
            return Err("Kayıt defterine başlangıç girdisi eklenemedi.".into());
        }
    } else {
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

/// Checks if Windows autostart is active in HKCU.
pub fn is_windows_autostart() -> bool {
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

    out.contains("Slipstream")
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

pub const WINWS_BYTES: &[u8] = include_bytes!("../../engine/winws.exe");
pub const WINDIVERT_DLL_BYTES: &[u8] = include_bytes!("../../engine/WinDivert.dll");
pub const WINDIVERT_SYS_BYTES: &[u8] = include_bytes!("../../engine/WinDivert64.sys");
pub const CYGWIN_DLL_BYTES: &[u8] = include_bytes!("../../engine/cygwin1.dll");
pub const LICENSE_BYTES: &[u8] = include_bytes!("../../engine/WinDivert-LICENSE.txt");

/// Ensures engine binaries exist in target directory, extracting embedded copies if needed.
pub fn ensure_engine_files(target_dir: &std::path::Path) -> Result<PathBuf, String> {
    let winws_path = target_dir.join("winws.exe");
    let dll_path = target_dir.join("WinDivert.dll");
    let sys_path = target_dir.join("WinDivert64.sys");
    let cygwin_path = target_dir.join("cygwin1.dll");
    let lic_path = target_dir.join("WinDivert-LICENSE.txt");

    fs::create_dir_all(target_dir).map_err(|e| e.to_string())?;

    let write_if_needed = |path: &PathBuf, data: &[u8]| -> std::io::Result<()> {
        let need_write = match fs::metadata(path) {
            Ok(meta) => meta.len() != data.len() as u64,
            Err(_) => true,
        };
        if need_write {
            fs::write(path, data)?;
        }
        Ok(())
    };

    write_if_needed(&winws_path, WINWS_BYTES).map_err(|e| e.to_string())?;
    write_if_needed(&dll_path, WINDIVERT_DLL_BYTES).map_err(|e| e.to_string())?;
    write_if_needed(&sys_path, WINDIVERT_SYS_BYTES).map_err(|e| e.to_string())?;
    write_if_needed(&cygwin_path, CYGWIN_DLL_BYTES).map_err(|e| e.to_string())?;
    write_if_needed(&lic_path, LICENSE_BYTES).map_err(|e| e.to_string())?;

    Ok(winws_path)
}

pub struct Engine {
    pub dir: PathBuf,
    pub bin: PathBuf,
    child: Option<Child>,
    relay_stop: Option<Arc<AtomicBool>>,
}

fn push(a: &mut Vec<String>, xs: &[&str]) {
    a.extend(xs.iter().map(|s| s.to_string()));
}

/// Builds the winws argument list. Traffic matching TCP 80/443 and Discord voice
/// UDP is desynced transparently at the kernel packet level via WinDivert.
fn build_args(cfg: &Config, hosts: &Path) -> Vec<String> {
    let strat = isp::strategy(
        cfg.strategy
            .as_deref()
            .unwrap_or_else(|| isp::order(&cfg.isp)[0]),
    );

    let mut a = vec![];
    push(&mut a, &["--wf-tcp=80,443"]);
    // Capture only UDP 443 for QUIC desync. Discord voice UDP (ports 19294-19344 & 50000-65535)
    // passes directly through kernel with zero WinDivert overhead, zero STUN corruption, and 30-40ms native ping.
    push(&mut a, &["--wf-udp=443"]);

    let hostlist_arg = format!("--hostlist={}", hosts.display());

    // QUIC (HTTP/3) - filtered by hostlist
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

    // Plain HTTP - filtered by hostlist
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
    // All non-listed sites (games, banking, local networks) pass through with ZERO overhead (0 ping)!
    push(
        &mut a,
        &[
            "--filter-tcp=443",
            &hostlist_arg,
        ],
    );
    push(&mut a, strat.tls);
    a
}

impl Engine {
    pub fn new(dir: PathBuf, bin: PathBuf) -> Self {
        Self {
            dir,
            bin,
            child: None,
            relay_stop: None,
        }
    }

    /// True while the engine (winws) is running.
    pub fn is_on(&self) -> bool {
        self.child.is_some()
    }

    /// Ensures the Discord updater relay is running if enabled, or stopped if not.
    pub fn ensure_relay(&mut self, enabled: bool) {
        if enabled {
            if self.relay_stop.is_none() {
                let r_stop = Arc::new(AtomicBool::new(false));
                self.relay_stop = Some(r_stop.clone());
                tauri::async_runtime::spawn(proxy::run_discord_relay(r_stop, proxy::DISCORD_RELAY_PORT));
            }
            proxy::patch_discord_settings(Some(&format!("http://127.0.0.1:{}/", proxy::DISCORD_RELAY_PORT)));
        } else {
            if let Some(s) = self.relay_stop.take() {
                s.store(true, Ordering::Relaxed);
            }
            proxy::patch_discord_settings(None);
        }
    }

    /// Starts (or re-applies) everything. On error, stops cleanly.
    pub fn start(&mut self, cfg: &Config) -> Result<(), String> {
        let r = self.start_inner(cfg);
        if r.is_err() {
            self.stop();
        }
        r
    }

/// Forcefully terminates any orphan winws.exe processes to free WinDivert driver handles.
pub fn kill_all_winws() {
    let _ = Command::new("taskkill")
        .args(["/IM", "winws.exe", "/T", "/F"])
        .creation_flags(0x0800_0000)
        .output();
}

    fn start_inner(&mut self, cfg: &Config) -> Result<(), String> {
        if let Some(mut c) = self.child.take() {
            let _ = c.kill();
            let _ = c.wait();
        }
        Self::kill_all_winws();

        if !self.bin.exists() {
            if let Some(p) = self.bin.parent() {
                let _ = ensure_engine_files(p);
            }
            if !self.bin.exists() {
                return Err(format!(
                    "Motor bulunamadı: {}. winws.exe ve WinDivert dosyalarının mevcut olduğundan emin olun.",
                    self.bin.display()
                ));
            }
        }
        fs::create_dir_all(&self.dir).map_err(|e| e.to_string())?;

        // 1) Ensure network adapters have clean DHCP and proxy settings are clean
        dns::recover(&self.dir);
        proxy::clean_stale_proxy();

        // 2) Write hostlist
        let domains = profiles::collect_domains(cfg);
        let hosts = self.dir.join("hosts.txt");
        fs::write(&hosts, domains.join("\n")).map_err(|e| e.to_string())?;

        let log = fs::File::create(self.dir.join("engine.log")).map_err(|e| e.to_string())?;
        let log2 = log.try_clone().map_err(|e| e.to_string())?;

        // High priority class (0x0000_0080) for zero scheduling jitter and lowest latency packet reinjection
        let priority_flag = 0x0000_0080;
        let creation_flags = 0x0800_0000 | priority_flag;

        // 3) Start winws packet engine at driver level
        let mut child = Command::new(&self.bin)
            .args(build_args(cfg, &hosts))
            .current_dir(self.bin.parent().unwrap_or(&self.dir))
            .creation_flags(creation_flags)
            .stdout(Stdio::from(log))
            .stderr(Stdio::from(log2))
            .spawn()
            .map_err(|e| format!("DPI motoru başlatılamadı: {e}"))?;

        thread::sleep(Duration::from_millis(300));
        if let Ok(Some(status)) = child.try_wait() {
            let out = fs::read_to_string(self.dir.join("engine.log")).unwrap_or_default();
            return Err(format!(
                "Motor beklenmedik şekilde durdu ({status}). {}",
                out.trim()
            ));
        }
        self.child = Some(child);

        // 4) Apply clean DNS & hosts-based poisoning bypass for Discord and Roblox
        if cfg.manage_dns {
            dns::apply_dns(cfg, &self.dir);
        }
        dns::apply_bypass_hosts(cfg);

        // 5) If Discord is enabled, ensure local Discord updater relay is active
        let has_discord = cfg.enabled_apps.iter().any(|a| a == "discord");
        self.ensure_relay(has_discord);

        // 6) Low-latency gaming optimizations (TCP NoDelay, Network Throttling removal, System Responsiveness)
        apply_gaming_network_tweaks(cfg.gaming_mode);

        Ok(())
    }

    /// Health watchdog: checks if winws is still alive, restarts if needed.
    pub fn check_health(&mut self, cfg: &Config) -> bool {
        if let Some(ref mut c) = self.child {
            if let Ok(Some(_)) = c.try_wait() {
                // Process died
                self.child = None;
                if cfg.auto_healing {
                    let _ = self.start_inner(cfg);
                    return true;
                }
            }
        }
        false
    }

    /// Full stop: terminates winws, stops relay, restores Discord settings, cleans hosts and proxy state.
    pub fn stop(&mut self) {
        // Kill winws child process
        if let Some(mut c) = self.child.take() {
            let _ = c.kill();
            let _ = c.wait();
        }
        Self::kill_all_winws();

        // Clean hosts file bypass entries
        dns::clear_bypass_hosts();

        // Restore network adapters to clean DHCP
        dns::restore_dns(&self.dir);

        // Clean Discord settings & ensure system proxy is completely clean
        self.ensure_relay(false);
        proxy::clean_stale_proxy();

        // Cleanly restore network defaults on engine stop (disable gaming mode tweaks)
        apply_gaming_network_tweaks(false);

        // Fast stop for WinDivert driver to ensure kernel handle release
        let out = Command::new("sc")
            .args(["stop", "windivert"])
            .creation_flags(0x0800_0000)
            .output();
        if let Ok(o) = out {
            let s = String::from_utf8_lossy(&o.stdout);
            if !s.contains("STOPPED") && !s.contains("1062") && !s.contains("1060") {
                thread::sleep(Duration::from_millis(40));
                let _ = Command::new("sc")
                    .args(["stop", "windivert"])
                    .creation_flags(0x0800_0000)
                    .output();
            }
        }
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        self.stop();
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
}

