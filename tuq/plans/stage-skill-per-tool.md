# Plan · stage-skill-per-tool

- **Ticket:** none
- **Date:** 2026-11-06
- **Decisions:** none in tuq/; this round's are ideas/no-skill-copies/DECISIONS.md (S5, S6, F12–F18), ideas/no-fallback-agents/DECISIONS.md, ideas/one-check-agent/DECISIONS.md (O1–O6)
- **Tech spec:** none; ideas/gap-review/REPORT.md holds the code evidence (file:line) per gap
- **Orchestrator prompt:** tuq/prompts/stage-skill-per-tool-orchestrator.md

<!-- tuq:plan:start -->

The rule behind this work: a stage runs only on a tool that has its skill, and a flow file says everything a flow
does (ideas/no-skill-copies S5–S6, ideas/no-fallback-agents, ideas/one-check-agent O1–O6). It is built in 16 pieces,
one behaviour each. Three come first: knowing which tools have a skill (1) and removing fallbacks (2) side by side,
then one agent per check stage (3, after 2 because both rewrite the per-check-agent holds). The per-ticket agent
choice (5) needs all three. The editor filter (6) and the held stage (7) build on 1 and 5. The stage-feature changes
(8–10) and the small follow-ups (11–14) are independent of each other. Docs (15) and a hands-on run on all four tools
(16) come last. F10 (provider errors) is already merged (E2) and isn't a piece. The agent in charge follows
tuq/prompts/stage-skill-per-tool-orchestrator.md.

## Pieces

1. Otaq knows which tools have each skill — waits for nothing
   The skill finder (crates/otaq-core/src/skills/found.rs; Board::has_skill in crates/otaq-core/src/board/mod.rs:146)
   answers per tool, not per project: a tool has a skill when it finds it in the ticket's worktree, which means that
   tool's own personal folder, the project's folders when the skill is committed at the ticket's base, or, for Claude
   Code only, the checkout's .claude/skills. A skill in another tool's folder doesn't count. The finder also reads the
   skills of the tools' installed plugins (Claude Code, Codex, Copilot), and reads CLAUDE_CONFIG_DIR, CODEX_HOME,
   COPILOT_HOME and XDG_CONFIG_HOME from the environment Otaq launches each tool with, not from the service's own
   (today agent_tools::process_var, crates/otaq-core/src/agent_tools/mod.rs:286). "Pick a skill…" in the flow editor
   (skills.list) shows, for each skill, the tools that have it. Otaq's own skills are had by every tool. Rules:
   ideas/no-skill-copies/DECISIONS.md S6 ("Which tool has a skill"). Gaps G4, G6, G18 in ideas/gap-review/REPORT.md.
   Check: tests show that ~/.claude/skills/x is had by Claude Code and not by Codex; an uncommitted project skill only
   by Claude Code; a plugin's skill is found; skills.list names the tools per skill.
2. No fallbacks of any kind — waits for nothing
   Remove Fallback per tool from Settings → Agents (crates/otaq-view/src/view_models/agents.rs:470-595, the
   agents.fallback request and setting, crates/otaq-core/src/store/settings.rs:22), the card's Use fallback
   (crates/otaq-view/src/view_models/card.rs:2545, ticket.use_fallback) and the banner's Use fallback now
   (limits.use_fallback_now; crates/otaq-core/src/agents/gate.rs:651-760, crates/otaq-desktop/src/api/limits.rs and
   notices.rs), with the amber "On Codex · Claude Code limit" line, the "on the fallback" counts in the banner and its
   notification (crates/otaq-core/src/attention/notify.rs:543), the "has no fallback" errors, and the fallback texts in
   crates/otaq-view/src/view_models/tool_problems.rs. A tool at its limit, logged out, not installed, hidden or needing
   an update, and a model its tool no longer lists, make their agents wait; the card says why and offers Open agent…
   (Change agent… comes in piece 5). Noyan's "New agent on…" stays and picks the Project agent default, else the first
   agent on a tool that can run (tool_problems.rs:418). Moving over: stored fallback settings and holds'
   fallback_asked are dropped silently; a session already on a fallback finishes there; History keeps its old lines.
   Leave the rule that Skip this check needs no usable fallback alone beyond making it compile: piece 10 replaces it.
   One behaviour across core, view, desktop and UI, on purpose. Rules: ideas/no-fallback-agents/DECISIONS.md.
   Check: git grep -i fallback finds only unrelated uses (font, models.dev, migrations); a ticket held for a Claude
   Code limit waits and starts on Claude Code after the reset.
3. A check stage runs one agent — waits for 2
   An agent check has one agent, like an Agent stage: Stage.agents (crates/otaq-core/src/flow/model.rs:56) gives way to
   agent; the file format writes `agent:` for a check (crates/otaq-core/src/flow/yaml.rs:281 refuses it today) and
   still reads `agents: [X]` with one name; the presets in crates/otaq-core/flows/*.yaml write `agent: Validator`.
   Remove the round shared by several agents (crates/otaq-core/src/board/validation.rs open_round, round_event,
   Round::several; the "Several check agents" part of crates/otaq-core/src/validation/mod.rs), per-check-agent holds
   (agent_holds.check_agent), the flow editor's Check agents list, Add a check agent and CHECK_AGENTS_RULE
   (crates/otaq-view/src/view_models/flow_editor.rs:1705-1771, crates/otaq-desktop/ui/src/screens/flow_editor/FlowEditor.tsx:823),
   the card's row per check agent (crates/otaq-view/src/view_models/check_agents.rs, ui/src/cards/CheckRound.tsx), the
   combined verdict texts (validation/mod.rs:1741-1759) and "Before you"'s "2 passed · 1 failed" for new rounds
   (validation_report.rs:518). Rounds stored before stay readable and show as they were. A validator's round in
   otaq prime --validate is its own stage's round (crates/otaq-core/src/prime.rs:687-800). otaq attach --agent and
   otaq show --agent are kept for one release and ignored with a note (crates/otaq-cli/src/cli.rs:134,160,958).
   Splitting flows that list several agents is piece 4: until it lands, a file with two check agents may be refused.
   One behaviour across core, view and UI, on purpose. Rules: ideas/one-check-agent/DECISIONS.md, Decision.
   Check: a flow with Validate (agent: Validator) runs one validator per round; the flow editor shows one agent field
   for a check; the core tests that pinned several check agents (tests/suite/check_agents.rs,
   tests/flows/every-kind.yaml) are rewritten or moved to piece 4.
4. Old flows with several check agents open split — waits for 3
   A flow (a stored version, Back to the previous flow, a flow file, a proposal from otaq flow propose) whose check
   stage lists several agents is split when it is read: one check stage per agent, in order; the first keeps the id
   and name, the others get new ids and the name "<stage> · <agent>", with " 2" added on a name clash (the Duplicate
   rule, crates/otaq-core/src/flow/check.rs:120); each copy keeps skill, rounds, fail_to, options and why; copies after
   the first share the first copy's column (with-previous). The split is saved only with the user's next change;
   stored versions are never rewritten (crates/otaq-core/src/flow/store.rs:1-7). A ticket's skip or moved-past on the
   old stage (ticket_stage_choices; check_counters by stage id, migrations/0051_check_counters.sql) applies to every
   copy; a chosen agent stays on the copy for that agent; every copy's rounds start at 0. A ticket in the middle of
   such a stage finishes its round or restarts on the first copy: pick the simpler and say which in the commit.
   Rules: ideas/one-check-agent/DECISIONS.md, Moving over.
   Check: tests read every-kind.yaml's Security (two agents) as two stages with the right names, columns and copied
   skips, and Back to the previous flow on an old version gives the same split.
5. A ticket picks any agent for any stage — waits for 1, 2, 3
   A stage no longer lists the agents it allows: the agents in a stage's options are read and ignored, so options holds
   only skip (crates/otaq-core/src/board/stage_choices.rs; StageOption in crates/otaq-core/src/flow/model.rs). For any
   stage of a ticket that hasn't started, the user can pick any agent whose tool has the stage's skill (piece 1); a
   stage running one of Otaq's skills allows every agent. Only the user picks, or Noyan with --by-user; a ticket's own
   agent picking an agent for a later stage is refused (stage_choices.rs:10-12 allows it today with a reason). The
   ticket sheet shows the choice per stage; a card waiting for its tool (piece 2) offers Change agent…, which is this
   same choice for the waiting stage. The CLI's --stage "<stage>=<agent>" follows the same rules. One behaviour across
   core, CLI, view and UI, on purpose. Rules: ideas/one-check-agent/DECISIONS.md O1; ideas/no-fallback-agents/DECISIONS.md,
   What stays.
   Check: on the default flow (no options), the user sets Dev=Strong for one ticket and it runs on Strong; picking an
   agent whose tool lacks the stage's skill is refused; a ticket agent's --stage Validate=Strong is refused; Change
   agent… on a card held for a limit starts it on the chosen agent.
6. The flow editor offers only agents whose tool has the stage's skill — waits for 1, 3
   In the flow editor (crates/otaq-view/src/view_models/flow_editor.rs, agent_field at :1601; the UI under
   crates/otaq-desktop/ui/src/screens/flow_editor), a stage's agent picker lists only agents whose tool has the stage's
   skill (piece 1); a stage running one of Otaq's skills lists every agent. A stage whose current agent's tool lacks
   the skill still applies and shows "<tool> doesn't have the skill `<name>`" on the stage. The Skill picker shows
   which tools have each skill. Rules: ideas/no-skill-copies/DECISIONS.md S5, S6.
   Check: with team-validate only in ~/.claude/skills, the Validate stage's agent picker offers only Claude Code
   agents; switching the skill back to Otaq's validate offers all agents.
7. A stage is held when its tool lacks its skill — waits for 1, 5
   Replaces F1 (crates/otaq-core/src/board/missing_skill.rs; the held-back reminder in
   crates/otaq-core/src/signals/reminder.rs:160): a stage due to start on a tool that doesn't have its skill (piece 1)
   doesn't start. The card goes to Needs you with "<tool> doesn't have the skill `<name>`", Check again (asks the
   finder again and starts the stage if found) and Change agent… (piece 5, only agents whose tool has the skill). The
   same check runs when a stage's skill is typed into a session already running: Open PR into the live Dev session
   (crates/otaq-core/src/pr/service.rs:851) and queued returns (crates/otaq-core/src/start/sequence.rs:863); there it
   holds the delivery instead. The old warning-while-running and the held-back reminder go. One behaviour across
   core, view and UI, on purpose. Rules: ideas/no-skill-copies/DECISIONS.md S5.
   Check: a Validate stage naming team-validate, run on a Codex agent with the skill only in ~/.claude/skills, stays
   held in Needs you and nothing is typed into a session; Check again after linking it into ~/.codex/skills starts it.
8. Nothing is skipped on its own — waits for 5
   Remove the automatic skip of a skippable human check when every Agent stage before it was skipped
   (crates/otaq-core/src/board/stage_choices.rs:24-27, NOTHING_BEFORE_IT_RUNS at :322, board/human_check.rs:79). The
   per-ticket choice "required" stays (REQUIRED in stage_choices.rs; --stage "X=required"): it means the check happens
   as the flow says (word kept 2026-11-06). Keep the refusal of a set of skips that leaves a non-skippable check with
   nothing before it (strands_check_refusal). Rules: ideas/one-check-agent/DECISIONS.md O3.
   Check: on Spec first, a ticket skipping Product spec, Tech spec and Milestone plan reaches Spec review and waits
   there; --stage "Spec review=required" is taken and skips nothing.
9. The Delivery does what the flow says — waits for nothing
   Remove every forced click Delivery: after a skipped human check (crates/otaq-core/src/flow/engine.rs:189 skips,
   crates/otaq-core/src/board/merge.rs:571), and after Move on anyway or Skip this check on an agent check
   (crates/otaq-core/src/board/validation.rs:1417-1485, crates/otaq-core/src/domain/validation.rs:195,370, the
   moved_past_reason texts). An automatic Delivery merges or opens its PR as the flow says; delivered_without_checks
   still records it for Review and History. In the flow editor, next to a check's skip mark, say that a ticket
   skipping it or moved past it is delivered without that check. One behaviour across core and the editor, on purpose.
   Rules: ideas/one-check-agent/DECISIONS.md O4.
   Check: on Solo autopilot with Validate marked skippable, a ticket that skips Validate is merged automatically, and
   the flow editor shows the note next to the skip mark.
10. Skip this check and Move on anyway only on skippable checks — waits for 2
   Skip this check (a check waiting for its tool's limit) and Move on anyway (after a check's last failed round) are
   offered and accepted only on a check the flow marks skippable (options: [skip]). Today Skip this check depends on
   there being no usable fallback (crates/otaq-core/src/validation/mod.rs:742-768 may_skip_held,
   crates/otaq-core/src/agents/facts.rs:235 can_skip_check, crates/otaq-view/src/view_models/card.rs:2549); that rule
   goes. The service refuses both on other checks. Rules: ideas/one-check-agent/DECISIONS.md O5.
   Check: on the default flow (Validate not skippable) neither button shows and both requests are refused; with
   Validate marked skippable both show at the right moment.
11. "Use your own Open PR skill" points at the Delivery stage — waits for nothing
   The PR error (crates/otaq-core/src/pr/service.rs:219), plugin/skills/open-pr/SKILL.md:46-47 and the PR provider
   row's link (crates/otaq-desktop/ui/src/screens/settings/Rows.tsx:273; crates/otaq-view/src/view_models/settings_page.rs:1866)
   say to name your own skill as the Delivery stage's skill, and the link opens the flow editor on the Delivery stage's
   Skill field, not Settings → Agents. Rules: ideas/no-skill-copies/DECISIONS.md F15.
   Check: on a GitLab project, the PR error and the settings link lead to the Delivery stage's Skill field; no text
   mentions the agent's skills.
12. Codex's plugin copy is refreshed on any plugin change — waits for nothing
   stale_cached_skills (crates/otaq-core/src/agent_tools/codex/setup.rs:518) compares only skills/; compare every file
   of the plugin (manifest, hooks, skills), so an update that changes only hooks reruns codex plugin add and records
   hook trust again, as setup does. Rules: ideas/no-skill-copies/DECISIONS.md F16.
   Check: a test with a cached copy whose hooks differ from runtime/codex-plugin's reruns the setup; an identical copy
   doesn't.
13. "What this agent reads" shows the user's own skill — waits for 1
   In Settings → Agents, "What this agent reads" (crates/otaq-core/src/agent_reads.rs:160-173) shows a stage's user
   skill with its text, from where the finder found it (piece 1), and Open skill folder → to that folder, instead of
   nothing. Rules: ideas/no-skill-copies/DECISIONS.md F17.
   Check: with team-validate in ~/.claude/skills on Validate, a Claude Code validator's page shows its SKILL.md and
   opens ~/.claude/skills/team-validate.
14. The code left from imported skills is gone — waits for nothing
   Remove the unused refusals BadSkill, OriginalNotFound and NotImported (crates/otaq-core/src/board/mod.rs:285-298)
   and fix the comments that still describe copies: crates/otaq-core/src/agent_tools/codex/launch.rs:12-14,
   pr/service.rs:137-139, agent_tools/mod.rs:910, project_agents/first_flow.rs:5, agent_tools/codex/setup.rs:859, and
   protocol.rs:985 ("the move-over's notice"). Comments name rules in words, never by decision number. No behaviour
   changes. Rules: ideas/no-skill-copies/DECISIONS.md F18.
   Check: git grep finds none of these names or texts; scripts/check.sh passes.
15. The docs, the website and the flow skill describe the new rules — waits for 2, 3, 4, 5, 7, 8, 9, 10
   docs/how-it-works.md: the "Agents: any tool, any model" section (lines 84-95 today: Developer/Validator rows, roles
   per project, agents' own stage choices, fallbacks) and Skills (a stage runs only on a tool that has its skill, and is
   held at start otherwise); website/src/components/Agents.astro:179-185 and the demo (website/src/demo/state.ts:629,
   view.ts:637); website/src/components/Flow.astro:17 ("Several check agents"); plugin/skills/flow/SKILL.md:184-187,275
   (one agent per check, `agent:`, one stage per check, no automatic skip, no forced click). The release
   notes say checks that ran side by side now run in turn. Rules: all three decision files.
   Check: git grep for fallback, check agents and "side by side" in docs/, website/src and plugin/skills
   finds no old rule; the website's tests pass.
16. Hands-on on all four tools — waits for 4, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15
   Like ideas/no-skill-copies/tasks/T10-hands-on.md, on the merged branch: Claude Code on the Mac; Codex, Copilot and
   OpenCode in the Windows VM (the memory "Windows VM test setup" says how), OpenCode with a provider that has quota.
   Per tool: a Dev session starts with Otaq's skill (on OpenCode, a shipped skill with disable-model-invocation loads);
   a personal skill in another tool's folder holds the stage with "<tool> doesn't have the skill"; Change agent… moves
   it; a limit makes only that tool's agents wait; a flow with two check agents opens split. Write the results to
   ideas/gap-review/hands-on-report.md. Rules: ideas/no-skill-copies/DECISIONS.md F13.
   Check: the report has a pass for every check on every tool, or names each failure with what was seen.

<!-- tuq:plan:end -->
