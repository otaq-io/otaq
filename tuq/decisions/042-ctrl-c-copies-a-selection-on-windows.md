# 042 · On Windows, Ctrl+C in a terminal copies when text is selected

- **Date:** 2026-11-11
- **Status:** Accepted

## Summary

In Otaq's terminals on Windows, Ctrl+C copies the selected text when there is a selection, as Windows Terminal does;
with no selection it goes to the program as today.

## Decision

- With text selected in an agent's (or any Otaq) terminal on Windows, Ctrl+C copies it and clears the selection; the
  program gets nothing, so a draft in its input line stays.
- With nothing selected, Ctrl+C goes to the program (an interrupt, or whatever the tool does with it).
- Ctrl+Shift+C keeps copying. macOS and Linux keep their own keys.

## Why

Round-5 issue 07: Codex clears its input line on Ctrl+C, so a Windows user pressing Ctrl+C to copy lost their draft
(likely re-test 2's lost text). The maintainer chose the Windows Terminal behaviour over always copying (no keyboard
interrupt) or leaving it.
