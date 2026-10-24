//! A stuck child goes to its epic's lead once (LEAD-07, LEAD-04; d0083, d0063, d0064, d0042, d0018).
//!
//! - **The first stop** ([`hold`]): a child of an epic with a **live** lead that has never had its lead attempt
//!   reaches "Validation failed N times" (the validation's last round), "Keeps conflicting with `<epic branch>`"
//!   (Auto-merge's third conflict) or Idle (its agent's Stop). The stop is **held**: no inbox item, no notification,
//!   no turquoise; the child keeps its working card (Working). The lead is woken ([`wake_text`]; its status reads
//!   "re-planning") and a handover is registered. The attempt is stored on the child (`lead_stuck`, one row per
//!   child), so it is used once, also across restarts. With no live lead, or on any later stop, the stop goes to the
//!   inbox as today. The board reaches this through [`crate::epic_lead::wake::LeadLink`].
//! - **The lead's note** ([`Stuck::answer`], `otaq note <KEY> [--view "…"] --stdin`): counts as **Answer the agent**.
//!   Validation failed: the round counter restarts at 0 and the note goes to the agent with the findings. Keeps
//!   conflicting: the conflict counter restarts and the merge is retried, the note going to the agent first. Idle:
//!   the note is typed in (§9.3). The note is stored read `with_stop`, and the card shows "Lead's note sent"
//!   ([`crate::domain::LeadNoteSent`]); its `--view` is kept for the second stop. Otaq itself never nudges (d0063):
//!   only the lead's note does.
//! - **The second stop** goes to the inbox as today; its item carries the lead's note and view
//!   ([`crate::board::needs_you::NeedsYouItem::lead_note`]). The lead isn't woken again for this child.
//! - **Hand-back** ([`hand_back`]): the lead's turn ended without a note to that child, or the lead ended: the held
//!   stop is applied as if no lead were live (its own inbox item, no lead's note). Handovers live in memory, so on
//!   startup every held stop is handed back ([`hand_back_after_restart`]) before the reconcile.

use std::collections::HashMap;

use serde_json::json;

use crate::board::tickets::{EventKind, Ticket, record_event};
use crate::board::{Board, BoardError};
use crate::domain::validation::{failed_times, keeps_conflicting, keeps_failing_merge_check};
use crate::domain::{
    Actor, AutoMergeStop, LeadNoteSent, Status, StuckStop, TicketKey, stored_enum,
};
use crate::epic_lead::notes::{LeadNote, ReadHow, mark_read};
use crate::epic_lead::wake::Handover;
use crate::naming;
use crate::project_agents::lead::{EpicLeads, Wake, lead_name};
use crate::protocol::Event;
use crate::service::EventBus;
use crate::session::deliveries::{Deliveries, DeliveryKind};
use crate::session::{SessionKind, Sessions};
use crate::signals::Signals;
use crate::store::now_ms;
use crate::store::tickets::set_state_since;
use crate::validation::Validation;

/// The handover owner of held stops ([`EpicLeads::on_hand_back`]).
pub const OWNER: &str = "stuck";

stored_enum! {
    /// Where a child's one lead attempt stands (`lead_stuck.state`).
    pub enum StuckState ("stuck state") {
        /// The stop waits for the lead.
        Held = "held",
        /// The lead's note answered it.
        Noted = "noted",
        /// It went to the user without a note.
        HandedBack = "handed_back",
    }
}

/// A stop as it happened: what the lead is told, and what is applied if it goes to the user.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FirstStop {
    pub stop: StuckStop,
    /// The last validation report, the merge target, or the agent's last line.
    pub detail: Option<String>,
    /// Validation failed: how many rounds failed.
    pub rounds: Option<u32>,
}

impl FirstStop {
    pub fn validation_failed(report: &str, rounds: u32) -> Self {
        Self {
            stop: StuckStop::ValidationFailed,
            detail: Some(report.trim().to_owned()).filter(|r| !r.is_empty()),
            rounds: Some(rounds),
        }
    }

    pub fn keeps_conflicting(target: &str) -> Self {
        Self {
            stop: StuckStop::KeepsConflicting,
            detail: Some(target.to_owned()),
            rounds: None,
        }
    }

    /// The third conflict or failed Merge check in a row, the last a failed check (d0120).
    pub fn keeps_failing_merge_check(target: &str) -> Self {
        Self {
            stop: StuckStop::KeepsFailingMergeCheck,
            detail: Some(target.to_owned()),
            rounds: None,
        }
    }

    pub fn idle(last_line: Option<&str>) -> Self {
        Self {
            stop: StuckStop::Idle,
            detail: last_line
                .map(str::trim)
                .filter(|l| !l.is_empty())
                .map(str::to_owned),
            rounds: None,
        }
    }

    /// "Validation failed 3 times", "Keeps conflicting with `wishlist`", "Idle".
    pub fn reason(&self) -> String {
        match self.stop {
            StuckStop::ValidationFailed => failed_times(self.rounds.unwrap_or(1)),
            StuckStop::KeepsConflicting => keeps_conflicting(self.detail.as_deref().unwrap_or("")),
            StuckStop::KeepsFailingMergeCheck => {
                keeps_failing_merge_check(self.detail.as_deref().unwrap_or(""))
            }
            StuckStop::Idle => Status::Idle.label().to_owned(),
        }
    }
}

/// A child's one lead attempt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeldStop {
    pub child_id: String,
    pub child: TicketKey,
    pub epic_id: String,
    pub epic: TicketKey,
    /// "Wishlist lead".
    pub lead: String,
    /// The lead's Project agent id.
    pub lead_id: String,
    pub first: FirstStop,
    pub state: StuckState,
    pub held_at: i64,
    pub note_id: Option<String>,
}

/// The wake-up typed into the lead: "SHOP-40.3 stopped: Validation failed 3 times. Last report: orders.test.ts
/// expects `list` …. Send it one note with `otaq note SHOP-40.3 --stdin`, or leave it to the user."
pub fn wake_text(child: &TicketKey, first: &FirstStop) -> String {
    let what = match (first.stop, first.detail.as_deref()) {
        (StuckStop::ValidationFailed, Some(report)) => {
            format!("{}. Last report: {}", first.reason(), summary(report))
        }
        (StuckStop::Idle, Some(line)) => {
            format!("{}. Its last message: {}", first.reason(), summary(line))
        }
        _ => first.reason(),
    };
    format!(
        "{child} stopped: {what}. Send it one note with `{} note {child} --stdin`, or leave it to the user.",
        naming::COMMAND_NAME
    )
}

/// The first line, cut at 120 characters, with " …" when there is more; no full stop of its own.
fn summary(text: &str) -> String {
    const MAX: usize = 120;
    let mut lines = text.lines().map(str::trim).filter(|l| !l.is_empty());
    let first = lines.next().unwrap_or_default();
    let more = lines.next().is_some() || first.chars().count() > MAX;
    let cut: String = first.chars().take(MAX).collect();
    let cut = cut.trim_end().trim_end_matches('.');
    if more {
        format!("{cut} \u{2026}")
    } else {
        cut.to_owned()
    }
}

/// The text typed into the child after an Idle or a conflict: "Note from Wishlist lead: …".
pub fn note_text(lead: &str, note: &str) -> String {
    format!("Note from {lead}: {}", note.trim())
}

#[derive(sqlx::FromRow)]
struct Row {
    child_id: String,
    epic_id: String,
    lead_id: String,
    stop: String,
    detail: Option<String>,
    rounds: Option<i64>,
    state: String,
    held_at: i64,
    note_id: Option<String>,
}

/// The child's lead attempt (by ticket id), if it had one.
pub async fn find(board: &Board, child_id: &str) -> Result<Option<HeldStop>, BoardError> {
    let row: Option<Row> = sqlx::query_as(
        "SELECT child_id, epic_id, lead_id, stop, detail, rounds, state, held_at, note_id FROM lead_stuck
         WHERE child_id = ?",
    )
    .bind(child_id)
    .fetch_optional(board.store().pool())
    .await?;
    let Some(row) = row else {
        return Ok(None);
    };
    let bad = |e: crate::domain::UnknownValue| BoardError::NotFound(e.to_string());
    let child = board.ticket_by_id(&row.child_id).await?;
    let epic = board.ticket_by_id(&row.epic_id).await?;
    Ok(Some(HeldStop {
        child_id: row.child_id,
        child: child.key,
        epic_id: row.epic_id,
        epic: epic.key,
        lead: lead_name(&epic.title),
        lead_id: row.lead_id,
        first: FirstStop {
            stop: row.stop.parse().map_err(bad)?,
            detail: row.detail,
            rounds: row.rounds.and_then(|n| u32::try_from(n).ok()),
        },
        state: row.state.parse().map_err(bad)?,
        held_at: row.held_at,
        note_id: row.note_id,
    }))
}

/// Whether the child's stop waits for its lead now.
pub async fn is_held(board: &Board, child_id: &str) -> Result<bool, BoardError> {
    Ok(find(board, child_id)
        .await?
        .is_some_and(|h| h.state == StuckState::Held))
}

/// Whether the child's next stop would go to its lead: an epic child with a live lead that never had its attempt.
pub async fn takes_stop(
    board: &Board,
    leads: &EpicLeads,
    child: &Ticket,
) -> Result<bool, BoardError> {
    let Some(epic) = &child.epic else {
        return Ok(false);
    };
    if find(board, &child.id).await?.is_some() {
        return Ok(false);
    }
    Ok(leads.live_lead(&epic.to_string()).await?.is_some())
}

/// The child's first stop: held for the epic's live lead, which is woken with a handover, and the attempt marked
/// used. Whether it was held; false (not an epic child, no live lead, the attempt used) leaves the stop to the
/// caller, which applies it as today. The caller has already put the child in its held state (Working).
pub async fn hold(
    board: &Board,
    leads: &EpicLeads,
    child: &Ticket,
    first: &FirstStop,
) -> Result<bool, BoardError> {
    if !takes_stop(board, leads, child).await? {
        return Ok(false);
    }
    let epic = child.epic.as_ref().expect("checked").to_string();
    let Some(lead) = leads.live_lead(&epic).await? else {
        return Ok(false);
    };
    let pool = board.store().pool();
    // Stored before the lead is woken, so its note finds the stop waiting.
    let stored = sqlx::query(
        "INSERT INTO lead_stuck (child_id, epic_id, lead_id, stop, detail, rounds, state, held_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?) ON CONFLICT (child_id) DO NOTHING",
    )
    .bind(&child.id)
    .bind(&lead.epic_id)
    .bind(&lead.agent.id)
    .bind(first.stop.as_str())
    .bind(first.detail.as_deref())
    .bind(first.rounds.map(i64::from))
    .bind(StuckState::Held.as_str())
    .bind(now_ms())
    .execute(pool)
    .await?;
    if stored.rows_affected() != 1 {
        return Ok(false);
    }
    let woken = leads
        .wake_with_handover(
            &epic,
            Wake::Stuck,
            &wake_text(&child.key, first),
            OWNER,
            &child.key,
            &child.id,
        )
        .await;
    if !matches!(woken, Ok(Some(_))) {
        if let Err(e) = woken {
            tracing::warn!(child = %child.key, error = %e, "can't wake the lead: the stop goes to the user");
        }
        // Not woken: the attempt isn't used.
        sqlx::query("DELETE FROM lead_stuck WHERE child_id = ? AND state = ?")
            .bind(&child.id)
            .bind(StuckState::Held.as_str())
            .execute(pool)
            .await?;
        return Ok(false);
    }
    if let Some(held) = find(board, &child.id).await? {
        log(board, &held, StuckState::Held, &Actor::current_user()).await?;
    }
    tracing::info!(child = %child.key, stop = %first.stop, "first stop sent to the lead");
    Ok(true)
}

/// A child whose stop is held stays in Dev at work (Working), its stop waiting. `conflicted` counts the Delivery's
/// conflict that stopped it, as the stop itself would have.
pub(crate) async fn hold_in_dev(
    board: &Board,
    ticket_id: &str,
    conflicted: bool,
) -> Result<(), BoardError> {
    let now = now_ms();
    let to = Status::Working;
    sqlx::query(concat!(
        "UPDATE tickets SET stage_id = ?, state = ?, signal_text = NULL, signal_since = NULL,
                            auto_merge_conflicts = auto_merge_conflicts + ?, updated_at = ?, ",
        set_state_since!(),
        " WHERE id = ?"
    ))
    // At work in the default flow's Agent stage.
    .bind(crate::flow::engine::default_stage(to, None))
    .bind(to.as_str())
    .bind(i64::from(conflicted))
    .bind(now)
    .bind(to.as_str())
    .bind(now)
    .bind(ticket_id)
    .execute(board.store().pool())
    .await?;
    Ok(())
}

/// The Stop of a working child's agent that would make it Idle: held for its lead instead when this is its first
/// stop (the hook then leaves it at work). Whether it was held.
pub async fn hold_at_idle(board: &Board, ticket_id: &str, last_line: Option<&str>) -> bool {
    let held = async {
        let ticket = board.ticket_by_id(ticket_id).await?;
        if ticket.epic.is_none() || !super::notes::idle_at_stop(ticket.status) {
            return Ok(false);
        }
        Ok::<_, BoardError>(
            board
                .leads()
                .child_stuck(&ticket, &FirstStop::idle(last_line))
                .await,
        )
    };
    held.await.unwrap_or_else(|e| {
        tracing::warn!(ticket_id, error = %e, "can't send the idle stop to the lead");
        false
    })
}

/// Moves the attempt out of `held`; false when it had left already (noted, handed back).
async fn resolve(
    board: &Board,
    child_id: &str,
    to: StuckState,
    note_id: Option<&str>,
) -> Result<bool, BoardError> {
    let done = sqlx::query(
        "UPDATE lead_stuck SET state = ?, resolved_at = ?, note_id = COALESCE(?, note_id)
         WHERE child_id = ? AND state = ?",
    )
    .bind(to.as_str())
    .bind(now_ms())
    .bind(note_id)
    .bind(child_id)
    .bind(StuckState::Held.as_str())
    .execute(board.store().pool())
    .await?;
    Ok(done.rows_affected() == 1)
}

/// Logs one step on the child and the epic (History).
async fn log(
    board: &Board,
    held: &HeldStop,
    step: StuckState,
    actor: &Actor,
) -> Result<(), BoardError> {
    let payload = json!({
        "child": held.child, "lead": held.lead, "stop": held.first.stop.as_str(),
        "reason": held.first.reason(), "step": step.as_str(),
    });
    let mut tx = board.store().begin_write().await?;
    for id in [&held.child_id, &held.epic_id] {
        record_event(&mut tx, id, EventKind::LeadStuck, actor, payload.clone()).await?;
    }
    tx.commit().await?;
    Ok(())
}

/// Hands one held stop back to the user (the lead's turn ended without a note to the child, or the lead ended, or
/// the service restarted): the stop is applied as if no lead were live, with its own inbox item. False when it no
/// longer waited for the lead.
pub async fn hand_back(
    board: &Board,
    signals: &Signals,
    sessions: &Sessions,
    child_id: &str,
) -> Result<bool, BoardError> {
    let Some(held) = find(board, child_id).await? else {
        return Ok(false);
    };
    if held.state != StuckState::Held
        || !resolve(board, child_id, StuckState::HandedBack, None).await?
    {
        return Ok(false);
    }
    let actor = Actor::ProjectAgent(held.lead_id.clone());
    log(board, &held, StuckState::HandedBack, &actor).await?;
    let ticket = board.ticket_by_id(child_id).await?;
    // Only a child still in its held state gets the stop: the user may have moved it meanwhile.
    let still_held = matches!(
        ticket.status,
        Status::Working | Status::WorkingInBackground | Status::Idle
    );
    if !still_held {
        return Ok(true);
    }
    match held.first.stop {
        StuckStop::ValidationFailed => {
            board
                .set_validation_status(
                    child_id,
                    &[Status::Working, Status::WorkingInBackground, Status::Idle],
                    Status::CheckFailed,
                    None,
                    false,
                )
                .await?;
        }
        StuckStop::KeepsConflicting => {
            let target = held.first.detail.clone().unwrap_or_default();
            board
                .delivery_stopped(
                    child_id,
                    AutoMergeStop::KeepsConflicting,
                    &keeps_conflicting(&target),
                    false,
                )
                .await?;
        }
        StuckStop::KeepsFailingMergeCheck => {
            let target = held.first.detail.clone().unwrap_or_default();
            board
                .delivery_stopped(
                    child_id,
                    AutoMergeStop::KeepsFailingMergeCheck,
                    &keeps_failing_merge_check(&target),
                    false,
                )
                .await?;
        }
        StuckStop::Idle => {
            // An agent at work again (the user typed to it) isn't idle: nothing to stop.
            let at_work = sessions
                .running_of(child_id, SessionKind::Agent)
                .is_some_and(|s| !signals.is_idle_between_turns(&s.id));
            if !at_work {
                signals
                    .idle_now(child_id, held.first.detail.clone())
                    .await?;
            }
        }
    }
    tracing::info!(child = %held.child, stop = %held.first.stop, "stop handed back by the lead");
    Ok(true)
}

/// On startup, before the reconcile: every stop still held for a lead goes to the user (no lead is live after a
/// restart, and handovers don't survive one). How many were handed back.
pub async fn hand_back_after_restart(
    board: &Board,
    signals: &Signals,
    sessions: &Sessions,
) -> Result<usize, BoardError> {
    let held: Vec<(String,)> = sqlx::query_as(
        "SELECT child_id FROM lead_stuck WHERE state = ? ORDER BY held_at, child_id",
    )
    .bind(StuckState::Held.as_str())
    .fetch_all(board.store().pool())
    .await?;
    let mut handed = 0;
    for (child_id,) in held {
        if hand_back(board, signals, sessions, &child_id).await? {
            handed += 1;
        }
    }
    Ok(handed)
}

/// The "Lead's note sent" flag of every ticket of the project whose lead answered its first stop, by ticket id.
pub(crate) async fn note_flags(
    board: &Board,
    project_id: &str,
) -> Result<HashMap<String, LeadNoteSent>, BoardError> {
    type NoteRow = (
        String,
        String,
        Option<String>,
        String,
        Option<i64>,
        i64,
        String,
    );
    let rows: Vec<NoteRow> = sqlx::query_as(
        "SELECT s.child_id, n.text, n.view, s.stop, s.rounds, n.created_at, e.title FROM lead_stuck s
         JOIN lead_notes n ON n.id = s.note_id JOIN tickets c ON c.id = s.child_id
         JOIN tickets e ON e.id = s.epic_id
         WHERE c.project_id = ? AND s.state = ?",
    )
    .bind(project_id)
    .bind(StuckState::Noted.as_str())
    .fetch_all(board.store().pool())
    .await?;
    Ok(rows
        .into_iter()
        .filter_map(|(child, text, view, stop, rounds, at, epic_title)| {
            let stop: StuckStop = stop.parse().ok()?;
            Some((
                child,
                LeadNoteSent {
                    lead: lead_name(&epic_title),
                    text,
                    view,
                    stop,
                    after_rounds: rounds
                        .filter(|_| stop == StuckStop::ValidationFailed)
                        .and_then(|n| u32::try_from(n).ok()),
                    at,
                },
            ))
        })
        .collect())
}

/// The statuses of a child's own stops, whose inbox item carries the lead's note once it was sent.
pub fn is_stop(status: Status) -> bool {
    matches!(
        status,
        Status::CheckFailed | Status::DeliveryStopped | Status::Idle
    )
}

/// Holds stops for leads and carries out the lead's note. Cheap to clone.
#[derive(Clone)]
pub struct Stuck {
    board: Board,
    leads: EpicLeads,
    signals: Signals,
    validation: Validation,
    deliveries: Deliveries,
    sessions: Sessions,
    events: EventBus,
}

/// The service's parts [`Stuck`] works with.
pub struct StuckParts {
    pub board: Board,
    pub leads: EpicLeads,
    pub signals: Signals,
    pub validation: Validation,
    pub deliveries: Deliveries,
    pub sessions: Sessions,
    pub events: EventBus,
}

impl Stuck {
    pub fn new(parts: StuckParts) -> Self {
        let StuckParts {
            board,
            leads,
            signals,
            validation,
            deliveries,
            sessions,
            events,
        } = parts;
        Self {
            board,
            leads,
            signals,
            validation,
            deliveries,
            sessions,
            events,
        }
    }

    /// Registers the hand-back of held stops (once, at service start).
    pub fn register_hand_back(&self) {
        let this = self.clone();
        self.leads.on_hand_back(
            OWNER,
            std::sync::Arc::new(move |handover: Handover| {
                let this = this.clone();
                Box::pin(async move {
                    match hand_back(&this.board, &this.signals, &this.sessions, &handover.reference).await {
                        Ok(true) => this.changed(&[&handover.child, &handover.epic]).await,
                        Ok(false) => {}
                        Err(e) => {
                            tracing::warn!(child = %handover.child, error = %e, "can't hand the stop back");
                        }
                    }
                })
            }),
        );
    }

    /// The lead's note `note` (stored already) reached its child: when the child's stop waits for the lead, the note
    /// answers it as Answer the agent would, and the handover is settled. Whether it answered a held stop; false
    /// leaves the note an ordinary unread note.
    pub async fn answer(&self, note: &LeadNote, actor: &Actor) -> Result<bool, BoardError> {
        let Some(held) = find(&self.board, &note.child_id).await? else {
            return Ok(false);
        };
        if held.state != StuckState::Held
            || !resolve(
                &self.board,
                &note.child_id,
                StuckState::Noted,
                Some(&note.id),
            )
            .await?
        {
            return Ok(false);
        }
        self.leads.settle_handover(OWNER, &note.child_id);
        log(&self.board, &held, StuckState::Noted, actor).await?;
        let child = self.board.ticket_by_id(&note.child_id).await?;
        let key = child.key.to_string();
        let agent = self.sessions.running_of(&child.id, SessionKind::Agent);
        // The note goes with the stop's own action; with no agent session to type into, it stays unread for the
        // agent's `otaq prime` / `otaq notes`.
        let mut with_stop = true;
        match held.first.stop {
            StuckStop::ValidationFailed => {
                self.validation
                    .lead_answer(&key, &note.lead, &note.text)
                    .await?;
            }
            StuckStop::KeepsConflicting | StuckStop::KeepsFailingMergeCheck => {
                with_stop = self
                    .type_in(agent.as_ref(), DeliveryKind::AgentMessage, note)
                    .await?;
                self.retry_merge(&child, actor).await?;
            }
            StuckStop::Idle => {
                with_stop = self
                    .type_in(agent.as_ref(), DeliveryKind::LeadNote, note)
                    .await?;
            }
        }
        if with_stop {
            mark_read(
                &self.board,
                std::slice::from_ref(&note.id),
                ReadHow::WithStop,
            )
            .await?;
        }
        tracing::info!(child = %held.child, stop = %held.first.stop, "the lead's note answered the stop");
        self.changed(&[&held.child, &held.epic]).await;
        Ok(true)
    }

    /// Queues the note for the agent's session (typed once it is idle with an empty input line). Whether there was
    /// one.
    async fn type_in(
        &self,
        agent: Option<&crate::session::SessionInfo>,
        kind: DeliveryKind,
        note: &LeadNote,
    ) -> Result<bool, BoardError> {
        let Some(session) = agent else {
            return Ok(false);
        };
        self.deliveries
            .queue_delivery(&session.id, kind, &note_text(&note.lead, &note.text))
            .await
            .map_err(|e| BoardError::refused(crate::board::Refusal::SessionEnded, e.to_string()))?;
        Ok(true)
    }

    /// Keeps conflicting, answered: the conflict counter restarts and the Delivery runs again (FLW-17, as Retry
    /// merge): an automatic one merges now, a click one waits for the user's. A new conflict goes back to the agent
    /// with restack, which has the note by then.
    async fn retry_merge(&self, child: &Ticket, actor: &Actor) -> Result<(), BoardError> {
        if !matches!(child.status, Status::Working | Status::WorkingInBackground) {
            return Ok(());
        }
        self.board.to_delivery(child, actor).await?;
        self.board.delivery().arrived(&child.id).await;
        Ok(())
    }

    /// Clients refetch these tickets (cards, History, inbox).
    async fn changed(&self, keys: &[&TicketKey]) {
        for key in keys {
            if let Ok(t) = self.board.ticket(&key.to_string()).await {
                self.events.publish(Event::TicketChanged {
                    project: t.key.project.to_string(),
                    id: t.id.clone(),
                    key: t.key.clone(),
                });
                self.events.publish(Event::CardStatusChanged {
                    project: t.key.project.to_string(),
                    id: t.id,
                    key: t.key,
                    status: t.status,
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wake_up_texts() {
        let cmd = naming::COMMAND_NAME;
        let child: TicketKey = "SHOP-40.3".parse().unwrap();
        assert_eq!(
            wake_text(
                &child,
                &FirstStop::validation_failed(
                    "orders.test.ts expects `list`\n- it gets `items`",
                    3
                )
            ),
            format!(
                "SHOP-40.3 stopped: Validation failed 3 times. Last report: orders.test.ts expects `list` \u{2026}. \
                 Send it one note with `{cmd} note SHOP-40.3 --stdin`, or leave it to the user."
            )
        );
        assert_eq!(
            wake_text(&child, &FirstStop::validation_failed("Two tests fail.", 1)),
            format!(
                "SHOP-40.3 stopped: Validation failed 1 time. Last report: Two tests fail. Send it one note with \
                 `{cmd} note SHOP-40.3 --stdin`, or leave it to the user."
            )
        );
        let branch = naming::branch("SHOP-40");
        assert_eq!(
            wake_text(&child, &FirstStop::keeps_conflicting(&branch)),
            format!(
                "SHOP-40.3 stopped: Keeps conflicting with `{branch}`. Send it one note with `{cmd} note \
                 SHOP-40.3 --stdin`, or leave it to the user."
            )
        );
        assert_eq!(
            wake_text(
                &child,
                &FirstStop::idle(Some("Should I also update the tests?"))
            ),
            format!(
                "SHOP-40.3 stopped: Idle. Its last message: Should I also update the tests?. Send it one note with \
                 `{cmd} note SHOP-40.3 --stdin`, or leave it to the user."
            )
        );
        assert_eq!(
            wake_text(&child, &FirstStop::idle(None)),
            format!(
                "SHOP-40.3 stopped: Idle. Send it one note with `{cmd} note SHOP-40.3 --stdin`, or leave it to the \
                 user."
            )
        );
    }

    #[test]
    fn a_long_line_is_cut() {
        let long = "x".repeat(200);
        let cut = summary(&long);
        assert!(cut.ends_with(" \u{2026}"));
        assert_eq!(cut.chars().count(), 122);
        assert_eq!(
            note_text("Wishlist lead", " Fix the test. "),
            "Note from Wishlist lead: Fix the test."
        );
    }

    #[test]
    fn only_a_childs_own_stops_carry_the_note() {
        for status in [Status::CheckFailed, Status::DeliveryStopped, Status::Idle] {
            assert!(is_stop(status), "{status:?}");
        }
        for status in [
            Status::AsksYou,
            Status::NeedsYou,
            Status::Working,
            Status::CheckNoResult,
        ] {
            assert!(!is_stop(status), "{status:?}");
        }
    }
}
