/// Split a shell command on sequential operators: `&&`, `||`, `;`.
///
/// Pipe (`|`) is intentionally excluded — piped commands form a single logical
/// unit and existing exception patterns (e.g. `\| grep`) rely on the full
/// segment context including the pipe.
///
/// Operators inside single-quoted (`'...'`) or double-quoted (`"..."`) strings
/// are skipped. Inside double quotes, `\"` is treated as an escaped quote.
/// Single-quoted strings have no escape sequences (POSIX semantics).
///
/// Each segment is trimmed. Empty segments are dropped.
pub fn sequential_segments(cmd: &str) -> Vec<&str> {
    #[derive(PartialEq)]
    enum Q {
        None,
        Single,
        Double,
    }

    let bytes = cmd.as_bytes();
    let mut segments: Vec<&str> = Vec::new();
    let mut start = 0usize;
    let mut i = 0usize;
    let mut quote = Q::None;

    while i < bytes.len() {
        match quote {
            Q::None => {
                match bytes[i] {
                    b'\'' => {
                        quote = Q::Single;
                        i += 1;
                    }
                    b'"' => {
                        quote = Q::Double;
                        i += 1;
                    }
                    // `&&` or `||`
                    b'&' | b'|' if i + 1 < bytes.len() && bytes[i + 1] == bytes[i] => {
                        let seg = cmd[start..i].trim();
                        if !seg.is_empty() {
                            segments.push(seg);
                        }
                        i += 2;
                        start = i;
                    }
                    b';' => {
                        let seg = cmd[start..i].trim();
                        if !seg.is_empty() {
                            segments.push(seg);
                        }
                        i += 1;
                        start = i;
                    }
                    _ => {
                        i += 1;
                    }
                }
            }
            Q::Single => {
                // No escape sequences in single quotes
                if bytes[i] == b'\'' {
                    quote = Q::None;
                }
                i += 1;
            }
            Q::Double => {
                match bytes[i] {
                    b'\\' if i + 1 < bytes.len() => {
                        i += 2;
                    } // skip escaped char
                    b'"' => {
                        quote = Q::None;
                        i += 1;
                    }
                    _ => {
                        i += 1;
                    }
                }
            }
        }
    }

    // Trailing segment
    let seg = cmd[start..].trim();
    if !seg.is_empty() {
        segments.push(seg);
    }

    segments
}

#[cfg(kani)]
mod kani_proofs {
    use super::*;

    /// Proof: splitting on && produces non-empty, trimmed segments.
    #[kani::proof]
    #[kani::unwind(20)]
    fn segments_nonempty_concrete() {
        let inputs = [
            "a && b",
            "  a  &&  b  ",
            "x; y; z",
            "a || b && c; d",
            "",
            "   ",
            "&&",
            ";",
        ];
        for cmd in &inputs {
            for seg in sequential_segments(cmd) {
                assert!(!seg.is_empty(), "empty segment from: {cmd:?}");
                assert_eq!(seg, seg.trim(), "untrimmed segment from: {cmd:?}");
            }
        }
    }

    /// Proof: idempotency — re-splitting a plain segment returns itself.
    #[kani::proof]
    #[kani::unwind(15)]
    fn single_segment_idempotent() {
        let singles = ["cargo build", "a", "hello world", "x-y.z"];
        for s in &singles {
            let segs = sequential_segments(s);
            assert!(segs.len() == 1);
            let re = sequential_segments(segs[0]);
            assert!(re.len() == 1);
            assert!(re[0] == segs[0]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_command_no_operators() {
        assert_eq!(sequential_segments("cargo build"), vec!["cargo build"]);
    }

    #[test]
    fn and_and_splits_two_segments() {
        assert_eq!(
            sequential_segments("cargo fmt --all && grep foo ."),
            vec!["cargo fmt --all", "grep foo ."]
        );
    }

    #[test]
    fn or_or_splits_two_segments() {
        assert_eq!(
            sequential_segments("cargo build || echo failed"),
            vec!["cargo build", "echo failed"]
        );
    }

    #[test]
    fn semicolon_splits_segments() {
        assert_eq!(
            sequential_segments("echo a; echo b; echo c"),
            vec!["echo a", "echo b", "echo c"]
        );
    }

    #[test]
    fn pipe_not_split() {
        // `| grep` stays in the same segment
        assert_eq!(
            sequential_segments("cmd | grep foo"),
            vec!["cmd | grep foo"]
        );
    }

    #[test]
    fn pipe_followed_by_and_and() {
        // pipe within segment, then && splits
        assert_eq!(
            sequential_segments("cmd | grep foo && cargo test"),
            vec!["cmd | grep foo", "cargo test"]
        );
    }

    #[test]
    fn empty_string_returns_empty() {
        let result: Vec<&str> = sequential_segments("");
        assert!(result.is_empty());
    }

    #[test]
    fn whitespace_only_returns_empty() {
        let result: Vec<&str> = sequential_segments("   ");
        assert!(result.is_empty());
    }

    #[test]
    fn leading_trailing_whitespace_trimmed() {
        assert_eq!(
            sequential_segments("  cargo build  &&  cargo test  "),
            vec!["cargo build", "cargo test"]
        );
    }

    #[test]
    fn mixed_operators() {
        assert_eq!(
            sequential_segments("a && b || c; d"),
            vec!["a", "b", "c", "d"]
        );
    }

    // ── quote-aware tests ─────────────────────────────────────────────────

    #[test]
    fn double_quoted_and_and_not_split() {
        // The && is inside a double-quoted string — must not split
        assert_eq!(
            sequential_segments(r#"echo "cargo build && grep foo src/""#),
            vec![r#"echo "cargo build && grep foo src/""#]
        );
    }

    #[test]
    fn single_quoted_and_and_not_split() {
        assert_eq!(
            sequential_segments("echo 'cargo build && grep foo src/'"),
            vec!["echo 'cargo build && grep foo src/'"]
        );
    }

    #[test]
    fn double_quoted_semicolon_not_split() {
        assert_eq!(
            sequential_segments(r#"echo "a; b""#),
            vec![r#"echo "a; b""#]
        );
    }

    #[test]
    fn operator_after_closing_quote_does_split() {
        assert_eq!(
            sequential_segments(r#"echo "hello" && cargo test"#),
            vec![r#"echo "hello""#, "cargo test"]
        );
    }

    #[test]
    fn escaped_double_quote_not_mistaken_for_close() {
        // The \" inside the string is not the closing quote
        assert_eq!(
            sequential_segments(r#"echo "say \"hi\" && bye""#),
            vec![r#"echo "say \"hi\" && bye""#]
        );
    }

    #[test]
    fn printf_json_payload_not_split() {
        // Simulates the hook-demo case that originally caused false positives
        let cmd = r#"printf '{"tool_name":"Bash","tool_input":{"command":"cargo build && grep foo src/"}}' | coursers pre"#;
        let segs = sequential_segments(cmd);
        assert_eq!(segs.len(), 1);
        assert_eq!(segs[0], cmd);
    }

    // ── property tests ────────────────────────────────────────────────────

    use proptest::prelude::*;

    /// Strategy: word characters only — no operators embedded in tokens
    fn word() -> impl Strategy<Value = String> {
        "[a-zA-Z0-9_./-]{1,20}".prop_map(|s| s)
    }

    /// Strategy: one of the three sequential operators
    fn operator() -> impl Strategy<Value = &'static str> {
        prop_oneof![Just("&&"), Just("||"), Just(";")]
    }

    /// Build a command from N plain words joined by operators.
    fn command_with_operators(
        min_segments: usize,
        max_segments: usize,
    ) -> impl Strategy<Value = (Vec<String>, String)> {
        prop::collection::vec(word(), min_segments..=max_segments).prop_flat_map(|words| {
            let n_ops = words.len() - 1;
            prop::collection::vec(operator(), n_ops).prop_map(move |ops| {
                let mut cmd = words[0].clone();
                for (op, word) in ops.iter().zip(words[1..].iter()) {
                    cmd.push(' ');
                    cmd.push_str(op);
                    cmd.push(' ');
                    cmd.push_str(word);
                }
                (words.clone(), cmd)
            })
        })
    }

    proptest! {
        /// Segment count equals the number of words we built the command from.
        #[test]
        fn prop_segment_count_matches_word_count(
            (words, cmd) in command_with_operators(1, 6)
        ) {
            let segs = sequential_segments(&cmd);
            prop_assert_eq!(segs.len(), words.len());
        }

        /// No segment contains `&&`, `||`, or a bare `;`.
        #[test]
        fn prop_segments_contain_no_sequential_operators(
            (_, cmd) in command_with_operators(1, 6)
        ) {
            for seg in sequential_segments(&cmd) {
                prop_assert!(!seg.contains("&&"), "segment contains &&: {seg:?}");
                prop_assert!(!seg.contains("||"), "segment contains ||: {seg:?}");
                prop_assert!(!seg.contains(';'), "segment contains ;: {seg:?}");
            }
        }

        /// All segments are non-empty and trimmed (no leading/trailing whitespace).
        #[test]
        fn prop_all_segments_trimmed_and_nonempty(
            (_, cmd) in command_with_operators(1, 6)
        ) {
            for seg in sequential_segments(&cmd) {
                prop_assert!(!seg.is_empty());
                prop_assert_eq!(seg, seg.trim());
            }
        }

        /// A command with no sequential operators produces exactly one segment equal
        /// to the trimmed input.
        #[test]
        fn prop_no_operators_single_segment(word in word()) {
            let segs = sequential_segments(&word);
            prop_assert_eq!(segs.len(), 1);
            prop_assert_eq!(segs[0], word.trim());
        }

        /// Pipe characters do NOT trigger splitting — a piped command is one segment.
        #[test]
        fn prop_pipe_never_splits(
            a in word(),
            b in word(),
        ) {
            let cmd = format!("{a} | {b}");
            let segs = sequential_segments(&cmd);
            prop_assert_eq!(segs.len(), 1);
        }
    }
}
