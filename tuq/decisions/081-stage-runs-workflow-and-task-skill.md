# 081 · A stage runs Otaq's workflow skill and a task skill

- **Date:** 2026-11-14
- **Status:** Accepted
- **Related:** 057, 070, 080

## Summary

A stage's agent gets two skills: Otaq's workflow skill, which says how Otaq works, and the stage's task skill, which
says what to do in the stage. A task skill never depends on how Otaq works, so a change to tuq, or to the user's own
skill, can never break Otaq.

## Decision

- **Workflow skill** (how Otaq works): getting the ticket's context, asking the user, committing, ending the stage.
  Always from Otaq's own bundle, matching the app (decision 080).
- **Task skill** (what to do): the stage's `skill:`, such as `tuq:product-spec`, `tuq:security-check` or the user's
  `team-validate`. It runs from the user's install when they have one (decision 080).
- A task skill calls no `otaq` command and never assumes it runs inside Otaq; the same skill works in a plain terminal
  and in an Otaq stage.
- tuq's `product-spec`, `tech-spec` and `milestone-plan` lose their Otaq parts ("Which mode", `otaq prime`,
  `otaq ask human`, `otaq stage next`, committing); those live in the workflow skill.
- Because a task skill doesn't touch Otaq, a user's tuq of any version runs; there is no version check between a
  user's tuq and their Otaq.

## Why

Today a stage runs one skill that mixes both halves (tuq's Product spec calls `otaq` 20 times), so a tuq change could
break how Otaq works, and a user's older tuq could call an `otaq` command the way an older app did. Splitting the two
removes that, and lets people use the same tuq in their own projects.
