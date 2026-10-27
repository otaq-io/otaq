//! `otaq uninstall` (d0134, SDD §17.3): what Otaq left on this computer. [`Uninstall::scan`] (UNI-02) only reads: it
//! lists every worktree under the data folder with its ticket and ✎, the merge worktrees, the projects, the sizes,
//! what is running, whether the app is open, the `otaq/` branches and the folder trust that stay. The CLI asks its
//! questions from it.
//!
//! [`Uninstall::remove`] (UNI-05, all but the data folder) runs only once the user answered both questions: it stops
//! every session and lets none start again, then, from a scan taken after that, runs each ticket's resource Remove
//! (Q564) and `git worktree remove --force` on each worktree (when they were confirmed) and on each merge worktree,
//! and removes Otaq's marked skill copies from the projects' checkouts. Never a branch, never the user's files.
//!
//! Which ticket owns a worktree is the Cleanup scan's rule ([`Cleanup::worktree_owners`], REC-04), never a second one.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::agent_tools::Tool;
use crate::board::{Board, BoardError, Refusal};
use crate::data_dir::DataDir;
use crate::domain::TicketKey;
use crate::git::Git;
use crate::naming;
use crate::recovery::cleanup::{Cleanup, WorktreeOwner, folder_size, normalized};
use crate::service::EventBus;
use crate::session::{SessionKind, Sessions};
use crate::store::projects::Project;
use crate::ticket_env::TicketEnv;

/// The data folders whose service is removing its worktrees (d0134): no worktree or merge worktree is made in them
/// any more, until the service restarts. Per data folder, not per process: tests run several services in one.
static CLOSED: Mutex<Vec<PathBuf>> = Mutex::new(Vec::new());

/// Whether making a worktree at `path` is refused: it is inside a data folder being uninstalled.
pub(crate) fn creation_refused(path: &Path) -> bool {
    CLOSED
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .iter()
        .any(|root| path.starts_with(root))
}

/// Why a start or a worktree is refused while the uninstall runs.
pub(crate) fn uninstalling_refusal() -> String {
    format!("{} is being uninstalled", naming::PRODUCT_NAME)
}

/// What [`delete_data_folder`] did.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FolderDeleted {
    /// The bytes still in the folder afterwards: what `--keep-data` kept, or 0 (symlinks not followed).
    pub left_bytes: u64,
    /// What couldn't be deleted, with why. The rest was deleted anyway.
    pub failed: Vec<(PathBuf, String)>,
}

/// The data folder's entries `--keep-data` keeps (UNI-05): the database files, the `otaq.db.before-<version>` copy
/// (d0140) and the comment reports.
fn kept_entries(data: &DataDir) -> Vec<PathBuf> {
    let db = data.database_path();
    let with_suffix = |suffix: &str| {
        let mut name = db.clone().into_os_string();
        name.push(suffix);
        PathBuf::from(name)
    };
    let mut kept = vec![
        db.clone(),
        with_suffix("-wal"),
        with_suffix("-shm"),
        data.reports_dir(),
    ];
    kept.extend(
        crate::store::versions::copies(&db)
            .into_iter()
            .map(|copy| copy.path),
    );
    kept
}

/// The size of what `--keep-data` keeps.
pub fn kept_size(data: &DataDir) -> u64 {
    kept_entries(data)
        .iter()
        .map(|path| match path.symlink_metadata() {
            Ok(meta) if meta.is_dir() => folder_size(path),
            Ok(meta) => meta.len(),
            Err(_) => 0,
        })
        .sum()
}

/// Deletes the data folder (UNI-05), or with `keep_data` everything in it but the database files, its `before-`
/// copy and `reports/`.
/// Run by the CLI once the service has exited: it takes the service's lock itself (waiting up to `wait` for the
/// stopping service to let go of it) and holds it while it deletes, so no service runs or starts meanwhile.
/// Refused, with nothing deleted, for a folder that doesn't hold Otaq's database, for the home folder and for a
/// filesystem root. Symlinks are deleted, never followed. A failure doesn't stop the rest.
pub fn delete_data_folder(
    data: &DataDir,
    keep_data: bool,
    wait: Duration,
) -> Result<FolderDeleted, String> {
    let root = data.root();
    let is_home = std::env::home_dir().is_some_and(|home| {
        crate::paths::canonical(&home).ok() == crate::paths::canonical(root).ok()
    });
    if root.parent().is_none() || is_home || !data.database_path().is_file() {
        return Err(format!(
            "{} doesn't look like {}'s data folder; nothing deleted",
            root.display(),
            naming::PRODUCT_NAME
        ));
    }
    let lock_path = data.service_lock_path();
    let deadline = std::time::Instant::now() + wait;
    let lock = loop {
        match crate::service::lock::ServiceLock::acquire(&lock_path) {
            Ok(lock) => break lock,
            Err(crate::service::lock::LockError::Held { .. })
                if std::time::Instant::now() < deadline =>
            {
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(crate::service::lock::LockError::Held { .. }) => {
                return Err("the background service is still running; nothing deleted".into());
            }
            Err(crate::service::lock::LockError::Io(e)) => {
                return Err(format!("can't check the background service: {e}"));
            }
        }
    };
    let kept = if keep_data {
        kept_entries(data)
    } else {
        Vec::new()
    };
    let mut failed = Vec::new();
    let entries: Vec<PathBuf> = std::fs::read_dir(root)
        .map(|entries| entries.filter_map(Result::ok).map(|e| e.path()).collect())
        .unwrap_or_default();
    for path in entries {
        if path == lock_path || kept.contains(&path) {
            continue;
        }
        let removed = match path.symlink_metadata() {
            Ok(meta) if meta.is_dir() => std::fs::remove_dir_all(&path),
            Ok(_) => std::fs::remove_file(&path),
            Err(e) => Err(e),
        };
        if let Err(e) = removed {
            failed.push((path, e.to_string()));
        }
    }
    // Lets go of the lock and removes its file.
    drop(lock);
    if !keep_data && let Err(e) = std::fs::remove_dir(root) {
        failed.push((root.to_owned(), e.to_string()));
    }
    let after = if root.exists() { folder_size(root) } else { 0 };
    tracing::info!(
        keep_data,
        failed = failed.len(),
        "uninstall: data folder deleted"
    );
    Ok(FolderDeleted {
        left_bytes: after,
        failed,
    })
}

/// "worktrees not confirmed": the CLI asks about them first (UNI-03).
pub const NOT_CONFIRMED: &str = "worktrees not confirmed";

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

/// What [`Uninstall::remove`] did, step by step.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct UninstallOutcome {
    pub removed: Vec<UninstallStep>,
    /// Steps that failed, with git's (or the command's) message. A failure never stops the others.
    pub failed: Vec<UninstallStep>,
    /// Left as they are, with why: a repository that is gone, a folder git doesn't know (both go with the data
    /// folder), worktrees that weren't confirmed.
    pub skipped: Vec<UninstallStep>,
}

/// One thing the removal did, or didn't.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UninstallStep {
    pub kind: StepKind,
    /// The folder; for resources, the ticket's resource name.
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ticket: Option<TicketKey>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,
    /// Why it failed or was skipped.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StepKind {
    /// A folder under `wt/`.
    Worktree,
    /// `scratch/<repo>`.
    MergeWorktree,
    /// A ticket's resource Remove (d0087, Q564).
    Resources,
    /// A marked skill copy in a project's checkout (d0098).
    SkillCopy,
}

impl UninstallStep {
    fn new(kind: StepKind, path: impl Into<String>) -> Self {
        Self {
            kind,
            path: path.into(),
            ticket: None,
            project: None,
            message: None,
        }
    }

    fn because(mut self, message: impl Into<String>) -> Self {
        self.message = Some(message.into());
        self
    }
}

/// The uninstall's service side. Cheap to clone.
#[derive(Clone)]
pub struct Uninstall {
    board: Board,
    data: DataDir,
    sessions: Sessions,
    cleanup: Cleanup,
    events: EventBus,
    ticket_env: TicketEnv,
}

impl Uninstall {
    pub fn new(
        board: Board,
        data: DataDir,
        sessions: Sessions,
        cleanup: Cleanup,
        events: EventBus,
        ticket_env: TicketEnv,
    ) -> Self {
        Self {
            board,
            data,
            sessions,
            cleanup,
            events,
            ticket_env,
        }
    }

    /// Removes what the data folder's delete can't tidy, once the user confirmed (UNI-05). `worktrees` says the
    /// user answered y to the worktrees' question: without it, refused ("worktrees not confirmed") before anything
    /// changes when there is any worktree under `wt/` or a ✎ merge worktree.
    ///
    /// Then every session stops (agents interrupted, Noyan conversations ended, as the service's stop does) and
    /// none, nor any worktree, can start again until the service restarts; the removal works from a scan taken
    /// after that. Per ticket, its resource Remove runs before its worktree goes (Q564). A worktree whose
    /// repository is gone, or that git doesn't know, is left for the data folder's delete and reported.
    pub async fn remove(&self, worktrees: bool) -> Result<UninstallOutcome, BoardError> {
        let first = self.scan().await?;
        if !worktrees && needs_confirmation(&first) {
            return Err(BoardError::refused(
                Refusal::NeedsConfirmation,
                NOT_CONFIRMED.to_owned(),
            ));
        }
        self.sessions.close();
        CLOSED
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(self.data.root().to_owned());
        self.sessions.shutdown().await;
        tracing::info!(
            worktrees,
            "uninstall: sessions stopped; removing the worktrees"
        );

        let scan = self.scan().await?;
        let git = self.board.git()?;
        let mut outcome = UninstallOutcome::default();

        // Each ticket's resources, once, before its worktree goes (their Remove may run in it).
        if worktrees {
            let mut done: HashSet<&TicketKey> = HashSet::new();
            for key in scan.worktrees.iter().filter_map(|w| w.ticket.as_ref()) {
                if done.insert(key) {
                    self.remove_resources(key, &mut outcome).await;
                }
            }
        }
        for wt in &scan.worktrees {
            let step = UninstallStep {
                ticket: wt.ticket.clone(),
                project: wt.project.clone(),
                ..UninstallStep::new(StepKind::Worktree, &wt.path)
            };
            if !worktrees {
                outcome.skipped.push(step.because(NOT_CONFIRMED));
                continue;
            }
            remove_with_git(
                git,
                step,
                wt.repo_path.as_deref(),
                wt.registered,
                &mut outcome,
            )
            .await;
        }
        for merge in &scan.merge_worktrees {
            let step = UninstallStep {
                project: merge.project.clone(),
                ..UninstallStep::new(StepKind::MergeWorktree, &merge.path)
            };
            if merge.dirty && !worktrees {
                outcome.skipped.push(step.because(NOT_CONFIRMED));
                continue;
            }
            remove_with_git(
                git,
                step,
                merge.repo_path.as_deref(),
                merge.registered,
                &mut outcome,
            )
            .await;
        }

        // Otaq's marked skill copies in each checkout (d0098), as the service's start removes them.
        for project in scan.projects.iter().filter(|p| p.repo_exists) {
            let root = PathBuf::from(&project.repo_path);
            let names = tokio::task::spawn_blocking({
                let root = root.clone();
                move || crate::skills::copy::remove_marked_copies(&root)
            })
            .await
            .unwrap_or_default();
            for name in names {
                outcome.removed.push(UninstallStep {
                    project: Some(project.key.clone()),
                    ..UninstallStep::new(
                        StepKind::SkillCopy,
                        root.join(&name).display().to_string(),
                    )
                });
            }
        }
        tracing::info!(
            removed = outcome.removed.len(),
            failed = outcome.failed.len(),
            skipped = outcome.skipped.len(),
            "uninstall: removal done"
        );
        Ok(outcome)
    }

    /// The ticket's resource Remove, as Close runs it (Q564): only for a recorded resource that isn't removed yet.
    async fn remove_resources(&self, key: &TicketKey, outcome: &mut UninstallOutcome) {
        let Ok(ticket) = self.board.ticket(&key.to_string()).await else {
            return;
        };
        let Ok(Some(row)) = self.board.store().ticket(&ticket.id).await else {
            return;
        };
        let Some(name) = row.resource_name else {
            return;
        };
        if row
            .resource_state
            .parse::<crate::domain::ResourceState>()
            .ok()
            == Some(crate::domain::ResourceState::Removed)
        {
            return;
        }
        let step = UninstallStep {
            ticket: Some(key.clone()),
            project: Some(key.project.to_string()),
            ..UninstallStep::new(StepKind::Resources, name)
        };
        match self.ticket_env.remove_resources(&ticket.id).await {
            Ok(()) => outcome.removed.push(step),
            Err(problem) => outcome.failed.push(step.because(problem)),
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

/// Whether the user must confirm first (UNI-03): any worktree under `wt/`, or a ✎ merge worktree.
fn needs_confirmation(scan: &UninstallScan) -> bool {
    !scan.worktrees.is_empty() || scan.merge_worktrees.iter().any(|m| m.dirty)
}

/// `git worktree remove --force` in the worktree's repository; a repository that is gone, or one that doesn't list
/// the folder, leaves it for the data folder's delete.
async fn remove_with_git(
    git: &Git,
    step: UninstallStep,
    repo: Option<&str>,
    registered: bool,
    outcome: &mut UninstallOutcome,
) {
    let Some(repo) = repo.filter(|r| Path::new(r).is_dir()) else {
        outcome
            .skipped
            .push(step.because("its repository is gone; it goes with the data folder"));
        return;
    };
    if !registered {
        outcome
            .skipped
            .push(step.because("git doesn't know it as a worktree; it goes with the data folder"));
        return;
    }
    match git
        .run(
            Path::new(repo),
            &["worktree", "remove", "--force", &step.path],
        )
        .await
    {
        Ok(_) => outcome.removed.push(step),
        Err(e) => {
            let message = e.to_string();
            outcome.failed.push(step.because(message));
        }
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
