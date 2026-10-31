#!/usr/bin/env bash
# Installs everything needed to build and test, then builds the plugins and the web app.
# Usage: scripts/setup.sh
source "$(dirname "$0")/lib.sh"
need node pnpm cargo

step "Installing packages"
pnpm install --frozen-lockfile
step "Building plugins and the web app"
pnpm build
ok "Ready. Run scripts/test.sh to check everything works."
