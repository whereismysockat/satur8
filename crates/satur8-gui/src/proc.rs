//! Enumerate running userspace programs so the GUI can offer a "pick a running
//! game" list - and, for Steam games, recover the Steam AppID from the process
//! environment so we can show the game's real icon and art.

use std::collections::BTreeMap;
use std::fs;

/// Wine/Proton uses one shared loader binary for every Windows process, so
/// `/proc/<pid>/exe` is `wine64-preloader`, not the game. The real image name
/// is in `argv[0]`, as a Windows path.
const WINE_LOADERS: &[&str] = &["wine", "wine64", "wine-preloader", "wine64-preloader"];

/// Wine's own processes. They are not games and would otherwise show up once
/// `argv[0]` is used as the display name for Wine processes.
const WINE_SYSTEM_EXES: &[&str] = &[
    "wineserver",
    "services",
    "winedevice",
    "plugplay",
    "svchost",
    "lsass",
    "explorer",
    "rpcss",
    "tabtip",
    "conhost",
    "steam",
    "steamwebhelper",
    "gameoverlayui",
    "steamerrorreporter",
    "wineboot",
    "winemenubuilder",
];

pub struct RunningApp {
    pub exe: String,
    pub app_id: Option<u32>,
}

/// Distinct running programs (by exe basename), each with its Steam AppID if it
/// is a Steam game. Kernel threads, Wine's own helper processes, and our own
/// tooling are skipped.
pub fn running_apps() -> Vec<RunningApp> {
    // Keyed by exe so we dedup; prefer an entry that carries an AppID.
    let mut map: BTreeMap<String, Option<u32>> = BTreeMap::new();
    let Ok(entries) = fs::read_dir("/proc") else {
        return Vec::new();
    };
    for entry in entries.flatten() {
        let pid = entry.file_name().to_string_lossy().to_string();
        if !pid.bytes().all(|b| b.is_ascii_digit()) {
            continue;
        }
        let Ok(target) = fs::read_link(format!("/proc/{pid}/exe")) else {
            continue;
        };
        let Some(base) = target.file_name().map(|n| n.to_string_lossy().to_string()) else {
            continue;
        };
        if base.is_empty() || base.starts_with("satur8") {
            continue;
        }

        // Under Wine the loader is shared by every process in the prefix. Use
        // the image name from argv[0] instead, and skip Wine's system helpers.
        let name = if is_wine_loader(&base) {
            let Some(name) = wine_process_name(&pid) else {
                continue;
            };
            name
        } else if base == "wineserver" {
            continue;
        } else {
            base
        };
        if name.is_empty() || name.starts_with("satur8") {
            continue;
        }

        let app_id = steam_app_id_of(&pid);
        map.entry(name)
            .and_modify(|cur| {
                if cur.is_none() {
                    *cur = app_id;
                }
            })
            .or_insert(app_id);
    }
    map.into_iter()
        .map(|(exe, app_id)| RunningApp { exe, app_id })
        .collect()
}

fn is_wine_loader(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    WINE_LOADERS.iter().any(|loader| *loader == lower)
}

fn is_wine_system_exe(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    let stem = lower.strip_suffix(".exe").unwrap_or(&lower);
    WINE_SYSTEM_EXES.iter().any(|exe| *exe == stem)
        || is_wine_loader(stem)
        // Crash reporters and accessibility helpers are not the game. Some,
        // such as xalia.exe, are bundled by Proton.
        || stem == "xalia"
        || stem.ends_with("crashhandler")
        || stem.ends_with("crashreporter")
        || stem.starts_with("crashpad")
}

/// Does this string look like a complete Windows image name?
fn has_windows_image_suffix(name: &str) -> bool {
    let lower = name.trim().trim_matches('"').to_ascii_lowercase();
    [".exe", ".bat", ".com", ".scr"]
        .iter()
        .any(|ext| lower.ends_with(ext))
}

/// Turn a Wine `argv[0]` (usually a Windows path) into the game's exe name.
/// Returns `None` for Wine's own system processes and loaders.
fn wine_image_name(argv0: &str) -> Option<String> {
    let argv0 = argv0.trim().trim_matches('"').trim();
    if argv0.is_empty() {
        return None;
    }
    let normalized = argv0.replace('\\', "/").to_ascii_lowercase();
    if normalized.contains("/windows/") {
        return None;
    }
    let name = argv0.rsplit(|c| c == '\\' || c == '/').next().unwrap_or("");
    if name.is_empty() || is_wine_system_exe(name) {
        return None;
    }
    Some(name.to_string())
}

/// Wine sometimes exposes a process title whose executable path was split at
/// spaces, so `/proc/<pid>/cmdline` first yields only e.g. `C:\Program`. Join
/// the following tokens until the image name has a real executable suffix, so
/// "Call of Duty ...\game.exe" isn't reported as just "Call".
fn wine_image_name_from_cmdline(cmdline: &[u8]) -> Option<String> {
    let mut parts = cmdline.split(|b| *b == 0).filter(|part| !part.is_empty());
    let first = String::from_utf8_lossy(parts.next()?).into_owned();
    let mut image = first.trim().trim_matches('"').trim().to_string();
    if image.is_empty() {
        return None;
    }

    if !has_windows_image_suffix(&image) {
        for part in parts.take(64) {
            let part = String::from_utf8_lossy(part);
            let part = part.trim();
            if part.is_empty() {
                continue;
            }
            // A command-line switch means the image path ended before it.
            if part.starts_with('-') {
                break;
            }
            image.push(' ');
            image.push_str(part);
            if has_windows_image_suffix(&image) {
                break;
            }
        }
    }

    wine_image_name(&image)
}

fn wine_process_name(pid: &str) -> Option<String> {
    let cmdline = fs::read(format!("/proc/{pid}/cmdline")).ok()?;
    wine_image_name_from_cmdline(&cmdline)
}

/// Read a process's environment for `SteamAppId` / `SteamGameId`.
fn steam_app_id_of(pid: &str) -> Option<u32> {
    let data = fs::read(format!("/proc/{pid}/environ")).ok()?;
    for var in data.split(|&b| b == 0) {
        let s = String::from_utf8_lossy(var);
        for key in ["SteamAppId=", "SteamGameId=", "STEAM_COMPAT_APP_ID="] {
            if let Some(v) = s.strip_prefix(key) {
                if let Ok(id) = v.trim().parse::<u32>() {
                    if id != 0 {
                        return Some(id);
                    }
                }
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wine_paths_resolve_to_the_game_exe() {
        assert_eq!(
            wine_image_name(r"Z:\mnt\games\Juvio\bin\juvio.exe").as_deref(),
            Some("juvio.exe")
        );
        assert_eq!(
            wine_image_name(r"C:\Program Files\Game\Game.exe").as_deref(),
            Some("Game.exe")
        );
        assert_eq!(
            wine_image_name("/home/user/games/game/game.exe").as_deref(),
            Some("game.exe")
        );
    }

    #[test]
    fn wine_system_processes_are_skipped() {
        for argv0 in [
            r"C:\windows\system32\services.exe",
            r"c:\windows\system32\steam.exe",
            r"C:\windows\system32\explorer.exe",
            r"C:\windows\system32\winedevice.exe",
            "wineserver",
            "wine64-preloader",
        ] {
            assert_eq!(wine_image_name(argv0), None, "argv0: {argv0}");
        }
    }

    #[test]
    fn wine_loaders_are_recognized() {
        assert!(is_wine_loader("wine64-preloader"));
        assert!(is_wine_loader("wine-preloader"));
        assert!(!is_wine_loader("juvio.exe"));
    }

    #[test]
    fn splits_on_spaces_are_rejoined_into_the_full_game_exe() {
        let cmdline = b"S:\\\\user\\Games\\Call of Duty Modern Warfare 4\\game\\game\\sp26\\sp26-cod.exe\0hdey\0--project\0cod\0";

        assert_eq!(
            wine_image_name_from_cmdline(cmdline).as_deref(),
            Some("sp26-cod.exe")
        );
    }

    #[test]
    fn quoted_paths_with_spaces_are_rejoined() {
        let cmdline = b"\"C:\\Program Files\\My Game\\My Game.exe\"\0";

        assert_eq!(
            wine_image_name_from_cmdline(cmdline).as_deref(),
            Some("My Game.exe")
        );
    }

    #[test]
    fn crash_handlers_and_accessibility_helpers_are_skipped() {
        for cmdline in [
            b"S:\\\\user\\Games\\Call of Duty Modern Warfare 4\\game\\game\\sp26\\\\codCrashHandler.exe\0hdey\0--project\0cod\0".as_slice(),
            b"\\\\?\\S:\\user\\Games\\Call of Duty Modern Warfare 4\\wine\\share\\xalia\\xalia.exe\0".as_slice(),
        ] {
            assert_eq!(wine_image_name_from_cmdline(cmdline), None);
        }
    }

    #[test]
    fn native_process_names_with_spaces_are_kept() {
        assert_eq!(
            wine_image_name("/games/My Game").as_deref(),
            Some("My Game")
        );
    }
}
