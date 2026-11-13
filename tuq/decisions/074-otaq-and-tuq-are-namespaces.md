# 074 · otaq: and tuq: are namespaces, written the way each tool calls them

- **Date:** 2026-11-14
- **Status:** Accepted
- **Related:** 015, 063, 072

## Summary

`otaq:` and `tuq:` are never part of a skill's own name: they are plugin namespaces. A flow file writes
`otaq:validate` or `tuq:product-spec`; a message about a stage writes the skill the way that stage's tool calls it,
as decision 015 says.

## Decision

- A skill's own name and folder are lowercase letters, digits and single hyphens (`validate`, `product-spec`), as
  the Agent Skills standard requires; a colon is never part of it.
- Flow files and places tied to no tool (the flow editor's Skill field, Settings) write `otaq:validate`,
  `tuq:product-spec`.
- A message about a stage uses that stage's tool's form, so the user can copy it into the tool: `/otaq:validate`
  (Claude Code, Copilot), `$otaq:validate` (Codex), `otaq-validate` (OpenCode, decision 072).
- The skills follow `mattpocock/skills`' conventions for working on every tool: they name each other through the
  Skill tool in the namespaced form ("Call the Skill tool with `tuq:step-back`"), written per tool like the OpenCode
  names, so a user's own `commit` or `step-back` can never answer; a skill only a person may start carries
  `disable-model-invocation: true` and, for Codex, `agents/openai.yaml` with `policy.allow_implicit_invocation:
  false`; one `.claude-plugin/plugin.json` serves Claude Code, Codex and Copilot; no symlinks (Codex drops them).

## Why

A colon isn't allowed in a skill's name, so the prefix can only come from the plugin. Writing the namespaced name on
every screen was turned down: a Codex or OpenCode user couldn't paste it into their tool, which is why decision 015
exists. Was the rest of S21 in `ideas/no-skill-copies/DECISIONS.md`.
