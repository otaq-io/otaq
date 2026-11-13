#!/usr/bin/env bash
# Otaq local checks: the frontend steps (Node version, npm ci, ts-rs freshness, tsc, ESLint, Prettier, Vitest,
# the page build), then fmt, clippy, tests (cargo-nextest, then the doc examples).
#
# Run this before every commit. A green run is the definition of done for every ticket (decision 0056,
# SDD §23.2). GitHub Actions runs it on Linux for every pull request (.github/workflows/checks.yml).
#
# Works from any folder inside the repository. Stops at the first failing step and exits non-zero. Needs Node 24 (the
# version in crates/otaq-desktop/.nvmrc) and npm on PATH; setup: scripts/README.md.
#
# With no arguments it runs every step. Naming groups runs only those, in the order above, so CI can run them in
# parallel jobs: `frontend` (node to page), `lint` (fmt, clippy), `test` (nextest), `doctest` (doc examples).
# CHECK_TEST_PARTITION (for example `hash:1/2`) runs one share of the nextest tests. scripts/check.ps1 takes the same
# groups on Windows.

set -uo pipefail

groups=("$@")
if [ ${#groups[@]} -eq 0 ]; then
    groups=(frontend lint test doctest)
fi
for group in "${groups[@]}"; do
    case "$group" in
        frontend | lint | test | doctest) ;;
        *)
            echo "check.sh: unknown group '$group' (frontend, lint, test, doctest)" >&2
            exit 1
            ;;
    esac
done
wants() {
    local group
    for group in "${groups[@]}"; do
        [ "$group" = "$1" ] && return 0
    done
    return 1
}

if ! command -v git >/dev/null 2>&1; then
    echo "check.sh: git is required (core tests run against temporary git repositories)" >&2
    exit 1
fi

if wants test && ! cargo nextest --version >/dev/null 2>&1; then
    echo "check.sh: cargo-nextest is required: brew install cargo-nextest (or cargo install cargo-nextest --locked)" >&2
    exit 1
fi

root="$(git -C "$(dirname "${BASH_SOURCE[0]}")" rev-parse --show-toplevel)" || {
    echo "check.sh: not inside the Otaq repository" >&2
    exit 1
}
cd "$root" || exit 1

run_step() {
    local name="$1"
    shift
    echo "==> $name: $*"
    if ! "$@"; then
        echo "check.sh: FAILED at step '$name'" >&2
        exit 1
    fi
}

# The frontend steps of SDD section 5.5 (decision 0075 point 7). The page lives inside the desktop crate; every npm
# command runs there.
if wants frontend; then
    desktop=crates/otaq-desktop
    node_wanted="$(tr -d '[:space:]v' < "$desktop/.nvmrc")"
    node_wanted="${node_wanted%%.*}"
    echo "==> node: Node $node_wanted (from $desktop/.nvmrc)"
    if ! command -v node >/dev/null 2>&1 || ! command -v npm >/dev/null 2>&1; then
        echo "check.sh: Node $node_wanted is required (node or npm not found on PATH); see $desktop/.nvmrc" >&2
        exit 1
    fi
    node_found="$(node --version)"
    node_major="${node_found#v}"
    node_major="${node_major%%.*}"
    if [ "$node_major" != "$node_wanted" ]; then
        echo "check.sh: Node $node_wanted is required (found $node_found); see $desktop/.nvmrc" >&2
        exit 1
    fi

    run_step npm-ci npm --prefix "$desktop" ci --no-audit --no-fund
    # Before tsc, so a stale generated type is reported as stale rather than as a type error.
    run_step types cargo test -p otaq-desktop generated_types_are_fresh
    run_step tsc npm --prefix "$desktop" run typecheck
    run_step eslint npm --prefix "$desktop" run lint
    run_step prettier npm --prefix "$desktop" run format:check
    run_step vitest npm --prefix "$desktop" run test
    # Plain cargo builds don't embed ui/dist (only the Tauri CLI's release build does), but building the page here
    # catches what tsc doesn't, such as an import Vite can't resolve, before the cargo steps.
    run_step page npm --prefix "$desktop" run build:page
fi

if wants lint; then
    run_step fmt cargo fmt --all --check
    run_step clippy cargo clippy --workspace --all-targets -- -D warnings
fi
if wants test; then
    # Each test in its own process, in parallel, no retries, killed after 5 minutes (.config/nextest.toml); nextest
    # skips doc examples.
    partition=()
    if [ -n "${CHECK_TEST_PARTITION:-}" ]; then
        partition=(--partition "$CHECK_TEST_PARTITION")
    fi
    # The `+` form: an empty array is "unbound" under `set -u` in the Mac's bash 3.2.
    run_step test cargo nextest run --workspace ${partition[@]+"${partition[@]}"}
fi
if wants doctest; then
    run_step doctest cargo test --workspace --doc
fi

echo "check.sh: all checks passed (${groups[*]})"
