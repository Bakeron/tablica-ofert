mod cities;
mod clock;
mod commands;
mod db;
mod dedup;
mod error;
mod filter;
mod model;
mod offers;
mod profiles;
mod secrets;
mod settings;
mod sources;
mod text;
mod usage;
mod net;

use std::sync::Mutex;
use std::sync::atomic::AtomicBool;

use std::time::Duration;

use tauri::{Emitter, Manager};

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
            app.manage(AppState { db: Mutex::new(state), fetching: AtomicBool::new(false) });
            start_auto_fetch(app.handle().clone());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::app_status,
            commands::list_offers,
            commands::count_offers,
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
            commands::list_profiles,
            commands::save_profile,
            commands::delete_profile,
            commands::get_experience,
            commands::set_experience,
            commands::secret_hints,
            commands::set_secret,
            commands::delete_secret,
            commands::source_status,
            commands::fetch_offers,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

/// While the app is open: a first look shortly after start, then every 30 minutes.
/// Each source decides by itself whether it is due (its own interval and daily limit).
fn start_auto_fetch(handle: tauri::AppHandle) {
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_secs(20)).await;
        loop {
            let state = handle.state::<AppState>();
            let auto = commands::with_db(&state, |c| settings::is_on(c, "fetch_auto")).unwrap_or(false);
            if auto {
                match sources::fetch(&state, false).await {
                    Ok(report) if !report.runs.is_empty() => {
                        let _ = handle.emit("offers-fetched", &report);
                    }
                    Ok(_) => {}
                    Err(e) => eprintln!("automatic fetch failed: {e}"),
                }
            }
            tokio::time::sleep(Duration::from_secs(30 * 60)).await;
        }
    });
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
