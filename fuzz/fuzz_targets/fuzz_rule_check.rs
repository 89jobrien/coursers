#![no_main]

use libfuzzer_sys::fuzz_target;
use coursers_core::rules::{Rule, check, check_pipeline, matched_rule_id};

fn test_rules() -> Vec<Rule> {
    vec![
        Rule {
            id: "no-grep".to_string(),
            enabled: true,
            pattern: r"\bgrep\b".to_string(),
            pattern_flags: String::new(),
            exceptions: vec![r"\|\s*grep".to_string()],
            target_commands: vec!["grep".to_string(), "rg".to_string()],
            message: Some("Use Grep tool.".to_string()),
        },
        Rule {
            id: "no-cat".to_string(),
            enabled: true,
            pattern: r"\bcat\s+[^|<]".to_string(),
            pattern_flags: String::new(),
            exceptions: vec![],
            target_commands: vec!["cat".to_string()],
            message: Some("Use Read tool.".to_string()),
        },
        Rule {
            id: "no-find".to_string(),
            enabled: true,
            pattern: r#"\bfind\s+[./~$"']"#.to_string(),
            pattern_flags: String::new(),
            exceptions: vec![],
            target_commands: vec!["find".to_string()],
            message: Some("Use Glob tool.".to_string()),
        },
    ]
}

fuzz_target!(|data: &[u8]| {
    if let Ok(s) = std::str::from_utf8(data) {
        let rules = test_rules();

        // check, check_pipeline, and matched_rule_id must never panic.
        let chk = check(s, &rules);
        let _pip = check_pipeline(s, &rules);
        let mid = matched_rule_id(s, &rules);

        // check and matched_rule_id must agree.
        assert_eq!(
            chk.is_some(),
            mid.is_some(),
            "check/matched_rule_id disagree on: {s:?}"
        );

        // If check returns a rule_id, matched_rule_id returns the same one.
        if let (Some((rule_id, _)), Some(mid_id)) = (&chk, &mid) {
            assert_eq!(
                rule_id, mid_id,
                "rule_id mismatch on: {s:?}"
            );
        }
    }
});
