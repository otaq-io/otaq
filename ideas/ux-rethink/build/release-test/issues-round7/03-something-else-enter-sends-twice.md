# 03 · ↵ in Something else's field sends the answer twice (and a digit typed there picks an option)

**Kind:** bug · **Release:** to decide · **From:** re-test 8 NEW-f1 · **Decision:** —

## What happens
On a ticket's Overview, picking "Something else", typing text and pressing ↵ delivers the answer once and also shows
the error toast "The answer to HOOK-14 wasn't sent · HOOK-14 isn't waiting for an answer". Found while tracing: a digit
typed into that field (e.g. "V2") doesn't type: it picks option 2 and the field disappears.

## Repro (fae1fd78, HOOK-14 idle in Dev, Claude Code; questions asked by CLI with `OTAQ_TICKET=HOOK-14`)
| Question | Action | Result |
|---|---|---|
| "R9 f1 one", options Alpha / Beta | Something else → "Go on" → ↵ | delivered once ("Answer to your question: Go on"; log one `needs-you answer … Sent`) **and** the "wasn't sent" toast |
| "R9 f1 three" | Something else → "Go on" → click **Send answer** | delivered, **no toast** |
| "R9 f1 four" | click Beta → ↵ | delivered ("Beta (option 2)"), **no toast** |
| "R9 f1 two" | Something else → type `V2` | "2" picked Beta, the field closed, "V" kept in state |

(The error toast stays until closed; "R9 f1 two"'s screenshot still shows the first one's toast. "R9 f1 three" was
checked after closing it.)

## Cause (confirmed by repro + code)
`crates/otaq-desktop/ui/src/screens/overview/NeedsYou.tsx`, `AnswerChoices`: Something else's `<textarea>` sits inside
the options `div`.
- The textarea's `onKeyDown` (`:228`, `sendOnEnter`) calls `onSend()` and only `preventDefault()`s; the event bubbles to
  the options div's `onKeyDown` (`:163-171`), whose guard is `!(e.target instanceof HTMLInputElement)`. A textarea is an
  `HTMLTextAreaElement`, so the guard passes and `onSend()` runs a second time in the same event.
- `send` in `AnswerBox` guards on `sending` (`:309`), but both calls see the same render's `sending === false`
  (state hasn't updated between the two handlers), so two `sheet.answer` requests go. The first delivers and closes the
  question; the second reaches the service after it closed and is refused (`attention/needs_you.rs:541`
  "isn't waiting for an answer"); `api/sheet.rs:1752` toasts it.
- The same guard lets digits through: `pickByKey(e.key)` (`:164`) picks a row for "1"–"4" typed in the textarea.

A click on Send answer and ↵ on a row go through one handler only, hence no toast. `AgentQuestion.tsx` (Noyan's panel
and Edit flow's box) uses the same `AnswerChoices`, so ↵ in Something else there should send twice too
("The answer to Noyan wasn't sent", `api/agent_panel.rs:504`); not tried.

## Fix direction
In the options div's `onKeyDown`, ignore events from any text field (`HTMLTextAreaElement` too, or `e.target !==
e.currentTarget` for non-option targets), or `stopPropagation()` in the textarea's handler. Make `send` single-flight
with a ref (not state) so a double call can't send twice.

## Risks to check
↵ and 1–4 on a focused row still pick/send (NEW-b3's several-answers ticking); Shift+↵ in the field still makes a new
line; IME composition; the same in Noyan's panel, Edit flow's box and the strip popover.

## Evidence
`../shots-retest-9/r9-f1-something-else-go-on-before-enter.jpg`, `r9-f1-enter-answered-and-wasnt-sent-toast.jpg`,
`r9-f1-send-button-click-no-toast.jpg`, `r9-f1-normal-option-enter-no-toast.jpg`,
`r9-f1-digit-typed-in-something-else-picks-beta.jpg`; `../RESULTS-retest-9-repro.md`; first seen
`../RESULTS-retest-8-fixes2.md`.
