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
//! **Apps** (agents run the app, monorepos): a project's run command is a list of named apps (`web`, `api`), each with its own Run
//! per ticket and its own port: `$PORT` in its Run, and `OTAQ_PORT_<NAME>` for every app in every app's Run. Every
//! request names the app, or means the only one; with several, a request without a name is refused with their names.
//! A project with one app works as before apps had names (its app is [`DEFAULT_APP`]).
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

use super::{TerminalEnv, app_port_vars, with_ports};
use crate::board::settings::{DEFAULT_APP, ProjectSettings, RunCommandSetBy};
use crate::board::tickets::Ticket;
use crate::board::{Board, BoardError, Refusal};
use crate::data_dir::DataDir;
use crate::domain::{RunningApp, Status, TicketKey};
use crate::naming;
use crate::ports::{self, PortPool};
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
    /// An agent of the ticket restarted it to run its new code (`otaq run restart`, d{RUN}); the Run that follows
    /// keeps its owner.
    Restarted {
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
    /// The app (agents run the app): `web`; [`DEFAULT_APP`] in a project with one app nobody named.
    pub app: String,
    pub state: RunState,
    /// The app's Run port (`$PORT` in its Run).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,
    /// The app's run command as configured, with `$PORT` (empty: none set yet, the UI asks for one, RUN-02).
    pub command: String,
    /// The app's Open URL, with `$PORT` (RUN-03); `None` when it has none.
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
    /// The agent that set the app's run command ("Set by Dev on SHOP-27", with Undo; agents run the app).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command_set_by: Option<RunCommandSetBy>,
}

/// What `otaq run log` prints (agents run the app): the Run's kept output as text, its last `tail` lines. Never read from disk:
/// Otaq writes no Run output there (d0045).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunLog {
    pub key: TicketKey,
    pub app: String,
    pub state: RunState,
    pub text: String,
}

/// How many lines `otaq run log` prints by default.
pub const LOG_TAIL: usize = 200;

impl RunInfo {
    /// The Open URL with the Run port in it, when the project sets one and the Run has a port.
    pub fn url(&self) -> Option<String> {
        Some(super::with_port(self.open_url.as_deref()?, self.port?))
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
                    (Some(StopCause::Restarted { stage }), _) => format!("restarted by {stage}"),
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

/// Tickets' apps whose Run is up, with their ports: the card's `▶ :4020` flag ([`crate::domain::Flags::run_apps`]).
/// The board reads it when it builds tickets; [`Runs`] keeps it. Cheap to clone.
#[derive(Debug, Clone, Default)]
pub struct RunPorts {
    inner: Arc<Mutex<HashMap<String, Vec<RunningApp>>>>,
}

impl RunPorts {
    /// The ticket's apps that run, lowest port first (the order their ports were given, so the project's).
    pub fn get(&self, ticket_id: &str) -> Vec<RunningApp> {
        self.inner
            .lock()
            .expect("run ports")
            .get(ticket_id)
            .cloned()
            .unwrap_or_default()
    }

    /// App `app` of the ticket runs on `port`, started by the agent of stage `started_by` (`None`: you), or stopped.
    fn set(&self, ticket_id: &str, app: &str, running: Option<(u16, Option<String>)>) {
        let mut all = self.inner.lock().expect("run ports");
        let apps = all.entry(ticket_id.to_owned()).or_default();
        apps.retain(|a| a.app != app);
        if let Some((port, started_by)) = running {
            apps.push(RunningApp {
                app: app.to_owned(),
                port,
                started_by,
            });
            apps.sort_by_key(|a| a.port);
        }
        if apps.is_empty() {
            all.remove(ticket_id);
        }
    }
}

/// A Run's key in [`Inner::records`]: the ticket's id and the app.
type RunKey = (String, String);

fn run_key(ticket_id: &str, app: &str) -> RunKey {
    (ticket_id.to_owned(), app.to_owned())
}

/// What the service remembers of a ticket's last Run.
#[derive(Debug, Clone)]
struct Record {
    key: TicketKey,
    app: String,
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
    /// The apps' ports (agents run the app): an app added while the ticket works gets its port at its first Run.
    ports: PortPool,
    /// By ticket id and app.
    records: Mutex<HashMap<RunKey, Record>>,
}

impl Runs {
    pub fn new(
        board: Board,
        sessions: Sessions,
        data: DataDir,
        events: EventBus,
        env: TerminalEnv,
        ticket_env: TicketEnv,
        ports: PortPool,
    ) -> Self {
        Self {
            inner: Arc::new(Inner {
                board,
                sessions,
                data,
                events,
                env,
                ticket_env,
                ports,
                records: Mutex::new(HashMap::new()),
            }),
        }
    }

    /// Starts app `app`'s Run of the ticket (the user's click; `None`: the only app). A Run already up is left alone.
    /// Refused without a worktree (the ticket never started), without a run command (the UI then asks for one,
    /// RUN-02), or without a name while the project has several apps. When another program holds the app's port,
    /// nothing starts and the state is "port in use".
    pub async fn start(&self, key: &str, app: Option<&str>) -> Result<RunInfo, BoardError> {
        self.start_as(key, app, None).await
    }

    /// [`Runs::start`] for `agent`, one of the ticket's agents (`otaq run start`, agents run the app), or for you (`None`). A
    /// Run already up, whoever started it, is left alone and used.
    pub async fn start_as(
        &self,
        key: &str,
        app: Option<&str>,
        agent: Option<RunAgent>,
    ) -> Result<RunInfo, BoardError> {
        let ticket = self.inner.board.ticket(key).await?;
        let (project, settings) = self.project_of(&ticket).await?;
        let app = resolve_app(&settings, &project.name, app, "start", agent.is_some())?;
        self.start_with(&ticket, app, agent, None, None).await
    }

    /// `otaq run restart [<app>]` (d{RUN}): the app's Run stops and starts again with the same command and port, to
    /// run code changed since it started. Its owner stays: your Run stays yours (an agent never stops it, and it never
    /// stops at a stage end), but when an agent restarts it, its processes act for that agent, which wrote the code
    /// it now runs. A Run that isn't up just starts, as [`Runs::start_as`].
    pub async fn restart_as(
        &self,
        key: &str,
        app: Option<&str>,
        agent: Option<RunAgent>,
    ) -> Result<RunInfo, BoardError> {
        let ticket = self.inner.board.ticket(key).await?;
        let (project, settings) = self.project_of(&ticket).await?;
        let app = self.resolve_live(
            &ticket,
            &settings,
            &project.name,
            app,
            "restart",
            agent.is_some(),
        )?;
        let Some(before) = self.record(&ticket.id, &app).filter(|r| r.state.is_live()) else {
            return self.start_with(&ticket, app, agent, None, None).await;
        };
        let cause = match &agent {
            Some(a) => StopCause::Restarted {
                stage: a.stage.clone(),
            },
            None => StopCause::You,
        };
        self.stop_with(&ticket.id, &app, cause).await;
        // The program may hold its port a moment after it ends.
        if let Some(port) = before.port {
            for _ in 0..40 {
                if ports::is_free_on_machine(port) {
                    break;
                }
                tokio::time::sleep(PORT_POLL).await;
            }
        }
        let acting = if before.agent.is_none() { agent } else { None };
        let ticket = self.inner.board.ticket(key).await?;
        self.start_with(&ticket, app, before.agent, None, acting)
            .await
    }

    /// `otaq run set [<app>] --command … [--open-url …]` (agents run the app): for an app the project doesn't have yet (or one
    /// without a command), `agent` starts `command` as this ticket's Run of it; it becomes the project's once the Run
    /// is running ("Set by Dev on SHOP-27"). `app` `None` adds [`DEFAULT_APP`] to a project with no app. Failed to
    /// start: nothing is saved. Refused for an app that has a command: an agent never changes one the user set.
    pub async fn set_command(
        &self,
        key: &str,
        agent: RunAgent,
        app: Option<&str>,
        command: &str,
        open_url: Option<&str>,
    ) -> Result<RunInfo, BoardError> {
        let ticket = self.inner.board.ticket(key).await?;
        let (project, settings) = self.project_of(&ticket).await?;
        let cmd = naming::COMMAND_NAME;
        let app = match app.map(str::trim) {
            Some("") => {
                return Err(BoardError::refused(
                    Refusal::NotAllowed,
                    "the app's name is empty".to_owned(),
                ));
            }
            Some(name) => name.to_owned(),
            None => match settings.apps.as_slice() {
                [] => DEFAULT_APP.to_owned(),
                [only] => {
                    return Err(BoardError::refused(
                        Refusal::NotAllowed,
                        format!(
                            "{} already has a run command (`{}`): an agent never changes one. Start it with `{cmd} \
                             run start --wait`; to add another app, name it: `{cmd} run set <app> --command …`",
                            project.name,
                            only.command.trim()
                        ),
                    ));
                }
                apps => {
                    return Err(BoardError::refused(
                        Refusal::NotAllowed,
                        format!(
                            "{} already has the apps {}: an agent never changes one. To add another app, name it: \
                             `{cmd} run set <app> --command …`",
                            project.name,
                            names(apps)
                        ),
                    ));
                }
            },
        };
        if let Some(existing) = settings.app(&app)
            && !existing.command.trim().is_empty()
        {
            return Err(BoardError::refused(
                Refusal::NotAllowed,
                format!(
                    "{}'s app {app} already has a run command (`{}`): an agent never changes one. Start it with \
                     `{}`",
                    project.name,
                    existing.command.trim(),
                    start_hint(&settings, &app)
                ),
            ));
        }
        let var = naming::app_port_env_var(&app);
        if let Some(other) = settings
            .apps
            .iter()
            .find(|a| a.name != app && naming::app_port_env_var(&a.name) == var)
        {
            return Err(BoardError::refused(
                Refusal::NotAllowed,
                format!(
                    "{app} and {} would both be {var}: pick another name",
                    other.name
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
        if self
            .record(&ticket.id, &app)
            .is_some_and(|r| r.state.is_live())
        {
            return Err(BoardError::refused(
                Refusal::NotAllowed,
                format!(
                    "{}'s Run of {app} is up: stop it first (`{cmd} run stop`)",
                    ticket.key
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
        self.start_with(
            &ticket,
            app,
            Some(agent),
            Some((command.to_owned(), trying)),
            None,
        )
        .await
    }

    async fn start_with(
        &self,
        ticket: &Ticket,
        app: String,
        agent: Option<RunAgent>,
        trying: Option<(String, Trying)>,
        // The agent its processes act for when that isn't its owner: one that restarted your Run runs the code it
        // wrote, so they act for it, never as you; the Run stays yours.
        acting: Option<RunAgent>,
    ) -> Result<RunInfo, BoardError> {
        let inner = &self.inner;
        if self
            .record(&ticket.id, &app)
            .is_some_and(|r| r.state.is_live())
        {
            return self.info_of(ticket, &app).await;
        }
        let work = ticket.work.as_ref();
        let worktree = work
            .and_then(|w| w.worktree.clone())
            .map(PathBuf::from)
            .filter(|w| w.is_dir() && work.is_some_and(|w| w.agent_port.is_some()));
        let Some(worktree) = worktree else {
            return Err(BoardError::refused(
                Refusal::NoWorktree,
                format!(
                    "{} has no worktree yet. Run works once the ticket has started.",
                    ticket.key
                ),
            ));
        };
        let (project, settings) = self.project_of(ticket).await?;
        let (command, trying) = match trying {
            Some((command, trying)) => (command, Some(trying)),
            None => (
                settings
                    .app(&app)
                    .map(|a| a.command.trim().to_owned())
                    .unwrap_or_default(),
                None,
            ),
        };
        if command.is_empty() {
            return Err(BoardError::refused(
                Refusal::NoRunCommand,
                no_run_command(&project.name, &settings, &app, agent.is_some()),
            ));
        }
        // Every app's port, this one's too when it is new (`run set`): `$PORT` is its own, `OTAQ_PORT_<NAME>` all.
        let mut names = settings.port_apps();
        if !names.contains(&app) {
            if settings.apps.is_empty() {
                names.clear();
            }
            names.push(app.clone());
        }
        let apps = inner
            .ports
            .app_ports(&ticket.id, &names)
            .await
            .map_err(|e| BoardError::refused(Refusal::NotAllowed, e.to_string()))?;
        let port = apps
            .iter()
            .find(|(name, _)| *name == app)
            .map(|(_, port)| *port)
            .expect("the app's port was asked for");

        let mut record = Record {
            key: ticket.key.clone(),
            app: app.clone(),
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
            tracing::info!(ticket = %ticket.key, app, port, "run port in use");
            self.store(&ticket.id, record);
            self.publish(ticket);
            return self.info_of(ticket, &app).await;
        }

        // A build freed at Review comes back first; the ticket's lock is held until the Run is open, so no Free
        // takes it away before the Run holds it.
        let (fetched, _held) = inner
            .ticket_env
            .fetch_for_terminal(ticket, FetchWhy::Run)
            .await;
        if let Err(e) = fetched {
            let log = inner.data.setup_log_path(&ticket.key.to_string()).ok();
            tracing::warn!(ticket = %ticket.key, error = %e, log = ?log, "can't bring the build back; the Run starts anyway");
        }
        // A second click waited for the lock while the first brought the build back and started the Run.
        if self
            .record(&ticket.id, &app)
            .is_some_and(|r| r.state.is_live())
        {
            return self.info_of(ticket, &app).await;
        }

        // Windows' cmd has no `$` variables: the documented placeholders are filled in there (the variables are set
        // too).
        let shown = with_ports(&command, Some(port), &apps);
        let run = if cfg!(windows) {
            shown.clone()
        } else {
            command.clone()
        };
        let shell = crate::shell::one_line_shown(&run, &shown);
        let mut env = inner.env.env(&inner.data, Some(&ticket.key), Some(port));
        env.extend(app_port_vars(&apps));
        env.extend(shell.env);
        let mut prepared = inner.sessions.prepared(None);
        if let Some(a) = acting.as_ref().or(agent.as_ref()) {
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
        if let Err(e) =
            sqlx::query("INSERT OR REPLACE INTO run_sessions (session_id, app) VALUES (?, ?)")
                .bind(&session.id)
                .bind(&app)
                .execute(inner.board.store().pool())
                .await
        {
            tracing::warn!(session = session.id, error = %e, "can't record the run's app");
        }
        record.state = RunState::Starting;
        record.session = Some(session.id.clone());
        record.started_at = Some(session.started_at);
        self.store(&ticket.id, record);
        let stage = agent.as_ref().map(|a| a.stage.clone());
        tracing::info!(ticket = %ticket.key, app, port, by = ?stage, "run started");
        inner
            .board
            .run_ports()
            .set(&ticket.id, &app, Some((port, stage)));
        self.publish(ticket);
        tokio::spawn(
            self.clone()
                .watch(ticket.id.clone(), app.clone(), session.id, port),
        );
        self.info_of(ticket, &app).await
    }

    /// Stops app `app`'s Run of the ticket (the user's Stop; `None`: the only app): its whole process group ends.
    pub async fn stop(&self, key: &str, app: Option<&str>) -> Result<RunInfo, BoardError> {
        self.stop_as(key, app, None).await
    }

    /// [`Runs::stop`] for `agent` (`otaq run stop`, agents run the app), or for you (`None`). An agent stops only a Run an agent
    /// of its ticket started: the user's is refused, and stays up.
    pub async fn stop_as(
        &self,
        key: &str,
        app: Option<&str>,
        agent: Option<&RunAgent>,
    ) -> Result<RunInfo, BoardError> {
        let ticket = self.inner.board.ticket(key).await?;
        let (project, settings) = self.project_of(&ticket).await?;
        let app = self.resolve_live(
            &ticket,
            &settings,
            &project.name,
            app,
            "stop",
            agent.is_some(),
        )?;
        let cause = match agent {
            None => StopCause::You,
            Some(a) => {
                if let Some(r) = self.record(&ticket.id, &app)
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
        self.stop_with(&ticket.id, &app, cause).await;
        self.info_of(&ticket, &app).await
    }

    /// The agent of `session` ended its stage or gave its verdict (agents run the app): every Run it started stops ("stage
    /// ended"), whichever app. A Run the user started, or another agent's, is left alone. `true` when one stopped.
    pub async fn stage_ended(&self, session: &str) -> bool {
        let started: Vec<(RunKey, String)> = self
            .inner
            .records
            .lock()
            .expect("runs")
            .iter()
            .filter(|(_, r)| r.state.is_live())
            .filter_map(|(key, r)| {
                let a = r.agent.as_ref().filter(|a| a.session == session)?;
                Some((key.clone(), a.stage.clone()))
            })
            .collect();
        let mut stopped = false;
        for ((ticket_id, app), stage) in started {
            stopped |= self
                .stop_with(&ticket_id, &app, StopCause::StageEnded { stage })
                .await;
        }
        stopped
    }

    /// App `app`'s Run output as text, its last `tail` lines (`otaq run log`, agents run the app): the Run now or the last one
    /// still in memory. Refused before any Run, and after a restart (output never goes to disk, d0045).
    pub async fn log(
        &self,
        key: &str,
        app: Option<&str>,
        tail: usize,
    ) -> Result<RunLog, BoardError> {
        let ticket = self.inner.board.ticket(key).await?;
        let (project, settings) = self.project_of(&ticket).await?;
        let app = self.resolve_live(&ticket, &settings, &project.name, app, "log", true)?;
        let record = self.record(&ticket.id, &app);
        let output = record
            .as_ref()
            .and_then(|r| r.session.as_deref())
            .and_then(|s| self.inner.sessions.output_of_session(s));
        let whose = if settings.apps.len() > 1 {
            format!("{}'s Run of {app}", ticket.key)
        } else {
            format!("{}'s Run", ticket.key)
        };
        let Some((_, bytes, size)) = output else {
            let message = match record.map(|r| r.state) {
                Some(RunState::StoppedByRestart) => {
                    format!("{whose} output isn't kept after a restart.")
                }
                Some(RunState::PortInUse) => {
                    format!("{whose} didn't start: its port is in use, so there is no output.")
                }
                _ => format!("{whose} hasn't run yet, so there is no output."),
            };
            return Err(BoardError::refused(Refusal::SessionEnded, message));
        };
        let text = crate::session::screen::render_above(&bytes, size, tail);
        let lines: Vec<&str> = text.lines().collect();
        let text = lines[lines.len().saturating_sub(tail)..].join("\n");
        let state = self.info_of(&ticket, &app).await?.state;
        Ok(RunLog {
            key: ticket.key,
            app,
            state,
            text,
        })
    }

    /// A ticket reached Done or was closed (REC-05): every Run of it stops and a toast says so.
    pub async fn finished(&self, ticket: &FinishedTicket) {
        let cause = StopCause::Finished {
            status: ticket.status,
        };
        if !self.stop_all(&ticket.id, cause).await {
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

    /// A ticket was parked (d0052): its Runs stop before the ports are freed, so the freed ports are really free
    /// (Q30). No toast: the Park confirmation already says the ticket's work stops.
    pub async fn parked(&self, ticket: &FinishedTicket) {
        self.stop_all(&ticket.id, StopCause::Parked).await;
    }

    /// A restart stopped this Run session (REC-01, d0045): its app's Run shows "stopped by restart".
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
            let app: Option<String> =
                sqlx::query_scalar("SELECT app FROM run_sessions WHERE session_id = ?")
                    .bind(&info.id)
                    .fetch_optional(runs.inner.board.store().pool())
                    .await
                    .ok()
                    .flatten();
            let app = app.unwrap_or_else(|| DEFAULT_APP.to_owned());
            if runs.record(&ticket.id, &app).is_some() {
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
                    port: ticket.work.as_ref().and_then(|w| w.app_port(&app)),
                    command: settings
                        .app(&app)
                        .map(|a| a.command.trim().to_owned())
                        .unwrap_or_default(),
                    app,
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

    /// App `app`'s Run of the ticket now (`None`: the only app; with several, refused with their names).
    pub async fn info(&self, key: &str, app: Option<&str>) -> Result<RunInfo, BoardError> {
        let ticket = self.inner.board.ticket(key).await?;
        let (project, settings) = self.project_of(&ticket).await?;
        let app = self.resolve_live(&ticket, &settings, &project.name, app, "status", true)?;
        self.info_of(&ticket, &app).await
    }

    /// Every app's Run of the ticket (agents run the app), in the project's order, then any app still running that the project
    /// no longer has. A project with no app yet gives one, [`DEFAULT_APP`], not started (the Run tab asks for a
    /// command).
    pub async fn apps(&self, key: &str) -> Result<Vec<RunInfo>, BoardError> {
        let ticket = self.inner.board.ticket(key).await?;
        let (_, settings) = self.project_of(&ticket).await?;
        let mut names = settings.port_apps();
        let mut extra: Vec<String> = self
            .inner
            .records
            .lock()
            .expect("runs")
            .iter()
            .filter(|((id, app), r)| *id == ticket.id && r.state.is_live() && !names.contains(app))
            .map(|((_, app), _)| app.clone())
            .collect();
        extra.sort();
        names.extend(extra);
        let mut all = Vec::with_capacity(names.len());
        for app in names {
            all.push(self.info_of(&ticket, &app).await?);
        }
        Ok(all)
    }

    /// Every Run that is up, across projects and apps, oldest first (RUN-04).
    pub async fn running(&self) -> Vec<RunInfo> {
        let live: Vec<RunKey> = self
            .inner
            .records
            .lock()
            .expect("runs")
            .iter()
            .filter(|(_, r)| r.state.is_live())
            .map(|(key, _)| key.clone())
            .collect();
        let mut all = Vec::new();
        for (id, app) in live {
            if let Ok(ticket) = self.inner.board.ticket_by_id(&id).await
                && let Ok(info) = self.info_of(&ticket, &app).await
            {
                all.push(info);
            }
        }
        all.sort_by(|a, b| {
            a.started_at
                .cmp(&b.started_at)
                .then(a.key.cmp(&b.key))
                .then(a.app.cmp(&b.app))
        });
        all
    }

    /// Stops every live Run of the ticket for `cause`; `true` when one was up.
    async fn stop_all(&self, ticket_id: &str, cause: StopCause) -> bool {
        let apps: Vec<String> = self
            .inner
            .records
            .lock()
            .expect("runs")
            .iter()
            .filter(|((id, _), r)| id == ticket_id && r.state.is_live())
            .map(|((_, app), _)| app.clone())
            .collect();
        let mut stopped = false;
        for app in apps {
            stopped |= self.stop_with(ticket_id, &app, cause.clone()).await;
        }
        stopped
    }

    /// Stops a live Run for `cause`; `true` when one was up.
    async fn stop_with(&self, ticket_id: &str, app: &str, cause: StopCause) -> bool {
        let session = {
            let mut records = self.inner.records.lock().expect("runs");
            match records.get_mut(&run_key(ticket_id, app)) {
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
            self.ended(ticket_id, app, &info).await;
        }
        true
    }

    /// Follows a Run until its session ends: starting becomes running once the port answers.
    async fn watch(self, ticket_id: String, app: String, session: String, port: u16) {
        let sessions = self.inner.sessions.clone();
        let ended = sessions.wait_ended(&session);
        tokio::pin!(ended);
        let mut tick = tokio::time::interval(PORT_POLL);
        loop {
            let starting = self
                .record(&ticket_id, &app)
                .is_some_and(|r| r.state == RunState::Starting);
            tokio::select! {
                info = &mut ended => {
                    if let Some(info) = info {
                        self.ended(&ticket_id, &app, &info).await;
                    }
                    return;
                }
                _ = tick.tick(), if starting => {
                    if port_answers(port).await {
                        self.became_running(&ticket_id, &app, &session).await;
                    }
                }
            }
        }
    }

    async fn became_running(&self, ticket_id: &str, app: &str, session: &str) {
        let key = run_key(ticket_id, app);
        let tried = {
            let mut records = self.inner.records.lock().expect("runs");
            match records.get_mut(&key) {
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
        // The command an agent tried runs: it becomes the app's (agents run the app), unless one was set meanwhile.
        if let Some((ticket, command, trying)) = tried {
            match self
                .inner
                .board
                .save_agent_app(
                    ticket.project.as_str(),
                    app,
                    &command,
                    trying.open_url.as_deref(),
                    trying.by,
                )
                .await
            {
                Ok(true) => {
                    tracing::info!(%ticket, app, %command, "run command set by the agent");
                    // Settings pages refetch the project's settings: "Set by Dev on SHOP-27" shows.
                    self.inner.events.publish(Event::ProjectsChanged);
                }
                Ok(false) => {
                    tracing::info!(%ticket, app, "a run command was set meanwhile; the agent's isn't saved")
                }
                Err(e) => {
                    tracing::warn!(%ticket, app, error = %e, "can't save the agent's run command")
                }
            }
        }
        // Running only now, once a tried command is saved: `run set` sees it saved when it sees running.
        {
            let mut records = self.inner.records.lock().expect("runs");
            match records.get_mut(&key) {
                Some(r)
                    if r.state == RunState::Starting && r.session.as_deref() == Some(session) =>
                {
                    r.state = RunState::Running;
                }
                _ => return,
            }
        }
        if let Ok(ticket) = self.inner.board.ticket_by_id(ticket_id).await {
            tracing::info!(ticket = %ticket.key, app, "run is up");
            self.publish(&ticket);
        }
    }

    /// Records how a Run's session ended (once): its state, its last line, the card's chip gone.
    async fn ended(&self, ticket_id: &str, app: &str, info: &SessionInfo) {
        let reason = info.end_reason.clone().unwrap_or_default();
        let exit_code = reason
            .strip_prefix("exit ")
            .and_then(|code| code.trim().parse::<i32>().ok());
        let line = {
            let mut records = self.inner.records.lock().expect("runs");
            let Some(r) = records.get_mut(&run_key(ticket_id, app)) else {
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
                Some(StopCause::Restarted { stage }) => {
                    (RunState::Stopped, format!("restarted by {stage}"))
                }
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
        self.inner.board.run_ports().set(ticket_id, app, None);
        if let Ok(ticket) = self.inner.board.ticket_by_id(ticket_id).await {
            tracing::info!(ticket = %ticket.key, app, %line, "run ended");
            self.publish(&ticket);
        }
    }

    async fn info_of(&self, ticket: &Ticket, app: &str) -> Result<RunInfo, BoardError> {
        let (project, settings) = self.project_of(ticket).await?;
        let record = self.record(&ticket.id, app);
        let configured = settings.app(app);
        let open_url = record
            .as_ref()
            .and_then(|r| r.trying.as_ref())
            .and_then(|t| t.open_url.clone())
            .or_else(|| configured.and_then(|a| a.url()).map(str::to_owned));
        let mut info = RunInfo {
            project: project.key.clone(),
            project_name: project.name.clone(),
            key: ticket.key.clone(),
            title: ticket.title.clone(),
            app: app.to_owned(),
            state: RunState::NotStarted,
            port: ticket.work.as_ref().and_then(|w| w.app_port(app)),
            command: configured
                .map(|a| a.command.trim().to_owned())
                .unwrap_or_default(),
            open_url,
            session: None,
            started_at: None,
            ended_at: None,
            exit_code: None,
            stopped_by: None,
            started_by: None,
            command_set_by: configured.and_then(|a| a.set_by.clone()),
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

    async fn project_of(
        &self,
        ticket: &Ticket,
    ) -> Result<(crate::store::projects::Project, ProjectSettings), BoardError> {
        let project = self
            .inner
            .board
            .project(ticket.key.project.as_str())
            .await?;
        let settings = self.inner.board.project_settings(&project.key).await?;
        Ok((project, settings))
    }

    /// [`resolve_app`], where a name left out may also mean the one app of the ticket still running that the project
    /// no longer lists (an app renamed while its Run was up).
    fn resolve_live(
        &self,
        ticket: &Ticket,
        settings: &ProjectSettings,
        project: &str,
        app: Option<&str>,
        verb: &str,
        agent: bool,
    ) -> Result<String, BoardError> {
        if let Some(name) = app
            && settings.app(name).is_none()
            && self.record(&ticket.id, name).is_some()
        {
            return Ok(name.to_owned());
        }
        resolve_app(settings, project, app, verb, agent)
    }

    fn record(&self, ticket_id: &str, app: &str) -> Option<Record> {
        self.inner
            .records
            .lock()
            .expect("runs")
            .get(&run_key(ticket_id, app))
            .cloned()
    }

    fn store(&self, ticket_id: &str, record: Record) {
        self.inner
            .records
            .lock()
            .expect("runs")
            .insert(run_key(ticket_id, &record.app), record);
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

/// Which app a Run request means (agents run the app): the one named, or the only app; with several apps and none named, refused
/// with their names. A name the project doesn't have is refused too, unless the project has no app yet (then the
/// start says how to add it). `verb` is the `run` subcommand, for the example in the refusal.
pub fn resolve_app(
    settings: &ProjectSettings,
    project: &str,
    app: Option<&str>,
    verb: &str,
    agent: bool,
) -> Result<String, BoardError> {
    let cmd = naming::COMMAND_NAME;
    match app.map(str::trim).filter(|a| !a.is_empty()) {
        Some(name) if settings.app(name).is_some() || settings.apps.is_empty() => {
            Ok(name.to_owned())
        }
        Some(name) => {
            let add = if agent {
                format!(
                    ". If you know how {name} starts, add it: `{cmd} run set {name} --command …`"
                )
            } else {
                String::new()
            };
            Err(BoardError::refused(
                Refusal::NoRunCommand,
                format!(
                    "{project} has no app {name}; its apps: {}{add}",
                    names(&settings.apps)
                ),
            ))
        }
        None => match settings.apps.as_slice() {
            [] => Ok(DEFAULT_APP.to_owned()),
            [only] => Ok(only.name.clone()),
            apps => Err(BoardError::refused(
                Refusal::NotAllowed,
                format!(
                    "{project} has {} apps: {}. Name the one you mean: `{cmd} run {verb} {}`",
                    apps.len(),
                    names(apps),
                    apps[0].name
                ),
            )),
        },
    }
}

/// "web, api".
fn names(apps: &[crate::board::settings::RunApp]) -> String {
    apps.iter()
        .map(|a| a.name.as_str())
        .collect::<Vec<_>>()
        .join(", ")
}

/// `otaq run start --wait`, with the app's name when the project has several.
fn start_hint(settings: &ProjectSettings, app: &str) -> String {
    let cmd = naming::COMMAND_NAME;
    if settings.apps.len() > 1 {
        format!("{cmd} run start {app} --wait")
    } else {
        format!("{cmd} run start --wait")
    }
}

/// The refusal when the app has no run command; an agent is told how to add one (agents run the app). A project with no app yet
/// and the app left unnamed reads as before apps had names.
fn no_run_command(project: &str, settings: &ProjectSettings, app: &str, agent: bool) -> String {
    let cmd = naming::COMMAND_NAME;
    let unnamed = settings.apps.is_empty() && app == DEFAULT_APP;
    match (agent, unnamed) {
        (true, true) => format!(
            "{project} has no run command yet: if you know how this project starts, run `{cmd} run set --command …`"
        ),
        (true, false) => format!(
            "{project} has no run command for {app} yet: if you know how it starts, run `{cmd} run set {app} \
             --command …`"
        ),
        (false, true) => format!("{project} has no run command yet"),
        (false, false) => format!("{project} has no run command for {app} yet"),
    }
}

/// An agent's `otaq run stop` of a Run the user started (agents run the app).
pub fn user_run_refusal(key: &TicketKey) -> String {
    format!(
        "{key}'s Run was started by the user, so it stays running: an agent stops only a Run an agent started. \
         Use it as it is."
    )
}
