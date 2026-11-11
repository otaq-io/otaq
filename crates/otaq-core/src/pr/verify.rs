//! Pushed-content verification (REV-10 step 4, REV-06; d0043, d0050): after a push, the PR branch's **content** on
//! the remote must equal what the user reviewed. Trees are compared, not commit IDs, because the dialog rewords
//! commits. A difference shows the "changed since you reviewed it" warning with only the extra content.
//!
//! Before that, Otaq checks that the delivery's push landed at all: when the delivery agent reports the push or the
//! PR done, the remote must have the ticket's branch at the commit the local branch is at. The agent still pushes;
//! Otaq doesn't take its report alone (a slow pre-push hook once made an agent report a push wrongly).

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::board::tickets::Ticket;
use crate::board::{Board, BoardError};
use crate::git::diff::DiffSpec;
use crate::store::now_ms;

/// What was pushed beyond the reviewed content.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PushedDiff {
    /// The reviewed head.
    pub reviewed: String,
    /// The remote branch's tip.
    pub pushed: String,
    /// The files whose content differs.
    pub files: Vec<String>,
}

impl PushedDiff {
    /// "SHOP-18 changed since you reviewed it · the pushed content differs in 2 files".
    pub fn message(&self, key: &str) -> String {
        let n = self.files.len();
        let files = if n == 1 { "1 file" } else { "files" };
        if n == 1 {
            format!(
                "{key} changed since you reviewed it \u{b7} the pushed content differs in {files}"
            )
        } else {
            format!(
                "{key} changed since you reviewed it \u{b7} the pushed content differs in {n} {files}"
            )
        }
    }
}

/// The ticket's branch isn't on the remote at the commit it should be, after the delivery agent reported its push.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PushNotLanded {
    pub branch: String,
    /// The local branch's tip: what the push should have put on the remote.
    pub expected: String,
    /// The remote branch's tip; `None` when the remote has no such branch.
    pub remote: Option<String>,
}

impl PushNotLanded {
    /// "The branch isn't on the remote as expected · the remote has no branch `feature/SHOP-1-login`", or "· the
    /// remote has `feature/SHOP-1-login` at 1a2b3c4, not 5d6e7f8".
    pub fn message(&self) -> String {
        let branch = &self.branch;
        let has = match &self.remote {
            None => format!("the remote has no branch `{branch}`"),
            Some(remote) => format!(
                "the remote has `{branch}` at {}, not {}",
                short(remote),
                short(&self.expected)
            ),
        };
        format!("The branch isn\u{2019}t on the remote as expected \u{b7} {has}")
    }
}

fn short(sha: &str) -> &str {
    sha.get(..7).unwrap_or(sha)
}

impl Board {
    /// After the delivery agent reported its push or PR done: whether `origin` has the ticket's branch at the local
    /// branch's commit. `None` when it does, or when nothing can be checked (no branch, or the remote can't be read:
    /// logged, since a wrong report can't be told from a network that's down).
    pub async fn check_delivery_push(
        &self,
        ticket: &Ticket,
    ) -> Result<Option<PushNotLanded>, BoardError> {
        let row = self
            .store()
            .ticket(&ticket.id)
            .await?
            .ok_or_else(|| BoardError::NotFound(ticket.key.to_string()))?;
        let Some(branch) = row.branch_name else {
            return Ok(None);
        };
        let repo = PathBuf::from(self.project(ticket.key.project.as_str()).await?.repo_path);
        let git = self.git()?;
        let reference = format!("refs/heads/{branch}");
        let Some(expected) = git.resolve(&repo, &reference).await? else {
            return Ok(None);
        };
        let listed = match git.run(&repo, &["ls-remote", "origin", &reference]).await {
            Ok(listed) => listed,
            Err(e) => {
                tracing::warn!(key = %ticket.key, error = %e, "can't read the remote to check the delivery's push");
                return Ok(None);
            }
        };
        let remote = listed.lines().find_map(|line| {
            let (sha, name) = line.split_once('\t')?;
            (name.trim() == reference).then(|| sha.trim().to_owned())
        });
        if remote.as_deref() == Some(expected.as_str()) {
            return Ok(None);
        }
        Ok(Some(PushNotLanded {
            branch,
            expected,
            remote,
        }))
    }

    /// Fetches the PR branch and compares its tree with the reviewed head's; stores the difference (or clears
    /// it). `None` when they match or nothing can be compared (no reviewed head, or the branch isn't on origin, which
    /// is [`Self::check_delivery_push`]'s to say).
    pub async fn verify_pushed(&self, ticket: &Ticket) -> Result<Option<PushedDiff>, BoardError> {
        let row = self
            .store()
            .ticket(&ticket.id)
            .await?
            .ok_or_else(|| BoardError::NotFound(ticket.key.to_string()))?;
        let (Some(branch), Some(reviewed)) = (row.branch_name, row.review_head_sha) else {
            return Ok(None);
        };
        let repo = PathBuf::from(self.project(ticket.key.project.as_str()).await?.repo_path);
        let git = self.git()?;
        let tracking = format!("refs/remotes/origin/{branch}");
        let refspec = format!("+refs/heads/{branch}:{tracking}");
        if let Err(e) = git
            .run(&repo, &["fetch", "--quiet", "origin", &refspec])
            .await
        {
            tracing::warn!(key = %ticket.key, error = %e, "can't fetch the pushed branch to compare it");
            return Ok(None);
        }
        let Some(pushed) = git.resolve(&repo, &tracking).await? else {
            return Ok(None);
        };
        let differs = if git.tree_of(&repo, &pushed).await? == git.tree_of(&repo, &reviewed).await?
        {
            None
        } else {
            let files = git
                .changed_files(&repo, &DiffSpec::between(&reviewed, &pushed))
                .await?
                .into_iter()
                .map(|f| f.path)
                .collect();
            Some(PushedDiff {
                reviewed,
                pushed,
                files,
            })
        };
        sqlx::query("UPDATE tickets SET pushed_differs = ?, updated_at = ? WHERE id = ?")
            .bind(
                differs
                    .as_ref()
                    .map(|d| serde_json::to_string(d).expect("diff serializes")),
            )
            .bind(now_ms())
            .bind(&ticket.id)
            .execute(self.store().pool())
            .await?;
        Ok(differs)
    }
}
