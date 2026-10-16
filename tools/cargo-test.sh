#!/usr/bin/env bash
# Runs `cargo test` with the given arguments. On GitHub Actions, failed tests and their panic
# messages also become annotations (GitHub shows 10 per step), readable without the job log.
set -uo pipefail

log="$(mktemp)"
cargo test "$@" 2>&1 | tee "$log"
status=${PIPESTATUS[0]}
if [ "$status" -ne 0 ] && [ -n "${GITHUB_ACTIONS:-}" ]; then
    # Colors off first: CI asks cargo for them.
    sed 's/\x1b\[[0-9;]*m//g' "$log" |
        grep -E "^test .* FAILED$|panicked at" -A1 | grep -v "^--$" | head -10 |
        while IFS= read -r line; do echo "::error::$line"; done
fi
rm -f "$log"
exit "$status"
