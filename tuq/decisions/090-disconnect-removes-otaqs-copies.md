# 090 · Disconnecting a tool removes only Otaq's own copies

- **Date:** 2026-11-14
- **Status:** Accepted
- **Related:** 078, 080, 082

## Summary

When the user disconnects an agent tool in Otaq, Otaq removes everything it put into that tool: the `otaq-hooks`
plugin, the `otaq` workflow skills, its copy of tuq, and their trust and rules. The user's own tuq install and skills
are never touched, and folder trust for their projects stays.

## Decision

- After a disconnect, a plain session of that tool in the user's own terminal shows no trace of Otaq.
- Only Codex needs it today: Otaq installs into Codex's own settings, while Claude Code, Copilot and OpenCode get
  Otaq's plugins per session and keep nothing to remove.

## Why

It is how disconnecting already treats Otaq's plugin and skills (gap G19); the new plugins follow the same rule. What
Otaq didn't put there isn't its to take away.
