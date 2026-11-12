# 07 · Ctrl+C in a Codex terminal on Windows wipes the user's draft

**Kind:** design · **Release:** decide · **From:** piece 26's hand-back (round 4)

## What happens
- On Windows, Ctrl+C in the app's terminal goes to the program (the app's copy is Ctrl+Shift+C). Codex clears its
  input line on Ctrl+C, so a user pressing Ctrl+C to copy loses a draft, and Otaq then (rightly) types a waiting
  message into the empty line. Likely what happened in re-test 2's NEW-3.

## Evidence
Screenshots in `../shots-retest-5/`: none (piece 26 reproduced it in the VM with a capture tool, `%USERPROFILE%\otaq-build\p26`).

## Where to start
The terminal's key handling in the desktop UI (copy shortcut, selection).

## Open questions
- Ctrl+C copies when there's a selection (like Windows Terminal), warn, or leave as is?
