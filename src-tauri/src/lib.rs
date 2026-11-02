mod clock;
mod commands;
mod db;
mod dedup;
mod error;
mod model;
mod offers;
mod secrets;
mod settings;

use std::sync::Mutex;

use tauri::Manager;

use commands::AppState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let mut builder = tauri::Builder::default();

    #[cfg(desktop)]
    {
        // A second copy of the app only brings the first window to the front.
        builder = builder
            .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.unminimize();
                    let _ = window.set_focus();
                }
            }))
            .plugin(tauri_plugin_window_state::Builder::default().build());
    }

    builder
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let state = match open_database(app) {
                Ok(conn) => Ok(conn),
                Err(e) => {
                    eprintln!("database unavailable: {e}");
                    Err(e.to_string())
                }
            };
            app.manage(AppState { db: Mutex::new(state) });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::app_status,
            commands::list_offers,
            commands::get_offer,
            commands::add_offer,
            commands::set_stage,
            commands::set_interest,
            commands::set_favorite,
            commands::set_notes,
            commands::trash_offer,
            commands::restore_offer,
            commands::purge_offers,
            commands::get_settings,
            commands::set_setting,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

fn open_database(app: &tauri::App) -> error::AppResult<rusqlite::Connection> {
    secrets::init()?;
    let mut dir = app
        .path()
        .app_data_dir()
        .map_err(|e| error::AppError::Locked(format!("no data folder: {e}")))?;
    // Development builds never touch real data.
    if cfg!(debug_assertions) {
        dir.push("dev");
    }
    db::open_app_database(&dir)
}
