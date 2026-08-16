#!/usr/bin/env nu
# xtest.nu — divergence testing between `crs probe` (matches the raw whole
# command string, no pipeline segmentation) and `crs pre` (the real
# PreToolUse hook path, which runs check_pipeline()'s ;/&&/|| segmentation
# before matching). When these two disagree on a command, a rule behaves
# differently through the real hook chain than a manual rule check would
# suggest — see CLAUDE.md "Coursers Rules Gotchas" and the no-bash-use-nu
# pipeline-splitting bug (doob todo b6ff3600-...) this technique caught.
#
# Rather than a fixed hand-picked list of commands, this generates variants
# automatically: for every course-correct rule with a known trigger example,
# it tests the bare form AND four chained forms (leading `;`, trailing `;`,
# `&&`, `||`) and auto-detects any variant where probe and pre disagree —
# no per-case "expected divergence" labels to maintain. Any new divergence
# surfaces as a failure the first time it's introduced, for any rule, not
# just the ones already known about.
#
# Usage:
#   nu scripts/xtest.nu "some command"     # test one command, all 5 forms
#   nu scripts/xtest.nu --suite            # run the auto-generated rule suite
#   nu scripts/xtest.nu --suite --json     # machine-readable suite output
#
# Planned: wire the --suite run into taskit (xtask) as a CI-checkable task
# once the nu-script version is validated. Not yet done.

# One known-good trigger example per course-correct rule id — the same
# examples used by scripts/demo-rewrites.nu, kept here so xtest can fuzz
# around each one independently. Update when rules are added/removed/renamed
# in ~/.config/coursers/course-correct-rules.json.
def rule_triggers [] {
    {
        no-grep-use-tool: 'grep -rn "TODO" src/'
        no-sed-n-use-read: "sed -n '10,20p' src/main.rs"
        no-cat-use-read: 'cat src/main.rs'
        no-heredoc-payload-to-kgx: "cat <<EOF | kgx wiki write\nsome content\nEOF"
        no-head-tail-use-read: 'tail -50 src/main.rs'
        no-find-use-glob: 'find ./src -name "*.rs"'
        no-npm-use-bun: 'npm install'
        no-nvm-use-mise: 'nvm use 20'
        no-pip-use-uv: 'pip install requests'
        no-sleep-find-work: 'sleep 30'
        no-ls-use-glob: 'ls -la src/'
        no-sed-use-edit: "sed -i 's/foo/bar/' src/main.rs"
        no-bash-use-nu: 'if [ -f foo ]; then echo hi; fi'
        no-python3-file-edit: "python3 -c 'open(\"f.txt\", \"w\").write(\"x\")'"
        no-cargo-install-multi-path: 'cargo install --path crates/foo --path crates/bar'
        no-cd-use-absolute-paths: 'cd crates/coursers'
        no-kubectl-use-personal-mcp: 'kubectl get pods'
        no-docker-use-personal-mcp: 'docker images'
        no-bw-use-personal-mcp: 'bw status --raw'
    }
}

# Build the 5 forms to fuzz a trigger command with: bare, and 4 chained
# variants using each shell separator check_pipeline() splits on, plus a
# leading-vs-trailing position for `;` since position can matter for how a
# rule's exceptions/anchors interact with the chain.
def chain_variants [trigger: string] {
    [
        { form: "bare", command: $trigger }
        { form: "leading ;", command: $"echo noop; ($trigger)" }
        { form: "trailing ;", command: $"($trigger); echo noop" }
        { form: "&&", command: $"echo noop && ($trigger)" }
        { form: "||", command: $"false || ($trigger)" }
    ]
}

def bin [] {
    if (which crs | length) > 0 {
        "crs"
    } else if (which coursers | length) > 0 {
        "coursers"
    } else {
        error make { msg: "crs/coursers binary not found — run: cargo install --path crates/coursers" }
    }
}

# Verdict via `crs probe`: scans every output line for a leading
# BLOCK/PASS/ALLOW verdict token — does NOT assume a fixed line offset,
# since a multi-line command (e.g. a heredoc) shifts where the verdict
# line falls in the output. Any BLOCK line means the command is blocked.
def probe_verdict [command: string] {
    let b = (bin)
    let result = (do { $command | run-external $b "probe" } | complete)
    let has_block = ($result.stdout | lines | any {|l| ($l | str trim | split row -r '\s+' | first | default "") == "BLOCK" })
    if $has_block { "blocked" } else { "allowed" }
}

# Verdict via `crs pre`: the real PreToolUse hook path, fed the same
# command as a Bash tool_input payload.
def pre_verdict [command: string] {
    let b = (bin)
    let payload = { tool_name: "Bash", tool_input: { command: $command } } | to json
    let result = (do { $payload | run-external $b "pre" } | complete)
    if $result.exit_code == 0 { "allowed" } else { "blocked" }
}

def run_one [command: string] {
    let probe = (probe_verdict $command)
    let pre = (pre_verdict $command)
    { command: $command, probe_verdict: $probe, pre_verdict: $pre, agree: ($probe == $pre) }
}

def run_suite [] {
    let triggers = (rule_triggers)
    let rule_ids = ($triggers | columns)

    mut results = []
    for rule_id in $rule_ids {
        let trigger = ($triggers | get $rule_id)
        for v in (chain_variants $trigger) {
            let r = (run_one $v.command)
            $results = ($results | append ($r | merge { rule: $rule_id, form: $v.form }))
        }
    }
    $results
}

def print_suite_report [results: list] {
    print ""
    print "xtest — auto-generated crs probe vs crs pre divergence suite"
    print "════════════════════════════════════════════════════════════════"

    let by_rule = ($results | group-by rule)
    for rule_id in ($by_rule | columns) {
        let rows = ($by_rule | get $rule_id)
        let divergent = ($rows | where agree == false)
        let status = if ($divergent | length) > 0 { $"DIVERGE \(($divergent | length)/($rows | length)\)" } else { "agree  " }
        print $"\n[($status)] ($rule_id)"
        for r in $rows {
            if not $r.agree {
                print $"    ✗ ($r.form): probe=($r.probe_verdict) pre=($r.pre_verdict)  «($r.command)»"
            }
        }
    }

    print ""
    print "════════════════════════════════════════════════════════════════"
}

def main [command?: string, --suite, --json] {
    if $suite {
        let results = (run_suite)

        if $json {
            print ($results | to json)
        } else {
            print_suite_report $results
        }

        let divergences = ($results | where agree == false)
        let rules_affected = ($divergences | get rule | uniq | length)
        let rules_total = ($results | get rule | uniq | length)

        if ($divergences | length) > 0 {
            print $"  ($divergences | length) divergent case\(s\) across ($rules_affected)/($rules_total) rules — see above"
            exit 1
        } else {
            print $"  0 divergences — ($rules_total) rules × 5 chain forms all agree between probe and pre"
        }
        return
    }

    if $command == null {
        error make { msg: "usage: nu scripts/xtest.nu \"<command>\"  OR  nu scripts/xtest.nu --suite" }
    }

    mut results = []
    for v in (chain_variants $command) {
        let r = (run_one $v.command)
        $results = ($results | append ($r | merge { form: $v.form }))
    }

    if $json {
        print ($results | to json)
    } else {
        print $"trigger : ($command)"
        for r in $results {
            let mark = if $r.agree { "OK  " } else { "FAIL" }
            print $"  [($mark)] ($r.form): probe=($r.probe_verdict) pre=($r.pre_verdict)  «($r.command)»"
        }
    }

    let divergences = ($results | where agree == false)
    if ($divergences | length) > 0 {
        exit 1
    }
}
