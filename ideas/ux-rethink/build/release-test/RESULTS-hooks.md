# Git hooks · how Otaq behaves in a repository with hooks · results (Windows 11 ARM64 VM, build release-fixes 5e8e9fc7)

Date: 2026-11-11, 11:07–11:50 VM time. This was an exploration, not a pass/fail re-test: it feeds the next milestone's
plan. No Otaq code was changed and nothing was committed to the Otaq repo or pushed anywhere real (the only remote was
a local bare repo in the VM). Screenshots: `shots-hooks/` next to this file, as JPEG. Windows paths that show the user
name are blacked out. Paths the tools shorten to `~\otaq-reltest\…` are left as they are. No logins or emails were on
screen.

## Summary

Otaq runs the user's own `git` with the user's hooks and never passes `--no-verify` (`git/mod.rs`,
`git/checkpoint.rs`). Agents see a hook's output and work with it, and Otaq's own merge leaves everything clean when a
hook refuses. What is missing is everything **around** the hooks:

- The squash message Otaq proposes always starts `KEY: title`, and a `feat:`/`fix:` commit-msg rule always rejects it.
  After a refusal the dialog throws the user's edit away.
- A `pre-rebase` hook makes **every** Approve & merge after the base has moved impossible. The toast shows only git's
  last line, so the hook's own reason is lost.
- The checkpoint commit (`wip: paused by Otaq`) can never pass a commit-msg rule. A reopened ticket with uncommitted
  files can then never start, and the error blames "your pre-commit hook".
- A pre-commit hook that rewrites files changes what lands on main after the user approved the diff, and nothing says
  so.
- A `post-checkout` hook that fails the worktree add fails the start once. The retry then skips the hook silently.
- Slow hooks during Approve & merge: the progress dialog stays on step 1, "Fetch main", for the whole run (46 s and
  82 s here). There is no timeout and no hook output.
- Push: Otaq never runs `git push` itself. Every push is the agent's own command, and Otaq's prompts tell it never to
  push. With a slow `pre-push` (50 s), Codex moved on after about 30 s while the push was still running, and told Otaq
  "the push reported no completion" when it had in fact succeeded.

Verdicts by scenario: 1 OK (with notes) · 2 problem · 3 (pre-push) problem/missing · 4 problem · 5 missing ·
6 problem · 7 problem.

## Setup

- Repo `%USERPROFILE%\otaq-reltest\hooksproj` (README, CONTRIBUTING, `src/calc.js`, `test.js` → `node test.js` prints
  ok, `test-push.js` that always fails, `package.json`), `main`, with a local bare remote
  `%USERPROFILE%\otaq-reltest\hooksproj-remote.git` as `origin`. Git 2.56.0.windows.4. The release test's
  `GIT_CONFIG_GLOBAL` was used.
- Added to the running app (5e8e9fc7, release-test data) as project **HOOK** with Otaq's default flow (Dev → Validate
  → Review, Developer and Checker on Codex), delivery "Either", Merge check `node test.js`. Settings showed "Pull
  requests through: None detected · origin is C:\…\hooksproj-remote: no GitHub or Azure DevOps repository".
  `setup-project-added.jpg`
- Hooks were `sh` scripts in `.git/hooks`, switched per scenario by a small script. **Every** hook name
  (pre-commit, prepare-commit-msg, commit-msg, post-commit, pre-merge-commit, post-merge, pre-rebase, post-checkout,
  post-rewrite, pre-push, pre-auto-gc) logged its name, `$PWD` and arguments to a file. That shows exactly which of
  Otaq's steps run which hook, and where. Test hooks: TODO lint (pre-commit), message must start with `feat: `/`fix: `
  (commit-msg), `HOOK-n:`/feat/fix allowed (commit-msg), sleep 45 s (pre-commit), header-adding reformatter
  (pre-commit), `exec < /dev/tty; read` (pre-commit and pre-push), always-fail (pre-rebase, pre-merge-commit,
  post-checkout), sleep 40 s (post-checkout), `node test-push.js` fails (pre-push), sleep 50 s (pre-push).
- Husky style once: `.githooks/` committed on main (pre-commit lint, commit-msg, 50 s pre-push, post-checkout log) with
  `core.hooksPath=.githooks` in the repo config.
- Tickets: HOOK-1 "Add subtract to calc", HOOK-2 "Add multiply to calc", HOOK-3 "Add divide to calc", all on Codex.
  Validate was skipped with Move → Review to save time (the Checker never ran a git step). Every Codex prompt was
  answered `y` or Esc, never `p`.
- Typing went through the clipboard. **The VM's clipboard is shared with the Mac**: twice, a paste brought in text
  that someone else had just copied (a ticket title and a URL). Both were removed at once: the title was changed with
  `otaq update` and the comment was deleted. Neither is in any screenshot. Future runs should paste and check right
  after `Set-Clipboard`.

## Which hooks Otaq's own git steps run (from the hook log)

| Otaq step | Hooks that ran | Where |
|---|---|---|
| Start: `git worktree add` (new ticket) | post-checkout (`0000… <base> 1`) | the new worktree `home\wt\<repo>\<KEY>` |
| Start: retry when the worktree is already registered | **none** (the add is skipped) | |
| Start of a reopened ticket: checkpoint commit `wip: paused by Otaq` | pre-commit, prepare-commit-msg, commit-msg | the ticket's worktree |
| Start of a reopened ticket: rebase onto base | pre-rebase, post-checkout, post-rewrite (husky `.githooks/post-checkout` ran too) | the ticket's worktree |
| Approve & merge: free rebase, when base moved | pre-rebase, post-checkout, prepare-commit-msg + post-commit per replayed commit, post-rewrite. **Not** pre-commit or commit-msg | the ticket's worktree |
| Approve & merge: scratch checkout | post-checkout (40 s slow hook ran here too) | `home\scratch\<repo>` |
| Approve & merge: `git merge --squash` | post-merge (`1` = squash) | scratch |
| Approve & merge: `git commit -F <msg>` | pre-commit, prepare-commit-msg, commit-msg, post-commit | scratch |
| Approve & merge: `git merge --ff-only` into the user's checkout | post-merge (`0`) | **the user's own repo folder** |
| Approve & merge: pre-merge-commit | never (Otaq squashes; no merge commit) | |
| Add project, Remove project, fetch | none | |
| `git push` | never run by Otaq: only by agents (see Pre-push) | |

The hook log fits the code: `git/merge.rs` (squash, `commit --allow-empty -F`, `merge --ff-only`),
`git/checkpoint.rs` (checkpoint, `rebase --no-autostash`), `start/worktree.rs` (`worktree add --quiet`),
`helpers/merge.rs` (a helper's squash into the agent's branch, `commit -F`, not exercised here). Every call is
`Git::run`: stdin is `/dev/null`, there is **no timeout** (only `kill_on_drop`), and the user's hooks always apply. A
failure becomes `git <args> failed (exit N): <stderr>`. PR rewording (`git/reword.rs`) uses `git commit-tree`, so the
user's commit-msg hook **never** checks the messages confirmed in the PR dialog.

## Scenarios

### 1 · An agent's commit is rejected by a hook — OK (with notes)
- HOOK-1 asked for `subtract` with a `// TODO: handle floats` comment, so it clashed with the TODO lint on purpose.
  Hooks: TODO lint, `feat:`/`fix:` commit-msg, failing pre-push.
- First commit (`HOOK-1: add subtract to calc`): the pre-commit refused it. Codex read the hook's message, said "I
  can't satisfy both instructions **or bypass the hook**", and asked on the ticket with `otaq ask human` (choices: keep
  the TODO, change the comment). The card and the sheet showed **Asks you** with the question.
  `s1-precommit-rejects-agent-asks.jpg`, `s1-question-on-overview.jpg`
- After my answer ("write NOTE, commit with a message the hooks accept"), it committed `HOOK-1: …` again and the
  commit-msg hook refused it. Codex saw that, re-ran as `feat: HOOK-1 add subtract to calc`, and it passed.
  `s1-commitmsg-rejects-agent-adapts-feat.jpg`
- `--no-verify` was never tried. The `commit` skill says "If a hook rejects the commit, fix the problem and commit
  again; never use `--no-verify`" (`plugin/skills/commit/SKILL.md` step 5). The work-ticket skill also mentions that
  the project's hooks may refuse a commit.
- What Otaq showed: only what the agent chose to say. A hook refusal isn't an event of its own; the hook's text is in
  the session only. The default message format (`<KEY>: …`, commit skill step 4) is the first thing a `feat:`/`fix:`
  rule rejects, so each commit costs a refused try and the tokens to retry. The husky run (HOOK-3) had the
  agent read `.githooks` and CONTRIBUTING first, and it got the format right the first time.

### 2 · Approve & merge with failing hooks — problem
- **commit-msg** (`feat:`/`fix:` only): the delivery agent's squash message, as Otaq tells it to write it, was
  "HOOK-1 Add subtraction to the calculator" (`s2-merge-dialog-message-starts-with-key.jpg`). Merge → toast "Approve &
  merge for HOOK-1 didn't go through · git commit failed: commit-msg: the message must start with "feat: " or "fix: "
  (got: HOOK-1 Add subtraction to the calculator)". The ticket stayed in Review, main didn't move, and the scratch
  worktree was reset. Recovery works: edit the message in the dialog. `s2-commitmsg-rejects-merge-toast.jpg`,
  `…-zoom.jpg`
- **The edit is lost after a refusal.** I edited the first line to `feat: …`. A pre-commit hook then refused the next
  try, and when I reopened Approve & merge, the message was back to "HOOK-1 Add subtraction…". The user has to fix it
  again on every try. `s2-retry-dialog-lost-edited-message.jpg`
- **Interactive pre-commit** (`exec < /dev/tty`): it failed at once (no hang) with "git commit failed: C:/…/.git/hooks/
  pre-commit: line 3: /dev/tty: No such device or address". The toast shows the full hook path, with the Windows user
  name in it. A hook that asks "commit anyway? [y/N]" can never pass in Otaq.
  `s2-interactive-precommit-fails-at-once.jpg`, `…-zoom.jpg`
- **pre-merge-commit**: never runs (squash, no merge commit), so a team's pre-merge-commit gate is silently skipped.
- **Reformatting pre-commit** (adds `// @formatted by the pre-commit hook` at the top of each staged `.js`): HOOK-3
  merged with "Squashed as a275c51". The commit on main has that line in `src/calc.js` and `test.js`; the branch the
  user reviewed doesn't (`git diff otaq/HOOK-3 main`: 2 lines). Nothing in Otaq says the merged result differs from
  the reviewed one, and the Merge check doesn't rerun after the commit. In the code the check runs on the commit, but
  only "when main has moved", and here it hadn't. `s2-reformatting-precommit-merged-silently.jpg`
- Failed attempts leave no trace on the ticket: History lists nothing for the two refused merges. Only the toast
  (until it's closed) and a WARN line in the service log remain.
- **post-merge runs in the user's own checkout** after every successful merge (`post-merge 0` in `hooksproj`). With
  husky-style "npm install on post-merge" hooks, that's an install in the user's folder that Otaq doesn't show.

### 3 · Pre-push (every push path) — problem / missing
**Which pushes exist.** Otaq never runs `git push`. The only `push` in `crates/` is in `git/testing.rs`. Every push is
an agent's own `git push` in its worktree:
- (a) an agent asked to push by the user (ticket text, comment, Tell now);
- (b) **Approve & open PR** → the delivery agent runs the `open-pr` skill (turn 2: `git push -u origin <branch>`, then
  `gh pr create` / `az repos pr create`); **Push to PR** after a Send back → the `push` skill. Before that, Otaq renames
  the branch and rewords commits itself with `commit-tree` (no hooks).

Path (b) **could not be tried**: with a local `origin`, "Approve & open PR" is disabled ("PR provider not detected",
`pp-b-open-pr-disabled-no-provider.jpg`), and `gh`/`az` aren't installed in the VM. From the skills: on a rejected push
the agent must not "retry with other options" and must tell the user with `otaq ask human --stdin` ("open-pr" step 8,
"push" step 6). So a failing pre-push would show as the ticket's **Asks you**, worded by the agent, with the commits
already reworded and the branch renamed by Otaq. Neither skill mentions hooks or `--no-verify` for push (only the
commit skill does).

- **(a) Failing pre-push** (`node test-push.js` → "integration: expected 200, got 500"): Codex first refused to push
  at all: "Otaq's ticket rules explicitly prohibit pushing" (work-ticket and commit skills: "Never push"). It
  pushed only once I answered its question with "I explicitly want this push". A comment didn't make it push either
  (HOOK-3, `pp-agent-refuses-push-from-comment.jpg`); I had to say "I override the no-push rule" with Tell Developer
  now. Then the hook refused. Codex showed the hook's output ("pre-push: tests failed, push refused. error: failed to
  push some refs…") and said "I won't bypass that hook". It reported the refusal in its Dev hand-off and went on to
  Validate, without asking or trying to fix. Fair here, since the failing test was outside the ticket.
  `pp-a-agent-pushes-on-request.jpg`, `pp-a-prepush-refuses-agent-reports-no-bypass.jpg`,
  `pp-a-prepush-output-in-session.jpg`
- **(c) Slow pre-push** (husky `.githooks/pre-push`, `sleep 50`): the UI didn't hang. The card said Working, and the
  session showed "Running git push … husky pre-push: running the integration suite (about 50 s)…". At about 30 s
  Codex stopped waiting and went on (`git status -sb` with the push still "Running"). The push then **succeeded**
  ("* [new branch] otaq/HOOK-3 -> otaq/HOOK-3"), but Codex's stage hand-off said "the push attempt reached the
  pre-push hook but reported no completion; local status shows no upstream". The ticket would have gone on with a
  wrong report. Otaq has no push step of its own to time, so there's nothing it shows or times out. On path (b),
  Otaq would wait for the agent's `otaq pr opened` and show nothing about a long push.
  `pp-c-slow-prepush-agent-working-29s.jpg`, `pp-c-slow-prepush-codex-moves-on-push-still-running.jpg`,
  `pp-c-push-succeeded-but-handoff-says-no-completion.jpg`
- **(d) Interactive pre-push** (`exec < /dev/tty`): it failed at once, "…/.git/hooks/pre-push: line 3: /dev/tty: No
  such device or address", with no hang. Codex reported it accurately and didn't bypass it.
  `pp-d-interactive-prepush-fails-no-tty.jpg`
- Husky (`core.hooksPath=.githooks`, relative): worktrees and the scratch worktree use their **own checkout's**
  `.githooks`, at the branch's version. During HOOK-3's rebase, `.githooks/post-checkout` ran right after the checkout
  that brought `.githooks` in. Hooks in `.git/hooks` are ignored while `core.hooksPath` is set.

### 4 · Rebase with a pre-rebase / post-checkout hook — problem
- HOOK-2 committed while I moved main on (a README commit). Approve & merge then rebases the agent's branch in its
  worktree first (`rebase --no-autostash --quiet <main>`).
- **pre-rebase that refuses** ("rebasing is not allowed in this repository (merge instead)."): the toast reads "git
  rebase failed: The pre-rebase hook refused to rebase." The hook's own reason is lost: the service log has the full
  stderr, the toast has only git's last line. Main, the branch and the worktree are unchanged and the ticket stays in
  Review. But **there is no way to merge**: Otaq always rebases when the base has moved, the restack skill tells the
  agent to `git rebase` too, and no merge-instead option exists. Only removing the hook helps.
  `s4-prerebase-refuses-merge-toast.jpg`, `s4-prerebase-toast-zoom-hook-reason-lost.jpg`
- Slow **post-checkout** during that rebase: see 5.
- Replayed commits ran prepare-commit-msg and post-commit but **not** pre-commit or commit-msg (git's rebase
  backend). So rules aren't re-checked on rebased commits, which is fine. The squash commit is checked anyway.
- Not run: a real conflict → `restack` with a pre-rebase hook (too costly in agent turns). From the code and the skill,
  the agent's own `git rebase main` hits the same hook, and the skill doesn't say what to do then.

### 5 · Slow hooks during Otaq's git steps — missing
- Slow pre-commit (45 s) in the squash commit: the "Approve & merge HOOK-1" progress dialog stayed on step 1, **"Fetch
  main and bring it up to date"** (spinner), at 2 s, 12 s and more, while git was really on step 3 (the hook log puts
  the pre-commit at 11:22:13 and the commit at 11:22:58). It finished at about 46 s with "HOOK-1 merged into main ·
  Squashed as ea65599". `s5-slow-precommit-merge-shows-fetch-step-12s.jpg`, `s5-slow-precommit-merge-done-46s.jpg`
- Slow post-checkout (40 s) on HOOK-2's merge: it ran twice (rebase in the worktree, then the scratch checkout). The
  dialog showed "Fetch main…" at 33 s and 60 s, and the merge finished at about 82 s. The rest of the app stayed
  usable (Close is there), but nothing says a hook is running or which one. Otaq has no timeout: a hook that hangs
  (for example one waiting on a network call) would keep "Merging…" up forever.
  `s5-slow-postcheckout-merge-still-fetch-step-33s.jpg`, `…-60s.jpg`, `s5-slow-postcheckout-merge-done-82s.jpg`
- The free rebase also stopped HOOK-2's delivery agent, which was mid-way through writing the message (its session
  read "Session ended (stopped)"). That's expected at a merge, but the merge used the plain message.

### 6 · Worktree creation with post-checkout hooks — problem
- Every new worktree runs post-checkout (`0000… <base> 1`) in the new folder. Husky-style setups (for example
  `npm install`, or generating files on post-checkout) run once per ticket, and Otaq's Workspace "Setup command" runs
  on top of that.
- **Failing post-checkout**: HOOK-3's start failed: card "Start failed · creating the worktree failed", sheet "git
  worktree add --quiet C:\…\home\wt\hooksproj\HOOK-3 otaq/HOOK-3 failed (exit 1): post-checkout: failing on purpose",
  with **Retry start**. Clear enough. But git had already made the worktree and checked out the files, and **Retry
  start then worked with the hook still failing**: the worktree is registered and its folder exists, so Otaq skips
  `worktree add` and the hook never runs again. Whatever the hook was meant to do (install, codegen) silently never
  happens for that ticket. `s6-postcheckout-fails-start-failed.jpg`
- Slow post-checkout at start: not timed separately. The same `Git::run` has no timeout and no progress, and the
  merge runs in 5 show what a slow post-checkout does.

### 7 · What Otaq commits itself, with commit-msg rules — problem
- **The squash commit** (`KEY: title` + body, from the delivery agent or the plain one): it always fails a
  `feat:`/`fix:` rule (2). Neither the delivery agent's instructions nor the default ("one line starting with
  HOOK-2") look at the project's rules.
- **The checkpoint commit** (`wip: paused by Otaq`) on a reopened ticket with uncommitted files: HOOK-3 was closed with
  an untracked file in its kept worktree, reopened, and started. "Start failed: creating the worktree failed · **Your
  pre-commit hook** rejected Otaq's temporary checkpoint. Commit or discard the files in HOOK-3, then retry." It was
  the **commit-msg** hook (pre-commit passed, per the hook log), and its message isn't shown. The heading blames the
  worktree. Files and index were left exactly as they were (`?? notes-wip.txt`). With any message-format rule, this
  checkpoint can never pass, so the only way on is to commit or delete the work by hand.
  `s7-checkpoint-rejected-by-commitmsg-says-precommit.jpg`, `s7-close-keeps-worktree-with-uncommitted.jpg`
- **PR rewording** (`git/reword.rs`, `commit-tree`): the user's commit-msg hook never sees the messages confirmed in
  the PR dialog, and nothing in that dialog says what the project's rule is. (Code reading only: no PR provider.)
- Skill files Noyan writes are left uncommitted for the user ("written but not committed yet"), so they don't involve
  Otaq's own commit.

## What Otaq is missing (ranked by how much it would bite a real user)

1. **Commit message rules for Otaq's own commits.** Teams with commitlint or Conventional Commits can't merge until
   they fix the squash message by hand on every ticket, and a reopened ticket with uncommitted work can never start
   (the `wip: paused by Otaq` checkpoint). Otaq needs to (a) let the project set the squash/checkpoint message format
   (or follow the Agent instructions/CONTRIBUTING when the delivery agent writes it), and (b) say which hook refused
   (commit-msg vs pre-commit) and show its output.
2. **No way to merge past a `pre-rebase` hook** (or any team that forbids rebases). Every Approve & merge after the
   base moves fails, and the restack skill hits the same wall. It needs a merge option that doesn't rebase the agent's
   branch (merge the base into it, or squash from the merge-base), or at least a clear "your pre-rebase hook forbids
   Otaq's rebase" message.
3. **Push guidance for agents.** "Never push" in the work-ticket and commit skills fights a user who asks for a push,
   and the push/open-pr skills say nothing about pre-push hooks: wait for them, report the real result, never
   `--no-verify`, and ask the user when a hook refuses. A slow pre-push made Codex hand in a wrong report (the push had
   succeeded). Otaq could check the outcome itself (`git ls-remote`/upstream) after `otaq pr opened` or the stage end,
   instead of trusting the agent's text.
4. **Hook output in Otaq's own failures.** Toasts show git's last line ("The pre-rebase hook refused to rebase."), not
   the hook's reason. They show the full Windows hook path, with the user name. They vanish, and nothing is written
   on the ticket's History. Failures need a lasting entry with the hook name and its full output, and a "Show output".
5. **Approve & merge forgets the edited message after a failure.** It needs to keep the user's edit for the retry.
6. **Silent changes from reformatting pre-commit hooks.** The commit that lands on main can differ from the reviewed
   diff without notice. Otaq should compare the commit's tree with the reviewed one and say "your pre-commit hook
   changed 2 files" (and maybe show the diff), or rerun the Merge check when that happens.
7. **Progress and time limits around hooks.** The merge dialog sits on "Fetch main" while hooks run in later steps.
   It needs the real step, "running your pre-commit hook… 40 s", a Stop, and a generous timeout (no hook should keep
   "Merging…" up forever). The same goes for a start's `worktree add` (post-checkout).
8. **Retry start skips a failed post-checkout.** A worktree whose post-checkout failed is reused on retry and the hook
   never reruns. Otaq should remove the half-made worktree on failure, or rerun the hook (`git checkout` of the same
   commit) on retry.
9. **Hooks that need a terminal can never pass** in Otaq's steps (stdin is `/dev/null`) or in agent sessions. That's
   fine, but Otaq should name it: "this hook reads from the terminal; Otaq can't answer it".
10. **pre-merge-commit is never run** (squash merges make no merge commit). Teams that gate merges there get no gate;
    Otaq should document it, or run the project's pre-merge-commit itself before moving the target.
11. **Hooks in the user's own checkout**: the fast-forward runs `post-merge` in the user's folder after every merge.
    Otaq should say so in the merge dialog ("your post-merge hook runs in ~\…") when the repo has one.

## Setup changes (what was put back)

| Change | Why | State now |
|---|---|---|
| Repo `~\otaq-reltest\hooksproj`, bare remote `hooksproj-remote.git`, hook library `hooks-lib`, `hooks-log.txt` | the test | deleted |
| Project HOOK (HOOK-1, HOOK-2, HOOK-3: all merged into the test repo's main) | the test | removed with Settings → Remove project; leftover empty `home\res\hooksproj` and `home\warm\hooksproj.trash` deleted; `home\wt\hooksproj` and `home\scratch\hooksproj` were still there after the removal (contents not checked) and were deleted with the repo |
| Codex permission answers | `y` or Esc only, never `p` | no Codex rules added |
| VM clipboard | used for typing | holds my last test text; a foreign title and URL that got pasted were removed from the ticket at once |
| Sign-ins, shop, notes | untouched | unchanged |

## State left in the VM
The installed app (**5e8e9fc7**) is running on screen on the release-test data (`otaq-app.exe` and `otaq.exe` only; no
codex, claude, copilot or opencode process). `otaq project list` shows SHOP and NOTE only. shop and notes weren't
touched.
