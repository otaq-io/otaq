# 002 · Any kind of file can be attached to a comment

- **Date:** 2026-11-07
- **Status:** Accepted
- **Related:** 001

## Summary

A comment's attachments can be files of any type. Images and text get a preview in the app; any other file shows as
a plain chip with its name and size.

## Decision

- No file type is refused: images, text, PDFs, archives, videos and anything else can be attached.
- **Images** show a preview (a thumbnail) and **text** keeps today's open-and-read behaviour. **Any other file** is a
  plain chip with its name and size, with no preview in the app.
- Agents are told about every attachment of the comments they read, whatever its type; whether a given agent tool can
  open it (a video, say) is up to that tool.

## Why

The user chose not to guess which files are useful to an agent or a teammate: a PDF spec, a zipped repro or a short
screen recording can all matter. Turned down: images and text only (refusing everything else), and images, text and
PDF (a third preview kind for one extra type).
