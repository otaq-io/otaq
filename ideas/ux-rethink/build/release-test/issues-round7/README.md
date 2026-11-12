# Round 7 · issues left after re-test 8

Left open by the Windows regression test of release-fixes (re-test 8, builds 0cc7efc5 → 4147a741 → fae1fd78;
`../RESULTS-retest-8-a.md` … `-e.md`, `-fixes.md`, `-fixes2.md`, screenshots `../shots-retest-8/`). Logged 2026-11-12.
All reproduced in the VM on fae1fd78 and each cause found (`../RESULTS-retest-9-repro.md`, `../shots-retest-9/`); one
file per issue below. Which of these block the release is still to be decided.

## Claude Code on Windows
- [01](01-claude-code-long-message-cut.md) · NEW-12 (from re-test 7) · A long message Otaq types into Claude Code loses its first 1,024 bytes, so the
  merge-message writer's request arrives cut and the written message never comes (Approve stays "still writing").
  Logged as a blocker. Cause found (not Windows-only): Claude Code takes a read over 800 characters as a paste and
  the shorter read after it replaces the line; fix by bracketing long one-line messages too. Evidence: `../RESULTS-retest-7.md`, "Follow-up · Claude Code on Windows";
  `../issues-round6/README.md`.
- [02](02-ctrl-v-unsent-text.md) · NEW-10 (from re-test 7) · Ctrl+V pastes nothing into Claude Code. Decision 048 settles the behaviour;
  round 6 piece 8's fix is on branch `round6-piece8-ctrl-v-paste` (04d9543e), unverified and not merged.
  - NEW-e1 goes with it · after that empty paste, Send back says "HOOK-10 has unsent text in its terminal" over an
    empty input line (the raw Ctrl+V likely counts as typed input). `../RESULTS-retest-8-e.md`

## Question boxes
- [05](05-long-question-send-below-fold.md) · NEW-c1 · On a ticket's Overview (908-px window), a long question with three noted options + Something else leaves
  Send ~36 CSS px below the fold; UX-62 says the options and Send stay in view. Unchanged on fae1fd78.
  `../RESULTS-retest-8-c.md`, `../RESULTS-retest-8-fixes2.md`
- [03](03-something-else-enter-sends-twice.md) · NEW-f1 · Answering with "Something else" + text and ↵ on Overview delivers the answer once, and also shows "The
  answer to HOOK-13 wasn't sent · HOOK-13 isn't waiting for an answer" (looks like a second send after the first
  closed the question). Edit flow's Send answer (click) gave no toast. `../RESULTS-retest-8-fixes2.md`
- [04](04-answer-picks-reset-on-tab-switch.md) · Ticks and picks reset to the recommended ones after leaving Overview (a tab click) and coming back; seen in the
  single-answer box too. `../RESULTS-retest-8-b.md` (notes), `../RESULTS-retest-8-fixes2.md` NOTE-f1

## Board and ticket
- [06](06-ask-human-in-review-shown-nowhere.md) · NEW-b2 · `otaq ask human` from a ticket in Review (Dev session idle, merge-message writer started) is accepted and
  shown nowhere: no card change, strip chip, Overview box, History or `otaq show`. That suspected cause was wrong; see
  06. Also seen with `--send-back Dev` (`../RESULTS-retest-8-fixes.md`). `../RESULTS-retest-8-b.md`
- [07](07-ctrl-j-opens-empty-session.md) · NEW-b4 · Ctrl+J (and the strip) open a start-failed or merge-stopped ticket on an empty Session tab; the reason and
  Retry are on Overview. Expected: Overview when there's no live prompt. `../RESULTS-retest-8-b.md`
- [08](08-done-without-merge-reads-merged.md) · NEW-b5 · A ticket closed with nothing to merge ("Done · nothing to merge" on its card) reads "Merged" in its sheet's
  pill, in `otaq list`, and for "Done, don't merge" (HOOK-12). `../RESULTS-retest-8-b.md`, `-c.md`, `-d.md`, `-e.md`
- [09](09-merge-into-offers-spec-review.md) · NEW-c2 · "Merge into another ticket…" offers a ticket still in Spec review (before any code). Expected: only tickets
  waiting in the final review (or with a PR open). `../RESULTS-retest-8-c.md`
- [10](10-noyan-switcher-lists-ended-twice.md) · Noyan's conversation switcher lists an ended conversation twice: in the top list and under RECENTLY ENDED.
  `../RESULTS-retest-8-b.md`
