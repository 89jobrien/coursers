//! Domain results produced by obfsck auditing and filter generation.

/// A pattern hit returned by the obfsck MCP `audit` tool.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditHit {
    pub label: String,
    pub count: usize,
}

/// A suggested redaction filter pattern.
#[derive(Debug, Clone)]
pub struct FilterSuggestion {
    pub pattern: String,
    pub label: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audit_hit_equality() {
        let a = AuditHit {
            label: "aws-key".into(),
            count: 2,
        };
        let b = AuditHit {
            label: "aws-key".into(),
            count: 2,
        };
        assert_eq!(a, b);
    }

    #[test]
    fn audit_hit_inequality_on_count() {
        let a = AuditHit {
            label: "aws-key".into(),
            count: 2,
        };
        let b = AuditHit {
            label: "aws-key".into(),
            count: 3,
        };
        assert_ne!(a, b);
    }

    #[test]
    fn filter_suggestion_holds_fields() {
        let s = FilterSuggestion {
            pattern: "AKIA[0-9A-Z]{16}".into(),
            label: "aws-access-key".into(),
        };
        assert_eq!(s.pattern, "AKIA[0-9A-Z]{16}");
        assert_eq!(s.label, "aws-access-key");
    }
}
