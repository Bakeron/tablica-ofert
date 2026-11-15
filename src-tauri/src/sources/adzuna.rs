//! Adzuna - job aggregator with an official API (free key of the user, "personal job search").
//! Their terms: every offer shows that it comes from Adzuna (the UI shows "Jobs by Adzuna").

use reqwest::Client;
use serde::Deserialize;

use crate::error::AppResult;
use crate::model::{OfferInput, SalaryPeriod};
use crate::net;
use crate::text::{cut, html_to_text, one_line};

use super::Query;

pub const SOURCE: &str = "adzuna";
/// Results on one page (the API maximum is 50).
const PER_PAGE: u32 = 50;

#[derive(Debug, Deserialize)]
struct Answer {
    #[serde(default)]
    results: Vec<Job>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct Job {
    pub title: Option<String>,
    pub description: Option<String>,
    pub redirect_url: Option<String>,
    pub company: Option<Named>,
    pub location: Option<Location>,
    pub salary_min: Option<f64>,
    pub salary_max: Option<f64>,
    /// "1" (or 1) = Adzuna estimated the salary - not taken as the offer's range.
    pub salary_is_predicted: serde_json::Value,
    pub contract_type: Option<String>,
    pub contract_time: Option<String>,
    pub created: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct Named {
    pub display_name: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct Location {
    pub display_name: Option<String>,
    pub area: Vec<String>,
}

pub struct Keys {
    pub app_id: String,
    pub app_key: String,
}

pub fn keys() -> AppResult<Option<Keys>> {
    let id = crate::secrets::get("adzuna-app-id")?;
    let key = crate::secrets::get("adzuna-app-key")?;
    Ok(match (id, key) {
        (Some(app_id), Some(app_key)) => Some(Keys { app_id, app_key }),
        _ => None,
    })
}

fn currency(country: &str) -> &'static str {
    match country {
        "pl" => "PLN",
        "gb" => "GBP",
        "us" => "USD",
        "ch" => "CHF",
        "au" => "AUD",
        "br" => "BRL",
        "ca" => "CAD",
        "in" => "INR",
        "mx" => "MXN",
        "nz" => "NZD",
        "sg" => "SGD",
        "za" => "ZAR",
        _ => "EUR",
    }
}

/// One page of results.
pub struct Page {
    pub offers: Vec<OfferInput>,
    /// The page had the most results Adzuna gives - the next page may have more.
    pub full: bool,
}

fn to_page(answer: Answer, country: &str) -> Page {
    let full = answer.results.len() >= PER_PAGE as usize;
    Page { offers: answer.results.into_iter().filter_map(|job| to_offer(job, country)).collect(), full }
}

/// One search (one API call) for one page, counted from 1. `country` is checked by the
/// settings (`ADZUNA_COUNTRIES`).
pub async fn search(client: &Client, keys: &Keys, country: &str, q: &Query, max_days_old: u32, page: u32) -> AppResult<Page> {
    let url = format!("https://api.adzuna.com/v1/api/jobs/{country}/search/{}", page.max(1));
    let mut params: Vec<(&str, String)> = vec![
        ("app_id", keys.app_id.clone()),
        ("app_key", keys.app_key.clone()),
        ("results_per_page", PER_PAGE.to_string()),
        ("what", q.keyword.clone()),
        ("max_days_old", max_days_old.to_string()),
        ("sort_by", "date".into()),
        ("content-type", "application/json".into()),
    ];
    if let Some(city) = &q.city {
        params.push(("where", city.clone()));
        if q.radius_km > 0 {
            params.push(("distance", q.radius_km.to_string()));
        }
    }
    let resp = net::send_retry(client.get(url).query(&params)).await?;
    let answer: Answer = net::json(resp).await?;
    Ok(to_page(answer, country))
}

fn predicted(v: &serde_json::Value) -> bool {
    match v {
        serde_json::Value::String(s) => s == "1",
        serde_json::Value::Number(n) => n.as_i64() == Some(1),
        serde_json::Value::Bool(b) => *b,
        _ => false,
    }
}

/// Adzuna gives yearly amounts in most countries. Very small amounts are hourly rates.
/// For Poland amounts under 60 000 are taken as monthly - the filter must never reject an
/// offer because of a wrong guess, and monthly pay in PLN is below that. Amounts from 300 to
/// 3000 PLN are B2B day rates (justjoin.it offers "1200-1400" a day) - below any monthly pay,
/// so they are a day rate, never taken as a month.
fn salary(value: Option<f64>, country: &str) -> (Option<i64>, Option<SalaryPeriod>) {
    let Some(v) = value.filter(|v| v.is_finite() && *v > 0.0) else { return (None, None) };
    if v < 300.0 {
        return (Some(v.round() as i64), Some(SalaryPeriod::Hour));
    }
    if country == "pl" && v < 3000.0 {
        return (Some(v.round() as i64), Some(SalaryPeriod::Day));
    }
    let yearly_from = if country == "pl" { 60_000.0 } else { 10_000.0 };
    let monthly = if v >= yearly_from { v / 12.0 } else { v };
    (Some(((monthly / 100.0).round() * 100.0) as i64), Some(SalaryPeriod::Month))
}

pub fn to_offer(job: Job, country: &str) -> Option<OfferInput> {
    let title = one_line(&html_to_text(job.title.as_deref()?), 300);
    let company = one_line(&job.company?.display_name.unwrap_or_default(), 300);
    if title.is_empty() || company.is_empty() {
        return None;
    }
    let (salary_min, salary_max, salary_period) = if predicted(&job.salary_is_predicted) {
        (None, None, None)
    } else {
        let (min, period_a) = salary(job.salary_min, country);
        let (max, period_b) = salary(job.salary_max, country);
        (min, max, period_a.or(period_b))
    };
    let has_salary = salary_min.is_some() || salary_max.is_some();
    let city = job.location.as_ref().and_then(|l| {
        // `area` goes from the country down to the town - the last one is the most precise.
        // A region alone ("mazowieckie") is no town: the filter could not measure it.
        let town = |t: &String| !super::alerts::is_region(t);
        l.area
            .last()
            .filter(|_| l.area.len() > 1)
            .filter(|t| town(t))
            .cloned()
            .or_else(|| l.display_name.as_ref().map(|d| d.split(',').next().unwrap_or(d).trim().to_string()).filter(town))
    });
    // Adzuna gives only the start of the description; the full text is on the linked page.
    let mut description = job.description.map(|d| html_to_text(&d)).unwrap_or_default();
    let extra: Vec<&str> = [job.contract_type.as_deref(), job.contract_time.as_deref()].into_iter().flatten().collect();
    if !extra.is_empty() {
        description = format!("{description}\n\n({})", extra.join(", ").replace('_', " "));
    }
    Some(OfferInput {
        title,
        company,
        source: SOURCE.into(),
        url: job.redirect_url.filter(|u| u.starts_with("https://") || u.starts_with("http://")),
        city: city.map(|c| one_line(&c, 300)).filter(|c| !c.is_empty()),
        remote: None,
        salary_min,
        salary_max,
        salary_currency: has_salary.then(|| currency(country).to_string()),
        salary_period: if has_salary { salary_period } else { None },
        description: Some(cut(description.trim(), 50_000)).filter(|d| !d.is_empty()),
        published_at: job.created,
        ..Default::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    // Shape of a real answer; companies and offers are made up.
    const SAMPLE: &str = r#"{
      "count": 3,
      "results": [
        {
          "title": "Frontend <strong>Developer</strong>",
          "description": "Szukamy osoby do zespołu&nbsp;React &amp; TypeScript. Praca zdalna…",
          "redirect_url": "https://www.adzuna.pl/land/ad/1111?se=x",
          "company": { "display_name": "Przykładowa Firma Sp. z o.o." },
          "location": { "display_name": "Wrocław, dolnośląskie", "area": ["Polska", "dolnośląskie", "Wrocław"] },
          "salary_min": 12000, "salary_max": 16000, "salary_is_predicted": "0",
          "contract_type": "permanent", "contract_time": "full_time",
          "created": "2026-11-01T08:00:00Z"
        },
        {
          "title": "Tester", "company": { "display_name": "Firma Zmyślona" },
          "salary_min": 90000, "salary_max": 120000, "salary_is_predicted": 0,
          "location": { "display_name": "Polska", "area": ["Polska"] }
        },
        { "title": "No company", "salary_min": 5000, "salary_is_predicted": "1" }
      ]
    }"#;

    #[test]
    fn answer_becomes_offers() {
        let answer: Answer = serde_json::from_str(SAMPLE).unwrap();
        let offers: Vec<OfferInput> = answer.results.into_iter().filter_map(|j| to_offer(j, "pl")).collect();
        assert_eq!(offers.len(), 2);
        let a = &offers[0];
        assert_eq!(a.title, "Frontend Developer");
        assert_eq!(a.company, "Przykładowa Firma Sp. z o.o.");
        assert_eq!(a.city.as_deref(), Some("Wrocław"));
        assert_eq!((a.salary_min, a.salary_max), (Some(12000), Some(16000)));
        assert_eq!(a.salary_currency.as_deref(), Some("PLN"));
        assert_eq!(a.salary_period, Some(SalaryPeriod::Month));
        assert!(a.description.as_deref().unwrap().starts_with("Szukamy osoby do zespołu React & TypeScript"));
        assert!(a.description.as_deref().unwrap().ends_with("(permanent, full time)"));
        assert_eq!(a.source, "adzuna");

        // Yearly amounts become monthly; a country-only location is not a city.
        let b = &offers[1];
        assert_eq!((b.salary_min, b.salary_max), (Some(7500), Some(10000)));
        assert_eq!(b.city.as_deref(), Some("Polska"));
    }

    #[test]
    fn full_page_means_more_may_follow() {
        let answer: Answer = serde_json::from_str(SAMPLE).unwrap();
        assert!(!to_page(answer, "pl").full);
        let job = r#"{ "title": "Dev", "company": { "display_name": "Firma Zmyślona" } }"#;
        let many = format!(r#"{{ "results": [{}] }}"#, vec![job; PER_PAGE as usize].join(","));
        let page = to_page(serde_json::from_str(&many).unwrap(), "pl");
        assert!(page.full);
        assert_eq!(page.offers.len(), PER_PAGE as usize);
    }

    #[test]
    fn predicted_salary_is_ignored() {
        let job = Job {
            title: Some("Dev".into()),
            company: Some(Named { display_name: Some("Firma".into()) }),
            salary_min: Some(10000.0),
            salary_is_predicted: serde_json::json!("1"),
            redirect_url: Some("javascript:alert(1)".into()),
            ..Default::default()
        };
        let offer = to_offer(job, "pl").unwrap();
        assert_eq!(offer.salary_min, None);
        assert_eq!(offer.salary_currency, None);
        assert_eq!(offer.url, None);
    }

    #[test]
    fn salary_periods() {
        assert_eq!(salary(Some(45.0), "gb"), (Some(45), Some(SalaryPeriod::Hour)));
        assert_eq!(salary(Some(36000.0), "gb"), (Some(3000), Some(SalaryPeriod::Month)));
        assert_eq!(salary(Some(18000.0), "pl"), (Some(18000), Some(SalaryPeriod::Month)));
        // A B2B day rate (real case: "PHP Developer", 1200-1400) is a day rate, never a month "too low".
        assert_eq!(salary(Some(1200.0), "pl"), (Some(1200), Some(SalaryPeriod::Day)));
        assert_eq!(salary(Some(272_000.0), "pl"), (Some(22700), Some(SalaryPeriod::Month)));
        assert_eq!(salary(Some(f64::NAN), "pl"), (None, None));
    }
}
