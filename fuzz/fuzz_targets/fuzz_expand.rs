#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(s) = std::str::from_utf8(data) {
        // expand_vars must never panic on any valid UTF-8 input.
        // The result is always a valid String.
        let _result = crs_core::expand::expand_vars(s);
    }
});
