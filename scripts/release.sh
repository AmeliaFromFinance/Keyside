#!/usr/bin/env bash
# Makes a new release: bumps the version, files the changelog notes under it, commits,
# pushes, tags, and waits for the draft release to build.
# Usage: scripts/release.sh 1.2.0
source "$(dirname "$0")/lib.sh"
need git gh cargo

new="${1#v}"
[[ "$new" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || die "Give a version like 1.2.0"
tag="v$new"
clean_tree || die "Commit your changes first."
git rev-parse -q --verify "refs/tags/$tag" >/dev/null && die "$tag already exists."

# Everything under "## Unreleased" becomes the release notes.
notes=$(awk '/^## Unreleased/{on=1; next} /^## \[/{on=0} on' CHANGELOG.md | sed '/^[[:space:]]*$/d')
[ -n "$notes" ] || die "Nothing under \"## Unreleased\" in CHANGELOG.md. Add the release notes there first."
printf '%sRelease notes for %s:%s\n%s\n\n' "$bold" "$tag" "$reset" "$notes"
confirm "Release $tag?" || exit 1

"$(dirname "$0")/bump.sh" "$new"
perl -pi -e "s/^## Unreleased\$/## Unreleased\n\n## [$new] - $(date +%Y-%m-%d)/" CHANGELOG.md

"$(dirname "$0")/commit.sh" "Release $new"
"$(dirname "$0")/push.sh"

step "Tagging $tag"
git_as tag -a "$tag" -m "Keyside $new"
git push -q origin "$tag"
watch_run release.yml "$(git rev-parse HEAD)"
ok "Draft ready: https://github.com/$REPO/releases. Run scripts/publish.sh $tag to publish it."
