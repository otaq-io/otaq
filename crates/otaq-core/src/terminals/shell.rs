//! The Shell tab (RUN-05; d0035, d0026) and the Project terminal (RUN-06; d0019, d0031): plain terminals running
//! the user's login shell, owned by the service (sessions of kind `shell` and `project`), so they survive closing
//! the window. Neither starts Claude; callers inside them are the user, with full `otaq` access (CLI-03).
//!
//! - **Shell**: in the ticket's worktree, with `$PORT` = the Run port and `$OTAQ_TICKET` set. It starts the
//!   first time the tab opens and closes when the ticket reaches Done or is closed.
//! - **Project terminals** (d0068): several per project, in the project's own folder (the user's checkout), no
//!   ticket, named "Terminal 1", "Terminal 2"… (the lowest number no open terminal of the project has). Each ends
//!   when the user exits its shell or closes it, or with a restart; after a restart they are gone (the names live
//!   in the service's memory only).
//!
//! Opening the Shell again shows the one there is: running, or ended (with its output while the service keeps
//! it). A Shell a restart stopped stays "stopped by restart" until the user's Reopen click (d0045); opening the
//! tab never restarts it.
//!
//! A new Shell of a ticket whose build was freed at Review brings it back first (M19-c); opening one that is there
//! (running or ended) never does. When the Shell ends the build is freed again.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};

use super::TerminalEnv;
use crate::board::tickets::Ticket;
use crate::board::{Board, BoardError, Refusal};
use crate::data_dir::DataDir;
use crate::ports::PortPool;
use crate::review::finish::FinishedTicket;
use crate::session::{
    END_STOPPED_BY_RESTART, SessionInfo, SessionKind, SessionTicket, Sessions, SpawnSpec, Start,
    TerminalSize,
};
use crate::ticket_env::TicketEnv;
use crate::ticket_env::fetch::FetchWhy;

/// The size a terminal starts with; the app resizes it.
const START_SIZE: TerminalSize = TerminalSize {
    cols: 120,
    rows: 32,
};

/// One of a project's Project terminals (RUN-06, d0068).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectTerminal {
    /// 1 for "Terminal 1".
    pub number: u32,
    /// "Terminal 1".
    pub name: String,
    pub session: SessionInfo,
}

/// An open Project terminal: its project and number.
#[derive(Debug, Clone)]
struct Open {
    project: String,
    number: u32,
}

/// Opens Shells and Project terminals. Cheap to clone.
#[derive(Clone)]
pub struct Shells {
    board: Board,
    sessions: Sessions,
    data: DataDir,
    env: TerminalEnv,
    /// Brings a freed build back before a new Shell (M19-c).
    ticket_env: TicketEnv,
    /// The ticket's app ports (d{RUN}): an app added while the ticket works gets its port at the next Shell.
    ports: PortPool,
    /// The open Project terminals by session id, until closed. Kept in memory: a restart forgets them.
    terminals: Arc<Mutex<HashMap<String, Open>>>,
}

/// "Terminal 2".
pub fn terminal_name(number: u32) -> String {
    format!("Terminal {number}")
}

impl Shells {
    pub fn new(
        board: Board,
        sessions: Sessions,
        data: DataDir,
        env: TerminalEnv,
        ticket_env: TicketEnv,
        ports: PortPool,
    ) -> Self {
        Self {
            board,
            sessions,
            data,
            env,
            ticket_env,
            ports,
            terminals: Arc::default(),
        }
    }

    /// The ticket's Shell: the running one, the last one (ended, or stopped by a restart), or a new one on first
    /// use. `reopen` (the user's click) starts a new one unless one runs.
    pub async fn open_shell(&self, key: &str, reopen: bool) -> Result<SessionInfo, BoardError> {
        let ticket = self.board.ticket(key).await?;
        if let Some(running) = self.sessions.running_of(&ticket.id, SessionKind::Shell) {
            return Ok(running);
        }
        let worktree = ticket
            .work
            .as_ref()
            .and_then(|w| w.worktree.clone())
            .map(PathBuf::from)
            .filter(|w| w.is_dir());
        let Some(worktree) = worktree else {
            return Err(BoardError::refused(
                Refusal::NoWorktree,
                format!(
                    "{} has no worktree. The Shell opens once the ticket has started, until it is done.",
                    ticket.key
                ),
            ));
        };
        if !reopen {
            if let Some(last) = self.sessions.latest_of(&ticket.id, SessionKind::Shell) {
                return Ok(last);
            }
            if let Some(stopped) = self
                .stopped_by_restart(
                    "SELECT id FROM sessions WHERE ticket_id = ? AND kind = 'shell'
                     ORDER BY started_at DESC, id DESC LIMIT 1",
                    &ticket.id,
                )
                .await?
            {
                return Ok(stopped);
            }
        }
        // A build freed at Review comes back first; the lock is held until the Shell is open.
        let (fetched, _held) = self
            .ticket_env
            .fetch_for_terminal(&ticket, FetchWhy::Shell)
            .await;
        if let Err(e) = fetched {
            tracing::warn!(ticket = %ticket.key, error = %e, "can't bring the build back; the Shell opens anyway");
        }
        // A second open waited for the lock while the first brought the build back and opened the Shell.
        if let Some(running) = self.sessions.running_of(&ticket.id, SessionKind::Shell) {
            return Ok(running);
        }
        self.spawn_shell(&ticket, worktree).await
    }

    /// A Project terminal of `project`: with `new` (the user's "+ Terminal" or Reopen click) always a new shell;
    /// otherwise the newest running one, else the newest ended one still open, else a new one.
    pub async fn open_project(
        &self,
        project: &str,
        new: bool,
    ) -> Result<ProjectTerminal, BoardError> {
        let project = self.board.project(project).await?;
        if !new {
            let open = self.project_terminals(&project.key);
            let newest = |running: bool| {
                open.iter()
                    .filter(|t| t.session.is_running() == running)
                    .max_by(|a, b| {
                        (a.session.started_at, &a.session.id)
                            .cmp(&(b.session.started_at, &b.session.id))
                    })
                    .cloned()
            };
            if let Some(last) = newest(true).or_else(|| newest(false)) {
                return Ok(last);
            }
        }
        let info = self
            .sessions
            .spawn(SpawnSpec {
                kind: SessionKind::Project,
                start: Start::User,
                project_id: project.id.clone(),
                project_key: project.key.clone(),
                ticket: None,
                program: self.env.shell.clone(),
                args: self.env.shell_args.clone(),
                cwd: PathBuf::from(&project.repo_path),
                env: self.env.env(&self.data, None, None),
                size: START_SIZE,
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
        let number = {
            let mut open = self.terminals.lock().expect("project terminals");
            let taken: Vec<u32> = open
                .values()
                .filter(|o| o.project == project.key)
                .map(|o| o.number)
                .collect();
            let number = (1..).find(|n| !taken.contains(n)).expect("a free number");
            open.insert(
                info.id.clone(),
                Open {
                    project: project.key.clone(),
                    number,
                },
            );
            number
        };
        tracing::info!(project = %project.key, number, "project terminal opened");
        Ok(ProjectTerminal {
            number,
            name: terminal_name(number),
            session: info,
        })
    }

    /// The open Project terminals of `project` (running, or ended and not closed yet), by number.
    pub fn project_terminals(&self, project: &str) -> Vec<ProjectTerminal> {
        let open: Vec<(String, u32)> = self
            .terminals
            .lock()
            .expect("project terminals")
            .iter()
            .filter(|(_, o)| o.project == project)
            .map(|(id, o)| (id.clone(), o.number))
            .collect();
        let mut list: Vec<ProjectTerminal> = open
            .into_iter()
            .filter_map(|(id, number)| {
                let session = self.sessions.info(&id).ok()?;
                Some(ProjectTerminal {
                    number,
                    name: terminal_name(number),
                    session,
                })
            })
            .collect();
        list.sort_by_key(|t| t.number);
        list
    }

    /// The newest open Project terminal of `project` (`session.attach {project}`).
    pub fn latest_project_terminal(&self, project: &str) -> Option<ProjectTerminal> {
        self.project_terminals(project).into_iter().max_by(|a, b| {
            (a.session.started_at, &a.session.id).cmp(&(b.session.started_at, &b.session.id))
        })
    }

    /// Closes a Project terminal: its shell ends and it leaves the list (its number is free again).
    pub async fn close_project_terminal(&self, session: &str) -> Result<(), BoardError> {
        let known = self
            .terminals
            .lock()
            .expect("project terminals")
            .contains_key(session);
        if !known {
            return Err(BoardError::NotFound(format!(
                "{session} isn't an open Project terminal"
            )));
        }
        if self.sessions.info(session).is_ok_and(|s| s.is_running()) {
            self.sessions
                .stop(session)
                .await
                .map_err(|e| BoardError::refused(Refusal::SessionEnded, e.to_string()))?;
        }
        self.terminals
            .lock()
            .expect("project terminals")
            .remove(session);
        tracing::info!(session, "project terminal closed");
        Ok(())
    }

    /// A ticket reached Done or was closed: its Shell closes (RUN-05).
    pub async fn finished(&self, ticket: &FinishedTicket) {
        if let Some(shell) = self.sessions.running_of(&ticket.id, SessionKind::Shell)
            && let Err(e) = self.sessions.stop(&shell.id).await
        {
            tracing::warn!(ticket = %ticket.key, error = %e, "can't close the shell");
        }
    }

    async fn spawn_shell(
        &self,
        ticket: &Ticket,
        worktree: PathBuf,
    ) -> Result<SessionInfo, BoardError> {
        let project = self.board.project(ticket.key.project.as_str()).await?;
        // `$PORT` is the app's port while the project has one app (or none yet); with several the Shell gets only
        // `OTAQ_PORT_<NAME>`, so a script never reaches the wrong app (d{RUN}).
        let settings = self.board.project_settings(&project.key).await?;
        let apps = match self
            .ports
            .app_ports(&ticket.id, &settings.port_apps())
            .await
        {
            Ok(apps) => apps,
            Err(e) => {
                tracing::warn!(ticket = %ticket.key, error = %e, "no app ports for the shell");
                Vec::new()
            }
        };
        let port = match apps.as_slice() {
            [(_, port)] => Some(*port),
            _ => None,
        };
        let mut env = self.env.env(&self.data, Some(&ticket.key), port);
        env.extend(super::app_port_vars(&apps));
        let info = self
            .sessions
            .spawn(SpawnSpec {
                kind: SessionKind::Shell,
                start: Start::User,
                project_id: project.id.clone(),
                project_key: project.key.clone(),
                ticket: Some(SessionTicket {
                    id: ticket.id.clone(),
                    key: ticket.key.clone(),
                }),
                program: self.env.shell.clone(),
                args: self.env.shell_args.clone(),
                cwd: worktree,
                env,
                size: START_SIZE,
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
        tracing::info!(ticket = %ticket.key, "shell opened");
        Ok(info)
    }

    /// The newest stored terminal (by `sql` with one id bound) when a restart stopped it.
    async fn stopped_by_restart(
        &self,
        sql: &'static str,
        id: &str,
    ) -> Result<Option<SessionInfo>, BoardError> {
        let newest: Option<(String,)> = sqlx::query_as(sql)
            .bind(id)
            .fetch_optional(self.board.store().pool())
            .await?;
        let Some((session,)) = newest else {
            return Ok(None);
        };
        Ok(self
            .sessions
            .stored(&session)
            .await
            .ok()
            .filter(|s| s.end_reason.as_deref() == Some(END_STOPPED_BY_RESTART)))
    }
}
