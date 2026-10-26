//! `otaq stage next` (and its other name `otaq review submit`; REV-07, AGM-10; d0050, d0004, d0108): the agent ends
//! its Agent stage with a summary and the ticket moves to the next stage: **Ready for Review** on the default flow.
//!
//! It is **refused while the worktree has uncommitted files**, tracked or untracked: "2 files are not committed.
//! Commit them, or discard them if they're scratch files". The agent commits through the Commit skill; Otaq
//! never commits by itself (GIT-08). Otaq runs no tests or checks of its own (P5).
//!
//! On submit the service records the review head (`review_head_sha`, the worktree's `HEAD`) and when the ticket
//! entered review (`review_entered_at`), for "changed since you reviewed it" later (T-0097). The summary is the agent's
//! comment on the ticket (CMT-05), posted with the move; History records `review_submitted` ("Submitted for review")
//! when the next stage is a human check, else `stage_finished` ("Finished Dev", Q263).

use std::path::Path;

use serde_json::json;

use super::hand_over::HandOver;
use super::tickets::{EventKind, Ticket, record_event};
use super::{Board, BoardError, Refusal};
use crate::domain::{Actor, ReviewEntry, Status};
use crate::flow::engine::Move;
use crate::signals::LIVE;
use crate::store::now_ms;
use crate::store::tickets::set_state_since;

/// The statuses the user can submit from: the agent isn't mid-turn (CLS-05).
pub const USER_SUBMITS_FROM: &[Status] = &[
    Status::Idle,
    Status::AsksYou,
    Status::NeedsYou,
    Status::Interrupted,
];

/// The user's Move to next stage only when the agent isn't mid-turn (CLS-05): "SHOP-19 can't move to the next stage
/// now: its agent is in the middle of a turn".
pub fn user_may_submit(ticket: &Ticket) -> Result<(), BoardError> {
    if USER_SUBMITS_FROM.contains(&ticket.status) {
        return Ok(());
    }
    let why = match ticket.status {
        Status::Working | Status::WorkingInBackground => {
            "its agent is in the middle of a turn".to_owned()
        }
        other => format!("it is {}", other.label()),
    };
    Err(BoardError::refused(
        Refusal::MoveRefused,
        format!("{} can't move to the next stage now: {why}", ticket.key),
    ))
}

/// What History records for a user's Move to next stage; the user's note is their comment (CMT-05).
pub const SUBMITTED_BY_YOU: &str = "Submitted by you";

/// How an agent's stage ended (`otaq stage next` / `stage done`, AGM-10): the ticket afterwards and what the agent
/// is told.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct StageEnded {
    pub ticket: Ticket,
    pub text: String,
}

/// What `otaq stage next` answers (AGM-10), from the ticket `before` and `after` the move: the stage it finished and
/// where it is now. "Finished Dev. Validate is running; the result will come back to you here. End your turn." /
/// "Finished Dev. SHOP-18 is Ready for Review." / "Finished Product spec. Dev starts next; end your turn."
pub fn stage_next_text(flow: &crate::flow::Flow, before: &Ticket, after: &Ticket) -> String {
    let name = |place: &crate::domain::Place| {
        place
            .stage()
            .map(|id| flow.stage(id).map_or(id, |s| s.name.as_str()).to_owned())
    };
    let finished = name(&before.place).unwrap_or_else(|| "the stage".to_owned());
    let next = after.place.stage().and_then(|id| flow.stage(id));
    let next_name = name(&after.place).unwrap_or_default();
    let now = match next {
        _ if after.place.is_waiting() => format!("{next_name} starts next; end your turn."),
        Some(stage) if stage.is_agent_check() => {
            format!("{next_name} is running; the result will come back to you here. End your turn.")
        }
        Some(stage) if stage.is_human_check() => {
            format!("{} is Ready for {next_name}.", after.key)
        }
        Some(_) => format!("{next_name} is next; end your turn."),
        None => format!("{} is {}.", after.key, after.status.label()),
    };
    format!("Finished {finished}. {now}")
}

/// What `otaq stage done` answers (AGM-10): "SHOP-18 is closed: nothing to merge. End your turn."
pub fn stage_done_text(after: &Ticket) -> String {
    format!("{} is closed: nothing to merge. End your turn.", after.key)
}

/// The payload of a [`EventKind::StageFinished`]: the name and id of the stage `ticket` leaves, the summary and the
/// head.
pub(crate) fn stage_finished(
    flow: &crate::flow::Flow,
    ticket: &Ticket,
    summary: &str,
    head: &str,
) -> serde_json::Value {
    let id = ticket.place.stage().unwrap_or_default();
    let name = flow.stage(id).map_or(id, |s| s.name.as_str());
    json!({ "stage": name, "stage_id": id, "summary": summary.trim(), "head": head })
}

/// "2 files are not committed. Commit them, or discard them if they're scratch files".
pub fn uncommitted_message(count: usize) -> String {
    if count == 1 {
        "1 file is not committed. Commit it, or discard it if it's a scratch file".to_owned()
    } else {
        format!(
            "{count} files are not committed. Commit them, or discard them if they're scratch files"
        )
    }
}

impl Board {
    /// Moves a working ticket to Ready for Review with the agent's summary as its comment (`hand_over`), unless its
    /// worktree has uncommitted files. A ticket whose agent fixed a Delivery's conflict moves on the same way (FLW-17).
    pub async fn submit_for_review(
        &self,
        key: &str,
        hand_over: Option<&HandOver>,
        actor: &Actor,
    ) -> Result<Ticket, BoardError> {
        let ticket = self.ticket(key).await?;
        if !LIVE.contains(&ticket.status) && ticket.status != Status::MergeConflictFixing {
            return Err(BoardError::refused(
                Refusal::MoveRefused,
                format!(
                    "{} is {}; only a ticket whose agent is working can be submitted",
                    ticket.key,
                    ticket.status.label()
                ),
            ));
        }
        let summary = hand_over.map_or("", |h| h.body.as_str()).to_owned();
        self.submit(ticket, &summary, hand_over, None, actor).await
    }

    /// **Move to next stage** by the user (was Submit for review; CLS-05, d0052): when the agent isn't mid-turn
    /// (Idle, Asks you, Needs you, Interrupted), with the same checks as the agent's submit. The optional note is the
    /// user's comment (`hand_over`, CMT-05); the session isn't stopped, so Send back works as usual.
    pub(crate) async fn submit_by_user(
        &self,
        key: &str,
        hand_over: Option<&HandOver>,
        actor: &Actor,
    ) -> Result<Ticket, BoardError> {
        let ticket = self.ticket(key).await?;
        user_may_submit(&ticket)?;
        // Past no agent check ([`crate::validation::Validation::submit_by_user`] runs those): "Submitted by you".
        self.submit(
            ticket,
            SUBMITTED_BY_YOU,
            hand_over,
            Some(crate::domain::ReviewEntry::SubmittedByUser),
            actor,
        )
        .await
    }

    /// A submit's checks (REV-07): the ticket has a worktree with nothing uncommitted. Returns its head.
    pub(crate) async fn submit_head(&self, ticket: &Ticket) -> Result<String, BoardError> {
        let worktree = ticket
            .work
            .as_ref()
            .and_then(|w| w.worktree.clone())
            .ok_or_else(|| {
                BoardError::refused(
                    Refusal::MoveRefused,
                    format!("{} has no worktree to submit", ticket.key),
                )
            })?;
        let git = self.git()?;
        let uncommitted = git.uncommitted_files(Path::new(&worktree)).await?;
        if !uncommitted.is_empty() {
            return Err(BoardError::refused(
                Refusal::Uncommitted,
                uncommitted_message(uncommitted.len()),
            ));
        }
        Ok(git
            .run(Path::new(&worktree), &["rev-parse", "HEAD"])
            .await?
            .trim()
            .to_owned())
    }

    /// `summary` is History's line; the comment is `hand_over`'s, posted with the move.
    async fn submit(
        &self,
        ticket: Ticket,
        summary: &str,
        hand_over: Option<&HandOver>,
        entry: Option<ReviewEntry>,
        actor: &Actor,
    ) -> Result<Ticket, BoardError> {
        let head = self.submit_head(&ticket).await?;
        let now = now_ms();
        // The agent's `stage next`, or the user's Move to next stage (STG-06), when no agent check comes next: those
        // run through `crate::validation::Validation::submit` (none on a flow with Validation off).
        let step = self
            .moved(&ticket, Status::Working, Move::SubmitByUser)
            .await?;
        let status = step.status;
        // On to another Agent stage: the ticket waits before it as a queued one does (STG-07), its wait counted from
        // now; the review's marks are a human check's.
        let waits = step.place.is_waiting();
        // "Submitted for review" only into a human check; anything else is "Finished Dev" (Q263).
        let flow = self.effective_flow(ticket.key.project.as_str()).await?;
        let into_human_check = !waits
            && step
                .place
                .stage()
                .and_then(|id| flow.stage(id))
                .is_some_and(crate::flow::Stage::is_human_check);
        let (kind, payload) = if into_human_check {
            (
                EventKind::ReviewSubmitted,
                // The stage it leaves and its head: what that stage's run committed ends there (`files.stages`).
                json!({
                    "summary": summary.trim(),
                    "head": head,
                    "stage_id": ticket.place.stage().unwrap_or_default(),
                }),
            )
        } else {
            (
                EventKind::StageFinished,
                stage_finished(&flow, &ticket, summary, &head),
            )
        };
        let mut tx = self.store.begin_write().await?;
        let posted = match hand_over {
            Some(h) => Some(h.post(&mut tx, &ticket.id, actor).await?),
            None => None,
        };
        step.record(&mut tx, &ticket.id, posted.as_ref().map(|p| p.id.as_str()))
            .await?;
        sqlx::query(concat!(
            "UPDATE tickets SET stage_id = ?, state = ?,
                                review_head_sha = CASE WHEN ? THEN review_head_sha ELSE ? END,
                                review_entered_at = CASE WHEN ? THEN review_entered_at ELSE ? END,
                                review_entry = CASE WHEN ? THEN review_entry ELSE ? END,
                                queued_at = CASE WHEN ? THEN ? ELSE queued_at END,
                                ready_since = CASE WHEN ? THEN NULL ELSE ready_since END,
                                signal_text = NULL, signal_since = NULL, wakeup_at = NULL, sent_back_at = NULL,
                                resume_skill = NULL, conflicts_with = NULL, conflict_files = NULL,
                                conflict_reason = NULL, pr_rework = FALSE, updated_by = ?,
                                updated_at = ?, ",
            set_state_since!(),
            " WHERE id = ?"
        ))
        .bind(step.place.stage_id())
        .bind(status.as_str())
        .bind(waits)
        .bind(&head)
        .bind(waits)
        .bind(now)
        .bind(waits)
        .bind(entry.map(|e| e.as_str()))
        .bind(waits)
        .bind(now)
        .bind(waits)
        .bind(actor.to_string())
        .bind(now)
        .bind(status.as_str())
        .bind(now)
        .bind(&ticket.id)
        .execute(&mut *tx)
        .await?;
        record_event(&mut tx, &ticket.id, kind, actor, payload).await?;
        tx.commit().await?;
        tracing::info!(key = %ticket.key, %actor, ?kind, "stage ended");
        self.ticket_by_id(&ticket.id).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_refusal_names_the_count() {
        assert_eq!(
            uncommitted_message(2),
            "2 files are not committed. Commit them, or discard them if they're scratch files"
        );
        assert_eq!(
            uncommitted_message(1),
            "1 file is not committed. Commit it, or discard it if it's a scratch file"
        );
    }
}

/// One commit made after the review began.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct NewCommit {
    pub sha: String,
    pub subject: String,
}

/// Has the ticket changed since it entered Ready for Review (REV-06, d0043)?
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum ApproveCheck {
    Unchanged,
    /// "SHOP-18 changed since you reviewed it · 1 new commit": the new commits, oldest first, and the files
    /// they changed (the diff from the reviewed head to the current head).
    Changed {
        new_commits: Vec<NewCommit>,
        files: Vec<crate::git::diff::ChangedFile>,
        /// The reviewed head, for `diff.file` of only the new commits.
        reviewed: String,
        head: String,
    },
}

/// "SHOP-18 changed since you reviewed it · 1 new commit".
pub fn changed_message(key: &str, commits: usize) -> String {
    let n = if commits == 1 {
        "1 new commit".to_owned()
    } else {
        format!("{commits} new commits")
    };
    format!("{key} changed since you reviewed it \u{b7} {n}")
}

impl Board {
    /// The check before any approve: new commits on the branch since its review head.
    pub async fn approve_check(&self, key: &str) -> Result<ApproveCheck, BoardError> {
        let ticket = self.ticket(key).await?;
        let Some(row) = self.store.ticket(&ticket.id).await? else {
            return Ok(ApproveCheck::Unchanged);
        };
        let (Some(reviewed), Some(branch)) = (row.review_head_sha, row.branch_name) else {
            return Ok(ApproveCheck::Unchanged);
        };
        let project = self.project(ticket.key.project.as_str()).await?;
        let repo = Path::new(&project.repo_path);
        let git = self.git()?;
        let Some(head) = git.resolve(repo, &format!("refs/heads/{branch}")).await? else {
            return Ok(ApproveCheck::Unchanged);
        };
        if head == reviewed {
            return Ok(ApproveCheck::Unchanged);
        }
        let range = format!("{reviewed}..{head}");
        let log = git
            .run(repo, &["log", "--reverse", "--format=%H%x09%s", &range])
            .await?;
        let new_commits: Vec<NewCommit> = log
            .lines()
            .filter_map(|l| l.split_once('\t'))
            .map(|(sha, subject)| NewCommit {
                sha: sha.to_owned(),
                subject: subject.to_owned(),
            })
            .collect();
        let files = git
            .changed_files(repo, &crate::git::diff::DiffSpec::between(&reviewed, &head))
            .await?;
        if new_commits.is_empty() && files.is_empty() {
            return Ok(ApproveCheck::Unchanged);
        }
        Ok(ApproveCheck::Changed {
            new_commits,
            files,
            reviewed,
            head,
        })
    }

    /// After Otaq itself rebased the ticket's branch from `old_head` to `new_head` (GIT-05): the rebase
    /// isn't a change to review. The reviewed head moves to its rebased counterpart: the new head when nothing
    /// was added after the review, else the commit as many commits below the new head as were added.
    pub async fn record_otaq_rebase(
        &self,
        ticket_id: &str,
        old_head: &str,
        new_head: &str,
    ) -> Result<(), BoardError> {
        if old_head == new_head {
            return Ok(());
        }
        let Some(row) = self.store.ticket(ticket_id).await? else {
            return Ok(());
        };
        let Some(reviewed) = row.review_head_sha else {
            return Ok(());
        };
        let project_key: String = sqlx::query_scalar(
            "SELECT p.key FROM projects p JOIN tickets t ON t.project_id = p.id WHERE t.id = ?",
        )
        .bind(ticket_id)
        .fetch_one(self.store.pool())
        .await?;
        let project = self.project(&project_key).await?;
        let repo = Path::new(&project.repo_path);
        let git = self.git()?;
        let added: u32 = if reviewed == old_head {
            0
        } else {
            let range = format!("{reviewed}..{old_head}");
            match git.run(repo, &["rev-list", "--count", &range]).await {
                Ok(n) => n.trim().parse().unwrap_or(0),
                // The reviewed head isn't on the branch any more: leave it, the check reports the change.
                Err(_) => return Ok(()),
            }
        };
        let rebased = git
            .run(repo, &["rev-parse", &format!("{new_head}~{added}")])
            .await?
            .trim()
            .to_owned();
        sqlx::query("UPDATE tickets SET review_head_sha = ? WHERE id = ?")
            .bind(rebased)
            .bind(ticket_id)
            .execute(self.store.pool())
            .await?;
        Ok(())
    }
}
