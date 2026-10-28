#!/usr/bin/env bash
# One-shot local CI for Ferrite, mirroring .github/workflows/ci.yml.
#
# Usage: tools/check.sh [--quick]
#   --quick   skip cargo clippy
#
# Runs, in order and stopping at the first failure:
#   cargo fmt --all --check
#   cargo clippy --workspace --all-targets --all-features -- -D warnings
#   cargo test --workspace --all-features
# then prints a PASS/FAIL line with the total elapsed seconds.

set -uo pipefail

# Rust lives in ~/.cargo/bin and is not on PATH by default (see tools/env.sh).
export PATH="$HOME/.cargo/bin:$PATH"

cd "$(dirname "${BASH_SOURCE[0]}")/.." || {
    echo "FAIL: cannot enter the repository root (0s)" >&2
    exit 1
}

quick=0
for arg in "$@"; do
    case "$arg" in
        --quick)
            quick=1
            ;;
        -h|--help)
            echo "Usage: tools/check.sh [--quick]"
            echo "  --quick   skip cargo clippy"
            exit 0
            ;;
        *)
            echo "FAIL: unknown argument: $arg" >&2
            echo "Usage: tools/check.sh [--quick]" >&2
            exit 2
            ;;
    esac
done

if ! command -v cargo >/dev/null 2>&1; then
    echo "FAIL: cargo not found on PATH or in $HOME/.cargo/bin (0s)" >&2
    exit 1
fi

SECONDS=0
failed=""

# run_step <label> <command...>: prints a header, runs the command, and records
# the failure instead of aborting the shell.
run_step() {
    local label="$1"
    shift
    local step_start=$SECONDS
    local status

    echo
    echo "==> $label"
    "$@"
    status=$?
    if [ "$status" -ne 0 ]; then
        echo "--- $label failed in $((SECONDS - step_start))s (exit $status)"
        failed="$label"
        return 1
    fi
    echo "--- $label passed in $((SECONDS - step_start))s"
    return 0
}

if ! run_step "cargo fmt --all --check" cargo fmt --all --check; then
    echo
    echo "FAIL: $failed (${SECONDS}s)"
    exit 1
fi

if [ "$quick" -eq 0 ]; then
    if ! run_step "cargo clippy --workspace --all-targets --all-features -- -D warnings" \
        cargo clippy --workspace --all-targets --all-features -- -D warnings; then
        echo
        echo "FAIL: $failed (${SECONDS}s)"
        exit 1
    fi
else
    echo
    echo "==> skipping cargo clippy (--quick)"
fi

if ! run_step "cargo test --workspace --all-features" cargo test --workspace --all-features; then
    echo
    echo "FAIL: $failed (${SECONDS}s)"
    exit 1
fi

if [ "$quick" -eq 1 ]; then
    echo
    echo "PASS: all checks passed (${SECONDS}s, quick: clippy skipped)"
else
    echo
    echo "PASS: all checks passed (${SECONDS}s)"
fi
exit 0
