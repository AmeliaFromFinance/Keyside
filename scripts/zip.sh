#!/usr/bin/env bash
# Bundles the project into a zip, leaving out everything .gitignore ignores and the .git folder.
# Includes changes you haven't committed yet.
# Usage: scripts/zip.sh [output.zip]
source "$(dirname "$0")/lib.sh"
need git zip

out="${1:-$ROOT/../keyside-$(version).zip}"
case "$out" in /*) ;; *) out="$HERE/$out" ;; esac
rm -f "$out"

step "Collecting files"
# Tracked and new files, minus ignored ones and files deleted from disk.
list=$(mktemp)
tmp=$(mktemp -d)
trap 'rm -rf "$list" "$tmp"' EXIT
git ls-files -z --cached --others --exclude-standard | while IFS= read -r -d '' f; do
  if [ -e "$f" ]; then printf '%s\n' "$f"; fi
done >"$list"
count=$(wc -l <"$list")

step "Zipping $count files"
# A keyside/ folder inside the zip, so it unpacks tidily.
ln -s "$ROOT" "$tmp/keyside"
(cd "$tmp" && sed 's|^|keyside/|' "$list" | zip -q -X "$out" -@)
ok "Saved $out ($(du -h "$out" | cut -f1))"
