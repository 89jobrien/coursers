#!/bin/sh
# test-enrich-handoff-jq.sh — fixture tests for enrich-handoff.jq
# Run: sh scripts/test-enrich-handoff-jq.sh

SCRIPT_DIR=$(cd "$(dirname "$0")" && pwd)
JQ_FILE="$SCRIPT_DIR/enrich-handoff.jq"

PASS=0
FAIL=0

pass() { PASS=$((PASS+1)); echo "PASS: $1"; }
fail() { FAIL=$((FAIL+1)); echo "FAIL: $1"; echo "  expected: $2"; echo "  got:      $3"; }

jqf() {
    # jqf <filter_name> <json>
    jq -r -L "$SCRIPT_DIR" "include \"enrich-handoff\"; $1" <<EOF
$2
EOF
}

if ! command -v jq >/dev/null 2>&1; then
    echo "SKIP: jq not found" >&2
    exit 0
fi

FIXTURE_FULL='{
  "sessions_scanned": 7,
  "total_commands": 42,
  "supported": [
    {
      "command": "cargo test",
      "count": 15,
      "rtk_equivalent": "rtk cargo test",
      "estimated_savings_tokens": 800,
      "estimated_savings_pct": 35.5
    },
    {
      "command": "cargo build",
      "count": 5,
      "rtk_equivalent": "rtk cargo build",
      "estimated_savings_tokens": 200,
      "estimated_savings_pct": 20.0
    }
  ],
  "unsupported": [
    {
      "base_command": "op account",
      "count": 3,
      "example": "op account list\nsome second line"
    },
    {
      "base_command": "git log",
      "count": 2,
      "example": "git log --oneline -5"
    }
  ]
}'

FIXTURE_EMPTY='{
  "sessions_scanned": 0,
  "total_commands": 0,
  "supported": [],
  "unsupported": []
}'

FIXTURE_SPECIAL='{
  "sessions_scanned": 1,
  "total_commands": 1,
  "supported": [],
  "unsupported": [
    {
      "base_command": "evil",
      "count": 1,
      "example": "evil --flag \"quoted: value\""
    }
  ]
}'

# T1: sessions_scanned scalar
t1() {
    got=$(jqf 'sessions_scanned' "$FIXTURE_FULL")
    if [ "$got" = "sessions_scanned: 7" ]; then
        pass "T1: sessions_scanned"
    else
        fail "T1: sessions_scanned" "sessions_scanned: 7" "$got"
    fi
}

# T2: total_commands scalar
t2() {
    got=$(jqf 'total_commands' "$FIXTURE_FULL")
    if [ "$got" = "total_commands: 42" ]; then
        pass "T2: total_commands"
    else
        fail "T2: total_commands" "total_commands: 42" "$got"
    fi
}

# T3: top_supported — produces correct YAML entries
t3() {
    got=$(jqf 'top_supported' "$FIXTURE_FULL")
    if echo "$got" | grep -q "command: cargo test"; then
        pass "T3a: top_supported contains first entry"
    else
        fail "T3a: top_supported contains first entry" "command: cargo test" "$got"
    fi
    if echo "$got" | grep -q "rtk_equivalent: rtk cargo test"; then
        pass "T3b: top_supported contains rtk_equivalent"
    else
        fail "T3b: top_supported rtk_equivalent" "rtk_equivalent: rtk cargo test" "$got"
    fi
    if echo "$got" | grep -q "est_savings_tokens: 800"; then
        pass "T3c: top_supported est_savings_tokens"
    else
        fail "T3c: top_supported est_savings_tokens" "est_savings_tokens: 800" "$got"
    fi
}

# T4: top_supported — empty supported list → no output (just blank/nothing)
t4() {
    got=$(jqf 'top_supported' "$FIXTURE_EMPTY")
    if [ -z "$got" ]; then
        pass "T4: top_supported empty → no entries"
    else
        fail "T4: top_supported empty → no entries" "" "$got"
    fi
}

# T5: top_unhandled — first line of example only, truncated
t5() {
    got=$(jqf 'top_unhandled' "$FIXTURE_FULL")
    if echo "$got" | grep -q "base_command: op account"; then
        pass "T5a: top_unhandled base_command present"
    else
        fail "T5a: top_unhandled base_command" "base_command: op account" "$got"
    fi
    if echo "$got" | grep -q "second line"; then
        fail "T5b: top_unhandled must not contain second line of example" "" "$got"
    else
        pass "T5b: top_unhandled example is first line only"
    fi
}

# T6: top_unhandled — special chars in example are escaped
t6() {
    got=$(jqf 'top_unhandled' "$FIXTURE_SPECIAL")
    if echo "$got" | grep -q "quoted"; then
        pass "T6a: special-char example present"
    else
        fail "T6a: special-char example present" "quoted" "$got"
    fi
    # The example value must be on a single line (no YAML bleed)
    EXAMPLE_LINES=$(echo "$got" | grep -c "example:" || echo 0)
    if [ "$EXAMPLE_LINES" -eq 1 ]; then
        pass "T6b: example: appears exactly once (no YAML bleed)"
    else
        fail "T6b: example: line count" "1" "$EXAMPLE_LINES"
    fi
}

# T7: top_cmd — returns "command (count)" string
t7() {
    got=$(jqf 'top_cmd' "$FIXTURE_FULL")
    if [ "$got" = "cargo test (15)" ]; then
        pass "T7: top_cmd"
    else
        fail "T7: top_cmd" "cargo test (15)" "$got"
    fi
}

# T8: top_cmd — empty supported → empty string
t8() {
    got=$(jqf 'top_cmd' "$FIXTURE_EMPTY")
    if [ -z "$got" ]; then
        pass "T8: top_cmd empty → empty string"
    else
        fail "T8: top_cmd empty" "" "$got"
    fi
}

# T9: total_savings — sum of estimated_savings_tokens
t9() {
    got=$(jqf 'total_savings' "$FIXTURE_FULL")
    if [ "$got" = "1000" ]; then
        pass "T9: total_savings"
    else
        fail "T9: total_savings" "1000" "$got"
    fi
}

# T10: total_savings — empty → 0
t10() {
    got=$(jqf 'total_savings' "$FIXTURE_EMPTY")
    if [ "$got" = "0" ]; then
        pass "T10: total_savings empty → 0"
    else
        fail "T10: total_savings empty" "0" "$got"
    fi
}

# T11: unhandled_top — returns "base_command (count)"
t11() {
    got=$(jqf 'unhandled_top' "$FIXTURE_FULL")
    if [ "$got" = "op account (3)" ]; then
        pass "T11: unhandled_top"
    else
        fail "T11: unhandled_top" "op account (3)" "$got"
    fi
}

# T12: unhandled_top — empty → empty string
t12() {
    got=$(jqf 'unhandled_top' "$FIXTURE_EMPTY")
    if [ -z "$got" ]; then
        pass "T12: unhandled_top empty → empty string"
    else
        fail "T12: unhandled_top empty" "" "$got"
    fi
}

t1; t2; t3; t4; t5; t6; t7; t8; t9; t10; t11; t12

echo ""
echo "Results: $PASS passed, $FAIL failed"
[ "$FAIL" -eq 0 ]
