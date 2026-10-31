#!/usr/bin/env bash
# Commits every change as the project identity (see lib.sh).
# Usage: scripts/commit.sh "Short summary" ["Longer description"]
source "$(dirname "$0")/lib.sh"
need git

[ -n "${1:-}" ] || die 'Give a commit message, like: scripts/commit.sh "Fix the mute key"'
clean_tree && die "Nothing to commit."

git add -A
if [ -n "$(git diff --cached --name-only --diff-filter=U)" ] || git diff --cached | grep -qE '^\+(<<<<<<<|>>>>>>>) '; then
  die "There are unresolved merge conflicts. Fix them first."
fi
git diff --cached --stat
if [ -n "${2:-}" ]; then
  git_as commit -q -m "$1" -m "$2"
else
  git_as commit -q -m "$1"
fi
ok "Committed $(git log -1 --format='%h %s')"
