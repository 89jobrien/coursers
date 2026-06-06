#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(s) = std::str::from_utf8(data) {
        // pipe_stages must never panic on any valid UTF-8 input.
        let stages = crs_core::pipeline::pipe_stages(s);

        // All stages must be non-empty and trimmed.
        for stage in &stages {
            assert!(!stage.is_empty(), "empty stage from: {s:?}");
            assert_eq!(*stage, stage.trim(), "untrimmed stage from: {s:?}");
        }

        // pipe_stage_commands must also never panic.
        let cmds = crs_core::pipeline::pipe_stage_commands(s);
        for cmd in &cmds {
            assert!(!cmd.is_empty(), "empty command name from: {s:?}");
        }

        // Command count can never exceed stage count.
        assert!(
            cmds.len() <= stages.len(),
            "more commands than stages: {} > {} for {s:?}",
            cmds.len(),
            stages.len()
        );
    }
});
