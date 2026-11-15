# 071 · Product decisions always live in tuq/

- **Date:** 2026-11-14
- **Status:** Accepted
- **Related:** 070

## Summary

In every repository, Product spec writes decisions to `tuq/decisions/`, indexed in `tuq/README.md`, and never asks where they go.

## Decision

- On first use it adds the pointer to the repository's README, CLAUDE.md and AGENTS.md where they exist (Codex,
  Copilot and OpenCode read AGENTS.md).
- A project that already keeps decisions elsewhere (`decisions/`, `docs/product.md`): new decisions still go to
  `tuq/`; the old folder is never moved or changed, but when the project's notes name it, Product spec, Tech spec and
  Milestone plan read it as earlier decisions, the conflict check included.

## Why

A fixed place lets Tech spec, Milestone plan and Step back find decisions without asking, and the `tuq` command
supports one layout. Asking on first use was turned down. Was S20.
