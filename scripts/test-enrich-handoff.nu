#!/usr/bin/env nu

const script_dir = path self | path dirname
let fixture_dir = ($script_dir | path join "fixtures/enrich-handoff")
let script = ($script_dir | path join "enrich-handoff.nu")
let root = (mktemp -d)
let ctx = ($root | path join ".ctx")
mkdir $ctx
cp ($fixture_dir | path join "input-state.yaml") ($ctx | path join "HANDOFF.state.yaml")

let nu_exe = $nu.current-exe
let result = (with-env { PATH: [] } {
    do { run-external $nu_exe $script "--input" ($fixture_dir | path join "rtk-discover.json") "--root" $root "--generated-date" "2026-09-05" "--since" 7 } | complete
})

if $result.exit_code != 0 {
    rm -rf $root
    error make { msg: $"enrichment failed: ($result.stderr | str trim)" }
}

def assert_fixture [actual_path: string, expected_path: string] {
    let actual = (open --raw $actual_path | str trim)
    let expected = (open --raw $expected_path | str trim)
    if $actual != $expected {
        error make { msg: $"output mismatch for ($actual_path)\nexpected:\n($expected)\nactual:\n($actual)" }
    }
}

assert_fixture ($ctx | path join "HANDOFF.tools.yaml") ($fixture_dir | path join "expected-tools.yaml")
assert_fixture ($ctx | path join "HANDOFF.state.yaml") ($fixture_dir | path join "expected-state.yaml")

rm -rf $root
print "PASS: fixture enrichment output matches expected YAML"
