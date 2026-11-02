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

word_enum!(View { Board => "board", Rejected => "rejected", Trash => "trash" });

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
    pub added_at: String,
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
    pub filter_reason: Option<String>,
    pub expires_at: Option<String>,
    pub deleted_at: Option<String>,
    pub links: Vec<SourceLink>,
    pub history: Vec<StageEvent>,
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
    /// The user deleted this offer before - it does not come back.
    Blocked,
}
