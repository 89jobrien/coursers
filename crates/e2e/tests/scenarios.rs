use serde::Deserialize;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use tempfile::{NamedTempFile, TempDir};

// ---------------------------------------------------------------------------
// Scenario schema
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct ScenarioFile {
    scenario: ScenarioMeta,
    pre: Option<PrePhase>,
    filter: Option<FilterPhase>,
    post_failures: Option<PostFailures>,
}

#[derive(Debug, Deserialize)]
struct ScenarioMeta {
    name: String,
    #[allow(dead_code)]
    description: Option<String>,
    coursers_rules_json: Option<String>,
    crs_filters_toml: Option<String>,
}

#[derive(Debug, Deserialize)]
struct PrePhase {
    command: String,
    expected_verdict: String,
}

#[derive(Debug, Deserialize)]
struct FilterPhase {
    command: String,
    output: String,
    exit_code: i32,
    expected_result: String,
}

#[derive(Debug, Deserialize)]
struct PostFailures {
    command: String,
    count: u32,
}

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

fn run_bin(bin: &str, subcommand: &str, payload: &str, envs: &[(&str, &str)]) -> Output {
    let mut cmd = Command::new(workspace_bin(bin));
    cmd.arg(subcommand)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for (k, v) in envs {
        cmd.env(k, v);
    }
    let mut child = cmd
        .spawn()
        .unwrap_or_else(|e| panic!("spawn {bin} {subcommand}: {e}"));
    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(payload.as_bytes())
        .unwrap();
    child.wait_with_output().unwrap()
}

fn pre_payload(cmd: &str) -> String {
    format!(r#"{{"tool_name":"Bash","tool_input":{{"command":{cmd:?}}}}}"#)
}

fn post_payload(cmd: &str, exit_code: i32) -> String {
    format!(
        r#"{{"tool_name":"Bash","tool_input":{{"command":{cmd:?}}},"tool_response":{{"exit_code":{exit_code}}}}}"#
    )
}

fn post_filter_payload(cmd: &str, output: &str, exit_code: i32) -> String {
    format!(
        r#"{{"tool_name":"Bash","tool_input":{{"command":{cmd:?}}},"tool_response":{{"output":{output:?},"exit_code":{exit_code}}}}}"#
    )
}

// ---------------------------------------------------------------------------
// Scenario runner
// ---------------------------------------------------------------------------

fn run_scenario(path: &Path) {
    let content =
        std::fs::read_to_string(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    let sf: ScenarioFile =
        toml::from_str(&content).unwrap_or_else(|e| panic!("parse {}: {e}", path.display()));
    let name = &sf.scenario.name;

    let tmp = TempDir::new().unwrap();
    let state_path = tmp.path().join("state.json");

    let rules_content = sf
        .scenario
        .coursers_rules_json
        .as_deref()
        .unwrap_or(r#"{"rules":[]}"#);
    let mut rules_file = NamedTempFile::new().unwrap();
    write!(rules_file, "{rules_content}").unwrap();

    let filters_file: Option<NamedTempFile> =
        sf.scenario.crs_filters_toml.as_ref().map(|toml_str| {
            let mut f = NamedTempFile::new().unwrap();
            write!(f, "{toml_str}").unwrap();
            f
        });

    let rules_path = rules_file.path().to_string_lossy().into_owned();
    let state_str = state_path.to_string_lossy().into_owned();
    let pre_envs: Vec<(&str, &str)> = vec![
        ("COURSERS_RULES", &rules_path),
        ("COURSERS_STATE", &state_str),
    ];

    // Phase: post_failures — record failures before testing pre-block
    if let Some(ref pf) = sf.post_failures {
        for _ in 0..pf.count {
            let payload = post_payload(&pf.command, 1);
            run_bin("coursers", "post", &payload, &pre_envs);
        }
    }

    // Phase: pre
    if let Some(ref pre) = sf.pre {
        let payload = pre_payload(&pre.command);
        let out = run_bin("coursers", "pre", &payload, &pre_envs);
        match pre.expected_verdict.as_str() {
            "block" => assert!(
                !out.status.success(),
                "[{name}] pre: expected block (non-zero exit), got {:?}",
                out.status
            ),
            "allow" => assert!(
                out.status.success(),
                "[{name}] pre: expected allow (exit 0), got {:?}\nstderr: {}",
                out.status,
                String::from_utf8_lossy(&out.stderr)
            ),
            other => panic!("[{name}] unknown expected_verdict: {other:?}"),
        }
    }

    // Phase: filter
    if let Some(ref fil) = sf.filter {
        let filter_path: Option<String> = filters_file
            .as_ref()
            .map(|f| f.path().to_string_lossy().into_owned());
        let filter_envs: Vec<(&str, &str)> = filter_path
            .as_deref()
            .map(|p| vec![("CRS_FILTERS", p)])
            .unwrap_or_default();

        let payload = post_filter_payload(&fil.command, &fil.output, fil.exit_code);
        let out = run_bin("crs", "filter", &payload, &filter_envs);
        assert!(
            out.status.success(),
            "[{name}] filter: expected exit 0, got {:?}",
            out.status
        );
        let stdout = String::from_utf8_lossy(&out.stdout);
        match fil.expected_result.as_str() {
            "suppress" => assert!(
                stdout.contains(r#""message":""#),
                "[{name}] filter: expected suppress (empty message), got: {stdout}"
            ),
            "passthrough" => assert!(
                stdout.is_empty() || !stdout.contains(r#""message""#),
                "[{name}] filter: expected passthrough (no stdout), got: {stdout}"
            ),
            "replace" => assert!(
                stdout.contains(r#""message""#) && !stdout.contains(r#""message":""#),
                "[{name}] filter: expected replace (non-empty message), got: {stdout}"
            ),
            other => panic!("[{name}] unknown expected_result: {other:?}"),
        }
    }
}

// ---------------------------------------------------------------------------
// Test entry point
// ---------------------------------------------------------------------------

fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/scenarios")
}

#[test]
fn all_scenarios_pass() {
    let dir = fixtures_dir();
    let mut entries: Vec<_> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("read fixtures dir {}: {e}", dir.display()))
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|x| x == "toml"))
        .collect();
    assert!(
        !entries.is_empty(),
        "no scenario fixtures found in {}",
        dir.display()
    );
    entries.sort_by_key(|e| e.path());
    for entry in entries {
        run_scenario(&entry.path());
    }
}
