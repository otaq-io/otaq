#!/usr/bin/env bash
# smoke.sh <app|deb|appimage|nsis> <built bundle> [<version>]: the release workflow's check of a built setup file on
# its own runner (UPD-10, d0140), before anything is uploaded. Also runs on the Mac against a local build:
#   scripts/release/smoke.sh app "$CARGO_TARGET_DIR/universal-apple-darwin/release/bundle/macos/Otaq.app" 0.1.0
#
# 1. The bundle has the `otaq` command and the plugin folder where the app looks for them:
#      app       Otaq.app/Contents/MacOS/otaq, Contents/Resources/plugin
#      deb       unpacked with dpkg-deb: usr/bin/otaq, usr/lib/Otaq/plugin
#      appimage  unpacked with --appimage-extract: usr/bin/otaq, usr/lib/Otaq/plugin
#      nsis      installed silently (/S) into a scratch folder: otaq.exe, plugin\
# 2. `otaq --version` names the version (when one is given).
# 3. The background service starts on a throwaway data folder and home, answers `daemon status`, has made its runtime
#    copy with the plugin folder in it (so it found the plugin folder, UPD-02), and stops.
# Nothing outside a scratch folder is used: OTAQ_HOME, HOME and the tools' config folders point into it.

set -euo pipefail

kind="${1:?usage: smoke.sh <app|deb|appimage|nsis> <bundle> [version]}"
bundle="${2:?usage: smoke.sh <app|deb|appimage|nsis> <bundle> [version]}"
version="${3:-}"

base="${RUNNER_TEMP:-${TMPDIR:-/tmp}}"
command -v cygpath >/dev/null && base="$(cygpath -u "$base")"
scratch="$(mktemp -d "${base%/}/otaq-smoke.XXXXXX")"
step() { echo "==> $*"; }
fail() {
    echo "smoke.sh: $*" >&2
    exit 1
}

exe=""
case "$kind" in
    app)
        bin="$bundle/Contents/MacOS"
        plugin="$bundle/Contents/Resources/plugin"
        ;;
    deb)
        dpkg-deb -x "$bundle" "$scratch/unpacked"
        bin="$scratch/unpacked/usr/bin"
        plugin="$scratch/unpacked/usr/lib/Otaq/plugin"
        ;;
    appimage)
        cp "$bundle" "$scratch/Otaq.AppImage"
        chmod +x "$scratch/Otaq.AppImage"
        (cd "$scratch" && ./Otaq.AppImage --appimage-extract >/dev/null)
        bin="$scratch/squashfs-root/usr/bin"
        plugin="$scratch/squashfs-root/usr/lib/Otaq/plugin"
        ;;
    nsis)
        # NSIS takes the folder as the last argument, unquoted, in Windows form.
        target="$(cygpath -w "$scratch/installed")"
        "$bundle" /S "/D=$target"
        bin="$scratch/installed"
        plugin="$scratch/installed/plugin"
        exe=".exe"
        ;;
    *) fail "unknown kind $kind (app, deb, appimage or nsis)" ;;
esac
otaq="$bin/otaq$exe"

step "The command and the plugin folder"
[[ -x "$otaq" ]] || fail "no otaq command at $otaq"
[[ -d "$plugin/claude" && -d "$plugin/skills" ]] || fail "no plugin folder (with claude/ and skills/) at $plugin"

step "otaq --version"
printed="$("$otaq" --version)"
echo "$printed"
if [[ -n "$version" && "$printed" != *" $version"* ]]; then
    fail "otaq --version says '$printed', not $version"
fi

home="$scratch/h"
mkdir -p "$home"
run() {
    OTAQ_HOME="$home" HOME="$home" USERPROFILE="$home" CLAUDE_CONFIG_DIR="$home/.claude" CODEX_HOME="$home/.codex" \
        COPILOT_HOME="$home/.copilot" XDG_CONFIG_HOME="$home/.config" XDG_DATA_HOME="$home/.local/share" \
        XDG_STATE_HOME="$home/.local/state" ZDOTDIR="$home" "$otaq" "$@"
}
stop() { run daemon stop >/dev/null 2>&1 || true; }
trap stop EXIT

step "The background service starts on a throwaway data folder"
run daemon start
running="$(run daemon status --json)"
grep -q '"running": *true' <<<"$running" || fail "daemon status says it isn't running: $running"

step "It found the plugin folder (its runtime copy has one)"
found=""
for copy in "$home"/runtime/*/plugin; do
    [[ -d "$copy/claude" ]] && found="$copy"
done
[[ -n "$found" ]] || fail "no runtime copy with a plugin folder in $home/runtime (the service didn't find the plugin folder; see $home/logs)"
echo "$found"

step "It stops"
run daemon stop
trap - EXIT
grep -q '"running": *false' <<<"$(run daemon status --json)" || fail "the service is still running"
# A tool check the service started (`copilot --version` unpacks itself into the throwaway home) may still be writing
# there for a moment; a scratch folder left behind isn't a failure.
rm -rf "$scratch" 2>/dev/null || true
echo "smoke.sh: $kind passed"
