# 045 · An answer session's prompt reads Needs you on the answering ticket

- **Date:** 2026-11-11
- **Status:** Accepted
- **Related:** 040, 041

## Summary

When a finished ticket's agent is started again to answer another ticket's question (an answer session) and stops at
a permission prompt, the answering ticket reads Needs you, while its card stays in Done. The asking ticket keeps
"asked … · waiting".

## Decision

- While an answer session runs, the answering ticket's card line reads "answering SHOP-36"; while it waits at a prompt,
  "Needs you · answering SHOP-36", with the Needs you highlight. The card stays in Done and keeps its status.
- The prompt gets its own Needs you item and notification, naming the command on screen; clicking it opens the
  answering ticket's session, where a key answers it. Focus in that terminal holds back only the alert (041).
- The answer session can be reached like any agent session: the ticket's Session tab and `otaq attach`.
- The asking ticket keeps "asked SHOP-35 · waiting"; it gets no Needs you for the other ticket's prompt.

## Why

Re-test 6 (NEW-4): SHOP-35's answer session stopped at a Codex prompt that showed nowhere and couldn't be reached;
SHOP-36's question stayed unanswered for 12 minutes. The prompt belongs to the answering ticket's agent, so the
maintainer chose to show it there, as decision 040 does for every other session kind.
