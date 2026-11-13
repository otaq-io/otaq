# 04 · A question whose answer session ended unanswered waits forever

**Kind:** bug · **Release:** must fix · **From:** re-test 6 NEW-4 · **Decision:** 046

## What happens
After SHOP-35's answer session was ended, SHOP-36 kept "asked SHOP-35 · waiting" for good, and SHOP-35's re-made
worktree was left behind. The same after an Otaq restart or when the session ends its turn without `otaq answer`
(T7 audit G4, ideas/noyan-intake/T7-audit.md).

## Cause (code, high confidence)
`end_answer_session` (crates/otaq-core/src/messages/answering.rs) runs only from `answer()` (messages/mod.rs). Nothing
reacts to an answer session ending by itself: the recovery loop (recovery/mod.rs) handles agent, validator and message
writer ends only, and no startup sweep clears `delivery='resumed'`. `route_waiting` selects only `delivery IS NULL`, so
the question is never routed again.

## Fix direction (decision 046)
`answer_session_ended` from the recovery loop beside `message_writer_ended`: an unanswered question goes to Needs you
(`Route::NeedsYou`: answer it yourself / Resume SHOP-35), not back to `delivery NULL` (that would restart it and could
loop); the worktree is removed if clean, else kept for Cleanup (shared with `end_answer_session`). A startup sweep for
`'resumed'` questions with no running answer session, after sessions are reconciled. A stop by pause (stop_reason) is
not this case (agents/kept.rs).

## Risks to check
Never remove a worktree a live ticket uses: guard removal (and the existing `end_answer_session`) with "the ticket is
still Done and no Agent session runs". The startup sweep must not race a resumed session.

## Evidence
`../RESULTS-retest-6.md` (gap 3, NEW-4, setup: SHOP-35's worktree left).
