//! Offers from e-mail alerts of job portals - plain code, no AI.
//!
//! Every portal formats its alert differently and changes it from time to time, so the
//! parser does not depend on one layout. It looks for links to offers of known portals
//! (by address), takes the link text as the title and reads the few lines after it:
//! company, place, salary. What it cannot tell stays empty - the hard filter then says
//! "unsure" instead of guessing, and AI can fill the fields later.

use reqwest::Url;

use crate::cities;
use crate::dedup::normalize;
use crate::model::{OfferInput, Remote, SalaryPeriod};
use crate::text::{cut, decode_entities, one_line, squeeze};

/// A job portal that sends alerts. `offer_paths` - pieces of the address of a single offer.
#[derive(Debug, PartialEq)]
pub struct Portal {
    pub source: &'static str,
    pub domains: &'static [&'static str],
    pub offer_paths: &'static [&'static str],
}

pub const PORTALS: &[Portal] = &[
    Portal { source: "jjit", domains: &["justjoin.it"], offer_paths: &["/job-offer/", "/offers/"] },
    Portal { source: "rocketjobs", domains: &["rocketjobs.pl"], offer_paths: &["/oferta-pracy/", "/job-offer/", "/oferty-pracy/"] },
    Portal { source: "nfj", domains: &["nofluffjobs.com"], offer_paths: &["/job/"] },
    Portal { source: "pracuj", domains: &["pracuj.pl"], offer_paths: &[",oferta,"] },
    Portal { source: "protocol", domains: &["theprotocol.it"], offer_paths: &["/szczegoly/praca/"] },
    Portal { source: "bulldogjob", domains: &["bulldogjob.pl", "bulldogjob.com"], offer_paths: &["/companies/jobs/"] },
    Portal { source: "linkedin", domains: &["linkedin.com"], offer_paths: &["/jobs/view/"] },
    Portal { source: "indeed", domains: &["indeed.com", "indeed.pl"], offer_paths: &["/viewjob", "/rc/clk", "/pagead/clk"] },
];

/// Links and texts that are never offers (account, unsubscribe, apps, social media…).
const NOT_OFFER: &[&str] = &[
    "unsubscribe", "wypisz", "rezygn", "ustawien", "settings", "preferen", "privacy", "prywatno", "polityk",
    "regulamin", "terms", "pomoc", "help", "kontakt", "contact", "app store", "google play", "facebook",
    "instagram", "twitter", "youtube", "tiktok", "zobacz wszystkie", "see all", "wiecej ofert", "more jobs",
    "zarzadzaj", "manage", "zaloguj", "log in", "sign in", "pobierz aplikacje", "download",
];

/// Lines that carry no information about the offer.
const NOISE: &[&str] = &[
    "nowa", "nowe", "new", "aplikuj", "apply", "zobacz", "zobacz oferte", "view job", "see job", "sprawdz",
    "szczegoly", "details", "promowana", "promoted", "polecana", "featured", "easy apply", "latwa aplikacja",
];

const REMOTE_WORDS: &[&str] = &["zdalnie", "zdalna", "praca zdalna", "remote", "fully remote", "100% zdalnie"];
const HYBRID_WORDS: &[&str] = &["hybrydowo", "hybrydowa", "hybrid", "praca hybrydowa"];

const MAX_OFFERS_PER_MAIL: usize = 60;
const MAX_DETAILS: usize = 8;

// ---------- HTML → links and lines ----------

#[derive(Debug, Clone, PartialEq)]
pub enum Seg {
    Link { href: String, text: String },
    Line(String),
}

fn attr(tag: &str, name: &str) -> Option<String> {
    let lower = tag.to_ascii_lowercase();
    let mut from = 0;
    while let Some(pos) = lower[from..].find(name) {
        let start = from + pos;
        from = start + name.len();
        // Must be a whole attribute name.
        if start > 0 && !lower.as_bytes()[start - 1].is_ascii_whitespace() {
            continue;
        }
        let rest = tag[from..].trim_start();
        let Some(rest) = rest.strip_prefix('=') else { continue };
        let rest = rest.trim_start();
        let value = match rest.chars().next() {
            Some(q @ ('"' | '\'')) => rest[1..].split(q).next().unwrap_or(""),
            _ => rest.split(|c: char| c.is_whitespace() || c == '>').next().unwrap_or(""),
        };
        return Some(decode_entities(value).trim().to_string());
    }
    None
}

const BLOCK: &[&str] = &[
    "br", "p", "div", "tr", "td", "th", "li", "ul", "ol", "table", "h1", "h2", "h3", "h4", "h5", "h6", "section",
    "article", "header", "footer", "hr", "tbody", "center",
];

/// Splits HTML into links (with their text) and lines of plain text, in reading order.
pub fn segments(html: &str) -> Vec<Seg> {
    let mut out = Vec::new();
    let mut line = String::new();
    let mut link: Option<(String, String)> = None;
    let mut rest = html;

    fn flush(line: &mut String, out: &mut Vec<Seg>) {
        let text = one_line(&decode_entities(line), 500);
        if !text.is_empty() {
            out.push(Seg::Line(text));
        }
        line.clear();
    }

    while let Some(start) = rest.find('<') {
        let text = &rest[..start];
        match link.as_mut() {
            Some((_, t)) => t.push_str(text),
            None => line.push_str(text),
        }
        rest = &rest[start..];
        let Some(end) = rest.find('>') else { break };
        let tag = &rest[1..end];
        rest = &rest[end + 1..];
        if tag.starts_with("!--") {
            if let Some(close) = rest.find("-->") {
                rest = &rest[close + 3..];
            }
            continue;
        }
        let closing = tag.starts_with('/');
        let name: String = tag
            .trim_start_matches('/')
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric())
            .collect::<String>()
            .to_ascii_lowercase();
        if !closing && (name == "script" || name == "style" || name == "head" || name == "title") {
            let lower = rest.to_ascii_lowercase();
            rest = match lower.find(&format!("</{name}")) {
                Some(i) => rest[i..].find('>').map_or("", |gt| &rest[i + gt + 1..]),
                None => "",
            };
            continue;
        }
        if name == "a" {
            if closing {
                if let Some((href, text)) = link.take() {
                    out.push(Seg::Link { href, text: one_line(&decode_entities(&text), 300) });
                }
            } else {
                flush(&mut line, &mut out);
                if let Some((href, text)) = link.take() {
                    out.push(Seg::Link { href, text: one_line(&decode_entities(&text), 300) });
                }
                link = Some((attr(tag, "href").unwrap_or_default(), String::new()));
            }
            continue;
        }
        if name == "img" {
            // An image link (logo) has no text; its description may name the company.
            if let (Some((_, t)), Some(alt)) = (link.as_mut(), attr(tag, "alt")) {
                if t.trim().is_empty() {
                    t.push_str(&alt);
                }
            }
            continue;
        }
        if BLOCK.contains(&name.as_str()) {
            match link.as_mut() {
                Some((_, t)) => t.push(' '),
                None => flush(&mut line, &mut out),
            }
        }
    }
    if let Some((href, text)) = link.take() {
        out.push(Seg::Link { href, text: one_line(&decode_entities(&text), 300) });
    }
    line.push_str(rest);
    flush(&mut line, &mut out);
    out
}

/// Plain-text e-mail: every address becomes a link; its text is the rest of the line,
/// or the line before when the address stands alone.
pub fn text_segments(text: &str) -> Vec<Seg> {
    let mut out: Vec<Seg> = Vec::new();
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }
        let urls: Vec<&str> = line
            .split(|c: char| c.is_whitespace() || c == '<' || c == '>' || c == '(' || c == ')')
            .filter(|w| w.starts_with("https://") || w.starts_with("http://"))
            .collect();
        if urls.is_empty() {
            out.push(Seg::Line(one_line(line, 500)));
            continue;
        }
        let mut rest = line.to_string();
        for u in &urls {
            rest = rest.replace(u, " ");
        }
        let mut label = one_line(rest.trim_matches(|c: char| c.is_whitespace() || c == ':' || c == '-' || c == '<' || c == '>'), 300);
        if label.is_empty() {
            if let Some(Seg::Line(prev)) = out.last() {
                label = prev.clone();
                out.pop();
            }
        }
        out.push(Seg::Link { href: urls[0].to_string(), text: label });
    }
    out
}

// ---------- recognising offer links ----------

fn host_matches(host: &str, domain: &str) -> bool {
    host == domain || host.ends_with(&format!(".{domain}"))
}

fn portal_of_host(host: &str) -> Option<&'static Portal> {
    PORTALS.iter().find(|p| p.domains.iter().any(|d| host_matches(host, d)))
}

/// Portal of a sender address (`alerty@justjoin.it`).
pub fn portal_of_sender(address: &str) -> Option<&'static Portal> {
    let domain = address.rsplit('@').next()?.trim().trim_end_matches('>').to_ascii_lowercase();
    portal_of_host(&domain)
}

fn is_web(url: &Url) -> bool {
    matches!(url.scheme(), "http" | "https")
}

fn is_offer_url(url: &Url, portal: &Portal) -> bool {
    let path = url.path().to_ascii_lowercase();
    portal.offer_paths.iter().any(|p| path.contains(p))
}

fn blocked(text: &str) -> bool {
    let n = normalize(text);
    NOT_OFFER.iter().any(|w| n.contains(w))
}

/// How sure we are that a link points to an offer.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Kind {
    /// Address of a single offer of a known portal.
    Offer(&'static Portal),
    /// Tracking address of the portal that sent the alert - an offer only when its text looks like a title.
    Tracked(&'static Portal),
}

fn classify(href: &str, sender: Option<&'static Portal>) -> Option<(Kind, String)> {
    let url = Url::parse(href).ok().filter(is_web)?;
    let host = url.host_str()?.to_ascii_lowercase();
    if blocked(url.path()) {
        return None;
    }
    // Address of the offer itself, possibly wrapped in a tracking link (`?url=https://…`).
    let mut candidates = vec![url.clone()];
    for (_, value) in url.query_pairs() {
        if value.starts_with("http") {
            if let Ok(inner) = Url::parse(&value) {
                candidates.push(inner);
            }
        }
    }
    for c in &candidates {
        if let Some(portal) = c.host_str().and_then(|h| portal_of_host(&h.to_ascii_lowercase())) {
            if is_web(c) && is_offer_url(c, portal) {
                return Some((Kind::Offer(portal), clean_url(c)));
            }
        }
    }
    let portal = portal_of_host(&host).or(sender)?;
    // A tracking link of the sending portal, or any link in its alert (e.g. a click-tracking service).
    if sender.is_some_and(|s| std::ptr::eq(s, portal)) {
        return Some((Kind::Tracked(portal), url.to_string()));
    }
    None
}

/// Offer address without tracking parameters (`utm_*`), so the same offer is recognised.
fn clean_url(url: &Url) -> String {
    let mut url = url.clone();
    let kept: Vec<(String, String)> = url
        .query_pairs()
        .filter(|(k, _)| !k.starts_with("utm_") && k != "ref" && k != "trk" && k != "refId" && k != "trackingId")
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect();
    if kept.is_empty() {
        url.set_query(None);
    } else {
        url.query_pairs_mut().clear().extend_pairs(kept);
    }
    url.set_fragment(None);
    url.to_string()
}

fn title_like(text: &str) -> bool {
    let n = normalize(text);
    let len = text.chars().count();
    (3..=150).contains(&len)
        && text.chars().any(char::is_alphabetic)
        && !blocked(text)
        && !NOISE.contains(&n.as_str())
        && parse_salary(text).is_none()
}

// ---------- salary, place ----------

/// Salary from a line like "12 000 - 16 000 PLN", "12k-16k zł netto", "45-60 EUR/h", "od 15 tys. zł".
/// Returns (min, max, currency, period). `None` when the line has no clear amount of money.
pub fn parse_salary(line: &str) -> Option<(i64, Option<i64>, String, SalaryPeriod)> {
    let lower = line.to_lowercase();
    let currency = if lower.contains("eur") || lower.contains('€') {
        "EUR"
    } else if lower.contains("usd") || lower.contains('$') {
        "USD"
    } else if lower.contains("gbp") || lower.contains('£') {
        "GBP"
    } else if lower.contains("chf") {
        "CHF"
    } else if lower.contains("zł") || lower.contains("pln") || lower.contains(" zl") {
        "PLN"
    } else {
        ""
    };
    let thousands_word = lower.contains("tys") || lower.contains('k');
    if currency.is_empty() && !lower.contains("tys") && !lower.contains("brutto") && !lower.contains("netto") {
        return None;
    }
    let hourly = lower.contains("/h") || lower.contains("/godz") || lower.contains("godzin") || lower.contains("hour") || lower.contains("/hr");

    // Numbers: "12 000", "12.000", "12,5k", "15 tys".
    let chars: Vec<char> = lower.chars().map(|c| if c == '\u{a0}' || c == '\u{202f}' { ' ' } else { c }).collect();
    let mut numbers: Vec<f64> = Vec::new();
    let mut i = 0;
    while i < chars.len() && numbers.len() < 2 {
        if !chars[i].is_ascii_digit() {
            i += 1;
            continue;
        }
        let mut digits = String::new();
        let mut decimal = String::new();
        while i < chars.len() {
            let c = chars[i];
            if c.is_ascii_digit() {
                digits.push(c);
                i += 1;
            } else if (c == ' ' || c == '.' || c == ',')
                && i + 3 < chars.len() + 1
                && chars[i + 1..].iter().take(3).all(char::is_ascii_digit)
                && chars.get(i + 4).is_none_or(|n| !n.is_ascii_digit())
            {
                // Thousands separator: exactly three digits follow.
                i += 1;
            } else if (c == ',' || c == '.') && chars.get(i + 1).is_some_and(char::is_ascii_digit) {
                i += 1;
                while i < chars.len() && chars[i].is_ascii_digit() {
                    decimal.push(chars[i]);
                    i += 1;
                }
                break;
            } else {
                break;
            }
        }
        let mut value: f64 = format!("{digits}.{}", if decimal.is_empty() { "0" } else { &decimal }).parse().unwrap_or(0.0);
        // "12k", "15 tys"
        let after: String = chars[i..].iter().take(5).collect::<String>();
        let after = after.trim_start();
        if after.starts_with('k') || after.starts_with("tys") {
            value *= 1000.0;
        }
        numbers.push(value);
    }
    // "12-16k": the first number shares the unit of the second.
    if numbers.len() == 2 && numbers[0] < 1000.0 && numbers[1] >= 1000.0 && thousands_word && !hourly {
        numbers[0] *= 1000.0;
    }
    let min = *numbers.first()?;
    let max = numbers.get(1).copied().filter(|m| *m >= min);
    let plausible = |v: f64| if hourly { (5.0..=2_000.0).contains(&v) } else { (500.0..=500_000.0).contains(&v) };
    if !plausible(min) || max.is_some_and(|m| !plausible(m)) {
        return None;
    }
    let currency = if currency.is_empty() { "PLN" } else { currency };
    Some((
        min.round() as i64,
        max.map(|m| m.round() as i64),
        currency.into(),
        if hourly { SalaryPeriod::Hour } else { SalaryPeriod::Month },
    ))
}

/// City and form of work from a line like "Wrocław, Kraków + 2", "Zdalnie", "Warszawa (hybrydowo)".
fn parse_place(line: &str) -> Option<(Option<String>, Option<Remote>)> {
    let n = normalize(line);
    let remote = if REMOTE_WORDS.iter().any(|w| n.contains(w)) {
        Some(Remote::Yes)
    } else if HYBRID_WORDS.iter().any(|w| n.contains(w)) {
        Some(Remote::Hybrid)
    } else {
        None
    };
    let city = line
        .split([',', '|', '(', ')', '·', '/', '•'])
        .map(str::trim)
        .find(|part| cities::coordinates(part).is_some())
        .map(|c| c.to_string());
    if city.is_none() && remote.is_none() {
        return None;
    }
    // A long sentence that happens to name a city is not a place line.
    if line.chars().count() > 80 {
        return None;
    }
    Some((city, remote))
}

// ---------- offers ----------

#[derive(Debug, Default)]
struct Draft {
    source: &'static str,
    url: String,
    title: String,
    details: Vec<String>,
}

/// Offers in one alert e-mail. `sender` - the From address, `html` / `text` - its bodies.
pub fn offers_in_mail(sender: &str, html: Option<&str>, text: Option<&str>) -> Vec<OfferInput> {
    let sender_portal = portal_of_sender(sender);
    let segs = match (html, text) {
        (Some(h), _) if !h.trim().is_empty() => segments(h),
        (_, Some(t)) => text_segments(t),
        _ => return Vec::new(),
    };
    let mut drafts: Vec<Draft> = Vec::new();
    for seg in segs {
        match seg {
            Seg::Link { href, text } => {
                let Some((kind, url)) = classify(&href, sender_portal) else {
                    // Other links (e.g. the company page) may still carry details.
                    if let Some(last) = drafts.last_mut() {
                        if !text.is_empty() && !blocked(&text) && last.details.len() < MAX_DETAILS {
                            last.details.push(text);
                        }
                    }
                    continue;
                };
                if let Some(last) = drafts.last_mut() {
                    // Logo, title and "apply" usually point to the same offer.
                    if last.url == url {
                        if last.title.is_empty() && title_like(&text) {
                            last.title = text;
                        } else if !text.is_empty() && text != last.title && last.details.len() < MAX_DETAILS && !blocked(&text) {
                            last.details.push(text);
                        }
                        continue;
                    }
                }
                let (portal, sure) = match kind {
                    Kind::Offer(p) => (p, true),
                    Kind::Tracked(p) => (p, false),
                };
                if !sure && !title_like(&text) {
                    if let Some(last) = drafts.last_mut() {
                        if !text.is_empty() && !blocked(&text) && last.details.len() < MAX_DETAILS {
                            last.details.push(text);
                        }
                    }
                    continue;
                }
                if drafts.len() >= MAX_OFFERS_PER_MAIL {
                    break;
                }
                drafts.push(Draft {
                    source: portal.source,
                    url,
                    title: if title_like(&text) { text } else { String::new() },
                    details: Vec::new(),
                });
            }
            Seg::Line(line) => {
                if let Some(last) = drafts.last_mut() {
                    if last.details.len() < MAX_DETAILS {
                        last.details.push(line);
                    }
                }
            }
        }
    }
    let mut seen = std::collections::HashSet::new();
    drafts
        .into_iter()
        .filter_map(finish)
        .filter(|o| seen.insert((normalize(&o.company), normalize(&o.title))))
        .collect()
}

fn finish(mut d: Draft) -> Option<OfferInput> {
    if d.title.is_empty() {
        let pos = d.details.iter().position(|l| title_like(l))?;
        d.title = d.details.remove(pos);
    }
    let mut salary = None;
    let mut city = None;
    let mut remote = None;
    let mut company = None;
    let mut rest = Vec::new();
    for line in &d.details {
        let n = normalize(line);
        if n.is_empty() || NOISE.contains(&n.as_str()) || blocked(line) || *line == d.title {
            continue;
        }
        if salary.is_none() {
            if let Some(s) = parse_salary(line) {
                salary = Some(s);
                continue;
            }
        }
        if let Some((c, r)) = parse_place(line) {
            city = city.or(c);
            remote = remote.or(r);
            continue;
        }
        if company.is_none() && line.chars().count() <= 80 && line.chars().any(char::is_alphabetic) {
            company = Some(line.clone());
            continue;
        }
        rest.push(line.clone());
    }
    // Without a company two different "React Developer" offers would look the same -
    // a short piece of the link keeps them apart until AI or the user fills it in.
    let company = company.unwrap_or_else(|| format!("? ({})", short_id(&d.url)));
    let (salary_min, salary_max, currency, period) = match salary {
        Some((min, max, cur, period)) => (Some(min), max, Some(cur), Some(period)),
        None => (None, None, None, None),
    };
    let description = squeeze(&d.details.join("\n"));
    Some(OfferInput {
        title: one_line(&d.title, 300),
        company: one_line(&company, 300),
        source: d.source.into(),
        url: Some(cut(&d.url, 2000)).filter(|u| u.starts_with("https://") || u.starts_with("http://")),
        city,
        remote,
        salary_min,
        salary_max,
        salary_currency: currency,
        salary_period: period,
        requirements: (!rest.is_empty()).then(|| cut(&rest.join(", "), 2000)),
        description: (!description.is_empty()).then(|| cut(&description, 5000)),
        ..Default::default()
    })
}

fn short_id(url: &str) -> String {
    // FNV-1a - stable between runs, no extra dependency.
    let mut h: u32 = 0x811c9dc5;
    for b in url.bytes() {
        h ^= b as u32;
        h = h.wrapping_mul(0x01000193);
    }
    format!("{:06x}", h & 0xff_ffff)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn salaries_in_many_shapes() {
        let s = |t: &str| parse_salary(t).map(|(a, b, c, p)| (a, b, c, p));
        assert_eq!(s("12 000 - 16 000 PLN"), Some((12000, Some(16000), "PLN".into(), SalaryPeriod::Month)));
        assert_eq!(s("12k-16k zł netto (B2B)"), Some((12000, Some(16000), "PLN".into(), SalaryPeriod::Month)));
        assert_eq!(s("12-16k PLN"), Some((12000, Some(16000), "PLN".into(), SalaryPeriod::Month)));
        assert_eq!(s("45-60 EUR/h"), Some((45, Some(60), "EUR".into(), SalaryPeriod::Hour)));
        assert_eq!(s("od 15 tys. zł brutto"), Some((15000, None, "PLN".into(), SalaryPeriod::Month)));
        assert_eq!(s("10.500,50 zł"), Some((10501, None, "PLN".into(), SalaryPeriod::Month)));
        assert_eq!(s("140 - 180 zł/h"), Some((140, Some(180), "PLN".into(), SalaryPeriod::Hour)));
        assert_eq!(s("Wrocław, ul. Długa 12"), None);
        assert_eq!(s("React Developer"), None);
        assert_eq!(s("3 lata doświadczenia"), None);
    }

    #[test]
    fn places() {
        assert_eq!(parse_place("Wrocław, Kraków + 2"), Some((Some("Wrocław".into()), None)));
        assert_eq!(parse_place("Zdalnie"), Some((None, Some(Remote::Yes))));
        assert_eq!(parse_place("Warszawa (hybrydowo)"), Some((Some("Warszawa".into()), Some(Remote::Hybrid))));
        assert_eq!(parse_place("Przykładowa Firma"), None);
    }

    #[test]
    fn segments_keep_link_text_and_lines() {
        let html = r#"<table><tr><td><a href="https://justjoin.it/job-offer/firma-x-react-dev?utm_source=mail&amp;x=1"><img alt="Firma X logo"></a></td>
            <td><a href="https://justjoin.it/job-offer/firma-x-react-dev?utm_source=mail&amp;x=1"><b>React</b> Developer</a><br>Firma X<br>Wrocław</td></tr></table>"#;
        let segs = segments(html);
        assert_eq!(segs[0], Seg::Link { href: "https://justjoin.it/job-offer/firma-x-react-dev?utm_source=mail&x=1".into(), text: "Firma X logo".into() });
        assert_eq!(segs[1], Seg::Link { href: "https://justjoin.it/job-offer/firma-x-react-dev?utm_source=mail&x=1".into(), text: "React Developer".into() });
        assert_eq!(segs[2], Seg::Line("Firma X".into()));
        assert_eq!(segs[3], Seg::Line("Wrocław".into()));
    }

    // Made-up alert in the shape of a portal alert (one card per offer).
    const ALERT_HTML: &str = r#"<html><head><style>.x{color:red}</style></head><body>
      <h1>Nowe oferty dla Ciebie</h1>
      <table>
        <tr><td>
          <a href="https://justjoin.it/job-offer/przykladowa-firma-frontend-developer-wroclaw?utm_campaign=alert"><img src="logo.png" alt=""></a>
          <a href="https://justjoin.it/job-offer/przykladowa-firma-frontend-developer-wroclaw?utm_campaign=alert">Frontend Developer (React)</a>
          <div>Przykładowa Firma</div>
          <div>Wrocław · Zdalnie</div>
          <div>12 000 - 16 000 PLN</div>
          <div>React, TypeScript</div>
          <a href="https://justjoin.it/job-offer/przykladowa-firma-frontend-developer-wroclaw?utm_campaign=alert">Aplikuj</a>
        </td></tr>
        <tr><td>
          <a href="https://justjoin.it/job-offer/zmyslona-sp-z-o-o-qa-engineer-krakow">QA Engineer</a>
          <p>Zmyślona Sp. z o.o.</p><p>Kraków</p><p>Nowa</p>
        </td></tr>
      </table>
      <a href="https://justjoin.it/settings/alerts">Zarządzaj alertami</a>
      <a href="https://justjoin.it/unsubscribe?id=1">Wypisz się</a>
    </body></html>"#;

    #[test]
    fn alert_with_offer_links() {
        let offers = offers_in_mail("alerty@justjoin.it", Some(ALERT_HTML), None);
        assert_eq!(offers.len(), 2);
        let a = &offers[0];
        assert_eq!(a.title, "Frontend Developer (React)");
        assert_eq!(a.company, "Przykładowa Firma");
        assert_eq!(a.city.as_deref(), Some("Wrocław"));
        assert_eq!(a.remote, Some(Remote::Yes));
        assert_eq!((a.salary_min, a.salary_max), (Some(12000), Some(16000)));
        assert_eq!(a.source, "jjit");
        assert_eq!(a.url.as_deref(), Some("https://justjoin.it/job-offer/przykladowa-firma-frontend-developer-wroclaw"));
        assert_eq!(a.requirements.as_deref(), Some("React, TypeScript"));
        let b = &offers[1];
        assert_eq!((b.title.as_str(), b.company.as_str(), b.city.as_deref()), ("QA Engineer", "Zmyślona Sp. z o.o.", Some("Kraków")));
        assert_eq!(b.salary_min, None);
    }

    #[test]
    fn tracking_links_of_the_sender_count_only_with_a_title() {
        let html = r#"<p><a href="https://click.pracuj.pl/c/abc123?x=1">Księgowa / Księgowy</a></p><p>Biuro Rachunkowe Zmyślone</p><p>Opole</p>
            <p><a href="https://click.pracuj.pl/c/zzz">Zobacz wszystkie oferty</a></p>
            <p><a href="https://www.pracuj.pl/praca/specjalista-ds-kadr-opole,oferta,1000123456">Specjalista ds. kadr</a></p><p>Firma Kadrowa</p><p>Opole</p><p>6 500 - 7 500 zł brutto / mies.</p>"#;
        let offers = offers_in_mail("Pracuj.pl <powiadomienia@pracuj.pl>", Some(html), None);
        assert_eq!(offers.len(), 2);
        assert_eq!(offers[0].title, "Księgowa / Księgowy");
        assert_eq!(offers[0].source, "pracuj");
        assert_eq!(offers[1].company, "Firma Kadrowa");
        assert_eq!(offers[1].salary_min, Some(6500));

        // From an unknown sender a tracking link is not trusted.
        let offers = offers_in_mail("ktos@example.com", Some(html), None);
        assert_eq!(offers.len(), 1);
    }

    #[test]
    fn wrapped_offer_address_is_found() {
        let html = r#"<a href="https://links.example-mailer.com/ls/click?url=https%3A%2F%2Fnofluffjobs.com%2Fpl%2Fjob%2Fbackend-developer-firma-y-remote%3Futm_source%3Dx">Backend Developer</a><br>Firma Y<br>Zdalnie<br>15 000 - 20 000 PLN"#;
        let offers = offers_in_mail("forwarded@example.com", Some(html), None);
        assert_eq!(offers.len(), 1);
        assert_eq!(offers[0].source, "nfj");
        assert_eq!(offers[0].url.as_deref(), Some("https://nofluffjobs.com/pl/job/backend-developer-firma-y-remote"));
        assert_eq!(offers[0].remote, Some(Remote::Yes));
    }

    #[test]
    fn plain_text_alert() {
        let text = "Nowe oferty:\n\nTester manualny\nhttps://theprotocol.it/szczegoly/praca/tester-manualny-gdansk,oferta,abc\nFirma Testowa\nGdańsk\n\nUnsubscribe: https://theprotocol.it/unsubscribe";
        let offers = offers_in_mail("alerty@theprotocol.it", None, Some(text));
        assert_eq!(offers.len(), 1);
        assert_eq!(offers[0].title, "Tester manualny");
        assert_eq!(offers[0].company, "Firma Testowa");
        assert_eq!(offers[0].city.as_deref(), Some("Gdańsk"));
    }

    #[test]
    fn unrelated_mail_gives_nothing_and_unknown_company_gets_an_id() {
        let html = r#"<p>Newsletter</p><a href="https://example.com/a">Read more</a><a href="javascript:alert(1)">x</a>"#;
        assert!(offers_in_mail("news@example.com", Some(html), None).is_empty());
        let html = r#"<a href="https://www.linkedin.com/comm/jobs/view/123456/?trk=eml">Data Analyst</a><br>Warszawa"#;
        let offers = offers_in_mail("jobs-noreply@linkedin.com", Some(html), None);
        assert_eq!(offers.len(), 1);
        assert!(offers[0].company.starts_with("? ("));
        assert_eq!(offers[0].url.as_deref(), Some("https://www.linkedin.com/comm/jobs/view/123456/"));
    }
}
