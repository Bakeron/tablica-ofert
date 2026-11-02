//! Functions the UI can call. Each one only checks input and hands it to `offers` / `settings`.
//! They are `async` so Tauri runs them off the window thread.

use std::collections::BTreeMap;
use std::sync::Mutex;
use std::sync::atomic::AtomicBool;

use rusqlite::Connection;
use serde::Serialize;
use tauri::State;

use crate::error::{AppError, AppResult};
use crate::model::*;
use crate::sources::{self, FetchReport, SourceStatus};
use crate::ai::{self, AiStatus, Assist, RateReport};
use crate::{offers, profiles, secrets, settings};

pub struct AppState {
    /// `Err` = the database could not be opened safely; every call returns `Locked`.
    pub db: Mutex<Result<Connection, String>>,
    /// Set while offers are being fetched - a second run waits for the first one.
    pub fetching: AtomicBool,
    /// Set while AI reads offers.
    pub rating: AtomicBool,
}

pub fn with_db<T>(state: &AppState, f: impl FnOnce(&mut Connection) -> AppResult<T>) -> AppResult<T> {
    let mut guard = state
        .db
        .lock()
        .map_err(|_| AppError::Locked("internal state is broken - restart the app".into()))?;
    match guard.as_mut() {
        Ok(conn) => f(conn),
        Err(reason) => Err(AppError::Locked(reason.clone())),
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppStatus {
    pub ok: bool,
    pub problem: Option<String>,
    pub version: String,
}

#[tauri::command]
pub async fn app_status(state: State<'_, AppState>, app: tauri::AppHandle) -> AppResult<AppStatus> {
    let guard = state.db.lock().map_err(|_| AppError::Locked("internal state is broken".into()))?;
    Ok(AppStatus {
        ok: guard.is_ok(),
        problem: guard.as_ref().err().cloned(),
        version: app.package_info().version.to_string(),
    })
}

/// `profile = null` - all profiles together.
#[tauri::command]
pub async fn list_offers(
    state: State<'_, AppState>,
    view: View,
    profile: Option<i64>,
    show_low: Option<bool>,
) -> AppResult<Vec<OfferRow>> {
    with_db(&state, |c| offers::list(c, view, profile, show_low.unwrap_or(false)))
}

#[tauri::command]
pub async fn count_offers(state: State<'_, AppState>, profile: Option<i64>) -> AppResult<ViewCounts> {
    with_db(&state, |c| offers::counts(c, profile))
}

#[tauri::command]
pub async fn get_offer(state: State<'_, AppState>, id: i64) -> AppResult<OfferDetail> {
    with_db(&state, |c| offers::detail(c, id))
}

#[tauri::command]
pub async fn add_offer(state: State<'_, AppState>, app: tauri::AppHandle, offer: OfferInput) -> AppResult<AddOutcome> {
    let outcome = with_db(&state, |c| offers::add(c, offer))?;
    if matches!(outcome, AddOutcome::Added { .. }) {
        spawn_ai(app);
    }
    Ok(outcome)
}

/// AI reads new offers in the background; the board refreshes when it is done.
pub fn spawn_ai(app: tauri::AppHandle) {
    use tauri::{Emitter, Manager};
    tauri::async_runtime::spawn(async move {
        let state = app.state::<AppState>();
        let on = with_db(&state, |c| settings::is_on(c, "ai_auto")).unwrap_or(false);
        if !on {
            return;
        }
        match ai::process(&state, None).await {
            Ok(report) if report.rated > 0 || report.error.is_some() => {
                let _ = app.emit("offers-rated", &report);
            }
            Ok(_) | Err(AppError::Busy) => {}
            Err(e) => eprintln!("AI run failed: {e}"),
        }
    });
}

#[tauri::command]
pub async fn set_stage(state: State<'_, AppState>, id: i64, stage: Stage) -> AppResult<()> {
    with_db(&state, |c| offers::set_stage(c, id, stage))
}

#[tauri::command]
pub async fn set_interest(state: State<'_, AppState>, id: i64, interest: Option<Interest>) -> AppResult<()> {
    with_db(&state, |c| offers::set_interest(c, id, interest))
}

#[tauri::command]
pub async fn set_favorite(state: State<'_, AppState>, id: i64, favorite: bool) -> AppResult<()> {
    with_db(&state, |c| offers::set_favorite(c, id, favorite))
}

#[tauri::command]
pub async fn set_notes(state: State<'_, AppState>, id: i64, notes: String) -> AppResult<()> {
    with_db(&state, |c| offers::set_notes(c, id, notes))
}

#[tauri::command]
pub async fn trash_offer(state: State<'_, AppState>, id: i64) -> AppResult<()> {
    with_db(&state, |c| offers::trash(c, id))
}

#[tauri::command]
pub async fn restore_offer(state: State<'_, AppState>, id: i64) -> AppResult<()> {
    with_db(&state, |c| offers::restore(c, id))
}

/// `id = null` empties the whole trash.
#[tauri::command]
pub async fn purge_offers(state: State<'_, AppState>, id: Option<i64>) -> AppResult<usize> {
    with_db(&state, |c| offers::purge(c, id))
}

#[tauri::command]
pub async fn get_settings(state: State<'_, AppState>) -> AppResult<BTreeMap<String, String>> {
    with_db(&state, |c| settings::all(c))
}

#[tauri::command]
pub async fn set_setting(state: State<'_, AppState>, key: String, value: String) -> AppResult<()> {
    with_db(&state, |c| settings::set(c, &key, &value))
}

#[tauri::command]
pub async fn list_profiles(state: State<'_, AppState>) -> AppResult<Vec<Profile>> {
    with_db(&state, |c| profiles::list(c))
}

/// Saves the profile and re-checks every offer against it. Returns the profile id.
#[tauri::command]
pub async fn save_profile(state: State<'_, AppState>, profile: ProfileInput) -> AppResult<i64> {
    with_db(&state, |c| profiles::save(c, profile))
}

#[tauri::command]
pub async fn delete_profile(state: State<'_, AppState>, id: i64) -> AppResult<()> {
    with_db(&state, |c| profiles::delete(c, id))
}

#[tauri::command]
pub async fn get_experience(state: State<'_, AppState>) -> AppResult<String> {
    with_db(&state, |c| profiles::experience(c))
}

#[tauri::command]
pub async fn get_experience_prev(state: State<'_, AppState>) -> AppResult<Option<String>> {
    with_db(&state, |c| profiles::experience_prev(c))
}

#[tauri::command]
pub async fn set_experience(state: State<'_, AppState>, text: String) -> AppResult<()> {
    with_db(&state, |c| profiles::set_experience(c, &text))
}

// ---------- secrets (keys, passwords) ----------

/// Which user secrets are set, as name -> last 4 characters. Values never leave Rust.
#[tauri::command]
pub async fn secret_hints() -> AppResult<BTreeMap<String, String>> {
    secrets::user_hints()
}

#[tauri::command]
pub async fn set_secret(name: String, value: String) -> AppResult<()> {
    secrets::set_user(&name, &value)
}

#[tauri::command]
pub async fn delete_secret(name: String) -> AppResult<()> {
    secrets::delete_user(&name)
}

// ---------- offer sources ----------

#[tauri::command]
pub async fn source_status(state: State<'_, AppState>) -> AppResult<Vec<SourceStatus>> {
    let adzuna_ready = sources::adzuna::keys()?.is_some();
    let settings = with_db(&state, |c| settings::all(c))?;
    let mail_ready = sources::mail::config(&settings)?.is_some();
    with_db(&state, |c| sources::status(c, adzuna_ready, mail_ready))
}

/// "Check connection" for the alert mailbox: number of messages in the inbox.
#[tauri::command]
pub async fn mail_test(state: State<'_, AppState>) -> AppResult<u32> {
    let settings = with_db(&state, |c| settings::all(c))?;
    let config = sources::mail::config(&settings)?.ok_or_else(|| AppError::Missing("mailbox".into()))?;
    tauri::async_runtime::spawn_blocking(move || sources::mail::test(&config))
        .await
        .map_err(|e| AppError::Net(e.to_string()))?
}

/// "Fetch now": every enabled source that may run (daily limits still apply).
#[tauri::command]
pub async fn fetch_offers(state: State<'_, AppState>, app: tauri::AppHandle) -> AppResult<FetchReport> {
    let report = sources::fetch(&state, true).await?;
    if report.added > 0 {
        spawn_ai(app);
    }
    Ok(report)
}

/// "Check my computer" next to Ollama.
#[tauri::command]
pub async fn check_hardware() -> AppResult<crate::hardware::Machine> {
    tauri::async_runtime::spawn_blocking(crate::hardware::check)
        .await
        .map_err(|e| AppError::Invalid(e.to_string()))?
}

// ---------- AI ----------

#[tauri::command]
pub async fn ai_status(state: State<'_, AppState>) -> AppResult<AiStatus> {
    let keys = secrets::user_hints()?;
    with_db(&state, |c| {
        ai::status(c, |p| p.key_name().is_some_and(|k| keys.contains_key(k)))
    })
}

/// AI reads waiting offers now (the same as after every fetch).
#[tauri::command]
pub async fn ai_rate_now(state: State<'_, AppState>) -> AppResult<RateReport> {
    ai::process(&state, None).await
}

/// "Rate again" on the offer card.
#[tauri::command]
pub async fn ai_rate_offer(state: State<'_, AppState>, id: i64) -> AppResult<RateReport> {
    with_db(&state, |c| ai::reset(c, id))?;
    ai::process(&state, Some(id)).await
}

#[tauri::command]
pub async fn ai_assist(state: State<'_, AppState>, id: i64, kind: Assist) -> AppResult<String> {
    ai::assist(&state, id, kind).await
}

#[tauri::command]
pub async fn ai_suggest_criteria(state: State<'_, AppState>, description: String) -> AppResult<Criteria> {
    ai::suggest_criteria(&state, &description).await
}

#[tauri::command]
pub async fn ai_tidy(state: State<'_, AppState>, text: String) -> AppResult<String> {
    ai::tidy(&state, &text).await
}
