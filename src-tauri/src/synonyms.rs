//! Polish and English names of the same job or skill. A profile written in Polish
//! ("programista", "tester") also finds offers in English ("developer", "QA") - from
//! We Work Remotely, Remote OK and other English sources - and the other way round.
//! Kept short on purpose: only words that mean the same thing in job offers.

use crate::dedup::normalize;

/// Groups of normalized phrases (lowercase, no Polish letters) that mean the same.
pub(crate) const GROUPS: &[&[&str]] = &[
    &["programista", "programistka", "developer", "deweloper", "programmer"],
    &["tester", "testerka", "qa", "quality assurance"],
    &["testy automatyczne", "test automation", "automated testing"],
    &["grafik", "graphic designer"],
    &["projektant", "projektantka", "designer"],
    &["projektant ux", "ux designer"],
    &["projektant ui", "ui designer"],
    &["analityk", "analityczka", "analyst"],
    &["analityk danych", "data analyst"],
    &["administrator", "admin", "sysadmin"],
    &["ksiegowy", "ksiegowa", "accountant"],
    &["kierownik projektu", "project manager"],
    &["kierownik", "manager"],
    &["specjalista", "specjalistka", "specialist"],
    &["inzynier", "engineer"],
    &["obsluga klienta", "customer service", "customer support"],
    &["wsparcie techniczne", "wsparcie it", "technical support", "it support", "helpdesk"],
    &["handlowiec", "sprzedawca", "sales"],
    &["rekruter", "rekruterka", "recruiter"],
    &["tlumacz", "tlumaczka", "translator"],
    &["strony internetowe", "strony www", "websites", "web development"],
    &["aplikacje mobilne", "mobile apps", "mobile development"],
    &["sztuczna inteligencja", "artificial intelligence", "ai"],
    &["uczenie maszynowe", "machine learning"],
    &["baza danych", "bazy danych", "database", "databases"],
    &["wtyczki", "plugins"],
    &["sklep internetowy", "e commerce", "ecommerce", "online shop"],
    &["frontend", "front end"],
    &["backend", "back end"],
    &["fullstack", "full stack"],
];

/// The word itself (normalized) and its other names, if it has any.
pub fn variants(word: &str) -> Vec<String> {
    let n = normalize(word);
    match GROUPS.iter().find(|g| g.contains(&n.as_str())) {
        Some(group) => group.iter().map(|w| w.to_string()).collect(),
        None => vec![n],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn polish_and_english_names_find_each_other() {
        assert!(variants("Programista").contains(&"developer".to_string()));
        assert!(variants("developer").contains(&"programista".to_string()));
        assert!(variants("Tester").contains(&"qa".to_string()));
        assert!(variants("Księgowa").contains(&"accountant".to_string()));
        assert_eq!(variants("WordPress"), vec!["wordpress".to_string()]);
    }

    #[test]
    fn every_group_is_already_normalized_and_appears_once() {
        let mut seen = Vec::new();
        for group in GROUPS {
            for w in *group {
                assert_eq!(normalize(w), *w, "{w}");
                assert!(!seen.contains(w), "{w} is in two groups");
                seen.push(*w);
            }
        }
    }
}
