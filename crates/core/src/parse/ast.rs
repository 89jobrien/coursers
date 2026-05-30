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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_returns_none_for_empty_string() {
        assert!(parse("").is_none());
    }

    #[test]
    fn parse_returns_none_for_whitespace_only() {
        assert!(parse("   ").is_none());
        assert!(parse("\t\n").is_none());
    }

    #[test]
    fn parse_simple_command() {
        let cmd = parse("cargo build").unwrap();
        assert_eq!(cmd.argv, vec!["cargo", "build"]);
    }

    #[test]
    fn parse_single_token() {
        let cmd = parse("ls").unwrap();
        assert_eq!(cmd.argv, vec!["ls"]);
    }

    #[test]
    fn parse_quoted_strings() {
        let cmd = parse(r#"echo "hello world""#).unwrap();
        assert_eq!(cmd.argv, vec!["echo", "hello world"]);
    }

    #[test]
    fn parse_single_quoted_strings() {
        let cmd = parse("echo 'hello world'").unwrap();
        assert_eq!(cmd.argv, vec!["echo", "hello world"]);
    }

    #[test]
    fn parse_escaped_characters() {
        let cmd = parse(r"echo hello\ world").unwrap();
        assert_eq!(cmd.argv, vec!["echo", "hello world"]);
    }

    #[test]
    fn parse_with_leading_trailing_whitespace() {
        let cmd = parse("  cargo build  ").unwrap();
        assert_eq!(cmd.argv, vec!["cargo", "build"]);
    }

    #[test]
    fn name_returns_first_element() {
        let cmd = parse("cargo build --release").unwrap();
        assert_eq!(cmd.name(), "cargo");
    }

    #[test]
    fn name_returns_empty_for_empty_argv() {
        let cmd = ShellCmd { argv: vec![] };
        assert_eq!(cmd.name(), "");
    }

    #[test]
    fn args_returns_tail() {
        let cmd = parse("cargo build --release").unwrap();
        assert_eq!(cmd.args(), &["build", "--release"]);
    }

    #[test]
    fn args_returns_empty_for_single_token() {
        let cmd = parse("ls").unwrap();
        assert!(cmd.args().is_empty());
    }

    #[test]
    fn args_returns_empty_for_empty_argv() {
        let cmd = ShellCmd { argv: vec![] };
        assert!(cmd.args().is_empty());
    }

    #[test]
    fn parse_complex_command() {
        let cmd = parse("git commit -m 'initial commit'").unwrap();
        assert_eq!(cmd.argv, vec!["git", "commit", "-m", "initial commit"]);
    }
}
