//! Duplicate detection: the same offer from several portals gets the same fingerprint,
//! built from the company and the job title after normalization.

/// Lowercase, Polish (and common European) letters without diacritics,
/// everything that is not a letter or digit becomes a single space.
pub fn normalize(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut space = true;
    for ch in text.chars().flat_map(char::to_lowercase) {
        let folded = fold(ch);
        if folded.chars().all(char::is_alphanumeric) && !folded.is_empty() {
            out.push_str(folded);
            space = false;
        } else if !space {
            out.push(' ');
            space = true;
        }
    }
    out.trim_end().to_string()
}

fn fold(ch: char) -> &'static str {
    match ch {
        'ą' | 'á' | 'à' | 'â' | 'ä' | 'ã' | 'å' => "a",
        'ć' | 'ç' | 'č' => "c",
        'ę' | 'é' | 'è' | 'ê' | 'ë' | 'ě' => "e",
        'í' | 'ì' | 'î' | 'ï' => "i",
        'ł' => "l",
        'ń' | 'ñ' | 'ň' => "n",
        'ó' | 'ò' | 'ô' | 'ö' | 'õ' | 'ø' => "o",
        'ś' | 'š' => "s",
        'ź' | 'ż' | 'ž' => "z",
        'ú' | 'ù' | 'û' | 'ü' | 'ů' => "u",
        'ý' => "y",
        'ř' => "r",
        'ß' => "ss",
        c if c.is_ascii_alphanumeric() => ascii(c),
        _ => " ",
    }
}

fn ascii(c: char) -> &'static str {
    const TABLE: &str = "0123456789abcdefghijklmnopqrstuvwxyz";
    let index = TABLE.find(c).unwrap_or(0);
    &TABLE[index..index + 1]
}

/// Legal-form words dropped from the end of a company name
/// ("Firma X sp. z o.o." and "Firma X" are the same employer).
const LEGAL_FORMS: &[&str] = &[
    "sp", "z", "o", "oo", "zoo", "s", "a", "sa", "ska", "sk", "spk", "spj", "spolka", "akcyjna",
    "ograniczona", "odpowiedzialnoscia", "komandytowa", "komandytowo", "jawna", "partnerska", "ltd",
    "limited", "gmbh", "inc", "llc", "plc", "bv", "ag",
];
/// Legal forms of two words whose last word alone may be a name: "sp. k.", "sp. j.", "sp. p.",
/// "S.K.A." (its "a" goes first), "GmbH & Co. KG".
const LEGAL_PAIRS: &[[&str; 2]] = &[["sp", "k"], ["sp", "j"], ["sp", "p"], ["s", "k"], ["co", "kg"]];

/// Gender markers in job titles - only in brackets or joined by slashes: "(k/m)", "(m/f/d)",
/// "(K/M/N)", "(h/f)", "m/w/div".
const TITLE_MARKERS: &[&str] = &["k", "m", "f", "d", "x", "w", "n", "h", "div"];
/// Signs that make another technology: "C++" is not "C#", ".NET" is not "net".
const TITLE_SIGNS: &[(&str, &str)] = &[("c++", " cpp "), ("c#", " csharp "), ("f#", " fsharp "), (".net", " dotnet ")];

pub fn company_key(company: &str) -> String {
    let normalized = normalize(company);
    let mut words: Vec<&str> = normalized.split(' ').filter(|w| !w.is_empty()).collect();
    loop {
        if words.len() > 1 && words.last().is_some_and(|w| LEGAL_FORMS.contains(w)) {
            words.pop();
        } else if words.len() > 2 && LEGAL_PAIRS.iter().any(|pair| words.ends_with(pair)) {
            words.truncate(words.len() - 2);
        } else {
            break;
        }
    }
    words.join(" ")
}

/// Only markers, joined by slashes ("k/m", "m / f / d").
fn is_marker_group(text: &str) -> bool {
    let parts: Vec<&str> = text.split('/').map(str::trim).collect();
    parts.iter().all(|p| TITLE_MARKERS.contains(p))
}

pub fn title_key(title: &str) -> String {
    let mut text = title.to_lowercase();
    // "(k/m)", "(M/F/D)": a bracket with markers only.
    let mut out = String::with_capacity(text.len());
    let mut rest = text.as_str();
    while let Some(open) = rest.find(['(', '[']) {
        let close = rest[open..].find([')', ']']).map(|c| open + c);
        match close {
            Some(close) if is_marker_group(&rest[open + 1..close]) => {
                out.push_str(&rest[..open]);
                out.push(' ');
                rest = &rest[close + 1..];
            }
            _ => {
                out.push_str(&rest[..=open]);
                rest = &rest[open + 1..];
            }
        }
    }
    out.push_str(rest);
    text = out;
    // "Developer m/w/d": markers joined by slashes outside brackets.
    text = text
        .split_whitespace()
        .filter(|word| !(word.contains('/') && is_marker_group(word.trim_matches(|c: char| !c.is_alphanumeric() && c != '/'))))
        .collect::<Vec<_>>()
        .join(" ");
    for (sign, word) in TITLE_SIGNS {
        text = text.replace(sign, word);
    }
    normalize(&text)
}

/// A key of an offer deleted for good, made again with today's rules (the company and the title
/// are gone - only the old key is left).
pub fn refresh_key(old: &str) -> String {
    match old.split_once('|') {
        Some((company, title)) => fingerprint(company, title),
        None => old.to_string(),
    }
}

pub fn fingerprint(company: &str, title: &str) -> String {
    format!("{}|{}", company_key(company), title_key(title))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_polish_letters_case_and_punctuation() {
        assert_eq!(normalize("  Łódź - Śródmieście!! "), "lodz srodmiescie");
        assert_eq!(normalize("ZAŻÓŁĆ GĘŚLĄ JAŹŃ"), "zazolc gesla jazn");
        assert_eq!(normalize("C++/C# Developer"), "c c developer");
    }

    #[test]
    fn same_offer_from_different_portals_matches() {
        let a = fingerprint("Przykładowa Firma Sp. z o.o.", "Frontend Developer (k/m)");
        let b = fingerprint("przykladowa firma", "Frontend  Developer");
        let c = fingerprint("PRZYKŁADOWA FIRMA S.A.", "frontend developer (M/F/D)");
        assert_eq!(a, b);
        assert_eq!(a, c);
    }

    #[test]
    fn different_offers_do_not_match() {
        assert_ne!(
            fingerprint("Firma X", "Frontend Developer"),
            fingerprint("Firma X", "Backend Developer")
        );
        assert_ne!(
            fingerprint("Firma X", "Frontend Developer"),
            fingerprint("Firma Y", "Frontend Developer")
        );
    }

    #[test]
    fn longer_legal_forms_are_dropped() {
        for name in [
            "Firma Przykładowa Sp. z o.o. Sp. k.",
            "Firma Przykładowa sp. j.",
            "Firma Przykładowa Sp. p.",
            "Firma Przykładowa S.K.A.",
            "Firma Przykładowa GmbH & Co. KG",
            "Firma Przykładowa spółka jawna",
            "Firma Przykładowa spółka komandytowa",
            "Firma Przykładowa Spółka Komandytowo-Akcyjna",
        ] {
            assert_eq!(company_key(name), "firma przykladowa", "{name}");
        }
        // A one-letter word that is part of the name stays.
        assert_eq!(company_key("Studio K"), "studio k");
    }

    #[test]
    fn signs_of_technologies_and_gender_markers() {
        assert_ne!(title_key("Programista C++"), title_key("Programista C#"));
        assert_ne!(title_key("F# Developer"), title_key("Developer"));
        assert_eq!(title_key("Programista .NET"), "programista dotnet");
        assert_eq!(title_key("ASP.NET Developer"), "asp dotnet developer");
        assert_eq!(title_key("Księgowa (K/M/N)"), "ksiegowa");
        assert_eq!(title_key("Kellner (h/f)"), "kellner");
        assert_eq!(title_key("Entwickler m/w/div"), "entwickler");
        assert_eq!(title_key("Developer [m/f/d]"), "developer");
        // A marker letter outside brackets is a word of the title.
        assert_eq!(title_key("Kierowca kat. D"), "kierowca kat d");
        assert_eq!(title_key("Tester (manualny)"), "tester manualny");
    }

    #[test]
    fn keys_of_deleted_offers_are_made_again() {
        assert_eq!(refresh_key("firma przykladowa sp k|tester"), "firma przykladowa|tester");
        assert_eq!(refresh_key("firma|tester"), "firma|tester");
    }

    #[test]
    fn company_made_only_of_a_legal_word_is_kept() {
        assert_eq!(company_key("S.A."), "s");
        assert_eq!(company_key(""), "");
    }
}
