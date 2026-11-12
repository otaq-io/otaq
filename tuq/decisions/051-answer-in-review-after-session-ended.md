# 051 · An answer in your review when the agent's session has ended

- **Date:** 2026-11-12
- **Status:** Accepted
- **Related:** 049, 025

## Summary

When the user answers a question on a ticket waiting in one of their checks and the agent's session has ended, the
answer isn't sent and the question stays open; Otaq says so plainly and points to Send back with the answer as the
note. Nothing about the ticket changes. Resuming the session in place is deferred to a planned feature.

## Decision

- Send answer on a question asked in a check of the user's (Review, Code review, Spec review) whose session has ended is
  refused as today: the question stays open and the ticket keeps its status, column, Approve and Send back.
- The refusal's message says the agent's session has ended and that the user can send the ticket back with their
  answer as the note. Send back closes the question (decision 025).
- No resume in place, no new states. Known gap: a question asked by a writer (a Delivery session) is answered this way
  through the Dev agent, which didn't ask it.

## Deferred

Resuming the asking session in place with the answer (recording which session asked, the ticket reading at work in
its column, prompts during that turn, one answer at a time, a restart during it, failure handling) is a planned
feature with its own investigation and plan. A WIP is on branch worktree-agent-a538190ea1d74874d (aab83528), for
reference only; a review found it still hides the question when a resume fails.

## Why

Re-test 10 (NEW-r10-2): after the app restarted, Send answer on a Review ticket's question failed with "session has
ended". A first attempt to resume the session from Review was found to move the card back to Dev and re-run Validate,
and a careful version touches ~20 files with open risks. The maintainer chose the message-only change for the release.
