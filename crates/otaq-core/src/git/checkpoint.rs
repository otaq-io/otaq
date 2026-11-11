//! Bringing a branch up to date in an agent's worktree, and the checkpoint commit around it (GIT-02; d0005, d0038).
//!
//! Otaq brings a branch up to date by **merging** the newer branch into it, never by rebasing, so a team's
//! pre-rebase hook or rule against rebasing never stops it ([`Git::merge_in`]). The merge commit is a real commit on
//! the branch: the project's hooks run on it (prepare-commit-msg, commit-msg, pre-merge-commit) and its message is
//! git's own plain merge message. A conflict or a refusal merges nothing and leaves the branch, the index and the
//! files exactly as they were.
//!
//! When the worktree has uncommitted changes (only a reopened ticket's start brings such a worktree up to date: every
//! other update refuses uncommitted files first), they are kept in a **checkpoint commit**, `wip: paused by Otaq`,
//! for a moment: the branch stays at its head, main is merged in, and the checkpoint's changes are put back on top of
//! the merge as uncommitted files, with the index as it was, so staged, unstaged and untracked files all come back
//! exactly as they were. The checkpoint is never on the branch, main or a remote. **Never a stash**: the merge runs
//! with `--no-autostash`, so not even the user's `merge.autoStash` setting makes one.
//!
//! **The checkpoint is the one place Otaq doesn't run the project's hooks**, by the user's decision: nothing of it
//! lands and it is undone right after, so a commit-msg rule or a lint pre-commit has nothing to protect there, and
//! judging half-done work or the save's own message would only stop a reopened ticket from starting. It is made with
//! plumbing (`write-tree` then `commit-tree`), which runs no hook at all, rather than `commit --no-verify`, which
//! still runs prepare-commit-msg (it can refuse) and post-commit (a team's hook may push or notify). `commit-tree`
//! also leaves HEAD and the branch where they are. Putting the files back (`cherry-pick --no-commit`, `reset`,
//! `read-tree`, `apply --cached`) makes no commit and runs no hook either.
//!
//! **Every commit that lands still runs the hooks**: the merge commit that brings the branch up to date runs
//! prepare-commit-msg, commit-msg and pre-merge-commit, and a hook that refuses it is the error
//! ([`GitError::HookRefused`]), with the merge aborted and the files back as they were. Nothing here passes
//! `--no-verify`. The rest of the user's git config applies to the checkpoint too (its identity, commit signing).

use std::fmt;
use std::path::Path;

use super::{Git, GitError, GitStep};
use crate::naming;

/// The checkpoint commit's message: `wip: paused by Otaq`.
pub fn checkpoint_message() -> String {
    format!("wip: paused by {}", naming::PRODUCT_NAME)
}

/// How [`Git::merge_with_checkpoint`] ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpdateOutcome {
    /// Up to date; any uncommitted files are back as they were.
    Clean,
    /// The merge conflicted on these files; it was aborted and everything is as before.
    Conflict { files: Vec<String> },
    /// The merge worked but the uncommitted changes didn't fit on top of it; everything was put back as before.
    UndoConflict { files: Vec<String> },
}

/// Why [`Git::merge_with_checkpoint`] stopped. The checkpoint runs no hook, so a hook can only refuse the merge
/// commit ([`GitError::HookRefused`]); the checkpoint itself fails only when git can't write it (for example a
/// broken index, no identity, or commit signing that fails), with git's own error.
#[derive(Debug)]
pub enum UpdateError {
    Git(GitError),
}

impl fmt::Display for UpdateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Git(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for UpdateError {}

impl From<GitError> for UpdateError {
    fn from(e: GitError) -> Self {
        Self::Git(e)
    }
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
    /// Merges `onto` (a branch name, so the message is git's plain "Merge branch 'main' into …", or a commit) into
    /// the branch checked out in the **clean** `worktree`. `Ok(None)`: merged (or already up to date, or
    /// fast-forwarded); `Ok(Some(files))`: it conflicted on these files and was aborted. Any other failure (a hook
    /// refused the merge commit) is the error, with the merge aborted too: either way the branch, the index and the
    /// files are as they were.
    pub async fn merge_in(
        &self,
        worktree: &Path,
        onto: &str,
    ) -> Result<Option<Vec<String>>, GitError> {
        self.step(GitStep::UpdateBranch);
        let merged = self
            .run(
                worktree,
                &[
                    "merge",
                    "--quiet",
                    "--no-edit",
                    "--no-autostash",
                    "--ff",
                    "--no-squash",
                    onto,
                ],
            )
            .await;
        let Err(e) = merged else {
            return Ok(None);
        };
        let files = self.conflicted_files(worktree).await?;
        // Abort if git left a merge in progress: a conflict, or a hook that refused the merge commit. It may also have
        // refused to start.
        if self.merge_in_progress(worktree).await? {
            self.run(worktree, &["merge", "--abort"]).await?;
        }
        if files.is_empty() {
            return Err(e);
        }
        Ok(Some(files))
    }

    /// Brings the branch checked out in `worktree` up to date by merging `onto` into it ([`Git::merge_in`]),
    /// keeping uncommitted files with a checkpoint commit. A clean worktree skips the checkpoint.
    pub async fn merge_with_checkpoint(
        &self,
        worktree: &Path,
        onto: &str,
    ) -> Result<UpdateOutcome, UpdateError> {
        if self.uncommitted_files(worktree).await?.is_empty() {
            return Ok(match self.merge_in(worktree, onto).await? {
                None => UpdateOutcome::Clean,
                Some(files) => UpdateOutcome::Conflict { files },
            });
        }
        self.step(GitStep::SaveUncommitted);
        let saved = self.checkpoint(worktree).await?;
        // The worktree goes back to the branch's head, with the files of that head: the checkpoint holds the changes.
        self.run(worktree, &["reset", "--quiet", "--hard", &saved.head])
            .await?;
        let merged = match self.merge_in(worktree, onto).await {
            Ok(merged) => merged,
            Err(e) => {
                self.step(GitStep::RestoreUncommitted);
                self.back_to_checkpoint(worktree, &saved).await?;
                return Err(UpdateError::Git(e));
            }
        };
        if let Some(files) = merged {
            self.step(GitStep::RestoreUncommitted);
            self.back_to_checkpoint(worktree, &saved).await?;
            return Ok(UpdateOutcome::Conflict { files });
        }

        self.step(GitStep::RestoreUncommitted);
        // The changes go back on top of the merge, uncommitted; a conflict with what came in puts everything back.
        let merge = self
            .run(worktree, &["rev-parse", "HEAD"])
            .await?
            .trim()
            .to_owned();
        if self
            .run(worktree, &["cherry-pick", "--no-commit", &saved.checkpoint])
            .await
            .is_err()
        {
            let files = self.conflicted_files(worktree).await?;
            tracing::warn!(
                "uncommitted changes don't fit the updated branch; putting everything back"
            );
            self.back_to_checkpoint(worktree, &saved).await?;
            return Ok(UpdateOutcome::UndoConflict { files });
        }
        // The index holds everything now; put back only what was staged, so untracked files are untracked again.
        self.run(worktree, &["reset", "--quiet", "--mixed", &merge])
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
            tracing::warn!(error = %e, "staged changes don't fit the updated branch; putting everything back");
            self.back_to_checkpoint(worktree, &saved).await?;
            return Ok(UpdateOutcome::UndoConflict { files });
        }
        Ok(UpdateOutcome::Clean)
    }

    /// Saves every uncommitted file (untracked ones too) in the checkpoint commit, running **no hook**: the
    /// temporary save is the one place the project's hooks don't judge (see the module's comments). The commit is
    /// made with `write-tree` and `commit-tree`, so HEAD and the branch don't move; the index holds everything
    /// afterwards, and the caller's `reset --hard` puts the worktree back to the head.
    async fn checkpoint(&self, worktree: &Path) -> Result<Saved, GitError> {
        let head = self
            .run(worktree, &["rev-parse", "HEAD"])
            .await?
            .trim()
            .to_owned();
        let index = self.run(worktree, &["write-tree"]).await?.trim().to_owned();
        let checkpoint = match self.checkpoint_commit(worktree, &head).await {
            Ok(checkpoint) => checkpoint,
            Err(e) => {
                // Put the index back as it was; the files never changed.
                self.run(worktree, &["read-tree", &index]).await?;
                return Err(e);
            }
        };
        Ok(Saved {
            head,
            index,
            checkpoint,
        })
    }

    /// Stages everything and writes it as a commit on top of `head` with plumbing, which runs no hook.
    async fn checkpoint_commit(&self, worktree: &Path, head: &str) -> Result<String, GitError> {
        self.run(worktree, &["add", "-A"]).await?;
        let tree = self.run(worktree, &["write-tree"]).await?.trim().to_owned();
        Ok(self
            .run(
                worktree,
                &[
                    "commit-tree",
                    &tree,
                    "-p",
                    head,
                    "-m",
                    &checkpoint_message(),
                ],
            )
            .await?
            .trim()
            .to_owned())
    }

    /// Back to where the update started: the checkpoint's files in the worktree, then the old HEAD and index.
    async fn back_to_checkpoint(&self, worktree: &Path, saved: &Saved) -> Result<(), GitError> {
        self.run(worktree, &["reset", "--quiet", "--hard", &saved.checkpoint])
            .await?;
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

    async fn merge_in_progress(&self, worktree: &Path) -> Result<bool, GitError> {
        let path = self
            .run(worktree, &["rev-parse", "--git-path", "MERGE_HEAD"])
            .await?;
        let path = Path::new(path.trim());
        let path = if path.is_absolute() {
            path.to_path_buf()
        } else {
            worktree.join(path)
        };
        Ok(path.exists())
    }
}
