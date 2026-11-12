//! The database as a sync snapshot and back. What travels: offers (without the ones rejected
//! by the filter - the phone does not need them), their sources, stage history and AI scores,
//! profiles, work experience, dates, contacts, contact history, deleted offers' fingerprints
//! and tombstones. Secrets never travel, and settings only those in `settings::SYNCED` -
//! otherwise every device has its own.

use rusqlite::{Connection, OptionalExtension, Transaction, params, types::Value as Sql};
use serde_json::{Map, Value, json};

use super::merge::{FORMAT, OFFER_USER_FIELDS, Row, Snapshot, Tombstone};
use crate::error::{AppError, AppResult};

/// Offer columns that travel (besides `uid`).
const OFFER_COLS: &[&str] = &[
    "fingerprint", "title", "company", "url", "city", "remote", "salary_min", "salary_max", "salary_currency",
    "salary_period", "contract", "level", "language", "requirements", "description", "notes", "interest", "favorite",
    "stage", "stage_changed_at", "expires_at", "expired", "added_at", "updated_at", "deleted_at", "ai_status", "ai_at",
    "ai_summary", "ai_flags", "ai_remote_quote", "ai_tr_lang", "ai_title_tr", "ai_description_tr", "content_removed",
    "published_at", "archived_at", "kept_at", "user_changed_at", "expired_by_user", "ai_precise",
    "own", "renewed_at", "renew_asked", "dup_why",
];
const PROFILE_COLS: &[&str] = &["name", "position", "description", "criteria", "description_prev", "created_at", "updated_at"];
const EVENT_COLS: &[&str] = &["kind", "at", "minutes", "note", "created_at"];
const CONTACT_COLS: &[&str] = &["name", "role", "email", "phone", "linkedin", "created_at", "updated_at"];
const LOG_COLS: &[&str] = &["at", "kind", "note"];

/// Offers the phone does not need: rejected by the filter and not acted on - an offer the person
/// added by hand, or gave a note, a date or a contact, travels (it would live on one device only).
fn not_synced() -> String {
    format!("(o.filter_result = 'fail' AND NOT {})", crate::offers::TOUCHED)
}

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
    let schema: i64 = conn.pragma_query_value(None, "user_version", |r| r.get(0))?;
    let mut s = Snapshot { format: FORMAT, schema, device: device.into(), exported_at: now.into(), ..Default::default() };
    let cols = |list: &[&str]| list.iter().map(|c| format!("o.{c}")).collect::<Vec<_>>().join(", ");

    let not_synced = not_synced();
    let mut offers = rows(conn, &format!("SELECT o.uid, {} FROM offers o WHERE NOT {not_synced}", cols(OFFER_COLS)), OFFER_COLS, "updated_at")?;
    for o in &mut offers {
        let id: i64 = conn.query_row("SELECT id FROM offers WHERE uid = ?1", [&o.uid], |r| r.get(0))?;
        // The row's time is the later of its two times (the merge compares each group by its own).
        if let Some(user) = o.data.get("user_changed_at").and_then(Value::as_str).filter(|u| *u > o.at.as_str()) {
            o.at = user.to_string();
        }
        let data = o.data.as_object_mut().expect("object");
        let mut stmt = conn.prepare("SELECT source, url, first_seen, city FROM offer_sources WHERE offer_id = ?1 ORDER BY source, url, first_seen")?;
        let sources: Vec<Value> = stmt
            .query_map([id], |r| {
                Ok(json!({ "source": r.get::<_, String>(0)?, "url": r.get::<_, Option<String>>(1)?, "first_seen": r.get::<_, String>(2)?, "city": r.get::<_, Option<String>>(3)? }))
            })?
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
        // The parent in the tree and the offer asked about ("The same?") by their uids - ids
        // differ between devices.
        let (parent, dup_of): (Option<String>, Option<String>) = conn.query_row(
            "SELECT p.uid, d.uid FROM offers o LEFT JOIN offers p ON p.id = o.parent_id LEFT JOIN offers d ON d.id = o.dup_of WHERE o.id = ?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        data.insert("parent_uid".into(), parent.map_or(Value::Null, Value::String));
        data.insert("dup_of_uid".into(), dup_of.map_or(Value::Null, Value::String));
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
                "SELECT t.uid, o.uid AS offer_uid, {} FROM {table} t JOIN offers o ON o.id = t.offer_id WHERE NOT {not_synced}",
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
    let mut shared = Vec::new();
    for key in crate::settings::SYNCED {
        if let Some(at) = crate::settings::changed_at(conn, key)? {
            shared.push(Row { uid: key.to_string(), at, data: json!({ "value": crate::settings::get(conn, key)? }) });
        }
    }
    s.tables.insert("settings".into(), shared);
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
    // Only the fields the row carries: a device with an older app version knows fewer columns,
    // and a missing one must keep its local value (or get the column's default in a new row) -
    // not become empty (`NOT NULL` columns would refuse the whole sync).
    let (names, values): (Vec<String>, Vec<Sql>) = names
        .into_iter()
        .zip(values)
        .enumerate()
        .filter(|(i, _)| *i >= cols.len() || data.get(cols[*i]).is_some())
        .map(|(_, pair)| pair)
        .unzip();
    if exists {
        if names.is_empty() {
            return Ok(());
        }
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

/// The row's time of change as the merge saw it: a time from the future (a device with its clock
/// ahead) is cut to now before it is saved - saved as it came, it would beat every later change
/// from the other device until the clock catches up.
fn at_most(data: &Value, col: &str, at: &str) -> Value {
    let mut data = data.clone();
    if data.get(col).and_then(Value::as_str).is_some_and(|t| t > at) {
        data[col] = Value::String(at.to_string());
    }
    data
}

fn local_at(tx: &Transaction, table: &str, at_col: &str, uid: &str) -> AppResult<Option<String>> {
    Ok(tx.query_row(&format!("SELECT {at_col} FROM {table} WHERE uid = ?1"), [uid], |r| r.get(0)).optional()?.flatten())
}

/// Whether the merged row brings a group of fields this device does not have yet: its time is
/// later, or the same with another content (the merge settled a tie for the other side). A local
/// change made while the sync ran (a later time here) stays.
fn newer_group(merged: &Value, local: &Value, user: bool) -> bool {
    let key = if user { "user_changed_at" } else { "updated_at" };
    let time = |v: &Value| v.get(key).or_else(|| v.get("updated_at")).and_then(Value::as_str).unwrap_or_default().to_string();
    let (theirs, mine) = (time(merged), time(local));
    if theirs != mine {
        return theirs > mine;
    }
    let group = |v: &Value| -> Vec<(String, Value)> {
        let mut fields: Vec<(String, Value)> = v
            .as_object()
            .map(|o| o.iter().filter(|(k, _)| OFFER_USER_FIELDS.contains(&k.as_str()) == user).map(|(k, v)| (k.clone(), v.clone())).collect())
            .unwrap_or_default();
        fields.sort_by(|a, b| a.0.cmp(&b.0));
        fields
    };
    group(merged) != group(local)
}

/// The stage history goes with the stage (the user's fields).
fn write_history(tx: &Transaction, id: i64, data: &Value) -> AppResult<()> {
    let history: Vec<&Value> = data.get("history").and_then(Value::as_array).into_iter().flatten().collect();
    if !history.is_empty() {
        tx.execute("DELETE FROM stage_history WHERE offer_id = ?1", [id])?;
        for h in history {
            tx.execute("INSERT INTO stage_history (offer_id, stage, at) VALUES (?1, ?2, ?3)", params![id, to_sql(h.get("stage")), to_sql(h.get("at"))])?;
        }
    }
    Ok(())
}

fn offer_id(tx: &Transaction, uid: &str) -> AppResult<Option<i64>> {
    Ok(tx.query_row("SELECT id FROM offers WHERE uid = ?1", [uid], |r| r.get(0)).optional()?)
}

/// A local child of the same parent with the same link as the incoming child, under another id.
fn twin_child(tx: &Transaction, uid: &str, data: &Value) -> AppResult<Option<String>> {
    let (Some(parent), Some(url)) = (data.get("parent_uid").and_then(Value::as_str), data.get("url").and_then(Value::as_str)) else {
        return Ok(None);
    };
    let key = crate::sources::alerts::link_key(url);
    let mut stmt = tx.prepare(
        "SELECT c.uid, c.url FROM offers c JOIN offers p ON p.id = c.parent_id
         WHERE p.uid = ?1 AND c.uid != ?2 AND c.url IS NOT NULL ORDER BY c.uid",
    )?;
    let found: Vec<(String, String)> = stmt.query_map(params![parent, uid], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<Result<_, _>>()?;
    Ok(found.into_iter().find(|(_, u)| crate::sources::alerts::link_key(u) == key).map(|(uid, _)| uid))
}

/// One level only: changes from two devices can put a parent under another offer, or two offers
/// under each other - a grandchild goes under the top offer, an offer under itself goes to the top.
fn flatten_tree(tx: &Transaction) -> AppResult<()> {
    tx.execute("UPDATE offers SET parent_id = NULL WHERE parent_id = id", [])?;
    for _ in 0..10_000 {
        let deeper: Option<(i64, i64)> = tx
            .query_row(
                "SELECT c.id, p.parent_id FROM offers c JOIN offers p ON p.id = c.parent_id WHERE p.parent_id IS NOT NULL LIMIT 1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        let Some((child, top)) = deeper else { break };
        tx.execute("UPDATE offers SET parent_id = ?2 WHERE id = ?1", params![child, (top != child).then_some(top)])?;
    }
    Ok(())
}

/// Writes the merged snapshot into this device's database: newer rows replace older ones,
/// tombstones delete. Rows missing from the snapshot are never deleted (the phone keeps
/// only a part of the offers). Ends with a full filter re-check.
pub fn apply(conn: &mut Connection, merged: &Snapshot, now: &str) -> AppResult<()> {
    if merged.format > FORMAT {
        return Err(AppError::Outdated);
    }
    let tx = conn.transaction()?;
    let empty = Vec::new();
    let table = |name: &str| merged.tables.get(name).unwrap_or(&empty);

    // Profiles first (scores point to them), then offers, then what belongs to offers.
    for r in table("profiles") {
        // At the same time the merged row is taken: the merge picked it on both devices alike.
        if local_at(&tx, "profiles", "updated_at", &r.uid)?.is_some_and(|mine| mine > r.at) {
            continue;
        }
        // The same field rules as on this device; a bad row is skipped, the local copy stays.
        if crate::profiles::check_synced(&r.data).is_err() {
            continue;
        }
        upsert(&tx, "profiles", PROFILE_COLS, &r.uid, &at_most(&r.data, "updated_at", &r.at), &[])?;
    }

    for t in table("removed") {
        tx.execute("INSERT OR IGNORE INTO removed_offers (fingerprint, removed_at) VALUES (?1, ?2)", params![t.uid, t.at])?;
    }

    // This device's offers as they are now: a group is written only when the merge took it from
    // the other side (newer, or the same time and another content).
    let mine: std::collections::HashMap<String, Row> =
        export(&tx, "", now)?.tables.remove("offers").unwrap_or_default().into_iter().map(|r| (r.uid.clone(), r)).collect();
    // The parent and "The same?" point to other offers - set after all offers are here.
    let mut links: Vec<(i64, Option<Value>, Option<Value>)> = Vec::new();
    for r in table("offers") {
        // The same field rules as data from outside; a bad row is skipped, the local copy stays.
        if crate::offers::check_synced(&r.data).is_err() {
            continue;
        }
        let fingerprint = r.data.get("fingerprint").and_then(Value::as_str).unwrap_or_default();
        let removed: Option<String> =
            tx.query_row("SELECT removed_at FROM removed_offers WHERE fingerprint = ?1", [fingerprint], |x| x.get(0)).optional()?;
        if let Some(removed) = removed {
            // Deleted for good - unless the user changed it later (a copy loaded after that).
            let user = r.data.get("user_changed_at").and_then(Value::as_str).unwrap_or_default();
            if user <= removed.as_str() {
                continue;
            }
            tx.execute("DELETE FROM removed_offers WHERE fingerprint = ?1", [fingerprint])?;
        }
        // The same offer added on both devices (same company and title, different ids):
        // both keep the smaller id, the other one becomes a tombstone - devices agree.
        let twin: Option<String> = tx
            .query_row("SELECT uid FROM offers WHERE fingerprint = ?1 AND uid != ?2", params![fingerprint, r.uid], |x| x.get(0))
            .optional()?;
        // The same ad as a child on both devices: each device split its offers with several links
        // into children by itself (version 25), so one link got a different id on each.
        let twin = match twin {
            Some(t) => Some(t),
            None => twin_child(&tx, &r.uid, &r.data)?,
        };
        if let Some(twin) = twin {
            // Unless the merge already chose this one (the twin has its tombstone) - the devices
            // could have kept different twins when they joined duplicates on their own.
            let twin_goes = merged.deleted.get("offers").is_some_and(|list| list.iter().any(|t| t.uid == twin));
            if twin < r.uid && !twin_goes {
                tombstone(&tx, "offers", &r.uid, now)?;
                continue;
            }
            if let Some(keep) = offer_id(&tx, &r.uid)? {
                // Both ids are here already (the company of one changed): the larger one goes -
                // renaming it would break the unique id and stop every sync. Its dates, contacts
                // and contact history move to the one that stays.
                for child in ["events", "contacts", "contact_log"] {
                    tx.execute(&format!("UPDATE {child} SET offer_id = ?1 WHERE offer_id = (SELECT id FROM offers WHERE uid = ?2)"), params![keep, twin])?;
                }
                // Its children and the questions about it too.
                for col in ["parent_id", "dup_of"] {
                    tx.execute(&format!("UPDATE offers SET {col} = ?1 WHERE {col} = (SELECT id FROM offers WHERE uid = ?2)"), params![keep, twin])?;
                }
                tx.execute("DELETE FROM offers WHERE uid = ?1", [&twin])?;
            } else {
                tx.execute("UPDATE offers SET uid = ?2 WHERE uid = ?1", params![twin, r.uid])?;
            }
            tombstone(&tx, "offers", &twin, now)?;
        }
        let data = at_most(&at_most(&r.data, "updated_at", &r.at), "user_changed_at", &r.at);
        let (write_data, write_user) = match (mine.get(&r.uid), offer_id(&tx, &r.uid)?) {
            (_, None) => (true, true),
            (Some(local), Some(_)) => (newer_group(&data, &local.data, false), newer_group(&data, &local.data, true)),
            // Here, but not synced from this side (rejected by the filter): by the times only, each
            // group by its own time (the row's time is the later of the two).
            (None, Some(_)) => {
                let theirs = |key: &str| data.get(key).or_else(|| data.get("updated_at")).and_then(Value::as_str).unwrap_or_default().to_string();
                let (data_at, user_at) = (theirs("updated_at"), theirs("user_changed_at"));
                (
                    local_at(&tx, "offers", "updated_at", &r.uid)?.is_none_or(|t| t < data_at),
                    local_at(&tx, "offers", "COALESCE(user_changed_at, updated_at)", &r.uid)?.is_none_or(|t| t < user_at),
                )
            }
        };
        if !write_data && !write_user {
            continue;
        }
        let cols: Vec<&str> = OFFER_COLS
            .iter()
            .copied()
            .filter(|c| if OFFER_USER_FIELDS.contains(c) { write_user } else { write_data })
            .collect();
        upsert(&tx, "offers", &cols, &r.uid, &data, &[])?;
        let id = offer_id(&tx, &r.uid)?.ok_or(AppError::NotFound)?;
        if write_user {
            write_history(&tx, id, &r.data)?;
            links.push((id, r.data.get("parent_uid").cloned(), r.data.get("dup_of_uid").cloned()));
        }
        if !write_data {
            continue;
        }
        for s in r.data.get("sources").and_then(Value::as_array).into_iter().flatten() {
            tx.execute(
                "INSERT OR IGNORE INTO offer_sources (offer_id, source, url, first_seen, city) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![id, to_sql(s.get("source")), to_sql(s.get("url")), to_sql(s.get("first_seen")), to_sql(s.get("city"))],
            )?;
        }
        // A source whose job closed (its terms): its links go on this device too.
        let closed: Option<String> = tx.query_row("SELECT content_removed FROM offers WHERE id = ?1", [id], |x| x.get(0))?;
        if let Some(source) = closed {
            crate::offers::drop_links(&tx, id, &source)?;
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

    // A key missing from the row (a file from an older app version) keeps the local value; an
    // offer this device does not have (not synced from the other side) leaves the field empty.
    for (id, parent, dup_of) in links {
        let local = |v: &Value| -> AppResult<Option<i64>> {
            Ok(match v.as_str() {
                Some(uid) => offer_id(&tx, uid)?.filter(|other| *other != id),
                None => None,
            })
        };
        if let Some(parent) = parent {
            tx.execute("UPDATE offers SET parent_id = ?2 WHERE id = ?1", params![id, local(&parent)?])?;
        }
        if let Some(dup_of) = dup_of {
            match local(&dup_of)? {
                Some(other) => tx.execute("UPDATE offers SET dup_of = ?2 WHERE id = ?1", params![id, other])?,
                None => tx.execute("UPDATE offers SET dup_of = NULL, dup_why = NULL WHERE id = ?1", [id])?,
            };
        }
    }
    flatten_tree(&tx)?;

    for (name, db_table, cols, at_col) in [
        ("events", "events", EVENT_COLS, "created_at"),
        ("contacts", "contacts", CONTACT_COLS, "updated_at"),
        ("log", "contact_log", LOG_COLS, "at"),
    ] {
        for r in table(name) {
            if crate::agenda::check_synced(name, &r.data).is_err() {
                continue;
            }
            let Some(offer) = r.data.get("offer_uid").and_then(Value::as_str) else { continue };
            let Some(id) = offer_id(&tx, offer)? else { continue };
            if local_at(&tx, db_table, at_col, &r.uid)?.is_some_and(|mine| mine > r.at) {
                continue;
            }
            // The log's time is the date the person chose, not a time of change - it stays.
            let data = if name == "contacts" { at_most(&r.data, "updated_at", &r.at) } else { r.data.clone() };
            upsert(&tx, db_table, cols, &r.uid, &data, &[("offer_id", Sql::Integer(id))])?;
        }
    }

    if let Some(r) = table("experience").first().filter(|r| crate::profiles::check_synced_experience(&r.data).is_ok()) {
        let mine: Option<String> = tx.query_row("SELECT updated_at FROM candidate WHERE id = 1", [], |x| x.get(0)).optional()?;
        if mine.is_none_or(|m| m <= r.at) {
            tx.execute(
                "INSERT INTO candidate (id, experience, updated_at) VALUES (1, ?1, ?2)
                 ON CONFLICT (id) DO UPDATE SET experience = excluded.experience, updated_at = excluded.updated_at",
                params![to_sql(r.data.get("experience")), r.at],
            )?;
        }
    }

    for r in table("settings") {
        let Some(value) = r.data.get("value").and_then(Value::as_str) else { continue };
        if crate::settings::changed_at(&tx, &r.uid)?.is_none_or(|mine| mine <= r.at) {
            crate::settings::apply_synced(&tx, &r.uid, value, &r.at)?;
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
        // Own and collected offers on the board.
        let mut t: Vec<String> = [View::Own, View::Board]
            .into_iter()
            .flat_map(|v| crate::offers::list(conn, v, None).unwrap())
            .map(|r| r.title)
            .collect();
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

    fn set_times(conn: &Connection, data: Option<&str>, user: Option<&str>) {
        if let Some(t) = data {
            conn.execute("UPDATE offers SET updated_at = ?1", [t]).unwrap();
        }
        if let Some(t) = user {
            conn.execute("UPDATE offers SET user_changed_at = ?1", [t]).unwrap();
        }
    }

    fn only_id(conn: &Connection) -> i64 {
        conn.query_row("SELECT id FROM offers", [], |r| r.get(0)).unwrap()
    }

    /// The case from the audit of 8.11.2026: the stage and note set on the phone at 10:00 survive
    /// the PC fetching the same offer again at 10:30 - and the PC's new source reaches the phone.
    #[test]
    fn a_fetch_on_one_device_does_not_undo_the_users_change_on_the_other() {
        let (mut pc, mut phone) = (test_db(), test_db());
        let pc_id = add(&mut pc, "Frontend Developer");
        round(&mut pc, &mut phone);
        for c in [&pc, &phone] {
            set_times(c, Some("2026-11-01T00:00:00Z"), Some("2026-11-01T00:00:00Z"));
        }
        let phone_id = only_id(&phone);
        crate::offers::set_stage(&mut phone, phone_id, Stage::Applied).unwrap();
        crate::offers::set_notes(&phone, phone_id, "Wysłane CV".into()).unwrap();
        set_times(&phone, None, Some("2026-11-08T10:00:00Z"));
        let again = OfferInput {
            title: "Frontend Developer".into(),
            company: "Firma Testowa".into(),
            source: "jjit".into(),
            description: Some("Pełny opis z portalu".into()),
            ..Default::default()
        };
        assert_eq!(crate::offers::add(&mut pc, again).unwrap(), AddOutcome::Merged { id: pc_id });
        set_times(&pc, Some("2026-11-08T10:30:00Z"), None);
        round(&mut pc, &mut phone);
        for (conn, id) in [(&pc, pc_id), (&phone, phone_id)] {
            let d = crate::offers::detail(conn, id).unwrap();
            assert_eq!((d.row.stage, d.notes.as_str()), (Stage::Applied, "Wysłane CV"));
            assert_eq!(d.description.as_deref(), Some("Pełny opis z portalu"));
            assert_eq!(d.links.len(), 2);
            assert_eq!(d.history.len(), 2);
        }
    }

    #[test]
    fn the_trash_on_one_device_is_not_undone_by_a_fetch_on_the_other() {
        let (mut pc, mut phone) = (test_db(), test_db());
        let pc_id = add(&mut pc, "Frontend Developer");
        round(&mut pc, &mut phone);
        for c in [&pc, &phone] {
            set_times(c, Some("2026-11-01T00:00:00Z"), Some("2026-11-01T00:00:00Z"));
        }
        crate::offers::trash(&phone, only_id(&phone)).unwrap();
        set_times(&phone, None, Some("2026-11-08T10:00:00Z"));
        let again = OfferInput { title: "Frontend Developer".into(), company: "Firma Testowa".into(), source: "jjit".into(), ..Default::default() };
        crate::offers::add(&mut pc, again).unwrap();
        set_times(&pc, Some("2026-11-08T10:30:00Z"), None);
        round(&mut pc, &mut phone);
        assert!(crate::offers::detail(&pc, pc_id).unwrap().deleted_at.is_some());
        assert!(titles(&pc).is_empty() && titles(&phone).is_empty());
    }

    #[test]
    fn a_purged_offer_is_not_brought_back_by_a_fetch_on_the_other_device() {
        let (mut pc, mut phone) = (test_db(), test_db());
        add(&mut pc, "Frontend Developer");
        round(&mut pc, &mut phone);
        let phone_id = only_id(&phone);
        crate::offers::trash(&phone, phone_id).unwrap();
        crate::offers::purge(&mut phone, Some(phone_id), crate::model::View::Trash).unwrap();
        phone.execute("UPDATE sync_deleted SET at = '2026-11-08T10:00:00Z'", []).unwrap();
        // The PC fetched the same offer later - data only, no change of the user's.
        let again = OfferInput { title: "Frontend Developer".into(), company: "Firma Testowa".into(), source: "jjit".into(), ..Default::default() };
        crate::offers::add(&mut pc, again).unwrap();
        set_times(&pc, Some("2026-11-08T10:30:00Z"), Some("2026-11-01T00:00:00Z"));
        round(&mut pc, &mut phone);
        let count = |c: &Connection| c.query_row("SELECT count(*) FROM offers", [], |r| r.get::<_, i64>(0)).unwrap();
        assert_eq!((count(&pc), count(&phone)), (0, 0));
    }

    /// The same second on both devices: both pick the same version instead of keeping their own.
    #[test]
    fn a_change_at_the_same_second_ends_the_same_on_both_devices() {
        let (mut pc, mut phone) = (test_db(), test_db());
        add(&mut pc, "Frontend Developer");
        round(&mut pc, &mut phone);
        crate::offers::set_notes(&pc, only_id(&pc), "z komputera".into()).unwrap();
        crate::offers::set_notes(&phone, only_id(&phone), "z telefonu".into()).unwrap();
        for c in [&pc, &phone] {
            set_times(c, Some("2026-11-08T10:00:00Z"), Some("2026-11-08T11:00:00Z"));
        }
        round(&mut pc, &mut phone);
        let notes = |c: &Connection| crate::offers::detail(c, only_id(c)).unwrap().notes;
        assert_eq!(notes(&pc), notes(&phone));
    }

    /// The same second for a profile, the experience and a synced setting (audit T8 of 8.11.2026):
    /// both devices end with one version instead of each keeping its own.
    #[test]
    fn profiles_experience_and_settings_at_the_same_second_end_the_same() {
        let (mut pc, mut phone) = (test_db(), test_db());
        let input = |id: Option<i64>, name: &str| ProfileInput { id, name: name.into(), description: "Frontend".into(), criteria: Criteria::default() };
        crate::profiles::save(&mut pc, input(None, "Plan A")).unwrap();
        round(&mut pc, &mut phone);
        let id_of = |c: &Connection| crate::profiles::list(c).unwrap()[0].id;
        let (pc_id, phone_id) = (id_of(&pc), id_of(&phone));
        crate::profiles::save(&mut pc, input(Some(pc_id), "Plan A - komputer")).unwrap();
        crate::profiles::save(&mut phone, input(Some(phone_id), "Plan A - telefon")).unwrap();
        crate::profiles::set_experience(&pc, "z komputera").unwrap();
        crate::profiles::set_experience(&phone, "z telefonu").unwrap();
        crate::settings::set(&pc, "fresh_days", "14").unwrap();
        crate::settings::set(&phone, "fresh_days", "21").unwrap();
        for c in [&pc, &phone] {
            c.execute_batch(
                "UPDATE profiles SET updated_at = '2026-11-08T10:00:00Z';
                 UPDATE candidate SET updated_at = '2026-11-08T10:00:00Z';
                 UPDATE settings SET value = '2026-11-08T10:00:00Z' WHERE key = 'fresh_days_changed_at';",
            )
            .unwrap();
        }
        round(&mut pc, &mut phone);
        let name = |c: &Connection| crate::profiles::list(c).unwrap()[0].name.clone();
        assert_eq!(name(&pc), name(&phone));
        assert_eq!(crate::profiles::experience(&pc).unwrap(), crate::profiles::experience(&phone).unwrap());
        assert_eq!(crate::settings::get(&pc, "fresh_days").unwrap(), crate::settings::get(&phone, "fresh_days").unwrap());
        // The next round changes nothing - the versions stay agreed.
        round(&mut pc, &mut phone);
        assert_eq!(name(&pc), name(&phone));
    }

    /// "Load a copy" on the PC after the phone deleted things: the copy wins on both devices.
    #[test]
    fn a_loaded_copy_wins_over_later_deletions_on_the_other_device() {
        let (mut pc, mut phone) = (test_db(), test_db());
        add(&mut pc, "Frontend Developer");
        add(&mut pc, "QA Engineer");
        round(&mut pc, &mut phone);
        for c in [&pc, &phone] {
            set_times(c, Some("2026-11-01T00:00:00Z"), Some("2026-11-01T00:00:00Z"));
        }
        // The phone: one offer to the trash, one deleted for good - later than the copy.
        let id_of = |c: &Connection, title: &str| c.query_row("SELECT id FROM offers WHERE title = ?1", [title], |r| r.get::<_, i64>(0)).unwrap();
        crate::offers::trash(&phone, id_of(&phone, "Frontend Developer")).unwrap();
        let qa = id_of(&phone, "QA Engineer");
        crate::offers::trash(&phone, qa).unwrap();
        crate::offers::purge(&mut phone, Some(qa), crate::model::View::Trash).unwrap();
        for sql in ["UPDATE offers SET user_changed_at = '2026-11-05T00:00:00Z'", "UPDATE sync_deleted SET at = '2026-11-05T00:00:00Z'", "UPDATE removed_offers SET removed_at = '2026-11-05T00:00:00Z'"] {
            phone.execute(sql, []).unwrap();
        }
        // The PC loads its copy (here: its data as it is) - it counts as changed now.
        crate::backup::copy_wins(&pc, "2026-11-08T12:00:00Z").unwrap();
        round(&mut pc, &mut phone);
        assert_eq!(titles(&pc), vec!["Frontend Developer", "QA Engineer"]);
        assert_eq!(titles(&phone), vec!["Frontend Developer", "QA Engineer"]);
        // Fetching the restored offer again works on both.
        for c in [&mut pc, &mut phone] {
            assert_ne!(add_outcome(c, "QA Engineer"), AddOutcome::Blocked);
        }
    }

    #[test]
    fn a_clock_ahead_and_an_older_app_do_not_spoil_the_row() {
        let (mut pc, mut phone) = (test_db(), test_db());
        let id = add(&mut pc, "Frontend Developer");
        round(&mut pc, &mut phone);
        let published: Option<String> = pc.query_row("SELECT published_at FROM offers WHERE id = ?1", [id], |r| r.get(0)).unwrap();
        assert!(published.is_some());
        // The phone's clock is years ahead, and its older app does not know `published_at`.
        let mut sb = export(&phone, "phone", NOW).unwrap();
        for r in sb.tables.get_mut("offers").unwrap() {
            r.at = "2031-01-01T00:00:00Z".into();
            r.data["updated_at"] = serde_json::json!("2031-01-01T00:00:00Z");
            r.data["notes"] = serde_json::json!("z telefonu");
            r.data.as_object_mut().unwrap().remove("published_at");
        }
        let (merged, _) = merge(&export(&pc, "pc", NOW).unwrap(), &sb, NOW);
        apply(&mut pc, &merged, NOW).unwrap();
        let (notes, updated, kept): (String, String, Option<String>) = pc
            .query_row("SELECT notes, updated_at, published_at FROM offers WHERE id = ?1", [id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .unwrap();
        assert_eq!(notes, "z telefonu");
        assert!(updated.as_str() <= NOW, "{updated}");
        assert_eq!(kept, published);
    }

    #[test]
    fn a_new_offer_from_an_older_app_gets_the_defaults_of_missing_columns() {
        let (mut old, mut phone) = (test_db(), test_db());
        add(&mut old, "Frontend Developer");
        // The sync file written by an app from before `expired_by_user` (NOT NULL DEFAULT 0).
        let mut sa = export(&old, "phone", NOW).unwrap();
        for r in sa.tables.get_mut("offers").unwrap() {
            r.data.as_object_mut().unwrap().remove("expired_by_user");
        }
        let (merged, _) = merge(&sa, &export(&phone, "phone", NOW).unwrap(), NOW);
        apply(&mut phone, &merged, NOW).unwrap();
        assert_eq!(titles(&phone), vec!["Frontend Developer"]);
        let by_user: i64 = phone.query_row("SELECT expired_by_user FROM offers", [], |r| r.get(0)).unwrap();
        assert_eq!(by_user, 0);
    }

    #[test]
    fn the_freshness_settings_travel_and_the_last_change_wins() {
        let (mut pc, mut phone) = (test_db(), test_db());
        round(&mut pc, &mut phone);
        assert_eq!(crate::settings::get(&phone, "fresh_days").unwrap(), "10");

        crate::settings::set(&pc, "fresh_days", "21").unwrap();
        crate::settings::set(&pc, "today_count", "30").unwrap();
        round(&mut pc, &mut phone);
        assert_eq!(crate::settings::get(&phone, "fresh_days").unwrap(), "21");
        assert_eq!(crate::settings::get(&phone, "today_count").unwrap(), "30");

        std::thread::sleep(std::time::Duration::from_millis(1100));
        crate::settings::set(&pc, "fresh_days", "10").unwrap();
        round(&mut pc, &mut phone);
        assert_eq!(crate::settings::get(&phone, "fresh_days").unwrap(), "10");
        assert_eq!(crate::settings::get(&pc, "fresh_days").unwrap(), "10");
    }

    #[test]
    fn a_setting_chosen_before_settings_travelled_still_reaches_the_phone() {
        let (mut pc, mut phone) = (test_db(), test_db());
        pc.execute("INSERT INTO settings (key, value) VALUES ('fresh_days', '14')", []).unwrap();
        round(&mut pc, &mut phone);
        assert_eq!(crate::settings::get(&phone, "fresh_days").unwrap(), "14");

        crate::settings::set(&phone, "fresh_days", "30").unwrap();
        round(&mut pc, &mut phone);
        assert_eq!(crate::settings::get(&pc, "fresh_days").unwrap(), "30");
    }

    #[test]
    fn a_snapshot_from_a_newer_version_is_not_applied() {
        let mut phone = test_db();
        let snapshot = Snapshot { format: FORMAT + 1, ..Default::default() };
        assert!(matches!(apply(&mut phone, &snapshot, NOW), Err(AppError::Outdated)));
    }

    #[test]
    fn only_listed_settings_with_good_values_come_from_the_sync_file() {
        let mut phone = test_db();
        crate::settings::set(&phone, "theme", "dark").unwrap();
        let row = |uid: &str, value: &str| Row { uid: uid.into(), at: "2099-01-01T00:00:00Z".into(), data: json!({ "value": value }) };
        let mut snapshot = Snapshot { format: FORMAT, ..Default::default() };
        snapshot.tables.insert("settings".into(), vec![row("fresh_days", "1; DROP TABLE offers"), row("theme", "light")]);
        apply(&mut phone, &snapshot, NOW).unwrap();
        assert_eq!(crate::settings::get(&phone, "fresh_days").unwrap(), "10");
        assert_eq!(crate::settings::get(&phone, "theme").unwrap(), "dark");
    }

    #[test]
    fn purged_offer_does_not_come_back() {
        let (mut pc, mut phone) = (test_db(), test_db());
        let id = add(&mut pc, "QA Engineer");
        round(&mut pc, &mut phone);
        crate::offers::trash(&pc, id).unwrap();
        crate::offers::purge(&mut pc, Some(id), crate::model::View::Trash).unwrap();
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

    /// Audit R22: an offer shared on the phone and worked on there, then collected on the PC before
    /// the phone synced - the person's work stays, whichever of the two ids wins.
    #[test]
    fn work_on_the_phone_survives_the_same_offer_collected_on_the_pc() {
        let (low, high) = ("a".repeat(32), "b".repeat(32));
        for (phone_uid, pc_uid) in [(&low, &high), (&high, &low)] {
            let (mut pc, mut phone) = (test_db(), test_db());
            let id = add(&mut phone, "Data Analyst");
            phone.execute("UPDATE offers SET uid = ?1", [phone_uid]).unwrap();
            crate::offers::set_stage(&mut phone, id, Stage::Applied).unwrap();
            crate::offers::set_notes(&phone, id, "Wysłane CV".into()).unwrap();
            std::thread::sleep(std::time::Duration::from_millis(1100));
            let fetched = OfferInput { title: "Data Analyst".into(), company: "Firma Testowa".into(), source: "jjit".into(), ..Default::default() };
            crate::offers::add(&mut pc, fetched).unwrap();
            pc.execute("UPDATE offers SET uid = ?1", [pc_uid]).unwrap();
            round(&mut pc, &mut phone);
            round(&mut pc, &mut phone);
            for c in [&pc, &phone] {
                let d = crate::offers::detail(c, only_id(c)).unwrap();
                assert_eq!((d.row.stage, d.notes.as_str()), (Stage::Applied, "Wysłane CV"), "phone id {}", &phone_uid[..1]);
            }
        }
    }

    /// An offer the filter rejects still travels once the person worked on it (a note here).
    #[test]
    fn a_rejected_offer_with_a_note_travels() {
        let (mut pc, mut phone) = (test_db(), test_db());
        let fetched = OfferInput { title: "Data Analyst".into(), company: "Firma Testowa".into(), source: "jjit".into(), ..Default::default() };
        let AddOutcome::Added { id } = crate::offers::add(&mut pc, fetched).unwrap() else { panic!() };
        pc.execute("UPDATE offers SET filter_result = 'fail'", []).unwrap();
        assert!(!export(&pc, "pc", NOW).unwrap().tables.contains_key("offers"));
        crate::offers::set_notes(&pc, id, "Zapytać o zdalną".into()).unwrap();
        round(&mut pc, &mut phone);
        assert_eq!(crate::offers::detail(&phone, only_id(&phone)).unwrap().notes, "Zapytać o zdalną");
    }

    /// A contact history entry dated in the future, deleted on one device, does not come back.
    #[test]
    fn a_deleted_log_entry_dated_later_stays_deleted() {
        let (mut pc, mut phone) = (test_db(), test_db());
        let id = add(&mut pc, "Data Analyst");
        let log = crate::agenda::LogInput { at: "2099-06-01T09:00:00Z".into(), kind: "call".into(), note: "Oddzwonić".into() };
        let entry = crate::agenda::add_log(&pc, id, log).unwrap();
        round(&mut pc, &mut phone);
        assert_eq!(crate::agenda::agenda(&phone, only_id(&phone)).unwrap().log.len(), 1);
        crate::agenda::delete_log(&pc, entry).unwrap();
        round(&mut pc, &mut phone);
        round(&mut pc, &mut phone);
        assert!(crate::agenda::agenda(&pc, id).unwrap().log.is_empty());
        assert!(crate::agenda::agenda(&phone, only_id(&phone)).unwrap().log.is_empty());
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

    #[test]
    fn bad_profile_date_contact_and_history_rows_from_the_sync_file_are_skipped() {
        let (mut pc, mut phone) = (test_db(), test_db());
        let input = |name: &str| ProfileInput { id: None, name: name.into(), description: "Frontend".into(), criteria: Criteria::default() };
        crate::profiles::save(&mut phone, input("Plan A")).unwrap();
        crate::profiles::save(&mut phone, input("Plan B")).unwrap();
        let id = add(&mut phone, "Tester");
        crate::agenda::add_event(&phone, id, crate::agenda::EventInput { kind: "interview".into(), at: "2026-11-05T13:00:00Z".into(), minutes: 60, note: String::new() }).unwrap();
        crate::agenda::add_event(&phone, id, crate::agenda::EventInput { kind: "other".into(), at: "2026-11-06T13:00:00Z".into(), minutes: 60, note: String::new() }).unwrap();
        crate::agenda::save_contact(&phone, id, crate::agenda::Contact { name: "Anna Zmyślona".into(), ..Default::default() }).unwrap();
        crate::agenda::save_contact(&phone, id, crate::agenda::Contact { name: "Jan Testowy".into(), ..Default::default() }).unwrap();
        crate::agenda::add_log(&phone, id, crate::agenda::LogInput { at: "2026-11-04T09:00:00Z".into(), kind: "call".into(), note: "Rozmowa".into() }).unwrap();
        crate::agenda::add_log(&phone, id, crate::agenda::LogInput { at: "2026-11-04T10:00:00Z".into(), kind: "mail".into(), note: String::new() }).unwrap();
        crate::profiles::set_experience(&phone, "4 lata React").unwrap();

        let mut snapshot = export(&phone, "phone", NOW).unwrap();
        let mut spoil = |table: &str, index: usize, key: &str, value: Value| {
            snapshot.tables.get_mut(table).unwrap()[index].data.as_object_mut().unwrap().insert(key.into(), value);
        };
        spoil("profiles", 1, "name", json!("x".repeat(1_000)));
        spoil("events", 1, "at", json!("jutro"));
        spoil("contacts", 1, "linkedin", json!("javascript:alert(1)"));
        spoil("log", 1, "kind", json!("telegram"));
        spoil("experience", 0, "experience", json!(42));
        apply(&mut pc, &snapshot, NOW).unwrap();

        let names: Vec<String> = crate::profiles::list(&pc).unwrap().into_iter().map(|p| p.name).collect();
        assert_eq!(names, vec!["Plan A"]);
        let pc_id: i64 = pc.query_row("SELECT id FROM offers", [], |r| r.get(0)).unwrap();
        let a = crate::agenda::agenda(&pc, pc_id).unwrap();
        assert_eq!(a.events.len(), 1);
        assert_eq!(a.contacts.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(), vec!["Anna Zmyślona"]);
        assert_eq!(a.log.len(), 1);
        assert_eq!(crate::profiles::experience(&pc).unwrap(), "");
    }

    /// The phone's first sync (R22, checked once on a copy of a real database): an empty phone ends
    /// with every synced field of the PC, and the next round changes nothing on either side.
    #[test]
    fn an_empty_phone_gets_everything_and_the_next_round_changes_nothing() {
        let (mut pc, mut phone) = (test_db(), test_db());
        crate::profiles::save(&mut pc, ProfileInput { id: None, name: "Plan A".into(), description: "Frontend".into(), criteria: Criteria::default() }).unwrap();
        crate::profiles::set_experience(&pc, "4 lata React").unwrap();
        let input = |city: &str, url: &str| OfferInput {
            title: "Frontend Developer".into(),
            company: "Firma Testowa".into(),
            source: "pracuj".into(),
            city: Some(city.into()),
            url: Some(url.into()),
            salary_min: Some(12000),
            salary_max: Some(15000),
            salary_currency: Some("PLN".into()),
            description: Some("Opis oferty".into()),
            ..Default::default()
        };
        let id = match crate::offers::add(&mut pc, input("Kraków", "https://example.com/oferta/1")).unwrap() {
            AddOutcome::Added { id } => id,
            other => panic!("{other:?}"),
        };
        // Another source without a link: one more link row of the same offer (another ad with a
        // link is a child - the tree travels from part 2 of zmiany.md, T1).
        crate::offers::add(&mut pc, OfferInput { source: "jjit".into(), url: None, ..input("Gdańsk", "https://example.com/oferta/2") }).unwrap();
        crate::offers::set_interest(&pc, id, Some(crate::model::Interest::High)).unwrap();
        crate::offers::set_notes(&pc, id, "Zadzwonić we wtorek".into()).unwrap();
        crate::offers::set_stage(&mut pc, id, Stage::Applied).unwrap();
        pc.execute(
            "INSERT OR REPLACE INTO offer_scores (offer_id, profile_id, score, source, why, quote) SELECT ?1, id, 80, 'ai', 'Pasuje', 'React' FROM profiles",
            [id],
        )
        .unwrap();
        let event = crate::agenda::EventInput { kind: "interview".into(), at: "2099-02-01T10:00:00Z".into(), minutes: 45, note: "Rozmowa".into() };
        crate::agenda::add_event(&pc, id, event).unwrap();
        let contact = crate::agenda::Contact { name: "Anna Zmyślona".into(), email: "anna@example.com".into(), ..Default::default() };
        crate::agenda::save_contact(&pc, id, contact).unwrap();
        let log = crate::agenda::LogInput { at: "2026-11-02T09:00:00Z".into(), kind: "call".into(), note: "Telefon".into() };
        crate::agenda::add_log(&pc, id, log).unwrap();
        let gone = add(&mut pc, "Tester");
        crate::offers::trash(&pc, gone).unwrap();
        crate::offers::purge(&mut pc, Some(gone), crate::model::View::Trash).unwrap();

        round(&mut pc, &mut phone);
        let (a, b) = (export(&pc, "pc", NOW).unwrap(), export(&phone, "phone", NOW).unwrap());
        assert_eq!((&a.tables, &a.deleted), (&b.tables, &b.deleted));
        for table in ["offers", "profiles", "experience", "events", "contacts", "log", "removed"] {
            assert!(a.tables.contains_key(table), "{table} missing in the test data");
        }
        let d = crate::offers::detail(&phone, only_id(&phone)).unwrap();
        assert_eq!(d.links.len(), 2);
        assert_eq!(d.history.len(), 2);

        let (_, stats) = merge(&b, &a, NOW);
        assert_eq!(stats.changed + stats.removed, 0);
        let (_, stats) = merge(&a, &b, NOW);
        assert_eq!(stats.changed + stats.removed, 0);
    }

    fn linked(conn: &mut Connection, source: &str, url: &str) -> i64 {
        let input = OfferInput { title: "Tester".into(), company: "Firma Testowa".into(), source: source.into(), url: Some(url.into()), ..Default::default() };
        match crate::offers::add(conn, input).unwrap() {
            AddOutcome::Added { id } | AddOutcome::Merged { id } | AddOutcome::Unchanged { id } => id,
            other => panic!("{other:?}"),
        }
    }

    fn id_by_uid(conn: &Connection, uid: &str) -> i64 {
        conn.query_row("SELECT id FROM offers WHERE uid = ?1", [uid], |r| r.get(0)).unwrap()
    }

    fn uid_of(conn: &Connection, id: i64) -> String {
        conn.query_row("SELECT uid FROM offers WHERE id = ?1", [id], |r| r.get(0)).unwrap()
    }

    fn children(conn: &Connection) -> Vec<(String, String)> {
        let mut stmt = conn.prepare("SELECT c.uid, p.uid FROM offers c JOIN offers p ON p.id = c.parent_id ORDER BY c.uid").unwrap();
        stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?))).unwrap().collect::<Result<_, _>>().unwrap()
    }

    /// Part 2, T1: a child travels under its parent (named by uid - ids differ), and "Detach" on
    /// the phone reaches the PC.
    #[test]
    fn the_tree_travels_and_detach_on_the_phone_reaches_the_pc() {
        let (mut pc, mut phone) = (test_db(), test_db());
        let parent = linked(&mut pc, "jjit", "https://justjoin.it/job-offer/firma-tester");
        linked(&mut pc, "nfj", "https://nofluffjobs.com/job/tester-firma");
        assert_eq!(children(&pc).len(), 1);
        round(&mut pc, &mut phone);
        assert_eq!(children(&phone), children(&pc));
        let rows = crate::offers::list(&phone, View::Board, None).unwrap();
        assert_eq!((rows.len(), rows[0].children.len()), (1, 1));

        let (child_uid, _) = children(&phone)[0].clone();
        std::thread::sleep(std::time::Duration::from_millis(1100));
        crate::tree::set_parent(&phone, id_by_uid(&phone, &child_uid), None).unwrap();
        round(&mut pc, &mut phone);
        assert!(children(&pc).is_empty() && children(&phone).is_empty());
        assert!(id_by_uid(&phone, &uid_of(&pc, parent)) > 0);
    }

    /// "The same?" (the same company and title, another city) points to the other offer by uid.
    #[test]
    fn the_same_question_travels() {
        let (mut pc, mut phone) = (test_db(), test_db());
        let first = linked(&mut pc, "jjit", "https://justjoin.it/job-offer/firma-tester");
        let second = add(&mut pc, "Tester Gdańsk");
        pc.execute(
            "UPDATE offers SET dup_of = ?1, dup_why = 'city', user_changed_at = '2026-11-12T10:00:00Z' WHERE id = ?2",
            params![first, second],
        )
        .unwrap();
        round(&mut pc, &mut phone);
        let on_phone = id_by_uid(&phone, &uid_of(&pc, second));
        let note = crate::dups::note(&phone, on_phone).unwrap().unwrap();
        assert_eq!((uid_of(&phone, note.was.id), note.why.as_str()), (uid_of(&pc, first), "city"));
        // "A separate offer" on the phone - the question goes on the PC too.
        std::thread::sleep(std::time::Duration::from_millis(1100));
        crate::dups::answer(&mut phone, on_phone, false).unwrap();
        round(&mut pc, &mut phone);
        assert!(crate::dups::note(&pc, second).unwrap().is_none());
    }

    /// Both devices split an offer with two links into a parent and a child by themselves
    /// (version 25) - each child got its own id. After a sync there is one child, on both.
    #[test]
    fn children_made_on_both_devices_become_one() {
        let (mut pc, mut phone) = (test_db(), test_db());
        let parent = linked(&mut pc, "jjit", "https://justjoin.it/job-offer/firma-tester");
        // The old way: one offer with two links.
        pc.execute(
            "INSERT INTO offer_sources (offer_id, source, url, first_seen) VALUES (?1, 'nfj', 'https://nofluffjobs.com/job/tester-firma', '2026-11-01T00:00:00Z')",
            [parent],
        )
        .unwrap();
        round(&mut pc, &mut phone);
        assert_eq!((crate::tree::convert(&pc).unwrap(), crate::tree::convert(&phone).unwrap()), (1, 1));
        assert_ne!(children(&pc), children(&phone));
        round(&mut pc, &mut phone);
        round(&mut pc, &mut phone);
        assert_eq!(children(&pc).len(), 1);
        assert_eq!(children(&pc), children(&phone));
    }

    /// Two devices' changes can make two levels (a parent put under another offer) - one level stays.
    #[test]
    fn the_tree_stays_one_level_deep() {
        let mut pc = test_db();
        let a = add(&mut pc, "Tester A");
        let b = add(&mut pc, "Tester B");
        let c = add(&mut pc, "Tester C");
        pc.execute("UPDATE offers SET parent_id = ?1 WHERE id = ?2", params![b, a]).unwrap();
        pc.execute("UPDATE offers SET parent_id = ?1 WHERE id = ?2", params![c, b]).unwrap();
        pc.execute("UPDATE offers SET parent_id = ?1 WHERE id = ?2", params![a, c]).unwrap();
        let tx = pc.transaction().unwrap();
        flatten_tree(&tx).unwrap();
        tx.commit().unwrap();
        let deeper: i64 = pc
            .query_row("SELECT count(*) FROM offers c JOIN offers p ON p.id = c.parent_id WHERE p.parent_id IS NOT NULL OR c.parent_id = c.id", [], |r| r.get(0))
            .unwrap();
        assert_eq!(deeper, 0);
    }
}
