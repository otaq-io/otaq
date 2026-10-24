//! The lead's suggestions (LEAD-08; d0083): an epic lead never closes or deletes a child itself. It suggests it with
//! `otaq suggest close|delete <KEY> --stdin`, the reason on stdin, and the user decides from Needs you:
//! - the suggestion is a needs-you item in its own group right after group 1 ([`NeedsYouGroup::Suggestions`]): "Wishlist
//!   lead suggests closing SHOP-40.6 Wishlist export (CSV)", with the reason;
//! - it blocks no agent, so it sends no OS notification, doesn't change the lead's status and doesn't turn its track
//!   turquoise;
//! - **Confirm** runs the usual close (d0044) or delete (d0060) of the child as the user; **Dismiss** only marks it.
//!   Both write History on the child and the epic ("Suggested closing SHOP-40.6 · closed by you"). Dismiss doesn't
//!   wake the lead; `prime --epic` lists dismissed suggestions ([`list_for_epic`]) so it doesn't suggest the same
//!   again.
//!
//! One open suggestion per child and kind. Only a never-started child can be suggested (CLI-07), and the item
//! disappears when the child starts or closes ([`shown`]) or is deleted (its rows go with it).
//!
//! [`NeedsYouGroup::Suggestions`]: crate::board::needs_you::NeedsYouGroup::Suggestions

use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::board::epic_flight::is_never_started;
use crate::board::tickets::{EventKind, Ticket, key_from_parts, record_event};
use crate::board::{Board, BoardError, Refusal};
use crate::domain::{Actor, TicketKey, stored_enum};
use crate::project_agents::lead::lead_name;
use crate::store::{new_id, now_ms};

stored_enum! {
    /// What the lead suggests doing with a child (`lead_suggestions.kind`).
    pub enum SuggestionKind ("suggestion kind") {
        /// Close it without merging (d0044).
        Close = "close",
        /// Delete it (d0060); only a never-started child.
        Delete = "delete",
    }
}

impl SuggestionKind {
    /// "closing", "deleting": the verb in "Wishlist lead suggests closing SHOP-40.6".
    pub fn gerund(self) -> &'static str {
        match self {
            Self::Close => "closing",
            Self::Delete => "deleting",
        }
    }

    /// "closed", "deleted": what Confirm did ("closed by you").
    pub fn done(self) -> &'static str {
        match self {
            Self::Close => "closed",
            Self::Delete => "deleted",
        }
    }
}

stored_enum! {
    /// Where a suggestion stands (`lead_suggestions.state`).
    pub enum SuggestionState ("suggestion state") {
        /// Waits for the user in Needs you.
        Open = "open",
        /// The user closed or deleted the child.
        Confirmed = "confirmed",
        /// The user dismissed it; the lead sees it in `prime --epic`.
        Dismissed = "dismissed",
    }
}

/// One suggestion of a lead.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Suggestion {
    pub id: String,
    pub epic: TicketKey,
    pub epic_id: String,
    pub child: TicketKey,
    pub child_id: String,
    pub child_title: String,
    pub kind: SuggestionKind,
    /// The lead's reason, from stdin.
    pub reason: String,
    pub state: SuggestionState,
    /// The lead's actor (`project-agent:<id>`).
    pub suggested_by: Actor,
    pub created_at: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolved_by: Option<Actor>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolved_at: Option<i64>,
}

impl Suggestion {
    /// "Wishlist lead suggests closing SHOP-40.6 Wishlist export (CSV)" (*10 → Row · Epic lead items (0083)*).
    pub fn headline(&self, lead: &str) -> String {
        headline(lead, self.kind, &self.child, &self.child_title)
    }
}

/// "Wishlist lead suggests closing SHOP-40.6 Wishlist export (CSV)".
pub fn headline(lead: &str, kind: SuggestionKind, child: &TicketKey, title: &str) -> String {
    format!("{lead} suggests {} {child} {title}", kind.gerund())
}

/// The reply to `otaq suggest`: nothing waits for it.
pub fn suggested_message(suggestion: &Suggestion) -> String {
    format!(
        "Suggested {} {}. The user decides from Needs you; nothing waits for it, so carry on.",
        suggestion.kind.gerund(),
        suggestion.child
    )
}

/// The refusal of a suggestion without a reason.
pub const MISSING_REASON: &str = "Give the reason on stdin: why the child should go";

#[derive(sqlx::FromRow)]
struct Row {
    id: String,
    epic_id: String,
    child_id: String,
    kind: String,
    reason: String,
    state: String,
    suggested_by: String,
    created_at: i64,
    resolved_by: Option<String>,
    resolved_at: Option<i64>,
    project_key: String,
    epic_number: i64,
    child_number: i64,
    child_title: String,
}

const SELECT: &str = "SELECT s.*, p.key AS project_key, c.number AS epic_number,
                             c.child_number, c.title AS child_title
                      FROM lead_suggestions s
                      JOIN tickets c ON c.id = s.child_id
                      JOIN projects p ON p.id = c.project_id";

impl Row {
    fn into_suggestion(self) -> Option<Suggestion> {
        Some(Suggestion {
            epic: key_from_parts(&self.project_key, self.epic_number, 0)?,
            child: key_from_parts(&self.project_key, self.epic_number, self.child_number)?,
            id: self.id,
            epic_id: self.epic_id,
            child_id: self.child_id,
            child_title: self.child_title,
            kind: self.kind.parse().ok()?,
            reason: self.reason,
            state: self.state.parse().ok()?,
            suggested_by: self.suggested_by.parse().expect("infallible"),
            created_at: self.created_at,
            resolved_by: self.resolved_by.map(|a| a.parse().expect("infallible")),
            resolved_at: self.resolved_at,
        })
    }
}

async fn select(
    board: &Board,
    filter: &str,
    bind: Option<&str>,
) -> Result<Vec<Suggestion>, BoardError> {
    let sql = format!("{SELECT} {filter} ORDER BY s.created_at, s.id");
    let mut query = sqlx::query_as::<_, Row>(sqlx::AssertSqlSafe(sql));
    if let Some(bind) = bind {
        query = query.bind(bind.to_owned());
    }
    Ok(query
        .fetch_all(board.store().pool())
        .await?
        .into_iter()
        .filter_map(Row::into_suggestion)
        .collect())
}

/// Stores the lead's suggestion to close or delete `child`, a child of the epic `epic_id` (the caller's epic; the
/// policy checked it's a never-started child, CLI-07). Refused without a reason, for a ticket outside the epic, and
/// while the same suggestion is still open.
pub async fn suggest(
    board: &Board,
    epic_id: &str,
    child: &str,
    kind: SuggestionKind,
    reason: &str,
    lead: &str,
    actor: &Actor,
) -> Result<Suggestion, BoardError> {
    let reason = reason.trim();
    if reason.is_empty() {
        return Err(BoardError::refused(Refusal::EmptyField, MISSING_REASON));
    }
    let ticket = board.ticket(child).await?;
    let epic = board.ticket_by_id(epic_id).await?;
    if ticket.epic.as_ref() != Some(&epic.key) || !is_never_started(&ticket) {
        return Err(BoardError::refused(
            Refusal::NotAllowed,
            crate::policy::lead_refusal(lead, &epic.key),
        ));
    }
    let open: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM lead_suggestions WHERE child_id = ? AND kind = ? AND state = 'open'",
    )
    .bind(&ticket.id)
    .bind(kind.as_str())
    .fetch_one(board.store().pool())
    .await?;
    if open > 0 {
        return Err(BoardError::refused(
            Refusal::AlreadyAdded,
            format!(
                "You already suggested {} {}; the user hasn't decided yet",
                kind.gerund(),
                ticket.key
            ),
        ));
    }
    let id = new_id();
    sqlx::query(
        "INSERT INTO lead_suggestions (id, epic_id, child_id, kind, reason, suggested_by, created_at)
         VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(epic_id)
    .bind(&ticket.id)
    .bind(kind.as_str())
    .bind(reason)
    .bind(actor.to_string())
    .bind(now_ms())
    .execute(board.store().pool())
    .await?;
    tracing::info!(child = %ticket.key, %kind, actor = %actor, "lead suggestion");
    get(board, &id).await
}

/// One suggestion by id.
pub async fn get(board: &Board, id: &str) -> Result<Suggestion, BoardError> {
    select(board, "WHERE s.id = ?", Some(id))
        .await?
        .pop()
        .ok_or_else(|| BoardError::NotFound(format!("unknown suggestion {id}")))
}

/// Every suggestion of the epic `epic` (by key), open, confirmed and dismissed, oldest first: `prime --epic` lists
/// the dismissed ones so the lead doesn't suggest them again (T-0283).
pub async fn list_for_epic(board: &Board, epic: &str) -> Result<Vec<Suggestion>, BoardError> {
    let epic_id = board.ticket_id(epic).await?;
    select(board, "WHERE s.epic_id = ?", Some(&epic_id)).await
}

/// Every open suggestion, oldest first; Needs you keeps those still [`shown`].
pub async fn open(board: &Board) -> Result<Vec<Suggestion>, BoardError> {
    select(board, "WHERE s.state = 'open'", None).await
}

/// Whether an open suggestion is still a needs-you item: its child hasn't started or closed meanwhile.
pub fn shown(suggestion: &Suggestion, child: &Ticket) -> bool {
    suggestion.state == SuggestionState::Open && is_never_started(child)
}

/// An open suggestion, or why it can't be confirmed or dismissed any more.
pub async fn open_one(board: &Board, id: &str) -> Result<Suggestion, BoardError> {
    let suggestion = get(board, id).await?;
    let child = board.ticket_by_id(&suggestion.child_id).await?;
    if suggestion.state != SuggestionState::Open {
        return Err(BoardError::refused(
            Refusal::NotAllowed,
            format!(
                "The suggestion to {} {} was already {}",
                suggestion.kind,
                suggestion.child,
                suggestion.state.as_str()
            ),
        ));
    }
    if !shown(&suggestion, &child) {
        return Err(BoardError::refused(
            Refusal::Started,
            format!("{} has started or closed meanwhile", suggestion.child),
        ));
    }
    Ok(suggestion)
}

/// Records what the user did with the suggestion: `Confirmed` after its close or delete ran, or `Dismissed`. The
/// state changes (unless a delete took the row with the child) and "Suggested closing SHOP-40.6 · closed by you"
/// goes in the epic's History, and in the child's when it still exists.
pub async fn resolve(
    board: &Board,
    suggestion: &Suggestion,
    state: SuggestionState,
    actor: &Actor,
) -> Result<(), BoardError> {
    let epic = board.ticket_by_id(&suggestion.epic_id).await?;
    let outcome = match state {
        SuggestionState::Confirmed => suggestion.kind.done(),
        SuggestionState::Dismissed => "dismissed",
        SuggestionState::Open => return Ok(()),
    };
    let payload = json!({
        "lead": lead_name(&epic.title),
        "child": suggestion.child,
        "kind": suggestion.kind,
        "outcome": outcome,
        "reason": suggestion.reason,
    });
    let mut tx = board.store().begin_write().await?;
    let child_left = sqlx::query(
        "UPDATE lead_suggestions SET state = ?, resolved_by = ?, resolved_at = ? WHERE id = ?",
    )
    .bind(state.as_str())
    .bind(actor.to_string())
    .bind(now_ms())
    .bind(&suggestion.id)
    .execute(&mut *tx)
    .await?
    .rows_affected()
        > 0;
    if child_left {
        record_event(
            &mut tx,
            &suggestion.child_id,
            EventKind::LeadSuggestion,
            actor,
            payload.clone(),
        )
        .await?;
    }
    record_event(
        &mut tx,
        &suggestion.epic_id,
        EventKind::LeadSuggestion,
        actor,
        payload,
    )
    .await?;
    tx.commit().await?;
    tracing::info!(child = %suggestion.child, outcome, "lead suggestion resolved");
    Ok(())
}
