#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Ok(s) = std::str::from_utf8(data) else {
        return;
    };

    // parse_session_content must never panic on arbitrary UTF-8 input.
    let records = coursers_core::jsonl_source::parse_session_content(s);

    // Invariant: every command string is non-empty (we filter empty stems in discover,
    // but the parser itself should never produce an empty command string from a
    // well-formed tool_use block — empty ones are simply omitted).
    for r in &records {
        assert!(
            !r.command.is_empty(),
            "parser produced empty command string"
        );
    }

    // Invariant: scanned_commands never exceeds line count.
    let line_count = s.lines().count();
    assert!(
        records.len() <= line_count,
        "more records ({}) than input lines ({})",
        records.len(),
        line_count
    );
});
