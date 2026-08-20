//! Conformance tests for `VarExpander` implementations.
//!
//! Every `impl VarExpander` must satisfy:
//! 1. expand() returns a String (never panics on valid input)
//! 2. expand() is idempotent for plain commands (no vars)
//! 3. NoopExpander returns input unchanged for any input
//! 4. EnvExpander leaves `$VAR`/`${VAR}`/`$env.VAR` references untouched —
//!    dollar-variable resolution is disabled (see expand.rs::expand_dollar
//!    doc comment: this pipeline always dispatches through Nushell, which
//!    resolves its own `$env.VAR`/`$name` references at run time, so
//!    pre-resolving them against the hook process's environment only ever
//!    corrupted Nushell syntax).
//! 5. EnvExpander still expands `~` to `$HOME` (tilde expansion is unaffected)

use coursers_core::expand::{EnvExpander, NoopExpander, VarExpander};

// ---------------------------------------------------------------------------
// Contract assertion: properties every VarExpander must satisfy
// ---------------------------------------------------------------------------

fn assert_var_expander_base_contract(expander: &impl VarExpander) {
    // 1. Plain command with no variable references passes through unchanged
    let plain = "cargo build --release";
    assert_eq!(
        expander.expand(plain),
        plain,
        "contract 1: plain command must pass through"
    );

    // 2. Empty string passes through
    assert_eq!(
        expander.expand(""),
        "",
        "contract 2: empty string must pass through"
    );

    // 3. expand() is deterministic — two calls yield same result
    let input = "echo $HOME ~/foo";
    let first = expander.expand(input);
    let second = expander.expand(input);
    assert_eq!(first, second, "contract 3: expand must be deterministic");
}

// ---------------------------------------------------------------------------
// NoopExpander
// ---------------------------------------------------------------------------

#[test]
fn noop_expander_satisfies_base_contract() {
    let expander = NoopExpander;
    assert_var_expander_base_contract(&expander);
}

#[test]
fn noop_expander_preserves_variable_references() {
    let expander = NoopExpander;

    // Dollar vars are NOT expanded
    assert_eq!(expander.expand("echo $HOME"), "echo $HOME");
    assert_eq!(expander.expand("echo ${HOME}"), "echo ${HOME}");
    assert_eq!(expander.expand("echo $env.HOME"), "echo $env.HOME");

    // Tilde is NOT expanded
    assert_eq!(expander.expand("cd ~/foo"), "cd ~/foo");
}

// ---------------------------------------------------------------------------
// EnvExpander
// ---------------------------------------------------------------------------

#[test]
fn env_expander_satisfies_base_contract() {
    let expander = EnvExpander;
    assert_var_expander_base_contract(&expander);
}

#[test]
fn env_expander_leaves_known_var_unresolved() {
    // Dollar-variable resolution is disabled — even a var that IS set in the
    // real environment must be left as literal Nushell/shell syntax.
    unsafe { std::env::set_var("_CRS_CONFORM_TEST", "resolved") };
    let expander = EnvExpander;
    let result = expander.expand("echo $_CRS_CONFORM_TEST");
    unsafe { std::env::remove_var("_CRS_CONFORM_TEST") };
    assert_eq!(result, "echo $_CRS_CONFORM_TEST");
}

#[test]
fn env_expander_leaves_unknown_var_as_is() {
    let expander = EnvExpander;
    unsafe { std::env::remove_var("_CRS_CONFORM_NONEXISTENT") };
    let result = expander.expand("echo $_CRS_CONFORM_NONEXISTENT");
    assert_eq!(result, "echo $_CRS_CONFORM_NONEXISTENT");
}
