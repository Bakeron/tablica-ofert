//! Fields of an offer from its plain text (pasted or taken from the preview page) - without AI.
//! Only what can be read for sure; the rest stays empty for the person (or AI) to fill in.

use serde::Serialize;

use crate::dedup::normalize;
use crate::model::SalaryPeriod;
use crate::sources::alerts::{parse_place, parse_salary};
use crate::text::{cut, one_line, squeeze};

/// Fields for the "Add offer" form. `by_ai` - AI filled them (the person should check).
#[derive(Debug, Clone, Default, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Draft {
    pub title: Option<String>,
    pub company: Option<String>,
    pub city: Option<String>,
    pub remote: Option<crate::model::Remote>,
    pub salary_min: Option<i64>,
    pub salary_max: Option<i64>,
    pub salary_currency: Option<String>,
    pub salary_period: Option<SalaryPeriod>,
    pub contract: Option<String>,
    pub level: Option<String>,
    pub requirements: Option<String>,
    pub description: Option<String>,
    pub by_ai: bool,
}

fn contract_of(text: &str) -> Option<String> {
    let n = format!(" {} ", normalize(text));
    let mut found = Vec::new();
    // Polish cases: "umowa / umowę / umowy / umowie o pracę".
    let uop = ["umowa", "umowe", "umowy", "umowie"].iter().any(|u| n.contains(&format!(" {u} o prace ")));
    if uop || n.contains(" uop ") || n.contains(" employment contract ") {
        found.push("UoP");
    }
    if n.contains(" b2b ") {
        found.push("B2B");
    }
    if n.contains(" umowa zlecenie ") || n.contains(" umowa zlecenia ") {
        found.push("umowa zlecenie");
    }
    (!found.is_empty()).then(|| found.join(", "))
}

fn level_of(title: &str) -> Option<String> {
    let n = format!(" {} ", normalize(title));
    [("junior", " junior "), ("mid", " mid "), ("mid", " regular "), ("senior", " senior "), ("senior", " lead ")]
        .iter()
        .find(|(_, w)| n.contains(w))
        .map(|(l, _)| l.to_string())
}

/// Plain-code reading of an offer text. `title_hint` - the page's `<h1>` or title.
pub fn from_text(text: &str, title_hint: Option<&str>) -> Draft {
    let text = squeeze(text);
    let lines: Vec<&str> = text.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
    let title = title_hint
        .map(|h| one_line(h, 300))
        .filter(|h| h.chars().count() >= 3)
        .or_else(|| lines.iter().find(|l| (3..=150).contains(&l.chars().count())).map(|l| one_line(l, 300)));
    let mut d = Draft { title: title.clone(), ..Default::default() };
    for line in &lines {
        if d.salary_min.is_none() {
            if let Some((min, max, currency, period)) = parse_salary(line) {
                d.salary_min = Some(min);
                d.salary_max = max;
                d.salary_currency = Some(currency);
                d.salary_period = Some(period);
                continue;
            }
        }
        if d.city.is_none() || d.remote.is_none() {
            if let Some((city, remote)) = parse_place(line) {
                d.city = d.city.or(city);
                d.remote = d.remote.or(remote);
            }
        }
    }
    d.contract = contract_of(&text);
    d.level = title.as_deref().and_then(level_of);
    d.description = (!text.is_empty()).then(|| cut(&text, 60_000));
    d
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Remote;

    #[test]
    fn reads_what_is_sure() {
        let text = "Senior Frontend Developer\nPrzykładowa Firma\nWrocław · hybrydowo\n18 000 - 24 000 zł netto (B2B)\n\nOferujemy umowę o pracę lub B2B.";
        let d = from_text(text, None);
        assert_eq!(d.title.as_deref(), Some("Senior Frontend Developer"));
        assert_eq!(d.city.as_deref(), Some("Wrocław"));
        assert_eq!(d.remote, Some(Remote::Hybrid));
        assert_eq!((d.salary_min, d.salary_max), (Some(18000), Some(24000)));
        assert_eq!(d.contract.as_deref(), Some("UoP, B2B"));
        assert_eq!(d.level.as_deref(), Some("senior"));
        // A company cannot be told apart from other lines for sure - left to the person or AI.
        assert_eq!(d.company, None);
        assert!(!d.by_ai);
    }

    #[test]
    fn page_heading_wins_and_empty_text_is_fine() {
        let d = from_text("Menu\nLogowanie\nOpis stanowiska", Some("Tester manualny (k/m)"));
        assert_eq!(d.title.as_deref(), Some("Tester manualny (k/m)"));
        assert_eq!(from_text("", None), Draft::default());
    }
}
