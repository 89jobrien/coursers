use clap::{Parser, Subcommand};
use crs_lib::{run_filter, run_rewrite, FilterPayload, FilterResult};
use serde::Deserialize;
use serde_json::Value;
use std::io::{self, Read, Write};

#[derive(Parser)]
#[command(name = "crs", about = "Command rewriter and output filter")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Filter PostToolUse output — reads hook JSON from stdin, emits hook response to stdout
    Filter,
    /// Rewrite a PreToolUse command — reads hook JSON from stdin, emits rewritten command or exits 1
    Rewrite,
    /// Discover missed savings from Claude Code session history
    Discover {
        /// Scan all projects (default: current project only)
        #[arg(short, long)]
        all: bool,
        /// Max rows per section
        #[arg(short, long, default_value = "15")]
        limit: usize,
        /// Scan sessions from last N days
        #[arg(short, long, default_value = "30")]
        since: u32,
        /// Output format: text or json
        #[arg(short, long, default_value = "text")]
        format: String,
    },
    /// Validate rules: check patterns compile, examples fire, exceptions work, alternatives on PATH
    Validate,
    /// Probe a command against all rules and show what would fire — reads command from stdin
    Probe,
}

/// Minimal PostToolUse hook payload.
#[derive(Debug, Deserialize)]
struct HookPayload {
    tool_name: Option<String>,
    tool_input: Option<ToolInput>,
    tool_response: Option<Value>,
}

#[derive(Debug, Deserialize)]
struct ToolInput {
    command: Option<String>,
}

fn main() {
    let cli = Cli::parse();
    match cli.command {
        Command::Filter => cmd_filter(),
        Command::Rewrite => cmd_rewrite(),
        Command::Discover { all, limit, since, format } => {
            cmd_discover(all, limit, since, &format);
        }
        Command::Validate => cmd_validate(),
        Command::Probe => cmd_probe(),
    }
}

fn read_stdin_payload() -> Option<HookPayload> {
    let mut buf = String::new();
    io::stdin().read_to_string(&mut buf).ok()?;
    serde_json::from_str(&buf).ok()
}

fn cmd_filter() {
    let Some(payload) = read_stdin_payload() else {
        return;
    };

    if payload.tool_name.as_deref() != Some("Bash") {
        return;
    }

    let command = match payload.tool_input.as_ref().and_then(|i| i.command.as_deref()) {
        Some(c) if !c.is_empty() => c.to_string(),
        _ => return,
    };

    let output = payload
        .tool_response
        .as_ref()
        .and_then(|r| r.get("output"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let exit_code = payload
        .tool_response
        .as_ref()
        .and_then(|r| r.get("exit_code"))
        .and_then(|v| v.as_i64())
        .unwrap_or(0);

    let config = crs_core::filters::load();
    let fp = FilterPayload { command, output, exit_code };

    match run_filter(&fp, &config) {
        FilterResult::Passthrough => {}
        FilterResult::Suppress => {
            emit_message("");
        }
        FilterResult::Replace(text) => {
            emit_message(&text);
        }
    }
}

fn cmd_rewrite() {
    let Some(payload) = read_stdin_payload() else {
        std::process::exit(1);
    };

    if payload.tool_name.as_deref() != Some("Bash") {
        std::process::exit(1);
    }

    let command = match payload.tool_input.as_ref().and_then(|i| i.command.as_deref()) {
        Some(c) if !c.is_empty() => c,
        _ => std::process::exit(1),
    };

    let config = load_rewrite_config();
    match run_rewrite(command, &config) {
        Some(rewritten) => {
            // Emit PreToolUse response with modified command
            emit_rewrite(&rewritten);
        }
        None => {
            // Passthrough — exit 1 signals no rewrite
            std::process::exit(1);
        }
    }
}

fn load_rewrite_config() -> crs_core::rewrite::RewriteConfig {
    let Some(path) = crs_core::filters::filters_path() else {
        return crs_core::rewrite::RewriteConfig::default();
    };
    let Ok(content) = std::fs::read_to_string(&path) else {
        return crs_core::rewrite::RewriteConfig::default();
    };
    toml::from_str::<RewriteToml>(&content)
        .map(|t| t.rewrite_config)
        .unwrap_or_default()
}

/// TOML shape: `[[rewrites]]` section alongside `[[filters]]`.
#[derive(serde::Deserialize, Default)]
struct RewriteToml {
    #[serde(flatten)]
    rewrite_config: crs_core::rewrite::RewriteConfig,
}

fn emit_message(text: &str) {
    let msg = serde_json::json!({
        "type": "result",
        "message": text,
        "decision": "allow"
    });
    let stdout = io::stdout();
    let mut handle = stdout.lock();
    writeln!(handle, "{}", msg).ok();
    handle.flush().ok();
}

fn emit_rewrite(command: &str) {
    let msg = serde_json::json!({
        "hookSpecificOutput": {
            "hookEventName": "PreToolUse",
            "permissionDecision": "allow",
            "permissionDecisionReason": format!("crs rewrite: {command}"),
            "updatedInput": { "command": command }
        }
    });
    let stdout = io::stdout();
    let mut handle = stdout.lock();
    writeln!(handle, "{}", msg).ok();
    handle.flush().ok();
}

fn cmd_validate() {
    use crs_core::rules::load as load_rules;
    use regex::Regex;

    // Map rule id → (commands that should trigger, commands that should NOT trigger via exceptions)
    let known: &[(&str, &[&str], &[&str], &[&str])] = &[
        (
            "no-grep-use-tool",
            &["grep foo .", "rg pattern src/"],              // must trigger
            &["cmd | grep foo", "cmd | rg foo", "grep -A3"], // must be excepted
            &[],                                              // alternative binaries to check on PATH
        ),
        (
            "no-cat-use-read",
            &["cat file.txt", "cat /etc/hosts"],
            &["cat << EOF", "cmd | cat", "cat /dev/stdin"],
            &[],
        ),
        (
            "no-head-tail-use-read",
            &["head -20 file.txt", "tail -5 log.txt"],
            &["cmd | head", "tail -f log.txt"],
            &[],
        ),
        (
            "no-find-use-glob",
            &["find . -name '*.rs'", "find /home -type f"],
            &["find . -exec rm {} \\;", "find . -delete", "find . -mtime 1"],
            &[],
        ),
        (
            "no-npm-use-bun",
            &["npm install", "npx tsc"],
            &["npm publish", "npm pack", "npx create-react-app"],
            &["bun", "bunx"],
        ),
        (
            "no-pip-use-uv",
            &["pip install requests", "pip3 upgrade pip"],
            &["pip install --target /tmp/x"],
            &["uv"],
        ),
    ];

    let config = load_rules();
    let mut any_fail = false;

    println!("CRS Validate — Rule Health Check");
    println!("{}", "=".repeat(60));

    for rule in &config.rules {
        let mut issues: Vec<String> = vec![];

        // 1. Pattern compiles
        let pat_str = if rule.pattern_flags.contains('i') {
            format!("(?i){}", rule.pattern)
        } else {
            rule.pattern.clone()
        };
        let re = match Regex::new(&pat_str) {
            Ok(r) => r,
            Err(e) => {
                println!("FAIL  [{}]  pattern does not compile: {}", rule.id, e);
                any_fail = true;
                continue;
            }
        };

        // 2. All exception patterns compile
        for exc in &rule.exceptions {
            if Regex::new(exc).is_err() {
                issues.push(format!("exception pattern does not compile: {exc}"));
            }
        }

        // 3. Known-good trigger examples actually trigger (after exceptions)
        if let Some((_, triggers, excepts, alts)) = known.iter().find(|(id, ..)| *id == rule.id) {
            for &cmd in *triggers {
                let excepted = rule.exceptions.iter().any(|exc| {
                    Regex::new(exc).map(|r| r.is_match(cmd)).unwrap_or(false)
                });
                if !re.is_match(cmd) || excepted {
                    issues.push(format!("should trigger but does not: `{cmd}`"));
                }
            }
            // 4. Known exception examples are correctly excepted
            for &cmd in *excepts {
                let excepted = rule.exceptions.iter().any(|exc| {
                    Regex::new(exc).map(|r| r.is_match(cmd)).unwrap_or(false)
                });
                if re.is_match(cmd) && !excepted {
                    issues.push(format!("should be excepted but triggers: `{cmd}`"));
                }
            }
            // 5. Alternative tools on PATH
            for &alt in *alts {
                if std::process::Command::new("sh")
                    .args(["-c", &format!("type -P {alt}")])
                    .output()
                    .map(|o| !o.status.success())
                    .unwrap_or(true)
                {
                    issues.push(format!("alternative `{alt}` not found on PATH"));
                }
            }
        }

        if issues.is_empty() {
            println!("OK    [{}]", rule.id);
        } else {
            any_fail = true;
            println!("FAIL  [{}]", rule.id);
            for issue in &issues {
                println!("        - {issue}");
            }
        }
    }

    println!("{}", "=".repeat(60));
    if any_fail {
        println!("Some rules have issues.");
        std::process::exit(1);
    } else {
        println!("All rules OK.");
    }
}

fn cmd_probe() {
    use crs_core::rules::load as load_rules;
    use regex::Regex;
    use std::io::Read as _;

    let mut raw = String::new();
    std::io::stdin().read_to_string(&mut raw).unwrap_or(0);
    let raw = raw.trim();

    // Accept: raw command string, JSON object with "command" key, or hook-style
    // {"tool_input":{"command":"..."}} / {"tool_name":"Bash","tool_input":...}
    let command: String = if let Ok(v) = serde_json::from_str::<Value>(raw) {
        v.get("command")
            .or_else(|| v.get("tool_input").and_then(|ti| ti.get("command")))
            .and_then(|c| c.as_str())
            .unwrap_or(raw)
            .to_string()
    } else {
        raw.to_string()
    };

    if command.is_empty() {
        eprintln!("crs probe: no command on stdin");
        std::process::exit(1);
    }

    let config = load_rules();

    println!("Command: {command}");
    println!("{}", "─".repeat(60));

    let mut any_match = false;

    for rule in &config.rules {
        if !rule.enabled {
            continue;
        }
        let pat_str = if rule.pattern_flags.contains('i') {
            format!("(?i){}", rule.pattern)
        } else {
            rule.pattern.clone()
        };
        let Ok(re) = Regex::new(&pat_str) else { continue };
        if !re.is_match(&command) {
            continue;
        }
        any_match = true;

        // Find the first matching exception, if any
        let matched_exc: Option<&str> = rule.exceptions.iter().find_map(|exc| {
            Regex::new(exc)
                .map(|r| if r.is_match(&command) { Some(exc.as_str()) } else { None })
                .unwrap_or(None)
        });

        if let Some(exc) = matched_exc {
            println!("ALLOW  [{}]", rule.id);
            println!("       pattern `{}` matched", rule.pattern);
            println!("       exception `{exc}` overrides → passthrough");
        } else {
            println!("BLOCK  [{}]", rule.id);
            println!("       pattern `{}` matched", rule.pattern);
            if let Some(ref msg) = rule.message {
                // Wrap message at 72 cols with 7-space indent
                let indent = "       ";
                let words = msg.split_whitespace();
                let mut line = format!("{indent}message: ");
                let prefix_len = line.len();
                for word in words {
                    if line.len() + word.len() + 1 > 79 && line.len() > prefix_len {
                        println!("{line}");
                        line = format!("{indent}         {word} ");
                    } else {
                        line.push_str(word);
                        line.push(' ');
                    }
                }
                if line.trim_end() != indent.trim_end() {
                    println!("{}", line.trim_end());
                }
            }
            // List exceptions that did NOT match (so user knows what would save them)
            if !rule.exceptions.is_empty() {
                println!("       would allow if any of:");
                for exc in &rule.exceptions {
                    println!("         - {exc}");
                }
            }
        }
        println!();
    }

    if !any_match {
        println!("PASS   no rule matched");
    }
}

fn cmd_discover(all: bool, limit: usize, since: u32, format: &str) {
    use crs_core::history::{DiscoverOpts, discover};
    use crs_core::rules::load as load_rules;

    let root = std::env::var("CLAUDE_PROJECTS_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| {
            dirs::home_dir()
                .expect("home dir")
                .join(".claude/projects")
        });

    let current_dir = std::env::current_dir().ok();
    let src = crs_lib::jsonl_source::JsonlCommandSource::new(root, all, current_dir.clone());

    let rules_cfg = load_rules();
    let opts = DiscoverOpts {
        limit,
        since_days: Some(since),
        all_projects: all,
        current_dir,
    };

    let report = discover(&src, &rules_cfg.rules, &opts);

    match format {
        "json" => print_discover_json(&report),
        _ => print_discover_text(&report),
    }
}

fn print_discover_text(report: &crs_core::history::DiscoverReport) {
    println!("CRS Discover — Savings Opportunities");
    println!("{}", "=".repeat(52));
    println!(
        "Scanned: {} sessions, {} Bash commands\n",
        report.scanned_sessions, report.scanned_commands
    );

    if !report.intercepted.is_empty() {
        println!("INTERCEPTED — commands with matching rules");
        println!("{}", "-".repeat(72));
        let has_tokens = report.intercepted.iter().any(|f| f.est_tokens > 0);
        if has_tokens {
            println!("{:<24} {:>6}   {:<24} {:>12}", "Command", "Count", "Rule", "Output tokens");
        } else {
            println!("{:<24} {:>6}   {}", "Command", "Count", "Rule");
        }
        for f in &report.intercepted {
            let rule = f.rule_id.as_deref().unwrap_or("-");
            if has_tokens {
                let savings = format_tokens(f.est_tokens);
                println!("{:<24} {:>6}   {:<24} {:>12}", f.stem, f.count, rule, savings);
            } else {
                println!("{:<24} {:>6}   {}", f.stem, f.count, rule);
            }
        }
        let total_tokens: u64 = report.intercepted.iter().map(|f| f.est_tokens).sum();
        let total_cmds: u64 = report.intercepted.iter().map(|f| f.count).sum();
        println!("{}", "-".repeat(72));
        if has_tokens {
            println!("Total: {} commands → {} output tokens", total_cmds, total_tokens);
        } else {
            println!("Total: {} commands (no output data in sessions)", total_cmds);
        }
    }

    if !report.unhandled.is_empty() {
        println!("\nTOP UNHANDLED — no matching rule");
        println!("{}", "-".repeat(52));
        println!("{:<24} {:>6}   {}", "Command", "Count", "Example");
        for f in &report.unhandled {
            let ex = if f.example.len() > 36 {
                format!("{}...", &f.example[..36])
            } else {
                f.example.clone()
            };
            println!("{:<24} {:>6}   {}", f.stem, f.count, ex);
        }
        println!("{}", "-".repeat(52));
    }

}

fn print_discover_json(report: &crs_core::history::DiscoverReport) {
    let out = serde_json::json!({
        "scanned_sessions": report.scanned_sessions,
        "scanned_commands": report.scanned_commands,
        "intercepted": report.intercepted.iter().map(|f| serde_json::json!({
            "stem": f.stem,
            "count": f.count,
            "example": f.example,
            "est_tokens": f.est_tokens,
            "rule_id": f.rule_id,
        })).collect::<Vec<_>>(),
        "unhandled": report.unhandled.iter().map(|f| serde_json::json!({
            "stem": f.stem,
            "count": f.count,
            "example": f.example,
        })).collect::<Vec<_>>(),
    });
    println!("{}", serde_json::to_string_pretty(&out).unwrap());
}

fn format_tokens(n: u64) -> String {
    if n >= 1000 {
        format!("~{:.1}K tokens", n as f64 / 1000.0)
    } else {
        format!("~{n} tokens")
    }
}
