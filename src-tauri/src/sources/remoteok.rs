//! Remote OK - remote jobs, public JSON API without a key. Their API terms (the "legal" field
//! of the answer and remoteok.com/legal, checked 3.11.2026): link back to the offer on
//! Remote OK, name "Remote OK" as the source, never use their logo. The answer has the 100
//! newest offers and no search, so only offers that mention the user's keywords are kept.

use reqwest::Client;
use serde::Deserialize;

use crate::error::AppResult;
use crate::model::{OfferInput, Remote};
use crate::net;
use crate::text::{cut, html_to_text, one_line};

pub const SOURCE: &str = "remoteok";
const URL: &str = "https://remoteok.com/api";

/// The first element is a notice ({"last_updated", "legal"}), offers follow - so every field
/// is optional and items without a position or company are skipped.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct Job {
    pub position: Option<String>,
    pub company: Option<String>,
    pub url: Option<String>,
    pub location: Option<String>,
    pub tags: Vec<String>,
    pub salary_min: Option<f64>,
    pub salary_max: Option<f64>,
    pub description: Option<String>,
}

/// The whole list (one call); offers without any of `words` are dropped.
pub async fn fetch(client: &Client, words: &[String]) -> AppResult<Vec<OfferInput>> {
    let resp = client.get(URL).send().await.map_err(net::net_error)?;
    let jobs: Vec<Job> = net::json(resp).await?;
    Ok(jobs
        .into_iter()
        .filter_map(to_offer)
        .filter(|o| crate::filter::mentions_any(&o.title, o.description.as_deref().unwrap_or(""), words))
        .collect())
}

/// Their texts are sometimes UTF-8 read as Latin-1 ("anÂ AI" for "an AI", "Ã©" for "é").
/// Such text is turned back; anything else stays as it is.
fn fix_encoding(s: &str) -> String {
    if !s.contains(['Â', 'Ã']) || s.chars().any(|c| c as u32 > 0xFF) {
        return s.to_string();
    }
    let bytes: Vec<u8> = s.chars().map(|c| c as u32 as u8).collect();
    String::from_utf8(bytes).unwrap_or_else(|_| s.to_string())
}

fn clean(s: &str) -> String {
    html_to_text(&fix_encoding(s)).replace('\u{a0}', " ")
}

pub fn to_offer(job: Job) -> Option<OfferInput> {
    let title = one_line(&clean(job.position.as_deref()?), 300);
    let company = one_line(&clean(job.company.as_deref()?), 300);
    let url = job.url.filter(|u| u.starts_with("https://"))?;
    if title.is_empty() || company.is_empty() {
        return None;
    }
    let mut head = Vec::new();
    if let Some(loc) = job.location.filter(|s| !s.trim().is_empty()) {
        head.push(format!("Candidate location: {}", one_line(&clean(&loc), 200)));
    }
    // Often 0 or far apart, so only as text - never as the offer's pay range.
    let pay = |v: Option<f64>| v.filter(|v| v.is_finite() && *v > 0.0).map(|v| v.round() as i64);
    match (pay(job.salary_min), pay(job.salary_max)) {
        (Some(a), Some(b)) => head.push(format!("Salary (as given): ${a} - ${b} a year")),
        (Some(a), None) | (None, Some(a)) => head.push(format!("Salary (as given): ${a} a year")),
        _ => {}
    }
    let tags: Vec<String> = job.tags.iter().map(|t| one_line(t, 40)).filter(|t| !t.is_empty()).take(15).collect();
    if !tags.is_empty() {
        head.push(format!("Tags: {}", tags.join(", ")));
    }
    let body = job.description.map(|d| clean(&d)).unwrap_or_default();
    let description = format!("{}\n\n{}", head.join("\n"), body);
    Some(OfferInput {
        title,
        company,
        source: SOURCE.into(),
        url: Some(url),
        remote: Some(Remote::Yes),
        description: Some(cut(description.trim(), 60_000)),
        ..Default::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    // Shape of the real answer (3.11.2026); companies and offers are made up.
    const SAMPLE: &str = r#"[
      { "last_updated": 1793704911, "legal": "API Terms of Service: Please link back ..." },
      {
        "slug": "remote-php-developer-przykladowa-firma-1", "id": "1", "epoch": 1793616215,
        "date": "2026-11-02T10:43:35+00:00", "company": "Przykładowa Firma", "company_logo": "",
        "position": "PHP Developer", "tags": ["dev", "php", "wordpress"],
        "description": "<p>We are searching for anÂ <strong>engineer</strong> in MalÃ©e.</p>",
        "location": "Europe", "salary_min": 40000, "salary_max": 60000,
        "apply_url": "https://remoteOK.com/remote-jobs/remote-php-developer-przykladowa-firma-1",
        "original": true, "verified": true, "logo": "",
        "url": "https://remoteOK.com/remote-jobs/remote-php-developer-przykladowa-firma-1"
      },
      { "id": "2", "company": "Firma", "position": "Dev", "salary_min": 0, "salary_max": 0, "url": "http://insecure.example" }
    ]"#;

    #[test]
    fn answer_becomes_offers() {
        let jobs: Vec<Job> = serde_json::from_str(SAMPLE).unwrap();
        let offers: Vec<OfferInput> = jobs.into_iter().filter_map(to_offer).collect();
        assert_eq!(offers.len(), 1);
        let o = &offers[0];
        assert_eq!(o.company, "Przykładowa Firma");
        assert_eq!(o.title, "PHP Developer");
        assert_eq!(o.remote, Some(Remote::Yes));
        assert_eq!(o.salary_min, None);
        let d = o.description.as_deref().unwrap();
        assert!(d.starts_with("Candidate location: Europe\nSalary (as given): $40000 - $60000 a year\nTags: dev, php, wordpress"), "{d}");
        assert!(d.contains("searching for an engineer in Malée."), "{d}");
    }

    #[test]
    fn broken_encoding_is_fixed_and_good_text_kept() {
        assert_eq!(fix_encoding("caf\u{c3}\u{a9}"), "café");
        assert_eq!(fix_encoding("Łódź Â"), "Łódź Â");
        assert_eq!(fix_encoding("plain"), "plain");
    }
}
