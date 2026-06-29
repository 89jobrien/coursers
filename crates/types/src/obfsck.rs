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
