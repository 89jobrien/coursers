#!/usr/bin/env nu
# smoke.nu — end-to-end smoke test for the workspace crs binary
# Usage: nu scripts/smoke.nu

const script_dir = path self | path dirname
const repo_root = $script_dir | path dirname
let manifest_path = ($repo_root | path join "Cargo.toml")
let bin = ($repo_root | path join "target/debug/crs")
let rules_path = ($repo_root | path join "tests/integration/fixtures/rules_basic.json")

let build = (do {
    run-external cargo build "--manifest-path" $manifest_path "--bin" "crs"
} | complete)
if $build.exit_code != 0 {
    error make { msg: $"failed to build workspace crs binary: ($build.stderr | str trim)" }
}

let tmp_dir = (mktemp -d)
let state_path = ($tmp_dir | path join "smoke-state.json")
mut results = []

# Run the exact binary built from this workspace, isolated from user rules and state.
def run_cmd [bin: path, rules_path: path, state_path: path, subcommand: string, payload: string] {
    with-env { COURSERS_RULES: $rules_path, COURSERS_STATE: $state_path } {
        do { $payload | run-external $bin $subcommand } | complete
    }
}

# Test 1: the smoke rules pass the CLI health check.
let validation = (do {
    run-external $bin "validate" "--rules" $rules_path
} | complete)
$results = ($results | append {
    test: "rules validate"
    pass: ($validation.exit_code == 0)
})

# Test 2: grep is blocked by the smoke rules.
let block_payload = ({ tool_name: "Bash", tool_input: { command: "grep foo ." } } | to json -r)
let block = (run_cmd $bin $rules_path $state_path "pre" $block_payload)
let block_pass = ($block.exit_code != 0) and (($block.stdout | str contains "deny") or ($block.stdout | str contains "block"))
$results = ($results | append { test: "should-block returns deny", pass: $block_pass })

# Test 3: a command absent from the smoke rules is allowed.
let allow_payload = ({ tool_name: "Bash", tool_input: { command: "ls -la" } } | to json -r)
let allow = (run_cmd $bin $rules_path $state_path "pre" $allow_payload)
$results = ($results | append { test: "should-allow exits 0", pass: ($allow.exit_code == 0) })

# Test 4: three failures teach the pre hook to block the command.
let test_cmd = "smoke-test-unique-xyz-12345"
let fail_payload = ({
    tool_name: "Bash"
    tool_input: { command: $test_cmd }
    tool_response: { exit_code: 1 }
} | to json -r)
let pre_payload = ({ tool_name: "Bash", tool_input: { command: $test_cmd } } | to json -r)
for _ in 1..3 {
    run_cmd $bin $rules_path $state_path "post" $fail_payload | ignore
}
let learned = (run_cmd $bin $rules_path $state_path "pre" $pre_payload)
$results = ($results | append {
    test: "learned failure blocks after threshold"
    pass: ($learned.exit_code != 0)
})

# Test 5: a successful post does not create failure state.
let tmp_dir2 = (mktemp -d)
let state_path2 = ($tmp_dir2 | path join "smoke-state2.json")
let ok_payload = ({
    tool_name: "Bash"
    tool_input: { command: "ls" }
    tool_response: { exit_code: 0 }
} | to json -r)
run_cmd $bin $rules_path $state_path2 "post" $ok_payload | ignore
$results = ($results | append {
    test: "exit-0 post does not record"
    pass: (not ($state_path2 | path exists))
})

rm -rf $tmp_dir
rm -rf $tmp_dir2

print ""
print "coursers smoke test results"
print "────────────────────────────────────────"
for result in $results {
    let icon = if $result.pass { "PASS" } else { "FAIL" }
    print $"  ($icon)  ($result.test)"
}
print "────────────────────────────────────────"
let failures = ($results | where pass == false | length)
if $failures > 0 {
    print $"  ($failures) test(s) failed"
    exit 1
}
print "  all tests passed"
