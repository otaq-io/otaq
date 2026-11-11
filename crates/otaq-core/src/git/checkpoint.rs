//! The checkpoint commit around Otaq's own rebases (GIT-02; d0005, d0038).
//!
//! Before Otaq rebases a branch in an agent's worktree (Local merge's free rebase, a reopened ticket's
//! rebase), the agent's uncommitted changes are kept in a **checkpoint commit**, `wip: paused by Otaq`; after
//! the rebase `git reset --soft HEAD~1` takes it out again, and the index is put back as it was, so staged,
//! unstaged and untracked files all come back exactly as they were (rebased). This is the only commit Otaq
//! makes by itself (GIT-08) and it is always undone. **Never a stash**: rebases run with `--no-autostash`, so not
//! even the user's `rebase.autoStash` setting makes one.
//!
//! The user's git config and hooks apply (P4): a hook that rejects the checkpoint stops the operation with an
//! error ([`RebaseError::CheckpointRejected`], naming the hook when one refused); nothing passes `--no-verify`. A conflict, in the rebase or in
//! putting the files back, leaves the branch, the index and the files exactly as they were before.

use std::fmt;
use std::path::Path;

use super::{Git, GitError, GitStep, HookRefusal};
use crate::naming;

/// The checkpoint commit's message: `wip: paused by Otaq`.
pub fn checkpoint_message() -> String {
    format!("wip: paused by {}", naming::PRODUCT_NAME)
}

/// How [`Git::rebase_with_checkpoint`] ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RebaseOutcome {
    /// Rebased; any uncommitted files are back as they were.
    Clean,
    /// The rebase conflicted on these files; it was aborted and everything is as before.
    RebaseConflict { files: Vec<String> },
    /// The rebase worked but the staged changes didn't fit on the new base; everything was put back as before.
    UndoConflict { files: Vec<String> },
}

#[derive(Debug)]
pub enum RebaseError {
    /// The user's hooks (or config) refused the checkpoint commit; the files are as they were. `message` is
    /// git's output; `refusal` says which hook refused, with its whole output, when one did.
    CheckpointRejected {
        message: String,
        refusal: Option<Box<HookRefusal>>,
    },
    Git(GitError),
}

impl fmt::Display for RebaseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CheckpointRejected { message, .. } => {
                write!(f, "the checkpoint commit was rejected: {message}")
            }
            Self::Git(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for RebaseError {}

impl From<GitError> for RebaseError {
    fn from(e: GitError) -> Self {
        Self::Git(e)
    }
}

/// The message for a rejected checkpoint (SDD GIT-02).
pub fn checkpoint_rejected_message(key: &str) -> String {
    format!(
        "Your pre-commit hook rejected {}'s temporary checkpoint. Commit or discard the files in {key}, then retry.",
        naming::PRODUCT_NAME
    )
}

/// What the worktree looked like before the checkpoint.
struct Saved {
    /// HEAD before anything happened.
    head: String,
    /// The index as a tree (`git write-tree`), to put staged changes back.
    index: String,
    /// The checkpoint commit.
    checkpoint: String,
}

impl Git {
    /// Rebases the branch checked out in `worktree` onto `onto`, keeping uncommitted files with a checkpoint
    /// commit. A clean worktree skips the checkpoint.
    pub async fn rebase_with_checkpoint(
        &self,
        worktree: &Path,
        onto: &str,
    ) -> Result<RebaseOutcome, RebaseError> {
        let saved = if self.uncommitted_files(worktree).await?.is_empty() {
            None
        } else {
            self.step(GitStep::SaveUncommitted);
            Some(self.checkpoint(worktree).await?)
        };

        self.step(GitStep::UpdateBranch);
        let rebase = self
            .run(worktree, &["rebase", "--no-autostash", "--quiet", onto])
            .await;
        if rebase.is_err() {
            let files = self.conflicted_files(worktree).await?;
            // Abort if git left a rebase in progress (it may also have refused to start).
            if self.rebase_in_progress(worktree).await? {
                self.run(worktree, &["rebase", "--abort"]).await?;
            }
            if let Some(saved) = &saved {
                self.step(GitStep::RestoreUncommitted);
                self.restore(worktree, saved).await?;
            }
            if files.is_empty() {
                // Not a conflict: report git's own error.
                return Err(RebaseError::Git(rebase.expect_err("failed")));
            }
            return Ok(RebaseOutcome::RebaseConflict { files });
        }

        let Some(saved) = saved else {
            return Ok(RebaseOutcome::Clean);
        };
        self.step(GitStep::RestoreUncommitted);
        // Take the checkpoint out again: its changes stay in the files.
        self.run(worktree, &["reset", "--quiet", "--soft", "HEAD~1"])
            .await?;
        // The index holds everything now; put back only what was staged, so untracked files are untracked again.
        let new_base = self.run(worktree, &["rev-parse", "HEAD"]).await?;
        self.run(worktree, &["reset", "--quiet", "--mixed", new_base.trim()])
            .await?;
        let staged = self
            .run(
                worktree,
                &["diff", "--binary", "--no-color", &saved.head, &saved.index],
            )
            .await?;
        if !staged.is_empty()
            && let Err(e) = self.apply_cached(worktree, &staged).await
        {
            let files = self
                .run(
                    worktree,
                    &["diff", "--name-only", &saved.head, &saved.index],
                )
                .await?
                .lines()
                .map(str::to_owned)
                .collect();
            tracing::warn!(error = %e, "staged changes don't fit the rebased branch; putting everything back");
            // Back to the checkpoint's files, then to the old HEAD and index.
            self.run(worktree, &["reset", "--quiet", "--hard", &saved.checkpoint])
                .await?;
            self.restore(worktree, &saved).await?;
            return Ok(RebaseOutcome::UndoConflict { files });
        }
        Ok(RebaseOutcome::Clean)
    }

    /// Commits every uncommitted file (untracked ones too) as the checkpoint.
    async fn checkpoint(&self, worktree: &Path) -> Result<Saved, RebaseError> {
        let head = self
            .run(worktree, &["rev-parse", "HEAD"])
            .await?
            .trim()
            .to_owned();
        let index = self.run(worktree, &["write-tree"]).await?.trim().to_owned();
        self.run(worktree, &["add", "-A"]).await?;
        if let Err(e) = self
            .run(
                worktree,
                &["commit", "--quiet", "-m", &checkpoint_message()],
            )
            .await
        {
            // Put the index back as it was; the files never changed.
            self.run(worktree, &["read-tree", &index]).await?;
            return Err(match e {
                GitError::Failed { stderr, .. } => RebaseError::CheckpointRejected {
                    message: stderr,
                    refusal: None,
                },
                GitError::HookRefused(refusal) => RebaseError::CheckpointRejected {
                    message: refusal.stderr.clone(),
                    refusal: Some(refusal),
                },
                e => RebaseError::Git(e),
            });
        }
        let checkpoint = self
            .run(worktree, &["rev-parse", "HEAD"])
            .await?
            .trim()
            .to_owned();
        Ok(Saved {
            head,
            index,
            checkpoint,
        })
    }

    /// From the checkpoint (HEAD, with its files in the worktree) back to the old HEAD and index.
    async fn restore(&self, worktree: &Path, saved: &Saved) -> Result<(), GitError> {
        self.run(worktree, &["reset", "--quiet", "--soft", &saved.head])
            .await?;
        self.run(worktree, &["read-tree", &saved.index]).await?;
        Ok(())
    }

    async fn apply_cached(&self, worktree: &Path, patch: &str) -> Result<(), GitError> {
        let file = tempfile::NamedTempFile::new().map_err(|e| GitError::Spawn {
            args: "apply --cached".into(),
            error: e,
        })?;
        std::fs::write(file.path(), patch).map_err(|e| GitError::Spawn {
            args: "apply --cached".into(),
            error: e,
        })?;
        let path = file.path().display().to_string();
        self.run(worktree, &["apply", "--cached", "--binary", &path])
            .await
            .map(|_| ())
    }

    async fn conflicted_files(&self, worktree: &Path) -> Result<Vec<String>, GitError> {
        Ok(self
            .run(worktree, &["diff", "--name-only", "--diff-filter=U"])
            .await?
            .lines()
            .map(str::to_owned)
            .collect())
    }

    async fn rebase_in_progress(&self, worktree: &Path) -> Result<bool, GitError> {
        for dir in ["rebase-merge", "rebase-apply"] {
            let path = self
                .run(worktree, &["rev-parse", "--git-path", dir])
                .await?;
            let path = Path::new(path.trim());
            let path = if path.is_absolute() {
                path.to_path_buf()
            } else {
                worktree.join(path)
            };
            if path.exists() {
                return Ok(true);
            }
        }
        Ok(false)
    }
}
