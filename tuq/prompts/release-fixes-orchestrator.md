# Orchestrator · release-fixes

- **Ticket:** none
- **Date:** 2026-11-10
- **Plan:** tuq/plans/release-fixes.md
- **Tech spec:** none (the decisions and the release test's evidence the plan names)
- **Decisions:** 009, 010; UX-11, UX-14, UX-16, UX-21, UX-26, UX-36, UX-55, UX-56, UX-58, UX-60, A20, A22, A42, A49, A52 in ideas/ux-rethink/; S5–S7 in ideas/no-skill-copies/

release-fixes makes the app do what was agreed everywhere the Windows release test found it didn't, before the
release.

You are the agent in charge of building release-fixes. You don't build the pieces: you start one agent per piece, check
what it hands back and merge it. Read the plan first; each piece's details there are its brief.

## Run order

<!-- tuq:order:start -->

Start a piece as soon as every piece it waits for is merged:

- First: 1 Edit flow never goes blank after a refused reorder, 2 A logged-out tool shows as signed out, 3 A card says why its tool stopped, 5 The merge message writer asks in the Approve dialog, and stops with the ticket, 6 Otaq's notes and "Tell <agent> now" reach a Codex agent, 7 Personal Codex skills are found, 8 Removing a stage waits for the tickets in it, held ones too, 9 A check that runs out of rounds can always be sent on, 12 Tools shows no permission warnings, 13 Windows notifications are Otaq's, 14 Quit Otaq from the tray asks while agents work, 15 A ticket's own agent shows from the moment it's set, 16 A refused drag says what the ticket waits for, 17 The palette says "Pause <project>" with its shortcut, 18 Overview shows the command a permission prompt asks for, 19 "What the agent did" shows the latest round, 20 Delivery problems read right, 21 The send-back reason is the agent's, 22 Noyan's panel lists the changes it made (side by side)
- After 1, 2: 4 The restart dialog lists a tool that can't resume, 10 "Ask the agent to write it" in Pick a skill… and the missing-skill warning, 11 The "drafting a flow" line goes when the draft does (side by side)

<!-- tuq:order:end -->

## Running a piece

### In an Otaq build stage ($OTAQ_TICKET set, prime shows this Plan)

- Start a piece with `otaq helper start N --title "<the Plan's title>"`; when it has a note below, pass the note on
  stdin (`--stdin`).
- Check what it hands back with `otaq helper diff N` and the project's tests in its worktree (`otaq helper list`
  names it).
- Merge with `otaq helper merge N`, one piece at a time, then start the pieces that waited for it.
- Follow the build stage's own rules for questions, hand-backs and re-plans; this prompt never replaces them.

### Anywhere else

- Work on the branch `release-fixes`, made from `ux-rethink-fixes` (PR #61), not on main.
- Start one Agent call per piece with isolation "worktree". Its prompt: the piece's details from the plan, its note
  below, and "commit on your branch; don't merge".
- Check its diff and the project's tests in its worktree.
- Merge its branch into the current branch with `git merge`, one piece at a time, then start the pieces that waited
  for it.

## Notes per piece (only pieces that need one)

Pass these to every piece: the repo's rules are in CLAUDE.md; the old `decisions/` folder is lost (don't look for it
or write to it); new comments name a rule in words, never by its number. Each piece reads its phase's RESULTS file for
the exact steps and words seen, in ideas/ux-rethink/build/release-test/ (RESULTS-A…D.md, the screenshots in its
shots/, the test setup in SETUP.md and the sorted list in FAILURES.md). Never change the main checkout. Never
push. Pieces that need Windows evidence (6, 7, 13, 14, and 1 if it doesn't reproduce on the Mac) may use the VMPal Windows VM: build in its own
`%USERPROFILE%\otaq-build\<name>` folder, never take a snapshot, and don't touch the release-test data in
`%USERPROFILE%\otaq-reltest` unless the piece says so. Never run the app on the maintainer's Mac session for
hand checks; use the VM.

Most pieces run side by side but several edit the same files. Merge them one at a time and have the later ones
rebase: the card's words (pieces 3, 9, 15: view_models/card.rs, card_line.rs), Overview (pieces 3, 5, 18, 19, 20,
21: the sheet's view models and screens/sheet), the flow editor (pieces 1, 8, 10, 11: flow_board.rs, flow_editor.rs,
screens/flow_board), the tool sign-in state (pieces 2, 4, 12: agent_tools/, the Tools and Agents pages).

### Piece 1

Find the cause before fixing: reproduce the refused reorder in a test first. If the blank screen is a page error,
the fix includes the page never going blank on a bad view (show what's wrong instead).

### Piece 2

The fix is in reading each tool's own sign-in answer; never guess from a config file's presence. Watch that a slow or
failing status command shows the tool as "couldn't check" with Re-check, not as signed in.

### Piece 5

Decision 010 is new: read it in full. The dialog's words are the decision's; the writer's terminal is the one its
session already has.

### Piece 6

Reproduce with Codex on Windows in the VM first: the placeholder text and its colour come from Codex's own screen.
The rule must hold for every tool's placeholder, not a list of Codex's words.

### Piece 9

Decision 009 replaces the "Move on anyway" part of one-check-agent O5; "Skip this check" stays as O5 says.

### Piece 13

Windows shows a toast under the identity of the process that sent it. Check it on the NSIS-installed app in the VM, not
a dev build.

## Bringing it together

After the last merge: run `scripts/check.sh` once on the merged branch. Then re-test every failure on Windows in the
VM on the release build (the same way as the release test: install the NSIS build, OTAQ_HOME in
`%USERPROFILE%\otaq-reltest\home`): each piece's Check: in the real app, and the 23 real failures listed in
ideas/ux-rethink/build/release-test/FAILURES.md, PASS or FAIL, in a RESULTS-retest.md beside them. Then a short
regression walk of first run, a ticket through Review to Approve & merge, Edit flow and Settings. Claude Code wasn't
logged in in the VM during the release test: piece 2's re-test needs it logged out once, then ask the maintainer to log
it in if the walk should use it. Report the re-test's result to the maintainer before anything is merged into main.
