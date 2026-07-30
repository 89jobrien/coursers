use std::path::PathBuf;

/// Approximate bytes per token (GPT/Claude tokenizer average).
pub const BYTES_PER_TOKEN: usize = 4;

/// Which hook protocol to use for output formatting and exit codes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HookProtocol {
    /// Claude Code: exit 2 for deny.
    #[default]
    Claude,
    /// Codex: exit 0 + JSON `permissionDecision: "deny"`.
    Codex,
}

/// Resolved paths for a named profile (or the default profile).
pub struct ProfileConfig {
    /// Path to the rules JSON file.
    pub rules_path: PathBuf,
    /// Path to the global (home-dir) state file.
    pub global_state_path: PathBuf,
    /// Project-local state path (`.ctx/crs-<profile>-state.json`).
    pub local_state_path: PathBuf,
    /// Hook I/O protocol (Claude vs Codex).
    pub protocol: HookProtocol,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hook_protocol_default_is_claude() {
        assert_eq!(HookProtocol::default(), HookProtocol::Claude);
    }

    #[test]
    fn hook_protocol_variants_are_distinct() {
        assert_ne!(HookProtocol::Claude, HookProtocol::Codex);
    }

    #[test]
    fn bytes_per_token_is_four() {
        assert_eq!(BYTES_PER_TOKEN, 4);
    }

    #[test]
    fn profile_config_holds_paths() {
        let cfg = ProfileConfig {
            rules_path: PathBuf::from("/rules.json"),
            global_state_path: PathBuf::from("/global.json"),
            local_state_path: PathBuf::from(".ctx/local.json"),
            protocol: HookProtocol::Codex,
        };
        assert_eq!(cfg.rules_path, PathBuf::from("/rules.json"));
        assert_eq!(cfg.protocol, HookProtocol::Codex);
    }
}
