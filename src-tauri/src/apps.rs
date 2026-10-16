use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::Path;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ScannedApp {
    pub id: String,
    pub name: String,
    pub exe_name: String,
    pub exe_path: Option<String>,
    pub category: String, // "game" or "app"
    pub is_running: bool,
    pub icon: String,
    pub domains: Vec<String>,
}

/// Automatically assigns known official network domains/endpoints for recognized apps & games.
/// This runs silently behind the scenes without exposing technical URLs in the UI.
pub fn get_preset_domains_for_app(name: &str, exe_name: &str) -> Vec<String> {
    let name_l = name.to_lowercase();
    let exe_l = exe_name.to_lowercase();

    if name_l.contains("discord") || exe_l.contains("discord") {
        vec![
            "discord.com".into(),
            "discord.gg".into(),
            "discordapp.com".into(),
            "discordapp.net".into(),
            "gateway.discord.gg".into(),
            "cdn.discordapp.com".into(),
            "media.discordapp.net".into(),
            "router.discordapp.net".into(),
        ]
    } else if name_l.contains("valorant") || exe_l.contains("valorant") {
        vec![
            "riotgames.com".into(),
            "pvp.net".into(),
            "riotcdn.net".into(),
            "playvalorant.com".into(),
            "valorant.secure.dyn.riotcdn.net".into(),
            "auth.riotgames.com".into(),
            "entitlements.auth.riotgames.com".into(),
        ]
    } else if name_l.contains("league of legends") || exe_l.contains("leagueclient") || name_l.contains("teamfight tactics") {
        vec![
            "leagueoflegends.com".into(),
            "riotgames.com".into(),
            "pvp.net".into(),
            "riotcdn.net".into(),
        ]
    } else if name_l.contains("counter-strike") || name_l.contains("cs2") || exe_l == "cs2.exe" || exe_l == "csgo.exe" {
        vec![
            "steampowered.com".into(),
            "steamcommunity.com".into(),
            "valve.net".into(),
            "counter-strike.net".into(),
            "steamserver.net".into(),
        ]
    } else if name_l.contains("steam") || exe_l == "steam.exe" {
        vec![
            "steampowered.com".into(),
            "steamcommunity.com".into(),
            "steamstatic.com".into(),
            "steamcontent.com".into(),
            "steamgames.com".into(),
        ]
    } else if name_l.contains("epic games") || exe_l.contains("epicgameslauncher") {
        vec![
            "epicgames.com".into(),
            "unrealengine.com".into(),
            "epicgames.dev".into(),
            "ol.epicgames.com".into(),
        ]
    } else if name_l.contains("roblox") || exe_l.contains("roblox") {
        vec![
            "roblox.com".into(),
            "rbxcdn.com".into(),
            "setup.rbxcdn.com".into(),
            "roblox.com.edgekey.net".into(),
        ]
    } else if name_l.contains("spotify") || exe_l.contains("spotify") {
        vec![
            "spotify.com".into(),
            "scdn.co".into(),
            "spclient.wg.spotify.com".into(),
            "apresolve.spotify.com".into(),
        ]
    } else if name_l.contains("assetto") || exe_l.contains("acs.exe") || exe_l.contains("assettocorsa") {
        vec![
            "assettocorsa.gg".into(),
            "kunos-simulazioni.com".into(),
            "steampowered.com".into(),
        ]
    } else if name_l.contains("euro truck") || exe_l.contains("eurotrucks2") {
        vec![
            "eurotrucksimulator2.com".into(),
            "scssoft.com".into(),
            "truckersmp.com".into(),
            "steampowered.com".into(),
        ]
    } else if name_l.contains("7 days to die") || exe_l.contains("7daystodie") {
        vec![
            "7daystodie.com".into(),
            "thefunpimps.com".into(),
            "steampowered.com".into(),
        ]
    } else if name_l.contains("telegram") || exe_l.contains("telegram") {
        vec![
            "telegram.org".into(),
            "t.me".into(),
            "telesco.pe".into(),
        ]
    } else if name_l.contains("ea") || name_l.contains("origin") || exe_l.contains("eadesktop") || exe_l.contains("origin") {
        vec![
            "ea.com".into(),
            "origin.com".into(),
            "electronicarts.com".into(),
        ]
    } else if name_l.contains("battle.net") || name_l.contains("blizzard") || exe_l.contains("battle.net") {
        vec![
            "battle.net".into(),
            "blizzard.com".into(),
            "blz-contentstack.com".into(),
        ]
    } else if name_l.contains("ubisoft") || name_l.contains("uplay") || exe_l.contains("upc.exe") {
        vec![
            "ubisoft.com".into(),
            "ubi.com".into(),
            "uplay.com".into(),
        ]
    } else if name_l.contains("kick") || exe_l.contains("kick") {
        vec![
            "kick.com".into(),
            "kick-cdn.com".into(),
        ]
    } else if name_l.contains("twitch") || exe_l.contains("twitch") {
        vec![
            "twitch.tv".into(),
            "ttvnw.net".into(),
            "jtvnw.net".into(),
        ]
    } else if name_l.contains("youtube") {
        vec![
            "youtube.com".into(),
            "youtu.be".into(),
            "googlevideo.com".into(),
            "ytimg.com".into(),
        ]
    } else {
        let clean_token: String = name_l
            .chars()
            .filter(|c| c.is_alphanumeric())
            .collect();
        if clean_token.is_empty() {
            vec![]
        } else {
            vec![format!("{}.com", clean_token)]
        }
    }
}

pub fn is_likely_game(name: &str, exe_name: &str, install_location: &str) -> bool {
    let lower = format!("{} {} {}", name, exe_name, install_location).to_lowercase();
    let game_keywords = [
        "game", "steam", "valve", "riot", "valorant", "league", "counter-strike", "cs2", "csgo",
        "epic", "unreal", "blizzard", "battle.net", "ea games", "electronic arts", "ubisoft",
        "rockstar", "gta", "grand theft auto", "cyberpunk", "witcher", "roblox", "minecraft",
        "simulator", "truck", "assetto", "corsa", "fifa", "nba", "pes", "konami", "genshin",
        "honkai", "star rail", "apex", "overwatch", "pubg", "fortnite", "dota", "destiny",
        "warframe", "rust", "tarkov", "battlestate", "unity", "7 days to die", "vanguard"
    ];
    game_keywords.iter().any(|&k| lower.contains(k))
}

pub fn is_system_process(name: &str) -> bool {
    let lower = name.to_lowercase();
    let sys = [
        "system", "registry", "smss.exe", "csrss.exe", "wininit.exe", "services.exe",
        "lsass.exe", "svchost.exe", "fontdrvhost.exe", "memory compression", "dwm.exe",
        "conhost.exe", "sihost.exe", "taskhostw.exe", "explorer.exe", "ctfmon.exe",
        "searchindexer.exe", "startmenuexperiencehost.exe", "shellexperiencehost.exe",
        "textinputhost.exe", "applicationframehost.exe", "runtimebroker.exe", "spoolsv.exe",
        "securityhealthsystray.exe", "securityhealthservice.exe", "smartscreen.exe",
        "wlanext.exe", "dashost.exe", "audiodg.exe", "taskmgr.exe", "cmd.exe", "powershell.exe",
        "pwsh.exe", "wscript.exe", "cscript.exe", "dllhost.exe", "msedgewebview2.exe",
        "slipstream.exe", "winws.exe", "windivert.sys", "vgc.exe", "vgtray.exe"
    ];
    sys.iter().any(|&s| lower == s)
}

#[cfg(target_os = "windows")]
pub fn pick_exe_dialog() -> Option<String> {
    use std::mem;

    #[repr(C)]
    #[allow(non_snake_case, clippy::upper_case_acronyms)]
    struct OPENFILENAMEW {
        lStructSize: u32,
        hwndOwner: isize,
        hInstance: isize,
        lpstrFilter: *const u16,
        lpstrCustomFilter: *mut u16,
        nMaxCustFilter: u32,
        nFilterIndex: u32,
        lpstrFile: *mut u16,
        nMaxFile: u32,
        lpstrFileTitle: *mut u16,
        nMaxFileTitle: u32,
        lpstrInitialDir: *const u16,
        lpstrTitle: *const u16,
        Flags: u32,
        nFileOffset: u16,
        nFileExtension: u16,
        lpstrDefExt: *const u16,
        lCustData: usize,
        lpfnHook: usize,
        lpTemplateName: *const u16,
        pvReserved: usize,
        dwReserved: u32,
        FlagsEx: u32,
    }

    #[link(name = "comdlg32")]
    extern "system" {
        fn GetOpenFileNameW(lpofn: *mut OPENFILENAMEW) -> i32;
    }

    let mut file_buf: [u16; 1024] = [0; 1024];

    let filter: Vec<u16> = "Uygulama ve Oyun Dosyaları (*.exe)\0*.exe\0Tüm Dosyalar (*.*)\0*.*\0\0"
        .encode_utf16()
        .collect();

    let title: Vec<u16> = "Slipstream - Oyun veya Uygulama (.exe) Seçin\0"
        .encode_utf16()
        .collect();

    let mut ofn: OPENFILENAMEW = unsafe { mem::zeroed() };
    ofn.lStructSize = mem::size_of::<OPENFILENAMEW>() as u32;
    ofn.hwndOwner = 0;
    ofn.lpstrFilter = filter.as_ptr();
    ofn.lpstrFile = file_buf.as_mut_ptr();
    ofn.nMaxFile = file_buf.len() as u32;
    ofn.lpstrTitle = title.as_ptr();
    // OFN_FILEMUSTEXIST (0x1000) | OFN_PATHMUSTEXIST (0x800) | OFN_EXPLORER (0x80000)
    ofn.Flags = 0x00001000 | 0x00000800 | 0x00080000;

    let res = unsafe { GetOpenFileNameW(&mut ofn) };
    if res != 0 {
        let len = file_buf.iter().position(|&c| c == 0).unwrap_or(file_buf.len());
        if len > 0 {
            return Some(String::from_utf16_lossy(&file_buf[..len]));
        }
    }
    None
}

#[cfg(not(target_os = "windows"))]
pub fn pick_exe_dialog() -> Option<String> {
    None
}

#[cfg(target_os = "windows")]
unsafe fn query_reg_string(key: isize, value_name: &str) -> Option<String> {
    #[link(name = "advapi32")]
    extern "system" {
        fn RegQueryValueExW(
            hKey: isize,
            lpValueName: *const u16,
            lpReserved: *mut u32,
            lpType: *mut u32,
            lpData: *mut u8,
            lpcbData: *mut u32,
        ) -> i32;
    }

    let mut val_u16: Vec<u16> = value_name.encode_utf16().collect();
    val_u16.push(0);

    let mut data_type: u32 = 0;
    let mut byte_len: u32 = 0;
    if RegQueryValueExW(
        key,
        val_u16.as_ptr(),
        std::ptr::null_mut(),
        &mut data_type,
        std::ptr::null_mut(),
        &mut byte_len,
    ) != 0
        || byte_len == 0
    {
        return None;
    }

    let mut buf = vec![0u8; byte_len as usize];
    if RegQueryValueExW(
        key,
        val_u16.as_ptr(),
        std::ptr::null_mut(),
        &mut data_type,
        buf.as_mut_ptr(),
        &mut byte_len,
    ) != 0
    {
        return None;
    }

    let u16_len = (byte_len as usize) / 2;
    let u16_slice = std::slice::from_raw_parts(buf.as_ptr() as *const u16, u16_len);
    let end = u16_slice.iter().position(|&c| c == 0).unwrap_or(u16_len);
    Some(String::from_utf16_lossy(&u16_slice[..end]).trim().to_string())
}

#[cfg(target_os = "windows")]
unsafe fn query_reg_dword(key: isize, value_name: &str) -> Option<u32> {
    #[link(name = "advapi32")]
    extern "system" {
        fn RegQueryValueExW(
            hKey: isize,
            lpValueName: *const u16,
            lpReserved: *mut u32,
            lpType: *mut u32,
            lpData: *mut u8,
            lpcbData: *mut u32,
        ) -> i32;
    }

    let mut val_u16: Vec<u16> = value_name.encode_utf16().collect();
    val_u16.push(0);

    let mut data_type: u32 = 0;
    let mut dword_val: u32 = 0;
    let mut byte_len: u32 = std::mem::size_of::<u32>() as u32;
    if RegQueryValueExW(
        key,
        val_u16.as_ptr(),
        std::ptr::null_mut(),
        &mut data_type,
        &mut dword_val as *mut u32 as *mut u8,
        &mut byte_len,
    ) == 0
    {
        Some(dword_val)
    } else {
        None
    }
}

pub fn extract_exe_info(display_icon: &str, install_location: &str, display_name: &str) -> Option<(String, Option<String>)> {
    let clean_icon = display_icon.trim().trim_matches('"');
    let path_part = clean_icon.split(',').next().unwrap_or("").trim();
    if path_part.to_lowercase().ends_with(".exe") {
        let p = Path::new(path_part);
        if let Some(file_name) = p.file_name().and_then(|n| n.to_str()) {
            return Some((file_name.to_string(), Some(path_part.to_string())));
        }
    }

    if !install_location.is_empty() {
        let loc = Path::new(install_location.trim().trim_matches('"'));
        if loc.is_dir() {
            if let Ok(entries) = std::fs::read_dir(loc) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_file() {
                        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                            if ext.eq_ignore_ascii_case("exe") {
                                if let Some(file_name) = path.file_name().and_then(|n| n.to_str()) {
                                    let lower = file_name.to_lowercase();
                                    if !lower.contains("unins")
                                        && !lower.contains("crash")
                                        && !lower.contains("update")
                                        && !lower.contains("setup")
                                    {
                                        return Some((
                                            file_name.to_string(),
                                            Some(path.to_string_lossy().to_string()),
                                        ));
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    let name_lower = display_name.to_lowercase();
    if name_lower.contains("valorant") {
        Some(("VALORANT-Win64-Shipping.exe".into(), None))
    } else if name_lower.contains("league of legends") || name_lower.contains("teamfight tactics") {
        Some(("LeagueClientUx.exe".into(), None))
    } else if name_lower.contains("counter-strike") || name_lower.contains("cs2") {
        Some(("cs2.exe".into(), None))
    } else if name_lower.contains("steam") {
        Some(("steam.exe".into(), None))
    } else if name_lower.contains("discord") {
        Some(("Discord.exe".into(), None))
    } else if name_lower.contains("spotify") {
        Some(("Spotify.exe".into(), None))
    } else if name_lower.contains("epic games") {
        Some(("EpicGamesLauncher.exe".into(), None))
    } else if name_lower.contains("roblox") {
        Some(("RobloxPlayerBeta.exe".into(), None))
    } else if name_lower.contains("assetto corsa") {
        Some(("acs.exe".into(), None))
    } else if name_lower.contains("euro truck") {
        Some(("eurotrucks2.exe".into(), None))
    } else if name_lower.contains("7 days to die") {
        Some(("7DaysToDie.exe".into(), None))
    } else {
        None
    }
}

#[cfg(target_os = "windows")]
fn scan_registry_roots() -> Vec<ScannedApp> {
    #[link(name = "advapi32")]
    extern "system" {
        fn RegOpenKeyExW(
            hKey: isize,
            lpSubKey: *const u16,
            ulOptions: u32,
            samDesired: u32,
            phkResult: *mut isize,
        ) -> i32;
        fn RegEnumKeyExW(
            hKey: isize,
            dwIndex: u32,
            lpName: *mut u16,
            lpcchName: *mut u32,
            lpReserved: *mut u32,
            lpClass: *mut u16,
            lpcchClass: *mut u32,
            lpftLastWriteTime: *mut std::ffi::c_void,
        ) -> i32;
        fn RegCloseKey(hKey: isize) -> i32;
    }

    const HKLM: isize = -2147483646; // 0x80000002
    const HKCU: isize = -2147483647; // 0x80000001
    const KEY_READ: u32 = 0x20019;

    let roots: &[(isize, &str)] = &[
        (HKCU, "Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall"),
        (HKLM, "SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Uninstall"),
        (HKLM, "SOFTWARE\\WOW6432Node\\Microsoft\\Windows\\CurrentVersion\\Uninstall"),
    ];

    let mut results = Vec::new();
    let mut seen_exes = HashSet::new();

    for &(root_key, subkey_path) in roots {
        let subkey_wide: Vec<u16> = subkey_path.encode_utf16().chain(std::iter::once(0)).collect();
        let mut key_handle: isize = 0;
        if unsafe { RegOpenKeyExW(root_key, subkey_wide.as_ptr(), 0, KEY_READ, &mut key_handle) } != 0 {
            continue;
        }

        let mut index: u32 = 0;
        let mut name_buf = [0u16; 256];
        loop {
            let mut name_len: u32 = name_buf.len() as u32;
            let res = unsafe {
                RegEnumKeyExW(
                    key_handle,
                    index,
                    name_buf.as_mut_ptr(),
                    &mut name_len,
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                )
            };
            if res != 0 {
                break;
            }
            index += 1;

            let sub_name = String::from_utf16_lossy(&name_buf[..name_len as usize]);
            let full_sub_path = format!("{}\\{}", subkey_path, sub_name);
            let full_wide: Vec<u16> = full_sub_path.encode_utf16().chain(std::iter::once(0)).collect();
            let mut app_key_handle: isize = 0;
            if unsafe { RegOpenKeyExW(root_key, full_wide.as_ptr(), 0, KEY_READ, &mut app_key_handle) } != 0 {
                continue;
            }

            let sys_comp = unsafe { query_reg_dword(app_key_handle, "SystemComponent") };
            if sys_comp.unwrap_or(0) != 0 {
                unsafe { RegCloseKey(app_key_handle) };
                continue;
            }

            let parent_key = unsafe { query_reg_string(app_key_handle, "ParentKeyName") };
            if parent_key.is_some() {
                unsafe { RegCloseKey(app_key_handle) };
                continue;
            }

            if let Some(display_name) = unsafe { query_reg_string(app_key_handle, "DisplayName") } {
                let lower_name = display_name.to_lowercase();
                // Filter out OS components, redistributables, drivers
                if lower_name.contains("visual c++")
                    || lower_name.contains("windows sdk")
                    || lower_name.contains("update for windows")
                    || lower_name.contains("security update")
                    || lower_name.contains("package cache")
                    || lower_name.contains("directx")
                    || lower_name.contains(".net framework")
                    || lower_name.contains("hotfix")
                {
                    unsafe { RegCloseKey(app_key_handle) };
                    continue;
                }

                let display_icon = unsafe { query_reg_string(app_key_handle, "DisplayIcon") }.unwrap_or_default();
                let install_loc = unsafe { query_reg_string(app_key_handle, "InstallLocation") }.unwrap_or_default();

                if let Some((exe_name, exe_path)) = extract_exe_info(&display_icon, &install_loc, &display_name) {
                    let exe_lower = exe_name.to_lowercase();
                    if !seen_exes.contains(&exe_lower) && !is_system_process(&exe_name) {
                        seen_exes.insert(exe_lower.clone());
                        let is_game = is_likely_game(&display_name, &exe_name, &install_loc);
                        let category = if is_game { "game".to_string() } else { "app".to_string() };
                        let icon = if is_game { "🎮".to_string() } else { "📱".to_string() };
                        let domains = get_preset_domains_for_app(&display_name, &exe_name);

                        results.push(ScannedApp {
                            id: format!("auto_{}", exe_lower.replace('.', "_")),
                            name: display_name,
                            exe_name,
                            exe_path,
                            category,
                            is_running: false,
                            icon,
                            domains,
                        });
                    }
                }
            }

            unsafe { RegCloseKey(app_key_handle) };
        }

        unsafe { RegCloseKey(key_handle) };
    }

    results
}

#[cfg(not(target_os = "windows"))]
fn scan_registry_roots() -> Vec<ScannedApp> {
    vec![]
}

/// Scans both installed (closed) and currently running (open) applications and games.
/// Running items will have `is_running: true` (🟢 Çalışıyor).
/// Non-running items will have `is_running: false` (⚪ Kapalı).
pub fn scan_installed_and_running_apps(running_process_names: &[String]) -> Vec<ScannedApp> {
    let running_set: HashSet<String> = running_process_names
        .iter()
        .map(|s| s.to_lowercase())
        .collect();

    let mut apps = scan_registry_roots();

    // Mark running status for installed apps
    for app in &mut apps {
        if running_set.contains(&app.exe_name.to_lowercase()) {
            app.is_running = true;
        }
    }

    let mut seen_exes: HashSet<String> = apps.iter().map(|a| a.exe_name.to_lowercase()).collect();

    // Also include any currently running user applications that were not in the registry
    for proc_name in running_process_names {
        let lower = proc_name.to_lowercase();
        if !seen_exes.contains(&lower) && !is_system_process(proc_name) {
            seen_exes.insert(lower.clone());
            let clean_name = proc_name
                .strip_suffix(".exe")
                .or_else(|| proc_name.strip_suffix(".EXE"))
                .unwrap_or(proc_name)
                .to_string();

            let is_game = is_likely_game(&clean_name, proc_name, "");
            let category = if is_game { "game".to_string() } else { "app".to_string() };
            let icon = if is_game { "🎮".to_string() } else { "📱".to_string() };
            let domains = get_preset_domains_for_app(&clean_name, proc_name);

            apps.push(ScannedApp {
                id: format!("live_{}", lower.replace('.', "_")),
                name: clean_name,
                exe_name: proc_name.clone(),
                exe_path: None,
                category,
                is_running: true,
                icon,
                domains,
            });
        }
    }

    // Sort: Running (Çalışıyor) items first, then Stopped (Kapalı) items.
    // Within each group, sort alphabetically by name.
    apps.sort_by(|a, b| {
        match b.is_running.cmp(&a.is_running) {
            std::cmp::Ordering::Equal => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
            other => other,
        }
    });

    apps
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_preset_domains_discord() {
        let domains = get_preset_domains_for_app("Discord", "Discord.exe");
        assert!(domains.contains(&"discord.com".to_string()));
        assert!(domains.contains(&"discord.gg".to_string()));
    }

    #[test]
    fn test_preset_domains_valorant() {
        let domains = get_preset_domains_for_app("VALORANT", "VALORANT-Win64-Shipping.exe");
        assert!(domains.contains(&"riotgames.com".to_string()));
        assert!(domains.contains(&"pvp.net".to_string()));
    }

    #[test]
    fn test_is_likely_game() {
        assert!(is_likely_game("Counter-Strike 2", "cs2.exe", ""));
        assert!(is_likely_game("Euro Truck Simulator 2", "eurotrucks2.exe", ""));
        assert!(!is_likely_game("Spotify", "Spotify.exe", ""));
    }

    #[test]
    fn test_scan_installed_and_running_apps() {
        let procs = vec!["discord.exe".to_string(), "cs2.exe".to_string()];
        let apps = scan_installed_and_running_apps(&procs);
        // Ensure running items have is_running = true and are sorted at the top
        for app in &apps {
            if app.is_running {
                assert!(procs.iter().any(|p| p.eq_ignore_ascii_case(&app.exe_name)));
            }
        }
    }
}
