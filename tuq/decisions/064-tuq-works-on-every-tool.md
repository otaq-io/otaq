# 064 · tuq works the same on every tool and system

- **Date:** 2026-11-14
- **Status:** Accepted
- **Related:** 063, 065

## Summary

Claude Code, Codex, Copilot and OpenCode all get the tuq skills and the commands they call (`tuq`, `tuq-plan`) on the PATH, on macOS, Windows and Linux.

## Decision

- No tuq skill is Claude-only or macOS-only.

## Why

Today the `tuq` commands reach only Claude Code (a plugin's `bin/` is put on the PATH only there), and `tuq-plan
pull` needs `python3`, which Windows often lacks. Was S17.
