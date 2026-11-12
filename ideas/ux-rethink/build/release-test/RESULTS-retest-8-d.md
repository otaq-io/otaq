# Re-test 8 · part d · phase D (Flow, settings, upkeep) on Windows · results (Windows 11 ARM64 VM, build 4147a741)

Date: 2026-11-12, 16:28–16:49 VM time. Build **4147a741** (installed; code = release-fixes 826d4f7f; About "Version
0.1.0 · build 4147a741 · Built from source"). Data: the normal release-test data (`otaq-reltest\home`, `launch.cmd` via
`otaqtest`), project **shop** for the flow work. Plan: `PLAN.md` phase D, checked against DECISIONS.md (UX-9, 10, 16,
25, 26, 28, 30, 34, 41, 48, 50–53, 57, 59–63; A1, A34, A36, A49–A53), `RESULTS-D.md`, re-tests 1–7 and 8a/b/c/fixes,
plus the new Edit flow box (UX-63; commits 2b343679, 6b1a9074, 9cb78867). Screenshots: `shots-retest-8/d-*.jpg`. No
screenshot shows the Windows user name, a login, email or token (shots that showed the Copilot login or a full path were
not saved). No code changed, nothing committed or pushed, nothing signed in or out, no paste used (all typing by key
presses). Real agent runs: Claude Code only (Noyan on the Claude Code agent Architect for four short flow drafts, and
one tiny ticket SHOP-45 for quit/restart); Codex not used.

## Summary
Phase D: **PASS 34 · FAIL 1 · PARTLY 2 · KNOWN 3 · NOT TESTED 5 · NOTE 9 · new problems 5 (none a blocker).**
Everything RESULTS-D failed that is in this phase holds now: Edit flow never went blank, "Ask the agent to write it…"
is in Pick a skill…, no stale "drafting" line after Discard, the tray's Quit asks while an agent works, the restart
dialog lists a resumable Claude Code row. The new UX-63 box works end to end (send without opening the panel, drafting
row with Show/Stop, answer box, ⤢/⤡/Esc, same-conversation changes), but a long question with noted options leaves
almost no room for the question itself (box and panel), Stop loses a proposal that was already shown, and the box
names the conversation rather than the agent.

| Check | Result |
|---|---|
| App settings: eight topics, foot line (UX-10, UX-57) | **PASS** |
| Agents: tools rows, Re-check, NOT SHOWN HERE, Default model, Noyan uses, agents table with Used in (UX-9, A31) | **PASS**; amber "Your Codex/Copilot settings ask for permission prompts" on the rows **NOTE** |
| Git & pull requests: git row, GitHub/Azure DevOps "isn't installed · winget … · Install" (UX-57) | **PASS** |
| Limits & usage: off by default, 5 / 3 greyed, usage "% left" per tool, Codex 20 % amber (UX-34, UX-50) | **PASS** |
| Notifications: master + three kinds, review off by default (toggled on/off), sound (UX-28) | **PASS**; "Not while you're looking" fixed "Always" **KNOWN (gap #47)** |
| Appearance: Dark at once; Match system follows Windows dark and back (UX-16, A34) | **PASS** |
| Keyboard: list, recorder, clash line "Ctrl+Shift+F is used by Find in terminal scrollback…", Cancel keeps / (A34) | **PASS** |
| Data: folders, usage data off, Cleanup "2 leftovers · 1 KB"; dialog; Delete 1 item → toast "Cleaned up 1 item" (A50) | **PASS** (RESULTS-D size NOTE fixed) |
| About: version + build, Updates "Built from source…", PATH, licences (A34) | **PASS** |
| Project Settings: section list, Flow card (strip, Export, Edit flow, ⋯ Use a preset… / Copy / Import / Reset), changed line + Go back (UX-8, A35) | **PASS**; changed line doesn't say what changed **NOTE** (as RESULTS-D) |
| Repository: delivery = flow setting, PRs "None detected · no origin", merge check (UX-25, UX-57) | **PASS** |
| Run: Run command + Ask the agent + Open URL; "More apps · Add app" once a command is set (UX-10 / FAILURES #25) | **PASS** (command cleared again) |
| Workspace, Notes for agents, Remove project dialog (UX-52) | **PASS**; dialog still "commands, env files" **NOTE**; actual removal not re-run |
| Files: committed only (an uncommitted README line in the folder didn't show), picker, Ctrl+N full form (UX-30, UX-48, A36) | **PASS** |
| Header hover shows the stage's description (UX-53) | **PASS** |
| Edit flow: bar, locked ends, + palette with missing-skill lines, New stage panel ("What it's for" only), rounds 1–5, Watch a ticket, Done "How work leaves the flow", ⋯ Export/Import/Copy/Go back, Undo (A1, UX-12, 25, 41, 59–61) | **PASS**; Cancel with changes discards without asking **NOTE** (as RESULTS-D) |
| Pick a skill…: Otaq's skills with descriptions, ends with "Ask the agent to write it… · Starts Noyan on the right and uses tokens"; warning "Pick one, or Ask the agent to write it." (UX-26) | **PASS** (RESULTS-D FAIL stays fixed) |
| Save flow → "Flow saved · Applied now…"; Go back to the previous flow → saved; `otaq flow show` identical to before (UX-60) | **PASS** |
| Every stage its own column (UX-51) | **PASS** |
| UX-63 box docked under the columns: "Ask Noyan to change this flow…", Suggest a whole flow, line names who answers ("Noyan · Architect · Claude Code" after switching Noyan's agent) and "Proposes a draft · nothing changes until you save" | **PASS** |
| UX-63 ↵ / Ask Noyan and Suggest a whole flow send without opening the agent panel | **PASS** |
| UX-63 Start from… holds only presets (+ Same as …) and Import; no Suggest there | **PASS** |
| UX-63 the box never covers the last cards or Done's line; it narrows beside a stage panel; it stays while Noyan's panel is open | **PASS** |
| UX-63 drafting row "<…> is drafting a change to shop's flow…" + Show (opens the panel on that conversation) + Stop, Suggest off, "Drag, + and × still work meanwhile" | **PASS**, but names the conversation title, not the agent **→ NEW-d4** |
| UX-63 Stop keeps what it proposed | **FAIL → NEW-d3** |
| UX-63 proposal: "Ask for changes to <…>'s proposal…", ↵ to the same conversation (Proposal 39→40, 41→42), bar keeps Watch a ticket / Discard / Save flow | **PASS** (conversation title again, NEW-d4) |
| UX-63 / UX-62 Noyan's question in the box: attention colour, "Needs you · <…> asks", options, Send answer → agent got "Answer to your question: Reuse Checker (option 3)."; Answer in terminal opens the panel | **PASS** |
| UX-63 long question: grows to 55 % of the editor, options + Send in view | **PARTLY → NEW-d1** (the question text gets one line) |
| UX-63 ⤢ opens to the editor's full height; ⤡, Esc and sending shrink it | **PASS** |
| UX-62 the same question above Noyan's terminal | **PARTLY → NEW-d2** |
| Tray menu: NEEDS YOU, "1 agent running · shop 1", YOUR CHECKS AND STOPPED TICKETS, Open Otaq Ctrl+O, Pause all, Quit Otaq Ctrl+Q (A51) | **PASS** |
| Quit Otaq (tray) while SHOP-45 works → "Quit Otaq? · 1 agent is working…" Keep them running / Quit and stop agents / Cancel (A52) | **PASS** (RESULTS-D FAIL fixed; Ctrl+Q covered by the fix check) |
| Quit and stop agents → reopen: "All projects are paused · Quit and stop agents stopped 1 agent…" + Resume; card "Paused · resumes where it was"; Resume → Working | **PASS** |
| Restart (service + app killed) → "4 agents were stopped by a restart": SHOP-45 "was working in Dev" + 3 Noyan conversations, all ticked, Resume 4; unticking → Resume 1; resumed (A49) | **PASS** (count = rows, gap #34 holds); toast titled "Otaq" |
| Restart row for a signed-out tool ("sign in first"), Retry / Start from scratch | **NOT TESTED** (all tools signed in) |
| Update banner / Restart to update (A53) | **NOT TESTED** (source build never checks) |
| CLI: `otaq list` plain words (Not started / Ready for you / Start failed / Merged / Closed · not merged), `otaq show`, `otaq prime --project` (## The flow with descriptions, ## Stage choices) | **PASS**; "Done · nothing to merge" tickets read "Merged" **KNOWN (NEW-b5 / RESULTS-D note)**; prime lists a deleted worktree **→ NEW-d5** |
| UX-61 first-time "This is your flow, as columns" hint | **NOT TESTED** (one-time, seen before) |
| Import / Export file dialogs, Copy from another project, Apply now… with a ticket in a changed stage | **NOT TESTED** this time (PASS in RESULTS-D and re-test piece 8; needs Codex runs) |
| Done order newest first | **KNOWN (gap #49)** |

## Checks (details)

### App settings · PASS
- About "Version 0.1.0 · build 4147a741". `d-settings-about-4147a741.jpg`
- Agents: Claude Code v2.1.322 signed in, Codex, Copilot, OpenCode rows with "Otaq can see" chips and "Otaq added…";
  Re-check "Checked just now"; NOT SHOWN HERE; Default model "Claude Code · tool default"; **Noyan uses** (Noyan,
  Developer, Checker, Spec writer, Architect, Milestone planner); table NAME / MODEL / USED IN.
  `d-settings-agents-noyan-uses-table.jpg`
- NOTE · Codex's and Copilot's rows carry an amber "Your Codex settings ask for permission prompts · With 5–10 agents,
  prompts will stop agents often. Otaq won't change your settings." UX-36 moved that warning off the Tools step into a
  one-time hint; it still sits in Settings → Agents (not seen in RESULTS-D's record). Not saved as a screenshot (the
  Copilot row shows the login).
- Git & pull requests `d-settings-git-and-prs.jpg`; Limits & usage `d-settings-limits-usage.jpg`; Keyboard
  `d-settings-keyboard.jpg`.
- Appearance: Dark applied at once; back to Match system; Windows AppsUseLightTheme 0 + WM_SETTINGCHANGE turned Otaq
  dark within seconds, 1 turned it back (registry put back to 1).
- Data: "Cleanup · 2 leftovers · 1 KB · copies and branches no ticket uses." `d-settings-data-cleanup-total.jpg`.
  Review… → "Cleanup · 2 items": Worktrees (`~\otaq-reltest\home\wt\shop\SHOP-5`, "no ticket uses this worktree", 0 KB,
  2 d) and "Done ticket with uncommitted changes" (SHOP-33, "1 uncommitted file: README.md"), nothing selected.
  `d-cleanup-dialog-two-groups.jpg`. Ticked SHOP-5 → Delete selected… → "Delete 1 item? This can't be undone…" →
  toast "Cleaned up 1 item · 1 item left in Cleanup". `d-cleanup-deleted-one-toast.jpg`. The branch otaq/SHOP-5 stays.

### Project settings and Files · PASS
- `d-project-settings-flow-repository.jpg`, `d-project-settings-use-a-preset.jpg`,
  `d-project-settings-run-more-apps-add-app.jpg`, `d-project-settings-remove-dialog-env-files.jpg` (Cancel).
- Files: `d-files-view-main.jpg`; "Your folder" README.md as committed (line `R8D-UNCOMMITTED-LINE` appended in the VM
  didn't show; reverted, shop clean at e97df39); Ctrl+N → New ticket with `README.md` in the description.
  `d-files-ctrl-n-new-ticket-from-file.jpg`. The source card reads "main · your folder", no "Showing" word (gap #44's
  default "Showing main" isn't built; RESULTS-D passed it this way) NOTE.

### Edit flow by hand · PASS
- `d-board-header-hover-description.jpg`, `d-edit-flow-box-docked-ask-noyan.jpg`,
  `d-edit-flow-start-from-presets-import-only.jpg`, `d-edit-flow-plus-palette.jpg`,
  `d-edit-flow-new-stage-panel-box-narrows.jpg` (Update the docs added, "1 change · added Update the docs · Undo",
  applies-at-once line; Validate's fails-go-to re-pointed to the new stage, as RESULTS-D noted),
  `d-edit-flow-watch-a-ticket-check-fails.jpg` ("Step 2 of 8"), `d-edit-flow-done-delivery-panel.jpg`,
  `d-edit-flow-more-menu.jpg`, `d-edit-flow-pick-a-skill-ask-agent-to-write.jpg`.
- Rounds +: 3 → 5, stops at 5; header "5 rounds · fails go to Update the docs". Undo takes back the last step (5 → 4).
- Save: Update the docs added after Validate → toast "Flow saved · Applied now. Tickets that haven't reached Update the
  docs will go through it; tickets past it finish as they are." Then Settings → Flow card "Changed by you · 16:44 · Go
  back to the previous flow" → draft "1 change · removed Update the docs · Another flow: no ticket is at work, so it
  applies at once." → Save. `otaq flow show -P SHOP` = the copy taken at the start (`fc`: no differences).

### Edit flow's box and Noyan (UX-63) · details
For these runs Settings → Agents → Noyan uses was switched from **Noyan (Codex)** to **Architect (Claude Code)** to
spare Codex; the box's line followed at once ("Noyan · Architect · Claude Code"). Put back afterwards.
- Typing shows **Ask Noyan** in place of Suggest. `d-edit-flow-box-typed-ask-noyan-button.jpg`. ↵ sent it; the panel
  stayed closed; the box became "New conversation is drafting a change to shop's flow… Show Stop", Suggest greyed,
  "Drag, + and × still work meanwhile". `d-edit-flow-drafting-line-names-conversation-show-stop.jpg`. Show opened the
  panel on that conversation. `d-edit-flow-show-opens-panel-while-drafting.jpg`
- My request asked Noyan to ask first with a long question. Claude ran `otaq ask human` (≈50-line context, 3 options
  with 2–3-line notes, recommended 1). The box turned green, "Needs you · Security review stage asks", grew to ≈418 of
  the editor's ≈757 px (55 %), the columns shrinking above it; options with notes, "1–4 pick · ↵ sends", Answer in
  terminal, Send answer. `d-edit-flow-noyan-question-box-55pct-text-squeezed.jpg` → NEW-d1.
- ⤢ ("Open larger"): the box fills the editor's height; question and context readable (paragraphs, a numbered list,
  inline code, ~60 characters a line), options and Send at the foot. `d-edit-flow-noyan-question-expanded.jpg`. Esc
  shrank it (`d-edit-flow-question-esc-shrinks.jpg`); ⤡ shrank it too.
- Answer in terminal opened the agent panel on the conversation ("Waiting for you", Architect · Claude Code); the box
  stayed and narrowed beside it. `d-edit-flow-answer-in-terminal-opens-panel-box-stays.jpg` → NEW-d2 for the panel's band.
- Send answer (option 3) from the box: Claude's terminal "Answer to your question: Reuse Checker (option 3).", the box
  back to the drafting row. `d-edit-flow-send-answer-agent-got-it-drafting-again.jpg`
- Proposal 39: bar "Security review stage's suggestion for shop · 1 change, marked · nothing changes until you save",
  Watch a ticket / Discard / Save flow (no Ask for changes… button); Why these changes; box "Ask for changes to
  Security review stage's proposal…" with "Goes to the same conversation · nothing changes until you save · Show
  conversation". `d-edit-flow-proposal-ask-for-changes-box.jpg`. Typing shows **Ask for changes**; ↵ "Also make
  Validate 2 rounds." → the same conversation wrote Proposal 40 (Added Security, Changed Validate "2 rounds").
  `d-edit-flow-ask-for-changes-same-conversation.jpg`
- Stop: see NEW-d3. `d-edit-flow-proposal-40-before-stop.jpg`, `d-edit-flow-after-stop-proposal-gone.jpg`,
  `d-edit-flow-proposal-41.jpg`, `d-edit-flow-redrafting-board-drops-proposal-41.jpg`,
  `d-edit-flow-proposal-42-same-conversation.jpg`.
- Suggest a whole flow: panel stayed closed, drafting row, then "Suggest shop flow's suggestion for shop · 2 changes"
  (Validate, Delivery → merge on Approve). Discard → plain editor, no stale drafting line. The greyed Suggest shows no
  reason on hover (commit: "the shortcut off with why") NOTE.
- NOTE · while Noyan drafted, the title bar showed Pause but no Running chip (same family as NEW-b1).

### Upkeep (A49, A51, A52) · PASS
- SHOP-45 "R8d quit and restart test" (CLI-created, Dev and Validate on Architect = Claude Code, told to run a 90 s ping
  and change nothing), started with Start → "1 running", card Working · Architect.
- Tray (the test app's icon; the qbox app's icon left alone): `d-tray-menu-agent-running.jpg`.
- Tray → Quit Otaq: `d-tray-quit-asks-agent-working.jpg` → **Quit and stop agents**: the test app, its service and
  its sessions ended (the qbox app untouched). Relaunched via `otaqtest`: banner "All projects are paused · Quit and stop
  agents stopped 1 agent in 1 project today at 16:45. Resume continues where each one was…" + Resume; card "Paused ·
  resumes where it was"; toast "SHOP-45 session interrupted · Resume from Otaq." titled Otaq.
  `d-reopen-after-quit-and-stop-paused-banner.jpg`. Resume → Working.
- Restart stand-in: `taskkill /T /F` of the test app and its service only, relaunch → "4 agents were stopped by a
  restart · SHOP-45 R8d quit and restart test · was working in Dev" + Noyan "Test step, no work", "Add Security stage",
  "Suggest shop flow" (conversation), "They wait in Needs you if you close this.", Not now / Resume 4. Unticked the
  three Noyan rows → Resume 1 → SHOP-45 Working again (Claude Code). `d-restart-dialog-4-agents.jpg`. SHOP-45 then
  finished on its own as Done (the agent ended it with no commit).

### CLI wording · PASS
- `otaq list -P SHOP` / `-P NOTE`: "Not started", "Ready for you", "Start failed", "Merged", "Closed · not merged".
  `otaq show SHOP-45`: "Status: Merged (Done) · … Dev: Architect · Validate: Architect · Choices: Dev: Architect · set
  by you". `otaq prime --project` (in shop): "# Noyan · shop (SHOP)", ## Tickets, ## The flow (each stage's
  description), ## Stage choices, ## Otaq's rules.

## New problems
- **NEW-d1 · Edit flow's answer box gives a long question's text one line when the options carry notes.** UX-63 / UX-62:
  the box grows to 55 % of the editor and past that "the question's text scrolls with the options and Send in view".
  With three options whose notes take 2–3 lines, the options, the hint row and Send fill the whole 55 %; the question's
  text area is ~1 line high: the bold question is cut in half and the context can't be read (the wheel moves it a line
  at a time). Only ⤢ shows it. Expected: the question line (and some context) always visible, e.g. the options' area
  scrolls or the text keeps a minimum. 1920×908 window (757 px editor). **Blocker: no** (⤢ shows everything; the
  options are readable). `d-edit-flow-noyan-question-box-55pct-text-squeezed.jpg`, `d-edit-flow-noyan-question-text-strip-zoom.jpg`
- **NEW-d2 · The same question above Noyan's terminal hides the question and stacks the buttons over the options.** In
  the agent panel (≈280 px wide) the band starts at option 1: no question line or context at all, "Answer in terminal" /
  "Send answer" and "1–4 pick" sit over the third option's note, and "Something else" is below the buttons. UX-62:
  first line is the question, the text scrolls while options and Send stay in view. Part b/fix check passed this with
  short questions. **Blocker: no** (Edit flow's ⤢ and the terminal show it; but a user answering in the panel can't
  read what is asked). `d-noyan-panel-question-band-no-question-text.jpg`, `d-noyan-panel-question-band-buttons-over-options.jpg`
- **NEW-d3 · Asking for changes drops the shown proposal, so Stop loses it.** UX-63 / commit 6b1a9074: "Stop (it ends;
  the draft keeps what it proposed)". When the user sends "Ask for changes…", the board drops the proposal at once and
  shows the saved flow while Noyan drafts; Stop then leaves the plain editor with no changes (Proposal 40 — Security +
  Validate 2 rounds — was gone). Expected: the last proposal stays on the board while it redrafts, and Stop keeps it.
  **Blocker: no** (ask again or redo by hand). `d-edit-flow-proposal-40-before-stop.jpg`,
  `d-edit-flow-after-stop-proposal-gone.jpg`, `d-edit-flow-redrafting-board-drops-proposal-41.jpg`
- **NEW-d4 · Edit flow's box and bar name Noyan's conversation title, not the agent.** UX-63: "<agent> is drafting a
  change…", "Ask for changes to <agent>'s proposal…", "Needs you · <agent> asks". Seen: "New conversation is drafting…",
  "Security review stage is drafting…", "Needs you · Security review stage asks", "Ask for changes to Add Security
  stage's proposal…", "Suggest shop flow's suggestion for shop", while the line under it says the agent (Architect).
  The first send reads "New conversation". RESULTS-D noted the same for the suggestion bar. **Blocker: no** (wording).
  `d-edit-flow-drafting-line-names-conversation-show-stop.jpg`
- **NEW-d5 · `otaq prime --project` still lists a worktree Cleanup deleted.** After Settings → Data → Cleanup deleted
  `home\wt\shop\SHOP-5` (folder gone, `git worktree list` no longer has it), prime's ticket list still says "SHOP-5 …
  worktree C:\…\home\wt\shop\SHOP-5 · branch otaq/SHOP-5", so Noyan is pointed at a folder that doesn't exist.
  **Blocker: no.**

## Notes
- NOTE-d1 · Settings → Agents still shows the amber permission-prompt warnings on Codex and Copilot rows (UX-36).
- NOTE-d2 · Cancel in Edit flow discards changes without asking (as RESULTS-D).
- NOTE-d3 · Remove project dialog still says "Project settings (commands, env files)" (UX-52; as RESULTS-D).
- NOTE-d4 · Flow card's changed line doesn't say what changed (as RESULTS-D).
- NOTE-d5 · The greyed Suggest a whole flow while drafting shows no reason on hover.
- NOTE-d6 · No Running chip in the title bar while Noyan drafts (Pause only); NEW-b1's family.
- NOTE-d7 · Files' source card has no "Showing" word (gap #44 default).
- NOTE-d8 · Adding an agent stage before a check re-points its "fails go to" silently (as RESULTS-D).
- NOTE-d9 · The ⤢ view has no visible scrim over the columns (they stay at full contrast beside it); minor.

## Not tested
- A53 update banner / Restart to update (source build never checks for updates).
- The restart dialog's unticked row for a signed-out tool and per-row Retry / Start from scratch (all tools signed in).
- UX-61's first-time Edit flow hint (one-time, already seen in this data).
- Import / Export file dialogs, Copy from another project, Apply now… / Cancel change with a ticket inside a changed
  stage (PASS in RESULTS-D and re-test 1; would need Codex runs on shop's flow).
- Actually removing a project (dialog only).

## Setup changes and state left
| Change | State now |
|---|---|
| Settings → Agents → Noyan uses: Noyan → Architect for the Noyan runs | **put back to Noyan** |
| Notifications "ready for review" toggled on and off; theme Dark → Match system; Windows AppsUseLightTheme 0 → 1 | put back (off; Match system; light) |
| shop Run command typed then cleared | put back (not set) |
| shop flow: Update the docs added and saved, then "Go back to the previous flow" saved; four Noyan proposals discarded | **back to Otaq default**; `otaq flow show -P SHOP` identical to `otaq-reltest\r8d-shop-flow-before.txt` (two new flow versions in its history) |
| Cleanup: deleted the leftover worktree `home\wt\shop\SHOP-5` (branch otaq/SHOP-5 kept) | SHOP-33's dirty worktree still listed, not deleted |
| shop SHOP-45 "R8d quit and restart test" (Architect / Claude Code, no file change) | Done (ended by its agent, no commit); branch otaq/SHOP-45 at main |
| shop README: a test line appended in the folder and reverted | shop main e97df39, clean |
| Noyan conversations (shop, Claude Code): "Security review stage" (stopped), "Add Security stage", "Suggest shop flow"; the fix check's "Test step, no work" (Codex) | all ended by the restart test and not resumed (they sit in Noyan's list) |
| Test app quit twice (tray "Quit and stop agents", then a taskkill of the app and its service as a restart stand-in) and relaunched via `otaqtest` | **running on the normal data**, app PID 17992, service PID 19780 (16:46), build 4147a741, shop board on screen, no agent running |
| Files `otaq-reltest\r8d-shop-flow-before.txt`, `r8d-shop-flow-now.txt`, `r8d-prime.txt` | left (no secrets) |
| The qbox Otaq (`~\otaq-qbox`, PIDs 8380 / 18580) | not touched; its window came to the front once after a test-app quit, no input sent to it |
| Clipboard | not used |
