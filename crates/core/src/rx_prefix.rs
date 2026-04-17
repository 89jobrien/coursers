/// One shell segment plus the separator that followed it (if any).
#[derive(Debug, Clone, PartialEq)]
pub struct Segment {
    /// The raw text of this segment (may have leading/trailing spaces).
    pub text: String,
    /// The separator token that terminated this segment: `&&`, `||`, `;`, or `|`.
    pub sep: Option<String>,
}

/// Split a shell command string into segments on `&&`, `||`, `;`, `|`.
///
/// Preserves surrounding whitespace in each segment so `rejoin` is lossless.
/// Does NOT handle quotes — splitting is purely textual, which is correct for
/// the commands we see in practice (no quoted separators).
pub fn split_segments(cmd: &str) -> Vec<Segment> {
    // Separators in priority order (longest first so `||` beats `|`).
    let seps = ["&&", "||", ";", "|"];
    let mut result = Vec::new();
    let mut remaining = cmd;

    'outer: loop {
        // Find the earliest separator in remaining.
        let mut earliest: Option<(usize, &str)> = None;
        for sep in &seps {
            if let Some(pos) = remaining.find(sep) {
                if earliest.is_none() || pos < earliest.unwrap().0 {
                    earliest = Some((pos, sep));
                }
            }
        }
        match earliest {
            None => {
                result.push(Segment { text: remaining.to_string(), sep: None });
                break 'outer;
            }
            Some((pos, sep)) => {
                result.push(Segment {
                    text: remaining[..pos].to_string(),
                    sep: Some(sep.to_string()),
                });
                remaining = &remaining[pos + sep.len()..];
            }
        }
    }
    result
}

/// Reconstruct the original command string from segments.
pub fn rejoin(segs: &[Segment]) -> String {
    let mut out = String::new();
    for seg in segs {
        out.push_str(&seg.text);
        if let Some(sep) = &seg.sep {
            out.push_str(sep);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_simple_pipeline() {
        let segs = split_segments("cargo build | tail -5");
        assert_eq!(segs, vec![
            Segment { text: "cargo build ".to_string(), sep: Some("|".to_string()) },
            Segment { text: " tail -5".to_string(), sep: None },
        ]);
    }

    #[test]
    fn split_and_and() {
        let segs = split_segments("git add -A && git commit -m 'msg'");
        assert_eq!(segs, vec![
            Segment { text: "git add -A ".to_string(), sep: Some("&&".to_string()) },
            Segment { text: " git commit -m 'msg'".to_string(), sep: None },
        ]);
    }

    #[test]
    fn split_semicolon() {
        let segs = split_segments("echo a; echo b");
        assert_eq!(segs, vec![
            Segment { text: "echo a".to_string(), sep: Some(";".to_string()) },
            Segment { text: " echo b".to_string(), sep: None },
        ]);
    }

    #[test]
    fn split_or_or() {
        let segs = split_segments("cargo check || echo failed");
        assert_eq!(segs, vec![
            Segment { text: "cargo check ".to_string(), sep: Some("||".to_string()) },
            Segment { text: " echo failed".to_string(), sep: None },
        ]);
    }

    #[test]
    fn split_no_separator_is_single_segment() {
        let segs = split_segments("cargo test --workspace");
        assert_eq!(segs, vec![
            Segment { text: "cargo test --workspace".to_string(), sep: None },
        ]);
    }

    #[test]
    fn rejoin_preserves_separators() {
        let segs = vec![
            Segment { text: "cargo build ".to_string(), sep: Some("|".to_string()) },
            Segment { text: " tail -5".to_string(), sep: None },
        ];
        assert_eq!(rejoin(&segs), "cargo build | tail -5");
    }

    #[test]
    fn rejoin_and_and() {
        let segs = vec![
            Segment { text: "git add -A ".to_string(), sep: Some("&&".to_string()) },
            Segment { text: " git commit -m 'msg'".to_string(), sep: None },
        ];
        assert_eq!(rejoin(&segs), "git add -A && git commit -m 'msg'");
    }
}
