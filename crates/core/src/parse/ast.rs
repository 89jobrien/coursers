/// Parsed shell command — argv[0] is the command name, rest are arguments.
#[derive(Debug, Clone, PartialEq)]
pub struct ShellCmd {
    pub argv: Vec<String>,
}

impl ShellCmd {
    pub fn name(&self) -> &str {
        self.argv.first().map(|s| s.as_str()).unwrap_or("")
    }
    pub fn args(&self) -> &[String] {
        if self.argv.is_empty() {
            &[]
        } else {
            &self.argv[1..]
        }
    }
}

/// Parse a shell command string into argv. Returns None on parse error or empty input.
pub fn parse(cmd: &str) -> Option<ShellCmd> {
    let argv = shell_words::split(cmd.trim()).ok()?;
    if argv.is_empty() {
        return None;
    }
    Some(ShellCmd { argv })
}
