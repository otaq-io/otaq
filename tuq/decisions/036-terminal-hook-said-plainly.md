# 036 · A hook that asks in the terminal is named as such

- **Date:** 2026-11-11
- **Status:** Accepted
- **Related:** 026

## Summary

When a hook fails because it tries to ask a question in the terminal, the refusal on the ticket says so in plain words
and what the user can do. Otaq doesn't try to answer such hooks.

## Decision

- When a hook fails because it needs a terminal (for example "commit anyway? [y/N]", reading from `/dev/tty`), the
  refusal on the ticket (026) says, above the hook's output: "Your pre-commit hook asks a question in the terminal;
  Otaq can't answer it. Make the hook skip its question when there's no terminal, or do this step by hand." (with
  the real hook name).
- Otaq doesn't give such hooks a terminal or relay their question: they can't pass in Otaq.
- Agents that hit the same failure in their sessions say the same to the user when they ask how to go on.

## Why

The release test (`ideas/ux-rethink/build/release-test/issues-hooks/09`) found that such hooks failed at once with
"/dev/tty: No such device or address" and nobody was told it was a hook needing a terminal. Turned down: giving the
hook a terminal and letting the user type its answer on the ticket (a big change for a rare case).
