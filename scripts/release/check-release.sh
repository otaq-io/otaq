#!/usr/bin/env bash
# check-release.sh [--dry-run] [<tag>]: what the release workflow checks before it builds anything (UPD-10, d0140).
#
# 1. The versions agree: `[workspace.package]` version in Cargo.toml, `version` in crates/otaq-desktop/tauri.conf.json
#    and, when a tag is given, the tag (`v0.2.0` for 0.2.0).
# 2. The updater's public key is a real minisign public key, not the placeholder the repository ships until the
#    maintainer makes the key pair (scripts/README.md, "The updater key"), and the same in both places it lives:
#    `PUBKEY` in crates/otaq-desktop/src/updates.rs (what the app checks updates against) and `plugins.updater.pubkey`
#    in tauri.conf.json.
#    With --dry-run a placeholder is only a warning: a dry run signs with a throwaway key and uploads nothing.
#
# Prints the version on stdout when everything passes; exits 1 with the reason otherwise. RELEASE_ROOT points it at
# another checkout (the tests use a copy).

set -euo pipefail

dry_run=false
if [[ "${1:-}" == "--dry-run" ]]; then
    dry_run=true
    shift
fi
tag="${1:-}"

root="${RELEASE_ROOT:-$(git -C "$(dirname "${BASH_SOURCE[0]}")" rev-parse --show-toplevel)}"
cargo_toml="$root/Cargo.toml"
tauri_conf="$root/crates/otaq-desktop/tauri.conf.json"

fail() {
    echo "check-release.sh: $*" >&2
    exit 1
}

# The `version = "…"` line inside the [workspace.package] table.
workspace_version="$(awk '
    /^\[/ { in_table = ($0 == "[workspace.package]") ; next }
    in_table && /^version *= *"/ { sub(/^version *= *"/, ""); sub(/".*$/, ""); print; exit }
' "$cargo_toml")"
[[ -n "$workspace_version" ]] || fail "no version in Cargo.toml's [workspace.package]"

# One field of tauri.conf.json, empty when it isn't there (Node, which every machine that builds Otaq has).
conf() {
    node -e 'let v = JSON.parse(require("fs").readFileSync(process.argv[1], "utf8"));
        for (const k of process.argv[2].split(".")) v = v?.[k];
        if (typeof v === "string") console.log(v);' "$tauri_conf" "$1"
}

tauri_version="$(conf version)"
[[ -n "$tauri_version" ]] || fail "no version in crates/otaq-desktop/tauri.conf.json"

if [[ "$tauri_version" != "$workspace_version" ]]; then
    fail "the versions differ: Cargo.toml's [workspace.package] says $workspace_version, crates/otaq-desktop/tauri.conf.json says $tauri_version. Set both to the release's version."
fi
if [[ -n "$tag" && "$tag" != "v$workspace_version" ]]; then
    fail "the tag $tag doesn't match the version $workspace_version in Cargo.toml and tauri.conf.json (the tag must be v$workspace_version). Bump the version first, or tag the right commit."
fi

pubkey="$(conf plugins.updater.pubkey)"
# The string after `pub const PUBKEY: &str =`, on that line or the next.
app_pubkey="$(node -e 'const m = /pub const PUBKEY: &str =\s*"([^"]*)"/.exec(require("fs").readFileSync(process.argv[1], "utf8"));
    if (m) console.log(m[1]);' "$root/crates/otaq-desktop/src/updates.rs")"
key_problem=""
if [[ "$app_pubkey" != "$pubkey" ]]; then
    key_problem="the updater's public key differs between crates/otaq-desktop/src/updates.rs (PUBKEY) and crates/otaq-desktop/tauri.conf.json (plugins.updater.pubkey); put the same key in both"
elif [[ -z "$pubkey" ]]; then
    key_problem="crates/otaq-desktop/tauri.conf.json has no updater public key (plugins.updater.pubkey)"
elif [[ "$pubkey" == PLACEHOLDER* ]]; then
    key_problem="the updater's public key in crates/otaq-desktop/tauri.conf.json is still the placeholder"
elif ! printf '%s' "$pubkey" | base64 -d 2>/dev/null | grep -q "minisign public key"; then
    key_problem="the updater's public key in crates/otaq-desktop/tauri.conf.json isn't a minisign public key (the one line \`tauri signer generate\` writes to the .pub file)"
fi
if [[ -n "$key_problem" ]]; then
    how="Make the key pair, store the private key and its password as the repository secrets TAURI_SIGNING_PRIVATE_KEY and TAURI_SIGNING_PRIVATE_KEY_PASSWORD, and put the public key in PUBKEY (crates/otaq-desktop/src/updates.rs) and plugins.updater.pubkey (scripts/README.md, \"The updater key\")."
    if $dry_run; then
        echo "check-release.sh: warning: $key_problem. A dry run goes on with a throwaway key; a release would stop here. $how" >&2
    else
        fail "$key_problem, so no release is made: every installed copy would refuse its updates. $how"
    fi
fi

echo "$workspace_version"
