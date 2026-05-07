/// Domain types and port for obfsck MCP server integration.
///
/// `ObfsckMcp` exposes two tools mirroring the obfsck-mcp JSON-RPC protocol:
///   - `audit`            — pipe text, get pattern hit counts back
///   - `generate_filters` — given example strings, suggest filter patterns

// ---------------------------------------------------------------------------
// Domain types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditHit {
    pub label: String,
    pub count: usize,
}

#[derive(Debug, Clone)]
pub struct FilterSuggestion {
    pub pattern: String,
    pub label: String,
}

// ---------------------------------------------------------------------------
// Port
// ---------------------------------------------------------------------------

/// Capability port for the obfsck MCP server.
pub trait ObfsckMcp {
    /// Audit `text` for secret patterns; returns hit counts per label.
    fn audit(&self, text: &str) -> Vec<AuditHit>;

    /// Given example strings that should be redacted, suggest filter patterns.
    fn generate_filters(&self, examples: &[String]) -> Vec<FilterSuggestion>;
}

// ---------------------------------------------------------------------------
// Null adapter — used when obfsck-mcp is not on PATH
// ---------------------------------------------------------------------------

pub struct NullObfsckMcpClient;

impl ObfsckMcp for NullObfsckMcpClient {
    fn audit(&self, _text: &str) -> Vec<AuditHit> {
        vec![]
    }

    fn generate_filters(&self, _examples: &[String]) -> Vec<FilterSuggestion> {
        vec![]
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    struct FakeObfsckMcpClient {
        audit_hits: Vec<AuditHit>,
        filter_suggestions: Vec<FilterSuggestion>,
    }

    impl ObfsckMcp for FakeObfsckMcpClient {
        fn audit(&self, _text: &str) -> Vec<AuditHit> {
            self.audit_hits.clone()
        }

        fn generate_filters(&self, _examples: &[String]) -> Vec<FilterSuggestion> {
            self.filter_suggestions.clone()
        }
    }

    #[test]
    fn null_client_returns_empty() {
        let c = NullObfsckMcpClient;
        assert!(c.audit("sk-abc123").is_empty());
        assert!(c.generate_filters(&["sk-abc123".to_string()]).is_empty());
    }

    #[test]
    fn fake_client_returns_injected_data() {
        let c = FakeObfsckMcpClient {
            audit_hits: vec![AuditHit {
                label: "api-key".into(),
                count: 2,
            }],
            filter_suggestions: vec![FilterSuggestion {
                pattern: r"sk-[A-Za-z0-9]{32}".into(),
                label: "api-key".into(),
            }],
        };
        let hits = c.audit("sk-abc123");
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].label, "api-key");
        assert_eq!(hits[0].count, 2);

        let suggs = c.generate_filters(&["sk-abc123".to_string()]);
        assert_eq!(suggs.len(), 1);
        assert_eq!(suggs[0].label, "api-key");
    }
}
