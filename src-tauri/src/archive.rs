use crate::{files::*, storage::*};
use anyhow::{ensure, Context, Result};
use serde::{Deserialize, Serialize};
use std::{
    fs::File,
    io::{Read, Write},
    path::Path,
};
use zip::{write::SimpleFileOptions, ZipArchive, ZipWriter};

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    version: u32,
    name: String,
    description: String,
    tags: Vec<String>,
    files: Vec<Entry>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Entry {
    name: String,
    hash: Option<String>,
}

pub fn export(path: &Path, g: &Game, p: &Profile, data: &Contents) -> Result<()> {
    let parent = path.parent().context("Select an archive destination.")?;
    let mut temp = tempfile::NamedTempFile::new_in(parent)?;
    let mut zip = ZipWriter::new(temp.as_file_mut());
    let manifest = Manifest {
        version: 1,
        name: p.name.clone(),
        description: p.description.clone(),
        tags: p.tags.clone(),
        files: g
            .maps
            .iter()
            .zip(data)
            .map(|(m, b)| Entry {
                name: m.name.clone(),
                hash: b.as_deref().map(digest),
            })
            .collect(),
    };
    let opts = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    zip.start_file("manifest.json", opts)?;
    zip.write_all(&serde_json::to_vec_pretty(&manifest)?)?;
    for (i, bytes) in data.iter().enumerate() {
        if let Some(b) = bytes {
            zip.start_file(format!("files/{i}"), opts)?;
            zip.write_all(b)?;
        }
    }
    zip.finish()?;
    temp.as_file().sync_all()?;
    temp.persist(path)?;
    Ok(())
}

pub struct Imported {
    pub name: String,
    pub description: String,
    pub tags: Vec<String>,
    pub data: Contents,
}
pub fn import(path: &Path, g: &Game) -> Result<Imported> {
    let f = File::open(path)?;
    ensure!(
        f.metadata()?.len() <= MAX_TOTAL as u64 + 1024 * 1024,
        "Archive exceeds the size limit."
    );
    let mut zip = ZipArchive::new(f).context("Invalid ZIP archive.")?;
    ensure!(zip.len() <= MAX_FILES + 1, "Too many archive entries.");
    let manifest: Manifest = {
        let mut file = zip.by_name("manifest.json")?;
        let mut bytes = vec![];
        (&mut file).take(65537).read_to_end(&mut bytes)?;
        ensure!(bytes.len() <= 65536, "Archive manifest is too large.");
        serde_json::from_slice(&bytes)?
    };
    ensure!(
        manifest.version == 1 && !manifest.files.is_empty() && manifest.files.len() <= g.maps.len(),
        "Incompatible archive version or file count."
    );
    let mut expected = std::collections::HashSet::from(["manifest.json".to_owned()]);
    let mut data = vec![];
    let mut total = 0;
    for (i, (entry, map)) in manifest.files.iter().zip(&g.maps).enumerate() {
        safe_name(&entry.name)?;
        ensure!(
            entry.name.eq_ignore_ascii_case(&map.name),
            "Mapping {} must match {} in this game's file list.",
            i + 1,
            entry.name
        );
        if let Some(hash) = &entry.hash {
            let key = format!("files/{i}");
            expected.insert(key.clone());
            let mut file = zip.by_name(&key)?;
            ensure!(
                file.is_file() && file.size() <= MAX_FILE as u64,
                "Invalid archived file."
            );
            let mut b = vec![];
            (&mut file).take(MAX_FILE as u64 + 1).read_to_end(&mut b)?;
            total += b.len();
            ensure!(
                total <= MAX_TOTAL && b.len() <= MAX_FILE && digest(&b) == *hash,
                "Archive integrity or size check failed."
            );
            data.push(Some(b));
        } else {
            data.push(None);
        }
    }
    ensure!(zip.len() == expected.len(), "Unexpected archive entries.");
    for i in 0..zip.len() {
        let file = zip.by_index(i)?;
        ensure!(
            expected.remove(file.name()) && file.unix_mode().unwrap_or(0) & 0o170000 != 0o120000,
            "Unsafe or repeated archive entry."
        );
    }
    let remaining = capture(&g.maps[data.len()..])?;
    ensure!(
        remaining.iter().all(Option::is_some),
        "Restore the newly managed files before importing an older archive."
    );
    data.extend(remaining);
    ensure!(
        data.iter().flatten().map(Vec::len).sum::<usize>() <= MAX_TOTAL,
        "Imported configuration exceeds 64 MiB."
    );
    Ok(Imported {
        name: title(&manifest.name)?,
        description: manifest.description,
        tags: manifest.tags,
        data,
    })
}
