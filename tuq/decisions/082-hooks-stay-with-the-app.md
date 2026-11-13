# 082 · Otaq's hooks stay with the app, in the otaq-hooks plugin

- **Date:** 2026-11-14
- **Status:** Accepted
- **Related:** 059, 062, 063, 077

## Summary

The hooks each agent tool runs to tell Otaq what the agent is doing stay with the app, in a plugin named
`otaq-hooks`, and change only with the app. Skill releases carry only skills and the `tuq` commands, so a session
loads Otaq's hooks plugin, Otaq's workflow skills (`otaq`) and tuq.

## Decision

- The hooks plugin per tool holds only the hooks (session started, working, waiting for permission, stopped,
  failed); it is updated by app updates, never by a skill release.
- It is renamed from `otaq` to `otaq-hooks`, because the workflow skills' plugin is now `otaq` (decision 063). Users
  never type its name; on Codex, the hooks' trust is recorded again once, as setup does.
- The workflow skills (`otaq`) and tuq (`tuq`) are separate plugins (decisions 057, 077).

## Why

The hooks are the board's eyes on the agent: Working, Needs you, Idle, Resume and the limit banner depend on them
(gap G11). Each hook only hands the event to the `otaq` command, so any change to what it does needs a new app anyway.
Letting skill releases carry hooks was turned down: it would gain nothing and would put programs that run on every
agent event into something downloaded outside the app's own update. Two plugins named `otaq` in one session would
clash, hence the rename.
