# 119 · A ticket keeps its slot between two Agent stages

- **Date:** 2026-11-25
- **Ticket:** OTAQ-8
- **Status:** Accepted
- **Supersedes:** 114

## Summary

With the agent limit on, a ticket that finishes one Agent stage and goes on to another Agent stage keeps its slot
through the hand-over and starts the next stage without waiting for a slot.

## Decision

- **What holds a slot** is 114's list plus one: a ticket that finished an Agent stage and waits before the next Agent
  stage (for example Tech spec → Milestone plan). It counts as holding its slot while it waits there.
- **Forward or back:** the same holds when an Agent stage (a work stage or an agent check such as Validate) sends the
  ticket back to an earlier Agent stage, for example Validate failing back to Dev.
- **It never waits for a slot** there: it never shows "Waiting for a slot" between two Agent stages, and no ticket
  waiting to begin can take its slot, whatever that ticket's age.
- **Neither limit holds it back:** not "Max agents at once" and not "Hold new starts when Needs you reaches". Both
  hold back only starts: a ticket's first, and its next Agent stage after one of your checks (120).
- **Max lowered meanwhile:** the ticket still goes on, so the count can briefly read above Max (as it already can when
  Max is lowered while agents run); new tickets wait until it is back under.
- **The Running popover** lists it with the other slot holders while it waits between the two stages, so the count and
  the list still agree (114).
- The rest of 114 is unchanged: the Running popover and Settings → Limits' wording.

## Why

OTAQ-8: with Max agents 3, a ticket moving from Tech spec to Milestone plan lost the slot it had just freed to an older
ticket waiting to begin, and sat as "Waiting for a slot" mid-flow. The limits are meant to hold back starts, never stop
running work, and to the user a hand-over between two stages is not a new start. Turned down: putting it ahead of the
queue as an ordinary start, which would still let back-pressure or a lowered Max stop a ticket in the middle of its
flow.
