# 02 · A pre-rebase hook (or a no-rebase team) makes Approve & merge impossible

**Kind:** missing · **Release:** must fix before the release (maintainer, 2026-11-11) · **From:** ../RESULTS-hooks.md, missing 2, scenario 4

## What happens
- When the base has moved, Approve & merge always rebases the agent's branch first. A pre-rebase hook that forbids
  rebasing ("merge instead") refuses it every time; main, branch and worktree stay unchanged and the ticket stays in
  Review, but there is no way to merge. Only removing the hook helps.
- The restack skill tells the agent to `git rebase` too, so a conflict hits the same wall (not run; from the code).
- The toast says "The pre-rebase hook refused to rebase." and loses the hook's own reason (see 04).

## Evidence
Screenshots in `../shots-hooks/`: `s4-prerebase-refuses-merge-toast.jpg`, `s4-prerebase-toast-zoom-hook-reason-lost.jpg`

## Where to start
`git/merge.rs` (free rebase), `git/checkpoint.rs` (`rebase --no-autostash`), `plugin/skills/restack/SKILL.md`.

## Open questions for the product spec
- A merge path that doesn't rebase: merge the base into the branch, or squash from the merge-base. Per project
  setting, automatic fallback when a rebase is refused, or a choice in the failure dialog?
- What does the restack skill do when rebasing is forbidden?
