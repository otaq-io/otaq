# 06 · A reformatting pre-commit changes what lands on main after approval, silently

**Kind:** missing · **Release:** must fix before the release (maintainer, 2026-11-11) · **From:** ../RESULTS-hooks.md, missing 6, scenario 2

## What happens
- A pre-commit that rewrites staged files (adds a header) ran on the squash commit: main got 2 lines the user never
  reviewed (`git diff otaq/HOOK-3 main`). Nothing in Otaq said so, and the Merge check didn't rerun (it only reruns
  when main has moved).

## Evidence
Screenshots in `../shots-hooks/`: `s2-reformatting-precommit-merged-silently.jpg`

## Where to start
`git/merge.rs` (commit in the scratch worktree, then `merge --ff-only`).

## Open questions for the product spec
- Compare the commit's tree with the reviewed tree before moving main; then: say "your pre-commit hook changed 2
  files" and show the diff, stop and ask, or rerun the Merge check?
