# Orchestrator · release-issues

- **Ticket:** none
- **Date:** 2026-11-10
- **Plan:** tuq/plans/release-issues.md
- **Tech spec:** none (bug fixes and the decisions above; each issue's evidence is in ideas/ux-rethink/build/release-test/issues/)
- **Decisions:** 011, 012, 013, 015, 016, 017 (014, 018 and 019 keep what is built: no piece)

release-issues fixes everything the Windows re-test of release-fixes and its piece agents left open, so the release
can go out.

You are the agent in charge of building release-issues. You don't build the pieces: you start one agent per piece, check
what it hands back and merge it. Read the plan first; each piece's details there are its brief.

## Run order

<!-- tuq:order:start -->

Start a piece as soon as every piece it waits for is merged:

- First: 1 The restart dialog reads each tool's real state, 2 The restart dialog's title counts every stopped agent, 4 A stage whose Codex skill has a byte order mark warns, 5 A queued writer request is dropped when the ticket leaves Review, 6 The Approve dialog never says someone is writing when no one is, 7 The running chip shows after Keep them running, 8 An agent idle at its prompt doesn't read as Working, 9 The merge-check failure dialog shows the check's output, 10 Cancel in Edit flow removes the skill the agent wrote, 11 A written but uncommitted skill's warning says so, 12 Edit flow's draft line names stages by name, 13 Noyan's CHANGED row says what changed, 16 A skill is named the way the stage's tool calls it, 17 "Drafting a flow" gives way to Needs you while the agent waits, 18 A send-back comment names each stage once, 19 Codex's Needs you clears once its prompt is answered, 20 Clicking a Windows notification opens Otaq on its ticket, 21 A parked ticket names the stage it was last parked from, 22 The flaky tests wait on state, not time (side by side)
- After 1, 13: 3 Sign in… on a signed-out row of the restart dialog, 14 Undo on Noyan's CHANGED row, 15 A ticket Noyan created and changed shows under CREATED only (side by side)
- After the round-2 fixes of 7, 8 and 17 (after re-test 2): 23 A key alone answers a Codex prompt in every agent session
- After re-test 4 (base 5e8e9fc7), side by side: 24–32 (Round 4 in the plan)

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

- Work on the branch `release-fixes`, where the re-test ran (7f36bd79 and later), not on main.
- Start one Agent call per piece with isolation "worktree". Its prompt: the piece's details from the plan, its note
  below, and "commit on your branch; don't merge".
- Check its diff and the project's tests in its worktree.
- Merge its branch into the current branch with `git merge`, one piece at a time, then start the pieces that waited
  for it.

## Notes per piece (only pieces that need one)

Pass these to every piece: the repo's rules are in CLAUDE.md; the old `decisions/` folder is lost (don't look for it
or write to it); product decisions are in tuq/decisions/ (read the ones the piece names in full); new comments name a
rule in words, never by its number. Each piece reads its issue file in ideas/ux-rethink/build/release-test/issues/
and the screenshots it cites in ../shots-retest/. Never change the main checkout. Never push. Pieces that need
Windows evidence (1, 7, 19, 20, and any that doesn't reproduce on the Mac) may use the VMPal Windows VM: build in its
own `%USERPROFILE%\otaq-build\<name>` folder, never take a snapshot, and don't touch the release-test data in
`%USERPROFILE%\otaq-reltest` unless the piece says so. Never run the app on the maintainer's Mac session for hand
checks; use the VM.

Several pieces running side by side edit the same files. Merge them one at a time and have the later ones rebase:
the restart dialog (1, 2, 3: after_restart.rs, AfterRestart.tsx), the flow editor (4, 10, 11, 12, 16, 17:
flow_editor.rs, flow_board.rs, screens/flow_editor and flow_board), skill messages (4, 16: tool_problems.rs and the
held message), the Review and delivery dialogs (5, 6, 9: pr_dialog.rs, review_page.rs, approve.rs), the card's state
(7, 8: card.rs, card_line.rs), Noyan's panel (13, 14, 15: agent_panel.rs).

### Piece 1

Find the cause before fixing: reproduce the "not installed" row in a test first (a restart while the service's first
tool check is still running is the likeliest). The re-test notes say a plain Codex restart with nothing removed from
PATH worked, so the failing runs differ in timing or in which tool was looked up.

### Piece 2

Decision 011 reverses what release-fixes piece 4 built on purpose; update that piece's tests to the new rule, don't
keep both.

### Piece 6

Decision 010 still holds while a writer is running. This piece is only about a writer that was asked and isn't
running any more.

### Piece 11

The maintainer chose the warning over hiding it: it must stay a warning, saying the skill is written but not
committed. Match its words to what Apply actually does with the file; if Apply doesn't commit or keep it, say what
the user has to do instead and report that back.

### Piece 13

Its agent in release-fixes (piece 22) said this needs a protocol change. Make the change the panel needs; pieces 14
and 15 build on it, so name in the handback what the panel now gets for a change.

### Pieces 19 and 21

Both may end with no code change. A piece that can't reproduce its issue reports what it tried and what it saw; that
counts as done.

### Piece 20

Check it on the NSIS-installed app in the VM, not a dev build, as release-fixes piece 13 did.

## Bringing it together

After the last merge: run `scripts/check.sh` once on the merged branch. Then re-test on Windows in the VM on the
release build, the way the re-test did (install the NSIS build, OTAQ_HOME in `%USERPROFILE%\otaq-reltest\home`): each
piece's Check: in the real app, PASS or FAIL, in a RESULTS-retest-2.md beside RESULTS-retest.md. Include issue 03,
which has no piece: ask the maintainer to sign Claude Code in on the VM, then check a session that ends on a tool
error (sign it out between) gives "Stopped · login expired", Sign in… (does it open `claude auth login`?) and Resume;
the restart dialog's signed-out row ("Claude Code isn't signed in · sign in first", now with Sign in… from piece 3);
and the notes project's Spec first flow (Product spec, Tech spec and Milestone plan on Claude Code). Then a short
regression walk of first run, a ticket through Review to Approve & merge, Edit flow and Settings. Report the result to
the maintainer before anything is merged into main.
