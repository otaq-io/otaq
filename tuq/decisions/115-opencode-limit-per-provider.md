# 115 · An OpenCode limit holds only agents on the same provider

- **Date:** 2026-11-16
- **Status:** Accepted
- **Related:** 114

## Summary

OpenCode runs models from many providers, each with its own quota, so a plan limit OpenCode reports holds only the
OpenCode agents whose model comes from that provider. Every other tool keeps one limit for the whole tool.

## Decision

- **The provider** is the part of OpenCode's model id before `/` (`opencode/big-pickle` → `opencode`, shown as
  "OpenCode Zen"; `github-copilot/claude-haiku-4.5` → `github-copilot`). An agent on the tool's default model counts as
  the default model's provider.
- **A limit row for OpenCode records the provider** it was reported for. Starts, resumes, checks, helpers and answer
  sessions on OpenCode are held only when their model's provider has a limit; agents on other providers start as usual.
- **The banner, card and Resume** name the provider: "OpenCode · OpenCode Zen limit reached · resets 3:00 am", "Resume
  OpenCode Zen". Each provider's limit resets and resumes on its own.
- Claude Code, Codex and Copilot: unchanged, one limit per tool.

## Why

Found in the 0.3 Windows regression test (F-E2-3): Big Pickle's free daily limit held every OpenCode agent until the
reset, including one on Claude Haiku through github-copilot, with no way on. Turned down: an enabled Resume before the
reset that lets the user try anyway (the user has to guess), and leaving it as known.

## Changelog

- 2026-11-16: scheduled after 0.3: the plan review showed limits are per tool in a dozen places and the store keys them by
  tool. 0.3 keeps one OpenCode limit for the whole tool; the release notes say so.
