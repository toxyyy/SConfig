use crate::desktop::Session;
use sconfig::files::capture;
use std::{
    collections::HashMap,
    sync::{
        mpsc::{Receiver, RecvTimeoutError},
        Mutex,
    },
    time::{Duration, Instant},
};
use tauri::{AppHandle, Emitter, Manager};
pub fn start(app: AppHandle, rx: Receiver<String>) {
    std::thread::spawn(move || {
        let mut pending: HashMap<String, (Instant, u8)> = HashMap::new();
        loop {
            match rx.recv_timeout(Duration::from_millis(200)) {
                Ok(id) => {
                    pending.insert(id, (Instant::now(), 0));
                }
                Err(RecvTimeoutError::Disconnected) => break,
                Err(RecvTimeoutError::Timeout) => {}
            }
            let ready: Vec<_> = pending
                .iter()
                .filter(|(_, v)| v.0.elapsed() >= Duration::from_millis(800))
                .map(|(id, v)| (id.clone(), v.1))
                .collect();
            for (id, retries) in ready {
                pending.remove(&id);
                let result = (|| -> anyhow::Result<()> {
                    let state = app.state::<Mutex<Session>>();
                    let frame = {
                        let s = state
                            .lock()
                            .map_err(|_| anyhow::anyhow!("The workspace is unavailable."))?;
                        let g = s.store.game(&id)?;
                        if g.active.is_none() {
                            return Ok(());
                        }
                        s.store.ready(&g)?;
                        let data = capture(&g.maps)?;
                        (g, data)
                    };
                    std::thread::sleep(Duration::from_millis(150));
                    let mut s = state
                        .lock()
                        .map_err(|_| anyhow::anyhow!("The workspace is unavailable."))?;
                    if s.store.auto_save(&frame.0, &frame.1)? {
                        let _ = app.emit("profile-auto-saved", &id);
                    }
                    Ok(())
                })();
                if let Err(error) = result {
                    if retries < 3 {
                        pending.insert(id, (Instant::now(), retries + 1));
                    } else {
                        let _=app.emit("auto-save-error",serde_json::json!({"game":id,"message":format!("Automatic save is waiting: {error:#}")}));
                    }
                }
            }
        }
    });
}
pub fn refresh(s: &mut Session) -> anyhow::Result<()> {
    use notify::{RecursiveMode, Watcher};
    let mut paths: HashMap<std::path::PathBuf, Vec<String>> = HashMap::new();
    let mut roots = std::collections::HashSet::new();
    for g in s.store.games()? {
        for map in &g.maps {
            if let Ok(path) = map.destination() {
                paths.entry(path).or_default().push(g.id.clone());
                roots.insert(map.root.clone());
            }
        }
        if g.active.is_some() {
            let _ = s.changes.send(g.id);
        }
    }
    let changes = s.changes.clone();
    let mut watcher = notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
        if let Ok(event) = event {
            for path in event.paths {
                let path = std::fs::canonicalize(&path).unwrap_or(path);
                if let Some(ids) = paths.get(&path) {
                    for id in ids {
                        let _ = changes.send(id.clone());
                    }
                }
            }
        }
    })?;
    for root in roots {
        watcher.watch(&root, RecursiveMode::NonRecursive)?;
    }
    s.watcher = Some(watcher);
    Ok(())
}
