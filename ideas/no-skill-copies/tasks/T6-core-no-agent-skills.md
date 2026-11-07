# T6. Core: agents without skills, flows that name skills (S4, S5, S6's warning, S7, S8)

**Depends on:** T2 (copy code gone) and T3 (`Board::has_skill`). **Runs in parallel with:** T4 and T5.

Read `ideas/no-skill-copies/DECISIONS.md` first (S4–S8). The UI belongs to T7 and T8. Here, change the core,
service, protocol and CLI, and keep the desktop crates compiling with the smallest edits, listing them in your
report.

## What to change

### Agents (S4)

- **`agents/`:**
  - remove `Agent::skills`, `SkillRef`, `ImportedSkill`, `Agent::imported_skill`;
  - remove the import and re-import code in `agents/skills.rs` (`copy_in`, `import_folder`, re-import);
  - remove the shipped agents' skill lists in `agents/shipped.rs`, and `agents/store.rs`'s skill rows.
- **Protocol and service:** remove `agents.import_skill` and `agents.reimport_skill` (`protocol.rs:~1016`,
  `~1150`, `~2250`), their handlers in `service/agents_requests.rs`, and their `every_request.rs` entries. Remove
  the skills field from any agent request or answer.
- **CLI:** remove any `otaq` command or flag for an agent's skills (`rg -i skill crates/otaq-cli/src`), keeping
  `otaq flow write-skill` (see S7 below).
- **Migration** `0074_no_agent_skills.sql`, the next number after `0073_helper_dropped.sql`. Drop `agent_skills`
  and any column that only it needs. Check `0047_agents.sql`, `0050`, `0063` and `0066` for what references it.
  Add a migration test in `tests/suite/migrations.rs`.
- **Files on disk:** at service start, once, delete `$OTAQ_HOME/skills/` and `$OTAQ_HOME/skill-writes/`
  (`DataDir::skills_dir`, and the write-skill folder). Remove `DataDir::skill_path` and its kind if unused. No
  notice (S8).

### Flows (S4, S5, S6)

- **Flow file `agents[].skills`** (`flow/model.rs:~30`): still parsed so old files open, ignored, never written.
  Leave the field out when Otaq writes a flow file (`flow/yaml.rs`, `flow/proposal.rs:~1007`, `flow/store.rs`).
- **`flow/apply.rs`** (~47, ~917–~975): applying a flow no longer creates or shares imported skills. Remove
  `SkillSource::Imported` and the import of proposal folders (`ApplyChoices::skill_folders`).
- **`flow/proposal.rs`:** remove `ImportSkillFolder` ("isn't on this machine · Import its folder…", ~174) and the
  agent-skill checks (~8–9). Instead, a stage whose `skill:` names a skill `Board::has_skill` doesn't know carries a
  warning with the exact text **"You don't have the skill `<name>`"**. It's a warning only: apply still works (S6).
  - Expose the warning wherever the flow editor's checks are read, so T8 can show it on the stage, for both a
    proposal and the saved flow.
  - Otaq's own skill names never warn.
- **`needs_skill`** (`flow/model.rs:~215`) stays: a custom Agent stage still needs a `skill:` name. Any name is
  accepted.

### Writing a skill (S7)

- **`flow/write_skill.rs`:**
  - `otaq flow write-skill <stage>` hands Noyan the folder inside the project's checkout: `.agents/skills/<name>/`
    when the stage's agent's tool is Codex, `.claude/skills/<name>/` otherwise;
  - decide how the name is chosen before writing. Today it comes from the SKILL.md after writing. Keep whatever
    flow works with the least change, and say what you did;
  - `--done` checks that a SKILL.md with readable front matter is there, and sets the stage's `skill:` to its name.
    No import, no `$OTAQ_HOME`;
  - refuse to write into a folder that already exists and wasn't created by this write, with a plain refusal
    message.
- **The draft's written state** (`flow/draft.rs` `WrittenSkill`) carries the folder, relative to the checkout, so T8
  can show S7's text. **Undo** follows S7: delete the folder only while git says it's untracked and it holds only
  what was written; otherwise just clear the field.
- **Update the `flow` skill text** (`plugin/skills/flow/SKILL.md`) if it describes the write-skill folder.
  Coordinate by keeping the edit to that section only, since T4 also edits skill texts.

## Tests

- **Rewrite or delete:** `tests/suite/agents.rs` skill cases, `write_skill.rs`, `store.rs`, `model_refused.rs`,
  `validation.rs`, the flow proposal and apply tests that import folders, and `every_request.rs`.
- **New:**
  - the migration drops imported skills, and agents keep everything else;
  - an old flow file with `agents[].skills` opens and re-saves without the field;
  - a stage naming an unknown skill applies and carries the warning, while `validate` doesn't warn;
  - write-skill lands in `.claude/skills/<name>/` for a Claude Code agent and in `.agents/skills/<name>/` for a Codex
    agent, and `--done` sets `skill:`;
  - Undo removes an untracked written folder but leaves a committed one;
  - `$OTAQ_HOME/skills/` is gone after a service start.

## Done when

- `scripts/check.sh lint test` passes, with desktop and view crates compiling.
- `rg "ImportedSkill|SkillRef|import_skill|reimport" crates --type rust` finds only the migration test's literal
  SQL, if anything.
