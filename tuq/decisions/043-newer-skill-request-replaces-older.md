# 043 · A newer "Ask the agent to write it" replaces one that never started

- **Date:** 2026-11-11
- **Status:** Accepted
- **Related:** 016, 024

## Summary

When the user asks an agent to write a stage's skill while an earlier such request to the same agent never started
writing (no skill folder named yet), the newer request replaces the older one instead of being refused.

## Decision

- An earlier request whose agent hasn't started writing (named no folder) is dropped when a newer one comes for a
  different stage; its stage stops showing "Being written…".
- An earlier request the agent already started writing is left alone: the newer one waits or is refused as today, so
  no half-written skill is abandoned silently.
- The agent may still only write the stage it was asked for (round-4 piece 24).

## Why

Round-5 issue 09: after round-4 piece 24, a dropped request blocked every new request for another stage until the
user stopped the old one. The maintainer chose to include the fix in round 5.
