//! Keeping a base branch current (SES-01 step 2, GIT-01; d0059, d0038).
//!
//! Before a delivery unit starts, and later before a Local merge into a base branch (T-0089), Otaq runs
//! `git fetch`, fast-forwards the local base if it is **only behind** its remote, and **holds** if the two have
//! **diverged**. It never merges, rebases, pushes or stashes, and never touches a checkout with uncommitted
//! changes (P3). Epic branches are local only and skip this step (the caller doesn't call it for them).
//!
//! A fetch that fails (an unreachable remote, no key for it) is [`BaseSync::FetchFailed`], not an error: whether the
//! ticket goes on from its local base branch is the caller's call (d0121).
//!
//! Which remote: the branch's upstream if it has one, otherwise `origin` (or the only remote). "Dirty" means
//! uncommitted changes to tracked files; untracked files don't count, and `git merge --ff-only` refuses on its
//! own rather than overwrite one.

use std::path::{Path, PathBuf};

use super::{Git, GitError};

/// What [`Git::sync_base`] found and did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BaseSync {
    /// The local branch has everything its remote has (it may be ahead, for example after Local merges).
    UpToDate,
    /// The local branch was behind and was fast-forwarded (and its clean checkout with it).
    FastForwarded,
    /// The local branch is behind, but it is checked out in a folder with uncommitted changes, so it was left
    /// alone. Not a reason to hold: the ticket starts from the local branch as it is.
    CheckoutDirty { message: String },
    /// Local and remote both have commits the other lacks; nothing was merged or rebased. Hold in Queued.
    Diverged { message: String },
    /// The branch exists only as the remote's (`origin/develop`); there is no local branch to update.
    OnlyOnRemote,
    /// The repository has no remote, or the branch isn't on it: nothing to fetch or compare.
    NoRemote,
    /// `git fetch` itself failed: nothing was compared or moved (d0121).
    FetchFailed(FetchFailure),
}

/// A `git fetch` that failed: which remote, and how git failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FetchFailure {
    /// `origin`.
    pub remote: String,
    pub code: Option<i32>,
    /// "Host key verification failed."
    pub stderr: String,
}

impl FetchFailure {
    /// The failure as the error a fetch returned before d0121.
    pub fn error(&self) -> GitError {
        GitError::Failed {
            args: format!("fetch --quiet {}", self.remote),
            code: self.code,
            stderr: self.stderr.clone(),
        }
    }

    /// Git's reason: "git fetch --quiet origin failed (exit 128): Host key verification failed."
    pub fn reason(&self) -> String {
        self.error().to_string()
    }
}

impl Git {
    /// Fetches and fast-forwards `branch` of the repository at `repo` if it is only behind its remote.
    /// A fetch that ran and failed is [`BaseSync::FetchFailed`]; git that can't run at all is the error.
    pub async fn sync_base(&self, repo: &Path, branch: &str) -> Result<BaseSync, GitError> {
        let local_ref = format!("refs/heads/{branch}");
        let Some(remote) = self.remote_for(repo, &local_ref).await? else {
            return Ok(BaseSync::NoRemote);
        };
        match self.run(repo, &["fetch", "--quiet", &remote]).await {
            Ok(_) => {}
            Err(GitError::Failed { code, stderr, .. }) => {
                return Ok(BaseSync::FetchFailed(FetchFailure {
                    remote,
                    code,
                    stderr,
                }));
            }
            Err(e) => return Err(e),
        }

        let upstream = self
            .run(
                repo,
                &["for-each-ref", "--format=%(upstream)", local_ref.as_str()],
            )
            .await?
            .trim()
            .to_owned();
        let remote_ref = if upstream.is_empty() {
            format!("refs/remotes/{remote}/{branch}")
        } else {
            upstream
        };
        let Some(remote_sha) = self.resolve(repo, &remote_ref).await? else {
            return Ok(BaseSync::NoRemote);
        };
        let Some(local_sha) = self.resolve(repo, &local_ref).await? else {
            return Ok(BaseSync::OnlyOnRemote);
        };
        let remote_name = remote_ref
            .strip_prefix("refs/remotes/")
            .unwrap_or(&remote_ref)
            .to_owned();

        let range = format!("{local_sha}...{remote_sha}");
        let counts = self
            .run(repo, &["rev-list", "--left-right", "--count", &range])
            .await?;
        let mut numbers = counts.split_whitespace().map(|n| n.parse::<u64>());
        let (Some(Ok(ahead)), Some(Ok(behind))) = (numbers.next(), numbers.next()) else {
            return Err(GitError::Failed {
                args: format!("rev-list --left-right --count {range}"),
                code: None,
                stderr: format!("unexpected output {counts:?}"),
            });
        };
        match (ahead, behind) {
            (_, 0) => Ok(BaseSync::UpToDate),
            (0, _) => {
                self.fast_forward(repo, branch, &local_ref, &local_sha, &remote_sha)
                    .await
            }
            _ => Ok(BaseSync::Diverged {
                message: format!(
                    "`{branch}` and `{remote_name}` have diverged. Push or pull, then retry"
                ),
            }),
        }
    }

    /// The remote to fetch for `local_ref`: its upstream's remote, else `origin`, else the only remote.
    async fn remote_for(&self, repo: &Path, local_ref: &str) -> Result<Option<String>, GitError> {
        let configured = self
            .run(
                repo,
                &["for-each-ref", "--format=%(upstream:remotename)", local_ref],
            )
            .await?;
        let configured = configured.trim();
        if !configured.is_empty() && configured != "." {
            return Ok(Some(configured.to_owned()));
        }
        let remotes: Vec<String> = self
            .run(repo, &["remote"])
            .await?
            .lines()
            .map(str::to_owned)
            .collect();
        Ok(match remotes.as_slice() {
            [] => None,
            [only] => Some(only.clone()),
            many => many.iter().find(|r| *r == "origin").cloned(),
        })
    }

    /// The commit a ref points to, or `None` if it doesn't exist.
    pub async fn resolve(&self, repo: &Path, reference: &str) -> Result<Option<String>, GitError> {
        let commit = format!("{reference}^{{commit}}");
        match self
            .run(repo, &["rev-parse", "--quiet", "--verify", &commit])
            .await
        {
            Ok(out) => Ok(Some(out.trim().to_owned())),
            Err(GitError::Failed { code: Some(1), .. }) => Ok(None),
            Err(e) => Err(e),
        }
    }

    async fn fast_forward(
        &self,
        repo: &Path,
        branch: &str,
        local_ref: &str,
        local_sha: &str,
        remote_sha: &str,
    ) -> Result<BaseSync, GitError> {
        match self.checkout_of(repo, local_ref).await? {
            Some(folder) => {
                if self.has_uncommitted_changes(&folder).await? {
                    return Ok(BaseSync::CheckoutDirty {
                        message: format!(
                            "`{branch}` is behind its remote, but {} has uncommitted changes on it, so it wasn't updated",
                            folder.display()
                        ),
                    });
                }
                self.run(&folder, &["merge", "--ff-only", "--quiet", remote_sha])
                    .await?;
            }
            // Not checked out anywhere: move the ref, but only if nobody moved it meanwhile.
            None => {
                self.run(repo, &["update-ref", local_ref, remote_sha, local_sha])
                    .await?;
            }
        }
        Ok(BaseSync::FastForwarded)
    }

    /// The folder (main checkout or a worktree) where `local_ref` is checked out, if any.
    pub async fn checkout_of(
        &self,
        repo: &Path,
        local_ref: &str,
    ) -> Result<Option<PathBuf>, GitError> {
        let list = self.run(repo, &["worktree", "list", "--porcelain"]).await?;
        let mut folder = None;
        for line in list.lines() {
            if let Some(path) = line.strip_prefix("worktree ") {
                folder = Some(PathBuf::from(path));
            } else if line.strip_prefix("branch ") == Some(local_ref) {
                return Ok(folder);
            }
        }
        Ok(None)
    }

    /// Every file in `folder` that isn't committed: changed, staged, deleted or untracked (ignored files don't
    /// count), one entry per path as `git status --porcelain` lists them.
    pub async fn uncommitted_files(&self, folder: &Path) -> Result<Vec<String>, GitError> {
        let status = self
            .run(folder, &["status", "--porcelain", "--untracked-files=all"])
            .await?;
        Ok(status
            .lines()
            .filter(|l| l.len() > 3)
            .map(|l| l[3..].to_owned())
            .collect())
    }

    /// Whether the checkout at `folder` has uncommitted changes to tracked files (staged or not).
    pub async fn has_uncommitted_changes(&self, folder: &Path) -> Result<bool, GitError> {
        let status = self
            .run(folder, &["status", "--porcelain", "--untracked-files=no"])
            .await?;
        Ok(!status.trim().is_empty())
    }
}
