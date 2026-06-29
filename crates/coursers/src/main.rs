mod crs_commands;
mod hook;
pub mod nu_check;
pub mod obfsck;
pub mod rtk;

use clap::{Parser, Subcommand};
use coursers_core::config::ConfigBuilder;
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "coursers",
    about = "Claude Code course-correction hook pipeline"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// PreToolUse hook — reads JSON payload from stdin, writes hook response to stdout
    Pre {
        /// Named profile (resolves to ~/.config/coursers/profiles/<name>/)
        #[arg(long)]
        profile: Option<String>,
        /// Override rules file path
        #[arg(long)]
        rules: Option<PathBuf>,
        /// Override global state file path
        #[arg(long)]
        state: Option<PathBuf>,
    },
    /// PostToolUse hook — reads JSON payload from stdin, records failures
    Post {
        /// Named profile (resolves to ~/.config/coursers/profiles/<name>/)
        #[arg(long)]
        profile: Option<String>,
        /// Override rules file path
        #[arg(long)]
        rules: Option<PathBuf>,
        /// Override global state file path
        #[arg(long)]
        state: Option<PathBuf>,
    },
    /// Filter PostToolUse output — reads hook JSON from stdin, emits hook response to stdout
    Filter {
        #[arg(long)]
        profile: Option<String>,
        #[arg(long)]
        rules: Option<PathBuf>,
        #[arg(long)]
        state: Option<PathBuf>,
    },
    /// Rewrite a PreToolUse command — reads hook JSON from stdin, emits rewritten command or exits 1
    Rewrite {
        #[arg(long)]
        profile: Option<String>,
        #[arg(long)]
        rules: Option<PathBuf>,
    },
    /// Discover missed savings from Claude Code session history
    Discover {
        #[arg(long)]
        profile: Option<String>,
        #[arg(long)]
        rules: Option<PathBuf>,
        #[arg(short, long)]
        all: bool,
        #[arg(short, long, default_value = "15")]
        limit: usize,
        #[arg(short, long, default_value = "30")]
        since: u32,
        #[arg(short, long, default_value = "text")]
        format: String,
        #[arg(long)]
        generate_filters: bool,
        #[arg(long, default_value = "1")]
        min_count: u64,
    },
    /// Validate rules: check patterns compile, examples fire, exceptions work
    Validate {
        #[arg(long)]
        profile: Option<String>,
        #[arg(long)]
        rules: Option<PathBuf>,
    },
    /// Probe a command against all rules — reads command from stdin
    Probe {
        #[arg(long)]
        profile: Option<String>,
        #[arg(long)]
        rules: Option<PathBuf>,
    },
    /// Show cumulative block counts by rule
    Stats {
        #[arg(long)]
        profile: Option<String>,
    },
    /// Analyze session facets enriched with git context
    Insights {
        #[arg(short, long, default_value = "text")]
        format: String,
        #[arg(short, long)]
        since: Option<u32>,
        #[arg(short, long)]
        repo: Option<String>,
    },
    /// Show rx prefix learning state
    Audit {
        #[arg(long)]
        remove: Option<String>,
    },
    /// Suggest new rules from unhandled commands
    Suggest {
        #[arg(long)]
        profile: Option<String>,
        #[arg(long)]
        rules: Option<PathBuf>,
        #[arg(short, long)]
        all: bool,
        #[arg(short, long, default_value = "30")]
        since: u32,
        #[arg(short, long, default_value = "10")]
        limit: usize,
        #[arg(short, long, default_value = "text")]
        format: String,
    },
    /// Show recent blocked commands
    History {
        #[arg(short, long, default_value = "20")]
        limit: usize,
        #[arg(short, long)]
        rule: Option<String>,
        #[arg(short, long, default_value = "text")]
        format: String,
    },
    /// Dump rules + stats + state as portable JSON
    Export {
        #[arg(short, long)]
        out: Option<String>,
    },
    /// Run the generic hook pipeline for a Claude Code event
    Hook { event: String },
    /// Validate hook pipeline config
    ValidateHooks {
        #[arg(long, default_value = "claude")]
        target: String,
    },
    /// Query the hook execution log
    Log {
        #[arg(short, long, default_value = "20")]
        limit: usize,
        #[arg(short, long)]
        event: Option<String>,
        #[arg(short, long)]
        outcome: Option<String>,
        #[arg(short, long, default_value = "text")]
        format: String,
        #[arg(long)]
        prune_hours: Option<u64>,
    },
    /// Show heatmap of rule firings
    Heat {
        #[arg(short, long)]
        rule: Option<String>,
    },
    /// Replay a session's Bash commands through the current ruleset
    Replay {
        #[arg(short, long)]
        session: Option<String>,
        #[arg(short, long, default_value = "text")]
        format: String,
    },
    /// Validate nu scripts using `nu --ide-check`
    NuCheck {
        #[arg(value_name = "FILE")]
        files: Vec<String>,
        #[arg(long)]
        hooks: bool,
        #[arg(long)]
        nu_libs: bool,
    },
}

fn build_profile(
    profile: Option<String>,
    rules: Option<PathBuf>,
    state: Option<PathBuf>,
) -> coursers_core::config::ProfileConfig {
    let mut b = ConfigBuilder::new();
    if let Some(p) = profile {
        b = b.profile(p);
    }
    if let Some(r) = rules {
        b = b.rules(r);
    }
    if let Some(s) = state {
        b = b.state(s);
    }
    b.build()
}

fn main() {
    let cli = Cli::parse();
    match cli.command {
        Command::Pre {
            profile,
            rules,
            state,
        } => {
            let profile_cfg = build_profile(profile, rules, state);
            hook::pre::run_with_profile(&profile_cfg);
        }
        Command::Post {
            profile,
            rules,
            state,
        } => {
            let profile_cfg = build_profile(profile, rules, state);
            hook::post::run_with_profile(&profile_cfg);
        }
        Command::Filter {
            profile,
            rules,
            state,
        } => {
            let _profile_cfg = crs_commands::resolve_profile(profile, rules, state);
            crs_commands::cmd_filter();
        }
        Command::Rewrite { profile, rules } => {
            let _profile_cfg = crs_commands::resolve_profile(profile, rules, None);
            crs_commands::cmd_rewrite();
        }
        Command::Discover {
            profile,
            rules,
            all,
            limit,
            since,
            format,
            generate_filters,
            min_count,
        } => {
            let profile_cfg = crs_commands::resolve_profile(profile, rules, None);
            crs_commands::cmd_discover(
                &profile_cfg,
                all,
                limit,
                since,
                &format,
                generate_filters,
                min_count,
            );
        }
        Command::Validate { profile, rules } => {
            let profile_cfg = crs_commands::resolve_profile(profile, rules, None);
            crs_commands::cmd_validate(&profile_cfg);
        }
        Command::Probe { profile, rules } => {
            let profile_cfg = crs_commands::resolve_profile(profile, rules, None);
            crs_commands::cmd_probe(&profile_cfg);
        }
        Command::Stats { profile } => {
            let _profile_cfg = crs_commands::resolve_profile(profile, None, None);
            crs_commands::cmd_stats();
        }
        Command::Insights {
            format,
            since,
            repo,
        } => crs_commands::cmd_insights(&format, since, repo.as_deref()),
        Command::Audit { remove } => crs_commands::cmd_audit(remove),
        Command::Suggest {
            profile,
            rules,
            all,
            since,
            limit,
            format,
        } => {
            let profile_cfg = crs_commands::resolve_profile(profile, rules, None);
            crs_commands::cmd_suggest(&profile_cfg, all, since, limit, &format);
        }
        Command::History {
            limit,
            rule,
            format,
        } => crs_commands::cmd_history(limit, rule.as_deref(), &format),
        Command::Export { out } => crs_commands::cmd_export(out.as_deref()),
        Command::Hook { event } => crs_commands::cmd_hook(&event),
        Command::ValidateHooks { ref target } => {
            if target == "codex" {
                crs_commands::cmd_validate_codex_hooks();
            } else {
                crs_commands::cmd_validate_hooks();
            }
        }
        Command::Log {
            limit,
            event,
            outcome,
            format,
            prune_hours,
        } => crs_commands::cmd_log(
            limit,
            event.as_deref(),
            outcome.as_deref(),
            &format,
            prune_hours,
        ),
        Command::Heat { rule } => crs_commands::cmd_heat(rule.as_deref()),
        Command::Replay { session, format } => {
            crs_commands::cmd_replay(session.as_deref(), &format);
        }
        Command::NuCheck {
            files,
            hooks,
            nu_libs,
        } => crs_commands::cmd_nu_check(&files, hooks, nu_libs),
    }
}
