use crs_core::{rules, state};
use super::{deny, read_stdin};

pub fn run() {
    let Some(payload) = read_stdin() else {
        return;
    };

    if payload.tool_name.as_deref() != Some("Bash") {
        return;
    }

    let command = match payload.tool_input.as_ref().and_then(|i| i.command.as_deref()) {
        Some(c) if !c.is_empty() => c,
        _ => return,
    };

    let config = rules::load();

    // 1. Predefined rules
    if let Some(msg) = rules::check(command, &config.rules) {
        deny(&msg);
    }

    // 2. Learned failures
    let fl = &config.failure_learning;
    if fl.enabled {
        let path = state::state_path(fl);
        let st = state::load(&path);
        if let Some(msg) = state::check_learned(command, &st, fl) {
            deny(&msg);
        }
    }
}

