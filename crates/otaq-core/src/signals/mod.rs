//! Agent signals: what each agent is doing, from its hooks (SDD §11.4, §12.1; d0042, d0088).
//!
//! A tool's hooks call `otaq hook --tool <tool> <event>` inside the agent's session; the service resolves the
//! caller to the agent (CLI-01), hands the hook to that tool's adapter, which turns it into Otaq's events
//! ([`ToolEvent`], ADP-02), and applies Otaq's rule to each ([`hooks::signal`]): the ticket's status becomes
//! **Needs you**, **Working in background**, **Idle** or back to **Working**. Only a ticket whose agent is at work
//! (Working, Working in background, or one of the needs-you family) changes status this way; a ticket that was
//! submitted, parked or closed keeps its column. Nothing here reads a tool's payload.
//!
//! Per session, the service remembers the current turn's facts (did `review submit` or `ask human` run?) and
//! whether the turn has ended. Both reset when the next turn starts. They live in memory: a service restart ends
//! every session anyway (its tickets become Interrupted, [`crate::recovery`]).
//!
//! [`state::transition`] is the state machine: hook signals, `ask human`, any other `otaq` command of the
//! agent that acts, and Enter in the ticket's terminal (ATT-01, ATT-02, ATT-04). Every hook and such command also
//! moves the ticket's **last signal** (ATT-05). The status, its text and the last signal are stored on the ticket;
//! the ticket's open question is read from the store at each step and holds Asks you (d0119).
//!
//! Hook status changes are not written to the History tab (they come several times per turn).

pub mod background;
pub mod hooks;
pub mod model_refused;
pub mod quiet;
pub mod reminder;
pub mod state;
pub mod watch;

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex, OnceLock};

use serde_json::Value;

use crate::agent_tools::{Adapters, SignalContext, Tool, ToolEvent};
use crate::board::tickets::Ticket;
use crate::board::{Board, BoardError};
use crate::caller::Caller;
use crate::domain::Status;
use crate::helpers::HelperStatus;
use crate::protocol::Event;
use crate::service::EventBus;
use crate::session::deliveries::IdleSignal;
use crate::session::{ModelRan, SessionError, SessionKind, Sessions};
use crate::store::now_ms;
use crate::store::tickets::set_state_since;
use background::{BackgroundTask, BackgroundWork};
pub use hooks::{HELPER_PROMPT_PREFIX, Signal, TurnFacts, reports_activity, signal};
pub use model_refused::Refusals;
pub use reminder::reminder_text;
pub use state::{Input, STALE_AFTER_MS, SignalState, is_stale, transition};

/// The statuses of a ticket whose agent is at work; only these follow signals. Waiting for limit reset is one:
/// a turn that starts again (Claude Code's own continue after the reset, the user's prompt, the tool's
/// Resume's "continue"; Q27) makes it Working.
pub const LIVE: &[Status] = &[
    Status::Working,
    Status::WorkingInBackground,
    Status::AsksYou,
    Status::NeedsYou,
    Status::Idle,
    Status::WaitingForLimitReset,
];

/// The signal state of every agent session. Cheap to clone.
#[derive(Clone)]
pub struct Signals {
    inner: Arc<Inner>,
}

struct Inner {
    board: Board,
    events: EventBus,
    adapters: Adapters,
    sessions: Sessions,
    turns: Mutex<HashMap<String, Turn>>,
    /// What applies a refused model (AGT-05; set once at service start).
    refusals: OnceLock<Refusals>,
    /// The tools' lists, which say whether an asked model is an exact one (the swap, T-0357; set once at service
    /// start).
    models: OnceLock<crate::agent_tools::models::ModelLists>,
    /// The sessions a turn of which got through (ended normally): a refused model after that isn't a first turn's.
    turned: Mutex<HashSet<String>>,
    /// One signal is applied at a time, so read-modify-write of a ticket's state never interleaves.
    applying: tokio::sync::Mutex<()>,
    /// What watches a tool's own file while its agent runs (set once at service start).
    watcher: OnceLock<watch::Watcher>,
    /// Each session's processes and pending looks, for turn ends the tool doesn't report (ADP-12).
    quiet: Mutex<HashMap<String, quiet::Quiet>>,
    quiet_timing: Mutex<quiet::QuietTiming>,
    /// One reminder before Idle (d0042 2026-10-28).
    reminders: reminder::Reminders,
}

/// One session's current turn.
#[derive(Debug, Clone, Copy, Default)]
struct Turn {
    facts: TurnFacts,
    /// The last hook ended the turn (Stop) and no new turn has started: the agent is idle between turns.
    ended: bool,
}

impl Signals {
    pub fn new(board: Board, events: EventBus, adapters: Adapters, sessions: Sessions) -> Self {
        Self {
            inner: Arc::new(Inner {
                board,
                events,
                adapters,
                sessions,
                turns: Mutex::new(HashMap::new()),
                refusals: OnceLock::new(),
                models: OnceLock::new(),
                turned: Mutex::new(HashSet::new()),
                applying: tokio::sync::Mutex::new(()),
                watcher: OnceLock::new(),
                quiet: Mutex::new(HashMap::new()),
                quiet_timing: Mutex::new(quiet::QuietTiming::default()),
                reminders: reminder::Reminders::default(),
            }),
        }
    }

    /// Gives the signals the tools' lists, for the swap mark (once, at service start).
    pub fn set_models(&self, models: crate::agent_tools::models::ModelLists) {
        let _ = self.inner.models.set(models);
    }

    /// The facts of a session's current turn.
    pub fn turn(&self, session: &str) -> TurnFacts {
        self.turn_state(session).facts
    }

    /// Whether the session's agent is idle between turns (its last hook ended the turn).
    pub fn is_idle_between_turns(&self, session: &str) -> bool {
        self.turn_state(session).ended
    }

    /// `otaq review submit` or `otaq ask human` ran in this turn, so its `Stop` isn't Idle.
    pub fn mark_submitted_or_asked(&self, session: &str) {
        self.inner
            .turns
            .lock()
            .expect("turns")
            .entry(session.to_owned())
            .or_default()
            .facts
            .submitted_or_asked = true;
    }

    /// Handles one hook from `caller` (ADP-03): `tool`'s adapter turns `event` and its payload into Otaq's events,
    /// and each is applied in order. Returns the signal of every event that concerns the status: none for a hook
    /// the adapter doesn't recognise (nothing changes), [`Signal::Ignored`] for a hook that gave no event (the
    /// last signal still moves). Hooks from outside an agent session (the plugin used by hand) change nothing.
    pub async fn hook(
        &self,
        caller: &Caller,
        tool: Tool,
        event: &str,
        payload: &Value,
    ) -> Result<Vec<Signal>, BoardError> {
        tracing::debug!(%tool, event, payload = %payload, agent = ?caller.ticket(), "hook");
        let session = session_of(caller);
        if let Some(session) = session {
            self.hook_came(session);
        }
        let stored = match session {
            Some(session) => self.stored_session(session).await?,
            None => StoredSession::default(),
        };
        // The command names the adapter (d0088): a session of another tool is only logged.
        if let Some(stored) = stored.tool
            && stored != tool
        {
            tracing::warn!(
                session,
                %stored,
                hook = %tool,
                "a hook names another tool than its session's; its adapter is used"
            );
        }
        let context = SignalContext {
            tool_id: stored.tool_id.as_deref(),
            cwd: stored.cwd.as_deref().map(std::path::Path::new),
        };
        let events = self
            .inner
            .adapters
            .get(tool)
            .map_hook(event, payload, context);
        self.tool_reported(caller, &events);
        if events.is_empty() {
            return Ok(vec![self.on_signal(caller, Signal::Ignored).await?]);
        }
        self.apply_events(caller, tool, &events).await
    }

    /// Applies `caller`'s events from `tool`'s adapter in order, as a hook's (a hook, or a read of the session's
    /// watched file, [`watch`]). Returns the signal of every event that concerns the status.
    pub async fn apply_events(
        &self,
        caller: &Caller,
        tool: Tool,
        events: &[ToolEvent],
    ) -> Result<Vec<Signal>, BoardError> {
        let session = session_of(caller);
        let mut signals = Vec::new();
        for event in events {
            // A turn end without background info: Otaq's own look at the processes (ADP-12).
            let found;
            let event = match session {
                Some(session) => {
                    self.quiet_event(tool, session, event);
                    match self.left_running(caller, tool, session, event) {
                        Some(with_work) => {
                            found = with_work;
                            &found
                        }
                        None => event,
                    }
                }
                None => event,
            };
            match event {
                // Stored, kept on a Project agent's row, and the session's file watched (ADP-07).
                ToolEvent::ToolIdKnown { tool_id } => {
                    if let Some(session) = session {
                        let stored = self.tool_id_known(session, tool_id).await?;
                        self.tool_id_reported(caller, tool, session, tool_id, stored)
                            .await?;
                    }
                    continue;
                }
                ToolEvent::ModelKnown { model } => {
                    if let Some(session) = session
                        && self.model_known(caller, session, model).await?
                    {
                        // A tool that quietly runs another model than the exact one asked for (AGT-05, T-0357).
                        self.swap_check(caller, tool, session, model).await?;
                    }
                    continue;
                }
                // Start failed, No result or a failed turn, and the agent's mark (AGT-05).
                ToolEvent::ModelRefused { model, message } => {
                    if let Some(session) = session
                        && let Some(signal) = self
                            .model_refused(caller, tool, session, model.as_deref(), message)
                            .await?
                    {
                        signals.push(self.on_signal(caller, signal).await?);
                    }
                    continue;
                }
                // A conversation's running totals from a tool that reports them as events (USG-09).
                ToolEvent::Tokens { totals } => {
                    if let Some(session) = session
                        && crate::usage::totals_reported(
                            self.inner.board.store().pool(),
                            session,
                            totals,
                        )
                        .await?
                    {
                        self.inner.events.publish(Event::UsageChanged);
                    }
                    continue;
                }
                ToolEvent::LimitWindows { .. } => {
                    tracing::debug!(?event, "usage from a hook not stored");
                    continue;
                }
                _ => {}
            }
            // A validator's turn facts don't matter: its turn ends are its validation's.
            let turn = match (caller, session) {
                (Caller::Validator { .. }, _) | (_, None) => TurnFacts::default(),
                (_, Some(session)) => self.turn(session),
            };
            if let Some(signal) = signal(event, turn) {
                // A turn that got through clears a mark its named agent has (AGT-05).
                if let Some(session) = session
                    && signal.ends_turn()
                    && !matches!(signal, Signal::LimitReached { .. })
                {
                    self.turn_got_through(session).await?;
                }
                signals.push(self.on_signal(caller, signal).await?);
            }
        }
        Ok(signals)
    }

    /// Applies one signal of `caller`'s hook and returns it as applied.
    async fn on_signal(&self, caller: &Caller, signal: Signal) -> Result<Signal, BoardError> {
        // A validator's hook only says where its turn is; its effect on the ticket is the validation's.
        if let Caller::Validator { .. } = caller {
            return Ok(signal);
        }
        // A Project agent has no ticket: its turn is tracked here (deliveries wait for its turn to end), its
        // effects are the Project agents' (d0068).
        if let Some((_, session)) = caller.project_agent() {
            self.track_turn(session, &signal);
            return Ok(signal);
        }
        // A helper's turn is tracked too (a note to it waits for its turn to end), but never moves its ticket:
        // the agent in charge does (d0133).
        if let Caller::Helper { session, .. } = caller {
            self.track_turn(session, &signal);
            return Ok(signal);
        }
        let Caller::Agent {
            ticket_id, session, ..
        } = caller
        else {
            tracing::info!("hook from outside an agent session ignored");
            return Ok(Signal::Ignored);
        };
        self.track_turn(session, &signal);
        // Helpers still at their pieces: the agent in charge waits for them, Working in background (d0133).
        let signal = self.with_helpers_at_work(ticket_id, signal).await?;
        // A stage still open at the turn's end: one reminder first, Idle only if it is ignored (d0042 2026-10-28).
        let signal = match signal {
            Signal::Idle { .. } if self.remind_instead_of_idle(ticket_id, session).await? => {
                Signal::TurnEnded
            }
            signal => signal,
        };
        if let Signal::Failed { error, message } = &signal {
            tracing::warn!(%error, message = ?message, "agent turn failed");
        }
        // The agent is alive in its conversation: its first prompt arrived.
        self.inner.board.first_prompt_delivered(ticket_id).await?;
        self.apply(ticket_id, &Input::Hook(signal.clone())).await?;
        Ok(signal)
    }

    /// The tool reported the session's own conversation id after start (d0088 "Session ids"): stored only while
    /// the session has none, so a later report never replaces it. Returns whether it was stored.
    pub async fn tool_id_known(&self, session: &str, tool_id: &str) -> Result<bool, BoardError> {
        self.inner
            .sessions
            .tool_id_known(session, tool_id)
            .await
            .map_err(|e| match e {
                SessionError::Store(e) => e.into(),
                other => BoardError::NotFound(other.to_string()),
            })
    }

    /// The tool reported the exact model the session runs on (ADP-04, AGT-04): the first value is saved on the
    /// session (and on its Project agent's row) and its ticket or agent is redrawn; a later different one
    /// (`/model` typed into the agent panel, the user's own business) is only logged, and the next launch passes the named
    /// agent's model again. Whether the value was the first (and saved).
    async fn model_known(
        &self,
        caller: &Caller,
        session: &str,
        model: &str,
    ) -> Result<bool, BoardError> {
        let ran = match self.inner.sessions.model_known(session, model).await {
            Ok(ran) => ran,
            Err(e) => {
                tracing::warn!(session, model, error = %e, "can't save the model the session runs on");
                return Ok(false);
            }
        };
        match ran {
            ModelRan::Written => {}
            ModelRan::Different { first } => {
                tracing::info!(
                    session,
                    first,
                    later = model,
                    "the session reports another model; the first is kept"
                );
                return Ok(false);
            }
            ModelRan::Same | ModelRan::NoSession => return Ok(false),
        }
        tracing::debug!(session, model, "model known");
        match caller {
            Caller::Agent {
                ticket, ticket_id, ..
            }
            | Caller::Validator {
                ticket, ticket_id, ..
            }
            | Caller::Helper {
                ticket, ticket_id, ..
            } => self.inner.events.publish(Event::TicketChanged {
                project: ticket.project.to_string(),
                id: ticket_id.clone(),
                key: ticket.clone(),
            }),
            Caller::ProjectAgent { project, agent, .. } => {
                sqlx::query("UPDATE project_agents SET model_ran = ? WHERE id = ?")
                    .bind(model)
                    .bind(agent)
                    .execute(self.inner.board.store().pool())
                    .await?;
                self.inner.events.publish(Event::ProjectAgentChanged {
                    project: project.clone(),
                    id: agent.clone(),
                });
            }
            Caller::User => {}
        }
        Ok(true)
    }

    /// The first model a session reports (AGT-05, d0088 "a tool that quietly swaps keeps running"): on a tool that
    /// swaps a refused model, a session that asked for one exact model and ran another marks its named agent ("Copilot
    /// ran auto instead of gpt-5.5 on SHOP-29"); "asked for X, ran on Y" is read from the session. A follow choice
    /// choosing a model is never a swap (Q-J).
    async fn swap_check(
        &self,
        caller: &Caller,
        tool: Tool,
        session: &str,
        ran: &str,
    ) -> Result<(), BoardError> {
        let adapter = self.inner.adapters.get(tool);
        if !adapter.swaps_refused_model() {
            return Ok(());
        }
        let row: Option<(Option<String>, Option<String>, String)> = sqlx::query_as(
            "SELECT s.model, COALESCE(s.agent_id, a.agent_id), p.name FROM sessions s
             JOIN projects p ON p.id = s.project_id LEFT JOIN project_agents a ON a.id = s.project_agent_id
             WHERE s.id = ?",
        )
        .bind(session)
        .fetch_optional(self.inner.board.store().pool())
        .await?;
        let Some((asked, Some(agent), project)) = row else {
            return Ok(());
        };
        let models = self
            .inner
            .models
            .get()
            .map(|m| m as &dyn crate::agents::ModelCatalog);
        let exact = crate::agents::mark::asked_exact(tool, asked.as_deref(), models);
        if !crate::agents::mark::is_swap(true, exact, asked.as_deref(), ran) {
            return Ok(());
        }
        let on = match caller {
            Caller::Agent { ticket, .. }
            | Caller::Validator { ticket, .. }
            | Caller::Helper { ticket, .. } => ticket.to_string(),
            Caller::ProjectAgent { .. } | Caller::User => project,
        };
        let asked = asked.unwrap_or_default();
        let mark = crate::agents::mark::swapped(adapter.display_name(), ran, &asked, &on);
        tracing::warn!(
            session,
            asked,
            ran,
            "the tool ran another model than the one asked for"
        );
        if crate::agents::mark::set(self.inner.board.store(), &agent, &mark).await? {
            self.inner.events.publish(Event::AgentsChanged);
        }
        Ok(())
    }

    /// The hook's session as stored: its tool, and what the adapter may need (its tool id and working folder).
    async fn stored_session(&self, session: &str) -> Result<StoredSession, BoardError> {
        let row: Option<(Option<String>, Option<String>, Option<String>)> = sqlx::query_as(
            "SELECT s.tool, s.tool_session_id, COALESCE(h.worktree, t.worktree_path, p.repo_path) FROM sessions s
             LEFT JOIN tickets t ON t.id = s.ticket_id LEFT JOIN projects p ON p.id = s.project_id
             LEFT JOIN helpers h ON h.session_id = s.id
             WHERE s.id = ?",
        )
        .bind(session)
        .fetch_optional(self.inner.board.store().pool())
        .await?;
        let Some((tool, tool_id, cwd)) = row else {
            return Ok(StoredSession::default());
        };
        Ok(StoredSession {
            tool: tool.and_then(|t| t.parse().ok()),
            tool_id: tool_id.filter(|id| !id.is_empty()),
            cwd,
        })
    }

    /// `caller`'s tool reported `events` (a hook, or a read of its watched file): when one says where the agent's turn
    /// is ([`reports_activity`]), the ticket's hand-off is picked up (d0118). The status line's model never is.
    pub fn tool_reported(&self, caller: &Caller, events: &[crate::agent_tools::ToolEvent]) {
        if events.iter().any(reports_activity) {
            self.agent_active(caller);
        }
    }

    /// The ticket's agent did something of its own (its tool reported its turn, or it ran an `otaq` command that isn't
    /// the status line's bookkeeping): a hand-off typed into its session is picked up (d0118).
    pub fn agent_active(&self, caller: &Caller) {
        if let Caller::Agent {
            ticket, ticket_id, ..
        } = caller
            && self.inner.board.hand_offs().picked_up(ticket_id)
        {
            self.inner.events.publish(Event::TicketChanged {
                project: ticket.project.to_string(),
                id: ticket_id.clone(),
                key: ticket.clone(),
            });
        }
    }

    /// The agent ran an `otaq` command that acts ([`crate::service::AgentActivity::Acts`]): it is active, so Needs
    /// you and Idle clear (ATT-02, never an open question, d0119) and the last signal moves (ATT-05).
    pub async fn command(&self, caller: &Caller) -> Result<(), BoardError> {
        if let Caller::Agent { ticket_id, .. } = caller {
            self.inner.board.first_prompt_delivered(ticket_id).await?;
            self.apply(ticket_id, &Input::Command).await?;
        }
        Ok(())
    }

    /// `otaq ask human` for `ticket_id`: the ticket shows Asks you with the question (ATT-01). From the
    /// ticket's agent, this turn's `Stop` won't turn it into Idle.
    pub async fn ask_human(
        &self,
        caller: &Caller,
        ticket_id: &str,
        question: &str,
    ) -> Result<(), BoardError> {
        if let Caller::Agent { session, .. } = caller {
            self.mark_submitted_or_asked(session);
        }
        self.apply(ticket_id, &Input::AskHuman(question.to_owned()))
            .await
    }

    /// Enter was pressed in the ticket's terminal: the user answered, so the needs-you label clears (ATT-02).
    /// `line` is what was sent from the agent's own terminal (none from a validator's): while the ticket has an open
    /// question and no permission prompt shows on top of it, a line that isn't empty answers it (d0119).
    /// Any other line of the user's own there starts a chat: its turn gets no stage reminder (d0042 2026-10-28).
    pub async fn enter(&self, ticket_id: &str, line: Option<&str>) -> Result<(), BoardError> {
        if let Some(line) = line
            && self.status_of(ticket_id).await? != Some(Status::NeedsYou)
        {
            match self.inner.board.answer_in_terminal(ticket_id, line).await? {
                Some(question) => tracing::info!(
                    ticket = ticket_id,
                    question = question.id,
                    "question answered in the terminal"
                ),
                None => self.chatting(ticket_id),
            }
        }
        self.apply(ticket_id, &Input::Enter).await
    }

    async fn status_of(&self, ticket_id: &str) -> Result<Option<Status>, BoardError> {
        let state: Option<String> = sqlx::query_scalar("SELECT state FROM tickets WHERE id = ?")
            .bind(ticket_id)
            .fetch_optional(self.inner.board.store().pool())
            .await?;
        Ok(state.and_then(|s| s.parse().ok()))
    }

    /// A helper's wait changed (d0133 audit 3): its prompt opened or closed (`ended_prompt`, the line the ticket showed
    /// for it), or its question was passed on to the user or answered. The ticket's status follows.
    pub async fn helper_wait_changed(
        &self,
        ticket_id: &str,
        ended_prompt: Option<String>,
    ) -> Result<(), BoardError> {
        self.apply(ticket_id, &Input::HelperWait { ended_prompt })
            .await
    }

    /// The line of the ticket's helper prompt that has been open the longest, of a helper whose session still runs:
    /// "40.3 needs you: allow …" (d0133 audit 3).
    async fn helper_prompt(&self, ticket_id: &str) -> Result<Option<String>, BoardError> {
        let open =
            crate::helpers::store::open_prompts(self.inner.board.store().pool(), ticket_id).await?;
        Ok(open.into_iter().find_map(|helper| {
            let running = helper
                .session
                .as_deref()
                .and_then(|s| self.inner.sessions.info(s).ok())
                .is_some_and(|info| info.is_running());
            let prompt = helper.prompt.as_deref().filter(|_| running)?;
            Some(crate::helpers::waits::needs_you_line(
                &helper.label(),
                prompt,
            ))
        }))
    }

    /// The user answered the agent from Needs you (or the card or sheet): the needs-you label clears (ATT-02).
    pub async fn answered(&self, ticket_id: &str) -> Result<(), BoardError> {
        self.apply(ticket_id, &Input::Answered).await
    }

    /// The agent in charge of `ticket_id` ended its turn: a helper still at its piece (its session mid-turn) or held for
    /// its tool's limit is work it waits for, so Idle becomes Working in background ("background: helper 6.1"), and
    /// those helpers join what the tool left running. A helper that hands back or stops is told to the agent in charge,
    /// whose next turn follows (d0133 audit 3).
    async fn with_helpers_at_work(
        &self,
        ticket_id: &str,
        signal: Signal,
    ) -> Result<Signal, BoardError> {
        let mut work = match &signal {
            Signal::Idle { .. } => BackgroundWork::default(),
            Signal::WorkingInBackground { work } => work.clone(),
            _ => return Ok(signal),
        };
        let running: HashSet<String> = self
            .inner
            .sessions
            .running_all_of(ticket_id, SessionKind::Helper)
            .into_iter()
            .map(|s| s.id)
            .collect();
        let helpers =
            crate::helpers::store::of_ticket(self.inner.board.store().pool(), ticket_id).await?;
        let at_work: Vec<BackgroundTask> = helpers
            .iter()
            .filter(|h| match h.status {
                HelperStatus::Held => true,
                HelperStatus::Working | HelperStatus::SentBack => h
                    .session
                    .as_ref()
                    .is_some_and(|s| running.contains(s) && !self.is_idle_between_turns(s)),
                _ => false,
            })
            .map(|h| BackgroundTask {
                kind: "helper".into(),
                description: Some(format!("helper {}", h.label())),
                helper_agent: false,
            })
            .collect();
        if at_work.is_empty() {
            return Ok(signal);
        }
        work.tasks.extend(at_work);
        Ok(Signal::WorkingInBackground { work })
    }

    /// A new turn starts afresh; a turn that ended leaves the session idle between turns.
    fn track_turn(&self, session: &str, signal: &Signal) {
        let mut turns = self.inner.turns.lock().expect("turns");
        let turn = turns.entry(session.to_owned()).or_default();
        match signal {
            Signal::Working { .. } => *turn = Turn::default(),
            s if s.ends_turn() => turn.ended = true,
            _ => {}
        }
    }

    fn turn_state(&self, session: &str) -> Turn {
        self.inner
            .turns
            .lock()
            .expect("turns")
            .get(session)
            .copied()
            .unwrap_or_default()
    }

    /// Runs one transition on the stored state, writes what changed and tells clients.
    async fn apply(&self, ticket_id: &str, input: &Input) -> Result<(), BoardError> {
        let _one_at_a_time = self.inner.applying.lock().await;
        let pool = self.inner.board.store().pool();
        let row: Option<StoredSignal> = sqlx::query_as(
            "SELECT state, signal_text, signal_since, wakeup_at, last_signal_at, interrupted_from FROM tickets
             WHERE id = ?",
        )
        .bind(ticket_id)
        .fetch_optional(pool)
        .await?;
        let Some((state, text, since, wakeup_at, last_signal_at, interrupted_from)) = row else {
            return Ok(());
        };
        let Ok(status) = state.parse::<Status>() else {
            return Ok(());
        };
        // An agent that waits for the user during a validation is marked so (interrupted_from validating).
        let behind = interrupted_from.as_deref() == Some(Status::Checking.as_str());
        // The store decides (d0119): an open question holds Asks you.
        let question = self
            .inner
            .board
            .open_user_question(ticket_id)
            .await?
            .map(|q| q.text);
        let helper_prompt = self.helper_prompt(ticket_id).await?;
        let before = SignalState {
            status,
            text,
            since,
            wakeup_at,
            last_signal_at,
            validating: status == Status::Checking
                || status == Status::CheckerNeedsYou
                || (behind && status.is_agent_waiting()),
            question,
            helper_prompt,
        };
        let after = transition(&before, input, now_ms());
        if after == before {
            return Ok(());
        }
        // An agent the plan limit stopped is at work again (Claude Code continued it, or the user did): it is no
        // longer the limit's to hide from Needs you (ATT-09).
        if before.status == Status::WaitingForLimitReset && after.status != before.status {
            self.inner.board.limit_stops().mark(ticket_id, None);
        }
        // Only a validating ticket's mark changes: set while its agent waits, cleared once it validates again.
        let behind_after = (before.validating && after.status.is_agent_waiting())
            .then_some(Status::Checking.as_str());
        sqlx::query(concat!(
            "UPDATE tickets SET state = ?, signal_text = ?, signal_since = ?, wakeup_at = ?,
                                last_signal_at = ?,
                                interrupted_from = CASE WHEN ? THEN ? ELSE interrupted_from END, ",
            set_state_since!(),
            " WHERE id = ?"
        ))
        .bind(after.status.as_str())
        .bind(&after.text)
        .bind(after.since)
        .bind(after.wakeup_at)
        .bind(after.last_signal_at)
        .bind(before.validating)
        .bind(behind_after)
        .bind(after.status.as_str())
        .bind(now_ms())
        .bind(ticket_id)
        .execute(pool)
        .await?;
        let ticket = self.inner.board.ticket_by_id(ticket_id).await?;
        self.publish(&ticket, after.status != before.status);
        Ok(())
    }

    fn publish(&self, ticket: &Ticket, status_changed: bool) {
        let project = ticket.key.project.to_string();
        self.inner.events.publish(Event::TicketChanged {
            project: project.clone(),
            id: ticket.id.clone(),
            key: ticket.key.clone(),
        });
        if status_changed {
            self.inner.events.publish(Event::CardStatusChanged {
                project,
                id: ticket.id.clone(),
                key: ticket.key.clone(),
                status: ticket.status,
            });
        }
    }
}

/// The session a caller runs in.
fn session_of(caller: &Caller) -> Option<&str> {
    match caller {
        Caller::Agent { session, .. }
        | Caller::Validator { session, .. }
        | Caller::Helper { session, .. }
        | Caller::ProjectAgent { session, .. } => Some(session),
        Caller::User => None,
    }
}

/// A hook's session as stored: what the adapter's [`SignalContext`] carries.
#[derive(Debug, Default)]
struct StoredSession {
    tool: Option<Tool>,
    tool_id: Option<String>,
    cwd: Option<String>,
}

/// `state, signal_text, signal_since, wakeup_at, last_signal_at, interrupted_from` of a ticket.
type StoredSignal = (
    String,
    Option<String>,
    Option<i64>,
    Option<i64>,
    Option<i64>,
    Option<String>,
);

/// The delivery queue's "idle between turns" (SES-06): the agent's last hook ended its turn.
impl IdleSignal for Signals {
    fn is_idle(&self, session: &str) -> bool {
        self.is_idle_between_turns(session)
    }

    /// Typing a message starts the agent's next turn: until its next Stop it isn't idle any more.
    fn delivered(&self, session: &str) {
        if let Some(turn) = self.inner.turns.lock().expect("turns").get_mut(session) {
            turn.ended = false;
        }
    }
}
