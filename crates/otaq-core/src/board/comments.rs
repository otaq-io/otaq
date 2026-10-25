//! Review comments and rounds (REV-03; d0017, d0034).
//!
//! The user writes **line comments** on a ticket's diff while it waits in their check. A comment is anchored to
//! a file, a line and a side (`old` for a removed line, `new` otherwise); a **thread** is every comment on the
//! same anchor, oldest first, and a reply is a new comment on that anchor. Comments belong to the **current
//! round** until Send back bundles them into one message for the agent (T-0086), which sets their `sent_at`:
//! pending comments can be edited and deleted, sent ones are read-only.
//!
//! Rounds count Send backs: the first review is round 1, and each Send back starts the next one, so after K
//! Send backs `tickets.sent_back_round` is K and the comments sent by the K-th one have round K.
//!
//! There is **no partial approval**: comments never approve a file or a line.

use serde::{Deserialize, Serialize};
use sqlx::FromRow;

use super::{Board, BoardError, Refusal};
use crate::domain::{Status, TicketKey, stored_enum};
use crate::store::{new_id, now_ms};

stored_enum! {
    /// Which side of the diff a comment's line number is on.
    pub enum Side ("diff side") {
        /// A removed line: its number in the old file.
        Old = "old",
        /// An added or unchanged line: its number in the new file.
        New = "new",
    }
}

/// One line comment.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReviewComment {
    pub id: String,
    pub round: u32,
    pub file: String,
    pub line: u32,
    pub side: Side,
    pub body: String,
    pub created_at: i64,
    /// When Send back sent it; `None` while pending.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sent_at: Option<i64>,
}

impl ReviewComment {
    pub fn is_pending(&self) -> bool {
        self.sent_at.is_none()
    }

    /// Two comments on the same file, line and side are one thread.
    pub fn same_anchor(&self, other: &Self) -> bool {
        self.file == other.file && self.line == other.line && self.side == other.side
    }
}

/// A ticket's comments with its current round and the pending counter ("3 comments pending").
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReviewComments {
    pub key: TicketKey,
    /// The round new comments go to.
    pub round: u32,
    /// Comments Send back would send now.
    pub pending: u32,
    /// Oldest first.
    pub comments: Vec<ReviewComment>,
}

/// Where a new comment goes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NewComment {
    pub key: String,
    pub file: String,
    pub line: u32,
    pub side: Side,
    pub body: String,
}

#[derive(FromRow)]
struct CommentRow {
    id: String,
    ticket_id: String,
    round: i64,
    file: String,
    line: i64,
    side: String,
    body: String,
    created_at: i64,
    sent_at: Option<i64>,
}

impl CommentRow {
    fn into_comment(self) -> ReviewComment {
        ReviewComment {
            id: self.id,
            round: u32::try_from(self.round).unwrap_or(0),
            file: self.file,
            line: u32::try_from(self.line).unwrap_or(0),
            side: self.side.parse().unwrap_or(Side::New),
            body: self.body,
            created_at: self.created_at,
            sent_at: self.sent_at,
        }
    }
}

const COLUMNS: &str = "id, ticket_id, round, file, line, side, body, created_at, sent_at";

impl Board {
    /// Adds a line comment to the ticket's current round. Only a ticket waiting in one of your checks takes comments:
    /// any human check of its flow, whose one status is `WaitingForYou` (FTB-05).
    pub async fn add_comment(&self, new: &NewComment) -> Result<ReviewComment, BoardError> {
        let ticket = self.ticket(&new.key).await?;
        if ticket.status != Status::WaitingForYou {
            return Err(BoardError::refused(
                Refusal::MoveRefused,
                format!(
                    "{} is {}; comments are written while it waits in your check",
                    ticket.key,
                    ticket.status.label()
                ),
            ));
        }
        if new.file.trim().is_empty() || new.line == 0 {
            return Err(BoardError::refused(
                Refusal::EmptyField,
                "A comment needs a file and a line",
            ));
        }
        let body = non_empty(&new.body)?;
        let round = self.current_round(&ticket.id).await?;
        let id = new_id();
        sqlx::query(
            "INSERT INTO review_comments (id, ticket_id, round, file, line, side, body, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(&ticket.id)
        .bind(i64::from(round))
        .bind(&new.file)
        .bind(i64::from(new.line))
        .bind(new.side.as_str())
        .bind(body)
        .bind(now_ms())
        .execute(self.store.pool())
        .await?;
        Ok(self.comment(&id).await?.1)
    }

    /// Replies in the thread of comment `id`: a new comment on the same file, line and side.
    pub async fn reply_to_comment(
        &self,
        id: &str,
        body: &str,
    ) -> Result<ReviewComment, BoardError> {
        let (key, comment) = self.comment(id).await?;
        self.add_comment(&NewComment {
            key: key.to_string(),
            file: comment.file,
            line: comment.line,
            side: comment.side,
            body: body.to_owned(),
        })
        .await
    }

    /// Changes a pending comment's text. Sent comments are read-only.
    pub async fn edit_comment(&self, id: &str, body: &str) -> Result<ReviewComment, BoardError> {
        let (_, comment) = self.pending_comment(id).await?;
        let body = non_empty(body)?;
        sqlx::query("UPDATE review_comments SET body = ? WHERE id = ? AND sent_at IS NULL")
            .bind(body)
            .bind(&comment.id)
            .execute(self.store.pool())
            .await?;
        Ok(self.comment(id).await?.1)
    }

    /// Deletes a pending comment. Sent comments stay.
    pub async fn delete_comment(&self, id: &str) -> Result<TicketKey, BoardError> {
        let (key, comment) = self.pending_comment(id).await?;
        sqlx::query("DELETE FROM review_comments WHERE id = ? AND sent_at IS NULL")
            .bind(&comment.id)
            .execute(self.store.pool())
            .await?;
        Ok(key)
    }

    /// The ticket's comments, oldest first: every round, or only `round`.
    pub async fn comments(
        &self,
        key: &str,
        round: Option<u32>,
    ) -> Result<ReviewComments, BoardError> {
        let ticket = self.ticket(key).await?;
        let rows: Vec<CommentRow> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
            "SELECT {COLUMNS} FROM review_comments WHERE ticket_id = ? ORDER BY created_at, id"
        )))
        .bind(&ticket.id)
        .fetch_all(self.store.pool())
        .await?;
        let all: Vec<ReviewComment> = rows.into_iter().map(CommentRow::into_comment).collect();
        let pending = u32::try_from(all.iter().filter(|c| c.is_pending()).count()).unwrap_or(0);
        Ok(ReviewComments {
            key: ticket.key,
            round: self.current_round(&ticket.id).await?,
            pending,
            comments: all
                .into_iter()
                .filter(|c| round.is_none_or(|r| c.round == r))
                .collect(),
        })
    }

    /// The round new comments go to: one more than the Send backs so far.
    pub(crate) async fn current_round(&self, ticket_id: &str) -> Result<u32, BoardError> {
        let sent_back: i64 = sqlx::query_scalar("SELECT sent_back_round FROM tickets WHERE id = ?")
            .bind(ticket_id)
            .fetch_one(self.store.pool())
            .await?;
        Ok(u32::try_from(sent_back).unwrap_or(0) + 1)
    }

    /// A comment and its ticket's key.
    pub async fn comment(&self, id: &str) -> Result<(TicketKey, ReviewComment), BoardError> {
        let row: Option<CommentRow> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
            "SELECT {COLUMNS} FROM review_comments WHERE id = ?"
        )))
        .bind(id)
        .fetch_optional(self.store.pool())
        .await?;
        let row = row.ok_or_else(|| BoardError::NotFound(format!("no comment {id}")))?;
        let key = self.ticket_by_id(&row.ticket_id).await?.key;
        Ok((key, row.into_comment()))
    }

    async fn pending_comment(&self, id: &str) -> Result<(TicketKey, ReviewComment), BoardError> {
        let (key, comment) = self.comment(id).await?;
        if !comment.is_pending() {
            return Err(BoardError::refused(
                Refusal::CommentSent,
                format!(
                    "This comment was sent in round {}; sent comments can't change",
                    comment.round
                ),
            ));
        }
        Ok((key, comment))
    }
}

fn non_empty(body: &str) -> Result<&str, BoardError> {
    let body = body.trim();
    if body.is_empty() {
        return Err(BoardError::refused(
            Refusal::EmptyField,
            "Write the comment first",
        ));
    }
    Ok(body)
}

/// What `otaq review feedback` prints (REV-05): the line comments of the latest Send back, then the unresolved PR
/// comments (fetched on demand, T-0148). The user's note is a ticket comment, read in `prime` (CMT-05).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReviewFeedback {
    pub key: TicketKey,
    /// The latest Send back's round; 0 when the ticket was never sent back.
    pub round: u32,
    /// The round's line comments, by file then line, oldest first on a line.
    pub comments: Vec<FeedbackComment>,
    /// For a ticket with a PR: its unresolved review comments, or why they couldn't be fetched.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pr: Option<crate::pr::comments::PrFeedback>,
}

/// One line comment of the feedback.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FeedbackComment {
    pub file: String,
    pub line: u32,
    pub side: Side,
    pub body: String,
}

impl Board {
    /// The line comments of the latest Send back. Reading it changes nothing: no move, no session (REV-05).
    pub async fn review_feedback(&self, key: &str) -> Result<ReviewFeedback, BoardError> {
        let ticket = self.ticket(key).await?;
        let round = self.latest_send_back_round(&ticket.id).await?.unwrap_or(0);
        let mut comments: Vec<FeedbackComment> = self
            .comments(key, Some(round))
            .await?
            .comments
            .into_iter()
            .filter(|c| round > 0 && !c.is_pending())
            .map(|c| FeedbackComment {
                file: c.file,
                line: c.line,
                side: c.side,
                body: c.body,
            })
            .collect();
        // Stable: comments on one line keep their order (a thread reads top to bottom).
        comments.sort_by(|a, b| a.file.cmp(&b.file).then(a.line.cmp(&b.line)));
        Ok(ReviewFeedback {
            key: ticket.key,
            round,
            comments,
            pr: None,
        })
    }
}
