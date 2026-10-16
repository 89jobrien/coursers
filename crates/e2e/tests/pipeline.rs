//! End-to-end tests for the installed hook pipeline.

mod common;

use std::io::Write as _;
use std::path::PathBuf;
use std::process::Output;

use common::run_bin;
use tempfile::{NamedTempFile, TempDir};

// ---------------------------------------------------------------------------
// Pipeline struct
// ---------------------------------------------------------------------------

struct Pipeline {
    state: TempDir,
    rules: NamedTempFile,
    filters: NamedTempFile,
}

impl Pipeline {
    fn new(rules_json: &str, filters_toml: &str) -> Self {
        let state = TempDir::new().unwrap();
        let mut rules = NamedTempFile::new().unwrap();
        write!(rules, "{rules_json}").unwrap();
        let mut filters = NamedTempFile::new().unwrap();
        write!(filters, "{filters_toml}").unwrap();
        Pipeline {
            state,
            rules,
            filters,
        }
    }

    fn state_file(&self) -> PathBuf {
        self.state.path().join("state.json")
    }

    fn run_pre(&self, command: &str) -> Output {
        let payload = format!(r#"{{"tool_name":"Bash","tool_input":{{"command":{command:?}}}}}"#);
        run_bin(
            "coursers",
            &["pre"],
            &payload,
            &[
                ("COURSERS_RULES", self.rules.path().to_str().unwrap()),
                ("COURSERS_STATE", self.state_file().to_str().unwrap()),
            ],
        )
    }

    fn run_post(&self, command: &str, exit_code: i32) -> Output {
        let payload = format!(
            r#"{{"tool_name":"Bash","tool_input":{{"command":{command:?}}},"tool_response":{{"exit_code":{exit_code}}}}}"#
        );
        run_bin(
            "coursers",
            &["post"],
            &payload,
            &[
                ("COURSERS_RULES", self.rules.path().to_str().unwrap()),
                ("COURSERS_STATE", self.state_file().to_str().unwrap()),
            ],
        )
    }

    fn run_filter(&self, command: &str, output: &str, exit_code: i32) -> Output {
        let payload = format!(
            r#"{{"tool_name":"Bash","tool_input":{{"command":{command:?}}},"tool_response":{{"output":{output:?},"exit_code":{exit_code}}}}}"#
        );
        run_bin(
            "crs",
            &["filter"],
            &payload,
            &[("CRS_FILTERS", self.filters.path().to_str().unwrap())],
        )
    }

    fn run_rewrite(&self, command: &str) -> Option<String> {
        let payload = format!(r#"{{"tool_name":"Bash","tool_input":{{"command":{command:?}}}}}"#);
        let out = run_bin(
            "crs",
            &["rewrite"],
            &payload,
            &[("CRS_FILTERS", self.filters.path().to_str().unwrap())],
        );
        if !out.status.success() {
            return None;
        }
        let stdout = String::from_utf8_lossy(&out.stdout);
        let v: serde_json::Value = serde_json::from_str(&stdout).ok()?;
        v["hookSpecificOutput"]["updatedInput"]["command"]
            .as_str()
            .map(str::to_string)
    }
}

// ---------------------------------------------------------------------------
// Pipeline tests
// ---------------------------------------------------------------------------

#[test]
fn pipeline_grep_blocked() {
    let pipe = Pipeline::new(
        r#"{"rules":[{"id":"no-grep","pattern":"\\bgrep\\b","message":"Use Grep tool","exceptions":[]}]}"#,
        "",
    );
    // rewrite: no rule → passthrough
    assert!(pipe.run_rewrite("grep foo .").is_none());
    // pre: blocked
    let pre = pipe.run_pre("grep foo .");
    assert!(
        !pre.status.success(),
        "expected block, got {:?}\nstdout: {}",
        pre.status,
        String::from_utf8_lossy(&pre.stdout)
    );
}

#[test]
fn pipeline_ls_allowed_and_filter_suppresses() {
    let pipe = Pipeline::new(
        r#"{"rules":[]}"#,
        "[[filters]]\npattern = \"ls\"\nmode = \"failures-only\"\n",
    );
    // pre: allowed
    let pre = pipe.run_pre("ls -la");
    assert!(pre.status.success(), "expected allow, got {:?}", pre.status);
    // post: exit 0 → no state file written
    pipe.run_post("ls -la", 0);
    assert!(
        !pipe.state_file().exists(),
        "exit-0 post should not write state file"
    );
    // filter: suppress on exit 0
    let fil = pipe.run_filter("ls -la", "total 8", 0);
    assert!(fil.status.success());
    let stdout = String::from_utf8_lossy(&fil.stdout);
    assert!(
        stdout.contains("\"message\":\"\""),
        "expected suppress, got: {stdout}"
    );
}

#[test]
fn pipeline_learned_failure_blocks_after_threshold() {
    let pipe = Pipeline::new(r#"{"rules":[]}"#, "");
    let cmd = "e2e-pipeline-unique-zzz999";
    // Record 3 failures via post
    for _ in 0..3 {
        pipe.run_post(cmd, 1);
    }
    // pre should now block
    let pre = pipe.run_pre(cmd);
    assert!(
        !pre.status.success(),
        "expected block after 3 failures, got {:?}",
        pre.status
    );
}

#[test]
fn pipeline_cargo_test_rewritten_and_filtered() {
    let pipe = Pipeline::new(
        r#"{"rules":[]}"#,
        "[[rewrites]]\npattern = \"^cargo test(.*)\"\nreplace = \"cargo nextest run$1\"\n\
         [[filters]]\npattern = \"cargo nextest\"\nmode = \"failures-only\"\n",
    );
    // rewrite: cargo test → cargo nextest run
    let rewritten = pipe.run_rewrite("cargo test --release");
    assert_eq!(
        rewritten.as_deref(),
        Some("cargo nextest run --release"),
        "expected rewrite, got: {rewritten:?}"
    );
    // pre: allowed (no block rules)
    let pre = pipe.run_pre("cargo nextest run --release");
    assert!(pre.status.success(), "expected allow");
    // filter: suppress on exit 0
    let fil = pipe.run_filter("cargo nextest run --release", "42 tests passed", 0);
    let stdout = String::from_utf8_lossy(&fil.stdout);
    assert!(
        stdout.contains("\"message\":\"\""),
        "expected suppress on success, got: {stdout}"
    );
}

#[test]
fn crs_hook_pre_tool_use_runs_course_correct_rules() {
    // Regression: settings.json wires PreToolUse to `crs hook pre-tool-use`,
    // which used to run only the TOML pipeline — course-correct rules from
    // the JSON rules file never fired through the real hook path.
    let pipe = Pipeline::new(
        r#"{"rules":[{"id":"no-bash-use-nu","pattern":"(;|&&|\\|\\|)","message":"Use nu","exceptions":["^nu\\b"]}]}"#,
        "",
    );
    let run = |command: &str| -> Output {
        let payload = format!(r#"{{"tool_name":"Bash","tool_input":{{"command":{command:?}}}}}"#);
        run_bin(
            "crs",
            &["hook", "pre-tool-use"],
            &payload,
            &[
                ("COURSERS_RULES", pipe.rules.path().to_str().unwrap()),
                ("COURSERS_STATE", pipe.state_file().to_str().unwrap()),
            ],
        )
    };

    // Sequential operators → course-correct deny (exit 2).
    let out = run("foo && bar");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(out.status.code(), Some(2), "expected deny, got: {stdout}");
    assert!(
        stdout.contains("\"deny\""),
        "expected deny response: {stdout}"
    );

    // The ^nu exception applies to the whole command → allowed.
    let out = run("nu -c ls");
    assert_eq!(out.status.code(), Some(0), "expected allow for nu-wrapped");
}

#[test]
fn harness_selects_active_profile_when_release_artifact_exists() {
    let target = tempfile::tempdir().unwrap();
    let debug = target.path().join("debug");
    let release = target.path().join("release");
    std::fs::create_dir_all(debug.join("deps")).unwrap();
    std::fs::create_dir_all(&release).unwrap();
    let binary_name = format!("coursers{}", std::env::consts::EXE_SUFFIX);
    std::fs::write(debug.join(&binary_name), "debug").unwrap();
    std::fs::write(release.join(&binary_name), "stale release").unwrap();

    assert_eq!(
        common::binary_for_test_executable(&debug.join("deps/pipeline-test-id"), "coursers"),
        debug.join(binary_name)
    );
}

#[test]
fn harness_derives_profile_from_cargo_test_executable() {
    let executable = std::path::Path::new("target/debug/deps/pipeline-test-id");

    assert_eq!(
        common::profile_dir_from_test_executable(executable),
        std::path::Path::new("target/debug")
    );
}
