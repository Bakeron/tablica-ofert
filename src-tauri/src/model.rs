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

word_enum!(View {
    Own => "own",
    // "Not for me", expired (the archive) and deleted together - one bin.
    Bin => "bin",
    Today => "today",
    Board => "board",
    Review => "review",
    Rejected => "rejected",
    Declined => "declined",
    Archive => "archive",
    Trash => "trash",
});

// Why an offer is in the bin: "not for me", expired (archived) or deleted.
word_enum!(BinKind { Declined => "declined", Expired => "expired", Deleted => "deleted" });

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

/// A job the AI thinks fits what the person can do - shown with its reason, added only by the person.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CareerIdea {
    /// Job title as offers name it - goes to the keywords as it is.
    pub name: String,
    /// Why it fits, in the app's language.
    pub why: String,
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
    /// Own offers the CV is still to be sent to ("new" or "interested") - the number on "Own".
    pub to_send: i64,
    pub today: i64,
    pub board: i64,
    pub review: i64,
    pub rejected: i64,
    /// Offers the user marked "no" - out of every other tab until the interest changes.
    pub declined: i64,
    /// Expired offers (the archive) - "Expired" in the bin.
    pub archive: i64,
    pub trash: i64,
    /// Everything in the bin: "not for me", expired and deleted.
    pub bin: i64,
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
    /// When the offer was published (the source's date, the alert mail's date, or when it was
    /// added); seen again in a source = the newer date.
    pub published_at: String,
    /// Older than the days in the settings and still here (the user worked on it) - "probably expired".
    pub stale: bool,
    /// Applied and further - never greyed out or pushed down, only a small mark.
    pub in_recruitment: bool,
    /// Added by hand - always at the top of "Today".
    pub manual: bool,
    /// Set = in the archive.
    pub archived_at: Option<String>,
    /// One sentence from AI: why this score (the best profile).
    pub score_why: Option<String>,
    /// What the "Very" / "No" ratings add to `score` when shown; `score` itself stays as it is.
    pub learned: i64,
    /// The words behind `learned`.
    pub learned_words: Vec<String>,
    /// No link to the offer anywhere - "NO LINK" on the list, below the ones with a link.
    pub no_link: bool,
    /// The person's own offer (tab "Own"): added by hand, moved there, or past the stage "new".
    pub own: bool,
    /// In the bin - and why; `None` = not there.
    pub bin: Option<BinKind>,
    /// The ad was renewed after the CV went - the mark RENEWED and the question on the card.
    pub renew_asked: bool,
    /// Set = a child: another ad of the job of that offer (the tree of duplicates).
    pub parent_id: Option<i64>,
    /// The same company and title as another offer, not joined (another city or salary) - the
    /// mark "SAME?" and the question on the card.
    pub dup_ask: bool,
    /// When the CV went (the first stage past "interested"); `None` = not in a recruitment. On a
    /// child it gives the parent's "sent through: OLX, 5.11".
    pub sent_at: Option<String>,
    /// On a parent: the CV went through one of its children - "sent through: OLX, 5.11".
    pub sent_via: Option<SentVia>,
    /// The other ads of this job, the first seen on top - shown under it on the list.
    pub children: Vec<OfferRow>,
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
    pub published_at: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceLink {
    pub source: String,
    pub url: Option<String>,
    pub first_seen: String,
    /// The city this link gave - one job in two cities keeps both links.
    pub city: Option<String>,
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
    /// When "Update the existing one" last brought the ad again (the history: "ad renewed").
    pub renewed_at: Option<String>,
    pub links: Vec<SourceLink>,
    pub history: Vec<StageEvent>,
    /// Hard filter result for every profile.
    pub filters: Vec<ProfileFilter>,
    /// What AI read from the offer and the match score for every profile.
    pub ai: crate::ai::OfferAi,
    /// Dates, contacts and contact history.
    pub agenda: crate::agenda::Agenda,
    /// "The same company and title as that offer - the same ad?" (`dups`).
    pub dup: Option<crate::dups::DupNote>,
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
    /// When the source published it (any common date format; a wrong one is dropped, a date in
    /// the future becomes now). `None` = now.
    pub published_at: Option<String>,
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

word_enum!(SameKind {
    // Nothing like it from this portal - a plain save.
    None => "none",
    // The same link, the same fields and text - nothing new is saved.
    Identical => "identical",
    // The same link with other fields (a newer date, other salary...) - "Update the existing one".
    Changed => "changed",
    // A new link of the same portal, the first ad gone, the text almost the same - the person
    // says whether it is the same offer.
    Similar => "similar",
    // A new link of the same portal while the first ad is still open - a second ad.
    Second => "second",
    // The same link, only empty fields filled in and the full text (read before in part, e.g.
    // from an alert) - "Fill in".
    Partial => "partial",
    // Another portal, the same company and title, but another city or clearly other salary - the
    // person says whether it is one job.
    Ask => "ask",
    // "Probably the same" offers (another title, a hidden company) - `candidates`.
    Probable => "probable",
    // "Fill in that one": what the form would change in the chosen offer.
    Complete => "complete",
    // Another portal, joined as a child by itself, but the CV already went for this job - said
    // before saving.
    Joined => "joined",
});

/// The offer already on the board, as "Add offer" compares it with the form.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SameWas {
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
    pub published_at: Option<String>,
    pub expires_at: Option<String>,
    pub own: bool,
    pub stage: Stage,
    /// When the CV went (the first "applied" of the history) - `None` before a recruitment.
    pub applied_at: Option<String>,
    /// The CV already went for this job through some ad of its family (the offer, its parent or a
    /// child) - the warning "you sent your CV on 5.11 through pracuj.pl".
    pub sent_via: Option<SentVia>,
}

/// Through which ad of a family the CV went: the ad, its portal (the link's, else the source) and when.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SentVia {
    pub id: i64,
    pub source: String,
    pub at: String,
}

/// What "Add offer" finds before saving (`same::check`).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SameOffer {
    pub kind: SameKind,
    pub was: Option<SameWas>,
    /// Fields of the form that differ from the offer on the board ("title", "salary",
    /// "description" ...); only fields the form fills.
    pub fields: Vec<String>,
    /// Of `fields`, the ones the offer on the board does not have yet - filled in, nothing to choose.
    pub filled: Vec<String>,
    /// How much of the description is the same, in % (without "1 hour ago" and the like).
    pub similarity: u8,
    /// "Probably the same" offers the code found (`Probable`), the closest first.
    pub candidates: Vec<crate::dups::DupCandidate>,
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
    /// While offers are fetched: which of the turned-on sources is asked now (1-based) and how
    /// many there are - "Source 3 of 7" on the left of the line. 0 = not part of a fetch.
    pub part: usize,
    pub parts: usize,
}

impl Progress {
    pub fn new(step: &str, source: Option<&str>, done: usize, total: usize) -> Self {
        Self { step: step.into(), source: source.map(Into::into), done, total, part: 0, parts: 0 }
    }

    /// The same step as source `part` of `parts` in a fetch.
    pub fn of(self, part: usize, parts: usize) -> Self {
        Self { part, parts, ..self }
    }
}

/// Called with every new step, and with `None` when the work is over.
pub type OnProgress<'a> = &'a (dyn Fn(Option<Progress>) + Sync);
