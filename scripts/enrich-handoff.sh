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

command -v rtk >/dev/null 2>&1 || exit 0
command -v handoff-detect >/dev/null 2>&1 || exit 0
command -v jq >/dev/null 2>&1 || exit 0

ROOT=$(handoff-detect --root 2>/dev/null) || exit 0
CTX="$ROOT/.ctx"
mkdir -p "$CTX"

TOOLS_FILE="$CTX/HANDOFF.tools.yaml"
STATE_FILE="$CTX/HANDOFF.state.yaml"
TODAY=$(date +%Y-%m-%d)

TMP=$(mktemp)
trap 'rm -f "$TMP"' EXIT

rtk discover --format json --since "$SINCE" >"$TMP" 2>/dev/null || exit 0

# --- write HANDOFF.tools.yaml ---

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
      .example |= (split("\n")[0] // "") |
      .example |= if (. | length) > 80 then .[:80] else . end |
      "  - base_command: \(.base_command)\n    count: \(.count)\n    example: \"\(.example)\""
    ' "$TMP"
} >"$TOOLS_FILE"

# --- merge tool_usage block into HANDOFF.state.yaml ---

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

BLOCK=$(printf 'tool_usage:\n  sessions_scanned: %s\n  total_commands: %s\n  top_command: "%s"\n  est_savings_tokens: %s\n  unhandled_top: "%s"' \
    "$SESSIONS" "$TOTAL" "$TOP_CMD" "$TOTAL_SAVINGS" "$UNHANDLED_TOP")

# Strip previous tool_usage block (everything from that line onward), append new one
if [ -f "$STATE_FILE" ]; then
    STRIPPED=$(awk '/^tool_usage:/{exit} {print}' "$STATE_FILE" | sed 's/[[:space:]]*$//' | awk 'NF{last=NR} {lines[NR]=$0} END{for(i=1;i<=last;i++) print lines[i]}')
else
    STRIPPED=""
fi

if [ -n "$STRIPPED" ]; then
    printf '%s\n%s\n' "$STRIPPED" "$BLOCK" >"$STATE_FILE"
else
    printf '%s\n' "$BLOCK" >"$STATE_FILE"
fi

echo "enrich-handoff: wrote $TOOLS_FILE and updated $STATE_FILE"
