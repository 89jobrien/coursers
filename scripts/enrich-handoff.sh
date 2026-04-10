#!/bin/sh
# enrich-handoff.sh — POSIX fallback for enrich-handoff.nu
# Requires: rtk, handoff-detect, jq
# Usage: sh scripts/enrich-handoff.sh [--since N]

set -e

SINCE=1
while [ "$#" -gt 0 ]; do
    case "$1" in
        --since) SINCE="$2"; shift 2 ;;
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

TMP=$(mktemp)
trap 'rm -f "$TMP" "$TMP_TOOLS" "$TMP_STATE"' EXIT
TMP_TOOLS=$(mktemp)
TMP_STATE=$(mktemp)

# --- run rtk discover and validate JSON output ---

if ! rtk discover --format json --since "$SINCE" >"$TMP" 2>/tmp/_rtk_err; then
    echo "enrich-handoff: rtk discover failed: $(cat /tmp/_rtk_err)" >&2
    exit 1
fi

if ! jq empty "$TMP" 2>/dev/null; then
    echo "enrich-handoff: rtk discover produced invalid JSON" >&2
    exit 1
fi

# --- write HANDOFF.tools.yaml (atomic via tmp file) ---

{
    echo "generated: $TODAY"
    echo "since_days: $SINCE"
    jq -r '"sessions_scanned: \(.sessions_scanned)"' "$TMP"
    jq -r '"total_commands: \(.total_commands)"' "$TMP"

    echo "top_supported:"
    jq -r '
      (.supported // [])[:10][] |
      "  - command: \(.command)\n    count: \(.count)\n    rtk_equivalent: \(.rtk_equivalent)\n    est_savings_tokens: \(.estimated_savings_tokens)\n    est_savings_pct: \(.estimated_savings_pct)"
    ' "$TMP"

    echo "top_unhandled:"
    jq -r '
      (.unsupported // [])[:10][] |
      (.example // "") |
      split("\n")[0] |
      if length > 80 then .[:80] else . end
    ' "$TMP" | while IFS= read -r example; do
        base=$(jq -r --arg ex "$example" '
          (.unsupported // [])[] | select((.example // "") | split("\n")[0] | startswith($ex))
          | .base_command' "$TMP" | head -1)
        count=$(jq -r --arg ex "$example" '
          (.unsupported // [])[] | select((.example // "") | split("\n")[0] | startswith($ex))
          | .count' "$TMP" | head -1)
        printf '  - base_command: %s\n    count: %s\n    example: %s\n' \
            "$base" "$count" "$(printf '%s' "$example" | jq -Rs .)"
    done
} >"$TMP_TOOLS"
mv "$TMP_TOOLS" "$TOOLS_FILE"

# --- merge tool_usage block into HANDOFF.state.yaml (atomic) ---

TOP_CMD=$(jq -r '
  (.supported // [])[0] |
  if . then "\(.command) (\(.count))" else "" end
' "$TMP")

TOTAL_SAVINGS=$(jq -r '
  [(.supported // [])[].estimated_savings_tokens] | add // 0
' "$TMP")

UNHANDLED_TOP=$(jq -r '
  (.unsupported // [])[0] |
  if . then "\(.base_command) (\(.count))" else "" end
' "$TMP")

SESSIONS=$(jq -r '.sessions_scanned' "$TMP")
TOTAL=$(jq -r '.total_commands' "$TMP")

BLOCK=$(printf 'tool_usage:\n  sessions_scanned: %s\n  total_commands: %s\n  top_command: %s\n  est_savings_tokens: %s\n  unhandled_top: %s' \
    "$SESSIONS" "$TOTAL" \
    "$(printf '%s' "$TOP_CMD" | jq -Rs .)" \
    "$TOTAL_SAVINGS" \
    "$(printf '%s' "$UNHANDLED_TOP" | jq -Rs .)")

# Strip previous tool_usage block (everything from that line onward), append new one
if [ -f "$STATE_FILE" ]; then
    STRIPPED=$(awk '/^tool_usage:/{exit} {print}' "$STATE_FILE" | \
        sed 's/[[:space:]]*$//' | \
        awk 'NF{last=NR} {lines[NR]=$0} END{for(i=1;i<=last;i++) print lines[i]}')
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
