#!/usr/bin/env bash
# macos-sign.sh <Otaq.app> [bundle id]: signs a built Otaq.app ad-hoc (decision 0021) with the otaq command inside it
# signed under the bundle's ID, which macOS's notification service needs to tie the background service to the bundle
# (decision 0090). Tauri signs the programs it bundles under their file names ("otaq"), so this runs after it, then
# seals the bundle again. With a bundle ID it also sets CFBundleIdentifier (a check bundle, install-local.sh
# --bundle-id). Used by install-local.sh and before the .dmg is made (scripts/README.md).

set -euo pipefail

app="${1:?usage: macos-sign.sh <Otaq.app> [bundle id]}"
plist="$app/Contents/Info.plist"
bundle_id="${2:-$(plutil -extract CFBundleIdentifier raw -o - "$plist")}"

plutil -replace CFBundleIdentifier -string "$bundle_id" "$plist"
plutil -lint -s "$plist"
find "$app" -name .DS_Store -delete
echo "==> Signing $app ad-hoc ($bundle_id)"
# otaq first, under the bundle's ID (by default codesign names it after the file), then the bundle, which seals it.
codesign --force --sign - --identifier "$bundle_id" "$app/Contents/MacOS/otaq"
codesign --force --sign - "$app"
codesign --verify --deep --strict "$app"
