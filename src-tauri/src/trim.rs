//! Cleaning the text of an offer page before it goes to AI and into the offer description:
//! cuts "similar offers" (other salaries, other companies - AI could take a number from a
//! neighbouring offer), the footer, cookie banners and the menu at the top.
//!
//! Works on plain text, so it helps both with the text taken from the preview window and with
//! a text pasted after Ctrl+A / Ctrl+C. Cleaning twice gives the same text as cleaning once.

use crate::dedup::normalize;
use crate::text::squeeze;

/// The offer itself takes at least this much text - a "similar offers" link in the menu at the
/// top of a page must not cut the whole offer away.
const MIN_BEFORE: usize = 300;
/// The menu, breadcrumbs and the like sit just above the title - this many lines stay with it.
const KEEP_ABOVE_TITLE: usize = 3;
/// How far up from the copyright line the footer may start.
const FOOTER_LINES: usize = 80;
/// Footer links and menu items are short.
const SHORT_LINE: usize = 80;

/// Headings of "similar offers" and other listings under an offer (normalized, whole line).
const SIMILAR: &[&str] = &[
    "podobne oferty",
    "podobne oferty pracy",
    "zobacz podobne oferty",
    "zobacz podobne oferty pracy",
    "sprawdz podobne oferty",
    "sprawdz podobne oferty pracy",
    "inne podobne oferty",
    "podobne ogloszenia",
    "zobacz podobne ogloszenia",
    "podobne ogloszenia o prace",
    "inne oferty tej firmy",
    "inne oferty pracodawcy",
    "inne oferty tego pracodawcy",
    "inne ogloszenia tego pracodawcy",
    "zobacz inne oferty pracy",
    "zobacz inne oferty",
    "inne oferty pracy",
    "polecane oferty",
    "polecane oferty pracy",
    "rekomendowane oferty",
    "oferty dla ciebie",
    "moze cie zainteresowac",
    "moga cie zainteresowac",
    "similar offers",
    "similar jobs",
    "similar job offers",
    "see similar jobs",
    "related jobs",
    "more jobs like this",
    "more jobs",
    "jobs you may be interested in",
    "people also viewed",
    "recommended jobs",
    "recommended for you",
    "you may also like",
    "other offers from this company",
    "more jobs from this company",
];

/// Menu items and skip links at the very top of a page (normalized, whole line).
const MENU: &[&str] = &[
    "menu",
    "oferty pracy",
    "praca",
    "pracodawcy",
    "dla pracodawcy",
    "dla pracodawcow",
    "dla firm",
    "dla kandydatow",
    "profile firm",
    "porady",
    "poradniki",
    "porady i narzedzia",
    "radar kariery",
    "kalkulator wynagrodzen",
    "kreator cv",
    "cennik",
    "publikuj",
    "ogloszenie",
    "dodaj ogloszenie",
    "dodaj oferte",
    "moje konto",
    "zaloguj",
    "zaloguj sie",
    "zarejestruj",
    "zarejestruj sie",
    "logowanie",
    "rejestracja",
    "szukaj",
    "wyszukaj",
    "strona glowna",
    "kariera",
    "blog",
    "pomoc",
    "kontakt",
    "new",
    "nowosc",
    "beta",
    "powrot do wyszukiwania",
    "wroc do wynikow",
    "zobacz najnowsze oferty pracy",
    "poprzednia oferta",
    "nastepna oferta",
    "home",
    "jobs",
    "job offers",
    "companies",
    "search",
    "location",
    "log in",
    "sign in",
    "sign up",
    "register",
    "post a job",
    "add offer",
    "for employers",
    "back to search",
];

/// Where a footer usually starts (normalized, whole line). Not "Kontakt" or "O firmie" -
/// offers use them as headings, and the short lines of an offer's list could join the footer.
const FOOTER_START: &[&str] = &[
    "dla kandydatow",
    "dla firm",
    "dla pracodawcow",
    "dla pracodawcy",
    "o nas",
    "regulamin",
    "polityka prywatnosci",
    "aplikacja mobilna",
    "pobierz aplikacje",
    "job offers",
    "about us",
    "for candidates",
    "for employers",
    "privacy policy",
];

/// Buttons and headings of a cookie banner - removed only next to a cookie line.
const COOKIE_BUTTONS: &[&str] = &[
    "akceptuj",
    "akceptuje",
    "akceptuj wszystkie",
    "akceptuj wybrane",
    "zaakceptuj",
    "zaakceptuj wszystkie",
    "zgadzam sie",
    "odrzuc",
    "odrzuc wszystkie",
    "dostosuj",
    "ustawienia",
    "zarzadzaj",
    "wiecej informacji",
    "cenimy twoja prywatnosc",
    "twoja prywatnosc",
    "accept",
    "accept all",
    "reject all",
    "decline all",
    "customize",
    "manage",
    "settings",
    "ok",
    "x",
];

/// Words that turn a line mentioning cookies into a cookie banner line.
const CONSENT_WORDS: &[&str] = &[
    "zgod", "akcept", "prywatnos", "polityk", "przegladar", "partner", "reklam", "marketing", "personaliz", "ustawien",
    "consent", "accept", "privacy", "advertis", "browser", "agree", "preferenc", "settings", "policy", "website", "serwis",
];

fn words(line: &str) -> String {
    format!(" {} ", normalize(line))
}

fn is_one_of(line: &str, list: &[&str]) -> bool {
    let n = normalize(line);
    !n.is_empty() && list.contains(&n.as_str())
}

/// A line of a cookie banner: mentions cookies and consent / privacy, or is short.
fn is_cookie_line(line: &str) -> bool {
    let n = words(line);
    if !n.contains(" cookie") {
        return false;
    }
    line.chars().count() <= SHORT_LINE || CONSENT_WORDS.iter().any(|w| n.contains(&format!(" {w}")))
}

fn is_copyright(line: &str) -> bool {
    let n = words(line);
    line.contains('©') || n.contains(" copyright ") || n.contains(" wszelkie prawa zastrzezone ") || n.contains(" all rights reserved ")
}

fn is_short(line: &str) -> bool {
    line.chars().count() <= SHORT_LINE
}

fn chars_before(lines: &[&str], end: usize) -> usize {
    lines[..end].iter().map(|l| l.chars().count() + 1).sum()
}

/// Everything from the first "similar offers" heading that has the offer above it.
fn cut_similar(lines: &mut Vec<&str>) {
    let at = (0..lines.len()).find(|&i| is_one_of(lines[i], SIMILAR) && chars_before(lines, i) >= MIN_BEFORE);
    if let Some(at) = at {
        lines.truncate(at);
    }
}

/// Cookie banner lines, with their buttons.
fn drop_cookies(lines: &mut Vec<&str>) {
    let cookie: Vec<bool> = lines.iter().map(|l| is_cookie_line(l)).collect();
    let near = |i: usize| (i.saturating_sub(4)..(i + 5).min(lines.len())).any(|j| cookie[j]);
    let keep: Vec<bool> = (0..lines.len()).map(|i| !(cookie[i] || (is_one_of(lines[i], COOKIE_BUTTONS) && near(i)))).collect();
    let mut i = 0;
    lines.retain(|_| {
        i += 1;
        keep[i - 1]
    });
}

/// The footer: a run of short lines ending with the copyright line, from its first typical
/// heading ("Dla kandydatów", "O nas", "Regulamin"...) to the end of the page.
fn cut_footer(lines: &mut Vec<&str>) {
    let Some(copyright) = (0..lines.len()).rev().find(|&i| is_copyright(lines[i])) else { return };
    let mut start = copyright;
    let mut i = copyright;
    while i > 0 && copyright - (i - 1) <= FOOTER_LINES {
        let line = lines[i - 1];
        if !is_short(line) && !is_copyright(line) {
            break;
        }
        i -= 1;
        if is_one_of(line, FOOTER_START) {
            start = i;
        }
    }
    if chars_before(lines, start) >= MIN_BEFORE {
        lines.truncate(start);
    }
}

/// The menu at the top: with a known title (the page's `<h1>`) - all but a few lines above its
/// first appearance; then menu items, counters and skip links before the first other line.
fn cut_menu(lines: &mut Vec<&str>, title: Option<&str>) {
    let title = title.map(normalize).filter(|t| t.chars().count() >= 3);
    let half = chars_before(lines, lines.len()) / 2;
    let at_title = title.and_then(|t| (0..lines.len()).find(|&i| normalize(lines[i]) == t)).filter(|&i| chars_before(lines, i) <= half);
    if let Some(at) = at_title {
        lines.drain(..at.saturating_sub(KEEP_ABOVE_TITLE));
    }
    let skip = |l: &str| {
        let n = normalize(l);
        n.is_empty() || n.chars().all(|c| c.is_ascii_digit()) || n.starts_with("przejdz do ") || n.starts_with("skip to ") || is_one_of(l, MENU)
    };
    let menu = lines.iter().take_while(|l| skip(l)).count();
    lines.drain(..menu);
}

/// The offer part of a page text. `title` - the page's `<h1>`, when known.
pub fn page_text(text: &str, title: Option<&str>) -> String {
    let text = squeeze(text);
    let mut lines: Vec<&str> = text.lines().collect();
    cut_similar(&mut lines);
    drop_cookies(&mut lines);
    cut_footer(&mut lines);
    cut_menu(&mut lines, title);
    squeeze(&lines.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Saved pages: the layout and the portal's own texts as on the real page (11.11.2026),
    /// the offers made up.
    const PRACUJ: &str = include_str!("trim_pages/pracuj.txt");
    const PRACA: &str = include_str!("trim_pages/praca.txt");
    const JUSTJOIN: &str = include_str!("trim_pages/justjoin.txt");
    const NOFLUFF: &str = include_str!("trim_pages/nofluff.txt");
    const PLAIN: &str = include_str!("trim_pages/plain.txt");

    fn check(page: &str, title: Option<&str>, keep: &[&str], gone: &[&str]) -> String {
        let out = page_text(page, title);
        for k in keep {
            assert!(out.contains(k), "lost {k:?}:\n{out}");
        }
        for g in gone {
            assert!(!out.contains(g), "left {g:?}:\n{out}");
        }
        assert_eq!(page_text(&out, title), out, "cleaning twice changed the text");
        out
    }

    #[test]
    fn pracuj() {
        for title in [Some("Piekarz (k/m)"), None] {
            check(
                PRACUJ,
                title,
                &["Piekarz (k/m)", "Zmyślona Piekarnia Sp. z o.o.", "umowa o pracę", "Twój zakres obowiązków", "wypiek pieczywa", "To oferujemy"],
                &["5 000–6 100 zł", "Przykładowe Delikatesy", "Sprawdź podobne oferty", "© Grupa Pracuj", "Cenimy Twoją prywatność", "Akceptuj wszystkie", "Przejdź do treści"],
            );
        }
        // Without the title only the menu items it knows go - the offer starts right after.
        assert!(page_text(PRACUJ, None).starts_with("Piekarz (k/m)"));
    }

    #[test]
    fn praca() {
        let out = check(
            PRACA,
            Some("Piekarz - Piecowy (k/m)"),
            &["Piekarz - Piecowy (k/m)", "Wymyślona Sieć Sklepów", "od 5 000,00 zł do 7 000,00 zł netto", "Oczekujemy od Ciebie"],
            &["PODOBNE OFERTY", "4 200 zł", "Copyright ©", "Zgoda na Pliki Cookies", "AKCEPTUJ WSZYSTKIE", "Kalkulator wynagrodzeń"],
        );
        // A line that only mentions similar offers is not the heading.
        assert!(out.contains("Chcesz otrzymywać podobne oferty pracy na e-mail?"));
    }

    #[test]
    fn justjoin() {
        check(
            JUSTJOIN,
            Some("Senior Software Engineer"),
            &["Senior Software Engineer", "Przykładowa Firma Software", "contract of employment", "TECH STACK"],
            &["SIMILAR OFFERS", "27 645,63", "PLN/month", "This website uses cookies", "ACCEPT ALL", "© 2026", "Add offer"],
        );
    }

    #[test]
    fn nofluff() {
        check(
            NOFLUFF,
            Some("Senior Backend Developer (JS/TS) – Data Domain"),
            &["Senior Backend Developer (JS/TS) – Data Domain", "Zmyślone IT", "18 000 – 24 000 PLN", "Oferta ważna do"],
            &["ZOBACZ PODOBNE OFERTY", "26 250 – 30 450", "OTRZYMUJ DOPASOWANE OFERTY", "ZALOGUJ SIĘ"],
        );
    }

    #[test]
    fn a_pasted_offer_without_a_page_around_it_stays_whole() {
        let out = page_text(PLAIN, None);
        assert_eq!(out, squeeze(PLAIN));
    }

    #[test]
    fn a_cookie_banner_and_a_footer_go_without_similar_offers() {
        let page = format!(
            "Cenimy Twoją prywatność\nTa strona używa plików cookies, aby dopasować reklamy do Twoich preferencji.\nAkceptuj wszystkie\nOdrzuć wszystkie\n{PLAIN}\nAplikuj\nO nas\nKontakt\nRegulamin\nPolityka prywatności\n© 2026 Przykładowy Portal"
        );
        assert_eq!(page_text(&page, None), squeeze(&format!("{PLAIN}\nAplikuj")));
    }

    #[test]
    fn a_similar_offers_link_in_the_menu_does_not_cut_the_offer() {
        let offer = "Opis stanowiska i obowiązki w zespole. ".repeat(20);
        let page = format!("Oferty pracy\nPodobne oferty\nTester\n{offer}\nPodobne oferty\nInny tester\n9 000 zł");
        let out = page_text(&page, None);
        assert!(out.contains("Opis stanowiska"));
        assert!(!out.contains("9 000 zł"));
    }

    #[test]
    fn baking_cookies_is_not_a_cookie_banner() {
        let offer = format!(
            "Cukiernik\n{}\nPieczenie ciasteczek zgodnie z recepturą.\nWe bake cookies by hand in a family bakery according to our own recipes and the HACCP rules.",
            "Opis stanowiska i obowiązki w zespole. ".repeat(10)
        );
        assert_eq!(page_text(&offer, None), squeeze(&offer));
    }

    #[test]
    fn counters_above_the_title_go() {
        let page = format!("0\n0\n0\n0\n0\nSenior Tester\nPrzykładowa Firma\n{}", "Opis stanowiska. ".repeat(30));
        assert!(page_text(&page, Some("Senior Tester")).starts_with("Senior Tester\nPrzykładowa Firma"));
    }

    #[test]
    fn a_short_page_is_not_cut_to_nothing() {
        assert_eq!(page_text("Tester\n© 2026 Firma", None), "Tester\n© 2026 Firma");
        assert_eq!(page_text("", Some("Tester")), "");
    }
}
