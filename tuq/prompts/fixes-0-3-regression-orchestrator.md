# Orchestrator · fixes-0-3-regression

- **Ticket:** none
- **Date:** 2026-11-16
- **Plan:** tuq/plans/fixes-0-3-regression.md
- **Tech spec:** none (investigations and plan reviews in ideas/ux-rethink/build/release-test/0.3/, by absolute path)
- **Decisions:** 113, 114, 116, 117, 118; refined 081, 108, 110

fixes-0-3-regression fixes what the 0.3 Windows regression test found, before Otaq 0.3, with no side effects on
anything else. Build on branch `fixes-0-3-regression` (from main 78e23752); every piece's worktree needs
`git submodule update --init` (`plugin/skills` is the otaq-skills submodule). You are the agent in charge: you don't
build the pieces; you start one agent per piece, check what it hands back and merge it. Read the plan first; each
piece's details there, its investigation and its review are its brief.

## Run order

<!-- tuq:order:start -->

Start a piece as soon as every piece it waits for is merged:

- First: all 21 pieces wait for nothing; run at most five at a time on the Mac (disk and CPU), starting with 5, 1, 7,
  2 and 3 (the largest and the ones the Windows check needs most). Piece 7 starts only after its investigation
  (`investigate-116-checks-since-approval.md`) says it isn't large; that investigation doesn't exist yet, so the
  orchestrator first starts one read-only agent to write it (it takes one of the five places).
- Piece 10 starts after piece 1 is merged (both change session screen reading and need the VM).
- The Windows VM is used by one piece at a time, in the order 1, 10, 21: the orchestrator schedules it, and a VM
  piece's prompt says the VM is its alone while it runs (an agent can't stop to ask).

<!-- tuq:order:end -->

## Running a piece

- Start one Agent call per piece with isolation "worktree". Its prompt: the piece's details from the plan, the paths
  of its investigation and review with the exact review section (the reviews number pieces as the plan's first
  draft did: plan 10–15 = review B §14, 11, 12, 13, 15, 16; plan 16–21 = review C §17–22; review A §7 is decision
  115, not piece 7, and review C §10 was replaced by decision 116, so piece 7 has no review), its note below, and
  "commit on your branch; don't merge; don't push".
- Where merges overlap (1/5/10 session reading; 2/3 Copilot start; 11/19 Needs you; 15/20 PR dialog; 6/14 protocol
  types; CHANGELOG), resolve by hand and regenerate ts-rs types and the protocol snapshot rather than merging them.
- Check its diff against the plan and the review, and the tests it ran, in its worktree.
- Merge its branch into `fixes-0-3-regression` with `git merge`, one piece at a time; delete the worktree's `target/`
  afterwards to keep the disk free.

## Notes per piece (only pieces that need one)

Every piece: run only the tests of what it changes (nextest filters, vitest for touched UI files), `cargo fmt
--check` and clippy `-D warnings` on changed crates, and the ts-rs freshness / protocol snapshot tests when it touches
view models or protocol types; never the full `scripts/check.sh`. Unset `PORT` and every `OTAQ_*` variable for tests.
Name the product through `naming` where the repo's naming_literals test requires it. Scratch tool homes only: never
the maintainer's real `~/.codex`, `~/.copilot`, `~/.claude`, `~/.config/opencode`, `~/.tuq`, user PATH or the
installed Otaq. The Windows VM (VMPal "Windows") may be used without asking, no snapshots; never touch
`%USERPROFILE%\.otaq`, `%USERPROFILE%\.codex` or the normal Otaq's processes there; the screen is shared, so only one
piece uses it at a time (ask the orchestrator). Each piece's report lists what it deliberately left unchanged and the
tests that prove it, and anything in its review it didn't follow, with why.

### Piece 1

Capture the extra hidden-cursor screens (Copilot `/model`, Claude Code `/resume` or another unnumbered picker,
OpenCode's permission prompt) in the Windows VM with the recorder in `investigate-1-captures/conrec.ps1` before
changing the rule.

### Piece 5

The largest piece. The skill text goes on a branch in the otaq-skills submodule (`plugin/skills`); commit there, then
commit the submodule pointer here. Don't push the submodule; the maintainer pushes both. A linked worktree keeps its
submodule's git data under `.git/worktrees/<name>/modules/`, so before merging the orchestrator fetches the piece's
submodule branch into the main `plugin/skills` (`git -C plugin/skills fetch <worktree>/plugin/skills <branch>:<branch>`)
and runs `git submodule update` after the merge; the worktree is never removed before that fetch.

### Piece 21

Build the NSIS installer in the Windows VM (`%USERPROFILE%\otaq-build\rel03` has the build setup; see
`otaq-build\rel03-build.ps1`) and run the install paths there, against a scratch install folder where possible, and
put the test install back to the state the regression left (0.3 build installed) afterwards.

## Bringing it together

After the last merge: `scripts/check.sh` on the Mac (Node 24 on PATH) and `scripts/check.ps1` in the Windows VM.
Then rebuild the installer in the VM, install it over the regression's test data (`%USERPROFILE%\otaq-reltest\home`)
and re-run the failed checks from the regression (each finding's steps in its RESULTS file), plus a short regression:
one ticket through the default flow on Claude Code, one on Copilot (answering a question with its terminal
unfocused), a Spec first ticket through Validate, and a check stage with the user's own skill.
