//! Conformance tests for `RtkAnalysis` implementations.
//!
//! Every `impl RtkAnalysis` must satisfy:
//! 1. All methods return without panic
//! 2. All methods are deterministic
//! 3. NullRtkClient returns None/None for all optional methods

use crs_core::rtk::{NullRtkClient, RtkAnalysis};

// ---------------------------------------------------------------------------
// Contract assertion
// ---------------------------------------------------------------------------

fn assert_rtk_analysis_contract(client: &impl RtkAnalysis) {
    // 1. discover() is callable and deterministic
    let d1 = client.discover(30);
    let d2 = client.discover(30);
    assert_eq!(
        d1.is_some(),
        d2.is_some(),
        "contract 1: discover must be deterministic"
    );

    // 2. gain() is callable and deterministic
    let g1 = client.gain();
    let g2 = client.gain();
    assert_eq!(
        g1.is_some(),
        g2.is_some(),
        "contract 2: gain must be deterministic"
    );

    // 3. session() is callable and deterministic
    let s1 = client.session();
    let s2 = client.session();
    assert_eq!(
        s1.is_some(),
        s2.is_some(),
        "contract 3: session must be deterministic"
    );

    // 4. verify() is callable and deterministic
    let v1 = client.verify();
    let v2 = client.verify();
    assert_eq!(
        v1.is_some(),
        v2.is_some(),
        "contract 4: verify must be deterministic"
    );

    // 5. hook_audit() is callable and deterministic
    let h1 = client.hook_audit();
    let h2 = client.hook_audit();
    assert_eq!(
        h1.is_some(),
        h2.is_some(),
        "contract 5: hook_audit must be deterministic"
    );

    // 6. version() is callable and deterministic
    let ver1 = client.version();
    let ver2 = client.version();
    assert_eq!(ver1, ver2, "contract 6: version must be deterministic");
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[test]
fn null_client_satisfies_contract() {
    let client = NullRtkClient;
    assert_rtk_analysis_contract(&client);
}

#[test]
fn null_client_returns_none_for_all() {
    let client = NullRtkClient;
    assert!(client.discover(30).is_none());
    assert!(client.gain().is_none());
    assert!(client.session().is_none());
    assert!(client.verify().is_none());
    assert!(client.hook_audit().is_none());
    assert!(client.version().is_none());
}
