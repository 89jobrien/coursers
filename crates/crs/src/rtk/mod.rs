pub mod process;

pub use process::ProcessRtkClient;

/// Returns a `ProcessRtkClient` if `rtk` is on PATH, otherwise `None`.
pub fn detect() -> Option<ProcessRtkClient> {
    which_rtk().map(|_| ProcessRtkClient)
}

fn which_rtk() -> Option<()> {
    std::process::Command::new("rtk")
        .arg("--version")
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|_| ())
}
