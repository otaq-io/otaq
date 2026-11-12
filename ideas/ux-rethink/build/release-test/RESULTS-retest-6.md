# Re-test 6 · release round 5 on Windows · results (Windows 11 ARM64 VM, build release-fixes d0a165b5)

Date: 2026-11-11, 19:34–20:36 VM time. Plan `tuq/plans/release-round5.md` (ten pieces, decisions 040–043), issues in
`issues-round5/`. Screenshots: `shots-retest-6/` next to this file (JPEG). The Windows user name is blacked out where a
Codex prompt or a shell printed a full path (`c8-shell-*`, `c10-try1-*`, `c2-noyan-*`,
`new-review-push-skill-prompt-*`), and the GitHub login on Settings → Agents. Paths shortened to `~\otaq-reltest\…` by
the tools are left. No sign-in URL, email or token was on screen. No Otaq code was changed, nothing committed or pushed
to anything real (the only remote is the local bare `hooksproj-remote.git`).

## Summary
Checks: **PASS 9 · PARTLY 5 · FAIL 0 · not tried 1** of 15 (pieces 1–10, the four issue-10 gaps, the regression
walk). The `$otaq:push` bug of the hooks re-test **still shows**.

| Check | Result |
|---|---|
| 1 Restack waits behind pasted text, then goes alone | **PASS** (Codex). The Claude Code half of "the other side" wasn't tried (no Claude Code session ran) |
| 2 Every queued Codex prompt reads Needs you, one after the other | **PARTLY**: checker 3 of 3 and a ticket's agent 2 of 2 PASS; a delivery/push session in Review never reads Needs you (NEW-2); Noyan only single prompts; helpers not tried |
| 3 Prompt while "Merge check failed · fixing" reads Needs you, "fixing" after "y" | **PASS** |
| 4 After a send back, no "1 message waiting" that never goes | **PASS** |
| 5 "Unsent text" footer only over text; a note goes into an emptied line | **PASS** |
| 6 Terminal focused + prompt: card like the pill, no alert | **PASS** (the card's quiet line couldn't be read: the sheet covers the card) |
| 7 Login expired offers Sign in… at once | **not tried**: no login expired, and nothing was signed out |
| 8 Ctrl+C copies a selection on Windows | **PARTLY**: shell PASS (copy, and ^C without a selection); in Codex a plain drag is Codex's own copy (draft kept), but a Shift+drag selection is not copied by Ctrl+C or Ctrl+Shift+C, and Ctrl+C wipes the draft (NEW-1) |
| 9 Every view names the stage a held ticket starts at | **PASS** (a start-failed ticket; drag says "can't go to Dev yet" over its own column, NOTE-3) |
| 10 Newer "Ask the agent to write it" replaces an older one | **PARTLY**: B always goes through, but A keeps "Being written…": each ask starts its own Noyan conversation, so 043's same-agent rule never applies (NEW-3) |
| Gap · held ticket's column | **PASS** (start-failed NOTE-18 sits in Dev) |
| Gap · "Removing waits until no ticket is in it" with a ticket in the stage | **PASS** on Review with SHOP-36 in it; not shown on Dev holding the start-failed NOTE-18 (NOTE-4) |
| Gap · round-4 piece 23 in answer sessions | **PARTLY / blocked**: an answer session started, but it can't be seen or reached in the app, so no key could be pressed in it (NEW-4) |
| Gap · Approve & open PR / Push to PR with pre-push and commit-msg | **PARTLY**: on the local bare remote with `local-pr`, Push to PR stops at the unsubmitted `$otaq:push` (NEW-5). The real gh/az paths couldn't be tried: the VM has neither |
| Regression · Review → Approve & merge with hooks; Edit flow; Settings | **PASS** |

New problems (details at the end): NEW-1 Shift-selection Ctrl+C in Codex wipes the draft and copies nothing; NEW-2 a
Codex prompt in a session while the ticket is in Review never reads Needs you; NEW-3 each "Ask the agent to write it"
is its own Noyan conversation, so an older request keeps "Being written…"; NEW-4 an answer session is invisible and
unreachable; NEW-5 (old bug) `$otaq:push` typed alone isn't submitted; NEW-6 Send back with an open Codex prompt says
"unsent text in its terminal". Notes NOTE-1…NOTE-5.

## Setup
- Build: branch release-fixes at **d0a165b5** ("Merge piece 6: the card says Needs you while you're in the agent's
  terminal"). An incremental bundle `1af8dafb..release-fixes` (128 KB) was fetched into
  `%USERPROFILE%\otaq-build\retest-hooks` (fast-forward 1af8dafb → d0a165b5, 63 files) and built with the existing
  `retest-hooks-build.ps1` (shared `CARGO_TARGET_DIR`, `npm.cmd ci` + `npm.cmd run bundle -- --bundles nsis --config
  noupd.json`): BUILD-EXIT=0, `Otaq_0.1.0_arm64-setup.exe` 20.3 MB, kept as
  `otaq-build\Otaq_0.1.0_arm64-setup-d0a165b5.exe`, log `otaq-build\retest6-build.log`.
- Install: the running 8f0d3893-service/1af8dafb app were stopped (taskkill), setup `/S` exit 0, started with
  `launch.cmd` through the `otaqtest` task on the release-test data (`otaq-reltest\home`). Firewall prompt for the new
  runtime folder: Cancel. About: "Version 0.1.0 · build d0a165b5 · Built from source". `setup-about-d0a165b5.jpg`
- Data: shop (Dev → Validate → Review, Codex Developer/Checker, merge check `node test.js`), notes (Spec first; Claude
  Code spec agents, Codex Dev/Validate), hooksproj **HOOK** (the hooks re-test's project, hooks in `.git/hooks`
  logging to `retest-hooks-lib\hooks-log.txt`: pre-commit TODO lint, commit-msg feat/fix, pre-rebase always-fail,
  pre-push 50 s, the rest pass; flow PR skill `local-pr`, origin = local bare repo). Noyan on Codex.
- New tickets: SHOP-35 "Retest6 restack", SHOP-36 "Retest6 checker", NOTE-18 "Retest6 held at Dev" (spec stages
  skipped), NOTE-19 "Retest6 waits for held" (depends on NOTE-18), all with `otaq create`.
- Typing went through `Set-Clipboard` + Ctrl+V, each paste checked. Codex prompts answered "y" (Esc four times: two
  prompts I set up a request over, one push-skill `otaq ask human`, one Noyan write), never "p".
- Settings → Agents at the start: Claude Code v2.1.322, Codex v0.166.0, GitHub Copilot v1.0.109, OpenCode all
  Connected. `reg-settings-agents-connected.jpg`

## Checks

### PASS · 1 · Otaq types into an agent's line only when the screen shows it empty
- SHOP-35 in Review, its Dev Codex session idle (delivery message written). A setup commit on main (9a50f86
  `setup-move-r6.txt`) and `failcheck.flag` + merge check `..\..\..\mc.cmd` made the merge check fail.
- In the Agent terminal I pressed **Ctrl+V** with "unsent draft line left by the tester retest6" on the clipboard (it
  reaches Codex as a paste) and **one Backspace**: the line read "› unsent draft line left by the tester retest".
  `c1-pasted-unsent-one-backspace.jpg`
- Approve & merge → "Merge check failed on main" (MERGE CHECK FAILED: the test flag file…), SHOP-35 back to Dev,
  "Merge check failed · fixing", header "1 message waiting · Deliver now". `c1-merge-check-failed-restack-waits.jpg`
- 15 s later: the line still "…tester retest", nothing typed. `c1-restack-waits-15s-text-intact.jpg`. **A second
  Backspace** (line "…tester retes"): still waiting, header still "1 message waiting". `c1-second-backspace-still-waits.jpg`
  (re-test 5 released it after the first Backspace).
- 50 Backspaces emptied the line: within 1 s Otaq typed **`$otaq:restack SHOP-35's merge check failed on the result
  with 'main' merged in; the comment has its output. Run 'otaq prime'.` alone**, it was submitted and Codex worked on it
  ("Working · Running hook", then "I'll load the full restack context…"). `c1-line-emptied-restack-typed-alone.jpg`,
  `c1-restack-submitted-agent-works.jpg`
- The other side, on empty Codex lines, Otaq's messages went in without waiting: the delivery request ("SHOP-35 is
  waiting in Review. You are its delivery's agent…"), "Review sent SHOP-35 back to you with a comment", a comment for
  the checker ("The user added comment 3…", queued while the checker worked and typed at its turn end), "The user added
  comment 10…" (see 5), and the address-review skill on HOOK-5. **Claude Code: not tried** (every agent that ran this
  session was Codex; notes' Claude Code stages were skipped). The stage reminder didn't come up.
- Cleanup: flag removed, merge check back to `node test.js`; SHOP-35's agent handed back, a checker passed, SHOP-35
  merged (5885c52).

### PARTLY · 2 · Every open prompt of a session reads Needs you, one at a time
- **Checker** (SHOP-35 Validate after the send back; I asked the Checker via "Tell Checker now" to run `git log -1
  --oneline`, `git show --stat HEAD`, `git rev-parse HEAD` in parallel). Codex showed three "Running" lines and drew
  one prompt at a time:
  - prompt 1 `git rev-parse HEAD`: pill "Needs you · checker", strip "Needs you · 1 SHOP-35 allow a command", Overview
    "Checker wants to run a command · $ git rev-parse HEAD". `c2-checker-three-parallel-1st-prompt.jpg`,
    `c2-checker-1st-named-rev-parse.jpg`
  - "y" → prompt 2 `git log -1 --oneline` drawn; ~1 s later Needs you · checker naming **git log -1 --oneline**.
    `c2-checker-2nd-prompt-git-log-drawn.jpg`, `c2-checker-2nd-named-git-log.jpg`
  - "y" → prompt 3 `git show --stat HEAD`, named **git show --stat HEAD**. `c2-checker-3rd-prompt-git-show-drawn.jpg`,
    `c2-checker-3rd-named-git-show.jpg`
  - "y" → "Checking · round 1 of 3", strip back to "Your review". `c2-checker-after-3rd-y-checking.jpg`
  - (Re-test 5's NEW-2, "the second queued prompt never reads Needs you", is gone: 3 of 3 here, and again on SHOP-35's
    second validation round.)
- **A ticket's agent** (HOOK-5 Dev, address-review): Codex queued `git log main -5 --oneline` and `git status --short;
  git diff --check; git diff -- README.md`. Overview named the one on screen ("git status --short; git diff --check;
  …"); after "y" it named **git log main -5 --oneline**. Pill Needs you both times.
  `c2-dev-agent-1st-of-two-named.jpg`, `c2-dev-agent-2nd-of-two-named-after-y.jpg`. SHOP-36's Dev with three parallel
  commands: session header "Waiting for you" on each. `c2-dev-three-parallel-first-prompt.jpg`,
  `c2-dev-second-prompt-waiting-for-you.jpg`
- **Delivery session / a session while the ticket is in Review: never Needs you.** SHOP-36's delivery-message turn
  (three parallel git commands) and HOOK-5's push skill stopped at Codex prompts while the header read **"Live"**, the
  pill "Ready for you", and the strip had no Needs you. `c2-delivery-session-parallel-prompt-header-live.jpg`,
  `new-review-push-skill-prompt-header-live-no-needs-you.jpg` → NEW-2.
- **Noyan**: single prompts read "Needs you · Update docs skill wants your permission: Run command: …" in the editor's
  line and "Waiting for you" in the panel. `c2-noyan-prompt-needs-you-line.jpg`. Queued Noyan prompts didn't come up.
- **Helpers**: not tried (no helper run this session).

### PASS · 3 · A prompt while a merge check is being fixed reads Needs you
- SHOP-35 "Merge check failed · fixing", its restack turn stopped at `otaq report SHOP-35 9; git status --short; git
  log …`: pill **Needs you**, strip "Needs you · 1 SHOP-35 allow a command", session "Waiting for you"; Overview
  "Merge check failed on main · 1 of 3" plus "Developer wants to run a command". `c3-fixing-prompt-pill-strip-needs-you.jpg`
- Card: "Asks you · run command: otaq report SHOP-35 9; git status --short; git log --oneline --decorate -5; g…",
  turquoise, Open. `c3-fixing-prompt-card-asks-you.jpg`
- "y" → pill back to **"Merge check failed · fixing"**, strip no Needs you. `c3-after-y-fixing-again.jpg`

### PASS · 4 · A message for a session that ended doesn't wait forever
- SHOP-35: while its checker ran (Validate) I added a comment ("Comment 2 from the tester…"); `pending_deliveries` held
  "The user added comment 3. Run `otaq prime`…" for the running validator, then it was typed at its turn end. The
  checker passed → Review. `c4-checker-passed-review.jpg`
- Send back · to Dev with a comment (after NEW-6's dialog, see there): the Dev session header shows **no "message
  waiting"**, through Dev, the second Validate and Review, until the merge. `c4-sent-back-dev-header-no-waiting.jpg`
- `pending_deliveries` had no row for any session started this session (checked before and after the send back);
  re-test 5's leftover rows from ended sessions (SHOP-2 … SHOP-32) are still there but belong to closed tickets and show
  nowhere.

### PASS · 5 · "Unsent text in the input line" shows only when the line holds text
- SHOP-35's restack agent ran `otaq stage next` at a prompt; I pressed **"y" twice**: the first answered it, the second
  landed in the now-empty line ("› y", Codex "tab to queue message"). `c5-stray-y-in-line-after-prompt-closed.jpg`
  The ticket went to Validate; my comment 10 ("the failcheck.flag file has been removed…") waited for the Dev session.
- One Backspace emptied the line: within 2 s "**The user added comment 10. Run 'otaq prime' to read it.**" was typed
  and submitted; the footer read "Typing goes to the agent", **no "Unsent text" footer**.
  `c5-line-emptied-no-footer-note-typed.jpg`
- Never saw "Unsent text in the input line" over an empty line. The positive case wasn't seen either: in 1 (text in
  the line, a note waiting) the ticket was "fixing", and for a busy/fixing agent the footer says "Otaq's notes only
  arrive when the agent is idle and your input line is empty" by design.

### PASS · 6 · The card says Needs you while you're in the agent's terminal
- SHOP-35 Dev at a Codex prompt, terminal focused (footer **"Typing goes to the agent · you're here, so no alert"**):
  the pill reads **Needs you**, the strip "Needs you · 0 here · You're through shop's items" (no chip, no sound or
  notification), and the card behind the sheet shows **"As[ks you] · RE…"** in turquoise, as unfocused (re-test 5: the
  card read "Working"). `c6-focused-sheet-prompt-strip-no-alert.jpg`, `c6-focused-card-edge-asks-you-zoom.jpg`
- Same in the fixing state with focus: pill Needs you, strip without a chip.
  `c6-focused-prompt-strip-no-needs-you-pill-needs-you.jpg`
- The card's quiet line "in terminal · no alert" couldn't be read: focus needs the sheet open on its Session tab, and the
  sheet covers the card's text. For the same reason I couldn't see whether the Overview's Asks-you answer box hides while
  the terminal is focused (it isn't on screen while you are in the terminal).

### not tried · 7 · A "login expired" stop offers Sign in… at once
All four tools stayed Connected; no login expired, and I didn't sign anything out. Resume after sign-in: not tried.

### PARTLY · 8 · On Windows, Ctrl+C in a terminal copies when text is selected
- **Shell terminal** (PowerShell in the Terminal drawer, no mouse capture): with `echo hello-ctrlc-test` on the line,
  drag-select "hello-ctrlc-test" → Ctrl+C: clipboard = "hello-ctrlc-test" (was SENTINEL-4), the line kept its text, no
  ^C. Ctrl+C with nothing selected: **^C**, new prompt. `c8-shell-selection-before-ctrl-c-zoom.jpg`,
  `c8-shell-ctrl-c-no-selection-interrupts-zoom.jpg`
- **Codex session, plain drag**: Codex takes the mouse, so a drag is Codex's own selection ("Copied 43 chars to host
  clipboard", footer "ctrl+c copy · enter copy & follow · esc clear"). Ctrl+C then kept the draft "my draft for ctrl c
  test", clipboard "Submitted the delivery message for SHOP-36 ". So the user's check passes, but via Codex, not Otaq.
  `c8-codex-selection-with-draft.jpg`, `c8-ctrl-c-copied-draft-kept.jpg`
- **Codex session, Shift+drag** (xterm's own selection over a mouse-capturing program; the highlight shows): **Ctrl+C
  copied nothing (clipboard stayed SENTINEL-2) and wiped the draft** ("› Ask Codex to do anything"), 2 of 2.
  `c8-FAIL-shift-selection-before-ctrl-c.jpg`, `c8-FAIL-ctrl-c-wiped-draft-not-copied.jpg`,
  `c8-FAIL-retry-shift-selection-ctrl-c-wiped-zoom.jpg`. **Ctrl+Shift+C** on a Shift+drag selection: the highlight went,
  the draft stayed, but the clipboard stayed SENTINEL-3 (nothing copied), 1 of 1.
  `c8-shift-selection-before-ctrl-shift-c-zoom.jpg`, `c8-ctrl-shift-c-selection-gone-not-copied-zoom.jpg` → NEW-1.
  (Selections were made with vmpal's key-down Shift + mouse down/move/up; the highlight was visible each time.)

### PASS · 9 · Every view names the stage a held ticket starts at
- Held ticket: NOTE-18 (notes, Spec first, all four spec stages skipped, starts at Dev). A temporary `post-checkout`
  hook in `notes\.git\hooks` that exits 1 made its start fail ("Start failed: post-checkout hook refused the start",
  Retry start). NOTE-19 waits for it. `c9-start-failed-sheet.jpg`
- Board: NOTE-18 in the **Dev** column ("Start failed · post-checkout hook refused the start · Skips Product spec, Tech
  spec, Mi…", Retry). `c9-gap-held-card-in-dev-column.jpg`
- Palette (Ctrl+K): suggested "Next that needs you NOTE-18"; Recent "NOTE-18 Retest6 held at Dev · **Dev**".
  `c9-palette-names-dev.jpg`
- Filter `in:dev`: the suggestion "in:dev · in Dev · 1"; applied, the Dev column shows NOTE-18 (1/1), the list
  "NOTE-18 Retest6 held at Dev · Start failed". `c9-filter-in-dev-shows-held.jpg`
- Dependencies: NOTE-19's sheet "WAITS FOR · NOTE-18 Retest6 held at Dev · **Dev · start failed**".
  `c9-dependency-names-dev.jpg`
- Sheet Move…: **Dev ticked** (current); Tech spec / Milestone plan / Validate "NOTE-18 can't go to … yet".
  `c9-sheet-move-menu-dev-ticked.jpg`
- Drag the card: over every column, including its own Dev column, the drop box reads **"NOTE-18 can't go to Dev yet ·
  It starts by itself when it can, in priority order."**; dropping back on Dev gives the same toast.
  `c9-drag-over-own-dev-says-cant-go-to-dev-yet.jpg`, `c9-drop-back-toast-cant-go-to-dev-yet.jpg` → NOTE-3.
- Caveat: the "held" kind here is "start stopped/failed"; a ticket held for an agent or a limit didn't come up.
- Cleanup: the post-checkout hook was removed; NOTE-18 stays in Dev "Start failed" (Retry), NOTE-19 in Backlog.

### PARTLY · 10 · A newer "Ask the agent to write it" replaces one that never started
- shop → Edit flow → added "Update the docs" (A, Developer, `update-docs` missing) and "Security review" (B, Checker,
  `security-check` missing).
- Try 1: A → "Ask the agent to write it" → Send: Noyan started a **new conversation** ("Update docs skill"), A read
  "Being written by Noyan… Stop"; it named its folder (`otaq flow write-skill "Update the docs" --name …`). Then B →
  Ask → Send: **another new conversation** ("Security review skill"); B went through ("Being written by Noyan…"), A
  still "Being written…". `c10-try1-a-named-folder-before-b-both-being-written.jpg`
- Try 2 (after Stop on both): A asked, B asked 1.2 s later (A's agent hadn't run anything): **B went through, and A
  still read "Being written by Noyan… Stop"**. The Noyan menu shows two live conversations, "New conversation" and
  "Docs stage skill", both "Waiting for you". `c10-try2-a-still-being-written-after-b.jpg`,
  `c10-a-still-being-written-two-conversations.jpg` → NEW-3.
- Cleanup: Stop on both, Edit flow → Cancel; an empty `shop\.agents\skills` folder was left behind after the two
  Stops (I removed it; NOTE-5). Flow back to Dev → Validate → Review, `git status` clean.

## Issue 10's gaps

### PASS · A held ticket's column
NOTE-18 (start failed, starts at Dev) is in the Dev column on the board (see 9).

### PASS (with a note) · "Removing waits until no ticket is in it"
- shop → Edit flow → Review with SHOP-36 in it: the panel foot reads **"Removing waits until no ticket is in it ·
  Remove stage"**. `gap-editflow-review-with-ticket-removing-waits.jpg`
- notes → Edit flow → Dev, whose column holds the start-failed NOTE-18: only "Remove stage", no note.
  `gap-editflow-dev-with-held-card-no-remove-note.jpg` → NOTE-4.

### PARTLY / blocked · Round-4 piece 23's answer sessions
- Set up: `otaq dep add SHOP-36 SHOP-35`, then as SHOP-36's agent `otaq ask SHOP-35 --stdin` ("which line did you add
  to README.md? Please check with git show…"). SHOP-35 was merged, so Otaq resumed it answer-only: a session of kind
  `answer` (Codex) started, the chip read "1 running · SHOP-35 Retest6 restack · Developer", the message "Delivered ·
  SHOP-35 is finished, resumed answer-only".
- **It can't be reached**: SHOP-35's card read "Merged into main" (no Needs you), the sheet's Session tab showed the
  old ended Dev session, the running chip's row opened that same sheet, `otaq attach SHOP-35` said "SHOP-35 has no live
  session", History lists "Session · Developer · Developer · 20:22 · running" without a way in. After 12 min no answer
  was stored. `gap-answer-session-card-merged-no-needs-you.jpg`, `gap-answer-session-running-history-only.jpg` → NEW-4.
- So "a key alone answers a Codex prompt there" couldn't be tried, nor "Claude Code still needs Enter". I ended the
  session by killing its codex process (session `ended · exit 1`); SHOP-35's re-made worktree `home\wt\shop\SHOP-35`
  is left.

### PARTLY · Approve & open PR / Push to PR with pre-push and commit-msg hooks
- **The real GitHub / Azure DevOps paths couldn't be tried: the VM has neither gh nor az.** The hooks re-test's
  "Manual checks for the maintainer" still stand.
- With the local bare remote and the `local-pr` skill: HOOK-5 (PR #2 open) → "Send back with PR comments" with a
  comment to add a README line. The address-review skill was submitted and Codex committed `fix: add retest note to
  README (HOOK-5)` first try through the feat/fix commit-msg hook (hook log 20:28:03: pre-commit, prepare-commit-msg,
  commit-msg, post-commit). Move → Review (skip Validate) → **Push to PR #2** ("1 new commit since the PR was opened")
  → the dialog "Push skill is preparing… · reading the commits" and the session line **"› $otaq:push" not submitted**,
  20 s+. `bug-otaq-push-typed-not-submitted-20s.jpg`
- Cancel + Enter in the session: the push skill ran (`otaq pr propose` with the commit `fix: add retest note…`) and was
  refused "HOOK-5 has no Open PR in progress"; Codex then wanted `otaq ask human` about it (Esc). A second Push to PR:
  the same unsubmitted `$otaq:push`; Enter while the dialog was up closed the dialog (flow cancelled). So pre-push (50 s)
  never ran via Push to PR this time; HOOK-5's new commit is not on the remote. → NEW-5.

## Regression walk — PASS
- **Review → Approve & merge with hooks** (HOOK): HOOK-3 (Validate · Interrupted) Move → Review → Approve & merge; the
  delivery agent was still writing, so I set the first line to `feat: add division (HOOK-3)` ("Use the proposed
  message" link appeared). Merge → toast "HOOK-3 merged into main · Squashed as c1ea67f". Hook log 20:33:36–37 in the
  scratch worktree: post-checkout, post-merge, pre-commit (TODO lint passed), prepare-commit-msg, commit-msg (feat/fix
  passed), post-commit; then post-merge in `~/otaq-reltest/hooksproj`. pre-rebase didn't run.
  `reg-hook3-merge-dialog-edited-message.jpg`, `reg-hook3-merged-toast.jpg`
- shop: SHOP-35 (5885c52, with the restack) and SHOP-36 (d285eac) merged with `node test.js`.
- Edit flow: opens on shop and notes, stages add, the details panel works, Cancel restores (see 10, gaps).
- Settings opens; Agents: all four tools Connected. `reg-settings-agents-connected.jpg`

## The hooks re-test's bug — still shows
`$otaq:push` typed by Otaq on its own lands in Codex's line and isn't submitted (2 of 2, NEW-5). Skill commands with
text after them were submitted every time: `$otaq:restack SHOP-35's merge check failed…`, `$otaq:address-review
HOOK-5 was sent back…`, `$otaq:validate`, `$otaq:work-ticket`, `$otaq:flow Write a skill for…`.

## New problems
- **NEW-1 (piece 8)** · In a Codex session, a Shift+drag selection (xterm's own, over a program that takes the mouse)
  isn't copied: Ctrl+C sends the interrupt and Codex clears the draft (2 of 2); Ctrl+Shift+C clears the highlight but
  copies nothing (1 of 1). A plain drag works only because Codex copies its own selection.
  `c8-FAIL-ctrl-c-wiped-draft-not-copied.jpg`, `c8-ctrl-shift-c-selection-gone-not-copied-zoom.jpg`
- **NEW-2 (piece 2)** · A Codex prompt in a ticket's session while the ticket is in Review (the delivery-message turn,
  the push skill) never reads Needs you: header "Live", pill and card "Ready for you", no strip chip. The agent sits at
  the prompt until someone opens the session. `c2-delivery-session-parallel-prompt-header-live.jpg`,
  `new-review-push-skill-prompt-header-live-no-needs-you.jpg`
- **NEW-3 (piece 10, decision 043)** · Every "Ask the agent to write it" starts its own Noyan conversation, so the
  older request is never the same agent's and is never dropped: A keeps "Being written by Noyan…" while B is written,
  and two Noyan conversations run side by side. `c10-try2-a-still-being-written-after-b.jpg`,
  `c10-a-still-being-written-two-conversations.jpg`
- **NEW-4** · An answer session (a finished ticket answering another ticket's question) runs invisibly: no Needs you,
  no session view, `otaq attach` says no live session. A Codex permission prompt there can't be answered; the question
  stayed unanswered 12 min. `gap-answer-session-card-merged-no-needs-you.jpg`, `gap-answer-session-running-history-only.jpg`
- **NEW-5 (hooks re-test bug, still there)** · `$otaq:push` alone isn't submitted in Codex; the Push dialog waits on
  "Push skill is preparing…", and pressing Enter in the session after Cancel runs a skill whose flow is gone.
  `bug-otaq-push-typed-not-submitted-20s.jpg`
- **NEW-6** · Send back on SHOP-35 while its delivery session showed a Codex permission prompt opened "SHOP-35 has unsent
  text in its terminal · Clear it and send / Cancel". The line held no text; a prompt was open. Holding back is right
  (typing would answer the prompt), but the words are wrong. `new-sendback-says-unsent-text-over-open-prompt.jpg`,
  `obs-review-delivery-prompt-session-live.jpg`
- NOTE-1 · Check 1's Claude Code half and check 2's helper and queued-Noyan cases weren't run.
- NOTE-2 · The positive "Unsent text in the input line" footer wasn't seen (it shows only for an idle or asking agent).
- NOTE-3 · Dragging the start-failed NOTE-18 over its own Dev column says "can't go to Dev yet · It starts by itself
  when it can, in priority order"; it is in Dev already, and a failed start doesn't start by itself (it needs Retry).
- NOTE-4 · Edit flow → Dev doesn't say "Removing waits…" while the start-failed NOTE-18 shows in Dev's column.
- NOTE-5 · Stopping two skill writes left an empty `.agents\skills` (the second write didn't make the folders, the first
  one's Stop couldn't remove them while the second's folder was there).

## Setup changes (what was put back)
| Change | Why | State now |
|---|---|---|
| Installed app 1af8dafb (service 8f0d3893) → **d0a165b5** | the re-test | installed d0a165b5, running on the release-test data (launch.cmd via `otaqtest`) |
| `retest-hooks` fast-forwarded to d0a165b5; `retest6.bundle`, `retest6-build.log`, `Otaq_0.1.0_arm64-setup-d0a165b5.exe`, `q6.js` (read-only DB query helper) added in `otaq-build` | build, checks | left |
| shop: setup commit 9a50f86 "Test setup: main moves (retest 6)" | make the merge check run (1) | not reverted; SHOP-35, SHOP-36 merged on top; main d285eac, clean |
| shop merge check `..\..\..\mc.cmd` + `failcheck.flag` | 1 | back to `node test.js`; flag removed |
| shop Edit flow draft (Update the docs, Security review, two write requests) | 10, gaps | Stopped and Cancelled; empty `.agents\skills` removed; flow unchanged |
| notes `.git\hooks\post-checkout` (exit 1) | 9 | removed |
| Tickets | checks | SHOP-35, SHOP-36 merged; SHOP-36 depends on SHOP-35 (added for the answer session). NOTE-18 Dev "Start failed" (Retry), NOTE-19 Backlog waiting for it. HOOK-3 merged (c1ea67f). HOOK-5 in Review "Ready for you", PR #2, one commit `fix: add retest note to README (HOOK-5)` not pushed. HOOK-4 unchanged (Needs you · branch isn't on the remote) |
| SHOP-35 answer session | gap 3 | killed (ended, exit 1); its worktree `home\wt\shop\SHOP-35` left |
| Noyan conversations: four from 10 ("Update docs skill", "Security review skill", "Docs stage skill", "New conversation") | 10 | left open, idle/waiting (not ended) |
| hooks log | regression | header line "### 20:25:31 retest 6 (d0a165b5) starts…" added; hooks unchanged |
| Codex answers | "y", Esc four times, never "p" | no Codex rules added |

## State left in the VM
The installed app (**d0a165b5**) is running on screen with its service on `otaq-reltest\home`. Codex processes still
running: HOOK-5's Dev session (live, Review) and the four Noyan conversations from check 10. shop: Backlog, Dev, Validate,
Review empty, main d285eac clean, flow Dev → Validate → Review, merge check `node test.js`. notes: NOTE-18 in Dev (start
failed), NOTE-19 in Backlog, main unchanged, no extra hooks. hooksproj: main c1ea67f, HOOK-4 and HOOK-5 in Review. All
four tools Connected; nothing signed in or out.
