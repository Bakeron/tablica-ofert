//! Collecting offers from the legal sources (zalozenia.md §3): aggregators with an official
//! API (Adzuna, Remotive) and, later, e-mail alerts. Plain code, no AI and no tokens.
//!
//! Every source has its own limits: how often it may run and how many calls a day it may
//! make. The network part runs without holding the database; offers are saved afterwards,
//! each one checked by `offers::add` like any other outside data.

pub mod adzuna;
pub mod alerts;
pub mod mail;
pub mod remotive;

use std::collections::BTreeMap;
use std::sync::atomic::Ordering;

use rusqlite::{Connection, OptionalExtension, params};
use serde::Serialize;
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

use crate::commands::{AppState, with_db};
use crate::error::{AppError, AppResult};
use crate::model::{AddOutcome, OfferInput, Profile, Remote};
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

/// Adzuna does not publish its free limits; this stays well below the commonly quoted ~1000 a month.
fn adzuna_limits(every_hours: i64) -> Limits {
    Limits { every_hours, forced_hours: 0, calls_a_day: 25, calls_a_run: 6 }
}

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
    /// Why it did not run: `off`, `missing_key`, `no_profiles`, `no_keywords`, `not_due`, `limit`.
    pub skipped: Option<&'static str>,
    pub added: i64,
    pub merged: i64,
    /// Already deleted by the user - not brought back.
    pub blocked: i64,
    /// Did not pass the checks of outside data.
    pub invalid: i64,
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
}

#[derive(Debug, Clone, Default)]
struct LastRun {
    last_try: Option<String>,
    last_ok: Option<String>,
}

// ---------- what to search for ----------

/// One search per keyword. Profiles take turns (first keyword of every profile, then the
/// second...), so a run cut by the limits still covers every profile; repeats are skipped.
/// A profile that accepts remote work searches the whole country; one without remote work
/// searches around its first city.
pub fn plan(profiles: &[Profile]) -> Vec<Query> {
    let mut out: Vec<Query> = Vec::new();
    for round in 0..8 {
        for p in profiles {
            let c = &p.criteria;
            let Some(keyword) = c.keywords.iter().map(|k| k.trim()).filter(|k| !k.is_empty()).nth(round) else {
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

/// Days of offers to ask Adzuna for: since the last good run (plus a day), 14 at first.
fn days_back(last: &LastRun, now: OffsetDateTime) -> u32 {
    match hours_since(last.last_ok.as_deref(), now) {
        Some(h) if h >= 0 => ((h / 24) + 2).clamp(1, 30) as u32,
        _ => 14,
    }
}

// ---------- database part ----------

fn last_runs(conn: &Connection) -> AppResult<BTreeMap<String, LastRun>> {
    let mut stmt = conn.prepare("SELECT source, last_try, last_ok FROM source_runs")?;
    let rows = stmt
        .query_map([], |r| Ok((r.get::<_, String>(0)?, LastRun { last_try: r.get(1)?, last_ok: r.get(2)? })))?
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

/// Saves fetched offers one by one - one bad offer never stops the others.
fn save(conn: &mut Connection, found: Vec<OfferInput>, run: &mut SourceRun, report: &mut FetchReport) -> AppResult<()> {
    for offer in found {
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
}

fn snapshot(conn: &Connection) -> AppResult<Snapshot> {
    Ok(Snapshot {
        settings: settings::all(conn)?,
        profiles: profiles::list(conn)?,
        runs: last_runs(conn)?,
        adzuna_calls: usage::calls_today(conn, adzuna::SOURCE)?,
        remotive_calls: usage::calls_today(conn, remotive::SOURCE)?,
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
/// (shorter waits, daily limits still apply).
pub async fn fetch(state: &AppState, force: bool) -> AppResult<FetchReport> {
    if state.fetching.swap(true, Ordering::SeqCst) {
        return Err(AppError::Busy);
    }
    let _running = Running(state);
    let snap = with_db(state, |c| snapshot(c))?;
    let now = OffsetDateTime::now_utc();
    let mut report = FetchReport::default();
    let on = |key: &str| snap.settings.get(key).is_some_and(|v| v == "on");
    let client = net::client()?;

    // Adzuna
    if on("source_adzuna") {
        let every: i64 = snap.settings.get("fetch_every").and_then(|v| v.parse().ok()).unwrap_or(6);
        let limits = adzuna_limits(every);
        let last = snap.runs.get(adzuna::SOURCE).cloned().unwrap_or_default();
        let mut run = SourceRun { source: adzuna::SOURCE.into(), ..Default::default() };
        let queries = plan(&snap.profiles);
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
            let days = days_back(&last, now);
            let mut found = Vec::new();
            let mut calls = 0;
            for q in for_this_run(&queries, limits.calls_a_run.min(left), snap.adzuna_calls) {
                calls += 1;
                match adzuna::search(&client, &keys, &country, q, days).await {
                    Ok(mut list) => found.append(&mut list),
                    Err(e) => {
                        run.error = Some((&e).into());
                        break;
                    }
                }
            }
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
            for (i, word) in words.iter().take(limits.calls_a_run.min(left)).enumerate() {
                if i > 0 {
                    tokio::time::sleep(std::time::Duration::from_secs(REMOTIVE_PAUSE_SECS)).await;
                }
                calls += 1;
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
            // IMAP is blocking - it runs on a worker thread, not on the async runtime.
            let result = tauri::async_runtime::spawn_blocking(move || mail::fetch(&config, &position))
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

    Ok(report)
}

/// State of every source for the settings window.
/// `adzuna_ready` = both Adzuna keys are in the credential store,
/// `mail_ready` = mailbox address, server and password are set.
pub fn status(conn: &Connection, adzuna_ready: bool, mail_ready: bool) -> AppResult<Vec<SourceStatus>> {
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
        })
    };
    Ok(vec![
        one(adzuna::SOURCE, adzuna_ready, adzuna_limits(6).calls_a_day)?,
        one(remotive::SOURCE, true, REMOTIVE_LIMITS.calls_a_day)?,
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
        let q = plan(&p);
        let words: Vec<(&str, Option<&str>)> = q.iter().map(|q| (q.keyword.as_str(), q.city.as_deref())).collect();
        // Profiles take turns; the same word with the same place is searched once.
        assert_eq!(words, vec![("react", None), ("księgowa", Some("Opole")), ("typescript", None)]);
        assert_eq!(q[1].radius_km, 30);
    }

    #[test]
    fn runs_take_turns_through_the_keywords() {
        let p = vec![profile(&["a1", "a2", "a3", "a4", "a5", "a6", "a7", "a8"], vec![], None)];
        let q = plan(&p);
        let words = |calls_today| for_this_run(&q, 6, calls_today).iter().map(|q| q.keyword.clone()).collect::<Vec<_>>();
        assert_eq!(words(0), vec!["a1", "a2", "a3", "a4", "a5", "a6"]);
        assert_eq!(words(6), vec!["a7", "a8", "a1", "a2", "a3", "a4"]);
        // Never the same search twice in one run.
        let short = plan(&[profile(&["x", "y"], vec![], None)]);
        assert_eq!(for_this_run(&short, 6, 5).len(), 2);
        assert!(for_this_run(&[], 6, 0).is_empty());
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
        let fresh = LastRun { last_try: Some("2026-11-02T10:00:00Z".into()), last_ok: Some("2026-11-02T10:00:00Z".into()) };
        let old = LastRun { last_try: Some("2026-11-01T05:00:00Z".into()), last_ok: Some("2026-10-20T05:00:00Z".into()) };
        assert!(due(&LastRun::default(), &REMOTIVE_LIMITS, false, now));
        assert!(!due(&fresh, &REMOTIVE_LIMITS, false, now));
        assert!(!due(&fresh, &REMOTIVE_LIMITS, true, now));
        assert!(due(&old, &REMOTIVE_LIMITS, false, now));
        assert!(due(&fresh, &adzuna_limits(6), true, now));
        assert!(!due(&fresh, &adzuna_limits(6), false, now));
        assert_eq!(days_back(&LastRun::default(), now), 14);
        assert_eq!(days_back(&fresh, now), 2);
        assert_eq!(days_back(&old, now), 15);
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
        let s = status(&conn, false, false).unwrap().into_iter().find(|s| s.source == "adzuna").unwrap();
        assert_eq!(s.last_try.as_deref(), Some("2026-11-02T16:00:00Z"));
        assert_eq!(s.last_ok.as_deref(), Some("2026-11-02T10:00:00Z"));
        assert_eq!(s.last_error.as_deref(), Some("timeout"));
        assert_eq!((s.added, s.merged), (3, 1));
    }

    #[test]
    fn saving_counts_every_outcome() {
        let mut conn = crate::db::tests::test_db();
        let offer = |t: &str| OfferInput { title: t.into(), company: "Firma Testowa".into(), source: "adzuna".into(), ..Default::default() };
        let mut run = SourceRun::default();
        let mut report = FetchReport::default();
        save(&mut conn, vec![offer("Dev"), offer("Dev"), offer("  "), offer("QA")], &mut run, &mut report).unwrap();
        assert_eq!((run.added, run.merged, run.invalid), (2, 1, 1));
        assert_eq!(report.new_on_board, 2);
        assert_eq!((report.new_review, report.new_rejected), (0, 0));
    }
}
