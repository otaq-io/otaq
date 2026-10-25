//! Watching a tool's own file while its agent runs (ADP-03, ADP-07; d0086, d0088): what the tool reports through
//! no hook (Codex's failed turns, limits and windows, in its session file). Nothing here knows a tool: the session's
//! adapter names the file ([`AgentTool::watched_file`]) and reads its lines ([`AgentTool::map_watched_lines`]).
//!
//! - **When:** once the session's tool id is known ([`ToolEvent::ToolIdKnown`]): a new conversation is read from the
//!   file's start (the file is new, so nothing older is in it), a resumed one from where an earlier session of it
//!   stopped reading (remembered in memory per conversation), else from the file's end. After a service restart,
//!   running sessions whose file exists are watched again from its end ([`Signals::rewatch_running`]).
//! - **How:** every [`Timing::poll`], whole lines only (a line still being written waits for its newline); the
//!   events go the way of a hook's ([`OnWatched`]). Watching stops when the session ends.
//! - **A file that can't be read** (not there after [`Timing::missing_after`], or its latest line isn't one the
//!   adapter can read): one warning, the tool's note [`LIMITS_NOT_REPORTED`] on Settings → Agent tools, and once the
//!   agent's terminal has been quiet for [`Timing::quiet_after`] while it is at work, its turn counts as ended
//!   (Idle), never a wrong state. A later readable line clears the note.
//!
//! The watcher only reads the tool's files.

use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::time::Instant;

use super::Signals;
use crate::agent_tools::connected::ConnectedTools;
use crate::agent_tools::{Adapters, AgentTool, SignalContext, Tool, ToolEvent};
use crate::board::BoardError;
use crate::caller::Caller;
use crate::domain::Status;
use crate::protocol::Event;
use crate::session::Sessions;

/// The note a tool's row gets while its watched file can't be read (d0086: "Codex: limits not reported").
pub const LIMITS_NOT_REPORTED: &str = "limits not reported";

/// How often and how long the watcher waits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Timing {
    /// How often the file is read.
    pub poll: Duration,
    /// How long a file may take to appear before it counts as unreadable.
    pub missing_after: Duration,
    /// How long the agent's terminal is quiet, while the file can't be read, before its turn counts as ended.
    pub quiet_after: Duration,
}

impl Default for Timing {
    fn default() -> Self {
        Self {
            poll: Duration::from_secs(1),
            missing_after: Duration::from_secs(10),
            // A working Codex redraws its "Working (12s • esc to interrupt)" line every second.
            quiet_after: Duration::from_secs(15),
        }
    }
}

/// What one read of a watched file gave: `caller`'s events, from `tool`'s adapter.
#[derive(Debug, Clone)]
pub struct Watched {
    pub caller: Caller,
    pub tool: Tool,
    pub events: Vec<ToolEvent>,
}

/// What applies a read's events (the service: the limit windows recorded, the rest applied as a hook's events,
/// [`Signals::apply_events`]).
pub type OnWatched = Arc<dyn Fn(Watched) -> Pin<Box<dyn Future<Output = ()> + Send>> + Send + Sync>;

/// Where a watch starts reading.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StartAt {
    /// A new conversation: its file's start.
    Start,
    /// A resumed conversation: where an earlier session of it stopped reading, else the end.
    Resume,
    /// The file's end (after a restart).
    End,
}

/// The watches of one service. Cheap to clone.
#[derive(Clone)]
pub struct Watcher {
    inner: Arc<Inner>,
}

struct Inner {
    sessions: Sessions,
    adapters: Adapters,
    connected: ConnectedTools,
    on_watched: OnWatched,
    timing: Mutex<Timing>,
    /// The sessions being watched.
    watching: Mutex<HashSet<String>>,
    /// How far each conversation's file was read when its last watch ended.
    offsets: Mutex<HashMap<(Tool, String), u64>>,
}

/// One session's watch.
struct Watch {
    caller: Caller,
    tool: Tool,
    session: String,
    tool_id: String,
    cwd: Option<PathBuf>,
    start: StartAt,
}

impl Watcher {
    pub fn new(
        sessions: Sessions,
        adapters: Adapters,
        connected: ConnectedTools,
        on_watched: OnWatched,
    ) -> Self {
        Self {
            inner: Arc::new(Inner {
                sessions,
                adapters,
                connected,
                on_watched,
                timing: Mutex::new(Timing::default()),
                watching: Mutex::new(HashSet::new()),
                offsets: Mutex::new(HashMap::new()),
            }),
        }
    }

    /// Applies events Otaq made itself (a quiet turn's end, [`super::quiet`]) the way a read's are.
    pub async fn apply(&self, watched: Watched) {
        (self.inner.on_watched)(watched).await;
    }

    /// Changes the waits (tests); watches already running take them on their next read.
    pub fn set_timing(&self, timing: Timing) {
        *self.inner.timing.lock().expect("watch timing") = timing;
    }

    /// Whether `session`'s file is being watched.
    pub fn is_watching(&self, session: &str) -> bool {
        self.inner
            .watching
            .lock()
            .expect("watching")
            .contains(session)
    }

    fn timing(&self) -> Timing {
        *self.inner.timing.lock().expect("watch timing")
    }

    /// Starts watching `watch.session`, unless it already is.
    fn start(&self, signals: &Signals, watch: Watch) {
        if !self
            .inner
            .watching
            .lock()
            .expect("watching")
            .insert(watch.session.clone())
        {
            return;
        }
        tracing::debug!(session = watch.session, tool = %watch.tool, start = ?watch.start, "watching the tool's file");
        let (watcher, signals) = (self.clone(), signals.clone());
        tokio::spawn(async move {
            let session = watch.session.clone();
            watcher.run(&signals, watch).await;
            watcher
                .inner
                .watching
                .lock()
                .expect("watching")
                .remove(&session);
        });
    }

    async fn run(&self, signals: &Signals, watch: Watch) {
        let adapter = self.inner.adapters.get(watch.tool);
        let Ok(attached) = self.inner.sessions.attach(&watch.session) else {
            return;
        };
        if !attached.info.is_running() {
            return;
        }
        let mut output = attached.output;
        let mut output_open = true;
        let ended = self.inner.sessions.wait_ended(&watch.session);
        tokio::pin!(ended);
        let context = SignalContext {
            tool_id: Some(&watch.tool_id),
            cwd: watch.cwd.as_deref(),
        };
        let mut file = FileState::default();
        let started = Instant::now();
        let mut last_output = Instant::now();
        let mut quiet_sent = false;
        let mut next_read = Instant::now();
        loop {
            tokio::select! {
                biased;
                _ = &mut ended => break,
                chunk = output.recv(), if output_open => {
                    if let Err(tokio::sync::broadcast::error::RecvError::Closed) = chunk {
                        output_open = false;
                    } else {
                        last_output = Instant::now();
                        quiet_sent = false;
                    }
                    continue;
                }
                () = tokio::time::sleep_until(next_read) => {}
            }
            let timing = self.timing();
            next_read = Instant::now() + timing.poll;
            let events = self.read(adapter, &watch, context, &mut file, started, timing);
            if !events.is_empty() {
                (self.inner.on_watched)(Watched {
                    caller: watch.caller.clone(),
                    tool: watch.tool,
                    events,
                })
                .await;
            }
            if file.unreadable
                && !quiet_sent
                && last_output.elapsed() >= timing.quiet_after
                && signals.at_work(&watch.caller).await
            {
                quiet_sent = true;
                tracing::info!(session = watch.session, tool = %watch.tool, "the agent went quiet; its turn counts as ended");
                (self.inner.on_watched)(Watched {
                    caller: watch.caller.clone(),
                    tool: watch.tool,
                    events: vec![ToolEvent::TurnEnded {
                        background: Default::default(),
                        last_message: None,
                    }],
                })
                .await;
            }
        }
        // A resume of the conversation reads on from here: what was written up to now belonged to this session.
        if let Some(path) = &file.path {
            let end = std::fs::metadata(path).map_or(file.offset, |m| m.len());
            self.inner
                .offsets
                .lock()
                .expect("offsets")
                .insert((watch.tool, watch.tool_id.clone()), end);
        }
        tracing::debug!(session = watch.session, "stopped watching the tool's file");
    }

    /// Reads what was added since the last read; returns its events, the unreadable lines left out.
    fn read(
        &self,
        adapter: &dyn AgentTool,
        watch: &Watch,
        context: SignalContext<'_>,
        file: &mut FileState,
        started: Instant,
        timing: Timing,
    ) -> Vec<ToolEvent> {
        let cwd = watch.cwd.as_deref().unwrap_or(Path::new(""));
        if file.path.is_none() {
            if let Some(path) = adapter.watched_file(&watch.tool_id, cwd) {
                let len = std::fs::metadata(&path).map_or(0, |m| m.len());
                let remembered = self
                    .inner
                    .offsets
                    .lock()
                    .expect("offsets")
                    .get(&(watch.tool, watch.tool_id.clone()))
                    .copied();
                file.offset = match watch.start {
                    StartAt::Start => 0,
                    StartAt::Resume => remembered.unwrap_or(len).min(len),
                    StartAt::End => len,
                };
                file.path = Some(path);
            } else {
                if started.elapsed() >= timing.missing_after {
                    self.readable(watch, file, false, "the file wasn't found");
                }
                return Vec::new();
            }
        }
        let path = file.path.clone().expect("found");
        let bytes = match std::fs::read(&path) {
            Ok(bytes) => bytes,
            Err(e) => {
                self.readable(watch, file, false, &e.to_string());
                return Vec::new();
            }
        };
        let start = usize::try_from(file.offset)
            .unwrap_or(usize::MAX)
            .min(bytes.len());
        let new = &bytes[start..];
        let Some(end) = new.iter().rposition(|b| *b == b'\n').map(|at| at + 1) else {
            return Vec::new();
        };
        file.offset += end as u64;
        let mut events = Vec::new();
        for line in String::from_utf8_lossy(&new[..end]).lines() {
            if line.trim().is_empty() {
                continue;
            }
            let mapped = adapter.map_watched_lines(&format!("{line}\n"), context);
            let unreadable = mapped
                .iter()
                .any(|e| matches!(e, ToolEvent::Unrecognised { .. }));
            self.readable(watch, file, !unreadable, "a line it can't read");
            events.extend(
                mapped
                    .into_iter()
                    .filter(|e| !matches!(e, ToolEvent::Unrecognised { .. })),
            );
        }
        events
    }

    /// The file became readable or unreadable: the tool's note follows, and an unreadable file is logged once per
    /// watch.
    fn readable(&self, watch: &Watch, file: &mut FileState, readable: bool, why: &str) {
        if readable == !file.unreadable {
            return;
        }
        file.unreadable = !readable;
        if !readable && !file.warned {
            file.warned = true;
            tracing::warn!(session = watch.session, tool = %watch.tool, why, "the tool's file can't be read: limits not reported");
        }
        self.inner
            .connected
            .set_note(watch.tool, LIMITS_NOT_REPORTED, !readable);
    }
}

/// How far a watch has got with its file.
#[derive(Debug, Default)]
struct FileState {
    path: Option<PathBuf>,
    /// The end of the last whole line read.
    offset: u64,
    /// Not found yet (past the wait), or its latest line can't be read.
    unreadable: bool,
    /// The one warning was logged.
    warned: bool,
}

impl Signals {
    /// Watches the files of every running agent session whose tool has one, from their end (at service start: a
    /// restart of the service, not of the sessions).
    pub fn rewatch_running(&self) {
        let Some(watcher) = self.inner.watcher.get() else {
            return;
        };
        for info in self.inner.sessions.in_memory() {
            let (Some(tool), Some(tool_id)) = (info.tool, info.tool_session_id.clone()) else {
                continue;
            };
            if !info.is_running() || tool_id.is_empty() {
                continue;
            }
            let Some(caller) = caller_of(&self.inner.sessions, &info.id) else {
                continue;
            };
            if self
                .inner
                .adapters
                .get(tool)
                .watched_file(&tool_id, Path::new(""))
                .is_none()
            {
                continue;
            }
            watcher.start(
                self,
                Watch {
                    caller,
                    tool,
                    session: info.id.clone(),
                    tool_id,
                    cwd: None,
                    start: StartAt::End,
                },
            );
        }
    }

    /// Sets the watcher (once, at service start); without it nothing is watched.
    pub fn set_watcher(&self, watcher: Watcher) {
        let _ = self.inner.watcher.set(watcher);
    }

    /// The service's watcher, once set.
    pub fn watcher(&self) -> Option<&Watcher> {
        self.inner.watcher.get()
    }

    /// `tool` reported `session`'s conversation id (`stored`: it was stored just now, a new conversation): a Project
    /// agent's own row keeps it too (its Resume continues it), and the session's file is watched.
    pub(super) async fn tool_id_reported(
        &self,
        caller: &Caller,
        tool: Tool,
        session: &str,
        tool_id: &str,
        stored: bool,
    ) -> Result<(), BoardError> {
        if let Some((agent, _)) = caller.project_agent() {
            let kept = sqlx::query(
                "UPDATE project_agents SET tool_session_id = ?
                 WHERE id = ? AND (tool_session_id IS NULL OR tool_session_id = '')",
            )
            .bind(tool_id)
            .bind(agent)
            .execute(self.inner.board.store().pool())
            .await?
            .rows_affected()
                > 0;
            if kept && let Caller::ProjectAgent { project, .. } = caller {
                self.inner.events.publish(Event::ProjectAgentChanged {
                    project: project.clone(),
                    id: agent.to_owned(),
                });
            }
        }
        let Some(watcher) = self.inner.watcher.get() else {
            return Ok(());
        };
        let cwd = self.stored_session(session).await?.cwd.map(PathBuf::from);
        watcher.start(
            self,
            Watch {
                caller: caller.clone(),
                tool,
                session: session.to_owned(),
                tool_id: tool_id.to_owned(),
                cwd,
                start: if stored {
                    StartAt::Start
                } else {
                    StartAt::Resume
                },
            },
        );
        Ok(())
    }

    /// Whether `caller`'s agent is at work, so a quiet turn may end: a ticket's agent Working, a Project agent whose
    /// turn hasn't ended. A validator's turns are its validation's.
    async fn at_work(&self, caller: &Caller) -> bool {
        match caller {
            Caller::Agent { ticket_id, .. } => self
                .inner
                .board
                .ticket_by_id(ticket_id)
                .await
                .is_ok_and(|t| t.status == Status::Working),
            Caller::ProjectAgent { session, .. } => !self.is_idle_between_turns(session),
            Caller::Validator { .. } | Caller::User => false,
        }
    }
}

/// The caller a running agent session's processes are (as [`crate::caller::resolve`] finds them).
pub(super) fn caller_of(sessions: &Sessions, session: &str) -> Option<Caller> {
    if let Some(root) = sessions
        .project_agent_roots()
        .into_iter()
        .find(|r| r.session == session)
    {
        return Some(Caller::ProjectAgent {
            project: root.project,
            agent: root.agent,
            session: root.session,
        });
    }
    let root = sessions
        .agent_roots()
        .into_iter()
        .find(|r| r.session == session)?;
    Some(if root.kind == crate::session::SessionKind::Validator {
        Caller::Validator {
            ticket: root.ticket,
            ticket_id: root.ticket_id,
            session: root.session,
        }
    } else {
        Caller::Agent {
            ticket: root.ticket,
            ticket_id: root.ticket_id,
            session: root.session,
        }
    })
}
