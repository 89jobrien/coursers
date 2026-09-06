#[path = "common_bin.rs"]
mod common_bin;

use common_bin::crs_bin;
use serde_json::{Value, json};
use std::io::Write;
use std::path::Path;
use std::process::{Command, Output, Stdio};

struct Fixture {
    _temp: tempfile::TempDir,
    home: std::path::PathBuf,
    project: std::path::PathBuf,
    rules: std::path::PathBuf,
    state: std::path::PathBuf,
    filters: std::path::PathBuf,
}

impl Fixture {
    fn new(rules: &str, filters: &str) -> Self {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path().join("home");
        let project = temp.path().join("project");
        std::fs::create_dir_all(&home).unwrap();
        std::fs::create_dir_all(&project).unwrap();
        let rules_path = temp.path().join("rules.json");
        let state = temp.path().join("state.json");
        let filters_path = temp.path().join("filters.toml");
        std::fs::write(&rules_path, rules).unwrap();
        std::fs::write(&filters_path, filters).unwrap();
        Self {
            _temp: temp,
            home,
            project,
            rules: rules_path,
            state,
            filters: filters_path,
        }
    }

    fn run(&self, event: &str, payload: &str) -> Output {
        run_hook(
            &self.project,
            &self.home,
            &self.rules,
            &self.state,
            &self.filters,
            &["hook", "--target", "opencode", event],
            payload,
        )
    }
}

fn run_hook(
    current_dir: &Path,
    home: &Path,
    rules: &Path,
    state: &Path,
    filters: &Path,
    args: &[&str],
    payload: &str,
) -> Output {
    let mut child = Command::new(crs_bin())
        .args(args)
        .current_dir(current_dir)
        .env("HOME", home)
        .env("COURSERS_RULES", rules)
        .env("COURSERS_STATE", state)
        .env("CRS_FILTERS", filters)
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

fn response(output: &Output) -> Value {
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

fn payload(command: &str, output: &str) -> String {
    json!({
        "tool_name": "Bash",
        "tool_input": {"command": command},
        "tool_response": {"exit_code": 0, "output": output},
        "session_id": "ses-root"
    })
    .to_string()
}

#[test]
fn opencode_tool_allow_returns_neutral_json() {
    let fixture = Fixture::new(r#"{"rules":[],"failure_learning":{"enabled":false}}"#, "");
    let json = response(&fixture.run("pre-tool-use", &payload("echo hi", "")));
    assert_eq!(json["decision"], "allow");
    assert!(json["reason"].is_null());
    assert!(json["updated_input"].is_null());
    assert!(json["replacement_output"].is_null());
}

#[test]
fn opencode_tool_deny_returns_zero_with_deny_decision() {
    let fixture = Fixture::new(
        r#"{"rules":[{"id":"no-rm","pattern":"rm -rf","message":"destructive command"}],"failure_learning":{"enabled":false}}"#,
        "",
    );
    let json = response(&fixture.run("pre-tool-use", &payload("rm -rf build", "")));
    assert_eq!(json["decision"], "deny");
    assert!(
        json["reason"]
            .as_str()
            .is_some_and(|reason| !reason.is_empty())
    );
}

#[test]
fn opencode_tool_rewrite_returns_updated_input() {
    let fixture = Fixture::new(
        r#"{"rules":[],"failure_learning":{"enabled":false}}"#,
        "[[rewrites]]\npattern = \"^ls$\"\nreplace = \"eza\"\n",
    );
    let json = response(&fixture.run("pre-tool-use", &payload("ls", "")));
    assert_eq!(json["updated_input"]["command"], "eza");
}

#[test]
fn opencode_tool_filter_returns_replacement_output() {
    let fixture = Fixture::new(
        r#"{"rules":[],"failure_learning":{"enabled":false}}"#,
        "[[filters]]\npattern = \"^ls$\"\nmode = \"truncate\"\nmax_lines = 2\n",
    );
    let json = response(&fixture.run("post-tool-use", &payload("ls", "one\ntwo\nthree")));
    assert!(
        json["replacement_output"]
            .as_str()
            .is_some_and(|text| text.starts_with("one\ntwo"))
    );
}

#[test]
fn opencode_lifecycle_notify_returns_messages() {
    let fixture = Fixture::new(r#"{"rules":[],"failure_learning":{"enabled":false}}"#, "");
    std::fs::create_dir_all(fixture.project.join(".ctx")).unwrap();
    std::fs::write(
        fixture.project.join(".ctx/crs-hooks.toml"),
        r#"
[[hooks]]
event = "session-start"
action = "notify"
template = "OpenCode session started"
label = "opencode/session-start"
"#,
    )
    .unwrap();

    let json = response(&fixture.run("session-start", r#"{"session_id":"ses-root"}"#));
    assert_eq!(json["messages"], json!(["OpenCode session started"]));
    assert_eq!(json["matched_rules"], json!(["opencode/session-start"]));
}

#[test]
fn opencode_lifecycle_deny_is_structured() {
    let fixture = Fixture::new(r#"{"rules":[],"failure_learning":{"enabled":false}}"#, "");
    std::fs::create_dir_all(fixture.project.join(".ctx")).unwrap();
    std::fs::write(
        fixture.project.join(".ctx/crs-hooks.toml"),
        r#"
[[hooks]]
event = "user-prompt-submit"
action = "deny"
message = "prompt denied"
label = "opencode/prompt"
"#,
    )
    .unwrap();

    let json = response(&fixture.run("user-prompt-submit", r#"{"target":"unsafe prompt"}"#));
    assert_eq!(json["decision"], "deny");
    assert_eq!(json["reason"], "prompt denied");
}

#[test]
fn opencode_invalid_json_exits_nonzero() {
    let fixture = Fixture::new(r#"{"rules":[],"failure_learning":{"enabled":false}}"#, "");
    let output = fixture.run("session-start", "{");
    assert!(!output.status.success());
    assert!(!output.stderr.is_empty());
}

fn validation_fixture() -> (tempfile::TempDir, std::path::PathBuf, std::path::PathBuf) {
    use std::os::unix::fs::PermissionsExt;

    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("home");
    let project = temp.path().join("project");
    let bin = temp.path().join("bin");
    std::fs::create_dir_all(&home).unwrap();
    std::fs::create_dir_all(&project).unwrap();
    std::fs::create_dir_all(&bin).unwrap();
    for binary in ["crs", "opencode"] {
        let path = bin.join(binary);
        std::fs::write(&path, "#!/bin/sh\nexit 0\n").unwrap();
        let mut permissions = std::fs::metadata(&path).unwrap().permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(path, permissions).unwrap();
    }
    (temp, home, project)
}

fn run_validation(temp: &Path, home: &Path, project: &Path) -> Output {
    Command::new(crs_bin())
        .args(["validate-hooks", "--target", "opencode"])
        .current_dir(project)
        .env("HOME", home)
        .env(
            "PATH",
            format!("{}:/usr/bin:/bin", temp.join("bin").display()),
        )
        .output()
        .unwrap()
}

fn write_opencode_plugin_pair(plugin: &Path) {
    std::fs::create_dir_all(plugin.parent().unwrap()).unwrap();
    std::fs::write(plugin, "export {}\n").unwrap();
    std::fs::write(plugin.with_file_name("opencode-plugin.d.ts"), "// types\n").unwrap();
}

#[test]
fn opencode_validation_accepts_project_plugin() {
    let (temp, home, project) = validation_fixture();
    let plugin = project.join(".opencode/plugins/coursers.ts");
    write_opencode_plugin_pair(&plugin);

    let output = run_validation(temp.path(), &home, &project);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains(&plugin.display().to_string()));
}

#[test]
fn opencode_validation_accepts_global_plugin() {
    let (temp, home, project) = validation_fixture();
    let plugin = home.join(".config/opencode/plugins/coursers.ts");
    write_opencode_plugin_pair(&plugin);

    let output = run_validation(temp.path(), &home, &project);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains(&plugin.display().to_string()));
}

#[test]
fn opencode_validation_reports_missing_plugin() {
    let (temp, home, project) = validation_fixture();
    let output = run_validation(temp.path(), &home, &project);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("MISSING plugin"));
}

#[test]
fn opencode_target_does_not_change_claude_default() {
    let fixture = Fixture::new(r#"{"rules":[],"failure_learning":{"enabled":false}}"#, "");
    std::fs::create_dir_all(fixture.project.join(".ctx")).unwrap();
    std::fs::write(
        fixture.project.join(".ctx/crs-hooks.toml"),
        r#"
[[hooks]]
event = "session-start"
action = "notify"
template = "Claude session started"
label = "regression/claude-default"
"#,
    )
    .unwrap();
    let output = run_hook(
        &fixture.project,
        &fixture.home,
        &fixture.rules,
        &fixture.state,
        &fixture.filters,
        &["hook", "session-start"],
        "{}",
    );
    assert!(output.status.success());
    let json: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["hookSpecificOutput"]["hookEventName"], "SessionStart");
    assert_eq!(
        json["hookSpecificOutput"]["systemMessage"],
        "Claude session started"
    );
    assert!(json.get("decision").is_none());
}

#[test]
fn opencode_empty_lifecycle_payload_allows() {
    let fixture = Fixture::new(r#"{"rules":[],"failure_learning":{"enabled":false}}"#, "");
    let json = response(&fixture.run("session-start", ""));
    assert_eq!(json["decision"], "allow");
}

fn write_hook_config(fixture: &Fixture, config: &str) {
    std::fs::create_dir_all(fixture.project.join(".ctx")).unwrap();
    std::fs::write(fixture.project.join(".ctx/crs-hooks.toml"), config).unwrap();
}

#[test]
fn opencode_chain_rewrite_feeds_generic_deny() {
    let fixture = Fixture::new(
        r#"{"rules":[],"failure_learning":{"enabled":false}}"#,
        "[[rewrites]]\npattern = \"^ls$\"\nreplace = \"eza\"\n",
    );
    write_hook_config(
        &fixture,
        r#"
[[hooks]]
event = "pre-tool-use"
matcher = "Bash"
pattern = "^eza$"
action = "deny"
message = "rewritten command denied"
label = "review/deny-rewritten"
"#,
    );

    let json = response(&fixture.run("pre-tool-use", &payload("ls", "")));
    assert_eq!(json["decision"], "deny");
    assert_eq!(json["reason"], "rewritten command denied");
    assert_eq!(json["updated_input"]["command"], "eza");
}

#[test]
fn opencode_chain_rewrite_feeds_generic_rewrite() {
    let fixture = Fixture::new(
        r#"{"rules":[],"failure_learning":{"enabled":false}}"#,
        "[[rewrites]]\npattern = \"^ls$\"\nreplace = \"eza\"\n",
    );
    write_hook_config(
        &fixture,
        r#"
[[hooks]]
event = "pre-tool-use"
matcher = "Bash"
pattern = "^eza$"
action = "rewrite"
replace = "eza --icons"
label = "review/rewrite-rewritten"
"#,
    );

    let json = response(&fixture.run("pre-tool-use", &payload("ls", "")));
    assert_eq!(json["updated_input"]["command"], "eza --icons");
    assert_eq!(json["matched_rules"], json!(["review/rewrite-rewritten"]));
}

#[test]
fn opencode_malformed_tool_requests_exit_nonzero() {
    let fixture = Fixture::new(r#"{"rules":[],"failure_learning":{"enabled":false}}"#, "");
    let cases = [
        ("pre-tool-use", r#"{"tool_input":{"command":"ls"}}"#),
        ("pre-tool-use", r#"{"tool_name":"Bash","tool_input":{}}"#),
        (
            "post-tool-use",
            r#"{"tool_name":"Bash","tool_input":{"command":"ls"}}"#,
        ),
        (
            "post-tool-use",
            r#"{"tool_name":"Bash","tool_input":{"command":"ls"},"tool_response":{"exit_code":"0","output":7}}"#,
        ),
    ];

    for (event, request) in cases {
        let output = fixture.run(event, request);
        assert!(
            !output.status.success(),
            "expected malformed {event} request to fail: {request}"
        );
        assert!(!output.stderr.is_empty());
    }
}

#[test]
fn opencode_validation_reports_missing_declaration() {
    let (temp, home, project) = validation_fixture();
    let plugin = project.join(".opencode/plugins/coursers.ts");
    std::fs::create_dir_all(plugin.parent().unwrap()).unwrap();
    std::fs::write(&plugin, "export {}\n").unwrap();

    let output = run_validation(temp.path(), &home, &project);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("opencode-plugin.d.ts"));
}
