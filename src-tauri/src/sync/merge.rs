//! Merging two copies of the data (PC and phone) row by row - the same rules as in
//! Mikropłatności (see "Scalanie zmian" in the glossary):
//! - every row has a stable `uid` and the time of its last change; the newer version wins,
//! - an offer has two times and two groups of fields: the user's own (stage, interest, star,
//!   note, trash, archive, "expired") and the data from sources and AI - each group is taken from
//!   the side where it changed last, so a fetch on one device never undoes a change made on the other,
//! - the same time on both sides is settled by the content - both devices pick the same version,
//! - a deleted row leaves a tombstone, so the other device does not bring it back;
//!   a change made after the deletion wins over the tombstone (for an offer: only the user's change),
//! - times "from the future" (a clock far ahead, a tampered file) are cut to now.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::{AppError, AppResult};

/// One synchronised row: its table, stable id, time of the last change and the fields.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Row {
    pub uid: String,
    /// RFC 3339 in UTC - compares correctly as text.
    pub at: String,
    pub data: Value,
}

/// A deleted row (or a deleted offer remembered by its fingerprint).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Tombstone {
    pub uid: String,
    pub at: String,
}

/// Everything one device sends: rows and tombstones per table.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Snapshot {
    /// Format of the file - a newer one is refused instead of being misread.
    pub format: u32,
    /// Database version of the app that wrote the file (`user_version`); 0 = written before the
    /// version travelled. Devices with different versions do not merge (see `check_versions`).
    #[serde(default)]
    pub schema: i64,
    pub device: String,
    pub exported_at: String,
    pub tables: BTreeMap<String, Vec<Row>>,
    pub deleted: BTreeMap<String, Vec<Tombstone>>,
}

pub const FORMAT: u32 = 1;

/// Fields of an offer that only the user changes (with `user_changed_at` as their time). The
/// stage history goes with the stage. All other fields are the data, timed by `updated_at`.
pub const OFFER_USER_FIELDS: &[&str] = &[
    "stage", "stage_changed_at", "interest", "favorite", "notes", "deleted_at", "archived_at", "kept_at", "expired",
    "expired_by_user", "user_changed_at", "history", "own", "renew_asked", "parent_uid", "dup_of_uid", "dup_why",
];

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MergeStats {
    /// Rows that came from the other device or replaced an older local version.
    pub changed: usize,
    /// Local rows the other device deleted.
    pub removed: usize,
}

/// A file from a newer app version is refused: merging it here would write it back
/// in the old format and drop the fields this version does not know.
pub fn check_format(remote: &Snapshot) -> AppResult<()> {
    if remote.format > FORMAT {
        return Err(AppError::Outdated);
    }
    Ok(())
}

/// Both devices must run the same database version: a newer version may change the data when
/// it starts (a clean start, new keys of offers), and an older device merged with it would bring
/// the old data back. The newer side waits for the other device's update; this device's own file
/// written before its update is fine - its data went through the update here.
pub fn check_versions(remote: &Snapshot, local: &Snapshot) -> AppResult<()> {
    check_format(remote)?;
    if remote.schema == 0 || remote.schema == local.schema {
        return Ok(());
    }
    if remote.schema > local.schema {
        return Err(AppError::Outdated);
    }
    if remote.device != local.device {
        return Err(AppError::OtherOutdated);
    }
    Ok(())
}

fn clip(at: &str, now: &str) -> String {
    if at > now { now.to_string() } else { at.to_string() }
}

/// A time field of an offer row (cut to now); a row from before two times falls back to `at`.
fn offer_time(r: &Row, key: &str, now: &str) -> String {
    let at = r.data.get(key).and_then(Value::as_str).filter(|t| !t.is_empty()).unwrap_or(&r.at);
    clip(at, now)
}

/// The user's fields or the data fields of an offer, always in the same order - compared when
/// both sides changed them at the same second.
fn offer_group(r: &Row, user: bool) -> String {
    let empty = serde_json::Map::new();
    let fields = r.data.as_object().unwrap_or(&empty);
    let picked: Vec<(&String, &Value)> =
        fields.iter().filter(|(k, _)| OFFER_USER_FIELDS.contains(&k.as_str()) == user).collect::<BTreeMap<_, _>>().into_iter().collect();
    serde_json::to_string(&picked).unwrap_or_default()
}

/// A row's fields in one fixed order - the same text on both devices for the same content.
fn content(r: &Row) -> String {
    match r.data.as_object() {
        Some(fields) => serde_json::to_string(&fields.iter().collect::<BTreeMap<_, _>>()).unwrap_or_default(),
        None => r.data.to_string(),
    }
}

/// One offer from both sides: the data from the side where it changed last, the user's fields
/// from the side where the user changed them last. A tie in time goes by the content, so both
/// devices end up with the same row; the user's fields at the same time follow the data - the
/// app's own jobs (archive, "expired" by date) change them together with the data.
fn merge_offer(mine: &Row, theirs: &Row, now: &str) -> Row {
    let data = |r: &Row| (offer_time(r, "updated_at", now), offer_group(r, false));
    let user = |r: &Row| (offer_time(r, "user_changed_at", now), offer_group(r, true));
    let (data_mine, data_theirs) = (data(mine), data(theirs));
    let data_cmp = data_mine.cmp(&data_theirs);
    let (user_mine, user_theirs) = (user(mine), user(theirs));
    let user_cmp = user_mine.0.cmp(&user_theirs.0).then(data_cmp).then(user_mine.1.cmp(&user_theirs.1));
    let data_from = if data_cmp.is_lt() { theirs } else { mine };
    let user_from = if user_cmp.is_lt() { theirs } else { mine };
    let mut out = data_from.data.clone();
    if let (Some(out), Some(from)) = (out.as_object_mut(), user_from.data.as_object()) {
        for key in OFFER_USER_FIELDS {
            match from.get(*key) {
                Some(v) => out.insert(key.to_string(), v.clone()),
                None => out.remove(*key),
            };
        }
    }
    let at = if data_cmp.is_lt() { data_theirs.0 } else { data_mine.0 };
    let at = at.max(if user_cmp.is_lt() { user_theirs.0 } else { user_mine.0 });
    Row { uid: mine.uid.clone(), at, data: out }
}

/// Merges `remote` into `local` and returns the result (what both devices should hold)
/// and what changed for the local side.
pub fn merge(local: &Snapshot, remote: &Snapshot, now: &str) -> (Snapshot, MergeStats) {
    let mut stats = MergeStats::default();
    let mut out = Snapshot {
        format: FORMAT,
        schema: local.schema,
        device: local.device.clone(),
        exported_at: now.to_string(),
        ..Default::default()
    };
    let names: std::collections::BTreeSet<&String> = local
        .tables
        .keys()
        .chain(remote.tables.keys())
        .chain(local.deleted.keys())
        .chain(remote.deleted.keys())
        .collect();
    for name in names {
        // Tombstones of both sides; the newest one per uid.
        let mut tombs: BTreeMap<String, String> = BTreeMap::new();
        for t in local.deleted.get(name).into_iter().flatten().chain(remote.deleted.get(name).into_iter().flatten()) {
            let at = clip(&t.at, now);
            let slot = tombs.entry(t.uid.clone()).or_default();
            if at > *slot {
                *slot = at;
            }
        }
        let offers = name == "offers";
        let mut rows: BTreeMap<String, Row> = BTreeMap::new();
        let mut local_rows: BTreeMap<&str, &Row> = BTreeMap::new();
        for r in local.tables.get(name).into_iter().flatten() {
            local_rows.insert(&r.uid, r);
            rows.insert(r.uid.clone(), Row { at: clip(&r.at, now), ..r.clone() });
        }
        for r in remote.tables.get(name).into_iter().flatten() {
            let candidate = Row { at: clip(&r.at, now), ..r.clone() };
            match rows.get(&r.uid) {
                Some(mine) if offers => {
                    let both = merge_offer(mine, &candidate, now);
                    if both.data != mine.data {
                        stats.changed += 1;
                    }
                    rows.insert(r.uid.clone(), both);
                }
                // A tie in time goes by the content, so both devices keep the same row.
                Some(mine) if mine.at > candidate.at || (mine.at == candidate.at && content(mine) >= content(&candidate)) => {}
                _ => {
                    rows.insert(r.uid.clone(), candidate);
                    stats.changed += 1;
                }
            }
        }
        // A tombstone newer than (or as new as) the row removes it; a later change revives it -
        // for an offer only the user's change: a fetch on the other device does not bring back
        // an offer deleted for good.
        let mut kept = Vec::new();
        for (uid, row) in rows {
            // A contact history entry cannot be edited, so its deletion always wins (its time is the
            // date the person chose - it may be later than the deletion).
            let alive_at = if offers {
                offer_time(&row, "user_changed_at", now)
            } else if name == "log" {
                String::new()
            } else {
                row.at.clone()
            };
            match tombs.get(&uid) {
                Some(dead) if *dead >= alive_at => {
                    if local_rows.contains_key(uid.as_str()) {
                        stats.removed += 1;
                    }
                }
                _ => {
                    tombs.remove(&uid);
                    kept.push(row);
                }
            }
        }
        // A deleted offer's fingerprint goes when the user changed that offer later (a copy
        // loaded after the deletion) - otherwise it would block the offer again on every sync.
        if name == "removed" {
            let revived: std::collections::HashMap<&str, String> = out
                .tables
                .get("offers")
                .into_iter()
                .flatten()
                .filter_map(|o| Some((o.data.get("fingerprint")?.as_str()?, offer_time(o, "user_changed_at", now))))
                .collect();
            kept.retain(|r| revived.get(r.uid.as_str()).is_none_or(|user| *user <= r.at));
        }
        if !kept.is_empty() {
            out.tables.insert(name.clone(), kept);
        }
        if !tombs.is_empty() {
            out.deleted.insert(name.clone(), tombs.into_iter().map(|(uid, at)| Tombstone { uid, at }).collect());
        }
    }
    (out, stats)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn row(uid: &str, at: &str, note: &str) -> Row {
        Row { uid: uid.into(), at: at.into(), data: json!({ "notes": note }) }
    }

    fn snap(device: &str, rows: Vec<Row>, deleted: Vec<Tombstone>) -> Snapshot {
        let mut s = Snapshot { format: FORMAT, device: device.into(), ..Default::default() };
        s.tables.insert("offers".into(), rows);
        if !deleted.is_empty() {
            s.deleted.insert("offers".into(), deleted);
        }
        s
    }

    const NOW: &str = "2026-11-05T12:00:00Z";

    #[test]
    fn a_file_from_a_newer_version_is_refused() {
        assert!(check_format(&snap("phone", vec![], vec![])).is_ok());
        let newer = Snapshot { format: FORMAT + 1, ..snap("phone", vec![row("a", NOW, "x")], vec![]) };
        assert!(matches!(check_format(&newer), Err(AppError::Outdated)));
    }

    #[test]
    fn devices_with_different_database_versions_do_not_merge() {
        let at = |device: &str, schema: i64| Snapshot { schema, ..snap(device, vec![], vec![]) };
        assert!(check_versions(&at("phone", 20), &at("pc", 20)).is_ok());
        // A file from before the version travelled.
        assert!(check_versions(&at("phone", 0), &at("pc", 20)).is_ok());
        assert!(matches!(check_versions(&at("phone", 21), &at("pc", 20)), Err(AppError::Outdated)));
        assert!(matches!(check_versions(&at("phone", 19), &at("pc", 20)), Err(AppError::OtherOutdated)));
        // This device's own file from before its update.
        assert!(check_versions(&at("pc", 19), &at("pc", 20)).is_ok());
        // The merged file says which version wrote it.
        assert_eq!(merge(&at("pc", 20), &at("phone", 20), NOW).0.schema, 20);
    }

    #[test]
    fn newer_row_wins_on_both_sides_and_new_rows_are_added() {
        let pc = snap("pc", vec![row("a", "2026-11-01T10:00:00Z", "pc old"), row("b", "2026-11-03T10:00:00Z", "pc new")], vec![]);
        let phone = snap("phone", vec![row("a", "2026-11-02T10:00:00Z", "phone new"), row("b", "2026-11-01T10:00:00Z", "phone old"), row("c", "2026-11-01T09:00:00Z", "only phone")], vec![]);
        let (merged, stats) = merge(&pc, &phone, NOW);
        let notes: Vec<&str> = merged.tables["offers"].iter().map(|r| r.data["notes"].as_str().unwrap()).collect();
        assert_eq!(notes, vec!["phone new", "pc new", "only phone"]);
        assert_eq!(stats, MergeStats { changed: 2, removed: 0 });
        // Merging the other way gives the same rows - both devices end up equal.
        let (back, _) = merge(&phone, &pc, NOW);
        assert_eq!(back.tables, merged.tables);
        // Merging again changes nothing.
        let (again, stats) = merge(&merged, &phone, NOW);
        assert_eq!(again.tables, merged.tables);
        assert_eq!(stats.changed, 0);
    }

    #[test]
    fn deleted_rows_do_not_come_back_unless_changed_later() {
        let pc = snap("pc", vec![row("b", "2026-11-03T10:00:00Z", "kept on pc")], vec![Tombstone { uid: "a".into(), at: "2026-11-04T10:00:00Z".into() }]);
        let phone = snap("phone", vec![row("a", "2026-11-02T10:00:00Z", "stale"), row("b", "2026-11-03T10:00:00Z", "kept on pc")], vec![]);
        let (merged, _) = merge(&pc, &phone, NOW);
        assert_eq!(merged.tables["offers"].len(), 1);
        assert_eq!(merged.deleted["offers"][0].uid, "a");

        // The phone deletes "b"; the PC changed it earlier - the deletion wins.
        let phone2 = snap("phone", vec![], vec![Tombstone { uid: "b".into(), at: "2026-11-04T11:00:00Z".into() }]);
        let (merged2, stats) = merge(&pc, &phone2, NOW);
        assert!(merged2.tables.get("offers").is_none());
        assert_eq!(stats.removed, 1);

        // Changed after the deletion - it comes back.
        let pc_late = snap("pc", vec![row("b", "2026-11-05T09:00:00Z", "edited later")], vec![]);
        let (merged3, _) = merge(&pc_late, &phone2, NOW);
        assert_eq!(merged3.tables["offers"][0].data["notes"], "edited later");
        assert!(merged3.deleted.get("offers").is_none());
    }

    #[test]
    fn times_from_the_future_are_cut() {
        let pc = snap("pc", vec![row("a", "2026-11-04T10:00:00Z", "real")], vec![]);
        let phone = snap("phone", vec![row("a", "2031-01-01T00:00:00Z", "clock far ahead")], vec![]);
        let (merged, _) = merge(&pc, &phone, NOW);
        // Cut to now - it still wins this time, but cannot block later real changes for years.
        assert_eq!(merged.tables["offers"][0].at, NOW);
        let pc_later = snap("pc", vec![row("a", "2026-11-05T12:00:01Z", "next real change")], vec![]);
        let (next, _) = merge(&pc_later, &merged, "2026-11-05T13:00:00Z");
        assert_eq!(next.tables["offers"][0].data["notes"], "next real change");
    }
}
