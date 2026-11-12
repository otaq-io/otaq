# 07 · Ctrl+J opens a start-failed or merge-stopped ticket on an empty Session tab

**Kind:** bug · **Release:** to decide · **From:** re-test 8 NEW-b4 · **Decision:** —

## What happens
Ctrl+J (and a chip of the Needs you strip) opens a ticket whose start failed, or whose delivery stopped, on its
Session tab: "No session · NOTE-18 has no agent session yet. It starts when the ticket goes to Dev." (it is in Dev), or
HOOK-4's empty terminal. The reason and Retry / Push to PR are on Overview and the card.

## Repro (fae1fd78)
1. notes board, no sheet open, strip "Interrupted · 1 · NOTE-18 start failed" → Ctrl+J → NOTE-18 opens on **Session**
   ("No session … starts when the ticket goes to Dev").
2. Ctrl+J again → NOTE-20 (Spec review) on Overview (correct); Ctrl+J again → across projects to HOOK-4
   ("Delivery stopped", "the branch isn't on the remote as expected") on **Session**, an empty terminal, "No session
   agent :4002".

## Cause (confirmed by repro + code)
`crates/otaq-view/src/view_models/needs_you_strip.rs:59-76` (`Item::target`): Overview only for the Review group and
for an Asks-you question; every other item lands on `session_tab::lands_on(i.status)` (`session_tab.rs:192-198`),
which is the Agent (or Check) tab for any status, including `StartInterrupted` and `DeliveryStopped` (both listed in
the strip, `needs_you_strip.rs:537-538`). `strip/logic.ts openTarget` opens what Rust says. Chips use the same target.

## Fix direction
In `target()`, send items with nothing to answer in a terminal to Overview: `StartInterrupted`, `DeliveryStopped`
(and a ticket whose `stopped_in_place` is set), probably `ResumeFailed`; keep Session for a live prompt (Needs you,
the checker's prompt, a helper's prompt).

## Risks to check
A delivery stop whose skill session still sits at a prompt (Needs you on top) must still open that terminal;
`Interrupted` tickets whose Resume lives on Session; the strip's "Open in the ticket" in the Asks popover.

## Evidence
`../shots-retest-9/r9-b4-ctrl-j-note18-start-failed-empty-session.jpg`,
`r9-b4-ctrl-j-hook4-delivery-stopped-empty-session.jpg`; first seen `../RESULTS-retest-8-b.md`.
