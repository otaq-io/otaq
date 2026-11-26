# Plan · slot-between-stages

- **Ticket:** OTAQ-8
- **Date:** 2026-11-25
- **Decisions:** 114, 119, 120
- **Tech spec:** tuq/tech-specs/slot-between-stages.md
- **Orchestrator prompt:** tuq/prompts/slot-between-stages-orchestrator.md

<!-- tuq:plan:start -->

slot-between-stages is built in four pieces. First the groundwork every behaviour reads: each queued ticket stores
how it came to wait (tech spec decisions 1 and 2). Then two behaviours side by side, both waiting only for it: a
ticket keeps its slot between two Agent stages (decision 119) and, after one of your checks, goes ahead of the queue
(decision 120). Last, the Running popover line for a kept ticket (119's "the Running popover lists it"), which needs
the scheduler's kept slot holders from piece 2. The agent in charge follows
tuq/prompts/slot-between-stages-orchestrator.md.

## Pieces

1. Every queued ticket stores how it came to wait — waits for nothing
   Groundwork for pieces 2 and 3; nothing the user sees changes yet. Follow tuq/tech-specs/slot-between-stages.md,
   "A stored wait kind" and architecture decisions 1 and 2.
   - New crates/otaq-core/src/board/wait.rs: `enum WaitKind { Begin, Kept, AfterCheck }` (stored as `begin`,
     `kept`, `after_check`), `enum Cause { Flow, UsersSendBack }`, and the one rule `WaitKind::after(left: Status,
     cause: Cause)`: `UsersSendBack` → AfterCheck; left a status in `HOLDS_A_SLOT` (crates/otaq-core/src/scheduler/
     mod.rs) → Kept; left a check by you (Spec review, Review or any human check) → AfterCheck; anything else
     (Backlog, Parked, Interrupted, Start interrupted…) → Begin.
   - A `set_queued!()` SQL-fragment macro next to `set_state_since!()` that binds `queued_at` and `wait_kind`
     together, so no writer sets one without the other.
   - Migration crates/otaq-core/migrations/0129_wait_kind.sql: nullable `tickets.wait_kind`, set to `begin` on every
     ticket that waits now. NULL reads as Begin (today's behaviour).
   - `Ticket` gets `wait_kind: WaitKind` (store/tickets.rs, board/tickets.rs).
   - Every UPDATE that sets `queued_at` uses `set_queued!()` with `WaitKind::after(<status it leaves>, Cause::Flow)`:
     board/moves.rs (`queue_in`, `record_drag_in`), board/human_check.rs, board/review.rs, board/send_back.rs,
     board/validation.rs (two writes) and review/resume.rs (`bring_back`). board/questions.rs
     `record_send_back_answer` passes `Cause::UsersSendBack` (it leaves AsksYou, a slot holder, but follows 120).
   - Starting the ticket (`Move::Begin`, start/sequence.rs) and unqueueing it set `wait_kind` to NULL.
   Check: a `WaitKind::after` unit test with one row per status set and cause; the migration test in
   tests/suite/migrations.rs (rows waiting before 0129 read as `begin`); a test that lists every `state = Queued`
   / `queued_at = ?` UPDATE under crates/otaq-core/src/ and fails if one doesn't use `set_queued!()`.
2. A ticket keeps its slot between two Agent stages — waits for 1
   Decision 119; tech spec "The scheduler", "The user's clicks" and architecture decision 3. In
   crates/otaq-core/src/scheduler/mod.rs: `slot_holders` (and so `running_count`, the header count, Settings'
   "N of M slots taken" and the Running popover's list) also counts tickets in `SCHEDULED` with
   `wait_kind = Kept`. `plan_with` splits the ready tickets after the pause and flow-change holds (which still hold
   a kept ticket, and it keeps counting): Kept ones always go to `plan.start`, never `WaitingForSlot`, whatever the
   free slots, a lowered Max or "Hold new starts when Needs you reaches"; the rest are sorted by `start_order` and
   get `free_slots(running)` as today. Update the module doc (lines 1-30: "limits only hold back starts"; a
   hand-over between two Agent stages is not a start).
   Validate Fail with the Dev session gone (board/validation.rs `tell_agent` → `resume_with_prompt` →
   review/resume.rs `bring_back`): pass Kept and skip `Review::limit_full()`, so it no longer queues behind its own
   slot at Max 1. `limit_full` stays as it is for the user's own clicks.
   Check: pure `plan` tests in tests/suite/limits.rs / waiting.rs (Max 1 with an older queued ticket: the kept one
   starts, the other stays `WaitingForSlot`; kept starts with back-pressure reached and with Max lowered below the
   count; a kept ticket in a paused project is held but counted); service tests in waiting.rs / hand_overs.rs: the
   OTAQ-8 repro (Max 1, an older queued ticket, Tech spec's `stage next` goes on to Milestone plan; it fails before
   this piece) and a Validate Fail with the Dev session gone resumes Dev instead of queueing.
3. After your check, a ticket goes ahead of the queue — waits for 1
   Decision 120; tech spec "The start order" and architecture decision 4. In crates/otaq-core/src/board/order.rs,
   `order_key` gets a leading `wait_kind != after_check` term, so AfterCheck tickets come before every Begin ticket
   (pinned and higher-priority ones too), and pin → priority → ready the longest only orders tickets within their
   kind. `Board::slot_line` (the "2nd in line" number) reads the same key from SQL: change it with `order_key` so
   the two can't disagree. It is still a start: it waits for a free slot and for back-pressure, and shows "Waiting
   for a slot".
   Check: pure `plan` tests in tests/suite/limits.rs / waiting.rs: after-check goes ahead of a pinned P1 ticket
   waiting to begin, waits for a free slot and for back-pressure; two after-check tickets keep pin → priority → age.
   A service test: a Send back from Review while full is `WaitingForSlot` and first in `slot_line`.
4. The Running popover shows a ticket handing over — waits for 2
   Decision 119 ("the Running popover lists it… so the count and the list still agree"); tech spec "The Running
   popover". One behaviour across the core and the view, on purpose: `SlotHolder.busy` gets `Busy::HandingOver`
   (crates/otaq-core/src/scheduler/mod.rs, `enum Busy`) for a queued Kept ticket, and `holder_line` in
   crates/otaq-view/src/view_models/activity.rs shows it as "starting <next stage>" ("starting Milestone plan").
   Check: an otaq-view test for the "starting Milestone plan" holder line, and a scheduler test that a queued Kept
   ticket is listed as a slot holder with `Busy::HandingOver`.

<!-- tuq:plan:end -->
