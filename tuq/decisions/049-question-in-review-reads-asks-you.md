# 049 · An agent's question shows while the ticket waits in your review

- **Date:** 2026-11-12
- **Status:** Accepted
- **Related:** 010, 025, 044, UX-62

## Summary

When an agent asks the user a question (`otaq ask human`) on a ticket that waits in one of the user's checks (Review,
Code review, Spec review), the ticket reads Asks you and the question shows on its Overview, while the ticket stays in
its column. Otaq never accepts a question and then shows it nowhere.

## Decision

- A question asked on a ticket waiting in a check of the user's reads Asks you on the card, the pill, the strip and in
  notifications, as it does in Dev. The ticket keeps its column and status; once the question is answered it reads as
  before ("Ready for you").
- The question's box shows on the ticket's Overview (drawn as UX-62 says) above the review's actions. Approve, Send
  back, Move and dragging stay available while it is open; sending the ticket back closes the question, as decision 025
  says.
- The answer goes to the agent that asked, as in Dev.
- The message writer's permission prompt keeps decision 010 (calm in Review, shown in the Approve dialog): this decision
  is about questions, not permission prompts.
- A question asked on a Done ticket is not covered here.

## Why

Re-test 8 (NEW-b2): a question asked on a ticket in Review was accepted and stored but shown nowhere, so the agent would
wait forever. The maintainer chose to show it rather than refuse it, so a Review skill can still ask the user.
