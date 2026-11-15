# 100 · "Your tuq" is tuq installed as a plugin, per tool

- **Date:** 2026-11-14
- **Status:** Accepted
- **Supersedes:** 069, 080
- **Related:** 063, 072, 083, 092

## Summary

Otaq treats as the user's own tuq only tuq installed as a plugin in an agent tool (on OpenCode, tuq's own install), and
looks per tool. Loose or repository copies of tuq's skills are the user's own skills by their bare names.

## Decision

- **Counts as your tuq:** tuq installed as a plugin in Claude Code, Codex or Copilot, and on OpenCode tuq installed
  the way tuq's README says. A stage naming `tuq:<name>` runs that tool's install (decision 080).
- **Doesn't count:** tuq's skills copied as loose folders (`~/.claude/skills/product-spec`) or committed in a
  repository (`.claude/skills/product-spec`). They are the user's own skills by their bare names: a flow that names
  `product-spec` runs them (decision 063); `tuq:product-spec` runs the tuq the session uses.
- **Per tool:** each tool is looked at on its own. A stage on a tool with tuq installed runs that install; on a tool
  without it, Otaq installs tuq there first (decision 102). One ticket's stages may therefore run different tuq
  versions; History names each session's (decision 069).
- **About** lists tuq per tool when they differ: "tuq 2026.11.12; Claude Code: 2026.11.10". With one version
  everywhere, one line.

## Why

Loose copies have no `tuq:` namespace, so `/tuq:product-spec` couldn't be typed for them, and their bare names already
mean the user's own skills (decision 063). Counting any copy, the repository's first, was turned down: `tuq:product-spec`
would mean different files per repository and per tool. Respecting a user's tuq only when every tool has the same
version was turned down: their install would often be ignored.
