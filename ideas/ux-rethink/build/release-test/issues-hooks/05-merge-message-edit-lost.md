# 05 · Approve & merge forgets the edited message after a refusal

**Kind:** bug · **Release:** must fix before the release (maintainer, 2026-11-11) · **From:** ../RESULTS-hooks.md, missing 5, scenario 2

## What happens
- The user edited the squash message's first line to `feat: …`; a pre-commit hook then refused the merge; reopening
  Approve & merge showed the original "HOOK-1 Add subtraction…" again. The user has to fix it on every try.

## Evidence
Screenshots in `../shots-hooks/`: `s2-retry-dialog-lost-edited-message.jpg`

## Where to start
The Approve dialog's message field and where the proposed message comes from (`pr_dialog.rs` / approve).

## Open questions for the product spec
- Keep the edit until the ticket merges or the user resets it? Also across an app restart?
