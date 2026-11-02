//! Match score without AI ("ocena bez AI"): how many words of the profile appear in the offer.
//! Instant, free and always the same. AI scores replace it when AI is available.

use std::collections::BTreeSet;

use crate::dedup::normalize;
use crate::filter::Facts;
use crate::model::Profile;

/// Short words that say nothing about a job (Polish and English).
const STOP: &[&str] = &[
    "oraz", "albo", "jako", "przy", "bardzo", "ktora", "ktore", "ktory", "jest", "beda", "bede", "chce",
    "chcialbym", "chcialabym", "szukam", "pracy", "praca", "prace", "najlepiej", "moze", "mozliwie", "tylko",
    "takze", "rowniez", "gdzie", "lubie", "mnie", "mojej", "moje", "swoje", "nie", "bez", "dla", "the", "and",
    "with", "for", "that", "this", "from", "looking", "work", "job", "want", "would", "like", "prefer",
    "lata", "latach", "roku", "years", "year", "firmie", "firma", "company", "zespole", "team", "osoba",
];

/// Words worth looking for: profile keywords (weight 2) and longer words of the description.
fn words(profile: &Profile) -> (Vec<String>, Vec<String>) {
    let keywords: Vec<String> = profile
        .criteria
        .keywords
        .iter()
        .map(|k| normalize(k))
        .filter(|k| !k.is_empty())
        .collect();
    let mut seen: BTreeSet<String> = keywords.iter().cloned().collect();
    let mut described = Vec::new();
    for w in normalize(&profile.description).split(' ') {
        if w.chars().count() >= 4 && !STOP.contains(&w) && !w.chars().all(|c| c.is_ascii_digit()) && seen.insert(w.to_string()) {
            described.push(w.to_string());
        }
    }
    (keywords, described)
}

/// 0-100, or `None` when the profile gives nothing to look for.
pub fn keyword_score(facts: &Facts, profile: &Profile) -> Option<i64> {
    let (keywords, described) = words(profile);
    if keywords.is_empty() && described.is_empty() {
        return None;
    }
    let text = format!(
        " {} ",
        normalize(&[
            facts.title.as_str(),
            facts.company.as_str(),
            facts.requirements.as_deref().unwrap_or(""),
            facts.description.as_deref().unwrap_or(""),
        ]
        .join(" "))
    );
    let has = |w: &str| text.contains(&format!(" {w} "));
    let kw_part = (!keywords.is_empty()).then(|| keywords.iter().filter(|k| has(k)).count() as f64 / keywords.len() as f64);
    // A description has many words that never appear in offers - a third of them is a full match.
    let desc_part = (!described.is_empty()).then(|| {
        let hits = described.iter().filter(|w| has(w)).count() as f64;
        (hits / (described.len() as f64 / 3.0).max(2.0)).min(1.0)
    });
    let score = match (kw_part, desc_part) {
        (Some(k), Some(d)) => 0.65 * k + 0.35 * d,
        (Some(k), None) => k,
        (None, Some(d)) => d,
        (None, None) => return None,
    };
    Some((score * 100.0).round().clamp(0.0, 100.0) as i64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Criteria;

    fn profile(keywords: &[&str], description: &str) -> Profile {
        Profile {
            id: 1,
            name: "Plan".into(),
            description: description.into(),
            position: 0,
            description_prev: None,
            criteria: Criteria { keywords: keywords.iter().map(|s| s.to_string()).collect(), ..Default::default() },
        }
    }

    fn facts(title: &str, description: &str) -> Facts {
        Facts { title: title.into(), company: "Firma".into(), description: Some(description.into()), ..Default::default() }
    }

    #[test]
    fn more_matching_words_mean_a_higher_score() {
        let p = profile(&["React", "TypeScript"], "Szukam pracy jako frontend developer, najlepiej zdalnie, mały zespół.");
        let good = keyword_score(&facts("Frontend Developer", "React, TypeScript, praca zdalna, mały zespół"), &p).unwrap();
        let half = keyword_score(&facts("Frontend Developer", "Angular i TypeScript"), &p).unwrap();
        let none = keyword_score(&facts("Księgowa", "Biuro rachunkowe, Excel"), &p).unwrap();
        assert!(good > half && half > none, "{good} {half} {none}");
        assert!(good >= 80);
        assert_eq!(none, 0);
    }

    #[test]
    fn whole_words_only_and_nothing_to_look_for() {
        let p = profile(&["Java"], "");
        assert_eq!(keyword_score(&facts("JavaScript Developer", "JavaScript"), &p), Some(0));
        assert_eq!(keyword_score(&facts("Dev", "x"), &profile(&[], "dla mnie")), None);
    }
}
