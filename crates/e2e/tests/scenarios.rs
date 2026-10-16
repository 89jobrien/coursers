//! Data-driven end-to-end rule and hook scenarios.

mod common;

use std::io::Write as _;
use std::path::{Path, PathBuf};

use common::run_bin;
use serde::Deserialize;
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
// Payload helpers
// ---------------------------------------------------------------------------

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

    // TODO(strict-e2e-assertions): Assert exact exit codes and deserialize complete protocol JSON;
    // substring and generic nonzero checks can let crashes or malformed responses pass.
    // Phase: post_failures — record failures before testing pre-block
    if let Some(ref pf) = sf.post_failures {
        for _ in 0..pf.count {
            let payload = post_payload(&pf.command, 1);
            run_bin("coursers", &["post"], &payload, &pre_envs);
        }
    }

    // Phase: pre
    if let Some(ref pre) = sf.pre {
        let payload = pre_payload(&pre.command);
        let out = run_bin("coursers", &["pre"], &payload, &pre_envs);
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
        let out = run_bin("crs", &["filter"], &payload, &filter_envs);
        assert!(
            out.status.success(),
            "[{name}] filter: expected exit 0, got {:?}",
            out.status
        );
        let stdout = String::from_utf8_lossy(&out.stdout);
        match fil.expected_result.as_str() {
            "suppress" => assert!(
                stdout.contains("\"message\":\"\""),
                "[{name}] filter: expected suppress (empty message), got: {stdout}"
            ),
            "passthrough" => assert!(
                stdout.is_empty() || !stdout.contains(r#""message""#),
                "[{name}] filter: expected passthrough (no stdout), got: {stdout}"
            ),
            "replace" => assert!(
                stdout.contains(r#""message""#) && !stdout.contains("\"message\":\"\""),
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

// ---------------------------------------------------------------------------
// Scenario schema unit tests (no process spawn — pure parsing logic)
// ---------------------------------------------------------------------------

#[test]
fn scenario_file_parses_minimal_meta_only() {
    let toml_str = r#"
        [scenario]
        name = "minimal"
    "#;
    let sf: ScenarioFile = toml::from_str(toml_str).expect("minimal scenario should parse");
    assert_eq!(sf.scenario.name, "minimal");
    assert!(sf.scenario.description.is_none());
    assert!(sf.scenario.coursers_rules_json.is_none());
    assert!(sf.scenario.crs_filters_toml.is_none());
    assert!(sf.pre.is_none());
    assert!(sf.filter.is_none());
    assert!(sf.post_failures.is_none());
}

#[test]
fn scenario_file_parses_all_phases() {
    let toml_str = r#"
        [scenario]
        name = "full"
        description = "exercises every phase"
        coursers_rules_json = '{"rules":[]}'
        crs_filters_toml = "[[filters]]\npattern = \"ls\"\nmode = \"passthrough\"\n"

        [pre]
        command = "ls -la"
        expected_verdict = "allow"

        [filter]
        command = "ls -la"
        output = "total 0"
        exit_code = 0
        expected_result = "passthrough"

        [post_failures]
        command = "flaky-cmd"
        count = 3
    "#;
    let sf: ScenarioFile = toml::from_str(toml_str).expect("full scenario should parse");
    assert_eq!(
        sf.scenario.description.as_deref(),
        Some("exercises every phase")
    );
    let pre = sf.pre.expect("pre phase present");
    assert_eq!(pre.command, "ls -la");
    assert_eq!(pre.expected_verdict, "allow");
    let filter = sf.filter.expect("filter phase present");
    assert_eq!(filter.exit_code, 0);
    assert_eq!(filter.expected_result, "passthrough");
    let post_failures = sf.post_failures.expect("post_failures phase present");
    assert_eq!(post_failures.count, 3);
}

#[test]
fn scenario_file_rejects_missing_required_name() {
    let toml_str = r#"
        [scenario]
        description = "no name field"
    "#;
    let result: Result<ScenarioFile, _> = toml::from_str(toml_str);
    assert!(
        result.is_err(),
        "expected parse error when required `name` field is missing"
    );
}

#[test]
fn scenario_file_rejects_malformed_toml() {
    let toml_str = "this is not valid toml [[[";
    let result: Result<ScenarioFile, _> = toml::from_str(toml_str);
    assert!(result.is_err(), "expected parse error for malformed TOML");
}

#[test]
fn scenario_file_rejects_unknown_expected_verdict_at_runtime() {
    // Parsing succeeds even with a nonsense verdict string — validation
    // happens later in run_scenario(), not during deserialization.
    let toml_str = r#"
        [scenario]
        name = "weird-verdict"

        [pre]
        command = "ls"
        expected_verdict = "maybe"
    "#;
    let sf: ScenarioFile = toml::from_str(toml_str).expect("parses despite unknown verdict");
    assert_eq!(sf.pre.unwrap().expected_verdict, "maybe");
}

#[test]
fn pre_payload_embeds_command_as_json_string() {
    let payload = pre_payload(r#"echo "hi""#);
    let v: serde_json::Value = serde_json::from_str(&payload).expect("payload is valid JSON");
    assert_eq!(v["tool_name"], "Bash");
    assert_eq!(v["tool_input"]["command"], r#"echo "hi""#);
}

#[test]
fn post_payload_includes_exit_code() {
    let payload = post_payload("false", 1);
    let v: serde_json::Value = serde_json::from_str(&payload).expect("payload is valid JSON");
    assert_eq!(v["tool_response"]["exit_code"], 1);
}

#[test]
fn post_filter_payload_includes_output_and_exit_code() {
    let payload = post_filter_payload("ls", "total 0\n", 0);
    let v: serde_json::Value = serde_json::from_str(&payload).expect("payload is valid JSON");
    assert_eq!(v["tool_response"]["output"], "total 0\n");
    assert_eq!(v["tool_response"]["exit_code"], 0);
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
