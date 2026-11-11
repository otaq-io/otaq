# 016 · "Drafting a flow" gives way to Needs you while the agent waits

- **Date:** 2026-11-10
- **Status:** Accepted

## Summary

While an agent drafting a flow is stopped at a permission prompt or a question for the user, Needs you takes the
place of the "drafting a flow" line. The line comes back once the user answers.

## Decision

- While the flow-drafting agent works, the "drafting a flow" line shows, as today.
- When it stops at a permission prompt or asks the user a question, the line is replaced by Needs you.
- Once the user answers and the agent works again, "drafting a flow" comes back.

## Why

A waiting agent shouldn't look busy, or the user may not notice it needs an answer. Piece 11 of the release fixes
kept the line during prompts and questions (issue 19), and the user chose to change that.
