# 075 · Existing flows are rewritten to otaq: and tuq: names once, on upgrade

- **Date:** 2026-11-14
- **Status:** Accepted
- **Related:** 063, 074

## Summary

On the first run of the Otaq version that renames its skills, every project gets a new flow version in which Otaq's
own skills carry their namespace: `otaq:<name>` for workflow skills, `tuq:<name>` for generic ones. Going back to an older flow version, or importing an older flow file, gets
the same rewrite, so from then on a bare skill name in a flow is always the user's.

## Decision

- Once per project, on upgrade: a new flow version replaces each bare name of one of Otaq's own skills
  with its namespaced name: `work-ticket`, `validate`, `commit`, `open-pr`, `push`, `address-review`, `restack`,
  `answer-question`, `noyan`, `flow` become `otaq:<name>`; `product-spec`, `tech-spec`, `milestone-plan` become
  `tuq:<name>`. The flow history shows it as Otaq's
  rename, not as the user's change. Earlier versions stay as they were stored.
- "Back to the previous flow" and importing a flow file written before the rename apply the same rewrite.
- The shipped flows, the presets and Edit flow's ready stages name Otaq's skills as `otaq:<name>` and `tuq:<name>`.
- A stage that names none of Otaq's own skills is left as it is: a 0.2 user's custom flow keeps every custom skill it
  names (`team-validate`, `my-pr`), and its bare name stays the user's skill. The rename changes no skill that runs:
  in 0.2 a bare name of one of Otaq's skills already ran Otaq's.

## Why

Without it, every existing ticket would be held for a missing skill after the upgrade (gap G1). Keeping stored flows
as they are and reading bare names in older flows the old way was turned down: Otaq would carry two reading rules
forever.
