#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(s) = std::str::from_utf8(data) {
        // parse must never panic on any valid UTF-8 input.
        // It may return None (empty/invalid) or Some(ShellCmd).
        let result = crs_core::ast::parse(s);

        // If it returns Some, argv must be non-empty.
        if let Some(cmd) = result {
            assert!(!cmd.argv.is_empty(), "parse returned Some with empty argv");
        }
    }
});
