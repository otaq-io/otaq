//! **Merge into another ticket** (the Approve menu's "Merge into another ticket…"): the user squashes this ticket's
//! branch into another open ticket's branch (its host), so the two land together, in one PR if the host opens one.
//! The git steps are a helper's merge into its ticket's branch ([`crate::helpers::merge`]):
//!
//! 1. Refused while either worktree has uncommitted files ("SHOP-20 has 2 uncommitted files"): nothing is
//!    checkpointed or stashed.
//! 2. **Free rebase** of this ticket's branch onto the host's, in this ticket's worktree. A conflict puts everything
//!    back and merges nothing; the ticket stays where it was.
//! 3. **Squash-merge** into the host's branch in the host's worktree: one commit named after this ticket, the user's
//!    hooks running as for any commit; a hook that refuses it leaves the host's worktree as it was.
//! 4. This ticket waits as Merged into the host. It isn't Done, so tickets waiting on it keep waiting; its worktree is
//!    removed and its branch kept (the Done cleanup's steps). The host goes back to the human check before its
//!    Delivery with the combined diff; with a PR open, Push to PR then carries the new commit. Which tickets went into
//!    which is the `merged_into` table ([`crate::store::merged_into`]).

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::Review;
use crate::board::merge_into::can_be_merged_in;
use crate::board::tickets::Ticket;
use crate::board::{BoardError, Refusal};
use crate::domain::{Actor, TicketKey};
use crate::git::checkpoint::{RebaseError, RebaseOutcome};
use crate::git::merge::uncommitted_files_message;
use crate::session::SessionKind;

/// The answer to Merge into another ticket.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum MergeIntoOutcome {
    /// Squashed into the host's branch as `commit`: `ticket` waits as Merged into `into`, which is back in review.
    Merged {
        ticket: Box<Ticket>,
        into: Box<Ticket>,
        commit: String,
    },
    /// "SHOP-20 has 2 uncommitted files" (this ticket's or the host's): nothing was merged.
    RefusedUncommitted { files: Vec<String>, message: String },
    /// The rebase onto the host's branch conflicted on these files: everything was put back and nothing merged.
    Conflict { into: TicketKey, files: Vec<String> },
}

/// What the conflict says: "SHOP-20 conflicts with SHOP-18 in lib.ts: nothing was merged. SHOP-20 stays where it
/// was."
pub fn conflict_message(key: &TicketKey, into: &TicketKey, files: &[String]) -> String {
    format!(
        "{key} conflicts with {into} in {}: nothing was merged. {key} stays where it was.",
        files.join(", ")
    )
}

impl Review {
    /// **Merge into another ticket**: squashes `key`'s branch into `into`'s (see the module).
    pub async fn merge_into(
        &self,
        key: &str,
        into: &str,
        actor: &Actor,
    ) -> Result<MergeIntoOutcome, BoardError> {
        let inner = &self.inner;
        let board = &inner.board;
        // One merge at a time: a Local merge of the host must not run while its branch takes this one.
        let _one_at_a_time = inner.merging.lock().await;
        let ticket = board.ticket(key).await?;
        let host = board.ticket(into).await?;
        if !can_be_merged_in(&ticket) {
            let why = match ticket.flags.pr_number {
                Some(n) => format!("{} has PR #{n} open; push to it instead", ticket.key),
                None => format!(
                    "{} is {}; Merge into another ticket works on a ticket in Ready for Review",
                    ticket.key,
                    ticket.status.label()
                ),
            };
            return Err(BoardError::refused(Refusal::MoveRefused, why));
        }
        let hosts = board.merge_into_hosts(key).await?;
        if !hosts.iter().any(|h| h.id == host.id) {
            return Err(BoardError::refused(
                Refusal::MoveRefused,
                format!(
                    "{} can't take {}: it must be a ticket of the same project and base that waits in a review, \
                     at its Delivery or with a PR open, with a branch",
                    host.key, ticket.key
                ),
            ));
        }
        for t in [&ticket, &host] {
            if inner
                .sessions
                .running_of(&t.id, SessionKind::Delivery)
                .is_some()
            {
                return Err(BoardError::refused(
                    Refusal::AlreadyRunning,
                    format!("{}'s Delivery is running", t.key),
                ));
            }
        }
        let branch = branch_of(&ticket)?;
        let host_branch = branch_of(&host)?;
        let ticket_wt = worktree_of(&ticket)?;
        let host_wt = worktree_of(&host)?;
        let project = board.project(ticket.key.project.as_str()).await?;
        let repo = PathBuf::from(&project.repo_path);
        // With the project's variables: the rebase and the commit run the user's hooks (ENV-02).
        let git = board.project_git(&project.key).await?;

        // 1. Both worktrees clean.
        for (t, folder) in [(&ticket, &ticket_wt), (&host, &host_wt)] {
            let files = git.uncommitted_files(folder).await?;
            if !files.is_empty() {
                return Ok(MergeIntoOutcome::RefusedUncommitted {
                    message: uncommitted_files_message(&t.key.to_string(), files.len()),
                    files,
                });
            }
        }

        // 2. The free rebase onto the host's branch, in this ticket's worktree.
        let branch_ref = format!("refs/heads/{branch}");
        let before = git.resolve(&repo, &branch_ref).await?;
        let onto = git
            .resolve(&repo, &format!("refs/heads/{host_branch}"))
            .await?
            .ok_or_else(|| {
                BoardError::refused(
                    Refusal::NoBranch,
                    format!("{}'s branch {host_branch} is gone", host.key),
                )
            })?;
        match git.rebase_with_checkpoint(&ticket_wt, &onto).await {
            Ok(RebaseOutcome::Clean) => {}
            Ok(RebaseOutcome::RebaseConflict { files } | RebaseOutcome::UndoConflict { files }) => {
                return Ok(MergeIntoOutcome::Conflict {
                    into: host.key,
                    files,
                });
            }
            Err(RebaseError::Git(e)) => return Err(e.into()),
            // Step 1 found it clean: no checkpoint was made.
            Err(e) => return Err(BoardError::refused(Refusal::MoveRefused, e.to_string())),
        }
        // The free rebase isn't a change to review (GIT-05), whatever happens next.
        if let (Some(before), Some(after)) = (before, git.resolve(&repo, &branch_ref).await?) {
            board
                .record_otaq_rebase(&ticket.id, &before, &after)
                .await?;
        }
        let range = format!("{onto}..{branch}");
        let subjects = git
            .run(&repo, &["log", "--reverse", "--format=%s", &range])
            .await?;
        if subjects.trim().is_empty() {
            return Err(BoardError::refused(
                Refusal::MoveRefused,
                format!(
                    "{} has nothing to merge into {}: its branch has no commits of its own",
                    ticket.key, host.key
                ),
            ));
        }

        // 3. The squash, in the host's worktree: one commit named after this ticket.
        let mut message = format!("{}: {}\n\n", ticket.key, ticket.title);
        for subject in subjects.lines().filter(|s| !s.trim().is_empty()) {
            message.push_str(&format!("- {subject}\n"));
        }
        crate::helpers::merge::squash(&git, &host_wt, branch, &message).await?;
        let commit = git
            .run(&host_wt, &["rev-parse", "HEAD"])
            .await?
            .trim()
            .to_owned();

        // 4. Merged into the host, which is back in review; this ticket's worktree goes, its branch stays.
        let (merged, host) = board
            .record_merged_into(&ticket, &host, &commit, actor)
            .await?;
        if let Err(e) = self.finish(&merged).await {
            tracing::warn!(ticket = %merged.key, error = %e, "cleanup after merging into another ticket failed");
        }
        // The host's branch changed: the early conflict flag of the tickets in Review that merge into the same base.
        if let Err(e) = self
            .flag_conflicts(&ticket.key.project.to_string(), &repo, ticket.base.branch())
            .await
        {
            tracing::warn!(error = %e, "can't test-merge the tickets in Review");
        }
        Ok(MergeIntoOutcome::Merged {
            ticket: Box::new(board.ticket_by_id(&merged.id).await?),
            into: Box::new(board.ticket_by_id(&host.id).await?),
            commit,
        })
    }
}

fn branch_of(ticket: &Ticket) -> Result<&str, BoardError> {
    ticket
        .work
        .as_ref()
        .and_then(|w| w.branch.as_deref())
        .ok_or_else(|| {
            BoardError::refused(Refusal::NoBranch, format!("{} has no branch", ticket.key))
        })
}

fn worktree_of(ticket: &Ticket) -> Result<PathBuf, BoardError> {
    ticket
        .work
        .as_ref()
        .and_then(|w| w.worktree.as_deref())
        .map(PathBuf::from)
        .filter(|w| Path::new(w).is_dir())
        .ok_or_else(|| {
            BoardError::refused(
                Refusal::NoWorktree,
                format!("{} has no worktree", ticket.key),
            )
        })
}
