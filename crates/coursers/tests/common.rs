use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

pub fn coursers_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_coursers"))
}

pub fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/integration/fixtures")
        .join(name)
}

#[allow(dead_code)]
pub fn run_pre(payload_path: &Path, rules_path: &Path, state_path: &Path) -> Output {
    let payload = std::fs::read_to_string(payload_path).unwrap();
    run_hook("pre", &payload, rules_path, state_path)
}

pub fn run_post(payload_path: &Path, rules_path: &Path, state_path: &Path) -> Output {
    let payload = std::fs::read_to_string(payload_path).unwrap();
    run_hook("post", &payload, rules_path, state_path)
}

pub fn run_hook(subcommand: &str, payload: &str, rules_path: &Path, state_path: &Path) -> Output {
    let mut child = Command::new(coursers_bin())
        .arg(subcommand)
        .env("COURSERS_RULES", rules_path)
        .env("COURSERS_STATE", state_path)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("failed to spawn coursers");

    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(payload.as_bytes())
        .unwrap();
    child.wait_with_output().unwrap()
}
