//! Learning from the "Very" / "No" ratings - plain counting on this device, without AI. Words
//! from a fixed list (technologies, jobs and skills of `synonyms`) that are common in offers
//! rated "Very" raise similar offers, the ones common in offers rated "No" lower them.
//!
//! The result is a small correction kept beside the score: AI scores, keyword scores and hard
//! filters never see it. It is worked out from scratch, so it needs no sync of its own - the
//! ratings travel with the offers and every device counts by itself.

use std::collections::HashSet;

use rusqlite::{Connection, params};

use crate::dedup::normalize;
use crate::error::{AppError, AppResult};

/// Ratings ("Very" and "No" together) needed before anything is learned.
pub const MIN_RATINGS: i64 = 5;
/// The largest correction, up or down. The migration checks the same number.
pub const MAX_POINTS: i64 = 15;
/// A word seen in one rated offer says nothing yet.
const MIN_OFFERS: usize = 2;
/// Points of a word found in all "Very" offers and in no "No" offer, when seen very often.
const POINTS_PER_WORD: f64 = 8.0;
/// A word about as common on both sides says nothing either.
const MIN_WEIGHT: f64 = 0.2;
/// How many words the card names.
const SHOWN_WORDS: usize = 3;

/// Technologies and tools: the name as it is written, then its other spellings. Only one or two
/// words each. `C#`, `C++`, `.NET` and `Go` are left out: without their signs they read as "c",
/// "net" and "go", which are everyday words in offers.
const TECHNOLOGIES: &[&[&str]] = &[
    &["WordPress"],
    &["WooCommerce"],
    &["Elementor"],
    &["Shopify"],
    &["Magento"],
    &["PrestaShop"],
    &["Drupal"],
    &["Joomla"],
    &["PHP"],
    &["Laravel"],
    &["Symfony"],
    &["JavaScript", "JS"],
    &["TypeScript", "TS"],
    &["React", "React.js", "ReactJS"],
    &["React Native"],
    &["Next.js", "NextJS"],
    &["Vue", "Vue.js", "VueJS"],
    &["Nuxt"],
    &["Angular"],
    &["Svelte"],
    &["Node.js", "NodeJS"],
    &["Express.js", "ExpressJS"],
    &["NestJS"],
    &["HTML", "HTML5"],
    &["CSS", "CSS3"],
    &["Sass", "SCSS"],
    &["Tailwind"],
    &["Python"],
    &["Django"],
    &["Flask"],
    &["FastAPI"],
    &["Java"],
    &["Spring", "Spring Boot"],
    &["Kotlin"],
    &["Swift"],
    &["Flutter"],
    &["Dart"],
    &["Android"],
    &["iOS"],
    &["Rust"],
    &["Ruby"],
    &["Rails"],
    &["Scala"],
    &["Golang"],
    &["SQL"],
    &["MySQL"],
    &["PostgreSQL", "Postgres"],
    &["MongoDB"],
    &["Redis"],
    &["GraphQL"],
    &["Docker"],
    &["Kubernetes", "K8s"],
    &["Terraform"],
    &["AWS"],
    &["Azure"],
    &["GCP", "Google Cloud"],
    &["Linux"],
    &["Git"],
    &["CI/CD"],
    &["DevOps"],
    &["Selenium"],
    &["Cypress"],
    &["Playwright"],
    &["Unity"],
    &["Unreal"],
    &["LLM"],
    &["ChatGPT"],
    &["OpenAI"],
    &["Claude"],
    &["Figma"],
    &["Photoshop"],
    &["Illustrator"],
    &["InDesign"],
    &["AutoCAD"],
    &["Excel"],
    &["Power BI"],
    &["Tableau"],
    &["SAP"],
    &["Salesforce"],
    &["HubSpot"],
    &["Jira"],
    &["SEO"],
    &["Google Ads"],
    &["Scrum"],
    &["Agile"],
];

/// `synonyms` keeps its phrases without Polish letters; these are the ones a card shows.
const WRITTEN: &[(&str, &str)] = &[
    ("ai", "AI"),
    ("qa", "QA"),
    ("ksiegowy", "księgowy"),
    ("ksiegowa", "księgowa"),
    ("inzynier", "inżynier"),
    ("tlumacz", "tłumacz"),
    ("tlumaczka", "tłumaczka"),
    ("obsluga klienta", "obsługa klienta"),
];

/// One word to look for: all its spellings, and its name (`None` = the spelling found in the offer).
struct Term {
    spellings: Vec<String>,
    name: Option<&'static str>,
}

fn terms() -> Vec<Term> {
    let technologies = TECHNOLOGIES.iter().map(|names| Term {
        spellings: names.iter().map(|n| normalize(n)).collect(),
        name: Some(names[0]),
    });
    let jobs = crate::synonyms::GROUPS.iter().map(|group| Term {
        spellings: group.iter().map(|w| w.to_string()).collect(),
        name: None,
    });
    technologies.chain(jobs).collect()
}

/// Every word and every pair of neighbouring words of a normalized text (single spaces only).
/// Faster than searching the text for each term, and only whole words match.
fn phrases(text: &str) -> HashSet<&str> {
    let mut out = HashSet::new();
    let mut previous = None;
    let mut start = 0;
    for word in text.split(' ') {
        let end = start + word.len();
        out.insert(&text[start..end]);
        if let Some(p) = previous {
            out.insert(&text[p..end]);
        }
        previous = Some(start);
        start = end + 1;
    }
    out
}

/// A term found in an offer: its place in the list and the name to show.
type Found = Vec<(usize, String)>;

fn find(terms: &[Term], title: &str, requirements: &str, description: &str) -> Found {
    let text = normalize(&format!("{title} {requirements} {description}"));
    let words = phrases(&text);
    terms
        .iter()
        .enumerate()
        .filter_map(|(i, term)| {
            let spelling = term.spellings.iter().find(|s| words.contains(s.as_str()))?;
            let name = term
                .name
                .map(str::to_string)
                .or_else(|| WRITTEN.iter().find(|(w, _)| *w == spelling).map(|(_, n)| n.to_string()))
                .unwrap_or_else(|| spelling.clone());
            Some((i, name))
        })
        .collect()
}

/// Weight of every term, -1 to 1: how much more common it is among "Very" than among "No",
/// trusted less when it was seen in few offers.
fn weights(term_count: usize, very: &[Found], no: &[Found]) -> Vec<f64> {
    let count = |offers: &[Found], term: usize| offers.iter().filter(|o| o.iter().any(|(t, _)| *t == term)).count();
    (0..term_count)
        .map(|term| {
            let (up, down) = (count(very, term), count(no, term));
            let seen = up + down;
            if seen < MIN_OFFERS {
                return 0.0;
            }
            let share = up as f64 / very.len().max(1) as f64 - down as f64 / no.len().max(1) as f64;
            let weight = share * seen as f64 / (seen as f64 + 2.0);
            if weight.abs() < MIN_WEIGHT { 0.0 } else { weight }
        })
        .collect()
}

/// The correction of one offer and the words behind it (only those pulling the same way);
/// `None` when it comes to nothing.
fn correction(found: &Found, weights: &[f64]) -> Option<(i64, Vec<String>)> {
    let total: f64 = found.iter().map(|(t, _)| weights[*t] * POINTS_PER_WORD).sum();
    let points = (total.round() as i64).clamp(-MAX_POINTS, MAX_POINTS);
    if points == 0 {
        return None;
    }
    let mut behind: Vec<(f64, &str)> = found
        .iter()
        .map(|(t, name)| (weights[*t], name.as_str()))
        .filter(|(w, _)| *w * points as f64 > 0.0)
        .collect();
    behind.sort_by(|a, b| b.0.abs().total_cmp(&a.0.abs()).then(a.1.cmp(b.1)));
    Some((points, behind.into_iter().take(SHOWN_WORDS).map(|(_, n)| n.to_string()).collect()))
}

/// Works everything out again. Rated offers count wherever they are (also the archive and the
/// trash: the rating still says something); the correction goes to live offers without a
/// "Very" or "No" of their own - those already have the user's answer.
pub fn refresh(conn: &mut Connection) -> AppResult<()> {
    let tx = conn.transaction()?;
    tx.execute("DELETE FROM offer_learned", [])?;
    let ratings: i64 = tx.query_row("SELECT COUNT(*) FROM offers WHERE interest IN ('high', 'no')", [], |r| r.get(0))?;
    if ratings >= MIN_RATINGS && crate::settings::is_on(&tx, "learn_scores")? {
        let terms = terms();
        let (mut very, mut no, mut live) = (Vec::new(), Vec::new(), Vec::new());
        {
            let mut stmt = tx.prepare(
                "SELECT id, COALESCE(interest, ''), title, COALESCE(requirements, ''), COALESCE(description, '') FROM offers
                 WHERE interest IN ('high', 'no') OR (deleted_at IS NULL AND archived_at IS NULL)",
            )?;
            let mut rows = stmt.query([])?;
            while let Some(r) = rows.next()? {
                let id: i64 = r.get(0)?;
                let interest: String = r.get(1)?;
                let (title, requirements, description): (String, String, String) = (r.get(2)?, r.get(3)?, r.get(4)?);
                let found = find(&terms, &title, &requirements, &description);
                match interest.as_str() {
                    "high" => very.push(found),
                    "no" => no.push(found),
                    _ => live.push((id, found)),
                }
            }
        }
        let weights = weights(terms.len(), &very, &no);
        let mut insert = tx.prepare("INSERT INTO offer_learned (offer_id, points, words) VALUES (?1, ?2, ?3)")?;
        for (id, found) in &live {
            if let Some((points, words)) = correction(found, &weights) {
                let words = serde_json::to_string(&words).map_err(|e| AppError::Invalid(e.to_string()))?;
                insert.execute(params![id, points, words])?;
            }
        }
    }
    tx.commit()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::tests::test_db;
    use crate::model::{Interest, OfferInput, View};
    use crate::offers;

    fn add(conn: &mut Connection, title: &str, description: &str) -> i64 {
        let input = OfferInput {
            title: title.into(),
            company: format!("Firma {title}"),
            source: "manual".into(),
            description: Some(description.into()),
            ..Default::default()
        };
        match offers::add(conn, input).unwrap() {
            crate::model::AddOutcome::Added { id } => id,
            other => panic!("{other:?}"),
        }
    }

    /// Five rated offers: three "Very" with WordPress and React, two "No" with Java.
    fn rated(conn: &mut Connection) {
        for (title, text, interest) in [
            ("A", "WordPress i React", Interest::High),
            ("B", "Budujemy strony: WordPress, React", Interest::High),
            ("C", "WordPress, React, TypeScript", Interest::High),
            ("D", "Backend w Java", Interest::No),
            ("E", "Java i Kotlin", Interest::No),
        ] {
            let id = add(conn, title, text);
            offers::set_interest(conn, id, Some(interest)).unwrap();
        }
    }

    fn learned(conn: &Connection, id: i64) -> (i64, Vec<String>) {
        let row = offers::detail(conn, id).unwrap().row;
        (row.learned, row.learned_words)
    }

    #[test]
    fn rated_words_move_similar_offers_and_the_card_names_them() {
        let mut conn = test_db();
        rated(&mut conn);
        let up = add(&mut conn, "Frontend", "Szukamy osoby: WordPress i React");
        let down = add(&mut conn, "Backend", "Tylko Java");
        let plain = add(&mut conn, "Księgowa", "Biuro rachunkowe");
        refresh(&mut conn).unwrap();
        let (points, words) = learned(&conn, up);
        assert!(points > 0 && points <= MAX_POINTS, "{points}");
        assert_eq!(words, vec!["React", "WordPress"]);
        let (points, words) = learned(&conn, down);
        assert!(points < 0, "{points}");
        assert_eq!(words, vec!["Java"]);
        assert_eq!(learned(&conn, plain), (0, vec![]));
    }

    #[test]
    fn nothing_is_learned_before_five_ratings_or_with_the_setting_off() {
        let mut conn = test_db();
        for title in ["A", "B", "C", "D"] {
            let id = add(&mut conn, title, "WordPress");
            offers::set_interest(&conn, id, Some(Interest::High)).unwrap();
        }
        let offer = add(&mut conn, "Nowa", "WordPress");
        refresh(&mut conn).unwrap();
        assert_eq!(learned(&conn, offer).0, 0);
        let fifth = add(&mut conn, "E", "WordPress");
        offers::set_interest(&conn, fifth, Some(Interest::High)).unwrap();
        refresh(&mut conn).unwrap();
        assert!(learned(&conn, offer).0 > 0);
        crate::settings::set(&conn, "learn_scores", "off").unwrap();
        refresh(&mut conn).unwrap();
        assert_eq!(learned(&conn, offer).0, 0);
    }

    #[test]
    fn the_correction_stays_within_its_limit() {
        let many = TECHNOLOGIES.iter().map(|n| n[0]).collect::<Vec<_>>().join(", ");
        let mut conn = test_db();
        for title in ["A", "B", "C", "D", "E", "F"] {
            let id = add(&mut conn, title, &many);
            offers::set_interest(&conn, id, Some(Interest::High)).unwrap();
        }
        let up = add(&mut conn, "Nowa", &many);
        refresh(&mut conn).unwrap();
        let (points, words) = learned(&conn, up);
        assert_eq!(points, MAX_POINTS);
        assert_eq!(words.len(), SHOWN_WORDS);
    }

    #[test]
    fn scores_and_filters_stay_as_they_were() {
        let mut conn = test_db();
        rated(&mut conn);
        let id = add(&mut conn, "Frontend", "WordPress i React");
        conn.execute("UPDATE offers SET score = 90, score_source = 'ai' WHERE id = ?1", [id]).unwrap();
        let filter_before = offers::detail(&conn, id).unwrap().row.filter_result;
        refresh(&mut conn).unwrap();
        let row = offers::detail(&conn, id).unwrap().row;
        assert_eq!((row.score, row.score_source.as_deref()), (Some(90), Some("ai")));
        assert_eq!(row.filter_result, filter_before);
        assert!(row.learned > 0);
    }

    #[test]
    fn rated_offers_get_no_correction_and_a_new_rating_counts_at_once() {
        let mut conn = test_db();
        rated(&mut conn);
        let id = add(&mut conn, "Frontend", "WordPress i React");
        refresh(&mut conn).unwrap();
        let before = learned(&conn, id).0;
        assert!(before > 0);
        // Another "No" with these words weakens them for every offer.
        let other = add(&mut conn, "Inna", "Czekam na ocenę: WordPress");
        for title in ["F", "G"] {
            let no = add(&mut conn, title, "WordPress");
            offers::set_interest(&conn, no, Some(Interest::No)).unwrap();
        }
        refresh(&mut conn).unwrap();
        assert!(learned(&conn, id).0 < before);
        offers::set_interest(&conn, other, Some(Interest::High)).unwrap();
        refresh(&mut conn).unwrap();
        assert_eq!(learned(&conn, other).0, 0);
    }

    #[test]
    fn today_is_ordered_by_the_corrected_score() {
        let mut conn = test_db();
        rated(&mut conn);
        let plain = add(&mut conn, "Księgowa", "Biuro rachunkowe");
        let liked = add(&mut conn, "Frontend", "WordPress i React");
        conn.execute("UPDATE offer_sources SET source = 'adzuna' WHERE offer_id IN (?1, ?2)", [plain, liked]).unwrap();
        conn.execute("UPDATE offers SET score = 60, score_source = 'ai' WHERE id = ?1", [plain]).unwrap();
        conn.execute("UPDATE offers SET score = 58, score_source = 'ai' WHERE id = ?1", [liked]).unwrap();
        let order = |conn: &Connection| offers::list(conn, View::Today, None).unwrap().iter().map(|o| o.id).collect::<Vec<_>>();
        let position = |list: &[i64], id: i64| list.iter().position(|x| *x == id).unwrap();
        let before = order(&conn);
        assert!(position(&before, plain) < position(&before, liked));
        refresh(&mut conn).unwrap();
        let after = order(&conn);
        assert!(position(&after, liked) < position(&after, plain));
    }

    #[test]
    fn job_names_come_in_both_languages_and_whole_words_only() {
        let terms = terms();
        let names = |text: &str| find(&terms, text, "", "").into_iter().map(|(_, n)| n).collect::<Vec<_>>();
        assert_eq!(names("Programista"), vec!["programista"]);
        assert_eq!(names("Senior Developer"), vec!["developer"]);
        assert_eq!(names("Księgowa"), vec!["księgowa"]);
        // Node.js is also JavaScript - its "js" counts for both.
        assert_eq!(names("Node.js i Google Ads"), vec!["JavaScript", "Node.js", "Google Ads"]);
        // "JavaScript" is not "Java", and a word inside another word does not count.
        assert_eq!(names("JavaScript"), vec!["JavaScript"]);
        assert!(names("gitara i reactive").is_empty());
    }

    #[test]
    fn every_term_can_be_found() {
        // `phrases` gives single words and pairs - a longer name would never match.
        for term in terms() {
            for s in &term.spellings {
                assert!(!s.is_empty());
                assert!(s.split(' ').count() <= 2, "{s}");
            }
        }
    }
}
