# 010 · The merge message writer's permission ask shows in the Approve dialog

- **Date:** 2026-11-10
- **Status:** Accepted

## Summary

When the agent writing a ticket's merge message (or PR title and description) stops at a permission prompt, the user
sees it only when they approve: the Approve & merge (or open PR) dialog says the agent is waiting for permission, with
Answer in terminal or Merge now with the plain message. The ticket in Review stays as it is, with no extra Needs you.

## Decision

- While the message writer waits for permission, the ticket in Review looks as usual ("Ready for you", magenta edge);
  it gets no turquoise edge, Needs you item or notification for it.
- Approve & merge and Approve & open PR open their dialog at once, as today. When the writer is waiting for
  permission, the dialog says so in plain words ("Developer wants to run a command to write the message") and offers
  Answer in terminal (it opens the writer's terminal) beside the plain message, which the user can merge or push now,
  edit, or wait for: the written message fills in when ready, unless the user has typed (UX-56).
- The writer works only while the ticket waits in Review: once the ticket is delivered, closed or sent back, its
  message session stops and leaves nothing running or behind.
- The calm holds only while the ticket waits in Review. If the ticket leaves Review while the writer's prompt is still
  open in a session that keeps running (the ticket's own agent session, e.g. after Approve & merge hits a conflict and
  the ticket goes back to Dev to be fixed), that prompt reads Needs you like any other open prompt, naming its
  command; Otaq types nothing into the session to end the writer's turn.
- Holds for every tool; it matters most for tools that ask before every command (Codex, Copilot).

## Why

The release test (2026-11-10, Windows, Codex) never got a written message: the writer stopped at a prompt shown
nowhere. The user chose to meet it at merge time, the only moment the message matters, and keep Review calm. Turned
down: making Review ask like any agent (an extra stop on every review on Codex/Copilot), and never asking (Codex and
Copilot users would never get a written message).

## Changelog

- 2026-11-11: confirmed for the message writer only; a delivery skill turn's prompt (Push to PR, Open PR, automatic PR) reads Needs you, see 044.
- 2026-11-12: refined: the calm holds only while the ticket is in Review; a writer prompt still open after the ticket leaves Review (a merge conflict sends it to Dev) reads Needs you. Re-test 7 NEW-4.
