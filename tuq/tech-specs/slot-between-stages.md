# Tech spec · slot-between-stages

- **Ticket:** OTAQ-8
- **Date:** 2026-11-25
- **Decisions:** 114, 119, 120

## How it is used

Every write that puts a ticket in Queued also says how it got there. The scheduler reads that:

```rust
// The one place the kind is decided (board/wait.rs), called by every queue write:
let kind = WaitKind::after(ticket.status, cause);   // Begin | Kept | AfterCheck
//   left a status in HOLDS_A_SLOT (Working, Checking, CheckFailed…)     -> Kept        (119)
//   left a check by you, or the user answered an agent's "Send back"    -> AfterCheck  (120)
//   anything else (Backlog, Parked, Interrupted, Start interrupted…)    -> Begin

// scheduler::plan_with, per pass:
//   Kept tickets: hold a slot while queued (slot_holders), start in this pass whatever the free slots or
//                 back-pressure; still wait for a pause or a waiting flow change.
//   the rest:     start_order puts AfterCheck before every Begin ticket (pin, priority and age only order them
//                 within their kind), then the free slots and back-pressure apply as today.
```

## How it is built

**A stored wait kind.** `tickets` gets a column `wait_kind` (`begin`, `kept`, `after_check`; NULL outside the
waiting statuses), with a migration that sets `begin` on every ticket that waits now. `Ticket` gets
`wait_kind: WaitKind`. It is written in the same UPDATE that writes `queued_at`. There are six such writes:
`moves.rs` (`queue_in`, `record_drag_in`), `human_check.rs`, `review.rs`, `send_back.rs`, `validation.rs` (two)
and `review/resume.rs` (`bring_back`). They take one SQL fragment macro, `set_queued!()`, next to `set_state_since!()`,
which binds `queued_at` and `wait_kind` together, so no writer can set one without the other. The kind itself comes
from one function, `WaitKind::after(left: Status, cause: Cause)`. `Cause` is `Flow` (the usual case) or
`UsersSendBack` (the one write the status can't tell apart: the user answering an agent's ask to send back, which
leaves `AsksYou`, a slot holder, yet follows 120). Starting the ticket (`Move::Begin`) and unqueueing it set NULL.

**The scheduler.** `slot_holders` (and so `running_count`, the header's count, Settings' "N of M slots taken" and the
Running popover) also counts tickets in `SCHEDULED` with `wait_kind = kept`. `plan_with` splits the ready tickets:
the `kept` ones always go to `plan.start` (never `WaitingForSlot`), the others are sorted by `start_order` and get
`free_slots(running)` as today. Because the kept ones are already in `running`, the others get only what is truly
free, and a lowered Max is overshot only by the kept ones (119). The pause and flow-change holds stay where they are,
before the split, so they hold a kept ticket too.

**The start order.** `order_key` in `board/order.rs` gets a leading `wait_kind != after_check` term, so `start_order`
and `Board::slot_line` (the "2nd in line" number, which reads the same key from SQL) can't disagree.

**The user's clicks.** `Review::limit_full()` (send back, drag back, resume, a conflict fix) is unchanged for the
user's own actions: when full, the ticket queues as `after_check` and the scheduler puts it first (120). The one agent
path that goes through it, a Validate Fail whose Dev session is gone (`validation::tell_agent` →
`resume_with_prompt` → `bring_back`), passes `Kept` and skips `limit_full`: today it can queue itself behind its own
slot when Max is 1.

**The Running popover.** `SlotHolder.busy` gets `Busy::HandingOver` for a kept ticket; `holder_line` shows it as
"starting <next stage>" ("starting Milestone plan"). In practice it shows for the second or so until its start runs,
then as `Busy::Starting`.

## What changes

```text
stage next / Pass / Fail / user's Pass, Send back, answer ──> queue writes [changed] ──> WaitKind::after [new]
                                                                   │  tickets.wait_kind [new column]
scheduler::plan_with [changed] ──> slot_holders [changed]          │
            │                └──> order::start_order [changed] <───┘ (Board::slot_line [changed])
            └─> Starter (start sequence: clears wait_kind)  [changed]
review::bring_back [changed] ──> WaitKind::Kept for Validate's resume
otaq-view holder_line [changed] ──> Busy::HandingOver [new]
```

| Part | Where | New / changed | What it does |
| ---- | ----- | ------------- | ------------ |
| WaitKind, `after()`, `set_queued!()` | `crates/otaq-core/src/board/wait.rs` | new | the kind and the one rule that picks it |
| Migration | `crates/otaq-core/migrations/0129_wait_kind.sql` | new | `tickets.wait_kind`, `begin` for waiting rows |
| Ticket row | `store/tickets.rs`, `board/tickets.rs` | changed | reads `wait_kind` |
| Queue writes | `board/moves.rs`, `human_check.rs`, `review.rs`, `send_back.rs`, `validation.rs`, `review/resume.rs` | changed | write the kind with `queued_at` |
| Send-back answer | `board/questions.rs` `record_send_back_answer` | changed | passes `Cause::UsersSendBack` |
| Scheduler | `scheduler/mod.rs` | changed | kept tickets hold and start; module doc updated |
| Start order | `board/order.rs` | changed | after-check first, in `order_key` |
| Start sequence | `start/sequence.rs` | changed | clears `wait_kind` on entering the stage |
| Running popover | `otaq-view/src/view_models/activity.rs` | changed | "starting <stage>" for `HandingOver` |

## Architecture decisions

### 1. Store how the ticket came to wait, as a column on `tickets`

Each queue write knows what the ticket left; the scheduler, a pure function of the tickets, doesn't. A column keeps
`plan` pure and survives a restart, so a kept ticket is still kept when the service comes back.
Considered: deriving it from the History (the last `Moved`/`Dragged` event's `from`). Lost: the scheduler would read
events on every pass, and the History's payloads aren't a contract. Also considered: no wait at all between two Agent
stages, the caller starting the next stage directly (as `review/drag.rs` does when a slot is free). Lost: a scheduler
pass between the Queued write and the direct start sees a free slot and gives it to another ticket (Max overshot by
one each time), a restart in between loses the hand-over, and the pause and flow-change holds would need a second
copy outside the scheduler.

### 2. The kind follows from the status the ticket leaves

"Held a slot just before" (`HOLDS_A_SLOT`) is Kept; "left a check by you" is AfterCheck; everything else is Begin.
This covers stage next, an agent check's Pass and Fail, Move on anyway, Another round and Answer the agent (from
Check failed, a slot holder), the user's drag of a working ticket, the user's Pass and Send back out of Spec review
or Review, and Resume or Retry of an Interrupted ticket (no slot: Begin, an ordinary start). One explicit cause
remains, the user's "Send back" answer to an agent's ask (120 names it, but the asking agent holds a slot), because
nothing in the status tells it apart.
Considered: each writer passing its kind by hand. Lost: six writers and every future one would have to pick right;
with one function, a new writer gets the rule for free.

### 3. Kept tickets are slot holders, not exempt from counting

They count in `running_count` while queued, so the count, the list and the free slots stay one number (114), and
another ticket can't take the slot between the write and the next pass. A kept ticket under a pause or a waiting flow
change keeps holding its slot: 119 says it holds the slot "while it waits there", and the pause or the change is the
user's own hold.

### 4. After-check goes into the order key, not a second queue

One `order_key` serves `start_order` and `slot_line`, so the line number on a card and the scheduler's choice can't
drift. A second list in `plan_with` would need `slot_line` changed to match by hand.

## Testing

- `tests/suite/waiting.rs` and `limits.rs` (pure `plan`): Max 1 with an older queued ticket. Kept starts, the other
  stays `WaitingForSlot`; Kept starts with back-pressure reached and with Max lowered below the count; kept ticket
  in a paused project is held but counted; after-check goes ahead of a pinned P1 ticket that waits to begin, waits
  for a free slot and for back-pressure; two after-check tickets keep pin → priority → age.
- `WaitKind::after` unit test: one row per status set and cause.
- Service tests (`waiting.rs`, `hand_overs.rs`): with Max 1 and a queued older ticket, Tech spec's `stage next` goes
  on to Milestone plan (the OTAQ-8 repro, failing before the change); a Validate Fail with the Dev session gone
  resumes Dev instead of queueing; a Send back from Review while full is `WaitingForSlot` and first in `slot_line`.
- Migration test: rows waiting before the migration read as `begin`.
- `otaq-view` test for the "starting Milestone plan" holder line.

## Risks

- **A missed queue write** leaves `wait_kind` NULL on a queued ticket. Read NULL as `begin` (today's behaviour), and
  a test lists every `state = Queued` UPDATE in `src/` and checks it uses `set_queued!()`.
- **The count above Max** only happens after Max is lowered: a kept ticket held its slot before, so it never adds
  one. 119 accepts it.
- **A long pause** holds a kept ticket's slot for its whole length (decision 3). With every slot kept by paused
  projects, other projects start nothing until a Resume.
- **Migration** adds a nullable column: no rewrite of existing rows beyond the waiting ones.
