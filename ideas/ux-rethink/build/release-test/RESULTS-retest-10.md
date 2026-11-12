# Re-test 10 · release-round7 on Windows · results (Windows 11 ARM64 VM, build release-fixes 14820575)

Date: 2026-11-12, 21:18–21:55 VM time. Build: `Otaq_0.1.0_arm64-setup-14820575.exe` (release-fixes 14820575, the
merged round 7), NSIS release bundle, installed over the test Otaq. Data: the normal release-test data
(`otaq-reltest\home`, `launch.cmd` via `otaqtest`), projects hooksproj (HOOK), notes (NOTE), shop (SHOP). Plan:
`tuq/plans/release-round7.md` (each piece's Check:), `tuq/prompts/release-round7-orchestrator.md` "Bringing it
together", and the open items of `REGRESSION-CHECKS-LEFT.md` the fixes unblock. Screenshots `shots-retest-10/r10-*.jpg`.

Privacy: no screenshot shows the Windows user name, a login, email or token (terminal paths read `~\otaq-reltest\…`;
the PowerShell paste shots are cropped after the user folder; the Agents settings page, which names a Copilot login,
wasn't saved). The clipboard was set only from PowerShell inside the VM and read back (Get-Clipboard / ContainsImage)
before each paste; at the end it was cleared. No code changed, nothing committed or pushed in the repo on the Mac. The
other Otaq (`~\otaq-qbox`, PIDs 8380/18580) was not touched. Agents: Claude Code v2.1.322 (Architect, auto mode) and
Codex v0.166.0 (shop's Developer).

## Summary
**All eleven pieces PASS on the release build; the three regression items they unblock PASS.** Two new findings,
neither a blocker in my judgement (see "New findings"): the Open PR "Pushing…" dialog can hang with no way out when the
delivery's skill never starts a turn (NEW-r10-1, the most serious one), and an answer to a question asked in Review
can't be sent once the agent's session ended by a restart (NEW-r10-2). Four small notes.

| # | Check | Result |
|---|---|---|
| Setup | Bundle fae1fd78..14820575, build, install `/S`, About | **PASS** · "Version 0.1.0 · build 14820575 · Built from source" |
| 1 | Long one-line message into Claude Code arrives whole; Approve dialog fills with the written message; a real Otaq skill command typed bracketed still runs | **PASS** · writer request arrived whole (1,360 chars in `<pasted_content>`), Approve dialog "Prepared by Architect while HOOK-15 waited in Review"; `/otaq:noyan` + an 883-char request ran as the skill with all its args |
| 2 | Ctrl+V pastes text into Claude Code and Codex once (unsent text seen); image in Claude Code; PowerShell once | **PASS** · text once in both, and both then read as unsent text; Codex pastes an image on Ctrl+V; Claude Code: Ctrl+V with an image does nothing, Alt+V pastes `[Image #1]` (known, not a FAIL); PowerShell pasted and ran once |
| 3 | Ctrl+V with an empty clipboard into Claude Code's bare prompt, then answer from Overview: no unsent-text dialog | **PASS** |
| 4 | ↵ in Something else's field sends once, no "wasn't sent" toast; digits type into the field | **PASS** · "V2 1 go on" typed whole, one answer delivered, no toast, one `needs-you answer` in the service log |
| 5 | Pick / tick / type, switch tab and back: kept | **PASS** · single (Gamma), several (ticks 1,2 after changing from 1,3) and Something else's text kept across Session / Changes and back |
| 6 | Long question (3 noted options + Something else): Send in view on a 908-px window, expanded, smallest window | **PASS** · 1920×908 maximized, ⤢ expanded, and the smallest window (1280×800, the app's minimum) |
| 7 | `otaq ask human` on a Review ticket: card, pill, strip Asks you; box above Approve / Send back; answer arrives; back to Ready for you; Send back closes it | **PASS** · notifications not checked (window focused: NEW-e2 posts nothing then). After a restart the answer can't be sent (NEW-r10-2) |
| 8 | Ctrl+J / strip on a start-failed or delivery-stopped ticket opens Overview | **PASS** · NOTE-18 (start failed) and HOOK-4 (delivery stopped) on Overview; HOOK-14 (Interrupted) on Session with Resume, as designed. No check-failed / check-no-result / parent-closed ticket exists in the data: those not observed |
| 9 | Done without a merge reads so on card, pill, subtitle, `otaq list` | **PASS** · SHOP-39 "Done · nothing to merge", HOOK-12 "Done · not merged" everywhere; merged ones still "Merged"; `--status merged` still lists SHOP-39/43 |
| 10 | Merge into another ticket: Spec review not offered; Code review / PR open is | **PASS** · NOTE-26: "Merge into another ticket…" greyed ("No other ticket of this base waits in review…"), NOTE-20 (Spec review) not offered; HOOK-13: picker lists HOOK-4 (Ready for you), HOOK-5 (PR #2), HOOK-10 (PR #3), not HOOK-14 (Dev) |
| 11 | Noyan's switcher lists an ended-by-restart conversation once | **PASS** · shop: ended-by-restart ones only in the top list, RECENTLY ENDED only the closed ones; the empty panel's RECENTLY ENDED still lists the ended ones |
| Reg | Claude Code: the written message reaches the Approve dialog | **PASS** (HOOK-15) |
| Reg | Claude Code: the written message reaches the Open PR dialog | **PASS** (branch, title, description filled, "Prepared by Architect…"). The push afterwards needed the NEW-13 test-data workaround and hit NEW-r10-1; PR #4 opened in the end |
| Reg | Ctrl+V in Claude Code and Codex (decision 048) | **PASS** (as check 2) |

## Setup
- `%USERPROFILE%\otaq-build\retest-hooks` was at fae1fd78. Bundle `r10-14820575.bundle` (fae1fd78..release-fixes,
  97 KB) sent with vmpal_send_files, verified, fetched, detached checkout at **14820575**.
- `retest-hooks-build.ps1` (+ `noupd.json`), log `otaq-build\retest10-build.log`: BUILD-EXIT=0 (Rust ~2 min each for
  the app and the sidecar), `Otaq_0.1.0_arm64-setup.exe` 19.45 MiB, kept as
  `otaq-build\Otaq_0.1.0_arm64-setup-14820575.exe`.
- Stopped only the test app (PID 13936) and `taskkill /T` its service (PID 22660, fae1fd78 runtime; HOOK-14's Claude
  session and other children went with it). Setup `/S` exit 0 (`otaq-app.exe` 21:23, `otaq.exe` 21:21). Started via
  `schtasks /run /tn otaqtest`. Dialog "1 agent was stopped by a restart · HOOK-14 still waits for your answer;
  answering resumes it" → Not now (`r10-start-restart-dialog-hook14-waits-answer.jpg`). A Windows Firewall prompt for the
  new runtime's `otaq` came on the first agent start: Cancel.
- About: **"Version 0.1.0 · build 14820575 · Built from source"** (`r10-about-14820575.jpg`).

## Checks (running log)

### 9 · Done without a merge · PASS
- `otaq list`: SHOP-39/43/44 and NOTE-24 "Done · nothing to merge", HOOK-12 "Done · not merged", HOOK-11/NOTE-21…
  "Merged". `otaq list -P SHOP --status merged` still lists SHOP-39 and SHOP-43 (filters by state). `otaq show
  SHOP-39`: "Status: Done · nothing to merge (Done)"; `otaq show HOOK-12`: "Done · not merged (Done)".
- SHOP-39 sheet: pill "Done · nothing to merge", subtitle "Done · nothing to merge", card the same
  (`r10-p9-shop39-pill-subtitle-card-nothing-to-merge.jpg`). HOOK-12 sheet: pill and subtitle "Done · not merged"
  (`r10-p9-hook12-pill-subtitle-not-merged.jpg`).

### 8 · Ctrl+J opens Overview · PASS
- notes board, strip "Interrupted · 1 · NOTE-18 start failed" → Ctrl+J → NOTE-18 on **Overview** with "Retry start" and
  "Start failed: post-checkout hook refused the start" (`r10-p8-ctrlj-note18-start-failed-overview.jpg`).
- Ctrl+J → NOTE-26 (Code review) Overview → NOTE-20 (Spec review) Overview → across projects to **HOOK-4 "Delivery
  stopped"** on **Overview** with "Retry push" and the reason (`r10-p8-ctrlj-hook4-delivery-stopped-overview.jpg`) →
  HOOK-14 (Interrupted after the install) on Session with "Resume", as the piece keeps it.
- Check-failed, check-no-result and parent-closed tickets: none in the release-test data, so not observed.

### 2 · Ctrl+V on Windows · PASS
- **Claude Code** (HOOK-14 resumed with Resume, bare `›`): clipboard `r10 paste check` (read back, no image) → Ctrl+V →
  the line read `r10 paste check` once (`r10-p2-cc-ctrlv-text-pasted-once.jpg`). A question asked then
  (`R10 q1`) → Send answer → **"HOOK-14 has unsent text in its terminal"** (the paste counts as unsent text,
  `r10-p2-cc-pasted-text-counts-as-unsent.jpg`) → Clear it and send → "Answer to your question: Gamma (option 3)."
  arrived (`r10-p2-cc-clear-and-send-gamma-delivered.jpg`).
- Claude Code, image (48×48 bitmap set by PowerShell, ContainsImage True, no text): Ctrl+V → nothing
  (`r10-p2-cc-ctrlv-image-nothing-known.jpg`; known: Claude Code on Windows pastes images on Alt+V); Alt+V →
  `[Image #1]` (`r10-p2-cc-altv-image-pasted.jpg`); one Backspace emptied the line.
- **PowerShell** (HOOK-14's Terminal tab): clipboard `echo r10-ps-paste` → Ctrl+V → typed once, Enter printed
  `r10-ps-paste` once (`r10-p2-ps-ctrlv-pasted-once.jpg`, `r10-p2-ps-ctrlv-ran-once.jpg`).
- **Codex**: new ticket **SHOP-48** "R10 Codex paste" (Codex Developer, told to reply "ready"/"noted" only) started
  from its card. Clipboard `r10 codex paste check` → Ctrl+V at "Ask Codex to do anything" → pasted once
  (`r10-p2-codex-ctrlv-text-pasted-once.jpg`); question `R10 cx1` → Send answer → "SHOP-48 has unsent text in its
  terminal" (`r10-p2-codex-pasted-text-counts-as-unsent.jpg`) → Clear it and send → delivered. Image on the clipboard →
  Ctrl+V → `[Image #1]` (`r10-p2-codex-ctrlv-image-pasted.jpg`); Backspace emptied the line.

### 3 · Empty clipboard Ctrl+V, then an answer · PASS
- Clipboard cleared from PowerShell, read back: no formats, no text, no image. HOOK-14 at its bare `›` → Ctrl+V →
  nothing typed (`r10-p3-empty-clipboard-ctrlv-bare-line.jpg`). Question `R10 q2` → Beta → Send answer → delivered at
  once, **no unsent-text dialog** ("You answered … Beta (option 2)", `r10-p3-answer-delivered-no-unsent-dialog.jpg`).

### 4 · ↵ in Something else sends once · PASS
- Question `R10 q3` → click Something else → typed `V2 1 go on`: the digits went into the field, no option picked,
  Something else stayed picked (`r10-p4-digits-type-into-something-else.jpg`). ↵ → "Answer to your question: V2 1 go
  on" delivered once; **no "wasn't sent" toast** (`r10-p4-enter-sent-once-no-toast.jpg`). Service log: one
  `needs-you answer ticket=HOOK-14 delivery=Sent { count: 1 }` per answer for all five HOOK-14 answers.

### 5 · Picks survive leaving Overview · PASS
- Single: picked Gamma (recommended Alpha) → Session → Overview: Gamma still picked
  (`r10-p5-gamma-kept-after-session-and-back.jpg`).
- Something else's text: `V2 1 go on` → Changes → Overview: picked and text kept
  (`r10-p5-something-else-text-kept-after-changes-and-back.jpg`).
- Several (`r8c-q-multi`, recommended 1,3): ticked Lint, unticked Price rounding → Session → Overview: Unit tests +
  Lint ticked (`r10-p5-ticks-kept-after-session-and-back.jpg`); sent "Unit tests (option 1), Lint (option 2)"
  (`r10-p5-ticks-kept-sent-1-2.jpg`).

### 6 · Long question keeps Send in view · PASS
- `r8c-q-long` (three noted options + Something else) on HOOK-14. Window maximized on the 1920×908 screen: the text
  is capped with its own scrollbar, all four options and **Send answer in view without scrolling**
  (`r10-p6-long-question-908-send-in-view.jpg`).
- ⤢ expanded: Send in view (`r10-p6-long-question-expanded-send-in-view.jpg`).
- Smallest window: dragged the window's corner as far as it goes → 1280×800 (client 1264×791, the app's minimum):
  Send in view (`r10-p6-long-question-min-window-1280x800-send-in-view.jpg`). Answered option 3; window maximized again.

### 11 · Noyan's switcher · PASS
- shop, Noyan panel: the empty panel's RECENTLY ENDED lists the ended conversations
  (`r10-p11-empty-panel-recently-ended-complete.jpg`). Switcher (▾): the "Ended by restart" ones only in the top list;
  RECENTLY ENDED holds only "Add Docs stage" and "Security review stage" (both closed, "Ended")
  (`r10-p11-switcher-each-once.jpg`). The two "Update docs skill" rows are two conversations (DB: different ids, both
  `ended_by_restart`), not a double.

### 10 · Merge into another ticket · PASS
- NOTE-26 (Code review) → Approve: "Merge into another ticket…" is greyed, "No other ticket of this base waits in
  review, to be delivered or with a PR open"; NOTE-20 (Spec review) isn't offered
  (`r10-p10-note26-merge-into-disabled-spec-review-not-offered.jpg`). Cancelled.
- HOOK-13 (Review) → ▾ → Merge into another ticket…: HOOK-4 "Ready for you", HOOK-5 "PR #2 open", HOOK-10 "PR #3
  open"; HOOK-14 (Dev) not listed (`r10-p10-hook13-merge-into-review-and-pr-hosts-only.jpg`). Cancelled.

### 7 · A question while the ticket waits in your review · PASS (with NEW-r10-2)
- HOOK-13 in Review "Ready for you" (its Architect session ended by the install). `OTAQ_TICKET=HOOK-13 otaq ask
  human` (`R10 p7`, Alpha/Beta/Gamma) → accepted. Pill **Asks you**, the box on Overview **above** the review's rows and
  the Send back / Approve bar (`r10-p7-review-ask-box-above-review-bar.jpg`); card stays in Review "Asks you · R10 p7…"
  with Answer, strip "HOOK-13 a question" (`r10-p7-review-ask-card-strip-asks-you.jpg`); `otaq list` "Review · Asks
  you", `otaq show` "Status: Asks you (Review) · Waiting on: R10 p7…".
- Send answer (Beta) → toast **"The answer to HOOK-13 wasn't sent · HOOK-13's session has ended"**; the question stays
  open (`r10-p7-answer-not-sent-session-ended-after-restart.jpg`) → NEW-r10-2.
- **Send back** with the question open (comment "test send back only, change nothing, run otaq stage next"): HOOK-13
  → Dev Working, strip item gone (`r10-p7-send-back-closed-question-dev-working.jpg`); DB `answered_via = 'closed'`.
  Architect ran `otaq stage next`, Validate (Architect) passed, back in Review within a minute, its session live.
- With the session live: `R10 p7b` asked → card/strip Asks you again (`r10-p7b-live-session-card-asks-you.jpg`) → Beta
  → Send answer → pill **"Ready for you"** again (`r10-p7b-answered-ready-for-you-again.jpg`), and the agent's
  terminal shows **"Answer to your question: Beta (option 2)."** (`r10-p7b-answer-arrived-in-agent-session.jpg`; the
  agent was puzzled, as the question came from the CLI, not from it).
- Notifications: not checked (the window was focused; NEW-e2: nothing is posted then).

### 1 and the regression items · Claude Code writer, Approve and Open PR dialogs · PASS
- hooksproj flow: proposal 49 (`otaq flow propose -f r8e-hook-flow-cc.yaml`: delivery agent Developer → Architect, as
  re-test 8e did), saved in Edit flow. **HOOK-15** "R10 Claude Code writer" (`--stage Dev=Architect --stage
  Validate=Architect`: append one README line) started from its card: commit ea53cda, Validate passed, Review in ~25 s.
- The writer (Architect, Claude Code, in HOOK-15's own session) got its request **whole**: the user turn in Claude's
  transcript is 1,360 chars, `<pasted_content id="8925">` + "HOOK-15 is waiting in Review. You are its delivery's
  agent: write the message…" … "…who may edit it." (the full request, not its tail). It ran `otaq pr message`; log
  "delivery message stored key=HOOK-15".
- **Approve & merge** dialog: "fix: append R10 writer check line to README (HOOK-15) · ## What changed …", "Prepared by
  Architect while HOOK-15 waited in Review. Edit it if you like." (`r10-p1-cc-approve-dialog-written-message.jpg`).
  Cancelled.
- **Approve & open PR**, first try: toast "Open PR for HOOK-15 didn't go through · Claude Code doesn't have the skill
  '/local-pr'" (`r10-reg-open-pr-local-pr-skill-missing-test-data.jpg`): re-test 8e's `.claude/skills/local-pr` copy
  was removed from hooksproj main afterwards (69ae147), so NEW-13's test-data gap is back. Committed the copy again on
  hooksproj main (96f04aa, test setup).
- Second try: the **Open PR dialog** filled with the written branch `otaq/HOOK-15-r10-writer-check`, title, description,
  "Prepared by Architect while HOOK-15 waited in Review" (`r10-reg-cc-open-pr-dialog-written-message.jpg`; its commit
  row still runs the subject into the trailer "…READMECo-Authored-By", NOTE-f3 from re-test 8). **Push & open PR** →
  "Pushing HOOK-15… · Pushing to origin…" and nothing more: Otaq typed `/local-pr The user confirmed the pull
  request…`, Claude Code answered **"Unknown command: /local-pr"** (the skill is in the project checkout, not on
  HOOK-15's branch), and the dialog stayed with Cancel greyed and Esc doing nothing over the whole window
  (`r10-new-pushing-dialog-stuck-no-cancel.jpg`, `r10-new-attach-unknown-command-local-pr.jpg`) → NEW-r10-1.
- Way out: `otaq attach HOOK-15` in a terminal; copied the skill into HOOK-15's worktree (`.claude/skills/local-pr`,
  untracked) and typed `/local-pr The user confirmed the pull request.`: the skill ran, the pre-push hook passed, it
  pushed and reported `otaq pr opened …/pull/4`; the dialog closed and HOOK-15 reads **"PR #4 open"**
  (`r10-reg-cc-hook15-pr-open-after-skill.jpg`). Then removed the worktree copy, removed the skill copy from hooksproj
  main (4ca1d80), and put the flow back (proposal 50 from `r8e-hook-flow-before.txt`, saved; `otaq flow show -P HOOK` =
  that file line for line).
- **A real Otaq skill command typed bracketed:** hooksproj Noyan panel, agent Architect (Claude Code), an 883-char
  one-line request pasted (clipboard set by PowerShell, read back, ending `END-OF-R10-REQUEST`) → Enter. Claude's
  transcript: `<command-name>/otaq:noyan</command-name><command-args>R10 long request check: … END-OF-R10-REQUEST
  </command-args>`: the skill ran with the whole request (it ran `otaq prime --project`, named itself "Flow stages
  overview" and answered) (`r10-p1-cc-noyan-skill-883-char-request-ran.jpg`).

## New findings
- **NEW-r10-1 · The Open PR "Pushing…" dialog hangs with no way out when the delivery's skill never starts a turn.**
  After Push & open PR, Otaq typed `/local-pr …` into Claude Code, which answered "Unknown command: /local-pr"; no turn
  ran, so neither `otaq pr opened` nor a skill problem ever came. The dialog stayed at "Pushing to origin…" with
  Cancel disabled (`Dialogs.tsx` "pushing": `onCancel={ignore}`, Cancel `disabled`; `pr.cancel` refuses a flow in
  Pushing, `pr/service.rs` `cancel`), over the whole window, for 5+ minutes; a later turn of my own in that session
  didn't end it either. The trigger here: Otaq's skill check counts the project checkout's skill folders even when the
  ticket has a worktree (`board/skill_tools.rs` `skill_folders`: `checkout: Some(checkout)` beside `worktree`), while
  Claude Code in the worktree sees only the worktree's (and personal) skills, so a project skill that's in the main
  checkout but not on the ticket's branch passes Otaq's check and fails in Claude. **Not a release blocker in my
  judgement**: it's not from round 7 (the PR flow and skill check are older), and it needs a delivery skill that's in
  the checkout but not on the ticket's branch (added to main after the ticket branched, or left uncommitted). But it
  is the most serious thing seen: the window is locked until the skill is run by hand or the app restarts. Known-issues
  list; worth a timeout or an enabled Cancel in Pushing, and checking the worktree only when there is one.
- **NEW-r10-2 · An answer to a question asked in Review can't be sent once the agent's session ended.** HOOK-13's
  question, asked after the install restart ended its Architect session: Send answer → "The answer to HOOK-13 wasn't
  sent · HOOK-13's session has ended", and the box keeps offering Send answer, the ticket keeps Asks you. In Dev the
  same case resumes the session with the answer (Interrupted). `attention/needs_you.rs` sends to a running Validator
  (send-back questions only) or Agent session, else refuses. Code reading, not reproduced: a question asked from the
  Delivery's own session (a writer whose agent isn't the Dev agent) would be refused the same way. **Not a blocker**:
  it needs a restart (or that Delivery-session case) between ask and answer; Send back closes the question and Approve
  still works. Known-issues list.
- NOTE-r10-3 · `otaq list`: "Done · nothing to merge" is one character wider than the status column, so the title on
  those rows starts one column later than on the others. Cosmetic.
- NOTE-r10-4 · On Overview the long question's text wraps at a narrow column (about 350 CSS px) also when the sheet is
  expanded on a 1920-px screen, leaving most of the box's width empty. Cosmetic; maybe meant as a reading measure.
- NOTE-r10-5 · After the restart, sheet subtitles count from it: HOOK-4 "Dev by Codex · 5 min", NOTE-20 "… · 1 min"
  for tickets that had waited for hours. Cosmetic.
- NOTE-r10-6 · Right after the restart the strip read "HOOK-14 idle" while the ticket was Interrupted (Resume on its
  Session), and the restart dialog said "HOOK-14 still waits for your answer" with no question open. Minor wording.

None of these is from a round 7 piece, and none starts another round: NEW-r10-1, NEW-r10-2 and the notes go on the
known-issues list.

## Setup changes and state left
| Change | State now |
|---|---|
| Installed Otaq (`%LOCALAPPDATA%\Otaq`) fae1fd78 → **14820575** | **left on this release build**, the intended state after a re-test; installer kept as `otaq-build\Otaq_0.1.0_arm64-setup-14820575.exe` |
| `otaq-build\retest-hooks` fae1fd78 → 14820575 (detached; ref `r10/release-fixes`) | left; build log `otaq-build\retest10-build.log` |
| The app and its service | **closed** (app PID 1600, service PID 14476 with `taskkill /T`; this ended HOOK-13/14/15's Claude sessions, SHOP-48's Codex session, the Noyan conversation and NOTE-26's Codex writer) |
| hooksproj flow: delivery agent → Architect (proposal 49) | **put back** (proposal 50); equals `r8e-hook-flow-before.txt` |
| hooksproj main: 96f04aa adds `.claude/skills/local-pr`, 4ca1d80 removes it | two test-setup commits on main, tree as before |
| HOOK-13 "R8f2-base-merge" | Review · Ready for you; sent back once (no change) and passed again; its R10 questions closed / answered |
| HOOK-14 "R9 question box" | Dev · Idle (session ended at close); six R10 questions asked and answered; no files changed |
| **HOOK-15** "R10 Claude Code writer" (new) | Review · **PR #4 open**, branch `otaq/HOOK-15-r10-writer-check` pushed to `hooksproj-remote.git` (ea53cda), not merged |
| **SHOP-48** "R10 Codex paste" (new) | Dev · Idle (session ended at close); no files changed, no commits |
| NOTE-26 | Code review · Ready for you (unchanged; its Codex writer started while I opened Approve) |
| hooksproj Noyan conversation "Flow stages overview" (Architect, Claude Code) | ended at close; it only answered, created nothing |
| `otaq-reltest\r10-ask.ps1` | left (asks a test question with three options; no secrets) |
| Scheduled task `r10attach`, `r10-attach.cmd` | created for `otaq attach`, both deleted |
| Clipboard | cleared |
| Deleted | `otaq-build\piece3`, `otaq-build\Otaq_0.1.0_arm64-setup-r7p2-86016e3d.exe`, `Desktop\r10-14820575.bundle` |
| `~\otaq-qbox` Otaq | untouched (PIDs 8380/18580 still running) |
