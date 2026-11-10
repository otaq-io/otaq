//! Where a ticket's branch stands on its base now: the stored `base_sha` moved up after a rebase Otaq didn't make.
//!
//! `base_sha` is recorded when the ticket starts and after Otaq's own rebases, but a branch is also rebased by the
//! agent (the `restack` skill), by the user by hand, or onto the base's remote branch. Then the stored commit is
//! older than where the branch left its base, and everything counted from it (the Files tab, Open PR's commits,
//! the agent's committed diff) shows the base's newer commits as the ticket's own.
//!
//! The current base is the newest `merge-base` of the branch with its base branch (local, or a remote's) that is
//! still a descendant of the stored `base_sha`; it is written back so every reader agrees. A Done ticket keeps
//! its own, and so does a branch whose commits are all on the base already (nothing would be left to show).

use std::path::Path;

use super::tickets::Ticket;
use super::{Board, BoardError};
use crate::git::Git;

impl Board {
    /// The ticket's base now (see the module), recorded when it moved; `None` without a `base_sha`.
    pub async fn current_base_sha(&self, ticket: &Ticket) -> Result<Option<String>, BoardError> {
        let Some(work) = &ticket.work else {
            return Ok(None);
        };
        let Some(stored) = &work.base_sha else {
            return Ok(None);
        };
        if ticket.place.is_done() {
            return Ok(Some(stored.clone()));
        }
        // Where the readers count: the worktree's HEAD when it's there, else the branch in the repository.
        let (cwd, head) = match (
            work.worktree.as_deref().filter(|w| Path::new(w).is_dir()),
            &work.branch,
        ) {
            (Some(worktree), _) => (worktree.to_owned(), "HEAD".to_owned()),
            (None, Some(branch)) => (
                self.project(ticket.key.project.as_str()).await?.repo_path,
                format!("refs/heads/{branch}"),
            ),
            (None, None) => return Ok(Some(stored.clone())),
        };
        let fresh = match newer_base(
            self.git()?,
            Path::new(&cwd),
            &head,
            ticket.base.branch(),
            stored,
        )
        .await
        {
            Ok(fresh) => fresh,
            Err(e) => {
                tracing::warn!(key = %ticket.key, error = %e, "can't work out the ticket's current base");
                None
            }
        };
        let Some(fresh) = fresh else {
            return Ok(Some(stored.clone()));
        };
        sqlx::query("UPDATE tickets SET base_sha = ? WHERE id = ? AND base_sha = ?")
            .bind(&fresh)
            .bind(&ticket.id)
            .bind(stored)
            .execute(self.store().pool())
            .await
            .map_err(crate::store::StoreError::from)?;
        tracing::info!(key = %ticket.key, from = %stored, to = %fresh, "the ticket's base moved");
        Ok(Some(fresh))
    }
}

/// The newest merge-base of `head` with `base` (or a remote's `base`) that descends from `stored` and isn't it;
/// `None` when the branch still stands at `stored`, or its commits are all on the base.
async fn newer_base(
    git: &Git,
    repo: &Path,
    head: &str,
    base: &str,
    stored: &str,
) -> Result<Option<String>, crate::git::GitError> {
    let Some(head) = git.resolve(repo, head).await? else {
        return Ok(None);
    };
    let remotes = git
        .run(
            repo,
            &[
                "for-each-ref",
                "--format=%(refname)",
                &format!("refs/remotes/*/{base}"),
            ],
        )
        .await?;
    let mut candidates = vec![format!("refs/heads/{base}")];
    candidates.extend(remotes.lines().map(str::to_owned));
    let mut best: Option<String> = None;
    for candidate in candidates {
        if git.resolve(repo, &candidate).await?.is_none() {
            continue;
        }
        let Ok(found) = git.run(repo, &["merge-base", &head, &candidate]).await else {
            continue;
        };
        let found = found.trim().to_owned();
        if found.is_empty() || found == stored || found == head {
            continue;
        }
        if !git.is_ancestor(repo, stored, &found).await? {
            continue;
        }
        let newer = match &best {
            None => true,
            Some(b) => git.is_ancestor(repo, b, &found).await?,
        };
        if newer {
            best = Some(found);
        }
    }
    Ok(best)
}
