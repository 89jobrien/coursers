/// Unified error type for `crs-core`.
///
/// All external error types are wrapped here so callers never need to depend on
/// `serde_json`, `toml`, `redb`, or `regex` just to match on an error variant.
#[derive(Debug, thiserror::Error, miette::Diagnostic)]
pub enum CourserError {
    /// Wraps [`std::io::Error`].
    #[error("I/O error: {0}")]
    #[diagnostic(
        code(coursers::io),
        help("check that the path exists and is readable/writable")
    )]
    Io(std::io::Error),
    /// Wraps [`serde_json::Error`].
    #[error("JSON error: {0}")]
    #[diagnostic(code(coursers::json), help("check the JSON file for syntax errors"))]
    Json(serde_json::Error),
    /// Wraps [`toml::de::Error`].
    #[error("TOML error: {0}")]
    #[diagnostic(code(coursers::toml), help("check the TOML file for syntax errors"))]
    Toml(toml::de::Error),
    /// Wraps [`redb::DatabaseError`].
    #[error("database error: {0}")]
    #[diagnostic(
        code(coursers::database),
        help("the redb database file may be corrupt or locked by another process")
    )]
    Database(redb::DatabaseError),
    /// Wraps [`redb::CommitError`].
    #[error("database commit error: {0}")]
    #[diagnostic(code(coursers::database_commit), help("retry the operation"))]
    DatabaseCommit(redb::CommitError),
    /// Wraps [`redb::StorageError`].
    #[error("database storage error: {0}")]
    #[diagnostic(
        code(coursers::database_storage),
        help("check available disk space and file permissions")
    )]
    DatabaseStorage(redb::StorageError),
    /// Wraps [`redb::TableError`].
    #[error("database table error: {0}")]
    #[diagnostic(
        code(coursers::database_table),
        help("the expected table may be missing from the database")
    )]
    DatabaseTable(redb::TableError),
    /// Wraps [`regex::Error`].
    #[error("regex error: {0}")]
    #[diagnostic(
        code(coursers::regex),
        help("check the rule's pattern for invalid regex syntax")
    )]
    Regex(regex::Error),
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
    #[allow(clippy::invalid_regex)]
    fn from_regex_error_display() {
        let regex_err = regex::Regex::new("[unclosed").unwrap_err();
        let err: CourserError = regex_err.into();
        assert!(matches!(err, CourserError::Regex(_)));
        assert!(!err.to_string().is_empty());
    }

    #[test]
    fn display_outputs_are_non_empty() {
        let s1 = CourserError::Io(std::io::Error::other("x")).to_string();
        let s2 = CourserError::Json(serde_json::from_str::<()>("!").unwrap_err()).to_string();
        assert!(!s1.is_empty());
        assert!(!s2.is_empty());
    }
}
