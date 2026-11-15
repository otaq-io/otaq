# 098 · In a stage, Otaq's workflow skill decides how, the task skill decides what

- **Date:** 2026-11-14
- **Status:** Accepted
- **Related:** 081, 093, 094

## Summary

When the stage's two skills disagree, Otaq's workflow skill wins on how the work meets Otaq (asking, committing,
pushing, ending the stage) and the task skill wins on what the work is.

## Decision

- **Otaq's workflow skill decides:** how the user is asked (through the ticket, never only in the terminal), when and
  how work is committed, whether anything is pushed, how the stage ends and what it hands on.
- **The task skill decides:** what the stage produces and how it is worked out (the questions, the documents, the
  checks), and its own shape (one question at a time, draft then discuss).
- A task skill's "never commit", "ask with AskUserQuestion" or "suggest `/tuq:milestone-plan`" is followed the Otaq
  way: committed at the stage's end, asked through the ticket, handed to the next stage.
- A task skill's "push" or "open a PR" is not a request from the user: pushing follows Otaq's existing rule (only when
  the user asks, in the ticket, a comment, a message or their Agent instructions); Otaq never opens a PR outside its
  delivery.
- Applies to tuq's skills and the user's own task skills alike.

## Why

Task skills are written for a plain terminal (decisions 081, 094), so some of their steps contradict how Otaq works,
and without a rule an agent picks either one. Letting the task skill win was turned down: a stage could end
uncommitted or ask in a terminal nobody watches. Letting Otaq win on everything was turned down: it would override what
a user's own skill deliberately does.
