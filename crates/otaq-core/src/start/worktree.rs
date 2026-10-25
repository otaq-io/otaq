//! Worktrees and temporary branches (SES-01 steps 1 and 3, SES-02, GIT-07; d0010, d0023, d0029, d0050, d0059).
//!
//! Each ticket works in its own git worktree **outside the user's repo**, at `wt/<repo-name>/<KEY>` in the data
//! folder, on the temporary branch `otaq/<KEY>` (the real name is chosen at the first push, M10). The base
//! is the ticket's base branch.
//!
//! Everything converges (SES-02): a branch or worktree that exists is reused, a worktree registered with git
//! whose folder is gone is added again, and the ticket row is filled in whatever happened before. Nothing is
//! written into the user's repository except git's own branch refs and worktree metadata under `.git/`.

use std::path::{Path, PathBuf};

use super::StartError;
use crate::board::Board;
use crate::data_dir::DataDir;
use crate::git::{Git, GitError};
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

    // Step 3: the ticket's branch.
    let branch = naming::branch(&ticket.key.to_string());
    let branch_ref = format!("refs/heads/{branch}");
    let stored = board.store().ticket(&ticket.id).await?;
    let stored_base_sha = stored.and_then(|row| row.base_sha);
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
        base_commit.clone()
    };

    let repo_name = repo
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| project.name.clone());
    let path = data.worktree_path(&repo_name, &ticket.key.to_string())?;
    add_worktree(git, &repo, &path, &branch).await?;

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
    })
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

/// Adds the worktree at `path` on `branch`, or reuses the one already there.
pub(crate) async fn add_worktree(
    git: &Git,
    repo: &Path,
    path: &Path,
    branch: &str,
) -> Result<(), StartError> {
    let registered = registered_worktrees(git, repo).await?;
    let is_registered = registered.iter().any(|p| same_path(p, path));
    if is_registered && path.is_dir() {
        return Ok(());
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
    git.run(repo, &args).await?;
    // The checkout writes the files and the index in the same second, so git can't trust the index's stat data and
    // reads every file again on each diff until something rewrites it (the Files view reads with
    // `--no-optional-locks`, which never does: ~300 ms a diff in a 3000-file repository). No agent runs here yet.
    let _ = git.run(path, &["update-index", "-q", "--refresh"]).await;
    Ok(())
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
