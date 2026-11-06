//! Jooble - job aggregator with an official REST API (free key of the user, asked for on the
//! Jooble site of the chosen country - the key decides which country's offers come back).
//! The free key allows 500 calls in its whole life, so the app makes at most one call a day.

use reqwest::Client;
use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};
use crate::model::OfferInput;
use crate::net;
use crate::text::{cut, html_to_text, one_line};

use super::Query;
use super::alerts::{parse_place, parse_salary};

pub const SOURCE: &str = "jooble";
pub const KEY_SECRET: &str = "jooble-key";
/// Calls a free key allows in its whole life.
pub const KEY_CALLS: i64 = 500;
const PER_PAGE: u32 = 50;
/// Radius values the API accepts (km).
const RADII: &[u16] = &[0, 4, 8, 16, 26, 40, 80];

#[derive(Debug, Serialize)]
struct Request {
    keywords: String,
    location: String,
    radius: String,
    page: String,
    #[serde(rename = "ResultOnPage")]
    result_on_page: String,
    companysearch: String,
}

#[derive(Debug, Deserialize)]
struct Answer {
    #[serde(default)]
    jobs: Vec<Job>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct Job {
    pub title: Option<String>,
    pub location: Option<String>,
    pub snippet: Option<String>,
    pub salary: Option<String>,
    /// Site the offer comes from, as Jooble names it.
    pub source: Option<String>,
    #[serde(rename = "type")]
    pub kind: Option<String>,
    pub link: Option<String>,
    pub company: Option<String>,
    /// When the offer was last updated at its site, e.g. "2026-11-02T00:00:00.0000000".
    pub updated: Option<String>,
}

pub fn key() -> AppResult<Option<String>> {
    crate::secrets::get(KEY_SECRET)
}

/// The key goes into the address, so only letters, digits and dashes are allowed.
fn valid_key(key: &str) -> bool {
    !key.is_empty() && key.len() <= 128 && key.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
}

/// Smallest allowed radius that covers the wanted one (the largest when none does).
fn radius(km: u16) -> u16 {
    RADII.iter().copied().find(|r| *r >= km).unwrap_or(80)
}

fn request(q: &Query) -> Request {
    Request {
        keywords: q.keyword.clone(),
        // Empty = the whole country of the key.
        location: q.city.clone().unwrap_or_default(),
        radius: if q.city.is_some() { radius(q.radius_km) } else { 0 }.to_string(),
        page: "1".into(),
        result_on_page: PER_PAGE.to_string(),
        companysearch: "false".into(),
    }
}

/// One search (one API call).
/// `sent` becomes true once the request reached Jooble - only then does it use one of the key's
/// 500 calls (no connection or a key with a wrong format costs nothing).
pub async fn search(client: &Client, key: &str, q: &Query, sent: &mut bool) -> AppResult<Vec<OfferInput>> {
    if !valid_key(key) {
        return Err(AppError::Auth("the Jooble key has a wrong format".into()));
    }
    let url = format!("https://jooble.org/api/{key}");
    let resp = client.post(url).json(&request(q)).send().await.map_err(|e| {
        *sent = !e.is_connect();
        net::net_error(e)
    })?;
    *sent = true;
    let answer: Answer = net::json(resp).await?;
    Ok(answer.jobs.into_iter().filter_map(to_offer).collect())
}

pub fn to_offer(job: Job) -> Option<OfferInput> {
    let title = one_line(&html_to_text(job.title.as_deref()?), 300);
    let company = one_line(&html_to_text(&job.company.unwrap_or_default()), 300);
    let url = job.link.filter(|u| u.starts_with("https://") || u.starts_with("http://"));
    if title.is_empty() || company.is_empty() {
        return None;
    }
    let place = job.location.as_deref().map(|l| one_line(&html_to_text(l), 200)).filter(|l| !l.is_empty());
    let (city, remote) = place.as_deref().and_then(parse_place).unwrap_or((None, None));
    let salary = job.salary.as_deref().map(|s| one_line(&html_to_text(s), 200)).filter(|s| !s.is_empty());
    let parsed = salary.as_deref().and_then(parse_salary);
    // Short facts above the description, as written by Jooble.
    let mut head = Vec::new();
    if let Some(place) = &place {
        head.push(format!("Location: {place}"));
    }
    if let Some(salary) = &salary {
        head.push(format!("Salary: {salary}"));
    }
    if let Some(kind) = job.kind.map(|k| one_line(&k, 60)).filter(|k| !k.is_empty()) {
        head.push(format!("Job type: {kind}"));
    }
    if let Some(site) = job.source.map(|s| one_line(&s, 100)).filter(|s| !s.is_empty()) {
        head.push(format!("Found by Jooble on: {site}"));
    }
    // Jooble gives only a short piece of the description; the full text is on the linked page.
    let body = job.snippet.map(|s| html_to_text(&s)).unwrap_or_default();
    let description = format!("{}\n\n{}", head.join("\n"), body);
    Some(OfferInput {
        title,
        company,
        source: SOURCE.into(),
        url,
        city: city.or_else(|| place.filter(|_| remote.is_none()).map(|p| p.split(',').next().unwrap_or(&p).trim().to_string())),
        remote,
        salary_min: parsed.as_ref().map(|p| p.0),
        salary_max: parsed.as_ref().and_then(|p| p.1),
        salary_currency: parsed.as_ref().map(|p| p.2.clone()),
        salary_period: parsed.as_ref().map(|p| p.3),
        description: Some(cut(description.trim(), 50_000)).filter(|d| !d.is_empty()),
        published_at: job.updated,
        ..Default::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Remote, SalaryPeriod};

    // Shape of the answer from the Jooble documentation; companies and offers are made up.
    const SAMPLE: &str = r#"{
      "totalCount": 3,
      "jobs": [
        {
          "id": 1234567890,
          "title": "Programista <b>WordPress</b>",
          "location": "Wrocław, dolnośląskie",
          "snippet": "Szukamy osoby do rozwijania stron&nbsp;na WordPressie &amp; WooCommerce…",
          "salary": "12 000 - 16 000 PLN",
          "source": "przyklad-ofert.pl",
          "type": "Pełny etat",
          "link": "https://pl.jooble.org/jdp/1234567890",
          "company": "Przykładowa Firma Sp. z o.o.",
          "updated": "2026-11-01T08:00:00.0000000"
        },
        {
          "id": 2, "title": "PHP Developer", "location": "Praca zdalna", "salary": "",
          "link": "javascript:alert(1)", "company": "Firma Zmyślona"
        },
        { "id": 3, "title": "Bez firmy", "company": "" }
      ]
    }"#;

    #[test]
    fn answer_becomes_offers() {
        let answer: Answer = serde_json::from_str(SAMPLE).unwrap();
        let offers: Vec<OfferInput> = answer.jobs.into_iter().filter_map(to_offer).collect();
        assert_eq!(offers.len(), 2);
        let a = &offers[0];
        assert_eq!(a.title, "Programista WordPress");
        assert_eq!(a.company, "Przykładowa Firma Sp. z o.o.");
        assert_eq!(a.source, "jooble");
        assert_eq!(a.city.as_deref(), Some("Wrocław"));
        assert_eq!((a.salary_min, a.salary_max), (Some(12000), Some(16000)));
        assert_eq!(a.salary_currency.as_deref(), Some("PLN"));
        assert_eq!(a.salary_period, Some(SalaryPeriod::Month));
        assert_eq!(a.url.as_deref(), Some("https://pl.jooble.org/jdp/1234567890"));
        let d = a.description.as_deref().unwrap();
        assert!(d.starts_with("Location: Wrocław, dolnośląskie\nSalary: 12 000 - 16 000 PLN\nJob type: Pełny etat\nFound by Jooble on: przyklad-ofert.pl"));
        assert!(d.contains("Szukamy osoby do rozwijania stron na WordPressie & WooCommerce"));

        // Remote work is read from the place; a bad link is dropped.
        let b = &offers[1];
        assert_eq!(b.remote, Some(Remote::Yes));
        assert_eq!(b.city, None);
        assert_eq!(b.url, None);
        assert_eq!(b.salary_min, None);
    }

    #[test]
    fn request_and_key() {
        let q = Query { keyword: "WordPress".into(), city: Some("Opole".into()), radius_km: 30 };
        let body = serde_json::to_value(request(&q)).unwrap();
        assert_eq!(body["keywords"], "WordPress");
        assert_eq!(body["location"], "Opole");
        assert_eq!(body["radius"], "40");
        assert_eq!(body["ResultOnPage"], "50");
        let whole = Query { keyword: "PHP".into(), city: None, radius_km: 0 };
        assert_eq!(serde_json::to_value(request(&whole)).unwrap()["location"], "");
        assert_eq!(radius(200), 80);
        assert!(valid_key("ab12-cd34"));
        assert!(!valid_key("ab/../x"));
        assert!(!valid_key(""));
    }
}
