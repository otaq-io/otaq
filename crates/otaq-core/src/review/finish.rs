//! Done cleanup (REC-05; d0018, d0035, d0026): when a ticket reaches Done (Local merge, Close ticket, later Mark
//! merged), pure bookkeeping follows without asking, because the user's click finished it: its agent session
//! ends, its **ports return to the pool**, its worktree is **removed when clean** (the branch is kept), and a
//! worktree with uncommitted changes is **kept** and flagged for the Cleanup view. A clean one's gitignored files
//! are first kept as the project's warm build (WRM-03, `warm::keep`), unless they were freed at Review already
//! (M19-c, [`TicketEnv::release`](crate::ticket_env::TicketEnv::release)). Then the ticket's resources are
//! removed, and after a merge the project's template is refreshed (M19-d,
//! [`TicketEnv::release_resources`](crate::ticket_env::TicketEnv::release_resources)). Run and Shell (M9) stop
//! through the [`on_ticket_finished`](super::Review::on_ticket_finished) hook.
//!
//! **Only on Merged or Closed** (STG-11, d0108): a ticket stays in its Delivery stage until it is delivered, and
//! keeps its worktree there (To deliver, PR open: a Send back with PR comments works in it again). Entering a human
//! check or the Delivery only frees the build ([`crate::ticket_env::free`]).
//!
//! **Tickets merged into it land with it**: when the finished ticket counts as merged (Done · Merged, or Done without a
//! merge), every ticket still waiting as Merged into it becomes Done the same way, with a History line "Landed with
//! SHOP-18", and gets its own cleanup here, which lands the tickets merged into *it* in turn. Every way to Merged (Local
//! merge, the automatic Delivery, Mark merged, Done) runs this cleanup, so none of them repeats it. The callers then
//! start the waiting tickets ([`crate::scheduler::Scheduler::on_parent_merged`]), which also counts the tickets that
//! landed with the parent.

use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;

use super::Review;
use crate::board::tickets::Ticket;
use crate::board::{BoardError, Refusal};
use crate::domain::{Actor, Status, TicketKey};
use crate::session::SessionKind;

/// A finished ticket, for the hooks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FinishedTicket {
    pub id: String,
    pub key: TicketKey,
    /// The status it finished in (Merged, Closed · not merged).
    pub status: Status,
}

/// Called for every finished ticket (Run stops with a toast and the Shell closes, M9). The cleanup waits for the
/// returned future before it frees the ports and removes the worktree, so nothing still runs in them.
pub type FinishedHook =
    Arc<dyn Fn(&FinishedTicket) -> Pin<Box<dyn Future<Output = ()> + Send>> + Send + Sync>;

/// What the cleanup did.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Cleanup {
    pub session_stopped: bool,
    pub worktree_removed: bool,
    /// The worktree was kept: these files aren't committed.
    pub kept_dirty: Vec<String>,
}

impl Review {
    /// Registers a hook run for every ticket that reaches Done.
    pub fn on_ticket_finished(&self, hook: FinishedHook) {
        self.inner
            .finished_hooks
            .lock()
            .expect("finished hooks")
            .push(hook);
    }

    /// The cleanup after Done (REC-05); also used by Close ticket. A ticket that counts as merged then lands the
    /// tickets merged into it (see the module), even when its own cleanup failed.
    pub async fn finish(&self, ticket: &Ticket) -> Result<Cleanup, BoardError> {
        let cleanup = self.cleanup(ticket).await;
        if ticket.status == Status::Merged {
            Box::pin(self.land_merged_in(ticket)).await;
        }
        cleanup
    }

    /// Lands every ticket still waiting as Merged into `host`, which just counted as merged, and runs its cleanup
    /// (its worktree is already gone and its branch stays). A ticket that can't land is logged and stays Merged into.
    async fn land_merged_in(&self, host: &Ticket) {
        let board = &self.inner.board;
        let waiting = match board.waiting_in(host).await {
            Ok(waiting) => waiting,
            Err(e) => {
                tracing::warn!(host = %host.key, error = %e, "can't list the tickets merged into it");
                return;
            }
        };
        for ticket in waiting {
            // The user landed the host (the automatic Delivery runs as the user too).
            let actor = Actor::current_user();
            match board.record_landed_with(&ticket, host, &actor).await {
                Ok(landed) => {
                    if let Err(e) = self.finish(&landed).await {
                        tracing::warn!(ticket = %landed.key, error = %e, "Done cleanup failed");
                    }
                }
                Err(e) => {
                    tracing::warn!(ticket = %ticket.key, host = %host.key, error = %e, "can't land it with its host");
                }
            }
        }
    }

    async fn cleanup(&self, ticket: &Ticket) -> Result<Cleanup, BoardError> {
        let inner = &self.inner;
        let mut cleanup = Cleanup::default();
        if let Some(session) = inner.sessions.running_of(&ticket.id, SessionKind::Agent) {
            inner
                .sessions
                .stop(&session.id)
                .await
                .map_err(|e| BoardError::refused(Refusal::SessionEnded, e.to_string()))?;
            cleanup.session_stopped = true;
        }
        let finished = FinishedTicket {
            id: ticket.id.clone(),
            key: ticket.key.clone(),
            status: ticket.status,
        };
        let hooks = inner.finished_hooks.lock().expect("finished hooks").clone();
        for hook in hooks {
            hook(&finished).await;
        }
        inner
            .ports
            .free(&ticket.id)
            .await
            .map_err(|e| BoardError::refused(Refusal::SessionEnded, e.to_string()))?;
        let worktree = ticket
            .work
            .as_ref()
            .and_then(|w| w.worktree.clone())
            .map(PathBuf::from)
            .filter(|w| w.is_dir());
        // The worktree's result waits until the resources are released: a worktree that couldn't be handled
        // (a git error) is no reason to leave the ticket's resources behind unlisted.
        let worktree_done = async {
            let Some(worktree) = worktree else {
                return Ok(());
            };
            let board = &inner.board;
            let git = board.git()?;
            let dirty = git.uncommitted_files(&worktree).await?;
            if dirty.is_empty() {
                let project = board.project(ticket.key.project.as_str()).await?;
                // Its build goes to the stock first unless it was freed at Review (WRM-03, M19-c); never a reason
                // for Done to fail. Then the worktree goes and the build state is reset.
                inner
                    .ticket_env
                    .release(ticket, &project, &worktree)
                    .await?;
                cleanup.worktree_removed = true;
            } else {
                sqlx::query("UPDATE tickets SET worktree_kept = TRUE WHERE id = ?")
                    .bind(&ticket.id)
                    .execute(board.store().pool())
                    .await?;
                tracing::info!(ticket = %ticket.key, files = dirty.len(), "worktree kept: uncommitted changes");
                cleanup.kept_dirty = dirty;
            }
            Ok::<_, BoardError>(())
        }
        .await;
        // Then its resources (d0087), worktree removed or kept: Remove, and after a merge the Template refresh.
        let released = inner.ticket_env.release_resources(ticket).await;
        worktree_done?;
        released?;
        Ok(cleanup)
    }
}

/// `git worktree remove` (it refuses a worktree with changes by itself, a second guard); the branch stays. `git`
/// is the project's ([`crate::board::Board::project_git`]), so its variables reach the user's hooks (ENV-02).
pub(crate) async fn remove_worktree(
    git: &crate::git::Git,
    repo: &Path,
    worktree: &Path,
) -> Result<(), BoardError> {
    let path = worktree.display().to_string();
    git.run(repo, &["worktree", "remove", &path]).await?;
    Ok(())
}
