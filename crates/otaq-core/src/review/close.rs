//! Close ticket (CLS-01, CLS-02; d0033, d0044).
//!
//! **Close ticket** works from any state that isn't finished: if an agent runs, the request needs `confirm`
//! ("its agent will be stopped first"). The ticket becomes Done · Closed · not merged; the Done cleanup follows
//! (T-0099): the agent stops, ports return, the worktree is removed unless it has uncommitted changes, the
//! **branch is kept**. Mark closed on a PR ticket (M10) takes the same path.
//!
//! **`otaq stage done`** (AGM-10, d0108) is the agent's close: from an Agent stage, only when the ticket's branch has
//! no commits beyond its base, Done as **Closed · nothing to merge** with the agent's reason. Reopen works as for any
//! closed ticket (d0033).

use std::path::Path;

use serde_json::json;

use super::Review;
use crate::board::close::{close_bookkeeping, close_bookkeeping_noting};
use crate::board::tickets::Ticket;
use crate::board::{BoardError, Refusal};
use crate::domain::{Actor, Status};
use crate::flow::engine::RunResult;
use crate::naming::COMMAND_NAME as CMD;
use crate::session::SessionKind;
use crate::signals::LIVE;

/// `stage done`'s refusal when there is work to deliver (AGM-10): "SHOP-18 has 2 commits. Use `otaq stage next` so
/// they're checked and delivered."
pub fn has_commits_message(key: &str, commits: u32) -> String {
    if commits == 1 {
        format!("{key} has 1 commit. Use `{CMD} stage next` so it's checked and delivered.")
    } else {
        format!(
            "{key} has {commits} commits. Use `{CMD} stage next` so they're checked and delivered."
        )
    }
}

impl Review {
    /// **Close ticket**: `confirm` is the user's yes to stopping its running agent.
    pub async fn close(
        &self,
        key: &str,
        confirm: bool,
        actor: &Actor,
    ) -> Result<Ticket, BoardError> {
        let board = &self.inner.board;
        let ticket = board.ticket(key).await?;
        match ticket.status {
            Status::Merged => {
                return Err(BoardError::refused(
                    Refusal::MoveRefused,
                    format!("{} is merged; merged tickets are locked", ticket.key),
                ));
            }
            Status::ClosedNotMerged => return Ok(ticket),
            _ => {}
        }
        if !confirm && self.agent_runs(&ticket) {
            return Err(BoardError::refused(
                Refusal::NeedsConfirmation,
                format!(
                    "{}'s agent is working. It will be stopped first.",
                    ticket.key
                ),
            ));
        }
        self.close_one(&ticket, actor).await
    }

    /// **`otaq stage done`** (AGM-10): ends a ticket in an Agent stage as Done · Closed · nothing to merge, the reason
    /// being the agent's comment (`hand_over`, CMT-05) and History's line. Refused outside an Agent stage,
    /// and when the ticket's branch has commits beyond its base. `own_session` is the calling session: when it is the
    /// ticket's agent, the Done cleanup (which ends that session) waits for its turn to end
    /// ([`Review::after_agent_turn`]).
    pub async fn stage_done(
        &self,
        key: &str,
        hand_over: Option<&crate::board::hand_over::HandOver>,
        own_session: Option<&str>,
        actor: &Actor,
    ) -> Result<Ticket, BoardError> {
        let board = &self.inner.board;
        let ticket = board.ticket(key).await?;
        let flow = board.effective_flow(ticket.key.project.as_str()).await?;
        let in_agent_stage = !ticket.place.is_waiting()
            && ticket
                .place
                .stage()
                .and_then(|id| flow.stage(id))
                .is_some_and(crate::flow::Stage::is_agent_stage);
        if !in_agent_stage || !LIVE.contains(&ticket.status) {
            return Err(BoardError::refused(
                Refusal::MoveRefused,
                format!(
                    "{} is {}; only a ticket whose agent works in an Agent stage can end with `{CMD} stage done`",
                    ticket.key,
                    ticket.status.label()
                ),
            ));
        }
        let commits = self.commits_beyond_base(&ticket).await?;
        if commits > 0 {
            return Err(BoardError::refused(
                Refusal::MoveRefused,
                has_commits_message(&ticket.key.to_string(), commits),
            ));
        }
        let reason = hand_over.map_or("", |h| h.body.as_str());
        let mut tx = board.store().begin_write().await?;
        let posted = match hand_over {
            Some(h) => Some(h.post(&mut tx, &ticket.id, actor).await?),
            None => None,
        };
        crate::board::stage_runs::end_open(
            &mut tx,
            &ticket.id,
            RunResult::Done,
            posted.as_ref().map(|p| p.id.as_str()),
        )
        .await?;
        close_bookkeeping_noting(
            &mut tx,
            &ticket.id,
            ticket.status,
            actor,
            json!({ "nothing_to_merge": true, "reason": reason }),
        )
        .await?;
        sqlx::query(
            "UPDATE tickets SET parked = FALSE, sent_back_at = NULL, resume_skill = NULL,
                                signal_text = NULL, signal_since = NULL, wakeup_at = NULL
             WHERE id = ?",
        )
        .bind(&ticket.id)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        tracing::info!(ticket = %ticket.key, from = %ticket.status, "closed: nothing to merge");
        let closed = board.ticket_by_id(&ticket.id).await?;
        let own_agent = own_session.is_some_and(|session| {
            self.inner
                .sessions
                .running_of(&ticket.id, SessionKind::Agent)
                .is_some_and(|s| s.id == session)
        });
        if !own_agent && let Err(e) = self.finish(&closed).await {
            tracing::warn!(ticket = %closed.key, error = %e, "cleanup after stage done failed");
        }
        board.ticket_by_id(&ticket.id).await
    }

    /// A ticket agent's turn ended: when its ticket is finished meanwhile (`otaq stage done` from that session), the
    /// Done cleanup follows now and ends the session.
    pub async fn after_agent_turn(&self, ticket_id: &str) -> Result<(), BoardError> {
        let ticket = self.inner.board.ticket_by_id(ticket_id).await?;
        if matches!(ticket.status, Status::Merged | Status::ClosedNotMerged)
            && self.agent_runs(&ticket)
        {
            self.finish(&ticket).await?;
        }
        Ok(())
    }

    /// The commits on the ticket's branch beyond its base: in its worktree, else on the branch; none without either.
    async fn commits_beyond_base(&self, ticket: &Ticket) -> Result<u32, BoardError> {
        let board = &self.inner.board;
        let Some(work) = &ticket.work else {
            return Ok(0);
        };
        let Some(base) = &work.base_sha else {
            return Ok(0);
        };
        let git = board.git()?;
        let (cwd, head) = match (&work.worktree, &work.branch) {
            (Some(worktree), _) if Path::new(worktree).is_dir() => {
                (worktree.clone(), "HEAD".to_owned())
            }
            (_, Some(branch)) => (
                board.project(ticket.key.project.as_str()).await?.repo_path,
                format!("refs/heads/{branch}"),
            ),
            _ => return Ok(0),
        };
        let range = format!("{base}..{head}");
        let count = git
            .run(Path::new(&cwd), &["rev-list", "--count", &range])
            .await?;
        Ok(count.trim().parse().unwrap_or(0))
    }

    fn agent_runs(&self, ticket: &Ticket) -> bool {
        self.inner
            .sessions
            .running_of(&ticket.id, SessionKind::Agent)
            .is_some()
    }

    async fn close_one(&self, ticket: &Ticket, actor: &Actor) -> Result<Ticket, BoardError> {
        let board = &self.inner.board;
        let mut tx = board.store().begin_write().await?;
        close_bookkeeping(&mut tx, &ticket.id, ticket.status, actor).await?;
        sqlx::query(
            "UPDATE tickets SET parked = FALSE, sent_back_at = NULL, resume_skill = NULL,
                                signal_text = NULL, signal_since = NULL, wakeup_at = NULL
             WHERE id = ?",
        )
        .bind(&ticket.id)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        tracing::info!(ticket = %ticket.key, from = %ticket.status, "closed");
        let closed = board.ticket_by_id(&ticket.id).await?;
        // Branch kept; worktree removed unless dirty; agent stopped; ports freed (the Done cleanup).
        if let Err(e) = self.finish(&closed).await {
            tracing::warn!(ticket = %closed.key, error = %e, "cleanup after close failed");
        }
        board.ticket_by_id(&ticket.id).await
    }
}
