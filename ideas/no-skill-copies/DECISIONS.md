# Otaq doesn't copy skills

Agreed with the maintainer on 2026-11-05. The old `decisions/` folder is lost, so this file is the record. Decision
numbers cited in code and tests (d0092, d0098, d0108, d0118, AGM-07, WSK-*) are history only.

**Code comments and tests name a rule in words, never by its number here** ("Otaq's own skills come from its
plugin", "Otaq never writes skills into a worktree"). A task reference such as "(T2)" is fine.

## The problem

Since T-0408 (`49604d8f`, "Skills by one name"), every session gets copies of its skills before it starts. Each
skill, Otaq's own and the user's imported ones, is written twice into the working folder: once under
`.agents/skills/otaq-<name>/` and once under `.claude/skills/otaq-<name>/`. That folder is a ticket's worktree, or the
user's real checkout for the Project agent and the epic lead. Each copy carries a marker, a `.gitignore` of `*`, a
rewritten `name:` and no `disable-model-invocation`. Supporting this takes:

- reference counting in checkouts, a tidy-up at service start and at uninstall;
- warm-build exclusions;
- refusals when a user folder has the same name;
- a copy of every hand-off skill at every launch and resume.

On top of that, a user's own skill had to be imported into `$OTAQ_HOME/skills/` before an agent could use it: an
extra step for something the user already has.

We went this way because it looked like the only option. It isn't.

## What other tools do

Vibe Kanban, Conductor, Claude Squad, Emdash, Superset/Paradev and Crystal/Nimbalyst don't manage skills at all.
They start the agent tool in a worktree, and the tool loads skills itself:

- **personal skills** (`~/.claude/skills` and the like);
- **project skills committed to the repo**, which git puts in every worktree;
- **gitignored project skills:** Claude Code (2.1.277 and later) falls back to the main checkout's `.claude/skills`
  when the worktree has none.

Skill managers (`npx skills`, ruler, rulesync) install once into each tool's own folders. None of them installs
skills per session, and none renames them.

## Decisions

### S1. Otaq never copies, imports or renames a user's skills

The user's skills stay where they are. The agent tool finds them the way it always does. Otaq writes no skill folder
into a worktree or a checkout. The one exception is S7, where the user asks for a skill to be written.

### S2. Otaq's own skills come from its plugin

The skills in `plugin/skills/` (`work-ticket`, `commit`, `validate`, `open-pr`, `push`, `address-review`,
`restack`, `answer-question`, `noyan`, `product-spec`, `tech-spec`, `milestone-plan`, `flow`) reach every session
through Otaq's per-tool plugin, the same plugin that already carries the hooks:

- **Claude Code:** `--plugin-dir <plugin>/claude`, which already passes; the plugin gains a `skills/` folder.
- **Copilot:** `--plugin-dir <plugin>/copilot`, which already passes; the plugin gains a `skills/` folder inside it.
  Copilot ignores a path that leaves the plugin folder.
- **Codex:** the `otaq@otaq-local` plugin that setup installs (`runtime/codex-plugin`). It gains a root `skills/`
  folder, which Codex finds without a manifest entry.
- **OpenCode:** a `skills` entry in `OPENCODE_CONFIG_CONTENT` that points at the shipped skills folder. Otaq did this
  before T-0408.

Every shipped skill is in every session from its start, so a hand-off can always type a skill by name. There's no
"started with" list, and no naming a skill by its file.

The shipped skills' SKILL.md files go out unchanged, `disable-model-invocation` included. If Copilot's skill tool
still breaks on that flag (see T-0408's note), the task reports it, and nothing is rewritten without a decision.

### S3. How a session names a skill

| Tool | Otaq's own skill | The user's own skill |
|---|---|---|
| Claude Code | `/otaq:<name>` | `/<name>` |
| Copilot | `/otaq:<name>` | `/<name>` |
| Codex | `$otaq:<name>` | `$<name>` |
| OpenCode | `<name>` | `<name>` |

These are the forms Otaq used before T-0408, and that commit's hands-on checks found they work.

**OpenCode (changed 2026-11-06, after the hands-on check):** OpenCode has no typed command for a skill; its model
loads a skill with its skill tool when it decides to, and a bare name was mostly ignored. So a session's first
message to OpenCode names the skill as an instruction: "Load the skill `<name>` with your skill tool and follow it.
If you don't have a skill with that name, say so and stop." followed by the usual text. The `<name>` form in the
table is the name inside that instruction (a stage's skill in prime, the picker, the warning stay as they are).

A name that matches one of Otaq's shipped skills always means Otaq's skill. Any other name is the user's.

### S4. Agents have no skill list

The agent's skills (shipped and imported), **Import skill folder…** and **Re-import** are removed from Settings →
Agents, the service, the CLI and the store. An agent is its instructions, its tool and model, and its role.

A flow file's `agents: … skills:` list is still read, so that old files still open, and ignored. Otaq never writes
it again.

So **Commit** and **Push** are always Otaq's `commit` and `push` (only Open PR is replaced, by the Delivery stage's
`skill:`, S5). A team's own way of committing goes in the agent's instructions: Otaq's `commit` skill follows them on
how commits are written (message format, how the work is split), as `push` already does, but never on committing
everything or never pushing (agreed 2026-11-06).

### S5. A stage's `skill:` is the only place a skill is picked

- **A stage names a skill** (`skill: team-validate`): the session runs it by its own name (S3).
- **A stage names no skill:** Otaq's own skill for that stage runs, as today (`validate`, `open-pr`, `work-ticket`…).
- **Open PR:** the Delivery stage's `skill:` is used as today (`skill: my-pr`).

**A stage runs only on a tool that has its skill** (agreed 2026-11-06, gap review; replaces "the stage still starts
with a warning", agreed after T10):

- The flow editor offers, for a stage, only agents whose tool has the stage's skill (S6 says what "has" means).
  Otaq's own skills are in every tool, so a stage running one of them can use any agent.
- A stage due to start on a tool that doesn't have its skill doesn't start. Its card goes to Needs you:
  "Copilot doesn't have the skill `/some-skill`", with **Check again** and **Change agent…** (only agents whose tool
  has the skill). This covers a skill deleted or moved, a flow file edited by hand, a written skill not yet
  committed, and a name typed by hand that isn't written yet.
- The same check runs when a stage's skill is typed into a session that is already running (Open PR into the live
  Dev session, a queued return), not only when a session starts.
- A flow that names such a skill still applies; the flow editor shows the warning on the stage.

Otaq never points the agent at the skill's file in another tool's folder and never copies it (S1). Rejected: starting
anyway with a warning (the agent stops at "Unknown command" or carries on without the skill), and refusing to apply
the flow (a skill deleted later still needs an answer, and a flow couldn't be applied while its skill is written).

### S6. Otaq lists the skills the user already has (read-only)

Otaq reads the folders the agent tools load skills from, without copying anything, and lists every skill it finds:

- **personal:** `$CLAUDE_CONFIG_DIR/skills` (else `~/.claude/skills`), `~/.agents/skills`, `$CODEX_HOME/skills`
  (else `~/.codex/skills`), `$COPILOT_HOME/skills` (else `~/.copilot/skills`), `$XDG_CONFIG_HOME/opencode/skills`
  (else `~/.config/opencode/skills`). Each tool's variable is followed as the tool follows it (agreed 2026-11-06);
- **project** (the project's checkout): `.claude/skills`, `.agents/skills`, `.github/skills`, `.opencode/skills`.

Each skill is listed with its name (the SKILL.md `name:`, else its folder's name), its description, and where it
lives. Links are followed, as the tools follow them (a skill kept in the user's own repo and linked into
`~/.claude/skills` is found); a skill reached from several folders is listed once (agreed 2026-11-06). Every folder holding a SKILL.md is
listed: front matter that is missing or doesn't read gives the folder's name and no description (agreed 2026-11-06). Otaq's own skills are listed too, marked as Otaq's.

The finder also reads the skills the tools' installed plugins provide (Claude Code, Codex and Copilot plugins), and
reads each tool's variables from the environment Otaq launches that tool with, not the service's own (agreed
2026-11-06, gap review), so a skill that exists isn't reported missing.

**Which tool has a skill** (agreed 2026-11-06, gap review): a stage's tool has a skill when that tool finds it in
the ticket's worktree: in that tool's own personal folder, in the project's folders when the skill is committed, or,
for Claude Code only, in the checkout's `.claude/skills` (its fallback). A skill in another tool's folder doesn't
count: `~/.claude/skills/some-skill` is had by Claude Code, not by Copilot or Codex. "Pick a skill…" lists every
skill with the tools that have it.

This list is used for:

- **Pick a skill…** in the flow editor's Skill field: every skill found plus Otaq's own, searchable. A name typed by
  hand is also allowed.
- **A flow that names a skill nobody has:** the flow still applies, and the stage shows the warning
  **"You don't have the skill `<name>`"**. This replaces a proposal's "isn't on this machine · Import its folder…".
  Otaq's own skills never warn.

### S7. "Ask the agent to write it" saves into the project

Noyan writes the new skill into the project's checkout, in the skills folder the stage's agent's tool reads:

- `.agents/skills/<name>/` when that tool is Codex;
- `.claude/skills/<name>/` otherwise (Claude Code, Copilot and OpenCode all read it).

Otaq doesn't commit it. Once written, the field says, word for word:

**"Saved in `<folder>` in this project. Commit it so agents get it in their worktrees."**

A ticket's worktree is made from a commit, so an uncommitted skill doesn't reach it. Claude Code's main-checkout
fallback is the one exception.

**Undo** deletes that folder, but only while it is still uncommitted and holds only what Noyan wrote. Otherwise
Undo just clears the field. "Holds only what Noyan wrote" means the folder's fingerprint (every file's name and
contents) is unchanged since Noyan finished, so a hand edit is never thrown away. When Undo deletes the folder, it
also removes `skills/` and `.claude/` (or `.agents/`) if they are left empty and Noyan's write made them (agreed
2026-11-06). A written folder that a draft edit stops using (the stage's skill changed, the stage deleted, the draft
discarded) stays in the project: only Undo, or Stop while Noyan is writing, deletes it (agreed 2026-11-06). Nothing
goes to `$OTAQ_HOME/skill-writes/` or `$OTAQ_HOME/skills/` any more.

### S8. Existing installs move over with no carry-over

- **Imported skills** are removed from every agent, with no carry-over to a stage's `skill:` and no notice. Stages
  that used one now run Otaq's own skill.
- **Otaq's stored skills:** `$OTAQ_HOME/skills/` and `$OTAQ_HOME/skill-writes/` are deleted.
- **Marked copies** (folders holding `.otaq-copy`, under `.agents/skills` or `.claude/skills`) are removed at
  service start from every live worktree and every project checkout, and by `otaq uninstall`. A folder without the
  marker is never touched. The old `otaq-custom-*` names are removed too, as today.

## Known gaps, accepted

- A gitignored project skill reaches a worktree only on Claude Code (its fallback). Otaq doesn't fill this gap
  (S1).
- OpenCode has no namespace, so a user skill with the same name as one of Otaq's may be the one OpenCode loads.
  Report it if a test shows it; don't work around it.

- A skill Noyan writes goes in the folder of the stage's agent's tool (S7). An agent on another tool (Codex versus
  the rest) doesn't have it, so it isn't offered for that stage, and a stage due to start on it is held (S5). Otaq
  doesn't write it twice (agreed 2026-11-06; a check stage has one agent, `ideas/one-check-agent`; "is held"
  replaces "the card warns", gap review).
- A skill only some tools have limits which agents can run that stage (S5). That is the price of not copying; the
  user makes a skill available to more tools by linking it into their folders or committing it to the project.

## Follow-ups agreed after T10

- **F1 (S5):** a running stage whose skill Otaq can't find shows the warning on its card, and gets no "stage still
  open" reminder. *Superseded by S5's "a stage runs only on a tool that has its skill" (gap review): the stage is
  held instead of started.*
- **F2 (S6):** the finder follows `CLAUDE_CONFIG_DIR`, `COPILOT_HOME` and `XDG_CONFIG_HOME`, as it follows
  `CODEX_HOME`.
- **F3 (S6):** the finder follows linked skill folders and lists a skill reached through several folders once.
- **F4 (S6):** a SKILL.md with no front matter, or front matter that doesn't read, is listed under its folder's name
  with no description, instead of being skipped.
- **F5 (S4):** Otaq's `commit` skill gets `push`'s "follow the Agent instructions" step: they decide how commits
  are written, never whether everything is committed or whether to push.
- **F6 (S5):** `otaq prime --validate` gets the same "Your stage" line as a working agent's prime ("Validate, as
  Validator, with the skill `/team-validate`."), so a validator still knows its skill after compaction or a resume.
- **F7 (S7):** Undo also removes the `skills/` folder and its parent when they are left empty and Noyan's write made
  them.
- **F8 (S4):** the website's agent cards lose their skill chips; the rest of each card stays. Showing skills on the
  website's flow ("Validate · skill: validate") is a later design change, not part of this.
- **F9 (S3):** OpenCode's first message is the "Load the skill `<name>`…" instruction instead of the bare name.
- **F10:** a tool stopped by a provider error doesn't leave its card on Working. A usage or quota limit goes through
  Otaq's limit handling (only that tool's agents wait, "Waiting for limit reset", the banner); any other error stops
  the card on a needs-you state with the tool's own message. A message held for the session waits with the card.
- **F11:** plain `otaq prime` in a validator's session prints the validator's prime, and the "Run `otaq prime`
  first" refusal names the right command for a validator.
- Not changed: a validator may pass a ticket with no change (an investigation, "already fixed"); judging that is the
  validator's job (agreed 2026-11-06).

## Follow-ups agreed in the gap review (2026-11-06)

From `ideas/gap-review/REPORT.md` (G-numbers) and its answers (`ideas/gap-review/DECISIONS.md`).

- **F12 (S5, S6; G4, G6, G17, G18):** build S5's "a stage runs only on a tool that has its skill" and S6's
  "which tool has a skill", the plugin skills and the launch environment.
- **F13 (G9):** the release waits for T10's checks 1, 3, 4 and 5 on OpenCode, run with a provider that has quota,
  including a shipped skill that carries `disable-model-invocation`.
- **F14 (G5):** F10 ships in the same release as these decisions.
- **F15 (S4, S5; G10):** "use your own Open PR skill" (the PR error, `open-pr`'s text, the PR provider row's link)
  points at the Delivery stage's Skill field in the flow editor, not at the agent's skills.
- **F16 (S2; G25):** Codex's installed copy of Otaq's plugin is refreshed whenever any plugin file changes, not only
  its skills, with hook trust recorded again as setup does.
- **F17 (S6; G28):** "What this agent reads" shows a user's stage skill from where the finder found it, with its
  text and Open skill folder → to the user's own folder.
- **F18 (S4; G29):** the code left from imported skills (unused refusals, comments about copies, "the move-over's
  notice") is removed in this work.
- Not done (G30): no checksum repair for migrations changed before v0.1.0; released installs aren't affected.
- **F19 (S5):** a stage's skill may not be a tool's built-in command (Claude Code's `/security-review`, `/review`,
  `/init` and the like, listed per tool by its adapter): a flow naming one is refused, in `otaq flow propose` and as
  a problem on the stage in the flow editor, "`/security-review` is Claude Code's built-in command, not a skill.
  Write a small skill that runs it, and name that skill here." A user skill of that name the finder finds is the
  user's and is accepted (agreed 2026-11-06, before the PR).
