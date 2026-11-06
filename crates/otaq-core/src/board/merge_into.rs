//! **Merge into another ticket** (the board side): which tickets can take another ticket's branch, the bookkeeping once
//! one did, and which tickets went into which. The git work and the cleanup are [`crate::review::Review::merge_into`].
//!
//! - **Who can be merged in**: a ticket whose Approve menu shows, waiting in the human check whose Pass offers the
//!   Delivery or at the Delivery, with a branch and no PR open (a ticket with an open PR only pushes to it).
//! - **Who can take it** (the host): a ticket of the same project and base that isn't Done, has a branch, and has no
//!   agent at work: it waits in a human check, at its Delivery, or with a PR open, and no automatic Delivery is under
//!   way. A host at its Delivery needs a human check before the Delivery to go back to.
//! - **Afterwards**: the ticket merged in waits as Merged into in its Delivery stage. It isn't Done, so tickets waiting
//!   on it keep waiting. The host goes back to the human check before its Delivery (one waiting in a human check
//!   stays there), its review starting again on the combined branch. The link is in the `merged_into` table
//!   ([`crate::store::merged_into`]).

use serde_json::json;

use super::tickets::{EventKind, Ticket, record_event};
use super::{Board, BoardError, Refusal};
use crate::domain::{Actor, Status};
use crate::flow::engine::{self, Move};
use crate::store::merged_into::{self, MergedIn};
use crate::store::now_ms;
use crate::store::tickets::set_state_since;

/// Whether `ticket` can be merged into another ticket now: its Approve menu shows (Ready for Review before the
/// Delivery, or at the Delivery), it has a branch, and no PR is open for it.
pub fn can_be_merged_in(ticket: &Ticket) -> bool {
    matches!(ticket.status, Status::WaitingForYou | Status::ToDeliver)
        && ticket.flags.pr_number.is_none()
        && branch_of(ticket).is_some()
}

/// Whether `host` can take `ticket`'s branch, from what the tickets say: same project and base, not Done, a branch,
/// and no agent at work (waiting in a human check, at its Delivery or with a PR open, with no automatic Delivery
/// under way). [`Board::merge_into_hosts`] also needs a human check before the Delivery for a host at its Delivery.
pub fn can_take(ticket: &Ticket, host: &Ticket) -> bool {
    host.id != ticket.id
        && host.key.project == ticket.key.project
        && host.base.branch() == ticket.base.branch()
        && matches!(
            host.status,
            Status::WaitingForYou | Status::ToDeliver | Status::PrOpen
        )
        && !host.flags.delivering_automatically
        && branch_of(host).is_some()
}

fn branch_of(ticket: &Ticket) -> Option<&str> {
    ticket.work.as_ref().and_then(|w| w.branch.as_deref())
}

impl Board {
    /// The tickets `key` can be merged into (the picker), in key order.
    pub async fn merge_into_hosts(&self, key: &str) -> Result<Vec<Ticket>, BoardError> {
        let ticket = self.ticket(key).await?;
        let flow = self.effective_flow(ticket.key.project.as_str()).await?;
        let back_to_review = engine::human_check_before_delivery(&flow).is_some();
        let mut hosts: Vec<Ticket> = self
            .tickets(ticket.key.project.as_str())
            .await?
            .into_iter()
            .filter(|host| can_take(&ticket, host))
            .filter(|host| back_to_review || host.status == Status::WaitingForYou)
            .collect();
        hosts.sort_by(|a, b| a.key.cmp(&b.key));
        Ok(hosts)
    }

    /// The host `key` was merged into, if it was.
    pub async fn merged_into(&self, key: &str) -> Result<Option<MergedIn>, BoardError> {
        let id = self.ticket_id(key).await?;
        Ok(merged_into::of_ticket(self.store().pool(), &id).await?)
    }

    /// Every ticket merged into the host `key`'s branch, oldest first.
    pub async fn merged_in(&self, key: &str) -> Result<Vec<MergedIn>, BoardError> {
        let id = self.ticket_id(key).await?;
        Ok(merged_into::of_host(self.store().pool(), &id).await?)
    }

    /// After the squash landed on `host`'s branch as `commit` (its head now): `ticket` waits as Merged into `host`,
    /// and `host` goes back to its review with its review starting again at `commit`. One transaction; the ticket's
    /// cleanup (its worktree goes, its branch stays) is the caller's.
    pub(crate) async fn record_merged_into(
        &self,
        ticket: &Ticket,
        host: &Ticket,
        commit: &str,
        actor: &Actor,
    ) -> Result<(Ticket, Ticket), BoardError> {
        let into = self.moved(ticket, ticket.status, Move::MergedInto).await?;
        let back = self.moved(host, host.status, Move::MergedIn).await?;
        if into.status != Status::MergedInto {
            return Err(BoardError::refused(
                Refusal::MoveRefused,
                format!("{} can't wait for {}", ticket.key, host.key),
            ));
        }
        let now = now_ms();
        let mut tx = self.store().begin_write().await?;
        into.record(&mut tx, &ticket.id, None).await?;
        sqlx::query(concat!(
            "UPDATE tickets SET stage_id = ?, state = ?, conflicts_with = NULL, conflict_files = NULL,
                                conflict_reason = NULL, sent_back_at = NULL, resume_skill = NULL, signal_text = NULL,
                                signal_since = NULL, wakeup_at = NULL, updated_by = ?, updated_at = ?, ",
            set_state_since!(),
            " WHERE id = ?"
        ))
        .bind(into.place.stage_id())
        .bind(Status::MergedInto.as_str())
        .bind(actor.to_string())
        .bind(now)
        .bind(Status::MergedInto.as_str())
        .bind(now)
        .bind(&ticket.id)
        .execute(&mut *tx)
        .await?;
        merged_into::record(&mut tx, &ticket.id, &host.id, commit, now).await?;
        record_event(
            &mut tx,
            &ticket.id,
            EventKind::MergedInto,
            actor,
            json!({ "from": ticket.status, "into": host.key, "commit": commit }),
        )
        .await?;
        back.record(&mut tx, &host.id, None).await?;
        sqlx::query(concat!(
            "UPDATE tickets SET stage_id = ?, state = ?, review_head_sha = ?, review_entered_at = ?,
                                conflicts_with = NULL, conflict_files = NULL, conflict_reason = NULL,
                                updated_by = ?, updated_at = ?, ",
            set_state_since!(),
            " WHERE id = ?"
        ))
        .bind(back.place.stage_id())
        .bind(back.status.as_str())
        .bind(commit)
        .bind(now)
        .bind(actor.to_string())
        .bind(now)
        .bind(back.status.as_str())
        .bind(now)
        .bind(&host.id)
        .execute(&mut *tx)
        .await?;
        record_event(
            &mut tx,
            &host.id,
            EventKind::MergedIn,
            actor,
            json!({
                "from": host.status,
                "ticket": ticket.key,
                "title": ticket.title,
                "commit": commit,
            }),
        )
        .await?;
        tx.commit().await?;
        tracing::info!(key = %ticket.key, into = %host.key, commit, "merged into another ticket");
        Ok((
            self.ticket_by_id(&ticket.id).await?,
            self.ticket_by_id(&host.id).await?,
        ))
    }
}
