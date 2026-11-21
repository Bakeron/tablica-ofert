//! The data an offer page gives search engines - schema.org `JobPosting` in a
//! `<script type="application/ld+json">`: when the offer was published, until when it is open,
//! the company and the salary. The portal writes them itself, so they come before what AI or
//! plain code read from the page's text (Useme says "Ważne przez 30 dni" in the text and
//! `"validThrough": "2026-12-04"` here). Checked field by field like any outside data.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use time::format_description::well_known::Rfc3339;
use time::{Date, Month, OffsetDateTime};

use crate::dates::{on_day, Kind};
use crate::extract::Draft;
use crate::fieldcheck::{Field, FieldNote};
use crate::model::SalaryPeriod;
use crate::text::one_line;

/// The preview script sends at most this many scripts, each at most this many characters
/// (all of it goes back in one address with the page's text).
pub const MAX_SCRIPTS: usize = 2;
pub const MAX_SCRIPT: usize = 20_000;
/// How deep a `JobPosting` may sit inside the script's JSON (`@graph`, lists).
const MAX_DEPTH: usize = 4;
const MAX_AMOUNT: f64 = 10_000_000.0;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Posting {
    /// RFC 3339, like the form's dates (published = midnight, open until = the end of the day).
    pub published_at: Option<String>,
    pub expires_at: Option<String>,
    pub company: Option<String>,
    pub salary_min: Option<i64>,
    pub salary_max: Option<i64>,
    pub salary_currency: Option<String>,
    pub salary_period: Option<SalaryPeriod>,
}

impl Posting {
    fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    /// Checked again when it comes back from the UI with "Fill in the fields".
    pub fn checked(self) -> Self {
        let date = |d: Option<String>, kind| d.and_then(|d| OffsetDateTime::parse(&d, &Rfc3339).ok()).and_then(|t| day(&t.date().to_string(), kind));
        let amount = |a: Option<i64>| a.filter(|a| (1..=MAX_AMOUNT as i64).contains(a));
        let (salary_min, salary_max) = (amount(self.salary_min), amount(self.salary_max));
        let has_salary = salary_min.is_some() || salary_max.is_some();
        Self {
            published_at: date(self.published_at, Kind::Published),
            expires_at: date(self.expires_at, Kind::Expires),
            company: self.company.as_deref().and_then(company_name),
            salary_min,
            salary_max,
            salary_currency: self.salary_currency.as_deref().and_then(currency).filter(|_| has_salary),
            salary_period: self.salary_period.filter(|_| has_salary),
        }
    }

    /// Puts the page's values into the fields and gives them the note "from the page's data"
    /// instead of the notes about the text.
    pub fn apply(&self, d: &mut Draft) {
        let mut from_page = Vec::new();
        if let Some(p) = &self.published_at {
            d.published_at = Some(p.clone());
            from_page.push(Field::Published);
        }
        if let Some(e) = &self.expires_at {
            d.expires_at = Some(e.clone());
            from_page.push(Field::Expires);
        }
        if let Some(c) = &self.company {
            d.company = Some(c.clone());
            from_page.push(Field::Company);
        }
        if self.salary_min.is_some() || self.salary_max.is_some() {
            d.salary_min = self.salary_min;
            d.salary_max = self.salary_max;
            d.salary_currency = self.salary_currency.clone().or(Some("PLN".into()));
            d.salary_period = self.salary_period;
            from_page.push(Field::Salary);
        }
        d.notes.retain(|n| !from_page.contains(&n.field));
        d.notes.extend(from_page.into_iter().map(|field| FieldNote { field, problem: None, fragment: None, more: None, from_page: true }));
    }
}

/// The first `JobPosting` of the page's scripts, `None` when there is none or it says nothing.
pub fn of_scripts(scripts: &[String]) -> Option<Posting> {
    scripts
        .iter()
        .take(MAX_SCRIPTS)
        .filter(|s| s.chars().count() <= MAX_SCRIPT)
        .filter_map(|s| serde_json::from_str::<Value>(s).ok())
        .find_map(|v| find_posting(&v, 0).map(read))
        .filter(|p| !p.is_empty())
}

fn is_posting(v: &Value) -> bool {
    match v.get("@type") {
        Some(Value::String(t)) => t == "JobPosting",
        Some(Value::Array(types)) => types.iter().any(|t| t.as_str() == Some("JobPosting")),
        _ => false,
    }
}

fn find_posting(v: &Value, depth: usize) -> Option<&Value> {
    if depth > MAX_DEPTH {
        return None;
    }
    match v {
        Value::Object(_) if is_posting(v) => Some(v),
        Value::Object(o) => o.get("@graph").and_then(|g| find_posting(g, depth + 1)),
        Value::Array(items) => items.iter().find_map(|i| find_posting(i, depth + 1)),
        _ => None,
    }
}

fn read(v: &Value) -> Posting {
    let text = |key: &str| v.get(key).and_then(Value::as_str);
    let company = match v.get("hiringOrganization") {
        Some(Value::String(name)) => company_name(name),
        Some(org) => org.get("name").and_then(Value::as_str).and_then(company_name),
        None => None,
    };
    let (salary_min, salary_max, salary_currency, salary_period) = v.get("baseSalary").and_then(salary).unwrap_or_default();
    Posting {
        published_at: text("datePosted").and_then(|d| day(d, Kind::Published)),
        expires_at: text("validThrough").and_then(|d| day(d, Kind::Expires)),
        company,
        salary_min,
        salary_max,
        salary_currency,
        salary_period,
    }
}

/// "2026-11-04" or "2026-11-04T10:00:00+01:00" - the day, as the form keeps it.
fn day(value: &str, kind: Kind) -> Option<String> {
    let mut parts = value.get(..10)?.split('-');
    let year: i32 = parts.next()?.parse().ok()?;
    let month: u8 = parts.next()?.parse().ok()?;
    let day: u8 = parts.next()?.parse().ok()?;
    if !(2000..=2100).contains(&year) {
        return None;
    }
    let date = Date::from_calendar_date(year, Month::try_from(month).ok()?, day).ok()?;
    on_day(date, kind).format(&Rfc3339).ok()
}

fn company_name(name: &str) -> Option<String> {
    let name = one_line(name, 300);
    (name.chars().any(char::is_alphanumeric)).then_some(name)
}

fn currency(code: &str) -> Option<String> {
    let code = code.trim().to_ascii_uppercase();
    (code.len() == 3 && code.bytes().all(|b| b.is_ascii_uppercase())).then_some(code)
}

fn number(v: &Value) -> Option<i64> {
    let n = v.as_f64().or_else(|| v.as_str().and_then(|t| t.replace([' ', '\u{a0}'], "").replace(',', ".").parse().ok()))?;
    (n.is_finite() && (1.0..=MAX_AMOUNT).contains(&n)).then(|| n.trunc() as i64)
}

type Salary = (Option<i64>, Option<i64>, Option<String>, Option<SalaryPeriod>);

/// `MonetaryAmount`: a number, or `QuantitativeValue` with `minValue` / `maxValue` / `value` and
/// `unitText`. A year or a week has no field of its own - such a salary is left to the text.
fn salary(v: &Value) -> Option<Salary> {
    let amount = v.get("value")?;
    let (min, max, unit) = if amount.is_object() {
        let one = amount.get("value").and_then(number);
        let min = amount.get("minValue").and_then(number).or(one);
        let max = amount.get("maxValue").and_then(number).or(one);
        (min, max, amount.get("unitText").and_then(Value::as_str))
    } else {
        let one = number(amount);
        (one, one, v.get("unitText").and_then(Value::as_str))
    };
    let period = match unit.map(|u| u.trim().to_ascii_uppercase()) {
        Some(u) if u == "MONTH" => Some(SalaryPeriod::Month),
        Some(u) if u == "DAY" => Some(SalaryPeriod::Day),
        Some(u) if u == "HOUR" => Some(SalaryPeriod::Hour),
        Some(_) => return None,
        None => None,
    };
    let (min, max) = match (min, max) {
        (Some(a), Some(b)) if a > b => (Some(b), Some(a)),
        other => other,
    };
    if min.is_none() && max.is_none() {
        return None;
    }
    let currency = v.get("currency").and_then(Value::as_str).and_then(currency);
    Some((min, max, currency, period))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn of(json: &str) -> Option<Posting> {
        of_scripts(&[json.to_string()])
    }

    #[test]
    fn useme_job_posting() {
        // As on a Useme job page (21.11.2026), the job made up.
        let json = r#"{"@context": "https://schema.org", "@type": "JobPosting", "datePosted": "2026-11-04",
            "employmentType": ["CONTRACTOR", "TEMPORARY"], "hiringOrganization": {"@type": "Organization", "name": "Zmyślone Studio"},
            "jobLocationType": "TELECOMMUTE", "title": "Programista WordPress z AI", "validThrough": "2026-12-04"}"#;
        let p = of(json).unwrap();
        assert_eq!(p.published_at.as_deref(), Some("2026-11-04T00:00:00Z"));
        assert_eq!(p.expires_at.as_deref(), Some("2026-12-04T21:59:59Z"));
        assert_eq!(p.company.as_deref(), Some("Zmyślone Studio"));
        assert_eq!((p.salary_min, p.salary_max), (None, None));
    }

    #[test]
    fn salary_in_its_forms() {
        let range = r#"{"@type":"JobPosting","baseSalary":{"@type":"MonetaryAmount","currency":"pln",
            "value":{"@type":"QuantitativeValue","minValue":"12000","maxValue":9000,"unitText":"MONTH"}}}"#;
        let p = of(range).unwrap();
        assert_eq!((p.salary_min, p.salary_max), (Some(9000), Some(12000)));
        assert_eq!(p.salary_currency.as_deref(), Some("PLN"));
        assert_eq!(p.salary_period, Some(SalaryPeriod::Month));
        let one = r#"{"@type":"JobPosting","baseSalary":{"currency":"EUR","value":{"value":45.5,"unitText":"HOUR"}}}"#;
        let p = of(one).unwrap();
        assert_eq!((p.salary_min, p.salary_max, p.salary_period), (Some(45), Some(45), Some(SalaryPeriod::Hour)));
        // A year has no field - left to the text.
        assert_eq!(of(r#"{"@type":"JobPosting","baseSalary":{"currency":"PLN","value":{"value":120000,"unitText":"YEAR"}}}"#), None);
    }

    #[test]
    fn the_posting_inside_a_graph_or_a_list() {
        let graph = r#"{"@context":"https://schema.org","@graph":[{"@type":"BreadcrumbList"},{"@type":["JobPosting"],"datePosted":"2026-11-10T08:30:00+01:00"}]}"#;
        assert_eq!(of(graph).unwrap().published_at.as_deref(), Some("2026-11-10T00:00:00Z"));
        let list = r#"[{"@type":"Organization","name":"Portal"},{"@type":"JobPosting","hiringOrganization":"Przykładowa Firma"}]"#;
        assert_eq!(of(list).unwrap().company.as_deref(), Some("Przykładowa Firma"));
    }

    #[test]
    fn bad_values_go() {
        assert_eq!(of(r#"{"@type":"Organization","name":"Portal","datePosted":"2026-11-04"}"#), None);
        assert_eq!(of("not json"), None);
        assert_eq!(of(r#"{"@type":"JobPosting","datePosted":"1999-01-01","validThrough":"2026-02-30","hiringOrganization":{"name":"  -  "}}"#), None);
        let p = of(r#"{"@type":"JobPosting","hiringOrganization":{"name":"Firma\nz nową linią"},"baseSalary":{"currency":"zł!","value":-5}}"#).unwrap();
        assert_eq!(p.company.as_deref(), Some("Firma z nową linią"));
        assert_eq!((p.salary_min, p.salary_currency), (None, None));
        // Too long a script is not read at all.
        let long = format!(r#"{{"@type":"JobPosting","datePosted":"2026-11-04","x":"{}"}}"#, "a".repeat(MAX_SCRIPT));
        assert_eq!(of(&long), None);
    }

    #[test]
    fn values_coming_back_from_the_ui_are_checked_again() {
        let p = Posting {
            published_at: Some("2026-11-04T10:00:00Z".into()),
            expires_at: Some("jutro".into()),
            company: Some("Firma\tA".into()),
            salary_min: Some(0),
            salary_max: Some(9000),
            salary_currency: Some("pln".into()),
            salary_period: Some(SalaryPeriod::Month),
        }
        .checked();
        assert_eq!(p.published_at.as_deref(), Some("2026-11-04T00:00:00Z"));
        assert_eq!(p.expires_at, None);
        assert_eq!(p.company.as_deref(), Some("Firma A"));
        assert_eq!((p.salary_min, p.salary_max, p.salary_currency.as_deref()), (None, Some(9000), Some("PLN")));
    }

    #[test]
    fn the_page_values_replace_the_read_ones_and_their_notes() {
        let mut d = Draft {
            company: Some("Inna Firma".into()),
            salary_min: Some(3000),
            salary_currency: Some("PLN".into()),
            salary_period: Some(SalaryPeriod::Month),
            expires_at: Some("2026-12-21T21:59:59Z".into()),
            notes: vec![FieldNote { field: Field::Expires, problem: None, fragment: Some("Ważne przez 30 dni".into()), more: None, from_page: false }],
            ..Default::default()
        };
        let p = of(r#"{"@type":"JobPosting","validThrough":"2026-12-04","hiringOrganization":{"name":"Zmyślone Studio"}}"#).unwrap();
        p.apply(&mut d);
        assert_eq!(d.expires_at.as_deref(), Some("2026-12-04T21:59:59Z"));
        assert_eq!(d.company.as_deref(), Some("Zmyślone Studio"));
        // No salary on the page - what was read stays.
        assert_eq!(d.salary_min, Some(3000));
        assert_eq!(d.notes.len(), 2);
        assert!(d.notes.iter().all(|n| n.from_page && n.fragment.is_none()));
    }
}
