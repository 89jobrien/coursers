#![no_main]

use libfuzzer_sys::fuzz_target;

// RulesConfig is deserialized directly from the untrusted
// ~/.config/coursers/course-correct-rules.json file (see coursers-core::rules
// and crs validate/probe/discover). Malformed or adversarial JSON must never
// panic serde_json -- it should return an `Err`, not crash.
fuzz_target!(|data: &[u8]| {
    if let Ok(s) = std::str::from_utf8(data) {
        let _ = serde_json::from_str::<coursers_types::rules::RulesConfig>(s);
        let _ = serde_json::from_str::<coursers_types::filters::FiltersConfig>(s);
        let _ = serde_json::from_str::<coursers_types::state::State>(s);
        let _ = serde_json::from_str::<coursers_types::hook::HookPayload>(s);
    }
});
