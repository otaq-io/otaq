# 05 · A long question on Overview still leaves Send below the fold

**Kind:** bug · **Release:** to decide · **From:** re-test 8 NEW-c1 · **Decision:** UX-62

## What happens
UX-62: on a ticket's Overview the question box has no cap of its own and caps "at half the page only when Send would
fall below it", so the options and Send stay in view. On a 908-px window a long question with three noted options and
Something else gets its text capped, but Something else's row is cut at the sheet's bottom and Send is ~36 CSS px
below the fold until the sheet scrolls one notch.

## Repro (fae1fd78, window 1920×908 at 100 %, HOOK-14 idle in Dev)
`otaq ask human` with `r8c-q-long.txt` and the three noted options of `r8c-ask-long.ps1` (recommended 3), asked on
HOOK-14 → open the sheet's Overview: text capped with its own scrollbar, options 1–3 with notes visible, "Something
else" cut, Send and "1–4 pick" not visible; one wheel notch on the sheet brings Send into view (≈24 screenshot px =
36 CSS px).

## Cause (confirmed by repro + code)
`crates/otaq-desktop/ui/src/screens/overview/QuestionText.tsx` `useSendInView` (`:169-181`): once the whole box doesn't
fit (`top + whole > body.clientHeight`), the cap is fixed at `Math.floor(body.clientHeight / 2)` (`:179`) whatever the
rest of the box needs. On this window the sheet body is ≈540 CSS px, so the text gets 270 px; the options with notes,
Something else, the hint/Send bar and the box's padding take ≈255–300 px more, plus the box's offset from the body's
top: ≈575 px in all, ≈36 px past the fold. Nothing re-checks that the capped box fits. (The Edit flow and Noyan boxes
don't have this: their options scroll on their own, NEW-d1/d2.)

## Fix direction
Cap the text at the room left, not at half: `cap = body.clientHeight - top - (box height without the text)`, kept at
`QUESTION_FLOOR` at least; when even the floor doesn't leave room, give the options their own scroll as the Edit flow
box does (UX-62's rule "the options give way, never the question" already exists there). Or decide UX-62's "half" is a
starting cap that shrinks until Send fits.

## Risks to check
ResizeObserver loop (the cap changes the box's height; measure from `scrollHeight` as now); the conflict strip and
meta line above the box change `top`; a very small window (floor + options > body); the sheet expanded (⤢).

## Evidence
`../shots-retest-9/r9-c1-long-question-send-below-fold.jpg`, `r9-c1-one-notch-down-send-in-view.jpg`;
`../RESULTS-retest-9-repro.md`; earlier `../RESULTS-retest-8-c.md`, `../RESULTS-retest-8-fixes2.md`.
