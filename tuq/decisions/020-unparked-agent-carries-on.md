# 020 · A ticket dragged back from Parked carries on

- **Date:** 2026-11-11
- **Status:** Accepted

## Summary

When a parked ticket is dragged back and its agent wakes up idle at its prompt (as Codex does, showing
"Conversation interrupted"), Otaq tells it to carry on where it left off, so the agent really works again.

## Decision

- Dragging a ticket back from Parked means "continue this work": its agent resumes the same conversation.
- If the agent comes back idle at its prompt, Otaq sends it a carry-on message, the way the restart dialog's Resume
  does for a waiting row.
- The card says Working only while the agent actually works.

## Why

The Windows re-test of release-issues (piece 8) saw an unparked Codex ticket sit at an empty prompt while the card,
its pill and Session all said Working for over a minute. The user chose carrying on over showing Needs you.
