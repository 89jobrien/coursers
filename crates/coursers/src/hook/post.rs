use crs_core::{rules, state};
use super::read_stdin;

const SIGNAL_EXIT_CODES: &[i64] = &[130, 137, 143];
const EXCLUDE_PATTERNS: &[&str] = &[
    r"^\s*false\s*$",
    r"\|\|\s*(true|:)\s*$",
    r";\s*(true|:)\s*$",
    r"^\s*\[",
    r"\btest\s+-[defhlrswxz]\b",
    r"2>/dev/null",
    r">/dev/null\s+2>&1",
];

pub fn run() {
    let Some(payload) = read_stdin() else {
        return;
    };

    if payload.tool_name.as_deref() != Some("Bash") {
        return;
    }

    let exit_code = payload
        .tool_response
        .as_ref()
        .and_then(|r| r.get("exit_code"))
        .and_then(|v| v.as_i64())
        .unwrap_or(0);

    if exit_code == 0 {
        return;
    }
    if SIGNAL_EXIT_CODES.contains(&exit_code) {
        return;
    }

    let command = match payload.tool_input.as_ref().and_then(|i| i.command.as_deref()) {
        Some(c) if !c.is_empty() => c,
        _ => return,
    };

    if is_excluded(command) {
        return;
    }

    let config = rules::load();
    let fl = &config.failure_learning;
    if !fl.enabled {
        return;
    }

    let path = state::state_path(fl);
    let st = state::load(&path);
    let st = state::record_failure(st, command, fl);
    state::save(&path, &st);
}

fn is_excluded(command: &str) -> bool {
    EXCLUDE_PATTERNS.iter().any(|pat| {
        regex::Regex::new(pat)
            .map(|re| re.is_match(command))
            .unwrap_or(false)
    })
}
