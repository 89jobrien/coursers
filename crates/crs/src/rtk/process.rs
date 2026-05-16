/// ProcessRtkClient — shells out to the `rtk` binary.
///
/// All methods fail-open: if rtk is missing, returns None/false/empty.
use std::process::Command;

use crs_core::rtk::{
    RtkAnalysis, RtkAuditEntry, RtkDiscoverReport, RtkGainReport, RtkHookAudit, RtkProbeResult,
    RtkRewrite, RtkSessionEntry, RtkSupportedEntry, RtkUnsupportedEntry, RtkVerifyResult,
};

pub struct ProcessRtkClient;

impl ProcessRtkClient {
    fn run(&self, args: &[&str]) -> Option<String> {
        let out = Command::new("rtk").args(args).output().ok()?;
        stdout_if_success(&out.status, &out.stdout)
    }

    fn run_json(&self, args: &[&str]) -> Option<serde_json::Value> {
        let stdout = self.run(args)?;
        serde_json::from_str(&stdout).ok()
    }
}

// ---------------------------------------------------------------------------
// Pure predicates — no I/O, fully unit-testable
// ---------------------------------------------------------------------------

/// Returns the stdout as a String when the process exited successfully.
pub(crate) fn stdout_if_success(
    status: &std::process::ExitStatus,
    stdout: &[u8],
) -> Option<String> {
    if status.success() {
        Some(String::from_utf8_lossy(stdout).into_owned())
    } else {
        None
    }
}

/// Interprets a raw `rtk rewrite` stdout string.
///
/// Returns `Some(s)` when the trimmed output is non-empty, `None` otherwise.
/// Does not touch any I/O.
pub(crate) fn parse_rewrite_output(raw: &str) -> Option<String> {
    let s = raw.trim().to_owned();
    if s.is_empty() { None } else { Some(s) }
}

/// Parses `rtk verify` text output for the hook-installed predicate.
pub(crate) fn hook_installed_from_verify(stdout: &str) -> bool {
    !stdout.contains("RTK hook not installed")
}

impl RtkAnalysis for ProcessRtkClient {
    fn discover(&self, since_days: u32) -> Option<RtkDiscoverReport> {
        let since = since_days.to_string();
        let v = self.run_json(&["discover", "--format", "json", "--since", &since])?;

        let supported = v["supported"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .map(|e| RtkSupportedEntry {
                        command: str_field(e, "command"),
                        count: u64_field(e, "count"),
                        rtk_equivalent: str_field(e, "rtk_equivalent"),
                        category: str_field(e, "category"),
                        est_savings_tokens: u64_field(e, "estimated_savings_tokens"),
                        est_savings_pct: f64_field(e, "estimated_savings_pct"),
                    })
                    .collect()
            })
            .unwrap_or_default();

        let unsupported = v["unsupported"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .map(|e| RtkUnsupportedEntry {
                        base_command: str_field(e, "base_command"),
                        count: u64_field(e, "count"),
                        example: str_field(e, "example"),
                    })
                    .collect()
            })
            .unwrap_or_default();

        Some(RtkDiscoverReport {
            sessions_scanned: u64_field(&v, "sessions_scanned"),
            total_commands: u64_field(&v, "total_commands"),
            since_days,
            supported,
            unsupported,
        })
    }

    fn gain(&self) -> Option<RtkGainReport> {
        // rtk gain has no --format json — parse text output best-effort
        // Returns None until rtk exposes a machine-readable format.
        None
    }

    fn session(&self) -> Option<Vec<RtkSessionEntry>> {
        // rtk session has no --format json — parse text output best-effort
        // Returns None until rtk exposes a machine-readable format.
        None
    }

    fn verify(&self) -> Option<RtkVerifyResult> {
        let stdout = self.run(&["verify"])?;
        let hook_installed = hook_installed_from_verify(&stdout);
        let tests_total = parse_test_count(&stdout, "tests passed").unwrap_or(0);
        Some(RtkVerifyResult {
            hook_installed,
            tests_passed: tests_total,
            tests_total,
        })
    }

    fn hook_audit(&self) -> Option<RtkHookAudit> {
        let v = self.run_json(&["hook-audit", "--format", "json"])?;
        let rewrites = v["rewrites"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .map(|e| RtkAuditEntry {
                        original: str_field(e, "original"),
                        rewritten: str_field(e, "rewritten"),
                        tokens_saved: u64_field(e, "tokens_saved"),
                    })
                    .collect()
            })
            .unwrap_or_default();
        Some(RtkHookAudit { rewrites })
    }

    fn version(&self) -> Option<String> {
        let out = Command::new("rtk").arg("--version").output().ok()?;
        Some(String::from_utf8_lossy(&out.stdout).trim().to_owned())
    }
}

impl RtkRewrite for ProcessRtkClient {
    fn rewrite(&self, command: &str) -> Option<String> {
        let out = Command::new("rtk")
            .args(["rewrite", command])
            .output()
            .ok()?;
        let raw = stdout_if_success(&out.status, &out.stdout)?;
        parse_rewrite_output(&raw)
    }

    fn probe(&self, command: &str) -> Option<RtkProbeResult> {
        let rewritten = self.rewrite(command);
        let supported = rewritten.is_some();
        Some(RtkProbeResult {
            original: command.to_owned(),
            rewritten: rewritten.clone(),
            supported,
            rtk_equivalent: rewritten,
        })
    }

    fn check(&self, command: &str) -> bool {
        self.rewrite(command).is_some()
    }

    fn list_proxies(&self) -> Vec<String> {
        // `rtk ls` lists directory — not proxy list. Use `rtk --help` parsing instead.
        // Returns empty until rtk exposes a dedicated subcommand.
        vec![]
    }

    fn flush(&self) -> bool {
        // `rtk learn` is the closest; no flush subcommand exists yet.
        false
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn str_field(v: &serde_json::Value, key: &str) -> String {
    v[key].as_str().unwrap_or("").to_owned()
}

fn u64_field(v: &serde_json::Value, key: &str) -> u64 {
    v[key].as_u64().unwrap_or(0)
}

fn f64_field(v: &serde_json::Value, key: &str) -> f64 {
    v[key].as_f64().unwrap_or(0.0)
}

fn parse_test_count(s: &str, suffix: &str) -> Option<u32> {
    s.lines()
        .find(|l| l.contains(suffix))
        .and_then(|l| l.split('/').next())
        .and_then(|part| part.trim().parse().ok())
}

// ---------------------------------------------------------------------------
// Unit tests for pure predicate functions
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_rewrite_output_returns_none_for_empty() {
        assert_eq!(parse_rewrite_output(""), None);
        assert_eq!(parse_rewrite_output("   \n"), None);
    }

    #[test]
    fn parse_rewrite_output_returns_trimmed_string() {
        assert_eq!(
            parse_rewrite_output("  rg foo  \n"),
            Some("rg foo".to_owned())
        );
    }

    #[test]
    fn hook_installed_from_verify_detects_missing_hook() {
        assert!(!hook_installed_from_verify(
            "RTK hook not installed\nsome other output"
        ));
    }

    #[test]
    fn hook_installed_from_verify_detects_present_hook() {
        assert!(hook_installed_from_verify(
            "All checks passed.\n3/3 tests passed"
        ));
    }
}
