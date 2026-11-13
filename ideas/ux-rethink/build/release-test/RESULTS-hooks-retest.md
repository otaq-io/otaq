# Git hooks · Windows retest of the git-hooks work · results (Windows 11 ARM64 VM, build release-fixes 8f0d3893)

Date: 2026-11-11, 16:00–17:10 VM time. This repeats the hooks exploration of `RESULTS-hooks.md` on the build that
fixes its 11 issues (plan `tuq/plans/git-hooks.md`, 16 pieces, decisions 026–036, 038, 039, with today's refinements
of 026, 030, 034 and 035). No Otaq code was changed and nothing was committed to the Otaq repo or pushed anywhere
real: the only remote was a local bare repo in the VM. Screenshots are in `shots-hooks-retest/` as JPEG. The Windows
user name was blacked out wherever it showed (only in Codex's own session text, never in Otaq's UI). No logins or
emails were on screen.

## Summary

14 of the 16 pieces pass in the real app; piece 16 fails at the retry; piece 13 passes its main check but its retry
didn't finish. The core of the work holds on Windows. A refusal of each kind ends up on the ticket, with the hook's
name and its full output with `~/` paths, whether the dialog was open or closed, and the ticket stays in Review. The
dialog and the card show the real step and the running hook with a growing time. Stop works on a hanging pre-commit
and on a hanging post-checkout and leaves no git, sh or sleep process. A failed start leaves nothing behind. The
reopened ticket's temporary save runs no hook. The hook log shows that pre-rebase never ran all session.

- **FAIL, piece 16:** after commit-msg refuses a PR commit message, **Edit messages and retry** opens the Open PR
  dialog, which has **no field for commit messages** (only branch, title and description). The user can't fix the
  refused message in Otaq. Retrying refuses the same message again. It only went on after I amended the commit by
  hand in the worktree.
- **Bug (Codex on Windows):** when Otaq types a skill command on its own, `$otaq:push` (Retry push) or
  `$otaq:restack` (conflict), it lands in Codex's input line but **isn't submitted**. The Push dialog then sits on
  "Push skill is preparing…" until someone presses Enter in the session.
- Smaller findings: a question an agent asks while its ticket waits in Review isn't shown on the ticket. The
  "Pushing…" dialog can't be closed while the delivery agent hasn't reported. The heading after a user's Stop of a
  start says "creating the worktree failed".

| Piece | Result |
|---|---|
| 1 Git steps know the hook, with its whole output | PASS (in the app) |
| 2 Refused Approve & merge on the ticket | PASS |
| 3 Refused start on the ticket | PASS |
| 4 Messages follow the project's rules | PASS (no-rules half NOT RUN) |
| 5 Real step and running hook shown | PASS |
| 6 Stop, no time limit | PASS |
| 7 Edited message kept | PASS (merge message; PR fields see 16) |
| 8 Merge main in, never rebase | PASS |
| 9 Temporary save skips hooks | PASS |
| 10 Failed start leaves nothing; Retry starts over | PASS |
| 11 Hook's changes to the landing commit said | PASS |
| 12 Agents push when asked | PASS (note on the question in Review) |
| 13 Otaq checks the delivery push | PASS for the mismatch; "real push shows nothing extra" and Retry push NOT completed |
| 14 Terminal hook named | PASS |
| 15 post-merge in the user's folder shown, kept | PASS (card; the dialog was closed at that moment) |
| 16 PR messages through commit-msg | **PASS on re-check** (release-fixes 1af8dafb; was FAIL on 8f0d3893: the way on couldn't fix the message). See "Re-check of piece 16" |

## Setup

- Built in the VM from a git bundle of `release-fixes` (8f0d3893) cloned to `%USERPROFILE%\otaq-build\retest-hooks`,
  shared `CARGO_TARGET_DIR`, `npm.cmd run bundle -- --bundles nsis` with `createUpdaterArtifacts:false`, installed
  with the setup `.exe /S` into `%LOCALAPPDATA%\Otaq`. It was started on the release-test data through
  `otaq-reltest\launch.cmd` (OTAQ_HOME `otaq-reltest\home`, `GIT_CONFIG_GLOBAL` `otaq-reltest\gitconfig`) with a new
  scheduled task `OtaqRetestHooks`. Git 2.56.0.windows.4, Codex as every agent.
- **The test project was gone** (the exploration deleted it), so I set it up again the same way:
  `%USERPROFILE%\otaq-reltest\hooksproj` (README, CONTRIBUTING with the Conventional Commits rule and "no TODO",
  `commitlint.config.js` allowing `feat`/`fix`, `src/calc.js`, `test.js`, `package.json`), `main`, and the local bare
  remote `hooksproj-remote.git` as `origin`. I also committed two project skills under `.agents/skills/`, used only as
  the delivery's PR skill: `local-pr` (pushes to the local origin) and `release-bot-pr` (reports `otaq pr opened`
  **without pushing**: the fake delivery for piece 13).
- Added in the app as project **HOOK** with the default flow (Dev → Validate → Review, Developer and Checker on Codex),
  delivery Either, Merge check `node test.js`. Validate was skipped with Move → Review to save time. Tickets HOOK-1
  subtract, HOOK-2 multiply, HOOK-3 divide and HOOK-4 power were made with `otaq create`. Codex prompts were answered
  `y` or Esc, never `p`.
- Hooks were `sh` scripts in `.git/hooks`. Every hook name (pre-commit, prepare-commit-msg, commit-msg, post-commit,
  pre-merge-commit, post-merge, pre-rebase, post-checkout, post-rewrite, pre-push, pre-auto-gc) starts by logging its
  name, `$PWD` and arguments to `%USERPROFILE%\otaq-build\retest-hooks-lib\hooks-log.txt`. A switcher
  (`retest-hooks-lib\hk.ps1 <hook> <variant>`) writes the variant into `.git/hooks/<hook>` and logs the switch.
  Variants: TODO lint (pre-commit), `feat:`/`fix:` only (commit-msg; like commitlint's default ignores it lets
  `Merge …` messages through, so Otaq's merge of main into a branch isn't refused by it), always-fail with two output
  lines (pre-commit, pre-merge-commit, post-checkout, **pre-rebase all session**), hang `sleep 100000`
  (pre-commit, post-checkout), 40 s slow lint + formatter that adds `// @formatted by the pre-commit hook` to each
  staged `.js` (pre-commit), `exec < /dev/tty; read` (pre-commit), fail-once (post-checkout), 20 s post-merge that
  prints two lines, failing post-merge, 50 s pre-push, failing pre-push.
- Typing went through the clipboard (`Set-Clipboard`, paste, check at once). Nothing foreign came in this time.

## The hook log (whole session)

`pre-rebase` never ran: the only line with "pre-rebase" in `hooks-log.txt` (136 lines) is my own "log started …
pre-rebase = fail all session" header. Whenever main had moved, Otaq merged main into the branch instead. Lines with
the user path shortened to `~`:

```
16:21:58 pre-merge-commit pwd=~/otaq-reltest/home/wt/hooksproj/HOOK-1            <- merge of main into HOOK-1, refused
16:22:44 pre-merge-commit / prepare-commit-msg (MERGE_MSG merge) / commit-msg / post-merge 0   (HOOK-1 worktree)
16:23:05 post-checkout + post-merge 1 (scratch)  16:23:25 pre-commit (40 s + formatter)  16:24:06 commit-msg, post-commit
16:24:06 post-merge pwd=~/otaq-reltest/hooksproj args=0                          <- the user's own folder
16:37:04 post-checkout HOOK-3 (failonce: refused)   16:37:39 post-checkout HOOK-3 (Retry start: ran again, passed)
16:38:43 pre-merge-commit HOOK-3                                                 <- reopened ticket's update, refused
16:39:14 pre-merge-commit, commit-msg (MERGE_MSG), post-merge 0 HOOK-3           <- retry; NO pre-commit/commit-msg for the save
16:48:03 pre-commit, prepare-commit-msg (merge), commit-msg  HOOK-3              <- restack agent's `git merge main` commit
16:49:18 post-checkout HOOK-4 (hang, stopped)       16:50:00 post-checkout HOOK-4 (Retry start)
16:58:59 commit-msg ×2 HOOK-4  (PR messages checked: 1st passes, 2nd "update stuff" refused) 16:59:59 commit-msg ×2 (pass)
```

## Pieces

### 1 · Otaq's git steps know which hook runs or refused, with its whole output — PASS (in the app)
Every refusal below named the right hook (pre-commit, commit-msg, pre-merge-commit, post-checkout and the
PR's commit-msg) with both of its output lines, and paths shortened to `~/…`
(`~/otaq-reltest/hooksproj/.git/hooks/pre-commit: line 3: /dev/tty: …`). A running hook was named while it ran
(pre-commit, post-merge, post-checkout). The unit tests themselves were run by the orchestrator's check.sh, not
here.

### 2 · A refused Approve & merge shows on the ticket — PASS
- **pre-commit, dialog open** (HOOK-1): the toast reads "HOOK-1: your pre-commit hook refused the merge · Its
  output and the way on are on the ticket". The card shows "Needs you · pre-commit hook refused the merge". The sheet
  has a callout "Your pre-commit hook refused the merge · It refused the squash commit", the full output, "Nothing
  was merged: main, the branch and the worktree are as they were. Edit the message so the hook accepts it, or fix what
  it found, then merge again." and **Edit message and retry**. History has "Merge didn't go through · pre-commit hook
  refused the merge" with the output. `p02-precommit-refused-dialog-open-toast-card.jpg`,
  `p02-precommit-refused-sheet-output-way-on.jpg`, `p02-precommit-refused-history-output.jpg`
- **commit-msg, dialog closed** (Esc right after Merge): the card shows "Needs you · commit-msg hook refused the
  merge", the toast points to the ticket, and History has the row with the output.
  `p02-commitmsg-refused-dialog-closed-history.jpg`, `p02-commitmsg-refused-card-needs-you.jpg`
- **pre-merge-commit, main moved on, dialog open**: the merge of main into the branch ran pre-merge-commit in the
  HOOK-1 worktree, which refused. The result is the same shape: "pre-merge-commit hook refused the merge", the output
  ("…refusing on purpose", "second line…", git's "Not committing merge; use 'git commit' to complete the merge."),
  and a History row. `p02-premergecommit-refused-dialog-open-history-toast.jpg`, `p02-premergecommit-sheet-callout.jpg`,
  `p02-premergecommit-history-output.jpg`
- After each refusal, main (`9e1a3f0`, then `1974e6d`), the branch (`c237df2`), the worktree (clean) and the user's
  folder were unchanged, the scratch worktree was clean, and the ticket stayed in **Review** (status chip "Delivery
  stopped", Needs you).

### 3 · A refused start shows on the ticket — PASS
- **post-checkout on a new ticket** (HOOK-3): the card shows "Start failed · post-checkout hook refused the start"
  with Retry. The sheet reads "Start failed: post-checkout hook refused the start · Your post-checkout hook refused
  the start when Otaq made the ticket's worktree. Nothing it made is kept, so Retry start runs the hook again", then
  "Output of your post-checkout hook" and Retry start. History has "Start failed" with the output.
  `p03-postcheckout-start-failed-card.jpg`, `p03-p10-postcheckout-sheet-output-retry.jpg`,
  `p03-postcheckout-history-output.jpg`
- **Reopened ticket's update from main refused by pre-merge-commit** (HOOK-3 closed with uncommitted files, a new
  commit on its branch, main moved on): "Start failed: pre-merge-commit hook refused the update from main · Your
  pre-merge-commit hook refused the merge of main into HOOK-3's branch. The branch and its files are as they were." It
  shows the full output and Retry start, and History keeps it. Branch `15ee9f5`, `M src/calc.js`, `?? notes-wip.txt`
  were all unchanged, with no leftover stash or wip commit.
  `p03-reopened-update-refused-by-pre-merge-commit-sheet.jpg`, `p03-reopened-update-refused-history.jpg`

### 4 · Otaq's merges and agents' commits follow the project's message rules — PASS (no-rules half NOT RUN)
- Every agent read CONTRIBUTING, `commitlint.config.js` and the commit-msg hook (`git rev-parse --git-path hooks`)
  first, and each commit passed the `feat:`/`fix:` hook on the first try: `feat: add subtraction (HOOK-1)`,
  `feat: add multiplication (HOOK-2)`, `feat: add division (HOOK-3)`, `feat: add power (HOOK-4)`. In the hook log,
  each commit-msg ran once per commit. `p04-agent-reads-rules-and-hooks.jpg`, `p04-agent-commit-feat-first-try.jpg`,
  `p04-hook2-agent-commit-feat.jpg`
- The delivery agent's prompt names the rules to look for. The proposed merge messages were
  `feat: add subtraction (HOOK-1)`, `feat: add multiplication (HOOK-2)` and `feat: add power to calc (HOOK-4)`.
  HOOK-1's merged on main as proposed (`5ea9b2d`), and so did HOOK-2's.
  `p04-delivery-message-prompt-reads-rules.jpg`, `p04-proposed-merge-message-feat.jpg`,
  `p04-hook2-proposed-message-feat.jpg`
- Note: when Approve & merge was opened before the delivery agent had written its message (HOOK-3), the dialog showed
  the plain `HOOK-3: Add divide to calc` ("Developer is still writing the message"). That plain message is unchanged
  by design and would fail this hook.
- NOT RUN: "in a project with no rules the key still leads". It needs a second project, and SHOP/NOTE were not to be
  touched.

### 5 · The real step and the running hook show — PASS
- With the 40 s pre-commit (plus the 20 s post-merge in every place it runs), the dialog moved "Fetch main and bring
  it up to date ✓ → Merge main into the branch (running your post-merge hook · 14 s) → Squash into main (running your
  post-merge hook · 11 s, then running your pre-commit hook · 5 s)". It has a Stop box and the note "Works in Otaq's
  own scratch worktree; only the last step moves main in your folder." `p05-dialog-merge-main-in-post-merge-14s.jpg`,
  `p05-dialog-squash-post-merge-11s.jpg`, `p05-dialog-squash-precommit-5s.jpg`,
  `p05-dialog-real-step-running-post-checkout.jpg`
- With the dialog closed, the card showed "Merging · running your pre-commit hook · 20 s", then "· 35 s".
  `p05-card-dialog-closed-precommit-20s.jpg`, `p05-card-dialog-closed-precommit-35s.jpg`
- Start: "Starting · running your post-checkout hook · 7 s", then "· 15 s", with Stop.
  `p05-p06-start-running-post-checkout-card-stop.jpg`, `p05-start-post-checkout-15s-zoom.jpg`
- Minor: while a retried merge runs, the sheet behind still shows the previous refusal's callout.
  `obs-sheet-during-retry-shows-old-refusal.jpg`, `p05-hang-precommit-dialog-11s-stale-callout-behind.jpg`

### 6 · Stop on a running git step, with no time limit — PASS
- **Hanging pre-commit, Stop from the card** ("Merging · running your pre-commit hook · 23 s"): the ticket went back
  to Review, "Ready for you". main `9e1a3f0`, the branch `c237df2` and the worktree were unchanged, and the scratch
  worktree was clean. Before Stop, the processes were `git commit` → `git.exe` → `sh …/pre-commit` → `sleep.exe 100000`.
  After it, `Get-CimInstance Win32_Process` listed **no** sh.exe, sleep.exe or git.exe. History has no row for a stop,
  which is fine. `p05-p06-card-running-precommit-23s-stop.jpg`, `p06-after-stop-back-in-review.jpg`,
  `p06-after-stop-history-no-stop-row.jpg`
- **Hanging post-checkout at a start, Stop** (HOOK-4): "Start failed · You stopped the start while your post-checkout
  hook was running. Retry start starts it again." with Retry start. The worktree was not registered, there was no
  folder, and no git/sh/sleep process was left. Retry start ran the hook again (16:50:00 in the log) and the ticket
  started. `p06-start-stopped-start-failed-retry.jpg`
- Wording nit: the heading for the user's own Stop is "Start failed: creating the worktree failed".

### 7 · An edited merge message is kept until the ticket lands — PASS
I edited HOOK-1's first line to "Subtraction for the calculator (HOOK-1)". The edit was kept after a commit-msg
refusal and a closed dialog (`p07-edit-kept-after-refusal-and-close.jpg`). It was also kept after killing
`otaq-app` and the `otaq` service and starting the app again (`p07-edit-kept-after-app-and-service-restart.jpg`).
**Use the proposed message** appeared while the box differed and put back `feat: add subtraction (HOOK-1)`
(`p07-edited-message-use-proposed-link.jpg`, `p07-use-the-proposed-message-restores.jpg`), and that message landed.
The PR side is limited by piece 16: the Open PR dialog has no commit-message fields.

### 8 · Otaq merges main in, never rebases — PASS
- With pre-rebase failing all session and main moved on, HOOK-1's Approve & merge landed. Its branch now ends in
  `Merge branch 'main' into otaq/HOOK-1`. The reopened HOOK-3 started (`Merge branch 'main' into otaq/HOOK-3`).
- **Conflict** (HOOK-3's divide against a new `square` on main): the dialog reads "Merging main in stopped —
  conflict · src/calc.js … It was aborted; main and your own checkout are unchanged. HOOK-3 went back to its agent,
  which merges main in". The restack agent ran `git merge main`, resolved the conflict and committed `Merge branch
  'main' into otaq/HOOK-3` (the hooks ran on it: 16:48). `p08-conflict-merging-main-in-goes-to-agent.jpg`,
  `p08-restack-agent-merges-main-in.jpg`, `p08-restack-merge-commit-made.jpg`
- The hook log shows that pre-rebase never ran (see above).

### 9 · Otaq's temporary save of unfinished files skips the hooks — PASS
HOOK-3 was closed with `?? notes-wip.txt` (containing "TODO: …") and `M src/calc.js` (`// TODO: add divide here`),
with the TODO-lint pre-commit and the `feat:`/`fix:` commit-msg active, then reopened. The refused try left everything
as it was (piece 3). On the retry it started. The hook log has only the merge commit's hooks (pre-merge-commit,
commit-msg on `MERGE_MSG`, post-merge) and **no** pre-commit or commit-msg for the save. Afterwards both files were back
uncommitted (`M src/calc.js`, `?? notes-wip.txt`, content intact).

### 10 · A failed start leaves nothing half-made; Retry start starts over — PASS
Fail-once post-checkout on HOOK-3: after the failure, `git worktree list` didn't list HOOK-3, there was no folder and
no `otaq/HOOK-3` branch. Retry start ran post-checkout again (16:37:04, then 16:37:39) and the ticket started. The
same holds for the stopped start in 6.

### 11 · A hook's changes to the landing commit are said — PASS
The formatter pre-commit landed HOOK-1. History's Merged row reads "into main · Your pre-commit hook changed 2 files"
with **Show changes**, which shows the added header line in `src/calc.js` and `test.js`. The Merge check didn't rerun
for it. HOOK-2 merged with a pre-commit that changes nothing and has no such line ("Merged · into main").
`p11-p15-history-merged-hook-changed-2-files.jpg`, `p11-show-changes.jpg`,
`p15-failing-post-merge-ticket-merged-history-toast.jpg`. (The dialog was closed during HOOK-1's landing, so the
merged toast for it wasn't seen.)

### 12 · Agents push when the user asks — PASS (with a note)
- A plain comment on HOOK-2, "Please push your branch to origin once your commit is in.", was enough. Codex said
  "The ticket comment explicitly asks for a push, so I'm pushing", with no override words.
  `p12-agent-pushes-from-plain-comment.jpg`
- 50 s pre-push: Codex waited ("Running git push … pre-push: running the integration suite (about 50 s)", Working
  1m 35s and on) and reported "Push to origin succeeded; pre-push integration suite passed" only after the push
  finished; the remote has `otaq/HOOK-2` at `b9c3839`. `p12-slow-prepush-agent-waits-1m35.jpg`,
  `p12-slow-prepush-success-reported-after-push.jpg`
- Failing pre-push: Codex committed, saw the refusal, didn't retry or use `--no-verify`, and ran
  `otaq ask human`: "…the push was refused by the pre-push integration hook: `expected 200, got 500`. How should I
  proceed with the failed push?" `p12-failing-prepush-agent-asks-no-bypass.jpg`
- **Note (separate bug):** I asked for this second push by typing into HOOK-2's session while the ticket waited in
  Review. The agent's `otaq ask human` was answered "HOOK-2 asks the user. The user answers on the ticket's Overview",
  but the ticket stayed "Ready for you" and the Overview showed **no question**: the question was lost. Only the
  session text had it. `obs-review-question-not-shown-session.jpg`, `obs-review-question-not-shown-overview.jpg`

### 13 · Otaq checks that a delivery push landed — PASS for the mismatch; rest NOT completed
- The flow's PR skill was set to the project's `release-bot-pr` (it reports without pushing), which makes **Approve &
  open PR** available on the local origin. After the confirm, Otaq reworded the commits and the agent ran
  `otaq pr opened https://git.example.test/hooksproj/pull/99`. The ticket stayed in Review, "Delivery stopped", with
  Needs you on the card and "The branch isn't on the remote as expected · the remote has no branch
  otaq/HOOK-4-add-power-to-calc" on the sheet, plus **Retry push**; the action button became "Push to PR #99". The
  reworded commits stayed on the local branch. `p13-push-not-landed-needs-you-retry-push.jpg`
- **Retry push didn't finish**: the Push dialog stayed on "Push skill is preparing… · reading the commits" because the
  `$otaq:push` Otaq typed into Codex was never submitted (see "Bug: skill command not submitted" below).
  `obs-retry-push-dialog-stuck-preparing.jpg`, `obs-skill-command-typed-not-submitted.jpg`. I cancelled the dialog and
  pressed Enter in the session; the push skill then ran, but by then the flow was cancelled ("HOOK-4 has no Open PR in
  progress"). `obs-push-skill-after-cancel-no-pr-flow.jpg`
- NOT RUN: "a real push shows nothing extra" (it needs a delivery with the `local-pr` skill that really pushes).
- Also seen: before the agent reported, the "Pushing HOOK-4…" dialog (Applied your edits · 2 commits reworded →
  Pushing to origin… → Opening the PR) could not be closed. Esc and the clicks outside did nothing and Cancel was
  greyed, so the sheet was out of reach until the app was restarted. The agent was waiting in its session for a PR URL
  and asked there, not with `otaq ask`. `obs-pushing-modal-blocks-sheet.jpg`

### 14 · A hook that asks in the terminal is named as such — PASS
With the `exec < /dev/tty` pre-commit, the callout reads "Your pre-commit hook refused the merge · Output of your
pre-commit hook · **Your pre-commit hook asks a question in the terminal; Otaq can't answer it. Make the hook skip
its question when there's no terminal, or do this step by hand.**" above
`~/otaq-reltest/hooksproj/.git/hooks/pre-commit: line 3: /dev/tty: No such device or address`. It failed at once.
`p14-tty-precommit-sheet.jpg`, `p14-tty-precommit-sentence-zoom.jpg`

### 15 · The post-merge hook in the user's folder is shown, its output kept — PASS
- 20 s post-merge: the card showed "Merging · running your post-merge hook in ~/otaq-reltest/hooksproj · 6 s" with no
  Stop. History reads "Your post-merge hook ran in ~/otaq-reltest/hooksproj" with both printed lines ("installing
  dependencies (20 s)", "installed 42 packages"). `p15-card-post-merge-in-user-folder-no-stop.jpg`,
  `p15-history-post-merge-output.jpg`. The dialog was closed during this last step, so its view of the user-folder
  post-merge wasn't captured. It did show the same line, without a folder, for the post-merge runs in Otaq's own
  worktrees (5).
- Failing post-merge (HOOK-2): the toast reads "HOOK-2 merged into main · Squashed as 430a82d", the ticket is
  Merged, there is no Needs you, and History has "Your post-merge hook failed in ~/otaq-reltest/hooksproj (exit 1)"
  with its output. `p15-failing-post-merge-ticket-merged-history-toast.jpg`

### 16 · Reworded PR messages go through the commit-msg hook first — FAIL (on 8f0d3893; PASS on re-check, see below)
- Good part: HOOK-4 had a second commit, "update stuff" (made with the hook off). Push & open PR with the
  `feat:`/`fix:` commit-msg rewrote and pushed **nothing**: the branch stayed `otaq/HOOK-4` at `91aefe7` and the
  remote had no new branch. The ticket stayed in Review ("Delivery stopped", Needs you) and the toast read "HOOK-4:
  your commit-msg hook refused the PR". The sheet reads "Your commit-msg hook refused the PR · It refused a commit
  message of the PR · The message it refused: update stuff", followed by the hook's output and "Nothing was rewritten
  or pushed… Edit the message so the hook accepts it, then open the PR again; your edits are kept." It has **Edit
  messages and retry**. The hook log shows commit-msg run per message (2 runs).
  `p16-approve-open-pr-offered-with-own-skill.jpg`, `p16-commitmsg-refused-pr-sheet.jpg`
- **FAIL:** Steps: Approve & open PR on a ticket whose branch has a commit message the commit-msg hook refuses →
  Push & open PR → refusal on the ticket → **Edit messages and retry**. The Open PR dialog that opens shows only
  Branch, Title and Description, with **no commit-message field** (`Dialogs.tsx` renders the commit list only for a
  Push to PR: `view.push && …`). The refused "update stuff" can't be fixed there, and Push & open PR refuses it again
  (16:59:28 in the log, a second toast). `p16-open-pr-dialog-no-commit-fields.jpg`,
  `p16-FAIL-edit-messages-and-retry-has-no-message-field.jpg`, `p16-FAIL-retry-refused-again.jpg`. The plan's Check
  ("fix it to `fix: …` and the PR goes on") can't be done in Otaq. After I amended the commit by hand to
  `fix: describe power in the README (HOOK-4)`, the retry passed commit-msg and showed "Applied your edits · 2 commits
  reworded → Pushing to origin…" (`p16-after-fix-commits-reworded-pushing.jpg`).

## The three extra checks

1. **A refusal of each kind ends on the ticket, dialog open and closed: PASS.** Approve & merge refused by
   pre-commit (dialog open), by commit-msg (dialog closed), by pre-merge-commit (main moved on, dialog open) and by
   the terminal pre-commit. A start refused by post-checkout on a new ticket, and the reopened ticket's update refused
   by pre-merge-commit. Each time the card had Needs you or Start failed naming the hook and step, the sheet had the
   full output and the way on, and History kept it. main, the branch and the worktree were unchanged, and the merges
   stayed in Review.
2. **No hook path shows the Windows user name: PASS.** Cards, sheets, History rows, toasts, the Merging dialog and
   the running-hook lines all used `~/otaq-reltest/…` or no path at all. The full path only appeared in Codex's own
   session output (its commands), which isn't Otaq's text, and it is blacked out in the screenshots
   (`p08-conflict-merging-main-in-goes-to-agent.jpg`, `obs-restack-prompt-stuck-in-input.jpg`).
3. **The hook log shows pre-rebase never runs: PASS** (whole session, 136 lines, see above).

## Other findings

- **Bug: a skill command typed alone isn't submitted (Codex, Windows).** Retry push typed `$otaq:push`, and a
  conflict typed `$otaq:restack`, into the Codex session. Both stayed in the input line ("› $otaq:push ▌") and
  nothing ran until I pressed Enter in the session. With text after the skill, as in `$release-bot-pr The user
  confirmed…`, it submitted fine. The likely cause is Codex's `$` skill popup taking the Enter. In the restack case I
  had also pressed Esc on a pending Codex prompt just before, so that one is less clean. Evidence:
  `obs-skill-command-typed-not-submitted.jpg`, `obs-retry-push-dialog-stuck-preparing.jpg`,
  `obs-restack-prompt-stuck-in-input.jpg`.
- The question asked from a session while its ticket waits in Review isn't shown (12).
- The "Pushing…" dialog can't be dismissed while the delivery hasn't reported (13).

## Manual checks for the maintainer (need gh or az; not faked)

- **Approve & open PR with the built-in open-pr skill** on a GitHub or Azure DevOps origin, with:
  - a 50 s pre-push: the agent waits and reports only after the push, the PR opens, and Otaq's check finds the branch
    at the right commit with nothing extra shown;
  - a failing pre-push: the agent asks (`otaq ask human`) without `--no-verify`, the reworded commits stay local, and
    trying again only pushes;
  - the `feat:`/`fix:` commit-msg with a refused commit message: refusal on the ticket, and then fixing the message in
    Otaq, which currently can't be done (piece 16 FAIL);
  - the push-landed check after a real push: nothing extra shown.
- **Push to PR** after a Send back with new commits, with the same pre-push and commit-msg hooks. The dialog has
  commit-message fields there, so the edit-and-retry path of piece 16 can be tried. Also check that `$otaq:push` is
  submitted in the session (see the bug above).

## Re-check of piece 16 (release-fixes 1af8dafb)

Date: 2026-11-11, 18:03–18:21 VM time. Build: local `release-fixes` at 1af8dafb (the fix: the Open PR dialog always
shows each commit's message field; the refused one is marked; edits are kept on the ticket; Use the proposed message
puts them back), brought in as a git bundle, fetched into `otaq-build\retest-hooks`, built with
`retest-hooks-build.ps1` (shared target), installed with the setup `.exe /S` over the existing install, started
through the `OtaqRetestHooks` task. The `otaq` CLI didn't change between the builds, so the service kept running from
its runtime folder; the app was restarted.

Setup: HOOK-4 wasn't usable (it already has PR #99 recorded, so its button is Push to PR), so I made a fresh ticket,
**HOOK-5** "Add modulo to calc" (`otaq create`), started it, and let Codex's Developer do it (`feat: add modulo
function (HOOK-5)`, first try through the `feat:`/`fix:` commit-msg; it asked one question on the ticket). Validate
was skipped with Move → Review. Then, with commit-msg switched to pass for that one commit and back to `featfix`
(both switches logged), I committed **"update stuff"** in the HOOK-5 worktree (a README line). The flow's PR skill was
set to `local-pr` in Edit flow (Done stage). Active hooks: pre-commit TODO lint, commit-msg feat/fix, pre-rebase
always-fail, pre-push 50 s, the rest pass (all log).

| Step | Result |
|---|---|
| 1 Approve & open PR shows the commit fields; confirm with "update stuff" | PASS |
| 2 Nothing rewritten or pushed; Review, Needs you; the sheet shows the message and the output | PASS |
| 3 Edit messages and retry: the field holds "update stuff", marked | PASS |
| 4 Use the proposed message puts the fields back; an edit is kept over close and reopen | PASS (adapted, see below) |
| 5 Fixed to `fix: …`: reworded, pushed by `local-pr`, PR reported, the push check shows nothing extra | PASS |
| 6 commit-msg ran on the confirmed messages; pre-rebase never ran | PASS |

1. **Approve & open PR** (the dropdown next to Approve & merge) opened "Open a pull request for HOOK-5" with Branch
   `otaq/HOOK-5-add-modulo`, **Commit messages (2 commits)** with one field each (`feat: add modulo function (HOOK-5)`,
   `update stuff`), Title and Description. The delivery agent's proposal didn't reword the commits, so "update stuff"
   was already in its field. `p16-recheck-1-open-pr-dialog-commit-fields.jpg`
2. **Push & open PR** → toast "HOOK-5: your commit-msg hook refused the PR · Its output and the way on are on the
   ticket"; the ticket stayed in Review, "Delivery stopped", counted in Needs you (the bar read "HOOK-5 merge
   stopped"), card "commit-msg hook refused the PR". The sheet: "Your commit-msg hook refused the PR · It refused a commit
   message of the PR · The message it refused: update stuff", the output (`commit-msg: the message must start with
   "feat: " or "fix: " …`, `commit-msg: got: update stuff`), "Nothing was rewritten or pushed… your edits are kept.",
   and **Edit messages and retry**. Git: the branch was still `otaq/HOOK-5` at `63c4f4a update stuff` (not renamed,
   not reworded), the worktree clean, and the remote had only `main` and `otaq/HOOK-2`. History has "PR didn't go
   through · commit-msg hook refused the PR". `p16-recheck-2-refused-sheet-needs-you.jpg`,
   `p16-recheck-5f-history-refusal-row.jpg`
3. **Edit messages and retry** opened the Open PR dialog with both commit fields; the second holds `update stuff`
   with a **red border** (the refused one), the first is plain. `p16-recheck-3-edit-and-retry-refused-field-marked.jpg`
4. Adapted to this ticket, where the proposal itself is "update stuff": I typed `fix: describe mod in the README
   (HOOK-5)` into the field. The red mark went at once and **Use the proposed message** appeared
   (`p16-recheck-4a-edited-field-unmarked-use-proposed-link.jpg`). Clicking it put `update stuff` back, marked red again,
   and the link went (`p16-recheck-4b-use-proposed-restores-proposal.jpg`). Then I typed another bad message, `more
   stuff` (not the refused one, so not marked), closed the dialog with Cancel and reopened it from Edit messages and
   retry: the field still held `more stuff`, with Use the proposed message shown (`p16-recheck-4c-bad-edit-typed.jpg`,
   `p16-recheck-4d-edit-kept-after-close-reopen.jpg`). Typing "update stuff" itself again wasn't a separate check here,
   since it equals the proposal and isn't an edit.
5. I set the field to `fix: describe mod in the README (HOOK-5)` and clicked Push & open PR
   (`p16-recheck-5a-fixed-message-before-confirm.jpg`). The dialog went to "Pushing HOOK-5… · Applied your edits · 2
   commits reworded → Pushing to origin…" (`p16-recheck-5b-reworded-pushing.jpg`). In the worktree the branch became
   `otaq/HOOK-5-add-modulo`: `cfbe503 fix: describe mod in the README (HOOK-5)` on `1d91796 feat: add modulo function
   (HOOK-5)`. Otaq typed `$local-pr The user confirmed the pull request…` into the session and it **was submitted**. Codex
   ran `git push -u origin otaq/HOOK-5-add-modulo` (after my `y`), waited for the 50 s pre-push, then `git ls-remote`,
   `otaq pr opened https://git.example.test/hooksproj/pull/2` ("HOOK-5 has PR #2"). The remote has
   `otaq/HOOK-5-add-modulo` at `cfbe503`, the same two commits. The ticket shows "PR open", the PR card
   "hooksproj/pull/2 · otaq/HOOK-5-add-modulo → main" with Mark merged / Mark closed, and **no** "isn't on the remote as
   expected" callout or Needs you: the push check (piece 13) shows nothing extra after a real push. History: "PR #2
   opened", "PR confirmed", "Branch renamed otaq/HOOK-5 → otaq/HOOK-5-add-modulo".
   `p16-recheck-5c-local-pr-pushed-pr-reported.jpg`, `p16-recheck-5d-pr-open-nothing-extra.jpg`,
   `p16-recheck-5e-history-pr-opened.jpg`
6. Hook log (user path as `~`):

   ```
   18:13:01 switch commit-msg <- pass; pre-commit, prepare-commit-msg, commit-msg, post-commit   <- my "update stuff"
   18:13:01 switch commit-msg <- featfix
   18:16:25 commit-msg ×2  HOOK-5    <- confirm with "update stuff": 1st passes, 2nd refused
   18:17:58 commit-msg ×2  HOOK-5    <- confirm with the fixed message: both pass, then the reword
   18:19:25 pre-push pwd=~/otaq-reltest/home/wt/hooksproj/HOOK-5 args=origin …hooksproj-remote.git
   ```

   The only line with "pre-rebase" in the whole log (153 lines) is still my header from the first run.

Seen along the way (not part of piece 16's fix):

- **Still there: the "Pushing HOOK-5…" dialog can't be closed** while the delivery agent hasn't reported (Esc does
  nothing, Cancel is greyed). Here it hid the session where Codex waited for permission to run `git push`, so the flow
  only went on after I restarted the app (`otaq-app` only; the service and session kept running) and answered `y` in
  the session. `p16-recheck-obs-pushing-modal-still-blocks.jpg`
- While Codex waited on that permission prompt in Review, the card said "Ready for you", not Needs you (like the
  lost question in 12).
- Not met: the skill command typed alone. The only command typed here, `$local-pr …`, had text after it and was
  submitted.
- Minor: closing the Open PR dialog with Cancel leaves a "PR cancelled · by you" row in History, though the user only
  closed the dialog to come back to it.

## State left in the VM

- The installed app is **1af8dafb** (re-check of piece 16), running on screen on the release-test data
  (`otaq-reltest\home`), started through the scheduled task `OtaqRetestHooks` (it runs `otaq-reltest\launch.cmd`). The
  service (unchanged CLI, runtime folder from 8f0d3893) and the Codex sessions are live.
- Project **HOOK** stays in the app. HOOK-1 and HOOK-2 are merged into the test repo's main. HOOK-3 is in Validate,
  "Stopped by a restart" (the re-check's restart; I chose Not now on the resume dialog). HOOK-4 is in Review,
  "Delivery stopped · the branch isn't on the remote as expected", PR #99 recorded, the flow cancelled. **HOOK-5** (the
  re-check's ticket) is in Review with "PR open", PR #2 recorded, branch `otaq/HOOK-5-add-modulo` at `cfbe503` locally
  and on the bare remote, its Codex session idle. The flow's PR skill is now set to **`local-pr`**.
- `%USERPROFILE%\otaq-reltest\hooksproj` and `hooksproj-remote.git` remain, with my test commits on main ("fix: point
  the README…", "fix: mention the tests…", "feat: add square") and `.agents/skills/{local-pr,release-bot-pr}`. Active
  hooks: pre-commit TODO lint, commit-msg feat/fix, pre-rebase always-fail, pre-push 50 s; the rest pass (all log).
  Nothing else under `otaq-reltest` was touched. SHOP and NOTE are unchanged, apart from the app's own restart.
- Mine under `%USERPROFILE%\otaq-build\`: `retest-hooks\` (the clone), `retest-hooks.bundle`,
  `retest-hooks-build.ps1`, `retest-hooks-build.log`, and `retest-hooks-lib\` (hook variants, `hk.ps1`, `state.ps1`,
  `setup.ps1`, `hooks-log.txt`), and from the re-check `retest-hooks-1af8dafb.bundle` (the clone is checked out
  detached at 1af8dafb). The shared `target\` has the 1af8dafb build.
- The five leftover desktop files (`piece6.bundle`, `piece6b.bundle`, `p6-test.ps1`, `p6-run.ps1`, `p6-run2.ps1`) were
  deleted. No VM snapshot was taken. The VM clipboard holds my last test text (`fix: describe mod in the README
  (HOOK-5)`).
- After the re-check there were no sh.exe, sleep.exe or git.exe processes. C: had 810 GB free. The Mac had 161 GB free
  before and after.
