//! Remotive - remote jobs, public API without a key. Their rules: at most about 4 calls
//! a day (never more than 2 a minute), link back to the offer on Remotive and name
//! Remotive as the source. Offers come with a 24-hour delay.

use reqwest::Client;
use serde::Deserialize;

use crate::error::AppResult;
use crate::model::{OfferInput, Remote};
use crate::net;
use crate::text::{cut, html_to_text, one_line};

pub const SOURCE: &str = "remotive";
const URL: &str = "https://remotive.com/api/remote-jobs";
const LIMIT: u32 = 100;

#[derive(Debug, Deserialize)]
struct Answer {
    #[serde(default)]
    jobs: Vec<Job>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct Job {
    pub url: Option<String>,
    pub title: Option<String>,
    pub company_name: Option<String>,
    pub category: Option<String>,
    pub job_type: Option<String>,
    pub candidate_required_location: Option<String>,
    pub salary: Option<String>,
    pub description: Option<String>,
    /// "2026-11-02T10:13:07" - UTC without a zone.
    pub publication_date: Option<String>,
}

/// One search by a keyword (one API call).
pub async fn search(client: &Client, keyword: &str) -> AppResult<Vec<OfferInput>> {
    let params = [("search", keyword.to_string()), ("limit", LIMIT.to_string())];
    let resp = client.get(URL).query(&params).send().await.map_err(net::net_error)?;
    let answer: Answer = net::json(resp).await?;
    Ok(answer.jobs.into_iter().filter_map(to_offer).collect())
}

pub fn to_offer(job: Job) -> Option<OfferInput> {
    let title = one_line(&html_to_text(job.title.as_deref()?), 300);
    let company = one_line(&job.company_name.unwrap_or_default(), 300);
    let url = job.url.filter(|u| u.starts_with("https://"))?;
    if title.is_empty() || company.is_empty() {
        return None;
    }
    // Short facts above the description: where candidates may live, pay as written, type of job.
    let mut head = Vec::new();
    if let Some(loc) = job.candidate_required_location.filter(|s| !s.trim().is_empty()) {
        head.push(format!("Candidate location: {}", one_line(&loc, 200)));
    }
    if let Some(salary) = job.salary.filter(|s| !s.trim().is_empty()) {
        head.push(format!("Salary: {}", one_line(&salary, 200)));
    }
    if let Some(kind) = job.job_type.filter(|s| !s.trim().is_empty()) {
        head.push(format!("Job type: {}", one_line(&kind.replace('_', " "), 60)));
    }
    if let Some(category) = job.category.filter(|s| !s.trim().is_empty()) {
        head.push(format!("Category: {}", one_line(&category, 100)));
    }
    let body = job.description.map(|d| html_to_text(&d)).unwrap_or_default();
    let description = format!("{}\n\n{}", head.join("\n"), body);
    Some(OfferInput {
        title,
        company,
        source: SOURCE.into(),
        url: Some(url),
        remote: Some(Remote::Yes),
        description: Some(cut(description.trim(), 60_000)),
        published_at: job.publication_date,
        ..Default::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    // Shape of a real answer; companies and offers are made up.
    const SAMPLE: &str = r#"{
      "0-legal-notice": "...",
      "job-count": 2,
      "jobs": [
        {
          "id": 1, "url": "https://remotive.com/remote-jobs/software-dev/react-developer-1",
          "title": "React Developer", "company_name": "Example Remote Co",
          "category": "Software Development", "job_type": "full_time",
          "candidate_required_location": "Europe", "salary": "$50k - $70k",
          "description": "<p>We build <b>tools</b>.</p><ul><li>React</li></ul>"
        },
        { "id": 2, "url": "http://insecure.example/job", "title": "Dev", "company_name": "X" }
      ]
    }"#;

    #[test]
    fn answer_becomes_offers() {
        let answer: Answer = serde_json::from_str(SAMPLE).unwrap();
        let offers: Vec<OfferInput> = answer.jobs.into_iter().filter_map(to_offer).collect();
        assert_eq!(offers.len(), 1);
        let o = &offers[0];
        assert_eq!(o.company, "Example Remote Co");
        assert_eq!(o.remote, Some(Remote::Yes));
        let d = o.description.as_deref().unwrap();
        assert!(d.starts_with("Candidate location: Europe\nSalary: $50k - $70k\nJob type: full time"));
        assert!(d.contains("We build tools."));
        assert!(d.contains("- React"));
    }
}
