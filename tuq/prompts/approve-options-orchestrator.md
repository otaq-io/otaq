# Orchestrator · approve-options

- **Ticket:** none
- **Date:** 2026-11-06
- **Plan:** tuq/plans/approve-options.md
- **Tech spec:** none (ideas/approve-options/DECISIONS.md and the code the plan names)
- **Decisions:** A1, A2, A2b, A3, A4 in ideas/approve-options/DECISIONS.md

approve-options adds more ways to finish a ticket from the Approve menu: Done without merging, and merging one ticket
into another so they land (or open one PR) together.

You are the agent in charge of building approve-options. You don't build the pieces: you start one agent per piece, check
what it hands back and merge it. Read the plan first; each piece's details there are its brief.

## Run order

<!-- tuq:order:start -->

Start a piece as soon as every piece it waits for is merged:

- First: 1 A user finishes a ticket with nothing to merge, 4 A user merges one ticket into another ticket's branch (side by side)
- After 1, 4: 2 A user finishes a ticket without merging its commits, 3 An agent's `otaq stage done` finishes the ticket as Done, 5 Tickets merged into a host finish when the host lands, 6 Tickets merged into a closed host go back to review, 7 A host's PR lists the tickets merged into it (side by side)

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

Pass these to every piece: the repo's rules are in CLAUDE.md, and the old `decisions/` folder is lost. Don't look
for it or write to it, and treat decision numbers in code comments (d0108…) as history. New comments name a rule in
words ("Done, nothing to merge counts as merged"), never by A1–A4. Never change the main checkout.

### Piece 1

The new outcome is the groundwork pieces 2, 3 and 5 build on. Before merging, check that it is one named value
(not a string repeated across files), that readiness needs no change, and that the Done cleanup keeps the branch.
If the outcome column has a CHECK constraint or an enum in the store, the migration belongs in this piece.

### Piece 4

Pieces 1 and 4 both edit approve.rs's `options`, ApproveChoice.tsx and Board.tsx. Merge 1 first if both are done,
and have 4 rebase. The picker's rules (no agent running on the host, same base, refusal on conflict) were not
discussed with the maintainer; they are the plan's choice. If the helper finds a reason they can't hold, stop and
ask rather than widen them.

### Piece 6

"Back to Ready for Review" means the human check before the Delivery on whatever flow the ticket uses, never a stage
named in code. Watch that it recreates a worktree only from the kept branch, and never touches the host's.

## Bringing it together

After the last merge: run the project's full tests, try each piece's Check: line on the merged branch, and
run `scripts/check.sh` once on the merged branch. Then walk the whole story in the app on a scratch project:
- a ticket with no commits ends with Done, nothing to merge, and the ticket waiting on it starts;
- an agent's `otaq stage done` does the same;
- a ticket with commits ends with Done, don't merge, and its branch still holds them;
- SHOP-20 is merged into SHOP-18, SHOP-18 is Local merged, and both are Merged;
- a second pair where the host is closed puts the merged-in ticket back in review.
