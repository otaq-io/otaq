# Re-test 8 · part e · checks left after round 6 (Windows) · results (Windows 11 ARM64 VM, build 4147a741)

Date: 2026-11-12, 16:52–17:50 VM time. Build **4147a741** (installed; code = release-fixes 826d4f7f). Data: the normal
release-test data (`otaq-reltest\home`, `launch.cmd` via `otaqtest`), project **hooksproj** (HOOK) for Claude Code and
the push checks, **shop** for Codex. Scope: `REGRESSION-CHECKS-LEFT.md` sections "Claude Code (Windows)", "NEW-12 and
NEW-10", "Codex (Windows)"; "Other OSes and providers" is out of scope. Screenshots `shots-retest-8/e-*.jpg`; none shows
the Windows user name, a login, email or token (shots whose terminal printed a full path were cropped above it or not
saved). No code changed, nothing committed in the repo on the Mac, nothing pushed anywhere but hooksproj's local bare
remote, nothing signed in or out. Claude Code: Architect agent (auto mode by default; switched to manual with
Shift+Tab where a prompt was needed). Codex: 20 % → **16 %** of the monthly allowance left (prompts answered "y", one
writer prompt declined with Esc to save quota; never "p"). Typing by key presses; the clipboard was used twice, each
time Set-Clipboard + Get-Clipboard read back right before Ctrl+V, and checked on screen (nothing foreign arrived).

## Summary
**PASS 13 · PARTLY 1 · KNOWN 6 · NOT TESTED 7 · new problems 2 (none a blocker) · notes 5.** Every round 6 path that
could be reached passes on Windows on both tools: Push to PR, Retry push and push-skill prompts reading Needs you in
Review (Claude Code and Codex), a prompt open at Approve & merge into a conflict (Claude Code), a failed merge check
sending the ticket to Dev with the Codex writer's prompt open, answer-session notification and dirty-worktree Cleanup.
NEW-12 and NEW-10 are still there (pieces 7/8 unmerged), and NEW-12 is why the writer itself never prompts on Claude
Code (a typed stand-in prompt was used).

| Check (REGRESSION-CHECKS-LEFT) | Result |
|---|---|
| **Claude Code** · Open PR with `local-pr` (now in `.claude/skills/`): skill prompts read Needs you, ticket stays in Review | **PASS** (HOOK-10 → PR #3) |
| Claude Code · Push to PR: `/otaq:push` submitted, its prompts read Needs you in Review, proposal shown, pushed | **PASS** |
| Claude Code · Retry push after a failed pre-push: "The skill stopped … Retry" → Retry starts the turn → pushed | **PASS** (HOOK-10; again on HOOK-5, whose delivery ran on Claude Code while the flow named Architect) |
| Claude Code · a prompt open at Approve & merge into a conflict → Dev Needs you, restack after the answer, merged | **PARTLY**: everything after the conflict **PASS** (Dev, Needs you, card, strip, "1 message waiting", `/otaq:restack` typed alone after the turn, resolved, merged d1558cc); but the prompt was one I had Claude open (`node -e 1`), because the **writer never runs a command on Claude Code** (it gets only the tail of its request: NEW-12) |
| Claude Code · the written message reaches the Approve and Open PR dialogs | **KNOWN (R7 NEW-12)**: "Architect is still writing…" in both; the writer printed JSON from the tail of its request (2 of 2) |
| Claude Code · resume from a "signed out" hold; login expiring mid-session → Sign in… | **NOT TESTED** (would need signing out; not allowed) |
| **NEW-12** · a long message typed into Claude Code arrives whole | **KNOWN (not fixed)**: the 1,296-char writer request arrived as its last ~270 chars, 2 of 2 (HOOK-10, HOOK-11); Claude showed "paste again to expand" under its input |
| **NEW-10** · Ctrl+V pastes text into Claude Code | **KNOWN (not fixed)**: nothing pasted (HOOK-10); it also left Otaq believing there was unsent text (NEW-e1) |
| NEW-10 · Ctrl+V in Codex; an image still pastes in Claude Code | **NOT TESTED** (no idle live Codex input line; no image on the clipboard: kept brief as asked) |
| NEW-10 · PowerShell (Terminal drawer) pastes once | **PASS** (one copy of the text) |
| **Codex** · Retry push in the app after a failed pre-push | **PASS** (HOOK-10, delivery agent Developer/Codex: "The skill stopped: HOOK-10 wasn't pushed" → flag removed → Retry → `git push` prompt → pushed, PR #3 updated, remote c9c5a4f) |
| Codex · Push skill prompts read Needs you in Review; the marked card's full wording | **PASS**: card stays in Review, Needs you edge, "…command: rg --files …", "…command: git push origin otaq/HOOK-10"; dialog names the command with Answer in terminal |
| Codex · an automatic PR whose skill hits a prompt | **NOT TESTED** (Codex quota: needs a flow switch to automatic and a full extra delivery) |
| Codex · a failed merge check (not a conflict) sends the ticket to Dev with the writer's prompt open (decision 010) | **PASS** (SHOP-46: "Merge check failed on main", In Dev, Needs you, card "Asks you · run command: git rev-parse --git-path hooks", restack typed after the writer's turn, fixed, merged c01c099) |
| Codex · a conflict hand-back while the agent limit is full (Queued): the writer prompt stays calm | **KNOWN (R7 NEW-4 rest)**, confirmed still only that: "Waits for a free agent", Needs you 0 with the prompt open; once a slot freed the pill read "Working" until the next prompt, which read Needs you |
| Codex · answer sessions: OS notification for the answering ticket's prompt | **PASS** with the window unfocused (toast "SHOP-46 needs permission · Run command: @' · shop · R8e merge check writer"); with the window focused nothing is posted (NEW-e2) |
| Codex · a dirty answer worktree kept for Cleanup | **PASS** ("SHOP-46 is Merged with uncommitted changes · 1 uncommitted file: r8e-answer-note.txt"; deleted from Cleanup: "Cleaned up 1 item") |
| Codex · a ticket reopened while its answer session runs | **NOT TESTED**: the answering ticket was merged and merged tickets are locked (Move: "Merged tickets are locked"; no Reopen in ⋯) |
| Codex · Ctrl-clicking a link while a selection shows (piece 5) | **NOT TESTED**: no link appeared in a live Codex session (it needs mouse tracking, so a live one) |
| Codex · answer-session prompt opened from the strip; second question answers its own id | **PASS** (extra: the strip chip opened SHOP-46's session at the prompt; the second question was answered with its new id, R7 NEW-2 not seen) |
| Other OSes and providers (macOS, Linux, real gh/az) | **NOT TESTED** (out of scope for this part: no macOS/Linux VM run, no real gh/az) |

## Checks (running log)
- 16:52 hooksproj: committed `.claude/skills/local-pr/SKILL.md` (a copy of `.agents/skills/local-pr`) on main as
  f4da09c "fix: test setup local-pr skill for Claude Code (retest 8e)" (NEW-13 workaround).
- 16:53 hooksproj flow: proposal 46 (`otaq flow propose`, adds agent Architect, delivery agent Developer → Architect),
  saved in Edit flow ("Flow saved · Applied now…"). Before: `otaq-reltest\r8e-hook-flow-before.txt`.
- HOOK-10 "R8e Claude Code PR" (`otaq create --stage Dev=Architect`): append one README line.
- 16:54 HOOK-10 started (Architect, auto mode): commit 647456c, `otaq stage next`; the Codex checker started, so
  Move → Review (skip Validate). Session switched to **manual mode** (Shift+Tab: "manual mode on").
- NEW-12 seen at once: the writer's request reached Claude as its tail only (`), "description": what changed and why…`),
  Claude printed JSON; Open PR dialog "Architect is still writing the title and description".
  `e-new12-writer-request-tail-only-hook10.jpg`, `e-new12-writer-json-printed-paste-again-hint.jpg`,
  `e-open-pr-dialog-still-writing-new12.jpg`
- 16:55 Approve & open PR → Push & open PR: `/local-pr` turn on Claude Code; each command a prompt: pill **Needs you**,
  strip "HOOK-10 allow a c…", card in **Review** "needs your permission", dialog "The agent is waiting for your
  permission · Claude needs your permission · Answer in terminal". "1" each time → push (pre-push 50 s, "integration
  ok") → `otaq pr opened …/pull/3` → **PR #3 open**. `e-cc-pr-skill-prompt-needs-you-in-review.jpg`,
  `e-cc-open-pr-done-pr3-open.jpg`
- 16:58 Setup: pre-push got a fail line while `retest-hooks-lib\prepush-fail.flag` exists (original saved as
  `otaq-reltest\r8e-pre-push.orig`), flag created; commit 453de95 "fix: note re-test 8e push check (HOOK-10)" made in
  HOOK-10's worktree by CLI. Send back with PR comments ("No code change needed… run otaq stage next") → Claude
  (`/otaq:address-review`), one prompt, `otaq stage next`; Codex checker started → Move → Review. Footer **Push to PR #3**.
- 16:59 Push to PR #3 → "Push skill is preparing…" with `/otaq:push` submitted on Claude Code; its prompts read Needs you
  in Review (`e-cc-push-to-pr-skill-prompt-needs-you.jpg`); proposal shown ("Proposed by push · edit anything", branch
  locked, 1 new commit); Push to PR #3 → pre-push refused (flag) → Claude asked `otaq ask human` → dialog **"The skill
  stopped: HOOK-10 wasn't pushed · … Retry"** (`e-cc-retry-push-skill-stopped-top.jpg`,
  `e-cc-retry-push-skill-stopped-retry-button.jpg`). Flag removed → **Retry** → turn started at once (prompt) → push
  (50 s pre-push) → toast **"Pushed to PR #3 · HOOK-10 → main"**, PR open. `e-cc-retry-push-done-pr-open.jpg`
- 17:03 HOOK-11 "R8e Claude Code writer conflict" (Architect, manual mode from the start): commit 0cfdfc9, stage next,
  Codex checker → Move → Review. The writer again got only the tail of its request (NEW-12) and printed JSON **without
  running any command**, in manual mode too: no writer prompt can be had on Claude Code in this build.
- Stand-in: I typed (key presses) "…Now run exactly this command and nothing else: node -e 1" into HOOK-11's Claude
  session in Review → a permission prompt open in the delivery agent's session; card calm "Ready for you".
  Setup commit on hooksproj main f9d8f7b "fix: test setup README conflict (retest 8e)".
- 17:05 Approve & merge: dialog "Architect wants to run a command to write the message. Merge now with this one…
  Answer in terminal" (`e-cc-approve-dialog-prompt-open.jpg`) → Merge → "Merging main in stopped — conflict ·
  README.md"; at once **In Dev, pill Needs you**, strip "HOOK-11 allow a command", "1 message waiting · Deliver now"
  (`e-cc-conflict-dev-needs-you-message-waiting.jpg`); card in Dev "Asks you · claude needs your permission"
  (`e-cc-conflict-card-dev-asks-you-strip.jpg`); Overview "Conflicts with main · 1 file … returns to Review when it's
  fixed" + "Architect wants your permission · Answer in terminal". "1" → the node turn ended → **`/otaq:restack` typed
  alone and submitted** (`e-cc-restack-typed-alone-after-turn.jpg`); its `git merge main` prompt read Needs you; I chose
  "switch to auto mode" for the rest; Claude merged (its own python-less resolve committed markers first, then fixed
  them in 2337bad: agent behaviour, not Otaq), stage next → Codex checker → Move → Review → Approve & merge (first line
  "fix: …" for the commit-msg hook) → **Merged d1558cc**. `e-cc-conflict-merged-d1558cc.jpg`
- 17:08 NEW-10 brief: HOOK-10's live Claude session, clipboard "r8e paste check" (set + read back), Ctrl+V → nothing
  arrived (KNOWN NEW-10). `e-new10-cc-ctrl-v-pastes-nothing.jpg`
- 17:10 Codex: SHOP-46 "R8e merge check writer" (Codex Developer): README line 39dccab; setup commit on shop main
  2addbce "Test setup: merge check needs docs/r8e.md (re-test 8e)" (test.js fails without docs/r8e.md); Codex checker
  → Move → Review. Writer (Codex) at a prompt, Review calm (`e-cx-writer-prompt-review-calm.jpg`).
- 17:12 Approve & merge over the open writer prompt → **"Merge check failed on main · docs/r8e.md is missing … SHOP-46
  went back to its agent"**; at once **In Dev, pill Needs you**, strip "SHOP-46 allow a command", "1 message waiting"
  (`e-cx-merge-check-failed-dev-needs-you.jpg`); card in Dev **"Asks you · run command: git rev-parse --git-path
  hooks"** (`e-cx-merge-check-card-names-writer-command.jpg`). "y" × 6 → the writer's turn ended (`otaq pr message`) →
  **`$otaq:restack SHOP-46's merge check failed …` typed and submitted**, pill "Merge check failed · fixing"
  (`e-cx-merge-check-restack-typed-after-writer-turn.jpg`). Codex added docs/r8e.md (a0dbb53), stage next.
- 17:18 Queued case: Codex checker → Move → Review (writer at a prompt again). Setup commit on shop main ffbb338
  "Test setup: README conflict (re-test 8e)". Limits on (Run at most 1; the Needs-you cap 3 first held HOOK-12 back,
  "needs you 5 (no new starts)", so raised to 8); HOOK-12 "R8e limit holder" (Claude Code, `sleep 150` first) took the
  slot ("1/1 agents"). Approve & merge → conflict → pill **"Waits for a free agent"**, Overview "Waiting for a slot.",
  card "Waits for a free agent · next in line", **Needs you 0 while the writer's prompt is open** (KNOWN R7 NEW-4 rest).
  `e-cx-queued-conflict-waits-for-slot.jpg`, `e-cx-queued-writer-prompt-calm-card.jpg`. Limits put back (off, 5, 3):
  SHOP-46 got its slot and read **"Working"** with the prompt still open (header "Waiting for you", "1 message
  waiting"; `e-cx-after-queue-working-pill-writer-prompt-open.jpg`) until the next prompt, which read Needs you. I
  declined the writer's next prompt (Esc) to save Codex quota → `$otaq:restack` typed alone after the turn.
- 17:28 HOOK-12 (the limit holder) reached Validate on the Codex checker → Move → Review → Done, don't merge.
- 17:29 Answer session: SHOP-47 "R8e asker" (`--deps SHOP-46`), `OTAQ_TICKET=SHOP-47 otaq ask SHOP-46 --stdin` (asked to
  leave an uncommitted file). Strip "Needs you · SHOP-46 allow a command"; the chip opened SHOP-46's Session at the
  answer prompt (`e-cx-answer-session-prompt-needs-you-opened-from-strip.jpg`). Codex didn't create the file, so I
  wrote `r8e-answer-note.txt` into the answer worktree myself before answering "y". After the session ended the
  worktree was kept and Cleanup listed it as above (`e-cx-answer-worktree-dirty-kept-for-cleanup.jpg`); deleted there.
  No OS notification came for this prompt: the Otaq window had focus (the service log shows none from 16:46 to 17:35;
  d0090 holds every notification while any Otaq window is focused, see NEW-e2).
- 17:34 Second question (same asker) with the window unfocused (clicked the taskbar first): log "notification
  title=SHOP-46 needs permission"; Windows' notification centre shows it (`e-cx-answer-session-os-notification.jpg`).
  Answered with its own new question id. The clean answer worktree was removed from git (`git worktree list`), but an
  empty `home\wt\shop\SHOP-46` folder stayed (NOTE-e4).
- 17:36 HOOK-5 Push to PR #2 with the fail flag: its delivery ran on **Claude Code** (the flow's delivery agent was still
  Architect from my setup; NEW-11's family) with no prompt (auto mode): "The skill stopped: HOOK-5 wasn't pushed" →
  flag off → Retry → pushed, PR #2 updated (remote 0e9976c). `e-cc-hook5-retry-push-skill-stopped.jpg`.
- 17:39 hooksproj flow put back (proposal 47 = the saved "before" YAML, Save flow; `otaq flow show -P HOOK` line for line
  the same as `r8e-hook-flow-before.txt`).
- 17:40 Codex Retry push: commit c9c5a4f in HOOK-10's worktree (CLI), fail flag on. Send back with PR comments → first
  refused with "HOOK-10 has unsent text in its terminal" over an empty input line (NEW-e1) → Clear it and send →
  Claude (`/otaq:address-review`) stage next → Codex checker → Move → Review. Push to PR #3 → Codex delivery session:
  each prompt read Needs you in Review, card naming the command (`e-cx-push-skill-prompt-needs-you-in-review-card-names-command.jpg`,
  `e-cx-pushing-prompt-git-push-card-in-review.jpg`), proposal, Push → pre-push refused → Codex `otaq ask human` →
  "The skill stopped … Retry" (`e-cx-codex-skill-stopped-retry.jpg`) → flag off → Retry → `git push` prompt → pushed
  (pre-push 50 s), PR #3 open, remote otaq/HOOK-10 = c9c5a4f.
- 17:48 Terminal drawer PowerShell: clipboard "r8e-paste-check" (set + read back) → Ctrl+V → pasted once; cleared with Esc.

## New problems
- **NEW-e1 · After a Ctrl+V that pasted nothing into Claude Code, Send back says the terminal has unsent text.** HOOK-10's
  Claude input line was empty (Ctrl+V pasted nothing, NEW-10), yet Send back with PR comments stopped with "HOOK-10 has
  unsent text in its terminal · Clear it and send / Cancel". Likely the raw Ctrl+V keystroke counts as typed input.
  The first try looked like nothing happened, because a click elsewhere dismissed that dialog and the comment was lost
  with it. Expected: an empty line isn't unsent text. **Blocker: no** (Clear it and send works; it goes with NEW-10's fix).
  `e-newe1-unsent-text-dialog-after-ctrl-v.jpg`, `e-newe1-claude-input-line-empty.jpg`
- **NEW-e2 · While any Otaq window has focus, no OS notification is posted for any ticket, though Settings says only "a
  ticket open in front of you doesn't notify".** Settings → Notifications: "Not while you're looking at that ticket ·
  A ticket open in front of you doesn't notify · Always". Seen: from 16:46 to 17:35 the service posted nothing (HOOK-10's
  push prompts, HOOK-11's conflict prompt, SHOP-46's merge-check and answer prompts, all with the board or another
  ticket in front), and "nothing later when the user switches away" (code: `needs_you.rs` `looking` = any app window
  focused). With the window unfocused the answer prompt notified at once. Either the sentence or the rule should
  change (gap-analysis #47 already notes the toggle). **Blocker: no** (the strip, card and rail still show it).
  `e-cx-answer-session-os-notification.jpg`

## Notes
- NOTE-e1 · The "Send … back with PR comments" dialog says "go to Dev (Developer)" while HOOK-10's Dev agent is Architect
  (the ticket did go to Architect).
- NOTE-e2 · In Review after the merge-check fix, SHOP-46's footer and Changes tab read "3 files · +3/+4" (main's test.js
  counted), while the squash on main had 2 files; after the merge Changes read 2 files.
- NOTE-e3 · Claude Code (HOOK-10) reported "Otaq listed the branch as already pushed, but the push created it as a new
  branch" during Open PR (prime's branch status before the first push).
- NOTE-e4 · The clean answer worktree's folder `home\wt\shop\SHOP-46` was left empty on disk after git removed it.
- NOTE-e5 · `otaq list` shows HOOK-12 (Done, don't merge) as "Merged" (NEW-b5's family). A Codex restack turn sat at
  "Working" ~2 min showing nothing before its first prompt (as R7's note).

## Not tested
- Claude Code: resume from a "signed out" hold, a login expiring mid-session (no signing out allowed); image paste.
- Codex: an automatic PR whose skill hits a prompt (quota); Ctrl-click a link while a selection shows (no link in a live
  Codex session); a ticket reopened while its answer session runs (merged tickets are locked); Ctrl+V into Codex.
- Other OSes and providers (macOS, Linux core paths with `$otaq:push`/`$otaq:restack` and Claude Code long messages;
  real gh/az PR paths): out of scope for this part.

## Setup changes and state left
| Change | State now |
|---|---|
| hooksproj main: f4da09c (`.claude/skills/local-pr` copy), f9d8f7b (README conflict setup), d1558cc (HOOK-11 merged), 69ae147 (copy removed) | **skill copy removed**; main 69ae147, clean; `.agents/skills` unchanged |
| hooksproj flow: Architect added, delivery agent → Architect (proposal 46) | **put back** (proposal 47); `otaq flow show -P HOOK` line for line = `otaq-reltest\r8e-hook-flow-before.txt` |
| hooksproj `.git/hooks/pre-push`: fail line while `prepush-fail.flag` exists | **restored** from `otaq-reltest\r8e-pre-push.orig` (hash equal); flag removed |
| HOOK-10 "R8e Claude Code PR" (Architect) | Review · **PR #3 open** (remote otaq/HOOK-10 c9c5a4f); its Claude session live and idle (auto mode); its Codex delivery session ended |
| HOOK-5 | Review · PR #2 open, remote now 0e9976c (the commit left from re-test 7b, pushed) |
| HOOK-11, HOOK-12 | HOOK-11 merged (d1558cc); HOOK-12 Done, don't merge (branch otaq/HOOK-12 kept) |
| shop main: 2addbce (test.js needs docs/r8e.md), ffbb338 (README conflict setup), c01c099 (SHOP-46 merged, adds docs/r8e.md) | not reverted; main c01c099, clean, `node test.js` ok |
| SHOP-46 "R8e merge check writer" (Codex) | Merged; two answer sessions done; its dirty answer worktree deleted via Cleanup (an empty folder remains) |
| SHOP-47 "R8e asker" (depends on SHOP-46) | Backlog, Not started (asked two questions from the CLI; both answered) |
| Settings → Limits & usage (on, 1, 8 for the Queued check) | **put back**: off, 5, 3 |
| Files `otaq-reltest\r8e-*.txt/.yaml`, `r8e-pre-push.orig`, `r8e-ask*.txt` | left (no secrets) |
| Test app | **running on the normal data** (app PID 17992, service 19780, build 4147a741), hooksproj board, Terminal drawer closed |
| The qbox Otaq (`~\otaq-qbox`, PIDs 8380 / 18580) | not touched |
| Clipboard | shared with the Mac (now the Mac's text) |

Part e is finished, so nothing is left in progress. A reinstall would end only HOOK-10's idle Claude Code session; no
agent is working and no prompt is open. NOTE-e2 (3 files counted after main was merged in) may be the base-sha fix
in fae1fd78's scope; worth a look on that build.
