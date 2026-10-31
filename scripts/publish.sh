#!/usr/bin/env bash
# Publishes a draft release, then offers to delete any other drafts.
# Usage: scripts/publish.sh [v1.2.0]   (defaults to the current version)
source "$(dirname "$0")/lib.sh"
need gh

tag="${1:-v$(version)}"
[[ "$tag" == v* ]] || tag="v$tag"
releases() { gh api "repos/$REPO/releases" --paginate --jq "$1"; }

read -r id draft assets < <(releases ".[] | select(.tag_name == \"$tag\") | \"\(.id) \(.draft) \(.assets | length)\"" | head -1) || true
[ -n "${id:-}" ] || die "No release for $tag."
[ "$draft" = true ] || die "$tag is already published."

step "$tag has $assets downloads"
confirm "Publish $tag? Everyone will see it." || exit 1
gh api -X PATCH "repos/$REPO/releases/$id" -F draft=false -f make_latest=true >/dev/null
ok "Published https://github.com/$REPO/releases/tag/$tag"

releases '.[] | select(.draft) | "\(.id) \(.tag_name)"' | while read -r old name; do
  if confirm "Delete the leftover $name draft?" </dev/tty; then
    gh api -X DELETE "repos/$REPO/releases/$old" >/dev/null && ok "Deleted $name draft"
  fi
done
