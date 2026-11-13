# Orchestrator · release-round5

- **Ticket:** none
- **Date:** 2026-11-11
- **Plan:** tuq/plans/release-round5.md
- **Tech spec:** none (bug fixes and the decisions below; each issue's evidence is in ideas/ux-rethink/build/release-test/issues-round5/)
- **Decisions:** 040, 041, 042, 043

release-round5 fixes what the Windows re-test of release-fixes 528d738a and the round-4 piece agents left open, so
the release can go out after the git-hooks milestone.

You are the agent in charge of building release-round5. You don't build the pieces: you start one agent per piece, check
what it hands back and merge it. Read the plan first; each piece's details there are its brief.

## Run order

<!-- tuq:order:start -->

Start a piece as soon as every piece it waits for is merged:

- First: 1 Otaq types into an agent's line only when the screen shows it empty, 2 Every open prompt of a session reads Needs you, one at a time, 4 A message for a session that ended doesn't wait forever, 7 A "login expired" stop offers Sign in… at once, 8 On Windows, Ctrl+C in a terminal copies when text is selected, 9 Every view names the stage a held ticket starts at, 10 A newer "Ask the agent to write it" replaces one that never started (side by side)
- After 1, 2: 3 A prompt while a merge check is being fixed reads Needs you, 5 "Unsent text in the input line" shows only when the line holds text, 6 The card says Needs you while you're in the agent's terminal (side by side)

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

- Start only once the git-hooks milestone (tuq/plans/git-hooks.md) is merged into `release-fixes`; work on that
  branch, never on main.
- Start one Agent call per piece with isolation "worktree". Its prompt: the piece's details from the plan, its note
  below, and "commit on your branch; don't merge". Agent worktrees may be cut from `main`: tell each piece to
  `git reset --hard <release-fixes head>` first, and check the base on merge (`git merge-base --is-ancestor`).
- Check its diff and the project's tests in its worktree.
- Merge its branch into the current branch with `git merge`, one piece at a time, then start the pieces that waited
  for it.

## Notes per piece (only pieces that need one)

Pass these to every piece: the repo's rules are in CLAUDE.md; the old `decisions/` folder is lost (don't look for it
or write to it); product decisions are in tuq/decisions/ (read the ones the piece names in full); new comments name a
rule in words, never by its number. Each piece reads its issue file in ideas/ux-rethink/build/release-test/issues-round5/
and the screenshots it cites in ../shots-retest-5/ (uncommitted files: read them from the main checkout). Never change
the main checkout. Never push. Pieces don't run the full `scripts/check.sh`: only their crates' tests (nextest with a
filter, vitest for touched UI), `cargo fmt --check`, clippy on the changed crates, and the ts-rs freshness / protocol
snapshot tests when they change a view model or protocol type; the orchestrator runs check.sh on the merged branch.
check.sh's frontend step needs Node 24. Pieces needing Windows evidence (1, 2, 8) may use the VMPal Windows VM: build
in their own `%USERPROFILE%\otaq-build\<name>` folder, never take a snapshot, don't touch the release-test data in
`%USERPROFILE%\otaq-reltest` or the installed Otaq. The VM's clipboard is shared with the Mac: check every paste. Never
run the app on the maintainer's Mac session for hand checks.

### Piece 1

Find the cause before fixing (read the tracker with a pasted chunk; bracketed paste is likely). Re-test 5's
evidence contradicts round-4 piece 26's conclusion that Otaq can't type over text; say which it was.

### Piece 2

The largest piece. It changes how every session kind keeps prompts, which pieces 3 and 6 build on: name in the
hand-back what a session's open prompts now look like and where the "one on screen" is decided. Round-4 piece 28's
hand-back says Codex fires the hooks in the same millisecond, so the screen has to say which prompt is drawn.

### Piece 8

Windows-only behaviour: check it in the VM on a dev build, or with a vitest that sets the platform.

## Bringing it together

After the last merge: run `scripts/check.sh` once on the merged branch (with Node 24 on PATH). Then re-test on
Windows in the VM on the release build, the way the earlier re-tests did (install the NSIS build, OTAQ_HOME
`%USERPROFILE%\otaq-reltest\home`): each piece's Check: in the real app, PASS or FAIL, in a RESULTS-retest-6.md beside
RESULTS-retest-5.md. Include issue 10's gaps: a held ticket's column (if it can be had without signing a tool out),
"Removing waits until no ticket is in it" on a stage with a ticket in it, round-4 piece 23's answer sessions, and
Approve & open PR / Push to PR with pre-push and commit-msg hooks on a machine with gh or az (or say it couldn't be
tried). Do the small cleanups in issues-round5/README.md yourself (the `AgentToolSignIn` doc comment, the stale
`check_first.view.json` fixture). Then a short regression walk: a ticket through Review to Approve & merge with a
hook in the repo, Edit flow, Settings. Report the result to the maintainer before anything is merged into main.
