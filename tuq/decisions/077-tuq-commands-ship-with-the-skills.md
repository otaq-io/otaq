# 077 · The tuq commands ship with the skills

- **Date:** 2026-11-14
- **Status:** Accepted
- **Related:** 058, 059, 062, 064, 073

## Summary

The helper commands the tuq skills call (`tuq`, `tuq-plan`) ship in each skill bundle, so they are updated without an
app release like the skills. They run on macOS, Windows and Linux with nothing extra installed, and every tool runs
them without a permission prompt, as it runs `otaq`.

## Decision

- Each bundle carries the commands its skills call; a session gets the commands of its own bundle (decision 061).
- No bash or `python3` needed: they work the same from Codex's PowerShell on Windows as from a shell on macOS or
  Linux.
- Claude Code, Codex, Copilot and OpenCode pre-approve `tuq` and `tuq-plan` the way they pre-approve `otaq`.
- The published `otaq-io/tuq` carries them too, so tuq works without Otaq (decision 073).

## Why

Today they are bash scripts (one needs `python3`) that fail on Windows and stop at a permission prompt in an
unattended stage (gap G3). Folding them into the `otaq` command was turned down: changing them would then need an app
release, and tuq used without Otaq would lose them.
