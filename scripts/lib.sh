# Shared helpers for the scripts in this folder. Not meant to be run on its own.

set -euo pipefail

export HERE="$PWD"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

# Who commits and tags. Set GIT_NAME and GIT_EMAIL to use someone else.
GIT_NAME="${GIT_NAME:-karasu}"
GIT_EMAIL="${GIT_EMAIL:-karasu@tuta.com}"
BRANCH="${BRANCH:-main}"
REPO="${REPO:-AmeliaFromFinance/Keyside}"

if [ -t 1 ]; then
  bold=$'\e[1m' red=$'\e[31m' green=$'\e[32m' yellow=$'\e[33m' reset=$'\e[0m'
else
  bold="" red="" green="" yellow="" reset=""
fi

step() { printf '%s==>%s %s\n' "$bold" "$reset" "$*"; }
ok() { printf '%s✓%s %s\n' "$green" "$reset" "$*"; }
warn() { printf '%s!%s %s\n' "$yellow" "$reset" "$*" >&2; }
die() {
  printf '%s✗ %s%s\n' "$red" "$*" "$reset" >&2
  exit 1
}

need() {
  for tool in "$@"; do
    command -v "$tool" >/dev/null || die "$tool isn't installed."
  done
}

# Asks a yes/no question. Defaults to no. Set YES=1 to answer yes to everything.
confirm() {
  [ "${YES:-}" = 1 ] && return 0
  local answer
  read -r -p "$1 [y/N] " answer
  [[ "$answer" =~ ^[Yy]$ ]]
}

git_as() { git -c user.name="$GIT_NAME" -c user.email="$GIT_EMAIL" "$@"; }

version() { sed -n '/^\[workspace.package\]/,/^\[/s/^version = "\(.*\)"/\1/p' Cargo.toml; }

clean_tree() { [ -z "$(git status --porcelain)" ]; }

# Waits for a workflow run on a commit to start, then follows it until it ends.
watch_run() {
  local workflow="$1" sha="$2" id=""
  step "Waiting for $workflow to start"
  for _ in $(seq 1 30); do
    id=$(gh run list -R "$REPO" --workflow "$workflow" --commit "$sha" --limit 1 --json databaseId -q '.[0].databaseId' 2>/dev/null || true)
    [ -n "$id" ] && break
    sleep 5
  done
  [ -n "$id" ] || die "$workflow didn't start. Check the Actions tab on GitHub."
  gh run watch "$id" -R "$REPO" --exit-status --interval 15 >/dev/null || die "$workflow failed: https://github.com/$REPO/actions/runs/$id"
  ok "$workflow passed"
}
