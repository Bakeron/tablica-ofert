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
    "sp", "z", "o", "oo", "zoo", "s", "a", "sa", "ska", "sk", "spk", "spolka", "akcyjna",
    "ograniczona", "odpowiedzialnoscia", "komandytowa", "ltd", "limited", "gmbh", "inc", "llc",
    "plc", "bv", "ag",
];

/// Gender markers in job titles: "(k/m)", "(m/f/d)", "(f/m/x)".
const TITLE_MARKERS: &[&str] = &["k", "m", "f", "d", "x", "w"];

pub fn company_key(company: &str) -> String {
    let normalized = normalize(company);
    let mut words: Vec<&str> = normalized.split(' ').filter(|w| !w.is_empty()).collect();
    while words.len() > 1 && words.last().is_some_and(|w| LEGAL_FORMS.contains(w)) {
        words.pop();
    }
    words.join(" ")
}

pub fn title_key(title: &str) -> String {
    normalize(title)
        .split(' ')
        .filter(|w| !w.is_empty() && !TITLE_MARKERS.contains(w))
        .collect::<Vec<_>>()
        .join(" ")
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
    fn company_made_only_of_a_legal_word_is_kept() {
        assert_eq!(company_key("S.A."), "s");
        assert_eq!(company_key(""), "");
    }
}
