//! Miette diagnostics for user-facing rule denies.
//!
//! Separate from [`crate::error::CourserError`] (internal/IO error paths):
//! `RuleViolation` renders a *deny*, not a failure — the source code is the
//! blocked shell command, and the label (when a byte-range match is
//! available) points at the exact substring that tripped the rule.

use miette::{Diagnostic, SourceSpan};
use thiserror::Error;

#[derive(Debug, Error, Diagnostic)]
#[error("{message}")]
#[diagnostic(code(coursers::rule_violation))]
pub struct RuleViolation {
    #[source_code]
    pub command: String,
    #[label("blocked here")]
    pub span: Option<SourceSpan>,
    pub message: String,
}

impl RuleViolation {
    pub fn new(command: &str, message: &str, span: Option<std::ops::Range<usize>>) -> Self {
        Self {
            command: command.to_string(),
            span: span.map(|r| (r.start, r.len()).into()),
            message: message.to_string(),
        }
    }

    /// Render this diagnostic to a plain string (colorless, ASCII-only — safe
    /// for embedding in a deny-reason payload that isn't a live terminal,
    /// e.g. JSON sent back over the Claude Code hook protocol).
    pub fn render(&self) -> String {
        let mut out = String::new();
        let handler = miette::GraphicalReportHandler::new_themed(miette::GraphicalTheme::none());
        handler
            .render_report(&mut out, self)
            .unwrap_or_else(|_| out.push_str(&self.message));
        out
    }

    /// Render with miette's default graphical handler — unicode box-drawing
    /// and ANSI color when the output stream supports it. Use this for
    /// terminal-facing previews (`crs probe`); use [`Self::render`] for
    /// anything embedded in a non-terminal payload.
    pub fn render_fancy(&self) -> String {
        let mut out = String::new();
        let handler = miette::GraphicalReportHandler::new();
        handler
            .render_report(&mut out, self)
            .unwrap_or_else(|_| out.push_str(&self.message));
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_includes_message_and_command() {
        let v = RuleViolation::new("grep foo .", "Blocked by rule 'no-grep'.", Some(0..4));
        let rendered = v.render();
        assert!(rendered.contains("Blocked by rule 'no-grep'."));
        assert!(rendered.contains("grep foo ."));
    }

    #[test]
    fn render_without_span_still_works() {
        let v = RuleViolation::new("nvm use 20", "Blocked by rule 'no-nvm'.", None);
        let rendered = v.render();
        assert!(rendered.contains("Blocked by rule 'no-nvm'."));
    }
}
