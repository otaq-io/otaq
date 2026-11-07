# T2. Sessions start without skill copies, and name skills the new way (S1, S2, S3, S5)

**Depends on:** nothing. **Runs in parallel with:** T1 and T3.

Read `ideas/no-skill-copies/DECISIONS.md` first (S1, S2, S3, S5). T1 puts the shipped skills into the plugins at
the same time. Assume they're there.

## What to change

- **Skill references** (`AgentTool::skill_reference`, `agent_tools/mod.rs:~783`, and the four adapters):
  - produce S3's two forms: Otaq's own skill gives `/otaq:<name>` (Claude Code, Copilot), `$otaq:<name>` (Codex) and
    `<name>` (OpenCode); the user's skill gives `/<name>`, `$<name>`, `<name>`;
  - "Otaq's own" means the name is one of `agents::shipped`'s skills;
  - `naming::PLUGIN_NAMESPACE` still exists; use it and don't hard-code `otaq`;
  - remove `skill_root`;
  - pre-T-0408 versions are in `git show 49604d8f^:crates/otaq-core/src/agent_tools/<tool>/mod.rs`.
- **Stop copying.** Remove every call that writes copies before a session starts or is told to run a skill:
  - `start/sequence.rs` (~865, ~1011, ~1067);
  - `helpers/start.rs` (~416, ~478);
  - `validation/mod.rs` (`copy_skills`, ~1476, ~1633);
  - `pr/service.rs` (`copy_skills`, ~626, ~740, ~779, ~864, ~942);
  - `messages/answering.rs` (~269);
  - `project_agents/checkout.rs` (the copy writing; leave the leftover tidy at ~76 to T5);
  - `session/deliveries.rs` (`skill_line` and its by-file text);
  - `review/apply_now.rs` (~214), `review/mod.rs` (~677), `review/delivery.rs` (~280).

  The first prompt (`FirstPrompt::Skill`) keeps the skill's own name: the stage's `skill:`, else Otaq's skill for
  the stage. A hand-off always types the skill's reference, because every shipped skill is in every session (S2).
- **Remove** `Sessions::set_started_skills`, `started_skills` and anything that exists only for them.
- **`crates/otaq-core/src/skills/`:**
  - delete `Sources`, `Written`, `write_session`, the `Board::copy_*` methods, `handoff_sources` and the copy
    writer (`write_copy`, `prepared`, partial/old handling);
  - keep, possibly in a smaller module, what finds and removes old copies: `marker_file`, `is_marked`,
    `marked_copies`, `remove_copy`, `remove_marked_copies`, `old_copies`, `remove_old_copies`, `copies_to_skip`.
    T5 uses them, and the warm build still skips leftovers;
  - keep `Board::delivery_skill` / `delivery_skill_name` (the flow's Delivery `skill:`), minus the imported-skill
    resolution: `Sources::runs` becomes "the stage's skill, else Otaq's";
  - rewrite the module comment to state S1–S3 in words.
- **Leave alone, for T6:** `agents::Agent::skills`, `SkillRef`, `ImportedSkill` and `skills::folder`, plus flow
  apply/proposal/write_skill (T6 removes them). Just stop reading the agent's skills when a session starts. If
  something won't compile without touching them, make the smallest change and say so in your report.
- `naming::copy_name`: remove it if nothing outside tests still needs it.

## Tests

- Delete or rewrite `tests/suite/worktree_skills.rs`, `checkout_skills.rs` and `workflow_skills.rs`, plus copy
  assertions in `agent_reads.rs`.
- Every test that waits for `/{copy_name(x)}` must wait for the new reference:
  - `otaq-cli/tests/suite/{open_pr,project_agents,comment_path}.rs`;
  - `otaq-core/tests/suite/{reopen,resume,project_agents,delivery,decision_outcome,validation,agent_launches,
    send_back,merge_check,answer_resume,preset_walks}.rs`.

  Add a small test helper for "Otaq's skill `x` as the stand-in tool sees it" instead of repeating the format.
- New: starting a ticket session writes nothing under `.agents/` or `.claude/` in its worktree. A Project agent
  start writes nothing into the checkout.
- New: a stage whose `skill:` names a user skill (`team-validate`) is typed as `/team-validate` (Claude Code
  stand-in), and with no `skill:`, as `/otaq:validate`.
- `plugin_texts.rs` and the skills' texts belong to T4. Where they fail only because of `copy_name`, mark them
  `#[ignore = "T4"]` and list them in your report.

## Done when

- `scripts/check.sh lint test` passes, apart from tests explicitly ignored for T4.
- `rg "write_copy|copy_for_session|copy_for_ticket_session|set_started_skills|handoff_sources" crates` finds nothing.
