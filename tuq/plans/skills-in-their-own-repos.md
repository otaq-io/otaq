# Plan · skills-in-their-own-repos

- **Ticket:** none
- **Date:** 2026-11-14
- **Decisions:** 057, 062, 063, 064, 065, 069, 071, 072, 073, 074, 075, 077, 079, 080, 081, 082, 083, 089, 090, 092, 093, 094, 098, 100, 102, 103, 104, 105, 106, 107, 108, 109
- **Tech spec:** tuq/tech-specs/skills-in-their-own-repos.md
- **Orchestrator prompt:** tuq/prompts/skills-in-their-own-repos-orchestrator.md

<!-- tuq:plan:start -->

Otaq's side is built in eight pieces, one behaviour each. Moving the workflow skills into their submodule (decision
102, tech spec §1) and renaming flows to `otaq:`/`tuq:` names (decisions 063, 075) come first, side by side: the
rest builds on one or both. Running `tuq:` stages on the user's tuq needs the new names and a published tuq release
(plan tuq/plans/tuq-on-its-own.md, piece 4); the two-skill stage start, the prefixed copies, the picker and
About/History follow from there, side by side where they can. The docs come last. The agent in charge follows
tuq/prompts/skills-in-their-own-repos-orchestrator.md.

## Pieces

1. Workflow skills come from the otaq-skills repository — waits for nothing
   Tech spec §1. A new local repository `otaq-skills` (beside this one, to become `otaq-io/otaq-skills`) holds
   `plugin/skills/` with its history, as one plugin named `otaq` (`.claude-plugin/plugin.json`, `skills/<name>/`,
   decision 057). This repository gets it back as a git submodule at `plugin/skills/`, pinned to a commit. Every
   place that builds or checks Otaq initialises the submodule: `scripts/check.sh`, `scripts/check.ps1`,
   `scripts/install-local.sh`, `scripts/release/`, CI, CONTRIBUTING's clone line (`--recursive`). The runtime copy
   (`crates/otaq-core/src/runtime.rs`) is unchanged: the folder is where it was. `product-spec`, `tech-spec` and
   `milestone-plan` stay for now; piece 3 removes them. Creating the GitHub repository and pushing are the
   maintainer's: the piece leaves the submodule's URL pointing at `https://github.com/otaq-io/otaq-skills.git` and
   says so.
   Check: a fresh `git clone --recursive` of this branch builds and `scripts/check.sh` passes; the runtime copy's
   `claude/skills/` holds the same skills as before.
2. Flows name Otaq's skills as otaq: and tuq: — waits for nothing
   Decisions 063, 074, 075, 065. Once per project, at the first start of this version, a new flow version written
   by Otaq (History: Otaq's rename, not the user's change) turns each bare `work-ticket`, `validate`, `commit`,
   `open-pr`, `push`, `address-review`, `restack`, `answer-question`, `noyan`, `flow` into `otaq:<name>` and
   `product-spec`, `tech-spec`, `milestone-plan` into `tuq:<name>`, the delivery's `skill:` included; Back to the
   previous flow and importing an older flow file apply the same rewrite. The shipped flows, presets and Edit flow's
   ready stages use the namespaced names, and Test in the browser leaves the ready stages (decision 065). From then
   on a bare name is always the user's skill: the rule "a name matching one of Otaq's shipped skills means Otaq's"
   goes from `crates/otaq-core/src/skills/` (mod.rs, found.rs) and `agent_tools/mod.rs` (`is_own_skill`), and each
   tool's `skill_reference` types `otaq:<name>` in its own form (`/otaq:validate`, `$otaq:validate`). Code: flow
   store, presets and model in `crates/otaq-core/src/flow/`. Until piece 3, `tuq:<name>` runs the copy still in
   Otaq's plugin.
   Check: a 0.2 flow with Otaq's and custom names (`team-validate`) migrates once, keeps the custom names, and its
   tickets start; Back and import give the same names; a flow naming a bare `validate` with no such user skill is
   held as missing.
3. tuq: stages run on the user's tuq, installed when missing — waits for 1, 2
   Decisions 102, 103, 104, 100, 083, 077, 106; tech spec §3, §5. Needs a published tuq release (plan
   tuq-on-its-own, piece 4). A new `crates/otaq-core/src/tuq/mod.rs` (`TuqInstalls`): `ensure(tool, "tuq:<name>")`
   before a stage starts or a `tuq:` skill is typed into a running session; it finds the tool's tuq through the skill
   finder (`skills/found.rs`: tuq installed as a plugin, or on OpenCode tuq's install, by the
   `metadata.tuq-release` stamp; loose or repository copies are the user's own skills, decision 100), runs tuq's
   installer for that tool when the skill is missing (`install.sh`/`install.ps1 --tool <tool>`, one install at a
   time), checks again, and records "tuq updated from … to …" in the ticket's History. Still missing or the
   installer failed: the stage holds in Needs you with "Your tuq doesn't have `tuq:<name>`" (`agents/gate.rs`,
   `board/missing_skill.rs`). Every launch puts `~/.tuq/bin` (`%LOCALAPPDATA%\tuq\bin`) on the session's PATH and
   pre-approves `tuq` and `tuq-plan` like `otaq` on all four tools (`agent_tools/*/launch.rs`, Codex's
   `rules/otaq.rules`). The session record stores the tuq version it ran (for piece 7). `product-spec`,
   `tech-spec` and `milestone-plan` leave the workflow-skills submodule. One behaviour across the core, the launches
   and the hold, on purpose.
   Check: with no tuq on Claude Code, a Product spec stage installs tuq and runs `/tuq:product-spec`; with an older
   tuq lacking a skill, it updates and runs; with the installer failing (offline), the stage holds with the message;
   `tuq next` runs in the session without a permission prompt.
4. A stage runs Otaq's workflow skill, then its task skill — waits for 1, 3
   Decisions 081, 089, 093, 098; tech spec §4. The stage starts with `/otaq:work-ticket`; when that turn ends, Otaq
   types the stage's task skill (`/tuq:product-spec`, or the user's own) as a hand-off (`crates/otaq-core/src/start/`,
   `session/hand_offs.rs`). In the workflow skills (the submodule from piece 1): `work-ticket` says how the task
   skill's steps meet Otaq (questions through `otaq ask human`, committing at the stage's end, pushing only when the
   user asks, ending with `otaq stage next`: decision 098), copies the plan file's Plan onto the ticket at the end of
   Milestone plan and the ticket's Plan back into the file when the ticket returns (decision 093), and carries the
   rule "never start a skill on your own initiative; a skill that was started may call the skills it names"
   (decision 089). Skills name each other namespaced, per tool (decision 074). One behaviour across the start code
   and the skills' text, on purpose.
   Check: a Product spec stage on Claude Code and on Codex runs both skills, asks through the ticket, commits and
   moves to Tech spec; a Milestone plan stage leaves the plan file's Plan on the ticket; describing a feature in
   Noyan's chat starts no skill.
5. Copilot and OpenCode run Otaq's skills as otaq-<name> — waits for 1, 2
   Decisions 092, 079, 072. The runtime copy (`crates/otaq-core/src/runtime.rs`) also writes the prefixed copy of
   the workflow skills (`otaq-<name>` folders and `name:`, references to other Otaq skills rewritten; real files, no
   links) and fills Copilot's plugin with it; OpenCode's `skills` path names it
   (`agent_tools/opencode/launch.rs`). Copilot's `skill_reference` gives the form Copilot runs; find it hands-on in
   the Windows VM first (`/otaq-validate` or `/otaq:otaq-validate`). Messages about a Copilot or OpenCode stage name
   that form.
   Check: in the Windows VM, a repository with its own `.github/skills/validate` still runs Otaq's Validate stage on
   Copilot; OpenCode runs `otaq-validate` interactively.
6. Pick a skill… shows Otaq's skills in their own group — waits for 2, 3
   Decision 108. In the flow editor's Pick a skill…, the user's own skills come first, unmarked; below them a group
   headed "Otaq" with Otaq's icon lists `otaq:` and `tuq:` skills by their namespaced names, each with an "Otaq"
   badge, also while searching. tuq's skills count whoever installed tuq (decision 104).
   `crates/otaq-desktop/src/api/flow_editor.rs` (the picker's items gain the group and badge),
   `crates/otaq-desktop/ui/src/screens/flow_editor/` and `ui/src/components/Menu.tsx`; one behaviour across API and
   page, on purpose.
   Check: the picker shows the group and badges with and without a search; a personal `validate` sits above,
   unmarked.
7. About and History name the tuq that ran — waits for 3
   Decisions 069, 100, 102. Settings → About shows the app version (which includes Otaq's workflow skills) and tuq's
   version: one line when every connected tool has the same, otherwise per tool ("tuq 2026.11.12; Claude Code:
   2026.11.10"). Each session row in a ticket's History names the tuq version it ran, from the session record (piece
   3). `crates/otaq-desktop/src/about.rs`, `crates/otaq-desktop/src/api/sections.rs`.
   Check: About with tuq on two tools at different versions shows both; a Product spec session's History row names
   its tuq version.
8. The docs name skills the new way — waits for 4, 5
   README, `docs/` and `website/` say `otaq:` and `tuq:` (each tool's typed form where it matters), that Otaq's
   workflow skills come with the app, and that Otaq installs tuq when a stage needs it (link to tuq's README).
   Screenshots that show a skill name are left for the readme-screenshots skill.
   Check: `grep -rn "/validate\b\|/work-ticket\|otaq-claude-skills" README.md docs website/src` finds only intended
   uses.

<!-- tuq:plan:end -->
