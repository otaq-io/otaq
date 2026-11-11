# 035 · A failed start leaves nothing half-made; Retry start starts over

- **Date:** 2026-11-11
- **Status:** Accepted
- **Related:** 026, 027

## Summary

When a ticket's start fails, for example because a post-checkout hook refused, Otaq leaves no half-made worktree
behind, so Retry start is a fresh start and the start's hooks run again.

## Decision

- A start that fails before its worktree is ready (making the worktree, seeding it, env files, the ticket's
  resources, setup), a hook's refusal included, leaves no half-made worktree for the ticket. The refusal shows on the
  ticket (026). A start that fails only at launching the agent (or allotting its ports), when every start hook has
  already run, keeps its ready worktree, so Retry start just launches again in it.
- **Retry start** then does the whole start again, and every hook a start runs (post-checkout among them) runs again.
- This applies to a new ticket's first start. A reopened ticket's worktree holds the agent's earlier work and is never
  removed by a failed start.

## Why

The release test (`ideas/ux-rethink/build/release-test/issues-hooks/08`) found that a failed post-checkout left a
worktree behind, Retry start reused it, and the hook never ran again, so its install or code generation silently
never happened for that ticket. Turned down: keeping the half-made worktree and rerunning only the hook (a special
case to explain).

## Changelog

- 2026-11-11: refined: only a failure before the worktree is ready removes it; a launch failure keeps the ready worktree (every start hook had already run).
