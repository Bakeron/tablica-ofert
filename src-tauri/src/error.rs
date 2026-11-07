use serde::ser::SerializeStruct;

/// Errors returned to the UI. Serialized as `{ code, detail }` - the UI translates `code`,
/// `detail` is shown only as technical info.
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("database: {0}")]
    Db(#[from] rusqlite::Error),
    #[error("file system: {0}")]
    Io(#[from] std::io::Error),
    #[error("credential store: {0}")]
    Secret(String),
    /// Data could not be read safely - every write is blocked until the cause is fixed.
    #[error("{0}")]
    Locked(String),
    #[error("{0}")]
    Invalid(String),
    #[error("not found")]
    NotFound,
    /// No connection, timeout or an unexpected answer from a service.
    #[error("network: {0}")]
    Net(String),
    /// The service refused the key or login.
    #[error("refused: {0}")]
    Auth(String),
    /// The service's limit is used up (HTTP 429 or our own daily cap).
    #[error("limit: {0}")]
    Limit(String),
    /// A required key or login is not set in the settings.
    #[error("missing: {0}")]
    Missing(String),
    /// The same long task (e.g. fetching offers) is already running.
    #[error("already running")]
    Busy,
    /// A sync file or a copy made by a newer app version - this device needs the update.
    /// The text is also matched in `SyncSettings.tsx` (a past sync keeps only the text).
    #[error("data from a newer app version")]
    Outdated,
    /// An AI provider answered with an error; `kind` says in plain words what is wrong.
    #[error("{status}: {message}")]
    Provider { kind: crate::ai::client::Problem, status: u16, message: String },
}

impl AppError {
    pub fn code(&self) -> &'static str {
        match self {
            AppError::Db(_) => "db",
            AppError::Io(_) => "io",
            AppError::Secret(_) => "secret",
            AppError::Locked(_) => "locked",
            AppError::Invalid(_) => "invalid",
            AppError::NotFound => "not_found",
            AppError::Net(_) => "net",
            AppError::Auth(_) => "auth",
            AppError::Limit(_) => "limit",
            AppError::Missing(_) => "missing",
            AppError::Busy => "busy",
            AppError::Outdated => "outdated",
            AppError::Provider { kind, .. } => {
                use crate::ai::client::Problem;
                match kind {
                    Problem::Key | Problem::ApiOff | Problem::NoFreeTier | Problem::Verify | Problem::Denied => "auth",
                    Problem::Limit => "limit",
                    _ => "net",
                }
            }
        }
    }
}

impl serde::Serialize for AppError {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut s = serializer.serialize_struct("AppError", 2)?;
        s.serialize_field("code", self.code())?;
        s.serialize_field("detail", &self.to_string())?;
        s.end()
    }
}

pub type AppResult<T> = Result<T, AppError>;
