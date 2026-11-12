//! The tree of duplicates: the first ad of a job is the parent, the same job seen later in another
//! ad (another portal, or a second ad of the same portal) is its child - a row of its own with its
//! own link and status, shown under the parent. One level only: a child never has children.

use rusqlite::{Connection, OptionalExtension, params};

use crate::clock;
use crate::error::{AppError, AppResult};
use crate::model::*;
use crate::offers::NOT_BY_USER;
use crate::sources::alerts::link_key;

/// Where a link of an offer that is already known belongs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Place {
    /// The parent's own link, or the family has no link yet - the parent takes it.
    Parent,
    /// The link of this child.
    Child(i64),
    /// A link the family does not know - a new ad, a new child.
    New,
}

/// Where `url` belongs in the family of `parent`.
pub fn place(conn: &Connection, parent: i64, url: &str) -> AppResult<Place> {
    let key = link_key(url);
    let mut stmt = conn.prepare(
        "SELECT o.id, s.url FROM offer_sources s JOIN offers o ON o.id = s.offer_id
         WHERE (o.id = ?1 OR o.parent_id = ?1) AND s.url IS NOT NULL
         UNION ALL SELECT id, url FROM offers WHERE (id = ?1 OR parent_id = ?1) AND url IS NOT NULL",
    )?;
    let links: Vec<(i64, String)> = stmt.query_map([parent], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<Result<_, _>>()?;
    if links.is_empty() {
        return Ok(Place::Parent);
    }
    Ok(match links.iter().find(|(_, l)| link_key(l) == key) {
        Some((id, _)) if *id == parent => Place::Parent,
        Some((id, _)) => Place::Child(*id),
        None => Place::New,
    })
}

/// A new ad of a known job: a child of `parent` with the ad's own fields and link. Added by hand -
/// the person's own ("interested", like a new offer added by hand); collected - "new". The child is
/// own when its parent is. Returns the child's id.
pub fn add_child(conn: &Connection, parent: i64, input: &OfferInput, now: &str) -> AppResult<i64> {
    let uid = crate::secrets::random_hex(16)?;
    let manual = input.source == "manual";
    let stage = if manual { Stage::Interested } else { Stage::New };
    conn.execute(
        "INSERT INTO offers (uid, fingerprint, title, company, url, city, remote,
            salary_min, salary_max, salary_currency, salary_period, contract, level, language,
            requirements, description, expires_at, stage, stage_changed_at, added_at, updated_at, published_at,
            user_changed_at, own, parent_id)
         SELECT ?2, p.fingerprint || '#' || ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17,
            ?18, ?19, ?19, ?19, COALESCE(?20, ?19), ?21, CASE WHEN ?22 THEN 1 ELSE COALESCE(p.own, 0) END, p.id
         FROM offers p WHERE p.id = ?1",
        params![
            parent, uid, input.title, input.company, input.url, input.city,
            input.remote.unwrap_or(Remote::Unknown), input.salary_min, input.salary_max,
            input.salary_currency, input.salary_period, input.contract, input.level, input.language,
            input.requirements, input.description, input.expires_at, stage, now, input.published_at,
            if manual { now } else { NOT_BY_USER }, manual
        ],
    )?;
    let child = conn.last_insert_rowid();
    conn.execute("INSERT INTO stage_history (offer_id, stage, at) VALUES (?1, ?2, ?3)", params![child, stage, now])?;
    // Added by hand - the whole job is the person's own.
    if manual {
        set_family_own(conn, child, true)?;
    }
    crate::offers::add_link(conn, child, input, now)?;
    crate::profiles::recheck_offer(conn, child)?;
    Ok(child)
}

/// Version 25: an offer that kept several links (other portals, the same job in other cities) gets
/// a child for every link but its own. The parent keeps the link it opens (`url`), else the first
/// seen; the same link written twice (with other tracking parameters) stays one ad. A child takes the
/// parent's fields, the city its link gave and the day it was seen; it starts as "new".
pub fn convert(conn: &Connection) -> AppResult<usize> {
    let parents: Vec<(i64, Option<String>)> = conn
        .prepare(
            "SELECT o.id, o.url FROM offers o
             WHERE (SELECT count(*) FROM offer_sources s WHERE s.offer_id = o.id AND s.url IS NOT NULL) > 1",
        )?
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<Result<_, _>>()?;
    let mut made = 0;
    for (parent, own_url) in parents {
        let links: Vec<(i64, String, String, String, Option<String>)> = conn
            .prepare(
                "SELECT rowid, source, url, first_seen, city FROM offer_sources
                 WHERE offer_id = ?1 AND url IS NOT NULL ORDER BY first_seen, rowid",
            )?
            .query_map([parent], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)))?
            .collect::<Result<_, _>>()?;
        let parent_key = own_url
            .as_deref()
            .map(link_key)
            .filter(|k| links.iter().any(|(_, _, u, _, _)| &link_key(u) == k))
            .unwrap_or_else(|| link_key(&links[0].2));
        // One child for every other link (its rows move with it).
        let mut children: Vec<(String, i64)> = Vec::new();
        for (rowid, _source, url, first_seen, city) in &links {
            let key = link_key(url);
            if key == parent_key {
                continue;
            }
            let child = match children.iter().find(|(k, _)| *k == key) {
                Some((_, child)) => *child,
                None => {
                    let uid = crate::secrets::random_hex(16)?;
                    conn.execute(
                        "INSERT INTO offers (uid, fingerprint, title, company, url, city, remote,
                            salary_min, salary_max, salary_currency, salary_period, contract, level, language,
                            requirements, description, expires_at, expired, stage, stage_changed_at, added_at,
                            updated_at, published_at, user_changed_at, own, filter_result, archived_at, deleted_at,
                            parent_id)
                         SELECT ?2, fingerprint || '#' || ?2, title, company, ?3, COALESCE(?4, city), remote,
                            salary_min, salary_max, salary_currency, salary_period, contract, level, language,
                            requirements, description, expires_at, expired, 'new', ?5, ?5,
                            ?6, ?5, ?7, own, filter_result, archived_at, deleted_at, id
                         FROM offers WHERE id = ?1",
                        params![parent, uid, url, city, first_seen, clock::now(), NOT_BY_USER],
                    )?;
                    let child = conn.last_insert_rowid();
                    conn.execute(
                        "INSERT INTO stage_history (offer_id, stage, at) VALUES (?1, 'new', ?2)",
                        params![child, first_seen],
                    )?;
                    children.push((key, child));
                    made += 1;
                    child
                }
            };
            conn.execute("UPDATE offer_sources SET offer_id = ?1 WHERE rowid = ?2", params![child, rowid])?;
        }
        for (_, child) in &children {
            crate::profiles::recheck_offer(conn, *child)?;
        }
    }
    Ok(made)
}

/// Through which ad of the family of `root` the CV went first (`children_only` - not the parent
/// itself): the portal of its link (else its first source) and the day of the first stage past
/// "interested". `None` = nobody in a recruitment.
pub fn sent_via(conn: &Connection, root: i64, children_only: bool) -> AppResult<Option<SentVia>> {
    let found: Option<(i64, Option<String>, Option<String>, String)> = conn
        .query_row(
            "SELECT o.id, o.url,
                (SELECT s.source FROM offer_sources s WHERE s.offer_id = o.id ORDER BY s.source = 'manual', s.first_seen, s.rowid LIMIT 1),
                COALESCE((SELECT min(h.at) FROM stage_history h WHERE h.offer_id = o.id AND h.stage NOT IN ('new', 'interested')),
                    o.stage_changed_at) AS sent
             FROM offers o
             WHERE (o.parent_id = ?1 OR (o.id = ?1 AND NOT ?2)) AND o.deleted_at IS NULL AND o.stage NOT IN ('new', 'interested')
             ORDER BY sent, o.id LIMIT 1",
            params![root, children_only],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .optional()?;
    Ok(found.map(|(id, url, source, at)| SentVia {
        id,
        source: url.as_deref().and_then(crate::sources::alerts::portal_of_link).or(source).unwrap_or_else(|| "manual".into()),
        at,
    }))
}

/// "Own" is one for the whole family (the parent and every child): the checkbox on the card of any
/// of its ads, a stage past "new" of any of them. Returns how many offers there are in the family.
pub fn set_family_own(conn: &Connection, id: i64, own: bool) -> AppResult<usize> {
    Ok(conn.execute(
        "UPDATE offers SET own = ?2, user_changed_at = ?3
         WHERE id = (SELECT COALESCE(parent_id, id) FROM offers WHERE id = ?1)
            OR parent_id = (SELECT COALESCE(parent_id, id) FROM offers WHERE id = ?1)",
        params![id, own, clock::now()],
    )?)
}

/// A family joined from offers of both kinds: own when any of them was.
pub fn join_own(conn: &Connection, parent: i64) -> AppResult<()> {
    conn.execute(
        "UPDATE offers SET own = 1 WHERE (id = ?1 OR parent_id = ?1) AND COALESCE(own, 0) = 0
            AND EXISTS (SELECT 1 FROM offers f WHERE (f.id = ?1 OR f.parent_id = ?1) AND COALESCE(f.own, 0) = 1)",
        [parent],
    )?;
    Ok(())
}

/// "Detach" (`parent = None`): the child is an offer of its own - a mistake of the duplicate rules
/// undone with one click (AI reads it then, like any new offer). With a parent: "Undo" puts it back.
/// Only a top-level offer without children can be a child, and only of a top-level offer.
pub fn set_parent(conn: &Connection, id: i64, parent: Option<i64>) -> AppResult<()> {
    let now = clock::now();
    let changed = match parent {
        None => conn.execute(
            "UPDATE offers SET parent_id = NULL, updated_at = ?2, user_changed_at = ?2 WHERE id = ?1 AND parent_id IS NOT NULL",
            params![id, now],
        )?,
        Some(parent) => {
            let fits: bool = conn
                .query_row(
                    "SELECT ?1 != ?2
                        AND (SELECT parent_id IS NULL FROM offers WHERE id = ?2)
                        AND NOT EXISTS (SELECT 1 FROM offers WHERE parent_id = ?1)",
                    params![id, parent],
                    |r| r.get::<_, Option<bool>>(0),
                )
                .optional()?
                .flatten()
                .unwrap_or(false);
            if !fits {
                return Err(AppError::Invalid("this offer cannot go under that one".into()));
            }
            let n = conn.execute(
                "UPDATE offers SET parent_id = ?2, updated_at = ?3, user_changed_at = ?3 WHERE id = ?1 AND parent_id IS NULL",
                params![id, parent, now],
            )?;
            if n > 0 {
                join_own(conn, parent)?;
            }
            n
        }
    };
    if changed == 0 {
        return Err(AppError::NotFound);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::tests::test_db;
    use crate::offers::{add, detail, list};

    fn offer(source: &str, url: Option<&str>, city: Option<&str>) -> OfferInput {
        OfferInput {
            title: "Tester".into(),
            company: "Firma Przykładowa".into(),
            source: source.into(),
            url: url.map(Into::into),
            city: city.map(Into::into),
            description: Some("Testy manualne aplikacji webowej.".into()),
            ..Default::default()
        }
    }

    fn id_of(outcome: AddOutcome) -> i64 {
        match outcome {
            AddOutcome::Added { id } | AddOutcome::Merged { id } | AddOutcome::Unchanged { id } => id,
            AddOutcome::Blocked => panic!("blocked"),
        }
    }

    fn children_of(conn: &Connection, parent: i64) -> Vec<i64> {
        conn.prepare("SELECT id FROM offers WHERE parent_id = ?1 ORDER BY id")
            .unwrap()
            .query_map([parent], |r| r.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap()
    }

    #[test]
    fn another_ad_of_a_known_job_is_a_child_and_the_same_link_is_not() {
        let mut conn = test_db();
        let parent = id_of(add(&mut conn, offer("jjit", Some("https://justjoin.it/job-offer/firma-tester"), Some("Kraków"))).unwrap());
        // Another portal: a child, under the parent and not on the list by itself.
        let other = add(&mut conn, offer("nfj", Some("https://nofluffjobs.com/job/tester-firma"), Some("Kraków"))).unwrap();
        assert_eq!(other, AddOutcome::Merged { id: parent });
        let kids = children_of(&conn, parent);
        assert_eq!(kids.len(), 1);
        // The same links again (also with tracking) - nothing new.
        add(&mut conn, offer("nfj", Some("https://nofluffjobs.com/job/tester-firma?utm_source=mail"), Some("Kraków"))).unwrap();
        add(&mut conn, offer("jjit", Some("https://justjoin.it/job-offer/firma-tester"), Some("Kraków"))).unwrap();
        assert_eq!(children_of(&conn, parent), kids);
        let rows = list(&conn, View::Board, None).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].children.iter().map(|c| c.id).collect::<Vec<_>>(), kids);
        let child = detail(&conn, kids[0]).unwrap();
        assert_eq!(child.row.parent_id, Some(parent));
        assert_eq!(child.links.len(), 1);
        assert_eq!(child.links[0].source, "nfj");
        // A child is not read by AI - its parent is.
        let waiting: i64 = conn
            .query_row(&format!("SELECT count(*) FROM offers o WHERE {}", crate::offers::waiting_for_ai(&conn).unwrap()), [], |r| r.get(0))
            .unwrap();
        assert_eq!(waiting, 1);
    }

    #[test]
    fn a_second_ad_added_by_hand_is_an_own_child() {
        let mut conn = test_db();
        let parent = id_of(add(&mut conn, offer("manual", Some("https://www.praca.pl/tester_1.html"), None)).unwrap());
        add(&mut conn, offer("manual", Some("https://www.praca.pl/tester_2.html"), Some("Gdańsk"))).unwrap();
        let kid = children_of(&conn, parent)[0];
        let d = detail(&conn, kid).unwrap();
        assert!(d.row.own);
        assert_eq!(d.row.stage, Stage::Interested);
        assert_eq!(d.row.city.as_deref(), Some("Gdańsk"));
        assert_eq!(list(&conn, View::Own, None).unwrap().len(), 1);
        assert_eq!(crate::offers::counts(&conn, None).unwrap().to_send, 1);
    }

    #[test]
    fn detach_makes_an_offer_of_its_own_and_undo_puts_it_back() {
        let mut conn = test_db();
        let parent = id_of(add(&mut conn, offer("jjit", Some("https://justjoin.it/job-offer/firma-tester"), None)).unwrap());
        add(&mut conn, offer("nfj", Some("https://nofluffjobs.com/job/tester-firma"), None)).unwrap();
        let kid = children_of(&conn, parent)[0];
        set_parent(&conn, kid, None).unwrap();
        assert_eq!(list(&conn, View::Board, None).unwrap().len(), 2);
        // Back under its parent; a parent with children never goes under another offer.
        set_parent(&conn, kid, Some(parent)).unwrap();
        assert_eq!(list(&conn, View::Board, None).unwrap().len(), 1);
        let other = id_of(add(&mut conn, OfferInput { title: "Analityk".into(), ..offer("jjit", None, None) }).unwrap());
        assert!(set_parent(&conn, parent, Some(other)).is_err());
        assert!(set_parent(&conn, other, Some(kid)).is_err(), "no grandchildren");
    }

    #[test]
    fn purging_a_parent_takes_its_children() {
        let mut conn = test_db();
        let parent = id_of(add(&mut conn, offer("jjit", Some("https://justjoin.it/job-offer/firma-tester"), None)).unwrap());
        add(&mut conn, offer("nfj", Some("https://nofluffjobs.com/job/tester-firma"), None)).unwrap();
        crate::offers::trash(&conn, parent).unwrap();
        assert_eq!(list(&conn, View::Bin, None).unwrap().len(), 1, "the child waits under its parent");
        crate::offers::purge(&mut conn, Some(parent), View::Bin).unwrap();
        assert_eq!(conn.query_row("SELECT count(*) FROM offers", [], |r| r.get::<_, i64>(0)).unwrap(), 0);
    }

    #[test]
    fn a_child_sent_is_the_family_sent() {
        let mut conn = test_db();
        let parent = id_of(add(&mut conn, offer("jjit", Some("https://justjoin.it/job-offer/firma-tester"), Some("Kraków"))).unwrap());
        add(&mut conn, offer("nfj", Some("https://nofluffjobs.com/job/tester-firma"), Some("Kraków"))).unwrap();
        let kid = children_of(&conn, parent)[0];
        crate::offers::set_stage(&mut conn, parent, Stage::Interested).unwrap();
        assert_eq!(crate::offers::counts(&conn, None).unwrap().to_send, 1);
        // The CV goes through the child: the child has the stage, the parent keeps its own and
        // says through which ad it went; the family is not "to send" any more.
        crate::offers::set_stage(&mut conn, kid, Stage::Applied).unwrap();
        let rows = list(&conn, View::Own, None).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].stage, Stage::Interested);
        let via = rows[0].sent_via.clone().expect("sent through the child");
        assert_eq!((via.id, via.source.as_str()), (kid, "nfj"));
        assert_eq!(rows[0].children[0].stage, Stage::Applied);
        assert_eq!(crate::offers::counts(&conn, None).unwrap().to_send, 0);
        // Another ad of the job: said before saving.
        let same = crate::same::check(&conn, offer("pracuj", Some("https://www.pracuj.pl/praca/tester,oferta,123"), Some("Kraków"))).unwrap();
        assert_eq!(same.kind, SameKind::Joined);
        assert_eq!(same.was.unwrap().sent_via.map(|v| v.id), Some(kid));
    }

    #[test]
    fn a_collected_family_goes_to_own_when_a_child_is_sent() {
        let mut conn = test_db();
        let parent = id_of(add(&mut conn, offer("jjit", Some("https://justjoin.it/job-offer/firma-tester"), None)).unwrap());
        add(&mut conn, offer("nfj", Some("https://nofluffjobs.com/job/tester-firma"), None)).unwrap();
        let kid = children_of(&conn, parent)[0];
        assert!(list(&conn, View::Own, None).unwrap().is_empty());
        // The checkbox "own" on the child's card: the whole family, there and back.
        crate::offers::set_own(&conn, kid, true).unwrap();
        assert!(list(&conn, View::Own, None).unwrap()[0].own);
        assert!(detail(&conn, kid).unwrap().row.own);
        crate::offers::set_own(&conn, kid, false).unwrap();
        assert!(list(&conn, View::Own, None).unwrap().is_empty());
        assert!(!detail(&conn, parent).unwrap().row.own);
        crate::offers::set_stage(&mut conn, kid, Stage::Applied).unwrap();
        assert_eq!(list(&conn, View::Own, None).unwrap()[0].id, parent);
        assert_eq!(crate::same::check(&conn, offer("jjit", Some("https://justjoin.it/job-offer/firma-tester"), None)).unwrap().was.unwrap().sent_via.map(|v| v.id), Some(kid));
        // A second ad added by hand to a collected job - the job is own.
        let mut conn = test_db();
        let parent = id_of(add(&mut conn, offer("jjit", Some("https://justjoin.it/job-offer/firma-tester"), None)).unwrap());
        add(&mut conn, offer("manual", Some("https://nofluffjobs.com/job/tester-firma"), None)).unwrap();
        assert_eq!(list(&conn, View::Own, None).unwrap()[0].id, parent);
    }

    #[test]
    fn merged_links_become_children_once() {
        let mut conn = test_db();
        let parent = id_of(add(&mut conn, offer("jjit", Some("https://justjoin.it/job-offer/firma-tester"), Some("Kraków"))).unwrap());
        conn.execute("UPDATE offers SET stage = 'applied', notes = 'moje' WHERE id = ?1", [parent]).unwrap();
        // As before version 25: more links of one offer (another portal, another city, the same
        // link with tracking from another source).
        for (source, url, city) in [
            ("nfj", "https://nofluffjobs.com/job/tester-firma", Some("Kraków")),
            ("jjit", "https://justjoin.it/job-offer/firma-tester-gdansk", Some("Gdańsk")),
            ("manual", "https://justjoin.it/job-offer/firma-tester?utm_source=x", None),
        ] {
            conn.execute(
                "INSERT INTO offer_sources (offer_id, source, url, first_seen, city) VALUES (?1, ?2, ?3, '2026-11-01T00:00:00Z', ?4)",
                params![parent, source, url, city],
            )
            .unwrap();
        }
        assert_eq!(convert(&conn).unwrap(), 2);
        let kids = children_of(&conn, parent);
        assert_eq!(kids.len(), 2);
        let gdansk = detail(&conn, kids[1]).unwrap();
        assert_eq!((gdansk.row.city.as_deref(), gdansk.row.stage), (Some("Gdańsk"), Stage::New));
        assert_eq!(gdansk.notes, "", "the person's work stays with the parent");
        let p = detail(&conn, parent).unwrap();
        assert_eq!((p.row.stage, p.notes.as_str(), p.links.len()), (Stage::Applied, "moje", 2));
        assert_eq!(convert(&conn).unwrap(), 0, "once");
    }
}
