# enrich-handoff.jq — jq filter library for enrich-handoff.sh
# Include via: jq -r -L scripts/ 'include "enrich-handoff"; <filter>'

def sessions_scanned: "sessions_scanned: \(.sessions_scanned)";

def total_commands: "total_commands: \(.total_commands)";

def top_supported:
  (.supported // [])[:10][] |
  "  - command: \(.command)\n    count: \(.count)\n    rtk_equivalent: \(.rtk_equivalent)\n    est_savings_tokens: \(.estimated_savings_tokens)\n    est_savings_pct: \(.estimated_savings_pct)";

def _example_first_line:
  (. // "") | split("\n")[0] | if length > 80 then .[:80] else . end;

def top_unhandled:
  (.unsupported // [])[:10][] |
  {
    base_command,
    count,
    example: (.example | _example_first_line | @json)
  } |
  "  - base_command: \(.base_command)\n    count: \(.count)\n    example: \(.example)";

def top_cmd:
  (.supported // [])[0] |
  if . then "\(.command) (\(.count))" else empty end;

def total_savings:
  [(.supported // [])[].estimated_savings_tokens] | add // 0;

def unhandled_top:
  (.unsupported // [])[0] |
  if . then "\(.base_command) (\(.count))" else empty end;
