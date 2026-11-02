//! Merging two copies of the data (PC and phone) row by row - the same rules as in
//! Mikropłatności (see "Scalanie zmian" in the glossary):
//! - every row has a stable `uid` and the time of its last change; the newer version wins,
//! - a deleted row leaves a tombstone, so the other device does not bring it back;
//!   a change made after the deletion wins over the tombstone,
//! - times "from the future" (a clock far ahead, a tampered file) are cut to now.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

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
    pub device: String,
    pub exported_at: String,
    pub tables: BTreeMap<String, Vec<Row>>,
    pub deleted: BTreeMap<String, Vec<Tombstone>>,
}

pub const FORMAT: u32 = 1;

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MergeStats {
    /// Rows that came from the other device or replaced an older local version.
    pub changed: usize,
    /// Local rows the other device deleted.
    pub removed: usize,
}

fn clip(at: &str, now: &str) -> String {
    if at > now { now.to_string() } else { at.to_string() }
}

/// Merges `remote` into `local` and returns the result (what both devices should hold)
/// and what changed for the local side.
pub fn merge(local: &Snapshot, remote: &Snapshot, now: &str) -> (Snapshot, MergeStats) {
    let mut stats = MergeStats::default();
    let mut out = Snapshot { format: FORMAT, device: local.device.clone(), exported_at: now.to_string(), ..Default::default() };
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
        let mut rows: BTreeMap<String, Row> = BTreeMap::new();
        let mut local_rows: BTreeMap<&str, &Row> = BTreeMap::new();
        for r in local.tables.get(name).into_iter().flatten() {
            local_rows.insert(&r.uid, r);
            rows.insert(r.uid.clone(), Row { at: clip(&r.at, now), ..r.clone() });
        }
        for r in remote.tables.get(name).into_iter().flatten() {
            let candidate = Row { at: clip(&r.at, now), ..r.clone() };
            match rows.get(&r.uid) {
                Some(mine) if mine.at >= candidate.at => {}
                _ => {
                    rows.insert(r.uid.clone(), candidate);
                    stats.changed += 1;
                }
            }
        }
        // A tombstone newer than (or as new as) the row removes it; a later change revives it.
        let mut kept = Vec::new();
        for (uid, row) in rows {
            match tombs.get(&uid) {
                Some(dead) if *dead >= row.at => {
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
