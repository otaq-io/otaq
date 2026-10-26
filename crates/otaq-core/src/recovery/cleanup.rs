//! Cleanup (REC-04; d0018, d0026): leftovers no ticket owns, listed for the user to delete by hand. **Nothing here
//! deletes anything on its own**: [`Cleanup::scan`] only reads, and [`Cleanup::delete`] runs only on the user's
//! confirmed request naming the items, each checked again against a fresh scan first.
//!
//! Groups:
//! - **Sessions**: running sessions whose ticket is gone, finished (Done) or back in Backlog.
//! - **Worktrees**: folders under `<data>/wt/<repo>/` that no unfinished ticket uses. A helper's worktree (d0133) is in
//!   use while its ticket is unfinished and its piece isn't merged; one left behind (a crash before its merge, a
//!   finished ticket's with uncommitted files) is listed under its ticket.
//! - **Branches**: `otaq/*` branches in a project's repository that no ticket names (an unfinished ticket's helpers'
//!   branches count as named). The user's own branches are never candidates.
//! - **Ports**: ports still held by tickets that are Done or in Backlog.
//! - **Kept worktrees**: Done / Closed tickets whose worktree was kept because of uncommitted changes (REC-05).
//!   Deleting one discards those changes, so it is deleted only when listed explicitly.
//! - **Could not free** (M19-c, d0086): Review tickets whose build couldn't leave the worktree at Review entry (a
//!   file held open, say). "Deleting" one is the user's retry of the Free (d0018); nothing is deleted.
//! - **Ticket resources** (M19-d, d0087): Done / Closed tickets whose resource Remove failed. "Deleting" one runs
//!   the Remove again, for the recorded name.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::board::tickets::Ticket;
use crate::board::{Board, BoardError, Refusal};
use crate::data_dir::DataDir;
use crate::domain::{BuildState, ResourceState, TicketKey};
use crate::naming;
use crate::ports::PortPool;
use crate::session::Sessions;
use crate::store::now_ms;
use crate::ticket_env::TicketEnv;
use crate::ticket_env::free::{FREED_IN, FreeOutcome};

/// The Cleanup view's groups, in order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CleanupGroup {
    Sessions,
    Worktrees,
    Branches,
    Ports,
    KeptWorktrees,
    /// Review tickets whose build couldn't be freed (M19-c).
    CouldNotFree,
    /// Finished tickets whose resource Remove failed (M19-d).
    TicketResources,
}

/// One leftover.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CleanupItem {
    pub group: CleanupGroup,
    /// Names the item in a delete request: a session id, a folder, `<project>:<branch>`, or a ticket id.
    pub id: String,
    /// What the user reads: "shop-web · otaq/SHOP-9", "SHOP-26 · ports 4031, 4032".
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ticket: Option<TicketKey>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size_bytes: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub age_ms: Option<i64>,
    /// Why it is listed: "no ticket uses this worktree", "SHOP-26 was closed with uncommitted changes".
    pub why: String,
    /// Kept worktrees: the files that aren't committed.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub uncommitted: Vec<String>,
}

/// An item to delete, as the scan named it.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CleanupTarget {
    pub group: CleanupGroup,
    pub id: String,
}

/// What a delete did.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CleanupOutcome {
    pub deleted: Vec<CleanupTarget>,
    /// Items that weren't deleted, with why (no longer a leftover, or the deletion failed).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub failed: Vec<(CleanupTarget, String)>,
}

/// Who a folder under `<data>/wt/<repo>/` belongs to: the one rule for "owned" (REC-04), shared by the Cleanup scan and
/// `otaq uninstall` (d0134).
#[derive(Debug, Clone)]
pub enum WorktreeOwner {
    /// An unfinished ticket's worktree.
    Ticket(Ticket),
    /// A ticket's worktree kept because of uncommitted changes (REC-05).
    Kept(Ticket),
    /// A helper's worktree (d0133): in use while its ticket is unfinished and its piece isn't merged.
    Helper {
        helper: Box<crate::helpers::Helper>,
        ticket: Ticket,
        in_use: bool,
    },
    /// No ticket uses it.
    None,
}

impl WorktreeOwner {
    /// The ticket it belongs to, if any.
    pub fn ticket(&self) -> Option<&Ticket> {
        match self {
            Self::Ticket(ticket) | Self::Kept(ticket) | Self::Helper { ticket, .. } => Some(ticket),
            Self::None => None,
        }
    }
}

impl CleanupItem {
    pub fn target(&self) -> CleanupTarget {
        CleanupTarget {
            group: self.group,
            id: self.id.clone(),
        }
    }
}

/// Scans for leftovers and deletes the ones the user picked. Cheap to clone.
#[derive(Clone)]
pub struct Cleanup {
    board: Board,
    data: DataDir,
    sessions: Sessions,
    ports: PortPool,
    ticket_env: TicketEnv,
}

impl Cleanup {
    pub fn new(
        board: Board,
        data: DataDir,
        sessions: Sessions,
        ports: PortPool,
        ticket_env: TicketEnv,
    ) -> Self {
        Self {
            board,
            data,
            sessions,
            ports,
            ticket_env,
        }
    }

    /// Every helper of these tickets, with its ticket (d0133).
    async fn helper_rows<'a>(
        &self,
        tickets: &'a [Ticket],
    ) -> Result<Vec<(crate::helpers::Helper, &'a Ticket)>, BoardError> {
        let mut rows = Vec::new();
        for ticket in tickets {
            let helpers =
                crate::helpers::store::of_ticket(self.board.store().pool(), &ticket.id).await?;
            rows.extend(helpers.into_iter().map(|h| (h, ticket)));
        }
        Ok(rows)
    }

    /// Every folder under `<data>/wt/<repo>/`, in path order, with who owns it among `tickets` (all the board's).
    /// Reads only.
    pub async fn worktree_owners(
        &self,
        tickets: &[Ticket],
    ) -> Result<Vec<(PathBuf, WorktreeOwner)>, BoardError> {
        let kept = self.kept_ids().await?;
        let with_worktree: Vec<(PathBuf, &Ticket)> = tickets
            .iter()
            .filter_map(|t| Some((normalized(t.work.as_ref()?.worktree.as_deref()?), t)))
            .collect();
        let helpers: Vec<(PathBuf, crate::helpers::Helper, &Ticket)> = self
            .helper_rows(tickets)
            .await?
            .into_iter()
            .map(|(h, t)| (normalized(&h.worktree), h, t))
            .collect();
        let mut owners = Vec::new();
        for folder in worktree_folders(&self.data.worktrees_dir()) {
            let path = normalized(&folder.display().to_string());
            let owner = |which: &dyn Fn(&Ticket) -> bool| {
                with_worktree
                    .iter()
                    .find(|(at, t)| *at == path && which(t))
                    .map(|(_, t)| (*t).clone())
            };
            let owner = if let Some(t) = owner(&|t| !t.place.is_done()) {
                WorktreeOwner::Ticket(t)
            } else if let Some(t) = owner(&|t| kept.contains(&t.id)) {
                WorktreeOwner::Kept(t)
            } else if let Some((_, helper, ticket)) = helpers.iter().find(|(at, ..)| *at == path) {
                WorktreeOwner::Helper {
                    in_use: !ticket.place.is_done()
                        && helper.status != crate::helpers::HelperStatus::Merged,
                    helper: Box::new(helper.clone()),
                    ticket: (*ticket).clone(),
                }
            } else {
                WorktreeOwner::None
            };
            owners.push((folder, owner));
        }
        Ok(owners)
    }

    /// Every leftover, grouped. Reads only.
    pub async fn scan(&self) -> Result<Vec<CleanupItem>, BoardError> {
        let now = now_ms();
        let projects = self.board.projects().await?;
        let mut tickets: Vec<Ticket> = Vec::new();
        for project in &projects {
            tickets.extend(self.board.tickets(&project.key).await?);
        }
        let mut items = Vec::new();

        // Sessions of tickets that are gone, done or in Backlog.
        for session in self.sessions.in_memory() {
            if !session.is_running() {
                continue;
            }
            let Some(key) = &session.ticket else {
                continue;
            };
            let owner = tickets.iter().find(|t| &t.key == key);
            let why = match owner {
                None => format!("{key} no longer exists"),
                Some(t) if t.place.is_done() || t.place.is_backlog() => {
                    format!("{key} is {}", t.status.label())
                }
                Some(_) => continue,
            };
            items.push(CleanupItem {
                group: CleanupGroup::Sessions,
                id: session.id.clone(),
                label: format!("{key} · {} session", session.kind),
                project: Some(session.project.clone()),
                ticket: Some(key.clone()),
                path: None,
                size_bytes: None,
                age_ms: Some((now - session.started_at).max(0)),
                why,
                uncommitted: Vec::new(),
            });
        }

        // Worktree folders no unfinished ticket uses; kept ones of finished tickets are their own group.
        for (folder, owner) in self.worktree_owners(&tickets).await? {
            let (label, project, ticket, why) = match owner {
                WorktreeOwner::Ticket(_)
                | WorktreeOwner::Kept(_)
                | WorktreeOwner::Helper { in_use: true, .. } => continue,
                // A helper's worktree left behind (its ticket finished, or its piece merged): listed under it.
                WorktreeOwner::Helper { helper, ticket, .. } => (
                    format!("{} \u{b7} {} {}", ticket.key, helper.label(), helper.title),
                    Some(ticket.key.project.to_string()),
                    Some(ticket.key.clone()),
                    if ticket.place.is_done() {
                        format!("{} is {}", ticket.key, ticket.status.label())
                    } else {
                        format!("{} was merged", helper.label())
                    },
                ),
                WorktreeOwner::None => (
                    folder
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_default(),
                    folder
                        .parent()
                        .and_then(Path::file_name)
                        .map(|n| n.to_string_lossy().into_owned()),
                    None,
                    "no ticket uses this worktree".into(),
                ),
            };
            items.push(CleanupItem {
                group: CleanupGroup::Worktrees,
                id: folder.display().to_string(),
                label,
                project,
                ticket,
                path: Some(folder.display().to_string()),
                size_bytes: Some(folder_size(&folder)),
                age_ms: modified_age(&folder, now),
                why,
                uncommitted: Vec::new(),
            });
        }

        // Branches named by Otaq that no ticket names.
        let helpers = self.helper_rows(&tickets).await?;
        let git = self.board.git()?;
        let prefix = naming::branch_prefix();
        for project in &projects {
            let named: HashSet<&str> = tickets
                .iter()
                .filter(|t| t.key.project.as_str() == project.key)
                .filter_map(|t| t.work.as_ref()?.branch.as_deref())
                .chain(
                    helpers
                        .iter()
                        .filter(|(_, t)| {
                            t.key.project.as_str() == project.key && !t.place.is_done()
                        })
                        .map(|(h, _)| h.branch.as_str()),
                )
                .collect();
            let refs = git
                .run(
                    Path::new(&project.repo_path),
                    &[
                        "for-each-ref",
                        "--format=%(refname:short) %(committerdate:unix)",
                        &format!("refs/heads/{prefix}"),
                    ],
                )
                .await
                .unwrap_or_default();
            for line in refs.lines() {
                let mut parts = line.split_whitespace();
                let (Some(branch), at) = (parts.next(), parts.next()) else {
                    continue;
                };
                if !branch.starts_with(&prefix) || named.contains(branch) {
                    continue;
                }
                let at = at.and_then(|s| s.parse::<i64>().ok());
                items.push(CleanupItem {
                    group: CleanupGroup::Branches,
                    id: format!("{}:{branch}", project.key),
                    label: format!("{} · {branch}", project.name),
                    project: Some(project.key.clone()),
                    ticket: None,
                    path: Some(project.repo_path.clone()),
                    size_bytes: None,
                    age_ms: at.map(|secs| (now - secs * 1000).max(0)),
                    why: "no ticket uses this branch".into(),
                    uncommitted: Vec::new(),
                });
            }
        }

        // Ports held by tickets that are finished or back in Backlog.
        for ticket in &tickets {
            let Some(work) = &ticket.work else { continue };
            let ports: Vec<u16> = [work.agent_port, work.run_port]
                .into_iter()
                .flatten()
                .collect();
            if ports.is_empty() || !(ticket.place.is_done() || ticket.place.is_backlog()) {
                continue;
            }
            items.push(CleanupItem {
                group: CleanupGroup::Ports,
                id: ticket.id.clone(),
                label: format!(
                    "{} · {} {}",
                    ticket.key,
                    if ports.len() == 1 { "port" } else { "ports" },
                    ports
                        .iter()
                        .map(u16::to_string)
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
                project: Some(ticket.key.project.to_string()),
                ticket: Some(ticket.key.clone()),
                path: None,
                size_bytes: None,
                age_ms: ticket.status_since.map(|s| (now - s).max(0)),
                why: format!("{} is {}", ticket.key, ticket.status.label()),
                uncommitted: Vec::new(),
            });
        }

        // Worktrees kept because of uncommitted changes.
        for (ticket, uncommitted) in self.kept_tickets(&tickets).await? {
            let Some(path) = ticket.work.as_ref().and_then(|w| w.worktree.clone()) else {
                continue;
            };
            let folder = PathBuf::from(&path);
            items.push(CleanupItem {
                group: CleanupGroup::KeptWorktrees,
                id: ticket.id.clone(),
                label: format!("{} · {}", ticket.key, ticket.title),
                project: Some(ticket.key.project.to_string()),
                ticket: Some(ticket.key.clone()),
                size_bytes: folder.is_dir().then(|| folder_size(&folder)),
                age_ms: modified_age(&folder, now),
                path: Some(path),
                why: format!(
                    "{} is {} with uncommitted changes",
                    ticket.key,
                    ticket.status.label()
                ),
                uncommitted,
            });
        }

        // Review builds that couldn't be freed (M19-c): the problem is why.
        for ticket in &tickets {
            let Some(work) = &ticket.work else { continue };
            let (Some(problem), Some(path)) = (&work.build_problem, &work.worktree) else {
                continue;
            };
            if work.build_state != BuildState::Present || !FREED_IN.contains(&ticket.status) {
                continue;
            }
            let folder = PathBuf::from(path);
            items.push(CleanupItem {
                group: CleanupGroup::CouldNotFree,
                id: ticket.id.clone(),
                label: format!("{} · {}", ticket.key, ticket.title),
                project: Some(ticket.key.project.to_string()),
                ticket: Some(ticket.key.clone()),
                path: Some(path.clone()),
                size_bytes: folder.is_dir().then(|| folder_size(&folder)),
                age_ms: ticket.status_since.map(|s| (now - s).max(0)),
                why: problem.clone(),
                uncommitted: Vec::new(),
            });
        }

        // Resources of finished tickets that Remove couldn't remove (M19-d): the recorded name, and the problem.
        for ticket in tickets.iter().filter(|t| t.place.is_done()) {
            let Some(work) = &ticket.work else { continue };
            let Some(name) = &work.resource_name else {
                continue;
            };
            if work.resource_state != ResourceState::Failed {
                continue;
            }
            let problem = sqlx::query_scalar::<_, Option<String>>(
                "SELECT resource_problem FROM tickets WHERE id = ?",
            )
            .bind(&ticket.id)
            .fetch_optional(self.board.store().pool())
            .await?
            .flatten();
            items.push(CleanupItem {
                group: CleanupGroup::TicketResources,
                id: ticket.id.clone(),
                label: name.clone(),
                project: Some(ticket.key.project.to_string()),
                ticket: Some(ticket.key.clone()),
                path: None,
                size_bytes: None,
                age_ms: ticket.status_since.map(|s| (now - s).max(0)),
                why: problem
                    .unwrap_or_else(|| format!("{}'s resources weren't removed", ticket.key)),
                uncommitted: Vec::new(),
            });
        }

        items.sort_by(|a, b| a.group.cmp(&b.group).then(a.label.cmp(&b.label)));
        Ok(items)
    }

    /// Deletes exactly the `targets` the user confirmed. Each one must still be a leftover in a fresh scan;
    /// anything else is reported, never deleted. Without `confirm`, refused with what would be deleted.
    pub async fn delete(
        &self,
        targets: &[CleanupTarget],
        confirm: bool,
    ) -> Result<CleanupOutcome, BoardError> {
        let scan = self.scan().await?;
        if !confirm {
            let names: Vec<String> = targets
                .iter()
                .filter_map(|t| scan.iter().find(|i| i.target() == *t))
                .map(|i| i.label.clone())
                .collect();
            return Err(BoardError::refused(
                Refusal::NeedsConfirmation,
                format!("Delete {}? This can't be undone.", names.join(", ")),
            ));
        }
        let mut outcome = CleanupOutcome::default();
        let mut repos: HashSet<PathBuf> = HashSet::new();
        for target in targets {
            let Some(item) = scan.iter().find(|i| i.target() == *target) else {
                outcome
                    .failed
                    .push((target.clone(), "not a leftover".into()));
                continue;
            };
            match self.delete_one(item, &mut repos).await {
                Ok(()) => {
                    tracing::info!(group = ?item.group, item = %item.label, "cleanup deleted");
                    outcome.deleted.push(target.clone());
                }
                Err(e) => outcome.failed.push((target.clone(), e.to_string())),
            }
        }
        // Worktrees removed by hand leave records in their repositories.
        if let Ok(git) = self.board.git() {
            for repo in repos {
                let _ = git.run(&repo, &["worktree", "prune"]).await;
            }
        }
        Ok(outcome)
    }

    async fn delete_one(
        &self,
        item: &CleanupItem,
        repos: &mut HashSet<PathBuf>,
    ) -> Result<(), BoardError> {
        let git = self.board.git()?;
        match item.group {
            CleanupGroup::Sessions => self
                .sessions
                .stop(&item.id)
                .await
                .map_err(|e| BoardError::refused(Refusal::SessionEnded, e.to_string())),
            CleanupGroup::Worktrees => {
                let folder = PathBuf::from(&item.id);
                self.remove_worktree(&folder, repos).await
            }
            CleanupGroup::Branches => {
                let (project, branch) = item.id.split_once(':').unwrap_or_default();
                if !branch.starts_with(&naming::branch_prefix()) {
                    return Err(BoardError::refused(
                        Refusal::NotAllowed,
                        format!("{branch} isn't a branch {} made", naming::PRODUCT_NAME),
                    ));
                }
                let repo = self.board.project(project).await?.repo_path;
                git.run(Path::new(&repo), &["branch", "-D", branch]).await?;
                Ok(())
            }
            CleanupGroup::Ports => self
                .ports
                .free(&item.id)
                .await
                .map_err(|e| BoardError::refused(Refusal::SessionEnded, e.to_string())),
            CleanupGroup::KeptWorktrees => {
                if let Some(path) = &item.path {
                    self.remove_worktree(Path::new(path), repos).await?;
                }
                sqlx::query(
                    "UPDATE tickets SET worktree_path = NULL, worktree_kept = FALSE, build_state = ?,
                                        build_problem = NULL
                     WHERE id = ?",
                )
                .bind(BuildState::Present.as_str())
                .bind(&item.id)
                .execute(self.board.store().pool())
                .await?;
                Ok(())
            }
            // The user's retry of the Free (d0018).
            CleanupGroup::CouldNotFree => {
                let Some(key) = &item.ticket else {
                    return Ok(());
                };
                match self.ticket_env.free(key).await {
                    FreeOutcome::Freed { .. } | FreeOutcome::NotNeeded => Ok(()),
                    FreeOutcome::Waiting => Err(BoardError::refused(
                        Refusal::NotAllowed,
                        format!("{key}'s Run or Shell is open; its build is freed when it closes"),
                    )),
                    FreeOutcome::Failed(problem) => {
                        Err(BoardError::refused(Refusal::NotAllowed, problem))
                    }
                }
            }
            // The user's retry of the Remove (d0018), for the recorded name.
            CleanupGroup::TicketResources => self
                .ticket_env
                .remove_resources(&item.id)
                .await
                .map_err(|e| BoardError::refused(Refusal::NotAllowed, e)),
        }
    }

    /// Removes a worktree folder inside the data folder: through its repository when it is a worktree of one
    /// (changes included: the user picked it), else the folder itself.
    async fn remove_worktree(
        &self,
        folder: &Path,
        repos: &mut HashSet<PathBuf>,
    ) -> Result<(), BoardError> {
        if !folder.starts_with(self.data.worktrees_dir()) {
            return Err(BoardError::refused(
                Refusal::NotAllowed,
                format!(
                    "{} isn't in {}'s worktree folder",
                    folder.display(),
                    naming::PRODUCT_NAME
                ),
            ));
        }
        if !folder.exists() {
            return Ok(());
        }
        let git = self.board.git()?;
        let path = folder.display().to_string();
        let common = git
            .run(
                folder,
                &["rev-parse", "--path-format=absolute", "--git-common-dir"],
            )
            .await
            .ok()
            .map(|dir| PathBuf::from(dir.trim()));
        if let Some(repo) = common.as_deref().and_then(Path::parent) {
            repos.insert(repo.to_owned());
            if git
                .run(repo, &["worktree", "remove", "--force", "--force", &path])
                .await
                .is_ok()
            {
                return Ok(());
            }
        }
        std::fs::remove_dir_all(folder).map_err(|e| {
            BoardError::refused(
                Refusal::NotAllowed,
                format!("can't remove {}: {e}", folder.display()),
            )
        })
    }

    /// The ids of the tickets whose worktree was kept (REC-05).
    async fn kept_ids(&self) -> Result<HashSet<String>, BoardError> {
        Ok(
            sqlx::query_scalar::<_, String>("SELECT id FROM tickets WHERE worktree_kept = TRUE")
                .fetch_all(self.board.store().pool())
                .await?
                .into_iter()
                .collect(),
        )
    }

    /// Done / Closed tickets whose worktree was kept, with their uncommitted files.
    async fn kept_tickets(
        &self,
        tickets: &[Ticket],
    ) -> Result<Vec<(Ticket, Vec<String>)>, BoardError> {
        let ids = self.kept_ids().await?;
        let mut kept = Vec::new();
        for ticket in tickets.iter().filter(|t| ids.contains(&t.id)) {
            let uncommitted = match ticket.work.as_ref().and_then(|w| w.worktree.as_deref()) {
                Some(path) if Path::new(path).is_dir() => self
                    .board
                    .git()?
                    .uncommitted_files(Path::new(path))
                    .await
                    .unwrap_or_default(),
                _ => Vec::new(),
            };
            kept.push((ticket.clone(), uncommitted));
        }
        Ok(kept)
    }
}

/// `<root>/<repo>/<KEY>` folders.
fn worktree_folders(root: &Path) -> Vec<PathBuf> {
    let dirs = |dir: &Path| -> Vec<PathBuf> {
        std::fs::read_dir(dir)
            .map(|entries| {
                entries
                    .filter_map(Result::ok)
                    .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
                    .map(|e| e.path())
                    .collect()
            })
            .unwrap_or_default()
    };
    let mut folders: Vec<PathBuf> = dirs(root).iter().flat_map(|repo| dirs(repo)).collect();
    folders.sort();
    folders
}

/// A path compared without symlinks (macOS's `/var` is `/private/var`).
pub(crate) fn normalized(path: &str) -> PathBuf {
    let path = PathBuf::from(path);
    crate::paths::canonical(&path).unwrap_or(path)
}

/// The size of everything in a folder (symlinks not followed).
pub(crate) fn folder_size(folder: &Path) -> u64 {
    let mut total = 0;
    let mut stack = vec![folder.to_owned()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.filter_map(Result::ok) {
            let Ok(meta) = entry.path().symlink_metadata() else {
                continue;
            };
            if meta.is_dir() {
                stack.push(entry.path());
            } else {
                total += meta.len();
            }
        }
    }
    total
}

fn modified_age(path: &Path, now: i64) -> Option<i64> {
    let modified = path.metadata().ok()?.modified().ok()?;
    let ms = modified
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_millis();
    Some((now - i64::try_from(ms).ok()?).max(0))
}
