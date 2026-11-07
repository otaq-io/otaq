//! The ticket environment (M19, d0085…d0087): a ticket's build and resources across Prepare, Free, Bring back and
//! Release.
//!
//! - [`handoff`]: how a stock entry is handed to a worktree on this disk ([`handoff::HandoffMode`]).
//! - [`stock`]: the stock of builds per repository: its limit, the nearest build for a ticket, and eviction.
//! - [`free`]: the Free at Review entry (M19-c, d0086).
//! - [`fetch`]: bringing the build back for Send back, Fix, a Run, a Shell or a resuming start (M19-c, d0086).
//! - [`resources`]: the ticket's own resources, their variables and the Create command (M19-d, d0087).
//! - [`schema_watch`]: the warning when two Review tickets add files under one schema path (M19-e, d0087).
//!
//! [`TicketEnv`] holds a lock per ticket, so a Free and a Release (Done) of the same ticket never both move its
//! build, nor a Free and a bring-back: whichever comes second reads the ticket again and sees what the first did.
//! **Lock order**: the ticket's lock first, then the stock's (a std mutex, never held across an `.await`).
//!
//! [`TicketEnv::run`] listens to the service's events: a ticket entering [`free::FREED_IN`] is freed; a Run or
//! Shell closing frees its ticket's build if it was held for it; a ticket entering or leaving waiting to begin changes
//! the stock limit; a ticket entering or leaving [`free::FREED_IN`] has its project's schema paths scanned again. A
//! slow tick (and once at startup) frees what no event announced and scans every project ([`TicketEnv::sweep`]).

pub mod fetch;
pub mod free;
pub mod handoff;
pub mod resources;
pub mod schema_watch;
pub mod stock;

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::sync::broadcast::error::RecvError;
use tokio::sync::{OwnedMutexGuard, watch};
use tokio::task::JoinSet;

use crate::board::tickets::Ticket;
use crate::board::{Board, BoardError};
use crate::data_dir::DataDir;
use crate::domain::{Actor, BuildState, ResourceState, Status, StatusSet, TicketKey};
use crate::protocol::Event;
use crate::service::EventBus;
use crate::session::{SessionKind, Sessions};
use crate::start::StartError;
use crate::store::projects::Project;
use crate::ticket_env::fetch::{FetchWhy, Fetched};
use crate::ticket_env::free::{FREED_IN, FreeOutcome};
use crate::ticket_env::handoff::HandoffMode;
use crate::ticket_env::resources::Resources;
use crate::ticket_env::stock::Stock;
use crate::warm::seed::Seeder;
use crate::warm::store::{WarmBuilds, repo_name};

/// How often [`TicketEnv::run`] sweeps for builds to free (the scheduler's slow tick).
pub const SWEEP_TICK: Duration = crate::scheduler::DEFAULT_TICK;

/// The four moments of a ticket's environment. Cheap to clone.
#[derive(Clone)]
pub struct TicketEnv(Arc<Inner>);

struct Inner {
    board: Board,
    data: DataDir,
    warm: WarmBuilds,
    stock: Stock,
    /// Hands a build from the stock back to a freed ticket's worktree.
    seeder: Seeder,
    sessions: Sessions,
    events: EventBus,
    /// Runs the projects' ticket resource commands (d0087).
    resources: Resources,
    /// One lock per ticket id: Free, bring-back and Release never run at once for a ticket.
    locks: Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>,
    /// Schema-path scans run one at a time ([`schema_watch`]).
    schema_scan: tokio::sync::Mutex<()>,
    /// How many tickets' added files were read from git for the schema-path warning (for tests and logs).
    schema_scans: AtomicU64,
}

impl std::fmt::Debug for TicketEnv {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TicketEnv")
            .field("stock", &self.0.stock)
            .finish_non_exhaustive()
    }
}

impl TicketEnv {
    pub fn new(
        board: Board,
        data: DataDir,
        warm: WarmBuilds,
        seeder: Seeder,
        sessions: Sessions,
        events: EventBus,
    ) -> Self {
        let stock = Stock::new(warm.clone(), board.clone());
        Self(Arc::new(Inner {
            board,
            data,
            warm,
            stock,
            seeder,
            sessions,
            events,
            resources: Resources::default(),
            locks: Mutex::new(HashMap::new()),
            schema_scan: tokio::sync::Mutex::new(()),
            schema_scans: AtomicU64::new(0),
        }))
    }

    pub(crate) fn board(&self) -> &Board {
        &self.0.board
    }

    pub(crate) fn data(&self) -> &DataDir {
        &self.0.data
    }

    pub(crate) fn seeder(&self) -> &Seeder {
        &self.0.seeder
    }

    pub(crate) fn warm(&self) -> &WarmBuilds {
        &self.0.warm
    }

    pub fn stock(&self) -> &Stock {
        &self.0.stock
    }

    /// The ticket resource commands, shared by every start (d0087).
    pub fn resources(&self) -> &Resources {
        &self.0.resources
    }

    pub(crate) fn sessions(&self) -> &Sessions {
        &self.0.sessions
    }

    pub(crate) fn events(&self) -> &EventBus {
        &self.0.events
    }

    /// Waits for the ticket's lock.
    pub(crate) async fn lock(&self, ticket_id: &str) -> OwnedMutexGuard<()> {
        let lock = self
            .0
            .locks
            .lock()
            .expect("ticket locks")
            .entry(ticket_id.to_owned())
            .or_default()
            .clone();
        lock.lock_owned().await
    }

    /// **Free** (M19-c): moves `key`'s build out of its worktree into the stock, if it is in Review with its build
    /// in the worktree and no Run or Shell open ([`free`]).
    pub async fn free(&self, key: &TicketKey) -> FreeOutcome {
        free::free_one(self, key).await
    }

    /// **Bring back** (M19-c): puts a build back into the worktree of `ticket` if it was freed: its own from the
    /// stock, else the nearest, then the setup command ([`fetch`]). Does nothing when the build is there.
    pub async fn fetch_build(&self, ticket: &Ticket, why: FetchWhy) -> Result<Fetched, StartError> {
        fetch::fetch_held(self, ticket, why).await.0
    }

    /// [`fetch_build`](Self::fetch_build) for a Run or Shell about to open, returning the ticket's lock still held:
    /// no Free takes the build away before the Run or Shell holds it. A failure never stops it: what came back is
    /// taken as present, so it is freed again when the Run or Shell closes ([`fetch::for_terminal`]).
    pub(crate) async fn fetch_for_terminal(
        &self,
        ticket: &Ticket,
        why: FetchWhy,
    ) -> (Result<Fetched, StartError>, OwnedMutexGuard<()>) {
        fetch::for_terminal(self, ticket, why).await
    }

    /// Brings the build back for an agent about to work again (Send back, Fix): a failure never stops it; the
    /// build is then taken as present and a start note says why ([`fetch::for_agent`]). The lock stays held.
    pub(crate) async fn fetch_for_agent(
        &self,
        ticket: &Ticket,
        why: FetchWhy,
        actor: &Actor,
    ) -> OwnedMutexGuard<()> {
        fetch::for_agent(self, ticket, why, actor).await
    }

    /// **Release** at Done (REC-05, WRM-03): under the ticket's lock, keeps the clean `worktree`'s build in the stock
    /// unless it was freed already (never a reason for Done to fail), removes the worktree (the branch stays), and
    /// resets the ticket's build state, so a reopened ticket is never taken for freed.
    pub(crate) async fn release(
        &self,
        ticket: &Ticket,
        project: &Project,
        worktree: &Path,
    ) -> Result<(), BoardError> {
        let board = &self.0.board;
        let _held = self.lock(&ticket.id).await;
        let state = board
            .ticket_by_id(&ticket.id)
            .await?
            .work
            .map(|w| w.build_state)
            .unwrap_or_default();
        if state == BuildState::Present {
            crate::warm::keep::keep(board, &self.0.warm, project, &ticket.key, worktree).await;
        } else {
            tracing::debug!(ticket = %ticket.key, state = state.as_str(), "the build was freed: nothing to keep");
        }
        let git = board.project_git(&project.key).await?;
        crate::review::finish::remove_worktree(&git, Path::new(&project.repo_path), worktree)
            .await?;
        sqlx::query(
            "UPDATE tickets SET worktree_path = NULL, worktree_kept = FALSE, build_state = ?, build_problem = NULL
             WHERE id = ?",
        )
        .bind(BuildState::Present.as_str())
        .bind(&ticket.id)
        .execute(board.store().pool())
        .await?;
        Ok(())
    }

    /// **Release** of the ticket's resources at Done or Close (d0087), whether its worktree was removed or kept:
    /// under the ticket's lock, Remove for a recorded resource that is created, failed, or whose Create never
    /// finished (a crash), else Otaq's resource folder (`OTAQ_RESOURCE_DIR`, made for every ticket, d0109) goes to
    /// the trash; then, after a merge only, the project's Template refresh. Never a reason for Done to fail: a
    /// failed Remove is listed in Cleanup.
    pub(crate) async fn release_resources(&self, ticket: &Ticket) -> Result<(), BoardError> {
        let board = &self.0.board;
        let project = board.project(ticket.key.project.as_str()).await?;
        {
            let _held = self.lock(&ticket.id).await;
            let row = board.store().ticket(&ticket.id).await?;
            let to_remove = row.is_some_and(|row| {
                row.resource_name.is_some()
                    && row.resource_state.parse::<ResourceState>().ok()
                        != Some(ResourceState::Removed)
            });
            if to_remove
                && let Err(e) = self
                    .0
                    .resources
                    .remove(board, &self.0.data, &self.0.warm, &project, ticket)
                    .await
            {
                tracing::warn!(ticket = %ticket.key, error = %e, "the ticket's resources stay; listed in Cleanup");
            } else if !to_remove {
                resources::trash_resource_dir(&self.0.data, &self.0.warm, &project, &ticket.key);
            }
        }
        if ticket.status == Status::Merged {
            self.0
                .resources
                .refresh_template(board, &self.0.data, &project, ticket)
                .await;
        }
        Ok(())
    }

    /// Cleanup's retry of a failed Remove (d0018): runs Remove again for the ticket with this id, under its lock.
    pub(crate) async fn remove_resources(&self, ticket_id: &str) -> Result<(), String> {
        let board = &self.0.board;
        let _held = self.lock(ticket_id).await;
        let ticket = board
            .ticket_by_id(ticket_id)
            .await
            .map_err(|e| e.to_string())?;
        let project = board
            .project(ticket.key.project.as_str())
            .await
            .map_err(|e| e.to_string())?;
        self.0
            .resources
            .remove(board, &self.0.data, &self.0.warm, &project, &ticket)
            .await
    }

    /// Scans `project`'s Review tickets for files added under one schema path and stores the warnings (M19-e,
    /// [`schema_watch`]). Runs no `git` when the project names no schema paths. Never touches a file.
    pub async fn rescan_schema(&self, project: &str) {
        let Ok(project) = self.0.board.project(project).await else {
            return;
        };
        let _one = self.0.schema_scan.lock().await;
        if let Err(e) = schema_watch::rescan(self, &project).await {
            tracing::warn!(project = %project.key, error = %e, "can't scan the schema paths");
        }
    }

    /// How many tickets' added files were read from git for the schema-path warning so far.
    pub fn schema_scans(&self) -> u64 {
        self.0.schema_scans.load(Ordering::Relaxed)
    }

    pub(crate) fn counted_schema_scan(&self) {
        self.0.schema_scans.fetch_add(1, Ordering::Relaxed);
    }

    /// Frees every ticket in [`FREED_IN`] whose build is still in its worktree and whose last Free didn't fail
    /// (Cleanup's "could not free" retries those): what no event announced. Then scans every project's schema
    /// paths.
    pub async fn sweep(&self) {
        let projects = match self.0.board.projects().await {
            Ok(projects) => projects,
            Err(e) => {
                tracing::warn!(error = %e, "can't read the projects to free builds");
                return;
            }
        };
        for project in projects {
            let tickets = match self.0.board.tickets(&project.key).await {
                Ok(tickets) => tickets,
                Err(e) => {
                    tracing::warn!(project = %project.key, error = %e, "can't read the tickets to free builds");
                    continue;
                }
            };
            for ticket in tickets {
                let failed = ticket
                    .work
                    .as_ref()
                    .is_some_and(|w| w.build_problem.is_some());
                if !failed && free::to_free(&ticket).is_some() {
                    self.free(&ticket.key).await;
                }
            }
            self.rescan_schema(&project.key).await;
        }
    }

    /// Applies the stock limit of `key`'s repository (a ticket entered or left waiting to begin).
    async fn limit_changed(&self, key: &TicketKey) {
        if self.0.warm.mode() != HandoffMode::Move {
            return;
        }
        let Ok(project) = self.0.board.project(key.project.as_str()).await else {
            return;
        };
        let repo = repo_name(&project);
        if let Err(e) = self.0.stock.enforce_limit(&repo).await {
            tracing::warn!(warm_build = %repo, error = %e, "can't apply the stock limit");
        }
    }

    /// Applies the stock limit of every project's repository (after missed events).
    async fn all_limits_changed(&self) {
        if self.0.warm.mode() != HandoffMode::Move {
            return;
        }
        let Ok(projects) = self.0.board.projects().await else {
            return;
        };
        let repos: HashSet<String> = projects.iter().map(repo_name).collect();
        for repo in repos {
            if let Err(e) = self.0.stock.enforce_limit(&repo).await {
                tracing::warn!(warm_build = %repo, error = %e, "can't apply the stock limit");
            }
        }
    }

    /// The ids of the tickets whose status is `is` now.
    async fn ids_in(&self, is: impl Fn(Status) -> bool) -> HashSet<String> {
        let mut ids = HashSet::new();
        let Ok(projects) = self.0.board.projects().await else {
            return ids;
        };
        for project in projects {
            if let Ok(tickets) = self.0.board.tickets(&project.key).await {
                ids.extend(tickets.into_iter().filter(|t| is(t.status)).map(|t| t.id));
            }
        }
        ids
    }

    /// Listens to the service's events until `stop` is set, and sweeps at once and then on a slow tick
    /// ([`SWEEP_TICK`]).
    pub async fn run(self, mut stop: watch::Receiver<bool>) {
        let mut listener = self.0.events.listen();
        let mut tick = tokio::time::interval(SWEEP_TICK);
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        let mut tasks = JoinSet::new();
        // The tickets last seen waiting to begin (those there at startup too): leaving it changes the stock limit.
        let mut ready = self
            .ids_in(|s| s.set().board_group() == StatusSet::Waiting)
            .await;
        // The tickets last seen in [`FREED_IN`]: entering or leaving it scans their project's schema paths again.
        let mut in_review = self.ids_in(|s| FREED_IN.contains(&s)).await;
        loop {
            let mut lagged = false;
            tokio::select! {
                _ = stop.wait_for(|s| *s) => break,
                _ = tick.tick() => {
                    let env = self.clone();
                    tasks.spawn(async move { env.sweep().await });
                }
                Some(_) = tasks.join_next(), if !tasks.is_empty() => {}
                event = listener.receiver.recv() => match event {
                    Ok(Event::CardStatusChanged { id, key, status, .. }) => {
                        let reviewed = FREED_IN.contains(&status);
                        if reviewed {
                            let (env, key) = (self.clone(), key.clone());
                            tasks.spawn(async move {
                                env.free(&key).await;
                            });
                        }
                        let out_of_review = !reviewed && in_review.remove(&id);
                        if reviewed {
                            in_review.insert(id.clone());
                        }
                        if reviewed || out_of_review {
                            let (env, project) = (self.clone(), key.project.to_string());
                            tasks.spawn(async move { env.rescan_schema(&project).await });
                        }
                        let entered = status.set().board_group() == StatusSet::Waiting;
                        let left = !entered && ready.remove(&id);
                        if entered {
                            ready.insert(id);
                        }
                        if entered || left {
                            let env = self.clone();
                            tasks.spawn(async move { env.limit_changed(&key).await });
                        }
                    }
                    Ok(Event::SessionState { session })
                        if !session.is_running()
                            && matches!(session.kind, SessionKind::Run | SessionKind::Shell) =>
                    {
                        if let Some(key) = session.ticket {
                            let env = self.clone();
                            tasks.spawn(async move {
                                env.free(&key).await;
                            });
                        }
                    }
                    Ok(_) => {}
                    Err(RecvError::Lagged(missed)) => {
                        tracing::warn!(missed, "the ticket environment fell behind on events; sweeping");
                        lagged = true;
                    }
                    Err(RecvError::Closed) => break,
                },
            }
            if lagged {
                // What the missed events changed: the sets are read again, and every stock's limit applied (a
                // ticket that entered or left waiting to begin unseen changed it).
                ready = self
                    .ids_in(|s| s.set().board_group() == StatusSet::Waiting)
                    .await;
                in_review = self.ids_in(|s| FREED_IN.contains(&s)).await;
                let env = self.clone();
                tasks.spawn(async move {
                    env.sweep().await;
                    env.all_limits_changed().await;
                });
            }
        }
        // A Free under way finishes: it never leaves a build half moved.
        while tasks.join_next().await.is_some() {}
    }
}
