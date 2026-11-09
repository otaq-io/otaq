# More ways to finish a ticket than merge or PR

Agreed with the maintainer on 2026-11-06. The old `decisions/` folder is lost, so this file is the record. Code
comments and tests name each rule in words, never by its number here.

## The problem

The Approve menu (`crates/otaq-view/src/view_models/approve.rs`) offers only **Local merge** and **Open PR** (or
**Push to PR #N** once a PR is open). Two cases don't fit:

- Tickets whose work lives outside the repository (configuration, a skill in the user's scope). There is nothing to
  merge, yet the only ways out are a merge, a PR or **Close ticket**. Close means "abandoned", so tickets waiting on
  it go to *Parent closed*.
- Two tickets that should land together, on one branch and in one PR.

## Decisions

### A1. Done, nothing to merge

When the ticket's branch has no commits beyond its starting point, the Approve menu leads with **Done, nothing to
merge**. It finishes the ticket and counts as merged, so tickets waiting on it start. The usual Done cleanup follows: the
worktree is removed and the branch is kept, as for every finished ticket.
Local merge and Open PR stay in the menu, disabled, with "No changes in the repo".

The diff decides. No ticket field or flow setting marks a ticket as "no repo changes". Uncommitted files can't reach
this point, because ending a stage with uncommitted files is refused and Otaq never commits by itself.

### A2. Done, don't merge, with commits

When the branch has commits, the menu still offers **Done, don't merge**, placed last. A confirmation says what
won't land ("SHOP-18's 3 commits won't be merged; its branch is kept"). After the confirmation it works like A1: the
ticket counts as merged, the worktree is removed and the branch is kept, so the commits can still be recovered.

### A2b. The agent's `stage done` is the same Done

`otaq stage done` (an agent ends a ticket whose branch has no commits) no longer closes the ticket as "Closed ·
nothing to merge". It ends the ticket the way A1 does: finished, counting as merged, so tickets waiting on it start,
with the agent's reason kept as its comment. "Nothing to merge" now means one thing, whoever ends the ticket.

### A3. Merge into another ticket

The Approve menu offers **Merge into another ticket…**, which opens a picker of tickets that are still open and
have a branch. Choosing SHOP-18:

- squashes this ticket's branch into SHOP-18's branch, the same way a Plan's helper merges into its ticket's
  branch;
- shows this ticket as "merged into SHOP-18". It finishes when SHOP-18 lands, and tickets waiting on it start then;
- sends SHOP-18 back to review with the combined diff.

If SHOP-18 is later closed without landing (Close ticket, or Mark closed on its PR), every ticket merged into it
goes back to Ready for Review (the human check before the Delivery) on its own branch, which was kept. From there
it can land alone or be merged into another ticket.

### A4. A combined PR comes from A3

There is no separate "bundle" object or multi-select "Open one PR". The ticket that received the merge opens the PR
as usual, and the PR's description lists every ticket merged into it.
