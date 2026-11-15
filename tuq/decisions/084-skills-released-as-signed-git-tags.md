# 084 · Skills are released as signed git tags, not bundles

- **Date:** 2026-11-14
- **Status:** Superseded by 102
- **Related:** 058, 059, 060, 061, 066, 068, 069, 076, 078

## Summary

A skill release is a git tag on the skills repository, signed with the maintainer's key. Otaq finds new tags with git,
fetches only the tagged skill files and checks the tag's signature. There are no packaged bundles, no release files
and no index; wherever an earlier decision says "bundle", it means one of these signed tags.

## Decision

- The publish command tags the skills repository and signs the tag with git's own signing, using the key on the
  maintainer's Mac (decision 059). The tag message carries the oldest Otaq it works with (decision 060) and the note
  for users (decision 069).
- Every 15 minutes, and when its service starts, Otaq asks git for newer tags (decision 068): one small request.
- Otaq fetches only the tagged skill files and verifies the tag's signature against the key built into the app; an
  unsigned tag or one signed by another key is ignored (decision 066).
- Each tag's files are kept in their own folder, so a session keeps its skills (decisions 061, 078).
- Tags are not GitHub releases: no release list shows them, and the app's releases stay the only ones in
  `otaq-io/otaq`.
- Each skills repository tags its own skills: tuq's tags and the workflow skills' tags (decision 057). Otaq
  follows both; only workflow-skills tags carry an Otaq range (decision 060).

## Why

The user asked for a way without skill bundles. Letting each tool's plugin system update the skills was turned down:
it drops the signature, lets the tool change skills under running sessions, has no compatibility check, and OpenCode
has no plugin system. Git is already required by Otaq, and a signed tag keeps everything decided for bundles without
packaging, release files or an index.
