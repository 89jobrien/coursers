/// Port: abstracts how shell variable references are resolved in a command string.
pub trait VarExpander {
    fn expand(&self, command: &str) -> String;
}

/// Production expander — resolves `$VAR`, `${VAR}`, `$env.VAR`, and `~` against
/// the real process environment.
pub struct EnvExpander;

impl VarExpander for EnvExpander {
    fn expand(&self, command: &str) -> String {
        expand_vars(command)
    }
}

/// No-op expander — returns the command unchanged. Use in tests or sandbox modes
/// where env expansion is undesirable.
pub struct NoopExpander;

impl VarExpander for NoopExpander {
    fn expand(&self, command: &str) -> String {
        command.to_string()
    }
}

/// Shell-agnostic environment variable expansion pass.
///
/// Resolves the following reference styles before a command is processed:
/// - `$VARNAME` and `${VARNAME}` — POSIX-style
/// - `$env.VARNAME` — Nushell style
/// - `~` as a path prefix (token starts with `~/` or is exactly `~`) — expands to `$HOME`
///
/// Expansion rules:
/// - Variables that do not resolve are left as-is (no error).
/// - `$$` is left as-is (shell PID placeholder).
/// - Tokens beginning with `http://` or `https://` are skipped entirely.
/// - References inside single-quoted tokens are not expanded (the token starts with `'`).
pub fn expand_vars(command: &str) -> String {
    expand_inline(command)
}

fn expand_inline(s: &str) -> String {
    let bytes = s.as_bytes();
    let len = bytes.len();
    let mut out = String::with_capacity(len);
    let mut i = 0;

    // Track whether we are inside a single-quoted region.
    let mut in_single_quote = false;

    // Track whether the current token might be a URL (starts with http:// or https://).
    // We reset this at whitespace boundaries.
    let mut token_start = true;
    let mut token_is_url = false;

    while i < len {
        let ch = bytes[i] as char;

        // Whitespace: reset token state.
        if ch == ' ' || ch == '\t' || ch == '\n' {
            out.push(ch);
            i += 1;
            token_start = true;
            token_is_url = false;
            // Single-quote state persists across tokens (shell keeps it open).
            continue;
        }

        // Detect URL tokens.
        if token_start {
            let rest = &s[i..];
            if rest.starts_with("http://") || rest.starts_with("https://") {
                token_is_url = true;
            }
            // Detect single-quote start.
            if ch == '\'' {
                in_single_quote = !in_single_quote;
                out.push(ch);
                i += 1;
                token_start = false;
                continue;
            }
            token_start = false;
        }

        // Toggle single-quote on unescaped `'`.
        if ch == '\'' {
            in_single_quote = !in_single_quote;
            out.push(ch);
            i += 1;
            continue;
        }

        // Inside single quotes or URL tokens: pass through verbatim.
        if in_single_quote || token_is_url {
            out.push(ch);
            i += 1;
            continue;
        }

        // Tilde expansion: `~` at start of token followed by `/` or end.
        if ch == '~' && (i + 1 == len || bytes[i + 1] == b'/' || bytes[i + 1] == b' ') {
            if let Ok(home) = std::env::var("HOME") {
                out.push_str(&home);
            } else {
                out.push('~');
            }
            i += 1;
            continue;
        }

        // `$` — start of variable reference.
        if ch == '$' {
            // `$$` — leave as-is.
            if i + 1 < len && bytes[i + 1] == b'$' {
                out.push_str("$$");
                i += 2;
                continue;
            }

            // `$env.VARNAME` (Nushell style).
            if s[i + 1..].starts_with("env.") {
                let var_start = i + 5; // skip '$', 'e', 'n', 'v', '.'
                let var_name = read_bare_name(&s[var_start..]);
                if !var_name.is_empty() {
                    match std::env::var(var_name) {
                        Ok(val) => out.push_str(&val),
                        Err(_) => {
                            out.push_str(&s[i..i + 5 + var_name.len()]);
                        }
                    }
                    i += 5 + var_name.len();
                    continue;
                }
            }

            // `${VARNAME}`.
            if i + 1 < len
                && bytes[i + 1] == b'{'
                && let Some(end) = s[i + 2..].find('}')
            {
                let var_name = &s[i + 2..i + 2 + end];
                match std::env::var(var_name) {
                    Ok(val) => out.push_str(&val),
                    Err(_) => {
                        out.push_str(&s[i..i + 2 + end + 1]);
                    }
                }
                i += 2 + end + 1; // skip past '}'
                continue;
            }

            // `$VARNAME` — bare name.
            let var_start = i + 1;
            let var_name = read_bare_name(&s[var_start..]);
            if !var_name.is_empty() {
                match std::env::var(var_name) {
                    Ok(val) => out.push_str(&val),
                    Err(_) => {
                        out.push('$');
                        out.push_str(var_name);
                    }
                }
                i += 1 + var_name.len();
                continue;
            }

            // Bare `$` with nothing valid after it — pass through.
            out.push('$');
            i += 1;
            continue;
        }

        out.push(ch);
        i += 1;
    }

    out
}

/// Read an identifier name from the start of `s` (stops at non-identifier char).
/// Returns a slice of `s`.
fn read_bare_name(s: &str) -> &str {
    let end = s
        .find(|c: char| !c.is_alphanumeric() && c != '_')
        .unwrap_or(s.len());
    &s[..end]
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static ENV_MUTEX: Mutex<()> = Mutex::new(());

    /// Set one or more env vars for the duration of a closure, then restore.
    /// Serializes all env-touching tests via ENV_MUTEX.
    macro_rules! with_env {
        ($(($k:expr, $v:expr)),+ => $body:expr) => {{
            let _guard = ENV_MUTEX.lock().unwrap();
            unsafe { $(std::env::set_var($k, $v);)+ }
            let _result = { $body };
            unsafe { $(std::env::remove_var($k);)+ }
            _result
        }};
    }

    #[test]
    fn expands_dollar_varname() {
        with_env!(("_CRS_TEST_FOO", "/test/foo") => {
            assert_eq!(expand_vars("echo $_CRS_TEST_FOO"), "echo /test/foo");
        });
    }

    #[test]
    fn expands_dollar_brace_varname() {
        with_env!(("_CRS_TEST_BAR", "/test/bar") => {
            assert_eq!(
                expand_vars("op run --env-file=${_CRS_TEST_BAR}/.secrets"),
                "op run --env-file=/test/bar/.secrets"
            );
        });
    }

    #[test]
    fn expands_nu_env_style() {
        with_env!(("_CRS_TEST_HOME", "/nu/home") => {
            assert_eq!(
                expand_vars("op run --env-file=$env._CRS_TEST_HOME/.secrets"),
                "op run --env-file=/nu/home/.secrets"
            );
        });
    }

    #[test]
    fn expands_tilde_slash() {
        with_env!(("HOME", "/home/joe") => {
            assert_eq!(expand_vars("op run --env-file=~/.secrets"), "op run --env-file=/home/joe/.secrets");
        });
    }

    #[test]
    fn expands_tilde_alone() {
        with_env!(("HOME", "/home/joe") => {
            assert_eq!(expand_vars("cd ~"), "cd /home/joe");
        });
    }

    #[test]
    fn does_not_expand_tilde_in_middle_of_word() {
        assert_eq!(expand_vars("echo foo~bar"), "echo foo~bar");
    }

    #[test]
    fn does_not_expand_double_dollar() {
        assert_eq!(expand_vars("echo $$"), "echo $$");
    }

    #[test]
    fn does_not_expand_inside_single_quotes() {
        assert_eq!(expand_vars("echo '$HOME'"), "echo '$HOME'");
    }

    #[test]
    fn does_not_expand_url_tokens() {
        let url = "https://example.com/$path";
        assert_eq!(expand_vars(url), url);
    }

    #[test]
    fn leaves_unresolved_var_as_is() {
        let _guard = ENV_MUTEX.lock().unwrap();
        unsafe { std::env::remove_var("_CRS_DEFINITELY_NOT_SET_XYZ") };
        assert_eq!(
            expand_vars("echo $_CRS_DEFINITELY_NOT_SET_XYZ"),
            "echo $_CRS_DEFINITELY_NOT_SET_XYZ"
        );
    }

    #[test]
    fn leaves_unresolved_brace_var_as_is() {
        let _guard = ENV_MUTEX.lock().unwrap();
        unsafe { std::env::remove_var("_CRS_DEFINITELY_NOT_SET_ABC") };
        assert_eq!(
            expand_vars("echo ${_CRS_DEFINITELY_NOT_SET_ABC}"),
            "echo ${_CRS_DEFINITELY_NOT_SET_ABC}"
        );
    }

    #[test]
    fn passthrough_plain_command() {
        assert_eq!(
            expand_vars("cargo build --release"),
            "cargo build --release"
        );
    }

    #[test]
    fn expands_multiple_vars_in_one_command() {
        with_env!(("_CRS_TEST_A", "aaa"), ("_CRS_TEST_B", "bbb") => {
            assert_eq!(
                expand_vars("echo $_CRS_TEST_A $_CRS_TEST_B"),
                "echo aaa bbb"
            );
        });
    }
}
