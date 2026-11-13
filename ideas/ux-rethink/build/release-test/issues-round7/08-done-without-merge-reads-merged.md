# 08 · "Done · nothing to merge" and "Done, don't merge" read "Merged" in the sheet and `otaq list`

**Kind:** bug · **Release:** to decide · **From:** re-test 8 NEW-b5 · **Decision:** —

## What happens
A ticket finished without a merge shows "Done · nothing to merge" (or "Done · not merged") on its card, but its sheet's
pill and the line under the title read "Merged", and `otaq list` / `otaq show` say "Merged".

## Repro (fae1fd78)
- SHOP-39 (`stage done`, "Question-box test only; nothing to change."): card "Done · nothing to merge"; sheet pill
  **Merged**, subtitle **Merged**.
- `otaq list`: SHOP-39, SHOP-43, SHOP-44, NOTE-24 (`stage done`) and HOOK-12 ("Done, don't merge") all read
  "Done Merged".
- DB: all five have `state = 'merged'`, `outcome = 'done_without_merge'` (SHOP-39/43/44/NOTE-24 with
  `nothing_to_merge` set, HOOK-12 without).

## Cause (confirmed by repro + DB + code)
These tickets are stored as status Merged with `outcome = done_without_merge` (they "count as merged" so dependants
start). Only the card looks at the outcome (`otaq-view/src/view_models/card.rs:1287-1299`, `flags.done_without_merge`
→ "Done · nothing to merge" / "Done · not merged"). The others word the bare status:
- sheet pill: `otaq-desktop/src/api/sheet.rs:734` `PillView::of(status::shown(ticket))` → `Status::Merged.label()`
  "Merged" (`otaq-view/src/theme.rs:617`);
- sheet subtitle: `otaq-view/src/view_models/sheet.rs:280-281` `ticket.status.label()` once in Done;
- CLI: `otaq-cli/src/read.rs:509-516` `state_words` → `status_words(Merged)` → "Merged".

## Fix direction
One helper for a finished ticket's words (from `flags.done_without_merge` / `nothing_to_merge`, as the card does) used
by the card, the sheet pill and subtitle, History's header if it has one, and `state_words` in the CLI.

## Risks to check
`otaq list --status merged` / "Merged" filters (decide whether "Done · nothing to merge" matches); the Done column's
counts and "Merged" subtitle; tickets landed with a host (`record_landed_with` copies the host's outcome).

## Evidence
`../shots-retest-9/r9-b5-shop39-card-nothing-to-merge-sheet-pill-merged.jpg`; `otaq list` and DB rows in
`../RESULTS-retest-9-repro.md`; first seen `../RESULTS-retest-8-b.md`.
