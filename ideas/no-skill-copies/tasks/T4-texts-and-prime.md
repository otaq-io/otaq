# T4. Skill texts and prime name skills the new way (S2, S3)

**Depends on:** T2, which provides the reference forms and removes `copy_name`. **Runs in parallel with:** T5, and
with T6 once that starts.

Read `ideas/no-skill-copies/DECISIONS.md` first (S2, S3).

## What to change

- **`plugin/skills/*/SKILL.md`:** every mention of a copy name changes. Today these say things like "(`otaq-commit`)",
  "the `otaq-push` skill" and "`.agents/skills/otaq-…`", in `address-review`, `milestone-plan`, `open-pr`,
  `product-spec`, `restack`, `tech-spec` and `work-ticket` (`rg "otaq-[a-z]" plugin/skills`).
  - The skills are shared by all four tools, so name another Otaq skill in a way that reads right on each:
    "Otaq's `commit` skill". Never give a path into the working folder, and never use an `otaq-` prefix.
  - Remove any text that tells the agent a skill was copied, or where copies live.
- **Prime** (`prime.rs`, `epic_lead/prime.rs`, `project_agents/prime.rs`, and anything else `rg "copy_name|otaq-"`
  finds under `crates/otaq-core/src` outside `agent_tools/`):
  - "Your stage" names the stage's skill by the reference the reading agent's tool uses (S3), via
    `AgentTool::skill_reference`;
  - drop the agent's skill list from prime if it's still there (S4).
- **Other files in the earlier grep:**
  - `attention/notify_check.rs`, `board/settings.rs`, `review/mod.rs`, `flow/store.rs`, `flow/yaml.rs`,
    `store/versions.rs`, `warm/store.rs`: fix any text or comment that describes copies;
  - leave code that T5 or T6 owns.

## Tests

- `crates/otaq-core/tests/suite/plugin_texts.rs` and `plugin.rs`: replace the `copy_name(...)` needles with the new
  wording, and un-ignore every test T2 marked `#[ignore = "T4"]`.
- Add a test: no file under `plugin/skills/` contains `otaq-` followed by a shipped skill's name, or
  `.agents/skills` / `.claude/skills`.
- `otaq-cli`'s `skill_commands` test (the skills only teach real CLI commands) still passes.
- Prime tests: a Claude Code reader sees `/otaq:validate` for a stage with no `skill:`, and `/team-validate` for
  `skill: team-validate`. A Codex reader sees `$otaq:validate`.

## Done when

- `scripts/check.sh lint test` passes, and nothing is left ignored for T4.
