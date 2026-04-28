#!/bin/sh
# enrich-handoff.sh — POSIX fallback for enrich-handoff.nu
# Requires: rtk, handoff-detect, jq
# Usage: sh scripts/enrich-handoff.sh [--since N]

set -e

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
mkdir -p "$CTX"

TOOLS_FILE="$CTX/HANDOFF.tools.yaml"
STATE_FILE="$CTX/HANDOFF.state.yaml"
TODAY=$(date +%Y-%m-%d)
JQ_LIB=$(cd "$(dirname "$0")" && pwd)

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

if ! rtk discover --format json --since "$SINCE" >"$TMP" 2>/tmp/_rtk_err; then
    echo "enrich-handoff: rtk discover failed: $(cat /tmp/_rtk_err)" >&2
    exit 1
fi

if ! jq empty "$TMP" 2>/dev/null; then
    echo "enrich-handoff: rtk discover produced invalid JSON" >&2
    exit 1
fi

jqf() { jq -r -L "$JQ_LIB" "include \"enrich-handoff\"; $1" "$TMP"; }

# --- write HANDOFF.tools.yaml (atomic via tmp file) ---

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

# Strip previous tool_usage block (everything from that line onward), append new one
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
