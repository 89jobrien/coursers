//! Property tests for crs-core modules: rules, date, ast.

use proptest::prelude::*;

// ---------------------------------------------------------------------------
// rules::check — exception always overrides a matching rule
// ---------------------------------------------------------------------------

use coursers_core::rules::{Rule, check};

fn make_rule_with_exception(pattern: &str, exception: &str) -> Rule {
    Rule {
        id: "test-rule".to_string(),
        enabled: true,
        pattern: pattern.to_string(),
        pattern_flags: String::new(),
        exceptions: vec![exception.to_string()],
        target_commands: vec![],
        message: None,
    }
}

proptest! {
    /// If a command matches both the rule pattern AND an exception, check() must
    /// return None (the exception overrides).
    #[test]
    fn exception_always_overrides_matching_rule(
        word in "[a-z]{3,10}"
    ) {
        // Rule matches any command containing `word`; exception also matches `word`.
        // So every command containing `word` should be excepted.
        let rule = make_rule_with_exception(&word, &word);
        let command = format!("run {word} now");
        prop_assert!(
            check(&command, &[rule]).is_none(),
            "exception must override: command = {command}"
        );
    }

    /// A disabled rule never blocks, regardless of pattern match.
    #[test]
    fn disabled_rule_never_blocks(command in "[a-zA-Z0-9 ._/-]{1,40}") {
        let rule = Rule {
            id: "disabled".to_string(),
            enabled: false,
            pattern: ".*".to_string(),  // matches everything
            pattern_flags: String::new(),
            exceptions: vec![],
            target_commands: vec![],
            message: None,
        };
        prop_assert!(check(&command, &[rule]).is_none());
    }
}

// ---------------------------------------------------------------------------
// date — validity and monotonicity
// ---------------------------------------------------------------------------

use coursers_core::date::unix_secs_to_ymd;

proptest! {
    /// Month is always in 1..=12, day is always in 1..=31.
    #[test]
    fn ymd_produces_valid_ranges(secs in 0u64..=(1u64 << 40)) {
        let (_, m, d) = unix_secs_to_ymd(secs);
        prop_assert!((1..=12).contains(&m), "month out of range: {m}");
        prop_assert!((1..=31).contains(&d), "day out of range: {d}");
    }

    /// Monotonicity: s1 < s2 implies (y1,m1,d1) <= (y2,m2,d2).
    #[test]
    fn ymd_monotonic(s1 in 0u32..u32::MAX) {
        let s2 = s1.saturating_add(1);
        if s1 < s2 {
            let (y1, m1, d1) = unix_secs_to_ymd(s1 as u64);
            let (y2, m2, d2) = unix_secs_to_ymd(s2 as u64);
            prop_assert!(
                (y2, m2, d2) >= (y1, m1, d1),
                "monotonicity violated: {y1}-{m1}-{d1} > {y2}-{m2}-{d2}"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// stats::StatsStore — record_block count invariant
// ---------------------------------------------------------------------------

use coursers_core::stats::{InMemoryStatsStore, StatsStore};

proptest! {
    /// After N calls to record_block(rule_id), blocks[rule_id] == N.
    #[test]
    fn record_block_count_equals_call_count(
        rule_id in "[a-z]{1,20}",
        n in 1u32..10u32,
    ) {
        let store = InMemoryStatsStore::new();
        for _ in 0..n {
            store.record_block(&rule_id).expect("record_block must succeed");
        }
        let stats = store.load().expect("load must succeed");
        let count = stats.blocks.get(&rule_id).copied().unwrap_or(0);
        prop_assert_eq!(count, n as u64);
    }

    /// record_block for one rule does not affect another rule's count.
    #[test]
    fn record_block_independent_rules(
        rule_a in "[a-z]{1,10}",
        rule_b in "[A-Z]{1,10}",
        n_a in 1u32..5u32,
        n_b in 1u32..5u32,
    ) {
        let store = InMemoryStatsStore::new();
        for _ in 0..n_a {
            store.record_block(&rule_a).unwrap();
        }
        for _ in 0..n_b {
            store.record_block(&rule_b).unwrap();
        }
        let stats = store.load().unwrap();
        let count_a = stats.blocks.get(&rule_a).copied().unwrap_or(0);
        let count_b = stats.blocks.get(&rule_b).copied().unwrap_or(0);
        prop_assert_eq!(count_a, n_a as u64);
        prop_assert_eq!(count_b, n_b as u64);
    }
}

// ---------------------------------------------------------------------------
// ast::parse — non-empty input produces non-empty argv
// ---------------------------------------------------------------------------

use coursers_core::ast::parse;

proptest! {
    /// If input contains at least one non-whitespace char and is valid shell syntax,
    /// parse() returns Some with non-empty argv.
    #[test]
    fn parse_nonempty_produces_nonempty_argv(
        word in "[a-zA-Z0-9_.-]{1,20}"
    ) {
        let result = parse(&word);
        prop_assert!(result.is_some(), "parse returned None for: {word:?}");
        let cmd = result.unwrap();
        prop_assert!(!cmd.argv.is_empty(), "argv empty for: {word:?}");
    }

    /// parse() always returns None for empty or whitespace-only input.
    #[test]
    fn parse_whitespace_returns_none(ws in "[ \t\n]{0,20}") {
        prop_assert!(parse(&ws).is_none());
    }

    /// name() always returns the first element of argv (or "" for empty).
    #[test]
    fn name_equals_first_argv(
        cmd in "[a-z]{1,10}( [a-z]{1,10}){0,3}"
    ) {
        if let Some(parsed) = parse(&cmd) {
            prop_assert_eq!(parsed.name(), &parsed.argv[0]);
        }
    }
}
