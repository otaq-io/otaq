//! **The pause** (PAU-01…PAU-03, PAU-06, PAU-07; d0112): the user's Pause stops every agent of a project at once,
//! and nothing starts an agent there on its own until the user's Resume. **A pause never lifts on its own.**
//!
//! - **The flag** is `projects.paused_since` (migration 0057), the time the user paused the project; `NULL` while it
//!   runs. The service stores it, so an app quit, a service restart and a reboot keep it; a project added later
//!   starts unpaused. Every change publishes [`Event::PauseChanged`] (and [`Event::ProjectsChanged`] for the rail).
//! - **Pausing** ([`Pause::pause`]) sets the flag **first**, then stops every live session of an agent kind in the
//!   project ([`SessionKind::runs_agent_tool`]: ticket agents, their answer and delivery sessions, checks, Project
//!   agents and epic leads) with [`END_PAUSED`]; an end whose follow-up would start an agent finds the project
//!   paused already. Run, Shell and Project terminals keep going. Once every session ended, recovery takes their
//!   tickets the restart's way with the cause "paused" ([`Recovery::paused`]) and the Project agents and leads
//!   become Paused ([`ProjectAgents::paused`]). Nothing is deleted. Pausing a paused project stops what runs (a
//!   ticket the user resumed alone) and keeps its `paused_since`.
//! - **Resume** ([`Pause::resume`]) lifts the flag, then resumes what the pause stopped, each on its own
//!   conversation: the project's Interrupted tickets (Resume all's scope, [`resume_all_eligible`]; a check starts
//!   afresh) and its Paused Project agents and leads, and hands back the conflicts the test merge flagged while
//!   paused ([`Review::hand_back_held`], d0127); then the starts kept while paused run
//!   ([`Pause::on_resume`]), and the scheduler wakes.
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
    /// Agents working now: a ticket with live agent sessions counts once (its check, answer and delivery sessions
    /// included), each Project agent and epic lead once. Pause stops them.
    pub working: u32,
    /// What Resume resumes: Interrupted tickets and Paused Project agents and epic leads.
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
    fn of(projects: Vec<ProjectPause>) -> Self {
        Self {
            working: projects.iter().map(|p| p.working).sum(),
            paused: projects.iter().map(|p| p.paused).sum(),
            projects,
        }
    }
}

/// The paused projects, by key, with since when.
pub async fn paused_projects(store: &Store) -> Result<Vec<(String, i64)>, StoreError> {
    Ok(sqlx::query_as(
        "SELECT key, paused_since FROM projects WHERE paused_since IS NOT NULL ORDER BY key",
    )
    .fetch_all(store.pool())
    .await?)
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

    /// **Pause** (PAU-02): sets the flag of every project in `scope` first, then stops every agent session there at
    /// once, then records what that left (PAU-03). The counts afterwards.
    pub async fn pause(&self, scope: &PauseScope) -> Result<PauseCounts, BoardError> {
        let keys = self.keys(scope).await?;
        let now = now_ms();
        for key in &keys {
            let set = sqlx::query(
                "UPDATE projects SET paused_since = ? WHERE key = ? AND paused_since IS NULL",
            )
            .bind(now)
            .bind(key)
            .execute(self.inner.board.store().pool())
            .await?;
            if set.rows_affected() > 0 {
                tracing::warn!(project = %key, "paused");
                self.changed(key, Some(now));
            }
        }
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

    /// **Resume** (PAU-06): lifts the flag of every paused project in `scope`, then resumes what the pause stopped,
    /// each on its own conversation, runs the kept starts and wakes the scheduler. A resume that fails stays as it is (its
    /// ticket shows why, as after Resume all) and the others go on. The counts afterwards.
    pub async fn resume(
        &self,
        scope: &PauseScope,
        actor: &Actor,
    ) -> Result<PauseCounts, BoardError> {
        let mut lifted = Vec::new();
        for key in self.keys(scope).await? {
            let done = sqlx::query(
                "UPDATE projects SET paused_since = NULL WHERE key = ? AND paused_since IS NOT NULL",
            )
            .bind(&key)
            .execute(self.inner.board.store().pool())
            .await?;
            if done.rows_affected() > 0 {
                tracing::info!(project = %key, "resumed");
                self.changed(&key, None);
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

    /// The project's Interrupted tickets and Paused Project agents and leads, resumed.
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
            // A ticket counts once, whatever sessions it runs; a Project agent or lead once per session.
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

    fn changed(&self, key: &str, paused_since: Option<i64>) {
        self.inner.events.publish(Event::PauseChanged {
            project: key.to_owned(),
            paused_since,
        });
        self.inner.events.publish(Event::ProjectsChanged);
    }
}
