#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod desktop;
use std::sync::Mutex;
use switchboard::{archive, discovery, files, storage, switching};
use tauri::Manager;

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _, _| {
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.unminimize();
                let _ = w.set_focus();
            }
        }))
        .setup(|app| {
            let dir = if std::env::args_os().any(|a| a == "--portable") {
                std::env::current_exe()?
                    .parent()
                    .ok_or("Missing executable folder")?
                    .join("SwitchboardData")
            } else {
                app.path().app_local_data_dir()?
            };
            let store = storage::Store::open(&dir)?;
            app.manage(Mutex::new(desktop::Session {
                store,
                grants: Default::default(),
                watcher: None,
            }));
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::DragDrop(tauri::DragDropEvent::Drop { paths, .. }) = event {
                if paths.len() == 1
                    && paths[0]
                        .extension()
                        .is_some_and(|e| e.eq_ignore_ascii_case("zip"))
                {
                    let app = window.app_handle().clone();
                    let path = paths[0].clone();
                    tauri::async_runtime::spawn_blocking(move || {
                        desktop::dropped_archive(&app, path)
                    });
                }
            }
        })
        .invoke_handler(tauri::generate_handler![desktop::request])
        .run(tauri::generate_context!())
        .expect("Unable to start Switchboard");
}
