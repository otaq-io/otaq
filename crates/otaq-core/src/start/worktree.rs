//! Worktrees and temporary branches (SES-01 steps 1 and 3, SES-02, GIT-07; d0010, d0023, d0029, d0050, d0059).
//!
//! Each ticket works in its own git worktree **outside the user's repo**, at `wt/<repo-name>/<KEY>` in the data
//! folder, on the temporary branch `otaq/<KEY>` (the real name is chosen at the first push, M10). The base
//! is the ticket's base branch.
//!
//! Everything converges (SES-02): the ticket's own branch or worktree that exists is reused, a worktree registered
//! with git whose folder is gone is added again, and the ticket row is filled in whatever happened before. A branch of
//! that name the ticket didn't make (a project added to a repository that already has `otaq/SHOP-1`) is never taken
//! over: the ticket's branch is the next free name, `otaq/SHOP-1-2` ([`free_branch`]), stored before it is made. Nothing is
//! written into the user's repository except git's own branch refs and worktree metadata under `.git/`.
//!
//! A new ticket's start that fails leaves nothing half-made: the worktree it added (a post-checkout hook may refuse
//! after git made it) is removed and unregistered, and the branch it created is deleted while it still points where
//! it was made ([`discard`]), so Retry start does the whole start again and the start's hooks run again. A reopened
//! ticket's worktree holds earlier work and is never removed; nor is a worktree or branch an earlier start made.

use std::path::{Path, PathBuf};

use super::StartError;
use crate::board::Board;
use crate::data_dir::DataDir;
use crate::git::{Git, GitError, GitStep};
use crate::naming;
use crate::store::now_ms;

/// Where a ticket works.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Worktree {
    pub path: PathBuf,
    /// `otaq/SHOP-18`.
    pub branch: String,
    /// The branch it started from: `main`.
    pub base_branch: String,
    /// The commit where the ticket left its base.
    pub base_sha: String,
    /// What this start made for a new ticket, removed again if the start fails. Nothing for a reopened ticket.
    pub made: Made,
}

/// What one start of a new ticket made, and only that: what [`discard`] removes when the start fails.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Made {
    /// This start's `worktree add` made the worktree.
    pub worktree: bool,
    /// This start created the branch, at this commit.
    pub branch_at: Option<String>,
}

impl Made {
    pub fn is_empty(&self) -> bool {
        !self.worktree && self.branch_at.is_none()
    }
}

/// Resolves the base, creates the branch and the worktree (or reuses them), and stores `branch_name`,
/// `base_sha` and `worktree_path` on the ticket, locking its base (d0051).
pub async fn prepare_worktree(
    board: &Board,
    data: &DataDir,
    key: &str,
) -> Result<Worktree, StartError> {
    let ticket = board.ticket(key).await?;
    let project = board.project(ticket.key.project.as_str()).await?;
    // With the project's variables: `worktree add` runs the user's hooks (ENV-02).
    let git = &board.project_git(&project.key).await?;
    let repo = PathBuf::from(&project.repo_path);

    // Step 1: the base.
    let base_branch = ticket.base.branch().to_owned();
    let base_commit = branch_commit(git, &repo, &base_branch).await?;

    // Step 3: the ticket's branch: the one an earlier start of it chose, else a free name, stored before it is made so
    // that a start cut short after making it finds it again as its own.
    let stored = board.store().ticket(&ticket.id).await?;
    let stored_branch = stored.as_ref().and_then(|row| row.branch_name.clone());
    // A reopened ticket's branch and worktree hold earlier work: a failed start never removes them.
    let reopened = stored.as_ref().is_some_and(|row| row.reopened);
    let stored_base_sha = stored.and_then(|row| row.base_sha);
    let branch = match stored_branch {
        Some(branch) => branch,
        None => {
            let branch = free_branch(git, &repo, &ticket.key.to_string()).await?;
            sqlx::query("UPDATE tickets SET branch_name = ?, updated_at = ? WHERE id = ?")
                .bind(&branch)
                .bind(now_ms())
                .bind(&ticket.id)
                .execute(board.store().pool())
                .await
                .map_err(crate::store::StoreError::from)?;
            branch
        }
    };
    let branch_ref = format!("refs/heads/{branch}");
    let mut made = Made::default();
    let base_sha = if git.resolve(&repo, &branch_ref).await?.is_some() {
        match stored_base_sha {
            Some(sha) => sha,
            // The branch was made but the start stopped before storing where it left its base.
            None => git
                .run(&repo, &["merge-base", &branch_ref, &base_commit])
                .await?
                .trim()
                .to_owned(),
        }
    } else {
        git.run(&repo, &["branch", "--no-track", &branch, &base_commit])
            .await?;
        made.branch_at = Some(base_commit.clone());
        base_commit.clone()
    };

    let repo_name = repo
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| project.name.clone());
    let path = data.worktree_path(&repo_name, &ticket.key.to_string())?;
    match add_worktree(git, &repo, &path, &branch).await {
        Ok(added) => made.worktree = added,
        Err(e) => {
            if !reopened {
                // git may have made the worktree before a hook refused. A worktree already there is reused, never
                // an error, so whatever is registered at the ticket's own path now was added by this call.
                made.worktree = is_registered(git, &repo, &path).await;
                discard(board, &ticket.id, git, &repo, &path, &branch, &made).await;
            }
            return Err(e);
        }
    }
    if reopened {
        made = Made::default();
    }

    sqlx::query(
        "UPDATE tickets SET branch_name = ?, base_sha = ?, worktree_path = ?, branch_locked = TRUE,
                            updated_at = ?
         WHERE id = ?",
    )
    .bind(&branch)
    .bind(&base_sha)
    .bind(path.display().to_string())
    .bind(now_ms())
    .bind(&ticket.id)
    .execute(board.store().pool())
    .await
    .map_err(crate::store::StoreError::from)?;

    Ok(Worktree {
        path,
        branch,
        base_branch,
        base_sha,
        made,
    })
}

/// Removes what a failed start of a new ticket made ([`Made`]), and nothing else: the worktree at `path`, removed
/// and unregistered with git, and the branch, deleted only while it still points at the commit it was made at.
/// Never fails: what can't be removed is logged. git unregisters the worktree even when it can't delete every file
/// (on Windows a file still held open can keep the folder for a moment), and a registration left behind with its
/// folder gone is pruned, so the next start adds the worktree again. The ticket then no longer names the removed
/// worktree, nor a deleted branch and where it left its base.
pub(crate) async fn discard(
    board: &Board,
    ticket_id: &str,
    git: &Git,
    repo: &Path,
    path: &Path,
    branch: &str,
    made: &Made,
) {
    remove_made(git, repo, path, branch, made).await;
    let branch_gone = made.branch_at.is_some()
        && matches!(
            git.resolve(repo, &format!("refs/heads/{branch}")).await,
            Ok(None)
        );
    let cleared = sqlx::query(
        "UPDATE tickets SET worktree_path = CASE WHEN ? THEN NULL ELSE worktree_path END,
                            base_sha = CASE WHEN ? THEN NULL ELSE base_sha END,
                            branch_name = CASE WHEN ? THEN NULL ELSE branch_name END
         WHERE id = ?",
    )
    .bind(made.worktree)
    .bind(branch_gone)
    .bind(branch_gone)
    .bind(ticket_id)
    .execute(board.store().pool())
    .await;
    if let Err(e) = cleared {
        tracing::warn!(ticket = %ticket_id, error = %e, "can't forget a failed start's worktree");
    }
}

async fn remove_made(git: &Git, repo: &Path, path: &Path, branch: &str, made: &Made) {
    if made.worktree {
        let path_arg = path.display().to_string();
        if let Err(e) = git
            .run(repo, &["worktree", "remove", "--force", &path_arg])
            .await
        {
            tracing::warn!(path = %path.display(), error = %e, "git couldn't remove a half-made worktree");
        }
        for attempt in 0..5u64 {
            if !path.exists() {
                break;
            }
            if attempt > 0 {
                tokio::time::sleep(std::time::Duration::from_millis(100 * attempt)).await;
            }
            if let Err(e) = std::fs::remove_dir_all(path)
                && attempt == 4
            {
                tracing::warn!(path = %path.display(), error = %e, "can't delete a half-made worktree's folder");
            }
        }
        if let Err(e) = git.run(repo, &["worktree", "prune"]).await {
            tracing::warn!(error = %e, "can't prune worktrees");
        }
    }
    if let Some(at) = &made.branch_at {
        // With the old value: a branch that moved since (something committed on it) is kept.
        let branch_ref = format!("refs/heads/{branch}");
        if let Err(e) = git.run(repo, &["update-ref", "-d", &branch_ref, at]).await {
            tracing::warn!(%branch, error = %e, "can't delete a failed start's branch");
        }
    }
}

/// The first of `otaq/SHOP-1`, `otaq/SHOP-1-2`, `otaq/SHOP-1-3`… that no local or remote branch of `repo` has: a new
/// ticket never takes over a branch it didn't make.
pub(crate) async fn free_branch(git: &Git, repo: &Path, key: &str) -> Result<String, GitError> {
    let mut n = 1;
    loop {
        let name = match n {
            1 => naming::branch(key),
            n => naming::branch(&format!("{key}-{n}")),
        };
        let found = git
            .run(
                repo,
                &[
                    "for-each-ref",
                    "--format=%(refname)",
                    &format!("refs/heads/{name}"),
                    &format!("refs/remotes/*/{name}"),
                ],
            )
            .await?;
        if found.trim().is_empty() {
            return Ok(name);
        }
        n += 1;
    }
}

/// Creates `branch` at `from`, or leaves it where it is when it already exists (another start made it a moment
/// ago). `true` when this call created it; a failure for any other reason is returned unchanged.
pub async fn create_branch_unless_exists(
    git: &Git,
    repo: &Path,
    branch: &str,
    from: &str,
) -> Result<bool, GitError> {
    match git.run(repo, &["branch", "--no-track", branch, from]).await {
        Ok(_) => Ok(true),
        Err(err) => match git.resolve(repo, &format!("refs/heads/{branch}")).await {
            Ok(Some(_)) => Ok(false),
            _ => Err(err),
        },
    }
}

/// The commit of a base branch: the local branch, or the remote's when there is no local one (`origin/develop`).
pub(crate) async fn branch_commit(
    git: &Git,
    repo: &Path,
    branch: &str,
) -> Result<String, StartError> {
    if let Some(sha) = git.resolve(repo, &format!("refs/heads/{branch}")).await? {
        return Ok(sha);
    }
    let remotes = git
        .run(
            repo,
            &[
                "for-each-ref",
                "--format=%(refname)",
                &format!("refs/remotes/*/{branch}"),
            ],
        )
        .await?;
    let mut candidates: Vec<&str> = remotes.lines().collect();
    // Prefer origin when several remotes have it.
    candidates.sort_by_key(|r| !r.starts_with("refs/remotes/origin/"));
    for candidate in candidates {
        if let Some(sha) = git.resolve(repo, candidate).await? {
            return Ok(sha);
        }
    }
    Err(StartError::Other(format!(
        "the base branch `{branch}` doesn't exist in {}",
        repo.display()
    )))
}

/// Adds the worktree at `path` on `branch`, or reuses the one already there. `true` when this call added it.
pub(crate) async fn add_worktree(
    git: &Git,
    repo: &Path,
    path: &Path,
    branch: &str,
) -> Result<bool, StartError> {
    if crate::uninstall::creation_refused(path) {
        return Err(StartError::Other(crate::uninstall::uninstalling_refusal()));
    }
    let registered = registered_worktrees(git, repo).await?;
    let is_registered = registered.iter().any(|p| same_path(p, path));
    if is_registered && path.is_dir() {
        return Ok(false);
    }
    if let Some(parent) = path.parent() {
        DataDir::ensure_dir(parent)
            .map_err(|e| StartError::Other(format!("can't create {}: {e}", parent.display())))?;
    }
    if !is_registered && path.exists() && !is_empty_dir(path) {
        return Err(StartError::Other(format!(
            "{} exists and isn't a worktree of {}; move it away and retry",
            path.display(),
            repo.display()
        )));
    }
    let path_arg = path.display().to_string();
    let mut args = vec!["worktree", "add", "--quiet"];
    if is_registered {
        // Registered with git but its folder is gone: `--force` lets git add it again.
        args.push("--force");
    }
    args.extend([path_arg.as_str(), branch]);
    git.step(GitStep::AddWorktree);
    git.run(repo, &args).await?;
    // The checkout writes the files and the index in the same second, so git can't trust the index's stat data and
    // reads every file again on each diff until something rewrites it (the Files view reads with
    // `--no-optional-locks`, which never does: ~300 ms a diff in a 3000-file repository). No agent runs here yet.
    let _ = git.run(path, &["update-index", "-q", "--refresh"]).await;
    Ok(true)
}

/// Whether git has a worktree registered at `path`; `false` when it can't tell.
async fn is_registered(git: &Git, repo: &Path, path: &Path) -> bool {
    registered_worktrees(git, repo)
        .await
        .is_ok_and(|list| list.iter().any(|p| same_path(p, path)))
}

async fn registered_worktrees(git: &Git, repo: &Path) -> Result<Vec<PathBuf>, GitError> {
    let list = git.run(repo, &["worktree", "list", "--porcelain"]).await?;
    Ok(list
        .lines()
        .filter_map(|l| l.strip_prefix("worktree "))
        .map(PathBuf::from)
        .collect())
}

/// Compares paths after resolving symlinks where possible (`/tmp` and `/private/tmp` on macOS).
fn same_path(a: &Path, b: &Path) -> bool {
    let canonical = |p: &Path| {
        crate::paths::canonical(p).unwrap_or_else(|_| {
            // A missing folder: canonicalize its parent and keep the name.
            match (
                p.parent().and_then(|d| crate::paths::canonical(d).ok()),
                p.file_name(),
            ) {
                (Some(dir), Some(name)) => dir.join(name),
                _ => p.to_path_buf(),
            }
        })
    };
    canonical(a) == canonical(b)
}

fn is_empty_dir(path: &Path) -> bool {
    std::fs::read_dir(path).is_ok_and(|mut entries| entries.next().is_none())
}
