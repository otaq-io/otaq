//! The check before every launch and the fallback (SCH-04 `[M23]`, AGM-03, AGM-05, USG-10, STG-07; d0108, d0086,
//! d0088, d0018).
//!
//! Every **new session** runs it right after resolution ([`super::resolve`]): an Agent stage's start (and Retry
//! start), each agent check's round, an epic lead's start (Open lead, the drag), a Project agent's first message.
//! **Resumes are never checked** (Send back, Park → Dev, Resume, the PR resume, Answer): a running conversation never
//! changes tool, so a resume on a tool that isn't available or is at its limit fails the existing way.
//!
//! The agent's model is resolved first (`default` → the default model; the new-session tool with its default model
//! while none is set), then checked, in order ([`check`]):
//! 1. **the tool available?** (its state on Settings → Agent tools) — no: `tool_unavailable` with the tool's reason;
//! 2. **the tool at its limit?** (an active limit of that tool not reset yet) — yes: `tool_limit`;
//! 3. **the model listed?** (Otaq's copy of the tool's list: an exact id or a follow choice must be in it, a family
//!    choice must resolve; the tool's default always passes; a tool whose list was never read passes, except a family
//!    choice) — no: `model_unavailable`, with the agent marked "Codex no longer lists gpt-5.4".
//!
//! A session that can't start is **held** ([`super::AgentHold`]), offering its tool's **fallback** (AGM-03: one per
//! tool, "When Claude Code hits its limit → Codex · newest GPT") when that can start now, or saying it can't either.
//! **Nothing moves to a fallback without the user's click** (user decision 2026-10-20): a card's **Use fallback**
//! ([`LaunchGate::use_fallback`]) and the banner's **Use fallback now** ([`LaunchGate::use_fallback_now`]) ask for it;
//! the held start then launches on the fallback, saved on the session ("on Codex · Claude Code limit"), so Resume
//! reopens the same tool and model. One step only: a fallback that can't run leaves the ticket waiting. A Project
//! agent's first message never takes a fallback (Q-B). A held Developer stays waiting before its Agent stage (Held,
//! its step naming the reason; Ready for Dev before the first, between two Agent stages before the next, STG-07).
//!
//! **A hold lifts** when its cause may have gone ([`LaunchGate::run`]): a tool's state changes, a limit is resumed or
//! resets, a list is read again, an agent changes, or the user's Use fallback. Each lift re-runs the check once for
//! every hold ([`LaunchGate::lift`]); the held start then runs as the drag already asked for it (Q-D). Nothing retries
//! on a timer. **A paused project's holds stay** (PAU-04, d0112): a lift leaves them as they are, and the project's
//! Resume lifts them once more.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, OnceLock};

use serde_json::json;
use tokio::sync::broadcast::error::RecvError;
use tokio::sync::{Mutex, watch};

use super::store::HoldScope;
use super::{
    Agent, AgentHold, FallbackFrom, Fallbacks, HoldReason, ModelRef, RunsOn, SessionRole, mark,
    resolve,
};
use crate::agent_tools::Tool;
use crate::agent_tools::connected::{ConnectedTools, NotAvailable};
use crate::agent_tools::follow::FollowChoice;
use crate::agent_tools::models::{ModelCopy, ModelLists};
use crate::board::tickets::{EventKind, Ticket, record_event};
use crate::board::{Board, BoardError, Refusal};
use crate::domain::{Actor, StartStep, Status, StatusSet, TicketKey};
use crate::protocol::Event;
use crate::service::EventBus;
use crate::store::tickets::set_state_since;
use crate::store::{new_id, now_ms};

/// What the checks read at one moment: built by the service from its tool states, limits and lists
/// ([`LaunchGate::facts`]); tests build it directly.
#[derive(Debug, Clone, Default)]
pub struct Facts {
    /// The tools that can't start an agent now, with why ("logged out").
    pub unavailable: HashMap<Tool, String>,
    /// The tools at their limit: an active limit that hasn't reset (USG-03).
    pub at_limit: HashSet<Tool>,
    /// Otaq's copy of each tool's list that was ever read, with its follow entries.
    pub lists: HashMap<Tool, ModelCopy>,
}

/// Why a model can't start now, in the order the checks run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Blocked {
    /// Its tool isn't available, with the tool's reason.
    Unavailable { reason: String },
    /// Its tool is at its limit.
    AtLimit,
    /// Its model isn't in its tool's list any more.
    ModelUnlisted,
}

impl Blocked {
    pub fn reason(&self) -> HoldReason {
        match self {
            Self::Unavailable { .. } => HoldReason::ToolUnavailable,
            Self::AtLimit => HoldReason::ToolLimit,
            Self::ModelUnlisted => HoldReason::ModelUnavailable,
        }
    }
}

/// The model `model` (as stored) passes to its tool, by `copy` (Otaq's copy of the tool's list, if it was ever read):
/// `Some(None)` passes no model option, `Some(Some(id))` passes `id` (a family choice resolved to its newest listed
/// model), `None` when the list doesn't have it (Model unavailable).
pub fn passed_model(model: Option<&str>, copy: Option<&ModelCopy>) -> Option<Option<String>> {
    let Some(model) = model.map(str::trim).filter(|m| !m.is_empty()) else {
        return Some(None);
    };
    let family = matches!(
        FollowChoice::parse(model),
        Some(FollowChoice::Newest { .. })
    );
    match copy {
        Some(copy) if family => copy.resolve_follow(model).map(Some),
        Some(copy) => copy.list.contains(model).then(|| Some(model.to_owned())),
        // Never block on a list that was never read (d0086), but a family has nothing to resolve to without one.
        None if family => None,
        None => Some(Some(model.to_owned())),
    }
}

/// The checks before a launch on `on`, in order: its tool available, not at its limit, its model listed. The model to
/// pass when they pass.
pub fn check(on: &ModelRef, facts: &Facts) -> Result<Option<String>, Blocked> {
    if let Some(reason) = facts.unavailable.get(&on.tool) {
        return Err(Blocked::Unavailable {
            reason: reason.clone(),
        });
    }
    if facts.at_limit.contains(&on.tool) {
        return Err(Blocked::AtLimit);
    }
    passed_model(on.model.as_deref(), facts.lists.get(&on.tool)).ok_or(Blocked::ModelUnlisted)
}

/// Whether a held session may go to its tool's fallback (Q-B): an Agent stage's, an agent check's round, an epic
/// lead; never a Project agent's first message.
pub fn takes_fallback(role: SessionRole) -> bool {
    role != SessionRole::ProjectAgent
}

/// A session that can't start, and what can take it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Held {
    /// The agent it would have started.
    pub agent: Agent,
    /// The tool and model it would have run on.
    pub on: ModelRef,
    pub blocked: Blocked,
    /// The tool's fallback when it can start now (Use fallback).
    pub fallback: Option<ModelRef>,
    /// The tool's fallback can't start either ("both at their limits").
    pub fallback_held: bool,
}

impl Held {
    /// The line stored with the hold: "Codex isn't available · logged out", "Claude Code limit reached", "Model
    /// unavailable · Codex no longer lists gpt-5.4". The cards and agent panels word it from the hold.
    pub fn message(&self) -> String {
        let tool = self.on.tool;
        match &self.blocked {
            Blocked::Unavailable { reason } => NotAvailable {
                reason: reason.clone(),
            }
            .message(tool),
            Blocked::AtLimit => format!("{} limit reached", tool.display_name()),
            Blocked::ModelUnlisted => format!(
                "Model unavailable \u{b7} {}",
                mark::no_longer_lists_text(
                    tool.display_name(),
                    self.on.model.as_deref().unwrap_or_default()
                )
            ),
        }
    }

    /// The Developer's start-problem step for it.
    pub fn step(&self) -> StartStep {
        match self.blocked.reason() {
            HoldReason::ToolLimit => StartStep::ToolLimit,
            HoldReason::ToolUnavailable => StartStep::ToolUnavailable,
            HoldReason::ModelUnavailable => StartStep::ModelUnavailable,
        }
    }
}

/// What the check decides for a new session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decision {
    /// Launch on `on`, passing `model`; `fallback` when it is the tool's fallback the user asked for.
    Launch {
        on: ModelRef,
        model: Option<String>,
        fallback: Option<FallbackFrom>,
    },
    Hold(Box<Held>),
}

/// The decision for a new session of `agent` on `on` (pure): launch when the checks pass; else, with `asked` (the
/// user's Use fallback) and a session that `takes` a fallback, launch on the tool's fallback when it passes them
/// itself (one step); else held, offering the fallback when it could start.
pub fn decide(
    agent: &Agent,
    on: &ModelRef,
    fallbacks: &Fallbacks,
    facts: &Facts,
    asked: bool,
    takes: bool,
) -> Decision {
    let blocked = match check(on, facts) {
        Ok(model) => {
            return Decision::Launch {
                on: on.clone(),
                model,
                fallback: None,
            };
        }
        Err(blocked) => blocked,
    };
    let fallback = fallbacks
        .get(&on.tool)
        .filter(|f| takes && f.tool != on.tool);
    let held = |fallback: Option<ModelRef>, fallback_held: bool| {
        Decision::Hold(Box::new(Held {
            agent: agent.clone(),
            on: on.clone(),
            blocked: blocked.clone(),
            fallback,
            fallback_held,
        }))
    };
    match fallback.map(|f| (f, check(f, facts))) {
        Some((f, Ok(model))) if asked => Decision::Launch {
            on: f.clone(),
            model,
            fallback: Some(FallbackFrom {
                tool: on.tool,
                reason: blocked.reason(),
            }),
        },
        Some((f, Ok(_))) => held(Some(f.clone()), false),
        Some((_, Err(_))) => held(None, true),
        None => held(None, false),
    }
}

/// What a session of `agent` launches on after `decision` launched it: the agent's effort only with its own model.
fn runs_on(
    agent: &Agent,
    on: ModelRef,
    model: Option<String>,
    fallback: Option<FallbackFrom>,
) -> RunsOn {
    let own = fallback.is_none() && agent.model.tool() == Some(on.tool);
    RunsOn {
        tool: Some(on.tool),
        model,
        effort: agent.effort.clone().filter(|_| own),
        agent: Some(agent.session_agent()),
        fallback,
    }
}

/// Where a checked session belongs.
#[derive(Debug, Clone, Copy)]
pub enum Scope<'a> {
    /// A ticket's Agent stage or agent check, an epic's lead.
    Ticket { id: &'a str, key: &'a TicketKey },
    /// A project's Project agent (its first message), with the project's name for the mark.
    Project { id: &'a str, name: &'a str },
}

impl<'a> Scope<'a> {
    pub fn ticket(ticket: &'a Ticket) -> Self {
        Self::Ticket {
            id: &ticket.id,
            key: &ticket.key,
        }
    }

    fn hold_scope(self) -> HoldScope<'a> {
        match self {
            Self::Ticket { id, .. } => HoldScope::Ticket(id),
            Self::Project { id, .. } => HoldScope::Project(id),
        }
    }

    /// Where a mark says it happened: the key, or the project.
    fn on(self) -> String {
        match self {
            Self::Ticket { key, .. } => key.to_string(),
            Self::Project { name, .. } => name.to_owned(),
        }
    }
}

/// What the check answers a launch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Gated {
    /// Launch on this.
    Launch(RunsOn),
    /// Held: nothing starts.
    Held(Box<Held>),
}

/// What a lift starts again once a validator's or a lead's hold goes (set once at service start).
#[derive(Clone)]
pub struct Restarts {
    pub validation: crate::validation::Validation,
    pub project_agents: crate::project_agents::ProjectAgents,
}

/// The check in the service. Cheap to clone.
#[derive(Clone)]
pub struct LaunchGate {
    inner: Arc<Inner>,
}

struct Inner {
    board: Board,
    connected: ConnectedTools,
    models: ModelLists,
    events: EventBus,
    /// Whether the tools' detected states count. Off (tests, which detect nothing): every tool counts as available,
    /// as the stand-in agents run as them.
    check_tool_states: bool,
    new_session_tool: Tool,
    restarts: OnceLock<Restarts>,
    /// One lift at a time.
    lifting: Mutex<()>,
    /// Asks the loop for a lift ([`LaunchGate::nudge`]).
    wake: tokio::sync::Notify,
}

impl LaunchGate {
    pub fn new(
        board: Board,
        connected: ConnectedTools,
        models: ModelLists,
        events: EventBus,
        check_tool_states: bool,
        new_session_tool: Tool,
    ) -> Self {
        Self {
            inner: Arc::new(Inner {
                board,
                connected,
                models,
                events,
                check_tool_states,
                new_session_tool,
                restarts: OnceLock::new(),
                lifting: Mutex::new(()),
                wake: tokio::sync::Notify::new(),
            }),
        }
    }

    /// Gives the gate what starts a validator or a lead again when their hold lifts.
    pub fn set_restarts(&self, restarts: Restarts) {
        let _ = self.inner.restarts.set(restarts);
    }

    /// The facts now: every tool's state, the tools at their limit, the lists.
    pub async fn facts(&self) -> Result<Facts, BoardError> {
        let inner = &self.inner;
        let mut facts = Facts::default();
        for &tool in Tool::ALL {
            if inner.check_tool_states
                && let Err(not) = inner.connected.is_available(tool)
            {
                facts.unavailable.insert(tool, not.reason);
            }
            if let Some(copy) = inner.models.copy(tool) {
                facts.lists.insert(tool, copy);
            }
        }
        facts.at_limit = crate::usage::plan::limits(inner.board.store())
            .await?
            .into_iter()
            .filter(|l| l.reset_seen_at.is_none())
            .map(|l| l.tool)
            .collect();
        Ok(facts)
    }

    /// The tool and model `agent` runs on now (AGM-03, AGM-05): its own, or the default model (the new-session tool
    /// with its default model while none is set).
    pub async fn model_of(&self, agent: &Agent) -> Result<ModelRef, BoardError> {
        let default = self.inner.board.store().default_model().await?;
        Ok(agent.model.on(default.as_ref()).unwrap_or(ModelRef {
            tool: self.inner.new_session_tool,
            model: None,
        }))
    }

    /// What `agent` would launch on now without the check (a gate-less test service, the PR path's tool check).
    pub async fn runs_on(&self, agent: &Agent) -> Result<RunsOn, BoardError> {
        let on = self.model_of(agent).await?;
        let model = on.model.clone();
        Ok(runs_on(agent, on, model, None))
    }

    /// The check before a new session of `role` in `scope`, on `agent`. A launch clears an earlier hold; a hold
    /// writes the hold (and the "no longer lists" mark). An earlier Use fallback on the hold launches on the
    /// fallback.
    pub async fn before_launch(
        &self,
        role: SessionRole,
        scope: Scope<'_>,
        agent: &Agent,
    ) -> Result<Gated, BoardError> {
        let store = self.inner.board.store();
        let asked = store
            .agent_hold(scope.hold_scope(), role)
            .await?
            .is_some_and(|h| h.fallback_asked);
        let decision = decide(
            agent,
            &self.model_of(agent).await?,
            &store.fallbacks().await?,
            &self.facts().await?,
            asked,
            takes_fallback(role),
        );
        match decision {
            Decision::Launch {
                on,
                model,
                fallback,
            } => {
                store.clear_agent_hold(scope.hold_scope(), role).await?;
                if let (Some(from), Scope::Ticket { id, key }) = (&fallback, scope) {
                    self.record_fallback(id, key, role, from, &on).await?;
                }
                Ok(Gated::Launch(runs_on(agent, on, model, fallback)))
            }
            Decision::Hold(held) => {
                self.write_hold(role, scope, &held).await?;
                Ok(Gated::Held(held))
            }
        }
    }

    /// A launch failed because of its tool (the program missing, the adapter's refusal; ADP-10): the tool is checked
    /// again, then the check decides once more. `Some` when the session is held now; `None` leaves the failure as it
    /// is.
    pub async fn after_failed_launch(
        &self,
        role: SessionRole,
        scope: Scope<'_>,
        agent: &Agent,
    ) -> Result<Option<Held>, BoardError> {
        if !self.inner.check_tool_states {
            return Ok(None);
        }
        let on = self.model_of(agent).await?;
        self.inner.connected.recheck(Some(on.tool)).await?;
        let decision = decide(
            agent,
            &on,
            &self.inner.board.store().fallbacks().await?,
            &self.facts().await?,
            false,
            takes_fallback(role),
        );
        let Decision::Hold(held) = decision else {
            return Ok(None);
        };
        self.write_hold(role, scope, &held).await?;
        Ok(Some(*held))
    }

    /// Asks for a lift soon: a hold was just written with everything it holds (a held Developer's ticket is Held),
    /// so a cause that went meanwhile isn't missed.
    pub fn nudge(&self) {
        self.inner.wake.notify_one();
    }

    /// History's line for a session started on its tool's fallback: "Started on Codex · Claude Code limit".
    async fn record_fallback(
        &self,
        ticket_id: &str,
        key: &TicketKey,
        role: SessionRole,
        from: &FallbackFrom,
        on: &ModelRef,
    ) -> Result<(), BoardError> {
        let store = self.inner.board.store();
        let mut tx = store.begin_write().await?;
        record_event(
            &mut tx,
            ticket_id,
            EventKind::FallbackUsed,
            &Actor::current_user(),
            json!({
                "role": role.as_str(),
                "tool": on.tool.as_str(),
                "model": on.model,
                "from_tool": from.tool.as_str(),
                "reason": from.reason.as_str(),
            }),
        )
        .await?;
        tx.commit().await?;
        tracing::info!(%key, %role, from = %from.tool, to = %on.tool, "started on the fallback");
        Ok(())
    }

    /// Writes the hold of `role` in `scope`; a held Developer's ticket keeps its message current. A model no longer
    /// listed marks the agent.
    async fn write_hold(
        &self,
        role: SessionRole,
        scope: Scope<'_>,
        held: &Held,
    ) -> Result<(), BoardError> {
        let store = self.inner.board.store();
        let (ticket_id, project_id) = match scope {
            Scope::Ticket { id, .. } => (Some(id.to_owned()), None),
            Scope::Project { id, .. } => (None, Some(id.to_owned())),
        };
        store
            .set_agent_hold(&AgentHold {
                id: new_id(),
                ticket_id,
                project_id,
                role,
                reason: held.blocked.reason(),
                tool: held.on.tool,
                model: held.on.model.clone(),
                agent_id: Some(held.agent.id.clone()),
                fallback_tool: held.fallback.as_ref().map(|_| held.on.tool),
                fallback_held: held.fallback_held,
                fallback_asked: false,
                since: now_ms(),
            })
            .await?;
        if held.blocked == Blocked::ModelUnlisted
            && let Some(model) = held.on.model.as_deref()
            && mark::set_if_new(
                store,
                &held.agent,
                &mark::no_longer_lists(held.on.tool.display_name(), model, &scope.on()),
            )
            .await?
        {
            self.inner.events.publish(Event::AgentsChanged);
        }
        if let Scope::Ticket { id, key } = scope {
            if role == SessionRole::Developer {
                // Already held (a lift that holds again): the reason may have changed.
                sqlx::query(
                    "UPDATE tickets SET start_problem_step = ?, start_problem_message = ? WHERE id = ? AND state = ?",
                )
                .bind(held.step().as_str())
                .bind(held.message())
                .bind(id)
                .bind(Status::Held.as_str())
                .execute(store.pool())
                .await?;
            }
            self.ticket_changed(id, key);
        }
        tracing::info!(on = scope.on(), %role, reason = %held.blocked.reason(), tool = %held.on.tool,
            agent = held.agent.name, "held before launch");
        Ok(())
    }

    /// The tool's fallback for `tool`'s waiting work, refused when there is none or it can't start now (one step:
    /// the tickets stay waiting).
    async fn usable_fallback(&self, tool: Tool) -> Result<ModelRef, BoardError> {
        let refused = |message: String| BoardError::refused(Refusal::MoveRefused, message);
        let fallback = self
            .inner
            .board
            .store()
            .fallbacks()
            .await?
            .get(&tool)
            .cloned()
            .filter(|f| f.tool != tool)
            .ok_or_else(|| {
                refused(format!(
                    "{} has no fallback: set one in Settings \u{2192} Agents",
                    tool.display_name()
                ))
            })?;
        if let Err(blocked) = check(&fallback, &self.facts().await?) {
            let why = match blocked {
                Blocked::Unavailable { reason } => NotAvailable { reason }.message(fallback.tool),
                Blocked::AtLimit => format!("{} limit reached", fallback.tool.display_name()),
                Blocked::ModelUnlisted => mark::no_longer_lists_text(
                    fallback.tool.display_name(),
                    fallback.model.as_deref().unwrap_or_default(),
                ),
            };
            return Err(refused(format!(
                "{}'s fallback can't start either: {why}",
                tool.display_name()
            )));
        }
        Ok(fallback)
    }

    /// **Use fallback** (`ticket.use_fallback`, the user's click; AGM-03): the ticket's held session starts on its
    /// tool's fallback. Refused when nothing is held, for a Project agent, without a fallback, or when the fallback
    /// can't start either (the ticket stays waiting).
    pub async fn use_fallback(&self, key: &str) -> Result<(), BoardError> {
        let board = &self.inner.board;
        let store = board.store();
        let ticket = board.ticket(key).await?;
        let key = ticket.key.to_string();
        let hold = store
            .ticket_holds(&ticket.id)
            .await?
            .into_iter()
            .find(|h| takes_fallback(h.role) && still_waits(&ticket, h.role))
            .ok_or_else(|| {
                BoardError::refused(Refusal::MoveRefused, format!("{key} isn't waiting"))
            })?;
        self.usable_fallback(hold.tool).await?;
        store.ask_fallback(&ticket.id, hold.role).await?;
        tracing::info!(%key, role = %hold.role, tool = %hold.tool, "use fallback");
        self.release(hold.role, &ticket).await
    }

    /// **Use fallback now** (`limits.use_fallback_now`, the banner's click; AGM-03): every ticket waiting on `tool`
    /// starts on its fallback. Refused without a fallback or when it can't start either (nothing moves). The tickets
    /// started, in key order.
    pub async fn use_fallback_now(&self, tool: Tool) -> Result<Vec<TicketKey>, BoardError> {
        self.usable_fallback(tool).await?;
        let board = &self.inner.board;
        let store = board.store();
        let mut started = Vec::new();
        for hold in store.holds_on_tool(tool).await? {
            let Some(ticket_id) = hold.ticket_id.as_deref() else {
                continue;
            };
            let Ok(ticket) = board.ticket_by_id(ticket_id).await else {
                continue;
            };
            if !takes_fallback(hold.role) || !still_waits(&ticket, hold.role) {
                continue;
            }
            store.ask_fallback(&ticket.id, hold.role).await?;
            self.release(hold.role, &ticket).await?;
            started.push(ticket.key);
        }
        started.sort();
        started.dedup();
        tracing::info!(%tool, tickets = started.len(), "use fallback now");
        Ok(started)
    }

    /// Runs the check once more for every hold (a lift): a hold whose cause is gone (or whose fallback the user asked
    /// for and can start) starts as it was asked to (the Developer back to Queued for the scheduler, a validator, a
    /// lead; a Project agent's hold just goes: its message waits in the agent panel for the user's send), one still blocked
    /// is written again, one whose work went away goes. Also clears the "no longer lists" marks whose model is listed
    /// again. The number of holds that started again.
    pub async fn lift(&self) -> Result<usize, BoardError> {
        let _one = self.inner.lifting.lock().await;
        let store = self.inner.board.store();
        let agents = store.agents().await?;
        let facts = self.facts().await?;
        let cleared = mark::clear_listed_again(store, &agents, |tool, model| {
            passed_model(Some(model), facts.lists.get(&tool)).is_some()
        })
        .await?;
        if !cleared.is_empty() {
            tracing::info!(agents = ?cleared, "the tool lists the model again: marks cleared");
            self.inner.events.publish(Event::AgentsChanged);
        }
        let mut holds = Vec::new();
        for &reason in HoldReason::ALL {
            holds.extend(store.holds_for(reason).await?);
        }
        if holds.is_empty() {
            return Ok(0);
        }
        let fallbacks = store.fallbacks().await?;
        let mut started = 0;
        for hold in holds {
            match self.lift_one(&hold, &fallbacks, &facts).await {
                Ok(true) => started += 1,
                Ok(false) => {}
                Err(e) => tracing::warn!(hold = hold.id, error = %e, "can't check a hold again"),
            }
        }
        Ok(started)
    }

    async fn lift_one(
        &self,
        hold: &AgentHold,
        fallbacks: &Fallbacks,
        facts: &Facts,
    ) -> Result<bool, BoardError> {
        let board = &self.inner.board;
        let store = board.store();
        if let Some(project_id) = &hold.project_id {
            let Some(project) = board
                .projects()
                .await?
                .into_iter()
                .find(|p| &p.id == project_id)
            else {
                return Ok(false);
            };
            if super::pause::is_paused(store, &project.key).await? {
                return Ok(false);
            }
            let scope = Scope::Project {
                id: &project.id,
                name: &project.name,
            };
            let agent = resolve::role_agent(store, super::Role::ProjectAgent).await?;
            let on = self.model_of(&agent).await?;
            if let Decision::Hold(held) = decide(&agent, &on, fallbacks, facts, false, false) {
                self.write_hold(hold.role, scope, &held).await?;
                return Ok(false);
            }
            store
                .clear_agent_hold(scope.hold_scope(), hold.role)
                .await?;
            return Ok(false);
        }
        let Some(ticket_id) = &hold.ticket_id else {
            return Ok(false);
        };
        let ticket = board.ticket_by_id(ticket_id).await?;
        // Kept for the project's Resume (PAU-04).
        if super::pause::is_paused(store, ticket.key.project.as_str()).await? {
            return Ok(false);
        }
        let scope = Scope::ticket(&ticket);
        if !still_waits(&ticket, hold.role) {
            store
                .clear_agent_hold(scope.hold_scope(), hold.role)
                .await?;
            return Ok(false);
        }
        // A start that is holding it right now writes the ticket's status after the hold.
        if hold.role == SessionRole::Developer && ticket.status != Status::Held {
            return Ok(false);
        }
        let agent = resolve::session_agent(board, &ticket, hold.role).await?;
        let on = self.model_of(&agent).await?;
        let decision = decide(
            &agent,
            &on,
            fallbacks,
            facts,
            hold.fallback_asked,
            takes_fallback(hold.role),
        );
        if let Decision::Hold(held) = decision {
            if !same_hold(hold, &held) {
                self.write_hold(hold.role, scope, &held).await?;
            }
            return Ok(false);
        }
        // The start itself checks once more; a Use fallback stays on the hold for it to read.
        if !hold.fallback_asked {
            store
                .clear_agent_hold(scope.hold_scope(), hold.role)
                .await?;
        }
        tracing::info!(key = %ticket.key, role = %hold.role, "hold lifted");
        self.release(hold.role, &ticket).await?;
        Ok(true)
    }

    /// Starts again what a lifted hold held back.
    async fn release(&self, role: SessionRole, ticket: &Ticket) -> Result<(), BoardError> {
        match role {
            SessionRole::Developer => self.requeue(ticket).await,
            SessionRole::Validator => match self.inner.restarts.get() {
                Some(restarts) => restarts.validation.start_held(&ticket.id).await,
                None => Ok(()),
            },
            SessionRole::EpicLead => match self.inner.restarts.get() {
                Some(restarts) => restarts
                    .project_agents
                    .leads()
                    .start_held(&ticket.key.to_string())
                    .await
                    .map(|_| ()),
                None => Ok(()),
            },
            SessionRole::ProjectAgent => Ok(()),
        }
    }

    /// A held Developer goes back to Queued, its start problem cleared; the scheduler starts it (SCH-02: the drag
    /// asked for it).
    async fn requeue(&self, ticket: &Ticket) -> Result<(), BoardError> {
        let store = self.inner.board.store();
        let queued = Status::Queued;
        let now = now_ms();
        let mut tx = store.begin_write().await?;
        let done = sqlx::query(concat!(
            "UPDATE tickets SET state = ?, start_problem_step = NULL, start_problem_message = NULL,
                                updated_at = ?, ",
            set_state_since!(),
            " WHERE id = ? AND state = ? AND start_problem_step IN (?, ?, ?)"
        ))
        .bind(queued.as_str())
        .bind(now)
        .bind(queued.as_str())
        .bind(now)
        .bind(&ticket.id)
        .bind(Status::Held.as_str())
        .bind(StartStep::ToolLimit.as_str())
        .bind(StartStep::ToolUnavailable.as_str())
        .bind(StartStep::ModelUnavailable.as_str())
        .execute(&mut *tx)
        .await?
        .rows_affected();
        if done == 0 {
            return Ok(());
        }
        record_event(
            &mut tx,
            &ticket.id,
            EventKind::Moved,
            &Actor::current_user(),
            json!({ "from": Status::Held, "to": queued }),
        )
        .await?;
        tx.commit().await?;
        self.ticket_changed(&ticket.id, &ticket.key);
        self.inner.events.publish(Event::CardStatusChanged {
            project: ticket.key.project.to_string(),
            id: ticket.id.clone(),
            key: ticket.key.clone(),
            status: queued,
        });
        Ok(())
    }

    /// A ticket's holds whose work went away (the ticket moved on): forgotten, nothing started.
    async fn forget_stale(&self, ticket_id: &str) -> Result<(), BoardError> {
        let store = self.inner.board.store();
        let holds = store.ticket_holds(ticket_id).await?;
        if holds.is_empty() {
            return Ok(());
        }
        let Ok(ticket) = self.inner.board.ticket_by_id(ticket_id).await else {
            return Ok(());
        };
        for hold in holds.iter().filter(|h| !still_waits(&ticket, h.role)) {
            store
                .clear_agent_hold(HoldScope::Ticket(ticket_id), hold.role)
                .await?;
            tracing::debug!(key = %ticket.key, role = %hold.role, "a hold whose work went away");
        }
        Ok(())
    }

    /// Lifts holds whenever their cause may have gone, until `stop` is set: a tool's state changed, a list was read,
    /// a limit came, reset or was resumed, an agent or a setting changed. Never on a timer (d0086). A ticket's
    /// change only forgets the holds whose work went away.
    pub async fn run(self, mut stop: watch::Receiver<bool>) {
        let mut tools = self.inner.connected.subscribe();
        let mut lists = self.inner.models.subscribe();
        let mut listener = self.inner.events.listen();
        enum Next {
            Lift,
            Ticket(String),
            Nothing,
        }
        loop {
            let next = tokio::select! {
                _ = stop.wait_for(|s| *s) => return,
                _ = self.inner.wake.notified() => Next::Lift,
                changed = tools.changed() => match changed { Ok(()) => Next::Lift, Err(_) => return },
                changed = lists.changed() => match changed { Ok(()) => Next::Lift, Err(_) => return },
                event = listener.receiver.recv() => match event {
                    Ok(Event::UsageChanged | Event::AgentsChanged | Event::SettingsChanged)
                    | Err(RecvError::Lagged(_)) => Next::Lift,
                    Ok(Event::TicketChanged { id, .. } | Event::CardStatusChanged { id, .. }) => Next::Ticket(id),
                    Ok(_) => Next::Nothing,
                    Err(RecvError::Closed) => return,
                },
            };
            match next {
                Next::Lift => {
                    if let Err(e) = self.lift().await {
                        tracing::warn!(error = %e, "can't lift the holds");
                    }
                }
                Next::Ticket(id) => {
                    if let Err(e) = self.forget_stale(&id).await {
                        tracing::warn!(ticket = id, error = %e, "can't check the ticket's holds");
                    }
                }
                Next::Nothing => {}
            }
        }
    }

    fn ticket_changed(&self, id: &str, key: &TicketKey) {
        self.inner.events.publish(Event::TicketChanged {
            project: key.project.to_string(),
            id: id.to_owned(),
            key: key.clone(),
        });
    }
}

/// Whether `stored` already says what `held` would write.
fn same_hold(stored: &AgentHold, held: &Held) -> bool {
    stored.reason == held.blocked.reason()
        && stored.tool == held.on.tool
        && stored.model == held.on.model
        && stored.agent_id.as_deref() == Some(held.agent.id.as_str())
        && stored.fallback_tool == held.fallback.as_ref().map(|_| held.on.tool)
        && stored.fallback_held == held.fallback_held
}

/// Whether the ticket still waits for its held `role`: an Agent stage's start held before it (Ready for Dev before
/// the first, STG-07), an agent check's round that opened without its session, an epic whose lead could still start.
pub fn still_waits(ticket: &Ticket, role: SessionRole) -> bool {
    match role {
        SessionRole::Developer => ticket.place.is_waiting(),
        SessionRole::Validator => matches!(
            ticket.status,
            Status::Checking | Status::WaitingForLimitReset
        ),
        SessionRole::EpicLead => !matches!(
            ticket.status.set(),
            StatusSet::HumanCheck | StatusSet::Delivery | StatusSet::Done
        ),
        SessionRole::ProjectAgent => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent_tools::models::{ModelEntry, ModelKind, ModelList};
    use crate::agents::{AgentModel, CameFrom};

    fn agent(tool: Tool) -> Agent {
        Agent {
            id: "dev".into(),
            name: "Developer".into(),
            instructions: String::new(),
            skills: Vec::new(),
            imported: Vec::new(),
            model: AgentModel::Tool {
                tool,
                model: Some("opus".into()),
            },
            effort: Some("high".into()),
            role: None,
            came_from: CameFrom::DefaultPreset,
            mark: None,
            created_at: 0,
            updated_at: 0,
        }
    }

    fn on(tool: Tool, model: Option<&str>) -> ModelRef {
        ModelRef {
            tool,
            model: model.map(str::to_owned),
        }
    }

    fn copy(tool: Tool, ids: &[&str]) -> ModelCopy {
        ModelCopy {
            tool,
            list: ModelList {
                entries: ids
                    .iter()
                    .map(|id| ModelEntry {
                        id: (*id).into(),
                        display_name: (*id).into(),
                        description: None,
                        resolves_to: None,
                        kind: ModelKind::Exact,
                        family: None,
                        released_at: None,
                    })
                    .collect(),
            },
            read_at: 0,
        }
    }

    #[test]
    fn the_model_passes_when_listed_empty_or_never_read() {
        let tool = Tool::ALL[1];
        let list = copy(tool, &["gpt-5.5"]);
        assert_eq!(passed_model(None, Some(&list)), Some(None));
        assert_eq!(passed_model(Some(" "), Some(&list)), Some(None));
        assert_eq!(
            passed_model(Some("gpt-5.5"), Some(&list)),
            Some(Some("gpt-5.5".into()))
        );
        assert_eq!(passed_model(Some("gpt-5.4"), Some(&list)), None);
        assert_eq!(
            passed_model(Some("gpt-5.4"), None),
            Some(Some("gpt-5.4".into()))
        );
        assert_eq!(passed_model(Some("newest:gpt-sol"), None), None);
        assert_eq!(passed_model(Some("newest:gpt-sol"), Some(&list)), None);
    }

    #[test]
    fn nothing_goes_to_the_fallback_without_the_users_click() {
        let [a, b] = [Tool::ALL[0], Tool::ALL[1]];
        let developer = agent(a);
        let own = on(a, Some("opus"));
        let fallbacks: Fallbacks = [(a, on(b, None))].into();
        let facts = Facts {
            at_limit: [a].into(),
            ..Facts::default()
        };
        // Not asked: held, offering the fallback.
        let Decision::Hold(held) = decide(&developer, &own, &fallbacks, &facts, false, true) else {
            panic!("held");
        };
        assert_eq!(
            (held.fallback.clone(), held.fallback_held),
            (Some(on(b, None)), false)
        );
        assert_eq!(
            held.message(),
            format!("{} limit reached", a.display_name())
        );
        // Asked: launched on the fallback, without the agent's effort.
        let Decision::Launch {
            on: there,
            fallback,
            ..
        } = decide(&developer, &own, &fallbacks, &facts, true, true)
        else {
            panic!("launched");
        };
        assert_eq!(there, on(b, None));
        assert_eq!(
            fallback,
            Some(FallbackFrom {
                tool: a,
                reason: HoldReason::ToolLimit
            })
        );
        let launched = runs_on(&developer, there, None, fallback);
        assert_eq!((launched.tool, launched.effort), (Some(b), None));
        // A Project agent's first message never takes it.
        assert!(matches!(
            decide(&developer, &own, &fallbacks, &facts, true, false),
            Decision::Hold(held) if held.fallback.is_none()
        ));
        // Its own model keeps its effort.
        let ok = runs_on(&developer, own.clone(), Some("opus".into()), None);
        assert_eq!(ok.effort.as_deref(), Some("high"));
    }

    #[test]
    fn a_fallback_that_cant_run_leaves_it_waiting() {
        let [a, b] = [Tool::ALL[0], Tool::ALL[1]];
        let developer = agent(a);
        let fallbacks: Fallbacks = [(a, on(b, None))].into();
        let facts = Facts {
            at_limit: [a, b].into(),
            ..Facts::default()
        };
        let Decision::Hold(held) = decide(&developer, &on(a, None), &fallbacks, &facts, true, true)
        else {
            panic!("held");
        };
        assert_eq!((held.fallback, held.fallback_held), (None, true));
    }
}
