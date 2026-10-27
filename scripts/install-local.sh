#!/usr/bin/env bash
# Build Otaq.app from this checkout with Tauri's bundler and install it (decision 0140, macOS): the same bundle a
# release's .dmg holds, but built from source, so it never checks for updates and About says "Built from source".
# No admin rights; shell files are never edited (About's Add to my shell's PATH does that, on a click).
#
#   scripts/install-local.sh                       build → ~/Applications/Otaq.app
#   scripts/install-local.sh --dest <folder>       <folder>/Otaq.app instead (tests, teammates, checks)
#   scripts/install-local.sh --bundle-id <id>      another bundle ID (default io.otaq.desktop), for a check bundle
#                                                  that asks macOS without using up the real ID's one question
#                                                  (never in a temporary folder such as /tmp: macOS refuses
#                                                  notifications for apps there)
#   scripts/install-local.sh --from <folder>       install <folder>/Otaq.app instead of building (tests)
#
# The build is `npm run bundle -- --bundles app` (crates/otaq-desktop: tauri build with tauri.bundle.conf.json, which
# adds the otaq command next to otaq-app and the plugin folder in Contents/Resources). Then scripts/macos-sign.sh
# sets the bundle ID and signs otaq under it, ad-hoc (decisions 0021, 0090). Before replacing an installed Otaq.app it
# stops the background service of the data folder in use (OTAQ_HOME, or ~/.otaq), only when that service runs from
# the bundle being replaced.

set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
app_name="Otaq.app"
default_id="io.otaq.desktop"

dest="$HOME/Applications"
bundle_id="$default_id"
from=""
while [ $# -gt 0 ]; do
    case "$1" in
        --dest) dest="${2:?--dest needs a folder}"; shift 2 ;;
        --bundle-id) bundle_id="${2:?--bundle-id needs an ID}"; shift 2 ;;
        --from) from="${2:?--from needs a folder}"; shift 2 ;;
        -h|--help) sed -n '2,18p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
        *) echo "install-local.sh: unknown option $1 (see --help)" >&2; exit 2 ;;
    esac
done

if [ "$(uname -s)" != "Darwin" ]; then
    echo "install-local.sh: builds Otaq.app, so it runs on macOS only" >&2
    exit 1
fi

if [ -z "$from" ]; then
    echo "==> Building $app_name (npm run bundle -- --bundles app)"
    # Never a release: OTAQ_RELEASE=1 belongs to the release workflow (d0140).
    # No updater files: those are a release's, signed with its key.
    (cd "$root/crates/otaq-desktop" && env -u OTAQ_RELEASE npm run bundle -- --bundles app \
        --config '{"bundle":{"createUpdaterArtifacts":false}}')
    from="${CARGO_TARGET_DIR:-$root/target}/release/bundle/macos"
fi
if [ ! -x "$from/$app_name/Contents/MacOS/otaq" ] || [ ! -x "$from/$app_name/Contents/MacOS/otaq-app" ]; then
    echo "install-local.sh: $from/$app_name with otaq and otaq-app is missing" >&2
    exit 1
fi

mkdir -p "$dest"
dest="$(cd "$dest" && pwd -P)"
target="$dest/$app_name"
staging="$(mktemp -d "$dest/.otaq-install.XXXXXX")"
trap 'rm -rf "$staging"' EXIT
bundle="$staging/$app_name"
cp -R "$from/$app_name" "$bundle"
"$root/scripts/macos-sign.sh" "$bundle" "$bundle_id"

# The service of the data folder in use, when it runs from the bundle being replaced: its lock file holds its PID.
data="${OTAQ_HOME:-$HOME/.otaq}"
pid="$(head -n 1 "$data/service.lock" 2>/dev/null | tr -cd '0-9' || true)"
if [ -n "$pid" ] && [ -d "$target" ]; then
    # -ww: the whole path, however long (ps may cut its columns otherwise).
    program="$(ps -ww -o comm= -p "$pid" 2>/dev/null || true)"
    case "$program" in
        "$target/Contents/MacOS/"*)
            echo "==> Stopping the background service of $data before replacing $target"
            OTAQ_HOME="$data" "$target/Contents/MacOS/otaq" daemon stop \
                || echo "    (the old otaq couldn't stop it; carrying on)"
            ;;
    esac
fi
rm -rf "$target"
mv "$bundle" "$target"
# Known to LaunchServices before it's first opened, so the service's notifications are accepted from the start
# (macOS refuses them for a bundle it hasn't registered: "Notifications are not allowed for this application"). Not
# in a temporary folder (the tests' bundles): macOS refuses notifications from there anyway.
case "$target" in
    /tmp/*|/private/tmp/*|/var/folders/*|/private/var/folders/*) ;;
    *) /System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister \
           -f "$target" || echo "    (couldn't register $target with LaunchServices; opening it once does)" ;;
esac
echo "==> Installed $target"
