# 013 · Noyan's CHANGED row says what changed and has Undo

- **Date:** 2026-11-10
- **Status:** Accepted

## Summary

In Noyan's panel, each row under CHANGED says what Noyan changed on the ticket ("SHOP-7 · now waits for SHOP-6") and
has an Undo that puts it back. This goes into the release.

## Decision

- A CHANGED row names the ticket and says in plain words what changed, for example "SHOP-7 · now waits for SHOP-6",
  as the canvas draws it (noyan-01 · 4). Naming only the ticket and its column ("✎ SHOP-16 Retest dep B · Backlog") is
  not enough.
- Each row has **Undo**, which takes back that change on the ticket.
- A ticket Noyan created and then changed in the same turn shows once, under CREATED only, with what it was given
  ("SHOP-16 · waits for SHOP-15"). It gets no CHANGED row, so there is no Undo for a change to a ticket Noyan just
  made.
- Undo shows only while that part of the ticket is still as Noyan left it. Once the user or an agent has changed it
  again (removed or changed the wait Noyan added, for example), the row keeps its text but loses Undo, so Undo never
  overwrites someone else's later edit. A ticket that only moved to another column keeps its Undo.

## Why

The user has to see what Noyan did to their tickets without opening each one, and to take back a change they don't
want in one click. The release test (issue 14) found only the ticket's name built; the user chose to build both the
what-changed text and Undo for the release, though it is the most work of the options (it needs a protocol change),
over shipping the text alone or leaving it for later.
