# 06 · With the terminal focused, the card says Working while the agent waits

**Kind:** design · **Release:** decide · **From:** re-test 5 NOTE-2

## What happens
- With the session's terminal focused ("Typing goes to the agent · you're here, so no alert") and a Codex prompt on
  screen, the card read "Working" while the sheet's pill read "Needs you", 12 s+, until focus left the terminal.
  No alert while you're there is by design; the card saying Working for a waiting agent may not be.

## Evidence
Screenshots in `../shots-retest-5/`: `c29-focused-card-working-pill-needs-you.jpg`

## Where to start
The "you're here" focus rule (attention/focus.rs) and what it suppresses (alert only, or the card's state too).

## Open questions
- Should focus only hold back the alert, and the card still say Needs you?
