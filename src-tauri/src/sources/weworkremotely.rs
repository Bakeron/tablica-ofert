//! We Work Remotely - remote jobs, public RSS feed without a key. Their rule
//! (weworkremotely.com/remote-job-rss-feed, checked 3.11.2026): "Anyone can use the feed,
//! all we ask is that you attribute the links back to We Work Remotely" - every offer keeps
//! its link to WWR and the source name. The feed has the 100 newest offers and no search,
//! so only offers that mention the user's keywords are kept.

use reqwest::Client;
use time::OffsetDateTime;
use time::format_description::well_known::{Rfc2822, Rfc3339};

use crate::error::{AppError, AppResult};
use crate::model::{OfferInput, Remote};
use crate::net;
use crate::text::{cut, decode_entities, html_to_text, one_line};

pub const SOURCE: &str = "weworkremotely";
const URL: &str = "https://weworkremotely.com/remote-jobs.rss";

/// The whole feed (one call); offers without any of `words` are dropped.
pub async fn fetch(client: &Client, words: &[String]) -> AppResult<Vec<OfferInput>> {
    let resp = net::check_status(client.get(URL).send().await.map_err(net::net_error)?).await?;
    let body = net::read_limited(resp, net::MAX_BODY).await?;
    let xml = String::from_utf8(body).map_err(|_| AppError::Net("unexpected answer: not UTF-8".into()))?;
    Ok(parse(&xml)
        .into_iter()
        .filter(|o| crate::filter::mentions_any(&o.title, o.description.as_deref().unwrap_or(""), words))
        .collect())
}

/// Text of the first `<tag>` inside one `<item>`: CDATA unwrapped, XML entities decoded.
fn tag(item: &str, name: &str) -> Option<String> {
    let open = format!("<{name}>");
    let start = item.find(&open)? + open.len();
    let end = start + item[start..].find(&format!("</{name}>"))?;
    let raw = item[start..end].trim();
    let text = match raw.strip_prefix("<![CDATA[").and_then(|r| r.strip_suffix("]]>")) {
        Some(inner) => inner.to_string(),
        None => decode_entities(raw),
    };
    Some(text).filter(|t| !t.trim().is_empty())
}

fn utc(rfc2822: Option<String>) -> Option<String> {
    let t = OffsetDateTime::parse(rfc2822?.trim(), &Rfc2822).ok()?;
    t.to_offset(time::UtcOffset::UTC).format(&Rfc3339).ok()
}

pub fn parse(xml: &str) -> Vec<OfferInput> {
    xml.split("<item>").skip(1).filter_map(|chunk| to_offer(chunk.split("</item>").next()?)).collect()
}

fn to_offer(item: &str) -> Option<OfferInput> {
    let url = tag(item, "link").filter(|u| u.starts_with("https://"))?;
    // The title is "Company: Job title".
    let full = one_line(&tag(item, "title")?, 400);
    let (company, title) = full.split_once(": ")?;
    let (company, title) = (one_line(company, 300), one_line(title, 300));
    if company.is_empty() || title.is_empty() {
        return None;
    }
    let mut head = Vec::new();
    if let Some(region) = tag(item, "region") {
        head.push(format!("Region: {}", one_line(&region, 200)));
    }
    // "🇺🇸 United States of America" - the flag is dropped, the name stays.
    if let Some(country) = tag(item, "country") {
        let name: String = country.chars().filter(|c| !('\u{1F1E6}'..='\u{1F1FF}').contains(c)).collect();
        if !name.trim().is_empty() {
            head.push(format!("Company country: {}", one_line(&name, 100)));
        }
    }
    if let Some(kind) = tag(item, "type") {
        head.push(format!("Job type: {}", one_line(&kind, 60)));
    }
    if let Some(category) = tag(item, "category") {
        head.push(format!("Category: {}", one_line(&category, 100)));
    }
    if let Some(skills) = tag(item, "skills") {
        head.push(format!("Skills: {}", one_line(&skills, 300)));
    }
    let body = tag(item, "description").map(|d| html_to_text(&d)).unwrap_or_default();
    let description = format!("{}\n\n{}", head.join("\n"), body);
    Some(OfferInput {
        title,
        company,
        source: SOURCE.into(),
        url: Some(url),
        remote: Some(Remote::Yes),
        description: Some(cut(description.trim(), 60_000)),
        expires_at: utc(tag(item, "expires_at")),
        ..Default::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    // Shape of the real feed (3.11.2026); companies and offers are made up.
    const SAMPLE: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<rss version="2.0" xmlns:dc="http://purl.org/dc/elements/1.1/" xmlns:media="http://search.yahoo.com/mrss">
  <channel>
    <title>We Work Remotely</title>
    <item>
      <media:content url="https://example.com/logo.gif" type="image/png"/>
      <title>Przykładowa Firma: Senior WordPress Developer</title>
      <region>Anywhere in the World</region>
      <country>🇵🇱 Poland</country>
      <state></state>
      <skills></skills>
      <category>Full-Stack Programming</category>
      <type>Full-Time</type>
      <description>&lt;img src="https://example.com/logo.gif" /&gt;
&lt;p&gt;We build &lt;strong&gt;plugins&lt;/strong&gt; &amp;amp; themes.&lt;/p&gt;&lt;ul&gt;&lt;li&gt;PHP&lt;/li&gt;&lt;/ul&gt;</description>
      <pubDate>Mon, 02 Nov 2026 18:18:39 +0000</pubDate>
      <expires_at>Wed, 02 Dec 2026 18:18:39 +0000</expires_at>
      <guid>https://weworkremotely.com/remote-jobs/przykladowa-firma-senior-wordpress-developer</guid>
      <link>https://weworkremotely.com/remote-jobs/przykladowa-firma-senior-wordpress-developer</link>
    </item>
    <item>
      <title>No company separator here</title>
      <link>https://weworkremotely.com/remote-jobs/x</link>
    </item>
    <item>
      <title>Firma: Dev</title>
      <link>http://insecure.example/job</link>
    </item>
  </channel>
</rss>"#;

    #[test]
    fn feed_becomes_offers() {
        let offers = parse(SAMPLE);
        assert_eq!(offers.len(), 1);
        let o = &offers[0];
        assert_eq!(o.company, "Przykładowa Firma");
        assert_eq!(o.title, "Senior WordPress Developer");
        assert_eq!(o.remote, Some(Remote::Yes));
        assert_eq!(o.expires_at.as_deref(), Some("2026-12-02T18:18:39Z"));
        let d = o.description.as_deref().unwrap();
        assert!(d.starts_with("Region: Anywhere in the World\nCompany country: Poland\nJob type: Full-Time"), "{d}");
        assert!(d.contains("We build plugins & themes."), "{d}");
        assert!(d.contains("- PHP"), "{d}");
        assert!(!d.contains("<img"), "{d}");
    }

    #[test]
    fn only_offers_with_the_users_words_are_kept() {
        let offers = parse(SAMPLE);
        let keep = |words: &[&str]| {
            let words: Vec<String> = words.iter().map(|w| w.to_string()).collect();
            offers.iter().filter(|o| crate::filter::mentions_any(&o.title, o.description.as_deref().unwrap_or(""), &words)).count()
        };
        assert_eq!(keep(&["wordpress"]), 1);
        assert_eq!(keep(&["PHP"]), 1);
        assert_eq!(keep(&["java", "python"]), 0);
    }
}
