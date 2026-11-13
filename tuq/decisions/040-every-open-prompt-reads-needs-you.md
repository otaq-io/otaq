# 040 · Every prompt an agent shows reads Needs you, one at a time

- **Date:** 2026-11-11
- **Status:** Accepted
- **Related:** 016

## Summary

When an agent's tool asks permission for several commands at once and shows them one after another (Codex), each
prompt reads Needs you while it is on screen, and the card, pill and strip name the command actually shown.

## Decision

- Otaq keeps every open permission prompt of a session, not only the latest.
- The one on screen is the one Needs you names; after it is answered, the next one shown reads Needs you at once,
  until none is left.
- The card never names a command that isn't on screen yet.
- Applies to every agent session kind: a ticket's agent, a checker, a helper, Noyan.

## Why

Re-test 5 (round-5 issue 02): a checker's second prompt, queued right behind the first, stayed "Checking" for 30 s+
with no Needs you, and the card once named the queued command while the first was shown. The maintainer chose to fix
it in round 5 rather than a later milestone.
