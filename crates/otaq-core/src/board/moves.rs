//! Board moves (SDD §7.3 transitions, §7.4 drag rules; TKT-04, FLW-18; d0004, d0034, d0036, d0037, d0041, d0044,
//! d0052, d0108).
//!
//! [`check_move`] is one pure function: given a ticket and where it is dropped (a board target or any stage of the
//! flow, [`DropOn`]), it says whether the move is allowed, needs a confirmation or a choice, or is refused (with a
//! reason for people). The flow engine decides it from the kinds of the stages involved
//! ([`crate::flow::engine::drag`], STG-06). [`Board::move_ticket`] carries out the bookkeeping moves: Start (a parked
//! ticket returns to the stage it left), Unqueue, Reopen and PR rework. Park and Approve have their own requests;
//! the drags that stop or start sessions (back to any earlier stage, forward past stages) are
//! [`crate::review::Review::drag`]'s. A ticket whose dependencies aren't merged can't be started past Ready for Dev,
//! by a drag or by Start ([`blocks_start`], DPV-03, d0049): [`Board::check_drag`] refuses it ("SHOP-20 is blocked").

use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::{Sqlite, Transaction};

use super::stage_runs::Moved;
use super::tickets::{EventKind, Ticket, record_event};
use super::{Board, BoardError, Refusal};
use crate::domain::{Actor, MovedPast, Place, Status, stored_enum};
use crate::flow::engine::{self, Target};
use crate::flow::{Flow, Stage};
use crate::store::now_ms;
use crate::store::tickets::{TicketRow, set_state_since};

stored_enum! {
    /// Where a card is dropped: one target per column of the default flow's board, and its Approved section
    /// (d0066). [`DropTarget::in_flow`] says what each means in a flow.
    pub enum DropTarget ("drop target") {
        Backlog = "backlog",
        /// The go-ahead: the ticket starts when it can (SCH-01).
        ReadyForDev = "ready_for_dev",
        /// Also the go-ahead from Backlog: the ticket lands in Ready for Dev when it can't start yet.
        Dev = "dev",
        ReadyForReview = "ready_for_review",
        Approved = "approved",
        Done = "done",
    }
}

stored_enum! {
    /// What an allowed move does.
    pub enum MoveAction ("move action") {
        /// Backlog → Ready for Dev (or Dev): Queued, starts when possible. Resumes the conversation for a Parked
        /// ticket (d0052).
        Start = "start",
        /// Ready for Dev → Backlog: out of the queue; nothing had started (d0066).
        Unqueue = "unqueue",
        /// Dev → Backlog: stop the agent and keep the work (d0052).
        Park = "park",
        /// Ready for Review → Approved: asks Local merge or Open PR (d0015).
        Approve = "approve",
        /// Approved → Ready for Review: PR rework (d0041).
        PrRework = "pr_rework",
        /// Done (Closed) → Backlog (d0044).
        Reopen = "reopen",
        /// Back to an earlier Agent stage (FLW-18): asks for the instructions that stage's agent gets; its session
        /// resumes with them (or a fresh one starts) and the card then walks every later stage again. A human Fail.
        SendBack = "send_back",
        /// Back to an earlier agent check: asks for a note; the check runs again with it.
        RunCheckAgain = "run_check_again",
        /// Back onto an earlier human check: no dialog; the card waits there, "Moved back by you".
        MoveBack = "move_back",
        /// To the very next stage the ticket runs: Move to next stage (from an Agent stage) or Pass (a human check).
        NextStage = "next_stage",
        /// Forward past one or more stages (or out of an agent check): they are skipped by you, and the Delivery
        /// waits for a click.
        SkipTo = "skip_to",
    }
}

/// Where a drop lands (FLW-18): one of the board's named targets, or any stage of the project's flow by its id. As
/// JSON a board target is its name (`"dev"`), a stage `{"stage": "security"}`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(untagged)]
pub enum DropOn {
    Board(DropTarget),
    Stage { stage: String },
}

impl DropOn {
    pub fn stage(id: impl Into<String>) -> Self {
        Self::Stage { stage: id.into() }
    }
}

impl From<DropTarget> for DropOn {
    fn from(target: DropTarget) -> Self {
        Self::Board(target)
    }
}

impl From<&DropOn> for DropOn {
    fn from(target: &DropOn) -> Self {
        target.clone()
    }
}

impl std::fmt::Display for DropOn {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Board(target) => target.fmt(f),
            Self::Stage { stage } => f.write_str(stage),
        }
    }
}

/// The verdict of [`check_move`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum MoveCheck {
    /// Dropped where it already is: nothing happens.
    Unchanged,
    Allowed {
        action: MoveAction,
    },
    /// Allowed after the user confirms (Park).
    Confirm {
        action: MoveAction,
    },
    /// Allowed after the user picks how (Approve: Local merge or Open PR).
    Choose {
        action: MoveAction,
    },
    Refused {
        reason: String,
    },
}

impl MoveCheck {
    pub fn action(&self) -> Option<MoveAction> {
        match self {
            Self::Allowed { action } | Self::Confirm { action } | Self::Choose { action } => {
                Some(*action)
            }
            Self::Unchanged | Self::Refused { .. } => None,
        }
    }

    pub fn is_refused(&self) -> bool {
        matches!(self, Self::Refused { .. })
    }
}

/// The reason given when a ticket is dragged into Ready for Review.
pub const ONLY_AGENT_SUBMITS: &str =
    "Only the agent or Move to next stage moves a ticket to Ready for Review";
pub const MERGED_LOCKED: &str = "Merged tickets are locked";
/// The refusal of Reopen on a merged ticket (CLS-03).
pub const MERGED_LOCKED_REOPEN: &str = "Merged tickets are locked. Create a new ticket.";
/// Ready for Dev → Dev: the scheduler starts it (SCH-01).
pub const STARTS_BY_ITSELF: &str = "It starts by itself when it can, in priority order";
/// A Backlog ticket dropped on an agent check: it starts with its first Agent stage.
pub const QUEUE_FIRST: &str = "Drag it to Ready for Dev to queue it";

/// The refusal of a drag or Start past Ready for Dev for a ticket whose dependencies aren't all merged (DPV-03,
/// d0049): "SHOP-20 is blocked". There is no way to start it anyway; a drop on Ready for Dev queues it as before.
pub fn blocked_text(key: &crate::domain::TicketKey) -> String {
    format!("{key} is blocked")
}

/// Whether dropping `ticket` on `target` would start it past Ready for Dev while a dependency isn't merged (DPV-03,
/// d0049): a ticket that hasn't started yet (Backlog or Ready for Dev) dropped on any stage of `flow`, while
/// [`readiness`](super::readiness::readiness) says it waits (or a parent closed, which never counts as merged).
/// `tickets` are its project's.
pub fn blocks_start(flow: &Flow, ticket: &Ticket, tickets: &[Ticket], target: &DropOn) -> bool {
    !ticket.depends_on.is_empty()
        && matches!(ticket.place, Place::Backlog | Place::Waiting(None))
        && matches!(target.in_flow(flow), Some(Target::Stage(_)))
        && !super::readiness::readiness(ticket, tickets).is_ready()
}

/// Decides a drag or a "Start" (⇧⌘D, which is a drop on [`DropTarget::ReadyForDev`]) on the default flow.
pub fn check_move(ticket: &Ticket, target: impl Into<DropOn>) -> MoveCheck {
    check_move_in(&crate::flow::default_file().flow, ticket, target)
}

/// Decides a drag in `flow` (the project's) for a ticket that skips nothing.
pub fn check_move_in(flow: &Flow, ticket: &Ticket, target: impl Into<DropOn>) -> MoveCheck {
    check_with(flow, ticket, &[], &target.into())
}

/// Decides a drag in `flow` for a ticket that skips `skips` by its choices (AGM-08). A forward drag from an Agent
/// stage is the user's Move to next stage, so it waits until the agent isn't mid-turn (CLS-05).
pub fn check_with(flow: &Flow, ticket: &Ticket, skips: &[String], target: &DropOn) -> MoveCheck {
    let check = engine::drag_with(flow, &ticket.place, ticket.status, skips, target);
    let from_agent_stage = ticket
        .place
        .stage()
        .and_then(|id| flow.stage(id))
        .is_some_and(Stage::is_agent_stage);
    if from_agent_stage
        && matches!(
            check.action(),
            Some(MoveAction::NextStage | MoveAction::SkipTo)
        )
        && let Err(e) = super::review::user_may_submit(ticket)
    {
        return MoveCheck::Refused {
            reason: e.to_string(),
        };
    }
    check
}

impl Board {
    /// Carries out a move. A confirmation or a choice needs `confirmed`; without it the move is refused with
    /// [`Refusal::NeedsConfirmation`] so the client can ask.
    pub async fn move_ticket(
        &self,
        key: &str,
        target: impl Into<DropOn>,
        confirmed: bool,
        actor: &Actor,
    ) -> Result<Ticket, BoardError> {
        let ticket = self.ticket(key).await?;
        // While a flow change waits for a span, a drop past its edge stops there (FED-08).
        let target = self.span_edge(&ticket, &target.into()).await?;
        let check = self.check_drag(&ticket, &target).await?;
        match &check {
            MoveCheck::Unchanged => return Ok(ticket),
            MoveCheck::Refused { reason } => {
                return Err(BoardError::refused(Refusal::MoveRefused, reason.clone()));
            }
            MoveCheck::Confirm { .. } | MoveCheck::Choose { .. } if !confirmed => {
                return Err(BoardError::refused(
                    Refusal::NeedsConfirmation,
                    "This move needs your confirmation",
                ));
            }
            _ => {}
        }
        let action = check.action().expect("allowed moves have an action");
        let not_yet = |what: &str| {
            Err(BoardError::refused(
                Refusal::NotAvailableYet,
                format!("{what} isn't available yet"),
            ))
        };
        match action {
            // A parked ticket resumes its conversation when the start sequence sees it (d0052).
            MoveAction::Start => self.start(&ticket, actor).await,
            MoveAction::Unqueue => self.unqueue(&ticket, actor).await,
            MoveAction::Reopen => self.reopen(key, actor).await,
            MoveAction::Park => not_yet("Park"),
            MoveAction::Approve => not_yet("Approve"),
            MoveAction::PrRework => self.pr_rework(&ticket, actor).await,
            // Carried out with the sessions they stop and start (`ticket.drag`, [`crate::review::Review::drag`]).
            MoveAction::SendBack
            | MoveAction::RunCheckAgain
            | MoveAction::MoveBack
            | MoveAction::NextStage
            | MoveAction::SkipTo => not_yet("This move"),
        }
    }

    /// A drag's verdict for `ticket` on its project's flow and its own skips ([`check_with`]).
    pub async fn check_drag(
        &self,
        ticket: &Ticket,
        target: &DropOn,
    ) -> Result<MoveCheck, BoardError> {
        let flow = self.effective_flow(ticket.key.project.as_str()).await?;
        // A blocked ticket can't be started past Ready for Dev, by a drag or by Start (DPV-03): the scheduler would
        // hold it anyway, and an agent started now would work without its parents' changes.
        if !ticket.depends_on.is_empty() {
            let tickets = self.tickets(ticket.key.project.as_str()).await?;
            if blocks_start(&flow, ticket, &tickets, target) {
                return Ok(MoveCheck::Refused {
                    reason: blocked_text(&ticket.key),
                });
            }
        }
        let skips = self.ticket_skips(ticket, &flow).await?;
        Ok(check_with(&flow, ticket, &skips, target))
    }

    /// Backlog → Ready for Dev (Queued).
    async fn start(&self, ticket: &Ticket, actor: &Actor) -> Result<Ticket, BoardError> {
        let returns_to = self.parked_return(ticket).await?;
        let mut tx = self.store.begin_write().await?;
        queue_in(&mut tx, ticket, returns_to.as_deref(), actor, json!({})).await?;
        tx.commit().await?;
        self.ticket_by_id(&ticket.id).await
    }

    /// Where a parked ticket waits when it is dragged out of Backlog (d0052, FLW-18 "Un-park"): before the Agent stage
    /// it left (its latest Agent stage run), through the usual waiting; `None` (Ready for Dev) when that is the flow's
    /// first Agent stage, as the default flow always queues, or for a ticket that isn't parked.
    pub(crate) async fn parked_return(
        &self,
        ticket: &Ticket,
    ) -> Result<Option<String>, BoardError> {
        if ticket.status != Status::Parked {
            return Ok(None);
        }
        let left: Option<String> = sqlx::query_scalar(
            "SELECT stage_id FROM stage_runs WHERE ticket_id = ? AND kind = 'agent'
             ORDER BY rowid DESC LIMIT 1",
        )
        .bind(&ticket.id)
        .fetch_optional(self.store.pool())
        .await?;
        let flow = self.effective_flow(ticket.key.project.as_str()).await?;
        Ok(left
            .filter(|id| flow.stage(id).is_some_and(Stage::is_agent_stage))
            .and_then(|id| super::send_back::waits_before(&flow, &id)))
    }

    /// Ready for Dev → Backlog (d0066): out of the queue. Nothing had started, so a ticket that never worked
    /// (no branch or worktree yet, not parked or reopened) is also "never started" again: its first-start time
    /// and its base lock go, and it can be deleted as before (implementation choice). A parked ticket goes back to
    /// Parked.
    async fn unqueue(&self, ticket: &Ticket, actor: &Actor) -> Result<Ticket, BoardError> {
        let Some(row) = self.store.ticket(&ticket.id).await? else {
            return Err(BoardError::NotFound(ticket.key.to_string()));
        };
        let mut tx = self.store.begin_write().await?;
        unqueue_in(&mut tx, ticket, &row, actor, json!({})).await?;
        tx.commit().await?;
        tracing::info!(key = %ticket.key, actor = %actor, "taken out of the queue");
        self.ticket_by_id(&ticket.id).await
    }

    /// **Reopen** (CLS-03; d0044): Done · Closed → Backlog. Merged tickets are locked. A ticket that has been to
    /// Dev is marked reopened: its next start rebases its old branch onto the current base and starts a new
    /// agent session (GIT-06).
    pub async fn reopen(&self, key: &str, actor: &Actor) -> Result<Ticket, BoardError> {
        let ticket = self.ticket(key).await?;
        match ticket.status {
            Status::Merged => {
                return Err(BoardError::refused(
                    Refusal::MoveRefused,
                    MERGED_LOCKED_REOPEN,
                ));
            }
            Status::ClosedNotMerged => {}
            other => {
                return Err(BoardError::refused(
                    Refusal::MoveRefused,
                    format!(
                        "{} is {}; only Closed tickets can be reopened",
                        ticket.key,
                        other.label()
                    ),
                ));
            }
        }
        let started = self.has_been_to_dev(&ticket).await?;
        let reopened = self
            .set_status(&ticket, Status::Backlog, actor, EventKind::Reopened)
            .await?;
        if started {
            sqlx::query("UPDATE tickets SET reopened = TRUE WHERE id = ?")
                .bind(&ticket.id)
                .execute(self.store.pool())
                .await?;
        }
        tracing::info!(key = %reopened.key, started, "reopened");
        self.ticket_by_id(&ticket.id).await
    }

    async fn set_status(
        &self,
        ticket: &Ticket,
        to: Status,
        actor: &Actor,
        kind: EventKind,
    ) -> Result<Ticket, BoardError> {
        let now = now_ms();
        let mut tx = self.store.begin_write().await?;
        sqlx::query(concat!(
            "UPDATE tickets SET stage_id = NULL, state = ?, outcome = NULL, updated_by = ?, updated_at = ?, ",
            set_state_since!(),
            " WHERE id = ?"
        ))
        .bind(to.as_str())
        .bind(actor.to_string())
        .bind(now)
        .bind(to.as_str())
        .bind(now)
        .bind(&ticket.id)
        .execute(&mut *tx)
        .await?;
        record_event(
            &mut tx,
            &ticket.id,
            kind,
            actor,
            json!({"from": ticket.status, "to": to}),
        )
        .await?;
        tx.commit().await?;
        self.ticket_by_id(&ticket.id).await
    }
}

/// Backlog → Ready for Dev (Queued) inside a transaction. Records the first start and the queue time, and locks
/// the base (d0051). The scheduler starts it once its
/// dependencies are merged (SCH-01) and fills in when it became ready. A parked ticket keeps its `parked` mark, so
/// the start sequence resumes its conversation (d0052), and waits before the stage it left (`returns_to`,
/// [`Board::parked_return`]). `extra` joins the History payload.
pub(crate) async fn queue_in(
    tx: &mut Transaction<'_, Sqlite>,
    ticket: &Ticket,
    returns_to: Option<&str>,
    actor: &Actor,
    extra: serde_json::Value,
) -> Result<(), BoardError> {
    let now = now_ms();
    sqlx::query(concat!(
        "UPDATE tickets SET stage_id = ?, state = ?, started_at = COALESCE(started_at, ?),
                            queued_at = ?, ready_since = NULL, branch_locked = TRUE, updated_by = ?,
                            updated_at = ?, ",
        set_state_since!(),
        " WHERE id = ?"
    ))
    .bind(returns_to)
    .bind(Status::Queued.as_str())
    .bind(now)
    .bind(now)
    .bind(actor.to_string())
    .bind(now)
    .bind(Status::Queued.as_str())
    .bind(now)
    .bind(&ticket.id)
    .execute(&mut **tx)
    .await?;
    let mut payload = json!({"from": ticket.status, "to": Status::Queued});
    if let (Some(payload), serde_json::Value::Object(extra)) = (payload.as_object_mut(), extra) {
        payload.extend(extra);
    }
    record_event(tx, &ticket.id, EventKind::Moved, actor, payload).await
}

/// Ready for Dev → Backlog inside a transaction ([`Board::unqueue`]'s write; `row` is the ticket's stored row,
/// read before). `extra` joins the History payload.
pub(crate) async fn unqueue_in(
    tx: &mut Transaction<'_, Sqlite>,
    ticket: &Ticket,
    row: &TicketRow,
    actor: &Actor,
    extra: serde_json::Value,
) -> Result<(), BoardError> {
    let now = now_ms();
    let to = if row.parked {
        Status::Parked
    } else {
        Status::Backlog
    };
    let never_worked =
        !row.parked && !row.reopened && row.worktree_path.is_none() && row.branch_name.is_none();
    sqlx::query(concat!(
        "UPDATE tickets SET stage_id = NULL, state = ?, queued_at = NULL, ready_since = NULL,
                            start_problem_step = NULL, start_problem_message = NULL,
                            started_at = CASE WHEN ? THEN NULL ELSE started_at END,
                            branch_locked = CASE WHEN ? THEN FALSE ELSE branch_locked END,
                            updated_by = ?, updated_at = ?, ",
        set_state_since!(),
        " WHERE id = ?"
    ))
    .bind(to.as_str())
    .bind(never_worked)
    .bind(never_worked)
    .bind(actor.to_string())
    .bind(now)
    .bind(to.as_str())
    .bind(now)
    .bind(&ticket.id)
    .execute(&mut **tx)
    .await?;
    let mut payload = json!({"from": ticket.status, "to": to});
    if let (Some(payload), serde_json::Value::Object(extra)) = (payload.as_object_mut(), extra) {
        payload.extend(extra);
    }
    record_event(tx, &ticket.id, EventKind::Moved, actor, payload).await
}

/// What a drag the board carries out writes besides the move ([`Board::record_drag`]).
#[derive(Debug, Clone)]
pub(crate) struct DragWrite {
    pub moved: Moved,
    /// Every agent check's counter restarts (a drag back counts as a human Fail, FLW-13).
    pub reset: bool,
    /// The stages the user moved the card past (a forward drag): its Delivery waits for a click.
    pub marks: Vec<(String, MovedPast)>,
    /// Into an agent check: its round opens on this head.
    pub check_head: Option<String>,
    /// The user's note as their comment, posted with the move (a check run again with a note, CMT-05).
    pub hand_over: Option<Box<super::hand_over::HandOver>>,
    /// History's [`EventKind::Dragged`] payload.
    pub event: serde_json::Value,
}

impl Board {
    /// Where a drag the board carries out by itself takes `ticket` to the stage `to` ([`engine::dragged`]).
    pub(crate) async fn drag_moved(&self, ticket: &Ticket, to: &str) -> Result<Moved, BoardError> {
        let flow = self.effective_flow(ticket.key.project.as_str()).await?;
        let skips = self.ticket_skips(ticket, &flow).await?;
        let Some(stage) = flow.stage(to) else {
            return Err(BoardError::refused(
                Refusal::MoveRefused,
                format!("{} has no stage {to}", ticket.key.project),
            ));
        };
        let at = engine::At {
            skips: &skips,
            ..engine::At::new(&flow, &ticket.place, ticket.status)
        };
        Ok(Moved::of(&engine::dragged(at, stage)))
    }

    /// Writes a drag (FLW-18): the run it leaves ends, the stages it walked over get skipped runs and their marks,
    /// the counters restart for a drag back, and the ticket is in its new stage: waiting for the user in a human
    /// check, Checking with a round open in an agent check, To deliver in its Delivery, or queued before an Agent
    /// stage. The caller starts the checker or wakes the scheduler.
    pub(crate) async fn record_drag(
        &self,
        ticket: &Ticket,
        drag: DragWrite,
        actor: &Actor,
    ) -> Result<Ticket, BoardError> {
        let (to, stage) = (
            drag.moved.status,
            drag.moved.place.stage_id().map(str::to_owned),
        );
        let mut tx = self.store.begin_write().await?;
        record_drag_in(&mut tx, ticket, drag, actor).await?;
        tx.commit().await?;
        tracing::info!(key = %ticket.key, %to, stage, "dragged");
        self.ticket_by_id(&ticket.id).await
    }
}

/// [`Board::record_drag`]'s writes inside the caller's transaction (Apply now writes its moves and the flow in one,
/// APN-04).
pub(crate) async fn record_drag_in(
    tx: &mut Transaction<'_, Sqlite>,
    ticket: &Ticket,
    drag: DragWrite,
    actor: &Actor,
) -> Result<(), BoardError> {
    let DragWrite {
        moved,
        reset,
        marks,
        check_head,
        hand_over,
        event,
    } = drag;
    let now = now_ms();
    let waits = moved.place.is_waiting();
    let into_human_check = moved.status.set() == crate::domain::StatusSet::HumanCheck;
    let posted = match &hand_over {
        Some(h) => Some(h.post(tx, &ticket.id, actor).await?),
        None => None,
    };
    moved
        .record(tx, &ticket.id, posted.as_ref().map(|p| p.id.as_str()))
        .await?;
    if reset {
        super::validation::reset_check_counters(tx, &ticket.id).await?;
    }
    for (stage, how) in &marks {
        super::validation::mark_moved_past(tx, &ticket.id, stage, *how).await?;
    }
    let round = match (&check_head, moved.place.stage()) {
        (Some(_), Some(check)) => {
            Some(super::validation::enter_check(tx, &ticket.id, check).await? + 1)
        }
        _ => None,
    };
    sqlx::query(concat!(
        "UPDATE tickets SET stage_id = ?, state = ?, review_entry = NULL, interrupted_from = NULL,
                            review_entered_at = CASE WHEN ? THEN ? ELSE review_entered_at END,
                            queued_at = CASE WHEN ? THEN ? ELSE queued_at END,
                            ready_since = CASE WHEN ? THEN NULL ELSE ready_since END,
                            signal_text = NULL, signal_since = NULL, wakeup_at = NULL, auto_merge_stop = NULL,
                            updated_by = ?, updated_at = ?, ",
        set_state_since!(),
        " WHERE id = ?"
    ))
    .bind(moved.place.stage_id())
    .bind(moved.status.as_str())
    .bind(into_human_check)
    .bind(now)
    .bind(waits)
    .bind(now)
    .bind(waits)
    .bind(actor.to_string())
    .bind(now)
    .bind(moved.status.as_str())
    .bind(now)
    .bind(&ticket.id)
    .execute(&mut **tx)
    .await?;
    if let (Some(head), Some(round)) = (&check_head, round) {
        sqlx::query(
            "INSERT INTO validations (id, ticket_id, round, trigger, started_at, head_sha) VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(crate::store::new_id())
        .bind(&ticket.id)
        .bind(i64::from(round))
        .bind(crate::domain::ValidationTrigger::User.as_str())
        .bind(now)
        .bind(head)
        .execute(&mut **tx)
        .await?;
    }
    // The check runs again with a new comment: its checker primes before its verdict (CMT-08).
    if posted.is_some() {
        super::stage_runs::mark_handed_over(tx, &ticket.id).await?;
    }
    record_event(tx, &ticket.id, EventKind::Dragged, actor, event).await
}
