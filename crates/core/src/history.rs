/// Extracts the command stem (1–2 token prefix) used for frequency grouping.
///
/// Rules:
/// 1. Strip leading `KEY=val` env assignments.
/// 2. Strip path prefix from token 0 (keep only the basename).
/// 3. If token 1 exists and does not start with `-`, append it: `cargo nextest`.
///    Otherwise stem = token 0 only.
pub fn stem_of(command: &str) -> String {
    let tokens: Vec<&str> = command.split_whitespace().collect();
    if tokens.is_empty() {
        return String::new();
    }

    // Strip leading KEY=val env assignments
    let start = tokens.iter().take_while(|t| t.contains('=') && !t.starts_with('-')).count();
    let tokens = &tokens[start..];
    if tokens.is_empty() {
        return String::new();
    }

    // Strip path prefix from token 0
    let base = tokens[0].rsplit('/').next().unwrap_or(tokens[0]);

    // Append token 1 if it exists, is not a flag, AND token 2 exists
    if tokens.len() > 2 {
        if let Some(t1) = tokens.get(1) {
            if !t1.starts_with('-') {
                return format!("{base} {t1}");
            }
        }
    }

    base.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stem_bare_command() {
        assert_eq!(stem_of("ls -la"), "ls");
    }

    #[test]
    fn stem_two_token_subcommand() {
        assert_eq!(stem_of("cargo nextest run -p crs-core"), "cargo nextest");
    }

    #[test]
    fn stem_subcommand_with_flag_token1() {
        assert_eq!(stem_of("git --no-pager log"), "git");
    }

    #[test]
    fn stem_strips_path_prefix() {
        assert_eq!(stem_of("/usr/bin/python3 script.py"), "python3");
    }

    #[test]
    fn stem_strips_env_assignment() {
        assert_eq!(stem_of("RUST_LOG=debug cargo build"), "cargo");
    }

    #[test]
    fn stem_strips_multiple_env_assignments() {
        assert_eq!(stem_of("A=1 B=2 cargo test"), "cargo");
    }

    #[test]
    fn stem_empty_command() {
        assert_eq!(stem_of(""), "");
    }

    #[test]
    fn stem_single_token() {
        assert_eq!(stem_of("make"), "make");
    }

    #[test]
    fn stem_doob_todo() {
        assert_eq!(stem_of("doob todo list --project coursers"), "doob todo");
    }
}
