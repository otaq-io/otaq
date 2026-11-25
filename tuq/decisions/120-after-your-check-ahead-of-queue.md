# 120 · After your check, a ticket goes ahead of the queue

- **Date:** 2026-11-25
- **Ticket:** OTAQ-8
- **Status:** Accepted
- **Related:** 114, 119

## Summary

With the agent limit on, a ticket waiting for one of the user's checks between two Agent stages holds no slot; once
the user passes it on, it starts its next Agent stage before every ticket waiting to begin, as soon as a slot is free.

## Decision

- **While it waits for your check** (Spec review between Milestone plan and Dev, or any human check before a later
  Agent stage) the ticket holds no slot, as today (114): other tickets can use it.
- **Once you pass it on** to the next Agent stage, it goes ahead of every ticket waiting to begin, pinned and
  higher-priority ones too. Several such tickets keep the usual start order among themselves (pinned → priority →
  ready the longest).
- **Forward or back:** the same holds when you send the ticket back from Review or one of your checks to an earlier
  Agent stage, or answer "Send back" to an agent asking for one.
- **It is still a start:** it waits for a free slot, and "Hold new starts when Needs you reaches" holds it back, as for
  any start. While it waits it shows "Waiting for a slot", and it takes the first slot that frees.

## Why

OTAQ-8: a ticket the user has already moved along its flow should not be overtaken by work that hasn't begun.
Turned down: keeping the slot through the check, which leaves a slot unused for as long as the user takes to review;
and an ordinary start by pin, priority and age, which would leave the overtaking OTAQ-8 reported in place after every
check.
