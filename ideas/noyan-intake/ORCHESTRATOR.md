# Orchestrator prompt: Noyan intake-only

Paste everything below the line into a fresh Claude Code session started at the repository root
(`/Users/nevzattopcu/Desktop/Workspace/otaq`).

---

You are the orchestrator for a set of tasks in the Otaq repository (`/Users/nevzattopcu/Desktop/Workspace/otaq`). You
don't implement the tasks yourself. You set up the branches, start one subagent per task, run tasks in parallel
whenever none depends on another, check each result, and merge the results together. You report to me at the end.

## Background

We changed how work gets split for agents. When an agent gets a ticket that is too big, its context grows until it
gets confused. Noyan, the project's agent that files tickets, also sometimes skipped the planning stages on medium
and large work. The agreed fix:

- Noyan only files tickets. It sets no stage choices of its own.
- The flow's planning stages and the Dev agent split the work, using a concrete size measure.
- Agents never skip stages.
- The service enforces the rules the app depends on.

Every decision is in `ideas/noyan-intake/DECISIONS.md` (N1–N14), and the work is split into tasks T1–T8 in
`ideas/noyan-intake/tasks/`. These files are the source of truth for this work. The old `decisions/` folder is lost:
don't look for it or write to it. Treat decision numbers cited in code and tests (d0129, d0133…) as history.

## Read first

1. `ideas/noyan-intake/DECISIONS.md`.
2. Every task file in `ideas/noyan-intake/tasks/` (T1–T8).
3. `CLAUDE.md` and `scripts/README.md` (how `scripts/check.sh` runs and what it needs).

## Setup (do this before starting any task)

**Never change the main checkout** (`/Users/nevzattopcu/Desktop/Workspace/otaq`): don't switch its branch, and don't
commit, stash or unstage anything in it. Work only in worktrees of your own:

1. Create the base worktree and branch from the current HEAD:
   `git worktree add ../otaq-noyan-intake -b noyan-intake HEAD`
2. The `ideas/noyan-intake/` folder isn't committed in the main checkout (it may be staged), so the new worktree
   doesn't have it. Copy
   it into `../otaq-noyan-intake/ideas/noyan-intake/` and commit it there as the first commit on `noyan-intake`
   (`noyan-intake: decisions and tasks`).
3. In the base worktree, run `scripts/check.sh lint test` once, to know the starting state. If it's already red,
   write down which tests fail, so you don't blame a task for them, and tell me in the final report.

## Dependency graph

```
T1 service guard ─────────────────────────────┐
T2 noyan skill ───────────────────────────────┼──► T6 docs (needs T2 and T5)
T3 milestone-plan size measure                │
T4 plan mid-build ──► T5 work-ticket splitting┘
T7 audit (read-only report)
T8 clear old choices (migration)
```

- **Wave 1, all in parallel:** T1, T2, T3, T4, T7, T8.
- **T5** starts as soon as T4 is merged into `noyan-intake`. Don't wait for the rest of wave 1. T5 reads T4's
  `ideas/noyan-intake/tasks/T4-findings.md`.
- **T6** starts as soon as T2 and T5 are merged.
- Never start a task before everything it depends on is merged.

## Running a task

For task `T<n>`:

1. Create its worktree from the **current** `noyan-intake`:
   `git worktree add ../otaq-T<n> -b noyan-intake/T<n> noyan-intake`
2. Start one subagent for it, in the background, with no `isolation` option: you made its worktree yourself. Give it
   this brief, filled in:

   > You work only in `/Users/nevzattopcu/Desktop/Workspace/otaq-T<n>` (a git worktree on branch
   > `noyan-intake/T<n>`). Never touch any other folder, especially `/Users/nevzattopcu/Desktop/Workspace/otaq`.
   >
   > Read `ideas/noyan-intake/DECISIONS.md`, then `ideas/noyan-intake/tasks/<task file>`, and do exactly that task.
   > - Use the canonical texts in DECISIONS.md word for word: N6's size measure, N11's and N12's refusal texts, and
   >   N13's defining sentence.
   > - In code comments and tests, name a rule in words ("only the user skips a stage"), never by its N-number.
   >   A task reference such as "(T1)" is fine.
   > - If the task needs a product decision that DECISIONS.md doesn't cover, stop and report it. Don't decide it
   >   yourself.
   > - Stay inside the task's scope. Note anything else you notice in your report instead of fixing it.
   > - Run the task's "Done when" checks in your worktree and report their real output, including failures.
   > - Commit on your branch with the message `noyan-intake T<n>: <summary>`, ending with the commit attribution lines
   >   your session gives you. Never push.
   > - Finish with a report: files changed, checks run and their results, anything left undone, and any product
   >   question you hit.

3. **T7 is read-only.** Its output is `ideas/noyan-intake/T7-audit.md`. It commits only that file, and you merge it
   like any other task.

## Checking and merging

When a subagent reports back:

1. **Review** its diff (`git -C ../otaq-noyan-intake diff noyan-intake...noyan-intake/T<n>`) against the task file's
   "What to change" and "Done when". Check that every canonical text is exact.
   - If it falls short, send that subagent back (SendMessage) with a specific note on what's wrong and where.
   - Do this at most twice. Then mark the task failed, and carry on with the tasks that don't depend on it. Tasks that
     depend on a failed task don't start, and are reported as blocked.
2. **Merge** it into `noyan-intake` in the base worktree, one task at a time
   (`git -C ../otaq-noyan-intake merge --no-ff noyan-intake/T<n>`).
   - T2, T3 and T5 all add assertions to `crates/otaq-core/tests/suite/plugin.rs` / `plugin_texts.rs`, in
     different test functions. Resolve those conflicts by keeping both sides.
   - T1 and T8 may both touch stage-choice tests. Keep both sides there too.
3. **Check** after each merge: run `scripts/check.sh lint test` in the base worktree. A red run (beyond the failures
   noted at setup) blocks further merges until it's fixed, either by sending the task's subagent back or by a short
   fix subagent working in the base worktree.
4. Start any task whose dependencies are now all merged.

## Finish

When every task is merged, failed, or blocked:

1. Run the full `scripts/check.sh` in the base worktree (`../otaq-noyan-intake`).
2. Report to me:
   - each task's status (merged / failed / blocked) and a one-line summary of what it changed;
   - the final check result, with its real output summarised;
   - T7's proposed guards (from `T7-audit.md`);
   - any product question a task stopped on.
3. Leave the worktrees and branches in place.
4. **Don't push, don't open a PR, and don't merge into `main`.** I'll decide what happens next.
