//! The database as a sync snapshot and back. What travels: offers (without the ones rejected
//! by the filter - the phone does not need them), their sources, stage history and AI scores,
//! profiles, work experience, dates, contacts, contact history, deleted offers' fingerprints
//! and tombstones. Settings and secrets never travel - every device has its own.

use rusqlite::{Connection, OptionalExtension, Transaction, params, types::Value as Sql};
use serde_json::{Map, Value, json};

use super::merge::{FORMAT, Row, Snapshot, Tombstone};
use crate::error::{AppError, AppResult};

/// Offer columns that travel (besides `uid`).
const OFFER_COLS: &[&str] = &[
    "fingerprint", "title", "company", "url", "city", "remote", "salary_min", "salary_max", "salary_currency",
    "salary_period", "contract", "level", "language", "requirements", "description", "notes", "interest", "favorite",
    "stage", "stage_changed_at", "expires_at", "expired", "added_at", "updated_at", "deleted_at", "ai_status", "ai_at",
    "ai_summary", "ai_flags", "ai_remote_quote", "ai_tr_lang", "ai_title_tr", "ai_description_tr", "content_removed",
];
const PROFILE_COLS: &[&str] = &["name", "position", "description", "criteria", "description_prev", "created_at", "updated_at"];
const EVENT_COLS: &[&str] = &["kind", "at", "minutes", "note", "created_at"];
const CONTACT_COLS: &[&str] = &["name", "role", "email", "phone", "linkedin", "created_at", "updated_at"];
const LOG_COLS: &[&str] = &["at", "kind", "note"];

/// Offers the phone does not need: rejected by the filter and not acted on.
const NOT_SYNCED: &str = "(o.filter_result = 'fail' AND o.stage = 'new' AND o.favorite = 0
    AND COALESCE(o.interest, '') NOT IN ('high', 'maybe'))";

fn to_json(v: Sql) -> Value {
    match v {
        Sql::Null => Value::Null,
        Sql::Integer(i) => json!(i),
        Sql::Real(f) => json!(f),
        Sql::Text(t) => Value::String(t),
        Sql::Blob(_) => Value::Null,
    }
}

fn to_sql(v: Option<&Value>) -> Sql {
    match v {
        None | Some(Value::Null) => Sql::Null,
        Some(Value::Bool(b)) => Sql::Integer(*b as i64),
        Some(Value::Number(n)) => n.as_i64().map(Sql::Integer).unwrap_or_else(|| Sql::Real(n.as_f64().unwrap_or(0.0))),
        Some(Value::String(s)) => Sql::Text(s.clone()),
        Some(other) => Sql::Text(other.to_string()),
    }
}

fn rows(conn: &Connection, sql: &str, cols: &[&str], at_col: &str) -> AppResult<Vec<Row>> {
    let mut stmt = conn.prepare(sql)?;
    let mut out = Vec::new();
    let mut q = stmt.query([])?;
    while let Some(r) = q.next()? {
        let uid: String = r.get("uid")?;
        let mut data = Map::new();
        for c in cols {
            data.insert(c.to_string(), to_json(r.get(*c)?));
        }
        let at = data.get(at_col).and_then(Value::as_str).unwrap_or_default().to_string();
        out.push(Row { uid, at, data: Value::Object(data) });
    }
    Ok(out)
}

/// Builds the snapshot of this device.
pub fn export(conn: &Connection, device: &str, now: &str) -> AppResult<Snapshot> {
    let mut s = Snapshot { format: FORMAT, device: device.into(), exported_at: now.into(), ..Default::default() };
    let cols = |list: &[&str]| list.iter().map(|c| format!("o.{c}")).collect::<Vec<_>>().join(", ");

    let mut offers = rows(conn, &format!("SELECT o.uid, {} FROM offers o WHERE NOT {NOT_SYNCED}", cols(OFFER_COLS)), OFFER_COLS, "updated_at")?;
    for o in &mut offers {
        let id: i64 = conn.query_row("SELECT id FROM offers WHERE uid = ?1", [&o.uid], |r| r.get(0))?;
        let data = o.data.as_object_mut().expect("object");
        let mut stmt = conn.prepare("SELECT source, url, first_seen FROM offer_sources WHERE offer_id = ?1 ORDER BY source")?;
        let sources: Vec<Value> = stmt
            .query_map([id], |r| Ok(json!({ "source": r.get::<_, String>(0)?, "url": r.get::<_, Option<String>>(1)?, "first_seen": r.get::<_, String>(2)? })))?
            .collect::<Result<_, _>>()?;
        let mut stmt = conn.prepare("SELECT stage, at FROM stage_history WHERE offer_id = ?1 ORDER BY at, id")?;
        let history: Vec<Value> = stmt
            .query_map([id], |r| Ok(json!({ "stage": r.get::<_, String>(0)?, "at": r.get::<_, String>(1)? })))?
            .collect::<Result<_, _>>()?;
        let mut stmt = conn.prepare(
            "SELECT p.uid, s.score, s.why, s.quote FROM offer_scores s JOIN profiles p ON p.id = s.profile_id
             WHERE s.offer_id = ?1 AND s.source = 'ai' ORDER BY p.uid",
        )?;
        let scores: Vec<Value> = stmt
            .query_map([id], |r| {
                Ok(json!({ "profile": r.get::<_, String>(0)?, "score": r.get::<_, i64>(1)?, "why": r.get::<_, Option<String>>(2)?, "quote": r.get::<_, Option<String>>(3)? }))
            })?
            .collect::<Result<_, _>>()?;
        data.insert("sources".into(), Value::Array(sources));
        data.insert("history".into(), Value::Array(history));
        data.insert("ai_scores".into(), Value::Array(scores));
    }
    s.tables.insert("offers".into(), offers);
    s.tables.insert("profiles".into(), rows(conn, &format!("SELECT uid, {} FROM profiles", PROFILE_COLS.join(", ")), PROFILE_COLS, "updated_at")?);

    let child = |table: &str, list: &[&str], at: &str| -> AppResult<Vec<Row>> {
        let mut out = rows(
            conn,
            &format!(
                "SELECT t.uid, o.uid AS offer_uid, {} FROM {table} t JOIN offers o ON o.id = t.offer_id WHERE NOT {NOT_SYNCED}",
                list.iter().map(|c| format!("t.{c}")).collect::<Vec<_>>().join(", ")
            ),
            list,
            at,
        )?;
        // The offer is named by its uid - ids differ between devices.
        let mut stmt = conn.prepare(&format!("SELECT o.uid FROM {table} t JOIN offers o ON o.id = t.offer_id WHERE t.uid = ?1"))?;
        for r in &mut out {
            let offer_uid: String = stmt.query_row([&r.uid], |x| x.get(0))?;
            r.data.as_object_mut().expect("object").insert("offer_uid".into(), Value::String(offer_uid));
        }
        Ok(out)
    };
    s.tables.insert("events".into(), child("events", EVENT_COLS, "created_at")?);
    s.tables.insert("contacts".into(), child("contacts", CONTACT_COLS, "updated_at")?);
    s.tables.insert("log".into(), child("contact_log", LOG_COLS, "at")?);

    // Work experience: one row with a fixed id.
    if let Some((text, at)) = conn
        .query_row("SELECT experience, updated_at FROM candidate WHERE id = 1", [], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
        .optional()?
    {
        s.tables.insert("experience".into(), vec![Row { uid: "experience".into(), at, data: json!({ "experience": text }) }]);
    }
    // Fingerprints of offers deleted for good - rows that never change.
    let mut stmt = conn.prepare("SELECT fingerprint, removed_at FROM removed_offers")?;
    let removed = stmt
        .query_map([], |r| Ok(Row { uid: r.get(0)?, at: r.get(1)?, data: json!({}) }))?
        .collect::<Result<Vec<_>, _>>()?;
    s.tables.insert("removed".into(), removed);

    let mut stmt = conn.prepare("SELECT kind, uid, at FROM sync_deleted")?;
    for t in stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, Tombstone { uid: r.get(1)?, at: r.get(2)? })))? {
        let (kind, tomb) = t?;
        s.deleted.entry(kind).or_default().push(tomb);
    }
    s.tables.retain(|_, v| !v.is_empty());
    Ok(s)
}

/// Remembers a deleted row, so sync tells the other device.
pub fn tombstone(conn: &Connection, kind: &str, uid: &str, at: &str) -> AppResult<()> {
    conn.execute(
        "INSERT INTO sync_deleted (kind, uid, at) VALUES (?1, ?2, ?3)
         ON CONFLICT (kind, uid) DO UPDATE SET at = max(at, excluded.at)",
        params![kind, uid, at],
    )?;
    Ok(())
}

/// How many local offers the merged snapshot would delete (the UI asks first above a few).
pub fn offers_to_remove(conn: &Connection, merged: &Snapshot) -> AppResult<usize> {
    let mut n = 0;
    for t in merged.deleted.get("offers").into_iter().flatten() {
        let found: Option<i64> = conn.query_row("SELECT id FROM offers WHERE uid = ?1", [&t.uid], |r| r.get(0)).optional()?;
        n += found.is_some() as usize;
    }
    Ok(n)
}

fn upsert(tx: &Transaction, table: &str, cols: &[&str], uid: &str, data: &Value, extra: &[(&str, Sql)]) -> AppResult<()> {
    let mut names: Vec<String> = cols.iter().map(|c| c.to_string()).collect();
    let mut values: Vec<Sql> = cols.iter().map(|c| to_sql(data.get(*c))).collect();
    for (name, value) in extra {
        names.push(name.to_string());
        values.push(value.clone());
    }
    let exists: bool = tx.query_row(&format!("SELECT EXISTS (SELECT 1 FROM {table} WHERE uid = ?1)"), [uid], |r| r.get(0))?;
    if exists {
        let set = names.iter().enumerate().map(|(i, n)| format!("{n} = ?{}", i + 2)).collect::<Vec<_>>().join(", ");
        let mut all = vec![Sql::Text(uid.to_string())];
        all.extend(values);
        tx.execute(&format!("UPDATE {table} SET {set} WHERE uid = ?1"), rusqlite::params_from_iter(all))?;
    } else {
        let marks = (1..=names.len() + 1).map(|i| format!("?{i}")).collect::<Vec<_>>().join(", ");
        let mut all = vec![Sql::Text(uid.to_string())];
        all.extend(values);
        tx.execute(&format!("INSERT INTO {table} (uid, {}) VALUES ({marks})", names.join(", ")), rusqlite::params_from_iter(all))?;
    }
    Ok(())
}

fn local_at(tx: &Transaction, table: &str, at_col: &str, uid: &str) -> AppResult<Option<String>> {
    Ok(tx.query_row(&format!("SELECT {at_col} FROM {table} WHERE uid = ?1"), [uid], |r| r.get(0)).optional()?.flatten())
}

fn offer_id(tx: &Transaction, uid: &str) -> AppResult<Option<i64>> {
    Ok(tx.query_row("SELECT id FROM offers WHERE uid = ?1", [uid], |r| r.get(0)).optional()?)
}

/// Writes the merged snapshot into this device's database: newer rows replace older ones,
/// tombstones delete. Rows missing from the snapshot are never deleted (the phone keeps
/// only a part of the offers). Ends with a full filter re-check.
pub fn apply(conn: &mut Connection, merged: &Snapshot, now: &str) -> AppResult<()> {
    if merged.format > FORMAT {
        return Err(AppError::Invalid("the sync file comes from a newer app version - update the app".into()));
    }
    let tx = conn.transaction()?;
    let empty = Vec::new();
    let table = |name: &str| merged.tables.get(name).unwrap_or(&empty);

    // Profiles first (scores point to them), then offers, then what belongs to offers.
    for r in table("profiles") {
        if local_at(&tx, "profiles", "updated_at", &r.uid)?.is_some_and(|mine| mine >= r.at) {
            continue;
        }
        upsert(&tx, "profiles", PROFILE_COLS, &r.uid, &r.data, &[])?;
    }

    for t in table("removed") {
        tx.execute("INSERT OR IGNORE INTO removed_offers (fingerprint, removed_at) VALUES (?1, ?2)", params![t.uid, t.at])?;
    }

    for r in table("offers") {
        // The same field rules as data from outside; a bad row is skipped, the local copy stays.
        if crate::offers::check_synced(&r.data).is_err() {
            continue;
        }
        let fingerprint = r.data.get("fingerprint").and_then(Value::as_str).unwrap_or_default();
        let blocked: bool = tx.query_row("SELECT EXISTS (SELECT 1 FROM removed_offers WHERE fingerprint = ?1)", [fingerprint], |x| x.get(0))?;
        if blocked {
            continue;
        }
        // The same offer added on both devices (same company and title, different ids):
        // both keep the smaller id, the other one becomes a tombstone - devices agree.
        let twin: Option<String> = tx
            .query_row("SELECT uid FROM offers WHERE fingerprint = ?1 AND uid != ?2", params![fingerprint, r.uid], |x| x.get(0))
            .optional()?;
        if let Some(twin) = twin {
            if twin < r.uid {
                tombstone(&tx, "offers", &r.uid, now)?;
                continue;
            }
            tx.execute("UPDATE offers SET uid = ?2 WHERE uid = ?1", params![twin, r.uid])?;
            tombstone(&tx, "offers", &twin, now)?;
        }
        if local_at(&tx, "offers", "updated_at", &r.uid)?.is_some_and(|mine| mine >= r.at) {
            continue;
        }
        upsert(&tx, "offers", OFFER_COLS, &r.uid, &r.data, &[])?;
        let id = offer_id(&tx, &r.uid)?.ok_or(AppError::NotFound)?;
        for s in r.data.get("sources").and_then(Value::as_array).into_iter().flatten() {
            tx.execute(
                "INSERT OR IGNORE INTO offer_sources (offer_id, source, url, first_seen) VALUES (?1, ?2, ?3, ?4)",
                params![id, to_sql(s.get("source")), to_sql(s.get("url")), to_sql(s.get("first_seen"))],
            )?;
        }
        // A source whose job closed (its terms): its link goes on this device too.
        tx.execute(
            "UPDATE offer_sources SET url = NULL WHERE offer_id = ?1
             AND source = (SELECT content_removed FROM offers WHERE id = ?1)",
            [id],
        )?;
        let history: Vec<&Value> = r.data.get("history").and_then(Value::as_array).into_iter().flatten().collect();
        if !history.is_empty() {
            tx.execute("DELETE FROM stage_history WHERE offer_id = ?1", [id])?;
            for h in history {
                tx.execute("INSERT INTO stage_history (offer_id, stage, at) VALUES (?1, ?2, ?3)", params![id, to_sql(h.get("stage")), to_sql(h.get("at"))])?;
            }
        }
        tx.execute("DELETE FROM offer_scores WHERE offer_id = ?1 AND source = 'ai'", [id])?;
        for sc in r.data.get("ai_scores").and_then(Value::as_array).into_iter().flatten() {
            tx.execute(
                "INSERT OR REPLACE INTO offer_scores (offer_id, profile_id, score, source, why, quote)
                 SELECT ?1, p.id, ?3, 'ai', ?4, ?5 FROM profiles p WHERE p.uid = ?2",
                params![id, to_sql(sc.get("profile")), to_sql(sc.get("score")), to_sql(sc.get("why")), to_sql(sc.get("quote"))],
            )?;
        }
    }

    for (name, db_table, cols, at_col) in [
        ("events", "events", EVENT_COLS, "created_at"),
        ("contacts", "contacts", CONTACT_COLS, "updated_at"),
        ("log", "contact_log", LOG_COLS, "at"),
    ] {
        for r in table(name) {
            let Some(offer) = r.data.get("offer_uid").and_then(Value::as_str) else { continue };
            let Some(id) = offer_id(&tx, offer)? else { continue };
            if local_at(&tx, db_table, at_col, &r.uid)?.is_some_and(|mine| mine >= r.at) {
                continue;
            }
            upsert(&tx, db_table, cols, &r.uid, &r.data, &[("offer_id", Sql::Integer(id))])?;
        }
    }

    if let Some(r) = table("experience").first() {
        let mine: Option<String> = tx.query_row("SELECT updated_at FROM candidate WHERE id = 1", [], |x| x.get(0)).optional()?;
        if mine.is_none_or(|m| m < r.at) {
            tx.execute(
                "INSERT INTO candidate (id, experience, updated_at) VALUES (1, ?1, ?2)
                 ON CONFLICT (id) DO UPDATE SET experience = excluded.experience, updated_at = excluded.updated_at",
                params![to_sql(r.data.get("experience")), r.at],
            )?;
        }
    }

    // Tombstones: delete here and remember (so this device passes them on).
    for (kind, list) in &merged.deleted {
        let db_table = match kind.as_str() {
            "offers" => "offers",
            "profiles" => "profiles",
            "events" => "events",
            "contacts" => "contacts",
            "log" => "contact_log",
            _ => continue,
        };
        for t in list {
            if kind == "offers" {
                // A purged offer is also remembered by its fingerprint - alerts never bring it back.
                tx.execute(
                    "INSERT OR IGNORE INTO removed_offers (fingerprint, removed_at) SELECT fingerprint, ?2 FROM offers WHERE uid = ?1",
                    params![t.uid, t.at],
                )?;
            }
            tx.execute(&format!("DELETE FROM {db_table} WHERE uid = ?1"), [&t.uid])?;
            tombstone(&tx, kind, &t.uid, &t.at)?;
        }
    }
    crate::profiles::recheck_all(&tx)?;
    tx.commit()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::tests::test_db;
    use crate::model::{AddOutcome, Criteria, OfferInput, ProfileInput, Stage, View};
    use crate::sync::merge::merge;

    const NOW: &str = "2099-01-01T00:00:00Z";

    fn add(conn: &mut Connection, title: &str) -> i64 {
        match crate::offers::add(conn, OfferInput { title: title.into(), company: "Firma Testowa".into(), source: "manual".into(), ..Default::default() }).unwrap() {
            AddOutcome::Added { id } => id,
            other => panic!("{other:?}"),
        }
    }

    fn titles(conn: &Connection) -> Vec<String> {
        let mut t: Vec<String> = crate::offers::list(conn, View::Board, None, false).unwrap().into_iter().map(|r| r.title).collect();
        t.sort();
        t
    }

    /// One sync round between two devices: both end with the merged data.
    fn round(a: &mut Connection, b: &mut Connection) {
        let (sa, sb) = (export(a, "pc", NOW).unwrap(), export(b, "phone", NOW).unwrap());
        let (merged, _) = merge(&sa, &sb, NOW);
        apply(a, &merged, NOW).unwrap();
        apply(b, &merged, NOW).unwrap();
    }

    #[test]
    fn offers_profiles_and_changes_travel_both_ways() {
        let (mut pc, mut phone) = (test_db(), test_db());
        crate::profiles::save(&mut pc, ProfileInput { id: None, name: "Plan A".into(), description: "Frontend".into(), criteria: Criteria::default() }).unwrap();
        let id = add(&mut pc, "Frontend Developer");
        crate::profiles::set_experience(&pc, "4 lata React").unwrap();
        round(&mut pc, &mut phone);
        assert_eq!(titles(&phone), vec!["Frontend Developer"]);
        assert_eq!(crate::profiles::list(&phone).unwrap()[0].name, "Plan A");
        assert_eq!(crate::profiles::experience(&phone).unwrap(), "4 lata React");

        // The phone moves the offer to "applied"; the PC gets it with the stage history.
        let phone_id: i64 = phone.query_row("SELECT id FROM offers", [], |r| r.get(0)).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(1100));
        crate::offers::set_stage(&mut phone, phone_id, Stage::Applied).unwrap();
        round(&mut pc, &mut phone);
        let d = crate::offers::detail(&pc, id).unwrap();
        assert_eq!(d.row.stage, Stage::Applied);
        assert_eq!(d.history.len(), 2);
    }

    #[test]
    fn purged_offer_does_not_come_back() {
        let (mut pc, mut phone) = (test_db(), test_db());
        let id = add(&mut pc, "QA Engineer");
        round(&mut pc, &mut phone);
        crate::offers::trash(&pc, id).unwrap();
        crate::offers::purge(&mut pc, Some(id)).unwrap();
        round(&mut pc, &mut phone);
        assert!(titles(&phone).is_empty());
        // The phone also remembers the fingerprint - alerts will not bring it back there either.
        assert_eq!(add_outcome(&mut phone, "QA Engineer"), AddOutcome::Blocked);
        round(&mut pc, &mut phone);
        assert!(titles(&pc).is_empty());
    }

    fn add_outcome(conn: &mut Connection, title: &str) -> AddOutcome {
        crate::offers::add(conn, OfferInput { title: title.into(), company: "Firma Testowa".into(), source: "jjit".into(), ..Default::default() }).unwrap()
    }

    #[test]
    fn same_offer_added_on_both_devices_becomes_one() {
        let (mut pc, mut phone) = (test_db(), test_db());
        add(&mut pc, "Data Analyst");
        add(&mut phone, "Data Analyst");
        round(&mut pc, &mut phone);
        round(&mut pc, &mut phone);
        let uid = |c: &Connection| -> Vec<String> { c.prepare("SELECT uid FROM offers").unwrap().query_map([], |r| r.get(0)).unwrap().map(Result::unwrap).collect() };
        assert_eq!(uid(&pc).len(), 1);
        assert_eq!(uid(&pc), uid(&phone));
    }

    #[test]
    fn bad_offer_rows_from_the_sync_file_are_skipped() {
        let (mut pc, mut phone) = (test_db(), test_db());
        add(&mut phone, "Frontend Developer");
        let good = export(&phone, "phone", NOW).unwrap().tables["offers"][0].clone();
        let bad = |uid: &str, change: &dyn Fn(&mut Map<String, Value>)| {
            let mut row = good.clone();
            row.uid = uid.into();
            let data = row.data.as_object_mut().unwrap();
            data.insert("fingerprint".into(), Value::String(format!("fp-{uid}")));
            change(data);
            row
        };
        let mut snapshot = export(&phone, "phone", NOW).unwrap();
        let offers = snapshot.tables.get_mut("offers").unwrap();
        offers.push(bad("u-js", &|d| {
            d.insert("url".into(), json!("javascript:alert(1)"));
        }));
        offers.push(bad("u-long", &|d| {
            d.insert("title".into(), json!("x".repeat(1_000_000)));
        }));
        offers.push(bad("u-stage", &|d| {
            d.insert("stage".into(), json!("xyz"));
        }));
        offers.push(bad("u-source", &|d| {
            d.insert("sources".into(), json!([{ "source": "mail", "url": "file:///C:/x", "first_seen": NOW }]));
        }));
        offers.push(bad("u-type", &|d| {
            d.insert("salary_min".into(), json!("dużo"));
        }));
        apply(&mut pc, &snapshot, NOW).unwrap();
        assert_eq!(titles(&pc), vec!["Frontend Developer"]);
        let count: i64 = pc.query_row("SELECT count(*) FROM offers", [], |r| r.get(0)).unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn dates_and_contacts_follow_their_offer() {
        let (mut pc, mut phone) = (test_db(), test_db());
        let id = add(&mut pc, "Tester");
        crate::agenda::add_event(&pc, id, crate::agenda::EventInput { kind: "interview".into(), at: "2026-11-05T13:00:00Z".into(), minutes: 60, note: "Teams".into() }).unwrap();
        crate::agenda::save_contact(&pc, id, crate::agenda::Contact { name: "Anna Zmyślona".into(), ..Default::default() }).unwrap();
        round(&mut pc, &mut phone);
        let phone_id: i64 = phone.query_row("SELECT id FROM offers", [], |r| r.get(0)).unwrap();
        let a = crate::agenda::agenda(&phone, phone_id).unwrap();
        assert_eq!(a.events.len(), 1);
        assert_eq!(a.contacts[0].name, "Anna Zmyślona");
        // Deleted on the phone - gone on the PC after the next round.
        crate::agenda::delete_event(&phone, a.events[0].id).unwrap();
        round(&mut pc, &mut phone);
        assert!(crate::agenda::agenda(&pc, id).unwrap().events.is_empty());
    }
}
