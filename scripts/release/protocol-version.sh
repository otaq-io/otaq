#!/usr/bin/env bash
# Print this checkout's protocol number (`PROTOCOL_VERSION` in crates/otaq-core/src/protocol.rs), for the release's
# `latest.json` field `protocol_version` (UPD-03, d0140). Reads the source, so nothing has to be built first; the test
# `protocol_snapshot_version_line_is_readable_by_the_release_script` keeps the line in the form read here.

set -euo pipefail

root="$(git -C "$(dirname "${BASH_SOURCE[0]}")" rev-parse --show-toplevel)"
version="$(sed -n 's/^pub const PROTOCOL_VERSION: u32 = \([0-9][0-9]*\);$/\1/p' "$root/crates/otaq-core/src/protocol.rs")"
if [[ -z "$version" ]]; then
    echo "protocol-version.sh: no 'pub const PROTOCOL_VERSION: u32 = N;' line in crates/otaq-core/src/protocol.rs" >&2
    exit 1
fi
echo "$version"
