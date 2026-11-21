//! Dates of an offer from its text: when it was published and until when it is open.
//! Written dates ("04.12.2026", "26 lis", "11 November 2026") and dates counted from the moment
//! the text was taken ("1 godz. temu", "za 2 dni wygasa", "zostało 23 dni") or from the
//! publication ("Ważne przez 30 dni" on Useme) - plain code, no AI.

use time::format_description::well_known::Rfc3339;
use time::{Date, Duration, Month, OffsetDateTime, Time};

use crate::dedup::normalize;
use crate::text::one_line;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Published,
    Expires,
}

/// Words of a line that gives the publication date (normalized, start of a word).
const PUBLISHED_WORDS: &[&str] = &["opublikowan", "publikacj", "dodano", "data dodania", "published", "posted", "date posted"];
/// Words of a line that gives the end of the offer.
const EXPIRES_WORDS: &[&str] = &[
    "wazn", "wygas", "zostal", "aplikuj do", "termin skladania", "termin nadsylania", "termin aplikacji", "przyjmujemy zgloszenia do",
    "expires", "valid until", "valid till", "valid for", "deadline", "apply by", "apply before", "closing date", "days left",
];

/// A date without a time: published = its midnight, open until = the end of that day in Poland
/// (21:59:59 UTC stays the same day in winter and in summer). The form uses the same hours.
pub(crate) fn on_day(day: Date, kind: Kind) -> OffsetDateTime {
    let time = match kind {
        Kind::Published => Time::MIDNIGHT,
        Kind::Expires => Time::from_hms(21, 59, 59).unwrap_or(Time::MIDNIGHT),
    };
    day.with_time(time).assume_utc()
}

/// The month a word names, and whether it is English ("Nov 26" - the day may come after it).
fn month_of(word: &str) -> Option<(Month, bool)> {
    const MONTHS: [&[&str]; 12] = [
        &["sty", "stycznia", "styczen", "january", "jan"],
        &["lut", "lutego", "luty", "february", "feb"],
        &["mar", "marca", "marzec", "march"],
        &["kwi", "kwietnia", "kwiecien", "april", "apr"],
        &["maj", "maja", "may"],
        &["cze", "czerwca", "czerwiec", "june", "jun"],
        &["lip", "lipca", "lipiec", "july", "jul"],
        &["sie", "sierpnia", "sierpien", "august", "aug"],
        &["wrz", "wrzesnia", "wrzesien", "september", "sep", "sept"],
        &["paz", "pazdziernika", "pazdziernik", "october", "oct"],
        &["lis", "listopada", "listopad", "november", "nov"],
        &["gru", "grudnia", "grudzien", "december", "dec"],
    ];
    const ENGLISH: &[&str] = &[
        "january", "jan", "february", "feb", "march", "mar", "april", "apr", "may", "june", "jun", "july", "jul", "august", "aug",
        "september", "sep", "sept", "october", "oct", "november", "nov", "december", "dec",
    ];
    let i = MONTHS.iter().position(|names| names.contains(&word))?;
    Some((Month::try_from(i as u8 + 1).ok()?, ENGLISH.contains(&word)))
}

/// A day without a year ("do 26 lis"): this year, or the nearest one that makes sense - an offer
/// is not published in the future and does not end long ago.
fn guess_year(month: Month, day: u8, kind: Kind, now: OffsetDateTime) -> Option<Date> {
    let this = Date::from_calendar_date(now.year(), month, day).ok()?;
    let today = now.date();
    match kind {
        Kind::Published if this > today => Date::from_calendar_date(now.year() - 1, month, day).ok(),
        Kind::Expires if this < today - Duration::days(30) => Date::from_calendar_date(now.year() + 1, month, day).ok(),
        _ => Some(this),
    }
}

fn day_of(year: i32, month: u8, day: u8) -> Option<Date> {
    if !(2000..=2100).contains(&year) {
        return None;
    }
    Date::from_calendar_date(year, Month::try_from(month).ok()?, day).ok()
}

/// "04.12.2026", "4/12/2026", "2026-12-04".
fn numeric(line: &str) -> Vec<Date> {
    let kept: String = line.chars().map(|c| if c.is_ascii_digit() || matches!(c, '.' | '-' | '/') { c } else { ' ' }).collect();
    let mut out = Vec::new();
    for chunk in kept.split_whitespace() {
        let parts: Vec<&str> = chunk.split(['.', '-', '/']).filter(|p| !p.is_empty()).collect();
        for w in parts.windows(3) {
            let n = |s: &str| s.parse::<i32>().ok();
            let found = match (w[0].len(), w[2].len()) {
                (1 | 2, 4) => day_of(n(w[2]).unwrap_or(0), n(w[1]).unwrap_or(0) as u8, n(w[0]).unwrap_or(0) as u8),
                (4, 1 | 2) => day_of(n(w[0]).unwrap_or(0), n(w[1]).unwrap_or(0) as u8, n(w[2]).unwrap_or(0) as u8),
                _ => None,
            };
            out.extend(found);
        }
    }
    out
}

pub(crate) fn small_number(word: &str) -> Option<i64> {
    (word.len() <= 3).then(|| word.parse::<i64>().ok()).flatten()
}

/// "26 lis", "4 grudnia 2026", "Nov 26, 2026", "26 November".
fn named(words: &[&str], kind: Kind, now: OffsetDateTime) -> Vec<Date> {
    let mut out = Vec::new();
    for (i, w) in words.iter().enumerate() {
        let Some((month, english)) = month_of(w) else { continue };
        let day = |j: Option<usize>| j.and_then(|j| words.get(j)).and_then(|d| small_number(d)).filter(|d| (1..=31).contains(d));
        let (day, year_at) = match (day(i.checked_sub(1)), day(Some(i + 1))) {
            (Some(d), _) => (d, i + 1),
            (None, Some(d)) if english => (d, i + 2),
            _ => continue,
        };
        let year = words.get(year_at).filter(|y| y.len() == 4).and_then(|y| y.parse::<i32>().ok());
        let date = match year {
            Some(y) => day_of(y, month as u8, day as u8),
            None => guess_year(month, day as u8, kind, now),
        };
        out.extend(date);
    }
    out
}

/// How long one unit is ("godz." → an hour), `None` = not a unit of time.
pub(crate) fn unit(word: &str) -> Option<Duration> {
    let w = word;
    if w == "min" || w.starts_with("minut") {
        Some(Duration::minutes(1))
    } else if w == "h" || w == "godz" || w.starts_with("godzin") || w.starts_with("hour") || w == "hr" || w == "hrs" {
        Some(Duration::hours(1))
    } else if ["dzien", "dni", "dnia", "day", "days", "d"].contains(&w) {
        Some(Duration::days(1))
    } else if w.starts_with("tydz") || w.starts_with("tygod") || w.starts_with("week") {
        Some(Duration::weeks(1))
    } else if w.starts_with("miesi") || w.starts_with("month") {
        Some(Duration::days(30))
    } else {
        None
    }
}

pub(crate) const PAST: &[&str] = &["temu", "ago"];
pub(crate) const FUTURE: &[&str] = &["za", "in", "zostalo", "zostal", "zostaly", "jeszcze", "left", "remaining", "wygasa", "expires"];

/// "1 godz. temu", "za 2 dni", "zostało 23 dni", "a week ago", "wczoraj", "przed chwilą".
fn relative(words: &[&str], kind: Kind, now: OffsetDateTime) -> Vec<OffsetDateTime> {
    let has = |list: &[&str]| words.iter().any(|w| list.contains(w));
    let line = format!(" {} ", words.join(" "));
    let mut out = Vec::new();
    for (phrases, days) in [
        (&[" przed chwila ", " just posted ", " just now ", " dzisiaj ", " dzis ", " today "][..], 0),
        (&[" wczoraj ", " yesterday "][..], -1),
        (&[" jutro ", " tomorrow "][..], 1),
    ] {
        if phrases.iter().any(|p| line.contains(p)) {
            out.push(now + Duration::days(days));
        }
    }
    // "Ważne przez 30 dni" counts from the publication, not from now (`after_published`).
    if period(words).is_some() {
        return out;
    }
    let past = if has(PAST) {
        true
    } else if has(FUTURE) {
        false
    } else {
        kind == Kind::Published
    };
    for (i, w) in words.iter().enumerate() {
        let Some(step) = unit(w) else { continue };
        let count = match i.checked_sub(1).and_then(|j| words.get(j)) {
            Some(n) if small_number(n).is_some() => small_number(n),
            // "tydzień temu", "an hour ago", "za godzinę" - one unit.
            Some(&("a" | "an" | "jeden" | "jedna" | "za" | "in")) => Some(1),
            _ if words.get(i + 1).is_some_and(|n| PAST.contains(n)) => Some(1),
            _ => None,
        };
        // A lone "d" or "h" is only a unit right after a number ("3 d").
        if count.is_none() || (w.len() == 1 && i.checked_sub(1).and_then(|j| words.get(j)).and_then(|n| small_number(n)).is_none()) {
            continue;
        }
        let Some(span) = count.and_then(|c| i32::try_from(c).ok()).map(|c| step * c) else { continue };
        out.push(if past { now - span } else { now + span });
    }
    out
}

/// "ważne przez 30 dni", "valid for 30 days": how long the offer is open from its publication.
/// Not with "jeszcze", "left"... - "ważna jeszcze przez 5 dni" is what is left from now.
fn period(words: &[&str]) -> Option<Duration> {
    if words.iter().any(|w| FUTURE.contains(w)) {
        return None;
    }
    words.windows(3).find_map(|w| {
        if !matches!(w[0], "przez" | "for") {
            return None;
        }
        let count = i32::try_from(small_number(w[1])?).ok()?;
        Some(unit(w[2])? * count)
    })
}

fn words_of(phrase: &str) -> Vec<String> {
    normalize(phrase).split(' ').filter(|w| !w.is_empty()).map(str::to_string).collect()
}

fn has_period(phrase: &str) -> bool {
    let words = words_of(phrase);
    period(&words.iter().map(String::as_str).collect::<Vec<_>>()).is_some()
}

/// The end of an offer open for a period ("ważne przez 30 dni") - from the publication date the
/// text gives; without it nothing (counting from now would move the end with every reading).
fn after_published(phrase: &str, text: &str, now: OffsetDateTime) -> Option<OffsetDateTime> {
    let words = words_of(phrase);
    let span = period(&words.iter().map(String::as_str).collect::<Vec<_>>())?;
    let (_, dates) = lines_with(text, Kind::Published, now).into_iter().next()?;
    Some(on_day((*dates.first()? + span).date(), Kind::Expires))
}

/// Every date a line (or a phrase) gives, the written ones first.
fn all_in(line: &str, kind: Kind, now: OffsetDateTime) -> Vec<OffsetDateTime> {
    let n = normalize(line);
    let words: Vec<&str> = n.split(' ').filter(|w| !w.is_empty()).collect();
    let mut out: Vec<OffsetDateTime> = numeric(line).into_iter().chain(named(&words, kind, now)).map(|d| on_day(d, kind)).collect();
    out.extend(relative(&words, kind, now));
    out
}

fn rfc(t: OffsetDateTime) -> Option<String> {
    t.replace_nanosecond(0).ok()?.format(&Rfc3339).ok()
}

/// The date of a phrase AI copied from the offer ("2 dni temu", "do 30.11.2026"), counted from
/// `now`. The phrase must be in the offer's text - a made-up one gives nothing.
pub fn of_phrase(phrase: &str, offer_text: &str, kind: Kind, now: OffsetDateTime) -> Option<String> {
    let p = normalize(phrase);
    if p.is_empty() || p.len() > 100 || !format!(" {} ", normalize(offer_text)).contains(&format!(" {p} ")) {
        return None;
    }
    // "30 dni" copied from "Ważne przez 30 dni" is a period too.
    let text_words = format!(" {} ", normalize(offer_text));
    let after = [" przez ", " for "].iter().any(|w| text_words.contains(&format!("{w}{p} ")));
    if kind == Kind::Expires && (after || has_period(phrase)) {
        let phrase = if after && !has_period(phrase) { format!("przez {p}") } else { phrase.to_string() };
        return after_published(&phrase, offer_text, now).and_then(rfc);
    }
    all_in(phrase, kind, now).into_iter().next().and_then(rfc)
}

/// "1 dzień temu" alone on a line - how portals show the age of an offer.
fn age_line(n: &str) -> bool {
    n.split(' ').count() <= 5
        && (n.ends_with(" temu") || n.ends_with(" ago") || ["przed chwila", "just posted", "dzisiaj", "today", "wczoraj", "yesterday"].contains(&n))
}

/// Lines of the text that give a date of this kind, with the dates: a line with its word
/// ("Ważna do: 04.12.2026"), a word alone on a line with the date on the next one, and for the
/// publication a short "3 dni temu" line.
fn lines_with(text: &str, kind: Kind, now: OffsetDateTime) -> Vec<(String, Vec<OffsetDateTime>)> {
    let words = if kind == Kind::Published { PUBLISHED_WORDS } else { EXPIRES_WORDS };
    let lines: Vec<&str> = text.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
    // A period ("Ważne przez 30 dni") counts from the publication, other phrases from `alone`.
    let dated = |phrase: &str, alone: &str| -> Vec<OffsetDateTime> {
        if kind == Kind::Expires && has_period(phrase) {
            after_published(phrase, text, now).into_iter().collect()
        } else {
            all_in(alone, kind, now)
        }
    };
    let mut out = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        let n = normalize(line);
        let spaced = format!(" {n}");
        // "od: 19 października 2026" / "do: 18 listopada 2026" - a short line of its own (theprotocol.it).
        let label = if kind == Kind::Published { "od:" } else { "do:" };
        let labelled = line.to_lowercase().starts_with(label) && n.split(' ').count() <= 5;
        let named = labelled || words.iter().any(|w| spaced.contains(&format!(" {w}")));
        let dates = if named || (kind == Kind::Published && age_line(&n)) { dated(line, line) } else { Vec::new() };
        if !dates.is_empty() {
            out.push((one_line(line, 200), dates));
        } else if named && n.split(' ').count() <= 4 {
            if let Some(next) = lines.get(i + 1).filter(|l| normalize(l).split(' ').count() <= 6) {
                let joined = format!("{line} {next}");
                let dates = dated(&joined, next);
                if !dates.is_empty() {
                    out.push((one_line(&joined, 200), dates));
                }
            }
        }
    }
    out
}

/// The date of this kind the text gives, and its line - for a field AI left empty (or no AI).
pub fn find(text: &str, kind: Kind, now: OffsetDateTime) -> Option<(String, String)> {
    let (line, dates) = lines_with(text, kind, now).into_iter().next()?;
    Some((rfc(*dates.first()?)?, line))
}

/// The line of the text that gives this date (the same day) - `None` = the text does not have it.
pub fn line_of(date: &str, text: &str, kind: Kind, now: OffsetDateTime) -> Option<String> {
    let day = OffsetDateTime::parse(date, &Rfc3339).ok()?.to_offset(time::UtcOffset::UTC).date();
    // Any line, also without the word ("4 grudnia 2026" in a sentence).
    let any = text.lines().map(str::trim).filter(|l| !l.is_empty()).map(|l| (one_line(l, 200), all_in(l, kind, now)));
    lines_with(text, kind, now)
        .into_iter()
        .chain(any)
        .find(|(_, dates)| dates.iter().any(|d| d.date() == day))
        .map(|(line, _)| line)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn now() -> OffsetDateTime {
        OffsetDateTime::parse("2026-11-11T10:00:00Z", &Rfc3339).unwrap()
    }

    fn day(s: &str) -> String {
        s[..10].to_string()
    }

    fn found(text: &str, kind: Kind) -> Option<String> {
        find(text, kind, now()).map(|(d, _)| d)
    }

    #[test]
    fn written_dates() {
        assert_eq!(found("Oferta ważna do: 04.12.2026 (zostało 23 dni)", Kind::Expires).as_deref(), Some("2026-12-04T21:59:59Z"));
        assert_eq!(found("Published: 11.11.2026", Kind::Published).as_deref(), Some("2026-11-11T00:00:00Z"));
        assert_eq!(found("Data dodania: 2026-11-09", Kind::Published).map(|d| day(&d)).as_deref(), Some("2026-11-09"));
        assert_eq!(found("Termin składania ofert: 30 listopada 2026 r.", Kind::Expires).map(|d| day(&d)).as_deref(), Some("2026-11-30"));
        assert_eq!(found("Apply by Dec 5, 2026", Kind::Expires).map(|d| day(&d)).as_deref(), Some("2026-12-05"));
        // The word alone, the date on the next line.
        assert_eq!(found("Ważna do:\n15.12.2026", Kind::Expires).map(|d| day(&d)).as_deref(), Some("2026-12-15"));
        // theprotocol.it: "od:" and "do:" on short lines of their own.
        let page = "Full Stack Software Developer\nFirma: Zmyślona Firma sp. z o.o.\nod: 19 października 2026\ndo: 18 listopada 2026";
        assert_eq!(found(page, Kind::Published).map(|d| day(&d)).as_deref(), Some("2026-10-19"));
        assert_eq!(found(page, Kind::Expires).map(|d| day(&d)).as_deref(), Some("2026-11-18"));
        // Not a label at the start of a short line: nothing.
        assert_eq!(found("Pracujemy od poniedziałku do piątku, 9-17. Start do 5 grudnia 2026 albo później.", Kind::Expires), None);
    }

    #[test]
    fn a_day_without_a_year() {
        // Pracuj.pl: "ważna jeszcze 16 dni (do 26 lis)" - both give the 26th.
        assert_eq!(found("ważna jeszcze 16 dni", Kind::Expires).map(|d| day(&d)).as_deref(), Some("2026-11-27"));
        assert_eq!(line_of("2026-11-26T21:59:59Z", "ważna jeszcze 16 dni\n(do 26 lis)", Kind::Expires, now()).as_deref(), Some("(do 26 lis)"));
        // Published "20 gru" can only be last year.
        assert_eq!(found("Opublikowano 20 gru", Kind::Published).map(|d| day(&d)).as_deref(), Some("2025-12-20"));
    }

    #[test]
    fn counted_from_now() {
        assert_eq!(found("1 godz. temu", Kind::Published).as_deref(), Some("2026-11-11T09:00:00Z"));
        assert_eq!(found("1 dzień temu", Kind::Published).as_deref(), Some("2026-11-10T10:00:00Z"));
        assert_eq!(found("Dodano przed chwilą", Kind::Published).as_deref(), Some("2026-11-11T10:00:00Z"));
        assert_eq!(found("2 weeks ago", Kind::Published).map(|d| day(&d)).as_deref(), Some("2026-10-28"));
        assert_eq!(found("Oferta wygasa za 2 dni", Kind::Expires).as_deref(), Some("2026-11-13T10:00:00Z"));
        assert_eq!(found("Expires in 3 days", Kind::Expires).map(|d| day(&d)).as_deref(), Some("2026-11-14"));
        assert_eq!(found("Opublikowana wczoraj", Kind::Published).map(|d| day(&d)).as_deref(), Some("2026-11-10"));
    }

    #[test]
    fn other_lines_give_nothing() {
        // Amounts, a start date, an address, a phone number - not the offer's dates.
        let text = "18 000 - 24 000 PLN\nRozpoczęcie pracy: 01.12.2026\nul. Przykładowa 12\ntel. 600 100 200\nWymagamy 3 lat doświadczenia";
        assert_eq!(found(text, Kind::Published), None);
        assert_eq!(found(text, Kind::Expires), None);
        // "się" is not August.
        assert_eq!(found("Ważna oferta - zgłoś się 5 razy", Kind::Expires), None);
    }

    #[test]
    fn dates_of_the_portal_pages_and_not_of_the_similar_offers() {
        let cases = [
            (include_str!("trim_pages/pracuj.txt"), "Piekarz (k/m)", None, Some("2026-11-27")),
            // "2 dni temu" under "Podobne oferty" is another offer's.
            (include_str!("trim_pages/praca.txt"), "Piekarz - Piecowy (k/m)", Some("2026-11-10"), None),
            (include_str!("trim_pages/justjoin.txt"), "Senior Software Engineer", Some("2026-11-11"), None),
            (include_str!("trim_pages/nofluff.txt"), "Senior Frontend Developer", None, Some("2026-12-04")),
            (include_str!("trim_pages/plain.txt"), "Specjalista ds. obsługi zamówień", None, None),
            // "Ważne przez 30 dni" from the publication; "Ważne przez 29 dni" under it is another job's.
            (include_str!("trim_pages/useme.txt"), "Programista WordPress z AI | Stała współpraca", Some("2026-11-04"), Some("2026-12-04")),
        ];
        for (page, title, published, expires) in cases {
            let text = crate::trim::page_text(page, Some(title));
            assert_eq!(found(&text, Kind::Published).map(|d| day(&d)).as_deref(), published, "{title}");
            assert_eq!(found(&text, Kind::Expires).map(|d| day(&d)).as_deref(), expires, "{title}");
        }
    }

    #[test]
    fn a_period_counts_from_the_publication() {
        let text = "Opublikowano
2026-11-04
Ważne przez
30 dni";
        assert_eq!(found(text, Kind::Expires).as_deref(), Some("2026-12-04T21:59:59Z"));
        assert_eq!(found("Opublikowano 2026-11-04
Ważne przez 2 tygodnie", Kind::Expires).map(|d| day(&d)).as_deref(), Some("2026-11-18"));
        assert_eq!(found("Posted: 04.11.2026
Valid for 30 days", Kind::Expires).map(|d| day(&d)).as_deref(), Some("2026-12-04"));
        // Without the publication date nothing - not 30 days from now.
        assert_eq!(found("Ważne przez
30 dni", Kind::Expires), None);
        // "jeszcze" - what is left, from now.
        assert_eq!(found("Ważna jeszcze przez 5 dni", Kind::Expires).map(|d| day(&d)).as_deref(), Some("2026-11-16"));
        // AI's phrase, also only the "30 dni" part of it.
        assert_eq!(of_phrase("Ważne przez 30 dni", text, Kind::Expires, now()).as_deref(), Some("2026-12-04T21:59:59Z"));
        assert_eq!(of_phrase("30 dni", text, Kind::Expires, now()).as_deref(), Some("2026-12-04T21:59:59Z"));
        assert_eq!(line_of("2026-12-04T21:59:59Z", text, Kind::Expires, now()).as_deref(), Some("Ważne przez 30 dni"));
    }

    #[test]
    fn a_phrase_from_ai_must_be_in_the_text() {
        let text = "Piekarz\n1 dzień temu\nOferta ważna do: 04.12.2026";
        assert_eq!(of_phrase("1 dzień temu", text, Kind::Published, now()).as_deref(), Some("2026-11-10T10:00:00Z"));
        assert_eq!(of_phrase("04.12.2026", text, Kind::Expires, now()).as_deref(), Some("2026-12-04T21:59:59Z"));
        assert_eq!(of_phrase("3 dni temu", text, Kind::Published, now()), None);
        assert_eq!(of_phrase("31.12.2026", text, Kind::Expires, now()), None);
    }

    #[test]
    fn a_date_is_checked_against_the_lines() {
        let text = "Piekarz\n1 dzień temu\nOferta ważna do: 04.12.2026";
        assert_eq!(line_of("2026-11-10T10:00:00Z", text, Kind::Published, now()).as_deref(), Some("1 dzień temu"));
        assert_eq!(line_of("2026-12-04T21:59:59Z", text, Kind::Expires, now()).as_deref(), Some("Oferta ważna do: 04.12.2026"));
        assert_eq!(line_of("2026-12-24T21:59:59Z", text, Kind::Expires, now()), None);
    }
}
