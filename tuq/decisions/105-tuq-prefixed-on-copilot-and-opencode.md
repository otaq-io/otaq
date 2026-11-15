# 105 · tuq's skills are named tuq-<name> on Copilot and OpenCode

- **Date:** 2026-11-14
- **Status:** Accepted
- **Supersedes:** 072, 092
- **Related:** 063, 074, 100, 102, 103

## Summary

tuq's install for Copilot and OpenCode names its skills with the prefix in their own name (`tuq-product-spec`), as
Otaq's workflow skills are named there (`otaq-validate`), so a repository's or the user's own `product-spec` can never
hide or be mistaken for tuq's.

## Decision

- On Copilot and OpenCode, every tuq skill is installed as `tuq-<name>`, with its references to other tuq skills
  rewritten to match; a person types `/tuq:tuq-product-spec` in Copilot and asks for `tuq-product-spec` in OpenCode.
- On Claude Code and Codex the plugin namespace does it: `/tuq:product-spec`, `$tuq:product-spec` (decision 074).
- Flow files keep `tuq:product-spec`; each tool's form is derived from it.
- An older tuq installed there with plain names still counts as the user's tuq (decision 100); Otaq's update
  (decision 103) brings the prefixed names.
- tuq's README says the same for people who install tuq without Otaq.

## Why

On Copilot a repository's own skill of the same name hid the plugin's (`Unknown command`, hands-on check with Copilot
CLI 1.0.109), and on OpenCode a plain `product-spec` can't be told from the user's own skill, which by decision 063 is
what a bare name means. Prefixing on every tool was turned down because Claude Code and Codex would read
`/tuq:tuq-product-spec`; keeping plain names was turned down because stages could fail.

## Changelog

- 2026-11-14: refined: Copilot runs a prefixed tuq skill typed as `/tuq:tuq-product-spec`; `/tuq-product-spec` answers
  "Unknown command" (hands-on check in the Windows VM, Copilot CLI 1.0.109, bring-together of
  skills-in-their-own-repos). OpenCode has no typed form.
