# Gap review: no skill copies, no fallback agents, one check agent

Written on 2026-11-06, read-only, on branch `no-skill-copies` at `c0135120`. Nothing else was changed.

The three decisions reviewed:

- `ideas/no-skill-copies/DECISIONS.md` (called **NSC** below);
- `ideas/no-fallback-agents/DECISIONS.md` (**NFA**);
- `ideas/one-check-agent/DECISIONS.md` (**OCA**).

Paths are relative to the repository root. "Sure" says how certain I am that the gap is real, after checking the code.

## Summary

| Decision | Blocks release | Confuses users | Cosmetic | Total |
|---|---|---|---|---|
| NSC (no skill copies) | 4 | 4 | 4 | 12 |
| NFA (no fallback agents) | 3 | 5 | 0 | 8 |
| OCA (one check agent) | 2 | 8 | 0 | 10 |
| Across decisions | 0 | 1 | 0 | 1 |
| **Total** | **9** | **18** | **4** | **31** |

The top five:

1. **G1.** Once fallbacks are gone, a waiting ticket can't be moved to another tool one ticket at a time, although NFA says it can.
2. **G4.** The "You don't have the skill" warning doesn't know which tool a stage runs on. A Claude-only skill on a Codex stage gets no warning. That is exactly the risk NFA was written to avoid.
3. **G5.** F10 isn't built. A provider error still leaves the card on Working.
4. **G3.** "Skip this check" at the plan limit is offered only when no fallback can take the ticket. NFA doesn't say what happens to it.
5. **G7.** Splitting a check stage keeps a ticket's choices, skips and round counters on the first copy only.

What already holds, so it isn't repeated below:

- F1–F9 and F11 are built.
- T10 bugs 1 and 3–11 are fixed.
- D2 is resolved: OpenCode always runs `--standalone`.
- Commit and Push can't be replaced.
- No preset has more than one check agent.

---

## Blocks release

### G1. A waiting ticket can't be moved to another agent "one ticket at a time"

- **Decision:** NFA, "What stays": "A ticket's Developer or Validator can still be changed by hand, one ticket at a time."
- **Evidence:**
  - A ticket changes its agent only through a **stage choice**, and "a stage without options refuses any" (`crates/otaq-core/src/board/stage_choices.rs:1-17`).
  - No shipped preset gives Dev or Validate any `options` (`crates/otaq-core/flows/*.yaml`).
  - A held card offers only **Use fallback** and **Open agent…** (`crates/otaq-view/src/view_models/card.rs:346-351`). Open agent opens Settings → Agents, which changes the agent for every ticket. It replaced "Change agent" (Q280).
  - There is no "Developer row" or "Validator row" on a ticket. Only the docs describe one (`docs/how-it-works.md:84`).
- **Why it matters:** with fallbacks removed, a ticket waiting on a Claude Code limit has only two ways out on the default flow: wait, or change the named agent for every ticket.
- **Sure:** high.
- **Question:** When a ticket waits for its tool, how does the user run just that ticket on another agent?
  - *Recommended:* the held card gets **Change agent…**, which picks another agent for that ticket's waiting stage only, even when the flow lists no `options` for it.
  - *Alternative:* there is no per-ticket change. The user waits, or adds `options` to the stage in the flow.

### G2. NFA names features the product doesn't have, and misses the ones it does have

- **Decision:** NFA, "Decision". It removes "an agent's fallback agent", **Use fallback agents automatically** and **Move waiting to…**.
- **Evidence:**
  - The product has a fallback per **tool**, not per agent: Settings → Agents → **"Fallback per tool"**, "Used when a tool hits its plan limit." (`crates/otaq-view/src/view_models/agents.rs:595,1087`).
  - The card has **Use fallback** (`card.rs:2545`), and the banner has **Use fallback now** (`crates/otaq-core/src/agents/limits.rs:36`).
  - The automatic setting was already removed: "Nothing moves to a fallback without the user's click (user decision 2026-10-20)" (`crates/otaq-core/src/agents/gate.rs:19`).
  - Fallbacks also feed other things:
    - the card's amber line "On Codex · Claude Code limit" (`card.rs:2115`);
    - History's "Started on Codex · Claude Code limit" (`gate.rs:524`);
    - the notification "3 tickets on the fallback" (`crates/otaq-core/src/attention/notify.rs:553`);
    - the error "has no fallback: set one in Settings → Agents" (`gate.rs:630`).
- **Why it matters:** whoever builds NFA has to guess whether **Use fallback** on one card is a by-hand move, which NFA allows, or a fallback, which it removes.
- **Sure:** high.
- **Question:** Does NFA remove all three of Fallback per tool, the card's Use fallback and the banner's Use fallback now?
  - *Recommended:* yes, all three. The by-hand way is G1's per-ticket change.
  - *Alternative:* remove Fallback per tool and the banner's Use fallback now, and keep the card's Use fallback as a one-ticket move to an agent the user picks.

### G3. "Skip this check" at the plan limit depends on there being no fallback

- **Decision:** NFA, "Decision".
- **Evidence:**
  - Skip this check is offered on a check held for the limit "only when no fallback can take it". Otherwise it is refused with "{key}'s check can run on {tool}'s fallback: use the fallback instead" (`crates/otaq-core/src/validation/mod.rs:742-768`, `crates/otaq-core/src/agents/facts.rs:235`).
  - The card has two states for this: "Plan limit · a fallback is set" and "Plan limit · no fallback can run" (`card.rs:3306-3314`).
- **Why it matters:** with no fallback anywhere, the rule either always allows skipping or needs a new reason. Users waiting on a limit will reach for this button.
- **Sure:** high.
- **Question:** With no fallbacks, is Skip this check always offered on a check that waits for its tool's plan limit?
  - *Recommended:* yes, always offered while the check waits for a limit or a logged-out tool.
  - *Alternative:* offered only after the user has seen the reset time (a check waiting longer than N hours).

### G4. The missing-skill warning ignores the stage's tool and the worktree

- **Decision:** NSC S5/S6 and "Known gaps, accepted", which says of a Codex agent: "that agent doesn't see the skill; the card warns (S5)". NFA, "Why".
- **Evidence:**
  - `Board::has_skill` (`crates/otaq-core/src/board/mod.rs:146-162`) counts a name found in **any** tool's folder. It reads the project **checkout**, not the worktree the stage will run in.
  - So a skill that exists only in `~/.claude/skills` is "found" for a Codex stage.
  - A skill Noyan wrote into `.claude/skills` is "found" when a ticket runs that stage on Codex.
  - An uncommitted or gitignored project skill is "found", although only Claude Code reaches it from a worktree.
- **Why it matters:**
  - The Known gaps text promises a warning that never comes.
  - It's the case NFA was written to avoid: a stage that quietly runs without its skill.
  - The "stage still open" reminder isn't held back either, so the agent carries on without the skill, as CDX-3 did in T10.
- **Sure:** high.
- **Question:** Should "You don't have the skill" be judged against what the stage's own tool will see in the ticket's worktree?
  - *Recommended:* yes. Personal folders count for that tool only. Project skills count only when committed, or when the tool is Claude Code.
  - *Alternative:* keep one answer per project, and change the Known gaps text to drop "the card warns".

### G5. F10 isn't built

- **Decision:** NSC, "Follow-ups agreed after T10", F10.
- **Evidence:** no code or commit handles a provider error that isn't a limit. `git log --all` shows no F10 commit. `crates/otaq-core/src/agent_tools/events.rs:54` has the "turn failed" event, but nothing moves the card to a needs-you state with the tool's message.
- **Why it matters:**
  - With fallbacks gone, Otaq's handling of limits and errors is the only safety net.
  - A card stuck on Working after "Free limit reached" (what T10 saw on OpenCode) looks like progress.
- **Sure:** high that it isn't built. Medium on exactly what the current behaviour is per tool.
- **Question:** Does F10 have to be built before NSC and NFA ship?
  - *Recommended:* yes, in the same release.
  - *Alternative:* ship without it, and list "a provider error can leave the card on Working" under Known gaps.

### G6. Skills from tool plugins give a false warning, and the agent then stops

- **Decision:** NSC S5 and S6.
- **Evidence:**
  - The finder reads only the folders S6 lists (`crates/otaq-core/src/skills/found.rs:6-20`). It never sees a skill a Claude Code, Codex or Copilot plugin or marketplace provides (`plugin:skill`).
  - It also never sees OpenCode's singular `skill/` folders (`~/.config/opencode/skill`, `.opencode/skill`).
  - Since F1, a skill that isn't found also **holds back the reminder** (`crates/otaq-core/src/signals/reminder.rs:160`).
- **Why it matters:**
  - A team that ships its skills as a plugin gets "You don't have the skill" on every card.
  - Once the reminder is held back, an agent that ends its turn early just stops.
  - The finder isn't only cosmetic any more: it now changes what the agent does.
- **Sure:** high for plugin skills. Medium for OpenCode's `skill/`, which depends on the OpenCode version.
- **Question:** Should the finder also read skills that the tools' installed plugins provide?
  - *Recommended:* yes, read each tool's installed plugins' skills, read-only, as S6 does for folders.
  - *Alternative:* no. Keep the warning, but stop it from holding back the reminder, so a false warning costs only a line on the card.

### G7. Splitting a check stage keeps a ticket's choices, skips and counters on the first copy only

- **Decision:** OCA, "Moving over".
- **Evidence:**
  - Stage choices are stored per `(ticket, stage_id)` (`ticket_stage_choices`, `stage_choices.rs:1-28`).
  - Round counters and `moved_past` are stored per `(ticket_id, stage_id)` (`crates/otaq-core/migrations/0051_check_counters.sql:12-21`).
  - The first copy keeps the id. The other copies get new ids with nothing stored.
  - A chosen agent on an agent check today replaces only the **first** check agent (`crates/otaq-core/src/board/validation.rs:1760-1790`).
- **Why it matters:**
  - A ticket the user set to skip Security would still run "Security · Performance reviewer".
  - A ticket moved past the check (Move on anyway, Skip this check) would get the other copies again.
  - A copy's rounds would restart at 0 while the first copy carries rounds that every agent built up together.
- **Sure:** high on the code. The consequences are inferred.
- **Question:** When a check stage is split, does a ticket's skip or "moved past" on the old stage apply to every copy?
  - *Recommended:* yes. Skip and moved-past apply to every copy. A chosen agent stays on the copy for that agent. Every copy's rounds start at 0.
  - *Alternative:* only the first copy keeps them, and the others run as for a new stage.

### G8. When the split happens is open: stored flow versions never change

- **Decision:** OCA, "Moving over": "Otaq writes the split flow back the next time it saves the flow".
- **Evidence:**
  - Stored versions are never edited, and "Back to the previous flow" is a new version made from an older one (`crates/otaq-core/src/flow/store.rs:1-7`).
  - So older versions with several agents stay, and going back would bring them in again.
  - A proposal's diff would show the copies as new stages (`crates/otaq-core/src/flow/proposal.rs:198,252,311`).
- **Why it matters:**
  - Going back to an older flow could bring back stages the user thought were gone.
  - Noyan's first proposal after the upgrade would show "new stages" the user never asked for.
- **Sure:** high on the code. Medium on how often users will hit it.
- **Question:** Is the split done every time a flow is read (stored versions, Back to the previous flow, files, proposals), and never written as its own version?
  - *Recommended:* yes. Split on every read, and save it only with the user's next change.
  - *Alternative:* split once at upgrade, as a new stored version "Split by Otaq".

### G9. OpenCode hasn't been checked end to end

- **Decision:** NSC S2 and S3 (OpenCode's "Load the skill `<name>`…").
- **Evidence:**
  - T10 checks 1, 3, 4 and 5 were "not checked: quota" for OpenCode (`ideas/no-skill-copies/T10-report.md`, Results table).
  - 12 of the 13 shipped skills carry `disable-model-invocation: true`. OpenCode loads a skill only when its model calls its skill tool, so whether OpenCode honours the flag decides whether any Otaq skill loads there.
  - OpenCode has no namespace, so a user's `work-ticket` may win (NSC "Known gaps").
- **Why it matters:** if OpenCode won't load a flagged skill, every OpenCode session runs without its stage's skill. That is the quiet failure NSC and NFA aim to prevent.
- **Sure:** medium. It is unverified, not known broken.
- **Question:** Does the release wait for an OpenCode hands-on run of T10 checks 1, 3, 4 and 5?
  - *Recommended:* yes, on a tool with quota (another provider in OpenCode).
  - *Alternative:* ship, and mark OpenCode as "skills not checked" in the release notes.

---

## Confuses users

### G10. The Open PR error and skill still send the user to agent skills

- **Decision:** NSC S4 and S5.
- **Evidence:**
  - `crates/otaq-core/src/pr/service.rs:219`: "use your own Open PR skill (add it to the agent's skills in Settings → Agents)".
  - `plugin/skills/open-pr/SKILL.md:46-47`: "…or add it to the agent's skills in Settings → Agents".
  - The PR provider row's link opens Settings → Agents (`crates/otaq-desktop/ui/src/screens/settings/Rows.tsx:273-274`), where there is no skill field any more.
- **Why it matters:** a GitLab or Bitbucket user follows the error and finds nothing to add.
- **Sure:** high.
- **Question:** Where should "use your own Open PR skill" send the user?
  - *Recommended:* the flow editor, open on the Delivery stage's Skill field, with the error naming that field.
  - *Alternative:* text only: "name your skill as the Delivery stage's skill in the flow", with no link.

### G11. The docs and the website still describe fallbacks, Developer/Validator rows and role defaults

- **Decision:** NFA, "Decision".
- **Evidence:**
  - `docs/how-it-works.md:93-95`: "Limits per tool, with fallbacks… The banner offers **Move waiting to…** … **Use fallback agents automatically**".
  - `docs/how-it-works.md:84`: "Every ticket has a **Developer** and a **Validator** row".
  - `docs/how-it-works.md:87`: "Defaults per role, overridable per project". The code resolves the stage's agent or the ticket's choice instead (`crates/otaq-core/src/agents/resolve.rs:1-5`).
  - `website/src/components/Agents.astro:179-185`: "**Use fallback now** moves them to another".
  - The website demo has `useFallback` (`website/src/demo/state.ts:629-633`).
- **Why it matters:** the docs promise a way around limits that will no longer exist, and per-ticket rows that don't exist now.
- **Sure:** high.
- **Question:** Should the docs' "Agents: any tool, any model" section and the website's limits card be rewritten in the same change as NFA?
  - *Recommended:* yes, in the same change. Describe agents per stage, per-ticket choices, and "only that tool's agents wait".
  - *Alternative:* the docs now, the website at the next website release.

### G12. A ticket's agent can still move a later stage to another tool on its own

- **Decision:** NFA, "Why", and OCA's principle. This sits across decisions.
- **Evidence:**
  - "A ticket's own agent may pick another agent (or `required`) only with a reason… for a **later** stage" (`stage_choices.rs:10-12`).
  - The docs give the example "a stronger Validator when the change turned out risky" (`docs/how-it-works.md:89-91`).
  - NSC "Known gaps" names this case: "When a ticket picks another agent for that stage on another tool".
- **Why it matters:**
  - This is a fallback by another name: work moves to a tool that may lack the user's skills, `CLAUDE.md` or hooks, chosen by an agent.
  - It's also a stage feature (`options`) that may not pass "as simple as possible".
- **Sure:** high on the code. Whether it is a conflict is a judgement.
- **Question:** May a ticket's own agent still pick another agent for a later stage?
  - *Recommended:* no. Only the user, and Noyan passing on the user's words, set stage choices.
  - *Alternative:* yes, but only an agent on the same tool as the stage's own agent.

### G13. A model the tool no longer lists isn't covered

- **Decision:** NFA, "Decision", which covers only "its usage limit or is logged out".
- **Evidence:**
  - There are three reasons to wait: `ToolLimit`, `ToolUnavailable` and `ModelUnavailable` (`crates/otaq-core/src/agents/mod.rs:99-106`).
  - For Model unavailable the callout offers "Give {name} another model in Settings → Agents, or use the fallback" (`crates/otaq-view/src/view_models/tool_problems.rs:350-360`).
- **Why it matters:** once fallbacks are gone, the only way out is Settings → Agents, which changes the model for every ticket.
- **Sure:** high.
- **Question:** When an agent's model disappears from its tool's list, do its tickets just wait until the user gives the agent another model?
  - *Recommended:* yes, with Open agent… as the one action (plus G1's per-ticket change).
  - *Alternative:* Otaq starts it on the tool's default model and says so on the card.

### G14. A tool that is removed, hidden or needs an update makes its tickets wait forever

- **Decision:** NFA, "Decision": "When a tool hits its usage limit or is logged out, its agents wait".
- **Evidence:** `ToolUnavailable` covers "not installed, logged out, hidden, update needed" (`agents/mod.rs:102-103`). Only "logged out" is named in NFA.
- **Why it matters:**
  - A user who uninstalls or hides Codex gets tickets that never start. Nothing resets, unlike a limit.
  - On Windows, a moved tool folder (an npm prefix change) looks like "not installed".
- **Sure:** high on the code. The Windows case is inferred.
- **Question:** When a tool is not installed or hidden, do its tickets wait like a logged-out tool?
  - *Recommended:* yes, they wait. The card says why ("Codex isn't installed") and offers G1's per-ticket change.
  - *Alternative:* hiding a tool is refused while a flow stage's agent uses it.

### G15. Tickets and settings that already use a fallback at upgrade

- **Decision:** NFA. It has no "Moving over" section, unlike NSC S8 and OCA.
- **Evidence:**
  - The stored setting `agents.fallback` (`crates/otaq-core/src/store/settings.rs:22`).
  - Holds with `fallback_asked` (`crates/otaq-core/src/agents/store.rs:452`).
  - Sessions running on a fallback, with the amber line "On Codex · Claude Code limit".
  - History lines "Started on Codex · Claude Code limit".
- **Why it matters:** a user who clicked Use fallback before the upgrade could see the ticket start on the fallback afterwards, or lose the click without a word.
- **Sure:** high on the stored data. The behaviour after upgrade is unknown.
- **Question:** At upgrade, what happens to a ticket the user already sent to a fallback but that hasn't started there yet?
  - *Recommended:* it waits for its own tool again, like every other ticket, with no notice, as S8 does for skills. Running sessions finish where they are, and History keeps its old lines.
  - *Alternative:* it still starts on the fallback once, and then the setting is gone.

### G16. Noyan's "New agent on …" picks the fallback's tool

- **Decision:** NFA, "Decision".
- **Evidence:** when the Project agent's tool can't run, "New agent on …" starts "the Project agent role's agent, else the first agent on the tool's fallback, else the first on another tool" (`crates/otaq-view/src/view_models/tool_problems.rs:418-441`, `agent_panel.rs:20-24`).
- **Why it matters:**
  - Noyan is a live conversation, so "wait for reset" hurts more there than on a ticket.
  - The decision doesn't say whether this by-hand button stays.
- **Sure:** high.
- **Question:** Does Noyan keep "New agent on …" when its tool is at its limit?
  - *Recommended:* yes. It's the user's click and starts a new conversation. It picks the Project agent default, else the first agent on a tool that can run.
  - *Alternative:* remove it, so Noyan waits like every other agent.

### G17. A skill typed into a session that is already running isn't checked

- **Decision:** NSC S5 (F1).
- **Evidence:**
  - Only a new session's first prompt is checked (`crates/otaq-core/src/start/sequence.rs:996-1050`, `crates/otaq-core/src/board/missing_skill.rs:11`).
  - Open PR typed into the live Dev session is never checked (`pr/service.rs:851`), nor is a queued return with `DeliveryKind::Skill` (`start/sequence.rs:863`).
- **Why it matters:** a Delivery stage naming a missing `my-pr` shows no warning, and the agent is still reminded to carry on without it.
- **Sure:** medium-high.
- **Question:** Does a stage's skill typed into a session that's already running get the same check, warning and held-back reminder as a new session?
  - *Recommended:* yes, the same in both cases.
  - *Alternative:* no. Delivery skills are checked only in the flow editor.

### G18. The finder reads the tools' folder variables from the service's environment

- **Decision:** NSC S6 / F2: "Each tool's variable is followed as the tool follows it".
- **Evidence:** the finder takes `CLAUDE_CONFIG_DIR`, `CODEX_HOME`, `COPILOT_HOME` and `XDG_CONFIG_HOME` from the service process (`crates/otaq-core/src/skills/found.rs:106-107` → `agent_tools::process_var` = `std::env::var_os`, `crates/otaq-core/src/agent_tools/mod.rs:286`).
- **Why it matters:** on macOS, a service started from the app doesn't see variables set in `~/.zshrc`. The finder then looks in `~/.claude/skills` while the tool uses another folder, and gives false warnings (which, through G6, stop the agent).
- **Sure:** medium. I didn't check whether the service takes in the login shell's environment.
- **Question:** Should the finder use the same environment Otaq gives the tool when it launches it?
  - *Recommended:* yes, the launch environment.
  - *Alternative:* the service's own environment, with the folder it read shown in "Pick a skill…".

### G19. The flow skill and flow files still allow several check agents

- **Decision:** OCA, "Decision".
- **Evidence:**
  - `plugin/skills/flow/SKILL.md:184`: "Its check agents, by name, one or more: `[Validator]`, `[Security reviewer, Performance reviewer]`. They run side by side…". Also `:187` and `:275`.
  - The flow check says "name one or more" (`crates/otaq-core/src/flow/check.rs:133-141`).
  - The flow editor's rule: "Passes when every check agent passes…" (`crates/otaq-view/src/view_models/flow_editor.rs:1705`).
  - The website: "Several check agents can run side by side." (`website/src/components/Flow.astro:17`), and the demo's "2 check agents · both passed" (`website/src/demo/view.ts:637`).
- **Why it matters:** Noyan follows the flow skill, so it will keep proposing flows that OCA removes.
- **Sure:** high.
- **Question:** After OCA, does Otaq refuse a flow file that lists two check agents, or split it?
  - *Recommended:* split it on read, as OCA says for stored flows, and tell Noyan's flow skill to write one stage per check.
  - *Alternative:* refuse it with "a check stage has one agent: make two stages".

### G20. The flow file keeps the list form `agents: [Validator]`

- **Decision:** OCA, "Decision".
- **Evidence:**
  - All four presets write `agents: [Validator]` (`crates/otaq-core/flows/*.yaml`).
  - An agent check refuses `agent:`: "an agent check lists its agents: `agents: [X]`" (`crates/otaq-core/src/flow/yaml.rs:281-284`).
  - An Agent stage uses `agent:`.
- **Why it matters:** a list that may hold only one name invites the old mistake, and it's a stage feature that is different for no reason.
- **Sure:** high.
- **Question:** Does a check stage name its agent as `agent:`, like an Agent stage?
  - *Recommended:* yes. Write `agent:`, and still read `agents: [X]` (one name) and `agents: [X, Y]` (split).
  - *Alternative:* keep `agents:` as the file format, limited to one name.

### G21. Split names can clash with existing stages, and every copy adds a board column

- **Decision:** OCA, "Moving over" (the name `<stage> · <agent>`; each copy keeps "skill, rounds, 'fails go to' and options").
- **Evidence:**
  - Two stages with one name are refused: "Two stages are named …" (`crates/otaq-core/src/flow/check.rs:120-128`).
  - The decision doesn't say what the copies' `column` and `why` are. With `column: own`, each copy becomes its own board column.
- **Why it matters:** an unlucky name makes the split flow fail its own check. A board can also grow columns the user never added.
- **Sure:** medium. The clash is rare. The extra columns are certain with `column: own`.
- **Question:** Do split copies after the first share the first copy's column?
  - *Recommended:* yes (`with-previous`), so the board looks as it did. A name clash adds " 2".
  - *Alternative:* each copy keeps `column: own`, one column per check.

### G22. Checks now run one after another: slower, and the second check's findings come later

- **Decision:** OCA, "Moving over" ("Every check the user set up still runs, now in turn").
- **Evidence:** today the check agents start together and their reports return together (`crates/otaq-core/src/validation/mod.rs:33-37`, `CHECK_AGENTS_RULE`). After the split, a fail at the first copy goes back to Dev before the second copy runs.
- **Why it matters:**
  - A user with Security and Performance checks gets more rounds and a longer wait.
  - The second check's findings arrive one round later.
  - NSC S8 chose "no notice", and OCA says nothing about a notice.
- **Sure:** high on the behaviour. The impact depends on how many users have several check agents (none of the presets do).
- **Question:** Are users told once that their parallel checks now run in turn?
  - *Recommended:* only in the release notes, with no notice in the app.
  - *Alternative:* a one-time note on the flow editor of each project whose flow was split.

### G23. Old rounds with several check agents in History and "Before you"

- **Decision:** OCA, "Removed: … the card's row per check agent and the combined verdict".
- **Evidence:**
  - One `validations` row per agent per round, grouped by `round_id` (`crates/otaq-core/migrations/0064_check_agents.sql`).
  - "Before you" shows "2 passed · 1 failed" (`crates/otaq-view/src/view_models/validation_report.rs:518-555`).
  - Holds are keyed by `check_agent` (`0065_check_agent_holds.sql`).
- **Why it matters:** a ticket in Review at upgrade has a combined round. Its report has to keep reading sensibly.
- **Sure:** high on the data. What the decision wants is unclear.
- **Question:** Do rounds stored before the split still show as one round with every agent's report?
  - *Recommended:* yes, read-only and as they are. Only new rounds are one agent per stage.
  - *Alternative:* old rounds are shown split, one per agent, like new ones.

### G24. The "as simple as possible" principle meets other special cases

- **Decision:** OCA, "Principle".
- **Evidence:** stage features that apply to only one kind or one situation:
  - `options` per stage (`stage_choices.rs`);
  - a skippable human check skipped on its own when every Agent stage before it was skipped (`stage_choices.rs:24-27`);
  - skipping a human check forces a click Delivery;
  - `delivers: either` is click only (`crates/otaq-core/flows/otaq-default.yaml`, `presets.rs:67`);
  - `column: with-previous`;
  - Skip this check only at the plan limit (G3);
  - Move on anyway.
- **Why it matters:** OCA states a principle but applies it to one feature. Builders will meet these next.
- **Sure:** high that they exist. Whether each is "truly necessary" is a judgement.
- **Question:** Is the principle applied now to the existing stage features, through a separate review of each one?
  - *Recommended:* yes, a separate review after these three decisions ship, starting with stage `options` (see G12).
  - *Alternative:* the principle applies only to new features.

### G25. Codex refreshes Otaq's plugin only when its skills change

- **Decision:** NSC S2 (Codex gets the skills through the installed `otaq@otaq-local` plugin).
- **Evidence:** the stale check compares only `skills/` (`crates/otaq-core/src/agent_tools/codex/setup.rs:518-527`). The manifest version is fixed at `0.1.0`.
- **Why it matters:** an update that changes only the hooks or the manifest leaves Codex on the old copy, with no error. It only works on the other tools. It is the same kind of problem as T10 bug 1.
- **Sure:** medium.
- **Question:** Should Codex's installed plugin copy be refreshed whenever any file of Otaq's plugin changes?
  - *Recommended:* yes, compare the whole plugin. It re-records hook trust as setup does now.
  - *Alternative:* refresh only on a new Otaq version.

### G26. `otaq prime --validate` counts rounds per ticket, not per check stage

- **Decision:** OCA, "Decision" ("each with its own … rounds").
- **Evidence:** prime uses `ticket.validation_round + 1` (`crates/otaq-core/src/prime.rs:687-800`). Counters are per `(ticket, stage)` (`0051_check_counters.sql`).
- **Why it matters:** with two check stages in a row, the second one's validator may read "round 3" on its first run.
- **Sure:** medium. I didn't trace how `validation_round` is reset between stages.
- **Question:** Is the round a validator sees always its own stage's round?
  - *Recommended:* yes, per stage.
  - *Alternative:* one round count per ticket, across every check.

### G27. CLI flags and protocol fields for "the check has several agents"

- **Decision:** OCA, "Decision".
- **Evidence:**
  - `otaq attach --agent` and `otaq show --terminal --stage --agent` are for "when the check has several" (`crates/otaq-cli/src/cli.rs:134,160,958`).
  - The protocol's `UseFallbackParams.agent`, `TerminalParams.agent` and "Validate has 2 check agents; add --agent" (`crates/otaq-core/src/protocol.rs:1760-1781,1909`).
- **Why it matters:** scripts and agents that pass `--agent` will break, or the flag stays as dead weight.
- **Sure:** high.
- **Question:** Is `--agent` removed from `otaq attach` and `otaq show`, or kept and ignored?
  - *Recommended:* kept for one release, ignored, with a note "a check has one agent".
  - *Alternative:* removed now. A passed `--agent` is refused.

---

## Cosmetic

### G28. "Open skill folder →" opens a folder that changes with every build

- **Decision:** NSC S2 and S6.
- **Evidence:**
  - "What this agent reads" opens `<runtime>/<build>/plugin/skills/<name>` (`crates/otaq-core/src/agent_reads.rs:160-173`).
  - A user's stage skill shows no text and no folder, although the finder knows where it is (`agent_reads.rs:160-163`).
- **Why it matters:** a user who bookmarks or edits that folder loses it at the next update. For their own skill they see nothing.
- **Sure:** high.
- **Question:** Should "What this agent reads" show a user's stage skill from where the finder found it?
  - *Recommended:* yes, with its text and Open skill folder → to the user's own folder.
  - *Alternative:* name it only ("Skill: team-validate · your own").

### G29. Leftover code from imported skills

- **Decision:** NSC S4. The NSC preamble also says to name rules in words.
- **Evidence:**
  - Unused refusals `BadSkill`, `OriginalNotFound` and `NotImported` (`crates/otaq-core/src/board/mod.rs:285-298`).
  - Doc comments saying skills are copied: `crates/otaq-core/src/agent_tools/codex/launch.rs:12-14`, `pr/service.rs:137-139`, `agent_tools/mod.rs:910`, `project_agents/first_flow.rs:5`.
  - `agents.list` mentions "the move-over's notice" (`protocol.rs:985`), which S8 rules out.
- **Why it matters:** contributors are misled. Users don't see any of it.
- **Sure:** high.
- **Question:** Is this cleanup part of the NSC change, or a later chore?
  - *Recommended:* part of NSC, in one small task.
  - *Alternative:* a later chore ticket.

### G30. T10 bug 2 (service refuses to start) affects only builds from before the release

- **Decision:** NSC S8 (moving over).
- **Evidence:**
  - The edit to `0071_app_ports.sql` (`31c6b06a`) is already in tag `v0.1.0`. I checked this with `git merge-base --is-ancestor`.
  - Installs of v0.1.0 or later have the edited checksum. Only dev builds made before it (like the VM's `1a371563`) fail.
  - There is no checksum repair in `crates/otaq-core/src/store/mod.rs`.
- **Why it matters:** released users aren't affected. The maintainer's own machines and VMs may be.
- **Sure:** high.
- **Question:** Does Otaq need to repair checksums of migrations changed before v0.1.0?
  - *Recommended:* no. Released installs are fine. Document the one-line fix for dev data folders.
  - *Alternative:* yes, accept a known list of old checksums for edited migrations.

### G31. NSC's Known gaps text promises a warning G4 shows doesn't come

- **Decision:** NSC, "Known gaps, accepted", third bullet: "that agent doesn't see the skill; the card warns (S5)".
- **Evidence:** see G4 (`crates/otaq-core/src/board/mod.rs:146-162`).
- **Why it matters:** the record of the decision is wrong today. If G4 is answered with "keep one answer per project", this text must change.
- **Sure:** high.
- **Question:** Does the Known gaps line keep "the card warns"?
  - *Recommended:* keep it, and make it true through G4.
  - *Alternative:* change it to "nothing warns; commit the skill for both tools if you mix tools".
