# 018 · No "unsent text" warning while the agent's tool shows its own menu

- **Date:** 2026-11-10
- **Status:** Accepted

## Summary

The "unsent text" warning under an agent's terminal doesn't show while the tool shows a menu of its own, such as a
command approval. It comes back once the menu is gone.

## Decision

- While the agent's tool shows its own prompt or menu (a command approval, for example), the strip doesn't warn
  "unsent text": the keys the user types there go to the menu, not the input line.
- Once the menu is gone, the warning works as usual again.
- Faded suggestion or placeholder text in the input line doesn't count as unsent text.

## Why

A warning on the tool's menu would be wrong and would confuse the user while they answer it. Piece 6 of the release
fixes made this change though nobody asked for it (issue 19), and the user chose to keep it rather than always warn.
