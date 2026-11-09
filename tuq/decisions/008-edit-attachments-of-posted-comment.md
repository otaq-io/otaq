# 008 · Editing a posted comment can add and remove its attachments

- **Date:** 2026-11-07
- **Status:** Accepted
- **Related:** 001, 003, 007

## Summary

When the user edits a posted comment, they can add attachments to it and remove existing ones, not only change its
text. History keeps what was removed.

## Decision

- Editing a comment shows its attachments as chips that can be removed, and accepts new ones by paste, drag and drop
  or the Attach button (003).
- A removed attachment is gone from the comment and the thread, but **History** still shows that it was there, as it
  keeps a comment's old text.
- Saving an edit follows 007: the comment must keep text or at least one attachment.
- Agents editing their own comments have the same choice (006).

## Why

A wrong or outdated screenshot can be fixed in place instead of deleting the comment and posting it again. Turned
down: attachments fixed once posted (simpler, and an agent never sees a file disappear from a comment it already
read).
