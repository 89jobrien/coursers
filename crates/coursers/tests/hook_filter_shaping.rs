//! Integration tests for `filters.toml` output shaping in the hook path.
//!
//! `filters.toml` used to be reachable only through the manual `crs filter` subcommand, so
//! these rules had no effect on live tool results. Both hook backends now shape output before
//! the pipeline runs, which matters most for the OpenCode harness: there the legacy
//! `chain.run_post` filter and the pipeline's Redact arm compete for the same
//! `replacement_output` field, and without shaping first the Redact arm wins and the
//! unshaped log is returned.
//!
//! `HOME` is isolated per test and `CRS_FILTERS` points at a temp file, so the developer's
//! real `~/.config/crs/filters.toml` and `plugins.d/` cannot leak into an assertion.

#[path = "common_bin.rs"]
mod common_bin;

use common_bin::crs_bin;
use serde_json::{Value, json};
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};

struct Fixture {
    _temp: tempfile::TempDir,
    project: PathBuf,
    home: PathBuf,
    filters: PathBuf,
}

impl Fixture {
    fn new(filter_rules: &str) -> Self {
        Self::with_plugin(filter_rules, None)
    }

    /// `plugin` is written into `$HOME/.config/crs/plugins.d/probe.toml`. There is no env
    /// override for that directory (see `TODO(plugins-dir-env-override)` in
    /// `crates/core/src/hook/pipeline.rs`), so repointing `HOME` is the only way to exercise a
    /// `plugins.d` rule without touching live configuration.
    fn with_plugin(filter_rules: &str, plugin: Option<&str>) -> Self {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path().join("home");
        let project = temp.path().join("project");
        std::fs::create_dir_all(&home).unwrap();
        std::fs::create_dir_all(&project).unwrap();
        let filters = temp.path().join("filters.toml");
        std::fs::write(&filters, filter_rules).unwrap();
        if let Some(plugin) = plugin {
            let dir = home.join(".config/crs/plugins.d");
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(dir.join("probe.toml"), plugin).unwrap();
        }
        Self {
            _temp: temp,
            project,
            home,
            filters,
        }
    }

    fn run(&self, target: &str, event: &str, payload: &str) -> Output {
        let mut child = Command::new(crs_bin())
            .args(["hook", "--target", target, event])
            .current_dir(&self.project)
            .env("HOME", &self.home)
            .env("COURSERS_STATE", self.project.join("state.json"))
            .env("CRS_FILTERS", &self.filters)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .as_mut()
            .unwrap()
            .write_all(payload.as_bytes())
            .unwrap();
        child.wait_with_output().unwrap()
    }

    /// OpenCode PostToolUse. The harness requires `tool_name`, `tool_input`, and a
    /// `tool_response` carrying both `exit_code` and `output` — it rejects the request
    /// otherwise, which is its documented contract.
    fn post_bash(&self, command: &str, output: &str, exit_code: i64) -> Value {
        self.response("opencode", command, output, exit_code)
    }

    fn response(&self, target: &str, command: &str, output: &str, exit_code: i64) -> Value {
        let payload = json!({
            "tool_name": "Bash",
            "tool_input": {"command": command},
            "tool_response": {"exit_code": exit_code, "output": output},
            "session_id": "ses-shaping",
        })
        .to_string();
        let out = self.run(target, "post-tool-use", &payload);
        assert!(
            out.status.success(),
            "{target} exited {:?}; stderr: {}",
            out.status.code(),
            String::from_utf8_lossy(&out.stderr)
        );
        serde_json::from_slice(&out.stdout)
            .unwrap_or_else(|e| panic!("stdout was not JSON ({e}): {:?}", out.stdout))
    }
}

/// OpenCode reports shaped output in `replacement_output`; Claude puts it in `message`.
/// Both return `None` when nothing needed replacing.
fn shaped(v: &Value) -> Option<String> {
    v["replacement_output"].as_str().map(str::to_string)
}

/// Claude equivalent of [`shaped`]. The Claude hook emits a response object only when it has
/// something to say, so an empty stdout is the "no replacement" signal — not a parse error.
fn claude_shaped(f: &Fixture, command: &str, output: &str, exit_code: i64) -> Option<String> {
    let payload = json!({
        "hook_event_name": "PostToolUse",
        "tool_name": "Bash",
        "tool_input": {"command": command},
        "tool_response": {"exit_code": exit_code, "output": output},
        "session_id": "ses-shaping",
    })
    .to_string();
    let out = f.run("claude", "post-tool-use", &payload);
    assert!(
        out.status.success(),
        "claude exited {:?}; stderr: {}",
        out.status.code(),
        String::from_utf8_lossy(&out.stderr)
    );
    if out.stdout.iter().all(u8::is_ascii_whitespace) {
        return None;
    }
    let json: Value = serde_json::from_slice(&out.stdout).unwrap_or_else(|e| {
        panic!(
            "claude stdout was not JSON ({e}): {:?}",
            String::from_utf8_lossy(&out.stdout)
        )
    });
    Some(json["message"].as_str()?.to_string())
}

const CARGO_LOG: &str = "    Checking foo v0.1.0\n   Compiling bar v1.0.0\nwarning: unused variable: `x`\n --> src/lib.rs:2:9\n    Finished `dev` profile in 1.20s\n";

const ERRORS_ONLY: &str = r#"
[[filters]]
pattern = "cargo (check|clippy)"
mode = "errors-only"
"#;

const MATCH_LINES: &str = r#"
[[filters]]
pattern = "cargo (check|clippy)"
mode = "match-lines"
match_pattern = '^(warning|error|help|note):|-->|^error\[|Finished'
"#;

const FAILURES_ONLY: &str = r#"
[[filters]]
pattern = "cargo (nextest|test)"
mode = "failures-only"
"#;

// ---------------------------------------------------------------------------
// The regression: shaping must survive the pipeline's Redact arm
// ---------------------------------------------------------------------------

/// The bug this wiring exists for. On OpenCode the legacy filter chain and the pipeline both
/// target `replacement_output`, and the Redact arm's assignment used to land last, returning
/// the untouched log. With shaping applied first, the surviving replacement is the shaped one.
#[test]
fn opencode_shaping_survives_a_matching_redact_rule() {
    let f = Fixture::new(MATCH_LINES);
    let json = f.post_bash("cargo check --workspace", CARGO_LOG, 0);
    let out = shaped(&json).expect("replacement_output should be set");
    assert!(
        out.contains("warning: unused variable"),
        "warning lost: {out:?}"
    );
    assert!(out.contains("Finished"), "completion marker lost: {out:?}");
    assert!(
        !out.contains("Compiling bar"),
        "progress noise leaked: {out:?}"
    );
}

/// A suppressing rule must still suppress, even though a redact rule also matches. Regression
/// guard: the Redact arm must not resurrect a suppressed payload.
#[test]
fn opencode_suppressing_rule_wins_over_redact_rule() {
    let f = Fixture::new(ERRORS_ONLY);
    let json = f.post_bash("cargo check --workspace", CARGO_LOG, 0);
    assert_eq!(
        shaped(&json),
        Some(String::new()),
        "errors-only should suppress a clean run, not fall back to the raw log"
    );
}

// ---------------------------------------------------------------------------
// errors-only — the mode that was reverted in the real config
// ---------------------------------------------------------------------------

/// `errors-only` returns Suppress when nothing contains "error", which is why it was wrong
/// as an automatic policy: a successful check with warnings came back empty, hiding the
/// signal CLAUDE.md says to act on. Pinned so the mode's semantics stay visible even though
/// the shipped rule now uses `match-lines`.
#[test]
fn errors_only_keeps_only_error_lines_on_failure() {
    let f = Fixture::new(ERRORS_ONLY);
    let json = f.post_bash(
        "cargo check --workspace",
        "    Checking foo v0.1.0\nerror[E0308]: mismatched types\n --> src/lib.rs:4:5\n",
        101,
    );
    let out = shaped(&json).expect("replacement_output should be set");
    assert!(out.contains("error[E0308]"), "got: {out:?}");
    assert!(!out.contains("Checking foo"), "noise leaked: {out:?}");
}

// ---------------------------------------------------------------------------
// match-lines — what cargo check/clippy actually ships with now
// ---------------------------------------------------------------------------

/// The behaviour the revert was chosen for: warnings must survive a successful run.
#[test]
fn match_lines_keeps_warnings_on_a_successful_check() {
    let f = Fixture::new(MATCH_LINES);
    let json = f.post_bash("cargo check --workspace", CARGO_LOG, 0);
    let out = shaped(&json).expect("replacement_output should be set");
    assert!(out.contains("warning: unused variable"), "got: {out:?}");
    assert!(!out.contains("Compiling bar"), "noise leaked: {out:?}");
}

/// `match-lines` short-circuits to full passthrough on a non-zero exit, so a failure is
/// deliberately not trimmed. Pinned because it is the cost of not hiding warnings.
///
/// "Unchanged" is signalled by the *absence* of a replacement: the harness then falls back to
/// the tool's own output, which is exactly the pass-through we want and saves emitting a
/// redundant copy of the whole log.
#[test]
fn match_lines_passes_failure_output_through_unfiltered() {
    let f = Fixture::new(MATCH_LINES);
    let log = "    Checking foo v0.1.0\nerror: boom\n    Finished `dev` profile\n";
    let json = f.post_bash("cargo check --workspace", log, 101);
    assert!(
        shaped(&json).is_none(),
        "failure should fall through untouched, got {:?}",
        shaped(&json)
    );
}

// ---------------------------------------------------------------------------
// failures-only
// ---------------------------------------------------------------------------

/// A suppressing rule must suppress, and a passing rule on failure must fall through rather
/// than re-emit the log it was told to keep.
#[test]
fn failures_only_suppresses_success_and_leaves_failure_alone() {
    let f = Fixture::new(FAILURES_ONLY);
    let json = f.post_bash("cargo nextest run", "3 passed\n", 0);
    assert_eq!(
        shaped(&json),
        Some(String::new()),
        "success should be suppressed"
    );

    let json = f.post_bash("cargo nextest run", "1 failed: boom\n", 1);
    assert!(
        shaped(&json).is_none(),
        "failure output is identical to input, so no replacement is needed; got {:?}",
        shaped(&json)
    );
}

// ---------------------------------------------------------------------------
// scope — the gate that keeps this from changing unrelated tools
// ---------------------------------------------------------------------------

/// An unmatched command must not be shaped. `chain_with_filter_hook_allow_when_no_rule_matches`
/// covers the legacy chain; this pins it end to end through the harness.
#[test]
fn unmatched_command_is_untouched() {
    let f = Fixture::new(ERRORS_ONLY);
    let log = "total 8\ndrwxr-xr-x  3 joe  staff  96 .\n";
    let json = f.post_bash("ls -la", log, 0);
    assert!(
        shaped(&json).is_none(),
        "unmatched command should be left alone, got {:?}",
        shaped(&json)
    );
}

#[test]
fn empty_filter_config_leaves_output_untouched() {
    let f = Fixture::new("");
    let json = f.post_bash("cargo check --workspace", CARGO_LOG, 0);
    assert!(
        shaped(&json).is_none(),
        "no rules configured should be a no-op, got {:?}",
        shaped(&json)
    );
}

/// Non-Bash tools must not be shaped even when their text would match a rule.
#[test]
fn non_bash_tool_is_not_shaped() {
    let f = Fixture::new(ERRORS_ONLY);
    let payload = json!({
        "tool_name": "Read",
        "tool_input": {"file_path": "cargo check --workspace"},
        "tool_response": {"exit_code": 0, "output": "warning: unused variable"},
        "session_id": "ses-shaping",
    })
    .to_string();
    let out = f.run("opencode", "post-tool-use", &payload);
    assert!(out.status.success());
    let json: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(
        json["replacement_output"].is_null(),
        "Read must not be shaped, got {}",
        json["replacement_output"]
    );
}

// ---------------------------------------------------------------------------
// backend parity — both hook targets must shape identically
// ---------------------------------------------------------------------------

/// The Claude path and the OpenCode path are separate implementations of the same invariant.
/// If they drift, one harness quietly shows raw logs while the other does not, which is
/// exactly the confusion that sent this through two rounds of fixing.
///
/// A changed log must come out identical from both. A run that needed no change must produce
/// *no response at all* from Claude (no stdout) and a null `replacement_output` from
/// OpenCode — both mean "keep the tool's own output", and neither should echo it back.
#[test]
fn claude_and_opencode_agree_on_a_shaped_log() {
    let f = Fixture::new(MATCH_LINES);
    let opencode = f.post_bash("cargo check --workspace", CARGO_LOG, 0);
    let claude_text = claude_shaped(&f, "cargo check --workspace", CARGO_LOG, 0);

    assert_eq!(
        claude_text,
        shaped(&opencode),
        "backends disagree on a shaped log:\n  claude:   {claude_text:?}\n  opencode: {:?}",
        shaped(&opencode)
    );
    assert!(
        claude_text
            .unwrap_or_default()
            .contains("warning: unused variable")
    );
}

#[test]
fn claude_and_opencode_agree_on_an_unchanged_log() {
    let f = Fixture::new(ERRORS_ONLY);
    // exit 1 with an error line: errors-only replaces, so both must emit.
    let failing = "    Checking foo v0.1.0\nerror: boom\n";
    let claude_changed = claude_shaped(&f, "cargo check --workspace", failing, 101);
    let opencode_changed = f.post_bash("cargo check --workspace", failing, 101);
    assert_eq!(claude_changed, shaped(&opencode_changed));

    // No matching rule: both must stay silent so the tool output survives.
    let log = "total 8\ndrwxr-xr-x  3 joe  staff  96 .\n";
    let claude_unchanged = claude_shaped(&f, "ls -la", log, 0);
    let opencode_unchanged = f.post_bash("ls -la", log, 0);
    assert_eq!(claude_unchanged, None, "claude should emit no response");
    assert_eq!(
        shaped(&opencode_unchanged),
        None,
        "opencode should emit no replacement"
    );
}

// ---------------------------------------------------------------------------
// REGRESSION: redaction must run before shaping, not after
// ---------------------------------------------------------------------------

/// A `redact` rule wired the way the live `godmode.toml` wires it. `obfsck` must be on PATH
/// for this to do anything — it is, in this environment, which is the point: the test is
/// about ordering, not about obfsck's availability.
const REDACT_RULE: &str = r#"
[[hooks]]
label = "probe/redact"
event = "post-tool-use"
matcher = "Bash"
pattern = ".*"
action = "redact"
level = "standard"
"#;

const GITHUB_TOKEN: &str = "ghp_ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";

/// Matches on `REDACTED`, a token that only exists after obfsck has run — the hinge the
/// ordering tests below turn on.
const MATCHES_ONLY_REDACTED: &str = r#"
[[filters]]
pattern = "cargo check"
mode = "match-lines"
match_pattern = "REDACTED"
"#;

/// The bug: shaping ran before redaction, so a `match-lines` rule could drop a secret-bearing
/// line before obfsck ever saw it. That was safe by accident — and it made a context-economy
/// rule into an accidental security control.
///
/// The filter matches on `REDACTED`, which only exists in the output *after* obfsck has run:
///   - redact-first (correct): the secret is replaced, the marker matches, the line survives
///   - shape-first (the bug):   no marker exists yet, nothing matches, output is suppressed
///
/// A suppressed line and a redacted line are both "the secret did not reach the model", so
/// assert on the marker surviving: only the correct order produces output at all.
///
/// `exit_code` MUST be 0. `match-lines` short-circuits to passthrough on a non-zero exit, so
/// a failing exit would skip the filter entirely and this test would pass under both
/// orderings — which it did, until a mutation check caught it.
#[test]
fn redaction_runs_before_shaping_so_filters_see_redacted_text() {
    let filters = MATCHES_ONLY_REDACTED;
    let f = Fixture::with_plugin(filters, Some(REDACT_RULE));
    let log = format!("error: token {GITHUB_TOKEN} rejected\n");
    let json = f.post_bash("cargo check", &log, 0);

    let out = shaped(&json).unwrap_or_default();
    assert!(
        out.contains("REDACTED"),
        "shaping must run on already-redacted text; got {out:?}"
    );
    assert!(
        !out.contains(GITHUB_TOKEN),
        "secret must never survive: {out:?}"
    );
}

/// Belt and braces: whatever the filter does, a secret in the output must not come back out.
#[test]
fn secret_never_survives_the_redact_then_shape_pipeline() {
    let filters = r#"
[[filters]]
pattern = ".*"
mode = "match-lines"
match_pattern = ".*"
"#;
    let f = Fixture::with_plugin(filters, Some(REDACT_RULE));
    let log = format!("prefix\ntoken={GITHUB_TOKEN}\nsuffix\n");
    let json = f.post_bash("some-arbitrary-command", &log, 0);
    let out = shaped(&json).unwrap_or_default();
    assert!(!out.contains(GITHUB_TOKEN), "secret leaked: {out:?}");
}

/// Both backends must agree on ordering too — the fix landed in `cmd_hook` and
/// `opencode::run_hook` separately, and they drifted once already.
#[test]
fn claude_and_opencode_agree_on_redaction_order() {
    let filters = MATCHES_ONLY_REDACTED;
    let f = Fixture::with_plugin(filters, Some(REDACT_RULE));
    let log = format!("error: token {GITHUB_TOKEN} rejected\n");

    let opencode = f.post_bash("cargo check", &log, 101);
    let claude = claude_shaped(&f, "cargo check", &log, 101);

    assert_eq!(
        claude,
        shaped(&opencode),
        "backends disagree on redact-then-shape:\n  claude:   {claude:?}\n  opencode: {:?}",
        shaped(&opencode)
    );
}
