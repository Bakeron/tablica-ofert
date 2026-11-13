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
use crate::{learn, offers, profiles, secrets, settings};

pub struct AppState {
    /// `Err` = the database could not be opened safely; every call returns `Locked`.
    pub db: Mutex<Result<Connection, String>>,
    /// Set while offers are being fetched - a second run waits for the first one.
    pub fetching: AtomicBool,
    /// Set while AI reads offers.
    pub rating: AtomicBool,
    /// "Stop rating" was clicked - the run ends before its next offer.
    pub stop_rating: AtomicBool,
    /// Set while the sync file on Drive is read or written (sync, new key, deleting it).
    pub syncing: AtomicBool,
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
) -> AppResult<Vec<OfferRow>> {
    with_db(&state, |c| offers::list(c, view, profile))
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
pub async fn add_offer(
    state: State<'_, AppState>,
    app: tauri::AppHandle,
    offer: OfferInput,
    reading: Option<String>,
    separate: Option<bool>,
    parent: Option<i64>,
) -> AppResult<AddOutcome> {
    // Fields and scores AI gave in "Add offer" - a new offer keeps them and is not read again.
    let reading = reading.as_deref().and_then(ai::take_reading);
    // Where the person said it goes (a duplicate question in "Add offer"), else the rules decide.
    let placement = match (parent, separate) {
        (Some(parent), _) => offers::Placement::Under(parent),
        (None, Some(true)) => offers::Placement::Separate,
        _ => offers::Placement::Auto,
    };
    let outcome = with_db(&state, |c| {
        let outcome = offers::add_placed(c, offer, placement)?;
        if let (AddOutcome::Added { id }, Some(r)) = (outcome, reading) {
            // The offer is saved either way; without the scores it waits for AI as before.
            if let Err(e) = ai::store_added(c, id, r) {
                eprintln!("storing the AI reading failed: {e}");
            }
        }
        learn::refresh(c)?;
        Ok(outcome)
    })?;
    if matches!(outcome, AddOutcome::Added { .. }) {
        spawn_ai(app);
    }
    Ok(outcome)
}

/// Before "Add offer" saves: is the same offer of the same portal already on the board?
#[tauri::command]
pub async fn check_same_offer(state: State<'_, AppState>, offer: OfferInput) -> AppResult<SameOffer> {
    with_db(&state, |c| crate::same::check(c, offer))
}

/// "Update the existing one" - the form's fields go into the offer on the board.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn update_same_offer(
    state: State<'_, AppState>,
    app: tauri::AppHandle,
    id: i64,
    offer: OfferInput,
    reading: Option<String>,
    keep: Option<Vec<String>>,
    renew: Option<bool>,
) -> AppResult<()> {
    let reading = reading.as_deref().and_then(ai::take_reading);
    // Field names only - anything else is ignored by `same::update`.
    let keep: Vec<String> = keep.unwrap_or_default().into_iter().take(16).filter(|k| k.len() <= 16).collect();
    let new_text = with_db(&state, |c| {
        let new_text = crate::same::update(c, id, offer, &keep, renew.unwrap_or(true))?;
        // AI read the new text while filling the form - its scores are kept, not asked again.
        if let (true, Some(r)) = (new_text, reading) {
            if let Err(e) = ai::store_added(c, id, r) {
                eprintln!("storing the AI reading failed: {e}");
            }
        }
        learn::refresh(c)?;
        Ok(new_text)
    })?;
    if new_text {
        spawn_ai(app);
    }
    Ok(())
}

/// "Fill in that one": what the form would change in an offer on the board (point 16).
#[tauri::command]
pub async fn compare_offer(state: State<'_, AppState>, id: i64, offer: OfferInput) -> AppResult<SameOffer> {
    with_db(&state, |c| crate::same::compare(c, id, offer))
}

/// "Check duplicates" on the card: offers that may be the same job (plain code, for free).
#[tauri::command]
pub async fn find_duplicates(state: State<'_, AppState>, id: i64) -> AppResult<Vec<crate::dups::DupCandidate>> {
    with_db(&state, |c| {
        let skip = crate::dups::tree_of(c, id)?;
        crate::dups::candidates(c, &crate::dups::Ad::load(c, id)?, &skip)
    })
}

/// AI judges the "probably the same" offers - of the form in "Add offer" (`offer`) or of an
/// offer on the board (`id`). Only on these two clicks, never in the background.
#[tauri::command]
pub async fn judge_duplicates(
    state: State<'_, AppState>,
    offer: Option<OfferInput>,
    id: Option<i64>,
    ids: Vec<i64>,
) -> AppResult<Vec<ai::DupVerdict>> {
    let (new, candidates) = with_db(&state, |c| {
        let new = match (offer, id) {
            (Some(offer), _) => crate::dups::Ad::of(&offers::clean(offer)?),
            (None, Some(id)) => crate::dups::Ad::load(c, id)?,
            (None, None) => return Err(AppError::Invalid("no offer to compare".into())),
        };
        let candidates = ids
            .into_iter()
            .take(crate::dups::MAX_CANDIDATES)
            .map(|i| Ok((i, crate::dups::Ad::load(c, i)?)))
            .collect::<AppResult<Vec<_>>>()?;
        Ok((new, candidates))
    })?;
    ai::judge_duplicates(&state, &new, &candidates).await
}

/// "Join as a child": two offers on the board become one tree (the first added is the parent).
/// Returns the parent.
#[tauri::command]
pub async fn join_offers(state: State<'_, AppState>, id: i64, other: i64) -> AppResult<i64> {
    with_db(&state, |c| {
        let (parent, _) = crate::dups::join(c, id, other)?;
        learn::refresh(c)?;
        Ok(parent)
    })
}

/// "The same company and title - the same ad?" answered on the card: joined (the parent) or not.
#[tauri::command]
pub async fn answer_duplicate(state: State<'_, AppState>, id: i64, join: bool) -> AppResult<Option<i64>> {
    with_db(&state, |c| crate::dups::answer(c, id, join))
}

/// "The ad was renewed after your CV - send it again?" answered.
#[tauri::command]
pub async fn answer_renewed(state: State<'_, AppState>, id: i64, resent: bool) -> AppResult<()> {
    with_db(&state, |c| crate::same::answer_renewed(c, id, resent))
}

/// AI reads new offers in the background; the board refreshes when it is done.
pub fn spawn_ai(app: tauri::AppHandle) {
    use tauri::{Emitter, Manager};
    tauri::async_runtime::spawn(async move {
        let state = app.state::<AppState>();
        // Paid Claude never runs on its own - only on a click.
        let on = with_db(&state, |c| Ok(settings::is_on(c, "ai_auto")? && !ai::paid(c)?)).unwrap_or(false);
        if !on {
            return;
        }
        let say = progress_of(&app);
        match until_done(ai::process(&state, None, Some(ai::PER_RUN), false, &say).await, &say) {
            Ok(report) if report.rated > 0 || report.error.is_some() => {
                let _ = app.emit("offers-rated", &report);
            }
            Ok(_) | Err(AppError::Busy) => {}
            Err(e) => eprintln!("AI run failed: {e}"),
        }
    });
}

/// Every step of the work in the background goes to the open window ("work-progress"),
/// `None` = the line at the bottom goes away.
pub fn progress_of(app: &tauri::AppHandle) -> impl Fn(Option<Progress>) + Sync + '_ {
    use tauri::Emitter;
    move |step| {
        let _ = app.emit("work-progress", step);
    }
}

/// The work is over - unless it never started because the same work was already running
/// (its line stays until that one is over).
pub fn until_done<T>(result: AppResult<T>, progress: OnProgress<'_>) -> AppResult<T> {
    if !matches!(result, Err(AppError::Busy)) {
        progress(None);
    }
    result
}

#[tauri::command]
pub async fn set_stage(state: State<'_, AppState>, id: i64, stage: Stage) -> AppResult<()> {
    with_db(&state, |c| offers::set_stage(c, id, stage))
}

/// "Expired" on the card: nobody touched it - to the archive; otherwise marked "expired".
#[tauri::command]
pub async fn set_expired(state: State<'_, AppState>, id: i64) -> AppResult<offers::ExpiredBy> {
    with_db(&state, |c| offers::set_expired(c, id))
}

#[tauri::command]
pub async fn unset_expired(state: State<'_, AppState>, id: i64) -> AppResult<()> {
    with_db(&state, |c| offers::unset_expired(c, id))
}

#[tauri::command]
pub async fn unarchive_offer(state: State<'_, AppState>, id: i64) -> AppResult<()> {
    with_db(&state, |c| {
        offers::unarchive(c, id)?;
        learn::refresh(c)
    })
}

#[tauri::command]
pub async fn set_interest(state: State<'_, AppState>, id: i64, interest: Option<Interest>) -> AppResult<()> {
    with_db(&state, |c| {
        offers::set_interest(c, id, interest)?;
        learn::refresh(c)
    })
}

#[tauri::command]
pub async fn set_favorite(state: State<'_, AppState>, id: i64, favorite: bool) -> AppResult<()> {
    with_db(&state, |c| offers::set_favorite(c, id, favorite))
}

#[tauri::command]
pub async fn set_own(state: State<'_, AppState>, id: i64, own: bool) -> AppResult<()> {
    with_db(&state, |c| offers::set_own(c, id, own))
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
    with_db(&state, |c| {
        offers::restore(c, id)?;
        learn::refresh(c)
    })
}

/// "Restore" in the bin (any kind); the answer is what "Undo" needs.
#[tauri::command]
pub async fn bring_back_offer(state: State<'_, AppState>, id: i64) -> AppResult<offers::BinPlace> {
    with_db(&state, |c| {
        let place = offers::bring_back(c, id)?;
        learn::refresh(c)?;
        Ok(place)
    })
}

/// "Undo" after "Restore".
#[tauri::command]
pub async fn put_back_offer(state: State<'_, AppState>, id: i64, place: offers::BinPlace) -> AppResult<()> {
    with_db(&state, |c| {
        offers::put_back(c, id, place)?;
        learn::refresh(c)
    })
}

/// Deletes for good one offer, or (`id = null`) everything the bin's chosen variant shows.
#[tauri::command]
pub async fn purge_offers(state: State<'_, AppState>, id: Option<i64>, view: View) -> AppResult<usize> {
    with_db(&state, |c| {
        let n = offers::purge(c, id, view)?;
        // Ratings of deleted offers no longer teach anything.
        learn::refresh(c)?;
        Ok(n)
    })
}

#[tauri::command]
pub async fn get_settings(state: State<'_, AppState>) -> AppResult<BTreeMap<String, String>> {
    with_db(&state, |c| settings::all(c))
}

#[tauri::command]
pub async fn set_setting(state: State<'_, AppState>, key: String, value: String) -> AppResult<()> {
    with_db(&state, |c| {
        settings::set(c, &key, &value)?;
        if key == "learn_scores" {
            learn::refresh(c)?;
        }
        // Profiles asleep or woken up: offers sorted again by the profiles that count now.
        if key == "multi_profile" {
            let tx = c.transaction()?;
            profiles::recheck_all(&tx)?;
            tx.commit()?;
        }
        ai::model_changed(c, &key)?;
        Ok(())
    })
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

/// Which user secrets are set, as name -> last 4 characters. A whole value leaves Rust only
/// through `reveal_secret`, on a "Show key" click.
#[tauri::command]
pub async fn secret_hints() -> AppResult<BTreeMap<String, String>> {
    secrets::user_hints()
}

#[tauri::command]
pub async fn set_secret(state: State<'_, AppState>, name: String, value: String) -> AppResult<()> {
    // A different Jooble key has its own 500 calls - its counter starts from zero.
    let new_jooble_key = name == sources::jooble::KEY_SECRET
        && secrets::get(sources::jooble::KEY_SECRET)?.as_deref() != Some(secrets::clean_value(&value)?.as_str());
    secrets::set_user(&name, &value)?;
    if new_jooble_key {
        with_db(&state, |c| crate::usage::reset_total(c, sources::jooble::SOURCE))?;
    }
    Ok(())
}

#[tauri::command]
pub async fn delete_secret(name: String) -> AppResult<()> {
    secrets::delete_user(&name)
}

/// The saved value of one user key or password - "Show key" in the settings.
#[tauri::command]
pub async fn reveal_secret(name: String) -> AppResult<Option<String>> {
    secrets::reveal_user(&name)
}

// ---------- offer sources ----------

#[tauri::command]
pub async fn source_status(state: State<'_, AppState>) -> AppResult<Vec<SourceStatus>> {
    let adzuna_ready = sources::adzuna::keys()?.is_some();
    let jooble_ready = sources::jooble::key()?.is_some();
    let settings = with_db(&state, |c| settings::all(c))?;
    let mail_ready = sources::mail::config(&settings)?.is_some();
    with_db(&state, |c| sources::status(c, adzuna_ready, jooble_ready, mail_ready))
}

/// "Check connection" for the alert mailbox: number of messages in the inbox.
#[tauri::command]
pub async fn mail_test(state: State<'_, AppState>) -> AppResult<u32> {
    let settings = with_db(&state, |c| settings::all(c))?;
    let config = sources::mail::config(&settings)?.ok_or_else(|| AppError::Missing("mailbox".into()))?;
    let count = tauri::async_runtime::spawn_blocking(move || sources::mail::test(&config))
        .await
        .map_err(|e| AppError::Net(e.to_string()))??;
    // The mailbox answered, so an error from an earlier run says nothing true any more.
    with_db(&state, |c| sources::clear_error(c, sources::mail::SOURCE))?;
    Ok(count)
}

/// "Check connection" next to a source (one small request, nothing saved).
#[tauri::command]
pub async fn source_test(state: State<'_, AppState>, source: String) -> AppResult<()> {
    sources::test(&state, &source).await
}

/// "Fetch now": every enabled source that may run (daily limits still apply).
#[tauri::command]
pub async fn fetch_offers(state: State<'_, AppState>, app: tauri::AppHandle) -> AppResult<FetchReport> {
    let say = progress_of(&app);
    let report = until_done(sources::fetch(&state, true, &say).await, &say)?;
    if report.added > 0 {
        spawn_ai(app.clone());
    }
    Ok(report)
}

/// Models already in Ollama (an error = Ollama not installed or not running).
#[tauri::command]
pub async fn ollama_models(state: State<'_, AppState>) -> AppResult<Vec<String>> {
    let base = with_db(&state, |c| settings::get(c, "ollama_url"))?;
    ai::ollama::models(&base).await
}

/// Downloads a model into Ollama; progress in the line at the bottom.
#[tauri::command]
pub async fn ollama_pull(state: State<'_, AppState>, app: tauri::AppHandle, model: String) -> AppResult<()> {
    let base = with_db(&state, |c| settings::get(c, "ollama_url"))?;
    let say = progress_of(&app);
    let result = ai::ollama::pull(&base, &model, &say).await;
    say(None);
    result
}

/// "Install Ollama for me": both folders may stay empty (default places).
#[tauri::command]
pub async fn ollama_install(app: tauri::AppHandle, program: String, models: String) -> AppResult<()> {
    let say = progress_of(&app);
    until_done(ai::ollama_install::install(&program, &models, &say).await, &say)
}

/// The phone has no folder picker - Ollama is installed only on the PC.
#[cfg(mobile)]
#[tauri::command]
pub async fn pick_folder(_app: tauri::AppHandle) -> AppResult<Option<String>> {
    Err(AppError::Invalid("choosing a folder is available on the computer".into()))
}

/// Folder picked for the Ollama install; `None` = the window was closed.
#[cfg(desktop)]
#[tauri::command]
pub async fn pick_folder(app: tauri::AppHandle) -> AppResult<Option<String>> {
    use tauri_plugin_dialog::DialogExt;
    let dialog = app.dialog().file();
    let picked = tauri::async_runtime::spawn_blocking(move || dialog.blocking_pick_folder())
        .await
        .map_err(|e| AppError::Invalid(e.to_string()))?;
    picked
        .map(|p| p.into_path().map(|p| p.display().to_string()).map_err(|e| AppError::Invalid(e.to_string())))
        .transpose()
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
    let mut status = with_db(&state, |c| {
        ai::status(c, |p| p.key_name().is_some_and(|k| keys.contains_key(k)))
    })?;
    status.running = state.rating.load(std::sync::atomic::Ordering::SeqCst);
    Ok(status)
}

/// "Stop rating": the run ends before its next offer (the one AI is reading now is finished and kept).
#[tauri::command]
pub async fn ai_stop(state: State<'_, AppState>) -> AppResult<()> {
    if state.rating.load(std::sync::atomic::Ordering::SeqCst) {
        state.stop_rating.store(true, std::sync::atomic::Ordering::SeqCst);
    }
    Ok(())
}

/// The dollar in zloty from NBP, for Claude's spending (`None` without a connection).
#[tauri::command]
pub async fn usd_rate() -> AppResult<Option<crate::nbp::UsdRate>> {
    Ok(crate::nbp::usd_rate().await)
}

/// AI reads all waiting offers now, until the daily limits run out.
#[tauri::command]
pub async fn ai_rate_now(state: State<'_, AppState>, app: tauri::AppHandle) -> AppResult<RateReport> {
    let say = progress_of(&app);
    until_done(ai::process(&state, None, None, false, &say).await, &say)
}

/// "Rate all again" in the settings: all of them, until the daily limits run out.
#[tauri::command]
pub async fn ai_rate_all_again(state: State<'_, AppState>, app: tauri::AppHandle) -> AppResult<RateReport> {
    if state.rating.load(std::sync::atomic::Ordering::SeqCst) {
        return Err(AppError::Busy);
    }
    with_db(&state, |c| ai::reset_all(c))?;
    let say = progress_of(&app);
    until_done(ai::process(&state, None, None, false, &say).await, &say)
}

/// "Rate again" on the offer card; `precise` = "Rate more precisely" (Claude alone).
#[tauri::command]
pub async fn ai_rate_offer(state: State<'_, AppState>, id: i64, precise: Option<bool>) -> AppResult<RateReport> {
    with_db(&state, |c| ai::reset(c, id))?;
    // One offer - the message "AI is working" is enough, no line at the bottom.
    ai::process(&state, Some(id), None, precise.unwrap_or(false), &|_| {}).await
}

/// "Translate" on the card of an offer in another language: its title and description.
#[tauri::command]
pub async fn ai_translate(state: State<'_, AppState>, id: i64) -> AppResult<()> {
    ai::translate(&state, id).await
}

/// "Check connection" next to an AI provider (one call of the day).
#[tauri::command]
pub async fn ai_check(state: State<'_, AppState>, provider: String) -> AppResult<ai::AiCheck> {
    let p = ai::client::Provider::parse(&provider).ok_or_else(|| AppError::Invalid("provider".into()))?;
    ai::check(&state, p).await
}

/// Free models of an AI provider for the "Model" field.
#[tauri::command]
pub async fn ai_free_models(state: State<'_, AppState>, provider: String) -> AppResult<ai::FreeModels> {
    let p = ai::client::Provider::parse(&provider).ok_or_else(|| AppError::Invalid("provider".into()))?;
    ai::free_models(&state, p).await
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
pub async fn ai_suggest_careers(state: State<'_, AppState>, description: String, experience: String) -> AppResult<Vec<CareerIdea>> {
    ai::suggest_careers(&state, &description, &experience).await
}

#[tauri::command]
pub async fn ai_tidy(state: State<'_, AppState>, text: String) -> AppResult<String> {
    ai::tidy(&state, &text).await
}

// ---------- offer from a link or a pasted text ----------

/// Fields from a pasted text or a preview page: AI when it is set up, plain code otherwise
/// (also when AI fails - the person fills in the rest).
#[tauri::command]
pub async fn fill_offer(state: State<'_, AppState>, text: String, title: Option<String>) -> AppResult<crate::extract::Draft> {
    let text = crate::trim::page_text(&crate::text::cut(&text, 100_000), title.as_deref());
    let ai_set = with_db(&state, |c| Ok(!settings::get(c, "ai_providers")?.is_empty()))?;
    let mut draft = None;
    if ai_set {
        draft = ai::extract(&state, &text, title.as_deref()).await.ok();
    }
    let mut draft = draft.unwrap_or_else(|| crate::extract::from_text(&text, title.as_deref()));
    // Every value checked against the text: made up, left out, and where it comes from.
    draft.notes = crate::fieldcheck::check(&draft, &text);
    Ok(draft)
}

#[tauri::command]
pub async fn open_preview(app: tauri::AppHandle, url: String) -> AppResult<()> {
    crate::preview::open(&app, &url)
}

/// "Take the text from the page" in the preview window.
#[tauri::command]
pub async fn capture_preview(app: tauri::AppHandle) -> AppResult<crate::preview::Captured> {
    crate::preview::capture(&app).await
}

#[tauri::command]
pub async fn close_preview(app: tauri::AppHandle) -> AppResult<()> {
    crate::preview::close(&app);
    Ok(())
}

/// Returns whether the description got longer (and was saved).
#[tauri::command]
pub async fn update_description(state: State<'_, AppState>, app: tauri::AppHandle, id: i64, text: String) -> AppResult<bool> {
    let text = crate::trim::page_text(&crate::text::cut(&text, 100_000), None);
    let changed = with_db(&state, |c| {
        let changed = offers::update_description(c, id, text)?;
        if changed {
            learn::refresh(c)?;
        }
        Ok(changed)
    })?;
    if changed {
        spawn_ai(app);
    }
    Ok(changed)
}

// ---------- collecting while the app is closed ----------

#[tauri::command]
pub async fn background_status() -> AppResult<crate::scheduler::TaskStatus> {
    tauri::async_runtime::spawn_blocking(crate::scheduler::status)
        .await
        .map_err(|e| AppError::Invalid(e.to_string()))?
}

/// Turns the Task Scheduler task on or off and returns its real state.
#[tauri::command]
pub async fn set_background(state: State<'_, AppState>, on: bool) -> AppResult<crate::scheduler::TaskStatus> {
    with_db(&state, |c| settings::set(c, "background_task", if on { "on" } else { "off" }))?;
    tauri::async_runtime::spawn_blocking(move || crate::scheduler::sync(on))
        .await
        .map_err(|e| AppError::Invalid(e.to_string()))?
}

// ---------- dates, contacts, calendar ----------

use crate::agenda::{self, Contact, EventInput, LogInput};

#[tauri::command]
pub async fn add_event(state: State<'_, AppState>, offer_id: i64, event: EventInput) -> AppResult<i64> {
    with_db(&state, |c| agenda::add_event(c, offer_id, event))
}

#[tauri::command]
pub async fn delete_event(state: State<'_, AppState>, id: i64) -> AppResult<()> {
    with_db(&state, |c| agenda::delete_event(c, id))
}

#[tauri::command]
pub async fn save_contact(state: State<'_, AppState>, offer_id: i64, contact: Contact) -> AppResult<i64> {
    with_db(&state, |c| agenda::save_contact(c, offer_id, contact))
}

#[tauri::command]
pub async fn delete_contact(state: State<'_, AppState>, id: i64) -> AppResult<()> {
    with_db(&state, |c| agenda::delete_contact(c, id))
}

#[tauri::command]
pub async fn add_log(state: State<'_, AppState>, offer_id: i64, entry: LogInput) -> AppResult<i64> {
    with_db(&state, |c| agenda::add_log(c, offer_id, entry))
}

#[tauri::command]
pub async fn delete_log(state: State<'_, AppState>, id: i64) -> AppResult<()> {
    with_db(&state, |c| agenda::delete_log(c, id))
}

/// "Add to Google Calendar": opens the filled-in event page in the browser (no login in the app).
#[tauri::command]
pub async fn calendar_google(state: State<'_, AppState>, app: tauri::AppHandle, id: i64) -> AppResult<()> {
    use tauri_plugin_opener::OpenerExt;
    let pl = crate::notify::polish(&state);
    let link = with_db(&state, |c| agenda::google_link(c, id, pl))?;
    app.opener().open_url(link, None::<&str>).map_err(|e| AppError::Invalid(e.to_string()))
}

/// "Save .ics file": into Downloads, then opened with the default calendar app. Returns the path.
#[tauri::command]
pub async fn calendar_ics(state: State<'_, AppState>, app: tauri::AppHandle, id: i64) -> AppResult<String> {
    use tauri::Manager;
    use tauri_plugin_opener::OpenerExt;
    let pl = crate::notify::polish(&state);
    let content = with_db(&state, |c| agenda::ics(c, id, pl))?;
    let dir = app.path().download_dir().map_err(|e| AppError::Invalid(e.to_string()))?;
    let path = dir.join(format!("tablica-ofert-{}-{id}.ics", crate::clock::file_stamp()));
    std::fs::write(&path, content)?;
    let shown = path.to_string_lossy().into_owned();
    // Opening is a convenience - the file is saved even when no calendar app is set.
    let _ = app.opener().open_path(shown.clone(), None::<&str>);
    Ok(shown)
}

// ---------- copies and CSV ----------

use std::path::PathBuf;

/// The "Save as" window of Windows; `None` = cancelled.
async fn pick_save(app: &tauri::AppHandle, name: String, filter: (&'static str, &'static [&'static str])) -> AppResult<Option<PathBuf>> {
    use tauri_plugin_dialog::DialogExt;
    let dialog = app.dialog().file().set_file_name(name).add_filter(filter.0, filter.1);
    let picked = tauri::async_runtime::spawn_blocking(move || dialog.blocking_save_file())
        .await
        .map_err(|e| AppError::Invalid(e.to_string()))?;
    picked.map(|p| p.into_path().map_err(|e| AppError::Invalid(e.to_string()))).transpose()
}

/// "Save a copy as…" - returns where it was saved (`None` = cancelled).
#[tauri::command]
pub async fn save_copy(state: State<'_, AppState>, app: tauri::AppHandle, password: String) -> AppResult<Option<String>> {
    crate::backup::save_copy_check(&password)?;
    let name = format!("tablica-ofert-{}.tablica", crate::clock::file_stamp());
    let Some(path) = pick_save(&app, name, ("Tablica Ofert", &["tablica"])).await? else { return Ok(None) };
    with_db(&state, |c| crate::backup::save_copy(c, &path, &password))?;
    Ok(Some(path.to_string_lossy().into_owned()))
}

/// "Load a copy…": replaces all data with the copy (the current data stays in `kopie/`).
/// Returns the number of offers loaded (`None` = cancelled).
#[tauri::command]
pub async fn load_copy(state: State<'_, AppState>, app: tauri::AppHandle, password: String) -> AppResult<Option<i64>> {
    use tauri_plugin_dialog::DialogExt;
    let dialog = app.dialog().file().add_filter("Tablica Ofert", &["tablica"]);
    let picked = tauri::async_runtime::spawn_blocking(move || dialog.blocking_pick_file())
        .await
        .map_err(|e| AppError::Invalid(e.to_string()))?;
    let Some(source) = picked else { return Ok(None) };
    let source = source.into_path().map_err(|e| AppError::Invalid(e.to_string()))?;
    let count = crate::backup::check_copy(&source, &password)?;
    let dir = crate::data_dir(&app)?;
    let db_path = dir.join(crate::db::DB_FILE);
    let key = secrets::get(secrets::DB_KEY)?.ok_or_else(|| AppError::Locked("database key is missing".into()))?;
    let mut guard = state.db.lock().map_err(|_| AppError::Locked("internal state is broken".into()))?;
    // The open connection is closed before its file is replaced.
    let old = std::mem::replace(&mut *guard, Err("loading a copy".into()));
    drop(old);
    let result = crate::backup::load_copy(&db_path, &key, &source, &password);
    *guard = crate::db::open_app_database(&dir, false).map_err(|e| e.to_string());
    result?;
    match guard.as_ref() {
        Err(problem) => return Err(AppError::Locked(problem.clone())),
        Ok(conn) => crate::backup::copy_wins(conn, &crate::clock::now())?,
    }
    Ok(Some(count))
}

/// Offers for "Export to Excel (CSV)": one tab (`view`) or all, the trash too (`None`).
#[tauri::command]
pub async fn export_rows(state: State<'_, AppState>, view: Option<View>) -> AppResult<Vec<ExportRow>> {
    with_db(&state, |c| offers::export_rows(c, view))
}

/// Saves the CSV cells the interface made (words in the app language) - returns where it was
/// saved (`None` = cancelled). `name` - the list, a part of the file name.
#[tauri::command]
pub async fn save_csv(app: tauri::AppHandle, name: String, cells: Vec<Vec<String>>, separator: String) -> AppResult<Option<String>> {
    crate::backup::csv_check(&name, &cells)?;
    let separator = crate::backup::csv_separator(&separator)?;
    let file = format!("tablica-{name}-{}.csv", crate::clock::file_stamp());
    let Some(path) = pick_save(&app, file, ("CSV", &["csv"])).await? else { return Ok(None) };
    std::fs::write(&path, crate::backup::csv_text(&cells, separator))?;
    Ok(Some(path.to_string_lossy().into_owned()))
}

/// "Delete from this computer": the database, its copies, every key and password in the
/// credential store and the Task Scheduler task. Then the app closes.
#[tauri::command]
pub async fn wipe_all(state: State<'_, AppState>, app: tauri::AppHandle) -> AppResult<()> {
    let dir = crate::data_dir(&app)?;
    // With Google Drive connected, the latest changes go there first - otherwise they would be
    // lost with this device. A failed sync stops the deleting (disconnect Drive to skip it).
    let sync = with_db(&state, |c| crate::sync::status(c))?;
    if sync.connected && sync.has_key {
        crate::sync::run(&state, true).await?;
    }
    {
        let mut guard = state.db.lock().map_err(|_| AppError::Locked("internal state is broken".into()))?;
        // Closed before its file is removed; nothing can be written any more.
        let old = std::mem::replace(&mut *guard, Err("all data was deleted".into()));
        drop(old);
    }
    let db_file = dir.join(crate::db::DB_FILE);
    for path in [db_file.clone(), db_file.with_extension("db-wal"), db_file.with_extension("db-shm")] {
        if path.exists() {
            std::fs::remove_file(&path)?;
        }
    }
    let copies = dir.join(crate::db::BACKUP_DIR);
    if copies.exists() {
        std::fs::remove_dir_all(&copies)?;
    }
    for file in [crate::keycopy::path(&dir), dir.join(crate::diag::FILE), dir.join(crate::diag::OLD_FILE)] {
        if file.exists() {
            std::fs::remove_file(&file)?;
        }
    }
    for name in secrets::USER_SECRETS {
        secrets::delete(name)?;
    }
    secrets::delete(crate::sync::KEY_SECRET)?;
    secrets::delete(crate::sync::drive::REFRESH_SECRET)?;
    secrets::delete(secrets::DB_KEY)?;
    let _ = tauri::async_runtime::spawn_blocking(crate::scheduler::remove).await;
    app.exit(0);
    Ok(())
}

// ---------- Google Drive sync ----------

#[tauri::command]
pub async fn sync_status(state: State<'_, AppState>) -> AppResult<crate::sync::SyncStatus> {
    with_db(&state, |c| crate::sync::status(c))
}

#[tauri::command]
pub async fn sync_login_start(state: State<'_, AppState>) -> AppResult<crate::sync::drive::DeviceCode> {
    let client = with_db(&state, |c| crate::sync::client(c))?.ok_or_else(|| AppError::Missing("google project".into()))?;
    crate::sync::drive::start_login(&crate::net::client()?, &client).await
}

/// `true` = logged in (the refresh token is now in the credential store).
#[tauri::command]
pub async fn sync_login_poll(state: State<'_, AppState>, device_code: String) -> AppResult<bool> {
    let client = with_db(&state, |c| crate::sync::client(c))?.ok_or_else(|| AppError::Missing("google project".into()))?;
    match crate::sync::drive::poll_login(&crate::net::client()?, &client, &device_code).await? {
        Some(refresh) => {
            secrets::set(crate::sync::drive::REFRESH_SECRET, &refresh)?;
            Ok(true)
        }
        None => Ok(false),
    }
}

#[tauri::command]
pub async fn sync_now(state: State<'_, AppState>, confirm: bool) -> AppResult<crate::sync::SyncOutcome> {
    crate::sync::run(&state, confirm).await
}

#[tauri::command]
pub async fn sync_qr(state: State<'_, AppState>) -> AppResult<String> {
    crate::sync::qr_svg(&state)
}

#[tauri::command]
pub async fn sync_new_key(state: State<'_, AppState>) -> AppResult<()> {
    crate::sync::new_key(&state).await
}

#[tauri::command]
pub async fn sync_delete_remote(state: State<'_, AppState>) -> AppResult<()> {
    crate::sync::delete_remote(&state).await
}

/// Forgets the Google login on this computer (the file on Drive stays).
#[tauri::command]
pub async fn sync_disconnect() -> AppResult<()> {
    secrets::delete(crate::sync::drive::REFRESH_SECRET)
}

/// The phone saves what it scanned from the PC's QR code: the sync key and, when the PC has
/// its own Google project, that project (its Client ID comes back for the settings on screen).
#[tauri::command]
pub async fn sync_from_qr(state: State<'_, AppState>, text: String) -> AppResult<Option<String>> {
    crate::sync::save_scanned(&state, &text)
}

#[tauri::command]
pub async fn upcoming_events(state: State<'_, AppState>) -> AppResult<Vec<crate::agenda::Upcoming>> {
    with_db(&state, |c| crate::agenda::upcoming(c, time::OffsetDateTime::now_utc()))
}

/// "Detach" a child (`parent = None`), or put it back under its parent ("Undo").
#[tauri::command]
pub async fn set_offer_parent(state: State<'_, AppState>, id: i64, parent: Option<i64>) -> AppResult<()> {
    with_db(&state, |c| crate::tree::set_parent(c, id, parent))
}
