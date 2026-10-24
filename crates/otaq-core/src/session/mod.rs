//! The PTY session manager (SES-04, SVC-01, SVC-05; d0022).
//!
//! The background service owns every PTY: agent sessions now, Shell and the Project terminal later (M9). A
//! session is a program running in a PTY with a cwd and environment. Its output is appended to a bounded
//! in-memory [`Scrollback`] and sent live to every attached client; a client that (re)attaches gets the stored
//! scrollback first, then live output, so it can redraw its terminal (SVC-05). Output never goes to disk
//! (d0045).
//!
//! Each spawn writes a `sessions` row (kind, state, started/ended, end reason, the agent tool, its own session id,
//! the model as launched and the named agent it runs on; d0088); every row is kept (d0045). One agent session per
//! ticket (SES-04). The exact model that ran is written on the row when the tool reports it
//! ([`Sessions::model_known`]).
//!
//! Threads: each session has a reader thread (PTY → scrollback and attached clients) and a waiter thread (child
//! exit → the row, a `session_state` event). Sessions end when their program exits, when Otaq stops them,
//! or when the service stops; the next service start reconciles their tickets ([`crate::recovery`]).

pub mod colors;
pub mod deliveries;
pub mod env;
pub mod hand_offs;
pub mod input;
pub mod screen;
pub mod scrollback;

use std::collections::HashMap;
use std::fmt;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use portable_pty::{ChildKiller, CommandBuilder, MasterPty, PtySize, native_pty_system};
use serde::{Deserialize, Serialize};
use tokio::sync::{broadcast, watch};

use crate::agent_tools::{PrepareStep, Tool};
use crate::attention::{ClientId, Focus};
use crate::board::settings::{TerminalColors, Theme, ThemeSetting};
use crate::domain::{TicketKey, stored_enum};
use crate::protocol::Event;
use crate::service::EventBus;
use crate::store::{Store, StoreError, new_id, now_ms};
pub use input::{InputEffect, InputTracker};
pub use scrollback::Scrollback;

stored_enum! {
    /// What runs in a session (the `sessions.kind` column).
    pub enum SessionKind ("session kind") {
        /// A ticket's agent session.
        Agent = "agent",
        /// A finished ticket resumed to answer another agent's question (M10).
        Answer = "answer",
        /// The short-lived session that opens an epic's PR (M10).
        Delivery = "delivery",
        /// A ticket's Shell tab (M9).
        Shell = "shell",
        /// The Project terminal (M9).
        Project = "project",
        /// A Run command (M9).
        Run = "run",
        /// The read-only judge of a ticket's submit (d0063): a fresh agent session in the ticket's worktree.
        Validator = "validator",
        /// A Project agent (d0068): an agent session in the project's folder, for no ticket.
        ProjectAgent = "project_agent",
    }
}

impl SessionKind {
    /// Whether the session runs an agent tool, whichever tool it is. Processes inside it are that ticket's agent,
    /// or the Project agent (CLI-01); Shell, Project terminal and Run sessions are the user's own terminals
    /// (CLI-03).
    pub fn runs_agent_tool(self) -> bool {
        matches!(
            self,
            Self::Agent | Self::Answer | Self::Delivery | Self::Validator | Self::ProjectAgent
        )
    }

    /// Run, Shell and the Project terminal: the user's own processes, stopped by a restart and never restarted
    /// on their own.
    pub fn is_users_terminal(self) -> bool {
        matches!(self, Self::Run | Self::Shell | Self::Project)
    }
}

stored_enum! {
    pub enum SessionState ("session state") {
        Running = "running",
        Ended = "ended",
    }
}

/// Why sessions end, as stored in `end_reason` (besides "exit N" and "signal SIGNAME").
pub const END_STOPPED: &str = "stopped";
pub const END_SERVICE_STOPPED: &str = "service stopped";
/// Run, Shell and Project terminal sessions ended by the service stopping: shown as "stopped by restart" and never
/// started again on their own (REC-01, d0035, d0045).
pub const END_STOPPED_BY_RESTART: &str = "stopped by restart";
/// A session Otaq ended because the tool refused its model on the first turn (AGT-05, d0088): it counts as not
/// started (its row unlocks) and its conversation isn't kept for Resume.
pub const END_MODEL_REFUSED: &str = "model refused";
/// An agent session the user's **Pause** ended (PAU-02, d0112): recovery takes its ticket through a restart's path
/// with the cause "paused" ([`crate::recovery::REASON_PAUSED`]); a Project agent or an epic lead becomes Paused.
pub const END_PAUSED: &str = "paused";

/// A terminal's size in character cells.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TerminalSize {
    pub cols: u16,
    pub rows: u16,
}

impl Default for TerminalSize {
    fn default() -> Self {
        Self { cols: 80, rows: 24 }
    }
}

impl TerminalSize {
    fn pty(self) -> PtySize {
        PtySize {
            rows: self.rows.max(1),
            cols: self.cols.max(1),
            pixel_width: 0,
            pixel_height: 0,
        }
    }
}

/// A session as clients see it: a `sessions` row, plus its size while it runs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionInfo {
    pub id: String,
    pub kind: SessionKind,
    /// The project's key.
    pub project: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ticket: Option<TicketKey>,
    pub state: SessionState,
    pub started_at: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ended_at: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end_reason: Option<String>,
    /// The agent tool it runs; `None` for Shell, Run and Project terminals (ADP-04).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool: Option<Tool>,
    /// The tool's own conversation id, kept for Resume, Park and "Before the restart" (d0045, d0088). Empty for
    /// a moment for a tool that reports it after start.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_session_id: Option<String>,
    /// The model passed to the tool at launch; `None` when none was.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// The agent it runs, as resolved at its start (or copied from the conversation it resumes; AGM-05). `None` for
    /// the user's terminals.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent: Option<SessionAgent>,
    /// The tool's effort level passed at launch (AGM-01); `None` when none was.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effort: Option<String>,
    /// It runs on its tool's fallback, by the user's click (AGM-03): the tool it would have run on and why.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fallback: Option<crate::agents::FallbackFrom>,
    /// The exact model the tool reported running (`claude-opus-5-5`), the first one it reported; `None` until it
    /// does.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_ran: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<TerminalSize>,
}

/// The agent a session ran (ADP-04, AGM-05): its id (no foreign key: a session outlives its agent) and its name at
/// the session's start ("Developer", shown "Developer (deleted)" once it is gone).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SessionAgent {
    pub id: String,
    pub name: String,
}

impl SessionAgent {
    fn of_columns(id: Option<String>, name: Option<String>) -> Option<Self> {
        Some(Self {
            id: id?,
            name: name?,
        })
    }
}

impl SessionInfo {
    pub fn is_running(&self) -> bool {
        self.state == SessionState::Running
    }
}

/// The ticket a session belongs to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionTicket {
    pub id: String,
    pub key: TicketKey,
}

/// A stored conversation to continue (Resume, Park, Send back, answers; d0045, d0088): the agent tool it ran
/// on, the tool's own id, the model and effort as launched and the agent, so Resume reopens the same tool and model
/// (ADP-04, AGM-03) and the resumed session's row names the same agent, whatever the agent is set to now (0088).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredConversation {
    pub tool: Tool,
    pub id: String,
    pub model: Option<String>,
    pub effort: Option<String>,
    pub agent: Option<SessionAgent>,
}

impl StoredConversation {
    /// The conversation `session` ran, when it ran an agent tool whose id is known. A session whose first turn was
    /// refused on the model has none to continue ("not started", T-0337).
    pub fn of(session: &SessionInfo) -> Option<Self> {
        if session.end_reason.as_deref() == Some(END_MODEL_REFUSED) {
            return None;
        }
        Some(Self {
            tool: session.tool?,
            id: session.tool_session_id.clone()?,
            model: session.model.clone(),
            effort: session.effort.clone(),
            agent: session.agent.clone(),
        })
    }

    /// The conversation of ticket `ticket_id`'s newest agent session whose tool id is known: Resume's (STG-09).
    /// By stage run (d0108): a session that ran an Agent stage counts whatever its kind, and when the ticket is in
    /// (or waits before) an Agent stage, that stage's own session comes first.
    pub async fn latest_agent(
        pool: &sqlx::SqlitePool,
        ticket_id: &str,
    ) -> Result<Option<Self>, sqlx::Error> {
        type Row = (
            String,
            String,
            Option<String>,
            Option<String>,
            Option<String>,
            Option<String>,
        );
        let row: Option<Row> = sqlx::query_as(
            "SELECT s.tool, s.tool_session_id, s.model, s.effort, s.agent_id, s.agent_name FROM sessions s
             WHERE s.ticket_id = ?1 AND s.tool IS NOT NULL AND s.tool_session_id IS NOT NULL
                   AND COALESCE(s.end_reason, '') <> ?2
                   AND (s.kind = 'agent'
                        OR EXISTS (SELECT 1 FROM stage_runs r WHERE r.session_id = s.id AND r.kind = 'agent'))
             ORDER BY EXISTS (SELECT 1 FROM stage_runs r JOIN tickets t ON t.id = r.ticket_id
                              WHERE r.session_id = s.id AND r.kind = 'agent' AND r.stage_id = t.stage_id) DESC,
                      s.started_at DESC, s.id DESC LIMIT 1",
        )
        .bind(ticket_id)
        .bind(END_MODEL_REFUSED)
        .fetch_optional(pool)
        .await?;
        Ok(
            row.and_then(|(tool, id, model, effort, agent_id, agent_name)| {
                Some(Self {
                    tool: tool.parse().ok()?,
                    id,
                    model,
                    effort,
                    agent: SessionAgent::of_columns(agent_id, agent_name),
                })
            }),
        )
    }
}

/// What [`Sessions::model_known`] did with a model the tool reported.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModelRan {
    /// The first report: written on the session.
    Written,
    /// The session already has this model.
    Same,
    /// The session already has another one (`/model` typed into its pane, say): kept; the later one is only logged.
    Different { first: String },
    /// No such session.
    NoSession,
}

/// A named agent's model that moved on between two sessions of a ticket's role (d0086: "when it differs from the
/// entry's previous run, the panel says so ('Sol → gpt-6.1-sol')"): both exact models. The view models word it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MovedOn {
    /// The exact model of the previous session on the same named agent (`claude-sonnet-5-5`).
    pub before: String,
    /// The session's own (`claude-sonnet-5-6`).
    pub now: String,
}

/// Whether `session`'s exact model differs from the previous session of the same ticket, role (session kind: the
/// Developer's are `agent` sessions, the Validator's `validator`) and named agent: both models when they do.
/// `None` when they are the same, when either isn't known (yet), or when there is no earlier session on that agent.
pub async fn moved_on(
    pool: &sqlx::SqlitePool,
    session: &str,
) -> Result<Option<MovedOn>, sqlx::Error> {
    let row: Option<(Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT s.model_ran,
                (SELECT b.model_ran FROM sessions b
                 WHERE b.ticket_id = s.ticket_id AND b.kind = s.kind AND b.agent_id = s.agent_id
                   AND b.model_ran IS NOT NULL
                   AND (b.started_at < s.started_at OR (b.started_at = s.started_at AND b.id < s.id))
                 ORDER BY b.started_at DESC, b.id DESC LIMIT 1)
         FROM sessions s WHERE s.id = ? AND s.ticket_id IS NOT NULL AND s.agent_id IS NOT NULL",
    )
    .bind(session)
    .fetch_optional(pool)
    .await?;
    Ok(match row {
        Some((Some(now), Some(before))) if now != before => Some(MovedOn { before, now }),
        _ => None,
    })
}

/// Who asked for a session (PAU-04, PAU-05; d0112). Every [`SpawnSpec`] says it; there is no default.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Start {
    /// Otaq on its own: the scheduler, a check after a stage, an answer session (d0030), a conflict's `restack`
    /// (d0064), a lead's start or wake (d0083), a lifted hold (d0086), a drag's start. Refused while its project is
    /// paused ([`Sessions::spawn`]); its caller keeps the start for Resume instead.
    Auto,
    /// The user's click on this one agent (answering it, its Resume, a Project agent's first message, a lead's Open or
    /// Resume), or the user's own terminal: it starts whether or not the project is paused, and the pause stays.
    User,
}

/// What to run.
#[derive(Debug, Clone)]
pub struct SpawnSpec {
    pub kind: SessionKind,
    /// Who asked for it: an automatic start doesn't run in a paused project.
    pub start: Start,
    /// Empty for a terminal of no project (an agent tool's Sign in…): it is kept in memory only, never stored.
    pub project_id: String,
    pub project_key: String,
    pub ticket: Option<SessionTicket>,
    pub program: String,
    pub args: Vec<String>,
    pub cwd: PathBuf,
    /// Added to (or replacing in) the service's environment, which the session inherits.
    pub env: Vec<(String, String)>,
    pub size: TerminalSize,
    /// The agent tool it runs, set by the adapter that built the command; `None` for the user's terminals.
    pub tool: Option<Tool>,
    pub tool_session_id: Option<String>,
    pub model: Option<String>,
    /// The agent it runs (AGM-05); `None` for the user's terminals.
    pub agent: Option<SessionAgent>,
    /// The tool's effort level passed (AGM-01).
    pub effort: Option<String>,
    /// It runs on its tool's fallback (AGM-03).
    pub fallback: Option<crate::agents::FallbackFrom>,
    /// The theme the session is told (`COLORFGBG`, the colour answers; d0117 point 4): an agent's launch read it
    /// ([`crate::agent_tools::Launched::theme`]); `None` reads the theme in use now (Shell, Run, terminals).
    pub theme: Option<Theme>,
}

/// [`Sessions::spawn`] after a launch's step ([`Sessions::prepared`]).
pub struct Prepared<'a> {
    sessions: &'a Sessions,
    step: Option<PrepareStep>,
}

impl Prepared<'_> {
    /// Runs the step, if any, then [`Sessions::spawn`].
    pub async fn spawn(self, spec: SpawnSpec) -> Result<SessionInfo, SessionError> {
        self.sessions.spawn_with(self.step.as_ref(), spec).await
    }
}

/// How long a [`Sessions::prepared`] step may take.
pub const PREPARE_TIMEOUT: Duration = Duration::from_secs(30);

/// Runs `step` to completion within `limit`, with the service's environment and the step's own variables on top.
/// A non-zero exit, a failure to start or the time running out is [`SessionError::Spawn`] with what the step
/// printed (its error output first).
async fn run_prepare(step: &PrepareStep, limit: Duration) -> Result<(), SessionError> {
    let mut command = tokio::process::Command::new(&step.program);
    command
        .args(&step.args)
        .current_dir(&step.cwd)
        .envs(step.env.iter().map(|(k, v)| (k, v)))
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);
    let output = match tokio::time::timeout(limit, command.output()).await {
        Ok(Ok(output)) => output,
        Ok(Err(e)) => {
            return Err(SessionError::Spawn(format!(
                "can't start {}: {e}",
                step.program
            )));
        }
        Err(_) => {
            return Err(SessionError::Spawn(format!(
                "{} didn't finish within {} s",
                step.program,
                limit.as_secs()
            )));
        }
    };
    if output.status.success() {
        return Ok(());
    }
    let printed: Vec<String> = [&output.stderr, &output.stdout]
        .into_iter()
        .map(|bytes| String::from_utf8_lossy(bytes).trim().to_owned())
        .filter(|text| !text.is_empty())
        .collect();
    Err(SessionError::Spawn(if printed.is_empty() {
        format!("{} failed ({})", step.program, output.status)
    } else {
        printed.join("\n")
    }))
}

/// The root process of a running agent session, for caller identity (CLI-01).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentRoot {
    pub pid: u32,
    /// Agent, answer, delivery or validator.
    pub kind: SessionKind,
    pub session: String,
    pub ticket: TicketKey,
    pub ticket_id: String,
}

/// The root process of a running Project agent session, for caller identity (CLI-01, d0068).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectAgentRoot {
    pub pid: u32,
    pub session: String,
    /// The project's key.
    pub project: String,
    /// The Project agent's id.
    pub agent: String,
    /// An epic lead's epic (its id and key, d0083), once [`Sessions::set_epic_lead`] said so.
    pub epic: Option<(String, TicketKey)>,
}

/// A piece of output and where it sits in the session's whole output.
#[derive(Debug, Clone)]
pub struct OutputChunk {
    pub offset: u64,
    pub data: Arc<[u8]>,
}

/// A client's terminal that answers the program's questions, attached ([`Sessions::attach_answering`]): while
/// one is held the service leaves the colour questions to it ([`colors`]). Dropped on detach or disconnect.
pub struct Answering {
    live: Arc<Live>,
}

impl fmt::Debug for Answering {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Answering")
    }
}

impl Drop for Answering {
    fn drop(&mut self) {
        let mut output = self.live.output.lock().expect("session output");
        output.answering = output.answering.saturating_sub(1);
    }
}

/// What a client gets when it attaches: the stored output, then live chunks from `output`.
#[derive(Debug)]
pub struct Attached {
    pub info: SessionInfo,
    pub scrollback: Vec<u8>,
    /// The offset of the first scrollback byte.
    pub start: u64,
    /// The offset after the last scrollback byte; live chunks continue from here.
    pub end: u64,
    pub output: broadcast::Receiver<OutputChunk>,
}

/// Live chunks kept for attached clients that are behind; one further behind is disconnected.
const OUTPUT_CHANNEL: usize = 1024;
/// How long a stopped session gets to exit after SIGHUP before SIGKILL.
const KILL_GRACE: Duration = Duration::from_secs(2);
/// How often [`Sessions::wait_tool_id`] looks again.
const TOOL_ID_POLL: Duration = Duration::from_millis(50);
/// How long the waiter lets the reader drain the last output after the program exited.
const DRAIN_WAIT: Duration = Duration::from_millis(500);

/// Owns every live session. Cheap to clone.
#[derive(Clone)]
pub struct Sessions {
    inner: Arc<Inner>,
}

struct Inner {
    store: Store,
    events: EventBus,
    scrollback_limit: usize,
    /// Which clients' terminals have focus on which session (ATT-03); shared with the board.
    focus: Focus,
    /// Running sessions, and the last ended one per ticket and kind (so its scrollback can still be shown).
    live: Mutex<HashMap<String, Arc<Live>>>,
    /// A project's variables (ENV-02), set once at service start; without it sessions get none.
    project_env: OnceLock<env::EnvProvider>,
    /// A ticket's own variables (d0087), set once at service start; without it ticket sessions get none.
    ticket_env: OnceLock<env::TicketEnvProvider>,
    /// The theme in use (d0117 point 4), set once at service start; without it sessions are told the default.
    theme: OnceLock<ThemeProvider>,
}

/// The theme in use, read when asked: set once at service start ([`Sessions::set_theme_provider`]).
pub type ThemeProvider = Arc<
    dyn Fn() -> std::pin::Pin<Box<dyn std::future::Future<Output = Theme> + Send>> + Send + Sync,
>;

struct Live {
    info: Mutex<SessionInfo>,
    ticket_id: Option<String>,
    /// The Project agent the session runs for (d0068), once [`Sessions::set_project_agent`] said so.
    project_agent: Mutex<Option<String>>,
    /// The epic it leads (id and key), once [`Sessions::set_epic_lead`] said so (d0083).
    lead_epic: Mutex<Option<(String, TicketKey)>>,
    /// The skill copies in its folder when it started (`otaq-address-review`…), once
    /// [`Sessions::set_started_skills`] said so (d0118): its tool read those, and only those, at its start.
    started_skills: Mutex<Vec<String>>,
    pid: Option<u32>,
    /// The program and what it started, stopped together.
    group: crate::process::Group,
    master: Mutex<Box<dyn MasterPty + Send>>,
    writer: Mutex<Box<dyn Write + Send>>,
    killer: Mutex<Box<dyn ChildKiller + Send + Sync>>,
    output: Mutex<Output>,
    /// The unsent-input flag (SVC-06, d0048).
    input: Mutex<InputTracker>,
    /// Set when Otaq stops the session; the end reason then says so.
    stop_reason: Mutex<Option<&'static str>>,
    ended: watch::Sender<bool>,
    /// The theme's terminal colours when the session started: the service's answer to the program's colour
    /// questions ([`colors`]). A running session keeps them.
    colors: TerminalColors,
}

struct Output {
    scrollback: Scrollback,
    live: broadcast::Sender<OutputChunk>,
    /// The terminal's size, kept after the session ended (its info's size isn't), for [`screen`].
    size: TerminalSize,
    /// The attached terminals that answer the program's questions ([`Answering`]); while there are none the
    /// service answers its colour questions itself ([`colors`]).
    answering: usize,
}

impl Sessions {
    pub fn new(store: Store, events: EventBus) -> Self {
        Self::with_scrollback_limit(store, events, scrollback::DEFAULT_LIMIT)
    }

    pub fn with_scrollback_limit(store: Store, events: EventBus, limit: usize) -> Self {
        Self::build(store, events, limit, Focus::default())
    }

    /// Sessions whose terminal focus is kept in `focus`: the service passes the board's, so ticket views and
    /// needs-you items follow the focused-terminal rule (ATT-03).
    pub fn with_focus(store: Store, events: EventBus, focus: Focus) -> Self {
        Self::build(store, events, scrollback::DEFAULT_LIMIT, focus)
    }

    fn build(store: Store, events: EventBus, limit: usize, focus: Focus) -> Self {
        Self {
            inner: Arc::new(Inner {
                store,
                events,
                scrollback_limit: limit,
                focus,
                live: Mutex::new(HashMap::new()),
                project_env: OnceLock::new(),
                ticket_env: OnceLock::new(),
                theme: OnceLock::new(),
            }),
        }
    }

    /// Gives the sessions the project variables (ENV-02, d0082): every spawn asks `provider` for the rows of its
    /// project, then.
    pub fn set_env_provider(&self, provider: env::EnvProvider) {
        let _ = self.inner.project_env.set(provider);
    }

    /// Gives the sessions a ticket's own variables (the ticket resource variables, d0087): every spawn for a
    /// ticket (its agent, validator, answering, Run and Shell) asks `provider` for them, then.
    pub fn set_ticket_env_provider(&self, provider: env::TicketEnvProvider) {
        let _ = self.inner.ticket_env.set(provider);
    }

    /// Gives the sessions the theme in use (d0117 point 4): every spawn asks `provider`, then, and tells the
    /// program whether it is light or dark (`COLORFGBG`) and its terminal colours ([`colors`]).
    pub fn set_theme_provider(&self, provider: ThemeProvider) {
        let _ = self.inner.theme.set(provider);
    }

    /// Marks sessions left running by a service that is gone as ended (agents "service stopped", Run / Shell /
    /// Project terminal "stopped by restart"). Called once when the service starts.
    pub async fn end_leftovers(store: &Store) -> Result<u64, StoreError> {
        let mut ended = 0;
        for (kinds, reason) in [(false, END_SERVICE_STOPPED), (true, END_STOPPED_BY_RESTART)] {
            let done = sqlx::query(
                "UPDATE sessions SET state = ?, ended_at = ?, end_reason = ?
                 WHERE ended_at IS NULL AND (kind IN ('run', 'shell', 'project')) = ?",
            )
            .bind(SessionState::Ended.as_str())
            .bind(now_ms())
            .bind(reason)
            .bind(kinds)
            .execute(store.pool())
            .await?;
            ended += done.rows_affected();
        }
        Ok(ended)
    }

    /// The sessions with the launch's step before the program ([`crate::agent_tools::Launched::prepare`]), for
    /// the next [`Prepared::spawn`]: the step runs to completion first, and a failure fails the spawn with its
    /// output and writes no row. `None` spawns exactly as [`Self::spawn`].
    pub fn prepared(&self, step: Option<PrepareStep>) -> Prepared<'_> {
        Prepared {
            sessions: self,
            step,
        }
    }

    /// Starts `spec.program` in a new PTY and records its `sessions` row. A second running agent session for
    /// the same ticket is refused, and so is an automatic start ([`Start::Auto`]) in a paused project (the backstop of
    /// PAU-04: its caller keeps the start for Resume).
    pub async fn spawn(&self, spec: SpawnSpec) -> Result<SessionInfo, SessionError> {
        self.spawn_with(None, spec).await
    }

    async fn spawn_with(
        &self,
        prepare: Option<&PrepareStep>,
        spec: SpawnSpec,
    ) -> Result<SessionInfo, SessionError> {
        if spec.kind == SessionKind::Agent
            && let Some(ticket) = &spec.ticket
            && let Some(running) = self.running_of(&ticket.id, SessionKind::Agent)
        {
            return Err(SessionError::AlreadyRunning {
                ticket: ticket.key.clone(),
                session: running.id,
            });
        }
        if spec.start == Start::Auto
            && let Some(name) =
                crate::agents::pause::paused_name(&self.inner.store, &spec.project_key)
                    .await
                    .map_err(SessionError::Store)?
        {
            tracing::info!(project = %spec.project_key, kind = ?spec.kind, "automatic start refused: paused");
            return Err(SessionError::Paused { project: name });
        }
        if let Some(step) = prepare {
            run_prepare(step, PREPARE_TIMEOUT).await?;
        }

        let pty = native_pty_system()
            .openpty(spec.size.pty())
            .map_err(|e| SessionError::Spawn(e.to_string()))?;
        let mut command = CommandBuilder::new(&spec.program);
        // On Windows portable-pty starts from the environment stored in the registry (a fresh login's), which
        // drops what the service's own environment changed, PATH first of all. Sessions get the service's
        // environment, as on macOS and Linux.
        if cfg!(windows) {
            for (key, value) in std::env::vars_os() {
                command.env(key, value);
            }
        }
        command.args(&spec.args);
        command.cwd(&spec.cwd);
        // The project's variables, read now (ENV-02), then Otaq's own on top ([`env`]'s order rule).
        let rows = match self.inner.project_env.get() {
            Some(provider) if !spec.project_id.is_empty() => {
                provider(spec.project_id.clone()).await
            }
            _ => Vec::new(),
        };
        // A service started from Finder or a script may have no TERM; terminals need one.
        let term = command.get_env("TERM").map_or_else(
            || "xterm-256color".to_owned(),
            |t| t.to_string_lossy().into_owned(),
        );
        // A ticket's own variables come before the spec's, so a value the spec sets wins.
        let ticket_vars = match (&spec.ticket, self.inner.ticket_env.get()) {
            (Some(ticket), Some(provider)) => provider(ticket.id.clone()).await,
            _ => Vec::new(),
        };
        // The theme in use now (d0117 point 4); a running session keeps what it was told.
        let theme = match (spec.theme, self.inner.theme.get()) {
            (Some(theme), _) => theme,
            (None, Some(provider)) => provider().await,
            (None, None) => ThemeSetting::default().in_use(),
        };
        // COLORFGBG ("text;background" as two of the 16 colours) comes last, so a ticket's or the spec's variable
        // of that name doesn't override what the theme is.
        let colorfgbg = if theme.is_dark() { "15;0" } else { "0;15" };
        let own: Vec<(String, String)> = [
            ("TERM".to_owned(), term),
            ("COLORTERM".to_owned(), "truecolor".to_owned()),
        ]
        .into_iter()
        .chain(ticket_vars)
        .chain(spec.env.iter().cloned())
        .chain([("COLORFGBG".to_owned(), colorfgbg.to_owned())])
        .collect();
        let inherited_path = command.get_env("PATH").map(ToOwned::to_owned);
        for (key, value) in env::layered(
            &rows,
            &own,
            inherited_path.as_deref(),
            crate::tools::Os::current(),
        ) {
            command.env(key, value);
        }
        let mut child = pty
            .slave
            .spawn_command(command)
            .map_err(|e| SessionError::Spawn(format!("can't start {}: {e}", spec.program)))?;
        // The reader only sees end-of-file once every slave handle is closed.
        drop(pty.slave);
        let reader = pty
            .master
            .try_clone_reader()
            .map_err(|e| SessionError::Spawn(e.to_string()))?;
        let writer = pty
            .master
            .take_writer()
            .map_err(|e| SessionError::Spawn(e.to_string()))?;

        let info = SessionInfo {
            id: new_id(),
            kind: spec.kind,
            project: spec.project_key.clone(),
            ticket: spec.ticket.as_ref().map(|t| t.key.clone()),
            state: SessionState::Running,
            started_at: now_ms(),
            ended_at: None,
            end_reason: None,
            tool: spec.tool,
            tool_session_id: spec.tool_session_id.clone(),
            model: spec.model.clone(),
            agent: spec.agent.clone(),
            effort: spec.effort.clone(),
            fallback: spec.fallback.clone(),
            model_ran: None,
            size: Some(spec.size),
        };
        // A terminal of no project (an agent tool's Sign in…, d0086) lives in memory only.
        let inserted = if spec.project_id.is_empty() {
            Ok(())
        } else {
            sqlx::query(
            "INSERT INTO sessions (id, project_id, ticket_id, kind, pty_id, tool, tool_session_id, model, state,
                                   started_at, agent_id, agent_name, effort, fallback_from, fallback_reason)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&info.id)
        .bind(&spec.project_id)
        .bind(spec.ticket.as_ref().map(|t| t.id.clone()))
        .bind(info.kind.as_str())
        .bind(child.process_id().map(|pid| pid.to_string()))
        .bind(info.tool.map(Tool::as_str))
        .bind(&info.tool_session_id)
        .bind(&info.model)
        .bind(info.state.as_str())
        .bind(info.started_at)
        .bind(info.agent.as_ref().map(|a| &a.id))
        .bind(info.agent.as_ref().map(|a| &a.name))
        .bind(&info.effort)
        .bind(info.fallback.as_ref().map(|f| f.tool.as_str()))
        .bind(info.fallback.as_ref().map(|f| f.reason.as_str()))
        .execute(self.inner.store.pool())
        .await
        .map(drop)
        };
        if let Err(e) = inserted {
            let _ = child.kill();
            return Err(SessionError::Store(e.into()));
        }

        let (ended, _) = watch::channel(false);
        let live = Arc::new(Live {
            info: Mutex::new(info.clone()),
            ticket_id: spec.ticket.as_ref().map(|t| t.id.clone()),
            project_agent: Mutex::new(None),
            lead_epic: Mutex::new(None),
            started_skills: Mutex::new(Vec::new()),
            pid: child.process_id(),
            group: crate::process::Group::of_leader(child.process_id()),
            killer: Mutex::new(child.clone_killer()),
            master: Mutex::new(pty.master),
            writer: Mutex::new(writer),
            output: Mutex::new(Output {
                scrollback: Scrollback::new(self.inner.scrollback_limit),
                live: broadcast::channel(OUTPUT_CHANNEL).0,
                size: spec.size,
                answering: 0,
            }),
            input: Mutex::new(InputTracker::new()),
            stop_reason: Mutex::new(None),
            ended,
            colors: theme.terminal_colors(),
        });
        self.inner
            .live
            .lock()
            .expect("sessions lock")
            .insert(info.id.clone(), live.clone());

        let (drained, drained_rx) = std::sync::mpsc::channel::<()>();
        let reading = live.clone();
        std::thread::Builder::new()
            .name(format!("pty-read-{}", info.id))
            .spawn(move || read_output(reader, &reading, drained))
            .map_err(|e| SessionError::Spawn(e.to_string()))?;
        let weak = Arc::downgrade(&self.inner);
        let runtime = tokio::runtime::Handle::current();
        let id = info.id.clone();
        std::thread::Builder::new()
            .name(format!("pty-wait-{id}"))
            .spawn(move || {
                let status = child.wait();
                let _ = drained_rx.recv_timeout(DRAIN_WAIT);
                let reason = match status {
                    Ok(status) => match status.signal() {
                        Some(signal) => format!("signal {signal}"),
                        None => format!("exit {}", status.exit_code()),
                    },
                    Err(e) => format!("lost: {e}"),
                };
                runtime.spawn(async move {
                    if let Some(inner) = weak.upgrade() {
                        inner.finish(&id, reason).await;
                    }
                });
            })
            .map_err(|e| SessionError::Spawn(e.to_string()))?;

        tracing::info!(session = %info.id, kind = %info.kind, ticket = ?info.ticket, program = %spec.program, "session started");
        self.inner.events.publish(Event::SessionState {
            session: info.clone(),
        });
        Ok(info)
    }

    /// The stored output and a live feed from right after it. Works for ended sessions still in memory too.
    pub fn attach(&self, id: &str) -> Result<Attached, SessionError> {
        self.attach_answering(id, false)
            .map(|(attached, _)| attached)
    }

    /// [`Self::attach`] for a client's terminal; with `answers` (the terminal answers the program's questions) also
    /// the [`Answering`] that keeps the service from answering colour questions while it is held. Taken together,
    /// so a question is answered by the terminal or by the service, never both and never neither ([`colors`]).
    pub fn attach_answering(
        &self,
        id: &str,
        answers: bool,
    ) -> Result<(Attached, Option<Answering>), SessionError> {
        let live = self.live(id)?;
        let info = live.info.lock().expect("session info").clone();
        let mut output = live.output.lock().expect("session output");
        let (scrollback, start) = output.scrollback.contents();
        let attached = Attached {
            info,
            end: output.scrollback.end(),
            scrollback,
            start,
            output: output.live.subscribe(),
        };
        let answering = answers.then(|| {
            output.answering += 1;
            Answering { live: live.clone() }
        });
        Ok((attached, answering))
    }

    /// Resizes the PTY; the program gets SIGWINCH.
    pub fn resize(&self, id: &str, size: TerminalSize) -> Result<(), SessionError> {
        let live = self.live(id)?;
        let mut info = live.info.lock().expect("session info");
        if !info.is_running() {
            return Err(SessionError::Ended(id.to_owned()));
        }
        live.master
            .lock()
            .expect("session pty")
            .resize(size.pty())
            .map_err(|e| SessionError::Io(e.to_string()))?;
        info.size = Some(size);
        live.output.lock().expect("session output").size = size;
        Ok(())
    }

    /// Relays keystrokes typed by a person (in the app or `otaq attach`) and updates the unsent-input
    /// flag (SVC-06). Each Enter is reported as a `session_enter` event (ATT-02 clears needs-you signals with it).
    pub fn input(&self, id: &str, bytes: &[u8]) -> Result<InputEffect, SessionError> {
        let live = self.live(id)?;
        let effect = {
            // Held while writing, so the flag follows the order the bytes reach the program.
            let mut input = live.input.lock().expect("session input");
            live.write(id, bytes)?;
            input.feed(bytes)
        };
        if effect.enters > 0 {
            let ticket = live.info.lock().expect("session info").ticket.clone();
            for _ in 0..effect.enters {
                self.inner.events.publish(Event::SessionEnter {
                    session: id.to_owned(),
                    ticket: ticket.clone(),
                });
            }
        }
        Ok(effect)
    }

    /// Adds a line of Otaq's own to the session's output, as if the program had printed it: Run's "[…] exited
    /// with code 1" (RUN-01). It goes to the scrollback and to attached clients, also after the session ended,
    /// and starts on a line of its own.
    pub fn append_line(&self, id: &str, line: &str) -> Result<(), SessionError> {
        let live = self.live(id)?;
        let mut output = live.output.lock().expect("session output");
        let mut bytes = Vec::with_capacity(line.len() + 4);
        if output.scrollback.last().is_some_and(|b| b != b'\n') {
            bytes.extend_from_slice(b"\r\n");
        }
        bytes.extend_from_slice(line.as_bytes());
        bytes.extend_from_slice(b"\r\n");
        let data: Arc<[u8]> = Arc::from(bytes.as_slice());
        let offset = output.scrollback.push(&data);
        let _ = output.live.send(OutputChunk { offset, data });
        Ok(())
    }

    /// Writes Otaq's own bytes to the program's input as they are. They don't touch the unsent-input
    /// flag: only a person's typing does.
    pub fn write(&self, id: &str, bytes: &[u8]) -> Result<(), SessionError> {
        self.live(id)?.write(id, bytes)
    }

    /// Whether the session's input line may hold text the user hasn't sent (d0048).
    pub fn has_unsent_input(&self, id: &str) -> Result<bool, SessionError> {
        Ok(self
            .live(id)?
            .input
            .lock()
            .expect("session input")
            .has_unsent_text())
    }

    /// Whether the unsent text may run over several lines (SES-07, ADP-06).
    pub fn has_multi_line_input(&self, id: &str) -> Result<bool, SessionError> {
        Ok(self
            .live(id)?
            .input
            .lock()
            .expect("session input")
            .has_multi_line_text())
    }

    /// A client reports that its terminal for this session gained or lost keyboard focus (SES-10). Focus is kept
    /// per client: the session counts as focused while any client holds it, and `otaq attach` holds it while
    /// attached (ATT-03). Publishes `session_focus` when the session's focus changes, and `ticket_changed` when
    /// the ticket's does on the session's side (a validator is a side of its own, d0063; its card, inbox item and
    /// notification depend on it). An ended session takes no focus.
    pub fn set_focused(
        &self,
        id: &str,
        client: ClientId,
        focused: bool,
    ) -> Result<(), SessionError> {
        let live = self.live(id)?;
        let info = live.info.lock().expect("session info").clone();
        // A Project agent's session is focused like a ticket's (ATT-03, d0068), under its own key.
        let project_agent = live
            .project_agent
            .lock()
            .expect("session project agent")
            .as_ref()
            .map(|agent| format!("project-agent:{agent}"));
        let Some(ticket_id) = live.ticket_id.as_deref().or(project_agent.as_deref()) else {
            return Ok(());
        };
        if focused && !info.is_running() {
            return Ok(());
        }
        let side = if info.kind == SessionKind::Validator {
            crate::attention::Side::Validator
        } else {
            crate::attention::Side::Agent
        };
        let change = self.inner.focus.set(ticket_id, id, side, client, focused);
        if change.session_changed {
            self.inner.events.publish(Event::SessionFocus {
                session: id.to_owned(),
                ticket: info.ticket.clone(),
                focused: change.session_focused,
            });
        }
        if change.ticket_changed {
            self.inner.ticket_focus_changed(ticket_id, info.ticket);
        }
        Ok(())
    }

    /// A client disconnected: every focus it held is lost (ATT-03).
    pub fn client_gone(&self, client: ClientId) {
        let released = self.inner.focus.client_gone(client);
        self.inner.publish_released(released);
    }

    /// Whether a client's terminal for this session has focus.
    pub fn is_focused(&self, id: &str) -> Result<bool, SessionError> {
        self.live(id)?;
        Ok(self.inner.focus.session_focused(id))
    }

    /// Clears the input line with `clear_line`, the bytes of the session's agent tool (ADP-06), and resets the
    /// flag. Only ever on the user's click ("Clear it and send", SES-07): Otaq never clears the user's text
    /// silently.
    pub fn clear_line(&self, id: &str, clear_line: &[u8]) -> Result<(), SessionError> {
        let live = self.live(id)?;
        let mut input = live.input.lock().expect("session input");
        live.write(id, clear_line)?;
        input.clear_line();
        Ok(())
    }

    /// The session as it is now (running, or ended and still in memory).
    pub fn info(&self, id: &str) -> Result<SessionInfo, SessionError> {
        Ok(self.live(id)?.info.lock().expect("session info").clone())
    }

    /// The id of the ticket the session runs for (in memory), if any.
    pub fn ticket_id_of(&self, id: &str) -> Option<String> {
        self.live(id).ok()?.ticket_id.clone()
    }

    /// Every session in memory: the running ones and the last ended one per ticket and kind.
    pub fn in_memory(&self) -> Vec<SessionInfo> {
        let mut all: Vec<SessionInfo> = self
            .inner
            .live
            .lock()
            .expect("sessions lock")
            .values()
            .map(|l| l.info.lock().expect("session info").clone())
            .collect();
        all.sort_by(|a, b| a.started_at.cmp(&b.started_at).then(a.id.cmp(&b.id)));
        all
    }

    /// The root processes of every running session that runs an agent tool for a ticket.
    pub fn agent_roots(&self) -> Vec<AgentRoot> {
        self.inner
            .live
            .lock()
            .expect("sessions lock")
            .values()
            .filter_map(|l| {
                let info = l.info.lock().expect("session info");
                if !info.is_running() || !info.kind.runs_agent_tool() {
                    return None;
                }
                Some(AgentRoot {
                    pid: l.pid?,
                    kind: info.kind,
                    session: info.id.clone(),
                    ticket: info.ticket.clone()?,
                    ticket_id: l.ticket_id.clone()?,
                })
            })
            .collect()
    }

    /// The running agent session (one that runs an agent tool) whose process group holds `pid` (Windows: its job; see
    /// [`crate::process::Group::contains`]).
    pub fn agent_root_holding(&self, pid: u32) -> Option<AgentRoot> {
        self.inner
            .live
            .lock()
            .expect("sessions lock")
            .values()
            .find_map(|l| {
                let info = l.info.lock().expect("session info");
                if !info.is_running() || !info.kind.runs_agent_tool() || !l.group.contains(pid) {
                    return None;
                }
                Some(AgentRoot {
                    pid: l.pid?,
                    kind: info.kind,
                    session: info.id.clone(),
                    ticket: info.ticket.clone()?,
                    ticket_id: l.ticket_id.clone()?,
                })
            })
    }

    /// Records that the session runs for the Project agent `agent` (its `sessions.project_agent_id`); from then on
    /// its processes are that Project agent's callers.
    pub async fn set_project_agent(&self, id: &str, agent: &str) -> Result<(), SessionError> {
        sqlx::query("UPDATE sessions SET project_agent_id = ? WHERE id = ?")
            .bind(agent)
            .bind(id)
            .execute(self.inner.store.pool())
            .await
            .map_err(|e| SessionError::Store(e.into()))?;
        *self
            .live(id)?
            .project_agent
            .lock()
            .expect("session project agent") = Some(agent.to_owned());
        Ok(())
    }

    /// Records that the Project agent session `id` leads the epic `epic_id` (`key`): from then on its processes are
    /// that epic lead's callers (d0083). In memory only: the lead's row keeps the epic.
    pub fn set_epic_lead(
        &self,
        id: &str,
        epic_id: &str,
        key: &TicketKey,
    ) -> Result<(), SessionError> {
        *self.live(id)?.lead_epic.lock().expect("session lead epic") =
            Some((epic_id.to_owned(), key.clone()));
        Ok(())
    }

    /// Records the skill copies (`otaq-work-ticket`, `otaq-address-review`…) written into the session's folder right
    /// before it started (d0118). In memory only: a restart relaunches the session, which copies them again.
    pub fn set_started_skills(&self, id: &str, copies: Vec<String>) -> Result<(), SessionError> {
        *self
            .live(id)?
            .started_skills
            .lock()
            .expect("session skills") = copies;
        Ok(())
    }

    /// Whether the session started with the skill copy `copy` (`otaq-restack`) in its folder: only then can its
    /// tool run it by name (d0118). A session nothing was recorded for started with none.
    pub fn started_with_skill(&self, id: &str, copy: &str) -> bool {
        self.live(id).is_ok_and(|live| {
            live.started_skills
                .lock()
                .expect("session skills")
                .iter()
                .any(|c| c == copy)
        })
    }

    /// The root processes of every running Project agent session.
    pub fn project_agent_roots(&self) -> Vec<ProjectAgentRoot> {
        self.inner
            .live
            .lock()
            .expect("sessions lock")
            .values()
            .filter_map(|l| l.project_agent_root())
            .collect()
    }

    /// The running Project agent session whose process group holds `pid` (Windows: its job).
    pub fn project_agent_holding(&self, pid: u32) -> Option<ProjectAgentRoot> {
        self.inner
            .live
            .lock()
            .expect("sessions lock")
            .values()
            .filter(|l| l.group.contains(pid))
            .find_map(|l| l.project_agent_root())
    }

    /// The running Project terminal whose process group holds `pid` (Windows: its job).
    pub fn project_terminal_holding(&self, pid: u32) -> Option<String> {
        self.inner
            .live
            .lock()
            .expect("sessions lock")
            .values()
            .find_map(|l| {
                let info = l.info.lock().expect("session info");
                (info.is_running() && info.kind == SessionKind::Project && l.group.contains(pid))
                    .then(|| info.id.clone())
            })
    }

    /// The root processes of every running Project terminal, with their session (T-0167: what was created there).
    pub fn project_terminal_roots(&self) -> Vec<(u32, String)> {
        self.inner
            .live
            .lock()
            .expect("sessions lock")
            .values()
            .filter_map(|l| {
                let info = l.info.lock().expect("session info");
                (info.is_running() && info.kind == SessionKind::Project)
                    .then(|| Some((l.pid?, info.id.clone())))
                    .flatten()
            })
            .collect()
    }

    /// The ticket's running session of this kind.
    pub fn running_of(&self, ticket_id: &str, kind: SessionKind) -> Option<SessionInfo> {
        self.latest_of(ticket_id, kind)
            .filter(SessionInfo::is_running)
    }

    /// The ticket's newest session of this kind still in memory (running or just ended).
    pub fn latest_of(&self, ticket_id: &str, kind: SessionKind) -> Option<SessionInfo> {
        self.inner
            .live
            .lock()
            .expect("sessions lock")
            .values()
            .filter(|l| l.ticket_id.as_deref() == Some(ticket_id))
            .map(|l| l.info.lock().expect("session info").clone())
            .filter(|i| i.kind == kind)
            .max_by(|a, b| a.started_at.cmp(&b.started_at).then(a.id.cmp(&b.id)))
    }

    /// The ticket's newest session of this kind still in memory (running or just ended), with its kept output and
    /// its terminal's size: what `otaq show <KEY> --terminal` replays ([`screen::render`], SHL-10).
    pub fn output_of(
        &self,
        ticket_id: &str,
        kind: SessionKind,
    ) -> Option<(SessionInfo, Vec<u8>, TerminalSize)> {
        let info = self.latest_of(ticket_id, kind)?;
        let live = self.live(&info.id).ok()?;
        let output = live.output.lock().expect("session output");
        Some((info, output.scrollback.contents().0, output.size))
    }

    /// Every stored session of a ticket, newest first, with the size of the running one.
    pub async fn history(&self, ticket: &SessionTicket) -> Result<Vec<SessionInfo>, SessionError> {
        let rows: Vec<SessionRow> = sqlx::query_as(
            "SELECT s.id, s.kind, p.key AS project_key, s.state, s.started_at, s.ended_at, s.end_reason,
                    s.tool, s.tool_session_id, s.model, s.agent_id, s.agent_name, s.model_ran, s.effort,
                    s.fallback_from, s.fallback_reason
             FROM sessions s JOIN projects p ON p.id = s.project_id
             WHERE s.ticket_id = ? ORDER BY s.started_at DESC, s.id DESC",
        )
        .bind(&ticket.id)
        .fetch_all(self.inner.store.pool())
        .await
        .map_err(|e| SessionError::Store(e.into()))?;
        Ok(rows
            .into_iter()
            .filter_map(|row| {
                let mut info = row.into_info(Some(ticket.key.clone()))?;
                if let Ok(live) = self.info(&info.id) {
                    info.size = live.size;
                }
                Some(info)
            })
            .collect())
    }

    /// The tool reported the exact model session `id` runs on (ADP-04; d0086 "the model is always the exact one
    /// that ran"): written once, on its row and its in-memory info, the first value only. Nothing waits for it.
    pub async fn model_known(&self, id: &str, model: &str) -> Result<ModelRan, SessionError> {
        let pool = self.inner.store.pool();
        let store = |e: sqlx::Error| SessionError::Store(e.into());
        let written =
            sqlx::query("UPDATE sessions SET model_ran = ? WHERE id = ? AND model_ran IS NULL")
                .bind(model)
                .bind(id)
                .execute(pool)
                .await
                .map_err(store)?
                .rows_affected()
                > 0;
        if written {
            if let Ok(live) = self.live(id) {
                live.info.lock().expect("session info").model_ran = Some(model.to_owned());
            }
            return Ok(ModelRan::Written);
        }
        let first: Option<Option<String>> =
            sqlx::query_scalar("SELECT model_ran FROM sessions WHERE id = ?")
                .bind(id)
                .fetch_optional(pool)
                .await
                .map_err(store)?;
        Ok(match first.flatten() {
            None => ModelRan::NoSession,
            Some(first) if first == model => ModelRan::Same,
            Some(first) => ModelRan::Different { first },
        })
    }

    /// The tool reported session `id`'s own conversation id after start (d0088 "Session ids", Codex): stored on its
    /// row and its in-memory info only while it has none, so a later report never replaces it. Whether it was
    /// stored.
    pub async fn tool_id_known(&self, id: &str, tool_id: &str) -> Result<bool, SessionError> {
        let stored = sqlx::query(
            "UPDATE sessions SET tool_session_id = ?
             WHERE id = ? AND (tool_session_id IS NULL OR tool_session_id = '')",
        )
        .bind(tool_id)
        .bind(id)
        .execute(self.inner.store.pool())
        .await
        .map_err(|e| SessionError::Store(e.into()))?
        .rows_affected()
            > 0;
        if stored && let Ok(live) = self.live(id) {
            live.info.lock().expect("session info").tool_session_id = Some(tool_id.to_owned());
        }
        Ok(stored)
    }

    /// Waits up to `within` for running session `id`'s tool to report its conversation id (Park, ADP-07). Returns
    /// the id once known; `None` when it wasn't within the time, or the session ended first.
    pub async fn wait_tool_id(&self, id: &str, within: Duration) -> Option<String> {
        let deadline = tokio::time::Instant::now() + within;
        loop {
            let info = self.info(id).ok()?;
            if let Some(known) = info.tool_session_id.clone().filter(|t| !t.is_empty()) {
                return Some(known);
            }
            if !info.is_running() || tokio::time::Instant::now() >= deadline {
                return None;
            }
            tokio::time::sleep(TOOL_ID_POLL).await;
        }
    }

    /// A session as stored (running, or ended long ago), with its ticket's key.
    pub async fn stored(&self, id: &str) -> Result<SessionInfo, SessionError> {
        #[derive(sqlx::FromRow)]
        struct Row {
            #[sqlx(flatten)]
            session: SessionRow,
            number: Option<i64>,
            child_number: Option<i64>,
        }
        let row: Row = sqlx::query_as(
            "SELECT s.id, s.kind, p.key AS project_key, s.state, s.started_at, s.ended_at, s.end_reason,
                    s.tool, s.tool_session_id, s.model, s.agent_id, s.agent_name, s.model_ran, s.effort,
                    s.fallback_from, s.fallback_reason,
                    t.number, t.child_number
             FROM sessions s JOIN projects p ON p.id = s.project_id LEFT JOIN tickets t ON t.id = s.ticket_id
             WHERE s.id = ?",
        )
        .bind(id)
        .fetch_optional(self.inner.store.pool())
        .await
        .map_err(|e| SessionError::Store(e.into()))?
        .ok_or_else(|| SessionError::NotFound(id.to_owned()))?;
        let ticket = row.number.and_then(|n| {
            crate::board::tickets::key_from_parts(
                &row.session.project_key,
                n,
                row.child_number.unwrap_or(0),
            )
        });
        row.session
            .into_info(ticket)
            .ok_or_else(|| SessionError::NotFound(id.to_owned()))
    }

    /// Waits until the session has ended; returns it as it ended. `None` for a session not in memory.
    pub async fn wait_ended(&self, id: &str) -> Option<SessionInfo> {
        let live = self.live(id).ok()?;
        let mut ended = live.ended.subscribe();
        let _ = ended.wait_for(|e| *e).await;
        Some(live.info.lock().expect("session info").clone())
    }

    /// Stops a session: SIGHUP to its process group, SIGKILL if it hasn't exited after a grace period.
    pub async fn stop(&self, id: &str) -> Result<(), SessionError> {
        self.stop_as(id, END_STOPPED).await
    }

    /// [`stop`](Self::stop), recording `reason` as the session's end ([`END_MODEL_REFUSED`]).
    pub async fn stop_as(&self, id: &str, reason: &'static str) -> Result<(), SessionError> {
        let live = self.live(id)?;
        live.stop(reason);
        let mut ended = live.ended.subscribe();
        if tokio::time::timeout(KILL_GRACE, ended.wait_for(|e| *e))
            .await
            .is_err()
        {
            live.kill_now();
            let _ = ended.wait_for(|e| *e).await;
        }
        Ok(())
    }

    /// Stops every running session as the service stops, and records why.
    pub async fn shutdown(&self) {
        let running: Vec<Arc<Live>> = self
            .inner
            .live
            .lock()
            .expect("sessions lock")
            .values()
            .filter(|l| l.info.lock().expect("session info").is_running())
            .cloned()
            .collect();
        for live in &running {
            let kind = live.info.lock().expect("session info").kind;
            live.stop(if kind.is_users_terminal() {
                END_STOPPED_BY_RESTART
            } else {
                END_SERVICE_STOPPED
            });
        }
        for live in &running {
            let mut ended = live.ended.subscribe();
            if tokio::time::timeout(KILL_GRACE, ended.wait_for(|e| *e))
                .await
                .is_err()
            {
                live.kill_now();
                let _ = tokio::time::timeout(KILL_GRACE, ended.wait_for(|e| *e)).await;
            }
        }
        // Anything that still didn't report its end is recorded as ended now.
        if let Err(e) = Self::end_leftovers(&self.inner.store).await {
            tracing::warn!(error = %e, "can't record the end of sessions");
        }
    }

    fn live(&self, id: &str) -> Result<Arc<Live>, SessionError> {
        self.inner
            .live
            .lock()
            .expect("sessions lock")
            .get(id)
            .cloned()
            .ok_or_else(|| SessionError::NotFound(id.to_owned()))
    }
}

impl Inner {
    /// Records a session's end: its row, its info, a `session_state` event; drops older ended sessions of the
    /// same ticket and kind from memory.
    async fn finish(&self, id: &str, exit: String) {
        let Some(live) = self.live.lock().expect("sessions lock").get(id).cloned() else {
            return;
        };
        let reason = live
            .stop_reason
            .lock()
            .expect("stop reason")
            .map_or(exit, str::to_owned);
        let ended_at = now_ms();
        let info = {
            let mut info = live.info.lock().expect("session info");
            info.state = SessionState::Ended;
            info.ended_at = Some(ended_at);
            info.end_reason = Some(reason.clone());
            info.size = None;
            info.clone()
        };
        if let Err(e) = sqlx::query(
            "UPDATE sessions SET state = ?, ended_at = ?, end_reason = ? WHERE id = ? AND ended_at IS NULL",
        )
        .bind(SessionState::Ended.as_str())
        .bind(ended_at)
        .bind(&reason)
        .bind(id)
        .execute(self.store.pool())
        .await
        {
            tracing::warn!(session = id, error = %e, "can't record the session's end");
        }
        {
            let mut all = self.live.lock().expect("sessions lock");
            all.retain(|other_id, other| {
                let other_info = other.info.lock().expect("session info");
                other_id == id
                    || other_info.is_running()
                    || other.ticket_id.is_none()
                    || other.ticket_id != live.ticket_id
                    || other_info.kind != info.kind
            });
        }
        tracing::info!(session = id, reason = %reason, "session ended");
        live.ended.send_replace(true);
        self.events.publish(Event::SessionState { session: info });
        let released = self.focus.session_gone(id);
        self.publish_released(released);
    }

    /// Tells clients that sessions lost their last focus holder, and which tickets lost focus.
    fn publish_released(&self, released: Vec<crate::attention::Released>) {
        for r in released {
            let ticket = self
                .live
                .lock()
                .expect("sessions lock")
                .get(&r.session)
                .and_then(|l| l.info.lock().expect("session info").ticket.clone());
            self.events.publish(Event::SessionFocus {
                session: r.session.clone(),
                ticket: ticket.clone(),
                focused: false,
            });
            if r.ticket_lost_focus {
                self.ticket_focus_changed(&r.ticket_id, ticket);
            }
        }
    }

    /// A ticket's focus changed: its card, inbox item and notification may change with it.
    fn ticket_focus_changed(&self, ticket_id: &str, key: Option<TicketKey>) {
        if let Some(key) = key {
            self.events.publish(Event::TicketChanged {
                project: key.project.to_string(),
                id: ticket_id.to_owned(),
                key,
            });
        }
    }
}

impl Drop for Inner {
    /// A manager dropped without [`Sessions::shutdown`] (tests) still leaves no process behind.
    fn drop(&mut self) {
        if let Ok(all) = self.live.lock() {
            for live in all.values() {
                live.kill_now();
            }
        }
    }
}

impl Live {
    /// The session as a running Project agent's root.
    fn project_agent_root(&self) -> Option<ProjectAgentRoot> {
        let info = self.info.lock().expect("session info");
        if !info.is_running() || info.kind != SessionKind::ProjectAgent {
            return None;
        }
        Some(ProjectAgentRoot {
            pid: self.pid?,
            session: info.id.clone(),
            project: info.project.clone(),
            agent: self
                .project_agent
                .lock()
                .expect("session project agent")
                .clone()?,
            epic: self.lead_epic.lock().expect("session lead epic").clone(),
        })
    }

    fn write(&self, id: &str, bytes: &[u8]) -> Result<(), SessionError> {
        if !self.info.lock().expect("session info").is_running() {
            return Err(SessionError::Ended(id.to_owned()));
        }
        let mut writer = self.writer.lock().expect("session writer");
        writer
            .write_all(bytes)
            .and_then(|()| writer.flush())
            .map_err(|e| SessionError::Io(e.to_string()))
    }

    fn stop(&self, reason: &'static str) {
        self.stop_reason
            .lock()
            .expect("stop reason")
            .get_or_insert(reason);
        self.group.hang_up();
        // Some programs ignore SIGHUP; the portable-pty killer is the fallback when the group is gone.
        let _ = self.killer.lock().expect("session killer").kill();
    }

    fn kill_now(&self) {
        if *self.ended.borrow() {
            return;
        }
        self.group.kill();
    }
}

/// The reader thread: PTY output → scrollback and attached clients, until end of file.
fn read_output(
    mut reader: Box<dyn Read + Send>,
    live: &Live,
    drained: std::sync::mpsc::Sender<()>,
) {
    let mut buf = vec![0u8; 16 * 1024];
    // Windows: ConPTY starts by asking the terminal where the cursor is and holds all output until it hears back.
    // The service answers that first question itself (top left), so a session runs before any terminal attaches,
    // and keeps it out of the scrollback, so no attached terminal answers it a second time.
    let mut startup_query = if cfg!(windows) { 4 } else { 0 };
    let mut queries = colors::QueryScanner::default();
    loop {
        match reader.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                let mut chunk = buf[..n].to_vec();
                if startup_query > 0 {
                    startup_query -= 1;
                    if let Some(at) = find(&chunk, CURSOR_POSITION_QUERY) {
                        chunk.drain(at..at + CURSOR_POSITION_QUERY.len());
                        startup_query = 0;
                        let mut writer = live.writer.lock().expect("session writer");
                        let _ = writer.write_all(b"\x1b[1;1R").and_then(|()| writer.flush());
                    }
                    if chunk.is_empty() {
                        continue;
                    }
                }
                let asked = queries.scan(&chunk);
                let data: Arc<[u8]> = Arc::from(chunk.as_slice());
                let mut output = live.output.lock().expect("session output");
                let offset = output.scrollback.push(&data);
                let _ = output.live.send(OutputChunk { offset, data });
                // Decided under the lock attaches take: a terminal attaching from now on sees these questions
                // only in its replay, and doesn't answer them.
                let answer: Vec<u8> = if output.answering == 0 {
                    asked.iter().flat_map(|q| q.answer(live.colors)).collect()
                } else {
                    Vec::new()
                };
                drop(output);
                if !answer.is_empty() {
                    let mut writer = live.writer.lock().expect("session writer");
                    let _ = writer.write_all(&answer).and_then(|()| writer.flush());
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
            // macOS reports EIO once the program and its children closed the terminal.
            Err(_) => break,
        }
    }
    let _ = drained.send(());
}

/// "Where is the cursor?" (DSR 6), which ConPTY sends when a session starts.
const CURSOR_POSITION_QUERY: &[u8] = b"\x1b[6n";

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

#[derive(sqlx::FromRow)]
struct SessionRow {
    id: String,
    kind: String,
    project_key: String,
    state: String,
    started_at: i64,
    ended_at: Option<i64>,
    end_reason: Option<String>,
    tool: Option<String>,
    tool_session_id: Option<String>,
    model: Option<String>,
    agent_id: Option<String>,
    agent_name: Option<String>,
    model_ran: Option<String>,
    effort: Option<String>,
    fallback_from: Option<String>,
    fallback_reason: Option<String>,
}

impl SessionRow {
    fn into_info(self, ticket: Option<TicketKey>) -> Option<SessionInfo> {
        Some(SessionInfo {
            id: self.id,
            kind: self.kind.parse().ok()?,
            project: self.project_key,
            ticket,
            state: self.state.parse().ok()?,
            started_at: self.started_at,
            ended_at: self.ended_at,
            end_reason: self.end_reason,
            tool: self.tool.and_then(|t| t.parse().ok()),
            tool_session_id: self.tool_session_id,
            model: self.model,
            agent: SessionAgent::of_columns(self.agent_id, self.agent_name),
            effort: self.effort,
            fallback: crate::agents::store::fallback_of(
                self.fallback_from.as_deref(),
                self.fallback_reason.as_deref(),
            ),
            model_ran: self.model_ran,
            size: None,
        })
    }
}

#[derive(Debug)]
pub enum SessionError {
    /// The ticket already has a running agent session (SES-04).
    AlreadyRunning {
        ticket: TicketKey,
        session: String,
    },
    NotFound(String),
    /// An automatic start in a paused project (PAU-04): the project's name.
    Paused {
        project: String,
    },
    /// The session has ended; it takes no input or resize.
    Ended(String),
    /// The program (or the launch's step before it, [`Sessions::prepared`]) couldn't start: why, or what the
    /// step printed.
    Spawn(String),
    Io(String),
    Store(StoreError),
}

impl fmt::Display for SessionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AlreadyRunning { ticket, .. } => {
                write!(f, "{ticket} already has a running agent session")
            }
            Self::NotFound(id) => write!(f, "no session {id}"),
            Self::Paused { project } => f.write_str(&crate::agents::pause::paused_refusal(project)),
            Self::Ended(id) => write!(f, "session {id} has ended"),
            Self::Spawn(message) | Self::Io(message) => f.write_str(message),
            Self::Store(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for SessionError {}

#[cfg(test)]
mod tests {
    use super::*;

    /// A step that takes longer than its limit is stopped and fails the start.
    #[tokio::test]
    async fn a_step_that_takes_too_long_fails() {
        let step = PrepareStep {
            program: "sh".into(),
            args: vec!["-c".into(), "sleep 5".into()],
            cwd: std::env::temp_dir(),
            env: Vec::new(),
        };
        let started = std::time::Instant::now();
        let err = run_prepare(&step, Duration::from_millis(200))
            .await
            .unwrap_err();
        assert_eq!(err.to_string(), "sh didn't finish within 0 s");
        assert!(started.elapsed() < Duration::from_secs(3));
    }
}
