#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod desktop;
mod monitoring;
use sconfig::{archive, discovery, files, migration, storage, switching};
use std::sync::Mutex;
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
            let portable = std::env::args_os().any(|a| a == "--portable");
            let dir = if portable {
                std::env::current_exe()?
                    .parent()
                    .ok_or("Missing executable folder")?
                    .join("SConfigData")
            } else {
                app.path().app_local_data_dir()?
            };
            // These names are retained solely to import installations made before the rename.
            let previous = dir
                .parent()
                .ok_or("Missing workspace parent")?
                .join(if portable {
                    "SwitchboardData"
                } else {
                    "local.switchboard.configmanager"
                });
            if !dir.exists() && previous.join("library.sqlite3").is_file() {
                if let Err(error) = files::game_closed(std::path::Path::new("switchboard.exe")) {
                    rfd::MessageDialog::new()
                        .set_title("SConfig")
                        .set_description("Close the previous version of the app, then open SConfig again. SConfig will import your saved library and profiles on first launch.")
                        .set_level(rfd::MessageLevel::Warning)
                        .show();
                    return Err(error.into());
                }
                migration::import_workspace(&previous, &dir)?;
            }
            let store = storage::Store::open(&dir)?;
            let (changes,receiver)=std::sync::mpsc::channel();
            app.manage(Mutex::new(desktop::Session {
                store,
                grants: Default::default(),
                watcher: None,
                changes,
            }));
            monitoring::start(app.handle().clone(),receiver);
            let window = app
                .config()
                .app
                .windows
                .first()
                .ok_or("Missing main window configuration")?;
            let window=tauri::WebviewWindowBuilder::from_config(app.handle(), window)?.build()?;
            window.set_icon(tauri::image::Image::new_owned(include_bytes!("../icons/sconfig.rgba").to_vec(),128,128))?;
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
        .expect("Unable to start SConfig");
}
