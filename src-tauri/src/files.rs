use anyhow::{bail, ensure, Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{Read, Write},
    path::{Component, Path, PathBuf},
};
use tempfile::NamedTempFile;

pub const MAX_FILE: usize = 16 * 1024 * 1024;
pub const MAX_TOTAL: usize = 64 * 1024 * 1024;
pub const MAX_FILES: usize = 128;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Mapping {
    pub root: PathBuf,
    pub name: String,
}

pub type Contents = Vec<Option<Vec<u8>>>;

pub fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

pub fn safe_name(name: &str) -> Result<()> {
    let stem = name
        .split('.')
        .next()
        .unwrap_or("")
        .trim_end()
        .to_ascii_uppercase();
    ensure!(
        !name.is_empty()
            && name != "."
            && name != ".."
            && !name.ends_with(['.', ' '])
            && !name.contains(['/', '\\', ':', '<', '>', '"', '|', '?', '*'])
            && !name.chars().any(char::is_control)
            && !matches!(
                stem.as_str(),
                "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$"
            )
            && !((stem.starts_with("COM") || stem.starts_with("LPT")) && stem.chars().count() == 4),
        "Invalid managed filename."
    );
    Ok(())
}

pub fn check_path(path: &Path, allow_missing_leaf: bool) -> Result<()> {
    ensure!(
        path.is_absolute() && !path.components().any(|c| matches!(c, Component::ParentDir)),
        "Select an absolute path without traversal."
    );
    #[cfg(windows)]
    ensure!(
        matches!(path.components().next(),Some(Component::Prefix(p)) if matches!(p.kind(),std::path::Prefix::Disk(_) | std::path::Prefix::VerbatimDisk(_))),
        "Choose a local drive."
    );
    for part in path.ancestors().filter(|p| p.parent().is_some()) {
        let m = match fs::symlink_metadata(part) {
            Ok(m) => m,
            Err(e)
                if e.kind() == std::io::ErrorKind::NotFound
                    && allow_missing_leaf
                    && part == path =>
            {
                continue
            }
            Err(e) => return Err(e).with_context(|| format!("Cannot access {}", part.display())),
        };
        #[cfg(windows)]
        let linked = {
            use std::os::windows::fs::MetadataExt;
            m.file_attributes() & 0x400 != 0
        };
        #[cfg(not(windows))]
        let linked = m.file_type().is_symlink();
        ensure!(
            !linked,
            "A link or redirected folder was found at {}.",
            part.display()
        );
    }
    Ok(())
}

impl Mapping {
    pub fn select(path: &Path, store: &Path) -> Result<Self> {
        check_path(path, false)?;
        ensure!(path.is_file(), "Choose a configuration file.");
        let full = fs::canonicalize(path)?;
        ensure!(
            !full.starts_with(fs::canonicalize(store)?),
            "Application data cannot be a game configuration."
        );
        let name = full
            .file_name()
            .and_then(|n| n.to_str())
            .context("Unsupported filename.")?
            .to_owned();
        safe_name(&name)?;
        Ok(Self {
            root: full
                .parent()
                .context("No configuration folder.")?
                .to_owned(),
            name,
        })
    }
    pub fn destination(&self) -> Result<PathBuf> {
        safe_name(&self.name)?;
        let path = self.root.join(&self.name);
        check_path(&path, true)?;
        ensure!(
            self.root.is_dir() && fs::canonicalize(&self.root)? == self.root,
            "Configuration folder moved or was redirected."
        );
        Ok(path)
    }
}

pub fn read_file(path: &Path) -> Result<Option<Vec<u8>>> {
    let mut options = fs::OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.custom_flags(windows_sys::Win32::Storage::FileSystem::FILE_FLAG_OPEN_REPARSE_POINT);
    }
    let file = match options.open(path) {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => {
            return Err(e).with_context(|| {
                format!(
                    "Cannot read {}. Close applications using it and check permissions.",
                    path.display()
                )
            })
        }
    };
    let meta = file.metadata()?;
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        ensure!(
            meta.file_attributes() & 0x400 == 0,
            "Refusing to read a redirected file."
        );
    }
    ensure!(
        meta.is_file() && meta.len() <= MAX_FILE as u64,
        "Unsupported file type or size (16 MiB limit)."
    );
    let mut bytes = Vec::with_capacity(meta.len() as usize);
    file.take(MAX_FILE as u64 + 1).read_to_end(&mut bytes)?;
    ensure!(bytes.len() <= MAX_FILE, "File grew beyond the size limit.");
    Ok(Some(bytes))
}

pub fn capture(maps: &[Mapping]) -> Result<Contents> {
    let mut total = 0;
    maps.iter()
        .map(|m| {
            let bytes = read_file(&m.destination()?)?;
            total += bytes.as_ref().map_or(0, Vec::len);
            ensure!(total <= MAX_TOTAL, "Configuration exceeds 64 MiB.");
            Ok(bytes)
        })
        .collect()
}

pub fn replace(mapping: &Mapping, before: &Option<Vec<u8>>, after: &Option<Vec<u8>>) -> Result<()> {
    #[cfg(windows)]
    let _directories = guard_directories(&mapping.root)?;
    let path = mapping.destination()?;
    ensure!(
        read_file(&path)? == *before,
        "{} changed during the operation. Retry after the game closes.",
        mapping.name
    );
    if before == after {
        return Ok(());
    }
    if let Ok(m) = fs::metadata(&path) {
        ensure!(
            !m.permissions().readonly(),
            "{} is read-only.",
            mapping.name
        );
    }
    if let Some(bytes) = after {
        let mut stage = NamedTempFile::new_in(&mapping.root)?;
        stage.write_all(bytes)?;
        stage.as_file().sync_all()?;
        ensure!(
            read_file(stage.path())?.as_ref() == Some(bytes),
            "Staged file verification failed."
        );
        mapping.destination()?;
        ensure!(
            read_file(&path)? == *before,
            "{} changed while staging.",
            mapping.name
        );
        stage.persist(&path).with_context(|| {
            format!(
                "Cannot replace {}. Check locks and permissions.",
                mapping.name
            )
        })?;
    } else {
        fs::remove_file(&path)?;
    }
    ensure!(
        read_file(&path)? == *after,
        "Verification failed for {}.",
        mapping.name
    );
    Ok(())
}

#[cfg(windows)]
fn guard_directories(root: &Path) -> Result<Vec<fs::File>> {
    use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
    use windows_sys::Win32::Storage::FileSystem::*;
    let mut chain: Vec<_> = root.ancestors().filter(|p| p.parent().is_some()).collect();
    chain.reverse();
    let mut guards = Vec::new();
    // Pin each ancestor before resolving the next. Denying delete sharing
    // prevents a directory from being renamed out from under a staged write.
    for dir in chain {
        let handle = fs::OpenOptions::new()
            .access_mode(FILE_READ_ATTRIBUTES)
            .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
            .open(dir)
            .with_context(|| format!("Cannot protect configuration folder {}", dir.display()))?;
        let meta = handle.metadata()?;
        ensure!(
            meta.is_dir() && meta.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT == 0,
            "Configuration folder redirected."
        );
        guards.push(handle);
    }
    Ok(guards)
}

#[cfg(windows)]
pub fn game_closed(exe: &Path) -> Result<()> {
    use windows_sys::Win32::{
        Foundation::{CloseHandle, GetLastError, ERROR_NO_MORE_FILES, INVALID_HANDLE_VALUE},
        System::Diagnostics::ToolHelp::*,
    };
    let name = exe
        .file_name()
        .and_then(|n| n.to_str())
        .context("Game executable is missing.")?;
    // Toolhelp exposes process names even when process handles cannot be opened.
    unsafe {
        let handle = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        ensure!(
            handle != INVALID_HANDLE_VALUE,
            "Could not check running games."
        );
        let mut entry: PROCESSENTRY32W = std::mem::zeroed();
        entry.dwSize = std::mem::size_of_val(&entry) as u32;
        let mut ok = Process32FirstW(handle, &mut entry);
        while ok != 0 {
            let end = entry
                .szExeFile
                .iter()
                .position(|&v| v == 0)
                .unwrap_or(entry.szExeFile.len());
            if String::from_utf16_lossy(&entry.szExeFile[..end]).to_lowercase()
                == name.to_lowercase()
            {
                CloseHandle(handle);
                bail!("Close {name} before changing live settings.");
            }
            ok = Process32NextW(handle, &mut entry);
        }
        let err = GetLastError();
        CloseHandle(handle);
        ensure!(
            err == ERROR_NO_MORE_FILES,
            "Could not complete the running-game check."
        );
    }
    Ok(())
}
#[cfg(not(windows))]
pub fn game_closed(_: &Path) -> Result<()> {
    bail!("Game-running protection requires Windows.")
}
