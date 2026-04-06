use crs_core::loader::RulesLoader;
use crs_core::store::StateStore;
use crs_core::{rules, state};

use super::{deny, HookPayload};

pub fn run_with<L: RulesLoader, S: StateStore>(loader: &L, store: &S, payload: &HookPayload) {
    if payload.tool_name.as_deref() != Some("Bash") {
        return;
    }

    let command = match payload.tool_input.as_ref().and_then(|i| i.command.as_deref()) {
        Some(c) if !c.is_empty() => c,
        _ => return,
    };

    let config = loader.load();

    // 1. Predefined rules
    if let Some(msg) = rules::check(command, &config.rules) {
        deny(&msg);
    }

    // 2. Learned failures
    let fl = &config.failure_learning;
    if fl.enabled {
        let st = store.load();
        if let Some(msg) = state::check_learned(command, &st, fl) {
            deny(&msg);
        }
    }
}

pub fn run() {
    use crs_core::loader::FsRulesLoader;
    use crs_core::state::state_path;
    use crs_core::store::FsStateStore;

    let Some(payload) = super::read_stdin() else {
        return;
    };

    let loader = FsRulesLoader;
    let config = loader.load();
    let path = state_path(&config.failure_learning);
    let store = FsStateStore { path };

    run_with(&loader, &store, &payload);
}
