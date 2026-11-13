# 02 · Ctrl+V pastes nothing into Claude Code, then Otaq thinks the empty line holds unsent text

**Kind:** bug · **Release:** to decide · **From:** re-test 7 NEW-10, re-test 8 NEW-e1 · **Decision:** tuq 048 (NEW-10)

## What happens
On Windows, Ctrl+V in a Claude Code session pastes nothing (NEW-10). Afterwards Otaq treats the session's input line as
holding unsent text although it is empty: an answer sent from Overview, Send back, Deliver now and Otaq's own notes
stop at "HOOK-14 has unsent text in its terminal · Clear it and send / Cancel", and Otaq's stage reminder waits
("1 message waiting · Deliver now", "Otaq's notes only arrive when the agent is idle and your input line is empty")
(NEW-e1). It only shows while Claude draws a bare `›` with nothing after it: while Claude shows its faded suggestion
("Answer to your question: Go on") the same state reads as empty.

## Repro (fae1fd78, 2026-11-12 18:39–18:46, HOOK-14, Dev = Architect · Claude Code, idle)
1. Clipboard set to `r9 paste check` (Set-Clipboard + Get-Clipboard in the same exec, read back).
2. Session tab → click the terminal (input line `›` empty, no suggestion) → Ctrl+V → nothing arrives (NEW-10).
3. `otaq ask human` on HOOK-14 by CLI → Overview → Send answer → **"HOOK-14 has unsent text in its terminal"** over
   the empty line (NEW-e1). Clear it and send → delivered.
4. Control: Claude now showed its dim suggestion on the line. Ctrl+V again (nothing pasted), next question → Send
   answer → **no dialog**, answer delivered.
5. A minute later Claude's suggestion was gone (bare `›`); Otaq's stage reminder sat as "1 message waiting"; Deliver
   now → the same unsent-text dialog. Clear it and send cleared it.

## Cause
NEW-10 (known, decision 048): xterm.js sends Ctrl+V as the raw byte 0x16; Claude Code on Windows reads it as its
image paste and finds no image.

NEW-e1 (confirmed by repro + code):
- The tracker counts the raw Ctrl+V as a program fill: `crates/otaq-core/src/session/input.rs:170`
  (`0x16 => self.filled_by_program()`), which sets `Line::Text { count: 1, exact: false, multi_line: true }` and
  `recalled` (`input.rs:319-330`). Nothing clears it until a user Enter or Otaq's `clear_line`: Otaq's own typing
  (an answer from Overview) doesn't touch the flag (`session/mod.rs:1179-1183`).
- `has_unsent_input` (`session/mod.rs:1194-1214`) then reads the screen: Claude's bare marker row is
  `InputLine::Blank`, which answers `keys && multi_line` (`mod.rs:1211`) → **true**, because `filled_by_program`
  set `multi_line`. With Claude's dim suggestion on the row it reads `InputLine::Placeholder` → false; that is why
  step 4 passed and step 5 failed with the same tracker state. (Blank, not Unknown: Claude's cursor is readable, or
  the Placeholder case of step 4 couldn't have been seen.)

## Does 04d9543e fix it?
Partly. On `round6-piece8-ctrl-v-paste` (04d9543e, `ui/src/terminal/Terminal.tsx` + `state.ts`) Ctrl+V with **text** on
the clipboard is taken by the page and sent as `term.paste(text)`: the tracker counts real characters (exact count,
`Typed` on screen), so the reported case (text clipboard) no longer leaves a phantom. With **no text** on the clipboard
(an image, nothing, a clipboard that can't be read) it still sends `\x16` (`pasteOrKey`), so the same
`filled_by_program` → `multi_line` → Blank-means-unsent path remains whenever Claude ends up drawing nothing (e.g. no
image found, or an image paste Claude refuses). Not tried in the VM (branch not built).

## Fix direction
- Merge 04d9543e for NEW-10.
- For NEW-e1: keep "the program may have filled the line" apart from "the draft has several lines". A raw Ctrl+V (and
  history keys) could set `recalled`/inexact without `multi_line`, or `has_unsent_input` could treat `Blank` as empty
  when the only reason for `multi_line` is a program fill and no line break was typed or pasted. The screen already
  decides `Typed` (Claude's `[Image #1]`, Codex's pasted text) correctly.

## Risks to check
- Codex pastes multi-line clipboard text itself on the raw key: its draft must still count (it shows on the marker row
  → `Typed`, or as "[Pasted Content …]" → `Typed`); check a multi-line Codex paste where the cursor ends on a row
  without the marker (`Unknown` → keys, unchanged).
- History recall (↑) of a multi-line entry whose cursor row is the bare marker: keep counting it as text.
- Image paste in Claude Code still works on Windows with 04d9543e (clipboard holding only an image).

## Evidence
`../shots-retest-9/r9-new10-ctrl-v-pastes-nothing-claude.jpg`, `r9-e1-unsent-text-dialog-after-empty-ctrl-v.jpg`,
`r9-e1-ctrl-v-over-claude-suggestion-no-dialog.jpg`, `r9-e1-reminder-held-blank-line-after-ctrl-v.jpg`,
`r9-e1-deliver-now-unsent-dialog-blank-line.jpg`; `../RESULTS-retest-9-repro.md`; first seen `../RESULTS-retest-8-e.md`.
