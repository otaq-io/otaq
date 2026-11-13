# 057 · Skills live in their own repositories: tuq and Otaq workflow skills

- **Date:** 2026-11-14
- **Status:** Accepted
- **Related:** 058, 073, 080, 081, 082, 084

## Summary

There are three repositories: Otaq (the app and its hooks), tuq (the generic skills) and Otaq workflow skills. Each
skill is edited, and released with signed tags, in its own repository, one copy each. Each app build ships the
tagged versions it was built with.

## Decision

- **tuq** (`otaq-io/tuq`): the generic skills (`product-spec`, `tech-spec`, `milestone-plan`, `step-back`,
  `security-check`, `update-docs`) and the `tuq` commands (decision 077).
- **Otaq workflow skills** (`otaq-io/otaq-skills`, plugin `otaq`, decision 063): `work-ticket`, `validate`, `commit`, `open-pr`, `push`,
  `address-review`, `restack`, `answer-question`, `noyan`, `flow`.
- **Otaq** (`otaq-io/otaq`): the app and the hooks plugin (decision 082); no skill is edited there. `plugin/skills/`
  moves out.
- The tuq skills from `otaq-claude-skills` and Otaq's versions are merged into one copy each: `product-spec`,
  `tech-spec`, `milestone-plan` (decision 070); `validate-in-browser` is retired (decision 065). No "merge the change here by hand" copies remain.

## Why

The user wants each kind of skill in its own place, released on its own. The cost was accepted: a change that touches
a workflow skill and app code (29 of the 31 skill commits since 0.1.0 did) is two commits in two repositories, and the
oldest Otaq a workflow release needs is set by hand (decision 060). Keeping every skill in the Otaq repository with
published copies was turned down.
