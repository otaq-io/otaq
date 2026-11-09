# Plan · approve-options

- **Ticket:** none
- **Date:** 2026-11-06
- **Decisions:** A1, A2, A2b, A3, A4 in ideas/approve-options/DECISIONS.md
- **Tech spec:** none (no tech spec was written; ideas/approve-options/DECISIONS.md and the code named below)
- **Orchestrator prompt:** tuq/prompts/approve-options-orchestrator.md

<!-- tuq:plan:start -->

More ways to finish a ticket than Local merge or Open PR, following ideas/approve-options/DECISIONS.md (A1–A4). There
are seven pieces, one behaviour each. Piece 1 builds "Done, nothing to merge" from the Approve menu, and with it the
new Done outcome that counts as merged. Pieces 2 and 3 reuse that outcome (with commits, and from the agent's
`otaq stage done`) and run side by side. Piece 4, merging one ticket into another, waits for nothing and runs beside
piece 1. Pieces 5, 6 and 7 build on 4 (what happens when the host lands, when it is closed, and its PR description)
and run side by side. The agent in charge follows tuq/prompts/approve-options-orchestrator.md.

## Pieces

1. A user finishes a ticket with nothing to merge — waits for nothing
   Decision A1. When the ticket's branch has no commits beyond its base, the Approve menu
   (crates/otaq-view/src/view_models/approve.rs, `options`) leads with a new `ApproveChoice::Done`, "Done, nothing
   to merge". Local merge and Open PR stay in the menu, disabled, with the detail "No changes in the repo". Choosing
   it finishes the ticket as Done with a new outcome that counts as merged: `Status::Merged` (so
   crates/otaq-core/src/board/readiness.rs starts waiting tickets with no change there), with an outcome that isn't
   'merged' and no merge commit. Model the bookkeeping on `Board::record_local_merge`
   (crates/otaq-core/src/board/merge.rs): the `Move::Delivered` move, a reviews row, a History event. The usual Done
   cleanup follows (crates/otaq-core/src/review/finish.rs): the worktree is removed and **the branch is kept**. The
   card and History read "Done · nothing to merge" (crates/otaq-view/src/view_models/card.rs, history.rs). Wire it
   end to end: a `review.done` request in crates/otaq-desktop/src/api/review.rs, the choice in
   crates/otaq-desktop/ui/src/screens/board/ApproveChoice.tsx, Board.tsx and screens/review/logic.ts. Run the ts-rs
   export so ui/src/generated is up to date. One behaviour across core, view model, desktop API and UI, on purpose.
   Check: a test drives a ticket with an empty branch through Done. It ends Merged with the new outcome, its branch
   still exists, its worktree is gone, and a ticket that depended on it becomes ready. A view-model test shows the
   menu with Done first and the other two disabled.
2. A user finishes a ticket without merging its commits — waits for 1
   Decision A2. When the branch has commits, the Approve menu also offers "Done, don't merge", placed last after
   Local merge and Open PR. Choosing it opens a confirmation that names what won't land: "SHOP-18's 3 commits won't
   be merged; its branch is kept" (singular for one commit). On confirm it ends exactly like piece 1's Done
   (same outcome, counts as merged, branch kept). The card and History read "Done · not merged" so it isn't
   mistaken for piece 1's empty case. Not offered on a ticket with an open PR (there the menu stays Push to PR #N).
   One behaviour across the view model, desktop API and the UI confirmation dialog
   (crates/otaq-desktop/ui/src/screens/review/Dialogs.tsx), on purpose.
   Check: with a 2-commit branch, the menu lists Done, don't merge last. Confirming ends the ticket Merged with no
   merge commit, the branch still holds both commits, and dependent tickets start.
3. An agent's `otaq stage done` finishes the ticket as Done — waits for 1
   Decision A2b. `Review::stage_done` (crates/otaq-core/src/review/close.rs) ends the ticket with piece 1's Done
   outcome instead of Closed · not merged. The agent's reason stays its comment and the `nothing_to_merge` reason
   (crates/otaq-core/src/domain/flags.rs). The refusal when the branch has commits is unchanged. The card and
   History read "Done · nothing to merge" (card.rs:1176, history.rs:1172 and :2437). Tickets that were already
   closed this way stay as they are. Update every text that calls it "Closed": the reply in
   crates/otaq-core/src/board/review.rs (`stage done`'s answer), the CLI help in crates/otaq-cli/src/cli.rs (lines
   274, 385) and agent.rs:379, crates/otaq-core/src/prime.rs, and the skills that mention `stage done` under
   plugin/skills/ (work-ticket, product-spec, tech-spec).
   Check: crates/otaq-core/tests/suite/hand_overs.rs's `stage done` test now expects Merged with the new outcome,
   and a new test shows a ticket depending on it becoming ready.
4. A user merges one ticket into another ticket's branch — waits for nothing
   Decision A3. The Approve menu offers "Merge into another ticket…" (not on a ticket with an open PR), which opens
   a picker. It lists tickets of the same project and base that aren't Done, have a branch, and have no agent running
   (waiting in a human check, in the Delivery, or with a PR open). Choosing SHOP-18 does the following:
   - Refuse while either worktree has uncommitted files.
   - Rebase this ticket's branch onto SHOP-18's branch, then squash-merge it into SHOP-18's branch in SHOP-18's
     worktree as one commit named after this ticket. Reuse the helper merge's git steps
     (crates/otaq-core/src/helpers/merge.rs, crates/otaq-core/src/git/merge.rs). A conflict puts everything back and
     merges nothing, with a message saying so, and the ticket stays where it was.
   - This ticket waits in a new status, "Merged into SHOP-18". It isn't Done, so tickets waiting on it keep waiting,
     and its worktree is removed while its branch is kept.
   - SHOP-18 goes back to its review (the human check before its Delivery, Ready for Review on the default flow) with
     the combined diff and a History line naming the ticket merged in. On a PR-open host, Push to PR then carries
     the new commit.
   Store which tickets were merged into which, for pieces 5–7. One behaviour across core, store, view model,
   desktop API and the picker UI, on purpose.
   Check: a test merges SHOP-20 into SHOP-18. SHOP-18's branch has one new squash commit with SHOP-20's changes,
   SHOP-20 shows Merged into SHOP-18 with its branch kept, and SHOP-18 is back in review. A conflicting pair merges
   nothing.
5. Tickets merged into a host finish when the host lands — waits for 4
   Decision A3. When the host becomes Merged (Local merge, the automatic Delivery, or Mark merged in
   crates/otaq-core/src/review/pr_state.rs), every ticket merged into it becomes Merged too. That means Done ·
   Merged, with the host's merge commit and a History line "Landed with SHOP-18", and the Done cleanup. Tickets
   waiting on them then start through readiness as usual. A host that ends with piece 1 or 2's Done (not merged)
   counts the same way, since that Done counts as merged.
   Check: SHOP-20 merged into SHOP-18 and SHOP-21 depending on SHOP-20. Local merge SHOP-18: SHOP-20 is Merged, and
   SHOP-21 becomes ready.
6. Tickets merged into a closed host go back to review — waits for 4
   Decision A3. When the host is closed without landing (Close ticket in crates/otaq-core/src/review/close.rs, or
   Mark closed in pr_state.rs), every ticket merged into it returns to Ready for Review (the human check before its
   Delivery) on its own kept branch. Its worktree is recreated from that branch, as Reopen does for a closed ticket.
   Each gets a History line "SHOP-18 was closed; back to review". From there it can land alone or be merged into
   another ticket.
   Check: SHOP-20 merged into SHOP-18. Close SHOP-18: SHOP-20 is back in review with a worktree on its branch, and
   its Approve menu works again.
7. A host's PR lists the tickets merged into it — waits for 4
   Decision A4. When the host opens a PR or pushes to one, the Open PR and Push skills get the merged-in tickets
   (key, title, description) through what the Delivery's agent reads (`otaq prime`, crates/otaq-core/src/prime.rs).
   plugin/skills/open-pr and plugin/skills/push tell the agent to list each one in the PR description. There is no
   bundle object and no multi-select Open PR.
   Check: a prime test for a host with SHOP-20 merged into it shows SHOP-20 in its delivery context, and the open-pr
   skill text says to list the merged-in tickets.

<!-- tuq:plan:end -->
