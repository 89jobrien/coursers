//! Post-condition and fuzz coverage for the command-rewrite engine
//! (`coursers_core::rewrite::apply`), specifically the `2>&1`-family rules in
//! the vendored `filters_snapshot.toml` (mirrors `~/.config/crs/filters.toml`).
//!
//! Every rewrite rule is expected to produce output that is valid nu syntax —
//! the rewritten string is what a caller pipes into `nu -c "<rewritten>"`.
//! We verify this by shelling out to `nu --ide-check` via `coursers::nu_check`,
//! the same mechanism the `crs nu-check` subcommand uses on real scripts.
//!
//! The fuzz target (step 3) generates randomized shell fragments covering
//! redirections, pipes, and quoting/interpolation/subshells, and asserts:
//!   (a) `apply` never panics (proptest's harness itself enforces this — a
//!       panicking case becomes a failing, shrunk regression)
//!   (b) every `Some(outcome)` is valid nu
//!   (c) rewriting is idempotent: re-running `apply` on an already-rewritten
//!       command produces no further change.
//!
//! Scope boundary: the quoting/interpolation/subshell fragments use nu's own
//! syntax (`(...)`, `$"...(...)..."`), not bash's (`$(...)`, `` `...` ``,
//! `${VAR:-default}`). Bash heredocs and command substitution are invalid nu
//! with or without any rewrite rule firing — no rule claims to translate
//! them, so asserting validity on them would test a contract `apply` never
//! made. Blocking those constructs upstream is `no-bash-use-nu`'s and
//! `no-heredoc-payload-to-kgx`'s job (see `rule_conformance.rs`), not the
//! rewrite engine's. This file only fuzzes constructs the rewrite rules are
//! actually meant to handle.

use coursers::nu_check::check_file;
use coursers_core::expand::NoopExpander;
use coursers_core::rewrite::{RewriteConfig, apply};
use std::io::Write as _;

const FILTERS_SNAPSHOT: &str = include_str!("fixtures/rule_conformance/filters_snapshot.toml");

fn load_rewrite_config() -> RewriteConfig {
    toml::from_str(FILTERS_SNAPSHOT).expect("vendored filters snapshot must parse")
}

/// Returns `Ok(())` if `command`, written verbatim to a `.nu` file, parses
/// cleanly under `nu --ide-check`.
fn assert_valid_nu(command: &str) -> Result<(), String> {
    let mut f = tempfile::NamedTempFile::with_suffix(".nu").map_err(|e| e.to_string())?;
    f.write_all(command.as_bytes()).map_err(|e| e.to_string())?;
    let result = check_file(f.path()).map_err(|e| e.to_string())?;
    if result.is_ok() {
        Ok(())
    } else {
        Err(result
            .errors
            .iter()
            .map(|e| e.display())
            .collect::<Vec<_>>()
            .join("; "))
    }
}

// ---------------------------------------------------------------------------
// Conformance: every rewrite rule that can fire on a representative
// redirection-bearing input produces valid nu.
// ---------------------------------------------------------------------------

#[test]
fn redirect_rewrites_produce_valid_nu() {
    let config = load_rewrite_config();
    let cases = [
        r#"cargo test --workspace 2>&1 | grep -E "FAILED|error""#,
        "cargo build 2>&1",
        "some_cmd 2>/dev/null",
        "hash -r 2>/dev/null; cargo build",
        "cargo build; hash -r 2>/dev/null",
        "hash -r 2>/dev/null",
        "cargo clippy",
        "cargo clippy -p coursers-core",
    ];

    let mut failures = Vec::new();
    for cmd in cases {
        if let Some(outcome) = apply(cmd, &config, &NoopExpander)
            && let Err(e) = assert_valid_nu(&outcome.command)
        {
            failures.push(format!("{cmd:?} -> {:?}: {e}", outcome.command));
        }
    }
    assert!(
        failures.is_empty(),
        "invalid nu rewrites:\n{}",
        failures.join("\n")
    );
}

/// Pin the previously-broken shape directly: `2>&1 | grep` must rewrite to
/// something `nu --ide-check` accepts, not the old POSIX-only form.
#[test]
fn stderr_merge_into_pipe_is_valid_nu() {
    let config = load_rewrite_config();
    let outcome = apply(
        r#"cargo test --workspace 2>&1 | grep -E "FAILED|error""#,
        &config,
        &NoopExpander,
    )
    .expect("2>&1 | grep must trigger at least one rewrite rule");
    assert_valid_nu(&outcome.command)
        .unwrap_or_else(|e| panic!("rewrite {:?} is not valid nu: {e}", outcome.command));
}

// ---------------------------------------------------------------------------
// Idempotency (also asserted generically by the fuzz target below, pinned
// here as a focused regression case).
// ---------------------------------------------------------------------------

#[test]
fn stderr_strip_rewrite_is_idempotent() {
    let config = load_rewrite_config();
    let once = apply("cargo build 2>&1", &config, &NoopExpander).unwrap();
    let twice = apply(&once.command, &config, &NoopExpander);
    assert!(
        twice.is_none() || twice.unwrap().command == once.command,
        "rewrite must be idempotent"
    );
}

// ---------------------------------------------------------------------------
// Fuzz target: randomized shell fragments through the rewrite engine.
// ---------------------------------------------------------------------------

use proptest::prelude::*;

/// A redirection fragment: `2>&1`, `2>&1 |`, `2>/dev/null`, or none.
fn redirection() -> impl Strategy<Value = String> {
    prop_oneof![
        Just(String::new()),
        Just(" 2>&1".to_string()),
        Just(" 2>&1 |".to_string()),
        Just(" 2>/dev/null".to_string()),
        Just(" 1>/dev/null".to_string()),
    ]
}

/// A pipe fragment. `&&`/`;` chains are deliberately excluded: the rewrite
/// engine only targets redirections/pipes/quoting (see module docs), and a
/// `&&`/`;` chain is invalid nu with or without any rewrite firing — that's
/// `no-bash-use-nu`'s job to block upstream, not something `apply` claims to
/// fix. Asserting nu-validity on inputs the engine was never meant to touch
/// would be testing the wrong contract.
fn connector() -> impl Strategy<Value = String> {
    prop_oneof![
        Just(String::new()),
        Just(" | grep foo".to_string()),
        Just(" | sort | uniq".to_string()),
    ]
}

/// A quoting/interpolation/subshell fragment appended to a base command,
/// using nu-native forms rather than bash ones.
///
/// Bash command substitution (`$(date)`, `` `date` ``) and parameter
/// expansion with a default (`${VAR:-default}`) are deliberately excluded:
/// no rewrite rule or `VarExpander` impl claims to translate them (only
/// `$HOME`/`${VAR}`/`$env.VAR`/`~` are handled), so they are invalid nu with
/// or without any rule firing — blocking them upstream is `no-bash-use-nu`'s
/// job, not something `apply` claims to fix. Asserting nu-validity on syntax
/// the engine was never meant to touch would test the wrong contract — the
/// same reasoning as the `&&`/`;` exclusion in `connector()` above. nu's own
/// subshell/interpolation equivalents (`(...)`, `$"...(...)..."`) are used
/// instead so this coverage still exercises the engine against inputs a
/// rule *could* plausibly misfire on.
fn quoting_fragment() -> impl Strategy<Value = String> {
    prop_oneof![
        Just(String::new()),
        Just(r#" $"home is ($env.HOME)""#.to_string()),
        Just(" '$HOME'".to_string()),
        Just(" (1 + 2)".to_string()),
        Just(r#" "quoted arg""#.to_string()),
    ]
}

fn base_command() -> impl Strategy<Value = String> {
    prop_oneof![
        Just("cargo build".to_string()),
        Just("cargo test".to_string()),
        Just("cargo clippy".to_string()),
        Just("git status".to_string()),
        Just("sudo apt update".to_string()),
    ]
}

fn fuzzed_fragment() -> impl Strategy<Value = String> {
    (
        base_command(),
        quoting_fragment(),
        redirection(),
        connector(),
    )
        .prop_map(|(base, quote, redir, conn)| format!("{base}{quote}{redir}{conn}"))
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(10_000))]

    /// (a) no panic — enforced by proptest's harness itself: a panicking
    ///     input becomes a failing, shrunk test case rather than a crash.
    /// (b) every produced rewrite is valid nu.
    /// (c) rewriting is idempotent: re-applying to the rewritten output
    ///     yields no further change.
    #[test]
    fn rewrite_engine_fuzz(cmd in fuzzed_fragment()) {
        let config = load_rewrite_config();

        let Some(outcome) = apply(&cmd, &config, &NoopExpander) else {
            return Ok(());
        };

        if let Err(e) = assert_valid_nu(&outcome.command) {
            return Err(TestCaseError::fail(format!(
                "rewrite of {cmd:?} -> {:?} is not valid nu: {e}",
                outcome.command
            )));
        }

        let second = apply(&outcome.command, &config, &NoopExpander);
        let stable = match &second {
            None => true,
            Some(o2) => o2.command == outcome.command,
        };
        prop_assert!(
            stable,
            "rewrite not idempotent: {:?} -> {:?} -> {:?}",
            cmd,
            outcome.command,
            second.map(|o| o.command)
        );
    }
}
