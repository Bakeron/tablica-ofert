//! Checking the fields of an offer being added against its own text - plain code, no AI.
//! A value that is not in the text gets "check" (AI may have made it up or taken it from
//! a neighbouring offer), an empty field the text clearly fills gets "check" too, and the
//! important fields show the line of the text they come from.

use serde::Serialize;

use crate::ai::number_in_text;
use crate::dates::{self, Kind};
use crate::dedup::normalize;
use crate::extract::{contract_of, Draft};
use crate::model::Remote;
use crate::text::one_line;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Field {
    Title,
    Company,
    City,
    Remote,
    Salary,
    Contract,
    Level,
    Published,
    Expires,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Problem {
    /// The value is not in the offer's text.
    NotInText,
    /// The field is empty, but the text has it (an amount, a contract, a work mode, a level).
    Missing,
    /// The text names more than the field has (`more` - what is missing).
    More,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FieldNote {
    pub field: Field,
    pub problem: Option<Problem>,
    /// The line of the offer the value comes from (or the line the empty field should take).
    pub fragment: Option<String>,
    pub more: Option<String>,
    /// The value comes from the data the page gives search engines (`posting.rs`), not its text.
    pub from_page: bool,
}

const FRAGMENT: usize = 200;

/// Lines of the text, each with its normalized form wrapped in spaces (whole-word search).
struct Text {
    lines: Vec<(String, String)>,
    all: String,
}

impl Text {
    fn new(text: &str) -> Self {
        let lines: Vec<(String, String)> = text
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .map(|l| (l.to_string(), format!(" {} ", normalize(l))))
            .collect();
        Text { all: format!(" {} ", normalize(text)), lines }
    }

    fn line(&self, found: impl Fn(&str, &str) -> bool) -> Option<String> {
        self.lines.iter().find(|(raw, n)| found(raw, n)).map(|(raw, _)| one_line(raw, FRAGMENT))
    }
}

/// Words that start a word of the text ("krak" finds "Krakowie").
fn starts_word(n: &str, stem: &str) -> bool {
    n.contains(&format!(" {stem}"))
}

/// Polish place names change their ending ("Warszawa" - "w Warszawie"), so a long word is
/// searched without its last two letters.
fn stems(value: &str) -> Vec<String> {
    normalize(value)
        .split(' ')
        .filter(|w| w.chars().count() >= 3)
        .map(|w| {
            let n = w.chars().count();
            if n >= 6 { w.chars().take(n - 2).collect() } else { w.to_string() }
        })
        .collect()
}

const LEGAL: &[&str] = &[
    "sp", "z", "o", "spolka", "ograniczona", "odpowiedzialnoscia", "akcyjna", "komandytowa", "jawna", "sa", "s", "a", "k",
    "inc", "ltd", "llc", "gmbh", "plc", "co",
];

/// A company name without its legal form ("Sp. z o.o.", "S.A.", "Ltd").
fn company_core(company: &str) -> String {
    let n = normalize(company);
    let mut words: Vec<&str> = n.split(' ').collect();
    while words.len() > 1 && words.last().is_some_and(|w| LEGAL.contains(w)) {
        words.pop();
    }
    words.join(" ")
}

const REMOTE_YES: &[&str] = &["zdaln", "remote", "home office"];
const REMOTE_HYBRID: &[&str] = &["hybryd", "hybrid"];
const REMOTE_NO: &[&str] = &["stacjonarn", "on site", "onsite", "w biurze", "z biura", "office"];

fn remote_words(r: Remote) -> &'static [&'static str] {
    match r {
        Remote::Yes => REMOTE_YES,
        Remote::Hybrid => REMOTE_HYBRID,
        Remote::No => REMOTE_NO,
        Remote::Unknown => &[],
    }
}

fn level_words(level: &str) -> &'static [&'static str] {
    match normalize(level).as_str() {
        "junior" => &["junior", "stazyst", "praktykant", "trainee", "intern", "mlodsz"],
        "mid" => &["mid", "regular"],
        "senior" => &["senior", "lead", "starsz", "ekspert", "expert"],
        _ => &[],
    }
}

/// A line with an amount of money: a number and "zł", "PLN", "brutto", "netto"…
fn money_line(n: &str) -> bool {
    n.bytes().any(|b| b.is_ascii_digit())
        && [" zl ", " pln ", " brutto ", " netto ", " eur ", " usd ", " gross ", " net "].iter().any(|w| n.contains(w))
}

fn note(field: Field, problem: Option<Problem>, fragment: Option<String>) -> Option<FieldNote> {
    (problem.is_some() || fragment.is_some()).then_some(FieldNote { field, problem, fragment, more: None, from_page: false })
}

/// Notes for the fields of `d` read from `text` (the offer's text after cutting the rest of
/// the page). Only fields with something to say get a note.
pub fn check(d: &Draft, text: &str) -> Vec<FieldNote> {
    check_at(d, text, time::OffsetDateTime::now_utc())
}

/// `now` - when the text was taken ("2 dni temu" is counted from it).
fn check_at(d: &Draft, text: &str, now: time::OffsetDateTime) -> Vec<FieldNote> {
    let t = Text::new(text);
    let mut notes = Vec::new();

    // Title: every longer word of it is in the text (AI may drop "(k/m)" - that is fine).
    if let Some(title) = d.title.as_deref() {
        let words: Vec<String> = stems(title);
        if !words.is_empty() && !words.iter().all(|w| starts_word(&t.all, w)) {
            notes.extend(note(Field::Title, Some(Problem::NotInText), None));
        }
    }

    if let Some(company) = d.company.as_deref() {
        let core = company_core(company);
        let found = core.chars().count() >= 2 && t.all.contains(&format!(" {core}"));
        let fragment = if found { t.line(|_, n| n.contains(&format!(" {core}"))) } else { None };
        notes.extend(note(Field::Company, (!found).then_some(Problem::NotInText), fragment));
    }

    if let Some(city) = d.city.as_deref() {
        let words = stems(city);
        let found = !words.is_empty() && words.iter().all(|w| starts_word(&t.all, w));
        let fragment = if found { t.line(|_, n| words.iter().all(|w| starts_word(n, w))) } else { None };
        notes.extend(note(Field::City, (!found).then_some(Problem::NotInText), fragment));
    }

    match d.remote.filter(|r| *r != Remote::Unknown) {
        Some(r) => {
            let words = remote_words(r);
            let fragment = t.line(|_, n| words.iter().any(|w| starts_word(n, w)));
            notes.extend(note(Field::Remote, fragment.is_none().then_some(Problem::NotInText), fragment));
        }
        None => {
            let any = [REMOTE_YES, REMOTE_HYBRID, REMOTE_NO].concat();
            let fragment = t.line(|_, n| any.iter().any(|w| starts_word(n, w)));
            notes.extend(note(Field::Remote, fragment.is_some().then_some(Problem::Missing), fragment));
        }
    }

    let amounts: Vec<i64> = [d.salary_min, d.salary_max].into_iter().flatten().collect();
    if amounts.is_empty() {
        let fragment = t.line(|_, n| money_line(n));
        notes.extend(note(Field::Salary, fragment.is_some().then_some(Problem::Missing), fragment));
    } else {
        let found = amounts.iter().all(|a| number_in_text(*a, text));
        let fragment = t.line(|raw, _| amounts.iter().any(|a| number_in_text(*a, raw)));
        notes.extend(note(Field::Salary, (!found).then_some(Problem::NotInText), fragment));
    }

    let in_text: Vec<String> = contract_of(text).map(|c| c.split(", ").map(str::to_string).collect()).unwrap_or_default();
    let contract_line = || t.line(|raw, _| contract_of(raw).is_some());
    match d.contract.as_deref().filter(|c| !c.trim().is_empty()) {
        Some(contract) => {
            let named: Vec<String> = contract_of(contract).map(|c| c.split(", ").map(str::to_string).collect()).unwrap_or_default();
            let found = if named.is_empty() {
                let n = normalize(contract);
                n.chars().count() >= 2 && t.all.contains(&format!(" {n} "))
            } else {
                named.iter().all(|c| in_text.contains(c))
            };
            let missing: Vec<&str> = in_text.iter().filter(|c| !named.contains(c)).map(String::as_str).collect();
            let fragment = contract_line();
            if !found {
                notes.extend(note(Field::Contract, Some(Problem::NotInText), fragment));
            } else if !named.is_empty() && !missing.is_empty() {
                notes.push(FieldNote { field: Field::Contract, problem: Some(Problem::More), fragment, more: Some(missing.join(", ")), from_page: false });
            } else {
                notes.extend(note(Field::Contract, None, fragment));
            }
        }
        None => {
            let fragment = (!in_text.is_empty()).then(contract_line).flatten();
            notes.extend(note(Field::Contract, fragment.is_some().then_some(Problem::Missing), fragment));
        }
    }

    match d.level.as_deref().filter(|l| !l.trim().is_empty()) {
        Some(level) => {
            let words = level_words(level);
            if !words.is_empty() && !words.iter().any(|w| starts_word(&t.all, w)) {
                notes.extend(note(Field::Level, Some(Problem::NotInText), None));
            }
        }
        None => {
            let fragment = t.line(|_, n| [" junior ", " mid ", " regular ", " senior "].iter().any(|w| n.contains(w)));
            notes.extend(note(Field::Level, fragment.is_some().then_some(Problem::Missing), fragment));
        }
    }

    // Dates: the same day must come out of a line of the text; an empty field the text fills - "check".
    for (field, kind, value) in [(Field::Published, Kind::Published, &d.published_at), (Field::Expires, Kind::Expires, &d.expires_at)] {
        match value.as_deref() {
            Some(date) => {
                let fragment = dates::line_of(date, text, kind, now);
                notes.extend(note(field, fragment.is_none().then_some(Problem::NotInText), fragment));
            }
            None => {
                let fragment = dates::find(text, kind, now).map(|(_, line)| line);
                notes.extend(note(field, fragment.is_some().then_some(Problem::Missing), fragment));
            }
        }
    }
    notes
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::SalaryPeriod;

    const PLAIN: &str = include_str!("trim_pages/plain.txt");

    fn problem(notes: &[FieldNote], field: Field) -> Option<Problem> {
        notes.iter().find(|n| n.field == field).and_then(|n| n.problem)
    }

    fn fragment(notes: &[FieldNote], field: Field) -> Option<&str> {
        notes.iter().find(|n| n.field == field).and_then(|n| n.fragment.as_deref())
    }

    fn right() -> Draft {
        Draft {
            title: Some("Specjalista ds. obsługi zamówień".into()),
            company: Some("Przykładowa Hurtownia Sp. z o.o.".into()),
            city: Some("Poznań".into()),
            remote: Some(Remote::Hybrid),
            salary_min: Some(6500),
            salary_max: Some(8000),
            salary_currency: Some("PLN".into()),
            salary_period: Some(SalaryPeriod::Month),
            contract: Some("UoP".into()),
            level: None,
            ..Default::default()
        }
    }

    #[test]
    fn right_fields_have_no_problems_and_show_their_lines() {
        let notes = check(&right(), PLAIN);
        assert!(notes.iter().all(|n| n.problem.is_none()), "{notes:#?}");
        assert_eq!(fragment(&notes, Field::Salary), Some("6 500 - 8 000 zł brutto / mies."));
        assert_eq!(fragment(&notes, Field::City), Some("Poznań · hybrydowo"));
        assert_eq!(fragment(&notes, Field::Remote), Some("Poznań · hybrydowo"));
        assert_eq!(fragment(&notes, Field::Contract), Some("- umowę o pracę"));
        assert_eq!(fragment(&notes, Field::Company), Some("Przykładowa Hurtownia Sp. z o.o."));
    }

    #[test]
    fn made_up_values_are_caught() {
        let d = Draft {
            title: Some("Kierownik magazynu".into()),
            company: Some("Inna Firma S.A.".into()),
            city: Some("Kraków".into()),
            remote: Some(Remote::Yes),
            salary_min: Some(9000),
            contract: Some("B2B".into()),
            level: Some("senior".into()),
            ..right()
        };
        let notes = check(&d, PLAIN);
        for f in [Field::Title, Field::Company, Field::City, Field::Remote, Field::Salary, Field::Contract, Field::Level] {
            assert_eq!(problem(&notes, f), Some(Problem::NotInText), "{f:?}: {notes:#?}");
        }
    }

    #[test]
    fn empty_fields_the_text_fills_are_caught() {
        let d = Draft { title: right().title, company: right().company, ..Default::default() };
        let notes = check(&d, PLAIN);
        assert_eq!(problem(&notes, Field::Salary), Some(Problem::Missing));
        assert_eq!(fragment(&notes, Field::Salary), Some("6 500 - 8 000 zł brutto / mies."));
        assert_eq!(problem(&notes, Field::Contract), Some(Problem::Missing));
        assert_eq!(problem(&notes, Field::Remote), Some(Problem::Missing));
        // No "junior / mid / senior" in this offer - nothing to say about the level.
        assert_eq!(problem(&notes, Field::Level), None);
    }

    #[test]
    fn dates_from_the_text_made_up_and_left_out() {
        let now = time::OffsetDateTime::parse("2026-11-11T10:00:00Z", &time::format_description::well_known::Rfc3339).unwrap();
        let text = "Piekarz
1 dzień temu
Oferta ważna do: 04.12.2026 (zostało 23 dni)";
        let right = Draft {
            published_at: Some("2026-11-10T10:00:00Z".into()),
            expires_at: Some("2026-12-04T21:59:59Z".into()),
            ..Default::default()
        };
        let notes = check_at(&right, text, now);
        assert_eq!(problem(&notes, Field::Published), None);
        assert_eq!(fragment(&notes, Field::Published), Some("1 dzień temu"));
        assert_eq!(fragment(&notes, Field::Expires), Some("Oferta ważna do: 04.12.2026 (zostało 23 dni)"));
        let made_up = Draft { published_at: Some("2026-11-01T00:00:00Z".into()), expires_at: Some("2026-12-31T21:59:59Z".into()), ..Default::default() };
        let notes = check_at(&made_up, text, now);
        assert_eq!(problem(&notes, Field::Published), Some(Problem::NotInText));
        assert_eq!(problem(&notes, Field::Expires), Some(Problem::NotInText));
        let notes = check_at(&Draft::default(), text, now);
        assert_eq!(problem(&notes, Field::Published), Some(Problem::Missing));
        assert_eq!(problem(&notes, Field::Expires), Some(Problem::Missing));
        // No dates in the text - nothing to say.
        assert_eq!(problem(&check_at(&Draft::default(), PLAIN, now), Field::Expires), None);
    }

    #[test]
    fn a_contract_left_out_of_the_field_is_named() {
        let text = "Oferujemy umowę o pracę lub B2B.";
        let d = Draft { contract: Some("B2B".into()), ..Default::default() };
        let n = check(&d, text).into_iter().find(|n| n.field == Field::Contract).unwrap();
        assert_eq!((n.problem, n.more.as_deref()), (Some(Problem::More), Some("UoP")));
    }

    #[test]
    fn city_in_another_case_and_company_without_its_legal_form() {
        let text = "Biuro w Warszawie\nZmyślona Piekarnia zatrudni piekarza";
        let d = Draft {
            city: Some("Warszawa".into()),
            company: Some("Zmyślona Piekarnia Sp. z o.o.".into()),
            ..Default::default()
        };
        let notes = check(&d, text);
        assert_eq!(problem(&notes, Field::City), None);
        assert_eq!(problem(&notes, Field::Company), None);
    }
}
