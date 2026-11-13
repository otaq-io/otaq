# Re-test · release-fixes on Windows · results (Windows 11 ARM64 VM, build release-fixes 7f36bd79)

Date: 2026-11-10. Screenshots: `shots-retest/` next to this file (JPEG; the GitHub login shown on Copilot rows and one
Windows path are blacked out).

## Setup
- Build: branch release-fixes at 7f36bd79 ("Merge piece 14…"), brought in as a git bundle, cloned to
  `%USERPROFILE%\otaq-build\retest\repo`, built with the shared `CARGO_TARGET_DIR` (`%USERPROFILE%\otaq-build\target`):
  `npm.cmd ci` + `npm.cmd run bundle -- --bundles nsis --config noupd.json` → BUILD-EXIT=0,
  `Otaq_0.1.0_arm64-setup.exe` (19.8 MB). The running release-test app (ef84b175) and its service were stopped
  (taskkill; this interrupted SHOP-10, a Codex ticket left at a permission prompt), the setup was installed with `/S`
  (exit 0; `%LOCALAPPDATA%\Otaq\otaq-app.exe` 17:52, `otaq.exe` 17:50) and started with the release test's
  `launch.cmd` (OTAQ_HOME=`%USERPROFILE%\otaq-reltest\home`, GIT_CONFIG_GLOBAL=its gitconfig); the new service started
  from the new runtime (`runtime\7f36bd793958…`). About shows "Version 0.1.0 · build 7f36bd79".
- Data: the release test's shop and notes projects. Claude Code is logged out in the VM (`claude auth status` →
  `"loggedIn": false`) and was left logged out; all agent work ran on Codex (every command asks permission: answered in
  Session). Copilot was used once (piece 4).
- Setup changes made for single checks and put back: a personal Codex skill `retest-docs` (piece 7, removed), shop's
  merge check set to a failing script with a flag file and two setup commits on shop's main (piece 20; check back to
  `node test.js`, flag removed, main reset to 3ff235e = SHOP-11), a "Retestcop" agent on Copilot (piece 4, deleted),
  a launcher without Codex or Copilot on PATH (piece 4, deleted), a throwaway project and fresh data folder for the first
  run (deleted, with the Codex trust line the app added for it).

## Pieces · each Check line
- PASS · 1 · Edit flow on shop, dragged Validate in front of Dev: nothing moved and a toast said "The flow didn't change
  · Validate can't go before Dev: a check sends its fails back to a stage where an agent works, so one has to come before
  it". Clicking Validate right after opened its panel; Cancel, then Edit flow again drew the whole editor (also every
  later Edit flow in this re-test). `p01-refused-reorder.jpg`
- PASS · 2 · Tools step (fresh data folder): "Claude Code · found · not signed in · Sign in…" with an amber mark, the
  other three signed in, "3 ready"; Check again kept it. Settings → Agents: "Claude Code · Found · not signed in ·
  v2.1.322 · not signed in · Sign in…"; Re-check → "Checked just now", still not signed in. (Sign in… not clicked: no
  login in the VM.) `p02-tools-step.jpg`, `p02-settings-agents.jpg`
- NOT TESTED · 3 · needs Claude Code signed in: a session that ends on a tool error needs a tool that starts and then
  fails; with the new sign-in check a signed-out Claude Code never starts. What was seen instead: NOTE-5 (Product spec
  on Claude Code) Create & start → app banner "Claude Code isn't available · signed out · 1 ticket waiting", card red
  "Can't start · Spec writer's model can't run: Claude Code isn't…", pill "Held", Overview "Claude Code isn't available
  · signed out · Nothing started and nothing is locked. Spec writer starts once Claude Code is available again." with
  Open agent… / Sign in… / Change agent…. `p03-held-signed-out.jpg`
- FAIL · 4 · The rows, the unticked row with a reason and the counts work, but the reason is wrong: twice, right after
  the service started, an installed and signed-in tool was called "not installed", so a resumable agent could not be
  resumed from the dialog.
  - First start of the new build: "1 agent was stopped by a restart · SHOP-10 Say pocket-sized in the README intro ·
    Codex isn't available (not installed)", unticked, "Resume 0". Settings → Agents a few seconds later said Codex
    "Connected · v0.166.0 · signed in with ChatGPT", and SHOP-10's own Resume worked. `p04-first-start-codex-not-installed.jpg`
  - Controlled: SHOP-18 (Dev on a Copilot agent, working), app and service stopped and reopened → "1 agent was stopped
    by a restart · SHOP-18 Retest cop two · Copilot isn't available (not installed)", unticked, "Resume 0", unchanged
    after 20 s; Settings → Agents in the same session: "GitHub Copilot · Connected · v1.0.108". (That launch had
    Codex, not Copilot, removed from PATH; Copilot was on PATH.) `p04-copilot-not-installed.jpg`,
    `p04-settings-copilot-connected.jpg`
  - Works: SHOP-10 working on Codex, restart → "1 agent was stopped by a restart · SHOP-10 … was working in Dev",
    ticked, "Resume 1"; Resume 1 → row "Resumed". `p04-resumable-row.jpg`. A restart with NOTE-8 (checking) and
    SHOP-17 (working) on Codex plus two Noyan conversations: "4 agents were stopped by a restart", 4 ticked rows, "Resume 4".
  - Not testable here: the drawn case "Claude Code isn't signed in · sign in first" needs an agent that ran on Claude
    Code before the restart (needs Claude Code signed in).
- PASS · 5 · SHOP-11 (Dev on Checker, so the message writer is a separate Developer session): in Review the title bar
  said "1 running" and Running listed "SHOP-11 … Developer", the strip only "Your review · 1" (calm). Approve & merge:
  plain message prefilled, "☐ Developer wants to run a command to write the message. Merge now with this one, edit it,
  or wait: it fills in once you answer, unless you've typed. [Answer in terminal]"; Answer in terminal opened
  "Developer · SHOP-11's message" at Codex's prompt. Merged with the plain message (3ff235e): the writer's Codex process
  ended, Running listed only the next ticket, `home\wt\shop\SHOP-11` and its git worktree are gone.
  `p05-approve-writer-asks.jpg`, `p05-after-merge-running.jpg`. Same for SHOP-10 (merged 6f1c346, worktree gone,
  "Session ended · The agent's session ended (stopped)"). See NEW-2 and NEW-3 for two stale-writer leftovers.
- PASS · 6 · SHOP-12's Codex agent idle at its own prompt (only the placeholder "Ask Codex to do anything"): a comment
  with Tell Developer now ("You 6:13 pm · told Developer now") reached it by itself within 15 s ("The user added comment
  1. Run 'otaq prime' to read it."), no "unsent text", no Deliver now. While an agent sat at a permission prompt the
  footer read "Otaq's notes only arrive when the agent is idle and your input line is empty" with "1 message waiting ·
  Deliver now", and the note arrived once the prompt was answered. `p06-told-comment.jpg`
- FAIL · 7 · Without a byte order mark: `%USERPROFILE%\.codex\skills\retest-docs\SKILL.md` (first bytes `---`), Pick a
  skill… listed "retest-docs · Personal · Codex · Re-test skill…", the stage had no warning; Update the docs (moved
  before Dev, Developer on Codex) with that skill, saved; SHOP-13 Ctrl+↵ started there and Codex ran `$retest-docs`
  ("I'm applying the retest-docs skill…"). PASS for that part. `p07-nobom-stage.jpg`, `p07-nobom-session.jpg`.
  With a byte order mark (EF BB BF): Pick a skill… said "retest-docs · Personal · Codex doesn't load it: its SKILL.md
  starts with a byte order mark · …" (as asked), and SHOP-14 was held: "Codex doesn't have the skill `/retest-docs` ·
  Nothing started. Link the skill into Codex's skills folder (a SKILL.md saved without a byte order mark: Windows
  PowerShell 5 writes one) or commit it to the project, then Check again; or give the stage an agent whose tool has it."
  But the stage itself showed no warning at all (panel Skill "retest-docs", card "retest-docs", also after Cancel and
  reopening Edit flow), where the piece says the stage warns "Codex doesn't load it…". `p07-bom-picker.jpg`,
  `p07-bom-stage-no-warning.jpg`, `p07-bom-held.jpg`. The test skill was removed afterwards.
- PASS · 8 · SHOP-14 held in Update the docs; Edit flow → × on the stage: "Update the docs removed. This waits until no
  ticket is between Update the docs and Dev: the ticket there now (1 in update-the-docs) finishes on the current order…
  The banner offers Apply now." Save → "Saved · waiting · Update the docs removed · 1 ticket still working…" with Apply
  now… / Cancel change, headers "changing". Apply now… → "MOVES · SHOP-14 Retest bom · waiting for Update the docs
  (removed) · held → Dev ▾" (Backlog / Start again in Dev / Dev / Validate / Review). Apply now → SHOP-14 "Working" in
  Dev, no hold left. `p08-apply-now.jpg`
- PASS · 9 · notes (Validate rounds 1), NOTE-7, the checker failed it on request: card "Validate failed 1 time ·
  decide" + Decide, strip "NOTE-7 decide"; Decide: "VALIDATE FAILED 1 TIME IN A ROW · Allow 1 more round · Send it on
  to Code review (You'll see Checker's last report there.) · Stop here · Rounds for Validate are set in Edit flow (1)".
  Send it on → Code review "Ready for you · Validate failed · moved on by you"; sheet "Your check", CHECKER "Failed 1
  time · moved by you · Full report". `p09-decide.jpg`, `p09-sent-on.jpg`
- PASS · 10 · Pick a skill… ends with "Ask the agent to write it… · Starts Noyan on the right and uses tokens." (also
  while searching); a missing skill warns "You don't have the skill update-docs · Pick one, or Ask the agent to write
  it." with both as links. The link opened "Write a skill for the Update the docs stage: …" Cancel / Send; Send opened
  Noyan (Codex) and the Skill field read "Being written by Noyan… Stop", Save flow disabled. Noyan ran
  `otaq flow write-skill` and the field became "update-docs · Written by the agent · Open · Undo · Saved in
  .agents/skills/update-docs in this project. Commit it so agents get it in their worktrees." `p10-picker-ask.jpg`,
  `p10-being-written.jpg`, `p10-written-still-warns.jpg`. NOTE: the stage card kept "You don't have the skill
  update-docs" after it was written (the file is uncommitted, so maybe by design); after Cancel the draft went but the
  written `.agents\skills\update-docs\SKILL.md` stayed in shop's folder, untracked (deleted by hand).
- PASS · 11 · Start from… → Suggest a flow for shop… ran Noyan (Codex), the board showed "Shop flow suggestion's
  suggestion for shop · 3 changes, marked · nothing changes until you save" with Why these changes. Discard: the line
  "Shop flow suggestion is drafting a flow for shop… Show" showed for about 3 s while Noyan answered "Proposal 19 is
  cancelled…", then went for good once Noyan was Idle. `p11-after-discard.jpg`
- PASS · 12 · The Tools step shows no permission warning on any row (fresh data folder). Settings → Agents keeps
  "Your Codex settings ask for permission prompts…" and the same for GitHub Copilot. `p02-tools-step.jpg`,
  `p12-settings-agents-warning.jpg`
- PASS · 13 · Every toast from the installed app is titled "Otaq" with Otaq's icon ("SHOP-18 session interrupted ·
  Resume from Otaq. · shop · Retest cop two"; earlier SHOP-10's). `p13-toast.jpg`
- PASS · 14 · With SHOP-13 working ("1 agent running · shop 1" in the tray), tray → Quit Otaq with the window open:
  "Quit Otaq? · 1 agent is working. Quitting keeps it running in the background; Quit and stop agents stops it where it
  is (Resume later). [Keep them running] [Quit and stop agents] [Cancel]". Keep them running: the window closed, the
  service and SHOP-13's Codex kept running. Reopened, closed the window to the tray, tray → Quit Otaq: the window came
  back with the same question (Cancel kept it). At the end, with SHOP-17 working: Quit and stop agents → no otaq,
  otaq-app or codex process left; on reopening, banner "All projects are paused · Quit and stop agents stopped 1 agent in
  1 project today at 19:06. Resume continues where each one was…" `p14-tray-quit-question.jpg`,
  `p14-tray-quit-closed-window.jpg`, `p14-after-quit-and-stop.jpg`. See NEW-4.
- PASS · 15 · SHOP-11 created in Backlog with Dev → Checker: card "Not started · 👤 Dev: Checker instead of Developer ·
  Start", Overview "Dev: Checker instead of Developer". `p15-backlog-own-agent.jpg`
- PASS · 16 · SHOP-12 waiting for SHOP-11 at the top of Dev, dragged to Validate: refused, toast "SHOP-12 can't go to
  Validate yet · It waits for SHOP-11 to merge, then starts by itself." `p16-refused-drag.jpg`
- PASS · 17 · Ctrl+K: SUGGESTED "Pause shop · Ctrl+Shift+."; typing "pause": "Pause shop Ctrl+Shift+." and "Pause all
  projects". `p17-palette.jpg`
- PASS · 18 · Overview at a permission prompt: "Developer wants to run a command · `$ git show --format= --check HEAD;
  …` · Answer in terminal"; a checker's: "Checker wants to run a command · `$ @' Pass: … '@ | otaq check pass --stdin`
  · Answer in terminal · Checker", pill "Needs you · checker". `p18-overview-command.jpg`
- PASS · 19 · SHOP-10 reached Review a second time (after the restart and an answered question): WHAT THE AGENT DID
  showed the second handoff ("Kept the README intro change to "pocket-sized"; the user clarified that no other docs need
  alignment. … at 54034e9: passed"). `p19-latest-handoff.jpg`
- PASS · 20 · Failed merge check (shop's check set to a script failing on a flag file, main moved): dialog "Merge check
  failed on main · Merge check `..\..\..\mc.cmd` failed on the result rebased onto main (exit 1). Fix it, then submit
  again. … Nothing was merged into main. SHOP-14 went back to its agent…", badge "Merge check failed · fixing",
  Overview strip "Merge check failed on main · Nothing was merged into main. SHOP-14 returns to Review when it's fixed.
  · 1 of 3", log attached as SHOP-14.log. Conflict (a setup commit on main changed README line 1): "Rebase onto main
  stopped — conflict · … README.md · SHOP-14 went back to its agent…", badge "Conflicts with main · fixing", Overview
  "Conflicts with main · 1 file (README.md) · b99152b changed README.md · Nothing was merged into main. SHOP-14 returns
  to Review when it's fixed. · 2 of 3". The words match the plan; what moved names the commit (b99152b) because a setup
  commit, not a ticket, moved main. `p20-merge-check-dialog.jpg`, `p20-merge-check-overview.jpg`,
  `p20-conflict-overview.jpg`. NOTE: the merge-check dialog shows an empty pink box where the check's output would go
  (the output is only in the attached log).
- PASS · 21 · NOTE-6: the Developer (Codex) ran `otaq ask human --send-back "Product spec"`; Overview's question "The
  wording of the line is a product question. · 1 Send back to Product spec · 2 Answer here…"; Send answer → NOTE-6 in
  Product spec. Comments: "CX Developer · Dev · 6:53 pm · Dev asked to send NOTE-6 back to Product spec: The wording of
  the line is a product question." (the agent's, not "You"); History "Sent back to Product spec · Dev asked: … · you sent
  it back · no longer skipped". `p21-agent-reason.jpg`
- PASS · 22 · Noyan asked to make two tickets and then "make Retest dep B wait for Retest dep A": it ran `otaq create`
  twice and `otaq dep add SHOP-16 SHOP-15`; the panel showed "CREATED 2 · IN BACKLOG · SHOP-15 / SHOP-16" and "CHANGED 1
  · ✎ SHOP-16 Retest dep B · Backlog". `p22-changed-row.jpg`. NOTE: the row doesn't say what changed (canvas noyan-01 ·
  4: "SHOP-7 · now waits for SHOP-6 · Undo"). Both tickets deleted afterwards.

## The release test's failures (FAILURES.md numbers)
FAILURES.md numbers 1–24 (24 items; the brief speaks of 23).
- PASS · 1 · Edit flow never went blank, refused reorder says why (piece 1).
- PASS · 2 · Claude Code shows "not signed in" with Sign in… on Tools and in Settings → Agents; nothing starts on it (pieces 2, 3).
- PASS · 3 · The writer's permission ask shows in the Approve dialog with Answer in terminal; its session ends at the
  merge and leaves no worktree (piece 5). (The writer's message itself wasn't waited for: its prompts were left unanswered on purpose.)
- PASS · 4 · Tell <agent> now reaches an idle Codex agent by itself (piece 6).
- PASS · 5 · A personal Codex skill without a BOM is Codex's and its stage starts (piece 7).
- PASS · 6 · Removing a stage with a held ticket waits, Apply now asks, the moved ticket has no old hold (piece 8).
- PASS · 7 · Send it on works on a non-skippable check (piece 9).
- NOT TESTED · 8 · needs Claude Code signed in (piece 3).
- PASS · 9 · No permission warnings on the Tools step (piece 12).
- PASS · 10 · Toasts are titled "Otaq" with its icon (piece 13).
- PASS · 11 · Own-agent line shows in Backlog and on Overview (piece 15).
- PASS · 12 · Refused drag names what it waits for (piece 16).
- PASS · 13 · Palette "Pause shop Ctrl+Shift+." (piece 17).
- PASS · 14 · Noyan's dependency change is a CHANGED row (piece 22; the row doesn't say what changed).
- PASS · 15 · Overview shows the command above Answer in terminal (piece 18).
- PASS · 16 · The send-back reason is the agent's comment (piece 21).
- PASS · 17 · "Validate failed 1 time" (piece 9).
- PASS · 18 · Badge "Merge check failed · fixing" (piece 20).
- PASS · 19 · What the agent did shows the latest round (piece 19).
- PASS · 20 · Conflict banner says what moved, nothing merged, "2 of 3" (piece 20).
- PASS · 21 · "Ask the agent to write it…" in Pick a skill… and the missing-skill warning (piece 10).
- PASS · 22 · The "drafting a flow" line goes after Discard (piece 11).
- PASS · 23 · Tray Quit Otaq asks while an agent works (piece 14).
- FAIL · 24 · The restart dialog now has unticked rows with a reason and honest counts, but it called installed,
  signed-in tools (Codex, Copilot) "not installed" right after a restart, so "Resume 0" again for an agent that could
  resume (piece 4). The signed-out Claude Code row itself needs Claude Code signed in to set up.

## New findings (not in a piece's Check line)
- NEW-1 (piece 4) · Restart dialog "<tool> isn't available (not installed)" for an installed tool, seen twice (see piece 4).
- NEW-2 (piece 5) · When the delivery's agent is the ticket's own Dev session, its writer request outlives a send-back:
  SHOP-14's merge check failed and it went back to Dev, and then the Dev Codex session got "SHOP-14 is waiting in Review.
  You are its delivery's agent: write the message…" typed in, followed by the restack request. (It had been queued
  behind text I had left in Codex's input line.) `p05-stale-writer-prompt.jpg`
- NEW-3 (piece 5) · SHOP-10's Approve dialog said "Developer is still writing the message…" and never filled in: the
  service log shows the writer was asked once at 12:08 under the old build (whose session never finished) and not again
  because the branch head didn't change; no writer session existed. Leftover from the old build's data, but the dialog
  says someone is writing when no one is.
- NEW-4 (piece 14) · After Keep them running and reopening, the title bar showed Pause but no "1 running" chip while the
  tray said "1 agent running · shop 1" and SHOP-13's card said "Working · in the background". `p14-relaunch-no-running-chip.jpg`
- NEW-5 · SHOP-13 (Update the docs with retest-docs) stayed "Working · in the background" / Session "Waiting on its
  background work" while Codex sat idle at its prompt after answering (the skill told it to finish but it didn't call
  `otaq stage next`). `p07-nobom-session.jpg`
- NEW-6 (piece 20) · The merge-check failure dialog has an empty output box.
- NEW-7 (piece 10) · Cancel in Edit flow leaves the skill file "Ask the agent to write it" wrote in the project folder.
- NOTE · Edit flow's draft line still uses a stage id: "(1 in update-the-docs)" (as RESULTS-D saw for validate).
- NOTE · Settings → Agents → New agent → Model lists Codex, GitHub Copilot, OpenCode but not Claude Code while it is
  signed out (Default model still reads "Claude Code · tool default").

## Regression walk
- PASS · First run (fresh OTAQ_HOME `%USERPROFILE%\otaq-build\retest\fresh-home`, gitconfig with an identity): Welcome
  "Coding agents that follow your process" → Get started → Tools (as piece 2/12) → Continue → Project: Choose folder…
  (throwaway clone), trust line "Codex and Copilot will run their hooks and settings in this folder.", Name freshproj,
  Base branch main, Ticket key FRES → Add project → board Backlog · Dev · Validate · Review, "Done 0 →", Edit flow, and
  the "Your first ticket" card "Tidy the README … Takes about 5 minutes and uses your Codex plan." Create & start /
  Not now (Not now used; the first ticket's run was covered on the release-test data).
- PASS · A ticket through Review to Approve & merge on Codex: SHOP-10 (Dev, question answered on Overview, Validate,
  Review, merged 6f1c346) and SHOP-11 (Dev on Checker, Validate, Review, merged 3ff235e); SHOP-12 started by itself when
  SHOP-11 merged and finished "Done · nothing to merge".
- PASS · Edit flow: opened (shop and notes), changed (added/moved/removed Update the docs, picked skills), Cancel
  (discards), Save flow ("Flow saved · Applied now…"), waiting save with Apply now and Cancel change; shop ends on Otaq
  default (Dev → Validate → Review).
- PASS · Settings: the gear's eight topics all open (Agents, Git & pull requests, Limits & usage, Notifications,
  Appearance, Keyboard, Data, About "Version 0.1.0 · build 7f36bd79"); project Settings (Flow, Repository with the merge
  check edited and put back, Run, Workspace, Notes for agents) open and save.
- Not tested: needs Claude Code signed in · piece 3 / failure 8 (a session ending on a tool error, card "Stopped ·
  login expired", Sign in… then Resume); piece 4's drawn "Claude Code isn't signed in · sign in first" row; Sign in…
  opening `claude auth login`; Spec first's Product spec / Tech spec / Milestone plan stages (their agents run on Claude Code).

## Summary
Pieces (Check lines): PASS 19 · FAIL 2 · NOT TESTED 1 (of 22).
Release-test failures 1–24: PASS 22 · FAIL 1 · NOT TESTED 1.

FAIL
- Piece 4 / failure 24 · Restart dialog calls installed, signed-in tools "not installed" right after a restart
  ("Codex isn't available (not installed)" at the new build's first start; "Copilot isn't available (not installed)"
  after a controlled restart while Settings said Connected), so a resumable agent is unticked and the button reads
  "Resume 0".
- Piece 7 · With a BOM, Pick a skill… and the held ticket say why, but the stage itself shows no warning.

Not tested (needs Claude Code signed in): piece 3 / failure 8; piece 4's signed-out Claude Code row.

State left in the VM: the release-test app (this build, 7f36bd79) running on screen with its service;
no agent running; no ticket in progress (SHOP-10/11 merged, SHOP-12 Done · nothing to merge, SHOP-13/14/17/18 and
NOTE-5…8 closed, SHOP-15/16 deleted). shop main 3ff235e, clean; merge check `node test.js`; flows unchanged (shop Otaq
default, notes Spec first with Validate rounds 1). New Noyan conversations "Update docs skill" and "Shop flow
suggestion" exist (Idle).
