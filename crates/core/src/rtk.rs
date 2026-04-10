/// Domain types and ports for RTK (Rust Token Killer) integration.
///
/// Two trait groups follow ISP/capability-group B:
///   - `RtkAnalysis`  — read-only: discover, gain, session, audit, verify
///   - `RtkRewrite`   — intercept: rewrite, probe, check, list proxies, flush

// ---------------------------------------------------------------------------
// Domain types — RtkAnalysis
// ---------------------------------------------------------------------------

#[derive(Debug, Default)]
pub struct RtkDiscoverReport {
    pub sessions_scanned: u64,
    pub total_commands: u64,
    pub since_days: u32,
    pub supported: Vec<RtkSupportedEntry>,
    pub unsupported: Vec<RtkUnsupportedEntry>,
}

#[derive(Debug, Default)]
pub struct RtkSupportedEntry {
    pub command: String,
    pub count: u64,
    pub rtk_equivalent: String,
    pub category: String,
    pub est_savings_tokens: u64,
    pub est_savings_pct: f64,
}

#[derive(Debug, Default)]
pub struct RtkUnsupportedEntry {
    pub base_command: String,
    pub count: u64,
    pub example: String,
}

#[derive(Debug, Default)]
pub struct RtkGainReport {
    pub total_commands: u64,
    pub tokens_saved: u64,
    pub savings_pct: f64,
    pub by_command: Vec<RtkGainEntry>,
}

#[derive(Debug, Default)]
pub struct RtkGainEntry {
    pub command: String,
    pub count: u64,
    pub tokens_saved: u64,
    pub avg_savings_pct: f64,
}

#[derive(Debug, Default)]
pub struct RtkSessionEntry {
    pub id: String,
    pub commands: u64,
    pub rtk_commands: u64,
    pub adoption_pct: f64,
    pub output_bytes: u64,
}

#[derive(Debug, Default)]
pub struct RtkVerifyResult {
    pub hook_installed: bool,
    pub tests_passed: u32,
    pub tests_total: u32,
}

#[derive(Debug, Default)]
pub struct RtkHookAudit {
    pub rewrites: Vec<RtkAuditEntry>,
}

#[derive(Debug, Default)]
pub struct RtkAuditEntry {
    pub original: String,
    pub rewritten: String,
    pub tokens_saved: u64,
}

// ---------------------------------------------------------------------------
// Domain types — RtkRewrite
// ---------------------------------------------------------------------------

#[derive(Debug, Default)]
pub struct RtkProbeResult {
    pub original: String,
    pub rewritten: Option<String>,
    pub supported: bool,
    pub rtk_equivalent: Option<String>,
}

// ---------------------------------------------------------------------------
// Ports
// ---------------------------------------------------------------------------

/// Read-only RTK capabilities: discovery, savings analysis, session stats.
pub trait RtkAnalysis {
    fn discover(&self, since_days: u32) -> Option<RtkDiscoverReport>;
    fn gain(&self) -> Option<RtkGainReport>;
    fn session(&self) -> Option<Vec<RtkSessionEntry>>;
    fn verify(&self) -> Option<RtkVerifyResult>;
    fn hook_audit(&self) -> Option<RtkHookAudit>;
    fn version(&self) -> Option<String>;
}

/// Intercept/rewrite RTK capabilities: command rewriting, probing, proxy listing.
pub trait RtkRewrite {
    fn rewrite(&self, command: &str) -> Option<String>;
    fn probe(&self, command: &str) -> Option<RtkProbeResult>;
    fn check(&self, command: &str) -> bool;
    fn list_proxies(&self) -> Vec<String>;
    fn flush(&self) -> bool;
}

// ---------------------------------------------------------------------------
// Null adapter — used when rtk is not on PATH; all methods are no-ops
// ---------------------------------------------------------------------------

pub struct NullRtkClient;

impl RtkAnalysis for NullRtkClient {
    fn discover(&self, _since_days: u32) -> Option<RtkDiscoverReport> { None }
    fn gain(&self) -> Option<RtkGainReport> { None }
    fn session(&self) -> Option<Vec<RtkSessionEntry>> { None }
    fn verify(&self) -> Option<RtkVerifyResult> { None }
    fn hook_audit(&self) -> Option<RtkHookAudit> { None }
    fn version(&self) -> Option<String> { None }
}

impl RtkRewrite for NullRtkClient {
    fn rewrite(&self, _command: &str) -> Option<String> { None }
    fn probe(&self, _command: &str) -> Option<RtkProbeResult> { None }
    fn check(&self, _command: &str) -> bool { false }
    fn list_proxies(&self) -> Vec<String> { vec![] }
    fn flush(&self) -> bool { false }
}
