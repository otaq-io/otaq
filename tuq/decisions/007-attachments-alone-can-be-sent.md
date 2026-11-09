# 007 · A comment can be sent with attachments and no text

- **Date:** 2026-11-07
- **Status:** Accepted
- **Related:** 001, 003

## Summary

A comment needs either a line of text or at least one attachment; a pasted screenshot can be sent on its own. This
replaces today's rule that a comment with only a "Pasted text" chip can't be sent.

## Decision

- The send button is on when the comment has **text, or at least one attachment, or both**.
- This holds for every attachment kind, "Pasted text" included: the old hint asking for a line is gone.
- It holds in every comment box (the thread and the Send back, Fail and drag-back dialogs) and for agents' comments.
- An empty comment (no text, no attachments) still can't be sent.

## Why

Sending a quick screenshot should take one paste and one send. Turned down: always requiring a line of text (the agent
always gets a sentence on what to look at, but it's an extra step every time, and the attachment usually says enough).
