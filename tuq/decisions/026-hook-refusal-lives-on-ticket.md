# 026 · A hook's refusal lives on the ticket

- **Date:** 2026-11-11
- **Status:** Accepted
- **Related:** 010

## Summary

When a project's git hook refuses one of Otaq's own git steps (Approve & merge, a ticket's start), the ticket shows
it: a Needs you flag on the card, and on the sheet which hook refused, its full output and the way on. History keeps
it. It no longer lives only in a toast.

## Decision

- Applies to every git step Otaq runs itself (Approve & merge, a start, the update from main (029) of a reopened
  ticket), in a repository whose hooks refuse it. An agent's own commits and pushes are not covered here: the agent
  reads the hook's output in its session.
- The card gets a **Needs you** flag naming the hook and the step, for example "pre-commit hook refused the merge".
- The ticket's sheet shows which hook refused, the hook's **full output** (not only git's last line), and the way on
  for that step (for example edit the message and retry).
- Paths in the output are shortened (`~/…`, `~\…`), so the user's name isn't shown.
- The refusal is written to the ticket's **History** with its output, so it stays after it is dealt with.
- It works the same whether the Merging dialog is still open or was closed: the ticket is the one place it shows. A
  toast may still announce it, but only points to the ticket.
- The ticket stays in the stage it was in (Review for Approve & merge and Approve & open PR); a refusal isn't the
  agent's work going wrong.

## Why

Since v0.2 Otaq's git steps run in the background (the Merging dialog can be closed, the card says "Merging…" or
"Starting…"), so the user is often not looking at a dialog when a hook refuses. The release test on Windows
(`ideas/ux-rethink/build/release-test/issues-hooks/04`) found the refusal only in a toast that kept git's last line,
lost the hook's reason, showed the user's full path and left nothing on the ticket. Turned down: also switching an
open Merging dialog to the refusal (two places to build and keep in step), and a better toast with a History line (no
Needs you, easy to miss).

## Changelog

- 2026-11-11: refined: a refusal keeps the ticket where it was (Review), not moved to the Agent stage.
