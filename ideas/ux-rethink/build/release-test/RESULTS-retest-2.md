# Re-test 2 · release-issues on Windows · results (Windows 11 ARM64 VM, build release-fixes ac5e6200)

Date: 2026-11-10 (into 11-11, 23:22–00:45 VM time). Screenshots: `shots-retest-2/` next to this file (JPEG; the GitHub
login on Copilot rows, two Windows paths in Codex prompts and the desktop icons on the first-run shots are blacked out).

## Summary
Pieces (Check lines): **PASS 18 · FAIL 3 · NOT APPLICABLE 1** (of 22). Issue 03: all four parts PASS (one new problem
on the way). Regression walk: PASS.

FAIL
- **Piece 7** · After Keep them running and reopening, the title bar now has the chip, but it says "2 running" while
  the tray says "1 agent running" and Quit says "1 agent is working": the chip counts an idle Codex agent the tray
  lists under Needs you ("SHOP-19 · idle").
- **Piece 8** · A Codex agent that answered and stopped at its prompt reads Idle / Needs you (PASS for that case), but a
  parked Codex ticket dragged back (unparked: Codex resumes its conversation and sits at its empty prompt after
  "Conversation interrupted") reads "Working" for over a minute, twice.
- **Piece 17** · Needs you replaces "drafting a flow" at a prompt (PASS), but after answering, the drafting line did not
  come back: Needs you stayed up, naming the command just approved and run, while Noyan's terminal said "Working (45s ·
  Running hook)", until the next prompt (about 10 s later). See the piece for why this may be the hook's next prompt.

New problems outside the Checks: NEW-1 (Ask the agent to write it… fills the wrong stage), NEW-2 (Sign in… on a
ticket's sheet opens an empty terminal panel), NEW-3 (unsent text lost when the restack request is typed), NEW-4 (a
later-turn wait on a ticket Noyan created goes under CREATED with no Undo), and three smaller notes.

## Setup
- Build: branch release-fixes at **ac5e6200** ("Merge piece 14: Undo on Noyan's CHANGED row"), brought in as a git
  bundle, cloned to `%USERPROFILE%\otaq-build\retest2\repo`, built with the shared `CARGO_TARGET_DIR`
  (`%USERPROFILE%\otaq-build\target`): `npm.cmd ci` + `npm.cmd run bundle -- --bundles nsis --config noupd.json`
  (`{"bundle":{"createUpdaterArtifacts":false}}`) → BUILD-EXIT=0, `Otaq_0.1.0_arm64-setup.exe` (18.96 MiB). The old
  service (7f36bd79 runtime) was stopped (no app was running), the setup installed with `/S` (exit 0;
  `%LOCALAPPDATA%\Otaq\otaq-app.exe` 23:32, `otaq.exe` 23:27; `HKCU\Software\Classes\otaq\shell\open\command` points at
  the installed `otaq-app.exe`) and started with the release test's `launch.cmd` (OTAQ_HOME=`%USERPROFILE%\otaq-reltest\home`).
  About: "Version 0.1.0 · build ac5e6200 · Built from source". `setup-about.jpg`
- Order: the maintainer was available for an hour, so issue 03 and pieces 2/3 (everything that signs Claude Code out
  and in) ran first; every sign-in was done through the app's own Sign in… and needed no help.
- Data: the release test's shop (Otaq default flow, Codex Developer/Checker) and notes (Spec first, Claude Code spec
  stages, Codex Dev/Validate). Claude Code signed in (maintainer), Codex and Copilot signed in. Every Codex command asks
  permission, answered in Session.
- How Claude Code was signed out: by moving `%USERPROFILE%\.claude\.credentials.json` aside (and back), three times;
  for "login expired" the stored tokens were replaced with invalid ones and an expiry in the past, so the running
  session's next request failed. Each was put back by the app's Sign in… (`claude auth login`, authorised in Edge in
  the VM) or by moving the file back.
- Setup changes for single checks, all put back unless said:
  - A "Retestcop" agent on GitHub Copilot (pieces 1 and 1-retry), added twice and deleted twice. Put back.
  - A personal Codex skill `%USERPROFILE%\.codex\skills\retest-docs` (with and without a byte order mark, pieces 4/16)
    and, for a minute, a copy in `%USERPROFILE%\.claude\skills\retest-docs`. Both removed.
  - shop's flow: an "Update the docs" stage before Dev (skill retest-docs), then also a draft "Update the docs 2".
    Removed in Edit flow and saved: shop is back on Dev → Validate → Review.
  - A user skill `shop\.agents\skills\keep-me` (piece 10). Removed; shop's folder is clean.
  - shop's merge check set to `..\..\..\mc.cmd` with `otaq-reltest\failcheck.flag` (piece 9). Back to `node test.js`,
    flag removed.
  - A setup commit on shop's main, 1ba63a2 "Test setup: main moves (retest 2)" (adds `setup-move.txt`), so the merge
    check ran. **Not reverted**: SHOP-19 (ea36c00) and SHOP-25 (f7ae8de) were merged on top of it; main is f7ae8de, clean.
  - A throwaway first-run data folder and clone (`otaq-build\retest2\fresh-home`, `freshproj`) and the trust lines the
    app added for it in Codex's `config.toml`, Copilot's `config.json` and `.claude.json`. All deleted; the scheduled
    task otaqfresh deleted.
  - Credential backups made during the test were deleted at the end.
  - Tickets SHOP-19…28 and NOTE-9…13: merged (SHOP-19, SHOP-25), closed, or deleted (SHOP-22/23). Four new Noyan
    conversations ("Retest2 README tickets", "Write docs skill", two "Reply with the single…") are left, Idle.

## Pieces · each Check line
- PASS · 1 · Restarts (app and service killed, reopened at once) never showed "not installed". Copilot: SHOP-27
  (Dev on Retestcop, working) → 1.5 s after launch "2 agents were stopped by a restart · SHOP-27 Retest2 p1 cop · was
  working in Dev", ticked, "Resume 1" (the Codex ticket SHOP-26 sat at a permission prompt: "SHOP-26 still waits for
  your answer"). Codex: SHOP-19 working → "SHOP-19 Retest2 codex · was working in Dev", ticked; and two Noyan
  conversations on Codex ticked in another restart ("Resume 2"). Not seen in one dialog: a Codex and a Copilot ticket
  row both ticked with "Resume 2", because every Codex command asks permission, so a Codex ticket killed mid-work was
  at a prompt (and listed under "still waits for your answer") in the controlled run. `p01-copilot-row-ticked-1s.jpg`,
  `p01-codex-row-ticked.jpg`
- PASS · 2 · Restart with two Noyan conversations (Codex) and NOTE-11 (Product spec, Claude Code signed out): "3 agents
  were stopped by a restart", two ticked rows, NOTE-11 unticked "Claude Code isn't signed in · sign in first · Sign
  in…", "Resume 2". Unticking one → "Resume 1", title unchanged. Another run: "3 agents were stopped" with one ticked
  row → "Resume 1". `p02-title-3-resume-2.jpg`, `p02-untick-resume-1.jpg`, `p02-restart-codex-and-signed-out.jpg`
- PASS · 3 · "3 agents were stopped…", NOTE-11's row "Claude Code isn't signed in · sign in first · Sign in…",
  "Resume 0". Sign in… opened an inline terminal running `claude auth login` (Edge opened the authorise page). Dropped
  (Ctrl+C in the terminal): "Not done yet — see the output above · Retry", row still unticked, "Resume 0". Sign in…
  again and Authorise in the browser: the row became "NOTE-11 Retest2 share notes · was waiting for you", ticked, button
  "Resume 1"; title still "3 agents were stopped". `p03-restart-signed-out-row.jpg`, `p03-sign-in-terminal.jpg`,
  `p03-sign-in-dropped.jpg`, `p03-after-sign-in-ticked.jpg`
- PASS · 4 · New stage "Update the docs" (Developer on Codex) with the personal skill retest-docs saved with a BOM: the
  card says "Codex doesn't load the skill `$retest-docs`: its SKILL.md starts with a byte order mark" and the panel says
  the same with "Pick one, or Ask the agent to write it."; also after Save and reopening Edit flow. With the mark
  removed and the skill picked again: no warning on card or panel. `p04-bom-picker.jpg`, `p04-bom-stage-warns.jpg`,
  `p04-nobom-no-warning.jpg`, `p04-saved-stage-bom-card-and-panel.jpg`
- PASS · 5 · SHOP-19 (Dev on Developer, the delivery's agent too) reached Review; its own Dev session showed "1 message
  waiting · Deliver now" behind text I left in its input line ("unsent draft line left by the tester"). Approve & merge
  with the failing merge check → "Merge check failed on main", SHOP-19 back to Dev. The Dev session then got only
  "$otaq:restack SHOP-19's merge check failed…"; no "You are its delivery's agent: write the message…" was typed in,
  then or later. (See NEW-3: my unsent text was gone.) `p05-writer-request-queued.jpg`, `p05-only-restack-delivered.jpg`
- PASS · 6 · SHOP-25 in Review with the writer asked (log "the delivery's agent writes the message"), app and service
  killed (the writer's Codex ended). After reopening, Approve & merge first said "Developer is still writing the
  message…", and the service log says "no session writes the delivery's message: asking again"; within seconds a new
  Developer session started and the dialog changed to "Developer wants to run a command to write the message… [Answer
  in terminal]". Merged with the plain message (f7ae8de). `p06-approve-after-restart-asks-again.jpg`
- FAIL · 7 · With NOTE-12 working (Claude Code) and SHOP-19's Codex idle at its prompt: title chip "2 running", tray "1
  agent running · notes 1" with SHOP-19 under NEEDS YOU "idle". Tray → Quit Otaq → "1 agent is working… [Keep them
  running]". Reopened: the chip is back (issue 06's missing chip is fixed) but reads **"2 running"** (list: SHOP-19
  Developer · idle, NOTE-12 Checker · needs you), while the tray still says "1 agent running". Expected: the chip shows
  what the tray counts ("1 running"). `p07-before-quit-chip2-tray1.jpg`, `p07-quit-question.jpg`,
  `p07-reopened-chip-list.jpg`, `p07-reopened-tray-1-running.jpg`
- FAIL · 8 · Works after a turn: SHOP-19's Codex was told to reply with one line and wait; it answered and sat at its
  empty prompt: card "Idle · Answer", strip "SHOP-19 idle", sheet "Idle", within 15 s. `p08-codex-idle-card.jpg`,
  `p08-codex-idle-session.jpg`. Fails after unparking: SHOP-25 (Codex, parked from Update the docs) dragged back: Codex
  resumed its conversation, showed "Conversation interrupted - use /feedback…" and the empty "Ask Codex to do anything"
  prompt, with nothing running, and the card, sheet pill and Session said "Working" for over a minute (until I typed to
  it). Same again after the second park from Dev. Expected: Idle / Needs you. `p08-unparked-codex-idle-reads-working.jpg`
- PASS · 9 · Merge check `..\..\..\mc.cmd` failing on a flag file: "Merge check failed on main · Merge check
  `..\..\..\mc.cmd` failed on the result rebased onto main (exit 1)…" with the output in the pink box: "MERGE CHECK
  FAILED: the test flag file failcheck.flag is present. Nothing in the repo can fix this; the tester removes it.", log
  still attached (SHOP-19.log). `p09-merge-check-output.jpg`
- PASS · 10 · With my own `shop\.agents\skills\keep-me\SKILL.md` in place, "Ask the agent to write it…" wrote
  `.agents\skills\update-docs\SKILL.md`; Cancel in Edit flow: `update-docs` is gone, `keep-me` stays.
- PASS · 11 · After the agent wrote it, the card reads "`update-docs` is written but not committed yet: commit it so
  agents get it" and the Skill field "update-docs · Written by the agent · Open · Undo · Saved in
  .agents/skills/update-docs in this project. Commit it so agents get it in their worktrees." (not "You don't have the
  skill"). `p11-written-not-committed.jpg` (see NEW-1: it went to the other stage)
- PASS · 12 · Removing Update the docs with tickets in it: "Update the docs removed. This waits until no ticket is
  between Update the docs and Dev: the 3 there now (2 in Dev, 1 in Update the docs) finish on the current order…".
  `p12-draft-line-stage-name.jpg`
- PASS · 13 · Noyan, asked to make SHOP-24 and SHOP-25 wait for SHOP-22 (both created by me): ran `otaq dep add` twice;
  panel "CHANGED 2 · ✎ SHOP-24 · now waits for SHOP-22 · Undo · ✎ SHOP-25 · now waits for SHOP-22 · Undo".
  `p13-changed-rows-now-waits.jpg`
- PASS · 14 · Undo on "SHOP-24 · now waits for SHOP-22": SHOP-24's wait is gone (card "Not started"), the row keeps its
  text and loses Undo. I removed SHOP-25's wait myself in its sheet: its row lost Undo. Noyan then made SHOP-24 wait
  for SHOP-11 (merged); I started SHOP-24 (Backlog → Update the docs): "SHOP-24 · now waits for SHOP-11 · Undo" kept its
  Undo. `p14-changed-row-with-undo.jpg`, `p14-undo-removed-wait.jpg`, `p14-user-edit-undo-gone.jpg`,
  `p14-after-column-move-undo-kept.jpg`
- PASS · 15 · One turn: Noyan ran `otaq create` twice and `otaq dep add SHOP-23 SHOP-22`; panel "CREATED 2 · IN BACKLOG ·
  SHOP-22 Retest2 dep A · SHOP-23 · waits for SHOP-22", no CHANGED row. `p15-created-only.jpg` (see NEW-4 for a later
  turn)
- PASS · 16 · Codex stage held: "Codex doesn't have the skill `$retest-docs` · Nothing started. Link the skill into
  Codex's skills folder…". Claude Code: Change agent… to Spec writer (Claude Code) on the same stage: "Spec writer runs
  on Claude Code, which doesn't have the skill `/retest-docs` for Update the docs" (and the same in the toast). The
  stage card, panel and Edit flow warnings also use `$retest-docs` for the Codex stage. (A held message on a stage
  running Claude Code itself wasn't produced.) `p16-codex-held-dollar.jpg`, `p16-claude-code-slash.jpg`,
  `p16-edit-flow-missing-skill-warning.jpg`
- FAIL · 17 · Ask the agent to write it… for a new stage (Noyan on Codex): the board line "Write docs skill is drafting
  a flow for shop… Show" while it worked; at its first permission prompt the line became "Needs you · Write docs skill
  wants your permission: Run command: Get-ChildItem … [Answer]" (PASS). After I answered "y", the line did not come
  back: Needs you stayed, now naming "Get-Content .agents/skills/keep-me/SKILL.md; Get-Content …" — a command the
  terminal shows as already approved and run — while the terminal said "Working (45s · esc to interrupt) · Running
  hook" and Noyan's header said "Waiting for you"; about 10 s later the next real prompt came. After the last answer
  Noyan finished (Idle) and neither line shows. Expected: "drafting a flow" back while the agent works between prompts.
  Caveat: "Running hook" may be Otaq's own permission hook for the next command, i.e. a prompt reported before Codex
  draws it, but the text named the earlier command. `p17-drafting-line.jpg`, `p17-needs-you-replaces-line.jpg`,
  `p17-stale-needs-you-while-working.jpg`, `p17-line-back-after-answer.jpg`
- PASS · 18 · NOTE-13 (Dev on Codex, told to ask for a send-back): `otaq ask human --send-back "Product spec"`; Overview
  question with "Send back to Product spec"; sent → NOTE-13 in Product spec. Comment: "CX Developer · Dev · 12:28 am ·
  Dev sent it back to Product spec: The tagline wording is a product question, so Product spec needs to settle it
  before Dev writes anything." `p18-send-back-comment.jpg`
- PASS · 19 · SHOP-19 at a Codex permission prompt ("Needs you", pill and card); "y" in the terminal: one second later
  the pill and card read "Working". Seen again on every Codex prompt answered in this re-test (card follows within
  1–2 s). `p19-codex-prompt-needs-you.jpg`, `p19-working-1s-after-y.jpg`
- PASS · 20 · Installed app (NSIS, the `otaq:` handler is the installed exe). Pop-up: "SHOP-28 needs permission · Run
  command…" clicked from the desktop → Otaq came to the front on SHOP-28's sheet (Session, at the prompt). Notification
  centre: "NOTE-12 session interrupted" clicked → Otaq on NOTE-12's sheet. (An earlier pop-up click while several
  restart toasts were cycling opened NOTE-11, whose toast was probably the one under the pointer by then.)
  `p20-popup-toast-2.jpg`, `p20-popup-click-opened-SHOP-28.jpg`, `p20-notification-centre.jpg`, `p20-opened-on-NOTE-12.jpg`
- PASS · 21 · SHOP-25 parked from Update the docs (round 1), dragged back, moved on to Dev, parked again from Dev
  (round 2). Dragging the parked card over Validate: "Returns to Dev, the stage it left"; dropped → it went to Dev.
  `p21-parked-twice-returns-to-dev.jpg`
- NOT APPLICABLE · 22 · Tests only; nothing seen.

## Issue 03 · checks that need Claude Code signed in
- PASS · Spec first on notes: NOTE-9 "Retest2 spec first" ran Product spec (Spec writer), Tech spec (Architect) and
  Milestone plan (Milestone planner) on Claude Code, each "Nothing to settle: README-only change…", and stopped in Spec
  review ("Product spec, Tech spec and Milestone plan by Claude Code", WHAT PLANNING DID lists all three). NOTE-11
  "Retest2 share notes" ran the real product-spec skill on Claude Code: questions on Overview ("Question 1 of about 4 …
  Recommended"), answered there. `i03-spec-first-stages.jpg`, `i03-product-spec-question.jpg`,
  `i03-spec-first-three-stages-on-claude-code.jpg`
- PASS · "Stopped · login expired": NOTE-11 waiting on its question, Claude Code's tokens replaced with invalid ones,
  question answered → card "Stopped · login expired" (red), strip "NOTE-11 stopped · login expired", sheet "Claude Code
  stopped · login expired · Sign in to Claude Code, then Resume: the agent goes on where it stopped. [Sign in…]
  [Resume] [Answer in terminal]". `i03-login-expired-card.jpg`, `i03-login-expired-overview.jpg`,
  `i03-login-expired-sign-in.jpg`, `i03-login-expired-session.jpg`
- PASS (with NEW-2) · Sign in… opens `claude auth login`: yes, the process `claude.exe auth login` started and Edge
  opened Claude's authorise page; but the sheet's Sign in… opened the bottom terminal panel showing "Nothing open. + →
  New terminal opens a fresh shell…", not the sign-in terminal. Authorised in the browser, `claude auth status` →
  loggedIn true. (In the restart dialog the sign-in terminal shows inline, piece 3.) `i03-sheet-sign-in-panel-empty.jpg`,
  `i03-sign-in-browser.jpg`
- PASS · Resume after signing in: NOTE-11 went on with the same conversation and asked "Question 3 of about 4 …".
  `i03-resumed-after-sign-in.jpg`
- PASS · The restart dialog's signed-out row: "Claude Code isn't signed in · sign in first · Sign in…" (piece 3).
- Sign-in state: Claude Code was signed back in by the app's own Sign in… (twice) and by moving the credentials file
  back; at the end `claude auth status` → loggedIn true, expires 2026-12-10. The maintainer's original login was
  replaced by these new logins (same account). Settings → Agents: Claude Code, Codex, GitHub Copilot and OpenCode
  "Connected". `reg-settings-agents-all-connected.jpg`, `i03-settings-not-signed-in.jpg` (while signed out)

## New findings (not in a piece's Check line)
- NEW-1 (piece 10/11) · "Ask the agent to write it…" on a new stage "Update the docs 2" (skill update-docs) wrote the
  skill but put it on the other stage, "Update the docs" (Noyan ran `otaq flow write-skill update-the-docs --done`;
  draft "2 changes · changed Update the docs, added Update the docs 2"): the old stage's skill retest-docs was replaced
  by update-docs, while "Update the docs 2" kept "Being written by Noyan… Stop" and "You don't have the skill
  update-docs" after Noyan was Idle. Two stages with near names; the request names the stage by name only.
  `new-skill-written-to-other-stage.jpg`, `p11-written-not-committed.jpg`
- NEW-2 (issue 03) · Sign in… on a ticket's sheet (Claude Code stopped · login expired) starts `claude auth login` but
  the terminal panel that opens shows "Nothing open", so its output (the URL to open if the browser doesn't, the paste
  prompt) can't be seen. `i03-sheet-sign-in-panel-empty.jpg`
- NEW-3 (piece 5) · Text I had left unsent in SHOP-19's Dev Codex input line was gone when, after the failed merge
  check, the restack request ("$otaq:restack …") was typed into that session; the log shows the conflict handed to the
  agent at once. The previous re-test saw the restack wait behind unsent text. `p05-writer-request-queued.jpg`,
  `p05-only-restack-delivered.jpg`
- NEW-4 (decision 013) · In a later turn Noyan made SHOP-22 (which it had created in an earlier turn) wait for SHOP-21:
  the CREATED row changed to "SHOP-22 · waits for SHOP-21" (the title "Retest2 dep A" went) and no CHANGED row or Undo
  appeared. Decision 013 folds into CREATED only a change made in the same turn. `new-created-row-later-turn-loses-title.jpg`
- NOTE · The Park dialog for SHOP-25 parked from Update the docs said "conversation · resumes on Dev"; dragged back it
  resumed in Update the docs. `new-park-says-resumes-on-dev.jpg`
- NOTE · A card can read "Working" while the sheet's pill says "Needs you" for the same Codex prompt (SHOP-25 in Update
  the docs, about 20 s). `new-card-working-pill-needs-you.jpg`
- NOTE · A ticket created with every spec stage skipped and Claude Code signed out was held in the Product spec column
  ("Can't start · Spec writer's model can't run: Claude Code isn't…"), the stage it skips. (Claude Code had been
  signed back in outside the app a minute before; the app hadn't re-checked yet.) `new-held-in-skipped-stage.jpg`
- NOTE · After SHOP-19 was resumed from a restart, Otaq's reminder "Otaq: you ended your turn but your stage is still
  open…" was sent once, and a second copy was typed into Codex's input line while Codex was already "Working (3s)"
  (left there, "tab to queue message"). Seen once; `new-reminder-typed-while-working.jpg`.

## Regression walk
- PASS · First run (fresh OTAQ_HOME, the release test's gitconfig): Welcome "Coding agents that follow your process" →
  Get started → Tools "4 ready" (Claude Code v2.1.322 signed in, Codex, GitHub Copilot, OpenCode; git required) →
  Continue → Add a project: Choose folder… (throwaway clone), "Claude Code, Codex and Copilot will run their hooks and
  settings in this folder.", Name freshproj, main, Ticket key FRES → Add project → board Backlog · Dev · Validate ·
  Review, "Done 0 →", Edit flow, "Your first ticket · Tidy the README … uses your Claude Code plan · Create & start /
  Not now". Folder and trust lines removed after. `reg-first-run-tools.jpg`, `reg-first-run-board.jpg`
- PASS · A ticket through Review to Approve & merge: SHOP-19 (Codex: Update the docs, Dev, Validate, Review; merge
  check failed, fixed by its agent, merged ea36c00) and SHOP-25 (merged f7ae8de with the plain message after the
  writer was asked again). Worktrees gone after merge. `reg-approve-merge-dialog.jpg`, `reg-merged-SHOP-19.jpg`
- PASS · Edit flow: added/removed stages, picked skills, Ask the agent to write it…, Cancel (discards and removes the
  written skill), Save flow ("Flow saved · Applied now: no ticket was in the stages it changes"), waiting draft with
  held tickets. shop ends on Dev → Validate → Review.
- PASS · Settings: Agents (four Connected, Re-check), Git & pull requests (git found, gh and az not installed),
  Limits & usage, Notifications, Appearance, Keyboard, Data, About "Version 0.1.0 · build ac5e6200"; project Settings
  Repository merge check edited and put back. (Git & pull requests drew an empty pane for about a second the first time.)

## State left in the VM
The installed app (ac5e6200) is running on screen with its service, release-test data folder; no agent running; no
ticket in progress (shop: everything Done; notes: everything Done). shop main f7ae8de, clean; merge check `node test.js`;
flows: shop Otaq default (Dev → Validate → Review), notes Spec first unchanged. Sign-ins: Claude Code signed in
(new login via the app), Codex and Copilot unchanged and Connected.
