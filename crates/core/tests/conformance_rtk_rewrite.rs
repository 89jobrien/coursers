//! Conformance tests for `RtkRewrite` implementations.
//!
//! Every `impl RtkRewrite` must satisfy:
//! 1. All methods return without panic
//! 2. All methods are deterministic
//! 3. NullRtkClient returns None/false/empty for all methods

use crs_core::rtk::{NullRtkClient, RtkRewrite};

// ---------------------------------------------------------------------------
// Contract assertion
// ---------------------------------------------------------------------------

fn assert_rtk_rewrite_contract(client: &impl RtkRewrite) {
    // 1. rewrite() is callable and deterministic
    let r1 = client.rewrite("grep foo .");
    let r2 = client.rewrite("grep foo .");
    assert_eq!(r1, r2, "contract 1: rewrite must be deterministic");

    // 2. probe() is callable and deterministic
    let p1 = client.probe("grep foo .");
    let p2 = client.probe("grep foo .");
    assert_eq!(
        p1.is_some(),
        p2.is_some(),
        "contract 2: probe must be deterministic"
    );

    // 3. check() is callable and deterministic
    let c1 = client.check("grep foo .");
    let c2 = client.check("grep foo .");
    assert_eq!(c1, c2, "contract 3: check must be deterministic");

    // 4. list_proxies() is callable and deterministic
    let l1 = client.list_proxies();
    let l2 = client.list_proxies();
    assert_eq!(l1, l2, "contract 4: list_proxies must be deterministic");

    // 5. flush() is callable
    let _f = client.flush();
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[test]
fn null_client_satisfies_contract() {
    let client = NullRtkClient;
    assert_rtk_rewrite_contract(&client);
}

#[test]
fn null_client_returns_defaults() {
    let client = NullRtkClient;
    assert!(client.rewrite("anything").is_none());
    assert!(client.probe("anything").is_none());
    assert!(!client.check("anything"));
    assert!(client.list_proxies().is_empty());
    assert!(!client.flush());
}
