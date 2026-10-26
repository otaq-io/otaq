//! `otaq uninstall` (d0134, SDD §17.3): what Otaq left on this computer. [`Uninstall::scan`] (UNI-02) only reads: it
//! lists every worktree under the data folder with its ticket and ✎, the merge worktrees, the projects, the sizes,
//! what is running, whether the app is open, the `otaq/` branches and the folder trust that stay. The CLI asks its
//! questions from it; nothing here deletes anything.
//!
//! Which ticket owns a worktree is the Cleanup scan's rule ([`Cleanup::worktree_owners`], REC-04), never a second one.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::agent_tools::Tool;
use crate::board::{Board, BoardError};
use crate::data_dir::DataDir;
use crate::domain::TicketKey;
use crate::git::Git;
use crate::naming;
use crate::recovery::cleanup::{Cleanup, WorktreeOwner, folder_size, normalized};
use crate::service::EventBus;
use crate::session::{SessionKind, Sessions};
use crate::store::projects::Project;

/// How long the scan waits for the service's startup trust pass, so the summary names the trust it recorded.
const TRUST_PASS_WAIT: Duration = Duration::from_secs(10);

/// What `otaq uninstall` would remove and ask about (UNI-02).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UninstallScan {
    /// The data folder, as `OTAQ_HOME` names it (or the default).
    pub data_folder: String,
    /// Every folder under `<data>/wt/<repo>/`, in path order: each ticket's and each no ticket owns.
    pub worktrees: Vec<UninstallWorktree>,
    /// Every folder under `<data>/scratch/` (d0038), in path order.
    pub merge_worktrees: Vec<MergeWorktree>,
    pub projects: Vec<UninstallProject>,
    /// Every ticket of every project, finished ones included.
    pub tickets: u64,
    /// The whole data folder, symlinks not followed.
    pub data_size_bytes: u64,
    /// `<data>/warm/`, symlinks not followed.
    pub warm_size_bytes: u64,
    pub running: Running,
    /// The desktop app is connected (its event subscription, Q565); the CLI's own connections never count.
    pub app_open: bool,
    /// The `otaq/` branches per project whose repository is still there, kept on purpose (d0134).
    pub branches: Vec<KeptBranches>,
    /// The folder trust kept on purpose (d0085, d0134), per tool: the projects whose trust Otaq recorded or found
    /// there. Only Otaq's own record; the tools' files aren't read.
    pub trust: Vec<KeptTrust>,
}

/// A folder under `<data>/wt/<repo>/`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UninstallWorktree {
    pub path: String,
    /// The folder it sits in under `wt/`: the repository's folder name.
    pub repo: String,
    /// The project it belongs to: the repository git lists it in, else its ticket's project, else the only project
    /// with that repository folder name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,
    /// That project's repository.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repo_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ticket: Option<TicketKey>,
    pub owner: WorktreeOwnerKind,
    /// `git worktree list` in the project's repository names it. A folder git doesn't know goes with the data folder.
    pub registered: bool,
    /// ✎: uncommitted changes, tracked or untracked (`git status --porcelain`). Also a folder git can't read (its
    /// repository is gone) and a non-empty folder that isn't a checkout: what is in them is lost.
    pub dirty: bool,
    pub size_bytes: u64,
}

/// Whose a worktree is ([`WorktreeOwner`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorktreeOwnerKind {
    /// An unfinished ticket's.
    Ticket,
    /// A helper's (d0133), in use or left behind.
    Helper,
    /// A ticket's, kept because of uncommitted changes (REC-05).
    Kept,
    /// No ticket's.
    None,
}

/// A merge worktree `<data>/scratch/<repo>` (d0038).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MergeWorktree {
    pub path: String,
    pub repo: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repo_path: Option<String>,
    pub registered: bool,
    /// ✎, as for [`UninstallWorktree::dirty`] (a crash during a merge).
    pub dirty: bool,
    pub size_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UninstallProject {
    pub key: String,
    pub name: String,
    pub repo_path: String,
    /// The repository folder is still there (moved or deleted otherwise).
    pub repo_exists: bool,
}

/// The sessions that end when the service stops (UNI-04).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Running {
    /// Running agent sessions of tickets: agents, checkers, helpers, answer and delivery sessions.
    pub agents: u32,
    /// Running conversations with Noyan (Project agent sessions).
    pub noyan_conversations: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeptBranches {
    pub project: String,
    /// The repository's folder name: "shop-web".
    pub repo: String,
    pub repo_path: String,
    /// `otaq/` branches (`git branch --list 'otaq/*'`).
    pub count: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeptTrust {
    pub tool: Tool,
    /// Project keys, in key order.
    pub projects: Vec<String>,
}

/// The uninstall's service side. Cheap to clone.
#[derive(Clone)]
pub struct Uninstall {
    board: Board,
    data: DataDir,
    sessions: Sessions,
    cleanup: Cleanup,
    events: EventBus,
}

impl Uninstall {
    pub fn new(
        board: Board,
        data: DataDir,
        sessions: Sessions,
        cleanup: Cleanup,
        events: EventBus,
    ) -> Self {
        Self {
            board,
            data,
            sessions,
            cleanup,
            events,
        }
    }

    /// What `otaq uninstall` would remove and ask about. Changes nothing: git is only asked to list and to report
    /// status (without refreshing any index), and sizes are read without following symlinks.
    pub async fn scan(&self) -> Result<UninstallScan, BoardError> {
        let projects = self.board.projects().await?;
        let mut tickets = Vec::new();
        for project in &projects {
            tickets.extend(self.board.tickets(&project.key).await?);
        }
        let git = self.board.git()?;
        let existing: Vec<&Project> = projects
            .iter()
            .filter(|p| Path::new(&p.repo_path).is_dir())
            .collect();

        // The worktrees git knows, by folder, with their project.
        let mut registered: HashMap<PathBuf, &Project> = HashMap::new();
        for &project in &existing {
            let list = git
                .run(
                    Path::new(&project.repo_path),
                    &["worktree", "list", "--porcelain"],
                )
                .await
                .unwrap_or_default();
            for path in list.lines().filter_map(|l| l.strip_prefix("worktree ")) {
                registered.insert(normalized(path), project);
            }
        }
        let by_repo_name = |name: &str| -> Option<&Project> {
            let mut matching = projects
                .iter()
                .filter(|p| crate::warm::store::repo_name(p) == name);
            match (matching.next(), matching.next()) {
                (Some(only), None) => Some(only),
                _ => None,
            }
        };

        let mut worktrees = Vec::new();
        for (folder, owner) in self.cleanup.worktree_owners(&tickets).await? {
            let repo = parent_name(&folder);
            let known = registered
                .get(&normalized(&folder.display().to_string()))
                .copied();
            let ticket = owner.ticket().map(|t| t.key.clone());
            let project = known
                .or_else(|| {
                    let key = ticket.as_ref()?.project.to_string();
                    projects.iter().find(|p| p.key == key)
                })
                .or_else(|| by_repo_name(&repo));
            worktrees.push(UninstallWorktree {
                path: folder.display().to_string(),
                repo,
                project: project.map(|p| p.key.clone()),
                repo_path: project.map(|p| p.repo_path.clone()),
                ticket,
                owner: match owner {
                    WorktreeOwner::Ticket(_) => WorktreeOwnerKind::Ticket,
                    WorktreeOwner::Helper { .. } => WorktreeOwnerKind::Helper,
                    WorktreeOwner::Kept(_) => WorktreeOwnerKind::Kept,
                    WorktreeOwner::None => WorktreeOwnerKind::None,
                },
                registered: known.is_some(),
                dirty: dirty(git, &folder).await,
                size_bytes: size(folder.clone()).await,
            });
        }

        let mut merge_worktrees = Vec::new();
        for folder in subfolders(&self.data.root().join("scratch")) {
            let repo = folder
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            let known = registered
                .get(&normalized(&folder.display().to_string()))
                .copied();
            let project = known.or_else(|| by_repo_name(&repo));
            merge_worktrees.push(MergeWorktree {
                path: folder.display().to_string(),
                repo,
                project: project.map(|p| p.key.clone()),
                repo_path: project.map(|p| p.repo_path.clone()),
                registered: known.is_some(),
                dirty: dirty(git, &folder).await,
                size_bytes: size(folder.clone()).await,
            });
        }

        let mut branches = Vec::new();
        let prefix = naming::branch_prefix();
        for &project in &existing {
            let refs = git
                .run(
                    Path::new(&project.repo_path),
                    &[
                        "for-each-ref",
                        "--format=%(refname:short)",
                        &format!("refs/heads/{prefix}"),
                    ],
                )
                .await
                .unwrap_or_default();
            branches.push(KeptBranches {
                project: project.key.clone(),
                repo: crate::warm::store::repo_name(project),
                repo_path: project.repo_path.clone(),
                count: refs.lines().filter(|l| l.starts_with(&prefix)).count() as u32,
            });
        }

        let mut running = Running::default();
        for session in self.sessions.in_memory() {
            if !session.is_running() || !session.kind.runs_agent_tool() {
                continue;
            }
            match session.kind {
                SessionKind::ProjectAgent => running.noyan_conversations += 1,
                _ => running.agents += 1,
            }
        }

        Ok(UninstallScan {
            data_folder: self.data.root().display().to_string(),
            worktrees,
            merge_worktrees,
            projects: projects
                .iter()
                .map(|p| UninstallProject {
                    key: p.key.clone(),
                    name: p.name.clone(),
                    repo_path: p.repo_path.clone(),
                    repo_exists: Path::new(&p.repo_path).is_dir(),
                })
                .collect(),
            tickets: tickets.len() as u64,
            data_size_bytes: size(self.data.root().to_owned()).await,
            warm_size_bytes: size(self.data.warm_dir()).await,
            running,
            app_open: self.events.app_count() > 0,
            branches,
            trust: self.kept_trust(&projects).await,
        })
    }

    /// The folder trust Otaq recorded (or found) for the stored projects, per tool in d0086's order.
    async fn kept_trust(&self, projects: &[Project]) -> Vec<KeptTrust> {
        let trust = self.board.trust();
        if tokio::time::timeout(TRUST_PASS_WAIT, trust.pass_finished())
            .await
            .is_err()
        {
            tracing::warn!(
                "the folder trust pass is still running: uninstall names what it has so far"
            );
        }
        let trusted = trust.trusted();
        Tool::ALL
            .iter()
            .filter_map(|&tool| {
                let keys: Vec<String> = trusted
                    .iter()
                    .filter(|(key, t)| *t == tool && projects.iter().any(|p| &p.key == key))
                    .map(|(key, _)| key.clone())
                    .collect();
                (!keys.is_empty()).then_some(KeptTrust {
                    tool,
                    projects: keys,
                })
            })
            .collect()
    }
}

/// ✎ for a worktree folder: `git status --porcelain` lists anything (tracked or untracked), or git can't read it
/// (its repository is gone). A folder that isn't a checkout is ✎ when it holds anything.
async fn dirty(git: &Git, folder: &Path) -> bool {
    if folder.join(".git").symlink_metadata().is_err() {
        return std::fs::read_dir(folder).is_ok_and(|mut entries| entries.next().is_some());
    }
    match git
        .run(
            folder,
            &[
                "--no-optional-locks",
                "status",
                "--porcelain",
                "--untracked-files=all",
            ],
        )
        .await
    {
        Ok(status) => !status.trim().is_empty(),
        Err(_) => true,
    }
}

/// A folder's size, symlinks not followed, off the async threads (the data folder can hold GBs).
async fn size(folder: PathBuf) -> u64 {
    tokio::task::spawn_blocking(move || folder_size(&folder))
        .await
        .unwrap_or_default()
}

fn parent_name(folder: &Path) -> String {
    folder
        .parent()
        .and_then(Path::file_name)
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// The folders directly in `dir`, in path order.
fn subfolders(dir: &Path) -> Vec<PathBuf> {
    let mut folders: Vec<PathBuf> = std::fs::read_dir(dir)
        .map(|entries| {
            entries
                .filter_map(Result::ok)
                .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
                .map(|e| e.path())
                .collect()
        })
        .unwrap_or_default();
    folders.sort();
    folders
}
