//! End-to-end tests for rules loaded from `~/.config/crs/plugins.d/*.toml`.
//!
//! `load_config()` resolves the plugins dir from `dirs::home_dir()` with no env
//! override (issue #118), so these tests repoint `HOME` at a TempDir. Verified
//! that setting HOME on the spawned child redirects plugin loading.
//!
//! These tests build each rule from a string constant rather than reading the
//! live dotfiles repo, so they assert on rule semantics in isolation. The dotfiles
//! edits that make the rules live are verified separately by hand.

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use tempfile::TempDir;

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

/// A HOME directory containing `.config/crs/plugins.d/<name>.toml`.
struct PluginsHome {
    dir: TempDir,
}

impl PluginsHome {
    fn new() -> Self {
        let dir = TempDir::new().unwrap();
        std::fs::create_dir_all(dir.path().join(".config/crs/plugins.d")).unwrap();
        PluginsHome { dir }
    }

    fn write_rule(&self, name: &str, toml: &str) {
        let path = self.dir.path().join(".config/crs/plugins.d").join(name);
        std::fs::write(path, toml).unwrap();
    }

    /// Feed a full hook payload to `crs hook <event>` with HOME repointed.
    fn run(&self, event: &str, payload: &str) -> Output {
        let mut cmd = Command::new(workspace_bin("crs"));
        cmd.args(["hook", event])
            .env("HOME", self.dir.path())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = cmd.spawn().expect("spawn crs hook");
        child
            .stdin
            .as_mut()
            .unwrap()
            .write_all(payload.as_bytes())
            .unwrap();
        child.wait_with_output().unwrap()
    }
}

fn pre_payload(command: &str) -> String {
    format!(
        r#"{{"hook_event_name":"PreToolUse","tool_name":"Bash","tool_input":{{"command":{command:?}}}}}"#
    )
}

// Consumed by the rewrite and notify tests added in later tasks in this file.
// Scoped `allow` rather than a file-level one so it disappears once they land.
#[allow(dead_code)]
fn rewritten_command(out: &Output) -> Option<String> {
    let stdout = String::from_utf8_lossy(&out.stdout);
    let v: serde_json::Value = serde_json::from_str(stdout.trim()).ok()?;
    v.get("updatedInput")?
        .get("command")?
        .as_str()
        .map(str::to_string)
}

#[allow(dead_code)]
fn system_message(out: &Output) -> Option<String> {
    let stdout = String::from_utf8_lossy(&out.stdout);
    let v: serde_json::Value = serde_json::from_str(stdout.trim()).ok()?;
    v.get("hookSpecificOutput")?
        .get("systemMessage")?
        .as_str()
        .map(str::to_string)
}

#[test]
fn home_override_redirects_plugin_loading() {
    // Proves the seam the other tests in this file depend on: a rule in the
    // temp HOME's plugins.d actually loads and denies.
    let home = PluginsHome::new();
    home.write_rule(
        "zzprobe.toml",
        r#"[[hooks]]
label = "probe/deny"
event = "pre-tool-use"
matcher = "Bash"
pattern = "zzprobe"
action = "deny"
message = "PROBE_LOADED"
"#,
    );
    let out = home.run("pre-tool-use", &pre_payload("zzprobe run"));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("PROBE_LOADED"),
        "plugin rule did not load; stdout: {stdout}"
    );
}
