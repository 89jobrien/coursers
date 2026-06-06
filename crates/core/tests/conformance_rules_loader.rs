//! Conformance tests for `RulesLoader` implementations.

use crs_core::loader::{InMemoryRulesLoader, RulesLoader};
use crs_core::rules::{FailureLearning, Rule, RulesConfig};

// ---------------------------------------------------------------------------
// Contract assertion
// ---------------------------------------------------------------------------

fn assert_rules_loader_contract(loader: &impl RulesLoader, expected_rule_count: usize) {
    // 1. load() returns a valid RulesConfig
    let config = loader.load();
    assert_eq!(
        config.rules.len(),
        expected_rule_count,
        "contract 1: rule count must match"
    );

    // 2. load() is idempotent — calling twice returns same data
    let config2 = loader.load();
    assert_eq!(
        config.rules.len(),
        config2.rules.len(),
        "contract 2: repeated load must be stable"
    );
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[test]
fn in_memory_loader_satisfies_contract_empty() {
    let config = RulesConfig {
        rules: vec![],
        failure_learning: FailureLearning::default(),
    };
    let loader = InMemoryRulesLoader(config);
    assert_rules_loader_contract(&loader, 0);
}

#[test]
fn in_memory_loader_satisfies_contract_with_rules() {
    let config = RulesConfig {
        rules: vec![
            Rule {
                id: "no-grep".to_string(),
                enabled: true,
                pattern: r"\bgrep\b".to_string(),
                pattern_flags: String::new(),
                exceptions: vec![r"\| grep".to_string()],
                target_commands: vec![],
                message: Some("Use Grep tool".to_string()),
            },
            Rule {
                id: "no-cat".to_string(),
                enabled: true,
                pattern: r"\bcat\b".to_string(),
                pattern_flags: String::new(),
                exceptions: vec![],
                target_commands: vec![],
                message: None,
            },
        ],
        failure_learning: FailureLearning::default(),
    };
    let loader = InMemoryRulesLoader(config);
    assert_rules_loader_contract(&loader, 2);

    // Verify field fidelity
    let loaded = loader.load();
    assert_eq!(loaded.rules[0].id, "no-grep");
    assert_eq!(loaded.rules[0].exceptions.len(), 1);
    assert_eq!(loaded.rules[1].id, "no-cat");
    assert!(loaded.rules[1].message.is_none());
}

#[test]
fn in_memory_loader_returns_exact_config() {
    let config = RulesConfig {
        rules: vec![Rule {
            id: "test".to_string(),
            enabled: false,
            pattern: ".*".to_string(),
            pattern_flags: "i".to_string(),
            exceptions: vec![],
            target_commands: vec![],
            message: Some("blocked".to_string()),
        }],
        failure_learning: FailureLearning {
            enabled: false,
            block_threshold: 5,
            window_seconds: 600,
            state_file: Some("/tmp/test-state.json".to_string()),
            max_tracked_commands: 100,
            cleanup_after_seconds: 7200,
            message_template: Some("custom template".to_string()),
        },
    };
    let loader = InMemoryRulesLoader(config);
    let loaded = loader.load();
    assert!(!loaded.rules[0].enabled);
    assert_eq!(loaded.rules[0].pattern_flags, "i");
    assert!(!loaded.failure_learning.enabled);
    assert_eq!(loaded.failure_learning.block_threshold, 5);
    assert_eq!(loaded.failure_learning.window_seconds, 600);
}
