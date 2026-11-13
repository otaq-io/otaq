# Release regression test · Windows VM · branch ux-rethink-fixes (ef84b175, PR #61)

Goal: before a release, prove in the real app (Windows build) that every agreed decision holds and every feature
works. Report results in RESULTS-<phase>.md next to this file: one line per check, PASS / FAIL / NOT TESTED, with
what was seen (and a screenshot path under shots/ for every FAIL).

## Sources of truth (read before testing your phase)
Repo on the Mac (read only; never run the app on the Mac): /Users/nevzattopcu/Desktop/Workspace/otaq
- ideas/ux-rethink/DECISIONS.md (UX-1…61), ideas/ux-rethink/ASSUMPTIONS.md (A1…53, as drawn)
- ideas/ux-rethink/build/gap-analysis.md: what is settled; its "Build differs" list are KNOWN gaps (report them as
  KNOWN, not new findings)
- ideas/ux-rethink/build/questions.md: what each piece built
- Canvas as text (the drawn design): /private/tmp/claude-501/-Users-nevzattopcu-Desktop-Workspace-otaq/de3a40a2-9ec6-4414-af3c-524ad89a007e/scratchpad/canvas-txt/*.txt
- Other decision folders: ideas/no-fallback-agents, ideas/no-skill-copies, ideas/one-check-agent, ideas/noyan-intake,
  ideas/approve-options, tuq/decisions (comment attachments 001–008)

## The VM (VMPal, Windows 11 ARM64, id 3F020B57-E98C-49D5-AA38-408A0D1D6D8E)
- ONLY the VM. Never launch Otaq, agents or browsers on the Mac; the user is working on it.
- No snapshots (the maintainer said so). The VM is only for tests.
- Screen: no tree; use vmpal_screenshot (and vmpal_act's text targets / OCR). Keyboard layout is UK: `type` may
  refuse, press keys or use method "keys"; backslash is IntlBackslash. Ctrl, not Cmd, for shortcuts (Otaq maps ⌘ to
  Ctrl on Windows).
- vmpal_exec runs commands (cmd.exe /c … or powershell.exe -NoProfile -Command …). Programs it starts are off-screen:
  start GUI apps with a scheduled task: `schtasks /create /tn otaqtest /tr "<cmd>" /sc once /st 00:00 /it /f` then
  `schtasks /run /tn otaqtest`.
- The test install: see SETUP.md (written by the lead before phase A). The test app uses its own data folder
  (OTAQ_HOME=%USERPROFILE%\otaq-reltest\home) so the user's normal install data is untouched.
- Agent tools in the VM: Claude Code (claude.exe), Codex, Copilot CLI, OpenCode. Codex and Copilot ask permission for
  every command. gh and az are NOT installed (good for testing "Set up GitHub…").
- Windows Firewall may ask about otaq: Cancel is fine.
- Real agents spend the user's tokens: keep tickets tiny (typo fixes, one-line changes).

## Phases (run one at a time; the screen is shared)
- A · First run: Welcome → Tools → Project → "Your first ticket" → Create & start (notifications ask) → question /
  checker fail hints → Review → Approve & merge → "That was your flow" → "New work starts here" box → usage banner.
  Git name/email asked before the first start (Save and start) and at merge. UX-14…18, 20, 35, 36, 61 (first part),
  A12–A17. Window close note (agents keep working; menu bar / tray).
- B · Board and window: columns = stages with kind headers, Backlog/Done ends, card states and edges (UX-21), Start
  (UX-12, 15), waiting cards, dependencies (Waits for, hover, refused drag; UX-29, 39, A41–42), List layout (UX-33,
  Backlog after stages), Fold empty columns (UX-38), box for new work ↵ / Ctrl↵ / ⇧↵ (UX-22, 48), full form (priority,
  waits for, attachments, own agents / skip, base branch; UX-11, 31, 46), search with in:/by: (UX-40), palette (A48),
  Running / Pause / limit chip (UX-19, 34), Needs you strip and Ctrl+J, project switcher, Terminal drawer (UX-5),
  Noyan panel: shape from the box, created rows with Start, switcher/continue, Ask Noyan about this (UX-22, 47,
  noyan-intake).
- C · Ticket: sheet tabs Overview / Session / Changes / History (UX-4, 45, A18–A24), questions answered on Overview,
  Comment vs Tell <agent> now (UX-43, 58) with attachments, Edit ticket in place and ⋯ menu (A46), Send back, an agent
  asking to send back to an earlier stage (UX-55) and undoing a skip on Spec first (UX-31/32/54), checker fail and
  rounds → Decide (UX-60), Approve & merge with the prepared message (UX-42, 56), Done nothing to merge / don't merge
  (UX-13), PR delivery without gh ("Set up GitHub…", UX-57), conflicts going back on their own (UX-24, 44), Plan &
  helpers (A43).
- D · Flow, settings, upkeep: Edit flow in place (A1, UX-59–61: + palette, Watch a ticket, Start from… with Suggest,
  presets, Import/Export UX-41, rounds, Saved changes / Apply now UX-60, skills UX-26, descriptions UX-53, delivery in
  the flow UX-25, every stage own column UX-51); project Settings page (UX-10, 25, 52, Notes, Remove); app Settings
  eight topics (UX-9, 28, 34, 50, 57, Appearance Match system UX-16/A34, Keyboard, Data with Cleanup A50, About);
  Files view committed only with "Showing" (UX-30, 48); restart dialog (A49, quit Otaq while agents run and reopen),
  tray menu (A51), quit dialog (A52), update banner if testable (A53); CLI `otaq list/show/prime` wording.

## Rules for testers
- Test what a user does: click through the real UI. Use the CLI only to set up state or confirm data.
- A FAIL needs the decision number, what was expected, what was seen, and a screenshot.
- Don't fix code. Don't push, commit or change the repo on the Mac. Don't touch the user's normal Otaq data
  (%USERPROFILE%\.otaq) or ~\otaq-trial.
- If something blocks the phase (crash, can't install), say so in RESULTS and stop.
