//! `otaq helper drop <n> --stdin` (T10): the agent in charge drops a piece, with the reason on stdin.
//!
//! 1. Refused for an empty reason, a piece with no helper, a merged piece ("40.1 is merged already") and one dropped
//!    already. Anyone but the agent in charge is refused, as for the other helper commands ([`Helpers::in_charge`]).
//! 2. The helper is `dropped`, the reason its note; its open question closes and its port goes back to the pool.
//! 3. The piece's helper session stops, if it runs; its worktree is removed (what it held is thrown away: the piece
//!    is dropped) and its branch deleted. The ticket's History reads "Developer dropped 40.3" · the reason.
//!
//! A dropped piece is never merged and can't start again. The agent in charge then takes it out of the Plan.

use std::path::Path;

use super::{HelperReply, HelperStatus, Helpers, dropped_already, store};
use crate::board::questions::AnsweredVia;
use crate::board::{BoardError, Refusal};
use crate::caller::Caller;
use crate::naming;
use crate::store::now_ms;

/// The refusal of a drop without a reason: "Say why 40.3 is dropped, on stdin: `otaq helper drop 3 --stdin`".
pub fn no_reason_text(label: &str, piece: u32) -> String {
    format!(
        "Say why {label} is dropped, on stdin: `{} helper drop {piece} --stdin`",
        naming::COMMAND_NAME
    )
}

/// What the agent in charge is told once a piece is dropped: "Dropped 40.3 (Wishlist page): its helper stopped and its
/// branch is gone. Take it out of the Plan with `otaq update --plan-stdin`."
pub fn dropped_text(label: &str, title: &str) -> String {
    format!(
        "Dropped {label} ({title}): its helper stopped and its branch is gone. Take it out of the Plan with `{} update \
         --plan-stdin`.",
        naming::COMMAND_NAME
    )
}

impl Helpers {
    /// `otaq helper drop <n> --stdin` from the agent in charge (see the module).
    pub async fn drop_piece(
        &self,
        caller: &Caller,
        key: &str,
        piece: u32,
        reason: &str,
    ) -> Result<HelperReply, BoardError> {
        let (ticket, _) = self.in_charge(caller, key).await?;
        let label = super::piece_label(&ticket.key, piece);
        if reason.trim().is_empty() {
            return Err(BoardError::refused(
                Refusal::EmptyField,
                no_reason_text(&label, piece),
            ));
        }
        let reason = crate::board::ticket_comments::check_body(reason)?;
        let lock = self.lock_of(&ticket.id);
        let _one = lock.lock().await;
        let inner = &self.inner;
        let board = &inner.board;
        let pool = board.store().pool();
        let helper = self.piece(&ticket, piece).await?;
        match helper.status {
            HelperStatus::Merged => {
                return Err(BoardError::refused(
                    Refusal::MoveRefused,
                    format!("{label} is merged already"),
                ));
            }
            HelperStatus::Dropped => return Err(dropped_already(&helper)),
            _ => {}
        }

        // 2. Dropped, with the reason, before its session ends (what an ended session reports reads the status); its
        // open question closes with it.
        store::dropped(pool, &helper.id, &reason).await?;
        sqlx::query(
            "UPDATE human_questions SET answered_at = ?, answered_via = ? WHERE helper_id = ? AND answered_at IS NULL",
        )
        .bind(now_ms())
        .bind(AnsweredVia::Closed.as_str())
        .bind(&helper.id)
        .execute(pool)
        .await?;

        // 3. Its session stops, its worktree and branch go.
        if let Some(live) = self.live_session(&helper)
            && let Err(e) = inner.sessions.stop(&live.id).await
        {
            tracing::warn!(helper = helper.id, error = %e, "can't stop a dropped helper's session");
        }
        let project = board.project(ticket.key.project.as_str()).await?;
        let repo = Path::new(&project.repo_path);
        let git = board.project_git(&project.key).await?;
        let worktree = Path::new(&helper.worktree);
        if worktree.is_dir()
            && let Err(e) = git
                .run(repo, &["worktree", "remove", "--force", &helper.worktree])
                .await
        {
            tracing::warn!(helper = helper.id, error = %e, "can't remove a dropped helper's worktree");
        }
        if let Err(e) = git
            .run(repo, &["branch", "--quiet", "-D", &helper.branch])
            .await
        {
            tracing::warn!(branch = helper.branch, error = %e, "can't delete a dropped helper's branch");
        }

        let helper = self.piece(&ticket, piece).await?;
        self.record(&ticket, &helper, "dropped", Some(&reason))
            .await?;
        self.wait_changed(&ticket.id, None).await?;
        Ok(HelperReply {
            text: dropped_text(&label, &helper.title),
            helper,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn texts() {
        let cmd = naming::COMMAND_NAME;
        assert_eq!(
            dropped_text("40.3", "Wishlist page"),
            format!(
                "Dropped 40.3 (Wishlist page): its helper stopped and its branch is gone. Take it out of the Plan with \
                 `{cmd} update --plan-stdin`."
            )
        );
        assert_eq!(
            no_reason_text("40.3", 3),
            format!("Say why 40.3 is dropped, on stdin: `{cmd} helper drop 3 --stdin`")
        );
    }
}
