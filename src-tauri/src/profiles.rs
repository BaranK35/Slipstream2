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
    AppProfile {
        id: "youtube",
        name: "YouTube",
        category: "app",
        tag: "Video, müzik ve resimler",
        icon: "▶️",
        domains: &[
            "youtube.com", "youtu.be", "googlevideo.com", "ytimg.com",
            "ggpht.com", "youtube-nocookie.com", "youtubei.googleapis.com",
        ],
        exes: &[],
        voice: false,
        probe: "https://www.youtube.com",
    },
    AppProfile {
        id: "x",
        name: "X (Twitter)",
        category: "app",
        tag: "Akış, tweet ve medya",
        icon: "𝕏",
        domains: &["twitter.com", "x.com", "twimg.com", "t.co"],
        exes: &[],
        voice: false,
        probe: "https://x.com",
    },
    AppProfile {
        id: "meta",
        name: "Instagram & Facebook",
        category: "app",
        tag: "Gönderiler, reels, WhatsApp ve CDN",
        icon: "📸",
        domains: &[
            "instagram.com", "cdninstagram.com", "fbcdn.net", "facebook.com",
            "fbsbx.com", "whatsapp.com", "whatsapp.net",
        ],
        exes: &[],
        voice: false,
        probe: "https://www.instagram.com",
    },
    AppProfile {
        id: "spotify",
        name: "Spotify",
        category: "app",
        tag: "Müzik yayını, web oynatıcı ve API",
        icon: "🎵",
        domains: &["spotify.com", "scdn.co", "spotifycdn.com", "spoti.fi"],
        exes: &["Spotify.exe"],
        voice: false,
        probe: "https://open.spotify.com",
    },
    AppProfile {
        id: "twitch",
        name: "Twitch",
        category: "app",
        tag: "Canlı yayınlar ve sohbet",
        icon: "🟣",
        domains: &["twitch.tv", "ttvnw.net", "jtvnw.net", "twitchcdn.net"],
        exes: &[],
        voice: false,
        probe: "https://www.twitch.tv",
    },
    AppProfile {
        id: "reddit",
        name: "Reddit",
        category: "app",
        tag: "Gönderiler, medya ve yorumlar",
        icon: "🤖",
        domains: &["reddit.com", "redd.it", "redditmedia.com", "redditstatic.com"],
        exes: &[],
        voice: false,
        probe: "https://www.reddit.com",
    },
    AppProfile {
        id: "kick",
        name: "Kick",
        category: "app",
        tag: "Canlı yayın ve sohbet",
        icon: "🟢",
        domains: &["kick.com", "kickcdn.com"],
        exes: &[],
        voice: false,
        probe: "https://kick.com",
    },
    AppProfile {
        id: "tiktok",
        name: "TikTok",
        category: "app",
        tag: "Kısa videolar ve akış",
        icon: "📱",
        domains: &["tiktok.com", "tiktokcdn.com", "tiktokv.com", "musical.ly"],
        exes: &[],
        voice: false,
        probe: "https://www.tiktok.com",
    },
    AppProfile {
        id: "wattpad",
        name: "Wattpad",
        category: "app",
        tag: "Hikaye ve topluluk",
        icon: "📖",
        domains: &["wattpad.com", "wp-assets.emu.net"],
        exes: &[],
        voice: false,
        probe: "https://www.wattpad.com",
    },
    AppProfile {
        id: "sharing",
        name: "Pastebin & Medium & Imgur",
        category: "app",
        tag: "Metin, blog ve görsel paylaşım",
        icon: "📝",
        domains: &[
            "pastebin.com", "medium.com", "imgur.com", "i.imgur.com",
            "linktr.ee", "archiveofourown.org", "tumblr.com",
            "wordpress.com", "wp.com", "blogspot.com", "blogger.com",
            "quora.com", "deviantart.com", "flickr.com", "pinterest.com",
            "soundcloud.com", "sndcdn.com", "vimeo.com", "vimeocdn.com",
            "dailymotion.com", "dmcdn.net",
        ],
        exes: &[],
        voice: false,
        probe: "https://pastebin.com",
    },
    AppProfile {
        id: "telegram",
        name: "Telegram & Signal",
        category: "app",
        tag: "Mesajlaşma, kanallar ve ses",
        icon: "✈️",
        domains: &["telegram.org", "telegram.me", "t.me", "signal.org"],
        exes: &["Telegram.exe"],
        voice: true,
        probe: "https://telegram.org",
    },
    AppProfile {
        id: "dev_tools",
        name: "Geliştirici & Bilgi (GitHub / Wiki)",
        category: "app",
        tag: "GitHub, GitLab, StackOverflow, Wikipedia, Tor",
        icon: "💻",
        domains: &[
            "github.com", "githubusercontent.com", "githubassets.com",
            "gitlab.com", "stackoverflow.com", "stackexchange.com", "sstatic.net",
            "wikipedia.org", "wikimedia.org", "archive.org",
            "duckduckgo.com", "proton.me", "protonmail.com", "tutanota.com", "torproject.org",
        ],
        exes: &[],
        voice: false,
        probe: "https://github.com",
    },
    AppProfile {
        id: "news_media",
        name: "Haber & Medya",
        category: "app",
        tag: "BBC, DW, VOA, Reuters, Guardian, NYTimes",
        icon: "📰",
        domains: &[
            "bbc.com", "bbc.co.uk", "bbci.co.uk", "dw.com", "voanews.com",
            "rferl.org", "reuters.com", "theguardian.com", "guim.co.uk",
            "nytimes.com", "nyt.com",
        ],
        exes: &[],
        voice: false,
        probe: "https://www.bbc.com",
    },
    AppProfile {
        id: "eksisozluk",
        name: "Ekşi Sözlük",
        category: "app",
        tag: "Gündem ve entryler",
        icon: "🟩",
        domains: &[
            "eksisozluk.com", "eksisozluk1923.com", "eksisozluk111.com", "eksisozluk2023.com",
        ],
        exes: &[],
        voice: false,
        probe: "https://eksisozluk.com",
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
    AppProfile {
        id: "steam",
        name: "Steam",
        category: "game",
        tag: "Mağaza, topluluk, sohbet ve indirme",
        icon: "🕹️",
        domains: &[
            "steampowered.com", "steamcommunity.com", "steamstatic.com",
            "steamcontent.com", "steamserver.net", "steam-chat.com",
        ],
        exes: &["steam.exe"],
        voice: false,
        probe: "https://store.steampowered.com",
    },
    AppProfile {
        id: "epicgames",
        name: "Epic Games",
        category: "game",
        tag: "Epic Store, Fortnite, Unreal",
        icon: "🎯",
        domains: &[
            "epicgames.com", "unrealengine.com", "fortnite.com",
            "ol.epicgames.com", "launcher-public-service-prod06.ol.epicgames.com",
        ],
        exes: &["EpicGamesLauncher.exe"],
        voice: false,
        probe: "https://store.epicgames.com",
    },
    AppProfile {
        id: "minecraft",
        name: "Minecraft",
        category: "game",
        tag: "Oturum, skin, doku ve sunucular",
        icon: "⛏️",
        domains: &[
            "minecraft.net", "mojang.com", "minecraftservices.com",
            "launcher.mojang.com", "authlib.injector.moe",
        ],
        exes: &["MinecraftLauncher.exe", "javaw.exe"],
        voice: false,
        probe: "https://www.minecraft.net",
    },
    AppProfile {
        id: "riotgames",
        name: "Riot Games (LoL / Valorant)",
        category: "game",
        tag: "Riot istemcisi, LoL, Valorant",
        icon: "⚔️",
        domains: &[
            "riotgames.com", "leagueoflegends.com", "playvalorant.com",
            "riotcdn.net", "pvp.net",
        ],
        exes: &["RiotClientServices.exe", "VALORANT.exe", "LeagueClient.exe"],
        voice: false,
        probe: "https://playvalorant.com",
    },
    AppProfile {
        id: "rockstar_blizzard_ea",
        name: "Rockstar, Blizzard & EA",
        category: "game",
        tag: "GTA V, Battle.net, EA App, Ubisoft",
        icon: "🎲",
        domains: &[
            "rockstargames.com", "socialclub.rockstargames.com",
            "blizzard.com", "battle.net", "bnetcmsus-a.akamaihd.net",
            "ea.com", "origin.com", "ubisoft.com", "uplay.com",
        ],
        exes: &["RockstarGamesLauncher.exe", "Battle.net.exe", "EADesktop.exe"],
        voice: false,
        probe: "https://www.rockstargames.com",
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

/// Automatically expands a domain, URL or app/game name to include all subdomains and service endpoints.
pub fn expand_domains(raw: &str) -> Vec<String> {
    let lower = raw.trim().to_lowercase();
    let mut list = Vec::new();

    // Check if user entered a known service name or keyword
    if lower.contains("discord") {
        list.extend(vec![
            "discord.com", "discord.gg", "discord.co", "discord.media",
            "discord.new", "discord.gift", "discord.gifts", "discord.dev",
            "discord.design", "discord.store", "discord.tools", "discordapp.com",
            "discordapp.net", "discordcdn.com", "discordstatus.com", "discordsays.com",
            "discordsez.com", "discordactivities.com", "discord-activities.com", "dis.gd",
            "gateway.discord.gg", "status.discord.com", "updates.discord.com", "dl.discordapp.net",
        ].into_iter().map(String::from));
    } else if lower.contains("roblox") || lower == "rbx" {
        list.extend(vec![
            "roblox.com", "www.roblox.com", "web.roblox.com", "m.roblox.com",
            "api.roblox.com", "apis.roblox.com", "auth.roblox.com",
            "games.roblox.com", "gamejoin.roblox.com", "matchmaking.roblox.com",
            "assetdelivery.roblox.com", "clientsettings.roblox.com",
            "clientsettingscdn.roblox.com", "clientsettings.api.roblox.com",
            "versioncompatibility.api.roblox.com", "setup.roblox.com",
            "avatar.roblox.com", "chat.roblox.com", "friends.roblox.com",
            "presence.roblox.com", "economy.roblox.com", "realtime.roblox.com",
            "realtime-signalr.roblox.com", "thumbnails.roblox.com",
            "locale.roblox.com", "metrics.roblox.com", "ncs.roblox.com",
            "client-telemetry.roblox.com", "ecsv2.roblox.com", "abtesting.roblox.com",
            "ephemeralcounters.api.roblox.com", "accountinformation.roblox.com",
            "accountsettings.roblox.com", "badges.roblox.com", "contacts.roblox.com",
            "economycreatorstats.roblox.com", "engagementpayouts.roblox.com",
            "followings.roblox.com", "gameinternationalization.roblox.com",
            "groups.roblox.com", "inventory.roblox.com", "itemconfiguration.roblox.com",
            "localizationtables.roblox.com", "midas.roblox.com", "notifications.roblox.com",
            "premiumfeatures.roblox.com", "privatemessages.roblox.com", "publish.roblox.com",
            "scs.roblox.com", "share.roblox.com", "textfilter.roblox.com", "trades.roblox.com",
            "translationroles.roblox.com", "translations.roblox.com",
            "twostepverification.roblox.com", "usersearch.roblox.com", "users.roblox.com",
            "voice.roblox.com", "devforum.roblox.com", "create.roblox.com",
            "developer.roblox.com", "corp.roblox.com", "help.roblox.com",
            "en.help.roblox.com", "titanium.roblox.com", "edge-term4.roblox.com",
            "nspx1.roblox.com", "nspx2.roblox.com", "nspx3.roblox.com", "nspx4.roblox.com",
            "setup.rbxcdn.com", "roblox-setup.rbxcdn.com", "rbxcdn.com", "rbxcdn.net",
            "c0.rbxcdn.com", "c1.rbxcdn.com", "c2.rbxcdn.com", "c3.rbxcdn.com",
            "c4.rbxcdn.com", "c5.rbxcdn.com", "c6.rbxcdn.com", "c7.rbxcdn.com",
            "t0.rbxcdn.com", "t1.rbxcdn.com", "t2.rbxcdn.com", "t3.rbxcdn.com",
            "t4.rbxcdn.com", "t5.rbxcdn.com", "t6.rbxcdn.com", "t7.rbxcdn.com",
            "tr.rbxcdn.com", "images.rbxcdn.com", "static.rbxcdn.com", "js.rbxcdn.com",
            "css.rbxcdn.com", "robloxcdn.com", "rbxinfra.net", "rbx.com",
            "rblx.co", "rblx.org", "rblx.game", "rblx.social", "rblx.wtf",
            "blox.com", "robloxlabs.com", "robloxdev.com", "robloxassets.com",
            "robloxlibrary.com", "robloxlocalization.com", "robloxqa.com",
            "robloxstudios.com", "roblox.net", "roblox.us", "roblox.gg", "roblox.org",
            "roblox.co.uk", "roblox.cn", "roblox.qq.com", "robloxapp.vnggames.com",
            "simulprod.com", "simulpong.com", "rover.na.roblox.net", "rover.link",
            "arkoselabs.com", "roblox-api.arkoselabs.com", "cdn.arkoselabs.com",
            "client-api.arkoselabs.com", "funcaptcha.com", "kaptcha.com",
            "withpersona.com", "persona.com", "hcaptcha.com", "rbxtrk.com",
            "rbxlabs.com", "guilded.gg", "recaptcha.net", "challenges.cloudflare.com",
            "awswaf.com", "datadome.co", "px-cloud.net", "captcha-delivery.com",
            "s3.amazonaws.com", "s3.us-east-2.amazonaws.com",
        ].into_iter().map(String::from));
    } else if lower.contains("wattpad") {
        list.extend(vec!["wattpad.com", "wp-assets.emu.net"].into_iter().map(String::from));
    } else if lower.contains("pastebin") {
        list.extend(vec!["pastebin.com"].into_iter().map(String::from));
    } else if lower.contains("medium") {
        list.extend(vec!["medium.com"].into_iter().map(String::from));
    } else if lower.contains("imgur") {
        list.extend(vec!["imgur.com", "i.imgur.com"].into_iter().map(String::from));
    } else if lower.contains("linktr") {
        list.extend(vec!["linktr.ee"].into_iter().map(String::from));
    } else if lower.contains("archiveofourown") || lower.contains("ao3") {
        list.extend(vec!["archiveofourown.org"].into_iter().map(String::from));
    } else if lower.contains("tumblr") {
        list.extend(vec!["tumblr.com"].into_iter().map(String::from));
    } else if lower.contains("wordpress") || lower.contains("wp") {
        list.extend(vec!["wordpress.com", "wp.com"].into_iter().map(String::from));
    } else if lower.contains("blogspot") || lower.contains("blogger") {
        list.extend(vec!["blogspot.com", "blogger.com"].into_iter().map(String::from));
    } else if lower.contains("quora") {
        list.extend(vec!["quora.com"].into_iter().map(String::from));
    } else if lower.contains("deviantart") {
        list.extend(vec!["deviantart.com", "deviantart.net"].into_iter().map(String::from));
    } else if lower.contains("flickr") {
        list.extend(vec!["flickr.com", "staticflickr.com"].into_iter().map(String::from));
    } else if lower.contains("pinterest") {
        list.extend(vec!["pinterest.com", "pinimg.com"].into_iter().map(String::from));
    } else if lower.contains("soundcloud") {
        list.extend(vec!["soundcloud.com", "sndcdn.com"].into_iter().map(String::from));
    } else if lower.contains("spotify") {
        list.extend(vec!["spotify.com", "scdn.co", "spotifycdn.com", "spoti.fi"].into_iter().map(String::from));
    } else if lower.contains("youtube") || lower.contains("youtu.be") {
        list.extend(vec!["youtube.com", "youtu.be", "googlevideo.com", "ytimg.com", "ggpht.com"].into_iter().map(String::from));
    } else if lower.contains("twitch") {
        list.extend(vec!["twitch.tv", "ttvnw.net", "jtvnw.net", "twitchcdn.net"].into_iter().map(String::from));
    } else if lower.contains("kick") {
        list.extend(vec!["kick.com", "kickcdn.com"].into_iter().map(String::from));
    } else if lower.contains("vimeo") {
        list.extend(vec!["vimeo.com", "vimeocdn.com"].into_iter().map(String::from));
    } else if lower.contains("dailymotion") {
        list.extend(vec!["dailymotion.com", "dmcdn.net"].into_iter().map(String::from));
    } else if lower.contains("twitter") || lower == "x" || lower.contains("x.com") {
        list.extend(vec!["x.com", "twitter.com", "twimg.com", "t.co"].into_iter().map(String::from));
    } else if lower.contains("instagram") || lower.contains("facebook") || lower.contains("meta") {
        list.extend(vec!["instagram.com", "cdninstagram.com", "facebook.com", "fbcdn.net", "fbsbx.com"].into_iter().map(String::from));
    } else if lower.contains("whatsapp") {
        list.extend(vec!["whatsapp.com", "whatsapp.net"].into_iter().map(String::from));
    } else if lower.contains("tiktok") {
        list.extend(vec!["tiktok.com", "tiktokcdn.com", "tiktokv.com", "musical.ly"].into_iter().map(String::from));
    } else if lower.contains("reddit") {
        list.extend(vec!["reddit.com", "redd.it", "redditstatic.com", "redditmedia.com"].into_iter().map(String::from));
    } else if lower.contains("telegram") {
        list.extend(vec!["telegram.org", "telegram.me", "t.me"].into_iter().map(String::from));
    } else if lower.contains("signal") {
        list.extend(vec!["signal.org"].into_iter().map(String::from));
    } else if lower.contains("wiki") {
        list.extend(vec!["wikipedia.org", "wikimedia.org"].into_iter().map(String::from));
    } else if lower.contains("archive.org") {
        list.extend(vec!["archive.org"].into_iter().map(String::from));
    } else if lower.contains("github") {
        list.extend(vec!["github.com", "githubusercontent.com", "githubassets.com"].into_iter().map(String::from));
    } else if lower.contains("gitlab") {
        list.extend(vec!["gitlab.com"].into_iter().map(String::from));
    } else if lower.contains("stackoverflow") || lower.contains("stackexchange") {
        list.extend(vec!["stackoverflow.com", "stackexchange.com", "sstatic.net"].into_iter().map(String::from));
    } else if lower.contains("duckduckgo") {
        list.extend(vec!["duckduckgo.com"].into_iter().map(String::from));
    } else if lower.contains("proton") {
        list.extend(vec!["proton.me", "protonmail.com"].into_iter().map(String::from));
    } else if lower.contains("tutanota") {
        list.extend(vec!["tutanota.com"].into_iter().map(String::from));
    } else if lower.contains("torproject") || lower.contains("tor") {
        list.extend(vec!["torproject.org"].into_iter().map(String::from));
    } else if lower.contains("steam") {
        list.extend(vec!["steampowered.com", "steamcommunity.com", "steamstatic.com", "steamcontent.com", "steamserver.net", "steam-chat.com"].into_iter().map(String::from));
    } else if lower.contains("epic") || lower.contains("fortnite") {
        list.extend(vec!["epicgames.com", "unrealengine.com", "fortnite.com", "ol.epicgames.com"].into_iter().map(String::from));
    } else if lower.contains("minecraft") || lower.contains("mojang") {
        list.extend(vec!["minecraft.net", "mojang.com", "minecraftservices.com", "launcher.mojang.com"].into_iter().map(String::from));
    } else if lower.contains("riot") || lower.contains("valorant") || lower.contains("leagueoflegends") {
        list.extend(vec!["riotgames.com", "leagueoflegends.com", "playvalorant.com", "riotcdn.net", "pvp.net"].into_iter().map(String::from));
    } else if lower.contains("rockstar") || lower.contains("gta") {
        list.extend(vec!["rockstargames.com", "socialclub.rockstargames.com"].into_iter().map(String::from));
    } else if lower.contains("blizzard") || lower.contains("battle.net") {
        list.extend(vec!["blizzard.com", "battle.net", "bnetcmsus-a.akamaihd.net"].into_iter().map(String::from));
    } else if lower.contains("ea") || lower.contains("origin") {
        list.extend(vec!["ea.com", "origin.com"].into_iter().map(String::from));
    } else if lower.contains("ubisoft") || lower.contains("uplay") {
        list.extend(vec!["ubisoft.com", "uplay.com"].into_iter().map(String::from));
    }

    // Also extract base domain and generate full family
    if let Some(base) = normalize(raw) {
        list.extend(vec![
            base.clone(),
            format!("www.{base}"),
            format!("api.{base}"),
            format!("cdn.{base}"),
            format!("auth.{base}"),
            format!("static.{base}"),
            format!("assets.{base}"),
            format!("media.{base}"),
            format!("login.{base}"),
            format!("app.{base}"),
            format!("download.{base}"),
            format!("updates.{base}"),
            format!("status.{base}"),
            format!("launcher.{base}"),
            format!("game.{base}"),
            format!("play.{base}"),
        ]);
    }

    list.sort();
    list.dedup();
    list
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
