use crs_core::loader::RulesLoader;
use crs_core::store::StateStore;
use crs_core::state;

use super::HookPayload;

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

pub fn run_with<L: RulesLoader, S: StateStore>(loader: &L, store: &S, payload: &HookPayload) {
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

    let config = loader.load();
    let fl = &config.failure_learning;
    if !fl.enabled {
        return;
    }

    let st = store.load();
    let st = state::record_failure(st, command, fl);
    store.save(&st);
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

fn is_excluded(command: &str) -> bool {
    EXCLUDE_PATTERNS.iter().any(|pat| {
        regex::Regex::new(pat)
            .map(|re| re.is_match(command))
            .unwrap_or(false)
    })
}
