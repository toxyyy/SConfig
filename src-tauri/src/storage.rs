use crate::files::*;
use anyhow::{ensure, Context, Result};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::{
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};
use uuid::Uuid;

pub fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}
pub fn id() -> String {
    Uuid::new_v4().to_string()
}
pub fn title(s: &str) -> Result<String> {
    let s = s.trim();
    ensure!(
        !s.is_empty() && s.chars().count() <= 120,
        "Use a name between 1 and 120 characters."
    );
    Ok(s.into())
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Game {
    pub id: String,
    pub name: String,
    pub platform: String,
    pub install: PathBuf,
    pub exe: Option<PathBuf>,
    pub maps: Vec<Mapping>,
    pub active: Option<String>,
    pub favorite: bool,
    pub hidden: bool,
    pub modified: i64,
    pub cover: bool,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Profile {
    pub id: String,
    pub game: String,
    pub name: String,
    pub description: String,
    pub tags: Vec<String>,
    pub created: i64,
    pub modified: i64,
    pub count: usize,
    pub recovery: bool,
}

pub struct Store {
    pub db: Connection,
    pub dir: PathBuf,
}
impl Store {
    pub fn open(dir: &Path) -> Result<Self> {
        std::fs::create_dir_all(dir)?;
        let db = Connection::open(dir.join("library.sqlite3"))?;
        db.execute_batch("PRAGMA foreign_keys=ON; PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL;
          CREATE TABLE IF NOT EXISTS games(id TEXT PRIMARY KEY, data TEXT NOT NULL, artwork BLOB);
          CREATE TABLE IF NOT EXISTS profiles(id TEXT PRIMARY KEY,game TEXT NOT NULL REFERENCES games(id),recovery INTEGER NOT NULL,data TEXT NOT NULL);
          CREATE INDEX IF NOT EXISTS by_game ON profiles(game,recovery);
          CREATE TABLE IF NOT EXISTS files(profile TEXT REFERENCES profiles(id) ON DELETE CASCADE,slot INTEGER NOT NULL,bytes BLOB,hash TEXT,PRIMARY KEY(profile,slot));
          CREATE TABLE IF NOT EXISTS journal(game TEXT PRIMARY KEY,backup TEXT NOT NULL REFERENCES profiles(id),target TEXT NOT NULL REFERENCES profiles(id));
          CREATE TABLE IF NOT EXISTS excluded(id TEXT PRIMARY KEY);
          CREATE TABLE IF NOT EXISTS prism_meta(game TEXT PRIMARY KEY REFERENCES games(id),data TEXT NOT NULL);
          CREATE TABLE IF NOT EXISTS media(id TEXT PRIMARY KEY,bytes BLOB NOT NULL);
          CREATE TABLE IF NOT EXISTS prefs(key TEXT PRIMARY KEY,value INTEGER NOT NULL);")?;
        Ok(Self {
            db,
            dir: dir.into(),
        })
    }
    pub fn game(&self, id: &str) -> Result<Game> {
        let s: String = self
            .db
            .query_row("SELECT data FROM games WHERE id=?", [id], |r| r.get(0))
            .context("Game was not found.")?;
        Ok(serde_json::from_str(&s)?)
    }
    pub fn games(&self) -> Result<Vec<Game>> {
        let mut q = self.db.prepare("SELECT data FROM games")?;
        let items = q
            .query_map([], |r| r.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(items
            .iter()
            .map(|s| serde_json::from_str(s))
            .collect::<serde_json::Result<_>>()?)
    }
    pub fn put_game(db: &Connection, g: &Game) -> Result<()> {
        db.execute("INSERT INTO games(id,data) VALUES (?1,?2) ON CONFLICT(id) DO UPDATE SET data=excluded.data",params![g.id,serde_json::to_string(g)?])?;
        Ok(())
    }
    pub fn profile(&self, game: &str, pid: &str) -> Result<Profile> {
        let s: String = self
            .db
            .query_row(
                "SELECT data FROM profiles WHERE id=? AND game=?",
                params![pid, game],
                |r| r.get(0),
            )
            .context("Configuration was not found for this game.")?;
        Ok(serde_json::from_str(&s)?)
    }
    pub fn profiles(&self, game: Option<&str>, recovery: bool) -> Result<Vec<Profile>> {
        let mut q=self.db.prepare("SELECT data FROM profiles WHERE (?1 IS NULL OR game=?1) AND recovery=?2 ORDER BY rowid DESC")?;
        let rows = q
            .query_map(params![game, recovery], |r| r.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows
            .iter()
            .map(|s| serde_json::from_str(s))
            .collect::<serde_json::Result<_>>()?)
    }
    pub fn put_profile(db: &Connection, p: &Profile) -> Result<()> {
        ensure!(
            p.description.len() <= 4096
                && p.tags.len() <= 16
                && p.tags.iter().all(|t| t.len() <= 80),
            "Description or tags are too long."
        );
        db.execute("INSERT INTO profiles(id,game,recovery,data) VALUES(?,?,?,?) ON CONFLICT(id) DO UPDATE SET data=excluded.data",params![p.id,p.game,p.recovery,serde_json::to_string(p)?])?;
        Ok(())
    }
    pub fn load(&self, p: &Profile, count: usize) -> Result<Contents> {
        let mut q = self
            .db
            .prepare("SELECT slot,bytes,hash FROM files WHERE profile=? ORDER BY slot")?;
        let mut rows = q.query([&p.id])?;
        let mut data = Vec::new();
        while let Some(r) = rows.next()? {
            let slot: usize = r.get(0)?;
            let bytes: Option<Vec<u8>> = r.get(1)?;
            let hash: Option<String> = r.get(2)?;
            ensure!(
                slot == data.len() && bytes.as_deref().map(digest) == hash,
                "Stored file integrity check failed."
            );
            data.push(bytes);
        }
        ensure!(
            data.len() == count && p.count == count,
            "Snapshot is missing file mappings."
        );
        Ok(data)
    }
    pub fn write(db: &Connection, p: &Profile, data: &Contents) -> Result<()> {
        Self::put_profile(db, p)?;
        for (slot, bytes) in data.iter().enumerate() {
            db.execute(
                "INSERT OR REPLACE INTO files(profile,slot,bytes,hash) VALUES(?,?,?,?)",
                params![p.id, slot, bytes, bytes.as_deref().map(digest)],
            )?;
        }
        Ok(())
    }
    pub fn make(g: &Game, name: &str, recovery: bool) -> Result<Profile> {
        Ok(Profile {
            id: id(),
            game: g.id.clone(),
            name: title(name)?,
            description: String::new(),
            tags: vec![],
            created: now(),
            modified: now(),
            count: g.maps.len(),
            recovery,
        })
    }
    pub fn retain(&self) -> Result<usize> {
        Ok(self.db.query_row(
            "SELECT COALESCE((SELECT value FROM prefs WHERE key='retention'),20)",
            [],
            |r| r.get(0),
        )?)
    }
    pub fn ready(&self, g: &Game) -> Result<()> {
        ensure!(
            !g.maps.is_empty() && g.exe.is_some(),
            "Select the game executable and its configuration files first."
        );
        ensure!(
            !self.db.query_row(
                "SELECT EXISTS(SELECT 1 FROM journal WHERE game=?)",
                [&g.id],
                |r| r.get::<_, bool>(0)
            )?,
            "An interrupted switch needs recovery. Close the game and retry recovery."
        );
        Ok(())
    }
    pub fn create(
        &mut self,
        g: &mut Game,
        name: &str,
        description: &str,
        tags: Vec<String>,
        source: Option<&str>,
    ) -> Result<Profile> {
        self.ready(g)?;
        let data = match source {
            Some(pid) => self.load(&self.profile(&g.id, pid)?, g.maps.len())?,
            None => {
                let bytes = capture(&g.maps)?;
                ensure!(
                    bytes.iter().all(Option::is_some),
                    "A selected source file is missing. Restore it before capturing a new profile."
                );
                bytes
            }
        };
        let mut p = Self::make(g, name, false)?;
        p.description = description.into();
        p.tags = tags;
        self.insert_snapshot(g, &p, &data, source.is_none())?;
        Ok(p)
    }
    pub fn insert_snapshot(
        &mut self,
        g: &mut Game,
        p: &Profile,
        data: &Contents,
        from_live: bool,
    ) -> Result<()> {
        let tx = self.db.transaction()?;
        Self::write(&tx, p, data)?;
        if from_live && g.active.is_none() {
            g.active = Some(p.id.clone());
        }
        g.modified = now();
        Self::put_game(&tx, g)?;
        tx.commit()?;
        Ok(())
    }
    pub fn add_files(&mut self, g: &mut Game, files: &[PathBuf]) -> Result<()> {
        self.ready(g)?;
        ensure!(!files.is_empty(), "Select configuration files to add.");
        let mut updated = g.clone();
        let mut destinations = std::collections::HashSet::new();
        for m in &g.maps {
            destinations.insert(m.destination()?.to_string_lossy().to_lowercase());
        }
        let executable = g.exe.as_deref().and_then(|p| std::fs::canonicalize(p).ok());
        let mut additions = vec![];
        for path in files {
            let mapping = Mapping::select(path, &self.dir)?;
            let dest = mapping.destination()?;
            ensure!(
                executable.as_ref() != Some(&dest),
                "The game executable cannot be managed as a configuration."
            );
            if destinations.insert(dest.to_string_lossy().to_lowercase()) {
                additions.push(mapping);
            }
        }
        ensure!(!additions.is_empty(), "These files are already managed.");
        updated.maps.extend(additions.iter().cloned());
        ensure!(
            updated.maps.len() <= MAX_FILES,
            "A game can manage up to 128 files."
        );
        capture(&updated.maps)?;
        let baseline = capture(&additions)?;
        ensure!(
            baseline.iter().all(Option::is_some),
            "A newly selected file is missing."
        );
        let mut snapshots = vec![];
        for mut profile in self.profiles(Some(&g.id), false)? {
            let mut bytes = self.load(&profile, g.maps.len())?;
            bytes.extend(baseline.iter().cloned());
            ensure!(
                bytes.iter().flatten().map(Vec::len).sum::<usize>() <= MAX_TOTAL,
                "An extended profile exceeds 64 MiB."
            );
            profile.count = updated.maps.len();
            profile.modified = now();
            snapshots.push((profile, bytes));
        }
        updated.modified = now();
        let tx = self.db.transaction()?;
        for (profile, bytes) in &snapshots {
            Self::write(&tx, profile, bytes)?;
        }
        // Existing recovery points keep their original file coverage.
        Self::put_game(&tx, &updated)?;
        tx.commit()?;
        *g = updated;
        Ok(())
    }
    pub fn save(&mut self, g: &Game) -> Result<()> {
        self.ready(g)?;
        let mut p = self.profile(&g.id, g.active.as_deref().context("No active profile.")?)?;
        let data = capture(&g.maps)?;
        p.modified = now();
        let tx = self.db.transaction()?;
        Self::write(&tx, &p, &data)?;
        tx.commit()?;
        Ok(())
    }
    pub fn auto_save(&mut self, expected: &Game, bytes: &Contents) -> Result<bool> {
        let g = self.game(&expected.id)?;
        self.ready(&g)?;
        ensure!(
            g.active == expected.active && g.maps == expected.maps,
            "The active configuration changed while settings were being saved."
        );
        ensure!(
            bytes.len() == g.maps.len() && bytes.iter().all(Option::is_some),
            "Waiting for all configuration files to finish writing."
        );
        ensure!(
            capture(&g.maps)? == *bytes,
            "Settings are still being written."
        );
        let Some(id) = &g.active else {
            return Ok(false);
        };
        let mut profile = self.profile(&g.id, id)?;
        let previous = self.load(&profile, g.maps.len())?;
        if previous == *bytes {
            return Ok(false);
        }
        let last = self
            .profiles(Some(&g.id), true)?
            .iter()
            .filter(|p| p.name == "Before automatic save")
            .map(|p| p.created)
            .max()
            .unwrap_or(0);
        let recovery = (now() - last >= 30_000)
            .then(|| Self::make(&g, "Before automatic save", true))
            .transpose()?;
        let retention = self.retain()?;
        let mut updated = g.clone();
        updated.modified = now();
        profile.modified = now();
        let tx = self.db.transaction()?;
        if let Some(point) = recovery {
            Self::write(&tx, &point, &previous)?;
        }
        Self::write(&tx, &profile, bytes)?;
        Self::put_game(&tx, &updated)?;
        tx.execute("DELETE FROM profiles WHERE id IN (SELECT id FROM profiles WHERE game=? AND recovery=1 AND id NOT IN (SELECT backup FROM journal UNION SELECT target FROM journal) ORDER BY rowid DESC LIMIT -1 OFFSET ?)",params![g.id,retention])?;
        tx.commit()?;
        Ok(true)
    }
    pub fn changed(&self, g: &Game) -> Result<Vec<String>> {
        let Some(pid) = &g.active else {
            return Ok(vec![]);
        };
        let old = self.load(&self.profile(&g.id, pid)?, g.maps.len())?;
        Ok(g.maps
            .iter()
            .zip(capture(&g.maps)?.iter().zip(old.iter()))
            .filter(|(_, (a, b))| a != b)
            .map(|(m, _)| m.root.join(&m.name).display().to_string())
            .collect())
    }
}
