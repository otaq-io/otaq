# 113 · A check stage runs Otaq's check workflow skill, then its task skill

- **Date:** 2026-11-16
- **Status:** Accepted
- **Refines:** 081
- **Related:** 089, 098

## Summary

A check stage whose skill isn't Otaq's own `otaq:validate` gets two skills, as a work stage does: Otaq's new workflow
skill for checkers, `otaq:check-ticket`, says how a check meets Otaq, and the stage's task skill (the user's
`team-validate`, `tuq:security-check`) says what to check. The checker always ends with a verdict Otaq reads.

## Decision

- **`otaq:check-ticket`** is a new workflow skill in Otaq's bundle (the otaq-skills repository), built like
  `otaq:work-ticket` but for a checker. It covers: getting the context with `otaq prime --validate`, staying a
  read-only judge (no edits, no commits), and ending with `otaq check pass|fail` and a short verdict. A checker
  never asks the user mid-check: an open point goes into a fail verdict or a send-back, as for Otaq's own Validate.
- **The start:** Otaq starts the checker with `/otaq:check-ticket`; when that turn ends, it types the stage's task
  skill, as it does for work stages (081, hand-off). The task skill's own checks are the "what".
- **Otaq's own Validate stage** (`otaq:validate`) is unchanged: it already holds both halves and starts alone.
- **When the two disagree** (a task skill that fixes what it finds, or never reports), the workflow skill wins on how
  the check meets Otaq (098): the checker stays read-only and always reports a verdict.
- Each tool types the skill in its own form (`/otaq:check-ticket`, `$otaq:check-ticket`, `otaq-check-ticket` on
  Copilot and OpenCode, 092).

## Why

Found in the 0.3 Windows regression test (F-U2-2, also on 0.2.0): a check stage with the user's own skill ran only that
skill, which never called `otaq check`, so every such check ended "gave no result · decide". 081 already gave every
stage the two halves, with a check skill as its example, but the start code applied it to work stages only. Turned
down: a one-line message on the checker's start telling it to report (smaller, but checkers would differ from every
other stage).

## Changelog

- 2026-11-16: refined before building: a checker doesn't ask the user (checkers can only give a verdict or send back);
  confirmed for 0.3 after the plan review showed it needs checker turn tracking and the task-skill gate on verdicts.
