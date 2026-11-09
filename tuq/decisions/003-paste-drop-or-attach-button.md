# 003 · Attachments are added by paste, drag and drop, or an Attach button

- **Date:** 2026-11-07
- **Status:** Accepted
- **Related:** 001, 002

## Summary

While writing a comment, the user adds attachments in any of three ways: pasting (⌘V), dropping files onto the comment
box, or an Attach button that opens the system file picker. Each added file becomes its own chip in the box.

## Decision

- **Paste (⌘V):** an image on the clipboard (a screenshot) becomes an image attachment; files copied in Finder become
  attachments that keep their own names (today a pasted file turns into "Pasted text"; it no longer does). Text over
  the box's limit still becomes a "Pasted text" chip, as today.
- **Drag and drop:** files dropped onto the comment box are added.
- **Attach button:** a button by the comment box opens the system file picker; several files can be picked at once.
- All three work in every comment box: the Ticket tab's thread and the Send back, Fail and drag-back dialogs.
- Every added file shows as its own chip in the box (with a preview for images, by 002) and can be removed before
  sending.

## Why

Paste is the fastest way to get a screenshot in, drag and drop is how files usually move between apps on a desktop,
and the Attach button is the one way a user finds without being told. The user wanted all three.
