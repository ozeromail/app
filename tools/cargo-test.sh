#!/usr/bin/env bash
# Runs `cargo test` with the given arguments. On GitHub Actions, each failed test's output also
# becomes an annotation (GitHub shows 10 per step), readable without the job log.
set -uo pipefail

log="$(mktemp)"
trap 'rm -f "$log"' EXIT
cargo test "$@" 2>&1 | tee "$log"
status=${PIPESTATUS[0]}
if [ "$status" -ne 0 ] && [ -n "${GITHUB_ACTIONS:-}" ]; then
    # libtest lists each failure as `---- name stdout ----` and its output up to a blank line.
    # Colors go first: CI asks cargo for them. Text is escaped as workflow commands require.
    sed 's/\x1b\[[0-9;]*[A-Za-z]//g' "$log" | awk '
        function emit() { if (text != "") print "::error title=" name "::" text; text = "" }
        /^---- .* stdout ----$/ { emit(); name = $2; gsub(/%/, "%25", name); gsub(/:/, "%3A", name); gsub(/,/, "%2C", name); inside = 1; next }
        inside && /^$/ { emit(); inside = 0; next }
        inside { gsub(/%/, "%25"); gsub(/\r/, "%0D"); text = text (text == "" ? "" : "%0A") $0 }
        END { emit() }
    ' | head -10
fi
exit "$status"
