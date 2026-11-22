mod agenda;
mod ai;
mod backup;
mod cities;
mod dates;
mod clock;
mod commands;
mod db;
mod dedup;
mod dups;
mod english;
mod diag;
mod error;
mod extract;
mod fieldcheck;
mod filter;
mod hardware;
mod keycopy;
mod model;
mod notify;
mod offers;
mod posting;
mod learn;
mod preview;
mod profiles;
mod secrets;
mod scheduler;
mod score;
mod settings;
mod sources;
mod same;
mod sync;
mod synonyms;
mod text;
mod tree;
mod trim;
#[cfg(target_os = "android")]
mod tls;
mod usage;
mod nbp;
mod net;

use std::sync::Mutex;
use std::sync::atomic::AtomicBool;

use std::time::Duration;

use tauri::{Emitter, Manager};

use commands::AppState;

/// A background run got a window (the person started the app meanwhile) - it must not quit.
static WINDOW_OPENED: AtomicBool = AtomicBool::new(false);

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let args: Vec<String> = std::env::args().collect();
    // Started by Task Scheduler: no window - collect, notify, quit.
    let background = scheduler::is_background(&args);
    let mut builder = tauri::Builder::default();

    #[cfg(desktop)]
    {
        // A second copy of the app only brings the first window to the front;
        // a background run while the app is open simply ends (the open app collects itself).
        builder = builder
            .plugin(tauri_plugin_single_instance::init(|app, args, _cwd| {
                if scheduler::is_background(&args) {
                    return;
                }
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.unminimize();
                    let _ = window.set_focus();
                } else if let Some(config) = app.config().app.windows.iter().find(|w| w.label == "main") {
                    // The person started the app during a background run (which has no window):
                    // the window opens now and the app stays after the run.
                    if tauri::WebviewWindowBuilder::from_config(app, config).and_then(|b| b.build()).is_ok() {
                        WINDOW_OPENED.store(true, std::sync::atomic::Ordering::SeqCst);
                        start_auto_fetch(app.clone());
                    }
                }
            }))
            .plugin(tauri_plugin_window_state::Builder::default().build());
    }

    // Phone: scanning the PC's QR code with the sync key.
    #[cfg(mobile)]
    {
        builder = builder.plugin(tauri_plugin_barcode_scanner::init());
    }

    builder
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(move |app| {
            let state = match open_database(app, !background) {
                Ok(conn) => Ok(conn),
                Err(e) => {
                    eprintln!("database unavailable: {e}");
                    Err(e.to_string())
                }
            };
            app.manage(AppState { db: Mutex::new(state), fetching: AtomicBool::new(false), rating: AtomicBool::new(false), stop_rating: AtomicBool::new(false), syncing: AtomicBool::new(false) });
            app.manage(preview::Pending::default());
            app.manage(preview::Loading::default());
            if background {
                let handle = app.handle().clone();
                tauri::async_runtime::spawn(async move {
                    one_pass(&handle, true).await;
                    if !WINDOW_OPENED.load(std::sync::atomic::Ordering::SeqCst) {
                        handle.exit(0);
                    }
                });
                return Ok(());
            }
            // The main window is created here (not from the config) so a background run has none.
            if let Some(config) = app.config().app.windows.iter().find(|w| w.label == "main") {
                tauri::WebviewWindowBuilder::from_config(app.handle(), config)?.build()?;
            }
            // Installed app: Task Scheduler follows the setting (on by default). Windows only.
            if !cfg!(debug_assertions) && cfg!(desktop) {
                let handle = app.handle().clone();
                tauri::async_runtime::spawn_blocking(move || {
                    let state = handle.state::<AppState>();
                    if let Ok(on) = commands::with_db(&state, |c| settings::is_on(c, "background_task")) {
                        if let Err(e) = scheduler::sync(on) {
                            eprintln!("task scheduler: {e}");
                        }
                    }
                });
            }
            start_auto_fetch(app.handle().clone());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::app_status,
            commands::list_offers,
            commands::count_offers,
            commands::get_offer,
            commands::add_offer,
            commands::check_same_offer,
            commands::update_same_offer,
            commands::answer_renewed,
            commands::set_offer_parent,
            commands::compare_offer,
            commands::find_duplicates,
            commands::judge_duplicates,
            commands::join_offers,
            commands::answer_duplicate,
            commands::set_stage,
            commands::set_stage_date,
            commands::set_interest,
            commands::set_favorite,
            commands::set_own,
            commands::set_notes,
            commands::trash_offer,
            commands::restore_offer,
            commands::bring_back_offer,
            commands::put_back_offer,
            commands::set_expired,
            commands::unset_expired,
            commands::unarchive_offer,
            commands::purge_offers,
            commands::get_settings,
            commands::set_setting,
            commands::list_profiles,
            commands::save_profile,
            commands::delete_profile,
            commands::get_experience,
            commands::set_experience,
            commands::get_experience_prev,
            commands::secret_hints,
            commands::set_secret,
            commands::reveal_secret,
            commands::delete_secret,
            commands::source_status,
            commands::fetch_offers,
            commands::ollama_models,
            commands::ollama_pull,
            commands::mail_test,
            commands::source_test,
            commands::ai_status,
            commands::usd_rate,
            commands::ai_stop,
            commands::ai_rate_now,
            commands::ai_rate_all_again,
            commands::ollama_install,
            commands::pick_folder,
            commands::ai_rate_offer,
            commands::ai_translate,
            commands::ai_check,
            commands::ai_free_models,
            commands::ai_assist,
            commands::ai_suggest_criteria,
            commands::ai_suggest_careers,
            commands::ai_tidy,
            commands::check_hardware,
            commands::fill_offer,
            commands::open_preview,
            commands::capture_preview,
            commands::fetch_page,
            commands::close_preview,
            commands::update_description,
            commands::background_status,
            commands::set_background,
            commands::add_event,
            commands::delete_event,
            commands::save_contact,
            commands::delete_contact,
            commands::add_log,
            commands::delete_log,
            commands::calendar_google,
            commands::calendar_ics,
            commands::save_copy,
            commands::load_copy,
            commands::export_rows,
            commands::save_csv,
            commands::wipe_all,
            commands::sync_status,
            commands::sync_login_start,
            commands::sync_login_poll,
            commands::sync_now,
            commands::sync_qr,
            commands::sync_new_key,
            commands::sync_delete_remote,
            commands::sync_disconnect,
            commands::sync_from_qr,
            commands::upcoming_events,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

/// While the app is open: a first look shortly after start, then every 30 minutes.
/// Each source decides by itself whether it is due (its own interval and daily limit).
fn start_auto_fetch(handle: tauri::AppHandle) {
    let reminders = handle.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_secs(20)).await;
        loop {
            one_pass(&handle, false).await;
            tokio::time::sleep(Duration::from_secs(30 * 60)).await;
        }
    });
    // Reminders need a finer clock than fetching ("an hour before"). The phone plans them
    // with the system's own notifications instead - here they would come twice.
    if cfg!(mobile) {
        return;
    }
    tauri::async_runtime::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(5 * 60)).await;
            remind(&reminders);
        }
    });
}

/// Shows reminders that are due (each one once).
fn remind(handle: &tauri::AppHandle) {
    let state = handle.state::<AppState>();
    let on = commands::with_db(&state, |c| settings::is_on(c, "notify_reminders")).unwrap_or(false);
    if !on {
        return;
    }
    let due = match commands::with_db(&state, |c| agenda::due(c, time::OffsetDateTime::now_utc())) {
        Ok(list) => list,
        Err(e) => {
            eprintln!("reminders failed: {e}");
            return;
        }
    };
    let pl = notify::polish(&state);
    for r in &due {
        let (title, body) = agenda::reminder_text(r, pl, agenda::local_label);
        notify::show(handle, &title, &body);
    }
    // The open window may play a short gong (sounds are off by default).
    if due.iter().any(|r| r.kind == "hour_before") {
        let _ = handle.emit("reminder", ());
    }
}

/// One round of work: collect offers, let AI read them, remind, notify.
/// `background` = started by Task Scheduler with no window.
async fn one_pass(handle: &tauri::AppHandle, background: bool) {
    let state = handle.state::<AppState>();
    let on = |key: &str| commands::with_db(&state, |c| settings::is_on(c, key)).unwrap_or(false);
    let mut new_on_board = 0;
    let say = commands::progress_of(handle);
    // The phone is a helper (zalozenia.md §2): collecting offers and AI stay on the PC.
    let collects = cfg!(desktop);
    // "Collect offers automatically" off: no fetching and no AI on its own - reminders and sync stay.
    let gathers = collects && on("collect");
    if gathers && (background || on("fetch_auto")) {
        match commands::until_done(sources::fetch(&state, false, &say).await, &say) {
            Ok(report) => {
                new_on_board = report.new_on_board;
                if !report.runs.is_empty() || report.closed > 0 {
                    let _ = handle.emit("offers-fetched", &report);
                }
            }
            Err(error::AppError::Busy) => {}
            Err(e) => eprintln!("automatic fetch failed: {e}"),
        }
    }
    // AI on its own only when turned on, and not more often than its "every" setting; paid Claude
    // never on its own - only on a click.
    if gathers
        && on("ai_auto")
        && !commands::with_db(&state, |c| ai::paid(c)).unwrap_or(true)
        && commands::with_db(&state, |c| ai::auto_due(c)).unwrap_or(false)
    {
        let _ = commands::with_db(&state, |c| ai::auto_ran(c));
        match commands::until_done(ai::process(&state, None, Some(ai::PER_RUN), false, &say).await, &say) {
            Ok(report) if report.rated > 0 => {
                let _ = handle.emit("offers-rated", &report);
            }
            Ok(_) | Err(error::AppError::Busy) => {}
            Err(e) => eprintln!("automatic AI run failed: {e}"),
        }
    }
    // Sync with Google Drive (only when turned on and connected); a big deletion waits for the person.
    if on("sync_drive") && secrets::get(sync::drive::REFRESH_SECRET).ok().flatten().is_some() {
        match sync::run(&state, false).await {
            Ok(sync::SyncOutcome::Done { changed, .. }) if changed > 0 => {
                let _ = handle.emit("offers-rated", &ai::RateReport::default());
            }
            Ok(_) => {}
            Err(e) => eprintln!("sync failed: {e}"),
        }
    }
    if collects {
        remind(handle);
    }
    // A notification only when the board is not in front of the person anyway.
    let focused = handle.get_webview_window("main").and_then(|w| w.is_focused().ok()).unwrap_or(false);
    if new_on_board > 0 && on("notify_new") && !focused {
        let (title, body) = notify::new_offers_text(new_on_board, notify::polish(&state));
        notify::show(handle, &title, &body);
    }
}

/// `%APPDATA%\pl.prywatne.tablicaofert` (development builds: its `dev` subfolder).
pub(crate) fn data_dir(app: &tauri::AppHandle) -> error::AppResult<std::path::PathBuf> {
    let mut dir = app
        .path()
        .app_data_dir()
        .map_err(|e| error::AppError::Locked(format!("no data folder: {e}")))?;
    // Development builds never touch real data.
    if cfg!(debug_assertions) {
        dir.push("dev");
    }
    Ok(dir)
}

fn open_database(app: &tauri::App, backup: bool) -> error::AppResult<rusqlite::Connection> {
    secrets::init()?;
    db::open_app_database(&data_dir(app.handle())?, backup)
}
