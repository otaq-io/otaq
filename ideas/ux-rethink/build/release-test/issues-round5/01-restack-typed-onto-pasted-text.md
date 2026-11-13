# 01 · A restack request is typed onto pasted unsent text after one Backspace

**Kind:** bug · **Release:** must fix · **From:** re-test 5 FAIL of round-4 piece 26 (`../RESULTS-retest-5.md` §26, NEW-1)

## What happens
- SHOP-32's Dev Codex session had an unsent line pasted into it (Ctrl+V in the app's terminal: "unsent draft line left
  by the tester retest5"). A failed merge check queued the restack request, which waited 15 s+ with the text intact.
- The tester then sent Backspace 50 times. After the **first** one, Otaq typed `$otaq:restack …` after the rest of the
  text and Codex got "…tester retest$otaq:restack SHOP-32's merge check failed…" as one message.
- Piece 26's agent had concluded Otaq can't type over text (its test types "half-typed" byte by byte, then deletes),
  and blamed Ctrl+C for re-test 2's NEW-3. This shows a real path: a paste, then one delete.

## Evidence
Screenshots in `../shots-retest-5/`: `c26-unsent-text-before-approve.jpg`, `c26-restack-waits-text-intact-15s.jpg`,
`c26-FAIL-restack-typed-after-one-backspace.jpg`, `c26-FAIL-sent-as-one-message.jpg`

## Where to start
The session's unsent-input tracking (`has_unsent_input`, crates/otaq-core/src/session/), how a pasted chunk (and
bracketed paste) is counted, and the screen check deliveries skip once the tracker says empty
(crates/otaq-core/src/session/deliveries.rs, service/session_requests.rs). Test: the piece-26 test in
crates/otaq-core/tests/suite/merge_check.rs with the text sent as one paste.

## Open questions
- Should the screen read always decide (never the keystroke count alone) before Otaq types into a line?
