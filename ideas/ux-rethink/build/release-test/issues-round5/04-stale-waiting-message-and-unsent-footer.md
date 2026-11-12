# 04 · A "1 message waiting" that never goes, and "Unsent text" over an empty line

**Kind:** bug · **Release:** must fix · **From:** re-test 5 NEW-4

## What happens
- After a send back, SHOP-32's Dev session header read "1 message waiting · Deliver now" until the merge. The waiting
  row in `pending_deliveries` belonged to the **ended** round-1 checker session ("The user added comment 2. Run `otaq
  prime`…").
- The footer said "Unsent text in the input line · Otaq's note waits until it's empty" while Codex's line was empty
  (maybe from a stray "y" after a prompt had closed).

## Evidence
Screenshots in `../shots-retest-5/`: `new-unsent-text-claimed-line-empty.jpg`

## Where to start
crates/otaq-core/src/session/deliveries.rs (pending rows of ended sessions: drop or move them), the waiting count in
the session header's view model, the unsent-input flag after a key a closed prompt took.

## Open questions
- A message queued for a session that ended: dropped, or moved to the ticket's live session?
