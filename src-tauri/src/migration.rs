use crate::files::check_path;
use anyhow::{Context, Result};
use rusqlite::{Connection, DatabaseName, OpenFlags};
use std::{fs, path::Path};

/// Import a previous workspace once, publishing it only after the copy succeeds.
pub fn import_workspace(previous: &Path, destination: &Path) -> Result<bool> {
    let database = previous.join("library.sqlite3");
    if destination.exists() || !database.is_file() {
        return Ok(false);
    }
    check_path(&database, false)?;
    let parent = destination.parent().context("Missing workspace parent.")?;
    fs::create_dir_all(parent)?;
    let staging = tempfile::tempdir_in(parent)?;
    let source = Connection::open_with_flags(&database, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    source.backup(
        DatabaseName::Main,
        staging.path().join("library.sqlite3"),
        None,
    )?;
    drop(source);

    for name in [
        "profiles",
        "recovery",
        "artwork",
        "logs",
        "EBWebView/Default/Local Storage",
    ] {
        let source = previous.join(name);
        if source.exists() {
            copy_directory(&source, &staging.path().join(name))?;
        }
    }
    fs::rename(staging.path(), destination)
        .context("Could not finish importing the previous workspace.")?;
    Ok(true)
}

fn copy_directory(source: &Path, destination: &Path) -> Result<()> {
    check_path(source, false)?;
    fs::create_dir_all(destination)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let path = entry.path();
        check_path(&path, false)?;
        let target = destination.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_directory(&path, &target)?;
        } else if entry.file_type()?.is_file() {
            fs::copy(path, target)?;
        }
    }
    Ok(())
}
