# T10 hands-on report: Otaq doesn't copy skills

Run on 2026-11-05 from branch `no-skill-copies-T10` at `b32a3ee3` (T1–T9 merged). No code was changed.

- **Mac:** Claude Code 2.1.318, Otaq built from the worktree (`cargo build -p otaq-cli -p otaq-desktop`, page from
  Vite), a throwaway `OTAQ_HOME` and git repo `demo` (project DEMO) in the session scratchpad, reached through a short
  symlink `/tmp/t10` (the scratchpad path is too long for the service socket). Service and app started with a clean
  environment, as `.claude/skills/readme-screenshots` does.
- **Windows VM** ("Windows", ARM64): Codex 0.166.0 (npm, `%LOCALAPPDATA%\node24\codex.cmd`: it is installed),
  Copilot CLI 1.0.105, OpenCode 2.2.5. The branch went in as a git bundle; built with `npm run bundle -- --bundles nsis`
  and installed with the setup `.exe /S` over the existing install. Snapshot taken first:
  **`before-no-skill-copies-T10`** (id `10DE907D-E8D0-4E31-B022-8088AAEBFA7A`).

Canonical texts checked word for word: S3's skill forms, S6's "You don't have the skill `<name>`", S7's "Saved in
`<folder>` in this project. Commit it so agents get it in their worktrees."

## Results

| Check | Claude Code (Mac) | Codex (VM) | Copilot (VM) | OpenCode (VM) |
|---|---|---|---|---|
| 1. Dev session starts with Otaq's `work-ticket`, skills listed once | **pass** | **fail** on the moved-over install (bug 1); pass after `codex plugin add` by hand | **pass** (listing: see notes) | not checked: free quota used up |
| 2. Nothing written into the worktree | **pass** | **pass** | **pass** | **pass** |
| 3. Personal `team-validate` picked and run | **pass** | **pass** (picker list via service, not the UI) | **pass** (same) | not checked: quota |
| 4. Send back types `address-review` and it runs | **pass** | **pass** | **pass** | not checked: quota |
| 5. Stage naming `nope`: warning, applies, tool reports it | **pass** | **pass** (see decision D1) | **pass** | not checked: quota |
| 6. Ask the agent to write it: folder, S7 text, Undo | **pass** (layout bug 3) | folder only: pass; the rest not checked | folder only: pass; the rest not checked | folder only: pass; the rest not checked |
| 7. Moving over from a data folder before this change | **pass** (leftovers only, see below) | **pass** on Windows, once bug 2 was worked around (one check for all three tools: same data folder) | ← same | ← same |
| 8. Copilot runs shipped skills with `disable-model-invocation: true` | n/a | n/a | **pass** | n/a |

## What was done and seen, per check

### 1. Dev session

- **Claude Code:** DEMO-1's transcript starts `/otaq:work-ticket`, the skill's base directory is
  `$OTAQ_HOME/runtime/<build>/plugin/claude/skills/work-ticket`. Claude Code's skill listing has `otaq:commit` once and
  no `otaq-*` entry; the other twelve shipped skills carry `disable-model-invocation: true`, so Claude Code leaves them
  out of the model's list, as expected. The model called `Skill otaq:commit` on its own.
- **Codex: fail.** CDX-1's first prompt was `$otaq:work-ticket`, but Codex's skills list had no Otaq root and no
  Otaq skill; the agent hunted for the file (`rg --files -g '*work-ticket*' -g 'SKILL.md' C:\Users\…\.codex
  C:\Users\…\.otaq`) and asked to run that. Cause: bug 1. After `codex plugin add otaq@otaq-local` by hand, CDX-2 ran
  `$otaq:work-ticket`, read `otaq:commit` from the plugin cache, and finished Dev. Codex's `$` picker lists Otaq's
  skills once, namespaced: `open-pr (otaq)`, `address-review (otaq)`, `validate (otaq)`, `flow (otaq)`….
- **Copilot:** CPL-1's session shows `/otaq:work-ticket`, then `skill(commit)`, `otaq prime`, and the work. The model's
  own skill call shows as `skill(commit)` without the namespace; I didn't open Copilot's skill list, so "listed once,
  namespaced" is only shown by that call and by there being no copies in the worktree.
- **OpenCode:** OPC-1's session started and `work-ticket` was typed, but OpenCode answered "Free limit reached …
  Resets at 03:00 · 06/11/2026" before any model turn. `opencode debug config` with Otaq's `OPENCODE_CONFIG_CONTENT`
  lists the inline document with the `skills` entry and reports no error, so the entry is accepted; whether the skills
  load wasn't seen.

### 2. Nothing written into the worktree

`git status --ignored` is empty and there is no `.agents` or `.claude` in DEMO-1, DEMO-2, CDX-2, CDX-3, CPL-1 and
OPC-1, nor in the projects' checkouts.

### 3. Personal skill

- **Claude Code:** `~/.claude/skills/team-validate` (created for the test; it didn't exist; removed at the end). Flow
  editor → Validate → Skill → the list showed `team-validate` first, as "Personal · The team's own validation …". Picked
  it, applied. DEMO-2's validator transcript starts `/team-validate`, base directory `~/.claude/skills/team-validate`,
  and it answered "TEAM-VALIDATE RAN".
- **Codex / Copilot:** `team-validate` put in `~\.codex\skills` and `~\.copilot\skills` (and
  `~\.config\opencode\skills`). `skills.list` (what the picker shows) listed it once as personal with all three places.
  The flow was set by `otaq flow propose -f` + `flow.apply`, not through the UI (UI checked on the Mac). Codex ran
  `$team-validate` ("I'm applying the team-validate skill"), Copilot `/team-validate` → `skill(team-validate)`; both
  answered "TEAM-VALIDATE RAN".

### 4. Send back with a line comment

A plain Send back types "Review sent DEMO-1 back to you with a comment. Run `otaq prime`." without a skill, as
designed. With one line comment (`review.comment_add` + `ticket.fail`) the session got
`/otaq:address-review DEMO-1 was sent back to Dev with 1 line comment. …` (Claude Code),
`$otaq:address-review CDX-2 …` (Codex) and `/otaq:address-review CPL-1 …` (Copilot); each ran `otaq review feedback`
and fixed the line.

### 5. `nope`

- **Flow editor (Mac):** typing `nope` in the search offers "Use `nope`"; picked. The stage card says "You don't have
  the skill nope" and the field "You don't have the skill `nope`" (text exact; layout bug 4). "Apply to demo" was
  enabled and the flow applied with `skill: nope`.
- **Claude Code:** DEMO-3's validator showed "Unknown command: /nope. Did you mean /copy?" and sat idle.
- **Copilot** (Dev's skill set to `nope`): "✗ Unknown command: /nope".
- **Codex** (same): "I'm not sure what you mean by "$nope". What would you like me to do?" Then Otaq's stop reminder
  ("you ended your turn but your stage is still open …") came, and Codex went on with `otaq prime` and the ticket
  without any skill. See D1.

### 6. Ask the agent to write it (Mac, Claude Code)

New stage after Review → "Ask the agent to write it" prefilled Noyan's box with "Write a skill for the New stage stage:";
I added the request and sent it. The field went to "Being written by Noyan… Stop", then showed `release-notes`,
"Written by the agent", Open and Undo, and the line "Saved in `.claude/skills/release-notes` in this project. Commit it
so agents get it in their worktrees." (text exact; layout bug 3). The folder held only `SKILL.md`, untracked. Undo
removed `release-notes` (empty `.claude/skills/` left behind, bug 9) and the field went back to Pick a skill… / Ask the
agent to write it. Nothing appeared in `$OTAQ_HOME/skills` or `skill-writes`.

On the VM, `flow.write_skill` for a stage per project named `C:\…\t10\cdx\.agents\skills\t10-written` (Codex) and
`…\cpl\.claude\skills\…`, `…\opc\.claude\skills\…` (Copilot, OpenCode): the S7 folder rule holds. Noyan writing, the
text and Undo weren't run there.

### 7. Moving over

- **Windows (the real before-state):** the data folder `~\.otaq` turned out to have no imported skill any more
  (`agent_skills` held only shipped rows, no `$OTAQ_HOME\skills`, `~\my-skills` empty). With the old installed build
  (`1a371563`) I recreated `~\my-skills\house-style` and imported it onto Developer and Noyan through the old service
  (`agents.import_skill`), which made `~\.otaq\skills\<id>\SKILL.md` twice. Its worktrees already had marked copies
  (`otaq-*` with `.otaq-copy` under both `.agents\skills` and `.claude\skills` in RUNV-1, RUNT-1, RUNS-1, RUNA-1,
  RUNA-2). I planted, in the `cafe-menu` checkout, marked `.claude\skills\otaq-commit`, `.agents\skills\otaq-commit`,
  `.claude\skills\otaq-custom-house-style`, an unmarked `.claude\skills\my-own`, plus an unmarked `my-own-wt` and a
  marked `otaq-validate` in RUNV-1, and `~\.otaq\skill-writes\t10`.
  The new build's service first refused to start (bug 2). After patching the stored checksum, it started and logged
  "an old skill folder removed dir=…\.otaq\skills" and "…\skill-writes", and "skill copies an older version left
  removed" for the checkout (2) and every worktree (7 each). Afterwards: `skills` and `skill-writes` gone, no marked
  folder anywhere, `my-own` and `my-own-wt` still there; `agents.list` has no skills on any agent; `skills.list` shows
  `my-own` as a project skill.
- **Mac:** the scratch data folder was new, so it had no imported skill in its database. I planted
  `$OTAQ_HOME/skills/…`, `skill-writes/…`, marked copies (also an `otaq-custom-*`) in the checkout and in DEMO-2's
  worktree, and unmarked `my-own` folders in both, then restarted the service: all marked folders and both old folders
  removed, the unmarked folders kept.

### 8. Copilot and `disable-model-invocation: true`

`work-ticket` and `address-review` both carry the flag and both ran when typed as `/otaq:work-ticket` and
`/otaq:address-review`. Copilot didn't break on the flag. Not tested: the model calling a flagged skill on its own (it
isn't supposed to).

## Bugs

1. **Codex keeps the old plugin copy after an update, so `$otaq:<name>` is unknown.**
   What I did: installed the new build over the old one and started a Codex Dev session.
   What I saw: `~\.codex\plugins\cache\otaq-local\otaq\0.1.0\` was still the hooks-only copy (manifest: "Otaq copies
   the skills its agents use into their working folder …", no `skills/`), while `runtime\codex-plugin` had `skills/`.
   The service didn't rerun the setup. `stale_cached_skills` looks in `cached_plugin()` =
   `<codex home>/plugins/cache/otaq-local/otaq/local`, but Codex 0.166 puts the copy in `…/otaq/<version>` (`0.1.0`;
   `codex plugin add` prints "Installed plugin root: …\otaq\0.1.0"). The folder isn't found, so it returns false.
   Expected: the setup runs again and `codex plugin add` refreshes the copy.
   Running `codex plugin add otaq@otaq-local` by hand did refresh it: the cache got `skills/` with all 13 skills, which
   answers T1's open question. Codex then wanted its hooks trusted again ("Hooks need review: 5 hooks are new or
   changed"; a running session showed "Hook failed — plugin contents changed; hook approval required"), because the
   trust is tied to the plugin's content. The setup re-records hook trust when it runs, so fixing the path should cover
   both. I chose "Trust all and continue" in the VM.
2. **Not from this change, but it blocks a move-over: migration 0071 was edited after it was committed.**
   What I saw: the new service exited with "database migration failed: migration 71 was previously applied but has been
   modified", and the app said "Background service couldn't start". `31c6b06a` (2026-11-04, "Comments name the rule in
   words…") changed the first comment line of `0071_app_ports.sql` (the file says "Never edit once committed"). Only data
   folders made by a dev build between `3cabb794` and `31c6b06a` are affected: `v0.1.0` already has the edited file. The
   VM's install was such a build. Workaround: I set `_sqlx_migrations.checksum` for version 71 to the new file's SHA-384
   in the VM's `~\.otaq\otaq.db`. The original is kept as `~\otaq-build\t10-db\otaq.db.before-checksum-fix`.
3. **The S7 line is laid out in three columns.** "Saved in" | `.claude/skills/release-notes` | "in this project. Commit
   it so agents get it in their worktrees." each wrap inside their own narrow column under the field. Expected: one
   flowing sentence.
4. **The warning puts the skill on its own line.** In the field, "You don't have the skill" is followed by `nope` on
   the next line, though there's room. The stage card's one-line version reads fine.
5. **"Use `nope`" shows literal backticks** in the picker's last item. Every other place draws the name as code.
   Expected: the name drawn as code, or quotes.
6. **Validate stage editor text overlaps:** "A ticket can" / "skip it" is drawn on top of "Passes when every check
   agent passes. If one fails, …" at the bottom of the panel. Probably older than this change.
7. **Noyan's terminal in the flow editor is drawn about ten columns wide**: "18s · / done 9:55 / PM" and the input line
   wrap in a narrow strip at the top left of a wide panel.
8. **Skill places on Windows mix separators:** `C:\Users\nevzattopcu\cafe-menu\.claude/skills\my-own`,
   `C:\Users\nevzattopcu\.copilot/skills\team-validate`, `…\.config/opencode/skills\…` (`skills.list`).
9. **Undo leaves empty folders:** after Undo, the checkout keeps empty `.claude/` and `.claude/skills/`. git ignores
   them, so it's cosmetic. `flow.write_skill` likewise makes the empty `<folder>/<name>` up front.
10. **Stale doc comment:** `agents.add` in `crates/otaq-core/src/protocol.rs` still shows
    `"skills": [{"shipped": "work-ticket"}]` in its example. `AgentFields` has no skills field.
11. **"What this agent reads" shows SKILL.md with its source line breaks plus soft wrapping**, so the long lines break
    twice ("You work in" / "the ticket's own git worktree, …"). Cosmetic.

## Needs a product decision

- **D1. A stage whose skill nobody has.** S5 says the tool says so, and it does. The board, though, keeps the card on
  "Validating… · round 1 of 3" or "Working" with nothing pointing at the problem: Claude Code and Copilot sit at their
  "Unknown command" line until someone looks. Codex answers "not sure what you mean", and then Otaq's "your stage is
  still open" reminder makes it carry on with the ticket **without** the skill (CDX-3), so a stage can quietly run with
  no skill at all. Should the card show the S6 warning at run time, or should the reminder hold off when the stage's
  skill is unknown?
- **D2 (to verify, not decide).** OpenCode 2.2.5's TUI connects to a shared background server by default (its help:
  `--standalone` "Run with a private server instead of the background service"). If so, a launch's
  `OPENCODE_CONFIG_CONTENT` (Otaq's plugin, its `skills`, the `otaq` rule) may not reach the server that runs the
  session. I couldn't check it while the quota was used up.

## The other questions the earlier tasks left open

- **Copilot and `disable-model-invocation: true`:** fine (check 8).
- **OpenCode's `skills` entry in `OPENCODE_CONFIG_CONTENT`:** accepted by `opencode debug config`; loading not seen
  (quota).
- **Codex's installed plugin copy after `codex plugin add otaq@otaq-local`:** it does pick up `skills/` (bug 1 is
  that Otaq doesn't rerun it).
- **OpenCode loading a user skill named like one of Otaq's:** not checked (quota).
- **Settings → Agents → "What this agent reads":** reads sensibly without a skill list. For Developer: Its
  instructions, Notes for demo, "Skill: work-ticket · Dev" with the full SKILL.md and "Open skill folder →", "Skill:
  open-pr · Done", then "Also has: `commit` (commits with it)" and "Not shown: the ticket and what earlier stages
  wrote…". Open skill folder → opened Finder on `$OTAQ_HOME/runtime/<build>/plugin/claude/skills/work-ticket`, a folder
  that changes with every build. The agent page itself has no skill list and no Import skill folder…/Re-import.
- **"Use `<typed>`" with literal backticks:** it looks odd (bug 5).

## Left behind

- **Mac:** `~/.claude/skills/team-validate` created for check 3 and removed; nothing else in `~/.claude` was touched by
  me. Claude Code itself added its usual per-folder entries for the scratch paths in `~/.claude.json` and transcript
  folders under `~/.claude/projects/-private-tmp-claude-501-…-scratchpad-t10-…`; not edited. The service, app and Vite
  are stopped; the scratch `OTAQ_HOME`, the demo repo and the `/tmp/t10` symlink are removed.
- **VM:**
  - The new build is installed in `%LOCALAPPDATA%\Otaq` and left there. The snapshot `before-no-skill-copies-T10` is
    kept.
  - To install, I stopped a stale service and app of `C:\otaqdemo\home`, started at 02:54 by another session.
  - Build files: the worktree `~\otaq-build\nsc-t10` (branch `nsc-t10` in `~\otaq-build\repo`), the
    `nsc-t10-build*.log` files, `noupd.json`, `t10-old-daemon.cmd`/`.log`, `t10-app.cmd` and `t10-db\`.
  - In `~\.otaq`:
    - `house-style` was imported with the old build and is now gone again; `~\my-skills\house-style` remains.
    - Migration 71's checksum was patched.
    - Projects CDX, CPL and OPC (`~\t10\cdx|cpl|opc`) were added, with tickets, agents T10 Codex/Copilot/OpenCode and
      their flows.
  - Codex: `codex plugin add otaq@otaq-local` was run and its hooks trusted. Copilot: "don't ask again" for file edits
    in `~\t10\cpl` and for git add/commit.
  - Removed: the personal `team-validate` folders, the planted `my-own` folders and empty skill folders, and the files
    I sent to the Desktop. The service and app are stopped.
