#!/bin/sh
# enrich-handoff.sh — POSIX fallback for enrich-handoff.nu
# Requires: rtk, handoff-detect, jq
# Usage: sh scripts/enrich-handoff.sh [--since N]
#
# TODO(enrich-fallback-doc): document which tools are optional vs required and
# what happens when handoff-detect returns a non-writable root or rtk discover
# times out. Add a BEHAVIOR section to this header comment block.
#
# TODO(enrich-dry-run): add --dry-run flag — write generated YAML to stdout
# instead of modifying files, for validation and integration testing.
#
# TODO(enrich-integration-tests): add integration test suite verifying output
# correctness with fixture JSON input and expected YAML output (no rtk required).
# See scripts/test-enrich-handoff.sh for a starting point.
#
# TODO(vector-tools-sink): wire HANDOFF.tools.yaml into a Vector pipeline:
#   1. ~/.local/bin/handoff-tools-to-ndjson.nu — glob all HANDOFF.tools.yaml,
#      parse each, extract repo from path, explode top_supported/top_unhandled
#      into tagged NDJSON, print to stdout.
#   2. ~/.config/vector/vector.toml — exec source polling every 5 min, file sink
#      to ~/.ctx/tools-sink/YYYY-MM-DD.ndjson (append mode).
#   3. ~/Library/LaunchAgents/com.joebrien.vector.plist — launchd agent to keep
#      vector running persistently.

set -e

# TODO(enrich-log-messages): emit structured log messages for all early-exit and
# noop paths (missing tools, bad --since, non-writable CTX, rtk timeout) so
# operators understand why enrichment was skipped. Affects lines 6, 15, 17,
# 19, and 30 in the original script.

SINCE=1
while [ "$#" -gt 0 ]; do
    case "$1" in
    --since)
        SINCE="$2"
        shift 2
        ;;
    *) shift ;;
    esac
done

# TODO(enrich-since-validate): validate that --since is a positive integer;
# reject non-integer or empty values with a descriptive error before any I/O.

# --- tool checks: exit 1 with diagnostic on missing deps ---

require_tool() {
    if ! command -v "$1" >/dev/null 2>&1; then
        echo "enrich-handoff: required tool '$1' not found on PATH" >&2
        exit 1
    fi
}

require_tool rtk
require_tool handoff-detect
require_tool jq

ROOT=$(handoff-detect --root 2>/dev/null) || {
    echo "enrich-handoff: handoff-detect --root failed" >&2
    exit 1
}
CTX="$ROOT/.ctx"

# TODO(enrich-ctx-writability): check writability of CTX before beginning any
# processing; exit 1 with diagnostic if not writable.
mkdir -p "$CTX"

TOOLS_FILE="$CTX/HANDOFF.tools.yaml"
STATE_FILE="$CTX/HANDOFF.state.yaml"
TODAY=$(date +%Y-%m-%d)
JQ_LIB=$(cd "$(dirname "$0")" && pwd)

# TODO(enrich-mktemp-guard): guard mktemp failure explicitly — exit 1 with a
# stderr diagnostic if mktemp cannot create a temp file.
TMP=$(mktemp)
trap 'rm -f "$TMP" "$TMP_TOOLS" "$TMP_STATE"' EXIT
TMP_TOOLS=$(mktemp)
TMP_STATE=$(mktemp)

# --- yaml_quote: emit a JSON-quoted string safe for YAML plain scalars ---
# Usage: yaml_quote <string>
# Outputs a double-quoted JSON string (e.g. "hello \"world\"")
yaml_quote() {
    printf '%s' "$1" | jq -Rs .
}

# --- run rtk discover and validate JSON output ---

# TODO(enrich-rtk-cache): cache rtk discover output keyed by --since value using
# a stable temp filename; skip re-run if fresh to enable safer re-runs and
# post-hoc analysis.
if ! rtk discover --format json --since "$SINCE" >"$TMP" 2>/tmp/_rtk_err; then
    echo "enrich-handoff: rtk discover failed: $(cat /tmp/_rtk_err)" >&2
    exit 1
fi

if ! jq empty "$TMP" 2>/dev/null; then
    echo "enrich-handoff: rtk discover produced invalid JSON" >&2
    exit 1
fi

# TODO(enrich-jq-extract): extract all jq filters below into a separate
# enrich-handoff.jq file and add fixture-based unit tests with mock rtk output.
# This decouples filter logic from shell plumbing and makes each filter testable.
jqf() { jq -r -L "$JQ_LIB" "include \"enrich-handoff\"; $1" "$TMP"; }

# --- write HANDOFF.tools.yaml (atomic via tmp file) ---

# TODO(enrich-jq-comments): add inline comments to each jqf invocation below
# explaining what each filter produces, particularly the truncation logic and
# aggregation patterns used in sessions_scanned, top_supported, top_unhandled.
# TODO(enrich-magic-numbers): parameterize magic numbers (e.g. top-N limit=10,
# min-count threshold=80) as env vars or CLI flags for operational flexibility.
{
    printf 'generated: %s\n' "$(yaml_quote "$TODAY")"
    printf 'since_days: %s\n' "$SINCE"
    jqf 'sessions_scanned'
    jqf 'total_commands'
    echo "top_supported:"
    jqf 'top_supported'
    echo "top_unhandled:"
    jqf 'top_unhandled'
} >"$TMP_TOOLS"

# TODO(enrich-yaml-validate): pipe generated YAML through yq or jq empty before
# writing to catch malformed output before it corrupts state files.
mv "$TMP_TOOLS" "$TOOLS_FILE"

# --- merge tool_usage block into HANDOFF.state.yaml (atomic) ---

TOP_CMD=$(jqf 'top_cmd')
TOTAL_SAVINGS=$(jqf 'total_savings')
UNHANDLED_TOP=$(jqf 'unhandled_top')
SESSIONS=$(jqf '.sessions_scanned')
TOTAL=$(jqf '.total_commands')

BLOCK=$(printf 'tool_usage:\n  sessions_scanned: %s\n  total_commands: %s\n  top_command: %s\n  est_savings_tokens: %s\n  unhandled_top: %s' \
    "$SESSIONS" "$TOTAL" \
    "$(yaml_quote "$TOP_CMD")" \
    "$TOTAL_SAVINGS" \
    "$(yaml_quote "$UNHANDLED_TOP")")

# TODO(enrich-state-merge-fn): extract this state file merge logic into a
# documented function with inline comments explaining the strip-and-append
# pattern. The awk/sed pipeline is non-obvious.
# TODO(enrich-state-merge-jq): refactor YAML merge into a pure jq filter
# replacing the awk/sed pipeline; use yq for YAML-JSON round-trip if available.
# Strip previous tool_usage block (everything from that line onward), append new one
# TODO(enrich-whitespace-norm): remove the unconditional trailing-whitespace and
# blank-line normalisation from the sed/awk pipeline below — these are destructive
# side-effects unrelated to the merge purpose.
if [ -f "$STATE_FILE" ]; then
    STRIPPED=$(awk '/^tool_usage:/{exit} {print}' "$STATE_FILE" \
        | sed 's/[[:space:]]*$//' \
        | awk 'NF{last=NR} {lines[NR]=$0} END{for(i=1;i<=last;i++) print lines[i]}')
else
    STRIPPED=""
fi

if [ -n "$STRIPPED" ]; then
    printf '%s\n%s\n' "$STRIPPED" "$BLOCK" >"$TMP_STATE"
else
    printf '%s\n' "$BLOCK" >"$TMP_STATE"
fi
mv "$TMP_STATE" "$STATE_FILE"

echo "enrich-handoff: wrote $TOOLS_FILE and updated $STATE_FILE"
