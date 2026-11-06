//! Collecting offers from the legal sources (zalozenia.md §3): aggregators with an official
//! API (Adzuna, Remotive, Himalayas, Jooble), public feeds that allow reuse (We Work Remotely, Remote OK)
//! and e-mail alerts. Plain code, no AI and no tokens.
//!
//! Every source has its own limits: how often it may run and how many calls a day it may
//! make. The network part runs without holding the database; offers are saved afterwards,
//! each one checked by `offers::add` like any other outside data.

pub mod adzuna;
pub mod alerts;
pub mod himalayas;
pub mod jooble;
pub mod mail;
pub mod remoteok;
pub mod remotive;
pub mod weworkremotely;

use std::collections::BTreeMap;
use std::sync::atomic::Ordering;

use rusqlite::{Connection, OptionalExtension, params};
use serde::Serialize;
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

use crate::commands::{AppState, with_db};
use crate::error::{AppError, AppResult};
use crate::model::{AddOutcome, OfferInput, OnProgress, Profile, Progress, Remote};
use crate::{clock, net, offers, profiles, settings, usage};

/// One search for one keyword of a profile.
#[derive(Debug, Clone, PartialEq)]
pub struct Query {
    /// Must appear in the offer (one word per search - a search for any of several words
    /// brought offers far from all of them).
    pub keyword: String,
    /// Only for profiles that do not accept remote work.
    pub city: Option<String>,
    pub radius_km: u16,
}


/// How often a source may run and how many calls it may make.
struct Limits {
    /// Hours between automatic runs.
    every_hours: i64,
    /// Hours between runs started with "Fetch now".
    forced_hours: i64,
    calls_a_day: i64,
    calls_a_run: usize,
}

const REMOTIVE_LIMITS: Limits = Limits { every_hours: 24, forced_hours: 6, calls_a_day: 4, calls_a_run: 2 };

/// Feeds without search (We Work Remotely, Remote OK): the whole list in one call, once a day.
const FEED_LIMITS: Limits = Limits { every_hours: 24, forced_hours: 6, calls_a_day: 4, calls_a_run: 1 };
const FEEDS: [&str; 2] = [weworkremotely::SOURCE, remoteok::SOURCE];

/// Adzuna does not publish its free limits; this stays well below the commonly quoted ~1000 a month.
fn adzuna_limits(every_hours: i64) -> Limits {
    Limits { every_hours, forced_hours: 0, calls_a_day: 25, calls_a_run: 6 }
}

/// Jooble: 500 calls for the whole life of a key - one a day lasts about a year and a half.
const JOOBLE_LIMITS: Limits = Limits { every_hours: 20, forced_hours: 0, calls_a_day: 1, calls_a_run: 1 };

/// Himalayas refreshes its data once a day; the keyless limit is shared by everyone, so few searches.
const HIMALAYAS_LIMITS: Limits = Limits { every_hours: 24, forced_hours: 6, calls_a_day: 8, calls_a_run: 4 };

/// Remotive asks for at most 2 calls a minute.
const REMOTIVE_PAUSE_SECS: u64 = 31;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ErrorInfo {
    pub code: &'static str,
    pub detail: String,
}

impl From<&AppError> for ErrorInfo {
    fn from(e: &AppError) -> Self {
        ErrorInfo { code: e.code(), detail: e.to_string() }
    }
}

/// What happened with one source in one run.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceRun {
    pub source: String,
    /// Why it did not run: `missing_key`, `no_profiles`, `no_keywords`, `not_due`, `limit`,
    /// `used_up` (no calls left on the key).
    pub skipped: Option<&'static str>,
    pub added: i64,
    pub merged: i64,
    /// Already deleted by the user - not brought back.
    pub blocked: i64,
    /// Did not pass the checks of outside data.
    pub invalid: i64,
    /// Published longer ago than offers stay fresh - not saved.
    pub old: i64,
    pub error: Option<ErrorInfo>,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FetchReport {
    pub runs: Vec<SourceRun>,
    pub added: i64,
    /// New offers that passed the filter of at least one profile (for notifications).
    pub new_on_board: i64,
    /// New offers in "To check" - the filter could not decide.
    pub new_review: i64,
    /// New offers in "Rejected by filter".
    pub new_rejected: i64,
    /// Offers closed on Himalayas (deleted or stripped of their text) or past their expiry date -
    /// the board must reload.
    pub closed: i64,
}

/// State of a source for the settings.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceStatus {
    pub source: String,
    pub enabled: bool,
    /// Key / login present (always true for sources without one).
    pub ready: bool,
    pub last_try: Option<String>,
    pub last_ok: Option<String>,
    pub last_error: Option<String>,
    pub added: i64,
    pub merged: i64,
    pub calls_today: i64,
    pub calls_a_day: i64,
    /// Calls left on the key for its whole life (only sources with such a limit).
    pub calls_left: Option<i64>,
}

#[derive(Debug, Clone, Default)]
struct LastRun {
    last_try: Option<String>,
}

// ---------- what to search for ----------

/// One search per keyword. Profiles take turns (first keyword of every profile, then the
/// second...), so a run cut by the limits still covers every profile; repeats are skipped.
/// A profile that accepts remote work searches the whole country; one without remote work
/// searches around its first city. `words` (the "search_words" setting) replace the keywords
/// of every profile when set - the keywords then only score the offers.
pub fn plan(profiles: &[Profile], words: &[String]) -> Vec<Query> {
    let mut out: Vec<Query> = Vec::new();
    for round in 0..8 {
        for p in profiles {
            let c = &p.criteria;
            let list = if words.is_empty() { &c.keywords } else { words };
            let Some(keyword) = list.iter().map(|k| k.trim()).filter(|k| !k.is_empty()).nth(round) else {
                continue;
            };
            let remote_ok = c.work_modes.is_empty() || c.work_modes.contains(&Remote::Yes);
            let city = (!remote_ok).then(|| c.cities.first().cloned()).flatten();
            let q = Query {
                keyword: keyword.to_string(),
                radius_km: city.as_ref().map_or(0, |r| r.radius_km),
                city: city.map(|r| r.name),
            };
            let repeat = out.iter().any(|o| {
                o.keyword.to_lowercase() == q.keyword.to_lowercase() && o.city == q.city && o.radius_km == q.radius_km
            });
            if !repeat {
                out.push(q);
            }
        }
    }
    out
}

/// The searches for one run: at most `count`, starting further down the list with every call
/// made today, so with more keywords than calls in a run the later ones get their turn too.
fn for_this_run(queries: &[Query], count: usize, calls_today: i64) -> Vec<&Query> {
    if queries.is_empty() {
        return Vec::new();
    }
    let start = calls_today.max(0) as usize % queries.len();
    queries.iter().cycle().skip(start).take(count.min(queries.len())).collect()
}

/// Where today's turn starts: one day further for every day, plus the calls made today - so the
/// first run of a day does not always begin with the same keywords.
fn turn(now: OffsetDateTime, per_run: usize, calls_today: i64) -> i64 {
    (now.unix_timestamp() / 86_400) * per_run as i64 + calls_today.max(0)
}

/// Like `for_this_run`, for a plain list of words.
fn for_this_run_words(words: &[String], count: usize, calls_today: i64) -> Vec<&String> {
    if words.is_empty() {
        return Vec::new();
    }
    let start = calls_today.max(0) as usize % words.len();
    words.iter().cycle().skip(start).take(count.min(words.len())).collect()
}

/// Keywords for Remotive (remote only): the first keyword of every profile, then the rest.
pub fn remote_keywords(profiles: &[Profile]) -> Vec<String> {
    let mut words: Vec<String> = Vec::new();
    let lists: Vec<&Vec<String>> = profiles
        .iter()
        .filter(|p| p.criteria.work_modes.is_empty() || p.criteria.work_modes.contains(&Remote::Yes))
        .map(|p| &p.criteria.keywords)
        .collect();
    for round in 0..8 {
        for list in &lists {
            if let Some(word) = list.get(round).map(|w| w.trim().to_lowercase()).filter(|w| !w.is_empty()) {
                if !words.contains(&word) {
                    words.push(word);
                }
            }
        }
    }
    words
}

// ---------- timing ----------

fn hours_since(at: Option<&str>, now: OffsetDateTime) -> Option<i64> {
    let at = OffsetDateTime::parse(at?, &Rfc3339).ok()?;
    Some((now - at).whole_hours())
}

fn due(last: &LastRun, limits: &Limits, force: bool, now: OffsetDateTime) -> bool {
    let wait = if force { limits.forced_hours } else { limits.every_hours };
    match hours_since(last.last_try.as_deref(), now) {
        None => true,
        // A clock moved back counts as "long ago".
        Some(h) => h < 0 || h >= wait,
    }
}

/// Days of offers every Adzuna search asks for and the first mailbox read reaches back: as long
/// as offers stay fresh (the "fresh_days" setting). Offers seen before only merge (and change
/// nothing when nothing is new), deleted ones never come back. The `source_queries` table
/// (migration 008) is no longer used.
fn fresh_days(settings: &BTreeMap<String, String>) -> u32 {
    settings.get("fresh_days").and_then(|v| v.parse().ok()).unwrap_or(10)
}

// ---------- database part ----------

fn last_runs(conn: &Connection) -> AppResult<BTreeMap<String, LastRun>> {
    let mut stmt = conn.prepare("SELECT source, last_try FROM source_runs")?;
    let rows = stmt
        .query_map([], |r| Ok((r.get::<_, String>(0)?, LastRun { last_try: r.get(1)? })))?
        .collect::<Result<BTreeMap<_, _>, _>>()?;
    Ok(rows)
}

fn record(conn: &Connection, run: &SourceRun, now: &str) -> AppResult<()> {
    let error = run.error.as_ref().map(|e| crate::text::cut(&e.detail, 300));
    conn.execute(
        "INSERT INTO source_runs (source, last_try, last_ok, last_error, added, merged)
         VALUES (?1, ?2, CASE WHEN ?3 IS NULL THEN ?2 END, ?3, ?4, ?5)
         ON CONFLICT (source) DO UPDATE SET
            last_try = excluded.last_try,
            last_ok = CASE WHEN ?3 IS NULL THEN excluded.last_try ELSE source_runs.last_ok END,
            last_error = ?3,
            added = CASE WHEN ?3 IS NULL THEN ?4 ELSE source_runs.added END,
            merged = CASE WHEN ?3 IS NULL THEN ?5 ELSE source_runs.merged END",
        params![run.source, now, error, run.added, run.merged],
    )?;
    Ok(())
}

/// The source works again (checked in the settings): the last run's result stays, its error goes.
pub fn clear_error(conn: &Connection, source: &str) -> AppResult<()> {
    conn.execute("UPDATE source_runs SET last_error = NULL WHERE source = ?1", params![source])?;
    Ok(())
}

/// Saves fetched offers one by one - one bad offer never stops the others. Offers published
/// longer ago than offers stay fresh are skipped (a feed or the first mailbox read reaches far back).
fn save(conn: &mut Connection, found: Vec<OfferInput>, run: &mut SourceRun, report: &mut FetchReport) -> AppResult<()> {
    let now = clock::now();
    let old = offers::cutoff(&now, offers::fresh_days(conn)?);
    for offer in found {
        if offers::published(offer.published_at.as_deref(), &now).is_some_and(|p| p < old) {
            run.old += 1;
            continue;
        }
        match offers::add(conn, offer) {
            Ok(AddOutcome::Added { id }) => {
                run.added += 1;
                match filter_result(conn, id)?.as_deref() {
                    Some("pass") => report.new_on_board += 1,
                    Some("fail") => report.new_rejected += 1,
                    _ => report.new_review += 1,
                }
            }
            Ok(AddOutcome::Merged { .. }) => run.merged += 1,
            Ok(AddOutcome::Unchanged { .. }) => {}
            Ok(AddOutcome::Blocked) => run.blocked += 1,
            Err(AppError::Invalid(_)) => run.invalid += 1,
            Err(e) => return Err(e),
        }
    }
    Ok(())
}


fn filter_result(conn: &Connection, id: i64) -> AppResult<Option<String>> {
    Ok(conn
        .query_row("SELECT filter_result FROM offers WHERE id = ?1", [id], |r| r.get(0))
        .optional()?)
}

struct Snapshot {
    settings: BTreeMap<String, String>,
    profiles: Vec<Profile>,
    runs: BTreeMap<String, LastRun>,
    adzuna_calls: i64,
    remotive_calls: i64,
    himalayas_calls: i64,
    feed_calls: BTreeMap<&'static str, i64>,
    jooble_calls: i64,
    jooble_total: i64,
}

fn snapshot(conn: &Connection) -> AppResult<Snapshot> {
    Ok(Snapshot {
        settings: settings::all(conn)?,
        profiles: profiles::list(conn)?,
        runs: last_runs(conn)?,

        adzuna_calls: usage::calls_today(conn, adzuna::SOURCE)?,
        remotive_calls: usage::calls_today(conn, remotive::SOURCE)?,
        himalayas_calls: usage::calls_today(conn, himalayas::SOURCE)?,
        feed_calls: FEEDS.iter().map(|s| Ok((*s, usage::calls_today(conn, s)?))).collect::<AppResult<_>>()?,
        jooble_calls: usage::calls_today(conn, jooble::SOURCE)?,
        jooble_total: usage::calls_total(conn, jooble::SOURCE)?,
    })
}

// ---------- the run ----------

/// Clears the "fetching" flag even when the run fails.
struct Running<'a>(&'a AppState);

impl Drop for Running<'_> {
    fn drop(&mut self) {
        self.0.fetching.store(false, Ordering::SeqCst);
    }
}

/// Collects offers from every enabled source that is due. `force` = "Fetch now" from the UI
/// (shorter waits, daily limits still apply). `progress` hears every request before it goes
/// (the caller says when it is over).
pub async fn fetch(state: &AppState, force: bool, progress: OnProgress<'_>) -> AppResult<FetchReport> {
    if state.fetching.swap(true, Ordering::SeqCst) {
        return Err(AppError::Busy);
    }
    let _running = Running(state);
    let say = |step: &str, done: usize, total: usize| progress(Some(Progress::new(step, None, done, total)));
    let snap = with_db(state, |c| snapshot(c))?;
    let now = OffsetDateTime::now_utc();
    let mut report = FetchReport::default();
    let on = |key: &str| snap.settings.get(key).is_some_and(|v| v == "on");
    let search_words = settings::search_words(snap.settings.get("search_words").map_or("", String::as_str));
    let client = net::client()?;

    // Adzuna
    if on("source_adzuna") {
        let every: i64 = snap.settings.get("fetch_every").and_then(|v| v.parse().ok()).unwrap_or(6);
        let limits = adzuna_limits(every);
        let last = snap.runs.get(adzuna::SOURCE).cloned().unwrap_or_default();
        let mut run = SourceRun { source: adzuna::SOURCE.into(), ..Default::default() };
        let queries = plan(&snap.profiles, &search_words);
        let keys = adzuna::keys()?;
        let left = (limits.calls_a_day - snap.adzuna_calls).max(0) as usize;
        if snap.profiles.is_empty() {
            run.skipped = Some("no_profiles");
        } else if keys.is_none() {
            run.skipped = Some("missing_key");
        } else if queries.is_empty() {
            run.skipped = Some("no_keywords");
        } else if !due(&last, &limits, force, now) {
            run.skipped = Some("not_due");
        } else if left == 0 {
            run.skipped = Some("limit");
        } else {
            let keys = keys.expect("checked above");
            let country = snap.settings.get("adzuna_country").cloned().unwrap_or_else(|| "pl".into());
            let budget = limits.calls_a_run.min(left);
            let mut found = Vec::new();
            // Searches whose first page was full - their second page goes after every first page.
            let mut more = Vec::new();
            let mut calls: usize = 0;
            let first = for_this_run(&queries, budget, turn(now, budget, snap.adzuna_calls));
            let first_total = first.len();
            for q in first {
                calls += 1;
                say(adzuna::SOURCE, calls, first_total);
                match adzuna::search(&client, &keys, &country, q, fresh_days(&snap.settings), 1).await {
                    Ok(mut page) => {
                        found.append(&mut page.offers);
                        if page.full {
                            more.push(q);
                        }
                    }
                    Err(e) => {
                        run.error = Some((&e).into());
                        break;
                    }
                }
            }
            if run.error.is_none() {
                let more: Vec<_> = more.into_iter().take(budget.saturating_sub(calls)).collect();
                let total = calls + more.len();
                for q in more {
                    calls += 1;
                    say(adzuna::SOURCE, calls, total);
                    match adzuna::search(&client, &keys, &country, q, fresh_days(&snap.settings), 2).await {
                        Ok(mut page) => found.append(&mut page.offers),
                        Err(e) => {
                            run.error = Some((&e).into());
                            break;
                        }
                    }
                }
            }
            let calls = calls as i64;
            let stamp = clock::now();
            with_db(state, |c| {
                usage::add_calls(c, adzuna::SOURCE, calls)?;
                save(c, found, &mut run, &mut report)?;
                record(c, &run, &stamp)
            })?;
        }
        report.added += run.added;
        report.runs.push(run);
    }

    // Remotive
    if on("source_remotive") {
        let limits = REMOTIVE_LIMITS;
        let last = snap.runs.get(remotive::SOURCE).cloned().unwrap_or_default();
        let mut run = SourceRun { source: remotive::SOURCE.into(), ..Default::default() };
        let words = remote_keywords(&snap.profiles);
        let left = (limits.calls_a_day - snap.remotive_calls).max(0) as usize;
        if snap.profiles.is_empty() {
            run.skipped = Some("no_profiles");
        } else if words.is_empty() {
            run.skipped = Some("no_keywords");
        } else if !due(&last, &limits, force, now) {
            run.skipped = Some("not_due");
        } else if left == 0 {
            run.skipped = Some("limit");
        } else {
            let mut found = Vec::new();
            let mut calls = 0;
            let count = limits.calls_a_run.min(left);
            let list = for_this_run_words(&words, count, turn(now, count, snap.remotive_calls));
            let total = list.len();
            for (i, word) in list.into_iter().enumerate() {
                if i > 0 {
                    progress(Some(Progress::new("wait", Some(remotive::SOURCE), i + 1, total)));
                    tokio::time::sleep(std::time::Duration::from_secs(REMOTIVE_PAUSE_SECS)).await;
                }
                calls += 1;
                say(remotive::SOURCE, i + 1, total);
                match remotive::search(&client, word).await {
                    Ok(mut list) => found.append(&mut list),
                    Err(e) => {
                        run.error = Some((&e).into());
                        break;
                    }
                }
            }
            let stamp = clock::now();
            with_db(state, |c| {
                usage::add_calls(c, remotive::SOURCE, calls)?;
                save(c, found, &mut run, &mut report)?;
                record(c, &run, &stamp)
            })?;
        }
        report.added += run.added;
        report.runs.push(run);
    }

    // Himalayas - a few keyword searches a day, then the check of closed jobs (their terms).
    if on("source_himalayas") {
        let limits = HIMALAYAS_LIMITS;
        let last = snap.runs.get(himalayas::SOURCE).cloned().unwrap_or_default();
        let mut run = SourceRun { source: himalayas::SOURCE.into(), ..Default::default() };
        let words = remote_keywords(&snap.profiles);
        let left = (limits.calls_a_day - snap.himalayas_calls).max(0) as usize;
        if snap.profiles.is_empty() {
            run.skipped = Some("no_profiles");
        } else if words.is_empty() {
            run.skipped = Some("no_keywords");
        } else if !due(&last, &limits, force, now) {
            run.skipped = Some("not_due");
        } else if left == 0 {
            run.skipped = Some("limit");
        } else {
            let mut found = Vec::new();
            let mut calls = 0;
            let count = limits.calls_a_run.min(left);
            let country = snap.settings.get("adzuna_country").cloned().unwrap_or_else(|| "pl".into());
            let list = for_this_run_words(&words, count, turn(now, count, snap.himalayas_calls));
            let total = list.len();
            for word in list {
                calls += 1;
                say(himalayas::SOURCE, calls as usize, total);
                match himalayas::search(&client, word, &country).await {
                    Ok(list) => found.extend(list.into_iter().filter(|o| {
                        crate::filter::mentions_any(&o.title, o.description.as_deref().unwrap_or(""), std::slice::from_ref(word))
                    })),
                    Err(e) => {
                        run.error = Some((&e).into());
                        break;
                    }
                }
            }
            let stamp = clock::now();
            with_db(state, |c| {
                usage::add_calls(c, himalayas::SOURCE, calls)?;
                save(c, found, &mut run, &mut report)?;
                record(c, &run, &stamp)
            })?;
        }
        report.added += run.added;
        report.runs.push(run);
    }
    // Closed Himalayas jobs past their expiry date - at every run, also when the source was
    // turned off since (the single-job checks come last, after every other source).
    let stamp = clock::now();
    let expired = with_db(state, |c| himalayas::close_expired(c, &stamp))?;
    report.closed += (expired.deleted + expired.kept) as i64;
    // Every other source: past the expiry date = "expired" on the board.
    report.closed += with_db(state, |c| offers::mark_expired(c, &stamp))? as i64;
    // Untouched offers past their fresh days leave for the archive.
    report.closed += with_db(state, |c| offers::archive_old(c, &stamp))? as i64;

    // We Work Remotely, Remote OK - the whole feed once a day; only offers with the keywords
    // of profiles that accept remote work are kept.
    for source in FEEDS {
        if !on(&format!("source_{source}")) {
            continue;
        }
        let limits = FEED_LIMITS;
        let last = snap.runs.get(source).cloned().unwrap_or_default();
        let mut run = SourceRun { source: source.into(), ..Default::default() };
        let words = remote_keywords(&snap.profiles);
        let left = limits.calls_a_day - snap.feed_calls.get(source).copied().unwrap_or(0);
        if snap.profiles.is_empty() {
            run.skipped = Some("no_profiles");
        } else if words.is_empty() {
            run.skipped = Some("no_keywords");
        } else if !due(&last, &limits, force, now) {
            run.skipped = Some("not_due");
        } else if left <= 0 {
            run.skipped = Some("limit");
        } else {
            say(source, 1, 1);
            let answer = if source == weworkremotely::SOURCE {
                weworkremotely::fetch(&client, &words).await
            } else {
                remoteok::fetch(&client, &words).await
            };
            let found = answer.unwrap_or_else(|e| {
                run.error = Some((&e).into());
                Vec::new()
            });
            let stamp = clock::now();
            with_db(state, |c| {
                usage::add_calls(c, source, 1)?;
                save(c, found, &mut run, &mut report)?;
                record(c, &run, &stamp)
            })?;
        }
        report.added += run.added;
        report.runs.push(run);
    }

    // Jooble - one search a day; the keywords take turns day after day.
    if on("source_jooble") {
        let limits = JOOBLE_LIMITS;
        let last = snap.runs.get(jooble::SOURCE).cloned().unwrap_or_default();
        let mut run = SourceRun { source: jooble::SOURCE.into(), ..Default::default() };
        let queries = plan(&snap.profiles, &search_words);
        let key = jooble::key()?;
        if snap.profiles.is_empty() {
            run.skipped = Some("no_profiles");
        } else if key.is_none() {
            run.skipped = Some("missing_key");
        } else if queries.is_empty() {
            run.skipped = Some("no_keywords");
        } else if snap.jooble_total >= jooble::KEY_CALLS {
            run.skipped = Some("used_up");
        } else if !due(&last, &limits, force, now) {
            run.skipped = Some("not_due");
        } else if snap.jooble_calls >= limits.calls_a_day {
            run.skipped = Some("limit");
        } else {
            let key = key.expect("checked above");
            let mut found = Vec::new();
            let mut sent = false;
            if let Some(q) = for_this_run(&queries, limits.calls_a_run, snap.jooble_total).first() {
                say(jooble::SOURCE, 1, 1);
                match jooble::search(&client, &key, q, &mut sent).await {
                    Ok(mut list) => found.append(&mut list),
                    Err(e) => run.error = Some((&e).into()),
                }
            }
            let stamp = clock::now();
            with_db(state, |c| {
                usage::add_calls(c, jooble::SOURCE, sent as i64)?;
                save(c, found, &mut run, &mut report)?;
                record(c, &run, &stamp)
            })?;
        }
        report.added += run.added;
        report.runs.push(run);
    }

    // E-mail alerts
    if on("source_mail") {
        let every: i64 = snap.settings.get("fetch_every").and_then(|v| v.parse().ok()).unwrap_or(6);
        let limits = Limits { every_hours: every, forced_hours: 0, calls_a_day: i64::MAX, calls_a_run: 1 };
        let last = snap.runs.get(mail::SOURCE).cloned().unwrap_or_default();
        let mut run = SourceRun { source: mail::SOURCE.into(), ..Default::default() };
        let config = mail::config(&snap.settings)?;
        if snap.profiles.is_empty() {
            run.skipped = Some("no_profiles");
        } else if config.is_none() {
            run.skipped = Some("missing_key");
        } else if !due(&last, &limits, force, now) {
            run.skipped = Some("not_due");
        } else {
            let config = config.expect("checked above");
            let position = with_db(state, |c| mail::position(c))?;
            let first_days = i64::from(fresh_days(&snap.settings));
            say(mail::SOURCE, 1, 1);
            // IMAP is blocking - it runs on a worker thread, not on the async runtime.
            let result = tauri::async_runtime::spawn_blocking(move || mail::fetch(&config, &position, first_days))
                .await
                .map_err(|e| AppError::Net(e.to_string()))
                .and_then(|r| r);
            let stamp = clock::now();
            match result {
                Ok(fetched) => with_db(state, |c| {
                    save(c, fetched.offers, &mut run, &mut report)?;
                    // Only after the offers are saved - a crash in between reads the mails again.
                    mail::save_position(c, &fetched.position)?;
                    record(c, &run, &stamp)
                })?,
                Err(e) => {
                    run.error = Some((&e).into());
                    with_db(state, |c| record(c, &run, &stamp))?;
                }
            }
        }
        report.added += run.added;
        report.runs.push(run);
    }

    // Closed Himalayas jobs (their terms) - every offer checked once a day, last, so the other
    // sources never wait for it. A stopped round goes on at the next run.
    let today = clock::now().get(..10).unwrap_or_default().to_string();
    let to_check = with_db(state, |c| himalayas::to_check(c, &today))?;
    if !to_check.is_empty() {
        let pause = std::time::Duration::from_millis(himalayas::CHECK_PAUSE_MS);
        let total = to_check.len();
        let mut done = 0;
        let round = himalayas::check_round(&to_check, pause, |link| {
            done += 1;
            progress(Some(Progress::new("check", Some(himalayas::SOURCE), done, total)));
            let client = client.clone();
            async move { himalayas::job_state(&client, &link).await }
        })
        .await;
        let gone = with_db(state, |c| {
            usage::add_calls(c, himalayas::CHECK_KEY, round.checked.len() as i64)?;
            himalayas::finish_round(c, &round, &today)
        })?;
        report.closed += (gone.deleted + gone.kept) as i64;
        if himalayas::suspicious(&round) {
            let error = AppError::Net(format!(
                "Himalayas reported {} of {} checked offers closed at once - none deleted, kept without the Himalayas text",
                round.closed.len(),
                round.answered
            ));
            with_db(state, |c| {
                c.execute(
                    "UPDATE source_runs SET last_error = ?1 WHERE source = ?2",
                    params![error.to_string(), himalayas::SOURCE],
                )?;
                Ok(())
            })?;
            match report.runs.iter_mut().find(|r| r.source == himalayas::SOURCE) {
                Some(run) => run.error = Some((&error).into()),
                None => report.runs.push(SourceRun {
                    source: himalayas::SOURCE.into(),
                    error: Some((&error).into()),
                    ..Default::default()
                }),
            }
        }
    }

    Ok(report)
}

/// State of every source for the settings window.
/// `adzuna_ready` = both Adzuna keys are in the credential store, `jooble_ready` = the
/// Jooble key is, `mail_ready` = mailbox address, server and password are set.
pub fn status(conn: &Connection, adzuna_ready: bool, jooble_ready: bool, mail_ready: bool) -> AppResult<Vec<SourceStatus>> {
    let s = settings::all(conn)?;
    let mut stmt = conn.prepare("SELECT last_try, last_ok, last_error, added, merged FROM source_runs WHERE source = ?1")?;
    let mut one = |source: &str, ready: bool, calls_a_day: i64| -> AppResult<SourceStatus> {
        let row = stmt
            .query_row([source], |r| {
                Ok((r.get::<_, Option<String>>(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
            })
            .optional()?;
        let (last_try, last_ok, last_error, added, merged) = row.unwrap_or((None, None, None, 0, 0));
        Ok(SourceStatus {
            source: source.into(),
            enabled: s.get(&format!("source_{source}")).is_some_and(|v| v == "on"),
            ready,
            last_try,
            last_ok,
            last_error,
            added,
            merged,
            calls_today: usage::calls_today(conn, source)?,
            calls_a_day,
            calls_left: None,
        })
    };
    Ok(vec![
        one(adzuna::SOURCE, adzuna_ready, adzuna_limits(6).calls_a_day)?,
        one(remotive::SOURCE, true, REMOTIVE_LIMITS.calls_a_day)?,
        one(himalayas::SOURCE, true, HIMALAYAS_LIMITS.calls_a_day)?,
        one(weworkremotely::SOURCE, true, FEED_LIMITS.calls_a_day)?,
        one(remoteok::SOURCE, true, FEED_LIMITS.calls_a_day)?,
        SourceStatus {
            calls_left: Some((jooble::KEY_CALLS - usage::calls_total(conn, jooble::SOURCE)?).max(0)),
            ..one(jooble::SOURCE, jooble_ready, JOOBLE_LIMITS.calls_a_day)?
        },
        one(mail::SOURCE, mail_ready, 0)?,
    ])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{CityRule, Criteria};
    use time::macros::datetime;

    fn profile(keywords: &[&str], modes: Vec<Remote>, city: Option<&str>) -> Profile {
        Profile {
            id: 1,
            name: "Plan".into(),
            description: "x".into(),
            position: 0,
            description_prev: None,
            criteria: Criteria {
                keywords: keywords.iter().map(|s| s.to_string()).collect(),
                work_modes: modes,
                cities: city.map(|c| vec![CityRule { name: c.into(), radius_km: 30 }]).unwrap_or_default(),
                ..Default::default()
            },
        }
    }

    #[test]
    fn plan_searches_whole_country_when_remote_is_fine() {
        let p = vec![
            profile(&["react", "typescript"], vec![Remote::Yes, Remote::Hybrid], Some("Wrocław")),
            profile(&["księgowa"], vec![Remote::No], Some("Opole")),
            profile(&[], vec![], None),
            profile(&["react", "typescript"], vec![], Some("Gdańsk")),
        ];
        let q = plan(&p, &[]);
        let words: Vec<(&str, Option<&str>)> = q.iter().map(|q| (q.keyword.as_str(), q.city.as_deref())).collect();
        // Profiles take turns; the same word with the same place is searched once.
        assert_eq!(words, vec![("react", None), ("księgowa", Some("Opole")), ("typescript", None)]);
        assert_eq!(q[1].radius_km, 30);
        // Words for alerts replace the keywords; each profile keeps its own place.
        let w = vec!["WordPress".to_string(), "WordPress AI".to_string()];
        let q = plan(&p, &w);
        let words: Vec<(&str, Option<&str>)> = q.iter().map(|q| (q.keyword.as_str(), q.city.as_deref())).collect();
        assert_eq!(words, vec![("WordPress", None), ("WordPress", Some("Opole")), ("WordPress AI", None), ("WordPress AI", Some("Opole"))]);
    }

    #[test]
    fn runs_take_turns_through_the_keywords() {
        let p = vec![profile(&["a1", "a2", "a3", "a4", "a5", "a6", "a7", "a8"], vec![], None)];
        let q = plan(&p, &[]);
        let words = |calls_today| for_this_run(&q, 6, calls_today).iter().map(|q| q.keyword.clone()).collect::<Vec<_>>();
        assert_eq!(words(0), vec!["a1", "a2", "a3", "a4", "a5", "a6"]);
        assert_eq!(words(6), vec!["a7", "a8", "a1", "a2", "a3", "a4"]);
        // Never the same search twice in one run.
        let short = plan(&[profile(&["x", "y"], vec![], None)], &[]);
        assert_eq!(for_this_run(&short, 6, 5).len(), 2);
        assert!(for_this_run(&[], 6, 0).is_empty());
    }

    #[test]
    fn first_run_of_every_day_starts_further_on() {
        let words: Vec<String> = ["a", "b", "c", "d", "e"].iter().map(|w| w.to_string()).collect();
        let day = |d: i64| OffsetDateTime::from_unix_timestamp(d * 86_400 + 3_600).unwrap();
        let first = |d: i64| for_this_run_words(&words, 2, turn(day(d), 2, 0)).into_iter().cloned().collect::<Vec<_>>();
        // Over a few days every word gets searched, not only the first two.
        let seen: std::collections::BTreeSet<String> = (0..3).flat_map(first).collect();
        assert_eq!(seen.len(), 5);
        // Later runs of the same day go on from where the day began.
        assert_ne!(for_this_run_words(&words, 2, turn(day(0), 2, 2)), for_this_run_words(&words, 2, turn(day(0), 2, 0)));
    }

    #[test]
    fn remote_keywords_take_turns_between_profiles() {
        let p = vec![
            profile(&["React", "CSS"], vec![], None),
            profile(&["Figma", "react"], vec![Remote::Yes], None),
            profile(&["SAP"], vec![Remote::No], None),
        ];
        assert_eq!(remote_keywords(&p), vec!["react", "figma", "css"]);
    }

    #[test]
    fn timing() {
        let now = datetime!(2026-11-02 12:00 UTC);
        let fresh = LastRun { last_try: Some("2026-11-02T10:00:00Z".into()) };
        let old = LastRun { last_try: Some("2026-11-01T05:00:00Z".into()) };
        assert!(due(&LastRun::default(), &REMOTIVE_LIMITS, false, now));
        assert!(!due(&fresh, &REMOTIVE_LIMITS, false, now));
        assert!(!due(&fresh, &REMOTIVE_LIMITS, true, now));
        assert!(due(&old, &REMOTIVE_LIMITS, false, now));
        assert!(due(&fresh, &adzuna_limits(6), true, now));
        assert!(!due(&fresh, &adzuna_limits(6), false, now));
    }

    #[test]
    fn records_runs_and_keeps_last_good_result_after_a_failure() {
        let conn = crate::db::tests::test_db();
        let ok = SourceRun { source: "adzuna".into(), added: 3, merged: 1, ..Default::default() };
        record(&conn, &ok, "2026-11-02T10:00:00Z").unwrap();
        let bad = SourceRun {
            source: "adzuna".into(),
            error: Some(ErrorInfo { code: "net", detail: "timeout".into() }),
            ..Default::default()
        };
        record(&conn, &bad, "2026-11-02T16:00:00Z").unwrap();
        let s = status(&conn, false, false, false).unwrap().into_iter().find(|s| s.source == "adzuna").unwrap();
        assert_eq!(s.last_try.as_deref(), Some("2026-11-02T16:00:00Z"));
        assert_eq!(s.last_ok.as_deref(), Some("2026-11-02T10:00:00Z"));
        assert_eq!(s.last_error.as_deref(), Some("timeout"));
        assert_eq!((s.added, s.merged), (3, 1));

        clear_error(&conn, "adzuna").unwrap();
        let s = status(&conn, false, false, false).unwrap().into_iter().find(|s| s.source == "adzuna").unwrap();
        assert_eq!(s.last_error, None);
        assert_eq!(s.last_ok.as_deref(), Some("2026-11-02T10:00:00Z"));
    }


    #[test]
    fn saving_counts_every_outcome() {
        let mut conn = crate::db::tests::test_db();
        let offer = |t: &str| OfferInput { title: t.into(), company: "Firma Testowa".into(), source: "adzuna".into(), ..Default::default() };
        let mut run = SourceRun::default();
        let mut report = FetchReport::default();
        let mut dev = offer("Dev");
        dev.city = Some("Opole".into());
        save(&mut conn, vec![offer("Dev"), offer("  "), offer("QA"), dev], &mut run, &mut report).unwrap();
        // The second "Dev" brings a city - merged; the empty title is invalid.
        assert_eq!((run.added, run.merged, run.invalid), (2, 1, 1));
        assert_eq!(report.new_on_board, 2);
        assert_eq!((report.new_review, report.new_rejected), (0, 0));
    }

    #[test]
    fn the_same_answer_fetched_twice_changes_nothing() {
        let mut conn = crate::db::tests::test_db();
        let published = clock::now();
        let offer = |t: &str| OfferInput {
            title: t.into(),
            company: "Firma Testowa".into(),
            source: "adzuna".into(),
            url: Some(format!("https://example.com/{t}")),
            city: Some("Wrocław".into()),
            description: Some("Opis oferty".into()),
            published_at: Some(published.clone()),
            ..Default::default()
        };
        let answer = || vec![offer("Dev"), offer("QA")];
        save(&mut conn, answer(), &mut SourceRun::default(), &mut FetchReport::default()).unwrap();
        let stamp = "2026-01-01T00:00:00Z";
        conn.execute("UPDATE offers SET updated_at = ?1", [stamp]).unwrap();
        let touched = |conn: &Connection| -> i64 {
            conn.query_row("SELECT count(*) FROM offers WHERE updated_at != ?1", [stamp], |r| r.get(0)).unwrap()
        };

        let mut run = SourceRun::default();
        let mut report = FetchReport::default();
        save(&mut conn, answer(), &mut run, &mut report).unwrap();
        assert_eq!((run.added, run.merged, report.new_on_board), (0, 0, 0));
        assert_eq!(touched(&conn), 0);

        // A longer description from the next fetch is real news - merged and marked as changed.
        let mut longer = offer("Dev");
        longer.description = Some("Opis oferty - dłuższy, z wymaganiami".into());
        let mut run = SourceRun::default();
        save(&mut conn, vec![longer], &mut run, &mut FetchReport::default()).unwrap();
        assert_eq!(run.merged, 1);
        assert_eq!(touched(&conn), 1);
    }

    #[test]
    fn old_offers_are_skipped_and_a_newer_date_starts_the_age_anew() {
        let mut conn = crate::db::tests::test_db();
        let now = clock::now();
        let days_ago = |days: i64| offers::cutoff(&now, days);
        let offer = |t: &str, published: String| OfferInput {
            title: t.into(),
            company: "Firma Testowa".into(),
            source: "adzuna".into(),
            published_at: Some(published),
            ..Default::default()
        };
        let mut run = SourceRun::default();
        save(&mut conn, vec![offer("Dev", days_ago(3)), offer("QA", days_ago(11))], &mut run, &mut FetchReport::default()).unwrap();
        assert_eq!((run.added, run.old), (1, 1));

        // Nobody touched it and its days passed - in the archive, not on the board.
        conn.execute("UPDATE offers SET published_at = ?1", [days_ago(20)]).unwrap();
        assert_eq!(offers::archive_old(&conn, &now).unwrap(), 1);
        assert!(offers::list(&conn, crate::model::View::Board, None).unwrap().is_empty());

        // A source brings it again, freshly published: back on the board, its age from the new date.
        let mut run = SourceRun::default();
        save(&mut conn, vec![offer("Dev", days_ago(1))], &mut run, &mut FetchReport::default()).unwrap();
        assert_eq!(run.merged, 1);
        let rows = offers::list(&conn, crate::model::View::Board, None).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].published_at, days_ago(1));
        assert!(offers::list(&conn, crate::model::View::Archive, None).unwrap().is_empty());
    }
}
