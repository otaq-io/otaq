//! Epic bookkeeping (EPC-01, EPC-06, EPC-07, TKT-07; d0039, d0044, d0059): children, Move to epic with a key
//! alias, progress and closing.
//!
//! Children are created with `create_ticket` and `epic` set (SHOP-40.1, 40.2, …). Move to epic is only for
//! tickets that never went to Dev: the ticket takes the epic's next child key and the epic's branch, and its old
//! key keeps resolving (d0044). A child can't move to another epic. Closing an epic closes its unfinished
//! children after one confirmation built from [`ClosePlan`]; merged children stay merged. Stopping agents and
//! removing worktrees on close come with sessions (T-0100).

use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::{Sqlite, Transaction};

use super::deps::check_same_base;
use super::epic_flight::epic_stop;
use super::tickets::{Base, EventKind, Ticket, allocate_child_number, record_event};
use super::{Board, BoardError, Refusal};
use crate::domain::{Actor, Status, StatusSet, TicketKey};
use crate::store::now_ms;
use crate::store::tickets::set_state_since;

/// "2 of 3 merged"; children closed without merging are counted apart (#31).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct EpicProgress {
    pub merged: u32,
    /// Every child, the closed ones too.
    pub total: u32,
    /// Children closed without merging (Closed · not merged).
    pub closed: u32,
}

impl EpicProgress {
    /// "1 of 3 merged"; with closed children, "3 of 7 merged · 1 closed", and once every other child is merged
    /// "11 merged · 1 closed" (a finished epic doesn't look one ticket short, #31).
    pub fn label(&self) -> String {
        if self.closed == 0 {
            return format!("{} of {} merged", self.merged, self.total);
        }
        let open = self.total.saturating_sub(self.closed);
        if self.merged >= open {
            format!("{} merged \u{b7} {} closed", self.merged, self.closed)
        } else {
            format!(
                "{} of {open} merged \u{b7} {} closed",
                self.merged, self.closed
            )
        }
    }

    /// The progress of `children` (an epic's children).
    pub fn of<'a>(children: impl Iterator<Item = &'a Ticket> + Clone) -> Self {
        let count = |status: Status| {
            u32::try_from(children.clone().filter(|c| c.status == status).count())
                .unwrap_or(u32::MAX)
        };
        Self {
            merged: count(Status::Merged),
            total: u32::try_from(children.clone().count()).unwrap_or(u32::MAX),
            closed: count(Status::ClosedNotMerged),
        }
    }
}

/// A child that closing the epic would close.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClosingChild {
    pub key: TicketKey,
    pub title: String,
    pub status: Status,
    /// An agent works on it and would be stopped.
    pub agent_running: bool,
}

/// What the single "Close epic" confirmation lists (EPC-06).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClosePlan {
    pub epic: TicketKey,
    /// Unfinished children to close.
    pub close: Vec<ClosingChild>,
    /// Merged children; they stay merged.
    pub stay_merged: Vec<TicketKey>,
}

/// An epic's progress from its project's tickets.
pub fn progress_of(epic: &TicketKey, tickets: &[Ticket]) -> EpicProgress {
    EpicProgress::of(tickets.iter().filter(|t| t.epic.as_ref() == Some(epic)))
}

impl Board {
    /// The epic's children, by key.
    pub async fn epic_children(&self, epic: &str) -> Result<Vec<Ticket>, BoardError> {
        let epic = self.epic(epic).await?;
        Ok(self
            .tickets(epic.key.project.as_str())
            .await?
            .into_iter()
            .filter(|t| t.epic.as_ref() == Some(&epic.key))
            .collect())
    }

    /// Merged children out of all children.
    pub async fn epic_progress(&self, epic: &str) -> Result<EpicProgress, BoardError> {
        let epic = self.epic(epic).await?;
        let tickets = self.tickets(epic.key.project.as_str()).await?;
        Ok(progress_of(&epic.key, &tickets))
    }

    /// Moves a never-started ticket into an epic (EPC-07, TKT-07): it gets the epic's next child key, uses the
    /// epic's branch, and its old key keeps resolving.
    pub async fn move_to_epic(
        &self,
        key: &str,
        epic: &str,
        actor: &Actor,
    ) -> Result<Ticket, BoardError> {
        let ticket = self.ticket(key).await?;
        let epic = self.epic(epic).await?;
        if ticket.is_epic() {
            return Err(BoardError::refused(
                Refusal::EpicInEpic,
                format!(
                    "{} is an epic; epics can't be inside another epic",
                    ticket.key
                ),
            ));
        }
        if let Some(current) = &ticket.epic {
            return Err(BoardError::refused(
                Refusal::AlreadyInEpic,
                format!(
                    "{} is part of {current} and can't move to another epic",
                    ticket.key
                ),
            ));
        }
        if ticket.key.project != epic.key.project {
            return Err(BoardError::refused(
                Refusal::OtherProject,
                format!("{} belongs to another project", epic.key),
            ));
        }
        if self.has_been_to_dev(&ticket).await? {
            return Err(BoardError::refused(
                Refusal::Started,
                format!(
                    "{} has been to Dev; only tickets that never started can move into an epic",
                    ticket.key
                ),
            ));
        }
        if let Some(external_id) = &ticket.external_id {
            return Err(BoardError::refused(
                Refusal::ExternalIdOnChild,
                format!(
                    "{} has the External ID {external_id}; epic children have none. Remove it or set it on the epic first",
                    ticket.key
                ),
            ));
        }
        if ticket.depends_on.contains(&epic.key) {
            return Err(BoardError::refused(
                Refusal::DependsOnOwnEpic,
                format!(
                    "{} depends on {}; remove that link first",
                    ticket.key, epic.key
                ),
            ));
        }
        let moved = Ticket {
            base: Base::Epic(epic.key.clone()),
            ..ticket.clone()
        };
        for other in self.tickets(ticket.key.project.as_str()).await? {
            let linked =
                ticket.depends_on.contains(&other.key) || other.depends_on.contains(&ticket.key);
            if linked {
                check_same_base(&moved, &other)?;
            }
        }

        let mut tx = self.store.begin_write().await?;
        let child_number = allocate_child_number(&mut tx, &epic.id).await?;
        sqlx::query(
            "UPDATE tickets SET number = ?, child_number = ?, epic_id = ?, base_branch = NULL, updated_by = ?,
                                updated_at = ?
             WHERE id = ?",
        )
        .bind(i64::from(epic.key.number))
        .bind(child_number)
        .bind(&epic.id)
        .bind(actor.to_string())
        .bind(now_ms())
        .bind(&ticket.id)
        .execute(&mut *tx)
        .await?;
        sqlx::query("INSERT INTO ticket_key_aliases (old_key, ticket_id) VALUES (?, ?)")
            .bind(ticket.key.to_string())
            .bind(&ticket.id)
            .execute(&mut *tx)
            .await?;
        record_event(
            &mut tx,
            &ticket.id,
            EventKind::MovedToEpic,
            actor,
            json!({"epic": epic.key, "old_key": ticket.key, "old_base": ticket.base}),
        )
        .await?;
        tx.commit().await?;
        let moved = self.ticket_by_id(&ticket.id).await?;
        tracing::info!(old = %ticket.key, new = %moved.key, "ticket moved to epic");
        self.epic_back_to_backlog(&epic.key.to_string(), actor)
            .await?;
        Ok(moved)
    }

    /// Epics are delivery units (EPC-04, d0059): when every child is Done with at least one merged into the epic
    /// branch, the epic moves to its stop (STG-11, d0108 rule 7: the flow's last human check, the default flow's
    /// Ready for Review, or the implicit Review epic when the flow has none; [`super::epic_flight::epic_stop`]),
    /// where its branch head is the reviewed head; when every child ended
    /// Closed, the epic is Done · Closed · not merged (d0044). Bookkeeping, run after a child's state changes.
    /// Returns the epic's new status when it moved.
    pub async fn reevaluate_epic(
        &self,
        epic: &str,
        actor: &Actor,
    ) -> Result<Option<Status>, BoardError> {
        let epic = self.epic(epic).await?;
        if !epic.place.is_backlog() {
            return Ok(None);
        }
        let children = self.epic_children(&epic.key.to_string()).await?;
        if children.is_empty() || !children.iter().all(|c| c.place.is_done()) {
            return Ok(None);
        }
        let merged = children.iter().any(|c| c.status == Status::Merged);
        let stop = epic_stop(&self.effective_flow(epic.key.project.as_str()).await?);
        let mut tx = self.store.begin_write().await?;
        let to = if merged {
            let head = match epic.work.as_ref().and_then(|w| w.branch.clone()) {
                Some(branch) => {
                    let project = self.project(epic.key.project.as_str()).await?;
                    self.git()?
                        .resolve(
                            std::path::Path::new(&project.repo_path),
                            &format!("refs/heads/{branch}"),
                        )
                        .await?
                }
                None => None,
            };
            let now = now_ms();
            let to = Status::WaitingForYou;
            sqlx::query(concat!(
                "UPDATE tickets SET stage_id = ?, state = ?, review_head_sha = ?, review_entered_at = ?,
                                    updated_by = ?, updated_at = ?, ",
                set_state_since!(),
                " WHERE id = ?"
            ))
            .bind(&stop.id)
            .bind(to.as_str())
            .bind(head)
            .bind(now)
            .bind(actor.to_string())
            .bind(now)
            .bind(to.as_str())
            .bind(now)
            .bind(&epic.id)
            .execute(&mut *tx)
            .await?;
            record_event(
                &mut tx,
                &epic.id,
                EventKind::Moved,
                actor,
                json!({"from": epic.status, "to": to}),
            )
            .await?;
            to
        } else {
            close_bookkeeping(&mut tx, &epic.id, epic.status, actor).await?;
            Status::ClosedNotMerged
        };
        tx.commit().await?;
        tracing::info!(epic = %epic.key, %to, "every child is done");
        if to == Status::WaitingForYou {
            // From here the user reviews the whole epic: its lead ends (d0083).
            self.leads().epic_in_review(&epic.key, actor).await;
        }
        Ok(Some(to))
    }

    /// Adding (or reopening) a child returns an epic at its stop (Ready for Review, Review epic) or in its Delivery
    /// (Approved) to Backlog until that child is merged (EPC-05). The only way back for an epic: it can't be dragged
    /// back (d0108).
    pub async fn epic_back_to_backlog(&self, epic: &str, actor: &Actor) -> Result<(), BoardError> {
        let epic = self.epic(epic).await?;
        if !matches!(
            epic.status.set(),
            StatusSet::HumanCheck | StatusSet::Delivery
        ) {
            return Ok(());
        }
        let now = now_ms();
        let to = Status::Backlog;
        let mut tx = self.store.begin_write().await?;
        sqlx::query(concat!(
            "UPDATE tickets SET stage_id = NULL, state = ?, updated_by = ?, updated_at = ?, ",
            set_state_since!(),
            " WHERE id = ?"
        ))
        .bind(to.as_str())
        .bind(actor.to_string())
        .bind(now)
        .bind(to.as_str())
        .bind(now)
        .bind(&epic.id)
        .execute(&mut *tx)
        .await?;
        record_event(
            &mut tx,
            &epic.id,
            EventKind::Moved,
            actor,
            json!({"from": epic.status, "to": to}),
        )
        .await?;
        tx.commit().await?;
        tracing::info!(epic = %epic.key, "a new or reopened child: back to Backlog");
        // Its lead, ended at Review, resumes with no message (LEAD-12).
        self.leads().epic_back(&epic.key).await;
        Ok(())
    }

    /// What closing the epic would do, for its single confirmation (EPC-06).
    pub async fn epic_close_plan(&self, epic: &str) -> Result<ClosePlan, BoardError> {
        let epic = self.epic(epic).await?;
        let children = self.epic_children(&epic.key.to_string()).await?;
        let mut plan = ClosePlan {
            epic: epic.key.clone(),
            close: Vec::new(),
            stay_merged: Vec::new(),
        };
        for child in children {
            match child.status {
                Status::Merged => plan.stay_merged.push(child.key),
                Status::ClosedNotMerged => {}
                status => plan.close.push(ClosingChild {
                    agent_running: self.agent_running(&child.id).await?,
                    key: child.key,
                    title: child.title,
                    status,
                }),
            }
        }
        Ok(plan)
    }

    /// Closes the epic and its unfinished children (bookkeeping only). Merged children stay merged; the epic
    /// ends "Closed · not merged" (d0044).
    pub async fn close_epic(&self, epic: &str, actor: &Actor) -> Result<ClosePlan, BoardError> {
        let plan = self.epic_close_plan(epic).await?;
        let epic = self.epic(epic).await?;
        let mut ids = Vec::new();
        for child in &plan.close {
            ids.push((self.ticket_id(&child.key.to_string()).await?, child.status));
        }
        let mut tx = self.store.begin_write().await?;
        for (id, status) in &ids {
            close_bookkeeping(&mut tx, id, *status, actor).await?;
        }
        close_bookkeeping(&mut tx, &epic.id, epic.status, actor).await?;
        tx.commit().await?;
        tracing::info!(epic = %epic.key, closed = plan.close.len(), "epic closed");
        Ok(plan)
    }

    /// A ticket that must be an epic.
    async fn epic(&self, key: &str) -> Result<Ticket, BoardError> {
        let epic = self.ticket(key).await?;
        if !epic.is_epic() {
            return Err(BoardError::refused(
                Refusal::NotAnEpic,
                format!("{} isn't an epic", epic.key),
            ));
        }
        Ok(epic)
    }

    /// Whether a ticket has ever been to Dev: a start time, a branch, a worktree or a session row (d0044, d0060).
    pub(crate) async fn has_been_to_dev(&self, ticket: &Ticket) -> Result<bool, BoardError> {
        if ticket.has_started() {
            return Ok(true);
        }
        let traces: i64 = sqlx::query_scalar(
            "SELECT (SELECT COUNT(*) FROM sessions WHERE ticket_id = ?)
                  + (SELECT COUNT(*) FROM tickets WHERE id = ? AND (branch_name IS NOT NULL OR worktree_path IS NOT NULL))",
        )
        .bind(&ticket.id)
        .bind(&ticket.id)
        .fetch_one(self.store.pool())
        .await?;
        Ok(traces > 0)
    }

    /// Whether an agent session runs for the ticket.
    pub(crate) async fn agent_running(&self, ticket_id: &str) -> Result<bool, BoardError> {
        let n: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM sessions WHERE ticket_id = ? AND kind = 'agent' AND ended_at IS NULL",
        )
        .bind(ticket_id)
        .fetch_one(self.store.pool())
        .await?;
        Ok(n > 0)
    }
}

/// Moves a ticket to Done · Closed · not merged and logs it. Agents, worktrees and Run/Shell are handled with
/// sessions (T-0100).
pub(crate) async fn close_bookkeeping(
    tx: &mut Transaction<'_, Sqlite>,
    ticket_id: &str,
    from: Status,
    actor: &Actor,
) -> Result<(), BoardError> {
    close_bookkeeping_noting(tx, ticket_id, from, actor, json!({})).await
}

/// [`close_bookkeeping`] with more fields in the Closed event (`otaq stage done`'s `nothing_to_merge` and reason).
pub(crate) async fn close_bookkeeping_noting(
    tx: &mut Transaction<'_, Sqlite>,
    ticket_id: &str,
    from: Status,
    actor: &Actor,
    note: serde_json::Value,
) -> Result<(), BoardError> {
    let to = Status::ClosedNotMerged;
    let now = now_ms();
    // `otaq stage done`'s reason, for the card's "Closed · nothing to merge" (Q276).
    let nothing_to_merge = (note.get("nothing_to_merge") == Some(&serde_json::Value::Bool(true)))
        .then(|| {
            note.get("reason")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default()
                .to_owned()
        });
    sqlx::query(concat!(
        "UPDATE tickets SET stage_id = NULL, state = ?, outcome = 'closed', nothing_to_merge = ?, updated_by = ?, ",
        "updated_at = ?, ",
        set_state_since!(),
        " WHERE id = ?"
    ))
    .bind(to.as_str())
    .bind(nothing_to_merge)
    .bind(actor.to_string())
    .bind(now)
    .bind(to.as_str())
    .bind(now)
    .bind(ticket_id)
    .execute(&mut **tx)
    .await?;
    record_event(tx, ticket_id, EventKind::Closed, actor, {
        let mut payload = json!({"from": from});
        if let (Some(payload), serde_json::Value::Object(note)) = (payload.as_object_mut(), note) {
            payload.extend(note);
        }
        payload
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::EpicProgress;

    /// #31: a closed child is counted apart, so a finished epic doesn't look one ticket short.
    #[test]
    fn closed_children_are_counted_apart() {
        let p = |merged, total, closed| {
            EpicProgress {
                merged,
                total,
                closed,
            }
            .label()
        };
        assert_eq!(p(1, 3, 0), "1 of 3 merged");
        assert_eq!(p(11, 12, 1), "11 merged \u{b7} 1 closed");
        assert_eq!(p(3, 8, 1), "3 of 7 merged \u{b7} 1 closed");
        assert_eq!(p(0, 2, 2), "0 merged \u{b7} 2 closed");
    }
}
