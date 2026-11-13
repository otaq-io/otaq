# 073 · otaq-io/tuq is tuq's own repository

- **Date:** 2026-11-14
- **Status:** Accepted
- **Related:** 057, 064, 080, 081, 084

## Summary

`otaq-io/otaq-claude-skills` is renamed `otaq-io/tuq` and becomes tuq's own repository: the generic skills and the
`tuq` commands are edited and released there, and anyone can browse them quickly and install tuq for their own
projects.

## Decision

- It holds the generic skills (`product-spec`, `tech-spec`, `milestone-plan`, `step-back`,
  `security-check`, `update-docs`) and the `tuq` commands, not Otaq's workflow skills, which have their own
  repository (decision 057).
- Pull requests go to it directly.
- Installed without Otaq, it works on Claude Code, Codex, Copilot and OpenCode, the `tuq` commands on the PATH
  included (decision 064); its README says what tuq is, how to install it on each tool and how Otaq uses it. The line
  about Otaq copying `SKILL.md` and the "tug" spelling go.
- GitHub redirects the old URL.

## Why

The user wants tuq to be its own project that people use in their own terminals and projects (decision 080).
