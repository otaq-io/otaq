//! **The early conflict** (GIT-04; d0038, d0127): after every merge into a branch, a read-only test merge of each
//! ticket waiting in Review that merges into it (no files touched, no tokens). A conflict is recorded on the ticket
//! ("conflicts with `main`") and, for a ticket waiting in a human check or a click-mode Delivery, handed to its
//! nearest earlier Agent stage at once, as a Delivery's conflict is ([`Review::delivery_conflict`]): its session
//! resumes with `restack`, no click, the card says "Conflicts with main · fixing", and it counts toward the same
//! three in a row (an epic child's stop goes to its lead first). The user's unsent line comments stay drafts.
//!
//! - **A PR open:** only the flag; its conflicts are the PR's.
//! - **The project paused** (d0112): only the flag. The project's Resume tests the flagged tickets again and hands
//!   back those that still conflict ([`Review::hand_back_held`]); the flag is stored, so a restart keeps it.
//! - **A clean test merge** clears the flag.

use std::path::{Path, PathBuf};

use super::Review;
use crate::agents::pause::is_paused;
use crate::board::BoardError;
use crate::board::conflicts::{Conflict, ConflictReason};
use crate::board::tickets::Ticket;
use crate::domain::{Actor, Status};

impl Review {
    /// After a merge into `target` (GIT-04, d0038, d0127): the test merge of every ticket waiting in Review that
    /// merges into it; "conflicts with `target`" is set or cleared, and a conflict goes back to its agent.
    pub async fn flag_conflicts(
        &self,
        project: &str,
        repo: &Path,
        target: &str,
    ) -> Result<(), BoardError> {
        let board = &self.inner.board;
        for ticket in board.tickets(project).await? {
            if !waits_in_review(&ticket) {
                continue;
            }
            let Ok(merge) = board.merge_target(&ticket).await else {
                continue;
            };
            if merge.target != target {
                continue;
            }
            self.test_merge(&ticket, repo, target, &merge.branch)
                .await?;
        }
        Ok(())
    }

    /// The hand-backs a pause held (d0127), on the project's Resume: each ticket still waiting in Review with the
    /// test merge's flag is tested again; one that still conflicts goes back to its agent, a clean one loses the flag.
    pub async fn hand_back_held(&self, project: &str) -> Result<(), BoardError> {
        let board = &self.inner.board;
        let repo = PathBuf::from(board.project(project).await?.repo_path);
        for ticket in board.tickets(project).await? {
            if !waits_in_review(&ticket)
                || board
                    .conflict(&ticket.id)
                    .await?
                    .is_none_or(|c| c.reason != ConflictReason::TestMerge)
            {
                continue;
            }
            let Ok(merge) = board.merge_target(&ticket).await else {
                continue;
            };
            self.test_merge(&ticket, &repo, &merge.target, &merge.branch)
                .await?;
        }
        Ok(())
    }

    /// One ticket's test merge of `branch` into `target`: the flag set or cleared, and a conflict handed back when
    /// [`Self::hands_back`] says so. A hand-back that fails is logged; the other tickets go on.
    async fn test_merge(
        &self,
        ticket: &Ticket,
        repo: &Path,
        target: &str,
        branch: &str,
    ) -> Result<(), BoardError> {
        let board = &self.inner.board;
        let Some(files) = board.git()?.test_merge(repo, target, branch).await? else {
            if board.conflict(&ticket.id).await?.is_some_and(|c| {
                matches!(c.reason, ConflictReason::Merge | ConflictReason::TestMerge)
            }) {
                board.clear_conflict(&ticket.id).await?;
            }
            return Ok(());
        };
        board
            .set_conflict(
                &ticket.id,
                &Conflict {
                    target: target.to_owned(),
                    files,
                    reason: ConflictReason::TestMerge,
                },
            )
            .await?;
        let hands_back = self.hands_back(ticket).await?;
        tracing::info!(ticket = %ticket.key, target, hands_back, "conflicts with the merged branch");
        if hands_back
            && let Err(e) = self
                .hand_back(ticket, target, None, &Actor::current_user())
                .await
        {
            tracing::warn!(ticket = %ticket.key, error = %e, "can't hand the conflict to the agent");
        }
        Ok(())
    }

    /// Whether a ticket the test merge flagged goes back to its agent now (d0127): it waits in a human check or a
    /// click-mode Delivery, it isn't an epic (an epic has no Agent stage), its PR isn't open, and its project isn't
    /// paused.
    async fn hands_back(&self, ticket: &Ticket) -> Result<bool, BoardError> {
        let board = &self.inner.board;
        if ticket.is_epic()
            || ticket.flags.pr_number.is_some()
            || is_paused(board.store(), ticket.key.project.as_str()).await?
        {
            return Ok(false);
        }
        Ok(match ticket.status {
            Status::WaitingForYou => true,
            Status::ToDeliver => board
                .delivery_now(ticket)
                .await?
                .is_some_and(|now| !now.automatic),
            _ => false,
        })
    }
}

/// Waiting in a human check or in its Delivery stage: what the test merge checks.
fn waits_in_review(ticket: &Ticket) -> bool {
    matches!(ticket.status, Status::WaitingForYou | Status::ToDeliver)
}
