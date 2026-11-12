# Orchestrator · release-round6

- **Ticket:** none
- **Date:** 2026-11-11
- **Plan:** tuq/plans/release-round6.md
- **Tech spec:** none (bug fixes; each issue's cause, fix direction and risks are in ideas/ux-rethink/build/release-test/issues-round6/)
- **Decisions:** 044, 045, 046 (010 and 042 kept)

release-round6 fixes the release blockers re-test 6 of release-fixes d0a165b5 found, so the release can go out.

You are the agent in charge of building release-round6. You don't build the pieces: you start one agent per piece,
check what it hands back and merge it. Read the plan first; each piece's details there are its brief.

## Run order

<!-- tuq:order:start -->

Start a piece as soon as every piece it waits for is merged:

- First: 1 A skill Otaq types on its own into Codex is sent, 5 In a Codex terminal on Windows, a selection survives until Ctrl+C (side by side)
- After 1: 2 A prompt during a delivery skill turn reads Needs you while the ticket stays in Review
- After 2: 3 An answer session can be seen and reached
- After 3: 4 A question whose answer session ended unanswered goes to Needs you
- After 4: 6 A writer prompt left open when the ticket leaves Review reads Needs you (added after re-test 7)

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
  file's path, its note below, and "commit on your branch; don't merge". Agent worktrees may be cut from `main`: tell
  each piece to `git reset --hard <release-fixes head>` first, and check the base on merge (`git merge-base
  --is-ancestor`).
- Check its diff and the project's tests in its worktree.
- Merge its branch into the current branch with `git merge`, one piece at a time, then start the pieces that waited
  for it.

## The real-tool rule

Rounds 4 and 5 merged pieces whose tests passed against a stand-in (`sh` printing a tool's text, an xterm stand-in)
and failed against the real tool. In this round:

- A piece's Check: is done against the real tool where the plan says so (Codex in the VMPal Windows VM), by the piece
  agent, and its hand-back says what it saw there. A piece that couldn't reach the real tool says so, and you don't
  merge it until you or another agent has checked it there.
- Tests may use stand-ins, but a stand-in must behave as the real tool does in the case under test (recorded output
  from the real tool, or the real tool's documented contract); say which in the test's comment.

## Notes per piece (only pieces that need one)

Pass these to every piece: the repo's rules are in CLAUDE.md; the old `decisions/` folder is lost (don't look for it
or write to it); product decisions are in tuq/decisions/ (read the ones the piece names in full; several are
uncommitted: read them from the main checkout). New comments name a rule in words, never by its number. Each piece
reads its issue file in ideas/ux-rethink/build/release-test/issues-round6/ and the screenshots it cites in
../shots-retest-6/ (uncommitted files: read them from the main checkout). Never change the main checkout. Never push.
Pieces don't run the full `scripts/check.sh`: only their crates' tests (nextest with a filter, vitest for touched UI),
`cargo fmt --check`, clippy on the changed crates, and the ts-rs freshness / protocol snapshot tests when they change
a view model or protocol type; the orchestrator runs check.sh on the merged branch. check.sh's frontend step needs
Node 24. The VMPal Windows VM: build in your own `%USERPROFILE%\otaq-build\<name>` folder, never take a snapshot,
don't touch the release-test data in `%USERPROFILE%\otaq-reltest` or the installed Otaq unless the piece's Check: needs
the release build (then say so and put things back), don't send keys to another piece's sessions; the VM's clipboard
is shared with the Mac: check every paste. Never run the app on the maintainer's Mac session for hand checks.

### Piece 1

The issue's probe (`issues-round6/probe-new5/`) shows how to drive real Codex in a ConPTY; turn it into an env-gated
test. Try a multi-line message before anything else: it's the one untested part of the fix.

### Piece 2

Decision 010 stays: don't change the writer's case or the test that locks it. Build the mark so piece 3 can reuse it,
and name the shape in the hand-back.

### Piece 4

The worktree guard protects a live ticket's work: test the reopened-ticket case first.

### Piece 5

The round-5 vitest passed while the app failed: the stand-in must clear its selection on a reported mouse move.

## Bringing it together

After the last merge: run `scripts/check.sh` once on the merged branch (with Node 24 on PATH). Then re-test on
Windows in the VM on the release build, the way re-test 6 did (install the NSIS build, OTAQ_HOME
`%USERPROFILE%\otaq-reltest\home`): each piece's Check: in the real app, PASS or FAIL, in a RESULTS-retest-7.md beside
RESULTS-retest-6.md, plus what re-test 6 couldn't try (round 5 piece 7's login-expired Sign in…, check 1's Claude Code
side) where it can be had without signing a tool out. Then the core-path walk on the release bar: a ticket started,
an agent prompt reading Needs you, review, Approve & merge with a hook, and Push to PR on the local bare remote with
pre-push and commit-msg hooks; Edit flow; Settings. Report the result to the maintainer before anything is merged into
main; new findings that aren't release blockers go on the known-issues list, not into another round.
