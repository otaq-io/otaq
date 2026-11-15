# 078 · Sessions keep their skills across app updates; Codex switches when idle

- **Date:** 2026-11-14
- **Status:** Superseded by 101
- **Related:** 061, 076

## Summary

A session keeps the skills it started with even when the app updates in between, the built-in skills included.
Sessions from before the tuq change resume on the newest bundle. If Codex can't give each session its own copy,
Codex's shared copy switches to a new bundle only when no Codex session is in the middle of a stage.

## Decision

- The built-in skills of an app version are kept, like any bundle, while a session started on them can still resume
  (decision 076).
- A session started before this change (with the old `otaq:` skill names) has no bundle; it resumes on the newest
  usable one.
- Codex, if its single installed copy can't hold a bundle per session: Otaq switches that copy only when no Codex
  session is working, waiting at a question, or stopped mid-stage. Other tools switch at once, per session. Codex
  users may get a fix later than others.

## Why

Decision 061 promises a running stage never mixes two versions of a skill (gap G4). Letting Codex switch at once was
turned down: a Codex stage could then follow two versions of a skill in one run.
