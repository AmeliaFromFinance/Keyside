#!/usr/bin/env bash
# Sets the version everywhere: companion, desktop app, Android and iOS. Doesn't commit.
# Usage: scripts/bump.sh 1.2.0
source "$(dirname "$0")/lib.sh"
need cargo

new="${1#v}"
[[ "$new" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || die "Give a version like 1.2.0"
old=$(version)
[ "$new" = "$old" ] && die "Already $new."

step "Changing $old to $new"
# perl rather than sed, so it works the same on Linux and macOS.
perl -0pi -e "s/(\[workspace\.package\]\nversion = )\"[^\"]*\"/\$1\"$new\"/" Cargo.toml
perl -0pi -e "s/^version = \"[^\"]*\"/version = \"$new\"/m" apps/desktop/src-tauri/Cargo.toml
perl -pi -e "s/\"version\": \"[^\"]*\"/\"version\": \"$new\"/" apps/desktop/src-tauri/tauri.conf.json
gradle=apps/mobile/android/app/build.gradle
code=$(perl -ne 'print $1 if /versionCode (\d+)/' "$gradle")
perl -pi -e "s/versionCode \d+/versionCode $((code + 1))/; s/versionName \"[^\"]*\"/versionName \"$new\"/" "$gradle"
perl -pi -e "s/MARKETING_VERSION = [^;]*;/MARKETING_VERSION = $new;/" apps/mobile/ios/App/App.xcodeproj/project.pbxproj

step "Updating lockfiles"
cargo update -q -w
(cd apps/desktop/src-tauri && cargo update -q -w)
ok "Version is now $new (Android build $((code + 1)))"
