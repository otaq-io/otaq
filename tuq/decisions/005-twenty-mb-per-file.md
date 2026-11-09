# 005 · Each attachment can be up to 20 MB, with no limit on how many

- **Date:** 2026-11-07
- **Status:** Accepted
- **Related:** 001, 002

## Summary

Every attachment, of any type, can be at most 20 MB (today's report limit); a comment can carry any number of them.

## Decision

- **20 MB per file**, the same limit reports have today; exactly 20 MB is accepted.
- A file over the limit is refused **when it is added** to the box, with a message that names the file; the other
  files and the text stay in the box.
- **No limit on the number** of attachments per comment, and no total size limit.

## Why

It keeps the rule users and agents already know. Turned down: a larger per-file limit for screen recordings (the data
folder grows faster, and files stay until their ticket is deleted) and a cap on the count (one more rule and refusal
for a case that hasn't come up).
