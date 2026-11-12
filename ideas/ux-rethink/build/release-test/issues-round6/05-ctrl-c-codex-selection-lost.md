# 05 · In a Codex terminal on Windows, a Shift+drag selection is lost before Ctrl+C

**Kind:** bug · **Release:** must fix · **From:** re-test 6 NEW-1 (round 5 piece 8) · **Decision:** 042

## What happens
In a Codex session, Shift+drag selects; Ctrl+C then sends the interrupt and Codex clears the draft (2 of 2);
Ctrl+Shift+C copies nothing. A plain drag works only because Codex copies its own selection. Shells are fine.

## Cause (confirmed in the app, VM, d0a165b5)
Piece 8's key handling works (a Shift-drag + Ctrl+C on a read-only replay copied). The selection is gone before the
key: on Windows Codex's mouse capture becomes any-motion tracking (DECSET 1003 via ConPTY); xterm.js 6 reports every
buttonless mousemove as user input (`CoreMouseService.triggerMouseEvent` → `triggerDataEvent(report, true)`), and
`SelectionService` clears the selection on user input. The first move after releasing the drag clears it (seen ~0.4 s
after release as a `MOUSE btn=0` input record in a test script). Ctrl+Shift+C never worked over such a program.

## Fix direction
On Windows, a capture-phase `mousemove` listener on the terminal element that stops xterm's report only when a
selection exists, `term.modes.mouseTrackingMode === "any"` and no button is held (crates/otaq-desktop/ui/src/terminal/
Terminal.tsx, a predicate in state.ts). Drags and shells (mode none) untouched. Not an xterm patch.

## Risks to check
Link hover/Ctrl+click while a selection shows (links.ts listens on the host: stop only xterm's report or re-dispatch);
the find bar's selected match now persists and Ctrl+C copies it; select-then-interrupt needs a click or Esc first
(decision 042 as intended). macOS has the same latent bug with Option-drag (crossterm sends ?1003h); 042 keeps macOS
unchanged, so it's a separate decision.

## Tests
The vitest stand-in hid this (its selection never clears on input): give it xterm's contract (mouse mode, a mousemove
that reports and clears). jsdom has no cell metrics, so real coverage is a real browser or the VM: in a live Codex
session Shift-drag, move the mouse away, Ctrl+C and Ctrl+Shift+C copy and the draft stays; PowerShell copy and interrupt
still work.

## Evidence
`../shots-retest-6/c8-FAIL-ctrl-c-wiped-draft-not-copied.jpg`, `c8-FAIL-retry-shift-selection-ctrl-c-wiped-zoom.jpg`,
`c8-ctrl-shift-c-selection-gone-not-copied-zoom.jpg`
