# 080 · Otaq respects the tuq the user installed

- **Date:** 2026-11-14
- **Status:** Accepted
- **Related:** 060, 063, 065, 073, 076

## Summary

When a user has installed the generic tuq skills themselves, Otaq's sessions use their installed version, the same one
they use in their own terminals and projects. An install of Otaq's workflow skills is ignored: those always come from
Otaq's own bundle. The old `tuq@otaq-claude-skills` installed before this change is ignored once, at the switch.

## Decision

- **Generic tuq skills** (`product-spec`, `tech-spec`, `milestone-plan`, `step-back`,
  `security-check`, `update-docs`): a user's own install is never ignored; an Otaq session runs the version installed
  on their system.
- **Otaq's workflow skills** (`work-ticket`, `validate`, `commit`, `open-pr`, `push`, `address-review`, `restack`,
  `answer-question`, `noyan`, `flow`): a user's install of them is ignored; Otaq's sessions always run Otaq's own
  bundle (decisions 058, 076).
- Without an install of their own, Otaq's sessions use Otaq's bundles for every skill.
- Once, at the switch to tuq: an install from before this change (the old `tuq@otaq-claude-skills` fork) is ignored,
  and Otaq's own tuq runs instead.
- `tuq:<name>` in a flow means the tuq the session uses (decision 063).
- `otaq-io/tuq` keeps `.claude-plugin/marketplace.json` pointing at `./plugins/tuq`, so `tuq@otaq-claude-skills`
  installs keep updating after the rename.
- `browser-validate` leaves the marketplace (decision 065); a project skill that hands off to
  `browser-validate:validate-in-browser` (the maintainer's ski-web) is pointed elsewhere: the checker now runs the app itself (decision 065).

## Why

Later versions expect people to download tuq and use it in their own terminals and projects; Otaq must work with the
version they chose, not hide it behind its own copy (gap G6). Ignoring today's installs once is accepted because they
are the old fork from before the rename.
