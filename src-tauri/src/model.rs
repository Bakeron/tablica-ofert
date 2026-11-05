//! Data shared between the database and the UI. Field names go to TypeScript as camelCase.

use serde::{Deserialize, Serialize};

/// Enum stored in the database and sent to the UI as a short lowercase word.
macro_rules! word_enum {
    ($name:ident { $($variant:ident => $word:literal),+ $(,)? }) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
        pub enum $name {
            $( #[serde(rename = $word)] $variant ),+
        }

        impl $name {
            pub fn as_str(self) -> &'static str {
                match self { $( Self::$variant => $word ),+ }
            }

            pub fn parse(word: &str) -> Option<Self> {
                match word { $( $word => Some(Self::$variant), )+ _ => None }
            }
        }

        impl rusqlite::types::ToSql for $name {
            fn to_sql(&self) -> rusqlite::Result<rusqlite::types::ToSqlOutput<'_>> {
                Ok(self.as_str().into())
            }
        }

        impl rusqlite::types::FromSql for $name {
            fn column_result(value: rusqlite::types::ValueRef<'_>) -> rusqlite::types::FromSqlResult<Self> {
                Self::parse(value.as_str()?).ok_or(rusqlite::types::FromSqlError::InvalidType)
            }
        }
    };
}

word_enum!(Stage {
    New => "new",
    Interested => "interested",
    Applied => "applied",
    Replied => "replied",
    HrInterview => "hr_interview",
    TechStage => "tech_stage",
    JobOffer => "job_offer",
    Rejected => "rejected",
    Withdrawn => "withdrawn",
});

word_enum!(Interest { High => "high", Maybe => "maybe", No => "no" });

word_enum!(Remote { Yes => "yes", Hybrid => "hybrid", No => "no", Unknown => "unknown" });

word_enum!(SalaryPeriod { Month => "month", Hour => "hour" });

word_enum!(FilterResult { Pass => "pass", Fail => "fail", Unsure => "unsure" });

word_enum!(View { Board => "board", Review => "review", Rejected => "rejected", Declined => "declined", Trash => "trash" });

word_enum!(Contract { Uop => "uop", B2b => "b2b", Mandate => "mandate" });

word_enum!(Level { Junior => "junior", Mid => "mid", Senior => "senior" });

word_enum!(OfferLanguage { Pl => "pl", En => "en" });

word_enum!(PayBasis { Gross => "gross", Net => "net" });

// `fail` and `unsure` decide the filter result; `note` is only shown (e.g. "no salary range").
word_enum!(ReasonKind { Fail => "fail", Unsure => "unsure", Note => "note" });

/// Why the hard filter gave its result. The UI translates `code`; `value` is a detail
/// taken from the offer or the profile (e.g. the matched phrase).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Reason {
    pub kind: ReasonKind,
    pub code: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct CityRule {
    pub name: String,
    /// 0 = only this city.
    pub radius_km: u16,
}

impl Default for CityRule {
    fn default() -> Self {
        Self { name: String::new(), radius_km: 0 }
    }
}

/// Salary expectation for one kind of contract.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SalaryRule {
    /// Hard filter: offers that cannot reach this amount are rejected.
    pub min: Option<i64>,
    /// Only for the match score (AI) - never rejects.
    pub expected: Option<i64>,
    pub basis: PayBasis,
    pub period: SalaryPeriod,
}

impl Default for SalaryRule {
    fn default() -> Self {
        Self { min: None, expected: None, basis: PayBasis::Gross, period: SalaryPeriod::Month }
    }
}

/// Which criteria are hard filters; the rest is left to the match score (AI).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct HardFlags {
    pub location: bool,
    pub contract: bool,
    pub salary: bool,
    pub level: bool,
    pub language: bool,
    pub companies: bool,
    pub exclude_words: bool,
    pub keywords: bool,
}

impl Default for HardFlags {
    fn default() -> Self {
        Self {
            location: true,
            contract: true,
            salary: true,
            level: true,
            language: true,
            companies: true,
            exclude_words: true,
            keywords: false,
        }
    }
}

/// Criteria of one profile. An empty list means "no requirement".
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Criteria {
    pub keywords: Vec<String>,
    /// Accepted forms of work: `yes` (remote), `hybrid`, `no` (office).
    pub work_modes: Vec<Remote>,
    pub cities: Vec<CityRule>,
    /// Trips to an office outside my cities: `None` = no, `Some(n)` = up to n a month.
    pub commute_per_month: Option<u8>,
    pub contracts: Vec<Contract>,
    pub levels: Vec<Level>,
    pub languages: Vec<OfferLanguage>,
    /// Employment contract and contract of mandate (amounts usually gross).
    pub salary_uop: SalaryRule,
    /// B2B (amounts usually net, without VAT).
    pub salary_b2b: SalaryRule,
    pub skip_companies: Vec<String>,
    pub exclude_words: Vec<String>,
    pub hard: HardFlags,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Profile {
    pub id: i64,
    pub name: String,
    pub description: String,
    pub criteria: Criteria,
    pub position: i64,
    /// The description before the last change - "Tidy up" can be undone.
    pub description_prev: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileInput {
    /// `None` = new profile.
    pub id: Option<i64>,
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub criteria: Criteria,
}

/// Filter result of one offer for one profile (shown on the offer card).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileFilter {
    pub profile_id: i64,
    pub profile_name: String,
    pub result: FilterResult,
    pub reasons: Vec<Reason>,
}

/// Number of offers in every tab (for the chosen profile or all of them).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ViewCounts {
    pub board: i64,
    pub review: i64,
    pub rejected: i64,
    /// Offers the user marked "no" - out of every other tab until the interest changes.
    pub declined: i64,
    pub trash: i64,
    /// Offers on the board hidden by "hide offers scored below X".
    pub hidden: i64,
}

/// One line on the board.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OfferRow {
    pub id: i64,
    pub title: String,
    pub company: String,
    pub city: Option<String>,
    pub remote: Remote,
    pub salary_min: Option<i64>,
    pub salary_max: Option<i64>,
    pub salary_currency: Option<String>,
    pub salary_period: Option<SalaryPeriod>,
    pub stage: Stage,
    pub stage_changed_at: String,
    pub interest: Option<Interest>,
    pub favorite: bool,
    pub score: Option<i64>,
    pub score_source: Option<String>,
    pub sources: Vec<String>,
    pub expired: bool,
    pub filter_result: FilterResult,
    /// Reasons behind `filter_result` (for the chosen profile, or the best one).
    pub filter_reasons: Vec<Reason>,
    pub added_at: String,
    /// The nearest date (interview, deadline) from 3 hours ago on - for "INTERVIEW TODAY".
    pub next_event: Option<String>,
    pub next_event_kind: Option<String>,
    /// Language of the AI translation ('pl' / 'en') - shown only when it is the app language.
    pub translation_lang: Option<String>,
    pub title_translated: Option<String>,
}

/// One offer for "Export to Excel (CSV)" - raw values; the interface turns them into words of
/// the app language and sends the cells back to be saved.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportRow {
    pub id: i64,
    pub title: String,
    pub company: String,
    pub city: Option<String>,
    pub remote: Remote,
    pub salary_min: Option<i64>,
    pub salary_max: Option<i64>,
    pub salary_currency: Option<String>,
    pub salary_period: Option<SalaryPeriod>,
    pub contract: Option<String>,
    pub level: Option<String>,
    pub stage: Stage,
    pub interest: Option<Interest>,
    pub favorite: bool,
    pub score: Option<i64>,
    pub score_source: Option<String>,
    pub sources: Vec<String>,
    pub url: Option<String>,
    pub added_at: String,
    pub notes: Option<String>,
    /// The tab the offer is in (all profiles together, like the board without a chosen profile).
    pub list: View,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceLink {
    pub source: String,
    pub url: Option<String>,
    pub first_seen: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StageEvent {
    pub stage: Stage,
    pub at: String,
}

/// Everything shown on the offer card.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OfferDetail {
    #[serde(flatten)]
    pub row: OfferRow,
    pub url: Option<String>,
    pub contract: Option<String>,
    pub level: Option<String>,
    pub language: Option<String>,
    pub requirements: Option<String>,
    pub description: Option<String>,
    pub notes: String,
    pub expires_at: Option<String>,
    pub deleted_at: Option<String>,
    /// The source whose text was removed when its job closed (its terms), e.g. "himalayas".
    pub content_removed: Option<String>,
    pub links: Vec<SourceLink>,
    pub history: Vec<StageEvent>,
    /// Hard filter result for every profile.
    pub filters: Vec<ProfileFilter>,
    /// What AI read from the offer and the match score for every profile.
    pub ai: crate::ai::OfferAi,
    /// Dates, contacts and contact history.
    pub agenda: crate::agenda::Agenda,
}

/// A new offer - typed in by hand now, later also from e-mail alerts and aggregators.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OfferInput {
    pub title: String,
    pub company: String,
    pub source: String,
    pub url: Option<String>,
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
    pub expires_at: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(tag = "status", rename_all = "camelCase")]
pub enum AddOutcome {
    /// New offer on the board.
    Added { id: i64 },
    /// Same offer already known - new source and missing fields merged into it.
    Merged { id: i64 },
    /// Same offer from the same source with nothing new - the row is left untouched.
    Unchanged { id: i64 },
    /// The user deleted this offer before - it does not come back.
    Blocked,
}

/// What the work in the background is doing now - the line at the bottom of the window.
/// `step` = a source ("adzuna", "himalayas" ...), "wait" (a pause the source asks for),
/// "check" (are Himalayas offers still open) or "ai" (AI reads offers).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    pub step: String,
    /// The source the step belongs to (for "wait" and "check").
    pub source: Option<String>,
    pub done: usize,
    pub total: usize,
}

impl Progress {
    pub fn new(step: &str, source: Option<&str>, done: usize, total: usize) -> Self {
        Self { step: step.into(), source: source.map(Into::into), done, total }
    }
}

/// Called with every new step, and with `None` when the work is over.
pub type OnProgress<'a> = &'a (dyn Fn(Option<Progress>) + Sync);
