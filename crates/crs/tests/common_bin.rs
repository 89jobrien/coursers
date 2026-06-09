use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};

pub fn crs_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_crs"))
}

pub fn run_crs(subcommand: &str, payload: &str, envs: &[(&str, &str)]) -> Output {
    let mut cmd = Command::new(crs_bin());
    cmd.arg(subcommand)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for (k, v) in envs {
        cmd.env(k, v);
    }
    let mut child = cmd.spawn().expect("failed to spawn crs");
    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(payload.as_bytes())
        .unwrap();
    child.wait_with_output().unwrap()
}

#[test]
fn crs_bin_exists() {
    assert!(crs_bin().exists(), "crs binary not found: {:?}", crs_bin());
}
