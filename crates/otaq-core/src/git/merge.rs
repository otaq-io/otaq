//! Local merge's git work (GIT-01, GIT-03, REV-08; d0015, d0038, d0059), in a **scratch worktree owned by
//! Otaq**, never the user's folder. In order:
//!
//! 1. refuse if the ticket's worktree has uncommitted files ("SHOP-19 has 2 uncommitted files");
//! 2. refuse if the target branch is checked out in a folder with uncommitted changes ("shop-web has uncommitted
//!    changes on main. Clean up and retry."), before anything moves, so that branch stays untouched; then bring
//!    a base branch up to date: fetch, fast-forward if only behind, refuse if diverged (the epic branch is local
//!    only and skips this). A fetch that fails goes on into the local target when the request allows it (d0121);
//!    otherwise it is the error;
//! 3. **free rebase** of the ticket's branch onto the target: in its worktree with the checkpoint rule, or in the
//!    scratch worktree when the branch has no worktree (an epic's branch); a conflict merges nothing;
//! 4. **squash-merge** with the confirmed message in the scratch worktree (the user's hooks run);
//! 5. the project's **Merge check** (d0120), when it has one and the rebase brought in commits the branch didn't
//!    have (or the caller asks again, [`MergeRequest::recheck`]): run on the squash commit in the scratch worktree,
//!    then the scratch worktree is cleaned. A failure or the user's Stop merges nothing;
//! 6. move the target: fast-forward its clean checkout, or update the ref when it isn't checked out anywhere.
//!
//! **Never pushes, never stashes, never commits in the user's folder.**

use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::checkpoint::RebaseOutcome;
use super::{BaseSync, FetchFailure, Git, GitError};

/// One Local merge.
#[derive(Debug, Clone)]
pub struct MergeRequest {
    pub repo: PathBuf,
    /// The project's name for messages: "shop-web has uncommitted changes on main".
    pub project_name: String,
    /// The ticket's key for messages: "SHOP-19 has 2 uncommitted files".
    pub key: String,
    /// The branch to merge: `otaq/SHOP-18`.
    pub branch: String,
    /// Where the branch is checked out, if anywhere (the ticket's worktree).
    pub worktree: Option<PathBuf>,
    /// The branch to merge into: `main`, or the epic's `otaq/SHOP-40`.
    pub target: String,
    /// A base branch is fetched and fast-forwarded first; the epic branch is local only.
    pub sync_target: bool,
    /// A fetch that fails goes on into the local target as it is (a flow that doesn't deliver by PR, d0121).
    pub goes_on_without_remote: bool,
    /// Otaq's scratch worktree for this repository.
    pub scratch: PathBuf,
    /// The confirmed squash message.
    pub message: String,
    /// The project's Merge check, if it has one (d0120).
    pub check: Option<MergeCheck>,
    /// Run the check even when the rebase brings in nothing new: an earlier merge of this branch rebased it onto
    /// new commits and ended before a check passed on them (failed, stopped, refused, or the target didn't move).
    pub recheck: bool,
}

/// What a Merge check's command did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CheckRun {
    Passed,
    /// A non-zero exit (`None`: a signal ended it); `output` is the file holding what it printed.
    Failed {
        exit: Option<i32>,
        output: PathBuf,
    },
    /// The user stopped it (Q475).
    Stopped,
}

/// Runs the project's Merge check in the scratch worktree it is given, at the squash commit: the caller seeds the
/// build, sets the environment and saves the output ([`crate::review`]).
#[derive(Clone)]
pub struct MergeCheck(
    Arc<
        dyn Fn(PathBuf) -> crate::agent_tools::BoxFuture<'static, Result<CheckRun, GitError>>
            + Send
            + Sync,
    >,
);

impl MergeCheck {
    pub fn new(
        run: impl Fn(PathBuf) -> crate::agent_tools::BoxFuture<'static, Result<CheckRun, GitError>>
        + Send
        + Sync
        + 'static,
    ) -> Self {
        Self(Arc::new(run))
    }
}

impl fmt::Debug for MergeCheck {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("MergeCheck")
    }
}

/// What a Local merge did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MergeOutcome {
    /// Merged as `commit` on top of `onto` (the target's commit the branch was rebased onto). `brought_in`: the
    /// rebase brought in commits the branch didn't have; `checked`: the Merge check ran and passed.
    Merged {
        commit: String,
        onto: String,
        brought_in: bool,
        checked: bool,
        /// The fetch before it failed and it merged into the local target as it was (d0121).
        fetch_failed: Option<FetchFailure>,
    },
    RefusedUncommitted {
        files: Vec<String>,
        message: String,
    },
    RefusedDiverged {
        message: String,
    },
    RefusedDirtyCheckout {
        message: String,
    },
    /// The free rebase conflicted on these files: nothing was merged.
    Conflict {
        files: Vec<String>,
    },
    /// The Merge check failed on the merged result: nothing was merged (d0120). `output` holds what it printed.
    CheckFailed {
        exit: Option<i32>,
        output: PathBuf,
    },
    /// The user stopped the Merge check: nothing was merged (Q475).
    CheckStopped,
}

impl Git {
    /// Runs a Local merge (steps 1–4 above).
    pub async fn local_merge(&self, m: &MergeRequest) -> Result<MergeOutcome, GitError> {
        // 1. Uncommitted files in the ticket's worktree.
        if let Some(worktree) = m.worktree.as_deref().filter(|w| w.is_dir()) {
            let files = self.uncommitted_files(worktree).await?;
            if !files.is_empty() {
                return Ok(MergeOutcome::RefusedUncommitted {
                    message: uncommitted_files_message(&m.key, files.len()),
                    files,
                });
            }
        }

        // 2. The target: its checkout must be clean, and a base branch is brought up to date.
        let target_ref = format!("refs/heads/{}", m.target);
        let checkout = self
            .checkout_of(&m.repo, &target_ref)
            .await?
            .filter(|folder| !same_folder(folder, &m.scratch));
        if let Some(folder) = &checkout
            && self.has_uncommitted_changes(folder).await?
        {
            return Ok(MergeOutcome::RefusedDirtyCheckout {
                message: dirty_checkout_message(&m.project_name, &m.target),
            });
        }
        let mut fetch_failed = None;
        if m.sync_target {
            match self.sync_base(&m.repo, &m.target).await? {
                BaseSync::FetchFailed(failure) if m.goes_on_without_remote => {
                    fetch_failed = Some(failure);
                }
                BaseSync::FetchFailed(failure) => return Err(failure.error()),
                BaseSync::Diverged { message } => {
                    return Ok(MergeOutcome::RefusedDiverged { message });
                }
                BaseSync::CheckoutDirty { .. } => {
                    return Ok(MergeOutcome::RefusedDirtyCheckout {
                        message: dirty_checkout_message(&m.project_name, &m.target),
                    });
                }
                BaseSync::OnlyOnRemote => {
                    // The base exists only as the remote's: make the local branch from it first.
                    self.run(
                        &m.repo,
                        &[
                            "branch",
                            "--quiet",
                            &m.target,
                            &format!("origin/{}", m.target),
                        ],
                    )
                    .await?;
                }
                BaseSync::UpToDate | BaseSync::FastForwarded | BaseSync::NoRemote => {}
            }
        }
        let onto = self
            .resolve(&m.repo, &target_ref)
            .await?
            .ok_or_else(|| GitError::Failed {
                args: format!("rev-parse {target_ref}"),
                code: None,
                stderr: format!("the branch `{}` doesn't exist", m.target),
            })?;

        // 3. The free rebase onto the target. It brings in new commits unless the branch already has `onto`.
        let branch_ref = format!("refs/heads/{}", m.branch);
        let brought_in = match self.resolve(&m.repo, &branch_ref).await? {
            Some(head) => !self.is_ancestor(&m.repo, &onto, &head).await?,
            None => true,
        };
        self.ensure_scratch(&m.repo, &m.scratch, &onto).await?;
        if let Some(files) = self.rebase_branch(m, &onto).await? {
            return Ok(MergeOutcome::Conflict { files });
        }

        // 4. Squash-merge in the scratch worktree, then move the target.
        let scratch = m.scratch.as_path();
        self.run(
            scratch,
            &["checkout", "--quiet", "--force", "--detach", &onto],
        )
        .await?;
        self.run(scratch, &["clean", "--quiet", "-fdx"]).await?;
        self.run(scratch, &["merge", "--quiet", "--squash", &m.branch])
            .await?;
        let message_file = tempfile::NamedTempFile::new().map_err(|e| GitError::Spawn {
            args: "commit -F".into(),
            error: e,
        })?;
        std::fs::write(message_file.path(), &m.message).map_err(|e| GitError::Spawn {
            args: "commit -F".into(),
            error: e,
        })?;
        let message_path = message_file.path().display().to_string();
        if let Err(e) = self
            .run(
                scratch,
                &["commit", "--quiet", "--allow-empty", "-F", &message_path],
            )
            .await
        {
            // A hook refused the squash commit: leave the scratch worktree clean; the target never moved.
            let _ = self
                .run(scratch, &["reset", "--quiet", "--hard", &onto])
                .await;
            return Err(e);
        }
        let commit = self
            .run(scratch, &["rev-parse", "HEAD"])
            .await?
            .trim()
            .to_owned();

        // 5. The Merge check on the merged result, before anything moves (d0120).
        let checked = match &m.check {
            Some(check) if brought_in || m.recheck => {
                let ran = (check.0)(m.scratch.clone()).await;
                // Cleaned whatever happened: the seeded build and anything the command wrote go.
                let at = match &ran {
                    Ok(CheckRun::Passed) => commit.as_str(),
                    _ => onto.as_str(),
                };
                let _ = self.run(scratch, &["reset", "--quiet", "--hard", at]).await;
                let _ = self.run(scratch, &["clean", "--quiet", "-fdx"]).await;
                match ran? {
                    CheckRun::Passed => true,
                    CheckRun::Failed { exit, output } => {
                        return Ok(MergeOutcome::CheckFailed { exit, output });
                    }
                    CheckRun::Stopped => return Ok(MergeOutcome::CheckStopped),
                }
            }
            _ => false,
        };

        // 6. Move the target.
        match &checkout {
            Some(folder) => {
                if self
                    .run(folder, &["merge", "--quiet", "--ff-only", &commit])
                    .await
                    .is_err()
                {
                    // Something in the folder (an untracked file in the way) stops the fast-forward.
                    return Ok(MergeOutcome::RefusedDirtyCheckout {
                        message: dirty_checkout_message(&m.project_name, &m.target),
                    });
                }
            }
            None => {
                self.run(&m.repo, &["update-ref", &target_ref, &commit, &onto])
                    .await?;
            }
        }
        Ok(MergeOutcome::Merged {
            commit,
            onto,
            brought_in,
            checked,
            fetch_failed,
        })
    }

    /// Whether `ancestor` is an ancestor of (or is) `of`.
    async fn is_ancestor(&self, repo: &Path, ancestor: &str, of: &str) -> Result<bool, GitError> {
        let args = ["merge-base", "--is-ancestor", ancestor, of];
        match self.run_with_code(repo, &args).await? {
            (0, _) => Ok(true),
            (1, _) => Ok(false),
            (code, _) => Err(GitError::Failed {
                args: args.join(" "),
                code: Some(code),
                stderr: format!("git merge-base --is-ancestor {ancestor} {of} failed"),
            }),
        }
    }

    /// Rebases the branch onto `onto`; `Some(files)` on a conflict (everything is as before then).
    async fn rebase_branch(
        &self,
        m: &MergeRequest,
        onto: &str,
    ) -> Result<Option<Vec<String>>, GitError> {
        if let Some(worktree) = m.worktree.as_deref().filter(|w| w.is_dir()) {
            return match self.rebase_with_checkpoint(worktree, onto).await {
                Ok(RebaseOutcome::Clean) => Ok(None),
                Ok(
                    RebaseOutcome::RebaseConflict { files } | RebaseOutcome::UndoConflict { files },
                ) => Ok(Some(files)),
                Err(super::checkpoint::RebaseError::Git(e)) => Err(e),
                // The worktree was checked clean in step 1, so no checkpoint was made.
                Err(e) => Err(GitError::Failed {
                    args: "rebase".into(),
                    code: None,
                    stderr: e.to_string(),
                }),
            };
        }
        // No worktree holds the branch: rebase it in the scratch worktree, then let go of it.
        let scratch = m.scratch.as_path();
        self.run(scratch, &["checkout", "--quiet", "--force", &m.branch])
            .await?;
        let rebased = self
            .run(scratch, &["rebase", "--no-autostash", "--quiet", onto])
            .await;
        let conflict = if rebased.is_err() {
            let files: Vec<String> = self
                .run(scratch, &["diff", "--name-only", "--diff-filter=U"])
                .await?
                .lines()
                .map(str::to_owned)
                .collect();
            let _ = self.run(scratch, &["rebase", "--abort"]).await;
            if files.is_empty() {
                self.run(scratch, &["checkout", "--quiet", "--detach"])
                    .await?;
                return Err(rebased.expect_err("failed"));
            }
            Some(files)
        } else {
            None
        };
        self.run(scratch, &["checkout", "--quiet", "--detach"])
            .await?;
        Ok(conflict)
    }

    /// Creates the scratch worktree (detached at `at`) or reuses it.
    async fn ensure_scratch(&self, repo: &Path, scratch: &Path, at: &str) -> Result<(), GitError> {
        let list = self.run(repo, &["worktree", "list", "--porcelain"]).await?;
        let registered = list
            .lines()
            .filter_map(|l| l.strip_prefix("worktree "))
            .any(|p| same_folder(Path::new(p), scratch));
        if registered && scratch.is_dir() {
            return Ok(());
        }
        if let Some(parent) = scratch.parent() {
            crate::data_dir::DataDir::ensure_dir(parent).map_err(|e| GitError::Spawn {
                args: "worktree add".into(),
                error: e,
            })?;
        }
        let path = scratch.display().to_string();
        let mut args = vec!["worktree", "add", "--quiet", "--detach"];
        if registered {
            args.push("--force");
        }
        args.extend([path.as_str(), at]);
        self.run(repo, &args).await.map(|_| ())
    }
}

impl Git {
    /// The early conflict check (GIT-04, d0038): would `branch` merge cleanly into `target`? A **read-only** test
    /// merge with `git merge-tree --write-tree` (git 2.38 or newer): it writes only unreferenced objects, and
    /// changes no worktree, index, branch or ref. `None` when clean, else the conflicting files.
    pub async fn test_merge(
        &self,
        repo: &Path,
        target: &str,
        branch: &str,
    ) -> Result<Option<Vec<String>>, GitError> {
        let args = [
            "merge-tree",
            "--write-tree",
            "--name-only",
            "--no-messages",
            target,
            branch,
        ];
        match self.run_with_code(repo, &args).await? {
            (0, _) => Ok(None),
            // 1: conflicts. The output is the merged tree's id, then one conflicted path per line.
            (1, out) => {
                let mut files: Vec<String> = out
                    .lines()
                    .skip(1)
                    .take_while(|l| !l.is_empty())
                    .map(str::to_owned)
                    .collect();
                files.dedup();
                Ok(Some(files))
            }
            (code, _) => Err(GitError::Failed {
                args: args.join(" "),
                code: Some(code),
                stderr: "git merge-tree failed (it needs git 2.38 or newer)".into(),
            }),
        }
    }
}

/// "SHOP-19 has 2 uncommitted files" (REV-08).
pub fn uncommitted_files_message(key: &str, count: usize) -> String {
    if count == 1 {
        format!("{key} has 1 uncommitted file")
    } else {
        format!("{key} has {count} uncommitted files")
    }
}

/// "shop-web has uncommitted changes on main. Clean up and retry." (GIT-03).
pub fn dirty_checkout_message(project: &str, branch: &str) -> String {
    format!("{project} has uncommitted changes on {branch}. Clean up and retry.")
}

fn same_folder(a: &Path, b: &Path) -> bool {
    let canonical = |p: &Path| crate::paths::canonical(p).unwrap_or_else(|_| p.to_path_buf());
    canonical(a) == canonical(b)
}
