//! "What I'm looking for" profiles, shared work experience, and storing hard filter results.

use rusqlite::{Connection, OptionalExtension, Transaction, params};

use crate::clock;
use crate::error::{AppError, AppResult};
use crate::filter::{self, Facts};
use crate::model::*;

const MAX_NAME: usize = 40;
const MAX_DESCRIPTION: usize = 10_000;
const MAX_EXPERIENCE: usize = 20_000;
const MAX_PROFILES: i64 = 12;
const MAX_LIST: usize = 50;
const MAX_WORD: usize = 100;
const MAX_CITIES: usize = 20;
const MAX_RADIUS_KM: u16 = 300;
const MAX_COMMUTE: u8 = 31;
const MAX_SALARY: i64 = 10_000_000;

// ---------- validation ----------

fn clean_text(value: &str, max: usize, field: &str, required: bool) -> AppResult<String> {
    let trimmed = value.trim();
    if required && trimmed.is_empty() {
        return Err(AppError::Invalid(format!("{field} is required")));
    }
    if trimmed.chars().count() > max {
        return Err(AppError::Invalid(format!("{field} is too long (max {max} characters)")));
    }
    if trimmed.chars().any(|c| c.is_control() && c != '\n' && c != '\r' && c != '\t') {
        return Err(AppError::Invalid(format!("{field} contains control characters")));
    }
    Ok(trimmed.to_string())
}

/// Trimmed, without empty entries and repeats (case-insensitive), limited in count and length.
fn clean_list(values: Vec<String>, field: &str) -> AppResult<Vec<String>> {
    let mut out: Vec<String> = Vec::new();
    for value in values {
        let v = clean_text(&value, MAX_WORD, field, false)?;
        if v.contains(['\n', '\r', '\t']) {
            return Err(AppError::Invalid(format!("{field}: one line per entry")));
        }
        if !v.is_empty() && !out.iter().any(|o| o.to_lowercase() == v.to_lowercase()) {
            out.push(v);
        }
    }
    if out.len() > MAX_LIST {
        return Err(AppError::Invalid(format!("{field}: at most {MAX_LIST} entries")));
    }
    Ok(out)
}

fn unique<T: PartialEq>(values: Vec<T>) -> Vec<T> {
    let mut out = Vec::new();
    for v in values {
        if !out.contains(&v) {
            out.push(v);
        }
    }
    out
}

fn clean_salary(rule: SalaryRule) -> AppResult<SalaryRule> {
    for amount in [rule.min, rule.expected].into_iter().flatten() {
        if !(0..=MAX_SALARY).contains(&amount) {
            return Err(AppError::Invalid("salary is out of range".into()));
        }
    }
    Ok(rule)
}

/// Every field checked before it is stored - the JSON may come from a newer UI or a sync file.
pub fn clean_criteria(c: Criteria) -> AppResult<Criteria> {
    if c.cities.len() > MAX_CITIES {
        return Err(AppError::Invalid(format!("at most {MAX_CITIES} cities")));
    }
    let mut cities: Vec<CityRule> = Vec::new();
    for city in c.cities {
        let name = clean_text(&city.name, MAX_WORD, "city", false)?;
        if name.is_empty() || cities.iter().any(|c| c.name.to_lowercase() == name.to_lowercase()) {
            continue;
        }
        cities.push(CityRule { name, radius_km: city.radius_km.min(MAX_RADIUS_KM) });
    }
    if c.commute_per_month.is_some_and(|n| n > MAX_COMMUTE) {
        return Err(AppError::Invalid("commute is out of range".into()));
    }
    Ok(Criteria {
        keywords: clean_list(c.keywords, "keywords")?,
        work_modes: unique(c.work_modes.into_iter().filter(|m| *m != Remote::Unknown).collect()),
        cities,
        commute_per_month: c.commute_per_month,
        contracts: unique(c.contracts),
        levels: unique(c.levels),
        languages: unique(c.languages),
        salary_uop: clean_salary(c.salary_uop)?,
        salary_b2b: clean_salary(c.salary_b2b)?,
        skip_companies: clean_list(c.skip_companies, "companies")?,
        exclude_words: clean_list(c.exclude_words, "excluded words")?,
        hard: c.hard,
    })
}

// ---------- profiles ----------

fn criteria_from(json: &str) -> Criteria {
    // A broken value never blocks the app - the profile simply has no criteria.
    serde_json::from_str(json).unwrap_or_default()
}

pub fn list(conn: &Connection) -> AppResult<Vec<Profile>> {
    let mut stmt =
        conn.prepare("SELECT id, name, description, criteria, position FROM profiles ORDER BY position, id")?;
    let rows = stmt
        .query_map([], |r| {
            Ok(Profile {
                id: r.get(0)?,
                name: r.get(1)?,
                description: r.get(2)?,
                criteria: criteria_from(&r.get::<_, String>(3)?),
                position: r.get(4)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// Creates or updates a profile and re-checks every offer against it. Returns the id.
pub fn save(conn: &mut Connection, input: ProfileInput) -> AppResult<i64> {
    let name = clean_text(&input.name, MAX_NAME, "profile name", true)?;
    let description = clean_text(&input.description, MAX_DESCRIPTION, "description", true)?;
    let criteria = clean_criteria(input.criteria)?;
    let json = serde_json::to_string(&criteria).map_err(|e| AppError::Invalid(e.to_string()))?;
    let now = clock::now();
    let tx = conn.transaction()?;
    let id = match input.id {
        Some(id) => {
            let n = tx.execute(
                "UPDATE profiles SET name = ?2, description = ?3, criteria = ?4, updated_at = ?5 WHERE id = ?1",
                params![id, name, description, json, now],
            )?;
            if n == 0 {
                return Err(AppError::NotFound);
            }
            id
        }
        None => {
            let count: i64 = tx.query_row("SELECT count(*) FROM profiles", [], |r| r.get(0))?;
            if count >= MAX_PROFILES {
                return Err(AppError::Invalid(format!("at most {MAX_PROFILES} profiles")));
            }
            let uid = crate::secrets::random_hex(16)?;
            tx.execute(
                "INSERT INTO profiles (uid, name, position, description, criteria, created_at, updated_at)
                 VALUES (?1, ?2, (SELECT COALESCE(max(position), -1) + 1 FROM profiles), ?3, ?4, ?5, ?5)",
                params![uid, name, description, json, now],
            )?;
            tx.last_insert_rowid()
        }
    };
    recheck_all(&tx)?;
    tx.commit()?;
    Ok(id)
}

pub fn delete(conn: &mut Connection, id: i64) -> AppResult<()> {
    let tx = conn.transaction()?;
    if tx.execute("DELETE FROM profiles WHERE id = ?1", [id])? == 0 {
        return Err(AppError::NotFound);
    }
    recheck_all(&tx)?;
    tx.commit()?;
    Ok(())
}

// ---------- shared work experience ----------

pub fn experience(conn: &Connection) -> AppResult<String> {
    Ok(conn
        .query_row("SELECT experience FROM candidate WHERE id = 1", [], |r| r.get(0))
        .optional()?
        .unwrap_or_default())
}

pub fn set_experience(conn: &Connection, text: &str) -> AppResult<()> {
    let text = clean_text(text, MAX_EXPERIENCE, "experience", false)?;
    conn.execute(
        "INSERT INTO candidate (id, experience, updated_at) VALUES (1, ?1, ?2)
         ON CONFLICT (id) DO UPDATE SET experience = excluded.experience, updated_at = excluded.updated_at",
        params![text, clock::now()],
    )?;
    Ok(())
}

// ---------- filter results ----------

const FACT_COLUMNS: &str = "id, title, company, city, remote, salary_min, salary_max, salary_currency,
    salary_period, contract, level, language, requirements, description";

fn facts_from(r: &rusqlite::Row) -> rusqlite::Result<(i64, Facts)> {
    Ok((
        r.get(0)?,
        Facts {
            title: r.get(1)?,
            company: r.get(2)?,
            city: r.get(3)?,
            remote: r.get(4)?,
            salary_min: r.get(5)?,
            salary_max: r.get(6)?,
            salary_currency: r.get(7)?,
            salary_period: r.get(8)?,
            contract: r.get(9)?,
            level: r.get(10)?,
            language: r.get(11)?,
            requirements: r.get(12)?,
            description: r.get(13)?,
        },
    ))
}

fn store(tx: &Transaction, offer_id: i64, facts: &Facts, profiles: &[Profile]) -> AppResult<()> {
    tx.execute("DELETE FROM offer_filters WHERE offer_id = ?1", [offer_id])?;
    let mut results = Vec::with_capacity(profiles.len());
    for p in profiles {
        let e = filter::evaluate(facts, &p.criteria);
        let reasons = serde_json::to_string(&e.reasons).map_err(|e| AppError::Invalid(e.to_string()))?;
        tx.execute(
            "INSERT INTO offer_filters (offer_id, profile_id, result, reasons) VALUES (?1, ?2, ?3, ?4)",
            params![offer_id, p.id, e.result, reasons],
        )?;
        results.push(e.result);
    }
    tx.execute(
        "UPDATE offers SET filter_result = ?2, filter_reason = NULL WHERE id = ?1",
        params![offer_id, filter::best(results)],
    )?;
    Ok(())
}

/// Checks one offer (just added or merged) against every profile.
pub fn recheck_offer(tx: &Transaction, offer_id: i64) -> AppResult<()> {
    let profiles = list(tx)?;
    let facts = tx
        .query_row(&format!("SELECT {FACT_COLUMNS} FROM offers WHERE id = ?1"), [offer_id], facts_from)
        .optional()?
        .ok_or(AppError::NotFound)?;
    store(tx, offer_id, &facts.1, &profiles)
}

/// Checks every offer again - after a profile was created, changed or deleted.
fn recheck_all(tx: &Transaction) -> AppResult<()> {
    let profiles = list(tx)?;
    let all = {
        let mut stmt = tx.prepare(&format!("SELECT {FACT_COLUMNS} FROM offers"))?;
        stmt.query_map([], facts_from)?.collect::<Result<Vec<_>, _>>()?
    };
    for (id, facts) in all {
        store(tx, id, &facts, &profiles)?;
    }
    Ok(())
}

pub fn reasons_from(json: Option<String>) -> Vec<Reason> {
    json.and_then(|j| serde_json::from_str(&j).ok()).unwrap_or_default()
}

pub fn offer_filters(conn: &Connection, offer_id: i64) -> AppResult<Vec<ProfileFilter>> {
    let mut stmt = conn.prepare(
        "SELECT p.id, p.name, f.result, f.reasons FROM offer_filters f JOIN profiles p ON p.id = f.profile_id
         WHERE f.offer_id = ?1 ORDER BY p.position, p.id",
    )?;
    let rows = stmt
        .query_map([offer_id], |r| {
            Ok(ProfileFilter {
                profile_id: r.get(0)?,
                profile_name: r.get(1)?,
                result: r.get(2)?,
                reasons: reasons_from(r.get(3)?),
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::tests::test_db;
    use crate::offers;

    fn plan(name: &str, criteria: Criteria) -> ProfileInput {
        ProfileInput {
            id: None,
            name: name.into(),
            description: "Szukam pracy jako frontend developer, najlepiej zdalnie.".into(),
            criteria,
        }
    }

    fn offer(title: &str, city: &str, remote: Remote) -> OfferInput {
        OfferInput {
            title: title.into(),
            company: "Firma Testowa".into(),
            source: "manual".into(),
            city: Some(city.into()),
            remote: Some(remote),
            ..Default::default()
        }
    }

    fn remote_only() -> Criteria {
        Criteria { work_modes: vec![Remote::Yes], ..Default::default() }
    }

    fn gdansk() -> Criteria {
        Criteria {
            work_modes: vec![Remote::Hybrid, Remote::No],
            cities: vec![CityRule { name: "Gdańsk".into(), radius_km: 20 }],
            ..Default::default()
        }
    }

    fn ids(rows: &[OfferRow]) -> Vec<String> {
        let mut t: Vec<String> = rows.iter().map(|r| r.title.clone()).collect();
        t.sort();
        t
    }

    #[test]
    fn profile_round_trip_and_order() {
        let mut conn = test_db();
        let a = save(&mut conn, plan("Plan A", remote_only())).unwrap();
        let b = save(&mut conn, plan("Plan B", gdansk())).unwrap();
        let list = list(&conn).unwrap();
        assert_eq!(list.iter().map(|p| p.id).collect::<Vec<_>>(), vec![a, b]);
        assert_eq!(list[1].criteria.cities[0].name, "Gdańsk");

        let mut changed = plan("  Plan A - zdalnie ", remote_only());
        changed.id = Some(a);
        save(&mut conn, changed).unwrap();
        assert_eq!(super::list(&conn).unwrap()[0].name, "Plan A - zdalnie");

        delete(&mut conn, b).unwrap();
        assert_eq!(super::list(&conn).unwrap().len(), 1);
        assert!(matches!(delete(&mut conn, b), Err(AppError::NotFound)));
    }

    #[test]
    fn rejects_bad_profiles() {
        let mut conn = test_db();
        let mut no_desc = plan("Plan A", Criteria::default());
        no_desc.description = "   ".into();
        assert!(save(&mut conn, no_desc).is_err());
        assert!(save(&mut conn, plan("", Criteria::default())).is_err());
        assert!(save(&mut conn, plan(&"x".repeat(MAX_NAME + 1), Criteria::default())).is_err());

        let mut salary = Criteria::default();
        salary.salary_uop.min = Some(-1);
        assert!(save(&mut conn, plan("Plan A", salary)).is_err());

        let too_many = Criteria { keywords: (0..=MAX_LIST).map(|i| format!("słowo {i}")).collect(), ..Default::default() };
        assert!(save(&mut conn, plan("Plan A", too_many)).is_err());

        let bad_commute = Criteria { commute_per_month: Some(40), ..Default::default() };
        assert!(save(&mut conn, plan("Plan A", bad_commute)).is_err());

        let mut missing = plan("Plan A", Criteria::default());
        missing.id = Some(999);
        assert!(matches!(save(&mut conn, missing), Err(AppError::NotFound)));
    }

    #[test]
    fn lists_are_cleaned() {
        let c = clean_criteria(Criteria {
            keywords: vec![" React ".into(), "react".into(), "".into(), "TypeScript".into()],
            work_modes: vec![Remote::Yes, Remote::Yes, Remote::Unknown],
            cities: vec![
                CityRule { name: "Gdańsk".into(), radius_km: 900 },
                CityRule { name: "gdańsk".into(), radius_km: 5 },
                CityRule { name: " ".into(), radius_km: 5 },
            ],
            ..Default::default()
        })
        .unwrap();
        assert_eq!(c.keywords, vec!["React", "TypeScript"]);
        assert_eq!(c.work_modes, vec![Remote::Yes]);
        assert_eq!(c.cities, vec![CityRule { name: "Gdańsk".into(), radius_km: MAX_RADIUS_KM }]);
    }

    #[test]
    fn offers_are_rechecked_when_profiles_change() {
        let mut conn = test_db();
        offers::add(&mut conn, offer("Zdalna", "Kraków", Remote::Yes)).unwrap();
        offers::add(&mut conn, offer("Biuro Gdynia", "Gdynia", Remote::No)).unwrap();
        offers::add(&mut conn, offer("Biuro Kraków", "Kraków", Remote::No)).unwrap();
        offers::add(&mut conn, offer("Bez informacji", "Wrocław", Remote::Unknown)).unwrap();
        // No profile yet: nothing is filtered out.
        assert_eq!(offers::list(&conn, View::Board, None).unwrap().len(), 4);

        let a = save(&mut conn, plan("Plan A", remote_only())).unwrap();
        assert_eq!(ids(&offers::list(&conn, View::Board, None).unwrap()), vec!["Zdalna"]);
        assert_eq!(ids(&offers::list(&conn, View::Review, None).unwrap()), vec!["Bez informacji"]);
        assert_eq!(ids(&offers::list(&conn, View::Rejected, None).unwrap()), vec!["Biuro Gdynia", "Biuro Kraków"]);

        // A second profile: one fitting profile is enough for the "all profiles" board.
        let b = save(&mut conn, plan("Plan B", gdansk())).unwrap();
        assert_eq!(ids(&offers::list(&conn, View::Board, None).unwrap()), vec!["Biuro Gdynia", "Zdalna"]);
        assert_eq!(ids(&offers::list(&conn, View::Board, Some(a)).unwrap()), vec!["Zdalna"]);
        // Plan B wants an office near Gdańsk: a remote offer may still allow the office - unsure.
        assert_eq!(ids(&offers::list(&conn, View::Board, Some(b)).unwrap()), vec!["Biuro Gdynia"]);
        assert_eq!(ids(&offers::list(&conn, View::Review, Some(b)).unwrap()), vec!["Zdalna"]);
        assert_eq!(ids(&offers::list(&conn, View::Rejected, Some(b)).unwrap()), vec!["Bez informacji", "Biuro Kraków"]);
        let counts = offers::counts(&conn, None).unwrap();
        assert_eq!(counts, ViewCounts { board: 2, review: 1, rejected: 1, trash: 0 });

        // Reasons travel with the rows and the card.
        let rejected = offers::list(&conn, View::Rejected, Some(a)).unwrap();
        assert_eq!(rejected[0].filter_reasons[0].code, "office_not_wanted");
        let detail = offers::detail(&conn, rejected[0].id).unwrap();
        assert_eq!(detail.filters.len(), 2);
        assert_eq!(detail.filters[0].profile_name, "Plan A");

        delete(&mut conn, b).unwrap();
        assert_eq!(ids(&offers::list(&conn, View::Board, None).unwrap()), vec!["Zdalna"]);
        delete(&mut conn, a).unwrap();
        assert_eq!(offers::list(&conn, View::Board, None).unwrap().len(), 4);
    }

    #[test]
    fn new_offers_are_checked_on_arrival() {
        let mut conn = test_db();
        save(&mut conn, plan("Plan A", remote_only())).unwrap();
        offers::add(&mut conn, offer("Biuro", "Kraków", Remote::No)).unwrap();
        assert_eq!(offers::list(&conn, View::Rejected, None).unwrap().len(), 1);
        // The same offer from another source says "remote" - merged and checked again.
        let mut again = offer("Biuro", "Kraków", Remote::Yes);
        again.source = "adzuna".into();
        again.description = Some("Możliwa praca w pełni zdalna.".into());
        offers::add(&mut conn, again).unwrap();
        assert_eq!(offers::list(&conn, View::Board, None).unwrap().len(), 1);
    }

    #[test]
    fn offers_the_user_acted_on_stay_on_the_board() {
        let mut conn = test_db();
        save(&mut conn, plan("Plan A", remote_only())).unwrap();
        let id = match offers::add(&mut conn, offer("Biuro", "Kraków", Remote::No)).unwrap() {
            AddOutcome::Added { id } => id,
            other => panic!("{other:?}"),
        };
        assert_eq!(offers::list(&conn, View::Rejected, None).unwrap().len(), 1);
        offers::set_interest(&conn, id, Some(Interest::High)).unwrap();
        assert_eq!(offers::list(&conn, View::Rejected, None).unwrap().len(), 0);
        assert_eq!(offers::list(&conn, View::Board, None).unwrap().len(), 1);
        offers::set_interest(&conn, id, Some(Interest::No)).unwrap();
        assert_eq!(offers::list(&conn, View::Rejected, None).unwrap().len(), 1);
        offers::set_stage(&mut conn, id, Stage::Applied).unwrap();
        assert_eq!(offers::list(&conn, View::Board, None).unwrap().len(), 1);
    }

    #[test]
    fn experience_is_shared_and_checked() {
        let conn = test_db();
        assert_eq!(experience(&conn).unwrap(), "");
        set_experience(&conn, "  5 lat w księgowości  ").unwrap();
        assert_eq!(experience(&conn).unwrap(), "5 lat w księgowości");
        assert!(set_experience(&conn, &"x".repeat(MAX_EXPERIENCE + 1)).is_err());
    }
}
