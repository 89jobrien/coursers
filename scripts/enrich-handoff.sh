#!/bin/sh
# Deprecated compatibility wrapper for the canonical Nushell implementation.
# Usage: sh scripts/enrich-handoff.sh [--since N]

set -eu

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
exec nu "$SCRIPT_DIR/enrich-handoff.nu" "$@"
