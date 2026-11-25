use crate::{
    files::{check_path, read_file},
    storage::*,
};
use anyhow::{ensure, Result};
use rusqlite::params;
use serde::Deserialize;
use std::{
    fs,
    path::{Path, PathBuf},
};

pub fn artwork(path: &Path) -> Result<Vec<u8>> {
    check_path(path, false)?;
    ensure!(
        fs::metadata(path)?.len() <= 4 * 1024 * 1024,
        "Artwork must be under 4 MiB."
    );
    let bytes = read_file(path)?.unwrap_or_default();
    ensure!(
        bytes.starts_with(b"\x89PNG\r\n\x1a\n") || bytes.starts_with(&[0xff, 0xd8, 0xff]),
        "Choose a PNG or JPEG cover."
    );
    Ok(bytes)
}
pub fn cover(cache: &Path, id: u32) -> Option<PathBuf> {
    let old = cache.join(format!("{id}_library_600x900.jpg"));
    if old.is_file() {
        return Some(old);
    }
    let path = cache.join(id.to_string());
    let direct = path.join("library_600x900.jpg");
    if direct.is_file() {
        return Some(direct);
    }
    fs::read_dir(path)
        .ok()?
        .flatten()
        .take(160)
        .map(|e| e.path())
        .find(|p| {
            p.file_name()
                .and_then(|s| s.to_str())
                .is_some_and(|s| s.ends_with("library_600x900.jpg"))
        })
}

struct Found {
    id: String,
    name: String,
    platform: &'static str,
    install: PathBuf,
    cover: Option<PathBuf>,
}
pub struct KnownFolders {
    pub documents: Option<PathBuf>,
    pub roaming: Option<PathBuf>,
}

// Resolve exact files only. Save directories frequently share these parents.
pub fn suggest(g: &Game, roots: &KnownFolders) -> (Option<PathBuf>, Vec<PathBuf>) {
    let (exe, config) = match g.id.as_str() {
        "steam-105600" => (
            "Terraria.exe",
            roots
                .documents
                .as_ref()
                .map(|d| d.join("My Games/Terraria/config.json")),
        ),
        "steam-413150" => (
            "Stardew Valley.exe",
            roots
                .roaming
                .as_ref()
                .map(|d| d.join("StardewValley/startup_preferences")),
        ),
        _ => return (None, vec![]),
    };
    let valid = |p: &PathBuf| p.is_file() && check_path(p, false).is_ok();
    (
        Some(g.install.join(exe)).filter(valid),
        config.filter(valid).into_iter().collect(),
    )
}

#[cfg(windows)]
fn registered_games(found: &mut Vec<Found>) {
    use winreg::{enums::*, RegKey};
    let machine = RegKey::predef(HKEY_LOCAL_MACHINE);
    for (key, field, prefix, platform) in [
        (r"SOFTWARE\GOG.com\Games", "path", "gog", "GOG Galaxy"),
        (
            r"SOFTWARE\Ubisoft\Launcher\Installs",
            "InstallDir",
            "ubisoft",
            "Ubisoft Connect",
        ),
        (r"SOFTWARE\EA Games", "Install Dir", "ea", "EA app"),
    ] {
        for view in [KEY_WOW64_32KEY, KEY_WOW64_64KEY] {
            let Ok(root) = machine.open_subkey_with_flags(key, KEY_READ | view) else {
                continue;
            };
            for sub in root.enum_keys().flatten() {
                let Ok(record) = root.open_subkey(&sub) else {
                    continue;
                };
                let Ok(dir) = record.get_value::<String, _>(field) else {
                    continue;
                };
                let install = PathBuf::from(dir);
                if !install.is_absolute() || !install.is_dir() {
                    continue;
                }
                let name = record.get_value::<String, _>("gameName").ok().or_else(|| {
                    install
                        .file_name()
                        .and_then(|n| n.to_str())
                        .map(String::from)
                });
                if let Some(name) = name {
                    found.push(Found {
                        id: format!("{prefix}-{sub}"),
                        name,
                        platform,
                        install,
                        cover: None,
                    });
                }
            }
        }
    }
}

#[cfg(windows)]
fn xbox_games(found: &mut Vec<Found>) {
    #[derive(Deserialize)]
    struct GameConfig {
        #[serde(rename = "Identity")]
        identity: Identity,
        #[serde(rename = "ExecutableList")]
        executables: Option<Executables>,
    }
    #[derive(Deserialize)]
    struct Identity {
        #[serde(rename = "@Name")]
        name: String,
    }
    #[derive(Deserialize)]
    struct Executables {
        #[serde(rename = "Executable", default)]
        entries: Vec<Executable>,
    }
    #[derive(Deserialize)]
    struct Executable {
        #[serde(rename = "@Name")]
        _name: String,
    }
    // Only inspect the immediate children of the conventional accessible XboxGames folder.
    use windows_sys::Win32::Storage::FileSystem::{GetDriveTypeW, GetLogicalDrives};
    let drives = unsafe { GetLogicalDrives() };
    for i in 0..26 {
        if drives & (1 << i) == 0 {
            continue;
        }
        let drive = format!("{}:\\", char::from(b'A' + i));
        if unsafe {
            GetDriveTypeW(
                drive
                    .encode_utf16()
                    .chain(Some(0))
                    .collect::<Vec<_>>()
                    .as_ptr(),
            )
        } != 3
        {
            continue;
        }
        let root = PathBuf::from(drive).join("XboxGames");
        let Ok(items) = fs::read_dir(root) else {
            continue;
        };
        for item in items.flatten() {
            let install = item.path().join("Content");
            let config = install.join("MicrosoftGame.config");
            if !fs::metadata(&config).is_ok_and(|m| m.is_file() && m.len() < 1024 * 1024) {
                continue;
            }
            let Ok(file) = fs::File::open(config) else {
                continue;
            };
            let Ok(m) = quick_xml::de::from_reader::<_, GameConfig>(std::io::BufReader::new(file))
            else {
                continue;
            };
            if m.executables.is_some_and(|e| !e.entries.is_empty()) {
                found.push(Found {
                    id: format!("xbox-{}", m.identity.name),
                    name: item.file_name().to_string_lossy().into_owned(),
                    platform: "Xbox",
                    install,
                    cover: None,
                });
            }
        }
    }
}
#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Epic {
    app_name: String,
    display_name: String,
    install_location: PathBuf,
    #[serde(default, rename = "bIsIncompleteInstall")]
    b_is_incomplete_install: bool,
}
pub fn scan(store: &mut Store) -> Result<usize> {
    let mut found = vec![];
    if let Ok(steam) = steamlocate::locate() {
        let cache = steam.path().join("appcache/librarycache");
        if let Ok(libs) = steam.libraries() {
            for lib in libs.flatten() {
                for app in lib.apps().flatten() {
                    // Steam distributes shared runtime dependencies as an app manifest.
                    if app.app_id == 228980 {
                        continue;
                    }
                    let install = lib.resolve_app_dir(&app);
                    if install.is_dir() {
                        if let Some(name) = app.name {
                            found.push(Found {
                                id: format!("steam-{}", app.app_id),
                                name,
                                platform: "Steam",
                                install,
                                cover: cover(&cache, app.app_id),
                            });
                        }
                    }
                }
            }
        }
    }
    if let Some(dir) = std::env::var_os("ProgramData") {
        if let Ok(items) =
            fs::read_dir(PathBuf::from(dir).join("Epic/EpicGamesLauncher/Data/Manifests"))
        {
            for item in items.flatten() {
                let p = item.path();
                if p.extension().and_then(|v| v.to_str()) != Some("item")
                    || !item.metadata().is_ok_and(|m| m.len() < 4_000_000)
                {
                    continue;
                }
                if let Some(m) = fs::read(&p)
                    .ok()
                    .and_then(|b| serde_json::from_slice::<Epic>(&b).ok())
                {
                    if m.install_location.is_dir() && !m.b_is_incomplete_install {
                        found.push(Found {
                            id: format!("epic-{}", m.app_name),
                            name: m.display_name,
                            platform: "Epic Games",
                            install: m.install_location,
                            cover: None,
                        });
                    }
                }
            }
        }
    }
    #[cfg(windows)]
    {
        registered_games(&mut found);
        xbox_games(&mut found);
    }
    let existing = store.games()?;
    let mut ids: std::collections::HashSet<_> = existing.iter().map(|g| g.id.clone()).collect();
    let mut installs: std::collections::HashSet<_> = existing
        .iter()
        .map(|g| g.install.to_string_lossy().to_lowercase())
        .collect();
    let tx = store.db.transaction()?;
    let mut added = 0;
    for item in found {
        if !ids.insert(item.id.clone())
            || !installs.insert(item.install.to_string_lossy().to_lowercase())
        {
            continue;
        }
        let bytes = item.cover.as_deref().and_then(|p| artwork(p).ok());
        let g = Game {
            id: item.id,
            name: item.name,
            platform: item.platform.into(),
            install: item.install,
            exe: None,
            maps: vec![],
            active: None,
            favorite: false,
            hidden: false,
            modified: 0,
            cover: bytes.is_some(),
        };
        Store::put_game(&tx, &g)?;
        if let Some(b) = bytes {
            tx.execute("UPDATE games SET artwork=? WHERE id=?", params![b, g.id])?;
        }
        added += 1;
    }
    tx.commit()?;
    Ok(added)
}

#[derive(serde::Serialize)]
pub struct Candidate {
    pub files: Vec<PathBuf>,
    pub overview: Vec<crate::overview::Setting>,
    pub modified: i64,
}
pub fn candidates(g: &Game, roots: &KnownFolders) -> (Option<PathBuf>, Vec<Candidate>) {
    let (mut exe, initial) = suggest(g, roots);
    let mut paths: Vec<Vec<PathBuf>> = if initial.is_empty() {
        vec![]
    } else {
        vec![initial]
    };
    let lower = g.name.to_lowercase();
    if g.id == "steam-252490" || lower == "rust" {
        let files: Vec<_> = [
            "client.cfg",
            "keys.cfg",
            "favorites.cfg",
            "keys_default.cfg",
            "autoexec.cfg",
        ]
        .iter()
        .map(|name| g.install.join("cfg").join(name))
        .filter(|p| p.is_file())
        .collect();
        if !files.is_empty() {
            paths.push(files);
        }
        let path = g.install.join("RustClient.exe");
        if path.is_file() {
            exe = Some(path);
        }
    }
    if lower.contains("rainbow") && lower.contains("siege") {
        exe = [
            "RainbowSix.exe",
            "RainbowSix_Vulkan.exe",
            "RainbowSix_DX11.exe",
        ]
        .iter()
        .map(|n| g.install.join(n))
        .find(|p| p.is_file());
        if let Some(d) = &roots.documents {
            for parent in [
                d.join("My Games/Rainbow Six - Siege"),
                d.join("My Games/Rainbow Six Siege"),
            ] {
                if let Ok(items) = fs::read_dir(parent) {
                    for item in items.flatten().take(128) {
                        let p = item.path().join("GameSettings.ini");
                        if p.is_file() {
                            paths.push(vec![p]);
                        }
                    }
                }
            }
        }
    }
    if lower.replace([' ', '-'], "").contains("robrun") {
        let low = roots
            .roaming
            .as_ref()
            .and_then(|p| p.parent())
            .map(|p| p.join("LocalLow"))
            .or_else(|| {
                std::env::var_os("USERPROFILE").map(|p| PathBuf::from(p).join("AppData/LocalLow"))
            });
        if let Some(low) = low {
            let base = low.join("Kefir/ROB RUN");
            let mut files = vec![];
            for name in ["settings", "player_prefs"] {
                let folder = base.join(name);
                if check_path(&folder, false).is_ok() {
                    if let Ok(items) = fs::read_dir(&folder) {
                        files.extend(items.flatten().take(128).map(|e| e.path()).filter(|p| {
                            p.is_file()
                                && p.extension().is_some_and(|e| e.eq_ignore_ascii_case("dat"))
                        }));
                    }
                }
                let file = base.join(format!("{name}.dat"));
                if file.is_file() {
                    files.push(file);
                }
            }
            files.sort();
            if !files.is_empty() {
                paths.push(files);
            }
        }
    }
    if g.platform == "Prism Launcher" {
        exe = Some(PathBuf::from("C:/Windows/javaw.exe"));
        let p = minecraft_dir(&g.install).join("options.txt");
        if p.is_file() {
            paths.push(vec![p])
        }
    }
    if paths.is_empty() && g.platform != "Prism Launcher" {
        let normalize = |s: &str| {
            s.chars()
                .filter(char::is_ascii_alphanumeric)
                .flat_map(char::to_lowercase)
                .collect::<String>()
        };
        let game_name = normalize(&g.name);
        let mut parents = vec![];
        for root in [&roots.documents, &roots.roaming].into_iter().flatten() {
            parents.push(root.clone());
            parents.push(root.join("My Games"));
        }
        if let Some(local) = std::env::var_os("LOCALAPPDATA") {
            parents.push(PathBuf::from(local));
        }
        if let Some(low) = roots
            .roaming
            .as_ref()
            .and_then(|p| p.parent())
            .map(|p| p.join("LocalLow"))
        {
            parents.push(low.clone());
            if let Ok(vendors) = fs::read_dir(low) {
                parents.extend(
                    vendors
                        .flatten()
                        .take(128)
                        .map(|e| e.path())
                        .filter(|p| p.is_dir() && check_path(p, false).is_ok()),
                );
            }
        }
        for parent in parents {
            if let Ok(items) = fs::read_dir(parent) {
                for item in items.flatten().take(512) {
                    let folder = item.path();
                    let name = normalize(&item.file_name().to_string_lossy());
                    if name.len() < 5
                        || game_name.len() < 5
                        || game_name != name
                        || check_path(&folder, false).is_err()
                    {
                        continue;
                    }
                    for sub in [
                        folder.clone(),
                        folder.join("Saved/Config/Windows"),
                        folder.join("Saved/Config/WindowsNoEditor"),
                    ] {
                        let files: Vec<_> = [
                            "GameUserSettings.ini",
                            "settings.ini",
                            "settings.cfg",
                            "settings.json",
                            "options.txt",
                            "config.cfg",
                        ]
                        .iter()
                        .map(|n| sub.join(n))
                        .filter(|p| p.is_file())
                        .collect();
                        if !files.is_empty() {
                            paths.push(files)
                        }
                    }
                }
            }
        }
        if exe.is_none() {
            let choices: Vec<_> = fs::read_dir(&g.install)
                .into_iter()
                .flatten()
                .flatten()
                .map(|e| e.path())
                .filter(|p| {
                    let name = p
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .to_lowercase();
                    p.is_file()
                        && name.ends_with(".exe")
                        && ![
                            "unins", "crash", "setup", "redist", "launcher", "report", "helper",
                            "service",
                        ]
                        .iter()
                        .any(|s| name.contains(s))
                })
                .take(3)
                .collect();
            if choices.len() == 1 {
                exe = choices.into_iter().next()
            }
        }
    }
    let mut candidates: Vec<Candidate> = paths
        .into_iter()
        .filter_map(|files| {
            let maps = files
                .iter()
                .map(|p| {
                    check_path(p, false)?;
                    Ok(crate::files::Mapping {
                        root: fs::canonicalize(p.parent().unwrap())?,
                        name: p.file_name().unwrap().to_string_lossy().into_owned(),
                    })
                })
                .collect::<Result<Vec<_>>>()
                .ok()?;
            let data = crate::files::capture(&maps).ok()?;
            Some(Candidate {
                files: files.clone(),
                overview: crate::overview::summarize(&data),
                modified: files
                    .iter()
                    .filter_map(|p| {
                        fs::metadata(p)
                            .ok()?
                            .modified()
                            .ok()?
                            .duration_since(std::time::UNIX_EPOCH)
                            .ok()
                            .map(|d| d.as_millis() as i64)
                    })
                    .max()
                    .unwrap_or(0),
            })
        })
        .collect();
    if exe.is_none() && g.platform != "Prism Launcher" {
        let choices: Vec<_> = fs::read_dir(&g.install)
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.path())
            .filter(|p| {
                let name = p
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_ascii_lowercase();
                p.is_file()
                    && name.ends_with(".exe")
                    && ![
                        "unins", "crash", "setup", "redist", "launcher", "report", "helper",
                        "service",
                    ]
                    .iter()
                    .any(|s| name.contains(s))
            })
            .take(3)
            .collect();
        if choices.len() == 1 {
            exe = choices.into_iter().next()
        }
    }
    candidates.sort_by_key(|c| std::cmp::Reverse(c.modified));
    (exe, candidates)
}

pub fn configure(store: &mut Store, g: &mut Game, files: &[PathBuf], exe: PathBuf) -> Result<()> {
    if !g.maps.is_empty() {
        return Ok(());
    }
    let maps = files
        .iter()
        .map(|p| crate::files::Mapping::select(p, &store.dir))
        .collect::<Result<Vec<_>>>()?;
    let data = crate::files::capture(&maps)?;
    g.maps = maps;
    g.exe = Some(exe);
    let p = Store::make(g, "Original settings", false)?;
    store.insert_snapshot(g, &p, &data, true)
}

pub fn minecraft_dir(instance: &Path) -> PathBuf {
    let visible = instance.join("minecraft");
    if visible.is_dir() {
        visible
    } else {
        instance.join(".minecraft")
    }
}
fn prism_setting<'a>(text: &'a str, key: &str) -> Option<&'a str> {
    text.lines()
        .find_map(|l| l.strip_prefix(key).and_then(|v| v.strip_prefix('=')))
        .map(str::trim)
}
pub fn prism(store: &mut Store, data: &Path) -> Result<()> {
    check_path(data, false)?;
    let config = data.join("prismlauncher.cfg");
    ensure!(
        config.is_file(),
        "Choose the Prism data folder containing prismlauncher.cfg."
    );
    let text = String::from_utf8_lossy(&fs::read(config)?).into_owned();
    let resolve = |value: &str| {
        if Path::new(value).is_absolute() {
            PathBuf::from(value)
        } else {
            data.join(value)
        }
    };
    let instances = prism_setting(&text, "InstanceDir")
        .map(resolve)
        .unwrap_or_else(|| data.join("instances"));
    let icons = prism_setting(&text, "IconsDir")
        .map(resolve)
        .unwrap_or_else(|| data.join("icons"));
    check_path(&instances, false)?;
    let mut container = store
        .games()?
        .into_iter()
        .find(|g| g.id == "prism-launcher")
        .unwrap_or(Game {
            id: "prism-launcher".into(),
            name: "Prism Launcher".into(),
            platform: "Prism".into(),
            install: data.into(),
            exe: None,
            maps: vec![],
            active: None,
            favorite: false,
            hidden: false,
            modified: 0,
            cover: false,
        });
    container.install = data.into();
    let mut executables = vec![data.join("prismlauncher.exe")];
    for env in ["LOCALAPPDATA", "ProgramFiles"] {
        if let Some(dir) = std::env::var_os(env) {
            let root = PathBuf::from(dir);
            executables.push(root.join("PrismLauncher/prismlauncher.exe"));
            executables.push(root.join("Programs/PrismLauncher/prismlauncher.exe"));
        }
    }
    let executable = executables.into_iter().find(|p| p.is_file());
    if container.exe != executable {
        container.modified = now();
    }
    container.exe = executable;
    Store::put_game(&store.db, &container)?;
    for entry in fs::read_dir(&instances)?.flatten().take(256) {
        let install = entry.path();
        let cfg = install.join("instance.cfg");
        if !cfg.is_file() || check_path(&cfg, false).is_err() {
            continue;
        }
        let id = format!(
            "prism-{}",
            crate::files::digest(install.to_string_lossy().as_bytes())
        );
        let text = String::from_utf8_lossy(&fs::read(cfg)?).into_owned();
        let name = prism_setting(&text, "name").unwrap_or("Minecraft instance");
        let icon = prism_setting(&text, "iconKey").unwrap_or("default");
        let mc = minecraft_dir(&install);
        let options = mc.join("options.txt");
        let mut game = store
            .games()?
            .into_iter()
            .find(|g| g.id == id)
            .unwrap_or(Game {
                id,
                name: name.into(),
                platform: "Prism Launcher".into(),
                install: install.clone(),
                exe: Some(PathBuf::from("C:/Windows/javaw.exe")),
                maps: vec![],
                active: None,
                favorite: false,
                hidden: false,
                modified: 0,
                cover: false,
            });
        if game.name != name {
            game.modified = now();
        }
        game.name = title(name)?;
        if crate::files::safe_name(icon).is_ok() {
            let image = [
                icons.join(format!("{icon}.png")),
                icons.join(format!("{icon}.jpg")),
                install.join("icon.png"),
                mc.join("icon.png"),
            ]
            .iter()
            .find_map(|p| artwork(p).ok());
            if let Some(image) = image {
                let previous: Option<Vec<u8>> = store
                    .db
                    .query_row("SELECT artwork FROM games WHERE id=?", [&game.id], |r| {
                        r.get(0)
                    })
                    .ok()
                    .flatten();
                if previous.as_ref() != Some(&image) {
                    game.modified = now();
                }
                game.cover = true;
                Store::put_game(&store.db, &game)?;
                store.db.execute(
                    "UPDATE games SET artwork=? WHERE id=?",
                    params![image, game.id],
                )?;
            }
        }
        Store::put_game(&store.db, &game)?;
        if game.maps.is_empty() && options.is_file() {
            configure(
                store,
                &mut game,
                &[options],
                PathBuf::from("C:/Windows/javaw.exe"),
            )?;
        }
        let version = fs::read(install.join("mmc-pack.json"))
            .ok()
            .and_then(|b| serde_json::from_slice::<serde_json::Value>(&b).ok())
            .and_then(|v| {
                v.get("components")?
                    .as_array()?
                    .iter()
                    .find(|c| c["uid"] == "net.minecraft")?
                    .get("version")?
                    .as_str()
                    .map(str::to_owned)
            });
        store.db.execute(
            "INSERT OR REPLACE INTO prism_meta VALUES(?,?)",
            params![
                game.id,
                serde_json::json!({"root":data,"version":version,"icon":icon}).to_string()
            ],
        )?;
    }
    Ok(())
}

#[cfg(windows)]
pub fn executable_artwork(exe: &Path) -> Option<Vec<u8>> {
    use std::os::windows::process::CommandExt;
    check_path(exe, false).ok()?;
    let root = std::env::var_os("SystemRoot")?;
    let output=std::process::Command::new(PathBuf::from(root).join("System32/WindowsPowerShell/v1.0/powershell.exe"))
 .args(["-NoProfile","-NonInteractive","-Command","Add-Type -AssemblyName System.Drawing; $icon=[System.Drawing.Icon]::ExtractAssociatedIcon($env:SCONFIG_ICON_EXE); if($icon){$b=$icon.ToBitmap();$m=New-Object IO.MemoryStream;$b.Save($m,[System.Drawing.Imaging.ImageFormat]::Png);[Convert]::ToBase64String($m.ToArray());$b.Dispose();$icon.Dispose();$m.Dispose()}"])
 .env("SCONFIG_ICON_EXE",exe).creation_flags(0x08000000).output().ok()?;
    use base64::Engine;
    if !output.status.success() {
        return None;
    }
    base64::engine::general_purpose::STANDARD
        .decode(String::from_utf8(output.stdout).ok()?.trim())
        .ok()
}
#[cfg(not(windows))]
pub fn executable_artwork(_: &Path) -> Option<Vec<u8>> {
    None
}
