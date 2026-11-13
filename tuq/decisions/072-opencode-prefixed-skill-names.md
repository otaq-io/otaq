# 072 · OpenCode gets skills named otaq-<name> and tuq-<name>

- **Date:** 2026-11-14
- **Status:** Accepted
- **Related:** 063, 064

## Summary

OpenCode has no plugin namespace for skills, so on OpenCode every Otaq skill is named with its prefix in the name
itself: `otaq-work-ticket`, `tuq-product-spec`.

## Decision

- Each skill's references to others are rewritten to match in that OpenCode version.
- A user's own `validate` on OpenCode can then never be taken for Otaq's.
- Otaq makes the OpenCode version itself, on the user's machine, from files whose tag signature it has just checked
  (decision 084), and the app build does the same for its built-in skills (decision 079). The repositories keep one
  copy of each skill; no OpenCode copy is committed.
- A user's own tuq installed for OpenCode keeps its plain names (`product-spec`), and Otaq names those skills as
  installed (decision 080).

## Why

On OpenCode Otaq names a skill by its bare name, so `validate` could mean the user's or Otaq's. Accepting that clash,
as `mattpocock/skills` does, was turned down. Committing a generated OpenCode copy in each repository was turned down:
it is a second copy that can fall out of step. Part of S21.
