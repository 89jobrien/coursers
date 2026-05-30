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

/// Number of bytes in the `$env.` prefix (`$`, `e`, `n`, `v`, `.`).
const NU_ENV_PREFIX_LEN: usize = 5;

fn expand_inline(s: &str) -> String {
    let bytes = s.as_bytes();
    let len = bytes.len();
    let mut out = String::with_capacity(len);
    let mut i = 0;
    let mut in_single_quote = false;
    let mut token_start = true;
    let mut token_is_url = false;

    while i < len {
        let ch = bytes[i] as char;

        if is_whitespace(ch) {
            out.push(ch);
            i += 1;
            token_start = true;
            token_is_url = false;
            continue;
        }

        if token_start {
            token_is_url = detect_url_token(&s[i..]);
            if ch == '\'' {
                in_single_quote = !in_single_quote;
                out.push(ch);
                i += 1;
                token_start = false;
                continue;
            }
            token_start = false;
        }

        if ch == '\'' {
            in_single_quote = !in_single_quote;
            out.push(ch);
            i += 1;
            continue;
        }

        if in_single_quote || token_is_url {
            out.push(ch);
            i += 1;
            continue;
        }

        if ch == '~' {
            let (expanded, consumed) = expand_tilde(bytes, i, len, s);
            out.push_str(&expanded);
            i += consumed;
            continue;
        }

        if ch == '$' {
            let (expanded, consumed) = expand_dollar(s, bytes, i, len);
            out.push_str(&expanded);
            i += consumed;
            continue;
        }

        out.push(ch);
        i += 1;
    }

    out
}

#[inline]
fn is_whitespace(ch: char) -> bool {
    ch == ' ' || ch == '\t' || ch == '\n'
}

#[inline]
fn detect_url_token(rest: &str) -> bool {
    rest.starts_with("http://") || rest.starts_with("https://")
}

/// Attempt tilde expansion at position `i`. Returns `(replacement, bytes_consumed)`.
fn expand_tilde(bytes: &[u8], i: usize, len: usize, _s: &str) -> (String, usize) {
    let at_end = i + 1 == len;
    let followed_by_slash_or_space = !at_end && (bytes[i + 1] == b'/' || bytes[i + 1] == b' ');
    if bytes[i] == b'~' && (at_end || followed_by_slash_or_space) {
        let replacement = std::env::var("HOME").unwrap_or_else(|_| "~".to_string());
        (replacement, 1)
    } else {
        ("~".to_string(), 1)
    }
}

/// Expand a `$`-prefixed variable reference at position `i`.
/// Returns `(replacement_text, bytes_consumed)`.
fn expand_dollar(s: &str, bytes: &[u8], i: usize, len: usize) -> (String, usize) {
    // `$$` — leave as-is.
    if i + 1 < len && bytes[i + 1] == b'$' {
        return ("$$".to_string(), 2);
    }

    // `$env.VARNAME` (Nushell style).
    if let Some((text, consumed)) = expand_dollar_nu_env(s, i) {
        return (text, consumed);
    }

    // `${VARNAME}`.
    if let Some((text, consumed)) = expand_dollar_brace(s, bytes, i, len) {
        return (text, consumed);
    }

    // `$VARNAME` — bare name.
    if let Some((text, consumed)) = expand_dollar_bare(s, i) {
        return (text, consumed);
    }

    // Bare `$` with nothing valid after it — pass through.
    ("$".to_string(), 1)
}

/// Expand `$env.VARNAME` (Nushell style). Returns `Some((text, consumed))` on match.
fn expand_dollar_nu_env(s: &str, i: usize) -> Option<(String, usize)> {
    if !s[i + 1..].starts_with("env.") {
        return None;
    }
    let var_start = i + NU_ENV_PREFIX_LEN;
    let var_name = read_bare_name(&s[var_start..]);
    if var_name.is_empty() {
        return None;
    }
    let consumed = NU_ENV_PREFIX_LEN + var_name.len();
    let text = match std::env::var(var_name) {
        Ok(val) => val,
        Err(_) => s[i..i + consumed].to_string(),
    };
    Some((text, consumed))
}

/// Expand `${VARNAME}`. Returns `Some((text, consumed))` on match.
fn expand_dollar_brace(s: &str, bytes: &[u8], i: usize, len: usize) -> Option<(String, usize)> {
    if i + 1 >= len || bytes[i + 1] != b'{' {
        return None;
    }
    let end = s[i + 2..].find('}')?;
    let var_name = &s[i + 2..i + 2 + end];
    let consumed = 2 + end + 1; // `${` + name + `}`
    let text = match std::env::var(var_name) {
        Ok(val) => val,
        Err(_) => s[i..i + consumed].to_string(),
    };
    Some((text, consumed))
}

/// Expand `$VARNAME` (bare). Returns `Some((text, consumed))` on match.
fn expand_dollar_bare(s: &str, i: usize) -> Option<(String, usize)> {
    let var_name = read_bare_name(&s[i + 1..]);
    if var_name.is_empty() {
        return None;
    }
    let consumed = 1 + var_name.len();
    let text = match std::env::var(var_name) {
        Ok(val) => val,
        Err(_) => format!("${var_name}"),
    };
    Some((text, consumed))
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
    fn expand_vars_dollar_varname() {
        let result = with_env!(("_CRS_TEST_FOO", "/test/foo") => {
            expand_vars("echo $_CRS_TEST_FOO")
        });
        assert_eq!(result, "echo /test/foo");
    }

    #[test]
    fn expand_vars_dollar_brace_varname() {
        let result = with_env!(("_CRS_TEST_BAR", "/test/bar") => {
            expand_vars("op run --env-file=${_CRS_TEST_BAR}/.secrets")
        });
        assert_eq!(result, "op run --env-file=/test/bar/.secrets");
    }

    #[test]
    fn expand_vars_nu_env_style() {
        let result = with_env!(("_CRS_TEST_HOME", "/nu/home") => {
            expand_vars("op run --env-file=$env._CRS_TEST_HOME/.secrets")
        });
        assert_eq!(result, "op run --env-file=/nu/home/.secrets");
    }

    #[test]
    fn expand_vars_tilde_slash() {
        let result = with_env!(("HOME", "/home/joe") => {
            expand_vars("op run --env-file=~/.secrets")
        });
        assert_eq!(result, "op run --env-file=/home/joe/.secrets");
    }

    #[test]
    fn expand_vars_tilde_alone() {
        let result = with_env!(("HOME", "/home/joe") => {
            expand_vars("cd ~")
        });
        assert_eq!(result, "cd /home/joe");
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
    fn expand_vars_multiple_in_one_command() {
        let result = with_env!(("_CRS_TEST_A", "aaa"), ("_CRS_TEST_B", "bbb") => {
            expand_vars("echo $_CRS_TEST_A $_CRS_TEST_B")
        });
        assert_eq!(result, "echo aaa bbb");
    }
}
