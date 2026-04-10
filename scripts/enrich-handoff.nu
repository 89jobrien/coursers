#!/usr/bin/env nu
# enrich-handoff.nu — write .ctx/HANDOFF.tools.yaml and update .ctx/HANDOFF.state.yaml
# Usage: nu scripts/enrich-handoff.nu [--since <int>]

def main [--since: int = 1] {
    # Verify rtk is on PATH — first? returns null on empty list rather than crashing
    let rtk_path = (which rtk | get path | first?)
    if $rtk_path == null {
        exit 0
    }

    # Resolve repo root via handoff-detect
    let root_result = (do { handoff-detect --root } | complete)
    if $root_result.exit_code != 0 {
        exit 0
    }
    let root = ($root_result.stdout | str trim)
    let ctx = ($root | path join ".ctx")

    # Create .ctx/ if absent
    mkdir $ctx

    # Run rtk discover
    let discover_result = (do { rtk discover --format json --since $since } | complete)
    if $discover_result.exit_code != 0 {
        exit 0
    }
    let data = ($discover_result.stdout | from json)

    write_tools_yaml $ctx $data $since
    merge_state_yaml $ctx $data
}

def write_tools_yaml [ctx: string, data: record, since: int] {
    let today = (date now | format date "%Y-%m-%d")

    let top_supported = (
        $data.supported?
        | default []
        | first 10
        | each {|r| {
            command: $r.command,
            count: $r.count,
            rtk_equivalent: $r.rtk_equivalent,
            est_savings_tokens: $r.estimated_savings_tokens,
            est_savings_pct: $r.estimated_savings_pct
        }}
    )

    let top_unhandled = (
        $data.unsupported?
        | default []
        | first 10
        | each {|r| {
            base_command: $r.base_command,
            count: $r.count,
            example: ($r.example | lines | first | str substring 0..<80)
        }}
    )

    let lines = [
        $"generated: ($today)"
        $"since_days: ($since)"
        $"sessions_scanned: ($data.sessions_scanned)"
        $"total_commands: ($data.total_commands)"
        "top_supported:"
    ] ++ (
        $top_supported | each {|r|
            [
                $"  - command: ($r.command)"
                $"    count: ($r.count)"
                $"    rtk_equivalent: ($r.rtk_equivalent)"
                $"    est_savings_tokens: ($r.est_savings_tokens)"
                $"    est_savings_pct: ($r.est_savings_pct)"
            ]
        } | flatten
    ) ++ ["top_unhandled:"] ++ (
        $top_unhandled | each {|r|
            [
                $"  - base_command: ($r.base_command)"
                $"    count: ($r.count)"
                $"    example: \"($r.example | str replace '"' '\"')\""
            ]
        } | flatten
    )

    $lines | str join "\n" | save --force ($ctx | path join "HANDOFF.tools.yaml")
}

def merge_state_yaml [ctx: string, data: record] {
    let state_path = ($ctx | path join "HANDOFF.state.yaml")

    # Compute summary values
    let top_cmd = (
        $data.supported? | default [] | first | default null
        | if $in != null { $"($in.command) \(($in.count)\)" } else { "" }
    )
    let savings_list = ($data.supported? | default [] | get estimated_savings_tokens)
    let total_savings = if ($savings_list | length) > 0 { $savings_list | math sum } else { 0 }
    let top_unhandled = (
        $data.unsupported? | default [] | first | default null
        | if $in != null { $"($in.base_command) \(($in.count)\)" } else { "" }
    )

    let block = [
        "tool_usage:"
        $"  sessions_scanned: ($data.sessions_scanned)"
        $"  total_commands: ($data.total_commands)"
        $"  top_command: \"($top_cmd)\""
        $"  est_savings_tokens: ($total_savings)"
        $"  unhandled_top: \"($top_unhandled)\""
    ] | str join "\n"

    # Read existing state, strip any previous tool_usage block, append new one
    let existing = if ($state_path | path exists) {
        open --raw $state_path | str trim
    } else {
        ""
    }

    let stripped = (
        $existing
        | split row "\n"
        | take while {|line| not ($line | str starts-with "tool_usage:") }
        | str join "\n"
        | str trim
    )

    let final = if ($stripped | str length) > 0 {
        $"($stripped)\n($block)\n"
    } else {
        $"($block)\n"
    }

    $final | save --force $state_path
}
