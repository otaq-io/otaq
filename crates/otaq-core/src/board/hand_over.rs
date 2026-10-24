//! Hand-overs become comments (CMT-05, d0109): what one stage hands to the next is a ticket comment, posted in the
//! same transaction as the move that hands it over, before any agent is resumed or started. That is an agent's `stage
//! next` / `stage done` summary, a check's verdict, and the user's note on Move to next stage, Send back (a human
//! check's Fail), Send back with PR comments and a drag back.
//!
//! A hand-over's text is checked ([`check_body`]) and its report copied ([`Board::prepare_report`]) **before** the
//! move, so a text over the limit or a missing report refuses the whole hand-over and nothing moves. An empty optional
//! note posts nothing. What the agent is then typed or resumed with names the hand-over, never its text ("SHOP-18 was
//! sent back to Dev with 2 line comments and a comment. Run `otaq prime`, then `otaq review feedback`."): the agent
//! reads the comment in `prime`.

use sqlx::{Sqlite, Transaction};

use super::manual_checks::{ManualHandOver, check_items};
use super::reports::{ReportSource, StoredReport};
use super::ticket_comments::{CommentAuthor, Posted, check_body, post_in};
use super::tickets::Ticket;
use super::{Board, BoardError};
use crate::domain::Actor;
use crate::naming::COMMAND_NAME;

/// A hand-over's comment, checked and with its report copied, ready to post inside the move's transaction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HandOver {
    pub author: CommentAuthor,
    /// The text as stored (trimmed, within the limit).
    pub body: String,
    pub report: Option<StoredReport>,
    /// A stage end's manual checks (d0125), stored with the comment; `None` for every other hand-over (a Send back's
    /// note, a drag back…), which leaves them as they are.
    pub manual: Option<ManualHandOver>,
}

impl HandOver {
    /// Posts it on the ticket `ticket_id` inside the move's transaction.
    pub(crate) async fn post(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        ticket_id: &str,
        actor: &Actor,
    ) -> Result<Posted, BoardError> {
        let posted = post_in(
            tx,
            ticket_id,
            &self.author,
            &self.body,
            self.report.as_ref(),
            actor,
        )
        .await?;
        if let Some(manual) = &self.manual {
            super::manual_checks::replace_in(tx, ticket_id, &self.author, manual, &posted.id)
                .await?;
        }
        Ok(posted)
    }

    /// Removes the copied report unless its comment was stored: the hand-over was refused or failed after the copy.
    /// Safe after a commit (a stored comment keeps its report).
    pub async fn discard_unposted(&self, board: &Board) {
        let Some(report) = &self.report else {
            return;
        };
        let posted: Result<Option<i64>, _> =
            sqlx::query_scalar("SELECT 1 FROM ticket_comments WHERE id = ?")
                .bind(&report.comment_id)
                .fetch_optional(board.store().pool())
                .await;
        if matches!(posted, Ok(None)) {
            report.discard();
        }
    }
}

/// Discards `hand_over`'s report when `result` failed ([`HandOver::discard_unposted`]), and passes `result` on.
pub async fn settled<T>(
    board: &Board,
    hand_over: Option<&HandOver>,
    result: Result<T, BoardError>,
) -> Result<T, BoardError> {
    if result.is_err()
        && let Some(hand_over) = hand_over
    {
        hand_over.discard_unposted(board).await;
    }
    result
}

impl Board {
    /// The hand-over comment of `author` on `ticket`: `body` checked first (CMT-02's limit, refused whole), then the
    /// report copied (missing or over 20 MB, refused). `None` when there is nothing to post: an empty optional note
    /// and no report. A report with no text is refused: a comment always has its own line.
    pub async fn hand_over(
        &self,
        ticket: &Ticket,
        author: CommentAuthor,
        body: &str,
        report: Option<ReportSource>,
    ) -> Result<Option<HandOver>, BoardError> {
        if body.trim().is_empty() && report.is_none() {
            return Ok(None);
        }
        let body = check_body(body)?;
        let report = match report {
            Some(source) => {
                let project = self.project(ticket.key.project.as_str()).await?;
                let repo = crate::warm::store::repo_name(&project);
                Some(self.prepare_report(&repo, &ticket.key, source).await?)
            }
            None => None,
        };
        Ok(Some(HandOver {
            author,
            body,
            report,
            manual: None,
        }))
    }

    /// The hand-over of a stage end (`stage next`, `stage done`, a check's verdict): [`Self::hand_over`] with the
    /// `--manual-check` items (d0125), checked before anything moves. An agent's stage end always replaces its stage's
    /// unticked items, so ending a round with none clears them; the user's own `stage next` touches them only when it
    /// lists some. Items need the comment's text: "Write the comment first".
    pub async fn stage_end_hand_over(
        &self,
        ticket: &Ticket,
        author: CommentAuthor,
        body: &str,
        report: Option<ReportSource>,
        manual_checks: &[String],
    ) -> Result<Option<HandOver>, BoardError> {
        let items = check_items(manual_checks)?;
        let (stage, round) = match &author {
            CommentAuthor::Agent { stage, round, .. } => (stage.clone(), *round),
            _ => (ticket.place.stage().map(str::to_owned), None),
        };
        let replaces =
            matches!(author, CommentAuthor::Agent { stage: Some(_), .. }) || !items.is_empty();
        if !items.is_empty() && body.trim().is_empty() && report.is_none() {
            return Err(BoardError::refused(
                super::Refusal::EmptyField,
                super::ticket_comments::EMPTY_COMMENT,
            ));
        }
        let mut hand_over = self.hand_over(ticket, author, body, report).await?;
        if let Some(hand_over) = hand_over.as_mut()
            && replaces
        {
            hand_over.manual = Some(ManualHandOver {
                stage,
                round,
                items,
            });
        }
        Ok(hand_over)
    }

    /// The user's hand-over on `ticket` (a Send back's note, a drag back's instructions…), `None` when the note is
    /// empty and nothing is attached.
    pub async fn user_hand_over(
        &self,
        ticket: &Ticket,
        note: &str,
        report: Option<ReportSource>,
        actor: &Actor,
    ) -> Result<Option<HandOver>, BoardError> {
        let author = CommentAuthor::User {
            name: actor.to_string(),
        };
        self.hand_over(ticket, author, note, report).await
    }
}

/// How a ticket came back to an Agent stage, for what its agent is told ([`sent_back_text`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SentBack<'a> {
    pub key: &'a str,
    /// The Agent stage's name ("Dev").
    pub stage: &'a str,
    /// The line comments sent with it.
    pub line_comments: u32,
    /// Send back with PR comments: the agent reads them with `otaq review feedback`.
    pub pr_comments: bool,
    /// The user's comment was posted.
    pub comment: bool,
    /// A drag back: "by you".
    pub by_you: bool,
}

/// What a returning agent is typed or resumed with: the hand-over, never its text (CMT-05). "SHOP-18 was sent back to
/// Dev with 2 line comments and a comment. Run `otaq prime`, then `otaq review feedback`."; "SHOP-19 was sent back to
/// Dev by you with a comment. Run `otaq prime`." `otaq review feedback` is named when there are line or PR comments
/// to read.
pub fn sent_back_text(back: &SentBack<'_>) -> String {
    let mut parts: Vec<String> = Vec::new();
    match back.line_comments {
        0 => {}
        1 => parts.push("1 line comment".to_owned()),
        n => parts.push(format!("{n} line comments")),
    }
    if back.pr_comments {
        parts.push("PR comments".to_owned());
    }
    if back.comment {
        parts.push("a comment".to_owned());
    }
    let with = match parts.split_last() {
        None => String::new(),
        Some((last, [])) => format!(" with {last}"),
        Some((last, rest)) => format!(" with {} and {last}", rest.join(", ")),
    };
    let by = if back.by_you { " by you" } else { "" };
    let feedback = if back.line_comments > 0 || back.pr_comments {
        format!(", then `{COMMAND_NAME} review feedback`")
    } else {
        String::new()
    };
    format!(
        "{} was sent back to {}{by}{with}. Run `{COMMAND_NAME} prime`{feedback}.",
        back.key, back.stage
    )
}

/// What the agent is told after a failed check (VAL-04, CMT-05): "Validate failed round 2. Run `otaq prime`." The
/// findings are the checker's comment.
pub fn check_failed_text(check: &str, round: u32) -> String {
    format!("{check} failed round {round}. Run `{COMMAND_NAME} prime`.")
}
