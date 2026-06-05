#!/usr/bin/env bash
#
# The canonical verification gate for lopress.
#
# These three commands ARE the gate: the Stop hook in .claude/settings.json
# runs this script, and AGENTS.md tells agents to run it before stopping. Keep
# this file as the single source of truth so the local check, the documented
# check, and the automated gate can never drift apart.
#
# Always uses --workspace (not -p <crate>) so it matches the Stop hook exactly —
# a per-crate clippy can pass locally while the workspace gate fails elsewhere.
#
# Usage:  bash scripts/check.sh   (or ./scripts/check.sh)
# Exit:   0 if all three pass, 1 if any fail.
#
# NOTE: `cargo test --release` is NOT run here. It is CI-only because it
# roughly doubles compile+test time. The Stop hook runs the same three
# commands as this script (see .claude/settings.json). Running --release
# on every stop would make the agent noticeably slow. The CI step is the
# sole place that exercises the release profile (debug_assert! compiled
# out, cfg(debug_assertions) false) — it adds coverage; it does not mask
# a debug failure.
set -u
export RUSTFLAGS="${RUSTFLAGS:--Dwarnings}"

# Run from the repo root regardless of where this is invoked from.
cd "$(dirname "$0")/.." || exit 1

failed=0
echo '=== cargo fmt ==='
cargo fmt --all || failed=1
echo '=== taplo fmt ==='
if command -v taplo >/dev/null 2>&1; then
    taplo fmt --check || failed=1
else
    echo "ERROR: taplo not found. Install with: cargo install taplo-cli --locked"
    failed=1
fi
echo '=== cargo clippy ==='
cargo clippy --workspace --all-targets -- -D warnings || failed=1
echo '=== suppression justifications ==='
bash scripts/check-suppressions.sh || failed=1
echo '=== cargo test ==='
cargo test --workspace || failed=1

exit $failed
