/// Report produced by `rtk discover`.
#[derive(Debug, Default)]
pub struct RtkDiscoverReport {
    pub sessions_scanned: u64,
    pub total_commands: u64,
    pub since_days: u32,
    pub supported: Vec<RtkSupportedEntry>,
    pub unsupported: Vec<RtkUnsupportedEntry>,
}

/// A command that RTK can rewrite, with savings statistics.
#[derive(Debug, Default)]
pub struct RtkSupportedEntry {
    pub command: String,
    pub count: u64,
    pub rtk_equivalent: String,
    pub category: String,
    pub est_savings_tokens: u64,
    pub est_savings_pct: f64,
}

/// A command that RTK does not yet support rewriting.
#[derive(Debug, Default)]
pub struct RtkUnsupportedEntry {
    pub base_command: String,
    pub count: u64,
    pub example: String,
}

/// Aggregate token-savings report from `rtk gain`.
#[derive(Debug, Default)]
pub struct RtkGainReport {
    pub total_commands: u64,
    pub tokens_saved: u64,
    pub savings_pct: f64,
    pub by_command: Vec<RtkGainEntry>,
}

/// Per-command savings breakdown within a [`RtkGainReport`].
#[derive(Debug, Default)]
pub struct RtkGainEntry {
    pub command: String,
    pub count: u64,
    pub tokens_saved: u64,
    pub avg_savings_pct: f64,
}

/// Per-session RTK adoption statistics.
#[derive(Debug, Default)]
pub struct RtkSessionEntry {
    pub id: String,
    pub commands: u64,
    pub rtk_commands: u64,
    pub adoption_pct: f64,
    pub output_bytes: u64,
}

/// Result of `rtk verify`.
#[derive(Debug, Default)]
pub struct RtkVerifyResult {
    pub hook_installed: bool,
    pub tests_passed: u32,
    pub tests_total: u32,
}

/// Audit of rewrite events recorded by the RTK hook.
#[derive(Debug, Default)]
pub struct RtkHookAudit {
    pub rewrites: Vec<RtkAuditEntry>,
}

/// A single rewrite event captured by the RTK hook.
#[derive(Debug, Default)]
pub struct RtkAuditEntry {
    pub original: String,
    pub rewritten: String,
    pub tokens_saved: u64,
}

/// Result of probing a single command through the RTK rewrite engine.
#[derive(Debug, Default)]
pub struct RtkProbeResult {
    pub original: String,
    pub rewritten: Option<String>,
    pub supported: bool,
    pub rtk_equivalent: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rtk_discover_report_defaults() {
        let report = RtkDiscoverReport::default();
        assert_eq!(report.sessions_scanned, 0);
        assert_eq!(report.total_commands, 0);
        assert_eq!(report.since_days, 0);
        assert!(report.supported.is_empty());
        assert!(report.unsupported.is_empty());
    }

    #[test]
    fn rtk_gain_report_defaults() {
        let report = RtkGainReport::default();
        assert_eq!(report.total_commands, 0);
        assert_eq!(report.tokens_saved, 0);
        assert_eq!(report.savings_pct, 0.0);
        assert!(report.by_command.is_empty());
    }

    #[test]
    fn rtk_verify_result_defaults() {
        let result = RtkVerifyResult::default();
        assert!(!result.hook_installed);
        assert_eq!(result.tests_passed, 0);
        assert_eq!(result.tests_total, 0);
    }

    #[test]
    fn rtk_probe_result_defaults() {
        let result = RtkProbeResult::default();
        assert_eq!(result.original, "");
        assert!(result.rewritten.is_none());
        assert!(!result.supported);
        assert!(result.rtk_equivalent.is_none());
    }

    #[test]
    fn rtk_hook_audit_defaults_empty() {
        let audit = RtkHookAudit::default();
        assert!(audit.rewrites.is_empty());
    }

    #[test]
    fn rtk_supported_entry_holds_fields() {
        let entry = RtkSupportedEntry {
            command: "cat".into(),
            count: 5,
            rtk_equivalent: "bat".into(),
            category: "read".into(),
            est_savings_tokens: 200,
            est_savings_pct: 0.9,
        };
        assert_eq!(entry.command, "cat");
        assert_eq!(entry.count, 5);
        assert_eq!(entry.est_savings_pct, 0.9);
    }
}
