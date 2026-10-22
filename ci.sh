#!/usr/bin/env bash
# Run the checks that GitHub Actions runs on this repository, locally.
#   usage: ci.sh [quick|full]
#   quick: fmt + clippy (host) + typos + machete
#   full : quick + nextest (host) + clippy/nextest for wasm32
set -u
cd "$(dirname "$0")"
MODE="${1:-quick}"
fail=0
step() {
    printf '\n=== %s\n' "$*"
    if "$@"; then printf '    OK: %s\n' "$*"; else printf '    FAIL: %s\n' "$*"; fail=1; fi
}

step cargo fmt --all --check
step typos
step cargo machete
step cargo clippy --workspace --all-targets --locked -- -D warnings

if [ "$MODE" = "full" ]; then
    step cargo nextest run --workspace --all-features --locked --no-fail-fast
    step cargo test --workspace --all-features --doc --locked
    step cargo clippy --target wasm32-unknown-unknown --package oxjvm-classfile --package oxjvm-platform --package oxjvm-vm --package oxjvm-java --package oxjvm-wasm --locked -- -D warnings
    step cargo nextest run --target wasm32-wasip1 --package oxjvm-classfile --package oxjvm-platform --package oxjvm-vm --package oxjvm-java --package oxjvm-wasm --locked --no-fail-fast
fi

printf '\n=== RESULT: %s\n' "$([ "$fail" = 0 ] && echo PASS || echo FAIL)"
exit "$fail"
