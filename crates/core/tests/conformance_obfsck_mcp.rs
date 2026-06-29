//! Conformance tests for `ObfsckMcp` implementations.
//!
//! Every `impl ObfsckMcp` must satisfy:
//! 1. audit() never panics, returns Vec<AuditHit>
//! 2. generate_filters() never panics, returns Vec<FilterSuggestion>
//! 3. Both methods are deterministic (same input -> same output)

use coursers_core::obfsck::{NullObfsckMcpClient, ObfsckMcp};

// ---------------------------------------------------------------------------
// Contract assertion
// ---------------------------------------------------------------------------

fn assert_obfsck_mcp_contract(client: &impl ObfsckMcp) {
    // 1. audit() on empty input returns a valid (possibly empty) vec
    let hits = client.audit("");
    let hits2 = client.audit("");
    assert_eq!(
        hits.len(),
        hits2.len(),
        "contract 1: audit must be deterministic"
    );

    // 2. audit() on non-empty input doesn't panic
    let _hits = client.audit("sk-abc123 some secret text");

    // 3. generate_filters() on empty input returns valid vec
    let filters = client.generate_filters(&[]);
    let filters2 = client.generate_filters(&[]);
    assert_eq!(
        filters.len(),
        filters2.len(),
        "contract 3: generate_filters must be deterministic"
    );

    // 4. generate_filters() on non-empty input doesn't panic
    let _filters =
        client.generate_filters(&["sk-abc123".to_string(), "ghp_xxxxxxxxxxxx".to_string()]);
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[test]
fn null_client_satisfies_contract() {
    let client = NullObfsckMcpClient;
    assert_obfsck_mcp_contract(&client);
}

#[test]
fn null_client_returns_empty_vecs() {
    let client = NullObfsckMcpClient;
    assert!(client.audit("anything").is_empty());
    assert!(
        client
            .generate_filters(&["anything".to_string()])
            .is_empty()
    );
}
