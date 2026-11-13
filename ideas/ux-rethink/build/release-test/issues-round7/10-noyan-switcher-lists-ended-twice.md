# 10 · Noyan's conversation switcher lists conversations ended by a restart twice

**Kind:** bug · **Release:** to decide · **From:** re-test 8 part b (UX-47 note) · **Decision:** —

## What happens
The switcher (▾ next to "Noyan") lists a conversation in its top list and again under RECENTLY ENDED, both rows
"Ended by restart · Resume".

## Repro (fae1fd78, shop)
Open Noyan's panel → ▾: top list "Suggest shop flow · Ended by restart", "Add Security stage", "Test step, no work",
"Whole dollar README", … ; RECENTLY ENDED again "Suggest shop flow", "Add Security stage", "Test step, no work",
"Whole dollar README", … . Conversations ended with Stop / End agent ("Add Docs stage", "Security review stage",
state `ended`) appear only under RECENTLY ENDED.

DB (`project_agents`, shop): the doubled ones are `state = ended_by_restart`, `closed_at IS NULL`; the single ones
`state = ended`, `closed_at` set.

## Cause (confirmed by repro + DB + code)
`crates/otaq-core/src/project_agents/mod.rs:1163-1186` `list`: `open` = `closed_at IS NULL` (any state, so ended by
restart too), `recent` = `state <> running` (closed or not). A conversation ended but not closed is in both. The
switcher view model (`otaq-view/src/view_models/agent_panel.rs:786-809`) draws `rows` from `open` and `recent` from
`recent` without removing the overlap.

## Fix direction
Decide what the top list is (UX-47): if it's "open" (running, or resumable not yet closed), list under RECENTLY ENDED
only the closed ones (`closed_at IS NOT NULL`), or drop from `recent` any id already in `open` in `switcher()`.

## Risks to check
`find_agent` (`api/agent_panel.rs:355-360`) chains both lists; the ⋯ "Close ended agents" and "Recently ended…" menu;
the empty panel's RECENTLY ENDED list (uses `recent` alone, keep it complete); the 10-row `RECENT` limit.

## Evidence
`../shots-retest-9/r9-switcher-ended-listed-twice.jpg`; DB rows in `../RESULTS-retest-9-repro.md`; first seen
`../RESULTS-retest-8-b.md`.
