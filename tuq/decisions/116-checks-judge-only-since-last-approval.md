# 116 · A check judges only what changed since the user last approved

- **Date:** 2026-11-16
- **Status:** Accepted
- **Related:** 071, 081, 098, 113

## Summary

Work the user approved at an approval step of the flow counts as approved: a
later check (Validate, Code review, a check with the user's own skill) judges only the changes made after that
approval. Otaq treats no stage or skill specially to get there; the flow's own review steps decide.

## Decision

- **Approval steps are one kind:** Spec review, Review and any other stage where the user approves are the same kind
  of step, asking the user for an approval; an approval at any of them counts.
- **What a check judges:** the ticket's changes since the most recent approval by the user at an approval step, not the whole branch against its base. With no such approval yet, it judges the whole branch, as today.
- **What it still sees:** the earlier, approved changes stay readable as context (what the plan said, the specs), but
  they are not in scope: a check never fails a ticket for something the user already approved.
- **Nothing is special-cased:** no rule about tuq, its decisions pointer, `tuq/` files or any named stage. If the user
  doesn't want something an earlier stage wrote (tuq's README pointer, say), they send it back at that review step.
- **tuq unchanged:** its decisions pointer keeps going to README.md, CLAUDE.md and AGENTS.md where they exist (071).

## Why

Found in the 0.3 Windows regression test (F-E1-3): on the Spec first flow, Product spec added tuq's pointer to the
README, the user approved it at Spec review, and Validate after Dev failed the ticket for it as out of scope. The user:
Otaq should never do anything specific to a stage; it's skills, and a review step is where the user approves. Turned
down: tuq writing the pointer only to CLAUDE.md/AGENTS.md (a tuq release, and it fixes only this one case), and
teaching Otaq's Validate about tuq's pointer.
