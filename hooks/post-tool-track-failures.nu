#!/usr/bin/env nu
# post-tool-track-failures.nu — PostToolUse/Bash hook
# Delegates all logic to the coursers binary.

def main [] {
    let raw = try { open --raw /dev/stdin } catch { exit 0 }
    echo $raw | ^coursers post
}
