# 029 · Otaq always squashes and never rebases

- **Date:** 2026-11-11
- **Status:** Accepted
- **Related:** 026, 028

## Summary

Otaq never rebases a ticket's branch: it brings the branch up to date by merging main into it, and Approve & merge
lands it on main as one squashed commit, as today. There is no merge-method setting.

## Decision

- Wherever a branch is brought up to date (Approve & merge when main has moved, a reopened ticket's start, an agent
  restacking after a conflict or a failed Merge check), main is **merged into the branch**. Nothing is rebased, so a
  team's pre-rebase hook, or a team rule against rebasing, never stops Otaq.
- Approve & merge lands the ticket as **one squashed commit**, with the one message the user sees and can edit in the
  dialog (028, 031).
- No project setting chooses the method. Squashing makes no merge commit, so a pre-merge-commit hook doesn't run, as
  with a squash merge on GitHub. Teams that need merge commits or a pre-merge-commit gate deliver through a pull
  request.

## Why

The release test (`ideas/ux-rethink/build/release-test/issues-hooks/02`) found that a pre-rebase hook, or a team that
forbids rebasing, made every Approve & merge after main moved impossible, because Otaq always rebased first. Never
rebasing fixes that for every project with nothing to set. A first version of this decision (agreed earlier the same
day, never committed) added a Squash / Merge commit / Rebase setting read from the PR provider. On a step back the
user dropped it: it needed a note about the skipped pre-merge-commit hook, left the branch's own commit messages
uneditable when they land on main, and tied the setting to PR providers, which are becoming plugins. Also turned
down: a two-way Squash / Merge commit setting.
