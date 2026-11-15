# 110 · Comments are shown as Markdown

- **Date:** 2026-11-15
- **Status:** Accepted
- **Related:** 004, 111, 112

## Summary

Every comment, the user's and agents' alike, is shown as Markdown wherever the app shows it, drawn the way a
description is. A single line break stays a line break, so comments written as plain text look as they do today.

## Decision

- **Where:** the Ticket tab's comment thread, the Delete dialog's quoted comment, comments on a diff in the Files tab
  and the comments listed in the review dialogs.
- **What is drawn:** paragraphs, bold and italic, inline code, code blocks (highlighted, scrolling sideways rather than
  wrapping), bulleted, numbered and task lists (their boxes can't be ticked), quotes, tables (scrolling sideways) and
  links. A link opens the way a description's link does.
- **Line breaks:** a single line break stays a line break, as in a GitHub comment; a blank line starts a new paragraph.
  Agents' "one line per criterion" lists and every comment posted before this change keep their lines.
- **HTML is never interpreted:** anything that looks like a tag (`Observable<number | null>`, `<why>`, a pasted
  `<div>`) shows as the text that was written, in every comment.
- **Old comments** (posted before this change) are shown the same way as new ones: no cutoff, no plain-text mode.
- **Headings** are drawn small, about the size of bold text: a comment sits in a thread and isn't a document.
- **Images written in Markdown** (`![](…)`) aren't loaded; pictures stay attachments, shown as thumbnails (004).
- **Copy** copies the comment's Markdown as written, and **Edit** opens it as written.
- **One-line previews** of a comment (History's rows, notices) show its words without the Markdown marks: "Done", not
  "\*\*Done\*\*".
- **Agents** read comments in `prime` as before; a heading inside a comment is nested under the comment's own heading,
  so it can't read as one of `prime`'s sections.

## Why

Agents already write their comments in Markdown, and the thread shows the marks as raw characters. Showing comments the
way descriptions are shown makes them readable with no new habit to learn. Keeping single line breaks means no comment,
old or new, changes shape. Turned down: standard Markdown line breaks (they would join every line-per-item list into one
paragraph) and loading Markdown images (attachments already cover pictures).

Old comments were checked against this (302 in the maintainer's database, 2026-11-15): two thirds of agents' comments
already use Markdown and read better; none turns into a stray heading, table or code block; 42 hold `<…>` placeholders
outside code, which is why HTML shows as written; about 13 have glob stars (`before-*, after-*`) that may set a few
words in italics, which is accepted. Turned down: showing comments from before the change as plain text (a hidden
cutoff, a thread with two looks, and agents' Markdown left raw) and a per-comment "Show as plain text" switch (a
control for a rare, cosmetic case).
