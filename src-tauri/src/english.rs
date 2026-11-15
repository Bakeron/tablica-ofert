//! "English required?" for the list's filter: read from the offer's own words, without AI.
//! An ad written in English needs English; otherwise the line that names the language decides -
//! with "nice to have" words it is a plus, without them a requirement.

use crate::filter::{offer_language, Facts};
use crate::model::{EnglishNeed, OfferLanguage};

/// Words that name English (lower case).
const ENGLISH: [&str; 2] = ["angielsk", "english"];

/// Words that make a requirement only a plus.
const NICE: [&str; 11] = [
    "mile widzian",
    "atut",
    "dodatkowy plus",
    "będzie plus",
    "będzie dodatkow",
    "nice to have",
    "nice-to-have",
    "is a plus",
    "would be a plus",
    "advantage",
    "preferred",
];

/// From the offer's language (its field, else its text) and the words of title, requirements and description.
pub fn need(f: &Facts) -> EnglishNeed {
    if offer_language(f) == Some(OfferLanguage::En) {
        return EnglishNeed::Required;
    }
    let text = [Some(f.title.as_str()), f.requirements.as_deref(), f.description.as_deref()].into_iter().flatten().collect::<Vec<_>>().join("\n");
    in_text(&text)
}

/// The line that names English decides: with "nice to have" words a plus, without them a requirement.
fn in_text(text: &str) -> EnglishNeed {
    let mut found = EnglishNeed::Unnamed;
    // A bullet list has a line each; a long paragraph is cut at sentence ends too.
    for part in text.split(['\n', '.', ';', '•']).map(str::to_lowercase) {
        if !ENGLISH.iter().any(|w| part.contains(w)) {
            continue;
        }
        if NICE.iter().any(|w| part.contains(w)) {
            found = EnglishNeed::Nice;
        } else {
            return EnglishNeed::Required;
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    fn offer(language: Option<&str>, requirements: &str) -> Facts {
        Facts {
            title: "Frontend Developer".into(),
            company: "Przystań Tech".into(),
            city: None,
            remote: None,
            salary_min: None,
            salary_max: None,
            salary_currency: None,
            salary_period: None,
            contract: None,
            level: None,
            language: language.map(Into::into),
            requirements: Some(requirements.into()),
            description: None,
            sources: Vec::new(),
        }
    }

    #[test]
    fn an_english_ad_needs_english() {
        assert_eq!(need(&offer(Some("en"), "React")), EnglishNeed::Required);
    }

    #[test]
    fn a_line_naming_english_is_a_requirement() {
        assert_eq!(need(&offer(Some("pl"), "Wymagania:\n- React\n- znajomość języka angielskiego na poziomie B2")), EnglishNeed::Required);
        assert_eq!(in_text("Good English (B2+)"), EnglishNeed::Required);
    }

    #[test]
    fn nice_to_have_is_only_a_plus() {
        assert_eq!(need(&offer(Some("pl"), "- TypeScript\n- angielski mile widziany")), EnglishNeed::Nice);
        assert_eq!(in_text("Znajomość j. angielskiego będzie atutem."), EnglishNeed::Nice);
    }

    #[test]
    fn a_requirement_wins_over_a_plus() {
        assert_eq!(in_text("angielski B2\nniemiecki mile widziany, angielski C1 będzie atutem"), EnglishNeed::Required);
    }

    #[test]
    fn no_word_for_english_no_need() {
        assert_eq!(need(&offer(Some("pl"), "React, TypeScript")), EnglishNeed::Unnamed);
        assert_eq!(in_text("Bangalore office"), EnglishNeed::Unnamed);
    }
}
