//! The start sequence (SES-01, SES-02; d0018, d0007, d0059), in order:
//!
//! 1. resolve the base (the epic's branch for an epic child);
//! 2. sync the base: fetch, fast-forward if only behind, **hold** in Queued if diverged (skipped for an epic
//!    branch that exists; the first child of an epic syncs the epic's base before creating the epic branch);
//! 3. branch `otaq/<KEY>`, worktree and `base_sha`; a new worktree is then seeded from the project's warm build
//!    or the user's checkout ([`crate::warm::seed`], WRM-04), as part of the same step;
//! 4. env files; then the ticket's resources (the project's Create command, d0087); 5. setup command; 6. two ports;
//! 7. launch the agent through its tool's adapter, store the tool and tool id, and set the ticket to Agent
//!    Working.
//!
//! Every step reuses what exists, so the whole sequence can run again (SES-02). A step that fails leaves the
//! ticket in Queued · **Start interrupted** with the step and its message; nothing is rolled back or deleted.
//! Only **Retry start** (a user's click, [`Starter::retry`]) runs it again; no code path re-runs a failed start
//! by itself (P2, d0018). The step a running start is at is stored too, so a start that the service stopping
//! cut short becomes Start interrupted at the next service start ([`Starter::interrupt_cut_short`]) rather
//! than being picked up again by the scheduler.
//!
//! **Returning to Dev** (Send back, a Delivery's conflict, a parked ticket dragged back) goes through the same
//! sequence, but the ticket already has its worktree: base sync, env files and setup are skipped while the
//! worktree is there, and step 7 continues the stored conversation (its tool's resume) instead of starting
//! a new one, with the skill still to run (`resume_skill`, followed by what came back in `resume_prompt`: "SHOP-18 was
//! sent back to Dev with a comment. Run `otaq prime`.", CMT-05; or an inbox answer in `resume_prompt`) as its first
//! prompt; both stay stored until the agent's first hook or command shows the conversation got them. If the agent's session is still
//! live, it is reused and the skill waits as an automatic delivery for an empty input line (d0048). A returning
//! ticket whose build was freed at Review gets it back before its ports and agent ([`TicketEnv::fetch_build`],
//! M19-c); a setup that fails there is Start interrupted at the setup step, like a new worktree's.
//!
//! **The next Agent stage** (STG-07, d0108): a ticket that finished one Agent stage waits before the next one as a
//! queued ticket does (dependencies, the limit, the holds), and the same sequence starts it there: it keeps its
//! worktree (no base sync, env files or setup), the previous stage's session is stopped, and a new conversation
//! begins, checked before its launch like any new one. The ticket enters the stage it waited before
//! ([`crate::flow::engine::Move::Begin`]).
//!
//! **While the project is paused** (PAU-04, d0112) an automatic start ([`Start::Auto`]: the scheduler, a drag, a
//! conflict's `restack`) does nothing: the ticket stays waiting where it is ([`StartOutcome::Kept`]) and the scheduler
//! starts it once the user resumes the project. Only the user's click on this one ticket ([`Start::User`]: its Resume,
//! an answer) starts it while paused, and the pause stays.
//!
//! **A model the tool refused** on the Developer's first turn (AGT-05) sends the ticket back the same way, as Start
//! interrupted at the `model` step (Start failed), counted as not started ([`Starter::model_refused`]).
//!
//! **The check before every launch** (SCH-04 `[M23]`, AGT-08; [`crate::agents::gate`]) runs first for a new
//! conversation, before any step, on the agent of the Agent stage it starts (AGM-05): a tool that isn't available or
//! is at its limit, or a model no longer listed, holds the ticket in Ready for Dev (Held, its step naming the reason;
//! nothing is created and nothing locks), or it starts on its tool's fallback when the user asked for it (AGM-03). A
//! resumed conversation isn't checked. A launch that fails because of its tool has the tool checked again, then the
//! check decides once more (ADP-10).

use std::collections::HashSet;
use std::path::Path;
use std::sync::{Arc, Mutex, OnceLock};

use serde_json::{Value, json};

use super::{StartError, begin_setup_log, copy_env_files, prepare_worktree, run_setup};
use crate::agent_tools::{Adapters, FirstPrompt, LaunchRequest, LaunchRole, WORK_TICKET_SKILL};
use crate::agents::gate::{Gated, Held, LaunchGate, Scope};
use crate::agents::{Agent, RunsOn, SessionRole, resolve};
use crate::board::stage_runs::{Moved, end_open};
use crate::board::tickets::{Base, EventKind, Ticket, record_event};
use crate::board::{Board, BoardError, Refusal};
use crate::data_dir::DataDir;
use crate::domain::{Actor, BuildState, Place, StartStep, Status};
use crate::flow::engine::{Move, RunResult, first_stage};
use crate::git::BaseSync;
use crate::naming;
use crate::ports::PortPool;
use crate::protocol::Event;
use crate::recovery::Recovery;
use crate::service::EventBus;
use crate::session::deliveries::{Deliveries, DeliveryKind};
use crate::session::env::applied;
use crate::session::{
    END_MODEL_REFUSED, SessionInfo, SessionKind, SessionTicket, Sessions, SpawnSpec, Start,
    StoredConversation, TerminalSize,
};
use crate::store::now_ms;
use crate::store::tickets::set_state_since;
use crate::ticket_env::TicketEnv;
use crate::ticket_env::fetch::FetchWhy;
use crate::ticket_env::resources::{Resources, command_vars};
use crate::tools::Os;
use crate::warm::seed::Seeder;

/// The agent's terminal size until a client attaches and resizes it.
const INITIAL_SIZE: TerminalSize = TerminalSize {
    cols: 120,
    rows: 36,
};

/// The statuses a start can begin from: Queued, and Queued while waiting for a slot (d0016).
pub const WAITING_TO_START: &[Status] = &[Status::Queued, Status::WaitingForSlot];

/// The message of a start that the service stopping cut short.
const CUT_SHORT: &str = "the background service stopped during this step";

/// How a start ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StartOutcome {
    /// Agent Working, with this session.
    Started(Box<SessionInfo>),
    /// Held in Queued: the base diverged from its remote, or the check before the launch held it (its tool isn't
    /// available or is at its limit, or its model isn't listed).
    Held { message: String },
    /// Start interrupted at `step`.
    Interrupted { step: StartStep, message: String },
    /// The ticket isn't Queued (any more), or a start for it is already running: nothing was done.
    Skipped,
    /// Its project is paused and the start was automatic: it stays waiting, and starts once the user resumes the
    /// project (PAU-04). Nothing was done.
    Kept,
}

/// Runs start sequences. Cheap to clone.
#[derive(Clone)]
pub struct Starter {
    inner: Arc<Inner>,
}

/// Tickets [`Starter::hold`] keeps from starting; dropping it lets them start again.
pub struct StartHold {
    inner: Arc<Inner>,
    ids: Vec<String>,
}

impl Drop for StartHold {
    fn drop(&mut self) {
        let mut running = self.inner.running.lock().expect("running starts");
        for id in &self.ids {
            running.remove(id);
        }
    }
}

struct Inner {
    board: Board,
    data: DataDir,
    ports: PortPool,
    sessions: Sessions,
    /// The agent tools. Without a launch context (the plugin can't be found, and in tests that never launch)
    /// the launch step fails.
    adapters: Adapters,
    events: EventBus,
    /// Tickets whose start is running, so one ticket never runs two at once.
    running: Mutex<HashSet<String>>,
    /// Types a returning ticket's skill into its live session (set once the service has its deliveries).
    deliveries: OnceLock<Deliveries>,
    /// Interrupts a ticket whose agent exited before its start finished (set once the service has it).
    recovery: OnceLock<Recovery>,
    /// Seeds new worktrees (set once the service has its warm builds; unset, nothing is seeded).
    seeder: OnceLock<Seeder>,
    /// Brings a returning ticket's freed build back (set once the service has it; unset, nothing is brought back).
    ticket_env: OnceLock<TicketEnv>,
    /// Runs Create where no ticket environment is set (standalone starters).
    resources: Resources,
    /// The check before every launch (set once the service has it; unset, nothing is checked).
    gate: OnceLock<LaunchGate>,
}

impl Starter {
    pub fn new(
        board: Board,
        data: DataDir,
        ports: PortPool,
        sessions: Sessions,
        adapters: Adapters,
        events: EventBus,
    ) -> Self {
        Self {
            inner: Arc::new(Inner {
                board,
                data,
                ports,
                sessions,
                adapters,
                events,
                running: Mutex::new(HashSet::new()),
                deliveries: OnceLock::new(),
                recovery: OnceLock::new(),
                seeder: OnceLock::new(),
                ticket_env: OnceLock::new(),
                resources: Resources::default(),
                gate: OnceLock::new(),
            }),
        }
    }

    /// Gives the starter the service's deliveries, for skills a returning ticket still has to run in a live
    /// session.
    pub fn set_deliveries(&self, deliveries: Deliveries) {
        let _ = self.inner.deliveries.set(deliveries);
    }

    /// Gives the starter the service's seeder, for new worktrees (WRM-04).
    pub fn set_seeder(&self, seeder: Seeder) {
        let _ = self.inner.seeder.set(seeder);
    }

    /// Gives the starter the service's ticket environment, for a returning ticket whose build was freed (M19-c).
    pub fn set_ticket_env(&self, ticket_env: TicketEnv) {
        let _ = self.inner.ticket_env.set(ticket_env);
    }

    /// The ticket resource commands: the service's (shared with Template refresh), else the starter's own.
    fn resources(&self) -> &Resources {
        self.inner
            .ticket_env
            .get()
            .map_or(&self.inner.resources, TicketEnv::resources)
    }

    /// Gives the starter the service's check before every launch (SCH-04 `[M23]`).
    pub fn set_gate(&self, gate: LaunchGate) {
        let _ = self.inner.gate.set(gate);
    }

    /// Gives the starter the service's recovery, for an agent that exits before its start is recorded.
    pub fn set_recovery(&self, recovery: Recovery) {
        let _ = self.inner.recovery.set(recovery);
    }

    /// Runs the whole start sequence for a ticket waiting to start (Queued, or waiting for a slot). The
    /// scheduler decides when (T-0079); this never checks dependencies or limits.
    ///
    /// The ticket is claimed before its status is read, so two callers can't both start it: the second one
    /// either finds the claim or, once the first has finished, a ticket that isn't waiting any more. `start` says who
    /// asked: an automatic start in a paused project is kept ([`StartOutcome::Kept`]).
    pub async fn start(
        &self,
        key: &str,
        actor: &Actor,
        start: Start,
    ) -> Result<StartOutcome, BoardError> {
        let id = self.inner.board.ticket(key).await?.id;
        if !self
            .inner
            .running
            .lock()
            .expect("running starts")
            .insert(id.clone())
        {
            return Ok(StartOutcome::Skipped);
        }
        let outcome = match self.inner.board.ticket_by_id(&id).await {
            Ok(ticket) if WAITING_TO_START.contains(&ticket.status) => {
                match self.kept(&ticket, start).await {
                    Ok(true) => Ok(StartOutcome::Kept),
                    Ok(false) => self.run(&ticket, actor, start).await,
                    Err(e) => Err(e),
                }
            }
            Ok(_) => Ok(StartOutcome::Skipped),
            Err(e) => Err(e),
        };
        self.inner
            .running
            .lock()
            .expect("running starts")
            .remove(&id);
        outcome
    }

    /// Whether an automatic start of `ticket` is kept because its project is paused (PAU-04).
    async fn kept(&self, ticket: &Ticket, start: Start) -> Result<bool, BoardError> {
        if start == Start::User
            || !crate::agents::pause::is_paused(
                self.inner.board.store(),
                ticket.key.project.as_str(),
            )
            .await?
        {
            return Ok(false);
        }
        tracing::info!(ticket = %ticket.key, "start kept: the project is paused");
        Ok(true)
    }

    /// The ids of the tickets whose start sequence is running now.
    pub fn starting(&self) -> HashSet<String> {
        self.inner.running.lock().expect("running starts").clone()
    }

    /// Holds off new starts of `ids` until the hold drops, so taking an epic back (EPC-14) can't race one: a start
    /// asked for meanwhile is skipped. The ids whose start already runs aren't held; they're returned.
    pub fn hold(&self, ids: impl IntoIterator<Item = String>) -> (StartHold, HashSet<String>) {
        let mut running = self.inner.running.lock().expect("running starts");
        let (mut held, mut starting) = (Vec::new(), HashSet::new());
        for id in ids {
            if running.insert(id.clone()) {
                held.push(id);
            } else {
                starting.insert(id);
            }
        }
        let hold = StartHold {
            inner: self.inner.clone(),
            ids: held,
        };
        (hold, starting)
    }

    /// Marks the step a start is at, so a start cut short by the service stopping (or crashing) is found at the
    /// next service start and shown as Start interrupted instead of being run again silently (SES-02).
    async fn mark_step(&self, ticket: &Ticket, step: StartStep) {
        let result = sqlx::query(
            "UPDATE tickets SET start_problem_step = ?, start_problem_message = NULL WHERE id = ?",
        )
        .bind(step.as_str())
        .bind(&ticket.id)
        .execute(self.inner.board.store().pool())
        .await;
        if let Err(e) = result {
            tracing::warn!(ticket = %ticket.key, error = %e, "can't record the start step");
        }
    }

    /// At service start: tickets still waiting to start with a step marked had their start cut short when the
    /// previous service stopped. They become Start interrupted at that step; only Retry start runs them again.
    pub async fn interrupt_cut_short(board: &Board) -> Result<u64, BoardError> {
        let now = now_ms();
        let result = sqlx::query(concat!(
            "UPDATE tickets SET state = ?, start_problem_message = ?, updated_at = ?, ",
            set_state_since!(),
            " WHERE state IN (?, ?) AND start_problem_step IS NOT NULL AND start_problem_message IS NULL"
        ))
        .bind(Status::StartInterrupted.as_str())
        .bind(CUT_SHORT)
        .bind(now)
        .bind(Status::StartInterrupted.as_str())
        .bind(now)
        .bind(Status::Queued.as_str())
        .bind(Status::WaitingForSlot.as_str())
        .execute(board.store().pool())
        .await?;
        Ok(result.rows_affected())
    }

    /// **Retry start** (the user's click): puts a ticket whose start stopped (Start interrupted, or Held
    /// because its base diverged) back to Queued so the whole sequence runs again. The caller then runs
    /// [`start`](Self::start).
    pub async fn retry(&self, key: &str, actor: &Actor) -> Result<Ticket, BoardError> {
        let ticket = self.inner.board.ticket(key).await?;
        let retryable = ticket.status == Status::StartInterrupted
            || (ticket.status == Status::Held
                && ticket
                    .start_problem
                    .as_ref()
                    .is_some_and(|p| p.step == StartStep::SyncBase));
        if !retryable {
            return Err(BoardError::refused(
                Refusal::MoveRefused,
                format!("{} has no stopped start to retry", ticket.key),
            ));
        }
        self.set_state(
            &ticket,
            Status::Queued,
            None,
            actor,
            EventKind::StartRetried,
            json!({}),
        )
        .await
    }

    /// The tool refused the Developer's model on its first turn (AGT-05, SDD §7.2 `[M22]`, d0088): the ticket
    /// counts as **not started**. It goes back to Ready for Dev as Start failed: Start interrupted at
    /// [`StartStep::Model`] with the tool's message, `started_at` cleared. The session ends as
    /// [`END_MODEL_REFUSED`], so the Developer row unlocks and its conversation isn't resumed (Retry start begins a
    /// fresh one); the worktree and branch stay. Nothing starts it again by itself (d0018): only Retry start or a new
    /// Developer row. Whether the ticket went back (its agent was at work on `session`).
    pub async fn model_refused(
        &self,
        ticket_id: &str,
        session: &str,
        message: &str,
    ) -> Result<bool, BoardError> {
        let board = &self.inner.board;
        let ticket = board.ticket_by_id(ticket_id).await?;
        let own = self
            .inner
            .sessions
            .running_of(ticket_id, SessionKind::Agent)
            .is_none_or(|running| running.id == session);
        if !own || !crate::signals::LIVE.contains(&ticket.status) {
            return Ok(false);
        }
        let to = Status::StartInterrupted;
        let now = now_ms();
        let actor = Actor::Agent(ticket.key.clone());
        // Not started: waiting again before the Agent stage it was starting (Ready for Dev before the first).
        let flow = board.effective_flow(ticket.key.project.as_str()).await?;
        let skips = board.ticket_skips(&ticket, &flow).await?;
        let waits_before = ticket
            .place
            .stage()
            .filter(|id| {
                first_stage(&flow, &skips)
                    .1
                    .is_none_or(|first| first.id != *id)
            })
            .map(str::to_owned);
        let mut tx = board.store().begin_write().await?;
        end_open(&mut tx, &ticket.id, RunResult::Cancelled, None).await?;
        sqlx::query(concat!(
            "UPDATE tickets SET stage_id = ?, state = ?, start_problem_step = ?, start_problem_message = ?,
                                started_at = NULL, signal_text = NULL, signal_since = NULL, wakeup_at = NULL,
                                interrupted_from = NULL, updated_by = ?, updated_at = ?, ",
            set_state_since!(),
            " WHERE id = ?"
        ))
        .bind(waits_before)
        .bind(to.as_str())
        .bind(StartStep::Model.as_str())
        .bind(message)
        .bind(actor.to_string())
        .bind(now)
        .bind(to.as_str())
        .bind(now)
        .bind(&ticket.id)
        .execute(&mut *tx)
        .await?;
        record_event(
            &mut tx,
            &ticket.id,
            EventKind::StartInterrupted,
            &actor,
            json!({ "step": StartStep::Model, "message": message, "session": session }),
        )
        .await?;
        tx.commit().await?;
        if let Err(e) = self
            .inner
            .sessions
            .stop_as(session, END_MODEL_REFUSED)
            .await
        {
            tracing::warn!(ticket = %ticket.key, error = %e, "can't end the refused session");
        }
        let after = board.ticket_by_id(&ticket.id).await?;
        self.publish(&after, true);
        tracing::warn!(ticket = %ticket.key, %message, "the tool refused the model: not started");
        Ok(true)
    }

    async fn run(
        &self,
        ticket: &Ticket,
        actor: &Actor,
        start: Start,
    ) -> Result<StartOutcome, BoardError> {
        let key = ticket.key.to_string();
        match self.steps(ticket, actor, start).await {
            Ok(Done::Started(session)) => {
                self.set_state(
                    ticket,
                    Status::Working,
                    None,
                    actor,
                    EventKind::Started,
                    json!({
                        "session": session.id,
                        "tool": session.tool,
                        "tool_session_id": session.tool_session_id,
                    }),
                )
                .await?;
                // The start is the agent's first sign of life (the card's "active … ago", ATT-05). A returning
                // ticket is no longer parked. Its first prompt (a Send back's skill, an inbox answer) stays until
                // the agent shows it got it ([`Board::first_prompt_delivered`]): a resume that can't find its
                // conversation never ran it, and Retry or Resume must send it again (T-0086, T-0112). A child
                // that joined its epic's queue isn't "new" any more once it started (EPC-13).
                // A ticket whose first turn was refused on the model counted as not started (T-0337): it has
                // started again now.
                sqlx::query(
                    "UPDATE tickets SET last_signal_at = ?, parked = FALSE, reopened = FALSE, return_mode = NULL,
                                        joined_queue_by = NULL, started_at = COALESCE(started_at, ?)
                     WHERE id = ?",
                )
                    .bind(now_ms())
                    .bind(now_ms())
                    .bind(&ticket.id)
                    .execute(self.inner.board.store().pool())
                    .await?;
                tracing::info!(ticket = %key, session = %session.id, "agent started");
                // An agent that exited before the ticket was Working wasn't seen as an exit (d0018): check now.
                if let (Ok(now), Some(recovery)) = (
                    self.inner.sessions.info(&session.id),
                    self.inner.recovery.get(),
                ) && !now.is_running()
                    && let Err(e) = recovery.agent_session_ended(&now).await
                {
                    tracing::warn!(ticket = %key, error = %e, "can't record the agent's exit");
                }
                Ok(StartOutcome::Started(session))
            }
            Ok(Done::AgentHeld(held)) => {
                let message = held.message();
                let step = held.step();
                self.set_state(
                    ticket,
                    Status::Held,
                    Some((step, &message)),
                    actor,
                    EventKind::Held,
                    json!({ "step": step, "message": message, "tool": held.on.tool,
                            "agent": held.agent.name }),
                )
                .await?;
                tracing::info!(ticket = %key, %message, "held before launch");
                if let Some(gate) = self.inner.gate.get() {
                    gate.nudge();
                }
                Ok(StartOutcome::Held { message })
            }
            // Paused while its steps ran: it waits as it was, its worktree kept, and the step mark goes so a restart
            // doesn't take it for a cut-short start.
            Ok(Done::Kept) => {
                sqlx::query(
                    "UPDATE tickets SET start_problem_step = NULL, start_problem_message = NULL WHERE id = ?",
                )
                .bind(&ticket.id)
                .execute(self.inner.board.store().pool())
                .await?;
                self.publish_changed(ticket).await;
                tracing::info!(ticket = %key, "start kept at the launch: the project is paused");
                Ok(StartOutcome::Kept)
            }
            Ok(Done::Held(message)) => {
                self.set_state(
                    ticket,
                    Status::Held,
                    Some((StartStep::SyncBase, &message)),
                    actor,
                    EventKind::Held,
                    json!({ "step": StartStep::SyncBase, "message": message }),
                )
                .await?;
                tracing::info!(ticket = %key, %message, "start held");
                Ok(StartOutcome::Held { message })
            }
            Err((step, e)) => {
                let message = e.to_string();
                self.set_state(
                    ticket,
                    Status::StartInterrupted,
                    Some((step, &message)),
                    actor,
                    EventKind::StartInterrupted,
                    json!({ "step": step, "message": message }),
                )
                .await?;
                tracing::warn!(ticket = %key, %step, %message, "start interrupted");
                Ok(StartOutcome::Interrupted { step, message })
            }
        }
    }

    async fn steps(
        &self,
        ticket: &Ticket,
        actor: &Actor,
        start: Start,
    ) -> Result<Done, (StartStep, StartError)> {
        let inner = &self.inner;
        let board = &inner.board;
        let key = ticket.key.to_string();
        let at = |step: StartStep| move |e: StartError| (step, e);
        let project = board
            .project(ticket.key.project.as_str())
            .await
            .map_err(|e| (StartStep::Base, e.into()))?;
        let repo = Path::new(&project.repo_path);
        // With the project's variables, for the user's hooks (ENV-02).
        let git = &board
            .project_git(&project.key)
            .await
            .map_err(|e| (StartStep::Base, e.into()))?;
        let returning = Returning::of(board, ticket)
            .await
            .map_err(|e| (StartStep::Base, e.into()))?;
        // Waiting before a later Agent stage, not coming back to it (Resume, Retry, a Send back's skill, Park): a new
        // conversation in the worktree it has (STG-07).
        let next_stage = matches!(ticket.place, Place::Waiting(Some(_))) && returning.is_none();
        // The check before every launch, for a new conversation only (AGT-05, d0088: once, at session start).
        let new_conversation = next_stage
            || (!returning.as_ref().is_some_and(|r| r.conversation.is_some())
                && inner
                    .sessions
                    .running_of(&ticket.id, SessionKind::Agent)
                    .is_none());
        let mut checked = None;
        if new_conversation {
            // The running agent of the Agent stage it starts (AGM-05).
            let agent = resolve::session_agent(board, ticket, SessionRole::Developer)
                .await
                .map_err(|e| (StartStep::Base, e.into()))?;
            let runs_on = match inner.gate.get() {
                Some(gate) => match gate
                    .before_launch(SessionRole::Developer, Scope::ticket(ticket), &agent)
                    .await
                    .map_err(|e| (StartStep::Base, e.into()))?
                {
                    Gated::Held(held) => return Ok(Done::AgentHeld(held)),
                    Gated::Launch(runs_on) => runs_on,
                },
                None => RunsOn::agent(
                    &agent,
                    board
                        .store()
                        .default_model()
                        .await
                        .map_err(|e| (StartStep::Base, BoardError::from(e).into()))?
                        .as_ref(),
                ),
            };
            checked = Some((runs_on, agent));
        }
        let keep_worktree = (returning.is_some() || next_stage)
            && ticket
                .work
                .as_ref()
                .and_then(|w| w.worktree.as_deref())
                .is_some_and(|w| Path::new(w).is_dir());

        // 1. The base, and which base branch to bring up to date.
        self.mark_step(ticket, StartStep::Base).await;
        let to_sync = match &ticket.base {
            _ if keep_worktree => None,
            Base::Branch(branch) if branch.is_empty() => {
                return Err((
                    StartStep::Base,
                    StartError::Other(format!("{key} has no base branch")),
                ));
            }
            Base::Branch(branch) => Some(branch.clone()),
            Base::Epic(epic) => {
                let epic = board
                    .ticket(&epic.to_string())
                    .await
                    .map_err(|e| (StartStep::Base, e.into()))?;
                let epic_ref = format!("refs/heads/{}", naming::branch(&epic.key.to_string()));
                let exists = git
                    .resolve(repo, &epic_ref)
                    .await
                    .map_err(|e| (StartStep::Base, e.into()))?
                    .is_some();
                match (&epic.base, exists) {
                    // The epic branch is local only (d0059).
                    (_, true) => None,
                    // The first child: the epic's base is synced before the epic branch is made from it.
                    (Base::Branch(branch), false) => Some(branch.clone()),
                    (Base::Epic(_), false) => None,
                }
            }
        };

        // 2. Keep the base current.
        self.mark_step(ticket, StartStep::SyncBase).await;
        if let Some(branch) = to_sync {
            match git.sync_base(repo, &branch).await {
                Ok(BaseSync::Diverged { message }) => return Ok(Done::Held(message)),
                Ok(BaseSync::CheckoutDirty { message }) => {
                    self.note(
                        ticket,
                        actor,
                        "base_not_updated",
                        json!({ "message": message }),
                    )
                    .await;
                }
                Ok(_) => {}
                Err(e) => return Err((StartStep::SyncBase, e.into())),
            }
        }

        // 3. Branch, worktree, base_sha.
        self.mark_step(ticket, StartStep::Worktree).await;
        // A folder an earlier attempt made is reused, never seeded twice (SES-02).
        let new_worktree = !keep_worktree
            && !inner
                .data
                .worktree_path(&crate::warm::store::repo_name(&project), &key)
                .is_ok_and(|p| p.join(".git").exists());
        let worktree = prepare_worktree(board, &inner.data, &key)
            .await
            .map_err(at(StartStep::Worktree))?;
        if board
            .store()
            .ticket(&ticket.id)
            .await
            .map_err(|e| (StartStep::Worktree, e.into()))?
            .is_some_and(|row| row.reopened)
        {
            self.rebase_reopened(ticket, &worktree, repo, actor)
                .await
                .map_err(at(StartStep::Worktree))?;
        }
        self.publish_changed(ticket).await;
        // Seeding never fails a start (WRM-04).
        let seeded = match inner.seeder.get() {
            Some(seeder) if new_worktree => {
                seeder
                    .seed(board, git, &project, &key, &worktree.path, false)
                    .await
            }
            _ => None,
        };

        // 4 and 5. Env files and the setup command, from the app's settings (d0023).
        self.mark_step(ticket, StartStep::EnvFiles).await;
        let settings = board
            .project_settings(&project.key)
            .await
            .map_err(|e| (StartStep::EnvFiles, e.into()))?;
        if !keep_worktree {
            // The log begins with where the worktree was seeded from, even if the env files fail.
            let log = inner
                .data
                .setup_log_path(&key)
                .map_err(|e| (StartStep::Setup, e.into()))?;
            let first_line = seeded.as_ref().map(|s| s.log_line());
            if let Err(e) = begin_setup_log(&log, first_line.as_deref()) {
                tracing::warn!(ticket = %key, error = %e, "can't begin the setup log");
            }
            copy_env_files(repo, &worktree.path, &settings.env_files)
                .map_err(at(StartStep::EnvFiles))?;
            // The ticket's own resources, before setup so setup can use them (M19-d, d0087).
            self.mark_step(ticket, StartStep::Resources).await;
            self.resources()
                .create(board, &inner.data, &project, ticket, &worktree.path, &log)
                .await
                .map_err(at(StartStep::Resources))?;
            self.mark_step(ticket, StartStep::Setup).await;
            let recorded = board
                .store()
                .ticket(&ticket.id)
                .await
                .map_err(|e| (StartStep::Setup, e.into()))?
                .and_then(|row| row.resource_name);
            let own = command_vars(
                &inner.data,
                &project,
                &ticket.key,
                recorded.as_deref(),
                &settings,
            );
            let project_env = applied(&settings.env_vars, Os::current());
            run_setup(
                &settings.setup_command,
                &worktree.path,
                &log,
                &project_env,
                &own,
            )
            .await
            .map_err(at(StartStep::Setup))?;
            // The worktree was set up afresh: whatever build it has is in it.
            if build_state(ticket) != BuildState::Present {
                board
                    .store()
                    .set_build_state(&ticket.id, BuildState::Present, None)
                    .await
                    .map_err(|e| (StartStep::Setup, e.into()))?;
            }
        } else if let Some(env) = inner.ticket_env.get() {
            // A returning ticket whose build was freed at Review (a full limit or a gone session): it comes back
            // before the agent does (M19-c). Always asked, never judged from `ticket` (read without the ticket's
            // lock): a Free still moving the build out when the ticket was queued again finishes first, and the
            // fetch reads the ticket again under the lock (a build still there costs one read).
            if build_state(ticket) != BuildState::Present {
                self.mark_step(ticket, StartStep::Setup).await;
            }
            env.fetch_build(ticket, FetchWhy::Start)
                .await
                .map_err(at(StartStep::Setup))?;
        }

        // 6. Two ports.
        self.mark_step(ticket, StartStep::Ports).await;
        let ports = inner
            .ports
            .allocate(&ticket.id)
            .await
            .map_err(|e| (StartStep::Ports, StartError::Other(e.to_string())))?;

        // 7. The agent. A session still running from an earlier attempt is reused; the previous stage's is stopped.
        self.mark_step(ticket, StartStep::Launch).await;
        let running = inner.sessions.running_of(&ticket.id, SessionKind::Agent);
        if let Some(previous) = running.as_ref().filter(|_| next_stage)
            && let Err(e) = inner.sessions.stop(&previous.id).await
        {
            tracing::warn!(ticket = %key, error = %e, "can't stop the previous stage's session");
        }
        if let Some(running) = running.filter(|_| !next_stage) {
            if let (Some(skill), Some(deliveries)) = (
                returning.as_ref().and_then(|r| r.skill.as_deref()),
                inner.deliveries.get(),
            ) {
                // By its name when the session started with it, else by its file, copied now (d0118); what came
                // back follows the skill (CMT-05), never a note's text.
                let text = deliveries
                    .skill_line(
                        board,
                        &running.id,
                        Some(&worktree.path),
                        skill,
                        returning.as_ref().and_then(|r| r.prompt.as_deref()),
                    )
                    .await;
                deliveries
                    .queue_delivery(&running.id, DeliveryKind::Skill, &text)
                    .await
                    .map_err(|e| (StartStep::Launch, StartError::Other(e.to_string())))?;
            }
            return Ok(Done::Started(Box::new(running)));
        }
        // A new conversation's own first message: the instructions of a drag back into a stage that never ran.
        let instructions = returning
            .as_ref()
            .filter(|r| r.conversation.is_none())
            .and_then(|r| r.prompt.clone());
        let conversation = returning.and_then(|r| {
            let prompt = r.first_prompt();
            r.conversation.map(|c| (c, prompt))
        });
        // A new conversation runs on the agent resolved at the check (AGM-05: the start, Retry start, Start from
        // scratch); a resumed one on the tool, model and agent it ran on (ADP-04, d0088).
        let mut agent_new: Option<Agent> = None;
        let (runs_on, resume, first_prompt) = match conversation {
            Some((c, prompt)) => (RunsOn::resumed(&c), Some(c.id), prompt),
            None => {
                // The stage's own skill (Q261): `grill` for a Grill stage, `work-ticket` for Dev.
                let flow = board
                    .effective_flow(&project.key)
                    .await
                    .map_err(|e| (StartStep::Launch, e.into()))?;
                let skips = board
                    .ticket_skips(ticket, &flow)
                    .await
                    .map_err(|e| (StartStep::Launch, e.into()))?;
                let skill = resolve::agent_stage_of(&flow, ticket, &skips)
                    .and_then(|stage| stage.skill.clone())
                    .unwrap_or_else(|| WORK_TICKET_SKILL.to_owned());
                let runs_on = match checked {
                    Some((runs_on, agent)) => {
                        agent_new = Some(agent);
                        runs_on
                    }
                    None => {
                        let agent = resolve::session_agent(board, ticket, SessionRole::Developer)
                            .await
                            .map_err(|e| (StartStep::Launch, e.into()))?;
                        let default = board
                            .store()
                            .default_model()
                            .await
                            .map_err(|e| (StartStep::Launch, BoardError::from(e).into()))?;
                        RunsOn::agent(&agent, default.as_ref())
                    }
                };
                (
                    runs_on,
                    None,
                    Some(FirstPrompt::Skill {
                        name: skill,
                        message: instructions,
                    }),
                )
            }
        };
        match self
            .launch(
                ticket,
                &project,
                &worktree.path,
                ports.agent,
                runs_on,
                agent_new.as_ref(),
                resume,
                first_prompt,
                start,
            )
            .await
        {
            Ok(session) => Ok(Done::Started(Box::new(session))),
            // The backstop refused it: the project was paused meanwhile.
            Err(_)
                if self
                    .kept(ticket, start)
                    .await
                    .map_err(|e| (StartStep::Launch, e.into()))? =>
            {
                Ok(Done::Kept)
            }
            Err(e) => {
                // A launch that failed because of its tool: checked again, then held if it can't start now.
                if let (Some(gate), Some(agent)) = (inner.gate.get(), agent_new.as_ref())
                    && let Some(held) = gate
                        .after_failed_launch(SessionRole::Developer, Scope::ticket(ticket), agent)
                        .await
                        .map_err(|e| (StartStep::Launch, e.into()))?
                {
                    return Ok(Done::AgentHeld(Box::new(held)));
                }
                Err((StartStep::Launch, e))
            }
        }
    }

    /// Builds the agent's launch and spawns its session. `agent` is a new conversation's agent; a resumed one runs
    /// the agent its session stored.
    #[allow(clippy::too_many_arguments)]
    async fn launch(
        &self,
        ticket: &Ticket,
        project: &crate::store::projects::Project,
        cwd: &Path,
        port: u16,
        runs_on: RunsOn,
        agent: Option<&Agent>,
        resume: Option<String>,
        first_prompt: Option<FirstPrompt>,
        start: Start,
    ) -> Result<SessionInfo, StartError> {
        let inner = &self.inner;
        // Every skill of the running agent, the one it runs first and every hand-off skill (d0118), written into the
        // worktree by one name (AGM-07); a copy that can't be written is logged.
        let stored = match (agent, &runs_on.agent) {
            (None, Some(stored)) => inner.board.store().agent(&stored.id).await?,
            _ => None,
        };
        let (first_prompt, written) = inner
            .board
            .copy_for_ticket_session(
                cwd,
                inner.adapters.shipped_skills(),
                agent.or(stored.as_ref()),
                first_prompt,
                &project.key,
            )
            .await;
        let launched = inner
            .adapters
            .launch(
                runs_on.tool,
                &LaunchRequest {
                    role: LaunchRole::TicketAgent,
                    cwd: cwd.to_owned(),
                    project: project.key.clone(),
                    ticket: Some(ticket.key.clone()),
                    epic: None,
                    port,
                    first_prompt,
                    resume,
                    model: runs_on.model,
                    effort: runs_on.effort.clone(),
                    title: Some(ticket.title.clone()),
                    theme: inner.board.theme_in_use().await,
                },
            )
            .map_err(|e| StartError::Other(e.to_string()))?;
        let session = inner
            .sessions
            .prepared(launched.prepare)
            .spawn(SpawnSpec {
                kind: SessionKind::Agent,
                start,
                project_id: project.id.clone(),
                project_key: project.key.clone(),
                ticket: Some(SessionTicket {
                    id: ticket.id.clone(),
                    key: ticket.key.clone(),
                }),
                program: launched.plan.program,
                args: launched.plan.args,
                cwd: launched.plan.cwd,
                env: launched.plan.env,
                size: INITIAL_SIZE,
                tool: Some(launched.tool),
                tool_session_id: launched.tool_session_id,
                model: launched.model,
                agent: runs_on.agent,
                effort: runs_on.effort,
                fallback: runs_on.fallback,
                theme: Some(launched.theme),
            })
            .await
            .map_err(|e| StartError::Other(e.to_string()))?;
        if let Err(e) = inner
            .sessions
            .set_started_skills(&session.id, written.names)
        {
            tracing::warn!(ticket = %ticket.key, error = %e, "the session's skills weren't recorded");
        }
        Ok(session)
    }

    /// A reopened ticket's old branch is first rebased onto the current base with no tokens (GIT-06; the
    /// checkpoint rule keeps uncommitted files). Clean: the new `base_sha` is recorded. A conflict leaves the
    /// branch as it was and records "conflicts with `main`" for `otaq prime`; the agent still starts. A
    /// hook that refuses the checkpoint stops the start (GIT-02).
    async fn rebase_reopened(
        &self,
        ticket: &Ticket,
        worktree: &super::Worktree,
        repo: &Path,
        actor: &Actor,
    ) -> Result<(), StartError> {
        use crate::board::conflicts::{Conflict, ConflictReason};
        use crate::git::checkpoint::{RebaseError, RebaseOutcome, checkpoint_rejected_message};
        let board = &self.inner.board;
        let git = &board.project_git(ticket.key.project.as_str()).await?;
        let onto = super::worktree::branch_commit(git, repo, &worktree.base_branch).await?;
        let outcome = match git.rebase_with_checkpoint(&worktree.path, &onto).await {
            Ok(outcome) => outcome,
            Err(RebaseError::CheckpointRejected { .. }) => {
                return Err(StartError::Other(checkpoint_rejected_message(
                    &ticket.key.to_string(),
                )));
            }
            Err(RebaseError::Git(e)) => return Err(e.into()),
        };
        let payload = match outcome {
            RebaseOutcome::Clean => {
                sqlx::query("UPDATE tickets SET base_sha = ? WHERE id = ?")
                    .bind(&onto)
                    .bind(&ticket.id)
                    .execute(board.store().pool())
                    .await
                    .map_err(crate::store::StoreError::from)?;
                board.clear_conflict(&ticket.id).await?;
                json!({ "onto": worktree.base_branch, "sha": onto })
            }
            RebaseOutcome::RebaseConflict { files } | RebaseOutcome::UndoConflict { files } => {
                board
                    .set_conflict(
                        &ticket.id,
                        &Conflict {
                            target: worktree.base_branch.clone(),
                            files: files.clone(),
                            reason: ConflictReason::Reopen,
                        },
                    )
                    .await?;
                json!({ "onto": worktree.base_branch, "conflict": files })
            }
        };
        let mut tx = board
            .store()
            .begin_write()
            .await
            .map_err(crate::store::StoreError::from)?;
        record_event(&mut tx, &ticket.id, EventKind::Rebased, actor, payload).await?;
        tx.commit().await.map_err(crate::store::StoreError::from)?;
        Ok(())
    }

    /// Sets the ticket's status and start problem, records the event and tells clients.
    async fn set_state(
        &self,
        ticket: &Ticket,
        status: Status,
        problem: Option<(StartStep, &str)>,
        actor: &Actor,
        kind: EventKind,
        payload: Value,
    ) -> Result<Ticket, BoardError> {
        let board = &self.inner.board;
        let now = now_ms();
        // Started: into the Agent stage it waited before (the first one it runs from Ready for Dev), past the stages
        // it skips. Any other status keeps it waiting where it is.
        let begun = match status {
            Status::Working => Some(self.begin(ticket).await?),
            _ => None,
        };
        let mut tx = board.store().begin_write().await?;
        if let Some(begun) = &begun {
            begun.record(&mut tx, &ticket.id, None).await?;
        }
        sqlx::query(concat!(
            "UPDATE tickets SET stage_id = COALESCE(?, stage_id), state = ?, start_problem_step = ?,
                                start_problem_message = ?, updated_by = ?, updated_at = ?, ",
            set_state_since!(),
            " WHERE id = ?"
        ))
        .bind(begun.as_ref().and_then(|b| b.place.stage_id()))
        .bind(status.as_str())
        .bind(problem.map(|(step, _)| step.as_str()))
        .bind(problem.map(|(_, message)| message))
        .bind(actor.to_string())
        .bind(now)
        .bind(status.as_str())
        .bind(now)
        .bind(&ticket.id)
        .execute(&mut *tx)
        .await?;
        record_event(&mut tx, &ticket.id, kind, actor, payload).await?;
        tx.commit().await?;
        let after = board.ticket_by_id(&ticket.id).await?;
        self.publish(&after, ticket.status != after.status);
        Ok(after)
    }

    /// Where the scheduler's start takes a waiting ticket in its project's flow ([`Move::Begin`]). A stage the flow
    /// no longer has (the flow changed while it waited) starts it in the first Agent stage instead.
    async fn begin(&self, ticket: &Ticket) -> Result<Moved, BoardError> {
        let board = &self.inner.board;
        let waiting = if ticket.place.is_waiting() {
            ticket.place.clone()
        } else {
            Place::Waiting(None)
        };
        let at = Ticket {
            place: waiting,
            ..ticket.clone()
        };
        match board.moved(&at, Status::Queued, Move::Begin).await {
            Ok(begun) => Ok(begun),
            Err(_) => {
                let first = Ticket {
                    place: Place::Waiting(None),
                    ..ticket.clone()
                };
                board.moved(&first, Status::Queued, Move::Begin).await
            }
        }
    }

    /// Records a note in the ticket's history without changing its status.
    async fn note(&self, ticket: &Ticket, actor: &Actor, what: &str, payload: Value) {
        let result = async {
            let mut tx = self.inner.board.store().begin_write().await?;
            let mut payload = payload;
            payload["note"] = json!(what);
            record_event(&mut tx, &ticket.id, EventKind::StartNote, actor, payload).await?;
            tx.commit().await?;
            Ok::<_, BoardError>(())
        }
        .await;
        if let Err(e) = result {
            tracing::warn!(ticket = %ticket.key, error = %e, "can't record a start note");
        }
    }

    async fn publish_changed(&self, ticket: &Ticket) {
        if let Ok(after) = self.inner.board.ticket_by_id(&ticket.id).await {
            self.publish(&after, false);
        }
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

/// Where the ticket's build was when it was read.
fn build_state(ticket: &Ticket) -> BuildState {
    ticket
        .work
        .as_ref()
        .map(|w| w.build_state)
        .unwrap_or_default()
}

enum Done {
    Started(Box<SessionInfo>),
    /// The base diverged.
    Held(String),
    /// The check before the launch held it.
    AgentHeld(Box<Held>),
    /// The project was paused before the launch (an automatic start).
    Kept,
}

/// A ticket coming back to Dev with its work: parked, sent back to its agent with a skill to run, or resumed
/// after it was interrupted (or started from scratch in the same worktree).
struct Returning {
    /// The skill to run once the agent is live (`address-review`, `restack`); `None` for a parked ticket.
    skill: Option<String>,
    /// The user's answer from the inbox to an Interrupted question (ATT-08), sent as the resumed
    /// conversation's first prompt; with a skill, what follows it (a Send back's "SHOP-18 was sent back to Dev with
    /// a comment. Run `otaq prime`.", CMT-05).
    prompt: Option<String>,
    /// The conversation of its latest agent session, continued with its tool's resume.
    conversation: Option<StoredConversation>,
    /// The user's Resume, Resume all or Retry brought it back (`return_mode` "resume").
    resumed: bool,
}

impl Returning {
    /// What the resumed conversation gets first: the skill followed by its text, else the inbox answer, else (a
    /// Resume with nothing waiting) "Carry on where you left off" (d0122).
    fn first_prompt(&self) -> Option<FirstPrompt> {
        match (&self.skill, &self.prompt) {
            (Some(name), message) => Some(FirstPrompt::Skill {
                name: name.clone(),
                message: message.clone(),
            }),
            (None, Some(prompt)) => Some(FirstPrompt::Text(prompt.clone())),
            (None, None) if self.resumed => {
                Some(FirstPrompt::Text(crate::recovery::carry_on_text()))
            }
            (None, None) => None,
        }
    }
}

impl Returning {
    async fn of(board: &Board, ticket: &Ticket) -> Result<Option<Self>, BoardError> {
        let Some(row) = board.store().ticket(&ticket.id).await? else {
            return Ok(None);
        };
        if row.resume_skill.is_none() && !row.parked && row.return_mode.is_none() {
            return Ok(None);
        }
        // Start from scratch: the same worktree, a new conversation; a drag back into a stage that never ran gives
        // it the user's instructions (FLW-18, Start from scratch keeps none).
        if row.return_mode.as_deref() == Some(crate::recovery::RETURN_SCRATCH) {
            return Ok(Some(Self {
                skill: None,
                prompt: row.resume_prompt,
                conversation: None,
                resumed: false,
            }));
        }
        let conversation =
            StoredConversation::latest_agent(board.store().pool(), &ticket.id).await?;
        Ok(Some(Self {
            skill: row.resume_skill,
            prompt: row.resume_prompt,
            conversation,
            resumed: row.return_mode.as_deref() == Some(crate::recovery::RETURN_RESUME),
        }))
    }
}
