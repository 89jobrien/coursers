use crate::ast::parse;
use serde_json::{Value, json};

/// Default line count for head/tail when no -n flag is given (matches coreutils).
const DEFAULT_HEAD_TAIL_LINES: usize = 10;

/// Config for tool-swap behaviour, loaded from `[tool_swap]` in crs-filters.toml.
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(default)]
pub struct ToolSwapConfig {
    /// Token budget for bare `cat <file>`. Default: 4000.
    pub cat_token_limit: usize,
    /// Refuse tail→Read swap if N lines requested exceeds this. Default: 500.
    pub tail_limit_max: usize,
    /// Refuse find→Glob swap if -maxdepth exceeds this. Default: 10.
    pub find_depth_max: usize,
}

impl Default for ToolSwapConfig {
    fn default() -> Self {
        Self {
            cat_token_limit: 4000,
            tail_limit_max: 500,
            find_depth_max: 10,
        }
    }
}

/// The result of attempting a tool swap.
#[derive(Debug, PartialEq)]
pub enum ToolAction {
    /// No swap — let the command run unchanged.
    Passthrough,
    /// Redirect to a different Claude Code tool entirely.
    SwapTool {
        tool_name: String,
        tool_input: Value,
    },
}

/// Attempt to swap `cmd` to a Claude Code tool call.
/// Returns `Passthrough` if no swap applies.
pub fn apply(cmd: &str, config: &ToolSwapConfig) -> ToolAction {
    let Some(parsed) = parse(cmd) else {
        return ToolAction::Passthrough;
    };

    match parsed.name() {
        "cat" => swap_cat(parsed.args(), config),
        "head" => swap_head(parsed.args(), config),
        "tail" => swap_tail(parsed.args(), config),
        "find" => swap_find(parsed.args(), config),
        _ => ToolAction::Passthrough,
    }
}

// ---------------------------------------------------------------------------
// cat
// ---------------------------------------------------------------------------

fn swap_cat(args: &[String], config: &ToolSwapConfig) -> ToolAction {
    // Only handle single-file, no-flag cat. Flags or multiple files → passthrough.
    let file = match args {
        [f] if !f.starts_with('-') => f.as_str(),
        _ => return ToolAction::Passthrough,
    };

    let file_path = expand_path(file);
    let mut input = json!({ "file_path": file_path });

    // If file exists and is over token budget, add a line limit.
    if let Some(limit) = compute_line_limit(&file_path, config.cat_token_limit) {
        input["limit"] = json!(limit);
    }

    ToolAction::SwapTool {
        tool_name: "Read".to_string(),
        tool_input: input,
    }
}

// ---------------------------------------------------------------------------
// head
// ---------------------------------------------------------------------------

fn swap_head(args: &[String], config: &ToolSwapConfig) -> ToolAction {
    let Some((n, file)) = parse_n_file(args, DEFAULT_HEAD_TAIL_LINES) else {
        return ToolAction::Passthrough;
    };

    let file_path = expand_path(&file);
    let limit = clamp_to_token_budget(n, &file_path, config.cat_token_limit);

    ToolAction::SwapTool {
        tool_name: "Read".to_string(),
        tool_input: json!({
            "file_path": file_path,
            "limit": limit,
        }),
    }
}

// ---------------------------------------------------------------------------
// tail
// ---------------------------------------------------------------------------

fn swap_tail(args: &[String], config: &ToolSwapConfig) -> ToolAction {
    let Some((n, file)) = parse_n_file(args, DEFAULT_HEAD_TAIL_LINES) else {
        return ToolAction::Passthrough;
    };

    if n > config.tail_limit_max {
        return ToolAction::Passthrough;
    }

    let file_path = expand_path(&file);

    // Count lines in file to compute offset.
    let total_lines = count_lines(&file_path).unwrap_or(0);
    let offset = total_lines.saturating_sub(n);
    let limit = clamp_to_token_budget(n, &file_path, config.cat_token_limit);

    ToolAction::SwapTool {
        tool_name: "Read".to_string(),
        tool_input: json!({
            "file_path": file_path,
            "offset": offset,
            "limit": limit,
        }),
    }
}

// ---------------------------------------------------------------------------
// find
// ---------------------------------------------------------------------------

fn swap_find(args: &[String], config: &ToolSwapConfig) -> ToolAction {
    // Only handle: find <path> -name <glob>
    // Refuse if -exec, -delete, -newer, -mtime, -atime, -ctime, -size, -perm, -empty, -L present.
    let complex_flags = [
        "-exec", "-delete", "-newer", "-mtime", "-atime", "-ctime", "-size", "-perm", "-empty",
        "-L",
    ];
    if args.iter().any(|a| complex_flags.contains(&a.as_str())) {
        return ToolAction::Passthrough;
    }

    // Check maxdepth
    if let Some(pos) = args.iter().position(|a| a == "-maxdepth")
        && let Some(d) = args.get(pos + 1).and_then(|v| v.parse::<usize>().ok())
        && d > config.find_depth_max
    {
        return ToolAction::Passthrough;
    }

    // Extract path (first non-flag arg) and -name value
    let search_path = args
        .iter()
        .find(|a| !a.starts_with('-'))
        .map(|s| s.as_str())
        .unwrap_or(".");
    let name_glob = args
        .iter()
        .position(|a| a == "-name")
        .and_then(|i| args.get(i + 1))
        .map(|s| s.as_str());

    let Some(glob) = name_glob else {
        return ToolAction::Passthrough;
    };

    // Build glob pattern: path + / + glob
    let pattern = if search_path == "." {
        format!("**/{glob}")
    } else {
        let base = search_path.trim_end_matches('/');
        format!("{base}/**/{glob}")
    };

    ToolAction::SwapTool {
        tool_name: "Glob".to_string(),
        tool_input: json!({ "pattern": pattern }),
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Parse `-n N file` or `-N file` or `file` from args. Returns (n, filepath).
/// Uses `default_n` if no -n flag present.
fn parse_n_file(args: &[String], default_n: usize) -> Option<(usize, String)> {
    let mut n = default_n;
    let mut file: Option<String> = None;
    let mut i = 0;

    while i < args.len() {
        let a = &args[i];
        if a == "-n" || a == "--lines" {
            n = args.get(i + 1)?.parse().ok()?;
            i += 2;
        } else if let Some(num) = a.strip_prefix('-').and_then(|s| s.parse::<usize>().ok()) {
            n = num;
            i += 1;
        } else if !a.starts_with('-') {
            file = Some(a.clone());
            i += 1;
        } else {
            // Unknown flag — bail
            return None;
        }
    }

    Some((n, file?))
}

/// Expand ~ and $HOME in a path string.
fn expand_path(path: &str) -> String {
    if path.starts_with('~')
        && let Some(home) = dirs::home_dir()
    {
        return path.replacen('~', &home.to_string_lossy(), 1);
    }
    if path.starts_with("$HOME")
        && let Some(home) = dirs::home_dir()
    {
        return path.replacen("$HOME", &home.to_string_lossy(), 1);
    }
    path.to_string()
}

/// Count lines in a file by reading it. Returns None if file unreadable.
fn count_lines(path: &str) -> Option<usize> {
    let content = std::fs::read(path).ok()?;
    Some(content.iter().filter(|&&b| b == b'\n').count())
}

/// Approximate bytes per token (GPT/Claude tokenizer average).
const BYTES_PER_TOKEN: u64 = 4;

/// Fallback average bytes per line when sampling fails.
const DEFAULT_AVG_BYTES_PER_LINE: usize = 80;

/// Number of sample lines to read for avg-bytes-per-line estimation.
const LINE_SAMPLE_SIZE: usize = 20;

/// Estimate token count from file size.
fn estimate_tokens_from_size(bytes: u64) -> usize {
    (bytes / BYTES_PER_TOKEN) as usize
}

/// Sample avg bytes per line from up to `LINE_SAMPLE_SIZE` lines of a file.
fn avg_bytes_per_line(path: &str) -> Option<usize> {
    use std::io::{BufRead, BufReader};
    let f = std::fs::File::open(path).ok()?;
    let reader = BufReader::new(f);
    let mut total_bytes = 0usize;
    let mut count = 0usize;
    for line in reader.lines().take(LINE_SAMPLE_SIZE) {
        let line = line.ok()?;
        total_bytes += line.len() + 1; // +1 for newline
        count += 1;
    }
    if count == 0 {
        return None;
    }
    Some(total_bytes.div_ceil(count))
}

/// If the file exceeds the token budget, return a line limit. Otherwise None (read whole file).
fn compute_line_limit(path: &str, token_limit: usize) -> Option<usize> {
    let size = std::fs::metadata(path).ok()?.len();
    if estimate_tokens_from_size(size) <= token_limit {
        return None; // under budget — read whole file
    }
    let avg = avg_bytes_per_line(path).unwrap_or(DEFAULT_AVG_BYTES_PER_LINE);
    let limit = (token_limit * BYTES_PER_TOKEN as usize) / avg.max(1);
    Some(limit.max(1))
}

/// Clamp requested N lines to token budget.
fn clamp_to_token_budget(n: usize, path: &str, token_limit: usize) -> usize {
    let max = compute_line_limit(path, token_limit).unwrap_or(n);
    n.min(max)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(kani)]
mod kani_proofs {
    use super::*;

    /// Proof: estimate_tokens_from_size is monotonic.
    #[kani::proof]
    #[kani::unwind(1)]
    fn estimate_tokens_monotonic() {
        let a: u64 = kani::any();
        let b: u64 = kani::any();
        kani::assume(a <= b);
        assert!(estimate_tokens_from_size(a) <= estimate_tokens_from_size(b));
    }

    /// Proof: parse_n_file returns default_n when no -n flag is present.
    #[kani::proof]
    #[kani::unwind(20)]
    fn parse_n_file_default_when_no_flag() {
        // A single non-flag argument should return (default_n, that_arg)
        let args = vec!["somefile.txt".to_string()];
        let result = parse_n_file(&args, DEFAULT_HEAD_TAIL_LINES);
        assert!(result.is_some());
        let (n, file) = result.unwrap();
        assert!(n == DEFAULT_HEAD_TAIL_LINES);
        assert!(file == "somefile.txt");
    }

    /// Proof: parse_n_file with -n flag returns the specified count.
    #[kani::proof]
    #[kani::unwind(20)]
    fn parse_n_file_explicit_count() {
        let args = vec!["-n".to_string(), "42".to_string(), "file.rs".to_string()];
        let result = parse_n_file(&args, DEFAULT_HEAD_TAIL_LINES);
        assert!(result.is_some());
        let (n, file) = result.unwrap();
        assert!(n == 42);
        assert!(file == "file.rs");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write as _;

    fn cfg() -> ToolSwapConfig {
        ToolSwapConfig::default()
    }

    fn write_lines(n: usize) -> tempfile::NamedTempFile {
        let mut f = tempfile::NamedTempFile::new().unwrap();
        for i in 0..n {
            writeln!(f, "line {i}").unwrap();
        }
        f
    }

    #[test]
    fn cat_single_file_small() {
        let f = write_lines(5);
        let cmd = format!("cat {}", f.path().display());
        let action = apply(&cmd, &cfg());
        assert!(
            matches!(action, ToolAction::SwapTool { ref tool_name, .. } if tool_name == "Read")
        );
        if let ToolAction::SwapTool { tool_input, .. } = action {
            assert_eq!(
                tool_input["file_path"].as_str().unwrap(),
                f.path().to_str().unwrap()
            );
            assert!(
                tool_input.get("limit").is_none(),
                "small file should have no limit"
            );
        }
    }

    #[test]
    fn cat_with_flags_passthrough() {
        assert_eq!(apply("cat -n file.txt", &cfg()), ToolAction::Passthrough);
    }

    #[test]
    fn cat_multiple_files_passthrough() {
        assert_eq!(apply("cat a.txt b.txt", &cfg()), ToolAction::Passthrough);
    }

    #[test]
    fn head_n_flag() {
        let f = write_lines(100);
        let cmd = format!("head -n 20 {}", f.path().display());
        let action = apply(&cmd, &cfg());
        if let ToolAction::SwapTool {
            tool_name,
            tool_input,
        } = action
        {
            assert_eq!(tool_name, "Read");
            assert_eq!(tool_input["limit"].as_u64().unwrap(), 20);
        } else {
            panic!("expected SwapTool");
        }
    }

    #[test]
    fn head_short_flag() {
        let f = write_lines(100);
        let cmd = format!("head -5 {}", f.path().display());
        let action = apply(&cmd, &cfg());
        if let ToolAction::SwapTool { tool_input, .. } = action {
            assert_eq!(tool_input["limit"].as_u64().unwrap(), 5);
        } else {
            panic!("expected SwapTool");
        }
    }

    #[test]
    fn tail_computes_offset() {
        let f = write_lines(50);
        let cmd = format!("tail -n 10 {}", f.path().display());
        let action = apply(&cmd, &cfg());
        if let ToolAction::SwapTool { tool_input, .. } = action {
            assert_eq!(tool_input["offset"].as_u64().unwrap(), 40);
            assert_eq!(tool_input["limit"].as_u64().unwrap(), 10);
        } else {
            panic!("expected SwapTool");
        }
    }

    #[test]
    fn tail_exceeds_limit_max_passthrough() {
        let f = write_lines(10);
        let cmd = format!("tail -n 600 {}", f.path().display());
        let cfg = ToolSwapConfig {
            tail_limit_max: 500,
            ..Default::default()
        };
        assert_eq!(apply(&cmd, &cfg), ToolAction::Passthrough);
    }

    #[test]
    fn find_name_glob() {
        let action = apply("find . -name '*.rs'", &cfg());
        if let ToolAction::SwapTool {
            tool_name,
            tool_input,
        } = action
        {
            assert_eq!(tool_name, "Glob");
            assert_eq!(tool_input["pattern"].as_str().unwrap(), "**/*.rs");
        } else {
            panic!("expected SwapTool");
        }
    }

    #[test]
    fn find_with_path() {
        let action = apply("find /Users/joe/dev -name '*.toml'", &cfg());
        if let ToolAction::SwapTool { tool_input, .. } = action {
            assert_eq!(
                tool_input["pattern"].as_str().unwrap(),
                "/Users/joe/dev/**/*.toml"
            );
        } else {
            panic!("expected SwapTool");
        }
    }

    #[test]
    fn find_exec_passthrough() {
        assert_eq!(
            apply("find . -name '*.rs' -exec cat {} \\;", &cfg()),
            ToolAction::Passthrough
        );
    }

    #[test]
    fn find_no_name_passthrough() {
        assert_eq!(apply("find . -type f", &cfg()), ToolAction::Passthrough);
    }

    #[test]
    fn unknown_command_passthrough() {
        assert_eq!(apply("git status", &cfg()), ToolAction::Passthrough);
    }

    #[test]
    fn empty_command_passthrough() {
        assert_eq!(apply("", &cfg()), ToolAction::Passthrough);
    }

    #[test]
    fn config_defaults() {
        let cfg = ToolSwapConfig::default();
        assert_eq!(cfg.cat_token_limit, 4000);
        assert_eq!(cfg.tail_limit_max, 500);
        assert_eq!(cfg.find_depth_max, 10);
    }
}
