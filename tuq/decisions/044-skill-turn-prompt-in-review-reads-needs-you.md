# 044 · A skill turn's prompt reads Needs you while the ticket stays in Review

- **Date:** 2026-11-11
- **Status:** Accepted
- **Related:** 010, 040, 041

## Summary

When the agent running a delivery skill turn (Push to PR, Open PR, an automatic PR) stops at a permission prompt, the
ticket reads Needs you on the card, the strip and in notifications, while it stays in its column (Review or To
deliver). The message writer's prompt stays calm, as decision 010 says.

## Decision

- A permission prompt during a skill turn the user started (Push to PR, Retry push, Open PR after confirming) or one
  that runs by itself (an automatic PR) reads Needs you on the card, the pill, the strip and in notifications, naming
  the command on screen. The ticket keeps its column and status; when the prompt is answered it reads as before.
- The message writer's prompt keeps decision 010: Review looks as usual and the prompt shows in the Approve dialog.
- The PR dialog, while it says Preparing or Pushing, says the agent is waiting for permission and offers Answer in
  terminal.
- A session's own header reads "Waiting for you" whenever a prompt is on screen in it, whatever the ticket shows.
- Approve, Send back, Move and dragging stay available while such a prompt is open.
- Send back, dragging back and Push to PR over an open prompt (any prompt on the ticket's session, the writer's
  included) say a permission prompt is waiting, name the command, and offer only Keep it and cancel: clearing the line
  would send Esc into the tool and decline the prompt, so the user answers it in the terminal first.

## Why

Re-test 6 (NEW-2): a Codex prompt during HOOK-5's push turn showed nowhere, and the agent waited unseen until someone
opened its session. The maintainer kept 010 for the writer (met at merge time, Review stays calm) and chose Needs you
for the skill turns 010 doesn't cover, where nothing else brings the user to the prompt.

## Changelog

- 2026-11-12: refined: over an open prompt, Send back, drag back and Push to PR offer only Keep it and cancel (no Clear it and send), confirmed by the maintainer after release-round6 piece 2.
