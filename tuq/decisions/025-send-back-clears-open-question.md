# 025 · Sending a ticket back closes its open question

- **Date:** 2026-11-11
- **Status:** Accepted

## Summary

When a ticket is sent back to an earlier stage, by the user or by an agent's send-back request the user accepts, the
question its agent held for the user is closed: it leaves Needs you, the card and the sheet, and the earlier stage's
agent starts without it.

## Decision

- Any send back (a human check's Send back, a drag back to an earlier stage, an accepted agent send-back request)
  closes the ticket's open question for the user.
- A question is also closed when its ticket is closed, parked or delivered (a database trigger already did this; it
  stays).
- A helper's question stays its helper's business (helpers drop theirs when they end).

## Why

The maintainer, 2026-11-11: the question was asked by the stage the ticket left; it no longer applies once the
ticket goes back. Before this, a send back left it open (only answering, a newer question from the same agent, an agent's own
send-back question, or closing, parking or delivering closed it), so a stale question could hold the ticket at Asks you in the earlier stage.
