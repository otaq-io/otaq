#!/usr/bin/env bash
# macos-finish.sh <Otaq.app> <version> <out folder>: the macOS setup files from a bundled Otaq.app (UPD-01, UPD-10).
# The bundler signs `otaq` under its own name; notifications need it under the bundle's ID (decision 0090), so the
# app is signed again first (scripts/macos-sign.sh), and the .dmg (scripts/macos-dmg.sh, never Tauri's dmg bundle,
# which would make and sign the app again) and the updater's archive are made from that app. The app is bundled
# without Tauri's updater files (`createUpdaterArtifacts: false`): they'd pack the app as Tauri signed it.
#   <out>/Otaq_<version>_universal.dmg              the setup file: the app and an Applications shortcut
#   <out>/Otaq_<version>_universal.app.tar.gz(.sig) what the updater downloads, signed with the updater key
# The archive's signature needs TAURI_SIGNING_PRIVATE_KEY and TAURI_SIGNING_PRIVATE_KEY_PASSWORD, as the bundler does.

set -euo pipefail

app="${1:?usage: macos-finish.sh <Otaq.app> <version> <out folder>}"
version="${2:?usage: macos-finish.sh <Otaq.app> <version> <out folder>}"
out="${3:?usage: macos-finish.sh <Otaq.app> <version> <out folder>}"
root="$(git -C "$(dirname "${BASH_SOURCE[0]}")" rev-parse --show-toplevel)"
name="Otaq_${version}_universal"
mkdir -p "$out"

"$root/scripts/macos-sign.sh" "$app"

echo "==> $name.dmg"
"$root/scripts/macos-dmg.sh" "$app" "$out/$name.dmg"

echo "==> $name.app.tar.gz and its signature"
COPYFILE_DISABLE=1 tar -czf "$out/$name.app.tar.gz" -C "$(dirname "$app")" "$(basename "$app")"
npm --prefix "$root/crates/otaq-desktop" exec -- tauri signer sign --app-version "$version" "$out/$name.app.tar.gz"
[[ -s "$out/$name.app.tar.gz.sig" ]] || {
    echo "macos-finish.sh: no signature written for $name.app.tar.gz" >&2
    exit 1
}
