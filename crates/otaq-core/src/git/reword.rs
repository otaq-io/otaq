//! Rewording commits for the PR flow (REV-10, d0050): Otaq rewrites the commit messages the user confirmed,
//! with no tokens and no file changes. Each new commit keeps its tree and author; the committer is the user's git
//! identity (P4). Commits whose message and parent are unchanged keep their IDs, so already-pushed commits stay
//! as they are. Nothing here moves a ref: the caller does that once the whole chain is built and checked.

use std::path::Path;

use super::{Git, GitError, GitStep};

/// One commit of a range.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitInfo {
    pub sha: String,
    /// More than one parent: a merge commit, which the PR flow doesn't reword.
    pub merge: bool,
    /// The full message, trimmed.
    pub message: String,
}

impl CommitInfo {
    /// The message's first line.
    pub fn subject(&self) -> &str {
        self.message.lines().next().unwrap_or_default()
    }
}

impl Git {
    /// The commits after `from` up to `to`, oldest first (`from..to`, following first parents).
    pub async fn commits_between(
        &self,
        repo: &Path,
        from: &str,
        to: &str,
    ) -> Result<Vec<CommitInfo>, GitError> {
        let range = format!("{from}..{to}");
        let out = self
            .run(
                repo,
                &[
                    "log",
                    "--reverse",
                    "--first-parent",
                    "--format=%H%x00%P%x00%B%x1e",
                    &range,
                ],
            )
            .await?;
        Ok(out
            .split('\u{1e}')
            .filter_map(|record| {
                let mut fields = record.trim_start_matches('\n').splitn(3, '\0');
                let sha = fields.next()?.trim();
                if sha.is_empty() {
                    return None;
                }
                let parents = fields.next().unwrap_or_default();
                Some(CommitInfo {
                    sha: sha.to_owned(),
                    merge: parents.split_whitespace().count() > 1,
                    message: fields.next().unwrap_or_default().trim().to_owned(),
                })
            })
            .collect())
    }

    /// Builds the chain `parent` ← `commits[0]` ← … with the given messages and returns its tip. Trees and authors
    /// stay; a commit whose message and parent are unchanged keeps its ID. Fails on a merge commit.
    pub async fn reword(
        &self,
        repo: &Path,
        parent: &str,
        commits: &[(CommitInfo, String)],
    ) -> Result<String, GitError> {
        self.step(GitStep::Reword);
        let mut parent = parent.to_owned();
        let mut changed = false;
        for (commit, message) in commits {
            if commit.merge {
                return Err(GitError::Failed {
                    args: format!("reword {}", commit.sha),
                    code: None,
                    stderr: format!(
                        "{} is a merge commit; it can't be reworded",
                        short(&commit.sha)
                    ),
                });
            }
            let message = message.trim();
            if !changed && message == commit.message {
                parent = commit.sha.clone();
                continue;
            }
            changed = true;
            let tree_ref = format!("{}^{{tree}}", commit.sha);
            let tree = self.run(repo, &["rev-parse", &tree_ref]).await?;
            let author = self
                .run(
                    repo,
                    &["log", "-1", "--format=%an%x00%ae%x00%aI", &commit.sha],
                )
                .await?;
            let mut author = author.trim_end_matches('\n').splitn(3, '\0');
            let (name, email, date) = (
                author.next().unwrap_or_default().to_owned(),
                author.next().unwrap_or_default().to_owned(),
                author.next().unwrap_or_default().to_owned(),
            );
            let new = self
                .run_with_env(
                    repo,
                    &["commit-tree", tree.trim(), "-p", &parent, "-m", message],
                    &[
                        ("GIT_AUTHOR_NAME", &name),
                        ("GIT_AUTHOR_EMAIL", &email),
                        ("GIT_AUTHOR_DATE", &date),
                    ],
                )
                .await?;
            parent = new.trim().to_owned();
        }
        Ok(parent)
    }

    /// The tree of a commit.
    pub async fn tree_of(&self, repo: &Path, commit: &str) -> Result<String, GitError> {
        let tree_ref = format!("{commit}^{{tree}}");
        Ok(self
            .run(repo, &["rev-parse", &tree_ref])
            .await?
            .trim()
            .to_owned())
    }
}

fn short(sha: &str) -> &str {
    &sha[..sha.len().min(7)]
}
