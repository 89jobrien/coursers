#!/usr/bin/env nu
# demo-rewrites.nu — walk every [[rewrites]] rule in ~/.config/crs/filters.toml
# (or COURSERS_FILTERS if set) with a representative example command, showing
# the crs rewrite hook's before/after for each rule.
# Usage: nu scripts/demo-rewrites.nu

let bin = if (which crs | length) > 0 {
    "crs"
} else if (which coursers | length) > 0 {
    "coursers"
} else if ("./target/release/coursers" | path exists) {
    "./target/release/coursers"
} else {
    error make { msg: "crs/coursers binary not found — run: cargo install --path crates/coursers" }
}

# One representative example command per [[rewrites]] rule in
# ~/.config/crs/filters.toml, in file order. Update this list when rules change.
let examples = [
    { label: "grep in a pipeline -> rg"
      command: 'git log | grep JOB-123' }
    { label: "UUOC: cat file | cmd -> < file cmd"
      command: 'cat Cargo.toml | wc -l' }
    { label: "sort | uniq -> sort -u"
      command: 'git log --format=%ae | sort | uniq' }
    { label: "cargo test -> cargo nextest run"
      command: 'cargo test --release -p coursers-core' }
    { label: "2>&1 (bash-ism) -> out+err>"
      command: 'cargo build 2>&1' }
    { label: "2>/dev/null (bash-ism) -> stripped"
      command: 'mise ls-remote cargo-deny 2>/dev/null' }
    { label: "hash -r (bash PATH-cache clear, no-op in nu) -> true"
      command: 'mise reshim; hash -r 2>/dev/null; which cargo-deny' }
    { label: "bare cargo clippy -> append -- -D warnings"
      command: 'cargo clippy --workspace' }
    { label: "sudo ... -> routed through GUI askpass"
      command: 'sudo systemsetup -settimezone America/New_York' }
    { label: "git worktree add/remove/list/prune -> godmode worktree"
      command: 'git worktree add ../foo bar' }
    { label: "nextest+clippy+fmt gate chain -> godmode verify"
      command: 'cargo nextest run && cargo clippy --workspace -- -D warnings && cargo fmt --check' }
    { label: "gh run view --log-failed -> godmode ci"
      command: 'gh run view 123456 --log-failed' }
    { label: "bat/less/more GODMODE.tasks.yaml -> godmode status"
      command: 'bat .ctx/GODMODE.tasks.yaml' }
    { label: "which <tool>...; op whoami -> godmode doctor"
      command: 'which cargo-nextest; which gh; op whoami' }
    { label: "yq/jq pending|runnable on tasks.yaml -> godmode task next"
      command: 'yq ".tasks[] | select(.status == \"pending\")" .ctx/GODMODE.tasks.yaml' }
    { label: "grep/yq/jq unblocked|independent|dispatch -> godmode dispatch"
      command: 'grep unblocked .ctx/GODMODE.tasks.yaml' }
    { label: "grep/yq/jq (generic) on tasks.yaml -> godmode task list"
      command: 'grep -c blocked .ctx/GODMODE.tasks.yaml' }
]

# Run one command through the `crs rewrite` PreToolUse hook and extract the
# rewritten command (if any) from the hookSpecificOutput JSON.
def run_rewrite [command: string] {
    let payload = { tool_name: "Bash", tool_input: { command: $command } } | to json
    let result = (do { $payload | run-external $bin "rewrite" } | complete)
    if $result.exit_code != 0 or ($result.stdout | str trim | is-empty) {
        { rewritten: null, reason: null }
    } else {
        let parsed = ($result.stdout | from json)
        {
            rewritten: ($parsed.hookSpecificOutput.updatedInput.command? | default null)
            reason: ($parsed.hookSpecificOutput.permissionDecisionReason? | default null)
        }
    }
}

print ""
print "crs rewrite — demo of all configured rules"
print "════════════════════════════════════════════════════════════════"

mut misses = []

for ex in $examples {
    let outcome = (run_rewrite $ex.command)
    print $"\n▸ ($ex.label)"
    print $"  before : ($ex.command)"
    if $outcome.rewritten == null {
        print "  after  : (no rule matched — passthrough)"
        $misses = ($misses | append $ex.label)
    } else {
        print $"  after  : ($outcome.rewritten)"
        print $"  reason : ($outcome.reason)"
    }
}

print ""
print "════════════════════════════════════════════════════════════════"
if ($misses | length) > 0 {
    print $"  ($misses | length) example\(s\) did not match their intended rule:"
    for m in $misses {
        print $"    - ($m)"
    }
    exit 1
} else {
    print $"  all ($examples | length) rewrite rules fired as expected"
}
