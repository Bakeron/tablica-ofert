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
    /// The alert's own tracking links (`click.portal.pl/…`) may lead to an offer. Portals whose
    /// offers always have an address of their own say no - there such links are the feed,
    /// company pages and ads.
    pub tracked: bool,
}

pub const PORTALS: &[Portal] = &[
    Portal { source: "jjit", domains: &["justjoin.it"], offer_paths: &["/job-offer/", "/offers/"], tracked: true },
    // `/oferty-pracy/…` is a list of offers ("Pokaż oferty"), a single one is `/oferta-pracy/…`.
    Portal { source: "rocketjobs", domains: &["rocketjobs.pl"], offer_paths: &["/oferta-pracy/", "/job-offer/"], tracked: true },
    // Every link goes through `mail-link.nofluffjobs.com/click/…/<address in base64>` - decoded in `classify`.
    Portal { source: "nfj", domains: &["nofluffjobs.com"], offer_paths: &["/job/"], tracked: false },
    Portal { source: "pracuj", domains: &["pracuj.pl"], offer_paths: &[",oferta,"], tracked: true },
    Portal { source: "protocol", domains: &["theprotocol.it"], offer_paths: &["/szczegoly/praca/"], tracked: true },
    Portal { source: "bulldogjob", domains: &["bulldogjob.pl", "bulldogjob.com"], offer_paths: &["/companies/jobs/"], tracked: true },
    Portal { source: "linkedin", domains: &["linkedin.com"], offer_paths: &["/jobs/view/"], tracked: false },
    Portal { source: "indeed", domains: &["indeed.com", "indeed.pl"], offer_paths: &["/viewjob", "/rc/clk", "/pagead/clk"], tracked: true },
    // Mail comes from @jooble.com, the offers live on jooble.org (`pl.jooble.org/desc/…`).
    Portal { source: super::jooble::SOURCE, domains: &["jooble.org", "jooble.com"], offer_paths: &["/desc/", "/jdp/", "/away/"], tracked: true },
    // `/land/ad/<id>` or a search link with `topdoc_id=<id>` - see `offer_url`.
    Portal { source: super::adzuna::SOURCE, domains: &["adzuna.pl"], offer_paths: &["/land/ad/", "/details/"], tracked: false },
];

/// Links and texts that are never offers (account, unsubscribe, apps, social media…).
const NOT_OFFER: &[&str] = &[
    "unsubscribe", "wypisz", "rezygn", "ustawien", "settings", "preferen", "privacy", "prywatno", "polityk",
    "regulamin", "terms", "pomoc", "help center", "contact us", "skontaktuj", "kontakt z nami", "app store",
    "google play", "facebook", "instagram", "twitter", "youtube", "tiktok", "zobacz wszystkie", "pokaz wszystkie",
    "wyswietl wszystkie", "see all", "view all", "wiecej ofert", "more jobs", "nowe oferty", "nowych ofert",
    "new jobs", "new offers", "zarzadzaj", "manage alert", "manage job alert", "manage your", "manage subscription",
    "zaloguj", "log in", "sign in", "pobierz aplikacje", "download", "podobne do tej", "similar to",
];

/// Whole texts that are never offers, though a job title may have them inside ("Help Desk", "PR Manager").
const NOT_OFFER_EXACT: &[&str] = &["help", "kontakt", "contact", "manage"];

/// Lines that carry no information about the offer.
const NOISE: &[&str] = &[
    "nowa", "nowe", "new", "aplikuj", "apply", "zobacz", "zobacz oferte", "view job", "see job", "sprawdz",
    "szczegoly", "details", "promowana", "promoted", "polecana", "featured", "easy apply", "latwa aplikacja",
    "sugerowane", "suggested", "aktywnie rekrutuje", "actively recruiting",
    // Badges (Pracuj.pl), card buttons and labels (justjoin.it, RocketJobs, No Fluff Jobs, Indeed, Adzuna).
    "najlepiej dopasowana", "hit", "spiesz sie", "nowosc", "tu zarobisz najwiecej", "be the first to apply",
    "badz pierwszym aplikujacym", "undisclosed salary", "aplikuj bezposrednio", "check salary match",
    "podglad opisu stanowiska", "wiecej szczegolow", "oferta dopasowana dla ciebie", "nowa oferta",
    "rekrutacja zamyka sie wkrotce", "rekrutacja zamyka sie wkotce",
];

const REMOTE_WORDS: &[&str] = &["zdalnie", "zdalna", "praca zdalna", "remote", "fully remote", "100% zdalnie"];
const HYBRID_WORDS: &[&str] = &["hybrydowo", "hybrydowa", "hybrid", "praca hybrydowa"];

const MAX_OFFERS_PER_MAIL: usize = 60;
const MAX_DETAILS: usize = 8;

// ---------- HTML → links and lines ----------

#[derive(Debug, Clone, PartialEq)]
pub enum Seg {
    /// `image` - the text is the description of an image (a logo), not words shown in the mail.
    Link { href: String, text: String, image: bool },
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

/// A block tag inside a link (a control character, so it never comes from the mail text itself).
const LINE_BREAK: char = '\u{1f}';

const BLOCK: &[&str] = &[
    "br", "p", "div", "tr", "td", "th", "li", "ul", "ol", "table", "h1", "h2", "h3", "h4", "h5", "h6", "section",
    "article", "header", "footer", "hr", "tbody", "center",
];

/// Splits HTML into links (with their text) and lines of plain text, in reading order.
pub fn segments(html: &str) -> Vec<Seg> {
    let mut out = Vec::new();
    let mut line = String::new();
    // (href, text, the text starts with an image description)
    let mut link: Option<(String, String, bool)> = None;
    let mut rest = html;

    // A link that holds a whole card (LinkedIn: title, company and place in one <a>): its first
    // line is the link text, the other lines follow as plain lines.
    fn push_link(out: &mut Vec<Seg>, (href, text, image): (String, String, bool)) {
        let mut parts = text.split(LINE_BREAK).map(|p| one_line(&decode_entities(p), 300)).filter(|p| !p.is_empty());
        out.push(Seg::Link { href, text: parts.next().unwrap_or_default(), image });
        out.extend(parts.map(Seg::Line));
    }

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
            Some((_, t, _)) => t.push_str(text),
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
                if let Some(l) = link.take() {
                    push_link(&mut out, l);
                }
            } else {
                flush(&mut line, &mut out);
                if let Some(l) = link.take() {
                    push_link(&mut out, l);
                }
                link = Some((attr(tag, "href").unwrap_or_default(), String::new(), false));
            }
            continue;
        }
        if name == "img" {
            // An image link (logo) has no text; its description may name the company.
            if let (Some((_, t, image)), Some(alt)) = (link.as_mut(), attr(tag, "alt")) {
                if t.chars().all(|c| c.is_whitespace() || c == LINE_BREAK) && !alt.trim().is_empty() {
                    t.push_str(&alt);
                    *image = true;
                }
            }
            continue;
        }
        if BLOCK.contains(&name.as_str()) {
            match link.as_mut() {
                Some((_, t, _)) => t.push(LINE_BREAK),
                None => flush(&mut line, &mut out),
            }
        }
    }
    if let Some(l) = link.take() {
        push_link(&mut out, l);
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
        out.push(Seg::Link { href: urls[0].to_string(), text: label, image: false });
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

/// The address of a single offer without what changes from link to link, so the logo, the title
/// and "apply" of one card are one offer; `None` when `url` is not an offer.
fn offer_url(url: &Url, portal: &Portal) -> Option<String> {
    let path = url.path().to_ascii_lowercase();
    let digits = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
    let param = |name: &str| url.query_pairs().find(|(k, _)| k == name).map(|(_, v)| v.into_owned());
    let host = url.host_str().unwrap_or_default();
    let listed = portal.offer_paths.iter().any(|p| path.contains(p));
    let known = match portal.source {
        // `/comm/jobs/view/123/?trackingId=…&lipi=…`: every link of one card has another query.
        "linkedin" => path
            .split("/jobs/view/")
            .nth(1)
            .and_then(|r| r.split('/').next())
            .filter(|id| digits(id))
            .map(|id| format!("https://www.linkedin.com/jobs/view/{id}/")),
        // `/rc/clk/dl?jk=<id>&from=ja&qd=…`: the offer is `jk`.
        "indeed" => param("jk")
            .filter(|jk| listed && !jk.is_empty() && jk.bytes().all(|b| b.is_ascii_alphanumeric()))
            .map(|jk| format!("https://{host}/viewjob?jk={jk}")),
        // `/land/ad/<id>?v=…` or a link to the search with the offer on top (`topdoc_id=<id>`).
        s if s == super::adzuna::SOURCE => ["/land/ad/", "/details/"]
            .iter()
            .find_map(|p| path.split(p).nth(1))
            .and_then(|r| r.split('/').next())
            .map(str::to_string)
            .or_else(|| param("topdoc_id"))
            .filter(|id| digits(id))
            .map(|id| format!("https://{host}/land/ad/{id}")),
        _ => None,
    };
    known.or_else(|| {
        listed.then(|| {
            // The path names the offer (`?sendid=…`, `?criteria=…` change from mail to mail);
            // Jooble's `/away/<id>` may need its query to lead anywhere.
            let mut u = url.clone();
            if portal.source != super::jooble::SOURCE {
                u.set_query(None);
            }
            clean_url(&u)
        })
    })
}

/// Text hidden in base64 (standard or URL-safe, padding optional); `None` for anything else.
fn base64_text(s: &str) -> Option<String> {
    let mut bits: u32 = 0;
    let mut count = 0;
    let mut out = Vec::with_capacity(s.len() * 3 / 4);
    for c in s.trim_end_matches('=').bytes() {
        let v = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' | b'-' => 62,
            b'/' | b'_' => 63,
            _ => return None,
        };
        bits = (bits << 6) | u32::from(v);
        count += 6;
        if count >= 8 {
            count -= 8;
            out.push((bits >> count) as u8);
            bits &= (1 << count) - 1;
        }
    }
    String::from_utf8(out).ok()
}

/// "10 ofert", "5 jobs for you": a link to a list of offers, never one offer.
fn offer_count(text: &str) -> bool {
    let n = normalize(text);
    let mut words = n.split_whitespace();
    words.next().is_some_and(|w| w.chars().all(|c| c.is_ascii_digit()))
        && words.any(|w| ["ofert", "offer", "job"].iter().any(|s| w.starts_with(s)))
}

fn blocked(text: &str) -> bool {
    let n = normalize(text);
    NOT_OFFER.iter().any(|w| n.contains(w)) || NOT_OFFER_EXACT.contains(&n.as_str()) || offer_count(text)
}

/// "3 dni temu", "1 dzień temu", "Dodano przed chwilą", "2 days ago": when the alert saw the offer.
/// `n` - a normalized line.
fn age_line(n: &str) -> bool {
    n.split_whitespace().count() <= 5
        && (n.ends_with(" temu") || n.ends_with(" ago") || n.ends_with("przed chwila") || ["just posted", "dzisiaj", "today"].contains(&n))
}

/// The company from a logo's description: "Firma X logo" → "Firma X"; "logo", "logo firmy" → none.
fn logo_company(alt: &str) -> Option<String> {
    let n = normalize(alt);
    if n == "logo" || n.starts_with("logo ") {
        return None;
    }
    let t = alt.trim();
    let t = match t.len().checked_sub(5).and_then(|i| t.get(i..).map(|end| (i, end))) {
        Some((i, end)) if end.eq_ignore_ascii_case(" logo") => t[..i].trim(),
        _ => t,
    };
    ((2..=80).contains(&t.chars().count()) && t.chars().any(char::is_alphabetic)).then(|| t.to_string())
}

/// How sure we are that a link points to an offer.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Kind {
    /// Address of a single offer of a known portal.
    Offer(&'static Portal),
    /// Tracking address of the portal that sent the alert - an offer only when its text looks like a title.
    Tracked(&'static Portal),
}

/// The link and the addresses it carries: an offer's address wrapped in a tracking link
/// (`?url=https://…`) or in base64 as a part of the path (`mail-link.nofluffjobs.com/click/<mail>/aHR0cHM6…`).
fn candidates(url: &Url) -> Vec<Url> {
    let mut out = vec![url.clone()];
    for (_, value) in url.query_pairs() {
        if value.starts_with("http") {
            if let Ok(inner) = Url::parse(&value) {
                out.push(inner);
            }
        }
    }
    for part in url.path_segments().into_iter().flatten().filter(|p| p.len() >= 16) {
        if let Some(inner) = base64_text(part).filter(|t| t.starts_with("http")).and_then(|t| Url::parse(&t).ok()) {
            out.push(inner);
        }
    }
    out
}

/// The same job offer of a portal, whatever its link looked like in a given mail (tracking
/// parameters, `/comm/`, a link wrapped in base64, Jooble's `?mid=…`). `None` - not a known
/// portal's offer. Lets a deleted offer stay deleted when a better reading of the same mail
/// gives it another company or title.
pub fn same_offer_key(link: &str) -> Option<String> {
    let url = Url::parse(link).ok().filter(is_web)?;
    candidates(&url).iter().filter(|c| is_web(c)).find_map(|c| {
        let portal = c.host_str().and_then(|h| portal_of_host(&h.to_ascii_lowercase()))?;
        offer_url(c, portal).map(|u| offer_key(&u))
    })
}

/// What tells two links of one source apart: the portal's offer, or else the address without
/// tracking parameters - the same job fetched again is not a second link.
pub fn link_key(link: &str) -> String {
    if let Some(key) = same_offer_key(link) {
        return key;
    }
    match Url::parse(link) {
        Ok(mut url) => {
            url.set_fragment(None);
            clean_url(&url)
        }
        Err(_) => link.to_string(),
    }
}

fn classify(href: &str, sender: Option<&'static Portal>) -> Option<(Kind, String)> {
    let url = Url::parse(href).ok().filter(is_web)?;
    let host = url.host_str()?.to_ascii_lowercase();
    if blocked(url.path()) {
        return None;
    }
    for c in candidates(&url).iter().filter(|c| is_web(c)) {
        if let Some(portal) = c.host_str().and_then(|h| portal_of_host(&h.to_ascii_lowercase())) {
            if let Some(offer) = offer_url(c, portal) {
                return Some((Kind::Offer(portal), offer));
            }
        }
    }
    let portal = portal_of_host(&host).or(sender)?;
    // A tracking link of the sending portal, or any link in its alert (e.g. a click-tracking service).
    if portal.tracked && sender.is_some_and(|s| std::ptr::eq(s, portal)) {
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
    // The description of a company logo ("logo firmy", "Firma X logo") is never a job title.
    let logo = n == "logo" || n.starts_with("logo ") || n.ends_with(" logo");
    (3..=150).contains(&len)
        && !logo
        && text.chars().any(char::is_alphabetic)
        && !blocked(text)
        && !NOISE.contains(&n.as_str())
        && !age_line(&n)
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
    // A day or week rate is no month - left out (the amount stays in the text), never "too low".
    if ["dzień", "dzien", "dziennie", "/day", "per day", "daily", "/d ", "tydz", "week"].iter().any(|w| lower.contains(w)) {
        return None;
    }
    // "120 000 zł rocznie": a year, counted as a month.
    let yearly = ["rocznie", "/rok", "na rok", "year", "annual", "/yr", "p.a."].iter().any(|w| lower.contains(w));

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
    // "100 zł" (Jooble): an amount this small is per hour, even when the alert does not say so.
    let hourly = hourly
        || (!currency.is_empty() && !thousands_word && !numbers.is_empty() && numbers.iter().all(|v| (5.0..500.0).contains(v)));
    if yearly && !hourly {
        numbers.iter_mut().for_each(|v| *v /= 12.0);
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
pub fn parse_place(line: &str) -> Option<(Option<String>, Option<Remote>)> {
    let n = normalize(line);
    // Both kinds of words ("Warszawa (hybrydowo, 2 dni zdalnie)", Jooble's tags "Praca hybrydowa
    // Praca zdalna"): no guess - the filter says "unsure" instead of a wrong reject or pass.
    let (remote_word, hybrid_word) = (REMOTE_WORDS.iter().any(|w| n.contains(w)), HYBRID_WORDS.iter().any(|w| n.contains(w)));
    let remote = match (remote_word, hybrid_word) {
        (true, false) => Some(Remote::Yes),
        (false, true) => Some(Remote::Hybrid),
        _ => None,
    };
    // "in Warszawa and also remotely" (No Fluff Jobs).
    let city = line
        .split([',', '|', '(', ')', '·', '/', '•'])
        .flat_map(|p| p.split(" and "))
        .map(|p| p.trim().strip_prefix("in ").unwrap_or(p.trim()).trim())
        .find(|part| cities::coordinates(part).is_some())
        .or_else(|| in_region(line))
        .map(|c| c.to_string());
    if city.is_none() && !remote_word && !hybrid_word {
        return None;
    }
    // A long sentence that happens to name a city is not a place line.
    if line.chars().count() > 80 {
        return None;
    }
    Some((city, remote))
}

const REGIONS: &[&str] = &[
    "dolnoslaskie", "kujawsko pomorskie", "lubelskie", "lubuskie", "lodzkie", "malopolskie", "mazowieckie", "opolskie",
    "podkarpackie", "podlaskie", "pomorskie", "slaskie", "swietokrzyskie", "warminsko mazurskie", "wielkopolskie",
    "zachodniopomorskie",
];

/// A Polish region ("mazowieckie", "Województwo pomorskie"), not a town.
pub fn is_region(text: &str) -> bool {
    let n = normalize(text);
    REGIONS.contains(&n.strip_prefix("wojewodztwo ").unwrap_or(&n))
}

/// "Nowa Sól, lubuskie" (Indeed, Adzuna): a town the city list does not know, named with its region.
fn in_region(line: &str) -> Option<&str> {
    let (town, region) = line.rsplit_once(',')?;
    let town = town.trim();
    (is_region(region) && (2..=40).contains(&town.chars().count()) && !town.contains(','))
        .then_some(town)
}

// ---------- offers ----------

#[derive(Debug, Default)]
struct Draft {
    source: &'static str,
    url: String,
    title: String,
    details: Vec<String>,
    /// The link has the address of a single offer (not only a tracking link of the sender).
    sure: bool,
    /// Named by the card's logo.
    company: Option<String>,
}

/// Offers in one alert e-mail. `sender` - the From address, `html` / `text` - its bodies.
pub fn offers_in_mail(sender: &str, html: Option<&str>, text: Option<&str>) -> Vec<OfferInput> {
    let sender_portal = portal_of_sender(sender);
    let segs = match (html, text) {
        (Some(h), _) if !h.trim().is_empty() => segments(h),
        (_, Some(t)) => text_segments(t),
        _ => return Vec::new(),
    };
    // A mail with real offer addresses links its offers that way - its tracking links are the
    // header (the search and its area), banners, company names or ads, never offers.
    let has_sure = segs.iter().any(|s| {
        matches!(s, Seg::Link { href, text, .. } if !offer_count(text)
            && matches!(classify(href, sender_portal), Some((Kind::Offer(_), _))))
    });
    let mut drafts: Vec<Draft> = Vec::new();
    // Lines after a skipped link belong to it, not to the offer before.
    let mut open = true;
    for seg in segs {
        match seg {
            Seg::Link { href, text, image } => {
                let Some((kind, url)) = classify(&href, sender_portal) else {
                    // "Zobacz wszystkie oferty pracy" ends the cards - the ads and the footer after it are no details.
                    if blocked(&text) {
                        open = false;
                    }
                    // Other links (e.g. the company page) may still carry details.
                    if let Some(last) = drafts.last_mut().filter(|_| open) {
                        if !text.is_empty() && !blocked(&text) && last.details.len() < MAX_DETAILS {
                            last.details.push(text);
                        }
                    }
                    continue;
                };
                // "10 ofert", "Zobacz wszystkie", "Oferty pracy podobne do tej: …": a list or a
                // header, even with an address that looks like an offer.
                if !image && blocked(&text) {
                    open = false;
                    continue;
                }
                // A logo names the company (LinkedIn, Pracuj.pl), never the job.
                let logo = if image { logo_company(&text) } else { None };
                let text = if image { String::new() } else { text };
                if has_sure && matches!(kind, Kind::Tracked(_)) {
                    // Like any other link: its text (e.g. the company name) may be a detail.
                    if let Some(last) = drafts.last_mut().filter(|_| open) {
                        if !text.is_empty() && !blocked(&text) && last.details.len() < MAX_DETAILS {
                            last.details.push(text);
                        }
                    }
                    continue;
                }
                if let Some(last) = drafts.last_mut() {
                    // Logo, title and "apply" usually point to the same offer.
                    if last.url == url {
                        open = true;
                        // At the start of a card; a logo after its lines is the job board (Adzuna: "Pracuj").
                        if last.company.is_none() && last.details.is_empty() {
                            last.company = logo;
                        }
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
                    if let Some(last) = drafts.last_mut().filter(|_| open) {
                        if !text.is_empty() && !blocked(&text) && last.details.len() < MAX_DETAILS {
                            last.details.push(text);
                        }
                    }
                    continue;
                }
                if drafts.len() >= MAX_OFFERS_PER_MAIL {
                    break;
                }
                open = true;
                drafts.push(Draft {
                    source: portal.source,
                    url,
                    title: if title_like(&text) { text } else { String::new() },
                    details: Vec::new(),
                    sure,
                    company: logo,
                });
            }
            Seg::Line(line) => {
                if let Some(last) = drafts.last_mut().filter(|_| open) {
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
    // "Company • City" and "✅ Suggested" / "💼 Hybrid" (LinkedIn): one fact per line, no icons in front.
    d.details = d
        .details
        .iter()
        .flat_map(|l| l.split(['•', '·']))
        .map(without_marks)
        .flat_map(|l| company_and_place(&l))
        .filter(|l| !l.is_empty())
        .collect();
    d.title = without_marks(&d.title);
    if d.title.is_empty() {
        pick_title(&mut d)?;
    }
    let mut salary = None;
    let mut city = None;
    let mut remote = None;
    let mut company = None;
    let mut rest = Vec::new();
    // "miasto Gdynia, 16 km": the search area from the alert's header - the link is the list of results.
    if d.details.iter().any(|l| search_radius(l)) {
        return None;
    }
    let logo = d.company.as_deref().map(normalize);
    for line in &d.details {
        let n = normalize(line);
        if n.is_empty() || NOISE.contains(&n.as_str()) || age_line(&n) || blocked(line) || *line == d.title || logo.as_deref() == Some(&n) {
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
        if company.is_none() && d.company.is_none() && line.chars().count() <= 80 && line.chars().any(char::is_alphabetic) {
            company = Some(line.clone());
            continue;
        }
        rest.push(line.clone());
    }
    // A tracking link alone is an offer only with a place or salary - "Wznów Premium za 0 zł",
    // a company name or a banner has neither.
    if !d.sure && salary.is_none() && city.is_none() && remote.is_none() {
        return None;
    }
    // Without a company two different "React Developer" offers would look the same -
    // a short piece of the link keeps them apart until AI or the user fills it in.
    let company = d.company.take().or(company).unwrap_or_else(|| format!("? ({})", short_id(&offer_key(&d.url))));
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

/// Icons and marks in front of a line ("! AI Engineer", "✅ Sugerowane") go; a line that starts
/// with a letter, a digit, a bracket or a quote stays as it is.
fn without_marks(line: &str) -> String {
    line.trim_start_matches(|c: char| !(c.is_alphanumeric() || "([„\"".contains(c))).trim().to_string()
}

/// "GEDIA Sp. z o.o. - Nowa Sól, lubuskie", "Firma - Zielona Góra - from 17000 to 18000 PLN per month"
/// (Adzuna): the company, the place and the salary in one line.
fn company_and_place(line: &str) -> Vec<String> {
    let parts: Vec<&str> = line.split(" - ").collect();
    match (1..parts.len()).find(|&i| matches!(parse_place(parts[i]), Some((Some(_), _)))) {
        Some(i) => std::iter::once(parts[..i].join(" - ")).chain(parts[i..].iter().map(|p| p.to_string())).map(|p| p.trim().to_string()).collect(),
        None => vec![line.to_string()],
    }
}

/// A card without a title link: "Company, City, Title" (justjoin.it, RocketJobs) or
/// "Title, Company, City" (LinkedIn). `None` when nothing looks like a title.
fn pick_title(d: &mut Draft) -> Option<()> {
    let logo = d.company.as_deref().map(normalize);
    let place = |l: &str| parse_place(l).is_some();
    let candidate = |l: &str| title_like(l) && !place(l) && logo.as_deref() != Some(&normalize(l));
    let near: Vec<usize> = (0..d.details.len().min(4)).filter(|&i| candidate(&d.details[i])).collect();
    let salary = |l: &str| parse_salary(l).is_some() || ["undisclosed salary", "wynagrodzenie nieujawnione"].contains(&normalize(l).as_str());
    // The line just above the salary (justjoin.it: company, city, title, salary).
    let above_salary = near.iter().copied().filter(|&i| i > 0).find(|&i| d.details.get(i + 1).is_some_and(|l| salary(l)));
    let pos = if let Some(i) = above_salary {
        if d.company.is_none() && near.first().is_some_and(|&f| f < i) {
            d.company = Some(d.details[near[0]].clone());
        }
        i
    } else if d.details.len() >= 3 && candidate(&d.details[0]) && place(&d.details[1]) && candidate(&d.details[2]) {
        // Company, city, title.
        if d.company.is_none() {
            d.company = Some(d.details[0].clone());
        }
        2
    } else {
        // The line whose words make up the address (`/job-offer/firma-x-website-builder-…`).
        let slug: std::collections::HashSet<String> = Url::parse(&d.url)
            .map(|u| normalize(u.path()).split_whitespace().map(str::to_string).collect())
            .unwrap_or_default();
        let score = |i: &usize| normalize(&d.details[*i]).split_whitespace().filter(|w| slug.contains(*w)).count();
        let best = near.iter().max_by_key(|i| score(i)).filter(|i| score(i) >= 2);
        let unique = best.is_some_and(|b| near.iter().filter(|i| score(i) == score(b)).count() == 1);
        match best {
            Some(b) if unique => *b,
            _ => match near.first() {
                Some(&i) => i,
                None => (0..d.details.len()).find(|&i| candidate(&d.details[i]))?,
            },
        }
    };
    d.title = d.details.remove(pos);
    Some(())
}

/// "Gdynia, 16 km", "miasto Olsztyn, 25 km": the area of a saved search, not of an offer.
fn search_radius(line: &str) -> bool {
    let n = normalize(line);
    let words: Vec<&str> = n.split_whitespace().collect();
    words.len() >= 3 && words.len() <= 8 && words[words.len() - 1] == "km" && words[words.len() - 2].chars().all(|c| c.is_ascii_digit())
}

/// The part of an offer address that names the offer. Jooble adds the mail, position and alert
/// to every link (`?mid=…&pos=3`), so the same offer from two mails had two ids.
fn offer_key(url: &str) -> String {
    match Url::parse(url) {
        Ok(u) if u.host_str().and_then(portal_of_host).is_some_and(|p| p.source == super::jooble::SOURCE) => {
            format!("{}{}", u.host_str().unwrap_or_default(), u.path())
        }
        _ => url.to_string(),
    }
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
    fn a_card_in_one_link_and_a_logo_description_are_read_right() {
        // LinkedIn: the whole card is one link; the lines inside are title, "company • city", marks.
        let html = r#"<a href="https://www.linkedin.com/comm/jobs/view/777/?trk=eml"><table><tr><td><p>AI Engineer (freelance)</p><p>Przykładowa Firma • Gdańsk</p><p>✅ Sugerowane</p><p>💼 Praca hybrydowa</p></td></tr></table></a>"#;
        let offers = offers_in_mail("jobs-noreply@linkedin.com", Some(html), None);
        assert_eq!(offers.len(), 1);
        assert_eq!(offers[0].title, "AI Engineer (freelance)");
        assert_eq!(offers[0].company, "Przykładowa Firma");
        assert_eq!(offers[0].city.as_deref(), Some("Gdańsk"));
        assert_eq!(offers[0].remote, Some(Remote::Hybrid));
        // A logo link first ("logo firmy"), then the title with a mark in front.
        let html = r#"<a href="https://www.pracuj.pl/praca/x,oferta,1234"><img alt="logo firmy"></a>
            <a href="https://www.pracuj.pl/praca/x,oferta,1234">! AI Software Engineer</a><br>20 000-24 000 zł netto (+ VAT) / mies.<br>Zmyślona Sp. z o.o.<br>Warszawa, Śródmieście"#;
        let offers = offers_in_mail("noreply@pracuj.pl", Some(html), None);
        assert_eq!(offers.len(), 1);
        assert_eq!(offers[0].title, "AI Software Engineer");
        assert_eq!(offers[0].company, "Zmyślona Sp. z o.o.");
        assert_eq!(offers[0].salary_min, Some(20000));
    }

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
        // A day rate is no month; a year is counted as a month.
        assert_eq!(s("1 000 - 1 400 zł/dzień netto (B2B)"), None);
        assert_eq!(s("120 000 - 144 000 zł rocznie"), Some((10000, Some(12000), "PLN".into(), SalaryPeriod::Month)));
    }

    #[test]
    fn places() {
        assert_eq!(parse_place("Wrocław, Kraków + 2"), Some((Some("Wrocław".into()), None)));
        assert_eq!(parse_place("Zdalnie"), Some((None, Some(Remote::Yes))));
        assert_eq!(parse_place("Warszawa (hybrydowo)"), Some((Some("Warszawa".into()), Some(Remote::Hybrid))));
        assert_eq!(parse_place("Przykładowa Firma"), None);
        assert_eq!(parse_place("Warszawa (hybrydowo, 2 dni zdalnie)"), Some((Some("Warszawa".into()), None)));
        assert!(is_region("mazowieckie") && is_region("Województwo pomorskie") && !is_region("Gdańsk"));
    }

    #[test]
    fn segments_keep_link_text_and_lines() {
        let html = r#"<table><tr><td><a href="https://justjoin.it/job-offer/firma-x-react-dev?utm_source=mail&amp;x=1"><img alt="Firma X logo"></a></td>
            <td><a href="https://justjoin.it/job-offer/firma-x-react-dev?utm_source=mail&amp;x=1"><b>React</b> Developer</a><br>Firma X<br>Wrocław</td></tr></table>"#;
        let segs = segments(html);
        assert_eq!(segs[0], Seg::Link { href: "https://justjoin.it/job-offer/firma-x-react-dev?utm_source=mail&x=1".into(), text: "Firma X logo".into(), image: true });
        assert_eq!(segs[1], Seg::Link { href: "https://justjoin.it/job-offer/firma-x-react-dev?utm_source=mail&x=1".into(), text: "React Developer".into(), image: false });
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
            <p><a href="https://click.pracuj.pl/c/def456">Specjalista ds. kadr</a></p><p>Firma Kadrowa</p><p>Opole</p><p>6 500 - 7 500 zł brutto / mies.</p>"#;
        let offers = offers_in_mail("Pracuj.pl <powiadomienia@pracuj.pl>", Some(html), None);
        assert_eq!(offers.len(), 2);
        assert_eq!(offers[0].title, "Księgowa / Księgowy");
        assert_eq!(offers[0].source, "pracuj");
        assert_eq!(offers[1].company, "Firma Kadrowa");
        assert_eq!(offers[1].salary_min, Some(6500));

        // From an unknown sender a tracking link is not trusted.
        assert!(offers_in_mail("ktos@example.com", Some(html), None).is_empty());

        // Next to real offer addresses a tracking link is not an offer of its own.
        let html = r#"<p><a href="https://click.pracuj.pl/c/abc123?x=1">Księgowa / Księgowy</a></p><p>Biuro Rachunkowe Zmyślone</p><p>Opole</p>
            <p><a href="https://www.pracuj.pl/praca/specjalista-ds-kadr-opole,oferta,1000123456">Specjalista ds. kadr</a></p><p>Firma Kadrowa</p><p>Opole</p>"#;
        let offers = offers_in_mail("Pracuj.pl <powiadomienia@pracuj.pl>", Some(html), None);
        assert_eq!(offers.iter().map(|o| o.title.as_str()).collect::<Vec<_>>(), ["Specjalista ds. kadr"]);
    }

    #[test]
    fn ads_headers_and_company_links_are_not_offers() {
        // An ad from the portal: tracking links only, no place, no salary.
        let html = r#"<p><a href="https://www.linkedin.com/comm/premium/products/?trk=eml">Wznów Premium za 0 zł</a></p>
            <p>Wypróbuj przez miesiąc za darmo</p><p><a href="https://www.linkedin.com/comm/premium/products/?trk=btn">Wypróbuj teraz</a></p>"#;
        assert!(offers_in_mail("LinkedIn <messages-noreply@linkedin.com>", Some(html), None).is_empty());

        // An alert: header with the search and its area (a link to the list of results), then
        // cards with the company as a separate link.
        let html = r#"<p><a href="https://pl.jooble.org/"><img alt="Jooble"></a></p><p>Twoje codzienne powiadomienia o ofertach pracy dla 07 listopada, 2026</p>
            <p><a href="https://pl.jooble.org/SearchResult?ukw=programista+wordpress&amp;rgns=Gdynia">Programista WordPress</a></p><p>miasto Gdynia, 16 km</p>
            <p><a href="https://pl.jooble.org/away/123456789?mid=a&amp;pos=1">Specjalista WordPress</a></p><p><a href="https://pl.jooble.org/company/zmyslona">Zmyślona Firma</a></p><p>Gdynia</p>"#;
        let offers = offers_in_mail("Jooble <alerty@jooble.com>", Some(html), None);
        assert_eq!(offers.iter().map(|o| o.title.as_str()).collect::<Vec<_>>(), ["Specjalista WordPress"]);
        assert_eq!(offers[0].company, "Zmyślona Firma");
        assert_eq!(offers[0].city.as_deref(), Some("Gdynia"));

        // The search area alone, even under an offer-like address.
        let html = r#"<p><a href="https://pl.jooble.org/desc/1">Programista AI</a></p><p>miasto Olsztyn, 16 km</p>"#;
        assert!(offers_in_mail("Jooble <alerty@jooble.com>", Some(html), None).is_empty());
    }

    #[test]
    fn the_same_jooble_offer_from_two_mails_gets_the_same_id() {
        let mail = |mid: &str| format!(r#"<p><a href="https://pl.jooble.org/away/42?mid={mid}&amp;pos=3">Tester aplikacji</a></p><p>Zdalnie</p>"#);
        let a = offers_in_mail("Jooble <alerty@jooble.com>", Some(&mail("a1")), None);
        let b = offers_in_mail("Jooble <alerty@jooble.com>", Some(&mail("b2")), None);
        assert!(a[0].company.starts_with("? ("));
        assert_eq!(a[0].company, b[0].company);
    }

    #[test]
    fn a_link_to_the_list_of_offers_is_not_an_offer() {
        let html = r#"<p><a href="https://click.pracuj.pl/c/list">10 nowych ofert</a></p><p>Programista WordPress</p><p>zdalnie</p>
            <p><a href="https://rocketjobs.pl/oferty-pracy/programista">5 ofert</a></p><p>Programista WordPress</p>
            <p><a href="https://click.pracuj.pl/c/all">Nowe oferty dla Ciebie</a></p>
            <p><a href="https://www.pracuj.pl/praca/programista,oferta,1000123456">Programista PHP</a></p><p>Firma Zmyślona</p><p>3 new jobs for you</p>"#;
        let offers = offers_in_mail("Pracuj.pl <powiadomienia@pracuj.pl>", Some(html), None);
        assert_eq!(offers.len(), 1);
        assert_eq!((offers[0].title.as_str(), offers[0].company.as_str()), ("Programista PHP", "Firma Zmyślona"));
        assert!(!offer_count("3D Artist") && !offer_count("2 Senior Developers") && !offer_count("Job Coach"));
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

    // Made-up alert in the shape of a Jooble subscription (the real one: SPRAWY in robota.md).
    #[test]
    fn jooble_alert() {
        let html = r#"<p><a href="https://pl.jooble.org/desc/-8124814081821320089?ckey=wordpress">Programista WordPress</a></p><p>Firma Testowa</p><p>Wrocław</p><p>9 000 - 12 000 PLN</p>
            <p><a href="https://pl.jooble.org/jdp/6680513090389885291?ckey=wordpress">Tester aplikacji</a></p><p>Zmyślona Sp. z o.o.</p><p>Zdalnie</p>
            <p><a href="https://pl.jooble.org/away/4415926535897932384?p=1&amp;pos=3">Analityk danych</a></p><p>Przykładowa Firma</p><p>Opole</p>
            <p><a href="https://click.jooble.com/t/abc123">Księgowa</a></p><p>Biuro Testowe</p>
            <p><a href="https://pl.jooble.org/unsubscribe?id=1">Wypisz się</a></p>"#;
        let offers = offers_in_mail("Jooble <alerty@jooble.com>", Some(html), None);
        // "Księgowa" has a tracking link only - next to real offer addresses that is not an offer.
        assert_eq!(offers.iter().map(|o| o.title.as_str()).collect::<Vec<_>>(), ["Programista WordPress", "Tester aplikacji", "Analityk danych"]);
        assert!(offers.iter().all(|o| o.source == "jooble"));
        assert_eq!(offers[0].company, "Firma Testowa");
        assert_eq!(offers[0].city.as_deref(), Some("Wrocław"));
        assert_eq!((offers[0].salary_min, offers[0].salary_max), (Some(9000), Some(12000)));
        assert_eq!(offers[1].remote, Some(Remote::Yes));
        assert_eq!(offers[2].url.as_deref(), Some("https://pl.jooble.org/away/4415926535897932384?p=1&pos=3"));

        // A forwarded alert: the address alone says it is a Jooble offer.
        let offers = offers_in_mail("ktos@example.com", Some(html), None);
        assert_eq!(offers.len(), 3);
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
        assert_eq!(offers[0].url.as_deref(), Some("https://www.linkedin.com/jobs/view/123456/"));
    }

    // The cases below come from real alerts (8.11.2026), with made-up companies and offers.

    #[test]
    fn linkedin_logo_link_names_the_company_not_a_second_offer() {
        let html = r#"<a href="https://www.linkedin.com/comm/jobs/search-results/?keywords=Tester">Twój alert o ofertach pracy – Tester</a>
            <a href="https://www.linkedin.com/comm/jobs/view/111/?trackingId=a&amp;lipi=x1"><img alt="Zmyślona Firma"></a>
            <a href="https://www.linkedin.com/comm/jobs/view/111/?trackingId=a&amp;lipi=x2"></a>
            <a href="https://www.linkedin.com/comm/jobs/view/111/?trackingId=a&amp;lipi=x3">Tester aplikacji</a><p>Zmyślona Firma · Warszawa (Praca hybrydowa)</p><p>Aktywnie rekrutuje</p>
            <a href="https://www.linkedin.com/comm/jobs/search-results/?keywords=Tester">Zobacz wszystkie oferty pracy</a>
            <a href="https://www.linkedin.com/comm/premium/products/?x=1">Wznów Premium za 0 zł</a>"#;
        let offers = offers_in_mail("jobalerts-noreply@linkedin.com", Some(html), None);
        assert_eq!(offers.len(), 1);
        assert_eq!((offers[0].title.as_str(), offers[0].company.as_str()), ("Tester aplikacji", "Zmyślona Firma"));
        assert_eq!(offers[0].city.as_deref(), Some("Warszawa"));
        assert_eq!(offers[0].remote, Some(Remote::Hybrid));
        assert_eq!(offers[0].url.as_deref(), Some("https://www.linkedin.com/jobs/view/111/"));
        assert_eq!(offers[0].requirements, None);

        // The whole card in one link with the logo first; the header names the offer the user saw.
        let html = r#"<a href="https://www.linkedin.com/comm/jobs/view/100?lipi=a">Oferty pracy podobne do tej: Tester w Firma Stara</a>
            <a href="https://www.linkedin.com/comm/jobs/view/222/?lipi=b"><img alt="Przykładowa Spółka"><p>Programista PHP</p><p>Przykładowa Spółka · Polska (Praca zdalna)</p></a>"#;
        let offers = offers_in_mail("jobs-noreply@linkedin.com", Some(html), None);
        assert_eq!(offers.len(), 1);
        assert_eq!((offers[0].title.as_str(), offers[0].company.as_str()), ("Programista PHP", "Przykładowa Spółka"));
        assert_eq!(offers[0].remote, Some(Remote::Yes));

        // "Companies hiring for your role": company pages, not offers ("2 tys. obserwujących" is not a salary).
        let html = r#"<a href="https://www.linkedin.com/comm/company/zmyslona-firma?lipi=c">Zmyślona Firma</a><p>1 nowy pracownik</p><p>2 tys. obserwujących</p>"#;
        assert!(offers_in_mail("messages-noreply@linkedin.com", Some(html), None).is_empty());
    }

    #[test]
    fn a_card_without_a_title_link_finds_its_title() {
        // justjoin.it / RocketJobs: logo, company, city, title, salary, form of work - all in one link.
        let html = r#"<a href="https://justjoin.it/job-offer/firma-x-tester-aplikacji-gdansk-testing?utm_source=mail"><img alt="logo"><p>Firma X</p><p>Gdańsk</p><p>Tester aplikacji</p><p>Undisclosed salary</p><p>Remote</p><p>Be the first to apply!</p></a>
            <a href="https://justjoin.it/job-offer/firma-y-programista-php-praga?utm_source=mail"><img alt="logo"><p>Firma Y</p><p>Praga Czeska</p><p>Programista PHP</p><p>9000 - 12000 PLN</p></a>
            <p>Didn't find anything for yourself?</p><a href="https://justjoin.it/job-offers/all-locations">Show job offers</a>"#;
        let offers = offers_in_mail("no-reply@justjoin.it", Some(html), None);
        let got: Vec<_> = offers.iter().map(|o| (o.title.as_str(), o.company.as_str())).collect();
        assert_eq!(got, [("Tester aplikacji", "Firma X"), ("Programista PHP", "Firma Y")]);
        assert_eq!(offers[0].url.as_deref(), Some("https://justjoin.it/job-offer/firma-x-tester-aplikacji-gdansk-testing"));
        assert_eq!(offers[0].requirements, None);
        assert_eq!(offers[1].salary_min, Some(9000));

        // RocketJobs: "Pokaż oferty" leads to a list (`/oferty-pracy/…`).
        let html = r#"<a href="https://rocketjobs.pl/oferty-pracy/wszystkie-lokalizacje">Pokaż oferty</a><p>Masz pytania?</p>"#;
        assert!(offers_in_mail("no-reply@rocketjobs.pl", Some(html), None).is_empty());
    }

    #[test]
    fn no_fluff_jobs_addresses_are_decoded() {
        let html = r#"<a href="https://mail-link.nofluffjobs.com/view/abc/def">Zobacz w przeglądarce</a>
            <a href="https://mail-link.nofluffjobs.com/click/abc/aHR0cHM6Ly9ub2ZsdWZmam9icy5jb20vcGwvP2NyaXRlcmlhPWNvbXBhbnklM0QlMjd6bXlzbG9uYSUyNw/x">company banner</a>
            <p>Twoje kryteria: remote 6 000+ PLN / month</p>
            <a href="https://mail-link.nofluffjobs.com/click/abc/aHR0cHM6Ly9ub2ZsdWZmam9icy5jb20vcGwvam9iL3Rlc3Rlci1hcGxpa2Fjamktem15c2xvbmEtcmVtb3RlP3V0bV9zb3VyY2U9ZW1haWw/y">Tester aplikacji</a>
            <p>Check Salary Match</p><p>Zmyślona</p><p>in Warszawa and also remotely</p>"#;
        let offers = offers_in_mail("notifications@nofluffjobs.com", Some(html), None);
        assert_eq!(offers.len(), 1);
        assert_eq!((offers[0].title.as_str(), offers[0].company.as_str()), ("Tester aplikacji", "Zmyślona"));
        assert_eq!(offers[0].url.as_deref(), Some("https://nofluffjobs.com/pl/job/tester-aplikacji-zmyslona-remote"));
        assert_eq!((offers[0].city.as_deref(), offers[0].remote), (Some("Warszawa"), Some(Remote::Yes)));

        // An ad mail of the portal ("Claim your benefits") has tracking links only.
        let html = r#"<a href="https://mail-link.nofluffjobs.com/view/abc/def">View in browser</a><p>6 000+ PLN</p>
            <a href="https://mail-link.nofluffjobs.com/click/abc/aHR0cHM6Ly9ub2ZsdWZmam9icy5jb20vcGwvP2NyaXRlcmlhPWNvbXBhbnklM0QlMjd6bXlzbG9uYSUyNw/x">Get benefits for subscribers</a>"#;
        assert!(offers_in_mail("notifications@nofluffjobs.com", Some(html), None).is_empty());
    }

    #[test]
    fn badges_ages_and_job_titles_with_common_words() {
        // Pracuj.pl: a badge between the title and the company.
        let html = r#"<a href="https://pracuj.pl/praca/x,oferta,1?sendid=a"><img alt="logo firmy"></a><a href="https://pracuj.pl/praca/x,oferta,1?sendid=a">PR Manager</a>
            <p>Najlepiej dopasowana</p><a href="https://pracuj.pl/praca/x,oferta,1?sendid=a">Zmyślona Sp. z o.o.</a><p>Kraków (Podgórze)</p>
            <a href="https://pracuj.pl/praca/y,oferta,2?sendid=a">Help Desk Specialist</a><p>Hit!</p><a href="https://pracuj.pl/praca/y,oferta,2?sendid=a">Firma Testowa</a><p>Opole</p>"#;
        let offers = offers_in_mail("rekomendacje@wysylka.pracuj.pl", Some(html), None);
        let got: Vec<_> = offers.iter().map(|o| (o.title.as_str(), o.company.as_str())).collect();
        assert_eq!(got, [("PR Manager", "Zmyślona Sp. z o.o."), ("Help Desk Specialist", "Firma Testowa")]);
        assert_eq!(offers[0].url.as_deref(), Some("https://pracuj.pl/praca/x,oferta,1"));

        // Indeed: "Manager" in the title, the age of the offer, the offer key `jk`.
        let html = r#"<a href="https://pl.indeed.com/rc/clk/dl?jk=ab12cd34&amp;from=ja&amp;qd=x">Senior Project Manager</a><p>Zmyślona Firma</p><p>Gdańsk, pomorskie</p><p>Aplikuj bezpośrednio</p><p>3 dni temu</p>
            <a href="https://pl.indeed.com/rc/clk/dl?jk=ef56&amp;from=ja">Programista</a><p>Firma Testowa</p><p>Nowa Wieś Testowa, lubuskie</p><p>Dodano przed chwilą</p>"#;
        let offers = offers_in_mail("donotreply@jobalert.indeed.com", Some(html), None);
        assert_eq!(offers.len(), 2);
        assert_eq!((offers[0].title.as_str(), offers[0].company.as_str()), ("Senior Project Manager", "Zmyślona Firma"));
        assert_eq!(offers[0].url.as_deref(), Some("https://pl.indeed.com/viewjob?jk=ab12cd34"));
        assert_eq!(offers[0].requirements, None);
        assert_eq!(offers[1].city.as_deref(), Some("Nowa Wieś Testowa"));
    }

    #[test]
    fn jooble_amount_without_a_period_is_hourly() {
        let html = r#"<a href="https://pl.jooble.org/away/77?pos=0&amp;mid=a">QA Engineer</a><p>100 zł</p><p>Zmyślona Firma • Zdalna</p><p>✅ Sugerowane Praca zdalna💼 Pełny etat</p>"#;
        let offers = offers_in_mail("subscribe@pl.jooble.org", Some(html), None);
        assert_eq!(offers.len(), 1);
        assert_eq!(offers[0].company, "Zmyślona Firma");
        assert_eq!((offers[0].salary_min, offers[0].salary_period), (Some(100), Some(SalaryPeriod::Hour)));
        assert_eq!(parse_salary("9000 zł").map(|s| s.3), Some(SalaryPeriod::Month));
    }

    #[test]
    fn the_same_offer_has_one_key_in_every_mail() {
        let k = |u: &str| same_offer_key(u);
        assert_eq!(k("https://www.linkedin.com/comm/jobs/view/111/?trackingId=a&lipi=x"), k("https://www.linkedin.com/jobs/view/111/"));
        assert_eq!(k("https://pl.jooble.org/away/42?mid=a&pos=1"), k("https://pl.jooble.org/away/42?mid=b&pos=7"));
        assert_eq!(k("https://www.pracuj.pl/praca/x,oferta,1?sendid=a"), k("https://www.pracuj.pl/praca/x,oferta,1"));
        assert_eq!(
            k("https://mail-link.nofluffjobs.com/click/abc/aHR0cHM6Ly9ub2ZsdWZmam9icy5jb20vcGwvam9iL3Rlc3Rlci1hcGxpa2Fjamktem15c2xvbmEtcmVtb3RlP3V0bV9zb3VyY2U9ZW1haWw/y"),
            k("https://nofluffjobs.com/pl/job/tester-aplikacji-zmyslona-remote")
        );
        assert_ne!(k("https://www.linkedin.com/jobs/view/111/"), k("https://www.linkedin.com/jobs/view/112/"));
        assert_eq!(k("https://example.com/job/1"), None);
    }

    #[test]
    fn adzuna_alert() {
        let html = r#"<a href="https://www.adzuna.pl?utm_source=alerts"><img alt="Adzuna"></a>
            <a href="https://www.adzuna.pl/search?q=tester&amp;topdoc_id=5001&amp;nid=1">Tester aplikacji</a>
            <p>OFERTA DOPASOWANA DLA CIEBIE</p><p>Zmyślona Sp. z o.o. - Nowa Wieś Testowa, lubuskie</p>
            <a href="https://www.adzuna.pl/search?q=tester&amp;topdoc_id=5001&amp;nid=1">więcej szczegółów &raquo;</a>
            <a href="https://www.adzuna.pl/search?q=tester&amp;topdoc_id=5001&amp;nid=1"><img alt="Pracuj"></a>
            <a href="https://www.adzuna.pl/land/ad/5002?v=AB&amp;se=x">Programista PHP</a>
            <p>REKRUTACJA ZAMYKA SIĘ WKRÓTCE</p><p>Firma - Testowa - Zielona Góra - from 17000 to 18000 PLN per month</p>
            <a href="https://www.adzuna.pl/search?q=tester">Pokaż wszystkie oferty pracy</a>"#;
        let offers = offers_in_mail("no-reply@adzuna.pl", Some(html), None);
        let got: Vec<_> = offers.iter().map(|o| (o.title.as_str(), o.company.as_str(), o.city.as_deref())).collect();
        assert_eq!(got, [("Tester aplikacji", "Zmyślona Sp. z o.o.", Some("Nowa Wieś Testowa")), ("Programista PHP", "Firma - Testowa", Some("Zielona Góra"))]);
        assert!(offers.iter().all(|o| o.source == "adzuna"));
        assert_eq!(offers[0].url.as_deref(), Some("https://www.adzuna.pl/land/ad/5001"));
        assert_eq!((offers[1].salary_min, offers[1].salary_max), (Some(17000), Some(18000)));
    }
}
