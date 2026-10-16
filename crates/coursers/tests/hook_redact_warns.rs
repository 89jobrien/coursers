//! Redaction must never fail silently.
//!
//! `run_redact` fails open by design — a hook must not crash the tool call it observes — but
//! failing open on a *secrecy* boundary means a broken or missing `obfsck` lets every secret
//! in every tool result reach the model in cleartext while the rule still logs `PASS`. These
//! tests pin the stderr warning that makes that condition visible, and pin that stdout stays
//! protocol-clean when it fires.
//!
//! `obfsck` is replaced by a stub on `PATH`, so these tests assert the failure wiring rather
//! than obfsck's own detection quality.

#[path = "common_bin.rs"]
mod common_bin;

use common_bin::crs_bin;
use serde_json::{Value, json};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

/// A directory holding only the stub `obfsck`, prepended to a minimal-but-usable PATH.
struct Fixture {
    _temp: tempfile::TempDir,
    project: PathBuf,
    home: PathBuf,
    stub_dir: PathBuf,
}

/// Writes `obfsck` as an executable shell script and returns its directory.
fn stub_dir(dir: &Path, script: &str) -> PathBuf {
    let bin = dir.join("stubbin");
    std::fs::create_dir_all(&bin).unwrap();
    let path = bin.join("obfsck");
    std::fs::write(&path, script).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    bin
}

impl Fixture {
    /// `obfsck_script` is the body of the stub. `None` omits `obfsck` from PATH entirely.
    fn new(obfsck_script: Option<&str>) -> Self {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path().join("home");
        let project = temp.path().join("project");
        let plugins = home.join(".config/crs/plugins.d");
        std::fs::create_dir_all(&plugins).unwrap();
        std::fs::create_dir_all(&project).unwrap();
        std::fs::write(
            plugins.join("redact.toml"),
            r#"
[[hooks]]
label = "test/redact"
event = "post-tool-use"
matcher = "Bash"
pattern = ".*"
action = "redact"
level = "standard"
"#,
        )
        .unwrap();

        let stub_dir = match obfsck_script {
            Some(script) => stub_dir(temp.path(), script),
            None => temp.path().join("no-obfsck-here"),
        };
        Self {
            _temp: temp,
            project,
            home,
            stub_dir,
        }
    }

    fn run(&self) -> Output {
        let payload = json!({
            "tool_name": "Bash",
            "tool_input": {"command": "echo hi"},
            "tool_response": {"exit_code": 0, "output": "aws=AKIAIOSFODNN7EXAMPLE"},
            "session_id": "ses-redact",
        })
        .to_string();

        // The stub dir leads PATH; /usr/bin and /bin follow so the hook still finds `sh`.
        // When the stub is absent, the real obfsck in ~/.cargo/bin is deliberately NOT on
        // PATH either, so `obfsck` is genuinely unresolvable.
        let path = format!("{}:/usr/bin:/bin", self.stub_dir.display());

        let mut child = Command::new(crs_bin())
            .args(["hook", "post-tool-use"])
            .current_dir(&self.project)
            .env("HOME", &self.home)
            .env("COURSERS_STATE", self.project.join("state.json"))
            .env("PATH", path)
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
}

fn stderr_of(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).to_string()
}

/// The regression: a non-zero exit must be loud, and must say the output was not redacted.
#[test]
fn nonzero_obfsck_exit_warns_that_output_was_not_redacted() {
    let f = Fixture::new(Some("#!/bin/sh\nexit 3\n"));
    let out = f.run();
    let err = stderr_of(&out);
    assert!(
        err.contains("NOT redacted"),
        "expected an explicit not-redacted warning, got: {err:?}"
    );
    assert!(
        err.contains("exit status: 3"),
        "status missing from: {err:?}"
    );
}

/// obfsck's own diagnostics are surfaced, not swallowed by the old `Stdio::null()`.
#[test]
fn obfsck_stderr_is_surfaced_in_the_warning() {
    let f = Fixture::new(Some("#!/bin/sh\necho 'fatal: bad config' >&2\nexit 1\n"));
    let err = stderr_of(&f.run());
    assert!(
        err.contains("fatal: bad config"),
        "obfsck stderr must reach the operator, got: {err:?}"
    );
}

/// A missing binary is the case that matters most — a launchd context with a narrower PATH
/// would otherwise redact nothing, silently, forever.
#[test]
fn missing_obfsck_warns_rather_than_passing_quietly() {
    let f = Fixture::new(None);
    let err = stderr_of(&f.run());
    assert!(
        err.contains("could not run") && err.contains("NOT redacted"),
        "missing obfsck must warn, got: {err:?}"
    );
}

/// Silence is reserved for "redaction ran and changed nothing", which is the ordinary case
/// for most tool output and must not be reported as a failure.
#[test]
fn successful_redaction_with_no_matches_is_reported_distinctly() {
    let f = Fixture::new(Some("#!/bin/sh\ncat\n"));
    let err = stderr_of(&f.run());
    assert!(
        err.contains("made no changes"),
        "a no-op redaction should say so, got: {err:?}"
    );
    assert!(
        !err.contains("NOT redacted"),
        "a no-op redaction is not a failure: {err:?}"
    );
}

/// Warnings must never contaminate stdout — it carries hook JSON.
#[test]
fn warnings_never_pollute_stdout() {
    for script in [Some("#!/bin/sh\nexit 3\n"), None] {
        let f = Fixture::new(script);
        let out = f.run();
        let json: Value = serde_json::from_slice(&out.stdout).unwrap_or_else(|e| {
            panic!(
                "stdout was not JSON ({e}): {:?} / stderr {:?}",
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            )
        });
        assert_eq!(json["type"], "result", "unexpected response: {json}");
    }
}
