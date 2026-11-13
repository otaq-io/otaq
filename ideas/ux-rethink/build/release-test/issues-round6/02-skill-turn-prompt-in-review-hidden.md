# 02 · A prompt during a delivery skill turn shows nowhere

**Kind:** bug · **Release:** must fix · **From:** re-test 6 NEW-2, NEW-6 · **Decisions:** 044 (and 010, kept)

## What happens
- A Codex prompt in a ticket's session while the ticket is in Review: header "Live", pill and card "Ready for you",
  no strip chip; the agent waits until someone opens the session. Seen in the push skill's turn (HOOK-5) and the
  message writer's turn.
- The writer's case is decision 010 (Review stays calm, the prompt shows in the Approve dialog). The push / Open PR
  skill turn and an automatic PR are not covered by anything: the PR dialog in Preparing/Pushing doesn't show it either.
- NEW-6: Send back while a prompt is open says "SHOP-35 has unsent text in its terminal" over an empty line. Holding
  back is right; the words are wrong.

## Cause
`signals/state.rs` `transition` returns early when the status isn't in `signals::LIVE` (signals/mod.rs); Review and
delivery statuses (`WaitingForYou`, `ToDeliver`, `PrOpen`) aren't, so only `last_signal_at` moves. `OpenPrompts`
(signals/asks.rs) keeps the prompt, but every surface reads `ticket.status`. The PR dialog fills `asks_in` only while
the message is unwritten (service/board_requests.rs `TicketPrFlow`).

## Fix direction (decision 044)
Keep the status; a needs-you mark on top, filled from `Signals::open_prompt` for the ticket's Agent/Delivery session
during a skill turn or automatic PR (not the writer), read like `stopped_in_place` (board/tickets.rs `shows_needs_you`,
`needs_user`; view_models/status.rs `shown`; board/needs_you.rs; card.rs). PR dialog `asks_in` in Preparing/Pushing.
Session header "Waiting for you" whenever a prompt is drawn. NEW-6: check `open_prompt` before the unsent-text
confirmation (review/mod.rs `return_with`, review/drag.rs, pr/service.rs `unsent_first`); don't change
`has_unsent_input`. Don't flip the status (piece 3's way): ~74 sites check the Review/delivery statuses (Approve
refusals, `store_delivery_message`, `still_waits_to_deliver`, drag, recovery).

## Risks to check
The mark leaking into status logic (Approve/Send back/Move/drag stay enabled); Your review vs Needs you counts and
notifications (`since` = when the prompt showed); Review ordering; the mark clearing on answer, session end, merge;
the writer under 010 unchanged (`message_writer.rs` `a_writer_waiting_for_permission_shows_in_the_approve_dialogs_only`).

## Evidence
`../shots-retest-6/c2-delivery-session-parallel-prompt-header-live.jpg`,
`new-review-push-skill-prompt-header-live-no-needs-you.jpg`, `new-sendback-says-unsent-text-over-open-prompt.jpg`,
`obs-review-delivery-prompt-session-live.jpg`
