# 046 · A question whose answer session ended unanswered goes to Needs you

- **Date:** 2026-11-11
- **Status:** Accepted
- **Related:** 045

## Summary

When an answer session ends without answering the question it was started for (it crashed, was stopped, or ended its
turn without answering), the question goes to Needs you on the asking ticket instead of waiting forever. Otaq doesn't
retry it by itself.

## Decision

- The question reads Needs you on the asking ticket, offering to answer it yourself or to resume the answering ticket.
- Otaq doesn't start the answer session again by itself.
- The answer session's worktree is removed if it is clean, else kept and listed in Cleanup; a worktree a live ticket
  uses again is never removed.
- An answer session stopped because its project was paused isn't this case: it carries on when the project resumes.
- The same applies after Otaq restarts and finds a question whose answer session is no longer running.

## Why

Re-test 6 (NEW-4): after SHOP-35's answer session was ended, SHOP-36's question stayed "waiting" for good and was never
offered again. A retry could loop, and a session that died at a prompt would most likely stop at the same prompt again.
