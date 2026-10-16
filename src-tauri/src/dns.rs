//! DNS utilities, hosts-based poisoning bypass, and recovery.
//! Strictly ensures Windows network adapters are never forced to 127.0.0.1.
//! Completely neutralizes ISP DNS poisoning (e.g. Türk Telekom 195.175.254.2 block)
//! by injecting clean IP mappings directly into the Windows hosts file during active bypass.

use std::{
    fs,
    os::windows::process::CommandExt,
    path::{Path, PathBuf},
    process::Command,
};
use crate::engine::Config;

const STATE_FILE: &str = "dns_state.txt";
const HOSTS_FILE: &str = r"C:\Windows\System32\drivers\etc\hosts";
const HOSTS_TAG_START: &str = "# --- SLIPSTREAM BYPASS HOSTS START ---";
const HOSTS_TAG_END: &str = "# --- SLIPSTREAM BYPASS HOSTS END ---";

// Real Cloudflare anycast IPs for Discord services (bypasses ISP poisoned DNS redirects)
pub const DISCORD_HOST_MAP: &[(&str, &str)] = &[
    ("discord.com", "162.159.138.232"),
    ("gateway.discord.gg", "162.159.130.234"),
    ("status.discord.com", "162.159.138.232"),
    ("updates.discord.com", "162.159.138.232"),
    ("dl.discordapp.net", "162.159.138.232"),
    ("cdn.discordapp.com", "162.159.130.233"),
    ("media.discordapp.net", "162.159.129.235"),
    ("images-ext-1.discordapp.net", "162.159.134.232"),
    ("images-ext-2.discordapp.net", "162.159.130.232"),
    ("attachments.discord.net", "162.159.135.232"),
    ("attachments.discord.com", "162.159.138.232"),
    ("cdn.discord.com", "162.159.138.232"),
    ("discordapp.com", "162.159.138.232"),
    ("discord.gg", "162.159.138.232"),
    ("discordapp.net", "162.159.138.232"),
    ("discordcdn.com", "162.159.138.232"),
    ("discordsays.com", "162.159.138.232"),
    ("discordsez.com", "162.159.138.232"),
    ("discordactivities.com", "162.159.138.232"),
    ("discord-activities.com", "162.159.138.232"),
    ("dis.gd", "162.159.138.232"),
    ("discord.co", "162.159.138.232"),
    ("discord.dev", "162.159.138.232"),
    ("discord.new", "162.159.138.232"),
    ("discord.gift", "162.159.138.232"),
    ("discord.gifts", "162.159.138.232"),
    ("discord.store", "162.159.138.232"),
    ("discord.tools", "162.159.138.232"),
    ("discord.design", "162.159.138.232"),
    ("discord.media", "162.159.129.235"),
];

// Real Anycast IPs for Roblox services (bypasses ISP DNS poisoning)
pub const ROBLOX_HOST_MAP: &[(&str, &str)] = &[
    ("roblox.com", "128.116.44.3"),
    ("www.roblox.com", "128.116.44.3"),
    ("web.roblox.com", "128.116.44.3"),
    ("m.roblox.com", "128.116.44.3"),
    ("api.roblox.com", "128.116.44.3"),
    ("apis.roblox.com", "128.116.44.3"),
    ("auth.roblox.com", "128.116.44.3"),
    ("games.roblox.com", "128.116.44.3"),
    ("gamejoin.roblox.com", "128.116.44.3"),
    ("matchmaking.roblox.com", "128.116.44.3"),
    ("assetdelivery.roblox.com", "128.116.44.3"),
    ("clientsettings.roblox.com", "128.116.44.3"),
    ("clientsettings.api.roblox.com", "128.116.44.3"),
    ("versioncompatibility.api.roblox.com", "128.116.44.3"),
    ("setup.roblox.com", "128.116.44.3"),
    ("avatar.roblox.com", "128.116.44.3"),
    ("chat.roblox.com", "128.116.44.3"),
    ("friends.roblox.com", "128.116.44.3"),
    ("presence.roblox.com", "128.116.44.3"),
    ("economy.roblox.com", "128.116.44.3"),
    ("realtime.roblox.com", "128.116.44.3"),
    ("realtime-signalr.roblox.com", "128.116.44.3"),
    ("thumbnails.roblox.com", "128.116.44.3"),
    ("locale.roblox.com", "128.116.44.3"),
    ("metrics.roblox.com", "128.116.44.3"),
    ("ncs.roblox.com", "128.116.44.3"),
    ("client-telemetry.roblox.com", "128.116.44.3"),
    ("ecsv2.roblox.com", "128.116.44.3"),
    ("abtesting.roblox.com", "128.116.44.3"),
    ("ephemeralcounters.api.roblox.com", "128.116.44.3"),
    ("accountinformation.roblox.com", "128.116.44.3"),
    ("accountsettings.roblox.com", "128.116.44.3"),
    ("badges.roblox.com", "128.116.44.3"),
    ("contacts.roblox.com", "128.116.44.3"),
    ("economycreatorstats.roblox.com", "128.116.44.3"),
    ("engagementpayouts.roblox.com", "128.116.44.3"),
    ("followings.roblox.com", "128.116.44.3"),
    ("gameinternationalization.roblox.com", "128.116.44.3"),
    ("groups.roblox.com", "128.116.44.3"),
    ("inventory.roblox.com", "128.116.44.3"),
    ("itemconfiguration.roblox.com", "128.116.44.3"),
    ("localizationtables.roblox.com", "128.116.44.3"),
    ("midas.roblox.com", "128.116.44.3"),
    ("notifications.roblox.com", "128.116.44.3"),
    ("premiumfeatures.roblox.com", "128.116.44.3"),
    ("privatemessages.roblox.com", "128.116.44.3"),
    ("publish.roblox.com", "128.116.44.3"),
    ("scs.roblox.com", "128.116.44.3"),
    ("share.roblox.com", "128.116.44.3"),
    ("textfilter.roblox.com", "128.116.44.3"),
    ("trades.roblox.com", "128.116.44.3"),
    ("translationroles.roblox.com", "128.116.44.3"),
    ("translations.roblox.com", "128.116.44.3"),
    ("twostepverification.roblox.com", "128.116.44.3"),
    ("usersearch.roblox.com", "128.116.44.3"),
    ("users.roblox.com", "128.116.44.3"),
    ("voice.roblox.com", "128.116.44.3"),
    ("devforum.roblox.com", "128.116.44.3"),
    ("create.roblox.com", "128.116.44.3"),
    ("developer.roblox.com", "128.116.44.3"),
    ("corp.roblox.com", "128.116.44.3"),
    ("help.roblox.com", "128.116.44.3"),
    ("en.help.roblox.com", "128.116.44.3"),
    ("titanium.roblox.com", "128.116.44.3"),
    ("edge-term4.roblox.com", "128.116.44.3"),
    ("nspx1.roblox.com", "128.116.64.1"),
    ("nspx2.roblox.com", "128.116.64.1"),
    ("nspx3.roblox.com", "128.116.64.1"),
    ("nspx4.roblox.com", "128.116.64.1"),
    ("clientsettingscdn.roblox.com", "104.96.152.82"),
    ("setup.rbxcdn.com", "2.16.16.147"),
    ("roblox-setup.rbxcdn.com", "2.16.16.147"),
    ("rblx.org", "128.116.44.129"),
    ("rblx.co", "18.235.67.62"),
    ("blox.com", "100.59.60.4"),
    ("robloxlabs.com", "128.116.44.3"),
    ("roblox.net", "100.59.60.4"),
    ("roblox.us", "100.59.60.4"),
    ("roblox.co.uk", "100.59.60.4"),
    ("robloxdev.com", "128.116.44.3"),
    ("robloxassets.com", "128.116.44.3"),
    ("robloxlibrary.com", "128.116.44.3"),
    ("robloxlocalization.com", "128.116.44.3"),
    ("robloxqa.com", "128.116.44.3"),
    ("robloxstudios.com", "128.116.44.3"),
    ("rblx.game", "128.116.44.3"),
    ("rblx.social", "128.116.44.3"),
    ("rblx.wtf", "128.116.44.3"),
    ("arkoselabs.com", "3.168.236.54"),
    ("roblox-api.arkoselabs.com", "108.157.60.94"),
    ("cdn.arkoselabs.com", "108.157.60.10"),
    ("client-api.arkoselabs.com", "108.157.60.10"),
];

fn ps(script: &str) -> String {
    Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", script])
        .creation_flags(0x0800_0000)
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
        .unwrap_or_default()
}

/// Flushes Windows DNS resolver cache.
pub fn flush() {
    let _ = Command::new("ipconfig")
        .arg("/flushdns")
        .creation_flags(0x0800_0000)
        .output();
}

#[allow(clippy::permissions_set_readonly_false)]
fn ensure_hosts_writable() {
    if let Ok(meta) = fs::metadata(HOSTS_FILE) {
        let mut perms = meta.permissions();
        if perms.readonly() {
            perms.set_readonly(false);
            let _ = fs::set_permissions(HOSTS_FILE, perms);
        }
    }
}

/// Removes Slipstream bypass mappings from the Windows hosts file.
pub fn clear_bypass_hosts() {
    if let Ok(content) = fs::read_to_string(HOSTS_FILE) {
        if content.contains(HOSTS_TAG_START) {
            let mut new_lines = Vec::new();
            let mut in_block = false;
            for line in content.lines() {
                if line.contains(HOSTS_TAG_START) {
                    in_block = true;
                    continue;
                }
                if line.contains(HOSTS_TAG_END) {
                    in_block = false;
                    continue;
                }
                if !in_block {
                    new_lines.push(line);
                }
            }
            let mut cleaned = new_lines.join("\r\n");
            if !cleaned.ends_with("\r\n") {
                cleaned.push_str("\r\n");
            }
            ensure_hosts_writable();
            let _ = fs::write(HOSTS_FILE, cleaned);
            flush();
        }
    }
}

/// Injects real clean IPs into Windows hosts file to neutralize ISP DNS poisoning
/// for enabled apps (such as Discord and Roblox) with zero proxy or adapter DNS tampering.
pub fn apply_bypass_hosts(cfg: &Config) {
    clear_bypass_hosts();

    let mut entries = Vec::new();
    let has_discord = cfg.enabled_apps.iter().any(|a| a == "discord");
    let has_roblox = cfg.enabled_apps.iter().any(|a| a == "roblox");

    if has_discord {
        for (domain, ip) in DISCORD_HOST_MAP {
            entries.push(format!("{ip:<15} {domain}"));
        }
    }

    if has_roblox {
        for (domain, ip) in ROBLOX_HOST_MAP {
            entries.push(format!("{ip:<15} {domain}"));
        }
    }

    if entries.is_empty() {
        return;
    }

    let existing = fs::read_to_string(HOSTS_FILE).unwrap_or_default();
    let mut output = existing.trim_end().to_string();
    output.push_str("\r\n\r\n");
    output.push_str(HOSTS_TAG_START);
    output.push_str("\r\n");
    for entry in entries {
        output.push_str(&entry);
        output.push_str("\r\n");
    }
    output.push_str(HOSTS_TAG_END);
    output.push_str("\r\n");

    ensure_hosts_writable();
    let _ = fs::write(HOSTS_FILE, output);
    flush();
}

/// Configures active network adapter DNS servers to fast clean public DNS (e.g. Cloudflare / Google / Quad9 / AdGuard)
/// to completely defeat ISP DNS poisoning for Discord voice servers and other dynamic subdomains.
/// Saves state file before modifying, and flushes DNS cache.
pub fn apply_dns(cfg: &Config, dir: &Path) {
    let state_file = dir.join(STATE_FILE);
    let _ = fs::write(&state_file, "active");

    let (primary, secondary) = match cfg.doh_provider.to_lowercase().as_str() {
        "google" => ("8.8.8.8", "8.8.4.4"),
        "quad9" => ("9.9.9.9", "149.112.112.112"),
        "adguard" => ("94.140.14.140", "94.140.14.141"),
        "yandex" => ("77.88.8.8", "77.88.8.1"),
        _ => ("1.1.1.1", "8.8.8.8"),
    };

    let script = format!(
        r"Get-NetAdapter | Where-Object {{ $_.Status -eq 'Up' }} | ForEach-Object {{ Set-DnsClientServerAddress -InterfaceIndex $_.InterfaceIndex -ServerAddresses ('{primary}', '{secondary}') }}"
    );
    ps(&script);
    flush();
}

/// Restores network adapter DNS to automatic DHCP, removes state file, and flushes DNS.
pub fn restore_dns(dir: &Path) {
    let state_file = dir.join(STATE_FILE);
    if state_file.exists() {
        let _ = fs::remove_file(state_file);
        reset_all_adapters();
    }
}

/// Restores any network adapter that was previously modified.
#[allow(dead_code)]
pub fn restore(state: &Path) {
    if state.exists() {
        let _ = fs::remove_file(state);
    }
    clear_bypass_hosts();
    reset_all_adapters();
}

/// Startup recovery: ensures no leftover DNS or proxy remains on any network adapter,
/// and cleans any stale hosts entries.
pub fn recover(dir: &Path) {
    clear_bypass_hosts();
    let state = dir.join(STATE_FILE);
    if state.exists() {
        restore_dns(dir);
    }
}

/// Force-resets all active network adapter DNS servers to automatic DHCP.
pub fn reset_all_adapters() {
    ps(r"Get-NetAdapter | Where-Object { $_.Status -eq 'Up' } | ForEach-Object { Set-DnsClientServerAddress -InterfaceIndex $_.InterfaceIndex -ResetServerAddresses }");
    flush();
}

#[allow(dead_code)]
pub struct Dns {
    _state: PathBuf,
}

#[allow(dead_code)]
impl Dns {
    pub fn start(dir: &Path, _domains: Vec<String>) -> Result<Dns, String> {
        recover(dir);
        Ok(Dns {
            _state: dir.join(STATE_FILE),
        })
    }
    pub fn set_domains(&self, _d: Vec<String>) {}
    pub fn stop(&mut self) {}
}

