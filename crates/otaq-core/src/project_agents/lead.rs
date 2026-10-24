//! The epic lead's session (LEAD-01, LEAD-02, LEAD-11, LEAD-12; d0083): one read-only agent per epic, stored as a
//! Project agent row with its epic set and run as a Project agent session with the epic lead role.
//!
//! - **Name:** worked out from the epic's title ("Wishlist lead"), never stored, so renaming the epic renames the
//!   lead. `otaq agent name` and the user's rename are refused for it.
//! - **Open lead** ([`EpicLeads::open`]) starts it with no first message; the user types in the Lead tab. An ended
//!   lead is resumed instead of starting a second one. Refused while it is live, while the epic is in Ready for
//!   Review, Approved or Done, and while the plan's usage limit holds.
//! - **Resume** ([`EpicLeads::resume`]) continues the same conversation (its tool's resume).
//! - **Queueing** a whole epic without a lead calls [`EpicLeads::start_for_queue`] (T-0279): the first message asks
//!   it to check the plan. An ended lead isn't restarted by a drag; the user resumes it.
//! - **Lifecycle** (LEAD-12, T-0279): it ends when the epic reaches Ready for Review (History "Lead ended · epic in
//!   review"); a child added or reopened after that resumes it with no message. A restart makes a running lead
//!   "ended by restart" ([`ProjectAgents::startup`]), resumed only by the user's click. Wake-ups and handovers are
//!   in [`crate::epic_lead::wake`].
//! - **Status** ([`LeadStatus`]): from the row, the session, its turn state and the reason for the current turn,
//!   which whoever types sets ([`EpicLeads::set_turn_reason`]; the user's Enter in its terminal is
//!   [`TurnReason::UserTyped`]). Each change publishes [`crate::protocol::Event::LeadChanged`].
//! - **While the project is paused** (PAU-04, PAU-05; d0112): Open lead and Resume are the user's click and start it;
//!   nothing else does. A queued epic's lead waits for the project's Resume ([`EpicLeads::start_kept`]), and an
//!   ended lead whose epic came back from review becomes Paused, so that Resume resumes it.
//! - **Tokens** ([`EpicLeads::lead_tokens`]) count on the epic and in today's total (d0020), never toward the agent
//!   limit (d0016).

use serde::{Deserialize, Serialize};

use std::sync::{Arc, Weak};

use super::{AgentState, Inner, ProjectAgent, ProjectAgents};
use crate::agent_tools::{EPIC_LEAD_SKILL, FirstPrompt, LaunchRequest, LaunchRole, Tool};
use crate::agents::{Role, SessionRole, resolve};
use crate::board::tickets::{EventKind, Ticket, record_event};
use crate::board::{BoardError, Refusal};
use crate::domain::{Actor, StatusSet, TicketKey};
use crate::epic_lead::wake::{Finished, Handovers, LeadEvents, closed_text, merged_text};
use crate::naming;
use crate::session::Start;
use crate::session::deliveries::Deliveries;
use crate::store::{new_id, now_ms};

/// Why no lead opens or resumes while the plan's usage limit holds (SCH-04).
pub const LEAD_LIMIT_REFUSAL: &str = "Usage limit reached · the lead opens once it resets";

/// Why `otaq agent name` and the user's rename are refused for a lead.
pub const NAMED_AFTER_EPIC: &str = "The lead is named after its epic";

/// Why the current turn of a lead runs, set by whoever typed into it (d0083).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TurnReason {
    /// The user typed to it: "re-planning", as a wake-up (the lead never plans an epic from scratch, d0091).
    UserTyped,
    /// The first turn after a drag started it: "checking the plan".
    CheckPlan,
    /// A wake-up (LEAD-04): "re-planning", or "answering SHOP-40.2" for a question.
    Wake { wake: Wake },
}

/// What woke a lead (LEAD-04): a child merged, was closed, got stuck, or asked.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Wake {
    Merge,
    Close,
    Stuck,
    /// The asking child.
    Question {
        ticket: TicketKey,
    },
}

/// What the lead is doing (LEAD-11): the words of *01 · Components · Epics on the board → Row · Lead Status*.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum LeadStatus {
    /// The epic has no lead: "Open lead".
    None,
    Idle,
    /// Its first turn after a drag started it.
    CheckingPlan,
    /// A turn the user started by typing, or a merge, close or stuck wake-up.
    Replanning,
    /// A question wake-up: "answering SHOP-40.2".
    Answering {
        ticket: TicketKey,
    },
    /// Its own `ask human` or a permission prompt (turquoise).
    AsksYou,
    /// "ended by restart · Resume".
    EndedByRestart,
    /// The user's Pause ended it (PAU-03, d0112): "paused · Resume", the same conversation.
    Paused,
    /// "ended · epic in review".
    EndedInReview,
    /// Its agent tool exited for another reason: "ended · Resume".
    Ended,
}

/// What a lead's status is worked out from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LeadFacts {
    /// The lead row's state; `None` when the epic has no lead.
    pub state: Option<AgentState>,
    /// Its session runs.
    pub live: bool,
    /// The epic's column.
    pub epic_column: StatusSet,
    /// It waits for the user: its own question or a permission prompt.
    pub waits_for_user: bool,
    /// Why its current turn runs; `None` between turns.
    pub turn: Option<TurnReason>,
}

/// The lead's status from its facts (LEAD-11).
pub fn lead_status(facts: &LeadFacts) -> LeadStatus {
    let Some(state) = facts.state else {
        return LeadStatus::None;
    };
    if !facts.live {
        let in_review = matches!(
            facts.epic_column,
            StatusSet::HumanCheck | StatusSet::Delivery | StatusSet::Done
        );
        return if in_review {
            LeadStatus::EndedInReview
        } else if state == AgentState::EndedByRestart {
            LeadStatus::EndedByRestart
        } else if state == AgentState::Paused {
            LeadStatus::Paused
        } else {
            LeadStatus::Ended
        };
    }
    if facts.waits_for_user {
        return LeadStatus::AsksYou;
    }
    match &facts.turn {
        None => LeadStatus::Idle,
        Some(TurnReason::CheckPlan) => LeadStatus::CheckingPlan,
        Some(
            TurnReason::UserTyped
            | TurnReason::Wake {
                wake: Wake::Merge | Wake::Close | Wake::Stuck,
            },
        ) => LeadStatus::Replanning,
        Some(TurnReason::Wake {
            wake: Wake::Question { ticket },
        }) => LeadStatus::Answering {
            ticket: ticket.clone(),
        },
    }
}

/// An epic's lead as clients see it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Lead {
    /// The epic's key and id.
    pub epic: TicketKey,
    pub epic_id: String,
    /// "Wishlist lead".
    pub name: String,
    pub status: LeadStatus,
    /// Its Project agent row: state, session, question, tokens.
    pub agent: ProjectAgent,
    /// What woke it for the current turn, the wake-up's text ("SHOP-40.1 merged: Wishlist API"); `None` between
    /// turns and in a turn nothing woke (the Lead Status tooltip, T-0289).
    #[serde(default)]
    pub woke_by: Option<String>,
    /// Its tokens today (d0020), for the Lead Status tooltip.
    #[serde(default)]
    pub tokens_today: crate::usage::Tokens,
}

/// "Wishlist lead": the lead's name, from its epic's title.
pub fn lead_name(epic_title: &str) -> String {
    format!("{} lead", epic_title.trim())
}

/// The first message of a lead a drag started (LEAD-02).
pub fn check_plan_message(epic: &TicketKey) -> String {
    format!(
        "{epic} was queued. Check the plan before the first child starts: `{} prime --epic`.",
        naming::COMMAND_NAME
    )
}

/// `agent name` and the user's rename: refused for a lead.
pub(super) fn refuse_naming(agent: &ProjectAgent) -> Result<(), BoardError> {
    if agent.epic.is_some() {
        return Err(BoardError::refused(Refusal::NotAllowed, NAMED_AFTER_EPIC));
    }
    Ok(())
}

/// A lead's turn starting or ending.
enum TurnEdge {
    Started,
    Ended,
}

/// A lead's turn: why the current one runs (while in one), and why the next one will.
#[derive(Debug, Clone, Default)]
pub(super) struct LeadTurn {
    current: Option<TurnReason>,
    next: Option<TurnReason>,
    /// The wake-up text of the current turn, and of the next one.
    current_woke: Option<String>,
    next_woke: Option<String>,
}

/// An epic's lead: open, resume, start for a queue, status, tokens. Cheap to clone.
#[derive(Clone)]
pub struct EpicLeads {
    agents: ProjectAgents,
}

impl ProjectAgents {
    /// The epic leads (d0083), which share the Project agents' rows and sessions.
    pub fn leads(&self) -> EpicLeads {
        EpicLeads {
            agents: self.clone(),
        }
    }

    /// A hook signal of the Project agent `id`: if it is a lead, a turn starts (with the reason set before it, else
    /// the user's) or ends. At a turn's end, the handovers typed in it that the lead didn't settle are handed back.
    /// Whether its status changed.
    pub(super) async fn lead_signal(&self, id: &str, signal: &crate::signals::Signal) -> bool {
        let Some(turn) = self.lead_turn_signal(id, signal) else {
            return false;
        };
        match turn {
            TurnEdge::Started => {
                let pending = match self.lead_session(id).await {
                    Some(session) => self
                        .inner
                        .deliveries
                        .pending(&session)
                        .await
                        .map(|p| p.into_iter().map(|d| d.text).collect())
                        .unwrap_or_default(),
                    None => Vec::new(),
                };
                self.inner.handovers.turn_started(id, &pending);
            }
            TurnEdge::Ended => {
                let back = self.inner.handovers.turn_ended(id);
                self.inner.handovers.hand_back(back).await;
            }
        }
        true
    }

    /// The turn edge a hook signal of the lead `id` makes, if any.
    fn lead_turn_signal(&self, id: &str, signal: &crate::signals::Signal) -> Option<TurnEdge> {
        if !self.is_lead(id) {
            return None;
        }
        let mut turns = self.inner.lead_turns.lock().expect("lead turns");
        let turn = turns.entry(id.to_owned()).or_default();
        if matches!(signal, crate::signals::Signal::Working { .. }) {
            if turn.current.is_none() {
                turn.current = Some(turn.next.take().unwrap_or(TurnReason::UserTyped));
                turn.current_woke = turn.next_woke.take();
                return Some(TurnEdge::Started);
            }
        } else if signal.ends_turn() && turn.current.take().is_some() {
            turn.current_woke = None;
            return Some(TurnEdge::Ended);
        }
        None
    }

    /// The session of the Project agent `id` ended (not by the service stopping): if it is a lead, every open
    /// handover is handed back.
    pub(super) async fn lead_session_ended(&self, id: &str) {
        let back = self.inner.handovers.lead_ended(id);
        self.inner.handovers.hand_back(back).await;
    }

    async fn lead_session(&self, id: &str) -> Option<String> {
        self.get(id).await.ok()?.session.map(|s| s.id)
    }

    /// What the board tells the leads (d0083), without keeping the Project agents alive.
    pub(super) fn lead_events(&self) -> Arc<dyn LeadEvents> {
        Arc::new(BoardLeadEvents(Arc::downgrade(&self.inner)))
    }

    /// Enter in a lead's terminal: the user typed, so its next turn is theirs.
    pub(super) fn lead_typed(&self, id: &str) {
        if !self.is_lead(id) {
            return;
        }
        let mut turns = self.inner.lead_turns.lock().expect("lead turns");
        let turn = turns.entry(id.to_owned()).or_default();
        if turn.current.is_none() {
            turn.next = Some(TurnReason::UserTyped);
            turn.next_woke = None;
        }
    }

    fn is_lead(&self, id: &str) -> bool {
        self.inner
            .lead_epics
            .lock()
            .expect("lead epics")
            .contains_key(id)
    }
}

impl EpicLeads {
    fn inner(&self) -> &super::Inner {
        &self.agents.inner
    }

    /// **Open lead** (LEAD-02): the user's click is the go-ahead. It starts the lead with no first message, or
    /// resumes an ended one. Refused when the epic has a live lead, is in Ready for Review / Approved / Done, when the
    /// check before the launch holds a new lead, or while the usage limit holds a resume.
    pub async fn open(&self, epic: &str) -> Result<Lead, BoardError> {
        let epic = self.epic(epic).await?;
        refuse_in_review(&epic)?;
        if let Some(lead) = self.get_for(&epic).await? {
            if is_live(&lead.agent) {
                return Err(BoardError::refused(
                    Refusal::AlreadyRunning,
                    format!("{} is already open", lead.name),
                ));
            }
            if !self.forget_if_not_started(&epic, &lead).await? {
                return self.resume_for(&epic, lead.agent, Start::User).await;
            }
        }
        self.launch_new(&epic, None, Start::User).await
    }

    /// **Resume** (LEAD-12): the same conversation, its tool's resume, the user's click; it clears "ended by
    /// restart". A live lead is returned as it is. Refused for an epic without a lead, in review, or while the
    /// usage limit holds.
    pub async fn resume(&self, epic: &str) -> Result<Lead, BoardError> {
        let epic = self.epic(epic).await?;
        refuse_in_review(&epic)?;
        let lead = self
            .get_for(&epic)
            .await?
            .ok_or_else(|| BoardError::NotFound(format!("{} has no lead to resume", epic.key)))?;
        if is_live(&lead.agent) {
            return Ok(lead);
        }
        if self.forget_if_not_started(&epic, &lead).await? {
            return self.launch_new(&epic, None, Start::User).await;
        }
        self.resume_for(&epic, lead.agent, Start::User).await
    }

    /// A lead whose first turn the tool refused on the model never started (AGT-05, Q-C): there is no conversation
    /// to resume, so it is forgotten and opens afresh, on what its Epic lead row says now. Whether it was.
    async fn forget_if_not_started(&self, epic: &Ticket, lead: &Lead) -> Result<bool, BoardError> {
        if self.inner().board.store().lead_started(&epic.id).await? {
            return Ok(false);
        }
        sqlx::query("DELETE FROM project_agents WHERE id = ?")
            .bind(&lead.agent.id)
            .execute(self.inner().board.store().pool())
            .await?;
        self.inner()
            .lead_turns
            .lock()
            .expect("lead turns")
            .remove(&lead.agent.id);
        tracing::info!(epic = %epic.key, agent = lead.agent.id, "a lead that never started is opened afresh");
        Ok(true)
    }

    /// Starts the lead of a queued epic (LEAD-02, called when the epic is queued; T-0279): the first message asks
    /// it to check the plan, and its status is "checking the plan". Refused when the epic already has a lead, and
    /// while its project is paused (PAU-04).
    pub async fn start_for_queue(&self, epic: &str) -> Result<Lead, BoardError> {
        let epic = self.epic(epic).await?;
        if let Some(lead) = self.get_for(&epic).await? {
            return Err(BoardError::refused(
                Refusal::AlreadyRunning,
                format!("{} already has {}", epic.key, lead.name),
            ));
        }
        let message = check_plan_message(&epic.key);
        self.launch_new(&epic, Some((&message, TurnReason::CheckPlan)), Start::Auto)
            .await
    }

    /// An epic lead's hold lifted (T-0357): the lead starts, checked once more; a queued epic's lead checks the plan
    /// first, as the drag asked for.
    pub(crate) async fn start_held(&self, epic: &str) -> Result<Lead, BoardError> {
        let epic = self.epic(epic).await?;
        if let Some(lead) = self.get_for(&epic).await? {
            return Ok(lead);
        }
        if matches!(
            epic.board_group(),
            StatusSet::Waiting | StatusSet::AgentStage
        ) {
            let message = check_plan_message(&epic.key);
            self.launch_new(&epic, Some((&message, TurnReason::CheckPlan)), Start::Auto)
                .await
        } else {
            self.launch_new(&epic, None, Start::Auto).await
        }
    }

    /// The epic's lead, if it has one.
    pub async fn get(&self, epic: &str) -> Result<Option<Lead>, BoardError> {
        let epic = self.epic(epic).await?;
        self.get_for(&epic).await
    }

    /// The epic's Lead Status (LEAD-11): [`LeadStatus::None`] when it has no lead.
    pub async fn status(&self, epic: &str) -> Result<LeadStatus, BoardError> {
        Ok(self
            .get(epic)
            .await?
            .map_or(LeadStatus::None, |lead| lead.status))
    }

    /// Sets why the lead's next turn runs (or the current one, when `reason` is typed into a turn it starts now):
    /// a wake-up sets [`TurnReason::Wake`] before its message is typed. Nothing happens for an epic without a lead.
    pub async fn set_turn_reason(&self, epic: &str, reason: TurnReason) -> Result<(), BoardError> {
        let Some(lead) = self.get(epic).await? else {
            return Ok(());
        };
        {
            let mut turns = self.inner().lead_turns.lock().expect("lead turns");
            let turn = turns.entry(lead.agent.id.clone()).or_default();
            turn.next = Some(reason);
            turn.next_woke = None;
        }
        self.agents.changed(&lead.agent.project, &lead.agent.id);
        Ok(())
    }

    /// The wake-up text the lead `agent`'s next turn is for (set right after its reason, by the wake-up).
    pub(crate) fn set_next_woke(&self, agent: &str, text: &str) {
        let mut turns = self.inner().lead_turns.lock().expect("lead turns");
        turns.entry(agent.to_owned()).or_default().next_woke = Some(text.to_owned());
    }

    /// Every lead of `project` (d0083), in epic key order: what the board shows on epic cards and track labels.
    pub async fn list(&self, project: &str) -> Result<Vec<Lead>, BoardError> {
        let project = self.inner().board.project(project).await?;
        let epics: Vec<(String,)> = sqlx::query_as(
            "SELECT a.epic_id FROM project_agents a WHERE a.project_id = ? AND a.epic_id IS NOT NULL",
        )
        .bind(&project.id)
        .fetch_all(self.inner().board.store().pool())
        .await?;
        let mut leads = Vec::new();
        for (epic_id,) in epics {
            let epic = self.inner().board.ticket_by_id(&epic_id).await?;
            if let Some(lead) = self.get_for(&epic).await? {
                leads.push(lead);
            }
        }
        leads.sort_by(|a, b| a.epic.cmp(&b.epic));
        Ok(leads)
    }

    /// The lead's tokens, every session and day (LEAD-12): zero for an epic without a lead.
    pub async fn lead_tokens(&self, epic: &str) -> Result<crate::usage::Tokens, BoardError> {
        Ok(self
            .get(epic)
            .await?
            .map(|lead| lead.agent.tokens)
            .unwrap_or_default())
    }

    /// The epic's lead when it is live (its session runs).
    pub async fn live_lead(&self, epic: &str) -> Result<Option<Lead>, BoardError> {
        Ok(self.get(epic).await?.filter(|lead| is_live(&lead.agent)))
    }

    pub(crate) fn handovers(&self) -> &Handovers {
        &self.inner().handovers
    }

    pub(crate) fn deliveries(&self) -> &Deliveries {
        &self.inner().deliveries
    }

    /// The tool a session with no tool recorded runs on.
    pub(crate) fn default_tool(&self) -> Tool {
        self.inner().adapters.for_new_session().tool()
    }

    /// Whether `tool`'s usage limit holds now (the lead's own tool, USG-10).
    pub(crate) async fn limit_held_now(&self, tool: Option<Tool>) -> Result<bool, BoardError> {
        self.agents.limit_held_on(tool).await
    }

    /// Every live lead's session, with its tool.
    /// Every live lead: its Project agent id, its epic, and when its session started (d0124).
    pub(crate) async fn live_leads(&self) -> Result<Vec<(String, TicketKey, i64)>, BoardError> {
        let rows: Vec<(String, String)> = sqlx::query_as(
            "SELECT id, epic_id FROM project_agents WHERE epic_id IS NOT NULL AND state = ?",
        )
        .bind(AgentState::Running.as_str())
        .fetch_all(self.inner().board.store().pool())
        .await?;
        let mut live = Vec::new();
        for (id, epic_id) in rows {
            let agent = self.agents.get(&id).await?;
            if is_live(&agent)
                && let Some(session) = agent.session
            {
                let epic = self.inner().board.ticket_by_id(&epic_id).await?;
                live.push((id, epic.key, session.started_at));
            }
        }
        Ok(live)
    }

    /// Publishes that the lead changed, so the board and the scheduler look again.
    pub(crate) fn lead_changed(&self, lead: &Lead) {
        self.agents.changed(&lead.agent.project, &lead.agent.id);
    }

    pub(crate) async fn live_sessions(&self) -> Result<Vec<(String, Option<Tool>)>, BoardError> {
        let ids: Vec<(String,)> =
            sqlx::query_as("SELECT id FROM project_agents WHERE epic_id IS NOT NULL AND state = ?")
                .bind(AgentState::Running.as_str())
                .fetch_all(self.inner().board.store().pool())
                .await?;
        let mut sessions = Vec::new();
        for (id,) in ids {
            let agent = self.agents.get(&id).await?;
            if is_live(&agent)
                && let Some(session) = agent.session
            {
                sessions.push((session.id, session.tool));
            }
        }
        Ok(sessions)
    }

    /// A whole epic was queued (LEAD-02): an epic with no lead at all gets one that checks the plan; an ended
    /// lead isn't restarted (the user resumes it). While its project is paused the start is kept: the project's
    /// Resume starts it ([`Self::start_kept`]). Whether one started.
    pub async fn start_if_none(&self, epic: &str) -> Result<bool, BoardError> {
        let ticket = self.epic(epic).await?;
        if self.get_for(&ticket).await?.is_some() {
            return Ok(false);
        }
        if crate::agents::pause::is_paused(self.inner().board.store(), ticket.key.project.as_str())
            .await?
        {
            tracing::info!(epic = %ticket.key, "lead start kept: the project is paused");
            return Ok(false);
        }
        self.start_for_queue(epic).await?;
        Ok(true)
    }

    /// The project resumed (PAU-06): the leads kept while it was paused start, checking the plan: each whole epic
    /// queued (and not in review) that has no lead and no held start (the launch gate's, which its lift runs). The
    /// epics whose lead started.
    pub async fn start_kept(&self, project: &str) -> Result<Vec<TicketKey>, BoardError> {
        let board = &self.inner().board;
        let mut started = Vec::new();
        for epic in board.tickets(project).await? {
            if !epic.is_epic()
                || epic.epic_queue_mode != crate::board::epic_flight::EpicQueueMode::Whole
                || refuse_in_review(&epic).is_err()
                || board
                    .store()
                    .agent_hold(
                        crate::agents::store::HoldScope::Ticket(&epic.id),
                        SessionRole::EpicLead,
                    )
                    .await?
                    .is_some()
            {
                continue;
            }
            match self.start_if_none(&epic.key.to_string()).await {
                Ok(true) => started.push(epic.key),
                Ok(false) => {}
                Err(e) => {
                    tracing::warn!(epic = %epic.key, error = %e, "the kept lead didn't start")
                }
            }
        }
        Ok(started)
    }

    /// The epic reached Ready for Review (LEAD-12): its running lead ends normally, `state = ended`, History "Lead
    /// ended · epic in review" on the epic, and whatever it was sent is handed back. Its status reads "ended ·
    /// epic in review".
    pub async fn end_in_review(&self, epic: &str, actor: &Actor) -> Result<(), BoardError> {
        let epic = self.epic(epic).await?;
        let Some(lead) = self.get_for(&epic).await? else {
            return Ok(());
        };
        let mut tx = self.inner().board.store().begin_write().await?;
        let ended = sqlx::query(
            "UPDATE project_agents SET state = ?, ended_at = ? WHERE id = ? AND state = ?",
        )
        .bind(AgentState::Ended.as_str())
        .bind(now_ms())
        .bind(&lead.agent.id)
        .bind(AgentState::Running.as_str())
        .execute(&mut *tx)
        .await?
        .rows_affected();
        if ended == 0 {
            return Ok(());
        }
        record_event(
            &mut tx,
            &epic.id,
            EventKind::LeadEnded,
            actor,
            serde_json::json!({ "lead": lead.name }),
        )
        .await?;
        tx.commit().await?;
        self.inner()
            .lead_turns
            .lock()
            .expect("lead turns")
            .remove(&lead.agent.id);
        let back = self.handovers().lead_ended(&lead.agent.id);
        self.handovers().hand_back(back).await;
        if let Some(session) = lead.agent.session.as_ref().filter(|s| s.is_running())
            && let Err(e) = self.inner().sessions.stop(&session.id).await
        {
            tracing::warn!(epic = %epic.key, error = %e, "can't stop the lead's session");
        }
        tracing::info!(epic = %epic.key, agent = lead.agent.id, "epic lead ended: epic in review");
        self.agents.changed(&lead.agent.project, &lead.agent.id);
        Ok(())
    }

    /// A child was added to, or reopened in, an epic that had reached Review (LEAD-12): its lead, ended there,
    /// resumes the same conversation with no message and is idle until a wake-up. "Ended by restart" waits for the
    /// user's click. Whether it resumed.
    pub async fn resume_quietly(&self, epic: &str) -> Result<bool, BoardError> {
        let epic = self.epic(epic).await?;
        let Some(lead) = self.get_for(&epic).await? else {
            return Ok(false);
        };
        if lead.agent.state != AgentState::Ended || is_live(&lead.agent) {
            return Ok(false);
        }
        refuse_in_review(&epic)?;
        // Kept as the pause leaves a lead (PAU-04): Paused, which the project's Resume resumes.
        if crate::agents::pause::is_paused(self.inner().board.store(), epic.key.project.as_str())
            .await?
        {
            self.agents
                .set_state(&lead.agent.id, AgentState::Paused)
                .await?;
            self.agents.changed(&lead.agent.project, &lead.agent.id);
            tracing::info!(epic = %epic.key, "lead resume kept: the project is paused");
            return Ok(false);
        }
        self.resume_for(&epic, lead.agent, Start::Auto).await?;
        Ok(true)
    }

    async fn epic(&self, key: &str) -> Result<Ticket, BoardError> {
        let epic = self.inner().board.ticket(key).await?;
        if !epic.is_epic() {
            return Err(BoardError::refused(
                Refusal::NotAnEpic,
                format!("{} isn't an epic", epic.key),
            ));
        }
        Ok(epic)
    }

    async fn get_for(&self, epic: &Ticket) -> Result<Option<Lead>, BoardError> {
        let id: Option<(String,)> =
            sqlx::query_as("SELECT id FROM project_agents WHERE epic_id = ?")
                .bind(&epic.id)
                .fetch_optional(self.inner().board.store().pool())
                .await?;
        let Some((id,)) = id else {
            return Ok(None);
        };
        let agent = self.agents.get(&id).await?;
        let (turn, woke_by) = self
            .inner()
            .lead_turns
            .lock()
            .expect("lead turns")
            .get(&id)
            .map(|t| (t.current.clone(), t.current_woke.clone()))
            .unwrap_or_default();
        let today: (Option<i64>, Option<i64>, Option<i64>, Option<i64>) = sqlx::query_as(
            "SELECT SUM(u.input), SUM(u.cached), SUM(u.output), SUM(u.reasoning)
             FROM usage u JOIN sessions s ON s.id = u.session_id WHERE s.project_agent_id = ? AND u.day = ?",
        )
        .bind(&id)
        .bind(crate::usage::local_day(now_ms()))
        .fetch_one(self.inner().board.store().pool())
        .await?;
        let turn_is_wake = matches!(turn, Some(TurnReason::Wake { .. }));
        let status = lead_status(&LeadFacts {
            state: Some(agent.state),
            live: is_live(&agent),
            epic_column: epic.board_group(),
            waits_for_user: agent.question.is_some() || agent.needs_you.is_some(),
            turn,
        });
        Ok(Some(Lead {
            epic: epic.key.clone(),
            epic_id: epic.id.clone(),
            name: agent.name.clone(),
            status,
            agent,
            woke_by: woke_by.filter(|_| turn_is_wake),
            tokens_today: crate::usage::tokens(today),
        }))
    }

    /// Starts a new lead for `epic`, with `first` (its first message, and why that turn runs) if any.
    async fn launch_new(
        &self,
        epic: &Ticket,
        first: Option<(&str, TurnReason)>,
        start: Start,
    ) -> Result<Lead, BoardError> {
        if start == Start::Auto {
            crate::agents::pause::refuse_while_paused(
                self.inner().board.store(),
                epic.key.project.as_str(),
            )
            .await?;
        }
        let (message, reason) = first.unzip();
        let agents = &self.agents;
        // The agent with the Epic lead role (LEAD-01, AGM-05), after the check before every launch (a hold keeps the
        // epic waiting).
        let agent = resolve::role_agent(self.inner().board.store(), Role::EpicLead).await?;
        let runs_on = agents
            .checked(
                SessionRole::EpicLead,
                crate::agents::gate::Scope::ticket(epic),
                &agent,
            )
            .await?;
        agents.inner.adapters.unavailable(runs_on.tool)?;
        let project = self
            .inner()
            .board
            .project(&epic.key.project.to_string())
            .await?;
        let id = new_id();
        // The agent's skills, copied into the project's folder while it runs (AGM-07).
        let first_prompt = agents
            .copy_into_checkout(
                &id,
                std::path::Path::new(&project.repo_path),
                Some(&agent),
                Some(FirstPrompt::Skill {
                    name: EPIC_LEAD_SKILL.to_owned(),
                    message: message.map(str::to_owned),
                }),
            )
            .await;
        let launched = match agents.launch(
            runs_on.tool,
            LaunchRequest {
                epic: Some(epic.key.clone()),
                model: runs_on.model.clone(),
                effort: runs_on.effort.clone(),
                title: Some(epic.title.clone()),
                ..super::request(
                    &self.inner().board,
                    &project,
                    LaunchRole::EpicLead,
                    first_prompt,
                )
                .await
            },
        ) {
            Ok(launched) => launched,
            Err(e) => {
                agents.release_checkout(&id).await;
                return Err(e);
            }
        };
        let pool = self.inner().board.store().pool();
        let inserted = sqlx::query(
            "INSERT INTO project_agents (id, project_id, tool, tool_session_id, model, agent_id, agent_name, effort,
                                         state, created_at, epic_id)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(&project.id)
        .bind(launched.tool.as_str())
        .bind(&launched.tool_session_id)
        .bind(&launched.model)
        .bind(runs_on.agent.as_ref().map(|a| &a.id))
        .bind(runs_on.agent.as_ref().map(|a| &a.name))
        .bind(&runs_on.effort)
        .bind(AgentState::Running.as_str())
        .bind(now_ms())
        .bind(&epic.id)
        .execute(pool)
        .await;
        if let Err(e) = inserted {
            agents.release_checkout(&id).await;
            // Another click opened it first: one lead per epic.
            tracing::info!(epic = %epic.key, error = %e, "the epic's lead wasn't added");
            return Err(BoardError::refused(
                Refusal::AlreadyRunning,
                format!("{} already has a lead", epic.key),
            ));
        }
        // Known as a lead, with its first turn's reason, before its first hook can arrive.
        self.inner()
            .lead_epics
            .lock()
            .expect("lead epics")
            .insert(id.clone(), (project.key.clone(), epic.key.clone()));
        self.inner().lead_turns.lock().expect("lead turns").insert(
            id.clone(),
            LeadTurn {
                current: reason,
                ..LeadTurn::default()
            },
        );
        if let Err(e) = agents
            .spawn(
                &project,
                launched,
                &id,
                super::INITIAL_SIZE,
                &runs_on,
                Some((&epic.id, &epic.key)),
                start,
            )
            .await
        {
            agents.release_checkout(&id).await;
            sqlx::query("DELETE FROM project_agents WHERE id = ?")
                .bind(&id)
                .execute(pool)
                .await?;
            self.inner()
                .lead_turns
                .lock()
                .expect("lead turns")
                .remove(&id);
            return Err(e);
        }
        tracing::info!(epic = %epic.key, agent = id, "epic lead started");
        agents.changed(&project.key, &id);
        self.get_for(epic)
            .await?
            .ok_or_else(|| BoardError::NotFound(format!("{}'s lead is gone", epic.key)))
    }

    async fn resume_for(
        &self,
        epic: &Ticket,
        agent: ProjectAgent,
        start: Start,
    ) -> Result<Lead, BoardError> {
        let agents = &self.agents;
        // An ended lead's Resume continues on its tool, model and named agent (d0086), unless that tool is at its
        // limit (USG-10).
        let runs_on = agents.runs_on(&agent).await?;
        self.refuse_during_limit(runs_on.tool).await?;
        agents.inner.adapters.unavailable(runs_on.tool)?;
        let project = self.inner().board.project(&agent.project).await?;
        let conversation = agent.conversation()?.to_owned();
        agents
            .copy_stored_agent_into_checkout(&agent.id, &project, &runs_on)
            .await?;
        let launched = match agents.launch(
            runs_on.tool,
            LaunchRequest {
                epic: Some(epic.key.clone()),
                resume: Some(conversation),
                model: runs_on.model.clone(),
                effort: runs_on.effort.clone(),
                title: Some(epic.title.clone()),
                ..super::request(
                    &self.inner().board,
                    &project,
                    LaunchRole::EpicLead,
                    super::carry_on(agent.state),
                )
                .await
            },
        ) {
            Ok(launched) => launched,
            Err(e) => {
                agents.release_checkout(&agent.id).await;
                return Err(e);
            }
        };
        self.inner()
            .lead_turns
            .lock()
            .expect("lead turns")
            .remove(&agent.id);
        if let Err(e) = agents
            .restart(
                &agent,
                &project,
                launched,
                &runs_on,
                Some((&epic.id, &epic.key)),
                start,
            )
            .await
        {
            agents.release_checkout(&agent.id).await;
            return Err(e);
        }
        tracing::info!(epic = %epic.key, agent = agent.id, "epic lead resumed");
        agents.changed(&agent.project, &agent.id);
        self.get_for(epic)
            .await?
            .ok_or_else(|| BoardError::NotFound(format!("{}'s lead is gone", epic.key)))
    }

    async fn refuse_during_limit(&self, tool: Option<Tool>) -> Result<(), BoardError> {
        if self.agents.limit_held_on(tool).await? {
            return Err(BoardError::refused(
                Refusal::MoveRefused,
                LEAD_LIMIT_REFUSAL,
            ));
        }
        Ok(())
    }
}

/// A lead whose session runs.
fn is_live(agent: &ProjectAgent) -> bool {
    agent.state == AgentState::Running
        && agent
            .session
            .as_ref()
            .is_some_and(crate::session::SessionInfo::is_running)
}

/// "SHOP-40 is in review; its lead has ended": no lead opens for an epic in Ready for Review, Approved or Done.
fn refuse_in_review(epic: &Ticket) -> Result<(), BoardError> {
    if matches!(
        epic.board_group(),
        StatusSet::HumanCheck | StatusSet::Delivery | StatusSet::Done
    ) {
        return Err(BoardError::refused(
            Refusal::MoveRefused,
            format!("{} is in review; its lead has ended", epic.key),
        ));
    }
    Ok(())
}

/// The board's side of the leads (d0083): each call logs its own failure, so a lead never fails a board change.
struct BoardLeadEvents(Weak<Inner>);

impl BoardLeadEvents {
    fn leads(&self) -> Option<EpicLeads> {
        self.0
            .upgrade()
            .map(|inner| ProjectAgents { inner }.leads())
    }
}

type Fut<'a, T> = std::pin::Pin<Box<dyn std::future::Future<Output = T> + Send + 'a>>;

impl LeadEvents for BoardLeadEvents {
    fn epic_queued<'a>(&'a self, epic: &'a TicketKey) -> Fut<'a, bool> {
        Box::pin(async move {
            let Some(leads) = self.leads() else {
                return false;
            };
            leads
                .start_if_none(&epic.to_string())
                .await
                .inspect_err(|e| tracing::warn!(%epic, error = %e, "the epic's lead didn't start"))
                .unwrap_or(false)
        })
    }

    fn child_finished<'a>(&'a self, child: &'a Ticket, how: Finished) -> Fut<'a, ()> {
        Box::pin(async move {
            let (Some(leads), Some(epic)) = (self.leads(), &child.epic) else {
                return;
            };
            let (wake, text) = match how {
                Finished::Merged => (Wake::Merge, merged_text(&child.key, &child.title)),
                Finished::Closed => (Wake::Close, closed_text(&child.key, &child.title)),
            };
            // The siblings that waited for it wait for the lead's turn about it too (d0124).
            let held = crate::scheduler::held_back_by(&leads.inner().board, child)
                .await
                .inspect_err(|e| tracing::warn!(child = %child.key, error = %e, "can't read the waiting siblings"))
                .unwrap_or_default();
            if let Err(e) = leads
                .wake_for_finished(&epic.to_string(), wake, &text, child, held)
                .await
            {
                tracing::warn!(%epic, child = %child.key, error = %e, "can't wake the epic's lead");
            }
        })
    }

    fn lead_waits(&self) -> Fut<'_, Vec<crate::epic_lead::wake::LeadWait>> {
        Box::pin(async move {
            let Some(leads) = self.leads() else {
                return Vec::new();
            };
            leads
                .lead_waits()
                .await
                .inspect_err(|e| tracing::warn!(error = %e, "can't read what waits for the leads"))
                .unwrap_or_default()
        })
    }

    fn epic_in_review<'a>(&'a self, epic: &'a TicketKey, actor: &'a Actor) -> Fut<'a, ()> {
        Box::pin(async move {
            let Some(leads) = self.leads() else {
                return;
            };
            if let Err(e) = leads.end_in_review(&epic.to_string(), actor).await {
                tracing::warn!(%epic, error = %e, "can't end the epic's lead");
            }
        })
    }

    fn epic_back<'a>(&'a self, epic: &'a TicketKey) -> Fut<'a, ()> {
        Box::pin(async move {
            let Some(leads) = self.leads() else {
                return;
            };
            if let Err(e) = leads.resume_quietly(&epic.to_string()).await {
                tracing::warn!(%epic, error = %e, "the epic's lead didn't resume");
            }
        })
    }

    fn takes_stop<'a>(&'a self, child: &'a Ticket) -> Fut<'a, bool> {
        Box::pin(async move {
            let Some(leads) = self.leads() else {
                return false;
            };
            crate::epic_lead::stuck::takes_stop(&leads.inner().board, &leads, child)
                .await
                .inspect_err(|e| tracing::warn!(child = %child.key, error = %e, "can't check the child's lead"))
                .unwrap_or(false)
        })
    }

    fn child_stuck<'a>(
        &'a self,
        child: &'a Ticket,
        stop: &'a crate::epic_lead::stuck::FirstStop,
    ) -> Fut<'a, bool> {
        Box::pin(async move {
            let Some(leads) = self.leads() else {
                return false;
            };
            crate::epic_lead::stuck::hold(&leads.inner().board, &leads, child, stop)
                .await
                .inspect_err(|e| tracing::warn!(child = %child.key, error = %e, "the stop didn't reach the lead"))
                .unwrap_or(false)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(k: &str) -> TicketKey {
        k.parse().unwrap()
    }

    fn live(turn: Option<TurnReason>) -> LeadFacts {
        LeadFacts {
            state: Some(AgentState::Running),
            live: true,
            epic_column: StatusSet::AgentStage,
            waits_for_user: false,
            turn,
        }
    }

    #[test]
    fn every_status_from_its_inputs() {
        let none = LeadFacts {
            state: None,
            live: false,
            epic_column: StatusSet::Backlog,
            waits_for_user: false,
            turn: None,
        };
        assert_eq!(lead_status(&none), LeadStatus::None);
        assert_eq!(lead_status(&live(None)), LeadStatus::Idle);
        assert_eq!(
            lead_status(&live(Some(TurnReason::CheckPlan))),
            LeadStatus::CheckingPlan
        );
        assert_eq!(
            lead_status(&live(Some(TurnReason::UserTyped))),
            LeadStatus::Replanning,
            "a turn the user typed reads re-planning (d0091)"
        );
        for wake in [Wake::Merge, Wake::Close, Wake::Stuck] {
            assert_eq!(
                lead_status(&live(Some(TurnReason::Wake { wake }))),
                LeadStatus::Replanning
            );
        }
        assert_eq!(
            lead_status(&live(Some(TurnReason::Wake {
                wake: Wake::Question {
                    ticket: key("SHOP-40.2")
                }
            }))),
            LeadStatus::Answering {
                ticket: key("SHOP-40.2")
            }
        );
        // Its own question or permission prompt wins over the turn.
        let asks = LeadFacts {
            waits_for_user: true,
            ..live(Some(TurnReason::UserTyped))
        };
        assert_eq!(lead_status(&asks), LeadStatus::AsksYou);

        let ended = |state, column| LeadFacts {
            state: Some(state),
            live: false,
            epic_column: column,
            waits_for_user: false,
            turn: None,
        };
        assert_eq!(
            lead_status(&ended(AgentState::EndedByRestart, StatusSet::AgentStage)),
            LeadStatus::EndedByRestart
        );
        assert_eq!(
            lead_status(&ended(AgentState::Ended, StatusSet::AgentStage)),
            LeadStatus::Ended
        );
        for column in [StatusSet::HumanCheck, StatusSet::Delivery, StatusSet::Done] {
            assert_eq!(
                lead_status(&ended(AgentState::Ended, column)),
                LeadStatus::EndedInReview
            );
        }
        // A row still running whose agent tool exited isn't live.
        let exited = LeadFacts {
            live: false,
            ..live(Some(TurnReason::UserTyped))
        };
        assert_eq!(lead_status(&exited), LeadStatus::Ended);
    }

    #[test]
    fn the_lead_is_named_after_its_epic_and_the_queue_message_names_prime() {
        assert_eq!(lead_name("Wishlist"), "Wishlist lead");
        assert_eq!(lead_name(" Search "), "Search lead");
        assert_eq!(
            check_plan_message(&key("SHOP-40")),
            format!(
                "SHOP-40 was queued. Check the plan before the first child starts: `{} prime --epic`.",
                naming::COMMAND_NAME
            )
        );
    }
}
