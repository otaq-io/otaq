# 10 · Checks not yet seen in the real app

**Kind:** re-test · **Release:** re-test in the next round · **From:** re-tests 5 and hooks

## What happens
- A held ticket's column (piece 27): no held ticket came up without signing a tool out.
- "Removing waits until no ticket is in it" on a stage **with** tickets (piece 30): only the empty case was seen.
- Approve & open PR / Push to PR with pre-push and commit-msg hooks: the VM has no PR provider (gh/az); git-hooks
  pieces 13 and 16 need it.
- Piece 23's answer sessions, and Claude Code still needing Enter there.

## Evidence
Screenshots in `../shots-retest-5/`: none.

## Where to start
The next Windows re-test; a machine with gh or az for the PR paths.

## Open questions
- none.
