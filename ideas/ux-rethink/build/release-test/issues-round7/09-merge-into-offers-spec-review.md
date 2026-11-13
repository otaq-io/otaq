# 09 · "Merge into another ticket…" offers a ticket still in Spec review

**Kind:** bug · **Release:** to decide · **From:** re-test 8 NEW-c2 · **Decision:** —

## What happens
The Approve chooser's "Merge into another ticket…" lists a ticket waiting in Spec review (before Dev, no code) as a
host. Merging into it would put this ticket's code on a branch that hasn't been built yet and send that ticket "back to
review".

## Repro (fae1fd78, project notes: Product spec → Tech spec → Milestone plan → Spec review → Dev → Validate → Code review)
NOTE-20 waits in Spec review, "Ready for you", with branch `otaq/NOTE-20`. New NOTE-26 (`otaq create`, spec stages
skipped, Dev = Architect, Validate = Checker) → Code review. Approve → chooser → "Merge into another ticket…" →
picker lists **"NOTE-20 Retest7 sign-in check · Ready for you"**. Cancelled.

## Cause (confirmed by repro + code)
`crates/otaq-core/src/board/merge_into.rs:40-51` `can_take` accepts any host with status `WaitingForYou`, `ToDeliver`
or `PrOpen`, same project/base, with a branch. Spec review is a check of yours, so its ticket is `WaitingForYou`
(`ready_for_review`) and has a branch from the start. `merge_into_hosts` (`:59-71`) only adds "a host at its Delivery
needs a human check before the Delivery"; nothing requires the host to be past its code stages. The module doc says
the same ("waits in a human check"), so the rule was written for flows with one check of yours.

## Fix direction
A host must wait in the human check before the Delivery (`engine::human_check_before_delivery(&flow)` is its stage),
at its Delivery, or have a PR open. Same for `can_be_merged_in` (`:31-35`), which accepts any `WaitingForYou` ticket.

## Risks to check
Flows with several checks of yours after Dev (e.g. QA then Code review): decide whether a ticket in the earlier one
can host. Flows without a human check before Delivery (`back_to_review` false) keep their rule.

## Evidence
`../shots-retest-9/r9-c2-note26-approve-chooser.jpg`, `r9-c2-merge-into-offers-note20-in-spec-review.jpg`;
first seen `../RESULTS-retest-8-c.md`.
