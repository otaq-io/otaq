# Orchestrator · fixes-before-0-3

- **Ticket:** none
- **Date:** 2026-11-15
- **Plan:** tuq/plans/fixes-before-0-3.md
- **Tech spec:** none (issues reproduced in the 0.3 bring-together of skills-in-their-own-repos, PR #73)
- **Decisions:** none

fixes-before-0-3 fixes the four issues still open on PR #73 before Otaq 0.3, with no side effects on anything else.
Build on branch `skills-repos-decisions` (the PR's branch); every piece's worktree needs `git submodule update --init`
(`plugin/skills` is the otaq-skills submodule). You are the agent in charge of building fixes-before-0-3. You don't
build the pieces: you start one agent per piece, check what it hands back and merge it. Read the plan first; each
piece's details there are its brief.

## Run order

<!-- tuq:order:start -->

Start a piece as soon as every piece it waits for is merged:

- First: 1 A Codex agent passes long text to otaq without leaving the sandbox, 2 A Codex agent commits without a prompt, 3 A second Otaq leaves the shared Codex home alone unless told, 4 Copilot never updates itself during a session, 5 The floating-layers test passes on Windows (side by side)

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

- Start one Agent call per piece with isolation "worktree". Its prompt: the piece's details from the plan, its note
  below, and "commit on your branch; don't merge".
- Check its diff and the project's tests in its worktree.
- Merge its branch into the current branch with `git merge`, one piece at a time, then start the pieces that waited
  for it.

## Notes per piece (only pieces that need one)

Every piece: run only the tests of what it changes (nextest filters, vitest for touched UI files), `cargo fmt
--check` and clippy `-D warnings` on changed crates; never the full `scripts/check.sh`. Unset `PORT` and every
`OTAQ_*` variable for tests. Scratch tool homes only: never the maintainer's real `~/.codex`, `~/.copilot`,
`~/.claude`, `~/.config/opencode`, `~/.tuq`, user PATH or the installed Otaq. The Windows VM (VMPal "Windows") may be
used without asking, no snapshots; record and restore the user PATH and hash the real tool configs before and after.
Each piece's report lists what it deliberately left unchanged and the tests that prove it.

### Piece 1

Start with the hands-on check; pass its findings on before the agent writes any CLI or prime change. Codex needs a
real sign-in: copy `auth.json` from a scratch Codex home the maintainer signed in (a device code: `codex login
--device-auth`, the maintainer enters it), never move or edit the original. If the check shows the cause is not the
heredoc (the sandbox blocks even a plain `otaq` command, say), stop and report before building. The prime change must
be Codex-only: compare the other tools' prime output before and after.

### Piece 3

The default data folder must behave exactly as today, including on upgrade from 0.2. Watch that the change doesn't
switch off Codex for anyone who never set OTAQ_HOME.

## Bringing it together

After the last merge: run `scripts/check.sh` on the Mac and `scripts/check.ps1` in the Windows VM, and try each
piece's Check: line on the merged branch. Then the regression that proves nothing else changed: one ticket through the
default flow on Claude Code and on Codex (real sign-in, fresh scratch homes, starting with no tuq), and one stage each
on Copilot and OpenCode in the Windows VM, as in the 0.3 bring-together of skills-in-their-own-repos. Expect the
Codex stages to need no permission prompt for `otaq`, `tuq`, `tuq-plan` or `git commit` while `git push` still asks.
Also start the installed Otaq and a dev build with its own OTAQ_HOME side by side and confirm the real `~/.codex`
stays byte for byte as the installed Otaq left it.
