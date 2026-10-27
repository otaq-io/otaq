#!/usr/bin/env bash
# macos-dmg.sh <Otaq.app> <out.dmg>: the .dmg of an Otaq.app already signed by scripts/macos-sign.sh (decision 0140):
# the app and an Applications shortcut to drag it onto, compressed. Tauri's own .dmg step makes the app again and
# signs `otaq` under its file name, so the release builds the app, signs it, then makes the .dmg with this. No Finder
# scripting, so it runs the same on a CI runner and here.

set -euo pipefail

app="${1:?usage: macos-dmg.sh <Otaq.app> <out.dmg>}"
out="${2:?usage: macos-dmg.sh <Otaq.app> <out.dmg>}"
codesign --verify --deep --strict "$app"

staging="$(mktemp -d)"
trap 'rm -rf "$staging"' EXIT
chmod 755 "$staging"
ditto "$app" "$staging/$(basename "$app")"
ln -s /Applications "$staging/Applications"
rm -f "$out"
hdiutil create -quiet -volname Otaq -srcfolder "$staging" -fs HFS+ -format UDZO -ov "$out"
hdiutil verify -quiet "$out"
echo "==> $out"
