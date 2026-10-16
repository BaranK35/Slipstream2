//! App and Game profiles with categories, domain expansion, and custom additions.
use crate::engine::Config;
use serde::Serialize;
use std::collections::BTreeSet;

#[derive(Serialize, Clone)]
pub struct AppProfile {
    pub id: &'static str,
    pub name: &'static str,
    pub category: &'static str, // "app" | "game"
    pub tag: &'static str,
    pub icon: &'static str,
    pub domains: &'static [&'static str],
    pub exes: &'static [&'static str],
    pub voice: bool,
    pub probe: &'static str,
}

pub const APPS: &[AppProfile] = &[
    // --- APPLICATIONS ---
    AppProfile {
        id: "discord",
        name: "Discord",
        category: "app",
        tag: "Ses, sohbet, medya ve güncelleyici",
        icon: "💬",
        domains: &[
            "discord.com", "discord.gg", "discord.co", "discord.media",
            "discord.new", "discord.gift", "discord.gifts", "discord.dev",
            "discord.design", "discord.store", "discord.tools", "discordapp.com",
            "discordapp.net", "discordcdn.com", "discordstatus.com", "discordsays.com",
            "gateway.discord.gg", "status.discord.com", "updates.discord.com", "dl.discordapp.net",
            "cdn.discordapp.com", "media.discordapp.net", "images-ext-1.discordapp.net",
            "images-ext-2.discordapp.net", "attachments.discord.net", "attachments.discord.com", "cdn.discord.com",
        ],
        exes: &["Discord.exe", "DiscordPTB.exe", "DiscordCanary.exe", "Update.exe"],
        voice: true,
        probe: "https://discord.com",
    },

    // --- GAMES ---
    AppProfile {
        id: "roblox",
        name: "Roblox",
        category: "game",
        tag: "Oyun, istemci, stüdyo, giriş ve tüm CDN'ler",
        icon: "🎮",
        domains: &[
            "roblox.com",
            "www.roblox.com",
            "web.roblox.com",
            "m.roblox.com",
            "api.roblox.com",
            "apis.roblox.com",
            "auth.roblox.com",
            "games.roblox.com",
            "gamejoin.roblox.com",
            "matchmaking.roblox.com",
            "assetdelivery.roblox.com",
            "clientsettings.roblox.com",
            "clientsettingscdn.roblox.com",
            "clientsettings.api.roblox.com",
            "versioncompatibility.api.roblox.com",
            "setup.roblox.com",
            "avatar.roblox.com",
            "chat.roblox.com",
            "friends.roblox.com",
            "presence.roblox.com",
            "economy.roblox.com",
            "realtime.roblox.com",
            "realtime-signalr.roblox.com",
            "thumbnails.roblox.com",
            "locale.roblox.com",
            "metrics.roblox.com",
            "ncs.roblox.com",
            "client-telemetry.roblox.com",
            "ecsv2.roblox.com",
            "abtesting.roblox.com",
            "ephemeralcounters.api.roblox.com",
            "accountinformation.roblox.com",
            "accountsettings.roblox.com",
            "badges.roblox.com",
            "contacts.roblox.com",
            "economycreatorstats.roblox.com",
            "engagementpayouts.roblox.com",
            "followings.roblox.com",
            "gameinternationalization.roblox.com",
            "groups.roblox.com",
            "inventory.roblox.com",
            "itemconfiguration.roblox.com",
            "localizationtables.roblox.com",
            "midas.roblox.com",
            "notifications.roblox.com",
            "premiumfeatures.roblox.com",
            "privatemessages.roblox.com",
            "publish.roblox.com",
            "scs.roblox.com",
            "share.roblox.com",
            "textfilter.roblox.com",
            "trades.roblox.com",
            "translationroles.roblox.com",
            "translations.roblox.com",
            "twostepverification.roblox.com",
            "usersearch.roblox.com",
            "users.roblox.com",
            "voice.roblox.com",
            "devforum.roblox.com",
            "create.roblox.com",
            "developer.roblox.com",
            "corp.roblox.com",
            "help.roblox.com",
            "en.help.roblox.com",
            "titanium.roblox.com",
            "edge-term4.roblox.com",
            "nspx1.roblox.com",
            "nspx2.roblox.com",
            "nspx3.roblox.com",
            "nspx4.roblox.com",
            "setup.rbxcdn.com",
            "roblox-setup.rbxcdn.com",
            "rbxcdn.com",
            "rbxcdn.net",
            "c0.rbxcdn.com",
            "c1.rbxcdn.com",
            "c2.rbxcdn.com",
            "c3.rbxcdn.com",
            "c4.rbxcdn.com",
            "c5.rbxcdn.com",
            "c6.rbxcdn.com",
            "c7.rbxcdn.com",
            "t0.rbxcdn.com",
            "t1.rbxcdn.com",
            "t2.rbxcdn.com",
            "t3.rbxcdn.com",
            "t4.rbxcdn.com",
            "t5.rbxcdn.com",
            "t6.rbxcdn.com",
            "t7.rbxcdn.com",
            "tr.rbxcdn.com",
            "images.rbxcdn.com",
            "static.rbxcdn.com",
            "js.rbxcdn.com",
            "css.rbxcdn.com",
            "robloxcdn.com",
            "rbxinfra.net",
            "rbx.com",
            "rblx.co",
            "rblx.org",
            "rblx.game",
            "rblx.social",
            "rblx.wtf",
            "blox.com",
            "robloxlabs.com",
            "robloxdev.com",
            "robloxassets.com",
            "robloxlibrary.com",
            "robloxlocalization.com",
            "robloxqa.com",
            "robloxstudios.com",
            "roblox.net",
            "roblox.us",
            "roblox.gg",
            "roblox.org",
            "roblox.co.uk",
            "roblox.cn",
            "roblox.qq.com",
            "robloxapp.vnggames.com",
            "simulprod.com",
            "simulpong.com",
            "rover.na.roblox.net",
            "rover.link",
            "arkoselabs.com",
            "roblox-api.arkoselabs.com",
            "cdn.arkoselabs.com",
            "client-api.arkoselabs.com",
            "funcaptcha.com",
            "kaptcha.com",
            "withpersona.com",
            "persona.com",
            "hcaptcha.com",
            "rbxtrk.com",
            "rbxlabs.com",
            "guilded.gg",
            "recaptcha.net",
            "challenges.cloudflare.com",
            "awswaf.com",
            "datadome.co",
            "px-cloud.net",
            "captcha-delivery.com",
            "s3.amazonaws.com",
            "s3.us-east-2.amazonaws.com",
        ],
        exes: &[
            "RobloxPlayerBeta.exe",
            "RobloxPlayerInstaller.exe",
            "RobloxStudioBeta.exe",
            "RobloxPlayerLauncher.exe",
            "RobloxStudioLauncherBeta.exe",
            "RobloxCrashHandler.exe",
            "Roblox.exe",
            "RobloxPlayer.exe",
            "RobloxStudio.exe",
            "RobloxStudioInstaller.exe",
            "Windows10Universal.exe",
        ],
        voice: false,
        probe: "https://www.roblox.com",
    },
];

/// Normalizes a URL or domain string into a clean base hostname.
pub fn normalize(raw: &str) -> Option<String> {
    let s = raw.trim().to_lowercase();
    let s = s.split("://").last()?;
    let host = s.split(['/', '?', '#', ':']).next()?;
    let host = host.trim_start_matches("www.").trim_matches('.');
    let ok = host.contains('.') && host.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '.');
    ok.then(|| host.to_string())
}

/// Extracts a clean alphanumeric slug and base domain from any URL, domain, or exe name.
pub fn extract_slug_and_base(raw: &str) -> (String, String) {
    let s = raw.trim().to_lowercase();
    let s = s.split("://").last().unwrap_or(&s);
    let s = s.split(['/', '?', '#', ':']).next().unwrap_or(s);
    let s = s.trim_start_matches("www.").trim_matches('.');
    
    // Strip common binary executable extensions
    let clean = s.strip_suffix(".exe")
        .or_else(|| s.strip_suffix(".msi"))
        .or_else(|| s.strip_suffix(".bat"))
        .or_else(|| s.strip_suffix(".cmd"))
        .unwrap_or(s);
    
    // If it has a known domain extension (.com, .net, .org, .gg, .io, .tv, etc.)
    if clean.contains('.') {
        let parts: Vec<&str> = clean.split('.').collect();
        if parts.len() >= 3 {
            let last = parts[parts.len() - 1];
            let second_last = parts[parts.len() - 2];
            if ["com", "co", "org", "net", "edu", "gov"].contains(&second_last) {
                let tld = format!("{}.{}", second_last, last);
                let sld = parts[parts.len() - 3];
                let slug = sld.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '-').collect::<String>();
                let base_domain = format!("{}.{}", slug, tld);
                return (slug, base_domain);
            }
        }
        if parts.len() >= 2 {
            let tld = parts.last().unwrap();
            let sld = parts[parts.len() - 2];
            let slug = sld.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '-').collect::<String>();
            let base_domain = format!("{}.{}", slug, tld);
            return (slug, base_domain);
        }
    }
    
    // Otherwise treat as plain brand or executable name
    let slug = clean
        .replace("-win64-shipping", "")
        .replace("-win64", "")
        .replace("_x64", "")
        .replace("_x86", "")
        .replace("client", "")
        .replace("launcher", "");
    let mut slug = slug.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '-').collect::<String>();
    if slug.is_empty() {
        slug = clean.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '-').collect::<String>();
    }
    let base_domain = format!("{}.com", slug);
    (slug, base_domain)
}

/// Automatically and intelligently predicts, discovers and generates all possible
/// domain variations, subdomains, CDNs, APIs, socket gateways and infrastructure endpoints
/// for any given app, game, URL, or executable name.
pub fn expand_domains(raw: &str) -> Vec<String> {
    let (slug, base_domain) = extract_slug_and_base(raw);
    if slug.is_empty() {
        return Vec::new();
    }

    let mut set: BTreeSet<String> = BTreeSet::new();

    // 0) Always include the normalized raw domain if it was already valid
    if let Some(norm) = normalize(raw) {
        set.insert(norm);
    }

    // 1) Known presets from apps.rs (Valorant, CS2, LoL, Steam, Epic, Roblox, Spotify, Assetto Corsa, EA, Blizzard, etc.)
    let preset_domains = crate::apps::get_preset_domains_for_app(&slug, raw);
    for d in preset_domains {
        if let Some(norm) = normalize(&d) {
            set.insert(norm);
        }
    }

    // 2) Known built-in service catalog checks
    let lower = raw.trim().to_lowercase();
    if lower.contains("discord") || slug == "discord" {
        set.extend(vec![
            "discord.com", "discord.gg", "discord.co", "discord.media",
            "discord.new", "discord.gift", "discord.gifts", "discord.dev",
            "discord.design", "discord.store", "discord.tools", "discordapp.com",
            "discordapp.net", "discordcdn.com", "discordstatus.com", "discordsays.com",
            "discordsez.com", "discordactivities.com", "discord-activities.com", "dis.gd",
            "gateway.discord.gg", "status.discord.com", "updates.discord.com", "dl.discordapp.net",
            "cdn.discordapp.com", "media.discordapp.net", "attachments.discord.net",
        ].into_iter().map(String::from));
    } else if lower.contains("roblox") || slug == "roblox" || slug == "rbx" {
        set.extend(vec![
            "roblox.com", "www.roblox.com", "web.roblox.com", "m.roblox.com",
            "api.roblox.com", "apis.roblox.com", "auth.roblox.com", "games.roblox.com", "gamejoin.roblox.com",
            "clientsettings.roblox.com", "clientsettingscdn.roblox.com", "setup.roblox.com",
            "setup.rbxcdn.com", "rbxcdn.com", "rbxcdn.net", "robloxcdn.com", "rbx.com",
            "roblox.net", "roblox.gg", "guilded.gg",
        ].into_iter().map(String::from));
    } else if lower.contains("youtube") || slug == "youtube" {
        set.extend(vec![
            "youtube.com", "youtu.be", "googlevideo.com", "ytimg.com", "ggpht.com",
            "youtube-nocookie.com", "youtubei.googleapis.com",
        ].into_iter().map(String::from));
    } else if lower.contains("kick") || slug == "kick" {
        set.extend(vec!["kick.com", "kickcdn.com", "stream.kick.com", "chat.kick.com"].into_iter().map(String::from));
    } else if lower.contains("twitch") || slug == "twitch" {
        set.extend(vec!["twitch.tv", "ttvnw.net", "jtvnw.net", "twitchcdn.net"].into_iter().map(String::from));
    } else if lower.contains("spotify") || slug == "spotify" {
        set.extend(vec!["spotify.com", "scdn.co", "spotifycdn.com", "spoti.fi", "spclient.wg.spotify.com"].into_iter().map(String::from));
    } else if lower.contains("steam") || slug == "steam" {
        set.extend(vec!["steampowered.com", "steamcommunity.com", "steamstatic.com", "steamcontent.com", "steamserver.net"].into_iter().map(String::from));
    } else if lower.contains("epic") || lower.contains("fortnite") {
        set.extend(vec!["epicgames.com", "unrealengine.com", "fortnite.com", "ol.epicgames.com", "epicgames.dev"].into_iter().map(String::from));
    } else if lower.contains("minecraft") || slug == "minecraft" {
        set.extend(vec!["minecraft.net", "mojang.com", "minecraftservices.com", "launcher.mojang.com"].into_iter().map(String::from));
    } else if lower.contains("riot") || lower.contains("valorant") {
        set.extend(vec!["riotgames.com", "leagueoflegends.com", "playvalorant.com", "riotcdn.net", "pvp.net"].into_iter().map(String::from));
    } else if lower.contains("telegram") || slug == "telegram" {
        set.extend(vec!["telegram.org", "telegram.me", "t.me", "telesco.pe"].into_iter().map(String::from));
    } else if lower.contains("tiktok") || slug == "tiktok" {
        set.extend(vec!["tiktok.com", "tiktokcdn.com", "tiktokv.com", "musical.ly"].into_iter().map(String::from));
    } else if lower.contains("reddit") || slug == "reddit" {
        set.extend(vec!["reddit.com", "redd.it", "redditstatic.com", "redditmedia.com"].into_iter().map(String::from));
    } else if lower.contains("github") || slug == "github" {
        set.extend(vec!["github.com", "githubusercontent.com", "githubassets.com"].into_iter().map(String::from));
    }

    // 3) Intelligent Generative Subdomains & Service Endpoints for the base domain
    let subdomains = [
        "", "www", "app", "web", "m",
        "api", "api-v1", "api-v2", "apis", "gateway", "backend", "graphql", "router",
        "ws", "wss", "socket", "realtime", "stream", "live", "chat", "voice",
        "auth", "login", "account", "accounts", "id", "sso", "oauth",
        "cdn", "cdn1", "cdn2", "static", "assets", "media", "images", "img", "content",
        "download", "downloads", "dl", "update", "updates", "patch", "launcher", "client",
        "game", "games", "play", "server", "servers", "status", "version"
    ];

    for sub in subdomains {
        let domain = if sub.is_empty() {
            base_domain.clone()
        } else {
            format!("{}.{}", sub, base_domain)
        };
        set.insert(domain);
    }

    // 4) Dedicated Brand CDN & Infrastructure Permutations
    let brand_cdns = [
        format!("{}cdn.com", slug),
        format!("{}-cdn.com", slug),
        format!("{}static.com", slug),
        format!("{}assets.com", slug),
        format!("{}media.com", slug),
        format!("{}app.com", slug),
        format!("{}app.net", slug),
        format!("{}api.com", slug),
        format!("{}services.com", slug),
    ];
    for b in brand_cdns {
        set.insert(b);
    }

    // 5) Multi-TLD Predictive Variations (.net, .gg, .io, .tv, .app, .co)
    let alt_tlds = ["com", "net", "gg", "io", "tv", "app", "co"];
    for tld in alt_tlds {
        let alt_base = format!("{}.{}", slug, tld);
        if alt_base != base_domain {
            set.insert(alt_base.clone());
            set.insert(format!("api.{}", alt_base));
            set.insert(format!("cdn.{}", alt_base));
            set.insert(format!("auth.{}", alt_base));
            set.insert(format!("ws.{}", alt_base));
        }
    }

    // Filter to ensure only strictly valid domain strings remain
    let mut result: Vec<String> = set
        .into_iter()
        .filter(|d| {
            d.contains('.')
                && !d.starts_with('.')
                && !d.ends_with('.')
                && !d.starts_with('-')
                && !d.ends_with('-')
                && !d.contains("..")
                && d.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '.')
        })
        .collect();

    result.sort();
    result
}

fn enabled<'a>(cfg: &'a Config) -> impl Iterator<Item = &'static AppProfile> + 'a {
    APPS.iter().filter(move |p| cfg.enabled_apps.iter().any(|e| e == p.id))
}

pub fn collect_domains(cfg: &Config) -> Vec<String> {
    let mut set: BTreeSet<String> = enabled(cfg)
        .flat_map(|p| p.domains.iter().map(|d| d.to_string()))
        .collect();

    // Custom user-added platforms (e.g. added via + Ekle)
    for plat in &cfg.custom_platforms {
        if cfg.enabled_apps.iter().any(|e| e == &plat.id) {
            for d in &plat.domains {
                if let Some(norm) = normalize(d) {
                    set.insert(norm);
                }
            }
        }
    }

    // Custom domains
    set.extend(cfg.custom_domains.iter().filter_map(|d| normalize(d)));
    set.into_iter().collect()
}

#[allow(dead_code)]
pub fn needs_voice(cfg: &Config) -> bool {
    enabled(cfg).any(|p| p.voice)
}

pub fn probe_urls(cfg: &Config) -> Vec<String> {
    let mut v: Vec<String> = enabled(cfg).map(|p| p.probe.to_string()).take(2).collect();
    if v.is_empty() {
        v.push("https://discord.com".into());
    }
    v
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct DomainSuggestionResult {
    pub input: String,
    pub base_domain: String,
    pub display_name: String,
    pub suggestions: Vec<String>,
    pub explanation: String,
}

pub fn get_domain_suggestions(raw: &str) -> DomainSuggestionResult {
    let raw_trimmed = raw.trim();
    let norm = normalize(raw_trimmed).unwrap_or_else(|| raw_trimmed.to_lowercase());
    let lower = norm.to_lowercase();

    // 1) Check if matches a known profile
    for app in APPS {
        let is_match = app.id == lower
            || app.name.to_lowercase() == lower
            || app.domains.iter().any(|d| *d == lower || lower.ends_with(d));
        if is_match {
            let mut list: Vec<String> = app.domains.iter().map(|s| s.to_string()).collect();
            list.sort();
            list.dedup();
            let count = list.len();
            return DomainSuggestionResult {
                input: raw_trimmed.to_string(),
                base_domain: norm,
                display_name: app.name.to_string(),
                suggestions: list,
                explanation: format!(
                    "'{name}' servisi için {count} adet resmi API, CDN ve oyun/medya sunucu adresi bulundu. Modern istemciler bağlantı kurarken bu sunucuları eşzamanlı kullanır. Engellemeleri tamamen aşmak için hepsinin birlikte eklenmesi önerilir.",
                    name = app.name,
                    count = count
                ),
            };
        }
    }

    // 2) Expand generic domain
    let mut expanded = expand_domains(&norm);
    if !expanded.contains(&norm) {
        expanded.insert(0, norm.clone());
    }
    expanded.sort();
    expanded.dedup();
    let count = expanded.len();

    DomainSuggestionResult {
        input: raw_trimmed.to_string(),
        base_domain: norm.clone(),
        display_name: norm.clone(),
        suggestions: expanded,
        explanation: format!(
            "'{base}' adresi ve bağlı servisleri için {count} adet ilişkili CDN, API, oturum ve içerik sunucusu tespit edildi. Sayfa yükleme hızını artırmak ve içerik blokajlarını önlemek için bu adreslerin eklenmesi önerilir.",
            base = norm,
            count = count
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_expand_domains_plain_name() {
        let domains = expand_domains("kick");
        assert!(domains.contains(&"kick.com".to_string()));
        assert!(domains.contains(&"api.kick.com".to_string()));
        assert!(domains.contains(&"cdn.kick.com".to_string()));
        assert!(domains.contains(&"auth.kick.com".to_string()));
        assert!(domains.contains(&"kickcdn.com".to_string()));
        assert!(domains.len() >= 20);
    }

    #[test]
    fn test_expand_domains_exe_file() {
        let domains = expand_domains("SuperRacer.exe");
        assert!(domains.contains(&"superracer.com".to_string()));
        assert!(domains.contains(&"api.superracer.com".to_string()));
        assert!(domains.contains(&"cdn.superracer.com".to_string()));
        assert!(domains.contains(&"game.superracer.com".to_string()));
        assert!(domains.contains(&"superracercdn.com".to_string()));
        assert!(domains.len() >= 20);
    }

    #[test]
    fn test_expand_domains_custom_url() {
        let domains = expand_domains("https://play.mycustomgame.io/match/123");
        assert!(domains.contains(&"mycustomgame.io".to_string()));
        assert!(domains.contains(&"api.mycustomgame.io".to_string()));
        assert!(domains.contains(&"game.mycustomgame.io".to_string()));
        assert!(domains.contains(&"mycustomgame.com".to_string()));
        assert!(domains.len() >= 20);
    }

    #[test]
    fn test_expand_domains_known_game() {
        let domains = expand_domains("valorant.exe");
        assert!(domains.contains(&"riotgames.com".to_string()));
        assert!(domains.contains(&"pvp.net".to_string()));
        assert!(domains.contains(&"playvalorant.com".to_string()));
    }
}

