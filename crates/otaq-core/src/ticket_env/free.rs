//! **Free** (M19-c, d0086, STG-11): when a ticket enters a human check or its Delivery stage (the default flow's
//! Review, and PR open), its build output (the gitignored files, not the env files)
//! leaves the worktree for the stock, so the next ticket can start from it. The worktree, branch, ports and agent
//! session stay as they are.
//!
//! - Only in [`FREED_IN`], only while the build is in the worktree (`build_state = present`).
//! - A Run or Shell still open holds it ([`FreeOutcome::Waiting`]): Otaq never stops the user's process. The Free
//!   runs again when it closes.
//! - Warm builds on: the files are added to the stock ([`Stock::add`], which applies the limit). Off: they are
//!   deleted (d0086: "deleted at Review and at Done").
//! - A failure leaves the build where it was (the rename moves back what it moved), records the problem for Cleanup's
//!   "could not free" group and never changes the ticket's status.

use std::path::{Path, PathBuf};

use crate::board::tickets::Ticket;
use crate::domain::{BuildState, Status, StatusSet, TicketKey};
use crate::protocol::Event;
use crate::session::{SessionKind, Sessions};
use crate::ticket_env::TicketEnv;
use crate::ticket_env::stock::Added;
use crate::warm::keep;

/// The statuses whose build is freed: every status of a human check or a Delivery stage (STG-11; the default flow's
/// Ready for Review, and Approved · PR open, where an approved ticket stays freed). [`freed_in`] says the same by
/// status set.
pub const FREED_IN: [Status; 3] = [Status::WaitingForYou, Status::ToDeliver, Status::PrOpen];

/// Whether a ticket with this status has its build freed: it waits in a human check or a Delivery stage (STG-11).
pub fn freed_in(status: Status) -> bool {
    matches!(status.set(), StatusSet::HumanCheck | StatusSet::Delivery)
}

/// What a Free did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FreeOutcome {
    /// The build left the worktree: into this stock entry, or nowhere (nothing gitignored, warm builds off, or the
    /// stock didn't keep it).
    Freed { entry: Option<u64> },
    /// A Run or Shell of the ticket is open: freed once it closes.
    Waiting,
    /// Nothing to free: not in Review, no worktree, or the build isn't in the worktree.
    NotNeeded,
    /// The build stays in the worktree: why.
    Failed(String),
}

/// A Run or Shell of the ticket is running.
pub(crate) fn has_live_terminal(sessions: &Sessions, ticket_id: &str) -> bool {
    [SessionKind::Run, SessionKind::Shell]
        .into_iter()
        .any(|kind| sessions.running_of(ticket_id, kind).is_some())
}

/// The ticket can have its build freed, judged from its row: in [`FREED_IN`], its build in its worktree. Returns the worktree.
pub(crate) fn to_free(ticket: &Ticket) -> Option<PathBuf> {
    let work = ticket.work.as_ref()?;
    if !freed_in(ticket.status) || work.build_state != BuildState::Present {
        return None;
    }
    work.worktree.as_deref().map(PathBuf::from)
}

/// Frees `key`'s build under its lock, after reading the ticket again.
pub(crate) async fn free_one(env: &TicketEnv, key: &TicketKey) -> FreeOutcome {
    let board = env.board();
    let Ok(id) = board.ticket_id(&key.to_string()).await else {
        return FreeOutcome::NotNeeded;
    };
    let _held = env.lock(&id).await;
    let Ok(ticket) = board.ticket_by_id(&id).await else {
        return FreeOutcome::NotNeeded;
    };
    let Some(worktree) = to_free(&ticket).filter(|w| w.is_dir()) else {
        return FreeOutcome::NotNeeded;
    };
    if has_live_terminal(env.sessions(), &id) {
        tracing::debug!(ticket = %ticket.key, "a Run or Shell is open: the build is freed when it closes");
        return FreeOutcome::Waiting;
    }
    let (state, problem, outcome) = match move_out(env, &ticket, &worktree).await {
        Ok(entry) => {
            tracing::info!(ticket = %ticket.key, entry, "build freed at Review");
            (BuildState::Freed, None, FreeOutcome::Freed { entry })
        }
        Err(problem) => {
            tracing::warn!(ticket = %ticket.key, error = %problem, "can't free the build; it stays in the worktree");
            (
                BuildState::Present,
                Some(problem.clone()),
                FreeOutcome::Failed(problem),
            )
        }
    };
    if let Err(e) = board
        .store()
        .set_build_state(&id, state, problem.as_deref())
        .await
    {
        tracing::warn!(ticket = %ticket.key, error = %e, "can't record where the build is");
        return FreeOutcome::Failed(e.to_string());
    }
    env.events().publish(Event::TicketChanged {
        project: ticket.key.project.to_string(),
        id,
        key: ticket.key.clone(),
    });
    outcome
}

/// Moves the gitignored files of `worktree` into the stock (or deletes them, with warm builds off).
async fn move_out(
    env: &TicketEnv,
    ticket: &Ticket,
    worktree: &Path,
) -> Result<Option<u64>, String> {
    let board = env.board();
    let project = board
        .project(ticket.key.project.as_str())
        .await
        .map_err(|e| e.to_string())?;
    let collected = keep::collect(board, &project, worktree)
        .await
        .map_err(|e| e.to_string())?;
    if collected.paths.is_empty() {
        return Ok(None);
    }
    let warm = env.warm();
    if !warm
        .enabled(&project.key)
        .await
        .map_err(|e| e.to_string())?
    {
        keep::move_to_trash(warm, &project, worktree, collected)
            .await
            .map_err(|e| e.to_string())?;
        return Ok(None);
    }
    let added = keep::move_to_stock(
        env.stock(),
        warm,
        &project,
        &ticket.key,
        worktree,
        collected,
    )
    .await
    .map_err(|e| e.to_string())?;
    Ok(match added {
        Added::Kept { entry } => Some(entry),
        Added::Dropped | Added::Evicted { .. } => None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// STG-11: the list the other modules read holds exactly the human check's and the Delivery's statuses.
    #[test]
    fn freed_in_every_human_check_and_delivery_status() {
        for status in Status::ALL {
            assert_eq!(FREED_IN.contains(status), freed_in(*status), "{status}");
        }
    }
}
