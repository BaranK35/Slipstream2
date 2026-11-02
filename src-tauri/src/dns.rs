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
    ("dl.discordapp.net", "104.18.52.172"),
    ("cdn.discordapp.com", "162.159.130.233"),
    ("media.discordapp.net", "162.159.129.232"),
    ("images-ext-1.discordapp.net", "162.159.130.232"),
    ("images-ext-2.discordapp.net", "162.159.130.232"),
    ("discordapp.com", "162.159.135.233"),
    ("discord.gg", "162.159.130.234"),
    ("discordcdn.com", "162.159.137.233"),
    ("discordsays.com", "172.64.153.7"),
    ("discordsez.com", "104.18.38.75"),
    ("discordactivities.com", "172.64.155.82"),
    ("dis.gd", "172.64.150.180"),
    ("discord.co", "104.17.51.93"),
    ("discord.dev", "104.21.72.221"),
    ("discord.new", "104.21.89.187"),
    ("discord.gift", "104.21.25.51"),
    ("discord.gifts", "104.21.82.160"),
    ("discord.store", "23.227.38.74"),
    ("discord.design", "104.21.80.173"),
    ("discord.media", "162.159.130.235"),
    ("remote-auth-gateway.discord.gg", "162.159.130.234"),
    ("latency.discord.media", "162.159.130.235"),
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
        .args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-Command", script])
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

fn is_stray_bypass_line(line: &str) -> bool {
    let trimmed = line.trim();
    if trimmed.starts_with('#') || trimmed.is_empty() {
        return false;
    }
    let code_part = trimmed.split('#').next().unwrap_or("");
    code_part.split_whitespace().skip(1).any(|host| {
        let h = host.to_ascii_lowercase();
        DISCORD_HOST_MAP.iter().any(|(d, _)| h == *d)
            || ROBLOX_HOST_MAP.iter().any(|(d, _)| h == *d)
            || h == "discord.com"
            || h.ends_with(".discord.com")
            || h == "discordapp.com"
            || h.ends_with(".discordapp.com")
            || h == "discordapp.net"
            || h.ends_with(".discordapp.net")
            || h == "discord.gg"
            || h.ends_with(".discord.gg")
            || h == "discordcdn.com"
            || h.ends_with(".discordcdn.com")
            || h == "discord.media"
            || h.ends_with(".discord.media")
            || h == "dis.gd"
            || h == "discord.co"
            || h == "discord.dev"
            || h == "discord.new"
            || h == "discord.gift"
            || h == "discord.gifts"
            || h == "discord.store"
            || h == "discord.design"
            || h == "discordsays.com"
            || h == "discordsez.com"
            || h == "discordactivities.com"
            || h == "discord-activities.com"
            || h == "roblox.com"
            || h.ends_with(".roblox.com")
            || h.ends_with(".rbxcdn.com")
            || h.ends_with(".arkoselabs.com")
    })
}

/// Filters out both the Slipstream bypass block and any stray bypass mappings from hosts content.
pub fn filter_hosts_content(content: &str) -> String {
    let mut new_lines = Vec::new();
    let mut in_block = false;
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.contains(HOSTS_TAG_START) {
            in_block = true;
            continue;
        }
        if trimmed.contains(HOSTS_TAG_END) {
            in_block = false;
            continue;
        }
        if in_block {
            continue;
        }

        // Remove stray un-tagged Discord/Roblox bypass entries
        if is_stray_bypass_line(line) {
            continue;
        }

        new_lines.push(line);
    }
    let mut cleaned = new_lines.join("\r\n");
    if !cleaned.ends_with("\r\n") {
        cleaned.push_str("\r\n");
    }
    cleaned
}

/// Removes Slipstream bypass mappings from the Windows hosts file.
pub fn clear_bypass_hosts() {
    if let Ok(content) = fs::read_to_string(HOSTS_FILE) {
        let has_tag = content.contains(HOSTS_TAG_START);
        let has_stray = content.lines().any(is_stray_bypass_line);

        if has_tag || has_stray {
            let cleaned = filter_hosts_content(&content);
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
/// across both IPv4 and IPv6 to completely defeat ISP DNS poisoning and leakage.
/// Saves state file before modifying, and flushes DNS cache.
pub fn apply_dns(cfg: &Config, dir: &Path) {
    let state_file = dir.join(STATE_FILE);
    let _ = fs::write(&state_file, "active");

    let (primary, secondary, ipv6_primary, ipv6_secondary) = match cfg.doh_provider.to_lowercase().as_str() {
        "google" => ("8.8.8.8", "8.8.4.4", "2001:4860:4860::8888", "2001:4860:4860::8844"),
        "quad9" => ("9.9.9.9", "149.112.112.112", "2620:fe::fe", "2620:fe::9"),
        "adguard" => ("94.140.14.140", "94.140.14.141", "2a10:50c0::ad1:ff", "2a10:50c0::ad2:ff"),
        "yandex" => ("77.88.8.8", "77.88.8.1", "2a02:6b8::feed:0ff", "2a02:6b8:0:1::feed:0ff"),
        _ => ("1.1.1.1", "1.0.0.1", "2606:4700:4700::1111", "2606:4700:4700::1001"),
    };

    let script = format!(
        r"Get-NetAdapter | Where-Object Status -eq 'Up' | ForEach-Object {{ $idx = $_.InterfaceIndex; Set-DnsClientServerAddress -InterfaceIndex $idx -ServerAddresses @('{primary}', '{secondary}') -ErrorAction SilentlyContinue; Set-DnsClientServerAddress -InterfaceIndex $idx -ServerAddresses @('{ipv6_primary}', '{ipv6_secondary}') -ErrorAction SilentlyContinue }}"
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
    ps(r"Get-NetAdapter | Where-Object Status -eq 'Up' | ForEach-Object { Set-DnsClientServerAddress -InterfaceIndex $_.InterfaceIndex -ResetServerAddresses -ErrorAction SilentlyContinue }");
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_filter_hosts_content_removes_block_and_strays() {
        let input = "\
# localhost name resolution
127.0.0.1 localhost
::1 localhost

# Stray untagged Discord bypass entries
162.159.138.232 updates.discord.com
162.159.138.232 discord.com # inline comment
162.159.130.234 gateway.discord.gg
128.116.44.3 roblox.com

# --- SLIPSTREAM BYPASS HOSTS START ---
162.159.138.232 status.discord.com
128.116.44.3 api.roblox.com
# --- SLIPSTREAM BYPASS HOSTS END ---

# Custom local network entries
192.168.1.1 router.local
";

        let cleaned = filter_hosts_content(input);
        assert!(!cleaned.contains("updates.discord.com"));
        assert!(!cleaned.contains("gateway.discord.gg"));
        assert!(!cleaned.contains("status.discord.com"));
        assert!(!cleaned.contains("roblox.com"));
        assert!(!cleaned.contains(HOSTS_TAG_START));
        assert!(!cleaned.contains(HOSTS_TAG_END));
        assert!(cleaned.contains("127.0.0.1 localhost"));
        assert!(cleaned.contains("::1 localhost"));
        assert!(cleaned.contains("192.168.1.1 router.local"));
    }
}
