#![no_main]

use libfuzzer_sys::fuzz_target;
use std::path::Path;

fuzz_target!(|data: &[u8]| {
    let Ok(s) = std::str::from_utf8(data) else {
        return;
    };

    // Split the fuzz input into a pretend `nu --ide-check` NDJSON stdout blob
    // and a pretend source file — the two arguments `parse_diagnostics` treats
    // most differently (arbitrary JSON vs. arbitrary UTF-8 with byte offsets
    // indexed into it).
    let mid = s.len() / 2;
    let mut split = mid;
    while split < s.len() && !s.is_char_boundary(split) {
        split += 1;
    }
    let (output, source) = s.split_at(split);

    // Must never panic — this parses untrusted subprocess output and untrusted
    // file contents, including out-of-range / mid-character byte offsets.
    let diags = coursers::nu_check::parse_diagnostics(output, Path::new("fuzz.nu"), source);

    // Invariant: every produced diagnostic's line/col are 1-based (never 0).
    for d in &diags {
        assert!(d.line >= 1, "line must be 1-based, got {}", d.line);
        assert!(d.col >= 1, "col must be 1-based, got {}", d.col);
    }
});
