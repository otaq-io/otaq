//! Manual checks for you (d0125): what an agent or a check couldn't check itself (a real browser, a device, a
//! service), one line each, listed with `--manual-check "<what to check>"` on `stage next`, `stage done` and `check
//! pass|fail`. The review shows them above Approve as a checklist the user ticks; it never blocks Approve.
//!
//! - Each item is one line of at most [`MANUAL_CHECK_LIMIT_CHARS`] characters ([`check_items`]), checked with the
//!   hand-over's comment before anything moves, and stored in the move's transaction ([`replace_in`]) with who added
//!   it (the comment's author), its stage and check round, and the comment it came with.
//! - A new round of the same stage replaces that stage's unticked items; ticked ones stay. A stage end with no
//!   `--manual-check` clears them: what the agent lists last is what is left.
//! - Ticking and unticking are the user's ([`Board::tick_manual_check`]), logged in History as
//!   `manual_check_ticked`.
//! - `n` numbers a ticket's items 1, 2, 3…, never reused: a replaced item keeps its row (`replaced_at`) and no list
//!   shows it. They go with their ticket.

use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::{FromRow, Sqlite, Transaction};

use super::ticket_comments::{COMMENT_LIMIT_CHARS, CommentAuthor};
use super::tickets::{EventKind, record_event};
use super::{Board, BoardError, Refusal};
use crate::agent_tools::Tool;
use crate::domain::{Actor, TicketKey};
use crate::store::{new_id, now_ms};

/// The longest manual check, in Unicode characters: a comment's limit (CMT-02).
pub const MANUAL_CHECK_LIMIT_CHARS: usize = COMMENT_LIMIT_CHARS;

/// The refusal of an empty `--manual-check`.
pub const EMPTY_MANUAL_CHECK: &str = "Write what to check: `--manual-check \"<what to check>\"`";

/// The refusal of a `--manual-check` with a line break.
pub const MANUAL_CHECK_ONE_LINE: &str =
    "A manual check is one line: give each check its own `--manual-check`";

/// The items as stored (each trimmed), or the refusal: empty, more than one line, or over the limit.
pub fn check_items(items: &[String]) -> Result<Vec<String>, BoardError> {
    items
        .iter()
        .map(|item| {
            let item = item.trim();
            if item.is_empty() {
                return Err(BoardError::refused(
                    Refusal::EmptyField,
                    EMPTY_MANUAL_CHECK,
                ));
            }
            if item.contains(['\n', '\r']) {
                return Err(BoardError::refused(
                    Refusal::MultiLine,
                    MANUAL_CHECK_ONE_LINE,
                ));
            }
            let chars = item.chars().count();
            if chars > MANUAL_CHECK_LIMIT_CHARS {
                return Err(BoardError::refused(
                    Refusal::TooLong,
                    format!(
                        "A manual check is {} characters; the limit is {}. Say in one line what the user should try.",
                        super::reports::count_label(chars as u64),
                        super::reports::count_label(MANUAL_CHECK_LIMIT_CHARS as u64)
                    ),
                ));
            }
            Ok(item.to_owned())
        })
        .collect()
}

/// A stage end's manual checks, checked, ready to store with its comment ([`super::hand_over::HandOver`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManualHandOver {
    /// The stage they come from (its id): the author's, or the ticket's when the author has none.
    pub stage: Option<String>,
    /// An agent check's round.
    pub round: Option<u32>,
    /// The items, checked ([`check_items`]); empty clears the stage's unticked ones.
    pub items: Vec<String>,
}

/// One manual check as the review, `prime` and the CLI show it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManualCheck {
    /// Its number on the ticket: what a tick names.
    pub n: u32,
    pub text: String,
    pub author: CommentAuthor,
    /// The stage it came from by its name in the ticket's flow now ("Validate"); its id when the flow no longer
    /// has it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stage_name: Option<String>,
    /// The hand-over comment it came with (its `n`), if that comment is still there.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub comment: Option<u32>,
    pub added_at: i64,
    /// When the user ticked it; `None` while it is open.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ticked_at: Option<i64>,
    /// Who ticked it (the OS user).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ticked_by: Option<String>,
}

impl ManualCheck {
    pub fn ticked(&self) -> bool {
        self.ticked_at.is_some()
    }

    /// Who added it: "Rust developer", "Validator", "You".
    pub fn author_label(&self) -> String {
        match &self.author {
            CommentAuthor::User { .. } => "You".to_owned(),
            CommentAuthor::Agent { name, .. } => name.clone().unwrap_or_else(|| "Agent".to_owned()),
            CommentAuthor::Lead { name, .. } => name.clone(),
        }
    }

    /// Where it came from: "Validator · Validate · round 2", "Rust developer · Dev".
    pub fn source(&self) -> String {
        let mut parts = vec![self.author_label()];
        parts.extend(self.stage_name.clone());
        if let CommentAuthor::Agent {
            round: Some(round), ..
        } = &self.author
        {
            parts.push(format!("round {round}"));
        }
        parts.join(" \u{b7} ")
    }
}

/// A ticket's manual checks, oldest first, with how many are not ticked: what the review's "Manual checks for you"
/// and the Approve dialog's "2 manual checks not ticked" read.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManualChecks {
    pub key: TicketKey,
    pub items: Vec<ManualCheck>,
    pub unticked: u32,
}

#[derive(FromRow)]
struct Row {
    n: i64,
    body: String,
    author_kind: String,
    author_id: Option<String>,
    author_name: Option<String>,
    author_tool: Option<String>,
    stage_id: Option<String>,
    round: Option<i64>,
    comment_n: Option<i64>,
    added_at: i64,
    ticked_at: Option<i64>,
    ticked_by: Option<String>,
}

impl Row {
    fn author(&self) -> CommentAuthor {
        let tool = self.author_tool.as_deref().and_then(|t| t.parse().ok());
        match self.author_kind.as_str() {
            "agent" => CommentAuthor::Agent {
                id: self.author_id.clone(),
                name: self.author_name.clone(),
                tool,
                stage: self.stage_id.clone(),
                round: self.round.and_then(|r| u32::try_from(r).ok()),
            },
            "lead" => CommentAuthor::Lead {
                epic_id: self.author_id.clone().unwrap_or_default(),
                name: self.author_name.clone().unwrap_or_default(),
                tool,
            },
            _ => CommentAuthor::User {
                name: self.author_name.clone().unwrap_or_default(),
            },
        }
    }
}

/// Inside the stage end's transaction: deletes the unticked items of `manual.stage` on the ticket `ticket_id`, then
/// adds `manual.items` by `author`, with the comment `comment_id` they came with.
pub(crate) async fn replace_in(
    tx: &mut Transaction<'_, Sqlite>,
    ticket_id: &str,
    author: &CommentAuthor,
    manual: &ManualHandOver,
    comment_id: &str,
) -> Result<(), BoardError> {
    let at = now_ms();
    sqlx::query(
        "UPDATE manual_checks SET replaced_at = ?
         WHERE ticket_id = ? AND stage_id IS ? AND ticked_at IS NULL AND replaced_at IS NULL",
    )
    .bind(at)
    .bind(ticket_id)
    .bind(manual.stage.as_deref())
    .execute(&mut **tx)
    .await?;
    if manual.items.is_empty() {
        return Ok(());
    }
    // Never reused: replaced rows stay and count.
    let last: Option<i64> =
        sqlx::query_scalar("SELECT MAX(n) FROM manual_checks WHERE ticket_id = ?")
            .bind(ticket_id)
            .fetch_one(&mut **tx)
            .await?;
    let mut n = last.unwrap_or(0);
    let (kind, author_id, author_name, tool) = match author {
        CommentAuthor::User { name } => ("user", None, Some(name.clone()), None),
        CommentAuthor::Agent { id, name, tool, .. } => ("agent", id.clone(), name.clone(), *tool),
        CommentAuthor::Lead {
            epic_id,
            name,
            tool,
        } => ("lead", Some(epic_id.clone()), Some(name.clone()), *tool),
    };
    for item in &manual.items {
        n += 1;
        sqlx::query(
            "INSERT INTO manual_checks (id, ticket_id, n, body, author_kind, author_id, author_name, author_tool,
                                        stage_id, round, comment_id, added_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(new_id())
        .bind(ticket_id)
        .bind(n)
        .bind(item)
        .bind(kind)
        .bind(&author_id)
        .bind(&author_name)
        .bind(tool.map(Tool::as_str))
        .bind(manual.stage.as_deref())
        .bind(manual.round.map(i64::from))
        .bind(comment_id)
        .bind(at)
        .execute(&mut **tx)
        .await?;
    }
    Ok(())
}

impl Board {
    /// The ticket's manual checks, oldest first, and how many are not ticked (d0125).
    pub async fn manual_checks(&self, key: &str) -> Result<ManualChecks, BoardError> {
        let ticket = self.ticket(key).await?;
        let rows: Vec<Row> = sqlx::query_as(
            "SELECT m.n, m.body, m.author_kind, m.author_id, m.author_name, m.author_tool, m.stage_id, m.round,
                    c.n AS comment_n, m.added_at, m.ticked_at, m.ticked_by
             FROM manual_checks m
             LEFT JOIN ticket_comments c ON c.id = m.comment_id AND c.deleted_at IS NULL
             WHERE m.ticket_id = ? AND m.replaced_at IS NULL ORDER BY m.n",
        )
        .bind(&ticket.id)
        .fetch_all(self.store.pool())
        .await?;
        let flow = self.effective_flow(ticket.key.project.as_str()).await.ok();
        let items: Vec<ManualCheck> = rows
            .into_iter()
            .map(|row| ManualCheck {
                n: u32::try_from(row.n).unwrap_or(0),
                author: row.author(),
                stage_name: row.stage_id.as_ref().map(|id| {
                    flow.as_ref()
                        .and_then(|f| f.stage(id))
                        .map_or_else(|| id.clone(), |s| s.name.clone())
                }),
                comment: row.comment_n.and_then(|n| u32::try_from(n).ok()),
                text: row.body,
                added_at: row.added_at,
                ticked_at: row.ticked_at,
                ticked_by: row.ticked_by,
            })
            .collect();
        let unticked = u32::try_from(items.iter().filter(|i| !i.ticked()).count()).unwrap_or(0);
        Ok(ManualChecks {
            key: ticket.key,
            items,
            unticked,
        })
    }

    /// Ticks (or unticks) manual check `n`, logging `manual_check_ticked` with its text; ticking a ticked item (or
    /// unticking an open one) changes nothing and logs nothing. The policy let only the user through.
    pub async fn tick_manual_check(
        &self,
        key: &str,
        n: u32,
        ticked: bool,
        actor: &Actor,
    ) -> Result<ManualChecks, BoardError> {
        let ticket = self.ticket(key).await?;
        let mut tx = self.store.begin_write().await?;
        let row: Option<(String, String, Option<i64>)> = sqlx::query_as(
            "SELECT id, body, ticked_at FROM manual_checks WHERE ticket_id = ? AND n = ? AND replaced_at IS NULL",
        )
        .bind(&ticket.id)
        .bind(i64::from(n))
        .fetch_optional(&mut *tx)
        .await?;
        let (id, body, ticked_at) = row.ok_or_else(|| {
            BoardError::NotFound(format!("{} has no manual check {n}", ticket.key))
        })?;
        if ticked_at.is_some() != ticked {
            sqlx::query("UPDATE manual_checks SET ticked_at = ?, ticked_by = ? WHERE id = ?")
                .bind(ticked.then(now_ms))
                .bind(ticked.then(|| actor.to_string()))
                .bind(&id)
                .execute(&mut *tx)
                .await?;
            record_event(
                &mut tx,
                &ticket.id,
                EventKind::ManualCheckTicked,
                actor,
                json!({"n": n, "text": body, "ticked": ticked}),
            )
            .await?;
        }
        tx.commit().await?;
        self.manual_checks(&ticket.key.to_string()).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn items_are_one_trimmed_line_within_the_limit() {
        assert_eq!(
            check_items(&["  Reload a page offline \n".into()]).unwrap(),
            ["Reload a page offline"]
        );
        let empty = check_items(&["ok".into(), "  ".into()]).unwrap_err();
        assert_eq!(
            (empty.refusal(), empty.to_string().as_str()),
            (Some(Refusal::EmptyField), EMPTY_MANUAL_CHECK)
        );
        let two = check_items(&["Open it on a phone\nand in Safari".into()]).unwrap_err();
        assert_eq!(
            (two.refusal(), two.to_string().as_str()),
            (Some(Refusal::MultiLine), MANUAL_CHECK_ONE_LINE)
        );
        let at_limit = "ş".repeat(MANUAL_CHECK_LIMIT_CHARS);
        assert!(check_items(&[at_limit]).is_ok());
        let over = check_items(&["x".repeat(2_310)]).unwrap_err();
        assert_eq!(over.refusal(), Some(Refusal::TooLong));
        assert_eq!(
            over.to_string(),
            "A manual check is 2,310 characters; the limit is 2,000. Say in one line what the user should try."
        );
    }
}
