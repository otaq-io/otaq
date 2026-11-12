# Orchestrator · release-round7

- **Ticket:** none
- **Date:** 2026-11-12
- **Plan:** tuq/plans/release-round7.md
- **Tech spec:** none (bug fixes; each issue's cause, fix direction and risks are in ideas/ux-rethink/build/release-test/issues-round7/)
- **Decisions:** 048, 049 (010, 025, 044 kept; UX-62)

release-round7 fixes the ten issues the Windows regression test of release-fixes fae1fd78 (re-test 8) left open, so
the release can go out.

You are the agent in charge of building release-round7. You don't build the pieces: you start one agent per piece, check
what it hands back and merge it. Read the plan first; each piece's details there are its brief.

## Run order

<!-- tuq:order:start -->

Start a piece as soon as every piece it waits for is merged:

- First: 1 A long one-line message Otaq types into Claude Code arrives whole, 2 On Windows, Ctrl+V in a terminal pastes text, 4 ↵ in an answer's Something else field sends the answer once, 6 A long question on Overview keeps Send in view, 8 Ctrl+J and the strip open Overview when nothing waits in a terminal, 9 A ticket done without a merge reads so everywhere, 10 Merge into another ticket offers only tickets in the final review, 11 Noyan's conversation switcher lists each conversation once (side by side)
- After 2, 4: 3 An empty input line isn't unsent text after a raw key, 5 An answer's picks and text survive leaving Overview (side by side)
- After 5: 7 An agent's question shows while the ticket waits in your review

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

- Work on `release-fixes`, never on main.
- Start one Agent call per piece with isolation "worktree". Its prompt: the piece's details from the plan, its issue
  file's path, the notes below for every piece and its own, and "commit on your branch; don't merge". Agent worktrees
  may be cut from `main`: tell each piece to `git reset --hard <release-fixes head>` first, and check the base on merge
  (`git merge-base --is-ancestor`).
- Check its diff and the project's tests in its worktree.
- Merge its branch into the current branch with `git merge`, one piece at a time, then start the pieces that waited
  for it.

## The real-tool rule

Rounds 4 and 5 merged pieces whose tests passed against a stand-in and failed against the real tool. A piece whose
Check: names the real tool (pieces 1, 2, 3, and 7's VM check) is checked there by its agent, and its hand-back says
what it saw. A piece that couldn't reach the real tool says so, and you don't merge it until you or another agent has
checked it there. Tests may use stand-ins only when they behave as the real tool does in the case under test (recorded
output from the real tool, or its documented contract); say which in the test's comment.

## Notes per piece (only pieces that need one)

Pass these to every piece: the repo's rules are in CLAUDE.md; the old `decisions/` folder is lost (don't look for it
or write to it); product decisions are in tuq/decisions/ (read the ones the piece names in full; 048 and 049 may be
uncommitted: read them from the main checkout), UX-62 in ideas/ux-rethink/DECISIONS.md. New comments name a rule in
words, never by its number. Each piece reads its issue file in ideas/ux-rethink/build/release-test/issues-round7/ and
the screenshots it cites in ../shots-retest-9/ (uncommitted files: read them from the main checkout). Never change the
main checkout. Never push. Pieces don't run the full `scripts/check.sh`: only their crates' tests (nextest with a
filter, vitest for touched UI), `cargo fmt --check`, clippy on the changed crates, and the ts-rs freshness / protocol
snapshot tests when they change a view model or protocol type; the orchestrator runs check.sh on the merged branch.
check.sh's frontend step needs Node 24. The VMPal Windows VM (any piece may use it): build in your own
`%USERPROFILE%\otaq-build\<name>` folder, never take a snapshot, don't touch the release-test data in
`%USERPROFILE%\otaq-reltest` or the installed Otaq unless the piece's Check: needs the release build (then say so and
put things back), don't send keys to another piece's sessions or the other Otaq in `~\otaq-qbox`; the VM's clipboard
is shared with the Mac: check every paste. Only one piece drives the VM's screen at a time: you hand it out. Never run
the app or a tool in the maintainer's own Mac sessions for hand checks; Claude Code on the Mac only in a harness of the
piece's own, in a scratch folder.

### Piece 1

The investigation's probe harness and measured table are in issue 01 and the VM's `%USERPROFILE%\otaq-build\new12`:
reuse them rather than starting over. Its scratch folders are already trusted by Claude Code in the VM. Prefer the
narrowest fix that holds (Claude Code's quirk) unless Codex is shown to take bracketed one-line text the same; never
change Codex's bytes without a real-Codex check.

### Piece 2

Start from 04d9543e (cherry-pick or merge it onto release-fixes); it was never built or tried in the app. The image
paste case needs an image on the VM's clipboard: set it from PowerShell, never from the Mac's clipboard.

## Bringing it together

After the last merge: run `scripts/check.sh` once on the merged branch (with Node 24 on PATH). Then re-test on Windows
in the VM on the release build, the way re-test 8 did (install the NSIS build, the release-test data in
`%USERPROFILE%\otaq-reltest`): each piece's Check: in the real app, PASS or FAIL, in a RESULTS-retest-10.md beside
RESULTS-retest-9-repro.md, then the regression checks still open in
ideas/ux-rethink/build/release-test/REGRESSION-CHECKS-LEFT.md that the fixes unblock (the written message reaching the
Approve and Open PR dialogs on Claude Code, Ctrl+V in Claude Code and Codex). Report the result to the maintainer
before anything is merged into main; new findings that aren't release blockers go on the known-issues list, not into
another round.
