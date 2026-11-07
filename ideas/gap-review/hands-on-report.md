# Hands-on report: stage skill per tool

Run on 2026-11-06 on branch `no-skill-copies-G16` at `bb67f36c`, with all 15 pieces merged. It covers all four tools:
Claude Code on the Mac (part A), and Codex, GitHub Copilot and OpenCode in the Windows VM (part B). Each part used a
throwaway data folder and throwaway repos. No VM snapshot was taken.

## Summary

| Check | Claude Code | Codex | Copilot | OpenCode |
|---|---|---|---|---|
| Dev starts with Otaq's skill; nothing is written into the worktree | pass | pass | pass | pass |
| A personal skill only in another tool's folder holds the stage; Check again after adding it starts it | pass (with Codex as a stand-in on the Mac) | pass, except a SKILL.md with a BOM (finding 1) | pass | pass |
| Change agent… offers only agents whose tool has the skill | pass (finding 2) | pass | pass | pass |
| A limit makes only that tool's agents wait | pass (limit simulated) | pass | pass | pass (also a real Google rate limit) |
| A flow with several check agents opens split | pass | pass | pass | pass |
| The flow editor's agent picker follows the skill | pass | | | |

Open question settled: Copilot CLI doesn't load skills from `~/.claude/skills`. A probe skill there was unknown to it,
and the same skill in `~/.copilot/skills` ran.

## Findings

1. **A SKILL.md that starts with a UTF-8 BOM counts as Codex's, but Codex doesn't load it.** Windows PowerShell 5's
   `Set-Content -Encoding utf8` writes a BOM. Check again started the stage and typed `$team-validate`, and Codex
   searched the repository for the skill instead. Without the BOM it worked. OpenCode loads such a file.
2. **A round that Change agent… moved to another agent is stored under the held agent.** On the Mac, round 1 was run
   and passed by Validator on Claude Code, but `ticket.validations` names "Codex checker". The comment author and
   `session.list` are right. History and "Before you" would show the wrong agent.
3. **No warning when a flow names a skill its agent's tool doesn't have.** `flow propose` and Apply gave no problem and
   no warning for such a stage, on both machines. The editor wasn't opened on that version.
4. **A held check is described as running.** `otaq stage next` answers "Validate is running", and the sheet shows
   "Validating · round 1 of 1", while the line below says "Nothing started". The held sheet also offers
   "Open terminal · Validator" although no session exists.
5. **Log noise.** For each held ticket, the gate logs "the tool doesn't have the stage's skill" at INFO every 1–2
   seconds.
6. **Minor UI.** When two check stages share a column, the first one's name shrinks to "V." in the header. Once, the
   flow editor's preview put a ticket under Validate while it was in Review. This was seen once and not checked again.
7. **Environment, not code.** Starting a second data folder's service in the VM ran Codex setup, which pointed
   `~\.codex\config.toml`'s `otaq-local` marketplace at the test build's plugin. The installed Otaq in the VM should
   have its Codex setup run again.

A skill in `~/.claude/skills` counts for OpenCode as well as Claude Code. That matches what OpenCode does.

## Not covered

- Codex was a stand-in on the Mac; it was tested for real only in the VM.
- Claude Code's limit was simulated in the database, not reached for real or caused by a logout.
- The split of a check with several agents was tested only through `flow propose` and Apply. Stored versions and Back
  were not tried. The automated tests cover them.

The full notes from each machine follow.

---

## Part A: the Mac (Claude Code)


Build: `no-skill-copies-G16` at bb67f36c (service build `bb67f36cafad`, protocol 8), from
/Users/nevzattopcu/Desktop/Workspace/otaq-nsc-G16. Throwaway `OTAQ_HOME` and repo `tinyrepo` (project TINY) under
.../scratchpad/hands-on-mac/. The service was started with `env -i`, and two extra settings:
- `CODEX_HOME` = a throwaway folder (.../hands-on-mac/codex-home), so Codex's "personal folder" was never ~/.codex.
- a stand-in `codex` script on its PATH (Codex isn't installed on the Mac). It answers `--version`, `login status`
  and `plugin …`, logs every call, and on a launch prints its arguments and then runs `cat`. With it, Otaq sees Codex as
  connected, so a Codex agent gets past the "tool available" check and reaches the skill check. Without it, the
  stage would be held as "Codex isn't available · not installed" before the skill is checked.

Real Claude Code 2.1.319 ran four short sessions: Dev TINY-1, Dev TINY-2, Validate TINY-2 and Validate TINY-3. Otaq's
own `validate` also checked TINY-1. Every session was stopped at the end.

| # | Check | Result | What was seen |
|---|---|---|---|
| 1 | Dev session starts with Otaq's skill; nothing written into the worktree | **pass** | The Dev agent was launched as `claude --plugin-dir …/runtime/<build>/plugin/claude … /otaq:work-ticket`. Its transcript has `<command-name>/otaq:work-ticket</command-name>`, and it loaded `Skill(otaq:commit)`. In the worktree, `git status --ignored` shows nothing, and there is no `.claude` or `.agents` folder. |
| 2 | Personal skill in another tool's folder holds the stage; Check again starts it | **pass** (Codex was a stand-in) | Validate was set to `skill: team-validate` with agent "Codex checker" (model `codex`). team-validate was only in ~/.claude/skills; `skills.list` gave its tools as `[claude, opencode]`. After Dev, the ticket stayed in Validate. Needs you showed "Codex doesn't have the skill `/team-validate`", and the card read "Codex checker can't start: missing skill". The sheet offered Check again and Change agent…. No Codex launch was logged and no validator session existed. Check again in the GUI before linking: still held. After `ln -s ~/.claude/skills/team-validate $CODEX_HOME/skills/`, `skills.list` gave `[claude, codex, opencode]`. Check again then lifted the hold and launched `codex --no-daemon -c check_for_update_on_startup=false $team-validate`, and the sheet showed Codex checker · Codex as the assignee. |
| 3 | Change agent… offers only agents whose tool has the skill; a Claude Code pick starts it on Claude Code | **pass** (one bug, below) | The link was removed and the ticket held again (resumed after the stand-in was killed). The dialog said "only agents whose tool has Validate's skill and can run now are listed". It offered Developer (pre-selected), and `ticket.choices` gave Validate exactly Developer/claude and Validator/claude, with no Codex checker. `--stage "Validate=Codex checker"` (CLI and request) was refused: "Codex checker runs on Codex, which doesn't have the skill `/team-validate` for Validate". Picking Validator launched `claude … /team-validate` on Claude Code, and it passed: "team-validate: README.md:4 adds only 'Second line.' …". The pick was sent through `ticket.set_choice`, the request the dialog sends, because GUI clicks weren't possible at that moment (see notes). |
| 4 | A limit makes only that tool's agents wait | **pass** | Claude Code's limit was simulated with an `active_limits` row in the throwaway DB, which is what the tests' `record_hit` writes. The banner read "Claude Code limit reached · Resets 1:41 pm. 1 ticket waiting". TINY-3's Dev ran on the Codex stand-in, as the user picked, and the limit didn't stop it. Its Validate (Validator, team-validate) went to `waiting_for_limit_reset` with a `tool_limit` hold on claude. The card read "At limit · Claude Code limit · resets 1:41 pm", and the sheet read "Nothing started and nothing is locked. Validator starts when you resume Claude Code after its reset". The GUI's Change agent… listed no agents: "No other agent whose tool has Validate's skill can run now." There was no Codex or Copilot entry, and no Codex launch in about 2 minutes. `limits.resume_tool claude` then started Validate on Claude Code (`/team-validate`), and it passed. |
| 5 | A flow with two check agents opens split | **pass** | `otaq flow propose -f` was given a file whose Validate has `agents: [Validator, Security reviewer]`. The proposal shows `validate` (agent Validator) and `validate-security-reviewer`, "Validate · Security reviewer", `column: with-previous`, with the same skill and rounds. The diff reads "stage Validate · Security reviewer (agent check) after Validate". After Apply, `otaq flow show` matched, and the board shows one column holding both stages, "V. 0 · Validate · Security reviewer 0". |
| 6 | Flow editor's agent picker follows the skill | **pass** | Change flow… was opened and Validate selected (skill `team-validate`). The Agent picker listed Noyan, Developer and Validator, all on Claude Code, and not Codex checker. Pick a skill… showed "team-validate · Personal · Claude Code, OpenCode". After switching the skill to Otaq's `validate`, the picker listed Noyan, Developer, Validator and Codex checker. The edit was cancelled. |

### Bugs and oddities

1. **A round run by the agent picked in Change agent… is recorded under the held agent's name.** On TINY-2, `ticket.validations` shows round 1, session `01a1afc9…` (the Claude Code Validator, which passed), with `"agent": "Codex checker"`. The round was opened when the Codex-held check was resumed, and wasn't updated when the pick started it on Validator. In the same ticket, the comment's author is correctly "Validator / claude", and `session.list` says Validator. Anything that reads the round's agent shows the wrong one, for example History and "Before you".
2. **The proposal shows no warning when the applied flow names a skill its agent's tool lacks.** Applying a file with Validate = Codex checker plus team-validate (only Claude Code and OpenCode have it) gave `problems: []` and `warnings: []` in `flow.proposal`. S5 says the flow editor shows "<tool> doesn't have the skill" on the stage. I didn't open the editor on that version, so the editor itself is unchecked. Worth a look.
3. **Minor UI:** with two check stages sharing a column, the first stage's name is squeezed to "V." in the column header ("V. 0 · Validate · Security reviewer 0").
4. **Minor UI, seen once and not verified:** the flow editor's top preview put "TINY-1 · Checking · Validator" under the Validate step, while TINY-1 was in Review and TINY-3 was the ticket in Validate.
5. **Minor:** a held check's sheet shows "Open terminal · Validator" even though no validator session exists. The sheet's Stages list also keeps the stopped Codex round as a second "Validate" row (it reads as history).
6. **Not a bug, for the record:** `ticket.set_choice` Validate = "Codex checker" (the stage's usual agent, whose tool lacks the skill) is accepted, as "the usual choice", and recorded in History as a choice. The same agent is refused when it isn't the usual one. The flow file can still name it, and then the hold applies.
7. **Rule check:** `~/.claude/skills/x` is counted for OpenCode as well as Claude Code. OpenCode really reads `~/.claude/skills`, so this looks intended, but check 6's "only Claude Code agents" holds only because no OpenCode agent existed.

### Notes

- **GUI:** clicks are synthetic CoreGraphics events. Partway through, the maintainer's Terminal came to the front over the dev window, and about six clicks meant for Otaq landed in that Terminal: plain clicks, no keys typed. Nothing visible changed, but please glance at it. After that I clicked only after checking that the dev window was frontmost. I raised it by clicking a thin uncovered edge of its own window. Change agent… for check 3 went through the request the dialog sends. Checks 4 and 6 were done in the GUI. An `osascript` call to raise the window hung, probably on an automation prompt, and was killed. If macOS shows a "Terminal wants to control System Events" prompt, it can be denied.
- **The Claude limit** was simulated through the throwaway DB, not a real limit or logout.
- **Codex never ran for real.** The stand-in shows that the hold is decided before launch, and that Check again lifts it and launches Codex with `$team-validate`.
- **Seen in the agents' own words:** git had no identity in the `env -i` service environment. Both Dev agents said so and committed with a per-commit identity, without changing git config.
- **Cleanup:**
  - Service stopped, with its Claude sessions; dev app and Vite stopped.
  - `~/.claude/skills/team-validate` was added for the run and removed. ~/.codex was never touched.
  - The temporary short-path link /private/tmp/claude-501/hom (used for the socket path) was removed.
  - Left behind by Claude Code itself: trusted-folder and per-folder entries for the throwaway worktrees in `~/.claude.json`, and three transcript folders under `~/.claude/projects/-private-tmp-claude-501--…-hands-on-mac-home-wt-tinyrepo-TINY-{1,2,3}`.
  - The throwaway data stays in .../scratchpad/hands-on-mac/: screenshots in `shots/`, the flow files and the stand-in `bin/codex`.

---

## Part B: the Windows VM (Codex, Copilot, OpenCode)


Run on 2026-11-06, 09:00–09:30 (VM time), branch `no-skill-copies-G16` at `bb67f36c` (all 15 pieces merged). No code
changed, nothing committed, no snapshot taken.

**Setup.** The branch went in as a full git bundle, fetched into `~\otaq-build\repo` as `ssp-g16`, worktree
`%USERPROFILE%\otaq-build\stage-skill-per-tool`, shared target `~\otaq-build\target`. `cargo build -p otaq-cli`
(debug) and later `cargo build -p otaq-desktop` + Vite for one look at the app. The service ran from that build with
its own `OTAQ_HOME=~\ssp\home` and `OTAQ_DEV_CONPTY_DIR`, started through a scheduled task (a service started by
vmpal_exec dies with the job). Three fresh repos `~\ssp\cdx|cpl|opc` (projects CDX, CPL, OPC), each with a flow of
Dev (work-ticket) → Validate (agent check, 1 round) → Review → Done, on agents SSP Codex (`codex`, ran gpt-6-luna),
SSP Copilot (`copilot`, auto model) and SSP OpenCode (`opencode:google/gemini-3.1-flash-lite`). The installed Otaq,
`~\.otaq` and `~\otaq-trial` were not touched (but see note 4). Driven through the service's pipe
(`ticket.move`, `ticket.choices`, `ticket.set_choice`, `ticket.check_again`, `needs_you.list`, `usage.mark_limit`,
`limits.resume_tool`, `session.input` to answer permission prompts) and read back from the tools' own logs (Codex
rollouts, Copilot `session-state\…\events.jsonl`, `opencode session export`). Tools: Codex 0.166.0, Copilot CLI
1.0.105 → 1.0.106 (it updated itself during the run), OpenCode 2.2.5 (Google provider had quota).

### Results

| Check | Codex | Copilot | OpenCode |
|---|---|---|---|
| 1. Dev starts with Otaq's skill; nothing in the worktree's skill folders | **pass** | **pass** | **pass** |
| 2. Personal skill only in another tool's folder holds the stage; Check again after adding it to this tool's folder starts it and the tool runs it by name | **pass** (see finding 1: a SKILL.md with a BOM is counted by Otaq but ignored by Codex) | **pass** | **pass** |
| 3. Change agent… from the held card offers only agents whose tool has the skill; picking one starts the stage there | **pass** (→ SSP OpenCode) | **pass** (→ SSP OpenCode) | **pass** (→ SSP Copilot) |
| 4. A limit makes only that tool's agents wait | **pass** | **pass** | **pass** (also hit a real Google rate limit, see note 3) |
| 5. A flow with several check agents opens split | **pass** (one check for all tools, see below) | ← same | ← same |

**Copilot probe (open question): Copilot CLI 1.0.105 does not load skills from `~\.claude\skills`.** With
`ssp-probe-claude` only in `~\.claude\skills`, `copilot -p` listed only `customize-cloud-agent` and `github-pr-media`;
`/ssp-probe-claude` → "isn't a documented or available Copilot CLI command"; asked to load it with its skill tool →
`✗ skill(ssp-probe-claude) Skill not found`. Control: the same skill in `~\.copilot\skills` ran
(`skill(ssp-probe-copilot)` → `PROBE-COPILOT-DIR-LOADED`). Run from `%TEMP%` (not a git repo). So S6's
"`~/.claude/skills` is Claude Code's, not Copilot's" matches what Copilot does. Both probes removed.

### What was seen, per check

#### 1. Dev session

- **Codex (CDX-1):** first prompt `$otaq:work-ticket`; the rollout injects `<skill><name>otaq:work-ticket</name>
  <path>C:\Users\…\.codex\plugins\cache\otaq-local\otaq\0.1.0\skills\work-ticket\SKILL.md` (Otaq's plugin, the
  0.1.0 cache folder T10's bug 1 was about). It later read `otaq:commit` from the same cache. Done, committed, `stage next`.
- **Copilot (CPL-1):** `user.message` `/otaq:work-ticket`, "Base directory for this skill:
  `…\ssp\home\runtime\bb67f36cafad…\plugin\copilot\skills\work-ticket`"; later `skill(commit)`.
- **OpenCode (OPC-1):** first prompt "Load the skill `work-ticket` with your skill tool and follow it. If you don't
  have a skill with that name, say so and stop."; the export shows `skill {"id":"work-ticket"} -> completed` with the
  skill's text, then `skill {"id":"commit"}`. Both carry `disable-model-invocation: true` (checked in the runtime
  copy), so the shipped flagged skill loads when given as an instruction.
- **Worktrees:** `git status --ignored` empty and only `.git`, `greet.txt`, `README.md` in CDX-1, CPL-1, OPC-1 and in
  the three checkouts: no `.agents`, `.claude`, `.github` or `.opencode` folder.

#### 2. Held for a personal skill in another tool's folder, then Check again

- Skills: `team-validate` only in `~\.claude\skills` (CDX, CPL Validate); `team-check` only in `~\.copilot\skills`
  (OPC Validate, since OpenCode also reads `~\.claude\skills`). `skills.list` said `team-validate` tools =
  claude, opencode; `team-check` tools = copilot. Right per S6.
- After Dev, each ticket stayed in Validate with a `missing_skill` hold and no validator session; `needs_you.list`
  reasons, word for word: "Codex doesn't have the skill `/team-validate`", "GitHub Copilot doesn't have the skill
  `/team-validate`", "OpenCode doesn't have the skill `/team-check`". Nothing typed into any session: the Dev sessions'
  only user messages are the work-ticket prompt (Codex rollout, Copilot events), `ticket.messages` empty.
- Adding the skill to the tool's own folder didn't start anything by itself (checked 12 s later); `ticket.check_again`
  then started the validator at once:
  - **Codex:** `~\.codex\skills\team-validate` → session typed `$team-validate`, the rollout injects the skill from
    `C:\Users\…\.codex\skills\team-validate\SKILL.md`, Codex said "TEAM-VALIDATE RAN" and passed. (First try failed
    because of finding 1.)
  - **Copilot:** `~\.copilot\skills\team-validate` → `/team-validate`, base directory `~\.copilot\skills\team-validate`,
    "TEAM-VALIDATE RAN", passed.
  - **OpenCode:** `~\.config\opencode\skills\team-check` → `skill {"id":"team-check"} -> completed`, "TEAM-CHECK RAN",
    `otaq check pass` → "OPC-1: Validate passed".
- In the app (CDX-4, same state with a `team-gui` skill only in `~\.claude\skills`): the card says "SSP Codex can't
  start: missing skill" and "Codex doesn't have the skill `/team-g…`"; the sheet says "Codex doesn't have the skill
  `/team-gui`. Nothing started. Link the skill into Codex's skills folder or commit it to the project, then Check
  again; or give the stage an agent whose tool has it." with **Check again** and **Change agent…**.

#### 3. Change agent…

- `ticket.choices` on the held Validate (what Change agent… lists): CDX-1/CPL-1 (`team-validate` in `~\.claude`) →
  Developer, Validator (Claude Code), SSP OpenCode; OPC-1 (`team-check` in `~\.copilot`) → SSP Copilot only. Dev on
  the same tickets lists every agent (Otaq's skill), as it should.
- A refused pick: `ticket.set_choice CDX-2 Validate=SSP Copilot` → "SSP Copilot runs on GitHub Copilot, which doesn't
  have the skill `/team-validate` for Validate".
- Picks that started the stage: CDX-2 (held on Codex) → SSP OpenCode: `skill {"id":"team-validate"}` loaded,
  "TEAM-VALIDATE RAN", passed. CPL-2 (held on Copilot) → SSP OpenCode: same, passed. OPC-2 (held on OpenCode) → SSP
  Copilot: `/team-check` from `~\.copilot\skills\team-check`, "TEAM-CHECK RAN" (its `otaq check pass` was refused with
  "Run `otaq prime` first: OPC-2 has 1 comment you haven't read" because my test skill skipped `otaq prime`; the
  stage ended without a result. The move and the skill run are what this check is about.)
- In the app, Change agent… on CDX-4 opened "Change Validate's agent · Validate waits for SSP Codex. It starts on the
  agent you pick; only agents whose tool has Validate's skill and can run now are listed." and offered only
  Developer and Validator (Claude Code): SSP OpenCode, which has the skill, was left out because OpenCode was at its
  (marked) limit then. I cancelled rather than spend Claude Code quota.

#### 4. Limits per tool

`usage.mark_limit` ("This is the usage limit") on a ticket with a running session of the tool set that tool's limit
(`usage.activity`: "Marked as the usage limit by you"); then two tickets were moved to Dev:

- **Codex limit:** CDX-3 (SSP Codex) held, `tool_limit/codex`, no session; CPL-3 (SSP Copilot) started at once. After
  `limits.resume_tool codex`, CDX-3 started on Codex.
- **Copilot limit:** CPL-4 held `tool_limit/copilot`; OPC-4 (OpenCode) started and finished Dev and Validate.
- **OpenCode limit:** OPC-3 held `tool_limit/opencode`; CDX-4 (Codex) started.
- The app's banner showed one line per tool ("Copilot limit reached · Copilot didn't say when it resets. 1 ticket
  waiting. Resume Copilot", the same for OpenCode).

#### 5. Split of several check agents

`otaq flow propose -P CPL -f` on a file with `Validate: agents: [SSP Copilot, SSP OpenCode]` and
`Security: agents: [SSP Codex, SSP OpenCode, SSP Copilot]`: the proposal reads as Validate (SSP Copilot),
**Validate · SSP OpenCode** (`id: validate-ssp-opencode`, `column: with-previous`), Security (SSP Codex),
**Security · SSP OpenCode**, **Security · SSP Copilot**, each keeping skill, rounds and fail_to; the refusal of the
first version (no skill on Security) already named the copies ("Security · SSP OpenCode needs a skill"). One file
covered all three tools' agents; reading it doesn't depend on the tool, so it counts for all three. The proposal was
cancelled. Stored versions and Back to the previous flow were not tried.

### Findings

1. **A SKILL.md that starts with a UTF-8 BOM is "had" by Codex for Otaq, but Codex doesn't load it.** Windows
   PowerShell 5's `Set-Content -Encoding utf8` writes a BOM, so this is a likely way for a Windows user to write a skill.
   What I did: rewrote `~\.codex\skills\team-validate\SKILL.md` with PowerShell (bytes `EF BB BF 2D…`), then Check again
   on CDX-1. What I saw: Otaq started the stage and typed `$team-validate`; Codex's skills list had no `team-validate`
   (no `<skill>` injected) and Codex answered "I'll inspect the repository guidance and available tooling for the
   `team-validate` workflow…" and started `rg --files -g '*team*validate*' -g 'SKILL.md'`, i.e. the "agent carries on
   without the skill" case S5 exists to prevent. The same file without the BOM was listed and injected. OpenCode
   loaded the BOM copy (CPL-2's validator); Copilot with a BOM wasn't tried. Expected: the finder reads front matter
   as each tool does, so a file Codex ignores isn't counted for Codex (or Otaq warns).
2. **Minor: while Validate is held, the texts say it runs.** The Dev agent's `otaq stage next` answer was "Finished
   Dev. Validate is running; the result will come back to you here. End your turn." (OPC-1), so the agents told the
   user "validation is running"; the ticket's status stays `validating` and the sheet's pill says "Validating · round
   1 of 1", while the sheet below says "Nothing started".
3. **Minor: log noise.** The gate logs `INFO … the tool doesn't have the stage's skill key=… tool=… skill=…` for every
   held ticket about every 1–2 s.
4. **Environment note (not new in this round, but it touched the installed setup): a second data folder repoints
   Codex's plugin.** Starting this build's service with its own `OTAQ_HOME` ran Codex's setup, which left
   `~\.codex\config.toml` with `[marketplaces.otaq-local] source = '\\?\C:\Users\…\ssp\home\runtime\codex-plugin'`.
   The installed Otaq's Codex sessions now use the plugin from this test folder until the installed app sets Codex up
   again. I left `~\ssp\home` in place so the path stays valid; the maintainer may want to rerun the installed app's
   Codex setup (or Disconnect/Connect) before using it.

### Notes

- **Real OpenCode limit.** Three OpenCode Dev sessions at once (CDX-2, CPL-2, OPC-2 on SSP OpenCode, to keep the
  others' quota) hit Google's free-tier per-minute limit ("Quota exceeded … generate_content_free_tier_requests, limit:
  15 … retry in 31 s"). Otaq recorded an OpenCode limit, the three waited, OPC-1's held Validate switched from
  `missing_skill` to `tool_limit` and back to `missing_skill` once the reset was seen. Nothing resumed until
  `limits.resume_tool opencode`, as designed ("nothing resumes until its Resume"); I had to press it twice.
- Codex and Copilot asked permission for each command (also for `otaq stage next` via a here-string); I answered
  through `session.input`.
- `flow.propose`'s diff had no warning for a Validate that names a skill its agent's tool lacks (S5 puts that warning
  in the flow editor, which I didn't open).
- Quota used: Codex ≈ 4 short sessions (CDX-1 Dev + 2 validators, CDX-3/CDX-4 Dev), Copilot ≈ 3 AIC, OpenCode
  Gemini Flash Lite ≈ $0.15. Claude Code wasn't run. All sessions stopped (`otaq daemon stop`), app and Vite closed,
  scheduled tasks deleted.
- Cleaned up: `team-validate`, `team-check`, `team-gui` and both Copilot probes removed from `~\.claude\skills`,
  `~\.codex\skills`, `~\.copilot\skills`, `~\.config\opencode\skills` (left as before: `~\.claude\skills\synced`,
  `~\.codex\skills\.system`). Kept for evidence: `~\ssp\` (scripts, data folder, test repos, exports) and the worktree
  `~\otaq-build\stage-skill-per-tool`.
