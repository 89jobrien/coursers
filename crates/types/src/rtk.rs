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
