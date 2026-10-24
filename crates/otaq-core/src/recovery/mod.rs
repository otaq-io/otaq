//! Failure and recovery (SDD §17; d0018, d0022, d0045): **detect and report, never self-heal**.
//!
//! - **Startup reconcile** (REC-01): when the service starts (after a reboot, a crash or a user-clicked restart),
//!   every ticket in a stage is compared with the session of that stage by [`reconcile::reconcile`], by the stage's
//!   kind (STG-09). A fresh service has no sessions yet, so they are all gone: tickets at work in an Agent stage become
//!   **Interrupted** (needs-you ones keep their label and question), tickets in an agent check are Interrupted and
//!   checked afresh on Resume, tickets in a human check or a click Delivery only get a History note. Run and Shell
//!   sessions end as "stopped by restart". Starts cut short are Start interrupted already ([`crate::start::Starter::interrupt_cut_short`]),
//!   so reconcile leaves them alone.
//! - **The agent tool exited** (d0018): a session that ends on its own while its ticket is at work in the stage it
//!   runs interrupts the ticket the same way. Sessions Otaq stopped itself (Park, Close, Done) don't.
//! - **Pause** (PAU-02, PAU-03; d0112): the sessions it ended ([`END_PAUSED`]) take their tickets the restart's way,
//!   with the cause "paused" ([`REASON_PAUSED`], [`Recovery::paused`]): one path, the same table.
//! - **Nothing resumes automatically.** Resume is the user's click (T-0112).
//!
//! Each service start is recorded in `service_runs`; a previous run without a clean stop means this start followed
//! a crash or a reboot (REC-06's summary).

pub mod cleanup;
pub mod reconcile;

use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::sync::{broadcast, watch};

use crate::agent_tools::{Adapters, Tool};
use crate::board::epic_flight::epic_flow;
use crate::board::tickets::{EventKind, Ticket, record_event};
use crate::board::{Board, BoardError};
use crate::caller::SessionStage;
use crate::domain::{Actor, StartStep, Status, TicketKey};
use crate::flow::Flow;
use crate::protocol::Event;
use crate::service::EventBus;
use crate::session::{
    END_MODEL_REFUSED, END_PAUSED, END_SERVICE_STOPPED, END_STOPPED, END_STOPPED_BY_RESTART,
    SessionInfo, SessionKind, Sessions,
};
use crate::store::tickets::set_state_since;
use crate::store::{Store, StoreError, new_id, now_ms};
pub use reconcile::{AT_WORK, Outcome, ResumeAs, StageAt, reconcile, resume_as};

/// What History records about a session that ended by itself.
fn ended_payload(info: &SessionInfo) -> serde_json::Value {
    let reason = info.end_reason.as_deref().unwrap_or_default();
    json!({ "reason": reason, "session": info.id, "tool": info.tool })
}

/// The flow a ticket walks: its project's, or for an epic the epic's own ([`epic_flow`]: its stop, e.g. Review
/// epic, then the Delivery).
fn walks(flow: Flow, ticket: &Ticket) -> Flow {
    if ticket.is_epic() {
        epic_flow(&flow)
    } else {
        flow
    }
}

/// The flows tickets walk, read once per project (and once more for its epics) for a startup reconcile.
#[derive(Default)]
struct Flows(std::collections::HashMap<(String, bool), Flow>);

impl Flows {
    async fn of(&mut self, board: &Board, ticket: &Ticket) -> Result<&Flow, BoardError> {
        let key = (ticket.key.project.to_string(), ticket.is_epic());
        if !self.0.contains_key(&key) {
            let flow = board.effective_flow(&key.0).await?;
            self.0.insert(key.clone(), walks(flow, ticket));
        }
        Ok(&self.0[&key])
    }
}

/// How many entries "Before the restart" shows by default.
pub const DEFAULT_ENTRIES: usize = 40;

/// "Before the restart" (REC-02; d0045, d0020): what the Terminal tab of an Interrupted ticket shows, read
/// **read-only** from the agent tool's own saved conversation ([`crate::agent_tools::AgentTool::last_entries`]). Otaq
/// never copies terminal output to disk. A change of the tool's format may break this view, never a workflow.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum BeforeRestart {
    /// The last messages and actions, oldest first.
    Available {
        /// The agent tool the conversation ran on.
        tool: Tool,
        tool_session_id: String,
        entries: Vec<TranscriptEntry>,
    },
    /// "Conversation no longer available": no stored tool session id, or the tool has removed (or can't give)
    /// the conversation.
    NotAvailable {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        tool: Option<Tool>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        tool_session_id: Option<String>,
    },
}

/// One line of "Before the restart".
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TranscriptEntry {
    /// The user's (or Otaq's) message to the agent.
    User {
        text: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        at: Option<i64>,
    },
    /// The agent's message.
    Assistant {
        text: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        at: Option<i64>,
    },
    /// A tool the agent ran: its name and a short summary ("Bash · pnpm test", "Edit · lib/pricing.ts").
    Action {
        tool: String,
        summary: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        at: Option<i64>,
    },
}

/// Puts `warm/` in order at start (d0082, d0085): a warm build from before the stock becomes entry `0`, and the
/// `<repo-name>.incoming/` and `<repo-name>.old/` folders and half-made entries a crash left are deleted. They are
/// Otaq's own temporary folders, not leftovers for the user to confirm (REC-04 is about worktrees, branches and
/// sessions). Returns the repos whose trash still holds something, for [`WarmBuilds::empty_trash_later`]: start
/// never waits for the trash.
///
/// [`WarmBuilds::empty_trash_later`]: crate::warm::store::WarmBuilds::empty_trash_later
pub async fn remove_interrupted_warm_work(data: &crate::data_dir::DataDir) -> Vec<String> {
    let data = data.clone();
    match tokio::task::spawn_blocking(move || crate::warm::store::remove_interrupted(&data)).await {
        Ok(found) => {
            if found.removed > 0 || found.migrated > 0 {
                tracing::info!(
                    removed = found.removed,
                    migrated = found.migrated,
                    "interrupted warm-build folders removed"
                );
            }
            found.trash
        }
        Err(_) => Vec::new(),
    }
}

/// "Before the restart" for a ticket (REC-02): the last `limit` messages and actions of its latest agent
/// conversation, read by the adapter of the tool it ran on.
/// For an epic it is its lead's conversation (LEAD-12, d0083: the Lead tab after a restart), which runs in the
/// project's folder. Read-only; never an error that blocks the ticket.
pub async fn before_restart(
    board: &Board,
    adapters: &Adapters,
    key: &str,
    limit: usize,
) -> Result<BeforeRestart, BoardError> {
    let ticket = board.ticket(key).await?;
    let latest: Option<(Tool, Option<String>)> = if ticket.is_epic() {
        let row: Option<(String, Option<String>)> = sqlx::query_as(
            "SELECT tool, tool_session_id FROM project_agents WHERE epic_id = ? LIMIT 1",
        )
        .bind(&ticket.id)
        .fetch_optional(board.store().pool())
        .await?;
        row.and_then(|(tool, id)| Some((tool.parse().ok()?, id)))
    } else {
        crate::session::StoredConversation::latest_agent(board.store().pool(), &ticket.id)
            .await?
            .map(|c| (c.tool, Some(c.id)))
    };
    let (tool, latest) = latest.unzip();
    let latest = latest.flatten();
    let (Some(tool), Some(session_id)) = (tool, latest.clone()) else {
        return Ok(BeforeRestart::NotAvailable {
            tool,
            tool_session_id: latest,
        });
    };
    let cwd = match ticket
        .work
        .as_ref()
        .and_then(|w| w.worktree.clone())
        .filter(|_| !ticket.is_epic())
    {
        Some(worktree) => std::path::PathBuf::from(worktree),
        None => {
            std::path::PathBuf::from(board.project(ticket.key.project.as_str()).await?.repo_path)
        }
    };
    let (adapters, id) = (adapters.clone(), session_id.clone());
    let entries =
        tokio::task::spawn_blocking(move || adapters.get(tool).last_entries(&id, &cwd, limit))
            .await
            .ok()
            .flatten();
    Ok(match entries {
        Some(entries) => BeforeRestart::Available {
            tool,
            tool_session_id: session_id,
            entries,
        },
        None => BeforeRestart::NotAvailable {
            tool: Some(tool),
            tool_session_id: Some(session_id),
        },
    })
}

/// Why a ticket was interrupted, as written in its History.
pub const REASON_RESTART: &str = "restart";
/// The user's Pause ended its session (PAU-03, d0112): "Paused" instead of "Interrupted by restart".
pub const REASON_PAUSED: &str = "paused";

/// Why reconcile runs: a service start, or a Pause.
#[derive(Debug, Clone, Copy)]
struct Why {
    /// The cause History records ([`REASON_RESTART`], [`REASON_PAUSED`]).
    reason: &'static str,
    /// The end the stage's sessions were given ([`END_SERVICE_STOPPED`], [`END_PAUSED`]), for the History note of a
    /// ticket that only waits for a click.
    end_reason: &'static str,
    /// Sessions started before this don't get a note (the previous run's start, for a restart).
    since: i64,
}

/// What reconcile did with one ticket.
enum Reconciled {
    Nothing,
    Interrupted(TicketKey),
    /// Interrupted from an agent check: Resume starts a fresh check.
    WasValidating(TicketKey),
    /// A ticket waiting for a click: a History note only.
    SessionEnded(TicketKey),
}

/// `tickets.return_mode`: the next start continues the stored conversation (Resume, Retry).
pub const RETURN_RESUME: &str = "resume";
/// `tickets.return_mode`: the next start is a new conversation in the same worktree (Start from scratch).
pub const RETURN_SCRATCH: &str = "scratch";

/// The first prompt of a ticket's conversation reopened by Resume, Resume all or Retry when nothing else was waiting
/// (REC-01, d0122): "You were interrupted by a restart. Carry on where you left off; run `otaq prime` if you need
/// the ticket again."
pub fn carry_on_text() -> String {
    format!(
        "You were interrupted by a restart. Carry on where you left off; run `{} prime` if you need the ticket \
         again.",
        crate::naming::COMMAND_NAME
    )
}

/// The first prompt of a Project agent or an epic lead resumed from "Ended by restart" (d0122, d0068, d0083).
pub const CARRY_ON_PROJECT_AGENT: &str =
    "You were interrupted by a restart. Carry on where you left off.";

/// The tickets Resume all resumes (d0045): Interrupted Dev tickets only, never Start interrupted, Resume failed,
/// Review or Approved.
pub fn resume_all_eligible<'a>(
    tickets: impl IntoIterator<Item = &'a crate::board::tickets::Ticket>,
) -> Vec<TicketKey> {
    let mut keys: Vec<TicketKey> = tickets
        .into_iter()
        .filter(|t| t.status == Status::Interrupted)
        .map(|t| t.key.clone())
        .collect();
    keys.sort();
    keys
}

/// The previous run of the service, as this start found it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PreviousRun {
    pub started_at: i64,
    /// It stopped cleanly (`otaq daemon stop`, a user-clicked restart). Otherwise it crashed or the machine
    /// rebooted.
    pub clean: bool,
}

/// This run of the service, recorded in `service_runs`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Run {
    pub id: String,
    pub previous: Option<PreviousRun>,
}

impl Run {
    /// Records the start of a run and reads the one before it.
    pub async fn begin(store: &Store, build_id: &str) -> Result<Self, StoreError> {
        let previous: Option<(i64, Option<i64>)> = sqlx::query_as(
            "SELECT started_at, stopped_at FROM service_runs ORDER BY started_at DESC, id DESC LIMIT 1",
        )
        .fetch_optional(store.pool())
        .await?;
        let id = new_id();
        sqlx::query("INSERT INTO service_runs (id, pid, build_id, started_at) VALUES (?, ?, ?, ?)")
            .bind(&id)
            .bind(i64::from(std::process::id()))
            .bind(build_id)
            .bind(now_ms())
            .execute(store.pool())
            .await?;
        Ok(Self {
            id,
            previous: previous.map(|(started_at, stopped_at)| PreviousRun {
                started_at,
                clean: stopped_at.is_some(),
            }),
        })
    }

    /// Records a clean stop.
    pub async fn end(&self, store: &Store) -> Result<(), StoreError> {
        sqlx::query("UPDATE service_runs SET stopped_at = ? WHERE id = ?")
            .bind(now_ms())
            .bind(&self.id)
            .execute(store.pool())
            .await?;
        Ok(())
    }
}

/// What this start found (REC-01) and the app summarizes (REC-06).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct StartupReport {
    /// The previous run ended without a clean stop: a crash or a reboot.
    pub unclean: bool,
    /// Tickets that became Interrupted.
    pub interrupted: Vec<TicketKey>,
    /// Ready for Review / Approved tickets whose session ended with the restart (History note only).
    pub session_ended: Vec<TicketKey>,
    /// Run, Shell and Project terminal sessions the restart stopped.
    pub stopped_by_restart: Vec<String>,
    /// Interrupted tickets that were validating: Resume starts a fresh validation (REC-01's M12 row, d0063).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub was_validating: Vec<TicketKey>,
    /// Project agents the restart ended, by id: "Ended by restart · Resume" (d0068).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub project_agents_ended: Vec<String>,
    /// "New agent"s that never got a request: gone after the restart.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub new_agents_discarded: u32,
    /// Epic leads the restart ended, by id: "Ended by restart · Resume", counted on their own ("1 epic lead
    /// ended", d0083). Not part of Resume all.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub leads_ended: Vec<String>,
    /// The projects that are paused (PAU-07, d0112), by key: the pause survived the restart, nothing of theirs
    /// resumes, and the app's reboot summary leaves them out.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub paused: Vec<String>,
}

fn is_zero(n: &u32) -> bool {
    *n == 0
}

/// Told about every Run / Shell / Project terminal session a restart stopped (M9 shows "stopped by restart").
pub type RestartHook = Arc<dyn Fn(&SessionInfo) + Send + Sync>;

/// Startup reconcile and agent exits. Cheap to clone.
#[derive(Clone)]
pub struct Recovery {
    inner: Arc<Inner>,
}

struct Inner {
    board: Board,
    sessions: Sessions,
    /// Reads a resumed session's output for the tool's "conversation not found" (REC-03).
    adapters: Adapters,
    events: EventBus,
    report: Mutex<StartupReport>,
    /// The Run / Shell / Project terminal sessions the last startup found stopped by the restart.
    stopped: Mutex<Vec<SessionInfo>>,
    restart_hooks: Mutex<Vec<RestartHook>>,
}

impl Recovery {
    pub fn new(board: Board, sessions: Sessions, adapters: Adapters, events: EventBus) -> Self {
        Self {
            inner: Arc::new(Inner {
                board,
                sessions,
                adapters,
                events,
                report: Mutex::new(StartupReport::default()),
                stopped: Mutex::new(Vec::new()),
                restart_hooks: Mutex::new(Vec::new()),
            }),
        }
    }

    /// Registers a hook for the Run / Shell / Project terminal sessions a restart stopped. A hook registered
    /// after [`startup`](Self::startup) is told about that startup's sessions at once.
    pub fn on_stopped_by_restart(&self, hook: RestartHook) {
        let stopped = self.inner.stopped.lock().expect("stopped sessions").clone();
        for info in &stopped {
            hook(info);
        }
        self.inner
            .restart_hooks
            .lock()
            .expect("restart hooks")
            .push(hook);
    }

    /// Adds what the Project agents' startup reconcile did to the report (REC-06, d0068).
    pub fn record_project_agents(&self, agents: &crate::project_agents::StartupAgents) {
        let mut report = self.inner.report.lock().expect("startup report");
        report.project_agents_ended = agents.ended.clone();
        report.new_agents_discarded = agents.discarded;
        report.leads_ended = agents.leads_ended.clone();
    }

    /// What the last [`startup`](Self::startup) found.
    pub fn startup_report(&self) -> StartupReport {
        self.inner.report.lock().expect("startup report").clone()
    }

    /// The startup reconcile (REC-01). Runs once, when the service starts, after the previous run's sessions were
    /// recorded as ended.
    pub async fn startup(
        &self,
        previous: Option<PreviousRun>,
    ) -> Result<StartupReport, BoardError> {
        let inner = &self.inner;
        let pool = inner.board.store().pool();
        let since = previous.map_or(0, |p| p.started_at);
        let mut report = StartupReport {
            unclean: previous.is_some_and(|p| !p.clean),
            // A pause never lifts on its own (PAU-07): read, never cleared here.
            paused: crate::agents::pause::paused_projects(inner.board.store())
                .await?
                .into_iter()
                .map(|(key, _)| key)
                .collect(),
            ..Default::default()
        };

        // Every ticket whose stage's session can be gone, by the kind of the stage it is in (STG-09).
        let watched: Vec<&str> = reconcile::watched().map(|s| s.as_str()).collect();
        let sql = format!(
            "SELECT id FROM tickets WHERE state IN ({}) ORDER BY rowid",
            vec!["?"; watched.len()].join(", ")
        );
        let mut query = sqlx::query_as::<_, (String,)>(sqlx::AssertSqlSafe(sql));
        for state in &watched {
            query = query.bind(*state);
        }
        let mut flows = Flows::default();
        let why = Why {
            reason: REASON_RESTART,
            end_reason: END_SERVICE_STOPPED,
            since,
        };
        for (id,) in query.fetch_all(pool).await? {
            let ticket = inner.board.ticket_by_id(&id).await?;
            let flow = flows.of(&inner.board, &ticket).await?;
            match self.reconcile_ticket(&ticket, flow, why).await? {
                Reconciled::Nothing => {}
                Reconciled::Interrupted(key) => report.interrupted.push(key),
                Reconciled::WasValidating(key) => {
                    report.was_validating.push(key.clone());
                    report.interrupted.push(key);
                }
                Reconciled::SessionEnded(key) => report.session_ended.push(key),
            }
        }

        let stopped: Vec<(String,)> = sqlx::query_as(
            "SELECT id FROM sessions WHERE kind IN ('run', 'shell', 'project') AND end_reason = ?
             AND started_at >= ? ORDER BY started_at, id",
        )
        .bind(END_STOPPED_BY_RESTART)
        .bind(since)
        .fetch_all(pool)
        .await?;
        let hooks = inner.restart_hooks.lock().expect("restart hooks").clone();
        let mut infos = Vec::new();
        for (session,) in stopped {
            if let Ok(info) = inner.sessions.stored(&session).await {
                for hook in &hooks {
                    hook(&info);
                }
                infos.push(info);
            }
            report.stopped_by_restart.push(session);
        }
        *inner.stopped.lock().expect("stopped sessions") = infos;

        report.interrupted.sort();
        report.session_ended.sort();
        report.was_validating.sort();
        if !report.interrupted.is_empty() || !report.session_ended.is_empty() {
            tracing::warn!(
                unclean = report.unclean,
                interrupted = report.interrupted.len(),
                session_ended = report.session_ended.len(),
                "startup reconcile"
            );
        }
        *inner.report.lock().expect("startup report") = report.clone();
        Ok(report)
    }

    /// One ticket through REC-01's table (STG-09), by the kind of the stage it is in and whether that stage's
    /// session runs: a restart's reconcile and a Pause's are the same path, only `why` differs.
    async fn reconcile_ticket(
        &self,
        ticket: &Ticket,
        flow: &Flow,
        why: Why,
    ) -> Result<Reconciled, BoardError> {
        let id = ticket.id.as_str();
        let from = self.interrupted_from(ticket).await?;
        let alive = self.stage_session_alive(flow, ticket, from);
        let reason = json!({ "reason": why.reason });
        let done = match reconcile(flow, &ticket.place, ticket.status, from, alive) {
            Outcome::Unchanged => None,
            Outcome::Interrupt { keep_label } => self
                .interrupt(id, ticket.status, keep_label, reason)
                .await?
                .map(Reconciled::Interrupted),
            Outcome::InterruptIn { stage } => self
                .interrupt_as(id, ticket.status, None, Some(&stage), false, reason)
                .await?
                .map(Reconciled::Interrupted),
            Outcome::NoteSessionEnded => self
                .note_session_ended(id, why)
                .await?
                .map(Reconciled::SessionEnded),
            // The checker is gone (REC-01's M12 row): Resume starts a fresh check.
            Outcome::InterruptValidation => self
                .interrupt_validation(id, ticket.status, reason)
                .await?
                .map(Reconciled::WasValidating),
            // The checker asked the user: the question stays in Needs you, the round isn't counted.
            Outcome::InterruptAsking => self
                .interrupt_asking(id, ticket.status, reason)
                .await?
                .map(Reconciled::Interrupted),
        };
        Ok(done.unwrap_or(Reconciled::Nothing))
    }

    /// The tickets of the sessions a **Pause** ended ([`END_PAUSED`]; PAU-03, d0112), the restart's way with the
    /// cause "paused": a working ticket becomes Interrupted ("Paused"), a needs-you one keeps its label and question,
    /// a check ends interrupted and its round doesn't count, an automatic Delivery is interrupted in the Agent stage
    /// that runs it, a ticket waiting for a click gets a History note. Project agents and leads are their own
    /// ([`crate::project_agents::AgentState::Paused`]). Called once every session has ended; the tickets that
    /// became Interrupted, in key order.
    pub async fn paused(&self, stopped: &[SessionInfo]) -> Result<Vec<TicketKey>, BoardError> {
        let mut keys: Vec<&TicketKey> = stopped
            .iter()
            .filter(|s| s.kind != SessionKind::ProjectAgent)
            .filter_map(|s| s.ticket.as_ref())
            .collect();
        keys.sort();
        keys.dedup();
        let why = Why {
            reason: REASON_PAUSED,
            end_reason: END_PAUSED,
            since: 0,
        };
        let mut flows = Flows::default();
        let mut interrupted = Vec::new();
        for key in keys {
            let ticket = match self.inner.board.ticket(&key.to_string()).await {
                Ok(ticket) => ticket,
                Err(BoardError::NotFound(_)) => continue,
                Err(e) => return Err(e),
            };
            let flow = flows.of(&self.inner.board, &ticket).await?;
            match self.reconcile_ticket(&ticket, flow, why).await? {
                Reconciled::Interrupted(key) | Reconciled::WasValidating(key) => {
                    interrupted.push(key);
                }
                Reconciled::Nothing | Reconciled::SessionEnded(_) => {}
            }
        }
        if !interrupted.is_empty() {
            tracing::info!(interrupted = interrupted.len(), "paused tickets");
        }
        Ok(interrupted)
    }

    /// A start Otaq kept because the ticket's project is paused (PAU-04, d0112): the ticket is taken the pause's way
    /// ([`Self::paused`]) though no session ran, so a checker that would have started leaves it interrupted from its
    /// check (the round not counted) and the project's Resume starts a fresh one. Its key when it became Interrupted.
    pub async fn kept_paused(&self, ticket_id: &str) -> Result<Option<TicketKey>, BoardError> {
        let ticket = self.inner.board.ticket_by_id(ticket_id).await?;
        let why = Why {
            reason: REASON_PAUSED,
            end_reason: END_PAUSED,
            since: 0,
        };
        let mut flows = Flows::default();
        let flow = flows.of(&self.inner.board, &ticket).await?;
        Ok(match self.reconcile_ticket(&ticket, flow, why).await? {
            Reconciled::Interrupted(key) | Reconciled::WasValidating(key) => Some(key),
            Reconciled::Nothing | Reconciled::SessionEnded(_) => None,
        })
    }

    /// An agent session ended while the service runs. If the agent tool exited on its own (not stopped by Otaq)
    /// while its ticket was at work in the Agent stage this session runs (or the automatic Delivery it delivers),
    /// the ticket is Interrupted (d0018, STG-09). A session that ran an agent check is its checker
    /// ([`Self::validator_session_ended`]). Returns the ticket's key when it was.
    pub async fn agent_session_ended(
        &self,
        info: &SessionInfo,
    ) -> Result<Option<TicketKey>, BoardError> {
        let Some((ticket, flow, checker)) = self.ended_by_itself(info).await? else {
            return Ok(None);
        };
        if checker {
            return Ok(None);
        }
        // A newer session of the ticket runs (a resume started meanwhile): this end changes nothing.
        if let Some(running) = self
            .inner
            .sessions
            .running_of(&ticket.id, SessionKind::Agent)
            && running.id != info.id
        {
            return Ok(None);
        }
        let at_work = matches!(
            StageAt::find(&flow, &ticket.place, ticket.status, None),
            Some(StageAt::Agent)
        ) && AT_WORK.contains(&ticket.status);
        // A resume whose tool can't find the conversation: Resume failed, never a fallback (REC-03).
        let output = self
            .inner
            .sessions
            .attach(&info.id)
            .map(|a| a.scrollback)
            .unwrap_or_default();
        if at_work
            && self
                .inner
                .adapters
                .of_session(info.tool)
                .conversation_not_found(&output)
        {
            return self.resume_failed(&ticket.id, ticket.status, info).await;
        }
        // The tool reports its conversation id after start and the agent ended before it did (Codex, ADP-07):
        // there is nothing to resume, so the start failed (Retry start begins a new conversation).
        if at_work
            && let Some(tool) = info.tool
            && info.tool_session_id.as_deref().is_none_or(str::is_empty)
        {
            return self
                .start_failed(&ticket.id, ticket.status, tool, info)
                .await;
        }
        // A card that moved on from the ended session's stage: only the session that runs its stage counts (STG-09).
        // An automatic PR Delivery run by the Delivery's own agent isn't Dev's leftover session's to interrupt.
        if matches!(
            StageAt::find(&flow, &ticket.place, ticket.status, None),
            Some(StageAt::Delivery { .. })
        ) && self.inner.board.delivery_session_kind(&ticket).await != SessionKind::Agent
        {
            tracing::info!(ticket = %ticket.key, session = info.id, "an earlier stage's session ended: nothing changes");
            return Ok(None);
        }
        let payload = ended_payload(info);
        match reconcile(&flow, &ticket.place, ticket.status, None, false) {
            Outcome::Interrupt { keep_label } => {
                self.interrupt(&ticket.id, ticket.status, keep_label, payload)
                    .await
            }
            Outcome::InterruptIn { stage } => {
                self.interrupt_as(
                    &ticket.id,
                    ticket.status,
                    None,
                    Some(&stage),
                    false,
                    payload,
                )
                .await
            }
            // The checker's session decides an agent check; a human check and a Delivery wait for a click.
            Outcome::Unchanged
            | Outcome::NoteSessionEnded
            | Outcome::InterruptValidation
            | Outcome::InterruptAsking => Ok(None),
        }
    }

    /// A checker session (a validator, or any session that ran an agent check) ended while the service runs, not
    /// stopped by Otaq (a crash, the tool exiting): its checking ticket is Interrupted from Checking (VAL-07,
    /// STG-09).
    pub async fn validator_session_ended(
        &self,
        info: &SessionInfo,
    ) -> Result<Option<TicketKey>, BoardError> {
        let Some((ticket, flow, checker)) = self.ended_by_itself(info).await? else {
            return Ok(None);
        };
        if !checker {
            return Ok(None);
        }
        let own = self
            .inner
            .board
            .open_round(&ticket.id)
            .await?
            .and_then(|(_, _, session, _)| session);
        if own.as_deref() != Some(info.id.as_str()) {
            return Ok(None);
        }
        let from = self.interrupted_from(&ticket).await?;
        match reconcile(&flow, &ticket.place, ticket.status, from, false) {
            Outcome::InterruptValidation => {
                self.interrupt_validation(&ticket.id, ticket.status, ended_payload(info))
                    .await
            }
            _ => Ok(None),
        }
    }

    /// A ticket's agent or checker session that ended by itself (not stopped by Otaq): its ticket, the ticket's
    /// flow, and whether the session is the checker (its stage run's kind, else its own kind; STG-10).
    async fn ended_by_itself(
        &self,
        info: &SessionInfo,
    ) -> Result<Option<(Ticket, Flow, bool)>, BoardError> {
        if !matches!(info.kind, SessionKind::Agent | SessionKind::Validator) || info.is_running() {
            return Ok(None);
        }
        let reason = info.end_reason.as_deref().unwrap_or_default();
        // Ended by Otaq on a refused model: its ticket's state is already set (AGT-05). A Pause's ends go the
        // restart's way once they all ended ([`Self::paused`]).
        if reason == END_STOPPED
            || reason == END_SERVICE_STOPPED
            || reason == END_MODEL_REFUSED
            || reason == END_PAUSED
        {
            return Ok(None);
        }
        let Some(key) = &info.ticket else {
            return Ok(None);
        };
        let ticket = match self.inner.board.ticket(&key.to_string()).await {
            Ok(ticket) => ticket,
            Err(BoardError::NotFound(_)) => return Ok(None),
            Err(e) => return Err(e),
        };
        let checker = match SessionStage::of(self.inner.board.store().pool(), &info.id).await? {
            Some(stage) if stage.is_check() => true,
            Some(stage) if stage.is_agent_stage() => false,
            _ => info.kind == SessionKind::Validator,
        };
        let flow = self
            .inner
            .board
            .effective_flow(ticket.key.project.as_str())
            .await?;
        let flow = walks(flow, &ticket);
        Ok(Some((ticket, flow, checker)))
    }

    /// The status a ticket's wait stands for (`interrupted_from`, as stored): a check's, for a checker waiting for
    /// the plan limit.
    async fn interrupted_from(&self, ticket: &Ticket) -> Result<Option<Status>, BoardError> {
        if ticket.interrupted_from.is_some() {
            return Ok(ticket.interrupted_from);
        }
        let row: Option<(Option<String>,)> =
            sqlx::query_as("SELECT interrupted_from FROM tickets WHERE id = ?")
                .bind(&ticket.id)
                .fetch_optional(self.inner.board.store().pool())
                .await?;
        Ok(row
            .and_then(|(from,)| from)
            .and_then(|from| from.parse().ok()))
    }

    /// Whether the session of the stage the ticket is in runs: the checker's in an agent check, the ticket
    /// agent's elsewhere (an automatic Delivery's too).
    fn stage_session_alive(&self, flow: &Flow, ticket: &Ticket, from: Option<Status>) -> bool {
        let sessions = &self.inner.sessions;
        match StageAt::find(flow, &ticket.place, ticket.status, from) {
            Some(StageAt::AgentCheck) => sessions
                .running_of(&ticket.id, SessionKind::Validator)
                .is_some(),
            _ => sessions
                .running_of(&ticket.id, SessionKind::Agent)
                .is_some(),
        }
    }

    /// Interrupted from Validating: the round ends "interrupted" (not counted); Resume starts a fresh one.
    async fn interrupt_validation(
        &self,
        ticket_id: &str,
        before: Status,
        mut payload: serde_json::Value,
    ) -> Result<Option<TicketKey>, BoardError> {
        payload["validation"] = json!(true);
        let key = self
            .interrupt_as(
                ticket_id,
                before,
                Some(Status::Checking),
                None,
                false,
                payload,
            )
            .await?;
        if key.is_some() {
            self.inner
                .board
                .abandon_round(ticket_id, crate::domain::ValidationOutcome::Interrupted)
                .await?;
        }
        Ok(key)
    }

    /// Interrupted keeping the checker's question (d0045, d0119): the round ends "interrupted" (not counted);
    /// Resume starts a fresh check.
    async fn interrupt_asking(
        &self,
        ticket_id: &str,
        before: Status,
        mut payload: serde_json::Value,
    ) -> Result<Option<TicketKey>, BoardError> {
        payload["validation"] = json!(true);
        let key = self.interrupt(ticket_id, before, true, payload).await?;
        if key.is_some() {
            self.inner
                .board
                .abandon_round(ticket_id, crate::domain::ValidationOutcome::Interrupted)
                .await?;
        }
        Ok(key)
    }

    /// Watches session ends until the service stops (agent exits, d0018).
    pub async fn run(self, mut stop: watch::Receiver<bool>) {
        let mut listener = self.inner.events.listen();
        loop {
            let event = tokio::select! {
                _ = stop.wait_for(|s| *s) => return,
                event = listener.receiver.recv() => event,
            };
            match event {
                Ok(Event::SessionState { session }) if !session.is_running() => {
                    if let Err(e) = self.agent_session_ended(&session).await {
                        tracing::warn!(session = session.id, error = %e, "can't record an agent's exit");
                    }
                    if let Err(e) = self.validator_session_ended(&session).await {
                        tracing::warn!(session = session.id, error = %e, "can't record a validator's exit");
                    }
                }
                Ok(_) => {}
                Err(broadcast::error::RecvError::Lagged(missed)) => {
                    tracing::warn!(missed, "recovery fell behind on session events");
                }
                Err(broadcast::error::RecvError::Closed) => return,
            }
        }
    }

    /// Sets a ticket at work to Interrupted, if it still is in `before`. A needs-you ticket keeps its label (the
    /// status it had), its text and its question.
    async fn interrupt(
        &self,
        ticket_id: &str,
        before: Status,
        keep_label: bool,
        payload: serde_json::Value,
    ) -> Result<Option<TicketKey>, BoardError> {
        let from = keep_label.then_some(before);
        self.interrupt_as(ticket_id, before, from, None, keep_label, payload)
            .await
    }

    /// Interrupted, remembering `from` (the needs-you status it kept, or Validating) and keeping the text when
    /// `keep_label`; in `stage` when given (the Agent stage that runs an automatic Delivery), else where it is.
    async fn interrupt_as(
        &self,
        ticket_id: &str,
        before: Status,
        from: Option<Status>,
        stage: Option<&str>,
        keep_label: bool,
        mut payload: serde_json::Value,
    ) -> Result<Option<TicketKey>, BoardError> {
        let board = &self.inner.board;
        let now = now_ms();
        let mut tx = board.store().begin_write().await?;
        let done = sqlx::query(concat!(
            "UPDATE tickets SET stage_id = COALESCE(?, stage_id), state = ?, interrupted_from = ?,
                                signal_text = CASE WHEN ? THEN signal_text ELSE NULL END,
                                signal_since = CASE WHEN ? THEN signal_since ELSE NULL END,
                                wakeup_at = NULL, updated_at = ?, ",
            set_state_since!(),
            " WHERE id = ? AND state = ?"
        ))
        .bind(stage)
        .bind(Status::Interrupted.as_str())
        .bind(from.map(|s| s.as_str()))
        .bind(keep_label)
        .bind(keep_label)
        .bind(now)
        .bind(Status::Interrupted.as_str())
        .bind(now)
        .bind(ticket_id)
        .bind(before.as_str())
        .execute(&mut *tx)
        .await?;
        if done.rows_affected() == 0 {
            return Ok(None);
        }
        payload["from"] = json!(before);
        record_event(
            &mut tx,
            ticket_id,
            EventKind::Interrupted,
            &Actor::current_user(),
            payload,
        )
        .await?;
        tx.commit().await?;
        let ticket = board.ticket_by_id(ticket_id).await?;
        tracing::info!(ticket = %ticket.key, from = %before, "interrupted");
        self.publish(&ticket.id, &ticket.key, Some(Status::Interrupted));
        Ok(Some(ticket.key))
    }

    /// The agent ended before its tool said its conversation's id: Start interrupted at the launch, with the reason;
    /// Retry start runs the start again.
    async fn start_failed(
        &self,
        ticket_id: &str,
        before: Status,
        tool: Tool,
        info: &SessionInfo,
    ) -> Result<Option<TicketKey>, BoardError> {
        let board = &self.inner.board;
        let message = format!(
            "{} stopped before its conversation started, so there is nothing to resume.",
            self.inner.adapters.get(tool).display_name()
        );
        let now = now_ms();
        let mut tx = board.store().begin_write().await?;
        let done = sqlx::query(concat!(
            "UPDATE tickets SET stage_id = NULL, state = ?, start_problem_step = ?, start_problem_message = ?,
                                signal_text = NULL, signal_since = NULL, wakeup_at = NULL, interrupted_from = NULL,
                                updated_at = ?, ",
            set_state_since!(),
            " WHERE id = ? AND state = ?"
        ))
        .bind(Status::StartInterrupted.as_str())
        .bind(StartStep::Launch.as_str())
        .bind(&message)
        .bind(now)
        .bind(Status::StartInterrupted.as_str())
        .bind(now)
        .bind(ticket_id)
        .bind(before.as_str())
        .execute(&mut *tx)
        .await?;
        if done.rows_affected() == 0 {
            return Ok(None);
        }
        record_event(
            &mut tx,
            ticket_id,
            EventKind::StartInterrupted,
            &Actor::current_user(),
            json!({ "step": StartStep::Launch, "message": message, "session": info.id, "from": before }),
        )
        .await?;
        tx.commit().await?;
        let ticket = board.ticket_by_id(ticket_id).await?;
        tracing::warn!(ticket = %ticket.key, "the agent ended before its tool id was known: start failed");
        self.publish(&ticket.id, &ticket.key, Some(Status::StartInterrupted));
        Ok(Some(ticket.key))
    }

    /// The resumed conversation wasn't found: Resume failed, with Retry and Start from scratch.
    async fn resume_failed(
        &self,
        ticket_id: &str,
        before: Status,
        info: &SessionInfo,
    ) -> Result<Option<TicketKey>, BoardError> {
        let board = &self.inner.board;
        let now = now_ms();
        let mut tx = board.store().begin_write().await?;
        let done = sqlx::query(concat!(
            "UPDATE tickets SET state = ?, wakeup_at = NULL, updated_at = ?, ",
            set_state_since!(),
            " WHERE id = ? AND state = ?"
        ))
        .bind(Status::ResumeFailed.as_str())
        .bind(now)
        .bind(Status::ResumeFailed.as_str())
        .bind(now)
        .bind(ticket_id)
        .bind(before.as_str())
        .execute(&mut *tx)
        .await?;
        if done.rows_affected() == 0 {
            return Ok(None);
        }
        record_event(
            &mut tx,
            ticket_id,
            EventKind::ResumeFailed,
            &Actor::current_user(),
            json!({
                "session": info.id,
                "tool": info.tool,
                "tool_session_id": info.tool_session_id,
            }),
        )
        .await?;
        tx.commit().await?;
        let ticket = board.ticket_by_id(ticket_id).await?;
        tracing::warn!(ticket = %ticket.key, "resume failed: conversation not found");
        self.publish(&ticket.id, &ticket.key, Some(Status::ResumeFailed));
        Ok(Some(ticket.key))
    }

    /// History notes "session ended (restart)" ("(paused)" after a Pause) for a Review / Approved ticket whose agent
    /// session the previous run (or the Pause) ended (once).
    async fn note_session_ended(
        &self,
        ticket_id: &str,
        why: Why,
    ) -> Result<Option<TicketKey>, BoardError> {
        let board = &self.inner.board;
        let ended: Option<(String,)> = sqlx::query_as(
            "SELECT id FROM sessions WHERE ticket_id = ? AND kind = 'agent' AND end_reason = ? AND started_at >= ?
             AND NOT EXISTS (SELECT 1 FROM events e WHERE e.ticket_id = sessions.ticket_id AND e.kind = ?
                             AND e.at >= sessions.ended_at)
             ORDER BY started_at DESC LIMIT 1",
        )
        .bind(ticket_id)
        .bind(why.end_reason)
        .bind(why.since)
        .bind(EventKind::SessionEnded.as_str())
        .fetch_optional(board.store().pool())
        .await?;
        let Some((session,)) = ended else {
            return Ok(None);
        };
        let mut tx = board.store().begin_write().await?;
        record_event(
            &mut tx,
            ticket_id,
            EventKind::SessionEnded,
            &Actor::current_user(),
            json!({ "reason": why.reason, "session": session }),
        )
        .await?;
        tx.commit().await?;
        let ticket = board.ticket_by_id(ticket_id).await?;
        self.publish(&ticket.id, &ticket.key, None);
        Ok(Some(ticket.key))
    }

    fn publish(&self, id: &str, key: &TicketKey, new_status: Option<Status>) {
        let project = key.project.to_string();
        self.inner.events.publish(Event::TicketChanged {
            project: project.clone(),
            id: id.to_owned(),
            key: key.clone(),
        });
        if let Some(status) = new_status {
            self.inner.events.publish(Event::CardStatusChanged {
                project,
                id: id.to_owned(),
                key: key.clone(),
                status,
            });
        }
    }
}
