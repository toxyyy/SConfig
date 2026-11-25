use crate::{
    archive,
    files::*,
    storage::*,
    switching::{Decision, Outcome},
};
use anyhow::{bail, Result};
use rusqlite::params;
use std::{fs, path::PathBuf};
use tempfile::TempDir;

struct Fixture {
    _temp: TempDir,
    store: Store,
    g: Game,
    a: Profile,
    b: Profile,
    first: PathBuf,
    second: PathBuf,
}
impl Fixture {
    fn new() -> Result<Self> {
        let temp = tempfile::tempdir()?;
        let mut store = Store::open(&temp.path().join("app"))?;
        let roots = [temp.path().join("video"), temp.path().join("controls")];
        for r in &roots {
            fs::create_dir(r)?
        }
        let first = roots[0].join("video.ini");
        let second = roots[1].join("bindings.cfg");
        fs::write(&first, b"low")?;
        fs::write(&second, [0xff, 0, 1])?;
        let maps = [&first, &second]
            .iter()
            .map(|p| Mapping::select(p, &store.dir))
            .collect::<Result<Vec<_>>>()?;
        let mut g = Game {
            id: id(),
            name: "Fixture".into(),
            platform: "Other".into(),
            exe: Some(temp.path().join("sconfig-test-game.exe")),
            install: temp.path().into(),
            maps,
            active: None,
            favorite: false,
            hidden: false,
            modified: now(),
            cover: false,
        };
        Store::put_game(&store.db, &g)?;
        let a = store.create(&mut g, "Low", "", vec![], None)?;
        fs::write(&first, b"high")?;
        fs::write(&second, [0, 7, 0])?;
        let b = store.create(&mut g, "High", "", vec![], None)?;
        fs::write(&first, b"low")?;
        fs::write(&second, [0xff, 0, 1])?;
        Ok(Self {
            _temp: temp,
            store,
            g,
            a,
            b,
            first,
            second,
        })
    }
    fn activate_b(&mut self) -> Result<Outcome> {
        self.store
            .apply(&mut self.g, &self.b.id, Decision::Check, None)
    }
    fn dirty(&mut self) -> Result<String> {
        fs::write(&self.first, b"custom")?;
        match self.activate_b()? {
            Outcome::Unsaved { token, .. } => Ok(token),
            _ => bail!("Missing unsaved prompt"),
        }
    }
}
#[test]
fn creates_binary_exact_profiles_and_reopens_database() -> Result<()> {
    let f = Fixture::new()?;
    let db = Store::open(&f.store.dir)?;
    let g = db.game(&f.g.id)?;
    assert_eq!(g.active, Some(f.a.id.clone()));
    assert_eq!(db.load(&f.a, 2)?[1], Some(vec![0xff, 0, 1]));
    Ok(())
}
#[test]
fn duplicate_and_rename_preserve_identity_and_data() -> Result<()> {
    let mut f = Fixture::new()?;
    let mut copy = f
        .store
        .create(&mut f.g, "Copy", "notes", vec!["pvp".into()], Some(&f.b.id))?;
    assert_ne!(copy.id, f.b.id);
    copy.name = "Renamed".into();
    Store::put_profile(&f.store.db, &copy)?;
    assert_eq!(f.store.profile(&f.g.id, &copy.id)?.name, "Renamed");
    assert_eq!(f.store.load(&copy, 2)?, f.store.load(&f.b, 2)?);
    Ok(())
}
#[test]
fn switches_across_roots_without_touching_other_files() -> Result<()> {
    let mut f = Fixture::new()?;
    assert_ne!(f.g.maps[0].root, f.g.maps[1].root);
    let save = f.first.with_file_name("player.sav");
    fs::write(&save, b"progress")?;
    assert!(matches!(f.activate_b()?, Outcome::Applied));
    assert_eq!(fs::read(f.first)?, b"high");
    assert_eq!(fs::read(f.second)?, [0, 7, 0]);
    assert_eq!(fs::read(save)?, b"progress");
    assert_eq!(f.g.active, Some(f.b.id));
    Ok(())
}
#[test]
fn unchanged_bytes_ignore_timestamps() -> Result<()> {
    let f = Fixture::new()?;
    fs::File::options()
        .write(true)
        .open(&f.first)?
        .set_modified(std::time::UNIX_EPOCH + std::time::Duration::from_secs(946684800))?;
    assert!(f.store.changed(&f.g)?.is_empty());
    Ok(())
}
#[test]
fn cancel_is_read_only_and_reports_changed_file() -> Result<()> {
    let mut f = Fixture::new()?;
    f.dirty()?;
    assert_eq!(f.store.changed(&f.g)?.len(), 1);
    assert_eq!(fs::read(&f.first)?, b"custom");
    assert_eq!(f.store.game(&f.g.id)?.active, Some(f.a.id));
    assert!(f.store.profiles(None, true)?.is_empty());
    Ok(())
}
#[test]
fn save_then_switch_preserves_unsaved_bytes_in_original() -> Result<()> {
    let mut f = Fixture::new()?;
    let token = f.dirty()?;
    f.store
        .apply(&mut f.g, &f.b.id, Decision::Save, Some(&token))?;
    assert_eq!(f.store.load(&f.a, 2)?[0].as_deref(), Some(&b"custom"[..]));
    assert_eq!(fs::read(f.first)?, b"high");
    Ok(())
}
#[test]
fn stale_discard_confirmation_requests_new_decision() -> Result<()> {
    let mut f = Fixture::new()?;
    let token = f.dirty()?;
    fs::write(&f.first, b"new edit")?;
    assert!(matches!(
        f.store
            .apply(&mut f.g, &f.b.id, Decision::Discard, Some(&token))?,
        Outcome::Unsaved { .. }
    ));
    assert_eq!(fs::read(f.first)?, b"new edit");
    Ok(())
}
#[test]
fn restores_discarded_changes_and_backs_up_restore() -> Result<()> {
    let mut f = Fixture::new()?;
    let token = f.dirty()?;
    f.store
        .apply(&mut f.g, &f.b.id, Decision::Discard, Some(&token))?;
    let backup = f.store.profiles(None, true)?.remove(0);
    f.store.apply(&mut f.g, &backup.id, Decision::Check, None)?;
    assert_eq!(fs::read(f.first)?, b"custom");
    assert_eq!(f.g.active, None);
    assert_eq!(f.store.profiles(None, true)?.len(), 2);
    Ok(())
}
#[test]
fn simulated_partial_failure_rolls_back() -> Result<()> {
    let mut f = Fixture::new()?;
    let err = f
        .store
        .apply_with(&mut f.g, &f.b.id, Decision::Check, None, |i| {
            if i == 1 {
                bail!("simulated failure")
            } else {
                Ok(())
            }
        })
        .unwrap_err();
    assert!(err.to_string().contains("restored"));
    assert_eq!(fs::read(f.first)?, b"low");
    assert_eq!(f.store.game(&f.g.id)?.active, Some(f.a.id));
    Ok(())
}
#[test]
fn corrupted_or_missing_snapshot_stops_before_write() -> Result<()> {
    let mut f = Fixture::new()?;
    f.store
        .db
        .execute("DELETE FROM files WHERE profile=? AND slot=0", [&f.b.id])?;
    assert!(f.activate_b().is_err());
    assert_eq!(fs::read(f.first)?, b"low");
    Ok(())
}
#[test]
fn missing_live_file_requires_decision_and_can_be_restored_as_absent() -> Result<()> {
    let mut f = Fixture::new()?;
    fs::remove_file(&f.first)?;
    assert!(f
        .store
        .create(&mut f.g, "Missing", "", vec![], None)
        .is_err());
    let Outcome::Unsaved { token, .. } = f.activate_b()? else {
        panic!("expected prompt")
    };
    f.store
        .apply(&mut f.g, &f.b.id, Decision::Discard, Some(&token))?;
    let backup = f.store.profiles(None, true)?.remove(0);
    f.store.apply(&mut f.g, &backup.id, Decision::Check, None)?;
    assert!(!f.first.exists());
    Ok(())
}
#[cfg(windows)]
#[test]
fn locked_file_keeps_active_profile_and_rolls_back_earlier_files() -> Result<()> {
    use std::os::windows::fs::OpenOptionsExt;
    let mut f = Fixture::new()?;
    let _guard = fs::OpenOptions::new()
        .read(true)
        .share_mode(1)
        .open(&f.second)?;
    assert!(f.activate_b().is_err());
    assert_eq!(fs::read(f.first)?, b"low");
    assert_eq!(f.store.game(&f.g.id)?.active, Some(f.a.id));
    Ok(())
}
#[test]
fn rejects_traversal_and_windows_special_names() {
    for name in [
        "../outside",
        "a/b",
        "x\\y",
        "C:escape",
        "video.cfg:ads",
        "aux.ini",
        "nul",
        "file.",
        "..",
    ] {
        assert!(safe_name(name).is_err(), "{name}")
    }
}
#[test]
fn reads_valid_archives_and_rejects_untrusted_entries() -> Result<()> {
    use std::io::Write;
    let f = Fixture::new()?;
    let path = f._temp.path().join("test.zip");
    let bytes = f.store.load(&f.a, 2)?;
    archive::export(&path, &f.g, &f.a, &bytes)?;
    assert_eq!(archive::import(&path, &f.g)?.data, bytes);
    let file = fs::OpenOptions::new().read(true).write(true).open(&path)?;
    let mut zip = zip::ZipWriter::new_append(file)?;
    zip.start_file("../outside", zip::write::SimpleFileOptions::default())?;
    zip.write_all(b"x")?;
    zip.finish()?;
    assert!(archive::import(&path, &f.g).is_err());
    assert!(!f._temp.path().join("outside").exists());
    Ok(())
}
#[test]
fn interrupted_switch_recovers_on_reopen() -> Result<()> {
    let f = Fixture::new()?;
    let backup = Store::make(&f.g, "Interrupted", true)?;
    let bytes = capture(&f.g.maps)?;
    Store::write(&f.store.db, &backup, &bytes)?;
    f.store.db.execute(
        "INSERT INTO journal VALUES(?,?,?)",
        params![f.g.id, backup.id, f.b.id],
    )?;
    fs::write(&f.first, b"high")?;
    let mut reopened = Store::open(&f.store.dir)?;
    assert!(reopened.ready(&f.g).is_err());
    assert_eq!(reopened.recover()?.len(), 1);
    reopened.ready(&f.g)?;
    assert_eq!(fs::read(f.first)?, b"low");
    assert_eq!(reopened.game(&f.g.id)?.active, Some(f.a.id));
    Ok(())
}
#[test]
fn running_game_blocks_activation() -> Result<()> {
    let mut f = Fixture::new()?;
    f.g.exe = Some(std::env::current_exe()?);
    assert!(f.activate_b().unwrap_err().to_string().contains("Close"));
    assert!(f.store.profiles(None, true)?.is_empty());
    Ok(())
}

#[test]
fn curated_suggestion_excludes_adjacent_saves_and_unknown_games() -> Result<()> {
    use crate::discovery::{suggest, KnownFolders};
    let mut f = Fixture::new()?;
    let documents = f._temp.path().join("Documents");
    let dir = documents.join("My Games/Terraria");
    fs::create_dir_all(dir.join("Players"))?;
    fs::write(dir.join("config.json"), b"{}")?;
    fs::write(dir.join("Players/character.plr"), b"progress")?;
    let roots = KnownFolders {
        documents: Some(documents),
        roaming: None,
    };
    assert!(suggest(&f.g, &roots).1.is_empty());
    f.g.id = "steam-105600".into();
    assert_eq!(suggest(&f.g, &roots).1, vec![dir.join("config.json")]);
    Ok(())
}

#[test]
fn compares_text_lines_and_binary_content() {
    let delta = crate::comparison::compare("cfg", Some(b"a\nb\nc\n"), Some(b"a\nd\nc\n"));
    assert_eq!(delta.lines.len(), 2);
    assert_eq!(
        (
            delta.lines[0].added,
            delta.lines[0].number,
            delta.lines[0].text
        ),
        (false, 2, "b\n")
    );
    assert_eq!(
        (
            delta.lines[1].added,
            delta.lines[1].number,
            delta.lines[1].text
        ),
        (true, 2, "d\n")
    );
    let binary = crate::comparison::compare("dat", Some(&[0xff]), Some(&[0xfe]));
    assert_eq!(binary.state, "Modified");
    assert!(binary.lines.is_empty());
}

#[test]
fn refuses_readonly_destination_and_keeps_the_original() -> Result<()> {
    let mut f = Fixture::new()?;
    let original = fs::metadata(&f.second)?.permissions();
    let mut readonly = original.clone();
    readonly.set_readonly(true);
    fs::set_permissions(&f.second, readonly)?;
    let result = f.activate_b();
    fs::set_permissions(&f.second, original)?;
    assert!(result.is_err());
    assert_eq!(fs::read(&f.first)?, b"low");
    assert_eq!(f.store.game(&f.g.id)?.active, Some(f.a.id));
    Ok(())
}

#[test]
fn invalid_archive_mapping_and_version_never_reach_the_live_folder() -> Result<()> {
    use std::io::Write;
    let f = Fixture::new()?;
    for (version, filename) in [(2, "video.ini"), (1, "../other.ini"), (1, "other.ini")] {
        let path = f._temp.path().join("unsafe.zip");
        let mut zip = zip::ZipWriter::new(fs::File::create(&path)?);
        let manifest = serde_json::json!({"version":version,"name":"Invalid","description":"","tags":[],"files":[{"name":filename,"hash":null},{"name":"bindings.cfg","hash":null}]});
        zip.start_file("manifest.json", zip::write::SimpleFileOptions::default())?;
        zip.write_all(&serde_json::to_vec(&manifest)?)?;
        zip.finish()?;
        assert!(archive::import(&path, &f.g).is_err());
    }
    assert_eq!(fs::read(f.first)?, b"low");
    Ok(())
}

#[test]
fn siege_candidates_are_separate_and_show_sensitivity() -> Result<()> {
    use crate::discovery::{candidates, KnownFolders};
    let mut f = Fixture::new()?;
    f.g.name = "Tom Clancy's Rainbow Six Siege".into();
    let docs = f._temp.path().join("Documents");
    for account in ["account-a", "account-b"] {
        let folder = docs.join("My Games/Rainbow Six - Siege").join(account);
        fs::create_dir_all(&folder)?;
        fs::write(
            folder.join("GameSettings.ini"),
            b"[INPUT]\nMouseYawSensitivity=12\nMousePitchSensitivity=14\n",
        )?;
    }
    let (_, found) = candidates(
        &f.g,
        &KnownFolders {
            documents: Some(docs),
            roaming: None,
        },
    );
    assert_eq!(found.len(), 2);
    assert_eq!(found[0].files.len(), 1);
    assert_eq!(found[0].overview[0].value, "12");
    Ok(())
}
#[test]
fn prism_instances_capture_original_and_keep_separate_destinations() -> Result<()> {
    let mut f = Fixture::new()?;
    let root = f._temp.path().join("Prism");
    fs::create_dir_all(&root)?;
    fs::write(
        root.join("prismlauncher.cfg"),
        b"[General]\nInstanceDir=instances\n",
    )?;
    for name in ["Vanilla", "Modded"] {
        let dir = root.join("instances").join(name);
        fs::create_dir_all(dir.join(".minecraft"))?;
        fs::write(dir.join("instance.cfg"), format!("name={name}\n"))?;
        fs::write(
            dir.join(".minecraft/options.txt"),
            b"mouseSensitivity:0.5\nrenderDistance:12\n",
        )?;
    }
    crate::discovery::prism(&mut f.store, &root)?;
    let games: Vec<_> = f
        .store
        .games()?
        .into_iter()
        .filter(|g| g.platform == "Prism Launcher")
        .collect();
    assert_eq!(games.len(), 2);
    assert_ne!(games[0].maps[0].root, games[1].maps[0].root);
    for g in &games {
        assert_eq!(
            f.store.profiles(Some(&g.id), false)?[0].name,
            "Original settings"
        );
        assert!(g.active.is_some());
    }
    crate::discovery::prism(&mut f.store, &root)?;
    assert_eq!(f.store.games()?.len(), 4);
    Ok(())
}

#[test]
fn overview_reads_cfg_console_values_without_exposing_unrecognized_keys() {
    let values = crate::overview::summarize(&vec![Some(
        b"input.sensitivity \"0.4\"\ngraphics.fov \"90\"\npassword \"private\"\n".to_vec(),
    )]);
    assert_eq!(values.len(), 2);
    assert_eq!(values[0].value, "0.4");
    assert_eq!(values[1].value, "90");
}

#[test]
fn renamed_workspace_imports_wal_profiles_and_preferences_once() -> Result<()> {
    let f = Fixture::new()?;
    let edits = f.store.dir.join("profiles/editable");
    fs::create_dir_all(&edits)?;
    fs::write(edits.join("settings.ini"), b"custom settings")?;
    let preferences = f.store.dir.join("EBWebView/Default/Local Storage/leveldb");
    fs::create_dir_all(&preferences)?;
    fs::write(preferences.join("fixture.log"), b"theme preference")?;
    let destination = f._temp.path().join("SConfigData");
    assert!(crate::migration::import_workspace(
        &f.store.dir,
        &destination
    )?);
    let imported = Store::open(&destination)?;
    assert_eq!(imported.game(&f.g.id)?.active, Some(f.a.id.clone()));
    assert_eq!(imported.load(&f.a, 2)?, f.store.load(&f.a, 2)?);
    assert_eq!(
        fs::read(destination.join("profiles/editable/settings.ini"))?,
        b"custom settings"
    );
    assert_eq!(
        fs::read(destination.join("EBWebView/Default/Local Storage/leveldb/fixture.log"))?,
        b"theme preference"
    );
    let mut renamed = f.a.clone();
    renamed.name = "Updated in SConfig".into();
    Store::put_profile(&imported.db, &renamed)?;
    assert!(!crate::migration::import_workspace(
        &f.store.dir,
        &destination
    )?);
    assert_eq!(
        imported.profile(&f.g.id, &f.a.id)?.name,
        "Updated in SConfig"
    );
    assert_eq!(f.store.profile(&f.g.id, &f.a.id)?.name, f.a.name);
    Ok(())
}
#[test]
fn failed_workspace_import_preserves_source_and_leaves_no_partial_destination() -> Result<()> {
    let f = Fixture::new()?;
    fs::write(f.store.dir.join("profiles"), b"invalid directory")?;
    let destination = f._temp.path().join("SConfigData");
    assert!(crate::migration::import_workspace(&f.store.dir, &destination).is_err());
    assert!(!destination.exists());
    assert_eq!(
        f.store.load(&f.a, 2)?[0].as_deref(),
        Some(b"low".as_slice())
    );
    Ok(())
}

#[test]
fn rust_discovery_groups_settings_bindings_and_favorites() -> Result<()> {
    use crate::discovery::{candidates, configure, KnownFolders};
    let mut f = Fixture::new()?;
    let dir = f._temp.path().join("cfg");
    fs::create_dir(&dir)?;
    for name in [
        "client.cfg",
        "keys.cfg",
        "favorites.cfg",
        "keys_default.cfg",
    ] {
        fs::write(dir.join(name), name.as_bytes())?;
    }
    fs::write(dir.join("steam_autocloud.vdf"), b"launcher metadata")?;
    fs::write(dir.join("unrelated.sav"), b"progress")?;
    let mut g = f.g.clone();
    g.id = "steam-252490".into();
    g.name = "Rust".into();
    g.install = f._temp.path().into();
    g.maps.clear();
    g.active = None;
    fs::write(g.install.join("RustClient.exe"), b"")?;
    Store::put_game(&f.store.db, &g)?;
    let (exe, found) = candidates(
        &g,
        &KnownFolders {
            documents: None,
            roaming: None,
        },
    );
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].files.len(), 4);
    configure(&mut f.store, &mut g, &found[0].files, exe.unwrap())?;
    let original = f.store.profile(&g.id, g.active.as_deref().unwrap())?;
    assert_eq!(original.count, 4);
    assert_eq!(f.store.load(&original, 4)?.len(), 4);
    assert!(!g
        .maps
        .iter()
        .any(|m| m.name.ends_with(".vdf") || m.name.ends_with(".sav")));
    Ok(())
}
#[test]
fn added_files_are_captured_in_every_profile_and_swapped_together() -> Result<()> {
    let mut f = Fixture::new()?;
    let keys = f.first.with_file_name("keys.cfg");
    let favorites = f.first.with_file_name("favorites.cfg");
    fs::write(&keys, b"bind one")?;
    fs::write(&favorites, b"server one")?;
    f.store
        .add_files(&mut f.g, &[keys.clone(), favorites.clone()])?;
    let a = f.store.profile(&f.g.id, &f.a.id)?;
    let b = f.store.profile(&f.g.id, &f.b.id)?;
    assert_eq!(a.count, 4);
    assert_eq!(b.count, 4);
    assert_eq!(f.store.load(&b, 4)?[0].as_deref(), Some(b"high".as_slice()));
    fs::write(&keys, b"bind two")?;
    fs::write(&favorites, b"server two")?;
    let c = f.store.create(&mut f.g, "All files", "", vec![], None)?;
    let Outcome::Unsaved { token, files } =
        f.store.apply(&mut f.g, &c.id, Decision::Check, None)?
    else {
        panic!("expected prompt")
    };
    assert_eq!(files.len(), 2);
    f.store
        .apply(&mut f.g, &c.id, Decision::Discard, Some(&token))?;
    f.store.apply(&mut f.g, &b.id, Decision::Check, None)?;
    assert_eq!(fs::read(&f.first)?, b"high");
    assert_eq!(fs::read(&keys)?, b"bind one");
    assert_eq!(fs::read(&favorites)?, b"server one");
    f.store.apply(&mut f.g, &c.id, Decision::Check, None)?;
    assert_eq!(fs::read(keys)?, b"bind two");
    assert_eq!(fs::read(favorites)?, b"server two");
    assert_eq!(f.store.load(&a, 4)?[0].as_deref(), Some(b"low".as_slice()));
    Ok(())
}
#[test]
fn old_recovery_points_preserve_files_added_after_the_backup() -> Result<()> {
    let mut f = Fixture::new()?;
    f.activate_b()?;
    let old = f.store.profiles(None, true)?.remove(0);
    let keys = f.first.with_file_name("keys.cfg");
    fs::write(&keys, b"keys baseline")?;
    f.store.add_files(&mut f.g, std::slice::from_ref(&keys))?;
    fs::write(&keys, b"keys edited")?;
    let Outcome::Unsaved { token, .. } = f.store.apply(&mut f.g, &old.id, Decision::Check, None)?
    else {
        panic!("expected prompt")
    };
    f.store
        .apply(&mut f.g, &old.id, Decision::Discard, Some(&token))?;
    assert_eq!(fs::read(f.first)?, b"low");
    assert_eq!(fs::read(keys)?, b"keys edited");
    assert_eq!(f.store.profile(&f.g.id, &old.id)?.count, 2);
    Ok(())
}
#[test]
fn adding_files_rolls_back_all_profiles_on_database_failure() -> Result<()> {
    let mut f = Fixture::new()?;
    let keys = f.first.with_file_name("keys.cfg");
    fs::write(&keys, b"bind")?;
    f.store.db.execute_batch(&format!("CREATE TEMP TRIGGER reject_added_file BEFORE INSERT ON files WHEN NEW.slot=2 AND NEW.profile='{}' BEGIN SELECT RAISE(ABORT,'simulated failure'); END;",f.a.id))?;
    assert!(f
        .store
        .add_files(&mut f.g, std::slice::from_ref(&keys))
        .is_err());
    assert_eq!(f.g.maps.len(), 2);
    assert_eq!(f.store.game(&f.g.id)?.maps.len(), 2);
    assert_eq!(f.store.profile(&f.g.id, &f.b.id)?.count, 2);
    assert_eq!(f.store.load(&f.b, 2)?.len(), 2);
    assert_eq!(fs::read(keys)?, b"bind");
    Ok(())
}
#[test]
fn interrupted_restore_of_old_recovery_point_handles_expanded_file_set() -> Result<()> {
    let mut f = Fixture::new()?;
    f.activate_b()?;
    let old = f.store.profiles(None, true)?.remove(0);
    let keys = f.first.with_file_name("keys.cfg");
    fs::write(&keys, b"keys baseline")?;
    f.store.add_files(&mut f.g, std::slice::from_ref(&keys))?;
    let backup = Store::make(&f.g, "Interrupted restore", true)?;
    Store::write(&f.store.db, &backup, &capture(&f.g.maps)?)?;
    f.store.db.execute(
        "INSERT INTO journal VALUES(?,?,?)",
        params![f.g.id, backup.id, old.id],
    )?;
    fs::write(&f.first, b"low")?;
    let mut reopened = Store::open(&f.store.dir)?;
    reopened.recover()?;
    assert_eq!(fs::read(f.first)?, b"high");
    assert_eq!(fs::read(keys)?, b"keys baseline");
    reopened.ready(&f.g)?;
    Ok(())
}

#[test]
fn older_archives_keep_newly_managed_files_at_their_current_settings() -> Result<()> {
    let mut f = Fixture::new()?;
    let path = f._temp.path().join("older.zip");
    archive::export(&path, &f.g, &f.a, &f.store.load(&f.a, 2)?)?;
    let keys = f.first.with_file_name("keys.cfg");
    fs::write(&keys, b"current keys")?;
    f.store.add_files(&mut f.g, std::slice::from_ref(&keys))?;
    let imported = archive::import(&path, &f.g)?;
    assert_eq!(imported.data.len(), 3);
    assert_eq!(imported.data[0].as_deref(), Some(b"low".as_slice()));
    assert_eq!(
        imported.data[2].as_deref(),
        Some(b"current keys".as_slice())
    );
    Ok(())
}
