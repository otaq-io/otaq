# 006 · Agents attach files under the same rules as the user

- **Date:** 2026-11-07
- **Status:** Accepted
- **Related:** 001, 002, 004, 005

## Summary

An agent's comment can carry several attachments of any type, like the user's, and its images show as thumbnails in
the thread the same way.

## Decision

- Agents (stage agents and epic leads) can attach **several files of any type** to one comment, no longer one report.
- The same limits apply: 20 MB per file, no count limit (005); a file over the limit is refused with a message naming
  it, and nothing is posted.
- An agent's images show as **thumbnails, click to enlarge** (004); its other files as chips (002).
- Agents reading the ticket are told about every attachment on every comment, the user's and other agents', whatever
  its type.

## Why

A check or review stage can then show the user what it saw (UI screenshots of a failing page) right in the thread.
One rule for people and agents keeps the thread and the agents' instructions simple. Turned down: agents keep a single
text report (their screenshots would show only as file paths).
