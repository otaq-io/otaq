# 048 · On Windows, Ctrl+V in a terminal pastes text

- **Date:** 2026-11-12
- **Status:** Accepted
- **Related:** 042

## Summary

In Otaq's terminals on Windows, Ctrl+V pastes the clipboard's text as the terminal's own paste (as Ctrl+Shift+V does),
the way Windows Terminal does. With no text on the clipboard, Ctrl+V goes to the program as today, so a tool's image
paste keeps working.

## Decision

- On Windows, Ctrl+V with text on the clipboard is the terminal's paste: the text reaches the program as a paste, and
  Otaq knows what is in the input line.
- With no text on the clipboard (an image, a screenshot, nothing), Ctrl+V goes to the program as a key, as today
  (Claude Code and Codex paste an image).
- Ctrl+Shift+V keeps pasting. macOS (⌘V) and Linux (Ctrl+Shift+V; Ctrl+V goes to the program) are unchanged.
- A program that wants a raw Ctrl+V while text is on the clipboard doesn't get it on Windows; Windows Terminal makes
  the same trade.

## Why

Re-test 7 (NEW-10): Ctrl+V pasted nothing into Claude Code on Windows, which takes the raw key as its image paste.
Codex reads the clipboard itself on the raw key, so its text appeared but Otaq saw one keystroke, not the text (round
5's restack typed onto pasted text). Windows users expect Ctrl+V to paste, and decision 042 already gave Ctrl+C its
Windows meaning. The maintainer chose this over leaving it (Ctrl+Shift+V only) and over pasting always (no image
paste).
