use crate::{archive, discovery, files::*, storage::*, switching::Decision};
use anyhow::{bail, ensure, Context, Result};
use base64::{engine::general_purpose::STANDARD, Engine};
use notify::RecommendedWatcher;
use rfd::FileDialog;
use rusqlite::params;
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    collections::HashSet,
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::Command,
    sync::Mutex,
};
use tauri::{AppHandle, Emitter, Manager};

#[derive(Default)]
pub struct Grants {
    exe: HashSet<PathBuf>,
    files: HashSet<PathBuf>,
    cover: HashSet<PathBuf>,
    archive: HashSet<PathBuf>,
}
pub struct Session {
    pub store: Store,
    pub grants: Grants,
    pub watcher: Option<RecommendedWatcher>,
    pub changes: std::sync::mpsc::Sender<String>,
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Action {
    Library,
    Game {
        game: String,
    },
    Removed,
    WindowTheme {
        light: bool,
        caption: String,
        foreground: String,
    },
    Thumbnail {
        game: String,
        profile: String,
        path: Option<PathBuf>,
    },
    PrismFolder,
    PrismInstances,
    PrismCopy {
        source: String,
        destination: String,
    },
    Share {
        game: String,
        profile: String,
        targets: Vec<String>,
    },
    AddFiles {
        game: String,
        files: Vec<PathBuf>,
    },
    ManagedFiles {
        game: String,
    },
    Suggestions {
        game: String,
    },
    Launch {
        game: String,
    },
    Scan,
    Recovery,
    Recover,
    Detail {
        game: String,
    },
    Watch {
        game: Option<String>,
    },
    Changes {
        game: String,
    },
    Pick {
        field: String,
        folder: Option<bool>,
        game: Option<String>,
    },
    Register {
        game: Option<String>,
        name: String,
        platform: String,
        exe: PathBuf,
        files: Vec<PathBuf>,
        cover: Option<PathBuf>,
    },
    Flag {
        game: String,
        flag: String,
    },
    Cover {
        game: String,
    },
    Create {
        game: String,
        name: String,
        description: String,
        tags: Vec<String>,
        source: Option<String>,
    },
    Edit {
        game: String,
        profile: String,
        name: String,
        description: String,
        tags: Vec<String>,
    },
    Activate {
        game: String,
        profile: String,
        decision: Decision,
        token: Option<String>,
    },
    Flush {
        game: String,
    },
    WindowControl {
        operation: String,
    },
    Save {
        game: String,
    },
    Delete {
        game: String,
        profile: String,
    },
    Export {
        game: String,
        profile: String,
    },
    Import {
        game: String,
        path: PathBuf,
    },
    ImportFiles {
        game: String,
        name: String,
        files: Vec<PathBuf>,
    },
    OpenFolder {
        game: String,
        slot: usize,
    },
    Retention {
        value: Option<usize>,
    },
    Compare {
        game: String,
        first: String,
        second: String,
    },
    OpenProfile {
        game: String,
        profile: String,
    },
    SaveProfileFiles {
        game: String,
        profile: String,
    },
}
fn choose(
    s: &mut Session,
    field: &str,
    folder: bool,
    game: Option<&str>,
    documents: Option<PathBuf>,
) -> Result<Value> {
    let mut dialog = FileDialog::new();
    if let Some(g) = game {
        let g = s.store.game(g)?;
        if field == "exe" && g.install.is_dir() {
            dialog = dialog.set_directory(g.install);
        }
    }
    if field == "files" || field == "source" {
        if let Some(doc) = &documents {
            let games = doc.join("My Games");
            dialog = dialog.set_directory(if games.is_dir() { games } else { doc.clone() });
        }
    }
    if field == "files" {
        if let Some(id) = game {
            if let Some(mapping) = s.store.game(id)?.maps.first() {
                dialog = dialog.set_directory(&mapping.root);
            }
        }
    }
    if matches!(field, "files" | "source") {
        if let Some(id) = game {
            let g = s.store.game(id)?;
            if g.maps.is_empty() {
                let roots = discovery::KnownFolders {
                    documents: documents.clone(),
                    roaming: std::env::var_os("APPDATA").map(PathBuf::from),
                };
                let (_, c) = discovery::candidates(&g, &roots);
                if let Some(p) = c
                    .first()
                    .and_then(|c| c.files.first())
                    .and_then(|p| p.parent())
                {
                    dialog = dialog.set_directory(p);
                }
            }
        }
    }
    let (set, paths) = match field {
        "exe" => (
            &mut s.grants.exe,
            dialog
                .set_title("Select the running game's executable")
                .add_filter("Windows executable", &["exe"])
                .pick_file()
                .into_iter()
                .collect(),
        ),
        "cover" => (
            &mut s.grants.cover,
            dialog
                .set_title("Select cover artwork")
                .add_filter("PNG or JPEG", &["png", "jpg", "jpeg"])
                .pick_file()
                .into_iter()
                .collect(),
        ),
        "archive" => (
            &mut s.grants.archive,
            dialog
                .add_filter("SConfig profile", &["zip"])
                .pick_file()
                .into_iter()
                .collect(),
        ),
        "source" => (
            &mut s.grants.files,
            dialog
                .set_title("Choose the source configuration file")
                .pick_file()
                .into_iter()
                .collect(),
        ),
        "files" => {
            if folder {
                let Some(dir) = dialog
                    .clone()
                    .set_title("Select a configuration folder")
                    .pick_folder()
                else {
                    return Ok(json!([]));
                };
                dialog = dialog.set_directory(dir);
            }
            (
                &mut s.grants.files,
                dialog
                    .set_title("Select only configuration files to manage")
                    .pick_files()
                    .unwrap_or_default(),
            )
        }
        _ => bail!("Unknown file selection."),
    };
    for p in &paths {
        check_path(p, false)?;
        set.insert(p.clone());
    }
    Ok(json!(paths))
}
fn explorer(p: &Path) -> Result<()> {
    check_path(p, false)?;
    ensure!(p.is_dir(), "Folder is missing.");
    let win = std::env::var_os("SystemRoot").context("Windows folder unavailable.")?;
    Command::new(PathBuf::from(win).join("explorer.exe"))
        .arg(p)
        .spawn()?;
    Ok(())
}
fn working_folder(s: &Store, p: &Profile) -> PathBuf {
    s.dir
        .join("profiles")
        .join(digest(p.game.as_bytes()))
        .join(&p.id)
}
fn work_maps(folder: &Path, g: &Game) -> Result<Vec<Mapping>> {
    g.maps
        .iter()
        .enumerate()
        .map(|(i, m)| {
            let dir = folder.join(i.to_string());
            check_path(&dir, false)?;
            Ok(Mapping {
                root: fs::canonicalize(dir)?,
                name: m.name.clone(),
            })
        })
        .collect()
}
fn revision(data: &Contents) -> Result<Vec<u8>> {
    Ok(serde_json::to_vec(
        &data
            .iter()
            .map(|b| b.as_deref().map(digest))
            .collect::<Vec<_>>(),
    )?)
}

fn control_window(app: &AppHandle, operation: &str) -> Result<Value> {
    if let Some(w) = app.get_webview_window("main") {
        match operation {
            "minimize" => w.minimize()?,
            "maximize" => {
                if w.is_maximized()? {
                    w.unmaximize()?
                } else {
                    w.maximize()?
                }
            }
            "close" => w.close()?,
            "drag" => w.start_dragging()?,
            _ => bail!("Unknown window action."),
        }
    }
    Ok(Value::Null)
}
fn execute(s: &mut Session, app: &AppHandle, action: Action) -> Result<Value> {
    let store = &mut s.store;
    match action {
        Action::Removed => {
            let games: Vec<_> = store
                .games()?
                .into_iter()
                .filter(|g| {
                    store
                        .db
                        .query_row(
                            "SELECT EXISTS(SELECT 1 FROM excluded WHERE id=?)",
                            [&g.id],
                            |r| r.get::<_, bool>(0),
                        )
                        .unwrap_or(false)
                })
                .collect();
            Ok(json!(games))
        }
        Action::WindowTheme {
            light,
            caption,
            foreground,
        } => {
            if let Some(w) = app.get_webview_window("main") {
                w.set_theme(Some(if light {
                    tauri::Theme::Light
                } else {
                    tauri::Theme::Dark
                }))?;
                #[cfg(windows)]
                {
                    #[link(name = "dwmapi")]
                    extern "system" {
                        fn DwmSetWindowAttribute(
                            hwnd: *mut std::ffi::c_void,
                            attribute: u32,
                            value: *const u32,
                            size: u32,
                        ) -> i32;
                    }
                    let color = |text: &str| -> Result<u32> {
                        ensure!(
                            text.len() == 7 && text.starts_with('#'),
                            "Invalid theme color."
                        );
                        let v = u32::from_str_radix(&text[1..], 16)?;
                        Ok(((v & 255) << 16) | (v & 0xff00) | ((v >> 16) & 255))
                    };
                    let bg = color(&caption)?;
                    let fg = color(&foreground)?;
                    unsafe {
                        DwmSetWindowAttribute(w.hwnd()?.0 as _, 35, &bg, 4);
                        DwmSetWindowAttribute(w.hwnd()?.0 as _, 36, &fg, 4);
                    }
                }
            }
            Ok(Value::Null)
        }
        Action::Thumbnail {
            game,
            profile,
            path,
        } => {
            store.profile(&game, &profile)?;
            if let Some(p) = path {
                ensure!(s.grants.cover.contains(&p), "Choose an image first.");
                let b = discovery::artwork(&p)?;
                ensure!(b.starts_with(b"\x89PNG"), "Choose a PNG thumbnail.");
                store.db.execute(
                    "INSERT OR REPLACE INTO media VALUES(?,?)",
                    params![profile, b],
                )?;
                Ok(Value::Null)
            } else {
                store
                    .db
                    .execute("DELETE FROM media WHERE id=?", [profile])?;
                Ok(Value::Null)
            }
        }
        Action::PrismInstances => {
            let mut out = vec![];
            for game in store
                .games()?
                .into_iter()
                .filter(|g| g.platform == "Prism Launcher")
            {
                let data = capture(&game.maps).unwrap_or_default();
                let meta: Option<String> = store
                    .db
                    .query_row(
                        "SELECT data FROM prism_meta WHERE game=?",
                        [&game.id],
                        |r| r.get(0),
                    )
                    .ok();
                let meta: Value = meta
                    .and_then(|s| serde_json::from_str(&s).ok())
                    .unwrap_or(Value::Null);
                out.push(json!({"game":game,"overview":sconfig::overview::summarize(&data),"available":!data.is_empty()&&data.iter().all(Option::is_some),"version":meta["version"]}));
            }
            Ok(json!(out))
        }
        Action::PrismCopy {
            source,
            destination,
        } => {
            ensure!(source != destination, "Choose a different source instance.");
            let from = store.game(&source)?;
            let mut to = store.game(&destination)?;
            ensure!(
                from.platform == "Prism Launcher" && to.platform == "Prism Launcher",
                "Choose Prism instances."
            );
            store.ready(&from)?;
            store.ready(&to)?;
            crate::switching::ensure_closed(&from)?;
            crate::switching::ensure_closed(&to)?;
            let incoming = capture(&from.maps)?;
            let current = capture(&to.maps)?;
            ensure!(
                incoming.iter().all(Option::is_some),
                "Launch the source instance once to generate its settings."
            );
            std::thread::sleep(std::time::Duration::from_millis(150));
            ensure!(
                capture(&from.maps)? == incoming,
                "The source instance settings are still changing."
            );
            if from.active.is_some() {
                store.auto_save(&from, &incoming)?;
            }
            if to.active.is_some() {
                store.auto_save(&to, &current)?;
            }
            let source_root = fs::canonicalize(discovery::minecraft_dir(&from.install))?;
            let target_root = fs::canonicalize(discovery::minecraft_dir(&to.install))?;
            let mut settings = std::collections::HashMap::new();
            for (mapping, bytes) in from.maps.iter().zip(&incoming) {
                let path = mapping.destination()?;
                if let Ok(key) = path.strip_prefix(&source_root) {
                    settings.insert(key.to_owned(), bytes.clone());
                }
            }
            let mut data = current.clone();
            let mut matched = 0;
            for (i, mapping) in to.maps.iter().enumerate() {
                let path = mapping.destination()?;
                if let Ok(key) = path.strip_prefix(&target_root) {
                    if let Some(bytes) = settings.get(key) {
                        data[i] = bytes.clone();
                        matched += 1;
                    }
                }
            }
            ensure!(
                matched > 0,
                "These instances have no matching managed configuration files."
            );
            if data == current {
                return Ok(json!({"changed":false}));
            }
            let profile = Store::make(
                &to,
                &format!("From {}", from.name.chars().take(100).collect::<String>()),
                false,
            )?;
            store.insert_snapshot(&mut to, &profile, &data, false)?;
            let result = store.apply(&mut to, &profile.id, Decision::Check, None);
            match result {
                Ok(crate::switching::Outcome::Applied) => Ok(json!({"changed":true})),
                Ok(crate::switching::Outcome::Unsaved { token, .. }) => {
                    let outcome =
                        store.apply(&mut to, &profile.id, Decision::Save, Some(&token))?;
                    ensure!(matches!(outcome, crate::switching::Outcome::Applied), "The destination settings are still changing. Try again after closing the game.");
                    Ok(json!({"changed":true}))
                }
                Err(e) => {
                    if store.ready(&to).is_ok() && to.active.as_deref() != Some(&profile.id) {
                        let _ = store.remove(&to, &profile.id);
                    }
                    Err(e)
                }
            }
        }
        Action::PrismFolder => {
            if let Some(dir) = FileDialog::new()
                .set_title("Choose Prism Launcher's data folder")
                .pick_folder()
            {
                discovery::prism(store, &dir)?;
            }
            Ok(Value::Null)
        }
        Action::Share {
            game,
            profile,
            targets,
        } => {
            let g = store.game(&game)?;
            ensure!(
                g.platform == "Prism Launcher",
                "Only Prism profiles can be shared between instances."
            );
            let p = store.profile(&game, &profile)?;
            let bytes = store.load(&p, g.maps.len())?;
            ensure!(targets.len() <= 128, "Choose fewer instances.");
            let mut plans = vec![];
            let mut seen = HashSet::new();
            for target in targets {
                ensure!(
                    target != game && seen.insert(target.clone()),
                    "Choose each destination once."
                );
                let mut other = store.game(&target)?;
                ensure!(
                    other.platform == "Prism Launcher"
                        && other
                            .maps
                            .iter()
                            .map(|m| &m.name)
                            .eq(g.maps.iter().map(|m| &m.name)),
                    "Instance has a different settings layout."
                );
                store.ready(&other)?;
                let mut copy = Store::make(&other, &p.name, false)?;
                copy.description = p.description.clone();
                copy.tags = p.tags.clone();
                other.modified = now();
                plans.push((other, copy));
            }
            let tx = store.db.transaction()?;
            for (other, copy) in &plans {
                Store::write(&tx, copy, &bytes)?;
                Store::put_game(&tx, other)?;
            }
            tx.commit()?;
            Ok(json!(plans
                .into_iter()
                .map(|(game, profile)| json!({"game":game,"profile":profile}))
                .collect::<Vec<_>>()))
        }
        Action::Flush { game } => {
            let g = store.game(&game)?;
            if g.active.is_some() {
                let data = capture(&g.maps)?;
                std::thread::sleep(std::time::Duration::from_millis(150));
                store.auto_save(&g, &data)?;
            }
            Ok(Value::Null)
        }
        Action::WindowControl { operation } => control_window(app, &operation),
        Action::Game { game } => Ok(json!(store.game(&game)?)),
        Action::Library => {
            let records = store.games()?;
            if !records.iter().any(|g| g.id == "prism-launcher") {
                if let Some(first) = records.iter().find(|g| g.platform == "Prism Launcher") {
                    let launcher = Game {
                        id: "prism-launcher".into(),
                        name: "Prism Launcher".into(),
                        platform: "Prism".into(),
                        install: first.install.clone(),
                        exe: None,
                        maps: vec![],
                        active: None,
                        favorite: false,
                        hidden: false,
                        modified: 0,
                        cover: false,
                    };
                    Store::put_game(&store.db, &launcher)?;
                }
            }
            let mut query=store.db.prepare("SELECT g.data,json_extract(p.data,'$.name') FROM games g LEFT JOIN profiles p ON p.id=json_extract(g.data,'$.active') WHERE g.id NOT IN (SELECT id FROM excluded) AND json_extract(g.data,'$.platform') <> 'Prism Launcher'")?;
            let rows = query.query_map([], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?))
            })?;
            let mut games = Vec::new();
            for row in rows {
                let (data, name) = row?;
                let mut g: Value = serde_json::from_str(&data)?;
                g["activeName"] = json!(name);
                games.push(g);
            }
            Ok(json!(games))
        }
        Action::ManagedFiles { game } => {
            let g = store.game(&game)?;
            let roots = discovery::KnownFolders {
                documents: app.path().document_dir().ok(),
                roaming: app.path().data_dir().ok(),
            };
            let (_, candidates) = discovery::candidates(&g, &roots);
            let managed: HashSet<_> = g
                .maps
                .iter()
                .map(|m| m.destination())
                .collect::<Result<Vec<_>>>()?
                .into_iter()
                .collect();
            let mut files = vec![];
            for candidate in candidates {
                for p in candidate.files {
                    if !managed.contains(&fs::canonicalize(&p)?) {
                        s.grants.files.insert(p.clone());
                        if !files.contains(&p) {
                            files.push(p);
                        }
                    }
                }
            }
            Ok(json!(files))
        }
        Action::AddFiles { game, files } => {
            ensure!(
                files.iter().all(|p| s.grants.files.contains(p)),
                "Select files through discovery or the file picker."
            );
            let mut g = store.game(&game)?;
            store.add_files(&mut g, &files)?;
            Ok(Value::Null)
        }
        Action::Suggestions { game } => {
            let g = store.game(&game)?;
            let roots = discovery::KnownFolders {
                documents: app.path().document_dir().ok(),
                roaming: app.path().data_dir().ok(),
            };
            let (exe, candidates) = discovery::candidates(&g, &roots);
            s.grants.exe.extend(exe.iter().cloned());
            for c in &candidates {
                s.grants.files.extend(c.files.iter().cloned());
            }
            let files = if candidates.len() == 1 {
                candidates[0].files.clone()
            } else {
                vec![]
            };
            Ok(json!({"exe":exe,"files":files,"candidates":candidates}))
        }

        Action::Launch { game } => {
            let g = store.game(&game)?;
            let app_id =
                g.id.strip_prefix("steam-")
                    .and_then(|s| s.parse::<u32>().ok())
                    .context("Launch this game from its platform client.")?;
            let root = std::env::var_os("SystemRoot").context("Windows folder unavailable.")?;
            Command::new(PathBuf::from(root).join("explorer.exe"))
                .arg(format!("steam://rungameid/{app_id}"))
                .spawn()?;
            Ok(Value::Null)
        }
        Action::Scan => {
            let count = discovery::scan(store)?;
            let roots = discovery::KnownFolders {
                documents: app.path().document_dir().ok(),
                roaming: app.path().data_dir().ok(),
            };
            if let Some(d) = &roots.roaming {
                let p = d.join("PrismLauncher");
                if p.join("prismlauncher.cfg").is_file() {
                    let _ = discovery::prism(store, &p);
                }
            }
            for mut g in store.games()? {
                if g.maps.is_empty() {
                    let (exe, c) = discovery::candidates(&g, &roots);
                    if let (Some(e), [one]) = (exe, c.as_slice()) {
                        discovery::configure(store, &mut g, &one.files, e)?;
                    }
                }
                if g.maps.len() > 0
                    && g.active.is_none()
                    && store.profiles(Some(&g.id), false)?.is_empty()
                {
                    store.create(&mut g, "Original settings", "", vec![], None)?;
                }
            }
            Ok(json!(count))
        }
        Action::Recovery => {
            let names: std::collections::HashMap<_, _> =
                store.games()?.into_iter().map(|g| (g.id, g.name)).collect();
            let mut points = vec![];
            for p in store.profiles(None, true)? {
                let mut value = serde_json::to_value(&p)?;
                value["gameName"] = json!(names.get(&p.game));
                points.push(value);
            }
            Ok(json!(points))
        }
        Action::Recover => Ok(json!(store.recover()?)),
        Action::Detail { game } => {
            let g = store.game(&game)?;
            let mut profiles = vec![];
            for p in store.profiles(Some(&game), false)? {
                let mut v = serde_json::to_value(&p)?;
                v["overview"] = json!(sconfig::overview::summarize(&store.load(&p, g.maps.len())?));
                let image: Option<Vec<u8>> = store
                    .db
                    .query_row("SELECT bytes FROM media WHERE id=?", [&p.id], |r| r.get(0))
                    .ok();
                v["thumbnail"] =
                    json!(image.map(|b| format!("data:image/png;base64,{}", STANDARD.encode(b))));
                profiles.push(v);
            }
            Ok(json!(profiles))
        }
        Action::Watch { game } => {
            let _ = game;
            crate::monitoring::refresh(s)?;
            Ok(Value::Null)
        }
        Action::Changes { game } => Ok(json!(store.changed(&store.game(&game)?)?)),
        Action::Pick {
            field,
            folder,
            game,
        } => choose(
            s,
            &field,
            folder.unwrap_or(false),
            game.as_deref(),
            app.path().document_dir().ok(),
        ),
        Action::Register {
            game,
            name,
            platform,
            exe,
            files,
            cover,
        } => {
            ensure!(
                s.grants.exe.contains(&exe)
                    && exe
                        .extension()
                        .is_some_and(|v| v.eq_ignore_ascii_case("exe")),
                "Select the game's actual executable in the file picker."
            );
            if platform != "Prism Launcher" {
                check_path(&exe, false)?;
            }
            ensure!(
                (exe.is_file() || platform == "Prism Launcher")
                    && !files.is_empty()
                    && files.len() <= MAX_FILES,
                "Select between 1 and 128 configuration files."
            );
            let mut seen = HashSet::new();
            let mut maps = vec![];
            for f in &files {
                ensure!(
                    s.grants.files.contains(f),
                    "Select each file in the native picker."
                );
                let m = Mapping::select(f, &store.dir)?;
                let path = m.destination()?;
                ensure!(
                    path != fs::canonicalize(&exe)?
                        && seen.insert(path.to_string_lossy().to_lowercase()),
                    "A file is repeated or is the game executable."
                );
                maps.push(m);
            }
            capture(&maps)?;
            let mut g = if let Some(existing) = game {
                let g = store.game(&existing)?;
                ensure!(g.maps.is_empty(), "This game is already configured.");
                g
            } else {
                Game {
                    id: id(),
                    name: String::new(),
                    platform: String::new(),
                    install: exe.parent().context("No install directory.")?.into(),
                    exe: None,
                    maps: vec![],
                    active: None,
                    favorite: false,
                    hidden: false,
                    modified: 0,
                    cover: false,
                }
            };
            let cover = cover
                .map(|p| {
                    ensure!(s.grants.cover.contains(&p), "Select cover artwork first.");
                    discovery::artwork(&p)
                })
                .transpose()?;
            g.name = title(&name)?;
            g.platform = title(&platform)?;
            g.exe = Some(exe);
            g.maps = maps;
            g.modified = now();
            g.cover |= cover.is_some();
            let tx = store.db.transaction()?;
            Store::put_game(&tx, &g)?;
            if let Some(b) = cover {
                tx.execute("UPDATE games SET artwork=? WHERE id=?", params![b, g.id])?;
            }
            tx.execute("DELETE FROM excluded WHERE id=?", [&g.id])?;
            tx.commit()?;
            store.create(&mut g, "Original settings", "", vec![], None)?;
            Ok(json!(g.id))
        }
        Action::Flag { game, flag } => {
            let mut g = store.game(&game)?;
            match flag.as_str() {
                "favorite" => g.favorite = !g.favorite,
                "hidden" => g.hidden = !g.hidden,
                "restore" => {
                    store
                        .db
                        .execute("DELETE FROM excluded WHERE id=?", [&g.id])?;
                }
                "removed" => {
                    store
                        .db
                        .execute("INSERT OR IGNORE INTO excluded VALUES(?)", [&g.id])?;
                }
                _ => bail!("Unknown library flag."),
            };
            Store::put_game(&store.db, &g)?;
            Ok(Value::Null)
        }
        Action::Cover { game } => {
            let mut b: Option<Vec<u8>> =
                store
                    .db
                    .query_row("SELECT artwork FROM games WHERE id=?", [&game], |r| {
                        r.get(0)
                    })?;
            if b.is_none() {
                let g = store.game(&game)?;
                if let Some(id) =
                    g.id.strip_prefix("steam-")
                        .and_then(|s| s.parse::<u32>().ok())
                {
                    if let Ok(steam) = steamlocate::locate() {
                        b = discovery::cover(&steam.path().join("appcache/librarycache"), id)
                            .and_then(|p| discovery::artwork(&p).ok());
                    }
                }
                let exe = g.exe.filter(|e| e.is_file()).or_else(|| {
                    fs::read_dir(&g.install)
                        .ok()?
                        .flatten()
                        .map(|e| e.path())
                        .find(|p| {
                            p.is_file()
                                && p.extension().is_some_and(|e| e.eq_ignore_ascii_case("exe"))
                        })
                });
                if let Some(exe) = exe {
                    if b.is_none() {
                        b = discovery::executable_artwork(&exe);
                    }
                    if let Some(bytes) = &b {
                        store.db.execute(
                            "UPDATE games SET artwork=? WHERE id=?",
                            params![bytes, game],
                        )?;
                    }
                }
            }
            Ok(json!(b.map(|b| format!(
                "data:image/{};base64,{}",
                if b.starts_with(b"\x89PNG") {
                    "png"
                } else {
                    "jpeg"
                },
                STANDARD.encode(b)
            ))))
        }
        Action::Create {
            game,
            name,
            description,
            tags,
            source,
        } => {
            let mut g = store.game(&game)?;
            Ok(json!(store.create(
                &mut g,
                &name,
                &description,
                tags,
                source.as_deref()
            )?))
        }
        Action::Edit {
            game,
            profile,
            name,
            description,
            tags,
        } => {
            store.ready(&store.game(&game)?)?;
            let mut p = store.profile(&game, &profile)?;
            ensure!(!p.recovery, "Recovery points are immutable.");
            p.name = title(&name)?;
            p.description = description;
            p.tags = tags;
            p.modified = now();
            Store::put_profile(&store.db, &p)?;
            Ok(Value::Null)
        }
        Action::Activate {
            game,
            profile,
            decision,
            token,
        } => {
            let mut g = store.game(&game)?;
            Ok(json!(store.apply(
                &mut g,
                &profile,
                decision,
                token.as_deref()
            )?))
        }
        Action::Save { game } => {
            store.save(&store.game(&game)?)?;
            Ok(Value::Null)
        }
        Action::Delete { game, profile } => {
            store.remove(&store.game(&game)?, &profile)?;
            Ok(Value::Null)
        }
        Action::Export { game, profile } => {
            let g = store.game(&game)?;
            let p = store.profile(&game, &profile)?;
            let bytes = store.load(&p, g.maps.len())?;
            let chosen = FileDialog::new()
                .add_filter("Profile archive", &["zip"])
                .set_file_name("configuration.zip")
                .save_file();
            if let Some(path) = chosen {
                check_path(&path, true)?;
                ensure!(
                    path.extension()
                        .is_some_and(|e| e.eq_ignore_ascii_case("zip")),
                    "Use a .zip archive name."
                );
                let parent = fs::canonicalize(path.parent().context("Missing archive folder.")?)?;
                let full = parent.join(path.file_name().context("Missing archive filename.")?);
                ensure!(
                    !full.starts_with(fs::canonicalize(&store.dir)?)
                        && !store
                            .games()?
                            .iter()
                            .flat_map(|g| &g.maps)
                            .any(|m| m.root.join(&m.name) == full),
                    "Choose a location outside managed files and application storage."
                );
                archive::export(&path, &g, &p, &bytes)?;
                Ok(json!(true))
            } else {
                Ok(json!(false))
            }
        }
        Action::Import { game, path } => {
            ensure!(
                s.grants.archive.contains(&path),
                "Choose the archive in the file picker."
            );
            let mut g = store.game(&game)?;
            store.ready(&g)?;
            let a = archive::import(&path, &g)?;
            let mut p = Store::make(&g, &a.name, false)?;
            p.description = a.description;
            p.tags = a.tags;
            store.insert_snapshot(&mut g, &p, &a.data, false)?;
            Ok(json!(p))
        }
        Action::ImportFiles { game, name, files } => {
            let mut g = store.game(&game)?;
            store.ready(&g)?;
            ensure!(
                files.len() == g.maps.len(),
                "Select a source for each managed file."
            );
            let mappings = files
                .iter()
                .map(|p| {
                    ensure!(
                        s.grants.files.contains(p),
                        "Choose source files in the picker."
                    );
                    Mapping::select(p, &store.dir)
                })
                .collect::<Result<Vec<_>>>()?;
            let bytes = capture(&mappings)?;
            ensure!(
                bytes.iter().all(Option::is_some),
                "An import source is missing."
            );
            let profile = Store::make(&g, &name, false)?;
            store.insert_snapshot(&mut g, &profile, &bytes, false)?;
            Ok(json!(profile))
        }
        Action::OpenFolder { game, slot } => {
            let g = store.game(&game)?;
            explorer(
                &g.maps
                    .get(slot)
                    .context("Unknown configuration folder.")?
                    .root,
            )?;
            Ok(Value::Null)
        }
        Action::Retention { value } => {
            if let Some(v) = value {
                ensure!(
                    (1..=100).contains(&v),
                    "Choose 1–100 recovery points per game."
                );
                store
                    .db
                    .execute("INSERT OR REPLACE INTO prefs VALUES('retention',?)", [v])?;
            }
            Ok(json!(store.retain()?))
        }
        Action::Compare {
            game,
            first,
            second,
        } => {
            let g = store.game(&game)?;
            let a = store.load(&store.profile(&game, &first)?, g.maps.len())?;
            let b = store.load(&store.profile(&game, &second)?, g.maps.len())?;
            let result: Vec<_> = g
                .maps
                .iter()
                .enumerate()
                .map(|(i, m)| {
                    sconfig::comparison::compare(&m.name, a[i].as_deref(), b[i].as_deref())
                })
                .collect();
            Ok(json!(result))
        }
        Action::OpenProfile { game, profile } => {
            let g = store.game(&game)?;
            let p = store.profile(&game, &profile)?;
            let data = store.load(&p, g.maps.len())?;
            let folder = working_folder(store, &p);
            if !folder.exists() {
                let parent = folder.parent().context("Missing profile directory.")?;
                for dir in [store.dir.join("profiles"), parent.to_owned()] {
                    check_path(&dir, true)?;
                    if !dir.is_dir() {
                        fs::create_dir(&dir)?;
                    }
                }
                let tmp = tempfile::tempdir_in(parent)?;
                for (i, m) in g.maps.iter().enumerate() {
                    let dir = tmp.path().join(i.to_string());
                    fs::create_dir(&dir)?;
                    if let Some(b) = &data[i] {
                        fs::write(dir.join(&m.name), b)?;
                    }
                }
                fs::write(tmp.path().join("baseline.json"), revision(&data)?)?;
                fs::rename(tmp.path(), &folder)?;
            }
            let baseline = folder.join("baseline.json");
            check_path(&baseline, false)?;
            let recorded =
                read_file(&baseline)?.context("The profile edit baseline is missing.")?;
            let hashes: Vec<Option<String>> = serde_json::from_slice(&recorded)?;
            let current: Vec<_> = data.iter().map(|b| b.as_deref().map(digest)).collect();
            if hashes.len() < current.len() {
                ensure!(current.starts_with(&hashes),"The stored profile changed since this edit folder was opened. Duplicate the profile to start a fresh edit.");
                for (i, m) in g.maps.iter().enumerate().skip(hashes.len()) {
                    let dir = folder.join(i.to_string());
                    check_path(&dir, true)?;
                    if !dir.exists() {
                        fs::create_dir(&dir)?;
                    }
                    let map = Mapping {
                        root: fs::canonicalize(dir)?,
                        name: m.name.clone(),
                    };
                    if read_file(&map.destination()?)?.is_none() {
                        replace(&map, &None, &data[i])?;
                    }
                }
                let marker = Mapping {
                    root: fs::canonicalize(&folder)?,
                    name: "baseline.json".into(),
                };
                replace(&marker, &Some(recorded), &Some(revision(&data)?))?;
            }
            explorer(&folder)?;
            Ok(Value::Null)
        }
        Action::SaveProfileFiles { game, profile } => {
            let g = store.game(&game)?;
            store.ready(&g)?;
            let mut p = store.profile(&game, &profile)?;
            ensure!(!p.recovery, "Recovery points are immutable.");
            let folder = working_folder(store, &p);
            check_path(&folder, false)?;
            let baseline = folder.join("baseline.json");
            check_path(&baseline, false)?;
            let old = store.load(&p, g.maps.len())?;
            let edits = capture(&work_maps(&folder, &g)?)?;
            ensure!(
                edits.iter().all(Option::is_some),
                "An edited file is missing."
            );
            let previous_baseline = read_file(&baseline)?;
            ensure!(old==edits || previous_baseline.as_ref()==Some(&revision(&old)?),"The stored profile changed after this folder was opened. Duplicate the profile to start a fresh edit.");
            if g.active.as_deref() == Some(&p.id) {
                crate::switching::ensure_closed(&g)?;
            }
            p.modified = now();
            let tx = store.db.transaction()?;
            Store::write(&tx, &p, &edits)?;
            tx.commit()?;
            if g.active.as_deref() == Some(&p.id) {
                let mut live = g.clone();
                match store.apply(&mut live, &p.id, Decision::Check, None)? {
                    crate::switching::Outcome::Unsaved { token, .. } => {
                        store.apply(&mut live, &p.id, Decision::Discard, Some(&token))?;
                    }
                    crate::switching::Outcome::Applied => {}
                }
            }
            let marker = Mapping {
                root: fs::canonicalize(folder)?,
                name: "baseline.json".into(),
            };
            replace(&marker, &previous_baseline, &Some(revision(&edits)?))?;
            Ok(Value::Null)
        }
    }
}

#[tauri::command]
pub async fn request(app: AppHandle, action: Action) -> Result<Value, String> {
    if let Action::WindowControl { operation } = &action {
        return control_window(&app, operation).map_err(|e| format!("{e:#}"));
    }
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<Mutex<Session>>();
        let mut s = state
            .lock()
            .map_err(|_| "The library is busy after a failure. Restart SConfig.".to_string())?;
        let refresh = matches!(
            &action,
            Action::Scan | Action::Register { .. } | Action::AddFiles { .. } | Action::PrismFolder
        );
        let kind = std::mem::discriminant(&action);
        let result = execute(&mut s, &app, action);
        if refresh && result.is_ok() {
            let _ = crate::monitoring::refresh(&mut s);
        }
        if result.is_err() {
            if let Ok(mut f) = fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(s.store.dir.join("diagnostics.log"))
            {
                let _ = writeln!(f, "{} operation {:?} failed", now(), kind);
            }
        }
        result.map_err(|e| format!("{e:#}"))
    })
    .await
    .map_err(|e| e.to_string())?
}

pub fn dropped_archive(app: &AppHandle, path: PathBuf) {
    if check_path(&path, false).is_err() || !path.is_file() {
        return;
    }
    let state = app.state::<Mutex<Session>>();
    if let Ok(mut s) = state.lock() {
        s.grants.archive.insert(path.clone());
        let _ = app.emit("profile-archive-dropped", path);
    };
}
