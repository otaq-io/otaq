# 050 · The Open PR "Pushing…" dialog can always be cancelled

- **Date:** 2026-11-12
- **Status:** Accepted
- **Related:** 010, 044, 049

## Summary

The Open PR "Pushing…" dialog's Cancel always works, so a delivery turn that never starts (or never reports back) can't
lock the window. Cancel only stops waiting: it never stops the agent's session.

## Decision

- In the "Pushing…" dialog, Cancel (and Esc) is always enabled. Cancel closes the dialog and ends the pull request
  flow, so the ticket stays where it was (its review) and no PR is recorded by the cancel itself. The agent's session is
  never stopped by it; a toast says a push may still be running.
- Ending the flow on Cancel is on purpose: a flow kept after Cancel would send the agent's later questions to the closed
  dialog (shown nowhere, against decision 049), hold an agent slot, and could move a ticket sent back to Dev into PR
  open. Accepted cost, for the rest of that one turn: a permission prompt isn't labelled as the flow's (decision 044),
  and `otaq prime` no longer shows the confirmed pull request.
- If the agent reports the PR afterwards (`otaq pr opened`), Otaq records it as it does today: never refused, so a PR
  that really exists isn't lost.
- An automatic Delivery (no dialog) still can't be cancelled this way.
- No automatic timeout: a push can legitimately take long (hooks, a slow remote); Cancel is the way out.
- The missing-skill hint also says that a ticket already started needs the skill on its own branch (committing it to
  the project doesn't reach an existing worktree).

## Not decided here (deferred)

Changing which project skills Otaq counts for Claude Code in a worktree (Claude Code loads the main checkout's
`.claude/skills` only when the worktree has no `.claude/skills` folder at all; Otaq falls back per skill name) is left
for a planned change: it didn't cause NEW-r10-1.

## Why

Re-test 10 (NEW-r10-1): Claude Code answered "Unknown command" to the delivery skill and the "Pushing…" dialog hung over
the whole window with Cancel disabled until the app restarted. The cause was timing in the test data (the project's
skills folder was created after the ticket's session started, and Claude Code doesn't pick up a folder created
mid-session), not Otaq's skill check. Two reviews showed that changing the skill check to "worktree only" would break
documented Claude Code setups and that keeping the flow after Cancel causes new bugs; the maintainer chose this narrow
scope for the release.
