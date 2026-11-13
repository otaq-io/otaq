# Orchestrator · git-hooks

- **Ticket:** none
- **Date:** 2026-11-11
- **Plan:** tuq/plans/git-hooks.md
- **Tech spec:** none (the decisions above; each issue's evidence is in ideas/ux-rethink/build/release-test/issues-hooks/, the exploration in ../RESULTS-hooks.md)
- **Decisions:** 026, 027, 028, 029, 030, 031, 032, 033, 034, 035, 036, 038, 039

git-hooks makes Otaq work with a project's git hooks: a hook at work and a hook's refusal show on the ticket, Otaq
never rebases, messages follow the project's rules, and the 11 issues of the Windows hooks exploration are fixed
before the release.

You are the agent in charge of building git-hooks. You don't build the pieces: you start one agent per piece, check
what it hands back and merge it. Read the plan first; each piece's details there are its brief.

## Run order

<!-- tuq:order:start -->

Start a piece as soon as every piece it waits for is merged:

- First: 1 Otaq's git steps know which hook runs or refused, with its whole output, 4 Otaq's merges and agents' commits follow the project's message rules, 7 An edited merge or PR message is kept until the ticket lands, 8 Otaq brings a branch up to date by merging main in, never by rebasing, 10 A failed start leaves nothing half-made; Retry start starts over, 12 Agents push when the user asks, or when delivering, 13 Otaq checks that a delivery push landed (side by side)
- After 1, 8: 2 A refused Approve & merge shows on the ticket, 3 A refused start shows on the ticket, 5 The real step and the running hook show while a git step runs, 9 Otaq's temporary save of unfinished files skips the hooks, 11 A hook's changes to the landing commit are said (side by side)
- After 2, 3, 5, 7: 6 Stop on a running git step, with no time limit, 14 A hook that asks in the terminal is named as such, 15 The post-merge hook in the user's folder is shown, its output kept, 16 Reworded PR messages go through the commit-msg hook first (side by side)

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

- Work on the branch `release-fixes`, not on main: every piece branches from it and is merged back into it
  **locally** with `git merge`. Never push, never open a PR, never merge into main.
- Start one Agent call per piece with isolation "worktree". Its prompt: the piece's details from the plan, its note
  below, the notes for every piece, and "commit on your branch; don't merge".
- Check its diff and the targeted tests it ran in its worktree.
- Merge its branch into `release-fixes` with `git merge`, one piece at a time, then start the pieces that waited
  for it.

## Notes per piece (only pieces that need one)

Pass these to every piece: the repo's rules are in CLAUDE.md; the old `decisions/` folder is lost (don't look for it
or write to it); product decisions are in tuq/decisions/ (read the ones the piece names in full); new comments name a
rule in words, never by its number. Each piece reads its issue file in ideas/ux-rethink/build/release-test/issues-hooks/
and the screenshots it cites in ../shots-hooks/; ../RESULTS-hooks.md has the table of which Otaq step runs which hook,
and where. Tests make real hooks in a temporary repository (sh scripts in `.git/hooks`, or `core.hooksPath`), never in
this repository. Run only the tests the piece touches: the full `scripts/check.sh` runs once, on the merged branch,
at the end. What must not change: Otaq runs the user's own git with the user's hooks and never passes `--no-verify`
(the one exception is piece 9); a refused hook leaves main, the branch and the worktree clean; agents read a hook's
output, don't bypass it, and ask. Never change the main checkout. Never push. Pieces that need Windows evidence
(path shortening, `/dev/tty`, sh hooks under Git for Windows) may use the VMPal Windows VM: build in its own
`%USERPROFILE%\otaq-build\<name>` folder, never take a snapshot, and don't touch `%USERPROFILE%\otaq-reltest`.
Never run the app on the maintainer's Mac session for hand checks; use the VM.

Several pieces running side by side edit the same files. Merge them one at a time and have the later ones merge
`release-fixes` in and resolve: Local merge (2, 5, 6, 8, 11, 15: git/merge.rs, review/, approve.rs, Dialogs.tsx),
the start (3, 6, 9, 10: start/worktree.rs, start/sequence.rs, git/checkpoint.rs), the skills (4, 12, 14:
plugin/skills/commit, push, open-pr, work-ticket, restack), the PR flow (7, 13, 16: pr/, pr_dialog.rs).

### Piece 1

It sets the shape every display piece builds on. Ask its agent to name, in the hand-back, what a git step now reports
(the running step, the running hook and since when, the refusing hook and its output) and how a caller gets it. Pass
that on to pieces 2, 3, 5, 11, 14, 15 and 16. If telling commit-msg from pre-commit reliably turns out to need Otaq to
run a hook itself, it must still be the user's own hook with the user's environment, never a copy.

### Piece 2

The refusal must be the same thing whether Approve & merge was a click or the flow's automatic delivery. Merge the two
ways into one rather than adding a third. Piece 3 and piece 16 reuse what this piece builds for the sheet and the
card: ask for it to be named in the hand-back.

### Piece 8

This is the largest piece and changes behaviour other parts rely on (the reviewed head, "changed since you reviewed
it", conflicts going to the agent, the Merge check's "main has moved" rule). Ask its agent to list in the hand-back
every place that rebased and what each does now. A merge of main into the branch is a commit on the agent's branch
and runs the project's hooks (prepare-commit-msg, commit-msg, pre-merge-commit): its refusal is handled like any
other (026); its message is git's plain merge message unless the project's rules (028) say otherwise. Piece 9 waits
for this one because it decides which updates still need the temporary save.

### Piece 9

This is the one place Otaq skips the user's hooks, by the user's decision (030). Keep it to the temporary save and say
so in the code's comments. Every other commit must still run the hooks.

### Piece 12

Skills change only in words. Read each skill in full before editing. "Never open a PR, never merge" stays where it is
today; only the push rule changes.

### Piece 13

This piece doesn't run the push: the delivery agent still does (034). It adds Otaq's check after the agent says it is
done.

## Bringing it together

After the last merge: run `scripts/check.sh` once on the merged `release-fixes`. Then re-run the hooks exploration on
Windows in the VM on the release build, the way ../RESULTS-hooks.md did it (the HOOK project in
`%USERPROFILE%\otaq-reltest\hooksproj`, the same hook scripts), scenario by scenario. Write PASS or FAIL for each
piece's Check: in the real app, with screenshots, in RESULTS-hooks-retest.md beside RESULTS-hooks.md. Also check:
- a refusal of each kind ends on the ticket, with the dialog open and closed;
- no hook path shows the Windows user name;
- the hook log shows pre-rebase never runs.

Approve & open PR and Push to PR with pre-push and commit-msg hooks still need a machine with gh or az. List them as
a manual check for the maintainer and don't fake them. Report the result to the maintainer; nothing goes into main.
