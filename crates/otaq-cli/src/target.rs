//! Which ticket or project a command is about (d0032): an explicit key, a bare number (`18`, `40.1`) in the
//! current project, or `$OTAQ_TICKET` when the key is omitted.
//!
//! The current project is `-P`, else the project of `$OTAQ_TICKET`, else the project whose worktree
//! (`wt/<repo-name>/<KEY>/…`) or repository holds the current folder, else the project whose repository the current
//! folder is a git worktree of (its common dir, #43).

use std::path::{Path, PathBuf};

use otaq_core::board::snapshot::ProjectInfo;
use otaq_core::client::Client;
use otaq_core::data_dir::DataDir;
use otaq_core::domain::{ProjectKey, TicketKey};
use otaq_core::naming;
use otaq_core::protocol::{ErrorBody, ErrorKind, Reply, Request};

use crate::cli::GlobalArgs;
use crate::service::{client_error, ticket_env};

/// The ticket a command names: the argument (a key or a bare number in the current project) or
/// `$OTAQ_TICKET`.
pub async fn resolve_key(
    client: &Client,
    data: &DataDir,
    key: Option<String>,
    global: &GlobalArgs,
) -> Result<TicketKey, ErrorBody> {
    let from_env = ticket_env();
    let input = key.or_else(|| from_env.clone()).ok_or_else(|| {
        ErrorBody::new(
            ErrorKind::Usage,
            format!(
                "name a ticket (SHOP-18), or run it where {} is set",
                naming::ticket_env_var()
            ),
        )
    })?;
    let needs_project = input.trim().starts_with(|c: char| c.is_ascii_digit());
    let current = if needs_project {
        current_project(client, data, global).await?
    } else {
        None
    };
    TicketKey::resolve(&input, current.as_ref())
        .map_err(|e| ErrorBody::new(ErrorKind::Usage, e.to_string()))
}

/// The project a project-wide command works in; an error when none can be found.
pub async fn require_project(
    client: &Client,
    data: &DataDir,
    global: &GlobalArgs,
) -> Result<ProjectKey, ErrorBody> {
    current_project(client, data, global).await?.ok_or_else(|| {
        ErrorBody::new(
            ErrorKind::Usage,
            "name a project with -P SHOP, or run it inside a project's folder",
        )
    })
}

pub async fn current_project(
    client: &Client,
    data: &DataDir,
    global: &GlobalArgs,
) -> Result<Option<ProjectKey>, ErrorBody> {
    if let Some(key) = global.project_key() {
        return ProjectKey::new(key.trim())
            .map(Some)
            .map_err(|e| ErrorBody::new(ErrorKind::Usage, e.to_string()));
    }
    if let Some(key) = ticket_env().and_then(|t| t.trim().parse::<TicketKey>().ok()) {
        return Ok(Some(key.project));
    }
    let Ok(cwd) = std::env::current_dir() else {
        return Ok(None);
    };
    let cwd = otaq_core::paths::canonical(&cwd).unwrap_or(cwd);
    // Inside a ticket's worktree: wt/<repo-name>/<KEY>/…
    let worktrees = data.worktrees_dir();
    let worktrees = otaq_core::paths::canonical(&worktrees).unwrap_or(worktrees);
    if let Ok(rest) = cwd.strip_prefix(&worktrees)
        && let Some(key) = rest
            .components()
            .nth(1)
            .and_then(|c| c.as_os_str().to_str())
            .and_then(|k| k.parse::<TicketKey>().ok())
    {
        return Ok(Some(key.project));
    }
    let Reply::Projects(projects) = client
        .request(Request::ProjectList)
        .await
        .map_err(client_error)?
    else {
        return Ok(None);
    };
    let repo_of = |p: &ProjectInfo| {
        let repo = PathBuf::from(&p.repo_path);
        otaq_core::paths::canonical(&repo).unwrap_or(repo)
    };
    let inside = projects
        .iter()
        .filter(|p| cwd.starts_with(repo_of(p)))
        .max_by_key(|p| p.repo_path.len());
    // A git worktree of a project's repository elsewhere (`git worktree add ../experiment`, #43): the repository
    // whose `.git` is its common dir.
    let found = inside.or_else(|| {
        let common = git_common_dir(&cwd)?;
        let repo = common.parent()?;
        projects.iter().find(|p| repo_of(p) == repo)
    });
    Ok(found.and_then(|p| ProjectKey::new(&p.key).ok()))
}

/// `git rev-parse --git-common-dir` in `folder`, canonical: the main repository's `.git` from any of its worktrees.
fn git_common_dir(folder: &Path) -> Option<PathBuf> {
    let out = std::process::Command::new("git")
        .args(["rev-parse", "--path-format=absolute", "--git-common-dir"])
        .current_dir(folder)
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .output()
        .ok()
        .filter(|o| o.status.success())?;
    let dir = PathBuf::from(String::from_utf8(out.stdout).ok()?.trim());
    Some(otaq_core::paths::canonical(&dir).unwrap_or(dir))
}
