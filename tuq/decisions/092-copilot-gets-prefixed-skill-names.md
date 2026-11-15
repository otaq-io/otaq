# 092 · Copilot gets skills named otaq-<name> and tuq-<name>, like OpenCode

- **Date:** 2026-11-14
- **Status:** Partly superseded by 102, 105
- **Supersedes:** 074
- **Related:** 063, 072, 079

## Summary

On Copilot, Otaq's skills carry their prefix in their own name (`otaq-validate`, `tuq-product-spec`), the same copy
OpenCode gets, so a repository's own skill of the same name can never hide one of Otaq's.

## Decision

- Copilot sessions get the prefixed copy of Otaq's skills (decision 072), with each skill's references to others
  rewritten to match, made the same way and for every set of skills, built-in ones included (decision 079).
- A message about a Copilot stage names the skill the way Copilot runs that copy (`/otaq:otaq-validate`), not
  `/otaq:validate`. This replaces only Copilot's form in decision 074; Claude Code keeps `/otaq:validate` and Codex
  `$otaq:validate`.
- Flow files, the flow editor and Settings still write `otaq:validate` and `tuq:product-spec` (decision 074).
- The user's own skills are untouched, and a user's own tuq installed for Copilot keeps the names it was installed
  with (decision 080).

## Why

The hands-on check (Copilot CLI 1.0.109, 2026-11-14) showed that Copilot lists a plugin's skills without their
plugin name and lets a repository's own skill of the same name hide the plugin's: a repository with its own
`.github/skills/validate` made `/otaq:validate` answer "Unknown command", and the stage sat idle. The namespace
decision 074 relied on doesn't protect Otaq's skills on Copilot. Holding the stage until the user renames their skill,
prefixing every skill's name on every tool, and accepting the clash were turned down: the prefixed copy already exists
for OpenCode, so Copilot gets it with no new mechanism and nothing for the user to do.

## Changelog

- 2026-11-14: refined: Copilot runs the prefixed copy typed as `/otaq:otaq-validate`; `/otaq-validate` answers
  "Unknown command" (hands-on check in the Windows VM, Copilot CLI 1.0.109, piece 5 of skills-in-their-own-repos).
