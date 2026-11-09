# 001 · A comment can carry several attachments, images among them

- **Date:** 2026-11-07
- **Status:** Accepted
- **Related:** 002, 006, 007

## Summary

While writing a ticket comment, the user can add several attachments to it: images (pasted or attached) and text
files or long pasted text, several of each. They are posted together with the comment's text.

## Decision

- One comment can hold **several attachments**, of more than one kind: images, text files, and long pasted text
  (today's "Pasted text" chip). There is no longer a one-report-per-comment rule.
- The attachments belong to the comment: they are posted with it. A comment may be only attachments, with no text (007).
- This applies to the person writing the comment in the app (the comment box in the Ticket tab's thread and in the Send
  back, Fail and drag-back dialogs) and to agents (006). Which file types are allowed is 002.

## Why

Image comments are mostly UI bug reports and review feedback, which usually need more than one screen (before and
after, several states), often next to a log or notes file. Turned down: one image per comment, using today's single
attachment slot (three screenshots would take three comments); several images plus at most one text report (an
arbitrary limit once several attachments are allowed at all); and images or a report but not both.
