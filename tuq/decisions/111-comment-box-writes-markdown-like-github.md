# 111 · The comment box writes Markdown, like GitHub's

- **Date:** 2026-11-15
- **Status:** Accepted
- **Related:** 003, 110, 112

## Summary

The comment box stays a text box holding Markdown, with GitHub's helpers around it: Write and Preview tabs, a small
toolbar that adds Markdown marks, shortcuts, lists that continue on Enter, and a link made by pasting a URL over
selected text. What the user writes is posted as written; there is no formatting as you type.

## Decision

- **Write / Preview:** two tabs over the box. Preview shows the comment exactly as the thread will (110); Write
  brings back the text with its caret where it was.
- **Toolbar** over the text: bold, italic, code, link, bulleted list, numbered list, task list, quote. Each wraps the
  selection (or the current line, for lists and quotes) in its Markdown marks: **B** on `fix` gives `**fix**`.
- **Shortcuts:** ⌘B bold, ⌘I italic, ⌘E code, ⌘K link (Ctrl on Windows and Linux).
- **Lists continue on Enter:** after `- item`, `1. item` or `- [ ] item`, Enter starts the next item; Enter on an empty
  item ends the list.
- **Paste a URL over selected text** to get `[text](url)`. Any other paste behaves as today (003, 112).
- **Everywhere a comment is written:** the Ticket tab's thread, editing a posted comment, and the Send back, Fail and
  drag-back dialogs, so there is one box to learn.
- Paste, drag and drop, Attach, attachments and ⌘↵ work in the box as they do today (003).
- The text is posted as written; Copy and Edit give back that same Markdown (110).

## Why

People who write Markdown comments already know GitHub's box, and it gives formatting help without changing what the box
holds. Because it stays a text box, paste, drop, attachments and ⌘↵ keep working as they are. Turned down: a plain
text box with no helpers (formatting is guesswork until the comment is posted) and the description's editor with
formatting as you type (moving paste, drop, attachments and ⌘↵ onto it is a large change, and the text posted would no
longer be exactly what was typed).
