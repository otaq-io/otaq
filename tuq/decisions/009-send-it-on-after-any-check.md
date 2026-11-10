# 009 · A check that runs out of rounds can always be sent on

- **Date:** 2026-11-10
- **Status:** Accepted

## Summary

When a check fails as many times in a row as its rounds allow, Decide always offers "Send it on to <next stage>", on
every check, skippable or not. This replaces the part of one-check-agent O5 that allowed moving on only on checks the
flow marks skippable.

## Decision

- Decide after the last failed round offers Allow 1 more round, Send it on to <next stage> and Stop here on every
  check stage (an agent check), as UX-60 and the canvas draw it.
- Send it on moves the ticket to the next stage; the checker's last report goes with it, so Review shows what failed.
- A stage's `skip` option keeps its one meaning: a ticket may skip the stage before it starts (UX-31). It no longer
  decides whether a failed check can be passed on.
- Unchanged from O5: "Skip this check" for a check waiting for its tool's limit still shows only on skippable checks.

## Why

The release test (2026-11-10) found the app offering Send it on and then refusing it with "Validate can't be skipped":
two rules disagreed. The user chose one plain rule: after its rounds run out, the user can always pass a ticket on and
judge it at Review. Turned down: Send it on only on skippable checks (O5), which ties two different things to one flag
and leaves a required check with only "allow more" or "stop".
