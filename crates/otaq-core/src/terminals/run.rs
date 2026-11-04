//! Run (RUN-01; d0035, d0026, d0045): the project's **run command** in a ticket's worktree, with `$PORT` = the
//! ticket's **Run port** (4020 for SHOP-19, never the agent's 4019). The user's click or the ticket's own agent
//! (`otaq run start`, agents run the app) starts it; Stop ends it; reaching Done or Close stops it (with a toast); a restart leaves
//! it "stopped by restart" and nothing starts it again.
//!
//! **Who started it** (agents run the app): one Run per ticket, shared by the user and the ticket's agents (its Dev agent, its
//! check). The Run records who started it ("started by Dev"). An agent uses a Run the user started and never stops
//! it; it stops only a Run an agent of its ticket started. Otaq stops an agent's Run when that agent's stage ends or
//! its verdict lands ([`Runs::stage_ended`], "stage ended"); a Run the user started is never stopped that way. The
//! processes of an agent's Run act for that agent ([`crate::session::ActingFor`]), never as the user.
//!
//! A Run is a service-owned PTY session of kind `run` ([`SessionKind::Run`]): its output is the session's bounded
//! scrollback, streamed to the app like any session's (`session.attach`, `session_output`), and Stop sends the
//! session's whole process group SIGHUP (SIGKILL after a grace period). The command runs as
//! `/bin/sh -c '<command>'`, like the setup command; a first line `$ pnpm dev --port 4020` shows what ran, and
//! Otaq adds a last line saying how it ended ("[otaq] exited with code 1").
//!
//! States (*13 · Run & Shell*): not started → **starting** (the process runs; the port doesn't answer yet) →
//! **running** (`▶ :4020`, a connection to the port succeeds) → **stopped** (by you, at Done or Park, or the program exited
//! after it ran). A program that exits while still starting with a non-zero code **failed to start**. The Run port
//! already held by another program is **port in use**, found before anything starts; Otaq never kills it.
//!
//! A Run of a ticket whose build was freed at Review brings it back first (M19-c, [`TicketEnv::fetch_build`]); a
//! failure there is logged and the Run starts anyway. When the Run ends the build is freed again.
//!
//! Implementation choices (SDD §26): states live in the service's memory; the port is polled every 250 ms while
//! starting, on 127.0.0.1 and ::1. Output is never written to disk (d0045), so after a restart the Run tab shows
//! "stopped by restart" without the old output (Q28).

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::{Deserialize, Serialize};

use super::{TerminalEnv, with_port};
use crate::board::settings::RunCommandSetBy;
use crate::board::tickets::Ticket;
use crate::board::{Board, BoardError, Refusal};
use crate::data_dir::DataDir;
use crate::domain::{Status, TicketKey};
use crate::naming;
use crate::ports;
use crate::protocol::Event;
use crate::review::finish::FinishedTicket;
use crate::service::EventBus;
use crate::session::{
    ActingFor, END_STOPPED_BY_RESTART, SessionInfo, SessionKind, SessionTicket, Sessions,
    SpawnSpec, Start, TerminalSize,
};
use crate::store::now_ms;
use crate::ticket_env::TicketEnv;
use crate::ticket_env::fetch::FetchWhy;

/// How often a starting Run's port is tried.
const PORT_POLL: Duration = Duration::from_millis(250);
/// How long one try waits for the port to answer.
const PORT_TRY: Duration = Duration::from_millis(200);
/// The size a Run's PTY starts with; the Run tab resizes it.
const RUN_SIZE: TerminalSize = TerminalSize {
    cols: 120,
    rows: 32,
};

/// Where a ticket's Run is (*13 · Run & Shell*).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunState {
    /// Nothing ran yet: "Not running".
    NotStarted,
    /// The process runs; its port doesn't answer yet.
    Starting,
    /// The port answers: `▶ :4020`.
    Running,
    /// Stopped by you, at Done or Close, or the program exited after it ran. The output is kept.
    Stopped,
    /// The program exited while starting, with a non-zero code: "Failed to start · exit 1".
    FailedToStart,
    /// Another program holds the Run port; nothing was started.
    PortInUse,
    /// A restart (reboot, crash, service restart) ended it; it never starts again on its own (d0045).
    StoppedByRestart,
}

impl RunState {
    /// The process is up: the card's chip, the sheet's pill and the header's count show it.
    pub fn is_live(self) -> bool {
        matches!(self, Self::Starting | Self::Running)
    }
}

/// Who or what stopped a Run that didn't end on its own.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "by", rename_all = "snake_case")]
pub enum StopCause {
    /// The user's Stop.
    You,
    /// The ticket reached Done or was closed (REC-05).
    Finished {
        status: Status,
    },
    /// The ticket was parked: its ports return to the pool (d0052, Q30).
    Parked,
    Restart,
    /// An agent of the ticket stopped it (`otaq run stop`, agents run the app): `stage` names it ("Dev").
    Agent {
        stage: String,
    },
    /// The agent that started it ended its stage (`stage next`, `stage done`) or gave its verdict (agents run the app).
    StageEnded {
        stage: String,
    },
}

/// Who started a Run (agents run the app): your click, or one of the ticket's agents, named by its stage ("Dev", "Validate").
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "by", rename_all = "snake_case")]
pub enum StartedBy {
    You,
    Agent { stage: String },
}

impl StartedBy {
    /// "started by Dev" for an agent's Run; `None` for yours (the board says nothing extra).
    pub fn agent_label(&self) -> Option<String> {
        match self {
            Self::You => None,
            Self::Agent { stage } => Some(format!("started by {stage}")),
        }
    }
}

/// The ticket agent asking to start or stop a Run (agents run the app): its session, its stage's name ("Dev") and the session
/// kind its processes are callers of.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunAgent {
    pub session: String,
    pub stage: String,
    pub kind: SessionKind,
}

/// A ticket's Run, as clients see it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunInfo {
    /// The project's key.
    pub project: String,
    /// The project's name, for the header's list ("shop-web").
    pub project_name: String,
    pub key: TicketKey,
    pub title: String,
    pub state: RunState,
    /// The Run port (`$PORT` in Run and Shell).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,
    /// The run command as configured, with `$PORT` (empty: none set yet, the UI asks for one, RUN-02).
    pub command: String,
    /// The project's Open URL, with `$PORT` (RUN-03); `None` when the project has none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub open_url: Option<String>,
    /// The Run's session, for `session.attach` (its output).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub started_at: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ended_at: Option<i64>,
    /// How the program exited, when it exited on its own.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stopped_by: Option<StopCause>,
    /// Who started the last Run (agents run the app); `None` before any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub started_by: Option<StartedBy>,
    /// The agent that set the project's run command ("Set by Dev on SHOP-27", with Undo; agents run the app).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command_set_by: Option<RunCommandSetBy>,
}

/// What `otaq run log` prints (agents run the app): the Run's kept output as text, its last `tail` lines. Never read from disk:
/// Otaq writes no Run output there (d0045).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunLog {
    pub key: TicketKey,
    pub state: RunState,
    pub text: String,
}

/// How many lines `otaq run log` prints by default.
pub const LOG_TAIL: usize = 200;

impl RunInfo {
    /// The Open URL with the Run port in it, when the project sets one and the Run has a port.
    pub fn url(&self) -> Option<String> {
        Some(with_port(self.open_url.as_deref()?, self.port?))
    }

    /// `otaq run status` in words (agents run the app): "not started", "starting :4020 (started by Dev)", "running :4020 (started
    /// by the user)", "stopped (Dev's stage ended)", "failed to start (exit 1)", "port :4020 in use", "stopped by
    /// restart".
    pub fn status_text(&self) -> String {
        let port = self.port.map_or_else(String::new, |p| format!(" :{p}"));
        let by = match &self.started_by {
            Some(StartedBy::Agent { stage }) => format!(" (started by {stage})"),
            Some(StartedBy::You) => " (started by the user)".to_owned(),
            None => String::new(),
        };
        match self.state {
            RunState::NotStarted => "not started".to_owned(),
            RunState::Starting => format!("starting{port}{by}"),
            RunState::Running => format!("running{port}{by}"),
            RunState::Stopped => {
                let how = match (&self.stopped_by, self.exit_code) {
                    (Some(StopCause::You), _) => "by the user".to_owned(),
                    (Some(StopCause::Agent { stage }), _) => format!("by {stage}"),
                    (Some(StopCause::StageEnded { stage }), _) => format!("{stage}'s stage ended"),
                    (Some(StopCause::Finished { status }), _) => format!(
                        "{} is {}",
                        self.key,
                        if *status == Status::Merged {
                            "done"
                        } else {
                            "closed"
                        }
                    ),
                    (Some(StopCause::Parked), _) => format!("{} was parked", self.key),
                    (Some(StopCause::Restart), _) => "by restart".to_owned(),
                    (None, Some(code)) => format!("exited with code {code}"),
                    (None, None) => "ended".to_owned(),
                };
                format!("stopped ({how})")
            }
            RunState::FailedToStart => match self.exit_code {
                Some(code) => format!("failed to start (exit {code})"),
                None => "failed to start".to_owned(),
            },
            RunState::PortInUse => format!("port{port} in use"),
            RunState::StoppedByRestart => "stopped by restart".to_owned(),
        }
    }
}

/// Tickets whose Run is up, with its port: the card's `▶ :4020` flag ([`crate::domain::Flags::run_port`]). The
/// board reads it when it builds tickets; [`Runs`] keeps it. Cheap to clone.
#[derive(Debug, Clone, Default)]
pub struct RunPorts {
    inner: Arc<Mutex<HashMap<String, RunPort>>>,
}

/// A live Run's port and the stage of the agent that started it (`None`: you did).
type RunPort = (u16, Option<String>);

impl RunPorts {
    pub fn get(&self, ticket_id: &str) -> Option<u16> {
        self.get_with_starter(ticket_id).map(|(port, _)| port)
    }

    /// The port, and the stage of the agent that started the Run ("Dev"; `None` when you did, agents run the app).
    pub fn get_with_starter(&self, ticket_id: &str) -> Option<(u16, Option<String>)> {
        self.inner
            .lock()
            .expect("run ports")
            .get(ticket_id)
            .cloned()
    }

    fn set(&self, ticket_id: &str, port: Option<(u16, Option<String>)>) {
        let mut all = self.inner.lock().expect("run ports");
        match port {
            Some(port) => all.insert(ticket_id.to_owned(), port),
            None => all.remove(ticket_id),
        };
    }
}

/// What the service remembers of a ticket's last Run.
#[derive(Debug, Clone)]
struct Record {
    key: TicketKey,
    state: RunState,
    port: Option<u16>,
    command: String,
    session: Option<String>,
    started_at: Option<i64>,
    ended_at: Option<i64>,
    exit_code: Option<i32>,
    stopped_by: Option<StopCause>,
    /// The agent that started it; `None`: you did (agents run the app).
    agent: Option<RunAgent>,
    /// The command an agent is trying (`otaq run set`, agents run the app): saved as the project's once this Run is running.
    trying: Option<Trying>,
}

/// A run command an agent is trying for a project that has none (agents run the app).
#[derive(Debug, Clone)]
struct Trying {
    open_url: Option<String>,
    by: RunCommandSetBy,
}

impl Record {
    fn started_by(&self) -> Option<StartedBy> {
        self.session.as_ref()?;
        Some(match &self.agent {
            Some(a) => StartedBy::Agent {
                stage: a.stage.clone(),
            },
            None => StartedBy::You,
        })
    }
}

/// Starts, watches and stops Runs. Cheap to clone.
#[derive(Clone)]
pub struct Runs {
    inner: Arc<Inner>,
}

struct Inner {
    board: Board,
    sessions: Sessions,
    data: DataDir,
    events: EventBus,
    env: TerminalEnv,
    /// Brings a freed build back before a Run (M19-c).
    ticket_env: TicketEnv,
    /// By ticket id.
    records: Mutex<HashMap<String, Record>>,
}

impl Runs {
    pub fn new(
        board: Board,
        sessions: Sessions,
        data: DataDir,
        events: EventBus,
        env: TerminalEnv,
        ticket_env: TicketEnv,
    ) -> Self {
        Self {
            inner: Arc::new(Inner {
                board,
                sessions,
                data,
                events,
                env,
                ticket_env,
                records: Mutex::new(HashMap::new()),
            }),
        }
    }

    /// Starts the ticket's Run (the user's click). A Run already up is left alone. Refused without a worktree
    /// (the ticket never started) or without a run command (the UI then asks for one, RUN-02). When another
    /// program holds the Run port, nothing starts and the state is "port in use".
    pub async fn start(&self, key: &str) -> Result<RunInfo, BoardError> {
        self.start_as(key, None).await
    }

    /// [`Runs::start`] for `agent`, one of the ticket's agents (`otaq run start`, agents run the app), or for you (`None`). A
    /// Run already up, whoever started it, is left alone and used.
    pub async fn start_as(
        &self,
        key: &str,
        agent: Option<RunAgent>,
    ) -> Result<RunInfo, BoardError> {
        self.start_with(key, agent, None).await
    }

    /// `otaq run set --command … [--open-url …]` (agents run the app): for a project with no run command, `agent` starts
    /// `command` as this ticket's Run; it becomes the project's run command once the Run is running ("Set by Dev on
    /// SHOP-27"). Failed to start: nothing is saved. Refused when the project already has one: an agent never changes
    /// a command the user set.
    pub async fn set_command(
        &self,
        key: &str,
        agent: RunAgent,
        command: &str,
        open_url: Option<&str>,
    ) -> Result<RunInfo, BoardError> {
        let ticket = self.inner.board.ticket(key).await?;
        let project = self
            .inner
            .board
            .project(ticket.key.project.as_str())
            .await?;
        let settings = self.inner.board.project_settings(&project.key).await?;
        let existing = settings.run_command.trim();
        if !existing.is_empty() {
            return Err(BoardError::refused(
                Refusal::NotAllowed,
                format!(
                    "{} already has a run command (`{existing}`): an agent never changes one. Start it with `{} run \
                     start --wait`",
                    project.name,
                    naming::COMMAND_NAME
                ),
            ));
        }
        let command = command.trim();
        if command.is_empty() {
            return Err(BoardError::refused(
                Refusal::NotAllowed,
                "the run command is empty".to_owned(),
            ));
        }
        if self.record(&ticket.id).is_some_and(|r| r.state.is_live()) {
            return Err(BoardError::refused(
                Refusal::NotAllowed,
                format!(
                    "{}'s Run is up: stop it first (`{} run stop`)",
                    ticket.key,
                    naming::COMMAND_NAME
                ),
            ));
        }
        let trying = Trying {
            open_url: open_url
                .map(str::trim)
                .filter(|u| !u.is_empty())
                .map(str::to_owned),
            by: RunCommandSetBy {
                stage: agent.stage.clone(),
                ticket: ticket.key.clone(),
                at: now_ms(),
                open_url: false,
            },
        };
        self.start_with(key, Some(agent), Some((command.to_owned(), trying)))
            .await
    }

    async fn start_with(
        &self,
        key: &str,
        agent: Option<RunAgent>,
        trying: Option<(String, Trying)>,
    ) -> Result<RunInfo, BoardError> {
        let inner = &self.inner;
        let ticket = inner.board.ticket(key).await?;
        if self.record(&ticket.id).is_some_and(|r| r.state.is_live()) {
            return self.info_of(&ticket).await;
        }
        let worktree = ticket
            .work
            .as_ref()
            .and_then(|w| w.worktree.clone())
            .map(PathBuf::from)
            .filter(|w| w.is_dir());
        let port = ticket.work.as_ref().and_then(|w| w.run_port);
        let (Some(worktree), Some(port)) = (worktree, port) else {
            return Err(BoardError::refused(
                Refusal::NoWorktree,
                format!(
                    "{} has no worktree yet. Run works once the ticket has started.",
                    ticket.key
                ),
            ));
        };
        let project = inner.board.project(ticket.key.project.as_str()).await?;
        let settings = inner.board.project_settings(&project.key).await?;
        let (command, trying) = match trying {
            Some((command, trying)) => (command, Some(trying)),
            None => (settings.run_command.trim().to_owned(), None),
        };
        if command.is_empty() {
            return Err(BoardError::refused(
                Refusal::NoRunCommand,
                no_run_command(&project.name, agent.is_some()),
            ));
        }

        let mut record = Record {
            key: ticket.key.clone(),
            state: RunState::PortInUse,
            port: Some(port),
            command: command.clone(),
            session: None,
            started_at: None,
            ended_at: None,
            exit_code: None,
            stopped_by: None,
            agent: agent.clone(),
            trying,
        };
        if !ports::is_free_on_machine_settled(port).await {
            tracing::info!(ticket = %ticket.key, port, "run port in use");
            self.store(&ticket.id, record);
            self.publish(&ticket);
            return self.info_of(&ticket).await;
        }

        // A build freed at Review comes back first; the ticket's lock is held until the Run is open, so no Free
        // takes it away before the Run holds it.
        let (fetched, _held) = inner
            .ticket_env
            .fetch_for_terminal(&ticket, FetchWhy::Run)
            .await;
        if let Err(e) = fetched {
            let log = inner.data.setup_log_path(&ticket.key.to_string()).ok();
            tracing::warn!(ticket = %ticket.key, error = %e, log = ?log, "can't bring the build back; the Run starts anyway");
        }
        // A second click waited for the lock while the first brought the build back and started the Run.
        if self.record(&ticket.id).is_some_and(|r| r.state.is_live()) {
            return self.info_of(&ticket).await;
        }

        // Windows' cmd has no `$PORT`: the documented placeholder is filled in there (the variable is set too).
        let run = if cfg!(windows) {
            with_port(&command, port)
        } else {
            command.clone()
        };
        let shell = crate::shell::one_line_shown(&run, &with_port(&command, port));
        let mut env = inner.env.env(&inner.data, Some(&ticket.key), Some(port));
        env.extend(shell.env);
        let mut prepared = inner.sessions.prepared(None);
        if let Some(a) = &agent {
            prepared = prepared.acting_for(ActingFor {
                kind: a.kind,
                session: a.session.clone(),
                ticket: ticket.key.clone(),
                ticket_id: ticket.id.clone(),
            });
        }
        let session = prepared
            .spawn(SpawnSpec {
                kind: SessionKind::Run,
                start: Start::User,
                project_id: project.id.clone(),
                project_key: project.key.clone(),
                ticket: Some(SessionTicket {
                    id: ticket.id.clone(),
                    key: ticket.key.clone(),
                }),
                program: shell.program,
                args: shell.args,
                cwd: worktree,
                env,
                size: RUN_SIZE,
                tool: None,
                tool_session_id: None,
                model: None,
                agent: None,
                effort: None,
                fallback: None,
                theme: None,
            })
            .await
            .map_err(|e| BoardError::refused(Refusal::SessionEnded, e.to_string()))?;
        record.state = RunState::Starting;
        record.session = Some(session.id.clone());
        record.started_at = Some(session.started_at);
        self.store(&ticket.id, record);
        let stage = agent.as_ref().map(|a| a.stage.clone());
        tracing::info!(ticket = %ticket.key, port, by = ?stage, "run started");
        inner.board.run_ports().set(&ticket.id, Some((port, stage)));
        self.publish(&ticket);
        tokio::spawn(self.clone().watch(ticket.id.clone(), session.id, port));
        self.info_of(&ticket).await
    }

    /// Stops the ticket's Run (the user's Stop): its whole process group ends.
    pub async fn stop(&self, key: &str) -> Result<RunInfo, BoardError> {
        self.stop_as(key, None).await
    }

    /// [`Runs::stop`] for `agent` (`otaq run stop`, agents run the app), or for you (`None`). An agent stops only a Run an agent
    /// of its ticket started: the user's is refused, and stays up.
    pub async fn stop_as(
        &self,
        key: &str,
        agent: Option<&RunAgent>,
    ) -> Result<RunInfo, BoardError> {
        let ticket = self.inner.board.ticket(key).await?;
        let cause = match agent {
            None => StopCause::You,
            Some(a) => {
                if let Some(r) = self.record(&ticket.id)
                    && r.state.is_live()
                    && r.agent.is_none()
                {
                    return Err(BoardError::refused(
                        Refusal::NotAllowed,
                        user_run_refusal(&ticket.key),
                    ));
                }
                StopCause::Agent {
                    stage: a.stage.clone(),
                }
            }
        };
        self.stop_with(&ticket.id, cause).await;
        self.info_of(&ticket).await
    }

    /// The agent of `session` ended its stage or gave its verdict (agents run the app): a Run it started stops ("stage ended").
    /// A Run the user started, or another agent's, is left alone. `true` when one stopped.
    pub async fn stage_ended(&self, session: &str) -> bool {
        let started: Vec<(String, String)> = self
            .inner
            .records
            .lock()
            .expect("runs")
            .iter()
            .filter(|(_, r)| r.state.is_live())
            .filter_map(|(id, r)| {
                let a = r.agent.as_ref().filter(|a| a.session == session)?;
                Some((id.clone(), a.stage.clone()))
            })
            .collect();
        let mut stopped = false;
        for (ticket_id, stage) in started {
            stopped |= self
                .stop_with(&ticket_id, StopCause::StageEnded { stage })
                .await;
        }
        stopped
    }

    /// The Run's kept output as text, its last `tail` lines (`otaq run log`, agents run the app): the Run now or the last one
    /// still in memory. Refused before any Run, and after a restart (output never goes to disk, d0045).
    pub async fn log(&self, key: &str, tail: usize) -> Result<RunLog, BoardError> {
        let ticket = self.inner.board.ticket(key).await?;
        let record = self.record(&ticket.id);
        let output = record
            .as_ref()
            .and_then(|r| r.session.as_deref())
            .and_then(|s| self.inner.sessions.output_of_session(s));
        let Some((_, bytes, size)) = output else {
            let message = match record.map(|r| r.state) {
                Some(RunState::StoppedByRestart) => {
                    format!("{}'s Run output isn't kept after a restart.", ticket.key)
                }
                Some(RunState::PortInUse) => format!(
                    "{}'s Run didn't start: its port is in use, so there is no output.",
                    ticket.key
                ),
                _ => format!(
                    "{}'s Run hasn't run yet, so there is no output.",
                    ticket.key
                ),
            };
            return Err(BoardError::refused(Refusal::SessionEnded, message));
        };
        let text = crate::session::screen::render_above(&bytes, size, tail);
        let lines: Vec<&str> = text.lines().collect();
        let text = lines[lines.len().saturating_sub(tail)..].join("\n");
        let state = self.info_of(&ticket).await?.state;
        Ok(RunLog {
            key: ticket.key,
            state,
            text,
        })
    }

    /// A ticket reached Done or was closed (REC-05): its Run stops and a toast says so.
    pub async fn finished(&self, ticket: &FinishedTicket) {
        if !self
            .stop_with(
                &ticket.id,
                StopCause::Finished {
                    status: ticket.status,
                },
            )
            .await
        {
            return;
        }
        let what = if ticket.status == Status::Merged {
            "done"
        } else {
            "closed"
        };
        self.inner.events.publish(Event::RunStopped {
            project: ticket.key.project.to_string(),
            key: ticket.key.clone(),
            message: format!("Run stopped · {} is {what}", ticket.key),
        });
    }

    /// A ticket was parked (d0052): its Run stops before the ports are freed, so the freed Run port is really free
    /// (Q30). No toast: the Park confirmation already says the ticket's work stops.
    pub async fn parked(&self, ticket: &FinishedTicket) {
        self.stop_with(&ticket.id, StopCause::Parked).await;
    }

    /// A restart stopped this Run session (REC-01, d0045): the Run shows "stopped by restart".
    pub fn stopped_by_restart(&self, info: &SessionInfo) {
        if info.kind != SessionKind::Run {
            return;
        }
        let Some(key) = info.ticket.clone() else {
            return;
        };
        let runs = self.clone();
        let info = info.clone();
        tokio::spawn(async move {
            let Ok(ticket) = runs.inner.board.ticket(&key.to_string()).await else {
                return;
            };
            if runs.record(&ticket.id).is_some() {
                return;
            }
            let settings = runs
                .inner
                .board
                .project_settings(key.project.as_str())
                .await
                .unwrap_or_default();
            runs.store(
                &ticket.id,
                Record {
                    key,
                    state: RunState::StoppedByRestart,
                    port: ticket.work.as_ref().and_then(|w| w.run_port),
                    command: settings.run_command.trim().to_owned(),
                    session: None,
                    started_at: Some(info.started_at),
                    ended_at: info.ended_at,
                    exit_code: None,
                    stopped_by: Some(StopCause::Restart),
                    agent: None,
                    trying: None,
                },
            );
            runs.publish(&ticket);
        });
    }

    /// The ticket's Run now.
    pub async fn info(&self, key: &str) -> Result<RunInfo, BoardError> {
        let ticket = self.inner.board.ticket(key).await?;
        self.info_of(&ticket).await
    }

    /// Every Run that is up, across projects, oldest first (RUN-04).
    pub async fn running(&self) -> Vec<RunInfo> {
        let live: Vec<String> = self
            .inner
            .records
            .lock()
            .expect("runs")
            .iter()
            .filter(|(_, r)| r.state.is_live())
            .map(|(id, _)| id.clone())
            .collect();
        let mut all = Vec::new();
        for id in live {
            if let Ok(ticket) = self.inner.board.ticket_by_id(&id).await
                && let Ok(info) = self.info_of(&ticket).await
            {
                all.push(info);
            }
        }
        all.sort_by(|a, b| a.started_at.cmp(&b.started_at).then(a.key.cmp(&b.key)));
        all
    }

    /// Stops a live Run for `cause`; `true` when one was up.
    async fn stop_with(&self, ticket_id: &str, cause: StopCause) -> bool {
        let session = {
            let mut records = self.inner.records.lock().expect("runs");
            match records.get_mut(ticket_id) {
                Some(r) if r.state.is_live() => {
                    r.stopped_by = Some(cause);
                    r.session.clone()
                }
                _ => return false,
            }
        };
        let Some(session) = session else {
            return false;
        };
        if let Err(e) = self.inner.sessions.stop(&session).await {
            tracing::warn!(session, error = %e, "can't stop the run");
        }
        if let Ok(info) = self.inner.sessions.info(&session) {
            self.ended(ticket_id, &info).await;
        }
        true
    }

    /// Follows a Run until its session ends: starting becomes running once the port answers.
    async fn watch(self, ticket_id: String, session: String, port: u16) {
        let sessions = self.inner.sessions.clone();
        let ended = sessions.wait_ended(&session);
        tokio::pin!(ended);
        let mut tick = tokio::time::interval(PORT_POLL);
        loop {
            let starting = self
                .record(&ticket_id)
                .is_some_and(|r| r.state == RunState::Starting);
            tokio::select! {
                info = &mut ended => {
                    if let Some(info) = info {
                        self.ended(&ticket_id, &info).await;
                    }
                    return;
                }
                _ = tick.tick(), if starting => {
                    if port_answers(port).await {
                        self.became_running(&ticket_id, &session).await;
                    }
                }
            }
        }
    }

    async fn became_running(&self, ticket_id: &str, session: &str) {
        let tried = {
            let mut records = self.inner.records.lock().expect("runs");
            match records.get_mut(ticket_id) {
                Some(r)
                    if r.state == RunState::Starting && r.session.as_deref() == Some(session) =>
                {
                    r.trying
                        .take()
                        .map(|t| (r.key.clone(), r.command.clone(), t))
                }
                _ => return,
            }
        };
        // The command an agent tried runs: it becomes the project's (agents run the app), unless one was set meanwhile.
        if let Some((key, command, trying)) = tried {
            match self
                .inner
                .board
                .save_agent_run_command(
                    key.project.as_str(),
                    &command,
                    trying.open_url.as_deref(),
                    trying.by,
                )
                .await
            {
                Ok(true) => {
                    tracing::info!(ticket = %key, %command, "run command set by the agent");
                    // Settings pages refetch the project's settings: "Set by Dev on SHOP-27" shows.
                    self.inner.events.publish(Event::ProjectsChanged);
                }
                Ok(false) => {
                    tracing::info!(ticket = %key, "a run command was set meanwhile; the agent's isn't saved")
                }
                Err(e) => {
                    tracing::warn!(ticket = %key, error = %e, "can't save the agent's run command")
                }
            }
        }
        // Running only now, once a tried command is saved: `run set` sees it saved when it sees running.
        {
            let mut records = self.inner.records.lock().expect("runs");
            match records.get_mut(ticket_id) {
                Some(r)
                    if r.state == RunState::Starting && r.session.as_deref() == Some(session) =>
                {
                    r.state = RunState::Running;
                }
                _ => return,
            }
        }
        if let Ok(ticket) = self.inner.board.ticket_by_id(ticket_id).await {
            tracing::info!(ticket = %ticket.key, "run is up");
            self.publish(&ticket);
        }
    }

    /// Records how a Run's session ended (once): its state, its last line, the card's chip gone.
    async fn ended(&self, ticket_id: &str, info: &SessionInfo) {
        let reason = info.end_reason.clone().unwrap_or_default();
        let exit_code = reason
            .strip_prefix("exit ")
            .and_then(|code| code.trim().parse::<i32>().ok());
        let line = {
            let mut records = self.inner.records.lock().expect("runs");
            let Some(r) = records.get_mut(ticket_id) else {
                return;
            };
            if !r.state.is_live() || r.session.as_deref() != Some(info.id.as_str()) {
                return;
            }
            if reason == END_STOPPED_BY_RESTART && r.stopped_by.is_none() {
                r.stopped_by = Some(StopCause::Restart);
            }
            r.ended_at = Some(info.ended_at.unwrap_or_else(now_ms));
            let (state, line) = match r.stopped_by.clone() {
                Some(StopCause::You) => (RunState::Stopped, "stopped by you".to_owned()),
                Some(StopCause::Finished { status }) => (
                    RunState::Stopped,
                    format!(
                        "stopped · {} is {}",
                        r.key,
                        if status == Status::Merged {
                            "done"
                        } else {
                            "closed"
                        }
                    ),
                ),
                Some(StopCause::Parked) => {
                    (RunState::Stopped, format!("stopped · {} was parked", r.key))
                }
                Some(StopCause::Restart) => {
                    (RunState::StoppedByRestart, "stopped by restart".to_owned())
                }
                Some(StopCause::Agent { stage }) => {
                    (RunState::Stopped, format!("stopped by {stage}"))
                }
                Some(StopCause::StageEnded { stage }) => (
                    RunState::Stopped,
                    format!("stopped · {stage}'s stage ended"),
                ),
                None => {
                    r.exit_code = exit_code;
                    let line = match exit_code {
                        Some(code) => format!("exited with code {code}"),
                        None => format!("ended ({reason})"),
                    };
                    let failed = r.state == RunState::Starting && exit_code != Some(0);
                    (
                        if failed {
                            RunState::FailedToStart
                        } else {
                            RunState::Stopped
                        },
                        line,
                    )
                }
            };
            r.state = state;
            line
        };
        let _ = self
            .inner
            .sessions
            .append_line(&info.id, &format!("[{}] {line}", naming::COMMAND_NAME));
        self.inner.board.run_ports().set(ticket_id, None);
        if let Ok(ticket) = self.inner.board.ticket_by_id(ticket_id).await {
            tracing::info!(ticket = %ticket.key, %line, "run ended");
            self.publish(&ticket);
        }
    }

    async fn info_of(&self, ticket: &Ticket) -> Result<RunInfo, BoardError> {
        let project = self
            .inner
            .board
            .project(ticket.key.project.as_str())
            .await?;
        let settings = self.inner.board.project_settings(&project.key).await?;
        let record = self.record(&ticket.id);
        let open_url = record
            .as_ref()
            .and_then(|r| r.trying.as_ref())
            .and_then(|t| t.open_url.clone())
            .or_else(|| Some(settings.open_url.trim().to_owned()).filter(|u| !u.is_empty()));
        let mut info = RunInfo {
            project: project.key.clone(),
            project_name: project.name.clone(),
            key: ticket.key.clone(),
            title: ticket.title.clone(),
            state: RunState::NotStarted,
            port: ticket.work.as_ref().and_then(|w| w.run_port),
            command: settings.run_command.trim().to_owned(),
            open_url,
            session: None,
            started_at: None,
            ended_at: None,
            exit_code: None,
            stopped_by: None,
            started_by: None,
            command_set_by: settings.run_command_set_by.clone(),
        };
        if let Some(r) = record {
            info.started_by = r.started_by();
            info.state = r.state;
            info.port = r.port.or(info.port);
            if r.state.is_live() {
                info.command = r.command;
            }
            info.session = r.session;
            info.started_at = r.started_at;
            info.ended_at = r.ended_at;
            info.exit_code = r.exit_code;
            info.stopped_by = r.stopped_by;
        }
        Ok(info)
    }

    fn record(&self, ticket_id: &str) -> Option<Record> {
        self.inner
            .records
            .lock()
            .expect("runs")
            .get(ticket_id)
            .cloned()
    }

    fn store(&self, ticket_id: &str, record: Record) {
        self.inner
            .records
            .lock()
            .expect("runs")
            .insert(ticket_id.to_owned(), record);
    }

    /// The Run changed: the Run tab, the header's list and the ticket's card (its chip) follow.
    fn publish(&self, ticket: &Ticket) {
        let project = ticket.key.project.to_string();
        self.inner.events.publish(Event::RunChanged {
            project: project.clone(),
            key: ticket.key.clone(),
        });
        self.inner.events.publish(Event::TicketChanged {
            project,
            id: ticket.id.clone(),
            key: ticket.key.clone(),
        });
    }
}

/// Whether something accepts connections on the port (the Run is up).
async fn port_answers(port: u16) -> bool {
    for host in ["127.0.0.1", "[::1]"] {
        let attempt = tokio::net::TcpStream::connect(format!("{host}:{port}"));
        if let Ok(Ok(_)) = tokio::time::timeout(PORT_TRY, attempt).await {
            return true;
        }
    }
    false
}

/// The refusal when the project has no run command; an agent is told how to add one (agents run the app).
fn no_run_command(project: &str, agent: bool) -> String {
    if agent {
        format!(
            "{project} has no run command yet: if you know how this project starts, run `{} run set --command …`",
            naming::COMMAND_NAME
        )
    } else {
        format!("{project} has no run command yet")
    }
}

/// An agent's `otaq run stop` of a Run the user started (agents run the app).
pub fn user_run_refusal(key: &TicketKey) -> String {
    format!(
        "{key}'s Run was started by the user, so it stays running: an agent stops only a Run an agent started. \
         Use it as it is."
    )
}
