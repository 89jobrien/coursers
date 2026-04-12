pub mod process;

pub use process::ProcessObfsckMcpClient;

/// Returns a `ProcessObfsckMcpClient` if `obfsck-mcp` is on PATH, otherwise `None`.
pub fn detect() -> Option<ProcessObfsckMcpClient> {
    which_obfsck_mcp().map(|_| ProcessObfsckMcpClient)
}

fn which_obfsck_mcp() -> Option<()> {
    std::process::Command::new("obfsck-mcp")
        .arg("--help")
        .output()
        .ok()
        .map(|_| ())
}
