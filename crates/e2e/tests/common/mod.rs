use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

pub fn workspace_bin(name: &str) -> PathBuf {
    let executable = std::env::current_exe().expect("resolve current e2e test executable");
    let binary = binary_for_test_executable(&executable, name);
    assert!(
        binary.is_file(),
        "binary {name:?} not found for the active test profile — run `cargo build --package coursers --bins` first\nchecked: {}",
        binary.display()
    );
    binary
}

pub fn run_bin(bin: &str, args: &[&str], payload: &str, envs: &[(&str, &str)]) -> Output {
    let mut command = Command::new(workspace_bin(bin));
    command
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for (key, value) in envs {
        command.env(key, value);
    }

    let mut child = command
        .spawn()
        .unwrap_or_else(|error| panic!("spawn {bin} {}: {error}", args.join(" ")));
    child
        .stdin
        .as_mut()
        .expect("subprocess stdin must be piped")
        .write_all(payload.as_bytes())
        .unwrap_or_else(|error| panic!("write payload to {bin}: {error}"));
    child
        .wait_with_output()
        .unwrap_or_else(|error| panic!("wait for {bin}: {error}"))
}

pub(crate) fn profile_dir_from_test_executable(executable: &Path) -> PathBuf {
    let executable_dir = executable
        .parent()
        .expect("e2e test executable must have a parent directory");
    if executable_dir
        .file_name()
        .is_some_and(|name| name == "deps")
    {
        executable_dir
            .parent()
            .expect("Cargo deps directory must have a profile parent")
            .to_path_buf()
    } else {
        executable_dir.to_path_buf()
    }
}

pub(crate) fn binary_for_test_executable(executable: &Path, name: &str) -> PathBuf {
    binary_in_profile(&profile_dir_from_test_executable(executable), name)
}

fn binary_in_profile(profile_dir: &Path, name: &str) -> PathBuf {
    profile_dir.join(format!("{name}{}", std::env::consts::EXE_SUFFIX))
}
