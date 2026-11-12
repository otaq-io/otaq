# 02 · A Codex prompt queued behind another never reads Needs you; the card may name the wrong command

**Kind:** bug / design · **Release:** decide: fix now or next milestone · **From:** re-test 5 PARTLY of piece 28 (NEW-2); piece 28's hand-back (two prompts at once)

## What happens
- When Codex runs two commands at once it fires both PermissionRequest hooks together but draws one prompt at a time.
  Otaq keeps one open prompt per session, so: after "y" on the first, the second prompt (drawn right after) never
  reads Needs you: a checker's pill and card stayed "Checking" and the strip empty for 30 s+ (3 of 3); and the card
  sometimes names the second command while the first is on screen.
- Helpers didn't show it in 6 answers; the agent in charge's own prompts may (unconfirmed: re-test 3's "NOTE-14's Dev
  prompts read Working").

## Evidence
Screenshots in `../shots-retest-5/`: `c28-checker-2nd-prompt-drawn-pill-checking.jpg`, `c28-checker-2nd-prompt-30s-still-checking.jpg`,
`c28-checker-2nd-prompt-card-checking-strip-0.jpg`, `c28-note16-checker-card-names-other-command.jpg`,
`c28-note16-2nd-prompt-9s-still-checking.jpg`

## Where to start
Piece 28's agent: Otaq would need a list of open prompts per session and to read the screen to know which is drawn.
crates/otaq-core/src/signals/ (asks.rs, mod.rs), validation/mod.rs (checker prompts), helpers/waits.rs, Codex hooks
(agent_tools/codex/hooks.rs). Piece 28's log copies: the orchestrator's scratchpad `p28/`.

## Open questions
- Keep every open prompt per session and show the one drawn (screen read), or the oldest unanswered one?
