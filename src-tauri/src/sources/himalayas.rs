//! Himalayas - remote jobs, public JSON API without a key (himalayas.app/docs/remote-jobs-api,
//! checked 4.11.2026). Their terms: "Apply" uses the API's `applicationLink`, visible credit
//! with a link to himalayas.app, no passing the offers on to other job boards, and closed jobs
//! are dropped - "Jobs can close before their expiryDate"; the single-job endpoint answers 410
//! (or 404) for a closed job. The data changes once a day, so one run a day is enough.
//!
//! Closed jobs (the author's choice of 4.11.2026): every offer from Himalayas is checked daily.
//! When closed (or past its expiry date), an offer the user never acted on is deleted for good;
//! one the user acted on (stage, note, interest, star, dates, contacts) keeps only the user's own
//! record - the Himalayas text is removed (`close`), so interview reminders are never lost.

use reqwest::Client;
use serde::Deserialize;
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

use rusqlite::{OptionalExtension, params};

use crate::error::{AppError, AppResult};
use crate::model::{OfferInput, Remote, SalaryPeriod};
use crate::net;
use crate::text::{cut, html_to_text, one_line};

pub const SOURCE: &str = "himalayas";
const SEARCH_URL: &str = "https://himalayas.app/jobs/api/search";
const JOB_URL: &str = "https://himalayas.app/jobs/api/companies";

#[derive(Debug, Deserialize)]
struct Answer {
    #[serde(default)]
    jobs: Vec<Job>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Job {
    pub title: Option<String>,
    pub company_name: Option<String>,
    pub employment_type: Option<String>,
    pub min_salary: Option<f64>,
    pub max_salary: Option<f64>,
    pub salary_period: Option<String>,
    pub currency: Option<String>,
    /// A list of words or of objects - the docs and the answers differ.
    pub seniority: serde_json::Value,
    /// Country names or `{ name }` objects; empty = worldwide.
    pub location_restrictions: Vec<serde_json::Value>,
    pub description: Option<String>,
    pub expiry_date: Option<i64>,
    pub pub_date: Option<i64>,
    pub application_link: Option<String>,
}

/// One search by a keyword (one API call, the first 20 results - newest first). `country` - the
/// "Country of offers" setting (shared with Adzuna): only offers people living there may take,
/// worldwide ones included.
pub async fn search(client: &Client, keyword: &str, country: &str) -> AppResult<Vec<OfferInput>> {
    let country = country.to_ascii_uppercase();
    let params = [("q", keyword), ("country", country.as_str()), ("sort", "recent")];
    let resp = client.get(SEARCH_URL).query(&params).send().await.map_err(net::net_error)?;
    let answer: Answer = net::json(resp).await?;
    Ok(answer.jobs.into_iter().filter_map(to_offer).collect())
}

/// What Himalayas says about one job.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum JobState {
    Open,
    /// 404 or 410.
    Closed,
    /// Any other error answer for this one job (e.g. 500) - try it again another day.
    Unknown,
}

/// State of one job. `link` is the offer's `applicationLink`. No connection, a refused request
/// or the limit (429) is an error - the whole round stops.
pub async fn job_state(client: &Client, link: &str) -> AppResult<JobState> {
    let Some((company, slug)) = job_path(link) else { return Ok(JobState::Open) };
    let resp = client.get(format!("{JOB_URL}/{company}/jobs/{slug}")).send().await.map_err(net::net_error)?;
    match resp.status().as_u16() {
        404 | 410 => Ok(JobState::Closed),
        401 | 403 | 429 => net::check_status(resp).await.map(|_| JobState::Unknown),
        s if (200..300).contains(&s) => Ok(JobState::Open),
        _ => Ok(JobState::Unknown),
    }
}

/// So many unknown answers in a row mean the service is down - the round stops.
const UNKNOWN_IN_A_ROW: usize = 5;

/// One round of checks.
#[derive(Debug, Default)]
pub struct Round {
    /// Every offer that got an answer (open, closed or unknown) - not checked again today.
    pub checked: Vec<i64>,
    pub closed: Vec<i64>,
    /// Offers with a clear open / closed answer.
    pub answered: usize,
    /// Why the round stopped early (next run goes on from the first unchecked offer).
    pub stopped: Option<AppError>,
}

/// Checks the offers one by one with a pause between them. One odd answer never stops the
/// others; no connection, the limit or several unknown answers in a row stop the round.
pub async fn check_round<F, Fut>(offers: &[(i64, String)], pause: std::time::Duration, mut check: F) -> Round
where
    F: FnMut(String) -> Fut,
    Fut: std::future::Future<Output = AppResult<JobState>>,
{
    let mut round = Round::default();
    let mut unknown_in_a_row = 0;
    for (i, (id, link)) in offers.iter().enumerate() {
        if i > 0 {
            tokio::time::sleep(pause).await;
        }
        match check(link.clone()).await {
            Ok(state) => {
                round.checked.push(*id);
                if state == JobState::Unknown {
                    unknown_in_a_row += 1;
                    if unknown_in_a_row >= UNKNOWN_IN_A_ROW {
                        round.stopped = Some(AppError::Net("Himalayas does not answer for single jobs".into()));
                        break;
                    }
                    continue;
                }
                unknown_in_a_row = 0;
                round.answered += 1;
                if state == JobState::Closed {
                    round.closed.push(*id);
                }
            }
            Err(e) => {
                round.stopped = Some(e);
                break;
            }
        }
    }
    round
}

/// More than this many closed at once, and more than half of the answers, looks like a change
/// on Himalayas' side (new addresses), not real closed jobs.
const SUSPICIOUS_ABOVE: usize = 5;

/// Too many closed at once? Then nothing is deleted for good: every closed offer is kept
/// without the Himalayas text (their terms are kept, the user loses nothing), and the source
/// shows an error.
pub fn suspicious(round: &Round) -> bool {
    round.closed.len() > SUSPICIOUS_ABOVE && round.closed.len() * 2 > round.answered
}

/// `https://himalayas.app/companies/<company>/jobs/<slug>?utm...` -> (company, slug).
/// Only plain slugs - nothing else ever goes into the address.
fn job_path(link: &str) -> Option<(&str, &str)> {
    let path = link.strip_prefix("https://himalayas.app/companies/")?;
    let path = path.split(['?', '#']).next()?;
    let mut parts = path.split('/');
    let (company, jobs, slug) = (parts.next()?, parts.next()?, parts.next()?);
    let plain = |s: &str| !s.is_empty() && s.len() <= 200 && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_');
    (jobs == "jobs" && parts.next().is_none_or(str::is_empty) && plain(company) && plain(slug)).then_some((company, slug))
}

fn words(v: &serde_json::Value) -> Vec<String> {
    let one = |v: &serde_json::Value| match v {
        serde_json::Value::String(s) => Some(s.clone()),
        serde_json::Value::Object(o) => o.get("name").and_then(|n| n.as_str()).map(str::to_string),
        _ => None,
    };
    let all: Vec<String> = match v {
        serde_json::Value::Array(list) => list.iter().filter_map(one).collect(),
        other => one(other).into_iter().collect(),
    };
    all.iter().map(|s| one_line(s, 60)).filter(|s| !s.is_empty()).collect()
}

/// Pay as a monthly or hourly range (the app's two periods); other periods stay only in the text.
fn salary(job: &Job) -> (Option<i64>, Option<i64>, Option<SalaryPeriod>) {
    let ok = |v: Option<f64>| v.filter(|v| v.is_finite() && *v > 0.0);
    let (min, max) = (ok(job.min_salary), ok(job.max_salary));
    if min.is_none() && max.is_none() {
        return (None, None, None);
    }
    let (div, period) = match job.salary_period.as_deref().unwrap_or("annual") {
        "annual" => (12.0, SalaryPeriod::Month),
        "monthly" => (1.0, SalaryPeriod::Month),
        "hourly" => (1.0, SalaryPeriod::Hour),
        _ => return (None, None, None),
    };
    let round = |v: f64| if period == SalaryPeriod::Hour { v.round() as i64 } else { ((v / div / 100.0).round() * 100.0) as i64 };
    (min.map(round), max.map(round), Some(period))
}

pub fn to_offer(job: Job) -> Option<OfferInput> {
    let title = one_line(&html_to_text(job.title.as_deref()?), 300);
    let company = one_line(&html_to_text(job.company_name.as_deref()?), 300);
    // Their condition: "Apply" goes through this link.
    let url = job.application_link.clone().filter(|u| u.starts_with("https://"))?;
    if title.is_empty() || company.is_empty() {
        return None;
    }
    let currency = job.currency.as_deref().map(str::trim).filter(|c| c.len() == 3 && c.bytes().all(|b| b.is_ascii_alphabetic()));
    let (salary_min, salary_max, salary_period) = if currency.is_some() { salary(&job) } else { (None, None, None) };
    let has_salary = salary_min.is_some() || salary_max.is_some();
    let mut head = Vec::new();
    let places: Vec<String> = job.location_restrictions.iter().flat_map(words).take(30).collect();
    head.push(format!("Candidate location: {}", if places.is_empty() { "worldwide".into() } else { places.join(", ") }));
    if let (Some(c), Some(p)) = (currency, job.salary_period.as_deref()) {
        if !has_salary && (job.min_salary.is_some() || job.max_salary.is_some()) {
            let n = |v: Option<f64>| v.filter(|v| v.is_finite() && *v > 0.0).map(|v| (v.round() as i64).to_string());
            let range = [n(job.min_salary), n(job.max_salary)].into_iter().flatten().collect::<Vec<_>>().join(" - ");
            if !range.is_empty() {
                head.push(format!("Salary: {range} {} ({})", c.to_uppercase(), one_line(p, 20)));
            }
        }
    }
    let level = words(&job.seniority);
    let description = format!("{}\n\n{}", head.join("\n"), job.description.as_deref().map(html_to_text).unwrap_or_default());
    Some(OfferInput {
        title,
        company,
        source: SOURCE.into(),
        url: Some(url),
        remote: Some(Remote::Yes),
        salary_min,
        salary_max,
        salary_currency: has_salary.then(|| currency.unwrap_or_default().to_uppercase()),
        salary_period: if has_salary { salary_period } else { None },
        contract: job.employment_type.as_deref().map(|t| one_line(t, 60)).filter(|t| !t.is_empty()),
        level: (!level.is_empty()).then(|| level.join(", ")),
        description: Some(cut(description.trim(), 60_000)),
        expires_at: job
            .expiry_date
            .and_then(|s| OffsetDateTime::from_unix_timestamp(s).ok())
            .and_then(|t| t.format(&Rfc3339).ok()),
        published_at: job.pub_date.map(|s| s.to_string()),
        ..Default::default()
    })
}

// ---------- closed jobs ----------

/// Offers from Himalayas whose text is still here (closed ones are handled only once).
const FROM_HIMALAYAS: &str = "EXISTS (SELECT 1 FROM offer_sources s WHERE s.offer_id = o.id AND s.source = 'himalayas')
    AND o.content_removed IS NULL";

/// Only from Himalayas, and the user never acted on it.
const ONLY_HIMALAYAS_UNTOUCHED: &str = "NOT EXISTS (SELECT 1 FROM offer_sources s WHERE s.offer_id = o.id AND s.source != 'himalayas')
    AND o.stage = 'new' AND o.notes = '' AND o.interest IS NULL AND o.favorite = 0
    AND NOT EXISTS (SELECT 1 FROM events e WHERE e.offer_id = o.id)
    AND NOT EXISTS (SELECT 1 FROM contacts c WHERE c.offer_id = o.id)
    AND NOT EXISTS (SELECT 1 FROM contact_log l WHERE l.offer_id = o.id)";

/// At most this many single-job checks in one run (their limit is 60 a minute); the next run
/// goes on with the offers not checked today.
pub const MAX_CHECKS: usize = 500;
/// Pause between two checks.
pub const CHECK_PAUSE_MS: u64 = 1_100;
/// Daily counter of the checks (`api_usage`).
pub const CHECK_KEY: &str = "himalayas-check";

/// What happened to the closed offers.
#[derive(Debug, Default, PartialEq)]
pub struct Closed {
    /// Deleted for good (the user never acted on them).
    pub deleted: usize,
    /// Kept as the user's own record without the Himalayas text.
    pub kept: usize,
}

/// Offers from Himalayas past their expiry date are closed.
pub fn close_expired(conn: &mut rusqlite::Connection, now: &str) -> AppResult<Closed> {
    let ids: Vec<i64> = {
        let mut stmt = conn.prepare(&format!(
            "SELECT o.id FROM offers o WHERE {FROM_HIMALAYAS} AND o.expires_at IS NOT NULL AND o.expires_at < ?1"
        ))?;
        stmt.query_map([now], |r| r.get(0))?.collect::<Result<_, _>>()?
    };
    close(conn, &ids)
}

/// Offers from Himalayas not checked yet on `day` (their terms: drop closed jobs daily).
/// Returns (offer id, apply link).
pub fn to_check(conn: &rusqlite::Connection, day: &str) -> AppResult<Vec<(i64, String)>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT o.id, s.url FROM offers o JOIN offer_sources s ON s.offer_id = o.id AND s.source = 'himalayas'
         WHERE {FROM_HIMALAYAS} AND s.url IS NOT NULL AND (s.checked_on IS NULL OR s.checked_on < ?1)
         ORDER BY o.id LIMIT {MAX_CHECKS}"
    ))?;
    let all = stmt.query_map([day], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<Result<_, _>>()?;
    Ok(all)
}

/// Saves the result of a round: the offers are not checked again on `day`, the closed ones are
/// closed - or, when the round looks wrong (`suspicious`), all kept without the Himalayas text.
pub fn finish_round(conn: &mut rusqlite::Connection, round: &Round, day: &str) -> AppResult<Closed> {
    {
        let mut stmt = conn.prepare("UPDATE offer_sources SET checked_on = ?2 WHERE offer_id = ?1 AND source = 'himalayas'")?;
        for id in &round.checked {
            stmt.execute(params![id, day])?;
        }
    }
    if suspicious(round) { keep(conn, &round.closed) } else { close(conn, &round.closed) }
}

/// The jobs closed on Himalayas. An offer only from Himalayas that the user never acted on is
/// deleted for good (like emptying the trash: never fetched again, gone on the phone too).
/// Any other offer keeps the user's own record - title, company, notes, stage, dates, contacts -
/// and loses what came from Himalayas: the description and requirements (with what AI read from
/// them) and the Himalayas link; an offer only from Himalayas also its pay, level and contract.
pub fn close(conn: &mut rusqlite::Connection, ids: &[i64]) -> AppResult<Closed> {
    strip(conn, ids, false)
}

/// Like `close`, but nothing is deleted for good - every offer keeps its row without the
/// Himalayas text (when the closings look like a fault on their side).
pub fn keep(conn: &mut rusqlite::Connection, ids: &[i64]) -> AppResult<Closed> {
    strip(conn, ids, true)
}

fn strip(conn: &mut rusqlite::Connection, ids: &[i64], keep_all: bool) -> AppResult<Closed> {
    let mut out = Closed::default();
    if ids.is_empty() {
        return Ok(out);
    }
    let now = crate::clock::now();
    let tx = conn.transaction()?;
    for id in ids {
        let state: Option<(bool, bool)> = tx
            .query_row(
                &format!(
                    "SELECT {ONLY_HIMALAYAS_UNTOUCHED},
                        NOT EXISTS (SELECT 1 FROM offer_sources s WHERE s.offer_id = o.id AND s.source != 'himalayas')
                     FROM offers o WHERE o.id = ?1 AND {FROM_HIMALAYAS}"
                ),
                [id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        let Some((untouched, only_himalayas)) = state else { continue };
        if untouched && !keep_all {
            tx.execute(
                "INSERT OR IGNORE INTO removed_offers (fingerprint, removed_at) SELECT fingerprint, ?2 FROM offers WHERE id = ?1",
                params![id, now],
            )?;
            tx.execute(
                "INSERT INTO sync_deleted (kind, uid, at) SELECT 'offers', uid, ?2 FROM offers WHERE id = ?1
                 ON CONFLICT (kind, uid) DO UPDATE SET at = excluded.at",
                params![id, now],
            )?;
            out.deleted += tx.execute("DELETE FROM offers WHERE id = ?1", [id])?;
            continue;
        }
        tx.execute(
            "UPDATE offers SET description = NULL, requirements = NULL, ai_summary = NULL, ai_flags = NULL,
                ai_remote_quote = NULL, ai_tr_lang = NULL, ai_title_tr = NULL, ai_description_tr = NULL,
                url = CASE WHEN url LIKE 'https://himalayas.app/%' THEN NULL ELSE url END,
                content_removed = 'himalayas', updated_at = ?2
             WHERE id = ?1",
            params![id, now],
        )?;
        if only_himalayas {
            tx.execute(
                "UPDATE offers SET salary_min = NULL, salary_max = NULL, salary_currency = NULL, salary_period = NULL,
                    level = NULL, contract = NULL, expired = 1
                 WHERE id = ?1",
                [id],
            )?;
        }
        // Quotes from the removed text go too; the scores and their short reasons stay.
        tx.execute("UPDATE offer_scores SET quote = NULL WHERE offer_id = ?1", [id])?;
        tx.execute("UPDATE offer_sources SET url = NULL WHERE offer_id = ?1 AND source = 'himalayas'", [id])?;
        crate::profiles::recheck_offer(&tx, *id)?;
        out.kept += 1;
    }
    tx.commit()?;
    Ok(out)
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::tests::test_db;
    use crate::offers;

    // Shape of the real answer (4.11.2026); companies and offers are made up.
    const SAMPLE: &str = r#"{
      "comments": "Search results come 20 to a page.", "updatedAt": 1793663934, "offset": 0, "limit": 20, "totalCount": 3,
      "jobs": [
        {
          "title": "WordPress Developer", "excerpt": "Short text", "companyName": "Przykładowa Firma",
          "companySlug": "przykladowa-firma", "companyLogo": "https://example.com/logo.png",
          "employmentType": "Full Time", "minSalary": 48000, "maxSalary": 60000, "salaryPeriod": "annual",
          "seniority": ["Mid-level"], "currency": "EUR", "locationRestrictions": ["Poland", "Serbia"],
          "timezoneRestrictions": [1, 2], "categories": ["WordPress-Developer"], "parentCategories": ["Developer"],
          "description": "<p>We build <strong>plugins</strong> &amp; themes.</p>",
          "pubDate": 1793238550, "expiryDate": 1795799025,
          "applicationLink": "https://himalayas.app/companies/przykladowa-firma/jobs/wordpress-developer?utm_source=x",
          "guid": "https://himalayas.app/companies/przykladowa-firma/jobs/wordpress-developer"
        },
        {
          "title": "PHP Contractor", "companyName": "Firma Zmyślona", "minSalary": 20, "maxSalary": 30,
          "salaryPeriod": "hourly", "currency": "USD", "locationRestrictions": [], "seniority": [],
          "applicationLink": "https://himalayas.app/companies/firma-zmyslona/jobs/php-contractor"
        },
        { "title": "Dev", "companyName": "X", "applicationLink": "http://insecure.example/job" }
      ]
    }"#;

    fn sample() -> Vec<OfferInput> {
        let answer: Answer = serde_json::from_str(SAMPLE).unwrap();
        answer.jobs.into_iter().filter_map(to_offer).collect()
    }

    #[test]
    fn answer_becomes_offers() {
        let offers = sample();
        assert_eq!(offers.len(), 2);
        let a = &offers[0];
        assert_eq!((a.title.as_str(), a.company.as_str()), ("WordPress Developer", "Przykładowa Firma"));
        assert_eq!(a.url.as_deref(), Some("https://himalayas.app/companies/przykladowa-firma/jobs/wordpress-developer?utm_source=x"));
        assert_eq!(a.remote, Some(Remote::Yes));
        assert_eq!((a.salary_min, a.salary_max, a.salary_period), (Some(4000), Some(5000), Some(SalaryPeriod::Month)));
        assert_eq!(a.salary_currency.as_deref(), Some("EUR"));
        assert_eq!(a.level.as_deref(), Some("Mid-level"));
        assert_eq!(a.contract.as_deref(), Some("Full Time"));
        assert_eq!(a.expires_at.as_deref(), Some("2026-11-27T17:03:45Z"));
        let d = a.description.as_deref().unwrap();
        assert!(d.starts_with("Candidate location: Poland, Serbia\n\nWe build plugins & themes."), "{d}");

        let b = &offers[1];
        assert_eq!((b.salary_min, b.salary_max, b.salary_period), (Some(20), Some(30), Some(SalaryPeriod::Hour)));
        assert!(b.description.as_deref().unwrap().starts_with("Candidate location: worldwide"));
        assert_eq!(b.level, None);
    }

    #[test]
    fn odd_pay_periods_stay_in_the_text() {
        let job = Job {
            title: Some("Dev".into()),
            company_name: Some("Firma".into()),
            min_salary: Some(1000.0),
            salary_period: Some("weekly".into()),
            currency: Some("usd".into()),
            application_link: Some("https://himalayas.app/companies/firma/jobs/dev".into()),
            ..Default::default()
        };
        let o = to_offer(job).unwrap();
        assert_eq!((o.salary_min, o.salary_currency), (None, None));
        assert!(o.description.unwrap().contains("Salary: 1000 USD (weekly)"));
    }

    #[test]
    fn only_plain_job_links_are_checked() {
        assert_eq!(
            job_path("https://himalayas.app/companies/acme/jobs/php-dev?utm_source=x"),
            Some(("acme", "php-dev"))
        );
        assert_eq!(job_path("https://himalayas.app/companies/acme/jobs/php-dev/"), Some(("acme", "php-dev")));
        assert_eq!(job_path("https://himalayas.app/companies/acme/jobs/../../x"), None);
        assert_eq!(job_path("https://himalayas.app/companies/acme/jobs/a%2Fb"), None);
        assert_eq!(job_path("https://evil.example/companies/acme/jobs/x"), None);
        assert_eq!(job_path("https://himalayas.app/companies/acme/about/x"), None);
    }

    #[test]
    fn untouched_offers_are_dropped_and_the_users_history_stays() {
        let mut conn = test_db();
        let mut ids = Vec::new();
        for (i, mut o) in sample().into_iter().enumerate() {
            o.expires_at = Some(if i == 0 { "2026-11-01T00:00:00Z" } else { "2026-12-31T00:00:00Z" }.into());
            ids.push(match offers::add(&mut conn, o).unwrap() {
                crate::model::AddOutcome::Added { id } => id,
                other => panic!("{other:?}"),
            });
        }
        // An expired offer the user acted on, and one also seen in the user's own alerts.
        let mut kept = sample().remove(0);
        kept.title = "Senior WordPress Developer".into();
        kept.expires_at = Some("2026-11-01T00:00:00Z".into());
        let kept = match offers::add(&mut conn, kept).unwrap() {
            crate::model::AddOutcome::Added { id } => id,
            other => panic!("{other:?}"),
        };
        conn.execute("UPDATE offers SET notes = 'Napisać w poniedziałek' WHERE id = ?1", [kept]).unwrap();
        let mut both = sample().remove(1);
        both.title = "PHP Developer".into();
        both.expires_at = Some("2026-11-01T00:00:00Z".into());
        offers::add(&mut conn, both.clone()).unwrap();
        both.source = "mail".into();
        offers::add(&mut conn, both).unwrap();

        conn.execute(
            "INSERT INTO events (uid, offer_id, kind, at, minutes, note, created_at)
             VALUES ('e1', ?1, 'interview', '2026-11-10T09:00:00Z', 60, 'Rozmowa techniczna', '2026-11-04T10:00:00Z')",
            [kept],
        )
        .unwrap();

        let closed = close_expired(&mut conn, "2026-11-04T12:00:00Z").unwrap();
        assert_eq!(closed, Closed { deleted: 1, kept: 2 });
        let left: Vec<String> = conn
            .prepare("SELECT title FROM offers ORDER BY id")
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(left, vec!["PHP Contractor", "Senior WordPress Developer", "PHP Developer"]);
        // Never fetched again, and deleted on the phone too.
        assert_eq!(offers::add(&mut conn, sample().remove(0)).unwrap(), crate::model::AddOutcome::Blocked);
        let tombstones: i64 = conn.query_row("SELECT count(*) FROM sync_deleted WHERE kind = 'offers'", [], |r| r.get(0)).unwrap();
        assert_eq!(tombstones, 1);

        // The user's own record stays - notes and the interview (its reminder) - without the Himalayas text.
        let d = offers::detail(&conn, kept).unwrap();
        assert_eq!((d.notes.as_str(), d.content_removed.as_deref()), ("Napisać w poniedziałek", Some("himalayas")));
        assert_eq!((d.description, d.url, d.row.salary_min, d.row.expired), (None, None, None, true));
        assert_eq!(d.agenda.events.len(), 1);
        assert!(d.links.iter().all(|l| l.url.is_none()));

        // Only offers that still have Himalayas text are checked; a closed untouched one goes.
        let check = to_check(&conn, "2026-11-04").unwrap();
        assert_eq!(check.iter().map(|c| c.0).collect::<Vec<_>>(), vec![ids[1]]);
        assert!(check[0].1.ends_with("/jobs/php-contractor"));
        assert_eq!(close(&mut conn, &[ids[1]]).unwrap(), Closed { deleted: 1, kept: 0 });

        // Open again on Himalayas: the link is back and it is checked again.
        let mut again = sample().remove(0);
        again.title = "Senior WordPress Developer".into();
        offers::add(&mut conn, again).unwrap();
        assert_eq!(to_check(&conn, "2026-11-04").unwrap().iter().map(|c| c.0).collect::<Vec<_>>(), vec![kept]);    }

    #[test]
    fn every_user_action_keeps_the_offer() {
        let mut conn = test_db();
        let mut add = |title: &str, source: &str| {
            let mut o = sample().remove(0);
            o.title = title.into();
            o.source = source.into();
            o.expires_at = Some("2026-11-01T00:00:00Z".into());
            match offers::add(&mut conn, o).unwrap() {
                crate::model::AddOutcome::Added { id } | crate::model::AddOutcome::Merged { id } => id,
                other => panic!("{other:?}"),
            }
        };
        let stage = add("Oferta etap", "himalayas");
        let favorite = add("Oferta ulubiona", "himalayas");
        let interest = add("Oferta ciekawa", "himalayas");
        let contact = add("Oferta kontakt", "himalayas");
        let logged = add("Oferta rozmowa", "himalayas");
        let untouched = add("Oferta nieruszona", "himalayas");
        let with_mail = add("Oferta z maila", "himalayas");
        add("Oferta z maila", "mail");

        conn.execute("UPDATE offers SET stage = 'applied' WHERE id = ?1", [stage]).unwrap();
        conn.execute("UPDATE offers SET favorite = 1 WHERE id = ?1", [favorite]).unwrap();
        conn.execute("UPDATE offers SET interest = 'high' WHERE id = ?1", [interest]).unwrap();
        conn.execute(
            "INSERT INTO contacts (offer_id, name, created_at) VALUES (?1, 'Anna Przykładowa', '2026-11-02T10:00:00Z')",
            [contact],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO contact_log (offer_id, at, kind) VALUES (?1, '2026-11-02T10:00:00Z', 'call')",
            [logged],
        )
        .unwrap();
        let mail_url: Option<String> = conn
            .query_row("SELECT url FROM offer_sources WHERE offer_id = ?1 AND source = 'mail'", [with_mail], |r| r.get(0))
            .unwrap();

        let closed = close_expired(&mut conn, "2026-11-04T12:00:00Z").unwrap();
        assert_eq!(closed, Closed { deleted: 1, kept: 6 });
        let gone: i64 = conn.query_row("SELECT count(*) FROM offers WHERE id = ?1", [untouched], |r| r.get(0)).unwrap();
        assert_eq!(gone, 0);
        // The link from the user's own mail stays.
        let after: Option<String> = conn
            .query_row("SELECT url FROM offer_sources WHERE offer_id = ?1 AND source = 'mail'", [with_mail], |r| r.get(0))
            .unwrap();
        assert_eq!(after, mail_url);
    }

    fn links(n: i64) -> Vec<(i64, String)> {
        (1..=n).map(|i| (i, format!("https://himalayas.app/companies/firma/jobs/oferta-{i}"))).collect()
    }

    fn round(offers: &[(i64, String)], answers: Vec<AppResult<JobState>>) -> Round {
        let mut answers = answers.into_iter();
        tauri::async_runtime::block_on(check_round(offers, std::time::Duration::ZERO, |_| {
            let answer = answers.next().unwrap_or(Ok(JobState::Open));
            async move { answer }
        }))
    }

    #[test]
    fn one_odd_answer_does_not_stop_the_round() {
        use JobState::*;
        let r = round(&links(4), vec![Ok(Open), Ok(Unknown), Ok(Closed), Ok(Open)]);
        assert_eq!((r.checked, r.closed, r.answered), (vec![1, 2, 3, 4], vec![3], 3));
        assert!(r.stopped.is_none());
    }

    #[test]
    fn no_connection_stops_the_round_and_it_goes_on_later() {
        use JobState::*;
        let r = round(&links(4), vec![Ok(Open), Err(AppError::Net("offline".into()))]);
        assert_eq!(r.checked, vec![1]);
        assert!(matches!(r.stopped, Some(AppError::Net(_))));
        // Several unknown answers in a row - the service is down.
        let r = round(&links(8), vec![Ok(Open), Ok(Unknown), Ok(Unknown), Ok(Unknown), Ok(Unknown), Ok(Unknown)]);
        assert_eq!(r.checked.len(), 6);
        assert!(r.stopped.is_some());
    }

    #[test]
    fn many_closed_at_once_deletes_nothing() {
        let mut conn = test_db();
        let mut ids = Vec::new();
        for i in 0..8 {
            let mut o = sample().remove(0);
            o.title = format!("Oferta {i}");
            match offers::add(&mut conn, o).unwrap() {
                crate::model::AddOutcome::Added { id } => ids.push(id),
                other => panic!("{other:?}"),
            }
        }
        let day = "2026-11-04";
        let checks = to_check(&conn, day).unwrap();
        assert_eq!(checks.len(), 8);
        // 7 of 8 closed at once - looks like new addresses on Himalayas, not closed jobs.
        let mut answers = vec![Ok(JobState::Open)];
        answers.extend((0..7).map(|_| Ok(JobState::Closed)));
        let r = round(&checks, answers);
        assert!(suspicious(&r));
        assert_eq!(finish_round(&mut conn, &r, day).unwrap(), Closed { deleted: 0, kept: 7 });
        let left: i64 = conn.query_row("SELECT count(*) FROM offers", [], |r| r.get(0)).unwrap();
        assert_eq!(left, 8);
        let stripped: i64 = conn
            .query_row("SELECT count(*) FROM offers WHERE content_removed = 'himalayas' AND description IS NULL", [], |r| r.get(0))
            .unwrap();
        assert_eq!(stripped, 7);
        // Checked today - nothing more to do until tomorrow.
        assert!(to_check(&conn, day).unwrap().is_empty());
        assert_eq!(to_check(&conn, "2026-11-05").unwrap().len(), 1);

        // A few closed among many open - a normal day, untouched offers go for good.
        assert!(!suspicious(&Round { closed: vec![1, 2], answered: 3, ..Default::default() }));
        assert!(!suspicious(&Round { closed: vec![1, 2, 3, 4, 5, 6], answered: 20, ..Default::default() }));
    }
}
