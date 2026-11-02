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
