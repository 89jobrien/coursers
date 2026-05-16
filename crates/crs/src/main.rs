use clap::{Parser, Subcommand};
use crs_lib::{FilterPayload, FilterResult, run_filter, run_rewrite};
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
        /// Generate .ctx/obfsck-filters.yaml from unhandled command examples
        #[arg(long)]
        generate_filters: bool,
        /// Only show commands seen at least N times (default: 1 = show all)
        #[arg(long, default_value = "1")]
        min_count: u64,
    },
    /// Validate rules: check patterns compile, examples fire, exceptions work, alternatives on PATH
    Validate,
    /// Probe a command against all rules and show what would fire — reads command from stdin
    Probe,
    /// Show cumulative block counts by rule
    Stats,
    /// Analyze session facets enriched with git context
    Insights {
        /// Output format: text or json
        #[arg(short, long, default_value = "text")]
        format: String,
        /// Only include facets from the last N days (based on git timestamp)
        #[arg(short, long)]
        since: Option<u32>,
        /// Filter to sessions in a specific repo (matches cwd basename)
        #[arg(short, long)]
        repo: Option<String>,
    },
    /// Show rx prefix learning state: confirmed mappings and pending probes
    Audit {
        /// Remove a confirmed mapping by key
        #[arg(long)]
        remove: Option<String>,
    },
    /// Suggest new rules from the top unhandled commands in session history
    Suggest {
        /// Scan all projects (default: current project only)
        #[arg(short, long)]
        all: bool,
        /// Scan sessions from last N days
        #[arg(short, long, default_value = "30")]
        since: u32,
        /// Max candidates to suggest
        #[arg(short, long, default_value = "10")]
        limit: usize,
        /// Output format: text or json
        #[arg(short, long, default_value = "text")]
        format: String,
    },
    /// Show recent blocked commands with timestamps and firing rules
    History {
        /// Max entries to show
        #[arg(short, long, default_value = "20")]
        limit: usize,
        /// Filter to a specific rule id
        #[arg(short, long)]
        rule: Option<String>,
        /// Output format: text or json
        #[arg(short, long, default_value = "text")]
        format: String,
    },
    /// Dump rules + stats + state as a portable JSON snapshot
    Export {
        /// Write output to this file instead of stdout
        #[arg(short, long)]
        out: Option<String>,
    },
    /// Show a heatmap of rule firings by hour-of-day and day-of-week
    Heat {
        /// Filter to a specific rule id
        #[arg(short, long)]
        rule: Option<String>,
    },
    /// Replay a session's Bash commands through the current ruleset (no side effects)
    Replay {
        /// Path to a .jsonl session file (default: most recent session for current project)
        #[arg(short, long)]
        session: Option<String>,
        /// Output format: text or json
        #[arg(short, long, default_value = "text")]
        format: String,
    },
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
        Command::Discover {
            all,
            limit,
            since,
            format,
            generate_filters,
            min_count,
        } => {
            cmd_discover(all, limit, since, &format, generate_filters, min_count);
        }
        Command::Validate => cmd_validate(),
        Command::Probe => cmd_probe(),
        Command::Stats => cmd_stats(),
        Command::Insights {
            format,
            since,
            repo,
        } => cmd_insights(&format, since, repo.as_deref()),
        Command::Audit { remove } => cmd_audit(remove),
        Command::Suggest {
            all,
            since,
            limit,
            format,
        } => cmd_suggest(all, since, limit, &format),
        Command::History {
            limit,
            rule,
            format,
        } => cmd_history(limit, rule.as_deref(), &format),
        Command::Export { out } => cmd_export(out.as_deref()),
        Command::Heat { rule } => cmd_heat(rule.as_deref()),
        Command::Replay { session, format } => cmd_replay(session.as_deref(), &format),
    }
}

fn read_stdin_payload() -> Option<HookPayload> {
    let mut buf = String::new();
    io::stdin().read_to_string(&mut buf).ok()?;
    serde_json::from_str(&buf).ok()
}

/// Executables registered as 1Password CLI plugins (from `op plugin list`).
/// `op plugin run --` may only be auto-confirmed for keys in this set.
const OP_PLUGIN_EXECUTABLES: &[&str] = &[
    "akamai",
    "argocd",
    "aws",
    "cdk",
    "axiom",
    "binance-cli",
    "cachix",
    "cargo",
    "circleci",
    "civo",
    "wrangler",
    "crowdin",
    "databricks",
    "dog",
    "doctl",
    "fastly",
    "flyctl",
    "fly",
    "fossa",
    "tea",
    "gh",
    "glab",
    "vault",
    "heroku",
    "hcloud",
    "brew",
    "huggingface-cli",
    "influx",
    "kaggle",
    "lacework",
    "forge",
    "vapor",
    "linode-cli",
    "localstack",
    "atlas",
    "mysql",
    "ngrok",
    "ohdear",
    "okta",
    "openai",
    "oaieval",
    "oaievalset",
    "pd",
    "psql",
    "pg_dump",
    "pg_restore",
    "pgcli",
    "pulumi",
    "rdme",
    "sentry-cli",
    "snowsql",
    "snyk",
    "src",
    "stripe",
    "todoist",
    "td",
    "tugboat",
    "twilio",
    "upstash",
    "vercel",
    "vsql",
    "vultr-cli",
    "ysqlsh",
    "zapier",
    "zcli",
];

fn is_op_plugin_prefix(prefix: &[String]) -> bool {
    matches!(prefix, [a, b, c, d] if a == "op" && b == "plugin" && c == "run" && d == "--")
}

fn apply_rx_learning(
    command: &str,
    exit_code: i64,
    probe_store: &dyn crs_core::rx_prefix::ProbeStore,
    prefix_store: &dyn crs_core::rx_prefix::PrefixStore,
) {
    let probes = probe_store.load();
    let matching: Vec<_> = probes
        .iter()
        .filter(|p| p.original_command == command)
        .collect();
    if matching.is_empty() {
        return;
    }
    if exit_code == 0 {
        for probe in &matching {
            if is_op_plugin_prefix(&probe.prefix)
                && !OP_PLUGIN_EXECUTABLES.contains(&probe.key.as_str())
            {
                continue;
            }
            prefix_store.confirm_mapping(&probe.key, &probe.prefix);
        }
    }
    probe_store.remove_matching(command);
}

fn cmd_filter() {
    let Some(payload) = read_stdin_payload() else {
        return;
    };

    if payload.tool_name.as_deref() != Some("Bash") {
        return;
    }

    let command = match payload
        .tool_input
        .as_ref()
        .and_then(|i| i.command.as_deref())
    {
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
    let fp = FilterPayload {
        command: command.clone(),
        output: output.clone(),
        exit_code,
    };

    // Apply compression rules first.
    let filtered_output = match run_filter(&fp, &config) {
        FilterResult::Passthrough => output.clone(),
        FilterResult::Suppress => {
            emit_message("");
            return;
        }
        FilterResult::Replace(text) => text,
    };

    // Apply obfsck redaction patterns if .ctx/obfsck-filters.yaml exists.
    let obfsck = crs_core::filters::load_obfsck_filters();
    let final_output = crs_core::filters::apply_redaction(&filtered_output, &obfsck);

    // Post-hook rx learning: confirm or discard candidate prefix probes.
    {
        let probe_store = crs_core::rx_prefix::FileProbeStore {
            path: crs_core::rx_prefix::FileProbeStore::default_path(),
        };
        let prefix_store = crs_core::rx_prefix::FilePrefixStore {
            path: crs_core::rx_prefix::FilePrefixStore::default_path(),
        };
        apply_rx_learning(&command, exit_code, &probe_store, &prefix_store);
    }

    // Only emit a hook message if output changed (avoids noise on passthrough).
    if final_output != output {
        emit_message(&final_output);
    }
}

fn cmd_rewrite() {
    let Some(payload) = read_stdin_payload() else {
        std::process::exit(1);
    };

    if payload.tool_name.as_deref() != Some("Bash") {
        std::process::exit(1);
    }

    let command = match payload
        .tool_input
        .as_ref()
        .and_then(|i| i.command.as_deref())
    {
        Some(c) if !c.is_empty() => c,
        _ => std::process::exit(1),
    };

    // 1. Try AST tool swap first
    let filters_cfg = crs_core::filters::load();
    let swap = crs_core::tool_swap::apply(command, &filters_cfg.tool_swap);
    if let crs_core::tool_swap::ToolAction::SwapTool {
        tool_name,
        tool_input,
    } = swap
    {
        emit_tool_swap(&tool_name, tool_input);
        return;
    }

    // 2. Regex rewrite rules from crs-filters.toml
    let config = load_rewrite_config();
    if let Some(rewritten) = run_rewrite(command, &config) {
        emit_rewrite(&rewritten);
        return;
    }

    // 3. rx prefix injection
    let rx_config = {
        use crs_core::rx_prefix::PrefixStore as _;
        crs_core::rx_prefix::FilePrefixStore {
            path: crs_core::rx_prefix::FilePrefixStore::default_path(),
        }
        .load()
    };
    let result = crs_core::rx_prefix::rewrite_command(command, &rx_config);
    if result.rewritten != command {
        if !result.probes.is_empty() {
            let probe_store = crs_core::rx_prefix::FileProbeStore {
                path: crs_core::rx_prefix::FileProbeStore::default_path(),
            };
            let mut existing = probe_store.load();
            existing.extend(result.probes);
            probe_store.write(&existing);
        }
        emit_rewrite(&result.rewritten);
        return;
    }

    // No rewrite matched.
    std::process::exit(1);
}

fn emit_tool_swap(tool_name: &str, tool_input: serde_json::Value) {
    let msg = serde_json::json!({
        "hookSpecificOutput": {
            "hookEventName": "PreToolUse",
            "permissionDecision": "allow",
            "permissionDecisionReason": format!("crs tool-swap: Bash → {tool_name}"),
            "updatedInput": {
                "tool_name": tool_name,
                "tool_input": tool_input,
            }
        }
    });
    let stdout = io::stdout();
    let mut handle = stdout.lock();
    writeln!(handle, "{}", msg).ok();
    handle.flush().ok();
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
    type KnownRule<'a> = (&'a str, &'a [&'a str], &'a [&'a str], &'a [&'a str]);
    let known: &[KnownRule<'_>] = &[
        (
            "no-grep-use-tool",
            &["grep foo .", "rg pattern src/"], // must trigger
            &["cmd | grep foo", "cmd | rg foo", "grep -A3"], // must be excepted
            &[],                                // alternative binaries to check on PATH
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
        (
            "no-nvm-use-mise",
            &["nvm install 20", "nvm use 18", "nvm alias default 20"],
            &[],
            &["mise"],
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
                let excepted = rule
                    .exceptions
                    .iter()
                    .any(|exc| Regex::new(exc).map(|r| r.is_match(cmd)).unwrap_or(false));
                if !re.is_match(cmd) || excepted {
                    issues.push(format!("should trigger but does not: `{cmd}`"));
                }
            }
            // 4. Known exception examples are correctly excepted
            for &cmd in *excepts {
                let excepted = rule
                    .exceptions
                    .iter()
                    .any(|exc| Regex::new(exc).map(|r| r.is_match(cmd)).unwrap_or(false));
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
        let Ok(re) = Regex::new(&pat_str) else {
            continue;
        };
        if !re.is_match(&command) {
            continue;
        }
        any_match = true;

        // Find the first matching exception, if any
        let matched_exc: Option<&str> = rule.exceptions.iter().find_map(|exc| {
            Regex::new(exc)
                .map(|r| {
                    if r.is_match(&command) {
                        Some(exc.as_str())
                    } else {
                        None
                    }
                })
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

fn cmd_discover(
    all: bool,
    limit: usize,
    since: u32,
    format: &str,
    generate_filters: bool,
    min_count: u64,
) {
    use crs_core::history::{DiscoverOpts, discover};
    use crs_core::obfsck::ObfsckMcp as _;
    use crs_core::rtk::RtkAnalysis as _;
    use crs_core::rules::load as load_rules;
    use std::collections::HashMap;

    let root = std::env::var("CLAUDE_PROJECTS_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| dirs::home_dir().expect("home dir").join(".claude/projects"));

    let current_dir = std::env::current_dir().ok();
    let src = crs_lib::jsonl_source::JsonlCommandSource::new(root, all, current_dir.clone());

    let rules_cfg = load_rules();
    let opts = DiscoverOpts {
        limit,
        since_days: Some(since),
        all_projects: all,
        current_dir,
        min_count,
    };

    let report = discover(&src, &rules_cfg.rules, &opts);

    // Enrich with RTK data if rtk is on PATH.

    // Build stem -> (rtk_equivalent, est_savings_tokens, est_savings_pct) lookup.
    let rtk_map: HashMap<String, (String, u64, f64)> = crs_lib::rtk::detect()
        .and_then(|c| c.discover(since))
        .map(|r| {
            r.supported
                .into_iter()
                .map(|e| {
                    (
                        e.command.clone(),
                        (e.rtk_equivalent, e.est_savings_tokens, e.est_savings_pct),
                    )
                })
                .collect()
        })
        .unwrap_or_default();

    match format {
        "json" => print_discover_json(&report),
        _ => print_discover_text(&report),
    }

    let ctx = std::path::Path::new(".ctx");
    if ctx.is_dir() {
        write_tools_yaml(&report, since, &rtk_map, ctx.join("HANDOFF.tools.yaml"));

        // Generate project-local obfsck filters from unhandled command examples.
        if generate_filters {
            let client = crs_lib::obfsck::detect();
            if let Some(client) = client {
                let examples: Vec<String> =
                    report.unhandled.iter().map(|f| f.example.clone()).collect();
                let suggestions = if examples.is_empty() {
                    vec![]
                } else {
                    client.generate_filters(&examples)
                };
                if !suggestions.is_empty() {
                    write_obfsck_filters(&suggestions, ctx.join("obfsck-filters.yaml"));
                }
            }
        }
    }
}

fn write_tools_yaml(
    report: &crs_core::history::DiscoverReport,
    since_days: u32,
    rtk_map: &std::collections::HashMap<String, (String, u64, f64)>,
    path: std::path::PathBuf,
) {
    use std::io::Write as _;

    let today = chrono::Local::now().format("%Y-%m-%d").to_string();

    let mut out = String::new();
    out.push_str(&format!("generated: {today}\n"));
    out.push_str(&format!("since_days: {since_days}\n"));
    out.push_str(&format!("sessions_scanned: {}\n", report.scanned_sessions));
    out.push_str(&format!("total_commands: {}\n", report.scanned_commands));

    if !report.intercepted.is_empty() {
        out.push_str("top_supported:\n");
        for f in &report.intercepted {
            out.push_str(&format!("  - command: {}\n", f.stem));
            out.push_str(&format!("    count: {}\n", f.count));
            if let Some(ref rule) = f.rule_id {
                out.push_str(&format!("    rule: {rule}\n"));
            }
            if let Some((rtk_eq, rtk_tokens, rtk_pct)) = rtk_map.get(&f.stem) {
                out.push_str(&format!("    rtk_equivalent: {rtk_eq}\n"));
                out.push_str(&format!("    est_savings_tokens: {rtk_tokens}\n"));
                out.push_str(&format!("    est_savings_pct: {rtk_pct}\n"));
            } else if f.est_tokens > 0 {
                out.push_str(&format!("    est_savings_tokens: {}\n", f.est_tokens));
            }
        }
    }

    if !report.unhandled.is_empty() {
        out.push_str("top_unhandled:\n");
        for f in &report.unhandled {
            let ex = if f.example.len() > 80 {
                format!("{}...", &f.example[..80])
            } else {
                f.example.clone()
            };
            out.push_str(&format!("  - base_command: {}\n", f.stem));
            out.push_str(&format!("    count: {}\n", f.count));
            out.push_str(&format!("    example: {:?}\n", ex));
        }
    }

    // Audit the YAML for secrets via obfsck-mcp before writing — surfaces hits to stderr.
    obfsck_audit_mcp(&out);

    match std::fs::File::create(&path).and_then(|mut f| f.write_all(out.as_bytes())) {
        Ok(()) => eprintln!("wrote {}", path.display()),
        Err(e) => eprintln!("warn: could not write {}: {e}", path.display()),
    }
}

/// Audit content for secrets via obfsck-mcp; surface hits to stderr.
/// Falls back silently if obfsck-mcp is not on PATH.
fn obfsck_audit_mcp(content: &str) {
    use crs_core::obfsck::ObfsckMcp as _;
    let Some(client) = crs_lib::obfsck::detect() else {
        return;
    };
    let hits = client.audit(content);
    if !hits.is_empty() {
        eprintln!("obfsck: secret pattern hits in generated YAML:");
        for h in &hits {
            eprintln!("  {} ({})", h.label, h.count);
        }
    }
}

/// Write filter suggestions from obfsck-mcp to `.ctx/obfsck-filters.yaml`.
/// Merges with any existing file — new patterns whose label already appears are skipped.
fn write_obfsck_filters(
    suggestions: &[crs_core::obfsck::FilterSuggestion],
    path: std::path::PathBuf,
) {
    use std::io::Write as _;

    // Load existing labels to avoid duplicates.
    let existing_content = std::fs::read_to_string(&path).unwrap_or_default();
    let existing_labels: std::collections::HashSet<String> = existing_content
        .lines()
        .filter_map(|l| {
            let l = l.trim();
            l.strip_prefix("- label: ").map(|s| s.trim().to_string())
        })
        .collect();

    let new_suggestions: Vec<&crs_core::obfsck::FilterSuggestion> = suggestions
        .iter()
        .filter(|s| !existing_labels.contains(&s.label))
        .collect();

    if new_suggestions.is_empty() {
        // Nothing to add; leave file untouched.
        return;
    }

    let today = chrono::Local::now().format("%Y-%m-%d").to_string();

    // Build merged output: preserve existing lines, append new entries.
    let mut out = if existing_content.trim().is_empty() {
        format!(
            "# Generated by crs discover on {today}\n\
             # Review before committing — patterns are regex-based.\n\
             filters:\n"
        )
    } else {
        // Strip trailing newline; we'll re-add it cleanly.
        existing_content.trim_end().to_string() + "\n"
    };

    for s in &new_suggestions {
        out.push_str(&format!("  - label: {}\n", s.label));
        out.push_str(&format!("    pattern: {:?}\n", s.pattern));
    }

    match std::fs::File::create(&path).and_then(|mut f| f.write_all(out.as_bytes())) {
        Ok(()) => eprintln!("wrote {}", path.display()),
        Err(e) => eprintln!("warn: could not write {}: {e}", path.display()),
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
            println!(
                "{:<24} {:>6}   {:<24} {:>12}",
                "Command", "Count", "Rule", "Output tokens"
            );
        } else {
            println!("{:<24} {:>6}   Rule", "Command", "Count");
        }
        for f in &report.intercepted {
            let rule = f.rule_id.as_deref().unwrap_or("-");
            if has_tokens {
                let savings = format_tokens(f.est_tokens);
                println!(
                    "{:<24} {:>6}   {:<24} {:>12}",
                    f.stem, f.count, rule, savings
                );
            } else {
                println!("{:<24} {:>6}   {}", f.stem, f.count, rule);
            }
        }
        let total_tokens: u64 = report.intercepted.iter().map(|f| f.est_tokens).sum();
        let total_cmds: u64 = report.intercepted.iter().map(|f| f.count).sum();
        println!("{}", "-".repeat(72));
        if has_tokens {
            println!(
                "Total: {} commands → {} output tokens",
                total_cmds, total_tokens
            );
        } else {
            println!(
                "Total: {} commands (no output data in sessions)",
                total_cmds
            );
        }
    }

    if !report.unhandled.is_empty() {
        println!("\nTOP UNHANDLED — no matching rule");
        println!("{}", "-".repeat(52));
        println!("{:<24} {:>6}   Example", "Command", "Count");
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

fn cmd_stats() {
    use crs_core::stats::{load, sorted_blocks, stats_path};

    let path = stats_path();
    let stats = load(&path);

    println!("CRS Block Stats — {}", path.display());
    println!("{}", "=".repeat(52));

    if stats.blocks.is_empty() {
        println!("No blocks recorded yet.");
        return;
    }

    let total: u64 = stats.blocks.values().sum();
    println!("{:<32} {:>8}", "Rule", "Blocks");
    println!("{}", "-".repeat(42));
    for (rule_id, count) in sorted_blocks(&stats) {
        println!("{:<32} {:>8}", rule_id, count);
    }
    println!("{}", "-".repeat(42));
    println!("{:<32} {:>8}", "Total", total);
}

fn cmd_insights(format: &str, since: Option<u32>, repo: Option<&str>) {
    use crs_core::insights::{aggregate, enrich, load_facets};

    let facets_dir = std::env::var("CLAUDE_FACETS_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| {
            dirs::home_dir()
                .expect("home dir")
                .join(".claude/usage-data/facets")
        });

    let projects_root = std::env::var("CLAUDE_PROJECTS_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| dirs::home_dir().expect("home dir").join(".claude/projects"));

    if !facets_dir.exists() {
        eprintln!(
            "crs insights: facets directory not found: {}",
            facets_dir.display()
        );
        eprintln!("  Run /insights in Claude Code first to generate facet data.");
        std::process::exit(1);
    }

    let facets = load_facets(&facets_dir);
    let mut enriched = enrich(facets, &projects_root);

    // Filter by repo if requested
    if let Some(repo_filter) = repo {
        enriched.retain(|ef| {
            ef.git
                .as_ref()
                .map(|g| g.repo == repo_filter)
                .unwrap_or(false)
        });
    }

    // Filter by since days (based on git timestamp)
    if let Some(days) = since {
        use std::time::{SystemTime, UNIX_EPOCH};
        let cutoff_secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
            .saturating_sub(days as u64 * 86400);
        let cutoff = format_unix_date(cutoff_secs);
        enriched.retain(|ef| {
            ef.git
                .as_ref()
                .and_then(|g| g.timestamp.as_deref())
                .map(|ts| &ts[..ts.len().min(10)] >= cutoff.as_str())
                .unwrap_or(true) // keep facets with no timestamp
        });
    }

    let report = aggregate(&enriched);

    match format {
        "json" => println!("{}", serde_json::to_string_pretty(&report).unwrap()),
        _ => print_insights_text(&report, &enriched),
    }
}

fn format_unix_date(secs: u64) -> String {
    let days = secs / 86400;
    let mut remaining = days + 719468;
    let era = remaining / 146097;
    remaining %= 146097;
    let yoe = (remaining - remaining / 1460 + remaining / 36524 - remaining / 146096) / 365;
    let y = yoe + era * 400;
    let doy = remaining - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}-{m:02}-{d:02}")
}

fn print_insights_text(
    report: &crs_core::insights::InsightsReport,
    enriched: &[crs_core::insights::EnrichedFacet],
) {
    println!("CRS Insights — Session Facet Analysis");
    println!("{}", "=".repeat(60));
    println!(
        "Sessions: {}  (git-enriched: {})\n",
        report.total, report.git_enriched
    );

    // Outcomes
    if !report.outcomes.is_empty() {
        println!("Outcomes");
        println!("{}", "-".repeat(40));
        let mut outcomes: Vec<_> = report.outcomes.iter().collect();
        outcomes.sort_by(|a, b| b.1.cmp(a.1));
        for (k, v) in &outcomes {
            let pct = *v * 100 / report.total;
            println!("  {:<28} {:>4}  ({pct}%)", k, v);
        }
        println!();
    }

    // Helpfulness
    if !report.helpfulness.is_empty() {
        println!("Claude Helpfulness");
        println!("{}", "-".repeat(40));
        let mut h: Vec<_> = report.helpfulness.iter().collect();
        h.sort_by(|a, b| b.1.cmp(a.1));
        for (k, v) in &h {
            println!("  {:<28} {:>4}", k, v);
        }
        println!();
    }

    // Friction
    if !report.friction.is_empty() {
        println!("Friction (cumulative across sessions)");
        println!("{}", "-".repeat(40));
        let mut f: Vec<_> = report.friction.iter().collect();
        f.sort_by(|a, b| b.1.cmp(a.1));
        for (k, v) in &f {
            println!("  {:<28} {:>4}", k, v);
        }
        println!();
    }

    // Top goal categories
    if !report.goal_categories.is_empty() {
        println!("Goal Categories");
        println!("{}", "-".repeat(40));
        let mut g: Vec<_> = report.goal_categories.iter().collect();
        g.sort_by(|a, b| b.1.cmp(a.1));
        for (k, v) in g.iter().take(10) {
            println!("  {:<28} {:>4}", k, v);
        }
        println!();
    }

    // Top repos
    if !report.top_repos.is_empty() {
        println!("Top Repos");
        println!("{}", "-".repeat(40));
        for (repo, count) in &report.top_repos {
            println!("  {:<28} {:>4}", repo, count);
        }
        println!();
    }

    // Top branches
    if !report.top_branches.is_empty() {
        println!("Top Branches");
        println!("{}", "-".repeat(40));
        for (branch, count) in report.top_branches.iter().take(10) {
            println!("  {:<28} {:>4}", branch, count);
        }
        println!();
    }

    // Recent sessions sample (last 5 with git context)
    let with_git: Vec<_> = enriched
        .iter()
        .filter(|ef| ef.git.is_some())
        .rev()
        .take(5)
        .collect();
    if !with_git.is_empty() {
        println!("Recent Sessions (with git context)");
        println!("{}", "-".repeat(60));
        for ef in &with_git {
            let git = ef.git.as_ref().unwrap();
            let branch = git.branch.as_deref().unwrap_or("?");
            let ts = git
                .timestamp
                .as_deref()
                .map(|t| &t[..t.len().min(10)])
                .unwrap_or("?");
            let outcome = ef.facet.outcome.as_deref().unwrap_or("?");
            let summary = ef.facet.brief_summary.as_deref().unwrap_or("");
            let summary = if summary.len() > 60 {
                &summary[..60]
            } else {
                summary
            };
            println!("  {} | {} | {} | {}", ts, git.repo, branch, outcome);
            if !summary.is_empty() {
                println!("    {}", summary);
            }
        }
    }
}

fn cmd_audit(remove: Option<String>) {
    use crs_core::rx_prefix::{FilePrefixStore, FileProbeStore, audit_state};

    let prefix_store = FilePrefixStore {
        path: FilePrefixStore::default_path(),
    };
    let probe_store = FileProbeStore {
        path: FileProbeStore::default_path(),
    };

    if let Some(ref key) = remove {
        if prefix_store.remove_mapping(key) {
            println!("Removed mapping: {key}");
        } else {
            println!("Key not found: {key}");
        }
        return;
    }

    let state = audit_state(&prefix_store, &probe_store);

    println!("Prefix Audit");
    println!("{}", "=".repeat(60));

    println!("\nConfirmed mappings ({})", state.mappings.len());
    println!("{}", "-".repeat(40));
    if state.mappings.is_empty() {
        println!("No confirmed mappings.");
    } else {
        for (key, prefix) in &state.mappings {
            println!("  {key} → {}", prefix.join(" "));
        }
    }

    println!("\nPending probes ({})", state.probes.len());
    println!("{}", "-".repeat(40));
    if state.probes.is_empty() {
        println!("No pending probes.");
    } else {
        for probe in &state.probes {
            println!(
                "  key={} prefix={} cmd={:?}",
                probe.key,
                probe.prefix.join(" "),
                probe.original_command
            );
        }
    }
}

fn cmd_suggest(all: bool, since: u32, limit: usize, format: &str) {
    use crs_core::history::{DiscoverOpts, discover};
    use crs_core::rules::load as load_rules;
    use crs_core::suggest::suggest;

    let root = std::env::var("CLAUDE_PROJECTS_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| dirs::home_dir().expect("home dir").join(".claude/projects"));

    let current_dir = std::env::current_dir().ok();
    let src = crs_lib::jsonl_source::JsonlCommandSource::new(root, all, current_dir.clone());

    let rules_cfg = load_rules();
    let opts = DiscoverOpts {
        limit,
        since_days: Some(since),
        all_projects: all,
        current_dir,
        min_count: 1, // suggest always scans all frequencies
    };

    let report = discover(&src, &rules_cfg.rules, &opts);
    let suggestions = suggest(&report.unhandled);

    match format {
        "json" => println!("{}", serde_json::to_string_pretty(&suggestions).unwrap()),
        _ => {
            if suggestions.is_empty() {
                println!("No unhandled commands found — nothing to suggest.");
                return;
            }
            println!("CRS Suggest — Candidate Rules from Session History");
            println!("{}", "=".repeat(60));
            println!(
                "Based on top {} unhandled commands in the last {since} days.\n",
                suggestions.len()
            );
            println!("Add to ~/.config/coursers/course-correct-rules.json:\n");
            println!("[");
            for (i, s) in suggestions.iter().enumerate() {
                let comma = if i + 1 < suggestions.len() { "," } else { "" };
                println!("  {{");
                println!("    \"id\": \"{}\",", s.id);
                println!("    \"pattern\": \"{}\",", s.pattern.replace('"', "\\\""));
                println!("    \"message\": \"{}\",", s.message);
                println!("    \"enabled\": true");
                println!("    // seen {} time(s) — example: {}", s.count, s.example);
                println!("  }}{comma}");
            }
            println!("]");
        }
    }
}

fn cmd_history(limit: usize, rule_filter: Option<&str>, format: &str) {
    use crs_core::rules::load as load_rules;
    use crs_core::state::{load as load_state, state_path};
    use crs_core::stats::{load as load_stats, stats_path};

    let rules_cfg = load_rules();
    let stats_p = stats_path();
    let stats = load_stats(&stats_p);

    let state_p = state_path(&rules_cfg.failure_learning);
    let state = load_state(&state_p);

    // Build per-rule history from stats last_seen + failure state entries
    #[derive(serde::Serialize)]
    struct HistoryEntry {
        rule_id: String,
        last_seen_unix: f64,
        last_seen_date: String,
        block_count: u64,
        command_preview: Option<String>,
    }

    let mut entries: Vec<HistoryEntry> = stats
        .last_seen
        .iter()
        .filter(|(rule_id, _)| rule_filter.map(|f| f == rule_id.as_str()).unwrap_or(true))
        .map(|(rule_id, &ts)| {
            let date = format_unix_date(ts as u64);
            let count = stats.blocks.get(rule_id).copied().unwrap_or(0);
            // Best-effort: find a matching failure state entry for a command preview
            let preview = state
                .failures
                .values()
                .max_by(|a, b| {
                    a.last_seen
                        .partial_cmp(&b.last_seen)
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
                .map(|e| e.command_preview.clone());
            HistoryEntry {
                rule_id: rule_id.clone(),
                last_seen_unix: ts,
                last_seen_date: date,
                block_count: count,
                command_preview: preview,
            }
        })
        .collect();

    entries.sort_by(|a, b| {
        b.last_seen_unix
            .partial_cmp(&a.last_seen_unix)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    entries.truncate(limit);

    match format {
        "json" => println!("{}", serde_json::to_string_pretty(&entries).unwrap()),
        _ => {
            if entries.is_empty() {
                println!("No block history found.");
                return;
            }
            println!("CRS History — Recent Rule Firings");
            println!("{}", "=".repeat(70));
            println!("{:<12}  {:<30}  {:>6}", "Last Seen", "Rule", "Blocks");
            println!("{}", "-".repeat(54));
            for e in &entries {
                println!(
                    "{:<12}  {:<30}  {:>6}",
                    e.last_seen_date, e.rule_id, e.block_count
                );
                if let Some(ref preview) = e.command_preview {
                    let p = if preview.len() > 60 {
                        &preview[..60]
                    } else {
                        preview
                    };
                    println!("              └─ {p}");
                }
            }
        }
    }
}

fn cmd_export(out_path: Option<&str>) {
    use crs_core::rules::load as load_rules;
    use crs_core::state::{load as load_state, state_path};
    use crs_core::stats::{load as load_stats, stats_path};

    let rules_cfg = load_rules();
    let stats = load_stats(&stats_path());
    let state = load_state(&state_path(&rules_cfg.failure_learning));

    let snapshot = serde_json::json!({
        "exported_at": chrono::Local::now().to_rfc3339(),
        "rules": rules_cfg.rules.iter().map(|r| serde_json::json!({
            "id": r.id,
            "enabled": r.enabled,
            "pattern": r.pattern,
            "pattern_flags": r.pattern_flags,
            "exceptions": r.exceptions,
            "message": r.message,
        })).collect::<Vec<_>>(),
        "failure_learning": {
            "enabled": rules_cfg.failure_learning.enabled,
            "block_threshold": rules_cfg.failure_learning.block_threshold,
            "window_seconds": rules_cfg.failure_learning.window_seconds,
            "max_tracked_commands": rules_cfg.failure_learning.max_tracked_commands,
        },
        "stats": {
            "blocks": stats.blocks,
            "last_seen": stats.last_seen,
        },
        "failure_state_entries": state.failures.len(),
    });

    let json = serde_json::to_string_pretty(&snapshot).unwrap();

    match out_path {
        Some(path) => {
            std::fs::write(path, &json).unwrap_or_else(|e| {
                eprintln!("crs export: failed to write {path}: {e}");
                std::process::exit(1);
            });
            eprintln!("exported to {path}");
        }
        None => println!("{json}"),
    }
}

fn cmd_heat(rule_filter: Option<&str>) {
    use crs_core::heat::build;
    use crs_core::stats::{load as load_stats, stats_path};

    let stats = load_stats(&stats_path());

    if stats.last_seen.is_empty() {
        println!("No block history to plot — run some commands first.");
        return;
    }

    // Build firing list from last_seen: one data point per rule (the most recent firing).
    // This is sparse by design — stats only records the most recent timestamp per rule,
    // not a full log. The heatmap shows *when rules last fired*, not full density.
    let firings: Vec<(String, u64)> = stats
        .last_seen
        .iter()
        .filter(|(rule_id, _)| rule_filter.map(|f| f == rule_id.as_str()).unwrap_or(true))
        .map(|(rule_id, &ts)| (rule_id.clone(), ts as u64))
        .collect();

    if firings.is_empty() {
        println!("No data for rule filter.");
        return;
    }

    let hm = build(&firings);

    let title = match rule_filter {
        Some(r) => format!("CRS Heat — Rule firing times for [{r}]"),
        None => "CRS Heat — Rule firing times (hour × day)".to_string(),
    };
    println!("{title}");
    println!("{}", "=".repeat(title.len()));
    println!("Total data points: {}", hm.total_blocks);
    println!("(Note: one data point per rule — shows when each rule last fired)\n");
    print!("{}", hm.render());
}

fn cmd_replay(session_path: Option<&str>, format: &str) {
    use crs_core::replay::{format_text, replay};
    use crs_core::rules::load as load_rules;

    let rules_cfg = load_rules();

    // Resolve session file: explicit path or most-recent .jsonl for current project
    let jsonl_path: std::path::PathBuf = if let Some(p) = session_path {
        std::path::PathBuf::from(p)
    } else {
        let root = std::env::var("CLAUDE_PROJECTS_DIR")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|_| dirs::home_dir().expect("home dir").join(".claude/projects"));
        let current_dir = std::env::current_dir().ok();
        find_most_recent_session(&root, current_dir.as_deref()).unwrap_or_else(|| {
            eprintln!("crs replay: no session found for current project. Use --session <path>.");
            std::process::exit(1);
        })
    };

    // Parse commands from the JSONL file
    let content = std::fs::read_to_string(&jsonl_path).unwrap_or_else(|e| {
        eprintln!("crs replay: cannot read {}: {e}", jsonl_path.display());
        std::process::exit(1);
    });

    let commands: Vec<String> = content
        .lines()
        .filter_map(|line| {
            let v: serde_json::Value = serde_json::from_str(line).ok()?;
            if v.get("type").and_then(|t| t.as_str()) != Some("assistant") {
                return None;
            }
            v.get("message")
                .and_then(|m| m.get("content"))
                .and_then(|c| c.as_array())
                .into_iter()
                .flatten()
                .find_map(|block| {
                    if block.get("type").and_then(|t| t.as_str()) != Some("tool_use") {
                        return None;
                    }
                    if block.get("name").and_then(|n| n.as_str()) != Some("Bash") {
                        return None;
                    }
                    block
                        .get("input")
                        .and_then(|i| i.get("command"))
                        .and_then(|c| c.as_str())
                        .map(|s| s.to_string())
                })
        })
        .collect();

    if commands.is_empty() {
        println!("No Bash commands found in {}", jsonl_path.display());
        return;
    }

    let report = replay(&commands, &rules_cfg.rules);

    match format {
        "json" => {
            let out = serde_json::json!({
                "session": jsonl_path.display().to_string(),
                "total": report.entries.len(),
                "blocked": report.blocked,
                "passed": report.passed,
                "entries": report.entries.iter().map(|e| {
                    match &e.verdict {
                        crs_core::replay::ReplayVerdict::Blocked { rule_id, message } => serde_json::json!({
                            "command": e.command,
                            "verdict": "blocked",
                            "rule_id": rule_id,
                            "message": message,
                        }),
                        crs_core::replay::ReplayVerdict::Pass => serde_json::json!({
                            "command": e.command,
                            "verdict": "pass",
                        }),
                    }
                }).collect::<Vec<_>>(),
            });
            println!("{}", serde_json::to_string_pretty(&out).unwrap());
        }
        _ => {
            println!("Session: {}", jsonl_path.display());
            print!("{}", format_text(&report));
        }
    }
}

/// Find the most recently modified `.jsonl` file matching the current project directory.
fn find_most_recent_session(
    root: &std::path::Path,
    current_dir: Option<&std::path::Path>,
) -> Option<std::path::PathBuf> {
    use walkdir::WalkDir;

    let cwd_str = current_dir.map(|p| p.to_string_lossy().to_string());

    let mut best: Option<(std::time::SystemTime, std::path::PathBuf)> = None;

    for entry in WalkDir::new(root)
        .min_depth(1)
        .max_depth(3)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let path = entry.into_path();
        if path.extension().map(|x| x == "jsonl").unwrap_or(false) && path.is_file() {
            // Quick project filter: read first line, check cwd
            if let Some(ref cwd) = cwd_str {
                let ok = std::fs::read_to_string(&path)
                    .ok()
                    .and_then(|s| s.lines().next().map(|l| l.to_string()))
                    .and_then(|l| serde_json::from_str::<serde_json::Value>(&l).ok())
                    .and_then(|v| {
                        v.get("cwd")
                            .and_then(|c| c.as_str())
                            .map(|s| s == cwd.as_str())
                    })
                    .unwrap_or(false);
                if !ok {
                    continue;
                }
            }
            if let Ok(meta) = std::fs::metadata(&path)
                && let Ok(modified) = meta.modified()
                && best.as_ref().map(|(t, _)| modified > *t).unwrap_or(true)
            {
                best = Some((modified, path));
            }
        }
    }

    best.map(|(_, p)| p)
}

#[cfg(test)]
mod cli_tests {
    use super::*;
    use clap::Parser;

    #[test]
    fn discover_default_no_generate_filters() {
        let cli = Cli::try_parse_from(["crs", "discover"]).unwrap();
        match cli.command {
            Command::Discover {
                generate_filters, ..
            } => {
                assert!(!generate_filters);
            }
            _ => panic!("expected Discover"),
        }
    }

    #[test]
    fn discover_generate_filters_flag() {
        let cli = Cli::try_parse_from(["crs", "discover", "--generate-filters"]).unwrap();
        match cli.command {
            Command::Discover {
                generate_filters, ..
            } => {
                assert!(generate_filters);
            }
            _ => panic!("expected Discover"),
        }
    }

    #[test]
    fn write_obfsck_filters_merges_existing() {
        use tempfile::TempDir;

        let dir = TempDir::new().unwrap();
        let path = dir.path().join("obfsck-filters.yaml");

        // Write an existing file with one pattern
        let existing = "# Generated by crs discover\nfilters:\n  - label: existing\n    pattern: \"existing-pat\"\n";
        std::fs::write(&path, existing).unwrap();

        let new_suggestions = vec![
            crs_core::obfsck::FilterSuggestion {
                label: "new-label".to_string(),
                pattern: "new-pat".to_string(),
            },
            // duplicate of existing — should not double-add
            crs_core::obfsck::FilterSuggestion {
                label: "existing".to_string(),
                pattern: "existing-pat".to_string(),
            },
        ];

        write_obfsck_filters(&new_suggestions, path.clone());

        let content = std::fs::read_to_string(&path).unwrap();
        // existing pattern retained
        assert!(
            content.contains("existing-pat"),
            "existing pattern must be retained"
        );
        // new pattern added
        assert!(content.contains("new-pat"), "new pattern must be added");
        // no duplicate label — "existing" should appear exactly once as a label value
        let label_count = content
            .lines()
            .filter(|l| l.trim() == "- label: existing")
            .count();
        assert_eq!(label_count, 1, "duplicate label must not be written twice");
    }

    #[test]
    fn rewrite_applies_rx_prefix_when_prefixes_toml_present() {
        use crs_core::rx_prefix::{RxPrefixConfig, rewrite_command};
        use std::collections::HashMap;

        let config = RxPrefixConfig {
            mappings: HashMap::from([(
                "gh".to_string(),
                vec![
                    "op".to_string(),
                    "plugin".to_string(),
                    "run".to_string(),
                    "--".to_string(),
                ],
            )]),
            candidate_prefixes: vec![],
            learn_on_successful_fallback: false,
        };
        let result = rewrite_command("gh issue list", &config);
        assert_eq!(result.rewritten, "op plugin run -- gh issue list");
        assert!(result.probes.is_empty());
    }

    #[test]
    fn rx_learning_confirms_mapping_on_success() {
        use crs_core::rx_prefix::{FilePrefixStore, FileProbeStore, PrefixStore as _, ProbeEntry};
        use tempfile::TempDir;

        let dir = TempDir::new().unwrap();
        let probe_path = dir.path().join("rx-candidates.toml");
        let prefixes_path = dir.path().join("prefixes.toml");

        let probe_store = FileProbeStore {
            path: probe_path.clone(),
        };
        probe_store.write(&[ProbeEntry {
            key: "gh".to_string(),
            prefix: vec![
                "op".to_string(),
                "plugin".to_string(),
                "run".to_string(),
                "--".to_string(),
            ],
            original_command: "gh issue list".to_string(),
        }]);

        let prefix_store = FilePrefixStore {
            path: prefixes_path.clone(),
        };
        apply_rx_learning("gh issue list", 0, &probe_store, &prefix_store);

        let config = prefix_store.load();
        assert_eq!(
            config.mappings.get("gh"),
            Some(&vec![
                "op".to_string(),
                "plugin".to_string(),
                "run".to_string(),
                "--".to_string(),
            ])
        );
        assert!(probe_store.load().is_empty());
    }

    #[test]
    fn rx_learning_removes_probe_on_failure() {
        use crs_core::rx_prefix::PrefixStore as _;
        use crs_core::rx_prefix::{FilePrefixStore, FileProbeStore, ProbeEntry};
        use tempfile::TempDir;

        let dir = TempDir::new().unwrap();
        let probe_path = dir.path().join("rx-candidates.toml");
        let prefixes_path = dir.path().join("prefixes.toml");

        let probe_store = FileProbeStore {
            path: probe_path.clone(),
        };
        probe_store.write(&[ProbeEntry {
            key: "gh".to_string(),
            prefix: vec!["op".to_string()],
            original_command: "gh issue list".to_string(),
        }]);

        let prefix_store = FilePrefixStore {
            path: prefixes_path.clone(),
        };
        apply_rx_learning("gh issue list", 1, &probe_store, &prefix_store);

        assert!(probe_store.load().is_empty());
        assert!(prefix_store.load().mappings.is_empty());
    }

    #[test]
    fn audit_subcommand_parses() {
        let cli = Cli::try_parse_from(["crs", "audit"]).unwrap();
        assert!(matches!(cli.command, Command::Audit { remove: None }));
    }

    #[test]
    fn audit_remove_flag_parses() {
        let cli = Cli::try_parse_from(["crs", "audit", "--remove", "gh"]).unwrap();
        assert!(matches!(cli.command, Command::Audit { remove: Some(ref k) } if k == "gh"));
    }

    #[test]
    fn audit_state_empty_stores_returns_empty() {
        use crs_core::rx_prefix::{FilePrefixStore, FileProbeStore, audit_state};
        use tempfile::TempDir;

        let dir = TempDir::new().unwrap();
        let prefix_store = FilePrefixStore {
            path: dir.path().join("prefixes.toml"),
        };
        let probe_store = FileProbeStore {
            path: dir.path().join("rx-candidates.toml"),
        };

        let state = audit_state(&prefix_store, &probe_store);
        assert!(state.mappings.is_empty());
        assert!(state.probes.is_empty());
    }

    #[test]
    fn audit_state_returns_sorted_mappings_and_probes() {
        use crs_core::rx_prefix::{
            FilePrefixStore, FileProbeStore, PrefixStore as _, ProbeEntry, ProbeStore as _,
            audit_state,
        };
        use tempfile::TempDir;

        let dir = TempDir::new().unwrap();
        let prefix_store = FilePrefixStore {
            path: dir.path().join("prefixes.toml"),
        };
        let probe_store = FileProbeStore {
            path: dir.path().join("rx-candidates.toml"),
        };

        prefix_store.confirm_mapping(
            "gh",
            &[
                "op".to_string(),
                "plugin".to_string(),
                "run".to_string(),
                "--".to_string(),
            ],
        );
        prefix_store.confirm_mapping(
            "cargo",
            &["dotenvx".to_string(), "run".to_string(), "--".to_string()],
        );
        probe_store.write(&[ProbeEntry {
            key: "gh".to_string(),
            prefix: vec!["op".to_string()],
            original_command: "gh issue list".to_string(),
        }]);

        let state = audit_state(&prefix_store, &probe_store);
        // Sorted: cargo before gh
        assert_eq!(state.mappings[0].0, "cargo");
        assert_eq!(state.mappings[1].0, "gh");
        assert_eq!(state.probes.len(), 1);
        assert_eq!(state.probes[0].key, "gh");
    }

    #[test]
    fn remove_mapping_returns_true_on_hit() {
        use crs_core::rx_prefix::{FilePrefixStore, PrefixStore as _};
        use tempfile::TempDir;

        let dir = TempDir::new().unwrap();
        let path = dir.path().join("prefixes.toml");
        let store = FilePrefixStore { path: path.clone() };
        store.confirm_mapping(
            "gh",
            &[
                "op".to_string(),
                "plugin".to_string(),
                "run".to_string(),
                "--".to_string(),
            ],
        );

        assert!(store.remove_mapping("gh"));
        assert!(store.load().mappings.is_empty());
    }

    #[test]
    fn remove_mapping_returns_false_on_miss() {
        use crs_core::rx_prefix::FilePrefixStore;
        use tempfile::TempDir;

        let dir = TempDir::new().unwrap();
        let path = dir.path().join("prefixes.toml");
        let store = FilePrefixStore { path };
        assert!(!store.remove_mapping("nonexistent"));
    }

    #[test]
    fn filter_redacts_output_matching_obfsck_patterns() {
        use crs_core::filters::{ObfsckFilters, RedactRule, apply_redaction};

        let filters = ObfsckFilters {
            filters: vec![RedactRule {
                label: "api-key".to_string(),
                pattern: r"sk-[A-Za-z0-9]{10,}".to_string(),
            }],
        };
        let output = "normal line\nsk-abc1234567890 leaked\nclean";
        let result = apply_redaction(output, &filters);
        assert!(result.contains("[REDACTED]"));
        assert!(!result.contains("sk-abc1234567890"));
        assert!(result.contains("normal line"));
    }
}
