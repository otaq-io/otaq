//! **Merge into another ticket** (the Approve menu's "Merge into another ticket…"): the user squashes this ticket's
//! branch into another open ticket's branch (its host), so the two land together, in one PR if the host opens one.
//! The git steps are a helper's merge into its ticket's branch ([`crate::helpers::merge`]):
//!
//! 1. Refused while either worktree has uncommitted files ("SHOP-20 has 2 uncommitted files"): nothing is
//!    checkpointed or stashed.
//! 2. **The host's branch is merged into this ticket's**, in this ticket's worktree, never a rebase; the merge commit runs
//!    the user's hooks. A conflict or a refusal puts everything back and merges nothing; the ticket stays where it was.
//! 3. **Squash-merge** into the host's branch in the host's worktree: one commit named after this ticket, the user's
//!    hooks running as for any commit; a hook that refuses it leaves the host's worktree as it was.
//! 4. This ticket waits as Merged into the host. It isn't Done, so tickets waiting on it keep waiting; its worktree is
//!    removed and its branch kept (the Done cleanup's steps). The host goes back to the human check before its
//!    Delivery with the combined diff; with a PR open, Push to PR then carries the new commit. Which tickets went into
//!    which is the `merged_into` table ([`crate::store::merged_into`]).
//!
//! When the host is closed without landing (Close ticket, Mark closed), every ticket merged into it goes back to the
//! human check before its Delivery on its own kept branch ([`Review::host_closed`]): its worktree is added again from
//! that branch, and the host's commits that Otaq's merge brought into it come off again, so it can land alone or
//! be merged into another ticket. The host's worktree and branch are never touched.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::Review;
use crate::board::merge_into::can_be_merged_in;
use crate::board::tickets::Ticket;
use crate::board::{BoardError, Refusal};
use crate::domain::{Actor, Status, TicketKey};
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
    /// Merging the host's branch into this one conflicted on these files: everything was put back and nothing merged.
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
                    "{} is {}; Merge into another ticket works on a ticket in your review",
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
        // With the project's variables: the merge commit and the squash commit run the user's hooks (ENV-02).
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

        // 2. The host's branch merged into this ticket's, in this ticket's worktree (clean, by step 1), never a rebase:
        // by its name, so the message is git's plain "Merge branch …"; the merge commit runs the user's hooks.
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
        if let Some(files) = git.merge_in(&ticket_wt, host_branch).await? {
            return Ok(MergeIntoOutcome::Conflict {
                into: host.key,
                files,
            });
        }
        // Otaq's own update isn't a change to review (GIT-05), whatever happens next.
        if let (Some(before), Some(after)) = (before, git.resolve(&repo, &branch_ref).await?) {
            board
                .record_otaq_update(&ticket.id, &before, &after)
                .await?;
        }
        let range = format!("{onto}..{branch}");
        let subjects = git
            .run(
                &repo,
                &["log", "--reverse", "--no-merges", "--format=%s", &range],
            )
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

impl Review {
    /// `host` was closed without landing: every ticket merged into it goes back to review on its own branch (see the
    /// module). A ticket whose worktree can't be added again still goes back; the failure is logged. The tickets that
    /// went back.
    pub(crate) async fn host_closed(&self, host: &Ticket, actor: &Actor) -> Vec<Ticket> {
        let board = &self.inner.board;
        let merged_in = match board.merged_in(&host.key.to_string()).await {
            Ok(rows) => rows,
            Err(e) => {
                tracing::warn!(host = %host.key, error = %e, "can't list the tickets merged into the closed ticket");
                return Vec::new();
            }
        };
        let mut back = Vec::new();
        for row in merged_in {
            let ticket = match board.ticket_by_id(&row.ticket_id).await {
                Ok(t) if t.status == Status::MergedInto => t,
                // Rows stay after the host landed, as the record: only a ticket still waiting for it goes back.
                Ok(_) => continue,
                Err(e) => {
                    tracing::warn!(ticket = %row.key, error = %e, "can't read a ticket merged into the closed one");
                    continue;
                }
            };
            if let Err(e) = self.own_worktree_again(&ticket, &row.commit).await {
                tracing::warn!(ticket = %ticket.key, error = %e, "can't add the worktree again on its own branch");
            }
            match board.record_host_closed(&ticket, host, actor).await {
                Ok(t) => back.push(t),
                Err(e) => {
                    tracing::warn!(ticket = %ticket.key, error = %e, "can't send it back to review");
                }
            }
        }
        if let Some(first) = back.first() {
            let project = board.project(first.key.project.as_str()).await;
            if let Ok(project) = project
                && let Err(e) = self
                    .flag_conflicts(
                        &first.key.project.to_string(),
                        &PathBuf::from(&project.repo_path),
                        first.base.branch(),
                    )
                    .await
            {
                tracing::warn!(error = %e, "can't test-merge the tickets in Review");
            }
        }
        back
    }

    /// Adds `ticket`'s worktree again on its own kept branch, as a reopened ticket's start does, then takes the host's
    /// commits off it: Otaq had merged the host's branch into it, at the parent of `squash` (the squash commit on the
    /// host's branch), and the branch goes back to the commit before that merge. Anything else on the branch since
    /// leaves it as it was.
    async fn own_worktree_again(&self, ticket: &Ticket, squash: &str) -> Result<(), BoardError> {
        let inner = &self.inner;
        let board = &inner.board;
        let branch = branch_of(ticket)?.to_owned();
        let project = board.project(ticket.key.project.as_str()).await?;
        let repo = PathBuf::from(&project.repo_path);
        // With the project's variables: `worktree add` runs the user's hooks (ENV-02).
        let git = board.project_git(&project.key).await?;
        let repo_name = repo
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| project.name.clone());
        let path = inner
            .data
            .worktree_path(&repo_name, &ticket.key.to_string())
            .map_err(|e| BoardError::refused(Refusal::NoWorktree, e.to_string()))?;
        crate::start::worktree::add_worktree(&git, &repo, &path, &branch)
            .await
            .map_err(|e| BoardError::refused(Refusal::NoWorktree, e.to_string()))?;
        sqlx::query("UPDATE tickets SET worktree_path = ?, worktree_kept = FALSE WHERE id = ?")
            .bind(path.display().to_string())
            .bind(&ticket.id)
            .execute(board.store().pool())
            .await?;

        // The host's commits come off: only when the branch still stands on the host's at the squash's parent.
        let Some(base) = board
            .store()
            .ticket(&ticket.id)
            .await?
            .and_then(|row| row.base_sha)
        else {
            return Ok(());
        };
        let branch_ref = format!("refs/heads/{branch}");
        let Some(before) = git.resolve(&repo, &branch_ref).await? else {
            return Ok(());
        };
        let Ok(onto) = git.run(&repo, &["rev-parse", &format!("{squash}^")]).await else {
            return Ok(());
        };
        let onto = onto.trim();
        let is_ancestor = async |a: &str, b: &str| {
            git.run(&repo, &["merge-base", "--is-ancestor", a, b])
                .await
                .is_ok()
        };
        if onto == base || !is_ancestor(onto, &before).await || is_ancestor(onto, &base).await {
            return Ok(());
        }
        if !git.uncommitted_files(&path).await?.is_empty() {
            return Ok(());
        }
        // Otaq's merge of the host's branch is the branch's head (its second parent the host's tip then): the branch
        // goes back to its first parent. A fast-forward onto the host's tip goes back to the base.
        let parents = git
            .run(&repo, &["rev-list", "--parents", "-n", "1", &before])
            .await?;
        let parents: Vec<&str> = parents.split_whitespace().skip(1).collect();
        let back_to = match parents.as_slice() {
            [own, host] if *host == onto => own.to_string(),
            _ if before == onto => base.clone(),
            _ => return Ok(()),
        };
        if let Err(e) = git
            .run(&path, &["reset", "--quiet", "--hard", &back_to])
            .await
        {
            tracing::warn!(ticket = %ticket.key, error = %e, "the host's commits don't come off its branch; left as it was");
            return Ok(());
        }
        // Otaq's own update isn't a change to review (GIT-05).
        if let Some(after) = git.resolve(&repo, &branch_ref).await? {
            board
                .record_otaq_update(&ticket.id, &before, &after)
                .await?;
        }
        Ok(())
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
