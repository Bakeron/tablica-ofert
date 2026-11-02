//! Functions the UI can call. Each one only checks input and hands it to `offers` / `settings`.
//! They are `async` so Tauri runs them off the window thread.

use std::collections::BTreeMap;
use std::sync::Mutex;

use rusqlite::Connection;
use serde::Serialize;
use tauri::State;

use crate::error::{AppError, AppResult};
use crate::model::*;
use crate::{offers, settings};

pub struct AppState {
    /// `Err` = the database could not be opened safely; every call returns `Locked`.
    pub db: Mutex<Result<Connection, String>>,
}

fn with_db<T>(state: &AppState, f: impl FnOnce(&mut Connection) -> AppResult<T>) -> AppResult<T> {
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

#[tauri::command]
pub async fn list_offers(state: State<'_, AppState>, view: View) -> AppResult<Vec<OfferRow>> {
    with_db(&state, |c| offers::list(c, view))
}

#[tauri::command]
pub async fn get_offer(state: State<'_, AppState>, id: i64) -> AppResult<OfferDetail> {
    with_db(&state, |c| offers::detail(c, id))
}

#[tauri::command]
pub async fn add_offer(state: State<'_, AppState>, offer: OfferInput) -> AppResult<AddOutcome> {
    with_db(&state, |c| offers::add(c, offer))
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
