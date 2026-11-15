# 091 · Codex sessions get their own copy of the skills, like every tool

- **Date:** 2026-11-14
- **Status:** Superseded by 101
- **Supersedes:** 078
- **Related:** 061, 076, 090

## Summary

Each Codex session runs on its own copy of Otaq's skills, as sessions on every other tool do. The rule that Codex's
shared copy switches only when no Codex session is mid-stage goes.

## Decision

- A new release reaches Codex's next new session at once, as on Claude Code, Copilot and OpenCode; a running or
  resumed Codex session keeps the skills it started with (decision 061).
- The rest of decision 078 stands: the built-in skills are kept while a session started on them can still resume,
  and sessions from before the change resume on the newest release.

## Why

The hands-on check (Codex CLI 0.166.0, 2026-11-14) showed Codex can give each session its own copy: with its own
Codex home per session, or with each copy installed in one home and one picked per run. The wait-until-idle rule
existed only because that was thought impossible, and it delayed fixes for Codex users.
