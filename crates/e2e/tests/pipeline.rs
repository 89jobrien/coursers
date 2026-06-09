use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use tempfile::{NamedTempFile, TempDir};

// ---------------------------------------------------------------------------
// Harness helpers
// ---------------------------------------------------------------------------

fn workspace_bin(name: &str) -> PathBuf {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let workspace = manifest.parent().unwrap().parent().unwrap();
    let debug = workspace.join("target/debug").join(name);
    let release = workspace.join("target/release").join(name);
    if release.exists() {
        release
    } else {
        assert!(
            debug.exists(),
            "binary {name:?} not found — run `cargo build --workspace` first\nchecked: {}",
            debug.display()
        );
        debug
    }
}

fn run_bin(bin: &str, sub: &str, payload: &str, envs: &[(&str, &str)]) -> Output {
    let mut cmd = Command::new(workspace_bin(bin));
    cmd.arg(sub)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for (k, v) in envs {
        cmd.env(k, v);
    }
    let mut child = cmd
        .spawn()
        .unwrap_or_else(|e| panic!("spawn {bin} {sub}: {e}"));
    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(payload.as_bytes())
        .unwrap();
    child.wait_with_output().unwrap()
}

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
            "pre",
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
            "post",
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
            "filter",
            &payload,
            &[("CRS_FILTERS", self.filters.path().to_str().unwrap())],
        )
    }

    fn run_rewrite(&self, command: &str) -> Option<String> {
        let payload = format!(r#"{{"tool_name":"Bash","tool_input":{{"command":{command:?}}}}}"#);
        let out = run_bin(
            "crs",
            "rewrite",
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
        stdout.contains(r#""message":""#),
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
        stdout.contains(r#""message":""#),
        "expected suppress on success, got: {stdout}"
    );
}
