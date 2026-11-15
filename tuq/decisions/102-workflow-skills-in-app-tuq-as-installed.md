# 102 · Otaq's workflow skills ship in the app; tuq is the one the user has, installed by Otaq when missing

- **Date:** 2026-11-14
- **Status:** Accepted
- **Supersedes:** 057, 058, 059, 060, 066, 067, 068, 069, 072, 076, 079, 080, 082, 084, 085, 088, 092, 095, 096, 097, 099, 101
- **Related:** 063, 081, 083, 093, 094, 098, 100

## Summary

Otaq's workflow skills (`work-ticket`, `validate`, `commit`…) ship inside the app, like its hooks, and change only
with it. tuq is never copied or released by Otaq: if the user has tuq installed, Otaq's sessions use it, the user's own
changes included; if not, Otaq installs tuq the tool's normal way and uses it. Otaq fetches, signs and stores no skill
releases.

## Decision

- **Workflow skills:** built into each app version, always matching it, as the hooks are (decision 082). A fix
  reaches users with the next app update. Their repository stays separate (decision 057), but nothing is released from
  it on its own: no signed tags, no "needs Otaq" range, no checks for new skills, no setting to turn them off.
- **tuq, when the user has it** in the tool a stage runs on: Otaq's sessions use that install as it is, including any
  change the user made to it.
- **tuq, when the user doesn't have it** in that tool: Otaq installs it the way tuq's README says for that tool, as an
  ordinary install the user's own terminals also get, and uses it.
- tuq is updated the way that tool updates any installed plugin or skill; Otaq doesn't update it on its own.
- Otaq's prefixed copies (`otaq-<name>` on OpenCode and Copilot, decisions 072, 079, 092) are made only of its built-in
  workflow skills.
- Settings → About shows the app version (which includes the workflow skills) and the tuq each connected tool has; a
  History row names the tuq version a session ran.

## Why

The release machinery decided earlier (signed tags, a store of releases, ranges, checks every 15 minutes, a setting,
Otaq's own tuq copy beside the user's) grew to protect cases that rarely happen, and made a simple idea hard to follow.
The workflow skills exist to drive Otaq and change with its code (29 of 31 skill commits since 0.1.0 also changed other
files), so they belong with the app like the hooks. tuq is a tool people use on their own: using what they have, and
installing it when they don't, is what any user expects. Keeping the workflow skills on their own release path was
turned down: it kept the version range and old-form rules for little gain.
