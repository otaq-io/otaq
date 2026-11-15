# 103 · A tuq that lacks a stage's skill is updated automatically

- **Date:** 2026-11-14
- **Status:** Accepted
- **Supersedes:** 083
- **Related:** 100, 102

## Summary

When a stage names a `tuq:` skill that the user's installed tuq doesn't have yet, Otaq updates their tuq the tool's
normal way and runs the stage. The stage waits in Needs you only if the skill is still missing after the update, or
the update fails.

## Decision

- Before a stage starts (and before Otaq types a `tuq:` skill into a running session), Otaq checks that the tool's tuq
  has the skill. If not, it updates tuq the way that tool updates an installed plugin or skill, then runs the stage.
- The ticket's History says tuq was updated, from which version to which.
- A change the user made inside tuq's installed files may be replaced by the update; that is accepted.
- Still missing after the update (no tuq release has the skill), or the update failed (offline, the tool refused):
  the stage waits in Needs you as decision 083 says, naming the skill: "Your tuq doesn't have `tuq:update-docs`".

## Why

Under decision 102 Otaq uses the user's tuq and installs it when missing; an install that is merely older is the same
case. Holding the stage until the user updates, with or without a button, was turned down as a stop for something Otaq
can do itself.
