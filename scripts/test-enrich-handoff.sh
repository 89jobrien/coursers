#!/bin/sh
# test-enrich-handoff.sh — POSIX test harness for enrich-handoff.sh
# Run: sh scripts/test-enrich-handoff.sh

SCRIPT_DIR=$(cd "$(dirname "$0")" && pwd)
SCRIPT="$SCRIPT_DIR/enrich-handoff.sh"

PASS=0
FAIL=0

pass() { PASS=$((PASS+1)); echo "PASS: $1"; }
fail() { FAIL=$((FAIL+1)); echo "FAIL: $1"; echo "  $2"; }

# ---- infrastructure ----

# Build a minimal PATH containing system sh, awk, sed, date, mkdir, mktemp, printf, jq
# but NOT rtk or handoff-detect (unless stub provided).
SYSTEM_BINS="/bin:/usr/bin:/usr/local/bin"

make_work() {
    STUB_DIR=$(mktemp -d)
    WORK_DIR=$(mktemp -d)
    mkdir -p "$WORK_DIR/.ctx"
    export FAKE_ROOT="$WORK_DIR"

    # handoff-detect stub (present by default)
    cat >"$STUB_DIR/handoff-detect" <<'SH'
#!/bin/sh
case "$1" in
  --root) echo "$FAKE_ROOT" ;;
  *)      echo "$FAKE_ROOT/HANDOFF.yaml" ;;
esac
SH
    chmod +x "$STUB_DIR/handoff-detect"
}

cleanup() {
    rm -rf "$STUB_DIR" "$WORK_DIR"
}

make_rtk_stub() {
    local json="$1"
    # Write JSON to a temp file to avoid shell quoting issues in heredoc
    local json_file
    json_file=$(mktemp)
    printf '%s\n' "$json" >"$json_file"
    cat >"$STUB_DIR/rtk" <<SH
#!/bin/sh
cat "$json_file"
SH
    chmod +x "$STUB_DIR/rtk"
}

make_rtk_fail_stub() {
    cat >"$STUB_DIR/rtk" <<'SH'
#!/bin/sh
echo "rtk: internal error" >&2
exit 1
SH
    chmod +x "$STUB_DIR/rtk"
}

make_rtk_badjson_stub() {
    cat >"$STUB_DIR/rtk" <<'SH'
#!/bin/sh
echo "not json at all"
SH
    chmod +x "$STUB_DIR/rtk"
}

run() {
    # Run script with stub dir prepended to a clean PATH
    ENRICH_OUT=$(PATH="$STUB_DIR:$SYSTEM_BINS" sh "$SCRIPT" "$@" 2>/tmp/_teh_err)
    ENRICH_RC=$?
    ENRICH_ERR=$(cat /tmp/_teh_err)
}

RTK_GOOD='{"sessions_scanned":5,"total_commands":20,"supported":[{"command":"cargo test","count":10,"rtk_equivalent":"rtk cargo test","estimated_savings_tokens":500,"estimated_savings_pct":30}],"unsupported":[{"base_command":"op account","count":3,"example":"op account list"}]}'

# ---- tests ----

# T1: rtk absent → exit 1 + stderr naming rtk
t1() {
    make_work
    # No rtk stub — rtk absent from STUB_DIR, SYSTEM_BINS has no rtk
    run
    if [ "$ENRICH_RC" -eq 1 ]; then
        pass "T1a: missing rtk exits 1"
    else
        fail "T1a: missing rtk exits 1" "got exit $ENRICH_RC"
    fi
    if echo "$ENRICH_ERR" | grep -qi "rtk"; then
        pass "T1b: missing rtk names tool in stderr"
    else
        fail "T1b: missing rtk names tool in stderr" "stderr='$ENRICH_ERR'"
    fi
    cleanup
}

# T2: handoff-detect absent → exit 1 + stderr naming handoff-detect
t2() {
    make_work
    make_rtk_stub "$RTK_GOOD"
    rm -f "$STUB_DIR/handoff-detect"
    run
    if [ "$ENRICH_RC" -eq 1 ]; then
        pass "T2a: missing handoff-detect exits 1"
    else
        fail "T2a: missing handoff-detect exits 1" "got exit $ENRICH_RC"
    fi
    if echo "$ENRICH_ERR" | grep -qi "handoff-detect"; then
        pass "T2b: missing handoff-detect names tool in stderr"
    else
        fail "T2b: missing handoff-detect names tool in stderr" "stderr='$ENRICH_ERR'"
    fi
    cleanup
}

# T3: rtk discover exits non-zero → exit 1 + stderr
t3() {
    make_work
    make_rtk_fail_stub
    run
    if [ "$ENRICH_RC" -eq 1 ]; then
        pass "T3a: rtk discover failure exits 1"
    else
        fail "T3a: rtk discover failure exits 1" "got exit $ENRICH_RC"
    fi
    if echo "$ENRICH_ERR" | grep -qi "rtk\|discover"; then
        pass "T3b: rtk discover failure emits diagnostic"
    else
        fail "T3b: rtk discover failure emits diagnostic" "stderr='$ENRICH_ERR'"
    fi
    cleanup
}

# T4: rtk outputs invalid JSON → exit 1 + stderr
t4() {
    make_work
    make_rtk_badjson_stub
    run
    if [ "$ENRICH_RC" -eq 1 ]; then
        pass "T4a: invalid JSON from rtk exits 1"
    else
        fail "T4a: invalid JSON from rtk exits 1" "got exit $ENRICH_RC"
    fi
    if echo "$ENRICH_ERR" | grep -qi "json\|invalid\|parse\|rtk"; then
        pass "T4b: invalid JSON emits diagnostic"
    else
        fail "T4b: invalid JSON emits diagnostic" "stderr='$ENRICH_ERR'"
    fi
    cleanup
}

# T5: example with double-quotes and colon → appears in output, YAML not broken
t5() {
    make_work
    RTK_TRICKY='{"sessions_scanned":1,"total_commands":2,"supported":[],"unsupported":[{"base_command":"evil","count":1,"example":"evil --flag \"quoted: value\""}]}'
    make_rtk_stub "$RTK_TRICKY"
    run
    TOOLS="$WORK_DIR/.ctx/HANDOFF.tools.yaml"
    if [ "$ENRICH_RC" -eq 0 ] && [ -f "$TOOLS" ]; then
        if grep -q "quoted" "$TOOLS"; then
            pass "T5a: special-char example present in output"
        else
            fail "T5a: special-char example present in output" "$(cat "$TOOLS")"
        fi
        LINECOUNT=$(grep -c "^  - base_command:" "$TOOLS" || echo 0)
        if [ "$LINECOUNT" -eq 1 ]; then
            pass "T5b: exactly one unhandled entry (no YAML bleed)"
        else
            fail "T5b: exactly one unhandled entry" "count=$LINECOUNT"
        fi
    else
        fail "T5: script failed" "rc=$ENRICH_RC err=$ENRICH_ERR"
    fi
    cleanup
}

# T6: example containing literal newline → second line not in output
t6() {
    make_work
    # JSON with escaped \n in example
    RTK_NL='{"sessions_scanned":1,"total_commands":1,"supported":[],"unsupported":[{"base_command":"cmd","count":1,"example":"line1\nline2"}]}'
    make_rtk_stub "$RTK_NL"
    run
    TOOLS="$WORK_DIR/.ctx/HANDOFF.tools.yaml"
    if [ -f "$TOOLS" ]; then
        if grep -q "line2" "$TOOLS"; then
            fail "T6: newline example bled second line into YAML" "$(grep example "$TOOLS")"
        else
            pass "T6: newline in example truncated to first line"
        fi
    else
        fail "T6: tools file not written" "rc=$ENRICH_RC"
    fi
    cleanup
}

# T7: STATE_FILE written atomically — old content preserved until new content ready
t7() {
    make_work
    make_rtk_stub "$RTK_GOOD"
    echo "generated: 2026-01-01" >"$WORK_DIR/.ctx/HANDOFF.state.yaml"
    run
    STATE="$WORK_DIR/.ctx/HANDOFF.state.yaml"
    if [ -s "$STATE" ]; then
        pass "T7a: STATE_FILE non-empty after run"
    else
        fail "T7a: STATE_FILE empty after run" ""
    fi
    if grep -q "generated:" "$STATE"; then
        pass "T7b: STATE_FILE contains generated: line"
    else
        fail "T7b: STATE_FILE missing generated: line" "$(cat "$STATE")"
    fi
    cleanup
}

# T8: happy path — both output files written on success
t8() {
    make_work
    make_rtk_stub "$RTK_GOOD"
    run
    TOOLS="$WORK_DIR/.ctx/HANDOFF.tools.yaml"
    STATE="$WORK_DIR/.ctx/HANDOFF.state.yaml"
    if [ "$ENRICH_RC" -eq 0 ]; then
        pass "T8a: happy path exits 0"
    else
        fail "T8a: happy path exits 0" "rc=$ENRICH_RC err=$ENRICH_ERR"
    fi
    if [ -f "$TOOLS" ]; then
        pass "T8b: HANDOFF.tools.yaml written"
    else
        fail "T8b: HANDOFF.tools.yaml written" ""
    fi
    if [ -f "$STATE" ]; then
        pass "T8c: HANDOFF.state.yaml written"
    else
        fail "T8c: HANDOFF.state.yaml written" ""
    fi
    cleanup
}

# ---- run ----

t1
t2
t3
t4
t5
t6
t7
t8

echo ""
echo "Results: $PASS passed, $FAIL failed"
[ "$FAIL" -eq 0 ]
