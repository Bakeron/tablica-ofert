//! Hard filters - plain code, no AI: instant, free and always the same answer.
//!
//! The filter is cautious: it rejects an offer only when the offer itself says it does not
//! fit (e.g. "office only" in a city outside the list). When the offer does not say
//! something, the answer is "unsure" - never a guess. It reads the whole offer text,
//! not only the structured fields ("Wrocław" in the city field, "remote possible" in the text).

use crate::cities;
use crate::dedup::{company_key, normalize};
use crate::model::*;

/// What the filter knows about an offer.
#[derive(Debug, Clone, Default)]
pub struct Facts {
    pub title: String,
    pub company: String,
    pub city: Option<String>,
    pub remote: Option<Remote>,
    pub salary_min: Option<i64>,
    pub salary_max: Option<i64>,
    pub salary_currency: Option<String>,
    pub salary_period: Option<SalaryPeriod>,
    pub contract: Option<String>,
    pub level: Option<String>,
    pub language: Option<String>,
    pub requirements: Option<String>,
    pub description: Option<String>,
    /// Where the offer came from (`offer_sources`), e.g. `["adzuna", "mail"]`.
    pub sources: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Evaluation {
    pub result: FilterResult,
    pub reasons: Vec<Reason>,
}

/// Best result over several profiles: one profile that fits is enough.
pub fn best(results: impl IntoIterator<Item = FilterResult>) -> FilterResult {
    let mut best: Option<FilterResult> = None;
    for r in results {
        best = Some(match (best, r) {
            (_, FilterResult::Pass) | (Some(FilterResult::Pass), _) => FilterResult::Pass,
            (_, FilterResult::Unsure) | (Some(FilterResult::Unsure), _) => FilterResult::Unsure,
            _ => FilterResult::Fail,
        });
    }
    // No profiles yet = nothing to filter by.
    best.unwrap_or(FilterResult::Pass)
}

/// Sources that search by keyword, but match words in the full text we never see (Adzuna and
/// Jooble give only the start of the description) and sometimes return offers far from the search.
const KEYWORD_SEARCH_SOURCES: &[&str] = &[crate::sources::adzuna::SOURCE, crate::sources::jooble::SOURCE];

/// An offer only from a keyword search that mentions no keyword of any profile - off topic.
/// Offers also seen elsewhere (e.g. in the user's own e-mail alerts) are never off topic.
pub fn off_topic(facts: &Facts, profiles: &[Profile]) -> bool {
    if facts.sources.is_empty() || !facts.sources.iter().all(|s| KEYWORD_SEARCH_SOURCES.contains(&s.as_str())) {
        return false;
    }
    let mut words = profiles.iter().flat_map(|p| &p.criteria.keywords).filter(|k| !k.trim().is_empty()).peekable();
    if words.peek().is_none() {
        return false;
    }
    let text = Text::new(facts);
    !words.any(|k| text.mentions(k))
}

/// Does this text mention any of the words (whole words, like the keyword filter)? For feeds
/// that send their whole list instead of searching - only offers for the user's words are kept.
pub fn mentions_any(title: &str, description: &str, words: &[String]) -> bool {
    let all = format!("{title}\n{description}");
    let text = Text { padded: format!(" {} ", normalize(&all)), raw: all.to_lowercase(), body_words: 0 };
    words.iter().any(|w| text.mentions(w))
}

impl Evaluation {
    /// Rejects an off-topic offer for this profile; replaces the profile's own keyword reasons.
    pub fn reject_off_topic(&mut self) {
        self.reasons.retain(|r| !r.code.starts_with("keyword_"));
        self.reasons.push(Reason { kind: ReasonKind::Fail, code: "keyword_none".into(), value: None });
        self.result = FilterResult::Fail;
    }
}

pub fn evaluate(facts: &Facts, c: &Criteria) -> Evaluation {
    let text = Text::new(facts);
    let mut out = Out::default();
    if c.hard.companies {
        companies(facts, c, &mut out);
    }
    if c.hard.exclude_words {
        exclude_words(&text, c, &mut out);
    }
    if c.hard.keywords {
        keywords(&text, c, &mut out);
    }
    if c.hard.location {
        location(facts, &text, c, &mut out);
    }
    if c.hard.contract {
        contract(facts, &text, c, &mut out);
    }
    if c.hard.level {
        level(facts, c, &mut out);
    }
    if c.hard.language {
        language(facts, c, &mut out);
    }
    if c.hard.salary {
        salary(facts, &text, c, &mut out);
    }
    let has = |kind| out.0.iter().any(|r: &Reason| r.kind == kind);
    let result = if has(ReasonKind::Fail) {
        FilterResult::Fail
    } else if has(ReasonKind::Unsure) {
        FilterResult::Unsure
    } else {
        FilterResult::Pass
    };
    Evaluation { result, reasons: out.0 }
}

#[derive(Default)]
struct Out(Vec<Reason>);

impl Out {
    fn push(&mut self, kind: ReasonKind, code: &str, value: Option<String>) {
        self.0.push(Reason { kind, code: code.into(), value });
    }
    fn fail(&mut self, code: &str, value: Option<String>) {
        self.push(ReasonKind::Fail, code, value);
    }
    fn unsure(&mut self, code: &str, value: Option<String>) {
        self.push(ReasonKind::Unsure, code, value);
    }
    fn note(&mut self, code: &str, value: Option<String>) {
        self.push(ReasonKind::Note, code, value);
    }
}

// ---------- reading the offer text ----------

struct Text {
    /// Normalized words of the whole offer with a space on both ends - `" praca zdalna "`
    /// matches only whole words.
    padded: String,
    /// Lowercase original text - for words with symbols like "C++" or ".NET".
    raw: String,
    /// Number of words in the description and requirements (how much there is to read).
    body_words: usize,
}

impl Text {
    fn new(f: &Facts) -> Self {
        let parts = [
            Some(f.title.as_str()),
            f.city.as_deref(),
            f.contract.as_deref(),
            f.level.as_deref(),
            f.requirements.as_deref(),
            f.description.as_deref(),
        ];
        let all = parts.iter().flatten().copied().collect::<Vec<_>>().join("\n");
        let body = [f.requirements.as_deref(), f.description.as_deref()];
        Self {
            padded: format!(" {} ", normalize(&all)),
            raw: all.to_lowercase(),
            body_words: body.iter().flatten().map(|t| t.split_whitespace().count()).sum(),
        }
    }

    /// Does the offer mention this word or phrase given by the user?
    fn mentions(&self, user_word: &str) -> bool {
        let word = user_word.trim().to_lowercase();
        let n = normalize(&word);
        // "C++", "C#", ".NET": normalizing would turn them into "c" / "net".
        let has_symbols = word.chars().any(|ch| !ch.is_alphanumeric() && ch != ' ' && ch != '-');
        if has_symbols || n.chars().count() < 2 {
            return !word.is_empty() && self.raw.contains(&word);
        }
        contains_phrase(&self.padded, &n)
    }
}

fn contains_phrase(padded: &str, phrase: &str) -> bool {
    padded.contains(&format!(" {phrase} "))
}

fn first_phrase(padded: &str, phrases: &[&'static str]) -> Option<&'static str> {
    phrases.iter().copied().find(|p| contains_phrase(padded, p))
}

const ENOUGH_TEXT: usize = 30;

// ---------- companies, excluded words, keywords ----------

fn companies(f: &Facts, c: &Criteria, out: &mut Out) {
    let offer = format!(" {} ", company_key(&f.company));
    for skip in &c.skip_companies {
        let key = company_key(skip);
        if !key.is_empty() && contains_phrase(&offer, &key) {
            out.fail("company_skipped", Some(skip.clone()));
            return;
        }
    }
}

fn exclude_words(text: &Text, c: &Criteria, out: &mut Out) {
    for word in &c.exclude_words {
        if text.mentions(word) {
            out.fail("excluded_word", Some(word.clone()));
        }
    }
}

fn keywords(text: &Text, c: &Criteria, out: &mut Out) {
    if c.keywords.is_empty() || c.keywords.iter().any(|k| text.mentions(k)) {
        return;
    }
    if text.body_words >= ENOUGH_TEXT {
        out.fail("keyword_missing", None);
    } else {
        out.unsure("keyword_unknown", None);
    }
}

// ---------- location and form of work ----------

/// Phrases that deny remote work - removed before looking for "remote" words.
const NO_REMOTE: &[&str] = &[
    "brak mozliwosci pracy zdalnej",
    "bez mozliwosci pracy zdalnej",
    "nie ma mozliwosci pracy zdalnej",
    "nie oferujemy pracy zdalnej",
    "praca zdalna nie jest mozliwa",
    "praca zdalna nie",
    "nie pracujemy zdalnie",
    "zdalnie nie",
    "no remote work",
    "no remote",
    "not remote",
    "remote work is not possible",
    "remote not possible",
];

const OFFICE_ONLY: &[&str] = &[
    "wylacznie stacjonarna",
    "wylacznie stacjonarnie",
    "tylko stacjonarnie",
    "tylko stacjonarna",
    "w pelni stacjonarna",
    "100 stacjonarnie",
    "praca stacjonarna",
    "pracy stacjonarnej",
    "stacjonarnie",
    "fully on site",
    "100 on site",
    "on site",
    "onsite",
    "office based",
    "100 office",
];

/// Remote work for sure, even next to words about hybrid work ("hybrid or fully remote").
const STRONG_REMOTE: &[&str] = &[
    "w pelni zdalna",
    "w pelni zdalnie",
    "100 zdalnie",
    "100 zdalna",
    "calkowicie zdalna",
    "calkowicie zdalnie",
    "lub zdalnie",
    "albo zdalnie",
    "lub w pelni zdalnie",
    "fully remote",
    "100 remote",
    "remote first",
    "or remote",
    "or fully remote",
];

const REMOTE: &[&str] = &[
    "praca zdalna",
    "pracy zdalnej",
    "prace zdalna",
    "praca z domu",
    "zdalnie",
    "zdalna",
    "zdalnej",
    "zdalny",
    "remote",
    "work from home",
    "home office",
    "wfh",
];

const HYBRID: &[&str] = &[
    "hybrydowo",
    "hybrydowa",
    "hybrydowy",
    "hybrydowej",
    "hybrydowym",
    "hybryda",
    "hybrid",
];

const OFFICE_WORDS: &[&str] = &[
    "biurze", "biura", "biuro", "office", "stacjonarnie", "siedzibie", "siedziby", "onsite", "site",
];

/// Words that may stand between a number and its period: "2 dni w tygodniu", "once a month".
const CONNECTORS: &[&str] = &[
    "dni", "dzien", "dnia", "razy", "raz", "w", "na", "a", "per", "in", "the", "times", "time", "days",
    "day", "x", "do", "po", "each", "every", "co", "za",
];

#[derive(Debug, Clone, Copy, PartialEq)]
enum Mode {
    Remote,
    /// Office days per month, when the offer says.
    Hybrid(Option<f32>),
    Office,
    Unknown,
}

fn small_number(token: &str) -> Option<f32> {
    let digits = token.strip_suffix('x').unwrap_or(token);
    if !digits.is_empty() && digits.len() <= 2 && digits.bytes().all(|b| b.is_ascii_digit()) {
        return digits.parse().ok();
    }
    Some(match token {
        "raz" | "jeden" | "jedna" | "once" | "one" => 1.0,
        "dwa" | "dwie" | "dwoch" | "twice" | "two" => 2.0,
        "trzy" | "trzech" | "three" => 3.0,
        "cztery" | "czterech" | "four" => 4.0,
        "piec" | "five" => 5.0,
        "kilka" | "few" => 3.0,
        _ => return None,
    })
}

/// How many periods fit in a month.
fn per_month(token: &str) -> Option<f32> {
    Some(match token {
        "tygodniu" | "tydzien" | "tygodnie" | "tygodni" | "tygodniowo" | "week" | "weeks" | "weekly" => 4.0,
        "miesiacu" | "miesiac" | "miesiace" | "miesiecy" | "miesiecznie" | "month" | "months" | "monthly" => 1.0,
        "kwartal" | "kwartale" | "quarter" | "quarterly" => 1.0 / 3.0,
        "roku" | "rok" | "year" | "yearly" => 1.0 / 12.0,
        _ => return None,
    })
}

/// Office visits per month from phrases like "2 dni w tygodniu w biurze",
/// "raz w miesiącu w biurze", "office once a month", "co dwa tygodnie w biurze".
/// When the offer gives several numbers, the smallest one counts (benefit of the doubt).
fn office_frequency(padded: &str) -> Option<f32> {
    let tokens: Vec<&str> = padded.split_whitespace().collect();
    let near_office = |from: usize, to: usize| {
        let a = from.saturating_sub(4);
        let b = (to + 5).min(tokens.len());
        tokens[a..b].iter().any(|t| OFFICE_WORDS.contains(t))
    };
    let mut found: Option<f32> = None;
    let mut i = 0;
    while i < tokens.len() {
        let Some(mut n) = small_number(tokens[i]) else {
            i += 1;
            continue;
        };
        let start = i;
        // Ranges like "2-3 dni" become "2 3 dni" after normalizing - take the lower one.
        while let Some(next) = tokens.get(i + 1).and_then(|t| small_number(t)) {
            n = n.min(next);
            i += 1;
        }
        let every = start > 0 && matches!(tokens[start - 1], "co" | "every");
        for j in (i + 1)..(i + 5).min(tokens.len()) {
            if let Some(factor) = per_month(tokens[j]) {
                let visits = if every { factor / n.max(1.0) } else { n * factor };
                if near_office(start, j) {
                    found = Some(found.map_or(visits, |f: f32| f.min(visits)));
                }
                break;
            }
            if !CONNECTORS.contains(&tokens[j]) {
                break;
            }
        }
        i += 1;
    }
    found
}

/// Form of work, and the phrase from the text that decided it (when the field was silent).
fn work_mode(f: &Facts, text: &Text) -> (Mode, Option<&'static str>) {
    let mut cleaned = text.padded.clone();
    for phrase in NO_REMOTE {
        let p = format!(" {phrase} ");
        while cleaned.contains(&p) {
            cleaned = cleaned.replace(&p, " ");
        }
    }
    let field = f.remote.unwrap_or(Remote::Unknown);
    if field == Remote::Yes {
        return (Mode::Remote, None);
    }
    let from_text = field != Remote::Hybrid;
    let quote = |p: &'static str| if from_text { Some(p) } else { None };

    if let Some(p) = first_phrase(&cleaned, STRONG_REMOTE) {
        return (Mode::Remote, quote(p));
    }
    let freq = office_frequency(&cleaned);
    let hybrid_word = first_phrase(&cleaned, HYBRID);
    if field == Remote::Hybrid || hybrid_word.is_some() || freq.is_some() {
        return (Mode::Hybrid(freq), hybrid_word.and_then(quote));
    }
    if let Some(p) = first_phrase(&cleaned, REMOTE) {
        return (Mode::Remote, quote(p));
    }
    let office_word = first_phrase(&text.padded, OFFICE_ONLY).or_else(|| first_phrase(&text.padded, NO_REMOTE));
    if field == Remote::No || office_word.is_some() {
        return (Mode::Office, if field == Remote::No { None } else { office_word });
    }
    (Mode::Unknown, None)
}

/// City fields that are not a place.
const NOT_A_PLACE: &[&str] = &["polska", "poland", "cala polska", "europa", "europe", "remote", "zdalnie", "praca zdalna"];

/// Is the offer's office in one of my cities (or close enough)?
/// `None` = cannot tell (no city, or a city missing from the table).
fn in_area(city: Option<&str>, rules: &[CityRule]) -> Option<bool> {
    if rules.is_empty() {
        return Some(true);
    }
    let city = city.map(str::trim).filter(|c| !c.is_empty())?;
    let normalized = normalize(city);
    if NOT_A_PLACE.contains(&normalized.as_str()) {
        return None;
    }
    let padded = format!(" {normalized} ");
    if rules.iter().any(|r| {
        let n = normalize(&r.name);
        !n.is_empty() && contains_phrase(&padded, &n)
    }) {
        return Some(true);
    }
    let found = cities::mentioned(city);
    let mut cannot_measure = found.is_empty();
    for rule in rules {
        let Some(home) = cities::coordinates(&rule.name) else {
            // My city is not in the table: only its own name can match.
            cannot_measure |= rule.radius_km > 0;
            continue;
        };
        for name in &found {
            if let Some(place) = cities::coordinates(name) {
                // 3 km slack: the table holds city centres, not borders.
                if cities::distance_km(home, place) <= f64::from(rule.radius_km) + 3.0 {
                    return Some(true);
                }
            }
        }
    }
    // An unknown city is surely "not mine" only when every rule means "this city only".
    if cannot_measure && rules.iter().any(|r| r.radius_km > 0) { None } else { Some(false) }
}

fn location(f: &Facts, text: &Text, c: &Criteria, out: &mut Out) {
    let all = [Remote::Yes, Remote::Hybrid, Remote::No];
    let wants = |m: Remote| c.work_modes.is_empty() || c.work_modes.contains(&m);
    if c.cities.is_empty() && all.iter().all(|m| wants(*m)) {
        return;
    }
    let area = in_area(f.city.as_deref(), &c.cities);
    let (mode, quote) = work_mode(f, text);
    if let Some(q) = quote {
        out.note("work_mode_in_text", Some(q.to_string()));
    }
    let city = || f.city.clone();
    let office_ok = wants(Remote::Hybrid) || wants(Remote::No);

    match mode {
        Mode::Remote => {
            // Never rejected: "remote possible" may still allow the office.
            if !wants(Remote::Yes) && area != Some(true) {
                out.unsure("remote_not_wanted", None);
            }
        }
        Mode::Office => {
            if !wants(Remote::No) {
                out.fail("office_not_wanted", None);
            } else {
                match area {
                    Some(true) => {}
                    Some(false) => out.fail("office_outside", city()),
                    None => out.unsure("city_unknown", city()),
                }
            }
        }
        Mode::Hybrid(freq) => {
            if area == Some(true) && office_ok {
                return;
            }
            match (c.commute_per_month, freq) {
                (Some(limit), Some(visits)) => {
                    let shown = Some(format_visits(visits));
                    if visits <= f32::from(limit) + 0.01 {
                        out.note("commute_ok", shown);
                    } else {
                        out.fail("commute_too_often", shown);
                    }
                }
                (Some(_), None) => out.unsure("commute_unknown", None),
                (None, _) => match area {
                    None => out.unsure("city_unknown", city()),
                    Some(false) if office_ok => out.fail("hybrid_outside", city()),
                    _ => out.fail("hybrid_not_wanted", None),
                },
            }
        }
        Mode::Unknown => match area {
            Some(true) if office_ok => {}
            Some(false) if !wants(Remote::Yes) && c.commute_per_month.is_none() => {
                out.fail("office_outside", city())
            }
            _ => out.unsure("remote_unknown", None),
        },
    }
}

fn format_visits(v: f32) -> String {
    if (v - v.round()).abs() < 0.05 { format!("{}", v.round() as i64) } else { format!("{v:.1}").replace('.', ",") }
}

// ---------- contract, level, language ----------

const UOP: &[&str] = &[
    "uop",
    "umowa o prace",
    "umowe o prace",
    "umowy o prace",
    "employment contract",
    "contract of employment",
    "permanent contract",
    "etat",
    "pelny etat",
];
const B2B: &[&str] = &["b2b"];
const MANDATE: &[&str] = &[
    "zlecenie",
    "umowa zlecenie",
    "umowa zlecenia",
    "umowe zlecenie",
    "mandate contract",
    "contract of mandate",
    "umowa o dzielo",
    "dzielo",
];

fn contracts_in(padded: &str) -> Vec<Contract> {
    let mut found = Vec::new();
    for (kind, phrases) in [(Contract::Uop, UOP), (Contract::B2b, B2B), (Contract::Mandate, MANDATE)] {
        if first_phrase(padded, phrases).is_some() {
            found.push(kind);
        }
    }
    found
}

fn field_contracts(f: &Facts) -> Vec<Contract> {
    f.contract.as_deref().map(|c| contracts_in(&format!(" {} ", normalize(c)))).unwrap_or_default()
}

fn contract(f: &Facts, text: &Text, c: &Criteria, out: &mut Out) {
    if c.contracts.is_empty() {
        return;
    }
    let field = field_contracts(f);
    if !field.is_empty() {
        if !field.iter().any(|k| c.contracts.contains(k)) {
            out.fail("contract_mismatch", f.contract.clone());
        }
        return;
    }
    // Words in the description are only a hint ("B2B" may be about sales), never a reason to reject.
    if !contracts_in(&text.padded).iter().any(|k| c.contracts.contains(k)) {
        out.unsure("contract_unknown", None);
    }
}

const JUNIOR: &[&str] = &[
    "junior", "jr", "mlodszy", "mlodsza", "intern", "internship", "staz", "stazysta", "stazystka", "trainee",
    "praktykant", "praktykantka", "entry level",
];
const MID: &[&str] = &["mid", "regular", "middle", "medior", "mid level"];
const SENIOR: &[&str] = &["senior", "sr", "starszy", "starsza", "lead", "principal", "ekspert", "expert"];

fn levels_in(text: &str) -> Vec<Level> {
    let padded = format!(" {} ", normalize(text));
    let mut found = Vec::new();
    for (level, words) in [(Level::Junior, JUNIOR), (Level::Mid, MID), (Level::Senior, SENIOR)] {
        if first_phrase(&padded, words).is_some() {
            found.push(level);
        }
    }
    found
}

fn level(f: &Facts, c: &Criteria, out: &mut Out) {
    if c.levels.is_empty() || c.levels.len() == 3 {
        return;
    }
    // The level field first; otherwise the job title ("Senior ...") - the description
    // is not used, it often mentions other levels ("you will mentor juniors").
    let mut found = f.level.as_deref().map(levels_in).unwrap_or_default();
    let mut shown = f.level.clone();
    if found.is_empty() {
        found = levels_in(&f.title);
        shown = Some(f.title.clone());
    }
    if found.is_empty() {
        out.unsure("level_unknown", None);
    } else if !found.iter().any(|l| c.levels.contains(l)) {
        out.fail("level_mismatch", shown);
    }
}

const PL_WORDS: &[&str] = &[
    "i", "w", "z", "na", "oraz", "dla", "jest", "się", "nie", "do", "lub", "od", "pracy", "będzie", "będziesz",
    "oferujemy", "wymagania", "zespole", "jak", "przy", "że",
];
const EN_WORDS: &[&str] = &[
    "the", "and", "with", "for", "you", "we", "our", "is", "are", "of", "in", "will", "your", "be", "as", "on",
];

/// Language of the offer text - only when there is enough text and a clear majority.
fn detect_language(f: &Facts) -> Option<OfferLanguage> {
    let body = [f.requirements.as_deref(), f.description.as_deref()];
    let text = body.iter().flatten().copied().collect::<Vec<_>>().join(" ").to_lowercase();
    let words: Vec<&str> = text.split(|ch: char| !ch.is_alphabetic()).filter(|w| !w.is_empty()).collect();
    if words.len() < ENOUGH_TEXT {
        return None;
    }
    let pl = words.iter().filter(|w| PL_WORDS.contains(w)).count();
    let en = words.iter().filter(|w| EN_WORDS.contains(w)).count();
    if pl >= 5 && pl >= 2 * en {
        Some(OfferLanguage::Pl)
    } else if en >= 5 && en >= 2 * pl {
        Some(OfferLanguage::En)
    } else {
        None
    }
}

fn field_language(f: &Facts) -> Option<OfferLanguage> {
    match normalize(f.language.as_deref()?).as_str() {
        "pl" | "pol" | "polski" | "polish" | "polska" => Some(OfferLanguage::Pl),
        "en" | "eng" | "angielski" | "english" => Some(OfferLanguage::En),
        _ => None,
    }
}

fn language(f: &Facts, c: &Criteria, out: &mut Out) {
    if c.languages.is_empty() || (c.languages.contains(&OfferLanguage::Pl) && c.languages.contains(&OfferLanguage::En)) {
        return;
    }
    match field_language(f).or_else(|| detect_language(f)) {
        None => out.unsure("language_unknown", None),
        Some(lang) if !c.languages.contains(&lang) => out.fail("language_mismatch", Some(lang.as_str().into())),
        Some(_) => {}
    }
}

// ---------- salary ----------

/// Full-time hours in a month - to compare hourly and monthly rates.
const HOURS_PER_MONTH: f64 = 168.0;
/// Rough share of a gross employment-contract salary that is paid out (2026, orientacyjnie).
const UOP_NET_SHARE: f64 = 0.72;
const VAT: f64 = 1.23;

fn monthly(amount: i64, period: SalaryPeriod) -> f64 {
    match period {
        SalaryPeriod::Month => amount as f64,
        SalaryPeriod::Hour => amount as f64 * HOURS_PER_MONTH,
    }
}

fn salary(f: &Facts, text: &Text, c: &Criteria, out: &mut Out) {
    if c.salary_uop.min.is_none() && c.salary_b2b.min.is_none() {
        return;
    }
    let Some(top) = f.salary_max.or(f.salary_min) else {
        // An offer without a salary range is not rejected - it only gets a mark.
        out.note("no_salary", None);
        return;
    };
    let currency = f.salary_currency.as_deref().unwrap_or("PLN");
    if currency != "PLN" {
        out.unsure("salary_currency", Some(currency.into()));
        return;
    }
    let offer = monthly(top, f.salary_period.unwrap_or(SalaryPeriod::Month));

    let mut kinds = field_contracts(f);
    if kinds.is_empty() {
        kinds = contracts_in(&text.padded);
    }
    if kinds.is_empty() {
        kinds = vec![Contract::Uop, Contract::B2b];
    }
    let (mut checked, mut reaches, mut maybe) = (false, false, false);
    for kind in kinds {
        let b2b = kind == Contract::B2b;
        let rule = if b2b { &c.salary_b2b } else { &c.salary_uop };
        let Some(min) = rule.min else { continue };
        checked = true;
        // Offers usually give gross amounts for employment contracts and net (without VAT) for B2B.
        let usual = if b2b { PayBasis::Net } else { PayBasis::Gross };
        let mut needed = monthly(min, rule.period);
        let exact = rule.basis == usual;
        if !exact {
            needed = match (b2b, rule.basis) {
                (false, PayBasis::Net) => needed / UOP_NET_SHARE,
                (true, PayBasis::Gross) => needed / VAT,
                _ => needed,
            };
        }
        if offer >= needed {
            reaches = true;
        } else if !exact {
            maybe = true;
        }
    }
    if !checked || reaches {
        return;
    }
    if maybe {
        out.unsure("salary_maybe_low", None);
    } else {
        out.fail("salary_low", None);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn offer(title: &str, city: Option<&str>, remote: Remote, description: &str) -> Facts {
        Facts {
            title: title.into(),
            company: "Firma Testowa".into(),
            city: city.map(Into::into),
            remote: Some(remote),
            description: Some(description.into()),
            ..Default::default()
        }
    }

    fn remote_only() -> Criteria {
        Criteria { work_modes: vec![Remote::Yes], ..Default::default() }
    }

    fn in_city(name: &str, radius_km: u16) -> Criteria {
        Criteria {
            work_modes: vec![Remote::Hybrid, Remote::No],
            cities: vec![CityRule { name: name.into(), radius_km }],
            ..Default::default()
        }
    }

    fn result(f: &Facts, c: &Criteria) -> FilterResult {
        evaluate(f, c).result
    }

    /// Codes that decided the result (notes left out).
    fn codes(f: &Facts, c: &Criteria) -> Vec<String> {
        evaluate(f, c).reasons.into_iter().filter(|r| r.kind != ReasonKind::Note).map(|r| r.code).collect()
    }

    const LONG_PL: &str = "Dołącz do naszego zespołu i twórz z nami aplikację dla szkół. Oferujemy pracę w małym \
        zespole, szkolenia oraz dofinansowanie do sportu. Wymagania: znajomość TypeScript i React, chęć do nauki \
        i dobra komunikacja w zespole. Będziesz pracować przy nowych funkcjach dla nauczycieli i uczniów.";
    const LONG_EN: &str = "Join our team and build a learning app for schools with us. We offer work in a small \
        team, training and a sports card. You will work on new features for teachers and students, and you \
        will be part of our product team. Requirements: TypeScript and React, and a will to learn.";

    #[test]
    fn no_profile_means_pass_and_best_picks_the_most_open_result() {
        let f = offer("Dev", Some("Gdańsk"), Remote::No, "");
        assert_eq!(result(&f, &Criteria::default()), FilterResult::Pass);
        assert_eq!(best([]), FilterResult::Pass);
        assert_eq!(best([FilterResult::Fail, FilterResult::Unsure]), FilterResult::Unsure);
        assert_eq!(best([FilterResult::Fail, FilterResult::Pass, FilterResult::Unsure]), FilterResult::Pass);
        assert_eq!(best([FilterResult::Fail]), FilterResult::Fail);
    }

    #[test]
    fn city_field_alone_is_unsure_for_a_remote_seeker() {
        // E-mail alert with only a city - nothing about remote work.
        let f = offer("Frontend Developer", Some("Wrocław"), Remote::Unknown, "");
        assert_eq!(result(&f, &remote_only()), FilterResult::Unsure);
        assert_eq!(codes(&f, &remote_only()), vec!["remote_unknown"]);
    }

    #[test]
    fn remote_phrase_in_the_text_wins_over_the_city() {
        let f = offer("Frontend Developer", Some("Wrocław"), Remote::Unknown, "Biuro we Wrocławiu, możliwa praca zdalna.");
        let e = evaluate(&f, &remote_only());
        assert_eq!(e.result, FilterResult::Pass);
        assert_eq!(e.reasons[0].code, "work_mode_in_text");
        assert_eq!(e.reasons[0].value.as_deref(), Some("praca zdalna"));

        let english = offer("Frontend Developer", Some("Kraków"), Remote::No, "We work remote from anywhere in Poland.");
        assert_eq!(result(&english, &remote_only()), FilterResult::Pass);
    }

    #[test]
    fn office_only_outside_my_cities_is_rejected() {
        let f = offer("Księgowy", Some("Kraków"), Remote::Unknown, "Praca wyłącznie stacjonarna w biurze.");
        assert_eq!(result(&f, &remote_only()), FilterResult::Fail);
        assert_eq!(codes(&f, &in_city("Gdańsk", 30)), vec!["office_outside"]);
        assert_eq!(result(&f, &in_city("Kraków", 0)), FilterResult::Pass);
    }

    #[test]
    fn denied_remote_work_is_not_read_as_remote() {
        let f = offer("Tester", Some("Łódź"), Remote::Unknown, "Brak możliwości pracy zdalnej.");
        assert_eq!(result(&f, &remote_only()), FilterResult::Fail);
        let g = offer("Tester", Some("Łódź"), Remote::Unknown, "No remote work, sorry.");
        assert_eq!(result(&g, &remote_only()), FilterResult::Fail);
    }

    #[test]
    fn rare_office_visits_pass_with_a_commute_allowance() {
        let mut c = remote_only();
        c.commute_per_month = Some(2);
        let once = offer("Grafik", Some("Poznań"), Remote::Hybrid, "Praca hybrydowa, raz w miesiącu w biurze.");
        let e = evaluate(&once, &c);
        assert_eq!(e.result, FilterResult::Pass);
        assert_eq!(e.reasons, vec![Reason { kind: ReasonKind::Note, code: "commute_ok".into(), value: Some("1".into()) }]);

        let weekly = offer("Grafik", Some("Poznań"), Remote::Hybrid, "2 dni w tygodniu w biurze w Poznaniu.");
        let e = evaluate(&weekly, &c);
        assert_eq!(e.result, FilterResult::Fail);
        assert_eq!(e.reasons[0].value.as_deref(), Some("8"));

        let unknown = offer("Grafik", Some("Poznań"), Remote::Hybrid, "Praca hybrydowa.");
        assert_eq!(codes(&unknown, &c), vec!["commute_unknown"]);

        // Without any allowance, hybrid work means trips - rejected.
        assert_eq!(codes(&once, &remote_only()), vec!["hybrid_not_wanted"]);
    }

    #[test]
    fn office_frequency_reads_common_phrases() {
        let freq = |t: &str| office_frequency(&format!(" {} ", normalize(t)));
        assert_eq!(freq("Biuro odwiedzamy raz w miesiącu."), Some(1.0));
        assert_eq!(freq("2-3 dni w tygodniu w biurze"), Some(8.0));
        assert_eq!(freq("1x w tygodniu w biurze"), Some(4.0));
        assert_eq!(freq("Office once a month"), Some(1.0));
        assert_eq!(freq("W biurze co dwa tygodnie"), Some(2.0));
        assert_eq!(freq("Spotkanie w biurze raz na kwartał"), Some(1.0 / 3.0));
        // Holidays are not office days.
        assert_eq!(freq("26 dni urlopu w roku, biuro w centrum"), None);
        assert_eq!(freq("3 lata doświadczenia"), None);
    }

    #[test]
    fn hybrid_or_fully_remote_counts_as_remote() {
        let f = offer("Dev", Some("Lublin"), Remote::Unknown, "Praca hybrydowa lub w pełni zdalnie - jak wolisz.");
        assert_eq!(result(&f, &remote_only()), FilterResult::Pass);
    }

    #[test]
    fn hybrid_in_my_city_passes_and_outside_fails() {
        let c = in_city("Warszawa", 30);
        let near = offer("Analityk", Some("Pruszków"), Remote::Hybrid, "");
        assert_eq!(result(&near, &c), FilterResult::Pass);
        let far = offer("Analityk", Some("Kraków"), Remote::Hybrid, "");
        assert_eq!(codes(&far, &c), vec!["hybrid_outside"]);
        let english = offer("Analyst", Some("Warsaw"), Remote::No, "");
        assert_eq!(result(&english, &in_city("Warszawa", 0)), FilterResult::Pass);
    }

    #[test]
    fn unknown_city_is_unsure_unless_only_my_city_counts() {
        // "Mała Wieś" is not in the table - distance cannot be measured.
        let f = offer("Magazynier", Some("Mała Wieś"), Remote::No, "");
        assert_eq!(codes(&f, &in_city("Warszawa", 30)), vec!["city_unknown"]);
        assert_eq!(codes(&f, &in_city("Warszawa", 0)), vec!["office_outside"]);
        let country = offer("Magazynier", Some("Polska"), Remote::No, "");
        assert_eq!(codes(&country, &in_city("Warszawa", 0)), vec!["city_unknown"]);
        let none = offer("Magazynier", None, Remote::No, "");
        assert_eq!(codes(&none, &in_city("Warszawa", 30)), vec!["city_unknown"]);
    }

    #[test]
    fn office_offer_for_someone_who_wants_hybrid_only() {
        let c = Criteria {
            work_modes: vec![Remote::Hybrid],
            cities: vec![CityRule { name: "Gdańsk".into(), radius_km: 20 }],
            ..Default::default()
        };
        let f = offer("Dev", Some("Gdynia"), Remote::No, "");
        assert_eq!(codes(&f, &c), vec!["office_not_wanted"]);
        let g = offer("Dev", Some("Gdynia"), Remote::Hybrid, "");
        assert_eq!(result(&g, &c), FilterResult::Pass);
    }

    #[test]
    fn contract_from_the_field_is_certain_from_the_text_only_a_hint() {
        let c = Criteria { contracts: vec![Contract::Uop], ..Default::default() };
        let mut f = offer("Dev", None, Remote::Yes, "");
        f.contract = Some("B2B".into());
        assert_eq!(codes(&f, &c), vec!["contract_mismatch"]);
        f.contract = Some("UoP lub B2B".into());
        assert_eq!(result(&f, &c), FilterResult::Pass);
        f.contract = None;
        assert_eq!(codes(&f, &c), vec!["contract_unknown"]);
        // "B2B" in the description is about clients - not a reason to reject.
        f.description = Some("Obsługa klientów B2B.".into());
        assert_eq!(result(&f, &c), FilterResult::Unsure);
        f.description = Some("Zatrudnienie na umowę o pracę.".into());
        assert_eq!(result(&f, &c), FilterResult::Pass);
    }

    #[test]
    fn level_from_the_field_or_the_title() {
        let c = Criteria { levels: vec![Level::Junior], ..Default::default() };
        let senior = offer("Senior Java Developer", None, Remote::Yes, "Będziesz wspierać juniorów.");
        assert_eq!(codes(&senior, &c), vec!["level_mismatch"]);
        let junior = offer("Młodszy księgowy", None, Remote::Yes, "");
        assert_eq!(result(&junior, &c), FilterResult::Pass);
        let unknown = offer("Java Developer", None, Remote::Yes, "Mentoring juniorów.");
        assert_eq!(codes(&unknown, &c), vec!["level_unknown"]);
        let mut both = offer("Java Developer", None, Remote::Yes, "");
        both.level = Some("Junior / Mid".into());
        assert_eq!(result(&both, &c), FilterResult::Pass);
    }

    #[test]
    fn language_from_the_field_or_enough_text() {
        let c = Criteria { languages: vec![OfferLanguage::Pl], ..Default::default() };
        let pl = offer("Dev", None, Remote::Yes, LONG_PL);
        assert_eq!(result(&pl, &c), FilterResult::Pass);
        let en = offer("Dev", None, Remote::Yes, LONG_EN);
        assert_eq!(codes(&en, &c), vec!["language_mismatch"]);
        let short = offer("Dev", None, Remote::Yes, "Krótki opis.");
        assert_eq!(codes(&short, &c), vec!["language_unknown"]);
        let mut field = offer("Dev", None, Remote::Yes, LONG_EN);
        field.language = Some("pl".into());
        assert_eq!(result(&field, &c), FilterResult::Pass);
    }

    #[test]
    fn missing_salary_passes_with_a_mark() {
        let mut c = Criteria::default();
        c.salary_uop.min = Some(9000);
        let f = offer("Dev", None, Remote::Yes, "");
        let e = evaluate(&f, &c);
        assert_eq!(e.result, FilterResult::Pass);
        assert_eq!(e.reasons[0].code, "no_salary");
    }

    #[test]
    fn salary_compares_the_top_of_the_range() {
        let mut c = Criteria::default();
        c.salary_uop.min = Some(9000);
        let mut f = offer("Dev", None, Remote::Yes, "");
        f.contract = Some("UoP".into());
        f.salary_min = Some(7000);
        f.salary_max = Some(9500);
        assert_eq!(result(&f, &c), FilterResult::Pass);
        f.salary_max = Some(8000);
        assert_eq!(codes(&f, &c), vec!["salary_low"]);
        // Hourly rate: 60 PLN x 168 h = 10 080 PLN a month.
        f.salary_min = Some(60);
        f.salary_max = None;
        f.salary_period = Some(SalaryPeriod::Hour);
        assert_eq!(result(&f, &c), FilterResult::Pass);
        f.salary_currency = Some("EUR".into());
        assert_eq!(codes(&f, &c), vec!["salary_currency"]);
    }

    #[test]
    fn salary_rule_follows_the_contract_and_the_basis() {
        let mut c = Criteria::default();
        c.salary_uop.min = Some(8000);
        c.salary_b2b = SalaryRule { min: Some(15000), basis: PayBasis::Net, ..Default::default() };
        let mut b2b = offer("Dev", None, Remote::Yes, "");
        b2b.contract = Some("B2B".into());
        b2b.salary_max = Some(12000);
        assert_eq!(codes(&b2b, &c), vec!["salary_low"]);
        // Contract unknown: enough when either expectation is reached.
        b2b.contract = None;
        assert_eq!(result(&b2b, &c), FilterResult::Pass);
        // Net expectation for an employment contract: 8 000 net is about 11 100 gross -
        // 10 000 gross may be too little, but it is only an estimate.
        let mut net = Criteria::default();
        net.salary_uop = SalaryRule { min: Some(8000), basis: PayBasis::Net, ..Default::default() };
        let mut uop = offer("Dev", None, Remote::Yes, "");
        uop.contract = Some("umowa o pracę".into());
        uop.salary_max = Some(10000);
        assert_eq!(codes(&uop, &net), vec!["salary_maybe_low"]);
        uop.salary_max = Some(12000);
        assert_eq!(result(&uop, &net), FilterResult::Pass);
    }

    #[test]
    fn skipped_companies_and_excluded_words() {
        let c = Criteria {
            skip_companies: vec!["Firma Testowa".into()],
            exclude_words: vec!["sprzedaż bezpośrednia".into()],
            ..Default::default()
        };
        let mut f = offer("Dev", None, Remote::Yes, "");
        f.company = "FIRMA TESTOWA Sp. z o.o.".into();
        assert_eq!(codes(&f, &c), vec!["company_skipped"]);
        let mut g = offer("Doradca", None, Remote::Yes, "Sprzedaż bezpośrednia u klienta.");
        g.company = "Inna Firma".into();
        assert_eq!(codes(&g, &c), vec!["excluded_word"]);
    }

    #[test]
    fn offers_from_a_keyword_search_without_any_keyword_are_off_topic() {
        let profile = |keywords: &[&str]| Profile {
            id: 1,
            name: "Plan".into(),
            description: "x".into(),
            position: 0,
            description_prev: None,
            criteria: Criteria { keywords: keywords.iter().map(|k| k.to_string()).collect(), ..Default::default() },
        };
        let profiles = vec![profile(&["WordPress", "PHP"]), profile(&[]), profile(&["ACF"])];
        let mut f = offer("Zwrotniczy (K/M)", None, Remote::Unknown, "Praca na stacji kolejowej.");
        f.sources = vec!["adzuna".into()];
        assert!(off_topic(&f, &profiles));
        // Any keyword of any profile, in the title or the description, is enough.
        let mut g = f.clone();
        g.description = Some("Wtyczki ACF i motywy.".into());
        assert!(!off_topic(&g, &profiles));
        // Seen in the user's own alerts too, or added by hand - never off topic.
        let mut h = f.clone();
        h.sources.push("mail".into());
        assert!(!off_topic(&h, &profiles));
        h.sources = vec!["manual".into()];
        assert!(!off_topic(&h, &profiles));
        // No keywords at all - nothing to compare with.
        assert!(!off_topic(&f, &[profile(&[])]));

        let mut c = Criteria { keywords: vec!["PHP".into()], ..Default::default() };
        c.hard.keywords = true;
        let mut e = evaluate(&f, &c);
        e.reject_off_topic();
        assert_eq!(e.result, FilterResult::Fail);
        assert_eq!(e.reasons.iter().filter(|r| r.code.starts_with("keyword_")).map(|r| r.code.as_str()).collect::<Vec<_>>(), vec!["keyword_none"]);
    }

    #[test]
    fn keywords_as_a_hard_filter_and_words_with_symbols() {
        let mut c = Criteria { keywords: vec!["C#".into(), "Unity".into()], ..Default::default() };
        let f = offer("Developer", None, Remote::Yes, LONG_PL);
        // By default keywords only feed the match score.
        assert_eq!(result(&f, &c), FilterResult::Pass);
        c.hard.keywords = true;
        assert_eq!(codes(&f, &c), vec!["keyword_missing"]);
        let short = offer("Developer", None, Remote::Yes, "Krótko.");
        assert_eq!(codes(&short, &c), vec!["keyword_unknown"]);
        let csharp = offer("C# Developer", None, Remote::Yes, "");
        assert_eq!(result(&csharp, &c), FilterResult::Pass);
        // "C" alone is not "C#".
        let plain_c = offer("C Developer", None, Remote::Yes, LONG_PL);
        assert_eq!(result(&plain_c, &c), FilterResult::Fail);
    }

    #[test]
    fn criteria_switched_to_score_only_never_reject() {
        let mut c = in_city("Gdańsk", 0);
        c.contracts = vec![Contract::Uop];
        c.hard.location = false;
        c.hard.contract = false;
        let mut f = offer("Dev", Some("Kraków"), Remote::No, "Praca wyłącznie stacjonarna.");
        f.contract = Some("B2B".into());
        assert_eq!(result(&f, &c), FilterResult::Pass);
    }

    #[test]
    fn several_problems_are_all_listed_and_fail_wins() {
        let mut c = remote_only();
        c.contracts = vec![Contract::Uop];
        let f = offer("Dev", Some("Kraków"), Remote::No, "");
        let e = evaluate(&f, &c);
        assert_eq!(e.result, FilterResult::Fail);
        assert_eq!(codes(&f, &c), vec!["office_not_wanted", "contract_unknown"]);
    }
}
