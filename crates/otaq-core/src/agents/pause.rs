//! **The pause** (PAU-01…PAU-03, PAU-06, PAU-07; d0112): the user's Pause stops every agent of a project at once,
//! and nothing starts an agent there on its own until the user's Resume. **A pause never lifts on its own.**
//!
//! - **The flag** is `projects.paused_since` (migration 0057), the time the user paused the project; `NULL` while it
//!   runs. The service stores it, so an app quit, a service restart and a reboot keep it; a project added later
//!   starts unpaused. Every change publishes [`Event::PauseChanged`] (and [`Event::ProjectsChanged`] for the rail).
//! - **How it was paused** is `projects.paused_by` (migration 0069, [`PauseKind`]; d0112 2026-10-27): on its own (This
//!   project) or by an All projects pause. An All projects pause leaves a project paused on its own as it is (its kind
//!   and `paused_since` stay); Resume all projects lifts only the All ones, Resume of one project lifts it whatever
//!   its kind.
//! - **Pausing** ([`Pause::pause`]) sets the flag **first**, then stops every live session of an agent kind in the
//!   project ([`SessionKind::runs_agent_tool`]: ticket agents, their answer and delivery sessions, checks, Project
//!   agents) with [`END_PAUSED`]; an end whose follow-up would start an agent finds the project
//!   paused already. Run, Shell and Project terminals keep going. Once every session ended, recovery takes their
//!   tickets the restart's way with the cause "paused" ([`Recovery::paused`]) and the Project agents
//!   become Paused ([`ProjectAgents::paused`]). Nothing is deleted. Pausing a paused project stops what runs (a
//!   ticket the user resumed alone) and keeps its `paused_since` and its kind.
//! - **Resume** ([`Pause::resume`]) lifts the flag (scope All: only where an All projects pause set it), then resumes
//!   what the pause stopped, each on its own
//!   conversation: the project's Interrupted tickets (Resume all's scope, [`resume_all_eligible`]; a check starts
//!   afresh) and its Paused Project agents, and hands back the conflicts the test merge flagged while
//!   paused ([`Review::hand_back_held`], d0127); then the starts kept while paused run
//!   ([`Pause::on_resume`]), and the scheduler wakes.
//! - **Stop agents and quit** (TRY-07, d0139 R5): [`Pause::hold`] stores an All projects pause first (the request
//!   answers only then), then [`Pause::stop_agents`] ends every agent session as Pause does and records what it
//!   stopped ([`StopRecord`], for the next launch's banner, PAU-07); the service stops after. Resume all forgets the
//!   record.
//! - **The counts** ([`Pause::counts`]) are what the Pause and Resume menus say ("shop-web · 5 working", "7 working in
//!   2 projects", the paused counts for Resume).
//!
//! Not "hold": that word is the launch gate's ([`super::gate`], table `agent_holds`). Agents can't pause or resume
//! (no `otaq` command; the requests are the user's only).

use std::collections::BTreeSet;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};

use crate::board::{Board, BoardError};
use crate::domain::Actor;
use crate::project_agents::ProjectAgents;
use crate::protocol::Event;
use crate::recovery::{Recovery, resume_all_eligible};
use crate::review::Review;
use crate::scheduler::Scheduler;
use crate::service::EventBus;
use crate::session::{END_PAUSED, SessionInfo, Sessions};
use crate::store::{Store, StoreError, now_ms};

/// What Pause, Resume and their counts are about: one project, or every project.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "scope", rename_all = "snake_case")]
pub enum PauseScope {
    /// "This project": the project's key ("SHOP").
    Project { project: String },
    /// "All projects".
    All,
}

/// How a paused project got paused (`projects.paused_by`, d0112 2026-10-27).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[serde(rename_all = "snake_case")]
#[sqlx(type_name = "TEXT", rename_all = "snake_case")]
pub enum PauseKind {
    /// By its own Pause (scope This project): only its own Resume lifts it.
    Own,
    /// By an All projects pause (or Stop agents and quit): Resume all projects lifts it, and so does its own Resume.
    All,
}

impl PauseKind {
    /// The kind a Pause of `scope` gives the projects it pauses.
    pub fn of(scope: &PauseScope) -> Self {
        match scope {
            PauseScope::Project { .. } => Self::Own,
            PauseScope::All => Self::All,
        }
    }
}

/// One project in the counts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectPause {
    /// The project's key ("SHOP").
    pub project: String,
    /// Its name ("shop-web").
    pub name: String,
    /// Since when it is paused; `None` while it runs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub paused_since: Option<i64>,
    /// How it was paused; `None` while it runs. Resume all projects leaves an [`PauseKind::Own`] pause as it is.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub paused_by: Option<PauseKind>,
    /// Agents working now: a ticket with live agent sessions counts once (its check, answer and delivery sessions
    /// included), each Project agent once. Pause stops them.
    pub working: u32,
    /// What Resume resumes: Interrupted tickets and Paused Project agents.
    pub paused: u32,
}

/// The counts of a scope (PAU-08's menus): "shop-web · 5 working", "7 working in 2 projects".
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PauseCounts {
    pub working: u32,
    pub paused: u32,
    /// The scope's projects, by name.
    pub projects: Vec<ProjectPause>,
}

impl PauseCounts {
    /// Whether an All projects pause holds any project now: Resume all projects has something to lift.
    pub fn all_pause_exists(&self) -> bool {
        self.projects
            .iter()
            .any(|p| p.paused_by == Some(PauseKind::All))
    }

    /// The projects paused on their own: Resume all projects leaves them paused.
    pub fn paused_on_their_own(&self) -> impl Iterator<Item = &ProjectPause> {
        self.projects
            .iter()
            .filter(|p| p.paused_by == Some(PauseKind::Own))
    }

    fn of(projects: Vec<ProjectPause>) -> Self {
        Self {
            working: projects.iter().map(|p| p.working).sum(),
            paused: projects.iter().map(|p| p.paused).sum(),
            projects,
        }
    }
}

/// What the last Stop agents and quit stopped (settings key [`STOP_AND_QUIT`]): the next launch's pause banner says
/// "Stop agents and quit stopped 7 agents in 2 projects yesterday at 18:02." Forgotten by Resume all projects.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct StopRecord {
    /// When it stopped them (Unix ms).
    pub at: i64,
    /// The agents working then (a ticket once, each Project agent once), and in how many projects.
    pub agents: u32,
    pub projects: u32,
}

/// The settings key of the last [`StopRecord`].
pub const STOP_AND_QUIT: &str = "pause.stop_and_quit";

/// The last Stop agents and quit's record, while Resume all hasn't lifted its pause.
pub async fn stop_record(store: &Store) -> Result<Option<StopRecord>, StoreError> {
    Ok(store
        .setting(STOP_AND_QUIT)
        .await?
        .and_then(|json| serde_json::from_str(&json).ok()))
}

/// The paused projects, by key, with since when.
pub async fn paused_projects(store: &Store) -> Result<Vec<(String, i64)>, StoreError> {
    Ok(sqlx::query_as(
        "SELECT key, paused_since FROM projects WHERE paused_since IS NOT NULL ORDER BY key",
    )
    .fetch_all(store.pool())
    .await?)
}

/// How the project `key` is paused now; `None` while it runs (or isn't a project).
pub async fn pause_kind(store: &Store, key: &str) -> Result<Option<PauseKind>, StoreError> {
    let kind: Option<(Option<PauseKind>,)> =
        sqlx::query_as("SELECT paused_by FROM projects WHERE key = ? AND paused_since IS NOT NULL")
            .bind(key)
            .fetch_optional(store.pool())
            .await?;
    Ok(kind.and_then(|(kind,)| kind))
}

/// Whether an All projects pause holds any project now (Resume all projects has something to lift; Stop agents and
/// quit stored one).
pub async fn all_pause_exists(store: &Store) -> Result<bool, StoreError> {
    let (any,): (bool,) = sqlx::query_as(
        "SELECT EXISTS (SELECT 1 FROM projects WHERE paused_since IS NOT NULL AND paused_by = 'all')",
    )
    .fetch_one(store.pool())
    .await?;
    Ok(any)
}

/// Whether the project `key` is paused now.
pub async fn is_paused(store: &Store, key: &str) -> Result<bool, StoreError> {
    let since: Option<(Option<i64>,)> =
        sqlx::query_as("SELECT paused_since FROM projects WHERE key = ?")
            .bind(key)
            .fetch_optional(store.pool())
            .await?;
    Ok(since.is_some_and(|(since,)| since.is_some()))
}

/// The name of the project `key` ("shop-web") while it is paused; `None` while it runs (or isn't a project: an agent
/// tool's Sign in… terminal has none).
pub async fn paused_name(store: &Store, key: &str) -> Result<Option<String>, StoreError> {
    if key.is_empty() {
        return Ok(None);
    }
    let name: Option<(String,)> =
        sqlx::query_as("SELECT name FROM projects WHERE key = ? AND paused_since IS NOT NULL")
            .bind(key)
            .fetch_optional(store.pool())
            .await?;
    Ok(name.map(|(name,)| name))
}

/// What an automatic start in a paused project is refused with (PAU-04), and Apply now: "shop-web is paused. Resume
/// to continue."
pub fn paused_refusal(name: &str) -> String {
    format!("{name} is paused. Resume to continue.")
}

/// Refused with [`paused_refusal`] while the project `key` is paused.
pub async fn refuse_while_paused(store: &Store, key: &str) -> Result<(), BoardError> {
    match paused_name(store, key).await? {
        Some(name) => Err(BoardError::refused(
            crate::board::Refusal::NotAllowed,
            paused_refusal(&name),
        )),
        None => Ok(()),
    }
}

/// What runs when a project resumes, after its stopped agents were resumed: the starts kept while it was paused
/// (T-0456). Given the project's key.
pub type ResumeHook = Arc<dyn Fn(String) -> Pin<Box<dyn Future<Output = ()> + Send>> + Send + Sync>;

/// Pause and Resume. Cheap to clone.
#[derive(Clone)]
pub struct Pause {
    inner: Arc<Inner>,
}

struct Inner {
    board: Board,
    sessions: Sessions,
    recovery: Recovery,
    review: Review,
    project_agents: ProjectAgents,
    scheduler: Scheduler,
    events: EventBus,
    on_resume: Mutex<Vec<ResumeHook>>,
}

/// What [`Pause::new`] takes.
pub struct PauseParts {
    pub board: Board,
    pub sessions: Sessions,
    pub recovery: Recovery,
    pub review: Review,
    pub project_agents: ProjectAgents,
    pub scheduler: Scheduler,
    pub events: EventBus,
}

impl Pause {
    pub fn new(parts: PauseParts) -> Self {
        Self {
            inner: Arc::new(Inner {
                board: parts.board,
                sessions: parts.sessions,
                recovery: parts.recovery,
                review: parts.review,
                project_agents: parts.project_agents,
                scheduler: parts.scheduler,
                events: parts.events,
                on_resume: Mutex::new(Vec::new()),
            }),
        }
    }

    /// Registers what runs when a project resumes, after its stopped agents (the kept starts, T-0456).
    pub fn on_resume(&self, hook: ResumeHook) {
        self.inner
            .on_resume
            .lock()
            .expect("resume hooks")
            .push(hook);
    }

    /// Whether the project `key` is paused now.
    pub async fn is_paused(&self, key: &str) -> Result<bool, StoreError> {
        is_paused(self.inner.board.store(), key).await
    }

    /// **Pause** (PAU-02): sets the flag of every project in `scope` first, with how it was paused ([`PauseKind::of`]
    /// the scope; a project paused already keeps its own), then stops every agent session there at once, then records
    /// what that left (PAU-03). The counts afterwards.
    pub async fn pause(&self, scope: &PauseScope) -> Result<PauseCounts, BoardError> {
        let keys = self.hold(scope).await?;
        let running: Vec<SessionInfo> = self
            .inner
            .sessions
            .in_memory()
            .into_iter()
            .filter(|s| s.is_running() && s.kind.runs_agent_tool() && keys.contains(&s.project))
            .collect();
        let stops = running.iter().map(|session| {
            let sessions = self.inner.sessions.clone();
            let id = session.id.clone();
            tokio::spawn(async move {
                if let Err(e) = sessions.stop_as(&id, END_PAUSED).await {
                    tracing::warn!(session = id, error = %e, "can't stop the agent");
                }
            })
        });
        for stop in stops.collect::<Vec<_>>() {
            let _ = stop.await;
        }
        let interrupted = self.inner.recovery.paused(&running).await?;
        let agents = self.inner.project_agents.paused(&running).await?;
        tracing::info!(
            sessions = running.len(),
            tickets = interrupted.len(),
            agents = agents.len(),
            "agents paused"
        );
        self.counts(scope).await
    }

    /// Stores the pause of every project in `scope` ([`PauseKind::of`] the scope; a project paused already keeps its
    /// kind and its time), and nothing else: from now on nothing starts an agent there on its own (PAU-04). The
    /// projects' keys.
    pub async fn hold(&self, scope: &PauseScope) -> Result<Vec<String>, BoardError> {
        let keys = self.keys(scope).await?;
        let now = now_ms();
        let kind = PauseKind::of(scope);
        for key in &keys {
            let set = sqlx::query(
                "UPDATE projects SET paused_since = ?, paused_by = ? WHERE key = ? AND paused_since IS NULL",
            )
            .bind(now)
            .bind(kind)
            .bind(key)
            .execute(self.inner.board.store().pool())
            .await?;
            if set.rows_affected() > 0 {
                tracing::warn!(project = %key, ?kind, "paused");
                self.changed(key, Some(now), Some(kind));
            }
        }
        Ok(keys)
    }

    /// **Stop agents and quit**'s stop (TRY-07), after [`Self::hold`] stored the All projects pause: ends every agent
    /// session in every project as Pause does (their tickets wait as Interrupted, Resume continues them) and records
    /// what it stopped for the next launch's banner. The service stops after (Run, Shell and Project terminals end
    /// with it).
    pub async fn stop_agents(&self) -> Result<StopRecord, BoardError> {
        let before = self.counts(&PauseScope::All).await?;
        let record = StopRecord {
            at: now_ms(),
            agents: before.working,
            projects: u32::try_from(before.projects.iter().filter(|p| p.working > 0).count())
                .unwrap_or(u32::MAX),
        };
        self.pause(&PauseScope::All).await?;
        let json = serde_json::to_value(record).unwrap_or_default().to_string();
        self.inner
            .board
            .store()
            .set_setting(STOP_AND_QUIT, &json)
            .await?;
        tracing::warn!(
            agents = record.agents,
            projects = record.projects,
            "agents stopped to quit"
        );
        Ok(record)
    }

    /// **Resume** (PAU-06): lifts the flag of every paused project in `scope`, then resumes what the pause stopped,
    /// each on its own conversation, runs the kept starts and wakes the scheduler. Scope All lifts only what an All
    /// projects pause set: a project paused on its own stays paused (d0112 2026-10-27); scope This project lifts the
    /// project however it was paused. A resume that fails stays as it is (its ticket shows why, as after Resume all)
    /// and the others go on. The counts afterwards.
    pub async fn resume(
        &self,
        scope: &PauseScope,
        actor: &Actor,
    ) -> Result<PauseCounts, BoardError> {
        let lift = match scope {
            PauseScope::Project { .. } => {
                "UPDATE projects SET paused_since = NULL, paused_by = NULL
                 WHERE key = ? AND paused_since IS NOT NULL"
            }
            PauseScope::All => {
                "UPDATE projects SET paused_since = NULL, paused_by = NULL
                 WHERE key = ? AND paused_since IS NOT NULL AND paused_by = 'all'"
            }
        };
        // The banner after a Stop agents and quit is about its pause, which Resume all lifts.
        if *scope == PauseScope::All {
            self.inner
                .board
                .store()
                .remove_setting(STOP_AND_QUIT)
                .await?;
        }
        let mut lifted = Vec::new();
        for key in self.keys(scope).await? {
            let done = sqlx::query(lift)
                .bind(&key)
                .execute(self.inner.board.store().pool())
                .await?;
            if done.rows_affected() > 0 {
                tracing::info!(project = %key, "resumed");
                self.changed(&key, None, None);
                lifted.push(key);
            }
        }
        // Only what a pause stopped: a project that wasn't paused keeps its tickets as they are.
        for key in &lifted {
            self.resume_stopped(key, actor).await?;
            let hooks = self.inner.on_resume.lock().expect("resume hooks").clone();
            for hook in hooks {
                hook(key.clone()).await;
            }
        }
        self.inner.scheduler.wake();
        self.counts(scope).await
    }

    /// The project's Interrupted tickets and Paused Project agents, resumed.
    async fn resume_stopped(&self, key: &str, actor: &Actor) -> Result<(), BoardError> {
        let tickets = self.inner.board.tickets(key).await?;
        for ticket in resume_all_eligible(&tickets) {
            if let Err(e) = self.inner.review.resume(&ticket.to_string(), actor).await {
                tracing::warn!(%ticket, error = %e, "can't resume the paused ticket");
            }
        }
        for id in self.inner.project_agents.paused_of(key).await? {
            if let Err(e) = self.inner.project_agents.resume(&id).await {
                tracing::warn!(agent = id, error = %e, "can't resume the paused agent");
            }
        }
        // Conflicts the test merge found while paused go to their agents now (d0127).
        if let Err(e) = self.inner.review.hand_back_held(key).await {
            tracing::warn!(project = key, error = %e, "can't hand the held conflicts to the agents");
        }
        Ok(())
    }

    /// The counts of `scope` now (PAU-08's menus).
    pub async fn counts(&self, scope: &PauseScope) -> Result<PauseCounts, BoardError> {
        let keys = self.keys(scope).await?;
        let running: Vec<SessionInfo> = self
            .inner
            .sessions
            .in_memory()
            .into_iter()
            .filter(|s| s.is_running() && s.kind.runs_agent_tool())
            .collect();
        let mut projects = Vec::new();
        for key in keys {
            let project = self.inner.board.project(&key).await?;
            // A ticket counts once, whatever sessions it runs; a Project agent once per session.
            let working: BTreeSet<String> = running
                .iter()
                .filter(|s| s.project == key)
                .map(|s| match &s.ticket {
                    Some(ticket) => ticket.to_string(),
                    None => s.id.clone(),
                })
                .collect();
            let tickets = self.inner.board.tickets(&key).await?;
            let paused = resume_all_eligible(&tickets).len()
                + self.inner.project_agents.paused_of(&key).await?.len();
            projects.push(ProjectPause {
                project: key,
                name: project.name,
                paused_since: project.paused_since,
                paused_by: project.paused_by,
                working: u32::try_from(working.len()).unwrap_or(u32::MAX),
                paused: u32::try_from(paused).unwrap_or(u32::MAX),
            });
        }
        projects.sort_by(|a, b| a.name.cmp(&b.name).then(a.project.cmp(&b.project)));
        Ok(PauseCounts::of(projects))
    }

    /// The paused projects with their counts (`ServiceStatus.paused`).
    pub async fn paused(&self) -> Result<Vec<ProjectPause>, BoardError> {
        let mut counts = self.counts(&PauseScope::All).await?;
        counts.projects.retain(|p| p.paused_since.is_some());
        Ok(counts.projects)
    }

    /// The projects' keys of `scope`: the one named (refused when it doesn't exist), or every project.
    async fn keys(&self, scope: &PauseScope) -> Result<Vec<String>, BoardError> {
        Ok(match scope {
            PauseScope::Project { project } => vec![self.inner.board.project(project).await?.key],
            PauseScope::All => self
                .inner
                .board
                .projects()
                .await?
                .into_iter()
                .map(|p| p.key)
                .collect(),
        })
    }

    fn changed(&self, key: &str, paused_since: Option<i64>, paused_by: Option<PauseKind>) {
        self.inner.events.publish(Event::PauseChanged {
            project: key.to_owned(),
            paused_since,
            paused_by,
        });
        self.inner.events.publish(Event::ProjectsChanged);
    }
}
