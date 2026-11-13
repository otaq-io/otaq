# 021 · A change Noyan makes in a later turn is a CHANGED row

- **Date:** 2026-11-11
- **Status:** Accepted

## Summary

A ticket shows under CREATED only in the turn Noyan created it. When Noyan changes that ticket in a later turn, the
change gets its own CHANGED row, with Undo, like a change to any other ticket.

## Decision

- In the turn that creates a ticket, what Noyan gives it (waits included) shows under CREATED, as decision 013 and
  release-issues piece 15 built.
- In any later turn, a change to that ticket is listed under CHANGED, saying what changed, with Undo (piece 14).
- The CREATED row of the earlier turn keeps what it said (its title and what it was given then).

## Why

The Windows re-test of release-issues (NEW-4) saw a later turn's "waits for SHOP-21" folded into the earlier CREATED
row, which lost its title, with no CHANGED row and no Undo: the user couldn't see the change or take it back.
