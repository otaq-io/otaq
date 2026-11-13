# 02 · A stage whose Codex skill has a byte order mark shows no warning

**Kind:** bug · **Release:** must fix · **From:** re-test FAIL, piece 7

## What happens
`SKILL.md` saved with a byte order mark (EF BB BF) in `%USERPROFILE%\.codex\skills\retest-docs`:
- Pick a skill… says it correctly: "retest-docs · Personal · Codex doesn't load it: its SKILL.md starts with a byte
  order mark".
- A ticket reaching the stage is held with a hint about the mark.
- **But the stage itself has no warning:** not on the card and not on the panel, also after Cancel and reopening Edit
  flow. Piece 7 says the stage warns "Codex doesn't load it…".

Without the mark, everything works (the skill is Codex's, and the ticket starts on it).

## Evidence
`../shots-retest/p07-bom-picker.jpg`, `p07-bom-stage-no-warning.jpg`, `p07-bom-held.jpg`; RESULTS-retest.md, piece 7.

## Where to start
`crates/otaq-core/src/flow/proposal.rs` ~997 (the BOM message), `agent_tools/codex/mod.rs` ~116, and wherever the
stage's skill warning is worked out (the missing-skill warning is shown there, but the BOM case isn't).
