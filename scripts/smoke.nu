#!/usr/bin/env nu
# smoke.nu — end-to-end smoke test for the coursers binary
# Usage: nu scripts/smoke.nu

# Find the binary
let bin = if (which coursers | length) > 0 {
    "coursers"
} else if ("./target/release/coursers" | path exists) {
    "./target/release/coursers"
} else {
    error make { msg: "coursers binary not found — run: cargo install --path crates/coursers" }
}

let rules_path = ($env.HOME | path join ".claude/hooks/course-correct-rules.json")
if not ($rules_path | path exists) {
    error make { msg: $"Rules file not found: ($rules_path)" }
}

let tmp_dir = (mktemp -d)
let state_path = ($tmp_dir | path join "smoke-state.json")

mut results = []

# Helper: run coursers subcommand with a JSON payload string
# Returns the `complete` record: { stdout, stderr, exit_code }
def run_cmd [subcommand: string, payload: string] {
    with-env { COURSERS_RULES: $rules_path, COURSERS_STATE: $state_path } {
        do { echo $payload | run-external $bin $subcommand } | complete
    }
}

# Test 1: should-block — grep is blocked by the live rules
let block_payload = '{"tool_name":"Bash","tool_input":{"command":"grep foo ."}}'
let t1 = (run_cmd "pre" $block_payload)
let t1_pass = ($t1.exit_code != 0) and (($t1.stdout | str contains "deny") or ($t1.stdout | str contains "block"))
$results = ($results | append { test: "should-block returns deny", pass: $t1_pass })

# Test 2: should-allow — ls is not blocked
let allow_payload = '{"tool_name":"Bash","tool_input":{"command":"ls -la"}}'
let t2 = (run_cmd "pre" $allow_payload)
let t2_pass = ($t2.exit_code == 0)
$results = ($results | append { test: "should-allow exits 0", pass: $t2_pass })

# Test 3: learned failure — post 3 failures then pre should block
# Use a unique command unlikely to be in real state
let test_cmd = "smoke-test-unique-xyz-12345"
let fail_payload = $'{"tool_name":"Bash","tool_input":{"command":"($test_cmd)"},"tool_response":{"exit_code":1}}'
let pre_payload = $'{"tool_name":"Bash","tool_input":{"command":"($test_cmd)"}}'

for _ in 1..3 {
    run_cmd "post" $fail_payload | ignore
}
let t3 = (run_cmd "pre" $pre_payload)
let t3_pass = ($t3.exit_code != 0)
$results = ($results | append { test: "learned failure blocks after threshold", pass: $t3_pass })

# Test 4: exit-0 post does not create state file (when starting fresh)
let tmp_dir2 = (mktemp -d)
let state_path2 = ($tmp_dir2 | path join "smoke-state2.json")
let ok_payload = '{"tool_name":"Bash","tool_input":{"command":"ls"},"tool_response":{"exit_code":0}}'
with-env { COURSERS_RULES: $rules_path, COURSERS_STATE: $state_path2 } {
    do { echo $ok_payload | run-external $bin "post" } | complete | ignore
}
let t4_pass = not ($state_path2 | path exists)
$results = ($results | append { test: "exit-0 post does not record", pass: $t4_pass })

# Cleanup
rm -rf $tmp_dir
rm -rf $tmp_dir2

# Print results
print ""
print "coursers smoke test results"
print "────────────────────────────────────────"
for r in $results {
    let icon = if $r.pass { "PASS" } else { "FAIL" }
    print $"  ($icon)  ($r.test)"
}
print "────────────────────────────────────────"
let failures = ($results | where pass == false | length)
if $failures > 0 {
    print $"  ($failures) test(s) failed"
    exit 1
} else {
    print "  all tests passed"
}
