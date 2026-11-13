# 015 · A skill is named the way the stage's tool calls it

- **Date:** 2026-11-10
- **Status:** Accepted

## Summary

Wherever Otaq names a skill in a message about a stage, it writes it the way that stage's tool calls it: `$retest-docs`
for Codex, `/retest-docs` for Claude Code.

## Decision

- A skill's name in Otaq's text (the held stage's message "Codex doesn't have the skill …", warnings and the like)
  uses the call form of the tool that stage runs on: `$name` for Codex, `/name` for Claude Code, and each other
  tool's own form.

## Why

The user can copy the name straight into that tool. The release test (issue 12) found "/retest-docs" in a Codex
stage's message, which isn't how Codex calls a skill. Writing the plain name everywhere was turned down because it
doesn't tell the user how to call the skill.
