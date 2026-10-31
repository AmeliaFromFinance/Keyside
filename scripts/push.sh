#!/usr/bin/env bash
# Checks for merge conflicts with GitHub, puts your commits on top, pushes to main and waits for CI.
# Usage: scripts/push.sh [--no-wait]
source "$(dirname "$0")/lib.sh"
need git gh

clean_tree || die "You have changes that aren't committed. Run scripts/commit.sh first."
[ "$(git branch --show-current)" = "$BRANCH" ] || die "Switch to $BRANCH first."
if git grep -nE '^(<<<<<<<|>>>>>>>) ' -- . ':!*.md' >/dev/null; then
  git grep -nE '^(<<<<<<<|>>>>>>>) ' -- . ':!*.md'
  die "Leftover conflict markers in the files above."
fi

step "Fetching $BRANCH from GitHub"
git fetch -q origin "$BRANCH"
behind=$(git rev-list --count "HEAD..origin/$BRANCH")
ahead=$(git rev-list --count "origin/$BRANCH..HEAD")
[ "$ahead" = 0 ] && die "Nothing to push."

if [ "$behind" != 0 ]; then
  step "GitHub has $behind newer commit(s). Checking for conflicts"
  if ! git merge-tree --write-tree --name-only --no-messages HEAD "origin/$BRANCH" >/tmp/keyside-merge; then
    tail -n +2 /tmp/keyside-merge
    die "Your commits conflict with GitHub in the files above. Run: git rebase origin/$BRANCH, fix them, then push again."
  fi
  ok "No conflicts"
  git_as rebase -q "origin/$BRANCH"
fi

git log --format='  %h %s' "origin/$BRANCH..HEAD"
confirm "Push these to $BRANCH?" || exit 1
step "Pushing $ahead commit(s) to $BRANCH"
git push -q origin "HEAD:$BRANCH"
ok "Pushed $(git log -1 --format='%h %s')"

[ "${1:-}" = "--no-wait" ] && exit 0
watch_run ci.yml "$(git rev-parse HEAD)"
