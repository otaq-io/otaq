# 10 · A pre-merge-commit hook never runs

**Kind:** missing · **Release:** must fix before the release (maintainer, 2026-11-11) · **From:** ../RESULTS-hooks.md, missing 10, scenario 2

## What happens
- Approve & merge squashes, so no merge commit is made and a team's pre-merge-commit gate is silently skipped.

## Evidence
Screenshots in `../shots-hooks/`: The hook log table in ../RESULTS-hooks.md.

## Where to start
`git/merge.rs`.

## Open questions for the product spec
- Run the project's pre-merge-commit hook itself before moving the target, or document and show it?
