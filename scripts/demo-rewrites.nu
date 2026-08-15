#!/usr/bin/env nu
# demo-rewrites.nu — walk every [[rewrites]] rule in ~/.config/crs/filters.toml
# and every course-correct block rule in ~/.config/coursers/course-correct-rules.json,
# each with a representative example command, showing crs's before/after (rewrites)
# or allow/block+redirect (course-corrections).
# Usage:
#   nu scripts/demo-rewrites.nu           # human-readable report
#   nu scripts/demo-rewrites.nu --json    # structured JSON (for tooling/UIs)

def main [--json] {
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
    # ~/.config/crs/filters.toml, in file order. Update when rules change.
    let rewrite_examples = [
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

    # One example command that SHOULD be blocked per rule id in
    # ~/.config/coursers/course-correct-rules.json, in file order.
    let course_correct_examples = [
        { rule: "no-grep-use-tool"
          command: 'grep -rn "TODO" src/' }
        { rule: "no-sed-n-use-read"
          command: "sed -n '10,20p' src/main.rs" }
        { rule: "no-cat-use-read"
          command: 'cat src/main.rs' }
        { rule: "no-heredoc-payload-to-kgx"
          command: "cat <<EOF | kgx wiki write\nsome content\nEOF" }
        { rule: "no-head-tail-use-read"
          command: 'tail -50 src/main.rs' }
        { rule: "no-find-use-glob"
          command: 'find ./src -name "*.rs"' }
        { rule: "no-npm-use-bun"
          command: 'npm install' }
        { rule: "no-nvm-use-mise"
          command: 'nvm use 20' }
        { rule: "no-pip-use-uv"
          command: 'pip install requests' }
        { rule: "no-sleep-find-work"
          command: 'sleep 30' }
        { rule: "no-ls-use-glob"
          command: 'ls -la src/' }
        { rule: "no-sed-use-edit"
          command: "sed -i 's/foo/bar/' src/main.rs" }
        # NOTE: `;`/`&&`/`||` alone do NOT trigger this rule through the real
        # `crs pre` hook path — check_pipeline() splits the command on those
        # exact separators before matching, so the separator is gone by the
        # time each segment is checked. Only the keyword alternatives in the
        # pattern (if/for/while/etc.) actually fire. Use a keyword example.
        { rule: "no-bash-use-nu"
          command: 'if [ -f foo ]; then echo hi; fi' }
        { rule: "no-python3-file-edit"
          command: "python3 -c 'open(\"f.txt\", \"w\").write(\"x\")'" }
        { rule: "no-cargo-install-multi-path"
          command: 'cargo install --path crates/foo --path crates/bar' }
        { rule: "no-cd-use-absolute-paths"
          command: 'cd crates/coursers' }
        { rule: "no-kubectl-use-personal-mcp"
          command: 'kubectl get pods' }
        { rule: "no-docker-use-personal-mcp"
          command: 'docker images' }
        { rule: "no-bw-use-personal-mcp"
          command: 'bw status --raw' }
    ]

    # Run one command through the `crs rewrite` PreToolUse hook and extract the
    # rewritten command (if any) and applied-rule count from the JSON.
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

    # Run one command through the `crs pre` PreToolUse hook and extract the
    # allow/deny decision and reason (course-correct rules).
    def run_pre [command: string] {
        let payload = { tool_name: "Bash", tool_input: { command: $command } } | to json
        let result = (do { $payload | run-external $bin "pre" } | complete)
        if $result.exit_code == 0 {
            { decision: "allow", reason: null }
        } else if ($result.stdout | str trim | is-empty) {
            { decision: "unknown", reason: null }
        } else {
            let parsed = ($result.stdout | from json)
            {
                decision: ($parsed.hookSpecificOutput.permissionDecision? | default "unknown")
                reason: ($parsed.hookSpecificOutput.permissionDecisionReason? | default null)
            }
        }
    }

    mut rewrite_results = []
    mut rewrite_misses = []
    for ex in $rewrite_examples {
        let outcome = (run_rewrite $ex.command)
        let matched = ($outcome.rewritten != null)
        if not $matched {
            $rewrite_misses = ($rewrite_misses | append $ex.label)
        }
        $rewrite_results = ($rewrite_results | append {
            label: $ex.label
            before: $ex.command
            after: $outcome.rewritten
            reason: $outcome.reason
            matched: $matched
        })
    }

    mut cc_results = []
    mut cc_misses = []
    for ex in $course_correct_examples {
        let outcome = (run_pre $ex.command)
        let matched = ($outcome.decision == "deny")
        if not $matched {
            $cc_misses = ($cc_misses | append $ex.rule)
        }
        $cc_results = ($cc_results | append {
            rule: $ex.rule
            command: $ex.command
            decision: $outcome.decision
            reason: $outcome.reason
            matched: $matched
        })
    }

    if $json {
        print ({
            rewrites: $rewrite_results
            course_corrections: $cc_results
            summary: {
                rewrite_total: ($rewrite_results | length)
                rewrite_misses: $rewrite_misses
                cc_total: ($cc_results | length)
                cc_misses: $cc_misses
            }
        } | to json)
        if (($rewrite_misses | length) > 0) or (($cc_misses | length) > 0) {
            exit 1
        }
        return
    }

    print ""
    print "crs rewrite — demo of all configured rules"
    print "════════════════════════════════════════════════════════════════"
    for r in $rewrite_results {
        print $"\n▸ ($r.label)"
        print $"  before : ($r.before)"
        if not $r.matched {
            print "  after  : (no rule matched — passthrough)"
        } else {
            print $"  after  : ($r.after)"
            print $"  reason : ($r.reason)"
        }
    }

    print ""
    print "crs pre (course-correct) — demo of all block rules"
    print "════════════════════════════════════════════════════════════════"
    for r in $cc_results {
        print $"\n▸ [($r.rule)]"
        print $"  command : ($r.command)"
        print $"  decision: ($r.decision)"
        if $r.reason != null {
            print $"  reason  : ($r.reason)"
        }
    }

    print ""
    print "════════════════════════════════════════════════════════════════"
    let total_misses = (($rewrite_misses | length) + ($cc_misses | length))
    if $total_misses > 0 {
        if ($rewrite_misses | length) > 0 {
            print $"  ($rewrite_misses | length) rewrite example\(s\) did not match:"
            for m in $rewrite_misses { print $"    - ($m)" }
        }
        if ($cc_misses | length) > 0 {
            print $"  ($cc_misses | length) course-correct rule\(s\) did not block:"
            for m in $cc_misses { print $"    - ($m)" }
        }
        exit 1
    } else {
        print $"  all ($rewrite_results | length) rewrite rules and ($cc_results | length) course-correct rules fired as expected"
    }
}
