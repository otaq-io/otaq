# 027 · A running hook is shown, with Stop and no time limit

- **Date:** 2026-11-11
- **Status:** Accepted
- **Related:** 026, 038

## Summary

While one of Otaq's own git steps runs, the card and the Merging dialog show the real step and, while a hook runs,
which hook and for how long ("running your pre-commit hook · 40 s"), with a Stop. Otaq never stops a hook on its own.

## Decision

- Covers the git steps Otaq runs itself (Approve & merge, a start, a reopened ticket's update from main (029)).
- The card and, while it is open, the Merging dialog show the step git is really on, not the first step for the whole
  run.
- While a hook runs, they say which one and how long it has run: "running your pre-commit hook · 40 s".
- **Stop** is offered while the step runs, on the ticket and in the dialog (not for the post-merge hook, which runs after
  the merge landed: 038). A stopped step leaves main, the branch and the worktree as they were, the same as a refusal, and
  the ticket stays where it was.
- **No time limit**: Otaq never stops a slow hook itself, so a long one (an install, a full test run) is never cut
  off. A hook that hangs runs until the user presses Stop.

## Why

The release test (`ideas/ux-rethink/build/release-test/issues-hooks/07`) saw the Merging dialog stay on "Fetch main"
for 46 s and 82 s while slow hooks ran on later steps, and a hanging hook would keep "Merging…" up forever with no
way out. Showing the step and the hook explains the wait; Stop gives the way out. Turned down: a fixed time limit (it
can cut off a legitimate slow hook) and a time-limit setting (one more setting to explain).
