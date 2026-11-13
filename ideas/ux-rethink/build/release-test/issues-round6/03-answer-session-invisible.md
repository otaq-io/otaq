# 03 · An answer session can't be seen or reached

**Kind:** bug · **Release:** must fix · **From:** re-test 6 NEW-4, issue 10's gap (round-4 piece 23) · **Decision:** 045

## What happens
SHOP-35 (merged) was started again to answer SHOP-36's question. Its Codex prompt read nowhere: no Needs you, no session
view, `otaq attach SHOP-35` "no live session". The question waited 12 min; the tester killed the process.

## Cause (code, high confidence)
- No Needs you: the answer session's hook resolves to `Caller::Agent`, but Merged isn't in `signals::LIVE`
  (signals/state.rs early return). `OpenPrompts` keeps it; no view reads it for an answer session.
- No session view: `session_tab::agent_session` (otaq-view session_tab.rs), its desktop users (api/terminal_tab.rs,
  api/sheet.rs), `session_requests::session_id` (`otaq attach`) and desktop terminal.rs `is_agent` look only at
  `SessionKind::Agent`, so they find the ended Dev session.
- The Done card's "answering SHOP-36" marker (card.rs, MSG-03) is overwritten by the one-line board's
  `card_line::words` ("Merged into main").
- Cleanup lists a running answer session as a stray ("SHOP-35 is Merged") and its worktree as unowned
  (recovery/cleanup.rs).

## Fix direction (decision 045)
Reach it: running Agent first, else a running Answer session, in `agent_session`, `session_id`, `is_agent`. Show it:
a Needs you item from the answer session's `Signals::open_prompt` (beside the inbox item in board/needs_you.rs, built
where Signals is reachable), the Done card line "answering SHOP-36" / "Needs you · answering SHOP-36"; not through
the ticket's status, `status::shown` or `needs_user()` (the card must stay in Done and not float or read live).
Cleanup treats a Done ticket with a running answer session as in use. Share the "a ticket's session prompt shown
without changing its status" shape with issue 02.

## Risks to check
Done cards reading live (sort, `faded`, recovery/Resume); focus holds back only the alert (041, `is_agent`);
`messages_waiting` and deliveries stay kind='agent' only (round 5 piece 4); Overview/stopped callout unchanged.

## Evidence
`../shots-retest-6/gap-answer-session-card-merged-no-needs-you.jpg`, `gap-answer-session-running-history-only.jpg`.
Backend for "a key alone answers" is in place: `quiet_turn_ends.rs` `y_at_a_codex_answer_sessions_prompt_answers_it`.
