#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(s) = std::str::from_utf8(data) {
        // sequential_segments must never panic on any valid UTF-8 input.
        let segments = crs_core::pipeline::sequential_segments(s);

        // All segments must be non-empty and trimmed.
        for seg in &segments {
            assert!(!seg.is_empty(), "empty segment from: {s:?}");
            assert_eq!(*seg, seg.trim(), "untrimmed segment from: {s:?}");
        }
    }
});
