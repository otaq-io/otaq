# 114 · The agent limit shows what holds each slot

- **Date:** 2026-11-16
- **Status:** Accepted
- **Related:** 113

## Summary

An agent that waits for the user keeps its slot under the agent limit, as it always has; with the limit on, the Running
popover lists those agents too, so the count and the list always agree.

## Decision

- **What holds a slot is unchanged:** an agent at work, an agent waiting for the user (asks you, needs you, idle at a
  decide), any check stage's status, a start in progress, a delivery in progress. A ticket in Review, Parked, PR open or
  Interrupted holds none. (This rule lived in the lost `decisions/` d0016, d0042, d0108; it is recorded here.)
- **The Running popover, with the limit on,** lists the agents that wait for the user under the working ones, each as
  "<KEY> · waits for you · <what>" and opening its ticket. "Nothing running" shows only when neither list has a row.
- **Settings → Limits** says "Right now: N of M slots taken" (not "running"), and "Run at most" says an agent waiting
  for you keeps its slot until you answer or move the ticket on.
- With the limit off nothing changes: Running counts the agents at work only.

## Why

Found in the 0.3 Windows regression test (F-B-2, also seen in the 0.2 re-test 8): with "Run at most 1", a check that
ended with no result held the only slot while the chip read "1/1 agents" and the popover said "Nothing running", and a
new ticket waited with no visible reason. Turned down: freeing the slot of a check with no result or a failed check, a
scheduler change that would let answers and retries push past the limit.
