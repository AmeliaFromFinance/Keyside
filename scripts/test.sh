#!/usr/bin/env bash
# Runs every check CI runs. Stops at the first failure.
# Usage: scripts/test.sh [--quick]   (--quick skips the slow end-to-end tests and clippy)
source "$(dirname "$0")/lib.sh"
need pnpm cargo
quick=0
[ "${1:-}" = "--quick" ] && quick=1

step "Building plugins"
pnpm -s build:plugins
step "Type checks"
pnpm -s check
step "Rust formatting"
cargo fmt --all -- --check
if [ "$quick" = 0 ]; then
  step "Rust lints"
  cargo clippy -q --all-targets -- -D warnings
fi
step "Rust tests"
cargo test -q --workspace
step "JavaScript tests"
pnpm -s test:js
if [ "$quick" = 0 ]; then
  step "End-to-end tests"
  pnpm -s test:e2e
fi
ok "All checks passed"
