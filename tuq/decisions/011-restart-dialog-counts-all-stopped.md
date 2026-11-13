# 011 · The restart dialog's title counts every stopped agent

- **Date:** 2026-11-10
- **Status:** Accepted

## Summary

After a restart, the dialog's title counts every agent the restart stopped, including those that can't resume. The
Resume button counts only the ticked ones.

## Decision

- The title counts all stopped agents ("3 agents were stopped by a restart"), whether or not they can resume.
- The button counts the ticked agents only ("Resume 2"); it changes as the user ticks or unticks rows.
- A row that can't resume (its tool is signed out, for example) stays in the list, unticked, with its reason, and is
  counted in the title.
- So the two numbers can differ. That is expected.

## Why

The title says what happened and the button says what will happen. Piece 4 of the release fixes counted only the
agents that could resume in the title, which put a signed-out agent's row under a title that left it out. Triage
item 34 (assumption A49) asked for all stopped agents in the title, and the user chose that.
