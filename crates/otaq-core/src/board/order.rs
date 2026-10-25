//! Pinning and the order of cards in each column (TKT-06, SDD §7.5; d0016, d0037, d0042, d0047, d0065, d0066).
//!
//! Every column: pinned first, most recently pinned first. A card that needs the user floats to the top of Dev
//! (d0047 changelog), so Dev reads: pinned → turquoise (Asks you / Needs you / Idle) → Interrupted / Resume failed →
//! working (incl. in background, waiting for limit reset). **Ready for Dev** lists its needs-you cards (Start
//! interrupted, Parent closed) first, then the **start order** the scheduler uses (SCH-01, d0065): pinned (most
//! recently pinned first) → priority 1→4 → ready the longest.
//! Inside a group of the other columns, tickets that block more tickets come first (direct and indirect
//! dependents, as in Needs you, d0016); ties go to the oldest ticket (created first, then the lower key), so the
//! order is stable. Pinning changes board and start order only: never the column, the priority, Needs you or the
//! agent (d0047, d0065).

use std::cmp::Reverse;
use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};
use serde_json::json;

use super::readiness::readiness;
use super::tickets::{EventKind, Ticket, record_event};
use super::{Board, BoardError};
use crate::domain::{Actor, Status, StatusSet, TicketKey};
use crate::store::now_ms;

/// A board group's cards in board order ([`StatusSet::BOARD`]: the default flow's columns).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ColumnOrder {
    pub column: StatusSet,
    pub cards: Vec<TicketKey>,
}

/// Does this status need the user (needs-you family or Needs you's second group)? Such cards float to the top
/// of their column and are never folded.
pub fn needs_user(status: Status) -> bool {
    status.is_needs_you_family()
        || matches!(
            status,
            Status::Interrupted
                | Status::StartInterrupted
                | Status::ResumeFailed
                | Status::ParentClosed
        )
}

/// The group a card sorts in, lowest first.
fn group(ticket: &Ticket) -> u8 {
    if ticket.flags.is_pinned() {
        0
    } else if ticket.shows_needs_you() {
        1
    } else if ticket.needs_user() {
        2
    } else {
        3
    }
}

/// How many tickets of the project wait on each ticket, directly or indirectly.
pub fn blocked_counts(tickets: &[Ticket]) -> HashMap<TicketKey, usize> {
    let mut children: HashMap<&TicketKey, Vec<&TicketKey>> = HashMap::new();
    for t in tickets {
        for parent in &t.depends_on {
            children.entry(parent).or_default().push(&t.key);
        }
    }
    tickets
        .iter()
        .map(|t| {
            let mut seen: HashSet<&TicketKey> = HashSet::new();
            let mut stack = vec![&t.key];
            while let Some(at) = stack.pop() {
                for child in children.get(at).into_iter().flatten() {
                    if seen.insert(child) {
                        stack.push(child);
                    }
                }
            }
            seen.remove(&t.key);
            (t.key.clone(), seen.len())
        })
        .collect()
}

/// When a queued ticket became ready to start: the moment it was queued, or its last parent's merge if that
/// came later; `None` while it waits for a parent (SCH-01). The scheduler stores it ([`Ticket::ready_since`]).
pub fn ready_since(ticket: &Ticket, tickets: &[Ticket]) -> Option<i64> {
    if !readiness(ticket, tickets).is_ready() {
        return None;
    }
    let queued = ticket
        .queued_at
        .or(ticket.status_since)
        .unwrap_or(ticket.created_at);
    let last_merge = ticket
        .depends_on
        .iter()
        .filter_map(|key| tickets.iter().find(|t| &t.key == key))
        .filter_map(|parent| parent.status_since)
        .max();
    Some(last_merge.map_or(queued, |merged| merged.max(queued)))
}

/// The start order (SCH-01, d0065): pinned (most recently pinned first) → priority 1→4 → ready the longest; ties go to the older ticket, then the lower key.
/// Ready for Dev is sorted the same way.
pub fn start_order(ticket: &Ticket, tickets: &[Ticket]) -> impl Ord + use<> {
    let ready = ticket
        .ready_since
        .or_else(|| ready_since(ticket, tickets))
        .unwrap_or(i64::MAX);
    let pin = ticket.flags.pinned_at;
    (
        pin.is_none(),
        Reverse(pin.unwrap_or(0)),
        ticket.priority,
        ready,
        ticket.created_at,
        ticket.key.clone(),
    )
}

/// Orders one board group (its status set's board group, [`StatusSet::board_group`]). `tickets` is the whole
/// project (needed for the "blocks more" counts and readiness).
pub fn sort_column(column: StatusSet, tickets: &[Ticket]) -> ColumnOrder {
    let column = column.board_group();
    let mut in_column: Vec<&Ticket> = tickets
        .iter()
        .filter(|t| t.status.set().board_group() == column)
        .collect();
    if column == StatusSet::Waiting {
        // Needs-you cards (Start interrupted, Parent closed) first, then the start order (d0066).
        in_column.sort_by_cached_key(|t| (!t.needs_user(), start_order(t, tickets)));
    } else {
        let blocks = blocked_counts(tickets);
        in_column.sort_by_key(|t| {
            (
                group(t),
                Reverse(t.flags.pinned_at.unwrap_or(0)),
                Reverse(blocks.get(&t.key).copied().unwrap_or(0)),
                t.created_at,
                t.key.clone(),
            )
        });
    }
    ColumnOrder {
        column,
        cards: in_column.into_iter().map(|t| t.key.clone()).collect(),
    }
}

/// The Dev column with its fold applied: at most `max_cards` cards are shown, but cards that need the user are
/// never folded; the fold covers only working cards ("4 more · working").
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DevFold {
    pub shown_cards: Vec<TicketKey>,
    pub hidden_working: usize,
}

impl DevFold {
    pub fn hidden(&self) -> usize {
        self.hidden_working
    }

    /// The fold row's text, "4 more · working"; `None` when nothing is folded.
    pub fn label(&self) -> Option<String> {
        (self.hidden_working > 0).then(|| format!("{} more · working", self.hidden_working))
    }
}

/// Folds a sorted Dev column to `max_cards`. Cards that need the user are always shown.
pub fn fold_dev(order: &ColumnOrder, tickets: &[Ticket], max_cards: usize) -> DevFold {
    let status: HashMap<&TicketKey, Status> = tickets.iter().map(|t| (&t.key, t.status)).collect();
    let needs = |k: &TicketKey| status.get(k).copied().is_some_and(needs_user);
    let always = order.cards.iter().filter(|k| needs(k)).count();
    let mut room = max_cards.saturating_sub(always);
    let mut fold = DevFold {
        shown_cards: Vec::new(),
        hidden_working: 0,
    };
    for key in &order.cards {
        if needs(key) {
            fold.shown_cards.push(key.clone());
        } else if room > 0 {
            room -= 1;
            fold.shown_cards.push(key.clone());
        } else {
            fold.hidden_working += 1;
        }
    }
    fold
}

impl Board {
    /// Pins a ticket to the top of its column; pinning again bumps it above the other pins.
    pub async fn pin(&self, key: &str, actor: &Actor) -> Result<Ticket, BoardError> {
        self.set_pin(key, Some(now_ms()), actor).await
    }

    pub async fn unpin(&self, key: &str, actor: &Actor) -> Result<Ticket, BoardError> {
        self.set_pin(key, None, actor).await
    }

    async fn set_pin(
        &self,
        key: &str,
        pinned_at: Option<i64>,
        actor: &Actor,
    ) -> Result<Ticket, BoardError> {
        let ticket = self.ticket(key).await?;
        // Pins made within the same millisecond still order by when they were made.
        let latest: Option<i64> = sqlx::query_scalar(
            "SELECT MAX(pinned_at) FROM tickets
             WHERE project_id = (SELECT project_id FROM tickets WHERE id = ?)",
        )
        .bind(&ticket.id)
        .fetch_one(self.store.pool())
        .await?;
        let pinned_at = pinned_at.map(|at| at.max(latest.unwrap_or(0) + 1));
        let mut tx = self.store.begin_write().await?;
        sqlx::query("UPDATE tickets SET pinned_at = ? WHERE id = ?")
            .bind(pinned_at)
            .bind(&ticket.id)
            .execute(&mut *tx)
            .await?;
        let kind = if pinned_at.is_some() {
            EventKind::Pinned
        } else {
            EventKind::Unpinned
        };
        record_event(&mut tx, &ticket.id, kind, actor, json!({})).await?;
        tx.commit().await?;
        self.ticket_by_id(&ticket.id).await
    }
}
