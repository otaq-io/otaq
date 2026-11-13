# 06 · `otaq ask human` from a ticket in Review is stored but shown nowhere

**Kind:** bug · **Release:** to decide · **From:** re-test 8 NEW-b2 · **Decision:** —

## What happens
An agent's `otaq ask human` on a ticket waiting in Review ("Ready for you") prints "HOOK-13 asks the user…", the
question is stored open, but the ticket keeps "Ready for you": no Asks you on the card, no strip chip, no Overview box,
nothing in History or `otaq show`. The same with `--send-back Dev`. The agent is told to wait for an answer that can't
be given.

## Repro (fae1fd78, HOOK-13 in Review "Ready for you", its Claude session live and idle, its merge-message writer
started at Review)
1. `pr_flows` empty (node:sqlite on `otaq.db`).
2. `OTAQ_TICKET=HOOK-13`: `'R9 b2: asked from Review, should this show?' | otaq ask human --stdin --option Yes --option No`
   → accepted. DB: `human_questions` row open (`answered_at NULL`); `pr_flows` still empty; ticket
   `state = ready_for_review`. Sheet: Overview without any question box, pill "Ready for you".
3. `'R9 b2: send it back to Dev? (test)' | otaq ask human --stdin --send-back Dev` → accepted, stored open with
   `send_back_stage = Dev` (it replaced step 2's question); `otaq show HOOK-13`: "Status: Ready for you (Review)", no
   question. Closed by hand afterwards (`answered_via = 'closed'`).

## Cause (confirmed by repro + DB + code; the suspected `pr_flow` route is refuted)
- `service/board_requests.rs:1898` sends the ask to `report_pr_problem` only while a `pr_flows` row exists, which
  only Open PR / Push / the automatic delivery create (`pr/service.rs:276, 289, 460`). The merge-message writer
  doesn't; the table was empty before and after the ask. So the question takes the normal path:
  `store_question` / `store_send_back_question` (`board_requests.rs:1909-1924`), then `signals.ask_human`.
- `signals::transition` ignores every input on a ticket whose status isn't in `LIVE` (`signals/state.rs:118`;
  `LIVE` = Working, Working in background, Asks you, Needs you, Idle, Waiting for limit reset, `signals/mod.rs:59`).
  "Ready for you" (`ready_for_review`) isn't, so the status stays.
- Everything that shows a question goes by the status: the ticket's `question` is filled only when the status is
  Asks you (`board/tickets.rs:1462-1465`), and the card, strip, Overview box and `otaq show` read that.

## Fix direction (needs a product call)
Either (a) refuse `otaq ask human` while the ticket waits in a check of yours (and in Done), telling the agent the
user is reviewing ("HOOK-13 is in your review: say it in your summary or a comment"), or (b) let an agent's question
show in Review: Asks you on a Review ticket (and its Overview box above Approve / Send back). (a) is small and keeps
Review's meaning; (b) needs the review bar, Approve's checks and the merge-message writer's prompt (which keeps its
calm in Review) to account for an open question.

## Risks to check
A question stored open stays open: once the ticket goes back to a live status (Send back to Dev) the stored question
would hold Asks you ("an open question holds Asks you", `signals/state.rs:210-222`) long after it was asked (not
tested). Helpers (`Caller::Helper`) and the PR-dialog route must keep working. Questions already stored open on tickets
in Review need a sweep or must be ignored.

## Evidence
`../shots-retest-9/r9-b2-review-ask-not-shown-overview.jpg`; DB rows in `../RESULTS-retest-9-repro.md`; first seen
`../RESULTS-retest-8-b.md`, `--send-back` in `../RESULTS-retest-8-fixes.md`.
