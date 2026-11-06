//! **Done, nothing to merge**: the Approve menu's way to finish a ticket whose branch has no commits beyond its base
//! (its work lives outside the repository). The ticket ends Done as [`Outcome::DoneWithoutMerge`]: it counts as
//! merged, so tickets waiting on it start, with no merge commit. The usual Done cleanup follows: the agent stops, the
//! ports return, the worktree is removed and the branch is kept.
//!
//! The diff decides: no ticket field or flow setting marks a ticket as having no repo changes. A branch with commits
//! is refused here; Local merge or Open PR delivers it.
//!
//! [`Outcome::DoneWithoutMerge`]: crate::domain::Outcome::DoneWithoutMerge

use serde::{Deserialize, Serialize};

use super::Review;
use crate::board::tickets::Ticket;
use crate::board::{BoardError, Refusal};
use crate::domain::{Actor, Status, TicketKey};

/// The answer to Done: the finished ticket, and the waiting tickets the scheduler started after it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DoneOutcome {
    pub ticket: Box<Ticket>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub started: Vec<TicketKey>,
}

/// The refusal when the branch has work to deliver: "SHOP-18 has 2 commits; Local merge or Open PR delivers them."
pub fn has_commits_message(key: &TicketKey, commits: u32) -> String {
    match commits {
        1 => format!("{key} has 1 commit; Local merge or Open PR delivers it."),
        n => format!("{key} has {n} commits; Local merge or Open PR delivers them."),
    }
}

impl Review {
    /// **Done, nothing to merge**: from where Local merge works (a human check offering the Delivery, or the Delivery
    /// stage), without an open PR, only when the branch has no commits beyond its base.
    pub async fn done(&self, key: &str, actor: &Actor) -> Result<DoneOutcome, BoardError> {
        let board = &self.inner.board;
        let ticket = board.ticket(key).await?;
        if !matches!(ticket.status, Status::WaitingForYou | Status::ToDeliver) {
            return Err(BoardError::refused(
                Refusal::MoveRefused,
                format!(
                    "{} is {}; Done works on a ticket in Ready for Review",
                    ticket.key,
                    ticket.status.label()
                ),
            ));
        }
        if let Some(n) = ticket.flags.pr_number {
            return Err(BoardError::refused(
                Refusal::MoveRefused,
                format!(
                    "{} has PR #{n} open; use Mark merged or Mark closed",
                    ticket.key
                ),
            ));
        }
        let commits = board.commits_beyond_base(&ticket).await?;
        if commits > 0 {
            return Err(BoardError::refused(
                Refusal::MoveRefused,
                has_commits_message(&ticket.key, commits),
            ));
        }
        self.finish_without_merge(&ticket, Some(""), actor).await
    }

    /// Ends `ticket` as Done without a merge (`nothing_to_merge`: see [`crate::board::Board::record_done_without_merge`]),
    /// then the Done cleanup (worktree removed unless dirty, branch kept), then the waiting tickets whose last parent
    /// this was start.
    async fn finish_without_merge(
        &self,
        ticket: &Ticket,
        nothing_to_merge: Option<&str>,
        actor: &Actor,
    ) -> Result<DoneOutcome, BoardError> {
        let board = &self.inner.board;
        let done = board
            .record_done_without_merge(ticket, nothing_to_merge, actor)
            .await?;
        if let Err(e) = self.finish(&done).await {
            tracing::warn!(ticket = %done.key, error = %e, "Done cleanup failed");
        }
        let started = match self
            .inner
            .scheduler
            .on_parent_merged(&ticket.key.to_string())
            .await
        {
            Ok(plan) => plan.start.into_iter().map(|t| t.key).collect(),
            Err(e) => {
                tracing::warn!(ticket = %ticket.key, error = %e, "can't start the waiting tickets");
                Vec::new()
            }
        };
        Ok(DoneOutcome {
            ticket: Box::new(board.ticket_by_id(&ticket.id).await?),
            started,
        })
    }
}
