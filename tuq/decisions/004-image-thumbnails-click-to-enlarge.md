# 004 · Posted images show as thumbnails that open full size

- **Date:** 2026-11-07
- **Status:** Accepted
- **Related:** 001, 002

## Summary

In the ticket's comment thread, a posted comment's images show as a row of small thumbnails under its text; clicking
one opens the image full size in the app.

## Decision

- Under a posted comment, its images appear as a **row of thumbnails**, after its text.
- **Clicking a thumbnail** opens that image full size inside the app.
- The comment's other attachments (text and other files) show as chips, as 002 describes.

## Why

The user needs to see what was sent without opening each file, while the thread stays compact when a comment carries
several screenshots. Turned down: full-width images inline (a few screenshots make one comment very tall) and chips
only with no preview after posting (you can't see what was sent).
