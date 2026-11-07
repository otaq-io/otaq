# Orchestrator prompt: Otaq doesn't copy skills

Paste everything below the line into a fresh Claude Code session started at the repository root
(`/Users/nevzattopcu/Desktop/Workspace/otaq`).

---

You are the orchestrator for a set of tasks in the Otaq repository (`/Users/nevzattopcu/Desktop/Workspace/otaq`).
You don't implement the tasks yourself. Your job:

- set up the branches;
- start one subagent (a teammate) per task;
- run tasks in parallel whenever their dependencies allow;
- review each result, merge the results together, and keep the build green;
- report to me at the end.

## Background

Since T-0408, Otaq copies every skill a session uses into the session's working folder before it starts. That's
Otaq's own skills and the user's imported ones, written twice, as `.agents/skills/otaq-<name>/` and
`.claude/skills/otaq-<name>/`, into worktrees and even into the user's real checkout. A user's own skill also had to
be imported into Otaq first. We chose this because it looked like the only option. It isn't.

Other orchestrators (Vibe Kanban, Conductor, Claude Squad, Emdash) don't manage skills at all. The agent tool finds
the user's skills itself. The agreed change:

- **The user's skills:** Otaq never copies, imports or renames them.
- **Otaq's own skills:** they ship inside its per-tool plugins, named `/otaq:<name>`.
- **Agents** have no skill list. A flow stage's `skill:` picks a skill by the user's own name for it.
- **The flow editor** lists the skills the user already has and warns about a skill nobody has. "Ask the agent to
  write it" saves into the project for the user to commit.
- **Existing imported skills and leftover copies** are removed.

Every decision is in `ideas/no-skill-copies/DECISIONS.md` (S1–S8), and the work is split into tasks T1–T10 in
`ideas/no-skill-copies/tasks/`. These files are the source of truth for this work. The old `decisions/` folder is
lost: don't look for it or write to it. Treat decision numbers cited in code and tests (d0092, d0098, d0108, d0118,
AGM-07, WSK-*) as history.

## Read first

1. `ideas/no-skill-copies/DECISIONS.md`.
2. Every task file in `ideas/no-skill-copies/tasks/` (T1–T10).
3. `CLAUDE.md` and `scripts/README.md` (how `scripts/check.sh` runs and what it needs).

## Setup (do this before starting any task)

**Never change the main checkout** (`/Users/nevzattopcu/Desktop/Workspace/otaq`): don't switch its branch, and don't
commit, stash or unstage anything in it. Work only in worktrees of your own.

1. Create the base worktree and branch from the current HEAD:
   `git worktree add ../otaq-no-skill-copies -b no-skill-copies HEAD`
2. The `ideas/no-skill-copies/` folder isn't committed in the main checkout, so the new worktree doesn't have it.
   Copy it into `../otaq-no-skill-copies/ideas/no-skill-copies/` and commit it there as the first commit on
   `no-skill-copies` (`no-skill-copies: decisions and tasks`).
3. In the base worktree, run `scripts/check.sh` once, to know the starting state. If it's already red, write down
   which steps or tests fail, so you don't blame a task for them, and tell me in the final report.

## Dependency graph

```
T1 plugins carry Otaq's skills ───────────────────────────────────────────────┐
T2 sessions start without copies ──┬──► T4 skill texts and prime ─────────────┤
                                   ├──► T5 leftover copies removed ────────────┤
T3 skill finder (read-only) ───────┴──► T6 core: no agent skills ──┬► T7 Settings → Agents UI ┤
                       (T6 needs T2 and T3)                        └► T8 flow editor UI ──────┤ (T8 also needs T3)
                                                                                              ▼
                                                                         T9 docs ──► T10 hands-on check
```

- **Wave 1, all in parallel:** T1, T2, T3.
- **T4 and T5** start as soon as T2 is merged. Don't wait for T1 or T3.
- **T6** starts as soon as T2 and T3 are both merged. It may run alongside T4 and T5.
- **T7 and T8** start as soon as T6 is merged. They run in parallel.
- **T9** starts when T1–T8 are all merged. **T10** starts when T9 is merged.
- Never start a task before everything it depends on is merged.

## Running a task

For task `T<n>`:

1. Create its worktree from the **current** `no-skill-copies`:
   `git worktree add ../otaq-nsc-T<n> -b no-skill-copies/T<n> no-skill-copies`
2. Start one subagent for it, in the background, with no `isolation` option: you made its worktree yourself. Give it
   this brief, filled in:

   > You work only in `/Users/nevzattopcu/Desktop/Workspace/otaq-nsc-T<n>` (a git worktree on branch
   > `no-skill-copies/T<n>`). Never touch any other folder, especially `/Users/nevzattopcu/Desktop/Workspace/otaq`.
   > The one exception is T10, which may also use a scratch folder and the VMPal Windows VM, as its task file says.
   >
   > Read `ideas/no-skill-copies/DECISIONS.md`, then `ideas/no-skill-copies/tasks/<task file>`, and do exactly that
   > task.
   > - Use the canonical texts in DECISIONS.md word for word: the skill reference forms in S3, the warning
   >   "You don't have the skill `<name>`" in S6, and the written-skill text in S7.
   > - In code comments and tests, name a rule in words ("Otaq never writes skills into a worktree"), never by its
   >   S-number. A task reference such as "(T2)" is fine.
   > - Line numbers in the task file are approximate (`~`). Find the code by the names given.
   > - If the task needs a product decision that DECISIONS.md doesn't cover, stop and report it. Don't decide it
   >   yourself.
   > - Stay inside the task's scope. Note anything else you notice in your report instead of fixing it.
   > - Run the task's "Done when" checks in your worktree and report their real output, including failures.
   > - Commit on your branch with the message `no-skill-copies T<n>: <summary>`, ending with the commit attribution
   >   lines your session gives you. Never push.
   > - Finish with a report: files changed, checks run and their results, anything left undone, any stopgap edits
   >   outside your scope, and any product question you hit.

3. **T10 writes no code.** Its output is `ideas/no-skill-copies/T10-report.md`, which you merge like any other
   task. If its report shows failures, don't fix them yourself: list them for me.

## Checking and merging

When a subagent reports back:

1. **Review** its diff (`git -C ../otaq-no-skill-copies diff no-skill-copies...no-skill-copies/T<n>`) against the
   task file's "What to change" and "Done when". Check that:
   - every canonical text is exact;
   - nothing outside the task's scope changed without being reported;
   - no code writes a skill folder into a worktree or a checkout, except T6's write-skill.

   If it falls short, send that subagent back (SendMessage) with a specific note on what's wrong and where. Do this
   at most twice. Then mark the task failed and carry on with the tasks that don't depend on it. Tasks that depend on
   a failed task don't start, and are reported as blocked.
2. **Merge** it into `no-skill-copies` in the base worktree, one task at a time
   (`git -C ../otaq-no-skill-copies merge --no-ff no-skill-copies/T<n>`). Expected conflicts:
   - **T2 and T3** both touch `crates/otaq-core/src/skills/mod.rs`: T2 rewrites it and T3 adds a `pub mod` line.
     Keep T2's file with T3's line.
   - **T4 and T6** may both edit `plugin/skills/*/SKILL.md`. T6 only touches the `flow` skill's write-skill section.
     Keep both sides.
   - **T5 and T6** both add work at service start: T5 sweeps marked copies, T6 deletes `$OTAQ_HOME/skills/` and
     `skill-writes/`. Keep both.
   - **T7 and T8** both regenerate `crates/otaq-desktop/ui/src/generated/`. After merging both, rerun
     `cargo test -p otaq-desktop generated_types_are_fresh` and commit any regenerated file.
   - **Test files** shared between tasks (`every_request.rs`, `plugin_texts.rs`, `migrations.rs`): keep both sides.
3. **Check** after each merge: run `scripts/check.sh lint test` in the base worktree, and the full
   `scripts/check.sh` after T7 or T8. A red run, beyond the failures noted at setup, blocks further merges until
   it's fixed. Either send the task's subagent back, or start a short fix subagent working in the base worktree.
   - Tests T2 marked `#[ignore = "T4"]` are expected until T4 is merged. After T4, none may remain.
4. Start any task whose dependencies are now all merged.

## Hand checks for you

After T7 and after T8 are merged, read the subagent's description of the screen it changed. If anything reads oddly,
send it back. Don't launch the app yourself: T10 does the real-tool checks.

## Finish

When every task is merged, failed, or blocked:

1. Run the full `scripts/check.sh` in the base worktree (`../otaq-no-skill-copies`).
2. Report to me:
   - each task's status (merged / failed / blocked) and a one-line summary of what it changed;
   - the final check result, with its real output summarised;
   - T10's table (tool × check), with every failure and every "not checked";
   - T1's packaging choice and how it behaves on Windows, and T3's final folder list;
   - any product question a task stopped on, and any stopgap edits a task reported.
3. Leave the worktrees and branches in place.
4. **Don't push, don't open a PR, and don't merge into `main`.** I'll decide what happens next.
