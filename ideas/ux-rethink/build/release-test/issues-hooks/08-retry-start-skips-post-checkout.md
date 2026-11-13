# 08 · Retry start skips a post-checkout hook that failed

**Kind:** bug · **Release:** must fix before the release (maintainer, 2026-11-11) · **From:** ../RESULTS-hooks.md, missing 8, scenario 6

## What happens
- A failing post-checkout failed the start ("creating the worktree failed … post-checkout: failing on purpose", Retry
  start). git had already made the worktree, so Retry start reused it, skipped `worktree add`, and the hook never ran
  again: whatever it does (install, codegen) silently never happens for that ticket.

## Evidence
Screenshots in `../shots-hooks/`: `s6-postcheckout-fails-start-failed.jpg`

## Where to start
`start/worktree.rs` (`worktree add --quiet`, the already-registered path).

## Open questions for the product spec
- Remove the half-made worktree on failure, or rerun the hook on retry (`git checkout` of the same commit)?
