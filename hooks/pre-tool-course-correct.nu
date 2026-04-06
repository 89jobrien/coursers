#!/usr/bin/env nu
# pre-tool-course-correct.nu — PreToolUse/Bash hook
# Delegates all logic to the coursers binary.

def main [] {
    let raw = try { open --raw /dev/stdin } catch { exit 0 }
    echo $raw | ^coursers pre
}
