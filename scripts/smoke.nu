#!/usr/bin/env nu

let bin = if ("./target/release/coursers" | path exists) {
    "./target/release/coursers" | path expand
} else {
    error make { msg: "coursers binary not found; run cargo build --release" }
}

let temp_dir = (^mktemp -d | str trim)
let rules_path = ($temp_dir | path join "rules.json")
let state_path = ($temp_dir | path join "state.json")
let rules = {
    rules: [{
        id: "smoke-block"
        enabled: true
        pattern: "^smoke-block$"
        pattern_flags: ""
        exceptions: []
        target_commands: []
        message: "blocked by smoke rule"
    }]
    failure_learning: {
        enabled: true
        block_threshold: 3
        window_seconds: 300
        state_file: null
        max_tracked_commands: 20
        cleanup_after_seconds: 600
        message_template: null
    }
}
def run_hook [bin: string, sandbox: string, rules_path: string, state_path: string, subcommand: string, payload: string] {
    cd $sandbox
    with-env {
        HOME: $sandbox
        XDG_CONFIG_HOME: $sandbox
        COURSERS_RULES: $rules_path
        COURSERS_STATE: $state_path
    } {
        do { $payload | run-external $bin $subcommand } | complete
    }
}

let results = try {
    $rules | to json | save --force $rules_path
    mut checks = []

    let blocked = (run_hook $bin $temp_dir $rules_path $state_path "pre" '{"tool_name":"Bash","tool_input":{"command":"smoke-block"}}')
    $checks = ($checks | append {
        test: "configured rule denies"
        pass: ($blocked.exit_code != 0 and ($blocked.stdout | str contains "deny"))
    })

    let allowed = (run_hook $bin $temp_dir $rules_path $state_path "pre" '{"tool_name":"Bash","tool_input":{"command":"smoke-allow"}}')
    $checks = ($checks | append { test: "clean command allows", pass: ($allowed.exit_code == 0) })

    let learned_command = "smoke-learned-failure"
    let failure = $'{"tool_name":"Bash","tool_input":{"command":"($learned_command)"},"tool_response":{"exit_code":1}}'
    for _ in 1..3 {
        run_hook $bin $temp_dir $rules_path $state_path "post" $failure | ignore
    }
    let learned = (run_hook $bin $temp_dir $rules_path $state_path "pre" $'{"tool_name":"Bash","tool_input":{"command":"($learned_command)"}}')
    $checks = ($checks | append {
        test: "learned failure denies"
        pass: ($learned.exit_code != 0 and ($learned.stdout | str contains "deny"))
    })

    let clean_state_path = ($temp_dir | path join "clean-state.json")
    let successful_post = '{"tool_name":"Bash","tool_input":{"command":"smoke-success"},"tool_response":{"exit_code":0}}'
    run_hook $bin $temp_dir $rules_path $clean_state_path "post" $successful_post | ignore
    $checks = ($checks | append {
        test: "successful post does not create failure state"
        pass: (not ($clean_state_path | path exists))
    })

    $checks
} catch {|error|
    rm -rf $temp_dir
    error make { msg: $error.msg }
}

rm -rf $temp_dir

for result in $results {
    let status = if $result.pass { "PASS" } else { "FAIL" }
    print $"($status) ($result.test)"
}

let failures = ($results | where pass == false | length)
if $failures > 0 {
    error make { msg: $"($failures) smoke test(s) failed" }
}
