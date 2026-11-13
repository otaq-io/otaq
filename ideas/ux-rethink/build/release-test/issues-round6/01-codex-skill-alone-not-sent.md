# 01 · A skill Otaq types on its own into Codex isn't sent

**Kind:** bug · **Release:** must fix · **From:** re-test 6 NEW-5 (`../RESULTS-retest-6.md`), the hooks re-test

## What happens
- Push to PR / Retry push type `$otaq:push`, a conflict restack types `$otaq:restack`, with nothing after them. The
  text lands in Codex's input line and stays there; the Push dialog waits on "Push skill is preparing…" (2 of 2).
- A restack carrying text (a failed merge check) is sent (re-test 6 check 1).
- The stuck line also makes the line read as holding text, so every later automatic message waits behind it.

## Cause (confirmed on real Codex 0.166.0, Windows VM, by a throwaway ConPTY probe)
A bare `$name` leaves Codex's skill popup open; Enter in that popup inserts the item ("enter insert · esc close") and
submits nothing. Text after the name closes the popup first. Probe: `$otaq:push` → not sent; `$otaq:push ` (one
trailing space) → sent, the skill loaded and ran `otaq prime`. Not ConPTY, timing, paste or length. Every automatic
path types through `type_and_submit` (crates/otaq-core/src/session/deliveries.rs); bare skills come from
`Deliveries::skill_text` with `told = None` (push: pr/service.rs, conflict restack, review/apply_now.rs
`address-review`, start/sequence.rs's queued hand-off). Codex's popup logic isn't OS-specific: macOS/Linux likely the
same (untested).

## Fix direction
A Codex input quirk: a trailing space after the typed text, in the same write, before the one Enter
(`InputQuirks`, agent_tools/mod.rs; set in agent_tools/codex/mod.rs; other tools unchanged). Rejected: a second Enter
(may land on a permission prompt and pick its default), Esc.

## Risks to check
- A multi-line (bracketed-paste) text with the space after it: untested on real Codex.
- A stuck line ending in a space must still read as text (`has_unsent_input`, input_line.rs).
- Exactly one Enter: no empty turn, no prompt answered.

## Evidence
`../shots-retest-6/bug-otaq-push-typed-not-submitted-20s.jpg`; the probe's source: `probe-new5/` beside this file (VM copy in `%USERPROFILE%\otaq-build\new5`).
