//! Owns the winws.exe child process and Discord updater relay.
//! Zero proxy overhead: traffic is handled at the kernel packet layer via WinDivert.
//! System proxy and network adapter DNS are NEVER modified.

use crate::{dns, isp, profiles, proxy};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    os::windows::process::CommandExt,
    path::PathBuf,
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
    #[serde(default)]
    pub manage_dns: bool,

    // === Sistem ve Başlangıç Ayarları ===
    #[serde(default)]
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
    #[serde(default = "default_true")]
    pub auto_healing: bool,
    #[serde(default)]
    pub auto_update: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            isp: "turk-telekom".into(),
            strategy: Some("fake-split-md5sig".into()),
            enabled_apps: vec!["discord".into(), "youtube".into(), "roblox".into()],
            custom_platforms: vec![],
            custom_domains: vec![],
            custom_exes: vec![],
            manage_dns: false,
            auto_start_pc: false,
            start_minimized: true,
            auto_connect: true,
            minimize_to_tray: true,
            hide_on_blur: true,
            doh_provider: "cloudflare".into(),
            gaming_mode: false,
            auto_healing: true,
            auto_update: false,
        }
    }
}

impl Config {
    pub fn load(dir: &PathBuf) -> Self {
        let mut cfg: Self = fs::read_to_string(dir.join("config.json"))
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        cfg.auto_start_pc = is_windows_autostart();
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
fn build_args(cfg: &Config, hosts: &PathBuf) -> Vec<String> {
    let strat = isp::strategy(
        cfg.strategy
            .as_deref()
            .unwrap_or_else(|| isp::order(&cfg.isp)[0]),
    );
    let voice = profiles::needs_voice(cfg);

    let mut a = vec![];
    push(&mut a, &["--wf-tcp=80,443"]);
    push(
        &mut a,
        &[if voice {
            "--wf-udp=443,50000-50100"
        } else {
            "--wf-udp=443"
        }],
    );

    // Discord voice discovery and STUN desync (lightweight, zero jitter, natural 40ms ping)
    if voice {
        push(
            &mut a,
            &[
                "--filter-udp=50000-50100",
                "--filter-l7=discord,stun",
                "--dpi-desync=fake",
                "--dpi-desync-repeats=1",
                "--new",
            ],
        );
    }

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

    fn start_inner(&mut self, cfg: &Config) -> Result<(), String> {
        if let Some(mut c) = self.child.take() {
            let _ = c.kill();
            let _ = c.wait();
        }

        if !self.bin.exists() {
            return Err(format!(
                "Motor bulunamadı: {}. winws.exe ve WinDivert dosyalarının mevcut olduğundan emin olun.",
                self.bin.display()
            ));
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

        // High priority class (0x0000_0080) for low latency gaming mode
        let priority_flag = if cfg.gaming_mode { 0x0000_0080 } else { 0x0000_0020 };
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

        // 4) Apply hosts-based poisoning bypass for Discord and Roblox
        dns::apply_bypass_hosts(cfg);

        // 5) If Discord is enabled, ensure local Discord updater relay is active
        let has_discord = cfg.enabled_apps.iter().any(|a| a == "discord");
        self.ensure_relay(has_discord);

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

        // Clean hosts file bypass entries
        dns::clear_bypass_hosts();

        // Clean Discord settings & ensure system proxy is completely clean
        self.ensure_relay(false);
        proxy::clean_stale_proxy();

        // Ensure network adapters are untouched
        dns::recover(&self.dir);

        let _ = Command::new("sc")
            .args(["stop", "WinDivert"])
            .creation_flags(0x0800_0000)
            .output();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_hide_on_blur_default() {
        let cfg = Config::default();
        assert!(cfg.hide_on_blur, "Config default hide_on_blur must be true");
    }

    #[test]
    fn test_config_deserialize_missing_hide_on_blur() {
        let json_without_field = r#"{
            "isp": "turk-telekom",
            "strategy": "fake-split-md5sig",
            "enabled_apps": ["discord"],
            "custom_domains": [],
            "custom_exes": []
        }"#;
        let cfg: Config = serde_json::from_str(json_without_field).expect("Deserialization should succeed");
        assert!(cfg.hide_on_blur, "Missing hide_on_blur should default to true via serde");
    }

    #[test]
    fn test_config_deserialize_explicit_false() {
        let json_with_false = r#"{
            "isp": "turk-telekom",
            "strategy": "fake-split-md5sig",
            "enabled_apps": ["discord"],
            "custom_domains": [],
            "custom_exes": [],
            "hide_on_blur": false
        }"#;
        let cfg: Config = serde_json::from_str(json_with_false).expect("Deserialization should succeed");
        assert!(!cfg.hide_on_blur, "Explicit false should be deserialized as false");
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
}
