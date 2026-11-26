# Orchestrator · slot-between-stages

- **Ticket:** OTAQ-8
- **Date:** 2026-11-25
- **Plan:** tuq/plans/slot-between-stages.md
- **Tech spec:** tuq/tech-specs/slot-between-stages.md
- **Decisions:** 114, 119, 120

With the agent limit on, a ticket keeps its slot between two Agent stages and, after one of your checks, goes ahead of
the queue (OTAQ-8).

You are the agent in charge of building slot-between-stages. You don't build the pieces: you start one agent per piece, check
what it hands back and merge it. Read the plan first; each piece's details there are its brief.

## Run order

<!-- tuq:order:start -->

Start a piece as soon as every piece it waits for is merged:

- First: 1 Every queued ticket stores how it came to wait
- After 1: 2 A ticket keeps its slot between two Agent stages, 3 After your check, a ticket goes ahead of the queue (side by side)
- After 2: 4 The Running popover shows a ticket handing over

<!-- tuq:order:end -->

## Running a piece

- Start one Agent call per piece with isolation "worktree". Its prompt: the piece's details from the plan, its note
  below, and "commit on your branch; don't merge".
- Check its diff and the project's tests in its worktree.
- Merge its branch into the current branch with `git merge`, one piece at a time, then start the pieces that waited
  for it.

## Notes per piece (only pieces that need one)

### Piece 1

Watch for a queue write the piece misses: `grep -rn "queued_at" crates/otaq-core/src/` on its branch should show
every write going through `set_queued!()`. Its migration number is 0129; if main gained a 0129 meanwhile, take the
next free one.

### Piece 2

The OTAQ-8 repro test must fail on piece 1's branch alone: ask the helper for the failing run before the change.

### Piece 4

Run otaq-view's tests too, not only otaq-core's.

## Bringing it together

After the last merge: run the project's full tests, try each piece's Check: line on the merged branch, and
check the two decisions together: with Max 1, a ticket A in Tech spec, an older ticket B waiting to begin and a
ticket C in Review, A's `stage next` goes on to Milestone plan with the count at 1 and the popover saying "starting
Milestone plan"; then sending C back to Dev while A runs shows C first in line, ahead of B, and C starts when A's
slot frees.
