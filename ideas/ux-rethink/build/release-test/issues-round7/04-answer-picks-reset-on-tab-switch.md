# 04 · An answer box's picks and ticks reset when you leave Overview and come back

**Kind:** bug · **Release:** to decide · **From:** re-test 8 notes (part b), NOTE-f1 · **Decision:** —

## What happens
On a ticket's Overview, changing the pick (single answer) or the ticks (several answers), then clicking another sheet
tab and back to Overview, brings the box back on the recommended option(s). Something else's typed text is lost too
(code).

## Repro (fae1fd78, HOOK-14 idle in Dev, questions asked by CLI)
1. Single answer ("R9 f1 one", Alpha recommended): click Beta → Changes tab → Overview → **Alpha picked again**.
2. Several answers (`r8c-q-multi.txt`, `--multiple`, recommended 1,3): untick Unit tests, tick Lint (2 and 3 ticked) →
   Session tab → Overview → **1 and 3 ticked again**.

## Cause (confirmed by repro + code)
The picks live in React state of the Overview's answer box, and the Overview unmounts when another tab shows:
- `screens/sheet/Sheet.tsx:252` renders only the shown tab's body (`{Body && <Body ticket={view.key} />}`).
- `screens/overview/NeedsYou.tsx:93` `useAnswerPicks` starts from `initialPicks(answer.rows)` (the recommended ones)
  on every mount; Something else's text is `useState("")` in `AnswerBox` (`:304`).
So every return to Overview is a fresh box. Noyan's panel (`agent_panel/AgentQuestion.tsx:58`) keeps its picks the
same way and would reset whenever it unmounts.

## Fix direction
Keep the draft answer per question outside the tab body: a small store keyed by `(ticket, question id)` holding picks
and Something else's text (like the description's draft in `overview/draft.ts`), read by `useAnswerPicks`, cleared
when the question closes or is replaced.

## Risks to check
A new question (different id) must start from its own recommended options; a replaced question must not inherit the old
picks; the card's option click (`fromCard`) still picks; drafts don't leak across tickets.

## Evidence
`../shots-retest-9/r9-reset-single-beta-picked.jpg`, `r9-reset-single-after-changes-and-back-alpha-again.jpg`,
`r9-reset-multi-ticked-2-3.jpg`, `r9-reset-multi-after-session-and-back-1-3-again.jpg`; `../RESULTS-retest-9-repro.md`.
