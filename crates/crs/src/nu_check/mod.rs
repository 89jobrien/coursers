//! Nu script validation via `nu --ide-check`.
//!
//! `nu-parser`/`nu-protocol`/`nu-engine` 0.111–0.113 have a compile-time bug
//! (`PipelineExecutionData` missing `exit` field in eval_ir.rs). We delegate
//! to `nu --ide-check <N> <file>` which emits newline-delimited JSON diagnostics
//! and is available on any system where `nu` is installed.
//!
//! Diagnostic JSON shape (one per line):
//! ```json
//! {"type":"diagnostic","severity":"Error","message":"...","span":{"start":N,"end":N}}
//! ```
//! Spans are byte offsets into the source. We convert to 1-based line/col.

use serde::Deserialize;
use std::path::{Path, PathBuf};
use std::process::Command;

/// A single parse error from a nu script.
#[derive(Debug, PartialEq)]
pub struct NuError {
    pub file: PathBuf,
    pub line: usize,
    pub col: usize,
    pub message: String,
}

impl NuError {
    /// Format as `file:line:col: message` for terminal output.
    pub fn display(&self) -> String {
        format!(
            "{}:{}:{}: {}",
            self.file.display(),
            self.line,
            self.col,
            self.message
        )
    }
}

/// Outcome of checking one or more nu scripts.
#[derive(Debug)]
pub struct CheckResult {
    pub errors: Vec<NuError>,
}

impl CheckResult {
    /// Returns `true` when no errors were found.
    pub fn is_ok(&self) -> bool {
        self.errors.is_empty()
    }
}

#[derive(Debug, Deserialize)]
struct Diagnostic {
    severity: String,
    message: String,
    span: DiagSpan,
}

#[derive(Debug, Deserialize)]
struct DiagSpan {
    start: usize,
}

/// Check a single file. Returns `Err` on I/O or subprocess failure.
pub fn check_file(path: &Path) -> std::io::Result<CheckResult> {
    let source = std::fs::read_to_string(path)?;
    // --ide-check <N> limits errors to N; use a large number to get all errors.
    let output = Command::new("nu")
        .args(["--ide-check", "1000", "--no-config-file"])
        .arg(path)
        .output()?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let errors = parse_diagnostics(&stdout, path, &source);
    Ok(CheckResult { errors })
}

/// Check multiple files, collecting all errors.
/// I/O / subprocess failures become `NuError` entries at line 0.
pub fn check_files(paths: &[PathBuf]) -> CheckResult {
    let mut errors = Vec::new();
    for path in paths {
        match check_file(path) {
            Ok(r) => errors.extend(r.errors),
            Err(e) => errors.push(NuError {
                file: path.clone(),
                line: 0,
                col: 0,
                message: format!("io error: {e}"),
            }),
        }
    }
    CheckResult { errors }
}

/// Parse newline-delimited JSON diagnostics from `nu --ide-check` stdout.
fn parse_diagnostics(output: &str, file: &Path, source: &str) -> Vec<NuError> {
    output
        .lines()
        .filter_map(|line| serde_json::from_str::<Diagnostic>(line).ok())
        .filter(|d| d.severity.eq_ignore_ascii_case("error"))
        .map(|d| {
            let (line, col) = byte_offset_to_line_col(source, d.span.start);
            NuError {
                file: file.to_path_buf(),
                line,
                col,
                message: d.message,
            }
        })
        .collect()
}

/// Convert a byte offset to 1-based (line, col).
fn byte_offset_to_line_col(source: &str, offset: usize) -> (usize, usize) {
    let clamped = offset.min(source.len());
    let before = &source[..clamped];
    // Count newlines directly — str::lines() does not count a trailing '\n'
    // as opening a new line, so it gives the wrong answer for offsets right
    // after a newline character.
    let line = before.bytes().filter(|&b| b == b'\n').count() + 1;
    let col = match before.rfind('\n') {
        Some(n) => clamped - n,
        None => clamped + 1,
    };
    (line, col)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    // ── unit: byte_offset_to_line_col ──────────────────────────────────────

    #[test]
    fn offset_zero_is_line1_col1() {
        assert_eq!(byte_offset_to_line_col("hello", 0), (1, 1));
    }

    #[test]
    fn offset_past_first_newline_is_line2() {
        let src = "line1\nline2";
        //         01234 5
        assert_eq!(byte_offset_to_line_col(src, 6), (2, 1));
    }

    #[test]
    fn offset_beyond_source_clamps() {
        let src = "hi";
        let (line, _col) = byte_offset_to_line_col(src, 999);
        assert_eq!(line, 1);
    }

    // ── unit: parse_diagnostics ────────────────────────────────────────────

    #[test]
    fn parse_diagnostics_extracts_errors() {
        let json = r#"{"type":"diagnostic","severity":"Error","message":"Unexpected end of code.","span":{"start":11,"end":12}}"#;
        let diags = parse_diagnostics(json, Path::new("x.nu"), "def broken {");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].message, "Unexpected end of code.");
        assert_eq!(diags[0].file, Path::new("x.nu"));
    }

    #[test]
    fn parse_diagnostics_ignores_hints_and_warnings() {
        let json = concat!(
            r#"{"type":"diagnostic","severity":"Hint","message":"hint msg","span":{"start":0,"end":1}}"#,
            "\n",
            r#"{"type":"diagnostic","severity":"Warning","message":"warn msg","span":{"start":0,"end":1}}"#,
        );
        let diags = parse_diagnostics(json, Path::new("x.nu"), "");
        assert!(diags.is_empty(), "expected no errors, got: {diags:?}");
    }

    #[test]
    fn parse_diagnostics_skips_malformed_lines() {
        let json = "not json\n{\"incomplete\":\n";
        let diags = parse_diagnostics(json, Path::new("x.nu"), "");
        assert!(diags.is_empty());
    }

    // ── integration: check_file via real `nu` ──────────────────────────────
    // These tests require `nu` on PATH.

    #[test]
    fn t1_empty_source_returns_ok() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("empty.nu");
        std::fs::write(&path, "").unwrap();
        let result = check_file(&path).unwrap();
        assert!(result.is_ok(), "errors: {:?}", result.errors);
    }

    #[test]
    fn t2_valid_def_returns_ok() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("valid.nu");
        std::fs::write(
            &path,
            r#"export def greet [name: string] { $"hello ($name)" }"#,
        )
        .unwrap();
        let result = check_file(&path).unwrap();
        assert!(result.is_ok(), "errors: {:?}", result.errors);
    }

    #[test]
    fn t3_syntax_error_returns_errors() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bad.nu");
        std::fs::write(&path, "def broken {").unwrap();
        let result = check_file(&path).unwrap();
        assert!(!result.is_ok());
        assert!(!result.errors.is_empty());
    }

    #[test]
    fn t4_error_includes_file_path() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("script.nu");
        std::fs::write(&path, "def broken {").unwrap();
        let result = check_file(&path).unwrap();
        assert!(!result.errors.is_empty());
        assert_eq!(result.errors[0].file, path);
    }

    #[test]
    fn t5_multiple_files_aggregate_errors() {
        let dir = tempfile::tempdir().unwrap();
        let good = dir.path().join("good.nu");
        std::fs::write(&good, "def ok [] {}").unwrap();
        let bad = dir.path().join("bad.nu");
        std::fs::write(&bad, "def broken {").unwrap();

        let result = check_files(&[good.clone(), bad.clone()]);
        assert!(result.errors.iter().all(|e| e.file != good));
        assert!(result.errors.iter().any(|e| e.file == bad));
    }

    #[test]
    fn t6_missing_file_returns_io_error() {
        let path = PathBuf::from("/nonexistent/path/file.nu");
        let result = check_files(std::slice::from_ref(&path));
        assert!(!result.is_ok());
        let e = &result.errors[0];
        assert_eq!(e.file, path);
        assert!(e.message.starts_with("io error:"), "got: {}", e.message);
    }
}
