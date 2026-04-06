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
    /// Discover missed savings from Claude Code history
    Discover {
        #[arg(short, long)]
        all: bool,
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
        Command::Discover { all } => {
            eprintln!("discover (all={all}): not yet implemented");
        }
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
