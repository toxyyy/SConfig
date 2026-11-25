use crate::{files::*, storage::*};
use anyhow::{anyhow, bail, ensure, Context, Result};
use rusqlite::params;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, PartialEq, Debug)]
#[serde(rename_all = "camelCase")]
pub enum Decision {
    Check,
    Save,
    Discard,
}

#[derive(Serialize, Debug)]
#[serde(tag = "status", rename_all = "camelCase")]
pub enum Outcome {
    Applied,
    Unsaved { token: String, files: Vec<String> },
}

impl Store {
    pub fn apply(
        &mut self,
        g: &mut Game,
        pid: &str,
        decision: Decision,
        token: Option<&str>,
    ) -> Result<Outcome> {
        self.apply_with(g, pid, decision, token, |_| Ok(()))
    }
    pub(crate) fn apply_with(
        &mut self,
        g: &mut Game,
        pid: &str,
        decision: Decision,
        token: Option<&str>,
        mut before_write: impl FnMut(usize) -> Result<()>,
    ) -> Result<Outcome> {
        self.ready(g)?;
        ensure_closed(g)?;
        let target = self.profile(&g.id, pid)?;
        let stage = self.load(&target, g.maps.len())?;
        let live = capture(&g.maps)?;
        let active = g
            .active
            .as_ref()
            .map(|id| self.profile(&g.id, id))
            .transpose()?;
        let previous = active
            .as_ref()
            .map(|p| self.load(p, g.maps.len()))
            .transpose()?;
        let changed: Vec<_> = previous
            .as_ref()
            .map(|old| {
                g.maps
                    .iter()
                    .zip(old.iter().zip(&live))
                    .filter(|(_, (a, b))| a != b)
                    .map(|(m, _)| m.root.join(&m.name).display().to_string())
                    .collect()
            })
            .unwrap_or_default();
        let revision = digest(&serde_json::to_vec(&(
            &g.active,
            pid,
            live.iter()
                .map(|b| b.as_deref().map(digest))
                .collect::<Vec<_>>(),
        ))?);
        if !changed.is_empty() && (decision == Decision::Check || token != Some(&revision)) {
            return Ok(Outcome::Unsaved {
                token: revision,
                files: changed,
            });
        }
        ensure!(
            !(decision == Decision::Save && g.active.as_deref() == Some(pid)),
            "Choose discard to revert the active configuration."
        );
        let backup = Self::make(
            g,
            &format!(
                "Before {}",
                target.name.chars().take(90).collect::<String>()
            ),
            true,
        )?;
        let tx = self.db.transaction()?;
        if decision == Decision::Save {
            if let Some(mut p) = active {
                p.modified = now();
                Self::write(&tx, &p, &live)?;
            }
        }
        Self::write(&tx, &backup, &live)?;
        tx.execute(
            "INSERT INTO journal(game,backup,target) VALUES(?,?,?)",
            params![g.id, backup.id, pid],
        )?;
        tx.commit()?;
        let result = (|| -> Result<()> {
            ensure_closed(g)?;
            for (i, map) in g.maps.iter().enumerate() {
                before_write(i)?;
                replace(map, &live[i], &stage[i])?;
            }
            ensure!(
                capture(&g.maps)? == stage,
                "Live settings changed during verification."
            );
            let mut updated = g.clone();
            // A recovery point may contain previously unsaved changes; it has no active profile.
            updated.active = (!target.recovery).then(|| pid.to_string());
            updated.modified = now();
            let retention = self.retain()?;
            let tx = self.db.transaction()?;
            Self::put_game(&tx, &updated)?;
            tx.execute("DELETE FROM journal WHERE game=?", [&g.id])?;
            tx.execute("DELETE FROM profiles WHERE id IN (SELECT id FROM profiles WHERE game=? AND recovery=1 ORDER BY rowid DESC LIMIT -1 OFFSET ?)",params![g.id,retention])?;
            tx.commit()?;
            *g = updated;
            Ok(())
        })();
        if let Err(err) = result {
            if let Err(rollback) = self.recover_game(g) {
                bail!("Switch failed: {err:#}. Recovery is pending: {rollback:#}. The backup is retained.")
            }
            bail!("Switch failed: {err:#}. Previous settings were restored.")
        }
        Ok(Outcome::Applied)
    }

    fn recover_game(&mut self, g: &Game) -> Result<()> {
        ensure_closed(g)?;
        let (bid, tid): (String, String) = self.db.query_row(
            "SELECT backup,target FROM journal WHERE game=?",
            [&g.id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        let backup = self.load(&self.profile(&g.id, &bid)?, g.maps.len())?;
        let stage = self.load(&self.profile(&g.id, &tid)?, g.maps.len())?;
        for (i, map) in g.maps.iter().enumerate().rev() {
            let current = read_file(&map.destination()?)?;
            if current == backup[i] {
                continue;
            }
            ensure!(current==stage[i],"{} was edited during the interrupted operation. Its current bytes and the backup have been preserved.",map.name);
            replace(map, &current, &backup[i])?;
        }
        ensure!(capture(&g.maps)? == backup, "Rollback verification failed.");
        self.db
            .execute("DELETE FROM journal WHERE game=?", [&g.id])?;
        Ok(())
    }
    pub fn recover(&mut self) -> Result<Vec<String>> {
        let ids = self
            .db
            .prepare("SELECT game FROM journal")?
            .query_map([], |r| r.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let mut report = vec![];
        for id in ids {
            let game = self.game(&id)?;
            match self.recover_game(&game) {
                Ok(()) => report.push(format!(
                    "Restored {} after an interrupted switch.",
                    game.name
                )),
                Err(e) => report.push(format!("{} needs recovery: {e:#}", game.name)),
            }
        }
        Ok(report)
    }
    pub fn remove(&mut self, g: &Game, pid: &str) -> Result<()> {
        self.ready(g)?;
        self.profile(&g.id, pid)?;
        if g.active.as_deref() == Some(pid) {
            return Err(anyhow!(
                "Activate a different profile before deleting this one."
            ));
        }
        self.db.execute("DELETE FROM profiles WHERE id=?", [pid])?;
        Ok(())
    }
}

fn ensure_closed(g: &Game) -> Result<()> {
    game_closed(g.exe.as_deref().context("Select the game executable.")?)?;
    if g.platform == "Prism Launcher" {
        for exe in ["java.exe", "javaw.exe"] {
            game_closed(std::path::Path::new(exe))?;
        }
    }
    if g.name.to_lowercase().contains("siege") {
        for exe in [
            "RainbowSix.exe",
            "RainbowSix_Vulkan.exe",
            "RainbowSix_DX11.exe",
        ] {
            game_closed(std::path::Path::new(exe))?;
        }
    }
    Ok(())
}
