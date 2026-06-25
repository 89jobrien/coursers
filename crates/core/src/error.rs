/// Unified error type for `crs-core`.
///
/// All external error types are wrapped here so callers never need to depend on
/// `serde_json`, `toml`, `redb`, or `regex` just to match on an error variant.
#[derive(Debug)]
pub enum CourserError {
    /// Wraps [`std::io::Error`].
    Io(std::io::Error),
    /// Wraps [`serde_json::Error`].
    Json(serde_json::Error),
    /// Wraps [`toml::de::Error`].
    Toml(toml::de::Error),
    /// Wraps [`redb::DatabaseError`].
    Database(redb::DatabaseError),
    /// Wraps [`redb::CommitError`].
    DatabaseCommit(redb::CommitError),
    /// Wraps [`redb::StorageError`].
    DatabaseStorage(redb::StorageError),
    /// Wraps [`redb::TableError`].
    DatabaseTable(redb::TableError),
    /// Wraps [`regex::Error`].
    Regex(regex::Error),
}

impl std::fmt::Display for CourserError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CourserError::Io(e) => write!(f, "I/O error: {e}"),
            CourserError::Json(e) => write!(f, "JSON error: {e}"),
            CourserError::Toml(e) => write!(f, "TOML error: {e}"),
            CourserError::Database(e) => write!(f, "database error: {e}"),
            CourserError::DatabaseCommit(e) => write!(f, "database commit error: {e}"),
            CourserError::DatabaseStorage(e) => write!(f, "database storage error: {e}"),
            CourserError::DatabaseTable(e) => write!(f, "database table error: {e}"),
            CourserError::Regex(e) => write!(f, "regex error: {e}"),
        }
    }
}

impl std::error::Error for CourserError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            CourserError::Io(e) => Some(e),
            CourserError::Json(e) => Some(e),
            CourserError::Toml(e) => Some(e),
            CourserError::Database(e) => Some(e),
            CourserError::DatabaseCommit(e) => Some(e),
            CourserError::DatabaseStorage(e) => Some(e),
            CourserError::DatabaseTable(e) => Some(e),
            CourserError::Regex(e) => Some(e),
        }
    }
}

impl From<std::io::Error> for CourserError {
    fn from(e: std::io::Error) -> Self {
        CourserError::Io(e)
    }
}

impl From<serde_json::Error> for CourserError {
    fn from(e: serde_json::Error) -> Self {
        CourserError::Json(e)
    }
}

impl From<toml::de::Error> for CourserError {
    fn from(e: toml::de::Error) -> Self {
        CourserError::Toml(e)
    }
}

impl From<redb::DatabaseError> for CourserError {
    fn from(e: redb::DatabaseError) -> Self {
        CourserError::Database(e)
    }
}

impl From<redb::CommitError> for CourserError {
    fn from(e: redb::CommitError) -> Self {
        CourserError::DatabaseCommit(e)
    }
}

impl From<redb::StorageError> for CourserError {
    fn from(e: redb::StorageError) -> Self {
        CourserError::DatabaseStorage(e)
    }
}

impl From<redb::TableError> for CourserError {
    fn from(e: redb::TableError) -> Self {
        CourserError::DatabaseTable(e)
    }
}

impl From<regex::Error> for CourserError {
    fn from(e: regex::Error) -> Self {
        CourserError::Regex(e)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_io_error_roundtrip() {
        let io = std::io::Error::new(std::io::ErrorKind::NotFound, "file missing");
        let err: CourserError = io.into();
        assert!(matches!(err, CourserError::Io(_)));
        assert!(!err.to_string().is_empty());
    }

    #[test]
    fn from_json_error_display() {
        let json_err = serde_json::from_str::<serde_json::Value>("not json").unwrap_err();
        let err: CourserError = json_err.into();
        assert!(matches!(err, CourserError::Json(_)));
        assert!(!err.to_string().is_empty());
    }

    #[test]
    fn from_toml_error_display() {
        let toml_err = toml::from_str::<toml::Value>("= invalid").unwrap_err();
        let err: CourserError = toml_err.into();
        assert!(matches!(err, CourserError::Toml(_)));
        assert!(!err.to_string().is_empty());
    }

    #[test]
    fn from_regex_error_display() {
        let regex_err = regex::Regex::new("[unclosed").unwrap_err();
        let err: CourserError = regex_err.into();
        assert!(matches!(err, CourserError::Regex(_)));
        assert!(!err.to_string().is_empty());
    }

    #[test]
    fn display_outputs_are_non_empty() {
        let s1 = CourserError::Io(std::io::Error::new(std::io::ErrorKind::Other, "x")).to_string();
        let s2 = CourserError::Json(serde_json::from_str::<()>("!").unwrap_err()).to_string();
        assert!(!s1.is_empty());
        assert!(!s2.is_empty());
    }
}
