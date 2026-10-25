//! Validation before Review (VAL-01, VAL-13, SCH-04; d0063): the agent's `review submit` starts a separate,
//! read-only **validator** instead of moving the ticket to Ready for Review.
//!
//! A submit is checked as before (REV-07: refused with uncommitted files), its summary and head are recorded, a
//! round opens in `validations` (round = counted rounds + 1, who started it, the head) and the ticket shows
//! **Validating…**. The service then starts the validator: a fresh agent session in the ticket's worktree with the
//! project's Validate skill as its prompt, the validator role and no `PORT` (its tool's adapter builds the
//! launch, [`crate::agent_tools::LaunchRole::Validator`]). The agent's submit is the go-ahead (a P1 exception, like
//! answering another agent). One validator per ticket; its tool and tool id are stored like every session's
//! (d0045).
//!
//! **Read-only** (VAL-02, VAL-08): the round's head is recorded when it opens (the worktree has no uncommitted
//! files then). If tracked files or the head differ when the verdict lands and the agent's session took no turn
//! meanwhile, the validator changed them: the verdict is discarded ("Validation discarded: the validator changed
//! files"). If the agent took a turn and changed them, or the user edited the description, the validation is
//! cancelled: the validator stops, the round doesn't count, the agent works again and is told so. Changes are
//! noticed on the agent's turn ends and on verdicts; Otaq never stashes or touches the worktree to check.
//!
//! **Any agent check of any flow** runs this way (FLW-13, d0108 second gap review rule 9): each counts its own fails
//! in a row ([`crate::board::validation::CheckCounts`]) against its own rounds; a pass into another agent check opens
//! that check's round and starts its checker; 0063's rules (VAL-02, VAL-07, VAL-08) and Stop this check hold
//! whatever the check's skill. **Move on anyway** and **Skip this check** take the card to the engine's next stage,
//! and its Delivery is then in click mode ([`Board::forced_click_reason`]); Skip this check is also offered on a check
//! held for the plan limit when no fallback can take it (Q277), never on a held Agent stage.
//!
//! With validation off for the project the submit goes straight to Ready for Review, as before M12. Every round's
//! validator goes through the check before every launch (SCH-04 `[M23]`, [`crate::agents::gate`]): when its
//! tool is at its limit the round opens but no validator starts and the ticket shows "Waiting for limit reset";
//! when its tool isn't available or its model isn't listed the round stays open without a validator (Validating…,
//! with the hold). The automatic fallback may move its Validator row instead. The validator starts once the hold
//! lifts ([`Validation::start_held`]), or with its tool's Resume ([`Validation::start_waiting`]).
//!
//! **While the project is paused** (PAU-04, d0112) a checker Otaq would start on its own ([`Start::Auto`]: after a
//! submit or a pass, a drag into a check, a lifted hold, a tool's Resume) doesn't start: the ticket is taken the
//! pause's way ([`crate::recovery::Recovery::kept_paused`]: interrupted from its check, the round not counted), and the
//! project's Resume starts a fresh check. The user's Retry validation, or Resume on the one ticket, starts it.

use std::collections::HashSet;
use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::{Arc, Mutex};

use crate::agent_tools::{Adapters, FirstPrompt, LaunchRequest, LaunchRole};
use crate::agents::gate::{Gated, Held, LaunchGate, Scope};
use crate::agents::store::HoldScope;
use crate::agents::{AgentHold, HoldReason, RunsOn, SessionRole, resolve};
use crate::board::hand_over::{HandOver, check_failed_text, settled};
use crate::board::tickets::Ticket;
use crate::board::validation::{
    CheckCounts, OpenRound, RoundClose, RowEnd, Transition, ValidationEvent, ValidationState,
    check_name, round_event, transition, verdict_of,
};
use crate::board::{Board, BoardError, Refusal};
use crate::domain::validation::{CancelledBy, cancelled_toast};
use crate::domain::{
    Actor, MovedPast, Place, Status, ValidationAction, ValidationOutcome, ValidationTrigger,
};
use crate::epic_lead::stuck::FirstStop;
use crate::flow::engine::Counters;
use crate::protocol::Event;
use crate::service::EventBus;
use crate::session::deliveries::{Deliveries, DeliveryKind};
use crate::session::{
    END_MODEL_REFUSED, SessionInfo, SessionKind, SessionTicket, Sessions, SpawnSpec, Start,
    TerminalSize,
};
use crate::signals::{LIVE, Signal};
use crate::store::tickets::set_state_since;

/// What the agent is told when its check is cancelled (VAL-08), naming the check (Q344): "Security was cancelled
/// because the code changed. Run `otaq stage next` when you're done."
pub fn cancelled_message(check: &str) -> String {
    format!(
        "{check} was cancelled because the code changed. Run `{} stage next` when you're done.",
        crate::naming::COMMAND_NAME
    )
}

/// Starts the No result text of a round whose validator's model the tool refused (AGT-05): "The validator couldn't
/// run `claude-fable-5-1`: There's an issue with the selected model …". [`refused_its_model`] tells it apart.
pub const VALIDATOR_COULDNT_RUN: &str = "The validator couldn't run";

/// The No result text of a refused validator's model (`model`: as the mark names it).
pub fn couldnt_run_text(model: &str, message: &str) -> String {
    format!("{VALIDATOR_COULDNT_RUN} `{model}`: {}", message.trim())
}

/// Whether a No result's text says the tool refused the validator's model (Change validator, T-0345).
pub fn refused_its_model(signal_text: &str) -> bool {
    signal_text.starts_with(VALIDATOR_COULDNT_RUN)
}

/// The size a validator starts with until a terminal attaches.
const INITIAL_SIZE: TerminalSize = TerminalSize {
    rows: 40,
    cols: 120,
};

/// Starts and tracks validators. Cheap to clone.
#[derive(Clone)]
pub struct Validation {
    inner: Arc<Inner>,
}

struct Inner {
    board: Board,
    sessions: Sessions,
    deliveries: Deliveries,
    adapters: Adapters,
    /// For the cancel toast (UI-07).
    events: EventBus,
    /// Tickets whose agent took a turn since their validation started (VAL-08 tells it from VAL-02).
    agent_turned: Mutex<HashSet<String>>,
    /// One checker start at a time per ticket: a tool's Resume and the hold lifting it causes both start a waiting
    /// round's checker, and two overlapping starts would leave the round naming a checker that was stopped.
    starting: Mutex<std::collections::HashMap<String, Arc<tokio::sync::Mutex<()>>>>,
    /// One end of a round's checker at a time per ticket (a verdict, a turn without one, a cancel): the last of a
    /// round's check agents closes it, and two verdicts at once would each wait for the other (d0130).
    ending: Mutex<std::collections::HashMap<String, Arc<tokio::sync::Mutex<()>>>>,
    /// Resumes a gone agent with a first prompt (Review's resume; set by the service).
    resume_agent: Mutex<Option<AgentResume>>,
    /// The check before every launch (set by the service; unset, nothing is checked).
    gate: std::sync::OnceLock<LaunchGate>,
    /// Keeps a checker's start while the project is paused (set by the service; unset, the ticket is left as it is).
    recovery: std::sync::OnceLock<crate::recovery::Recovery>,
}

/// How a validator's start went.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValidatorStart {
    Started(Box<SessionInfo>),
    /// The check before the launch held it: the round waits without a validator.
    Held(Box<Held>),
    /// The project is paused and the start was automatic: the ticket was taken the pause's way (PAU-04).
    Kept,
}

impl ValidatorStart {
    /// The validator's session, if it started.
    pub fn session(self) -> Option<Box<SessionInfo>> {
        match self {
            Self::Started(session) => Some(session),
            Self::Held(_) | Self::Kept => None,
        }
    }
}

/// Resumes the agent of a ticket (its id) with a first prompt, started as `Start` says.
pub type AgentResume = Arc<
    dyn Fn(String, String, Start) -> Pin<Box<dyn Future<Output = Result<(), BoardError>> + Send>>
        + Send
        + Sync,
>;

impl Validation {
    pub fn new(
        board: Board,
        sessions: Sessions,
        deliveries: Deliveries,
        adapters: Adapters,
        events: EventBus,
    ) -> Self {
        Self {
            inner: Arc::new(Inner {
                board,
                sessions,
                deliveries,
                adapters,
                events,
                agent_turned: Mutex::new(HashSet::new()),
                starting: Mutex::new(std::collections::HashMap::new()),
                ending: Mutex::new(std::collections::HashMap::new()),
                resume_agent: Mutex::new(None),
                gate: std::sync::OnceLock::new(),
                recovery: std::sync::OnceLock::new(),
            }),
        }
    }

    /// Gives the validators the service's check before every launch (SCH-04 `[M23]`).
    pub fn set_gate(&self, gate: LaunchGate) {
        let _ = self.inner.gate.set(gate);
    }

    /// Gives the validators the service's recovery, which keeps a checker's start while its project is paused.
    pub fn set_recovery(&self, recovery: crate::recovery::Recovery) {
        let _ = self.inner.recovery.set(recovery);
    }

    /// Whether an automatic checker start for `ticket` is kept because its project is paused (PAU-04): then the
    /// ticket is taken the pause's way, once.
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
        tracing::info!(key = %ticket.key, "checker start kept: the project is paused");
        if let Some(recovery) = self.inner.recovery.get() {
            recovery.kept_paused(&ticket.id).await?;
        }
        self.forget_turns(&ticket.id);
        Ok(true)
    }

    /// `otaq check pass|fail --stdin [--report-file PATH]` from the ticket's checker (VAL-03 … VAL-06): the verdict's
    /// text is the checker's comment (`hand_over`, with its report; CMT-05). Pass: Ready for Review, "Validated ·
    /// round N". Fail with rounds left: the agent is told "Validate failed round 2. Run `otaq prime`." and works
    /// again, "Validation failed · round N of M". The last round: "Validation failed M times", nothing delivered. The
    /// validator ends once its turn ends. A refused verdict posts nothing.
    pub async fn verdict(
        &self,
        key: &str,
        pass: bool,
        hand_over: Option<&HandOver>,
        session: &str,
    ) -> Result<Ticket, BoardError> {
        let result = self.verdict_with(key, pass, hand_over, session).await;
        settled(&self.inner.board, hand_over, result).await
    }

    async fn verdict_with(
        &self,
        key: &str,
        pass: bool,
        hand_over: Option<&HandOver>,
        session: &str,
    ) -> Result<Ticket, BoardError> {
        let board = &self.inner.board;
        let ticket = board.ticket(key).await?;
        let lock = self.end_lock(&ticket.id);
        let _one_end = lock.lock().await;
        let ticket = board.ticket(key).await?;
        let not_running = || {
            BoardError::refused(
                Refusal::NotAllowed,
                format!("This isn't {}'s running check", ticket.key),
            )
        };
        // The calling session's own row of the open round (d0130: one per check agent).
        let Some((round, row)) = board.open_round(&ticket.id).await?.and_then(|round| {
            let row = round.open_row_of(session)?.id.clone();
            Some((round, row))
        }) else {
            return Err(not_running());
        };
        // The worktree must still be what was submitted (VAL-02, VAL-08).
        if let Some(head) = &round.head
            && board.worktree_changed(&ticket, head).await?
        {
            return Err(self.changed_under_validation(&ticket).await?);
        }
        let facts = self.check_facts(&ticket).await?;
        let (event, outcome) = if pass {
            (ValidationEvent::Pass, ValidationOutcome::Pass)
        } else {
            (ValidationEvent::Fail, ValidationOutcome::Fail)
        };
        // Refused where the ticket doesn't validate, whatever the other check agents said.
        transition(facts.state(&ticket), event)
            .map_err(|e| BoardError::refused(Refusal::MoveRefused, e.0))?;
        let end = RowEnd {
            row: &row,
            outcome,
            reason: "",
            hand_over,
        };
        let actor = Actor::Agent(ticket.key.clone());
        if !self
            .row_ended(&ticket, &facts, &round, end, None, &actor)
            .await?
        {
            return Err(not_running());
        }
        board.ticket_by_id(&ticket.id).await
    }

    /// One check agent's row of the open round ended (its verdict, or none): the round waits while another still
    /// checks; the last one closes it (d0130). Every one passed: Pass. Any failed: Fail, the hand-over naming each
    /// agent's verdict in the stage's order and the round counted once. Else it ended without a verdict: `signal` is
    /// the card's text for one check agent; several name the agents with none. `false` when the row had ended
    /// already (the round was cancelled meanwhile): nothing is written.
    async fn row_ended(
        &self,
        ticket: &Ticket,
        facts: &CheckFacts,
        round: &OpenRound,
        end: RowEnd<'_>,
        signal: Option<&str>,
        actor: &Actor,
    ) -> Result<bool, BoardError> {
        let board = &self.inner.board;
        if round.open().any(|r| r.id != end.row) {
            return board
                .end_in_round(ticket, round, Some(end), None, actor)
                .await;
        }
        let verdicts: Vec<(String, Option<ValidationOutcome>)> = round
            .latest()
            .into_iter()
            .map(|r| {
                let outcome = if r.id == end.row {
                    Some(end.outcome)
                } else {
                    r.outcome
                };
                let agent = r.agent.clone().unwrap_or_else(|| "The check".to_owned());
                (agent, outcome)
            })
            .collect();
        let outcomes: Vec<Option<ValidationOutcome>> = verdicts.iter().map(|(_, o)| *o).collect();
        let event = round_event(&outcomes);
        let step = transition(facts.state(ticket), event)
            .map_err(|e| BoardError::refused(Refusal::MoveRefused, e.0))?;
        if event == ValidationEvent::NoVerdict {
            let named = round.several().then(|| no_verdict_text(&verdicts));
            let close = RoundClose {
                step: &step,
                reason: "",
                signal: named.as_deref().or(signal),
                resumed: false,
            };
            let ended = board
                .end_in_round(ticket, round, Some(end), Some(close), actor)
                .await?;
            if ended {
                tracing::info!(key = %ticket.key, "validation ended without a result");
            }
            return Ok(ended);
        }
        // An epic child's first "Validation failed N times" goes to its live lead once (LEAD-07): held at work.
        let held = step.to == Status::CheckFailed && board.leads().takes_stop(ticket).await;
        let recorded = if held {
            // Held at work: back in the Agent stage before the check.
            let place = ticket
                .place
                .stage()
                .and_then(|id| crate::flow::engine::back_to(&facts.flow, id, &[]))
                .map_or_else(|| ticket.place.clone(), |s| Place::stage_of(&s.id));
            Transition {
                to: Status::Working,
                place,
                ..step.clone()
            }
        } else {
            step.clone()
        };
        let close = RoundClose {
            step: &recorded,
            reason: "",
            signal: None,
            resumed: step.deliver_findings || held,
        };
        if !board
            .end_in_round(ticket, round, Some(end), Some(close), actor)
            .await?
        {
            return Ok(false);
        }
        if held {
            let report = board.last_report(&ticket.id).await?.unwrap_or_default();
            if !board
                .leads()
                .child_stuck(ticket, &FirstStop::validation_failed(&report, facts.rounds))
                .await
            {
                board
                    .set_validation_status(
                        &ticket.id,
                        &[Status::Working],
                        Status::CheckFailed,
                        None,
                        false,
                    )
                    .await?;
            }
        }
        if step.deliver_findings {
            // The findings are the checkers' comments: the agent reads them all in `prime` (CMT-05, d0130).
            let number = facts.fails + 1;
            let check = check_name(&facts.flow, &ticket.place);
            let text = if round.several() {
                checks_failed_text(&check, number, &verdicts)
            } else {
                check_failed_text(&check, number)
            };
            self.tell_agent(ticket, &text, Start::Auto).await;
        }
        if step.start_validator {
            // A pass into the next agent check: its own round and checkers (FLW-13).
            self.start_next_check(&ticket.id, ValidationTrigger::Agent)
                .await?;
        }
        // A pass into an automatic Delivery delivers now, unless the user moved the card past a check (FLW-15).
        board.delivery().arrived(&ticket.id).await;
        Ok(true)
    }

    /// What the rules need about the ticket's agent check ([`CheckFacts`]).
    async fn check_facts(&self, ticket: &Ticket) -> Result<CheckFacts, BoardError> {
        let board = &self.inner.board;
        let flow = board.effective_flow(ticket.key.project.as_str()).await?;
        let skips = board.ticket_skips(ticket, &flow).await?;
        let counts = board.check_counts(&ticket.id).await?;
        let count = counts
            .counting(&flow, &ticket.place)
            .map(|check| counts.of(&check.id))
            .unwrap_or_default();
        let rounds = flow.check_rounds(
            counts
                .counting(&flow, &ticket.place)
                .map(|check| check.id.as_str()),
            count.extra,
        );
        Ok(CheckFacts {
            flow,
            skips,
            fails: count.fails,
            rounds,
            counts,
        })
    }

    /// The ticket has just entered another agent check from one (a pass, a skip, Move on anyway): that check's round
    /// opens and its checker starts; a checker that can't start ends the round without a result, as a submit's.
    async fn start_next_check(
        &self,
        ticket_id: &str,
        trigger: ValidationTrigger,
    ) -> Result<(), BoardError> {
        let board = &self.inner.board;
        let ticket = board.ticket_by_id(ticket_id).await?;
        if !validating(ticket.status) {
            return Ok(());
        }
        board.open_next_check(&ticket, trigger).await?;
        self.forget_turns(ticket_id);
        if let Err(e) = self.start_validator(ticket_id, Start::Auto).await {
            tracing::warn!(key = %ticket.key, error = %e, "the next check's checker didn't start");
            board
                .validator_did_not_start(ticket_id, &e.to_string())
                .await?;
        }
        Ok(())
    }

    /// The worktree changed during validation: cancelled when the agent took a turn, else discarded (the
    /// validator changed it). Returns the refusal the validator's verdict gets.
    async fn changed_under_validation(&self, ticket: &Ticket) -> Result<BoardError, BoardError> {
        if self.agent_took_a_turn(&ticket.id) {
            self.cancel_locked(ticket, CancelledBy::Code).await?;
            Ok(BoardError::refused(
                Refusal::MoveRefused,
                format!(
                    "{}'s validation was cancelled: the code changed",
                    ticket.key
                ),
            ))
        } else {
            self.discard(ticket).await?;
            Ok(BoardError::refused(
                Refusal::MoveRefused,
                format!("{}: {}", ticket.key, crate::domain::validation::DISCARDED),
            ))
        }
    }

    /// VAL-02: the validator changed tracked files or committed. Its verdict is thrown away; the ticket shows
    /// "Validation discarded: the validator changed files" with the no-result actions. Not a round.
    async fn discard(&self, ticket: &Ticket) -> Result<(), BoardError> {
        let step = self
            .step(ticket, ValidationEvent::ValidatorChangedFiles)
            .await?;
        // Which checker changed them can't be told: the whole round is discarded (d0130).
        self.stop_validator(ticket).await;
        self.inner
            .board
            .end_round(
                ticket,
                &step,
                "",
                Some(crate::domain::validation::DISCARDED),
                &Actor::current_user(),
            )
            .await?;
        tracing::warn!(key = %ticket.key, "validator changed files: verdict discarded");
        Ok(())
    }

    /// VAL-08: the code changed through the agent, or the user edited the description. Every checker of the round
    /// stops (0108 rule 9, d0130 row 17), the round doesn't count, the agent works again and is told so; clients get
    /// the cancel toast (UI-07). A verdict that arrives after it is refused.
    pub async fn cancel(&self, ticket: &Ticket, by: CancelledBy) -> Result<(), BoardError> {
        let lock = self.end_lock(&ticket.id);
        let _one_end = lock.lock().await;
        self.cancel_locked(ticket, by).await
    }

    async fn cancel_locked(&self, ticket: &Ticket, by: CancelledBy) -> Result<(), BoardError> {
        let step = self.step(ticket, ValidationEvent::CodeChanged).await?;
        let flow = self
            .inner
            .board
            .effective_flow(ticket.key.project.as_str())
            .await?;
        let check = check_name(&flow, &ticket.place);
        self.stop_validator(ticket).await;
        self.inner
            .board
            .end_round(ticket, &step, "", None, &Actor::current_user())
            .await?;
        self.forget_turns(&ticket.id);
        if step.tell_cancelled {
            self.tell_agent(ticket, &cancelled_message(&check), Start::Auto)
                .await;
        }
        let (title, message) = cancelled_toast(&ticket.key.to_string(), by);
        self.inner.events.publish(Event::ValidationCancelled {
            project: ticket.key.project.to_string(),
            key: ticket.key.clone(),
            title,
            message,
        });
        tracing::info!(key = %ticket.key, ?by, "validation cancelled");
        Ok(())
    }

    /// The rules' row for `event` where the ticket stands.
    async fn step(
        &self,
        ticket: &Ticket,
        event: ValidationEvent,
    ) -> Result<crate::board::validation::Transition, BoardError> {
        let facts = self.check_facts(ticket).await?;
        transition(facts.state(ticket), event)
            .map_err(|e| BoardError::refused(Refusal::MoveRefused, e.0))
    }

    /// A drag takes the card out of its agent check (FLW-18): its checker stops (or a held one is let go) and the
    /// open round ends Stopped (a drag back), or Skipped (forward: "Skipped Security · by you"), neither of which
    /// counts; the drag moves the card.
    pub(crate) async fn stop_for_drag(
        &self,
        ticket: &Ticket,
        skipped: bool,
    ) -> Result<(), BoardError> {
        let board = &self.inner.board;
        self.stop_validator(ticket).await;
        if ticket.status == Status::WaitingForLimitReset {
            board
                .store()
                .clear_agent_hold(HoldScope::Ticket(&ticket.id), SessionRole::Validator)
                .await?;
        }
        sqlx::query("UPDATE validations SET ended_at = ?, outcome = ? WHERE ticket_id = ? AND ended_at IS NULL")
            .bind(crate::store::now_ms())
            .bind(match skipped {
                true => crate::domain::ValidationOutcome::Skipped,
                false => crate::domain::ValidationOutcome::Stopped,
            }
            .as_str())
            .bind(&ticket.id)
            .execute(board.store().pool())
            .await?;
        self.forget_turns(&ticket.id);
        Ok(())
    }

    /// A fresh checker for a card a drag put in an agent check (FLW-18), once its round is open.
    pub(crate) async fn start_dragged_check(&self, ticket_id: &str) -> Result<(), BoardError> {
        self.forget_turns(ticket_id);
        self.start_held(ticket_id).await
    }

    /// Stops every checker of the ticket (d0130: one per check agent).
    async fn stop_validator(&self, ticket: &Ticket) {
        for session in self
            .inner
            .sessions
            .running_all_of(&ticket.id, SessionKind::Validator)
        {
            if let Err(e) = self.inner.sessions.stop(&session.id).await {
                tracing::warn!(key = %ticket.key, error = %e, "can't stop the validator");
            }
        }
    }

    /// Stops one checker, if it runs.
    async fn stop_session(&self, ticket: &Ticket, session: &str) {
        let running = self
            .inner
            .sessions
            .info(session)
            .is_ok_and(|info| info.is_running());
        if running && let Err(e) = self.inner.sessions.stop(session).await {
            tracing::warn!(key = %ticket.key, error = %e, "can't stop the validator");
        }
    }

    fn agent_took_a_turn(&self, ticket_id: &str) -> bool {
        self.inner
            .agent_turned
            .lock()
            .expect("turns")
            .contains(ticket_id)
    }

    fn forget_turns(&self, ticket_id: &str) {
        self.inner
            .agent_turned
            .lock()
            .expect("turns")
            .remove(ticket_id);
    }

    /// The agent's session started a turn (its prompt was submitted). While its ticket validates, that turn may
    /// change the code (VAL-08).
    pub async fn agent_working(&self, ticket_id: &str) {
        if let Ok(ticket) = self.inner.board.ticket_by_id(ticket_id).await
            && validating(ticket.status)
        {
            self.inner
                .agent_turned
                .lock()
                .expect("turns")
                .insert(ticket_id.to_owned());
        }
    }

    /// The agent's turn ended: if its ticket validates and tracked files or the head changed, the validation is
    /// cancelled (VAL-08).
    pub async fn agent_turn_ended(&self, ticket_id: &str) -> Result<(), BoardError> {
        let board = &self.inner.board;
        let ticket = board.ticket_by_id(ticket_id).await?;
        if !validating(ticket.status) || !self.agent_took_a_turn(ticket_id) {
            return Ok(());
        }
        let Some(head) = board.open_round(ticket_id).await?.and_then(|r| r.head) else {
            return Ok(());
        };
        if board.worktree_changed(&ticket, &head).await? {
            self.cancel(&ticket, CancelledBy::Code).await?;
        }
        Ok(())
    }

    /// The user saved a change to the ticket (SES-08). A description edit while it validates cancels the
    /// validation (VAL-08); a title-only edit doesn't.
    pub async fn ticket_edited(&self, before: &Ticket, after: &Ticket) -> Result<(), BoardError> {
        if validating(after.status) && before.body.trim() != after.body.trim() {
            self.cancel(after, CancelledBy::Description).await?;
        }
        Ok(())
    }

    /// The user's actions on a check (VAL-06, VAL-07, VAL-09). None of these routes to Review auto-merges.
    pub async fn user_action(
        &self,
        key: &str,
        action: ValidationAction,
        note: &str,
        actor: &Actor,
    ) -> Result<Ticket, BoardError> {
        let board = &self.inner.board;
        let ticket = board.ticket(key).await?;
        match action {
            ValidationAction::Stop => {
                let step = self.step(&ticket, ValidationEvent::StopValidation).await?;
                self.stop_validator(&ticket).await;
                board.end_round(&ticket, &step, "", None, actor).await?;
                self.forget_turns(&ticket.id);
            }
            ValidationAction::Skip => {
                if ticket.status == Status::WaitingForLimitReset {
                    self.may_skip_held(&ticket).await?;
                }
                let step = self.step(&ticket, ValidationEvent::SkipCheck).await?;
                self.stop_validator(&ticket).await;
                if ticket.status == Status::WaitingForLimitReset {
                    board
                        .store()
                        .clear_agent_hold(HoldScope::Ticket(&ticket.id), SessionRole::Validator)
                        .await?;
                }
                board.end_round(&ticket, &step, "", None, actor).await?;
                if let Some(check) = ticket.place.stage() {
                    board
                        .moved_past_check(&ticket.id, check, MovedPast::Skipped)
                        .await?;
                }
                self.forget_turns(&ticket.id);
                if step.start_validator {
                    self.start_next_check(&ticket.id, ValidationTrigger::User)
                        .await?;
                }
            }
            ValidationAction::MoveOnAnyway => {
                let step = self.step(&ticket, ValidationEvent::MoveOnAnyway).await?;
                let how = if ticket.status == Status::CheckFailed {
                    MovedPast::Failed
                } else {
                    MovedPast::Skipped
                };
                board.move_on_from_check(&ticket, &step, how, actor).await?;
                if step.start_validator {
                    self.start_next_check(&ticket.id, ValidationTrigger::User)
                        .await?;
                }
            }
            ValidationAction::AnotherRound | ValidationAction::AnswerAgent => {
                let event = if action == ValidationAction::AnotherRound {
                    ValidationEvent::AnotherRound
                } else {
                    ValidationEvent::AnswerAgent
                };
                let step = self.step(&ticket, event).await?;
                let findings = board.last_report(&ticket.id).await?.unwrap_or_default();
                board.back_to_agent(&ticket, &step, actor).await?;
                let after = board.ticket_by_id(&ticket.id).await?;
                let text = user_findings_message(&after, &findings, note, step.extra_round);
                // The user's Give it another round / Answer the agent: a click on this one agent (PAU-05).
                self.tell_agent(&after, &text, Start::User).await;
            }
            // A stopped Delivery's (FLW-17): back to where it is offered; Retry merge runs an automatic one again.
            ValidationAction::RetryMerge | ValidationAction::MoveToReview => {
                board.back_to_delivery(&ticket, actor).await?;
                if action == ValidationAction::RetryMerge {
                    board.delivery().arrived(&ticket.id).await;
                }
            }
            ValidationAction::Retry => {
                self.step(&ticket, ValidationEvent::RetryValidation).await?;
                let head = board.submit_head(&ticket).await?;
                board.reopen_round(&ticket, &head, actor).await?;
                self.forget_turns(&ticket.id);
                // Retry validation: the user's click on this one check (PAU-05).
                if let Err(e) = self.start_validator(&ticket.id, Start::User).await {
                    board
                        .validator_did_not_start(&ticket.id, &e.to_string())
                        .await?;
                }
            }
        }
        if matches!(
            action,
            ValidationAction::Skip | ValidationAction::MoveOnAnyway
        ) {
            // On into a Delivery: moved past a check, it waits for the click (FLW-15); the hook decides.
            board.delivery().arrived(&ticket.id).await;
        }
        board.ticket_by_id(&ticket.id).await
    }

    /// Skip this check on a check held for the plan limit is the user's only when no fallback can take it (Q277):
    /// none set, or the fallback at its limit too. Otherwise Use fallback is the way on.
    async fn may_skip_held(&self, ticket: &Ticket) -> Result<(), BoardError> {
        let hold = self
            .inner
            .board
            .store()
            .agent_hold(HoldScope::Ticket(&ticket.id), SessionRole::Validator)
            .await?;
        match hold {
            Some(hold) if skip_at_limit(&hold) => Ok(()),
            Some(hold) if hold.reason == HoldReason::ToolLimit => Err(BoardError::refused(
                Refusal::MoveRefused,
                format!(
                    "{}'s check can run on {}'s fallback: use the fallback instead",
                    ticket.key,
                    hold.tool.display_name()
                ),
            )),
            _ => Err(BoardError::refused(
                Refusal::MoveRefused,
                format!(
                    "{}'s check isn't waiting for its tool's plan limit",
                    ticket.key
                ),
            )),
        }
    }

    /// The epic lead's one note to a child whose "Validation failed N times" waits for it (LEAD-07, d0083): it
    /// counts as Answer the agent (VAL-06). The round counter restarts at 0 and the note goes to the agent with the
    /// last findings. The child is at work already (its stop was held there).
    pub async fn lead_answer(
        &self,
        key: &str,
        lead: &str,
        note: &str,
    ) -> Result<Ticket, BoardError> {
        let board = &self.inner.board;
        let ticket = board.ticket(key).await?;
        let facts = self.check_facts(&ticket).await?;
        // The child is at work already (its stop was held there): the row is the failed check's, the one that
        // counted last (the lead's note is per check stage, d0083).
        let Some(check) = facts.counts.counting(&facts.flow, &ticket.place) else {
            return Err(BoardError::refused(
                Refusal::MoveRefused,
                format!("{} has no failed check to answer", ticket.key),
            ));
        };
        let place = Place::stage_of(&check.id);
        let failed = ValidationState {
            place: &place,
            status: Status::CheckFailed,
            ..facts.state(&ticket)
        };
        let step = transition(failed, ValidationEvent::AnswerAgent)
            .map_err(|e| BoardError::refused(Refusal::MoveRefused, e.0))?;
        debug_assert_eq!(step.counters, Counters::ResetThis);
        board.reset_check_counter(&ticket.id, &check.id).await?;
        let findings = board.last_report(&ticket.id).await?.unwrap_or_default();
        let after = board.ticket_by_id(&ticket.id).await?;
        self.tell_agent(
            &after,
            &lead_findings_message(&after, &findings, lead, note),
            Start::Auto,
        )
        .await;
        Ok(after)
    }

    /// Resume of a ticket interrupted while it validated (REC-01, VAL-07): a fresh validator on a new row with
    /// the same round number (not counted); the old validator conversation is never resumed and the agent's
    /// session is left alone. `start` is who asked (the user's Resume, or Resume all).
    pub async fn resume_interrupted(
        &self,
        ticket_id: &str,
        start: Start,
    ) -> Result<(), BoardError> {
        let board = &self.inner.board;
        let ticket = board.ticket_by_id(ticket_id).await?;
        let head = board.submit_head(&ticket).await?;
        board
            .reopen_round(&ticket, &head, &Actor::current_user())
            .await?;
        self.forget_turns(ticket_id);
        if let Err(e) = self.start_validator(ticket_id, start).await {
            board
                .validator_did_not_start(ticket_id, &e.to_string())
                .await?;
        }
        tracing::info!(key = %ticket.key, "validation resumed: a fresh validator");
        Ok(())
    }

    /// Park and Close stop the agent and the validator: the validation is discarded, recorded as stopped
    /// (CLS-01, CLS-06).
    pub async fn abandon(&self, ticket_id: &str) -> Result<(), BoardError> {
        for session in self
            .inner
            .sessions
            .running_all_of(ticket_id, SessionKind::Validator)
        {
            self.inner.sessions.stop(&session.id).await.map_err(other)?;
        }
        if self
            .inner
            .board
            .abandon_round(ticket_id, crate::domain::ValidationOutcome::Stopped)
            .await?
        {
            tracing::info!(ticket = ticket_id, "validation stopped with its ticket");
        }
        self.forget_turns(ticket_id);
        Ok(())
    }

    /// How a gone agent is resumed (the service registers Review's resume).
    pub fn on_resume_agent(&self, hook: AgentResume) {
        *self.inner.resume_agent.lock().expect("resume agent") = Some(hook);
    }

    /// Queues `text` for the ticket's agent (SES-06 rules: typed once it is idle and its input line is empty).
    /// An agent whose session is gone (a restart ended it) is resumed with `text` as its first prompt: the only
    /// time a validation resumes the agent (VAL-07); an automatic one (`start`) waits while the project is paused.
    async fn tell_agent(&self, ticket: &Ticket, text: &str, start: Start) {
        let inner = &self.inner;
        let Some(session) = inner.sessions.running_of(&ticket.id, SessionKind::Agent) else {
            let hook = inner.resume_agent.lock().expect("resume agent").clone();
            match hook {
                Some(hook) => {
                    if let Err(e) = hook(ticket.id.clone(), text.to_owned(), start).await {
                        tracing::warn!(key = %ticket.key, error = %e, "can't resume the agent");
                    }
                }
                None => tracing::warn!(key = %ticket.key, "no agent session to tell"),
            }
            return;
        };
        if let Err(e) = inner
            .deliveries
            .queue_delivery(&session.id, DeliveryKind::Validation, text)
            .await
        {
            tracing::warn!(key = %ticket.key, error = %e, "can't queue the validation message");
        }
    }

    /// The validator's turn ended: once its round has a verdict, it is done and its session ends (VAL-03).
    pub async fn after_validator_turn(&self, session: &str) -> Result<(), BoardError> {
        if self.inner.board.round_of_session_ended(session).await? {
            self.inner.sessions.stop(session).await.map_err(other)?;
        }
        Ok(())
    }

    /// A hook of the ticket's validator (SDD §11.4 M12 rows, VAL-07, VAL-11): a permission prompt is "Needs you ·
    /// validator"; the user typing into it makes it Validating… again; its turn ending without `validate
    /// pass|fail` is "Validation ended without a result" (or "discarded" when it changed files); the usage limit
    /// is "Waiting for limit reset" until its tool's Resume starts a fresh validator. Otaq never nudges it (d0018).
    pub async fn validator_signal(
        &self,
        ticket_id: &str,
        session: &str,
        signal: &Signal,
    ) -> Result<(), BoardError> {
        let board = &self.inner.board;
        if signal.ends_turn() && board.round_of_session_ended(session).await? {
            return self.after_validator_turn(session).await;
        }
        let lock = self.end_lock(ticket_id);
        let _one_end = lock.lock().await;
        // One of the open round's checkers (d0130: one per check agent).
        let Some((round, row)) = board.open_round(ticket_id).await?.and_then(|round| {
            let row = round.open_row_of(session)?.id.clone();
            Some((round, row))
        }) else {
            return Ok(());
        };
        let head = round.head.clone();
        let ticket = board.ticket_by_id(ticket_id).await?;
        match signal {
            Signal::NeedsYou { prompt } => {
                board
                    .set_validation_status(
                        ticket_id,
                        &[Status::Checking],
                        Status::CheckerNeedsYou,
                        Some(prompt),
                        false,
                    )
                    .await?;
            }
            Signal::Working { .. } => {
                board
                    .set_validation_status(
                        ticket_id,
                        &[Status::CheckerNeedsYou],
                        Status::Checking,
                        None,
                        false,
                    )
                    .await?;
            }
            Signal::Idle { .. } | Signal::TurnEnded if validating(ticket.status) => {
                // No verdict this turn.
                if let Some(head) = head
                    && board.worktree_changed(&ticket, &head).await?
                {
                    let _ = self.changed_under_validation(&ticket).await?;
                    return Ok(());
                }
                // Its row ends without one; the round waits for the other check agents (d0130).
                self.stop_session(&ticket, session).await;
                let facts = self.check_facts(&ticket).await?;
                let end = RowEnd {
                    row: &row,
                    outcome: ValidationOutcome::NoResult,
                    reason: "",
                    hand_over: None,
                };
                self.row_ended(&ticket, &facts, &round, end, None, &Actor::current_user())
                    .await?;
            }
            Signal::LimitReached { .. } if validating(ticket.status) => {
                board
                    .set_validation_status(
                        ticket_id,
                        &[Status::Checking, Status::CheckerNeedsYou],
                        Status::WaitingForLimitReset,
                        None,
                        true,
                    )
                    .await?;
            }
            _ => {}
        }
        Ok(())
    }

    /// The tool refused the validator's model (AGT-05, SDD §7.2 `[M22]` "No result: validator's model", VAL-07): the
    /// round ends as No result with the tool's message and **doesn't count** (the round number stays; the ticket
    /// stays in Dev and doesn't go back to its developer). Change validator, Retry validation and Move to Review
    /// anyway are the user's; Otaq never retries (d0018). The validator's session ends as [`END_MODEL_REFUSED`].
    /// Whether the open round was this session's.
    pub async fn model_refused(
        &self,
        ticket_id: &str,
        session: &str,
        model: &str,
        message: &str,
    ) -> Result<bool, BoardError> {
        let board = &self.inner.board;
        let lock = self.end_lock(ticket_id);
        let one_end = lock.lock().await;
        let own = board.open_round(ticket_id).await?.and_then(|round| {
            let row = round.open_row_of(session)?.id.clone();
            Some((round, row))
        });
        let ticket = board.ticket_by_id(ticket_id).await?;
        let ended = match own {
            Some((round, row)) if validating(ticket.status) => {
                // Its row ends without a verdict; with other check agents the round waits for them (d0130).
                let facts = self.check_facts(&ticket).await?;
                let text = couldnt_run_text(model, message);
                let end = RowEnd {
                    row: &row,
                    outcome: ValidationOutcome::NoResult,
                    reason: &text,
                    hand_over: None,
                };
                self.row_ended(
                    &ticket,
                    &facts,
                    &round,
                    end,
                    Some(&text),
                    &Actor::current_user(),
                )
                .await?
            }
            _ => false,
        };
        drop(one_end);
        if ended {
            tracing::warn!(key = %ticket.key, %message, "the tool refused the validator's model: no result");
            let after = board.ticket_by_id(ticket_id).await?;
            let project = ticket.key.project.to_string();
            self.inner.events.publish(Event::TicketChanged {
                project: project.clone(),
                id: ticket.id.clone(),
                key: ticket.key.clone(),
            });
            if after.status != ticket.status {
                self.inner.events.publish(Event::CardStatusChanged {
                    project,
                    id: ticket.id.clone(),
                    key: ticket.key.clone(),
                    status: after.status,
                });
            }
        }
        if let Err(e) = self
            .inner
            .sessions
            .stop_as(session, END_MODEL_REFUSED)
            .await
        {
            tracing::warn!(key = %ticket.key, error = %e, "can't end the refused validator");
        }
        Ok(ended)
    }

    /// The user's **Move to next stage** (CLS-05, d0108 rule 10): the agent's own `stage next`, started by the user
    /// when the agent isn't mid-turn. Into an agent check the check runs (and an automatic Delivery may follow its pass);
    /// anything else is [`Board::submit_by_user`]. The note, if any, is the user's comment (CMT-05); History says
    /// "Submitted by you".
    pub async fn submit_by_user(
        &self,
        key: &str,
        note: &str,
        actor: &Actor,
    ) -> Result<Ticket, BoardError> {
        let board = &self.inner.board;
        let ticket = board.ticket(key).await?;
        crate::board::review::user_may_submit(&ticket)?;
        let hand_over = board.user_hand_over(&ticket, note, None, actor).await?;
        let result = async {
            if !self.next_is_agent_check(&ticket).await? {
                let after = board.submit_by_user(key, hand_over.as_ref(), actor).await?;
                // Into a Delivery: automatic only after passed checks (none walked over, FLW-15).
                board.delivery().arrived(&after.id).await;
                return board.ticket_by_id(&after.id).await;
            }
            self.submit(key, hand_over.as_ref(), actor, ValidationTrigger::User)
                .await
        }
        .await;
        settled(board, hand_over.as_ref(), result).await
    }

    /// Whether the ticket's next stage is an agent check (STG-06).
    async fn next_is_agent_check(&self, ticket: &Ticket) -> Result<bool, BoardError> {
        let flow = self
            .inner
            .board
            .effective_flow(ticket.key.project.as_str())
            .await?;
        let skips = self.inner.board.ticket_skips(ticket, &flow).await?;
        Ok(crate::flow::engine::step(
            crate::flow::engine::At {
                skips: &skips,
                ..crate::flow::engine::At::new(&flow, &ticket.place, Status::Working)
            },
            crate::flow::engine::Move::NextStage,
        )
        .ok()
        .and_then(|step| step.stage)
        .is_some_and(crate::flow::Stage::is_agent_check))
    }

    /// The agent's `stage next` (or `review submit`; with `trigger` user, the user's Move to next stage): into an
    /// agent check, Validating… and a checker; anything else (Ready for Review when validation is off, another Agent
    /// stage) is [`Board::submit_for_review`]. The summary (or the user's note) is `hand_over`'s comment, posted with
    /// the move (CMT-05); the caller discards its report when this fails ([`settled`]).
    pub async fn submit(
        &self,
        key: &str,
        hand_over: Option<&HandOver>,
        actor: &Actor,
        trigger: ValidationTrigger,
    ) -> Result<Ticket, BoardError> {
        let board = &self.inner.board;
        let ticket = board.ticket(key).await?;
        // History's line: the agent's summary, or "Submitted by you".
        let summary = match trigger {
            ValidationTrigger::Agent => hand_over.map_or("", |h| h.body.as_str()),
            ValidationTrigger::User => crate::board::review::SUBMITTED_BY_YOU,
        };
        // The agent's "next stage" (STG-06): an agent check validates; anything else (Validation off: Review) is
        // the plain submit.
        if !self.next_is_agent_check(&ticket).await? {
            let after = board.submit_for_review(key, hand_over, actor).await?;
            // Straight into an automatic Delivery (a flow with no check before it, FLW-15).
            board.delivery().arrived(&after.id).await;
            return board.ticket_by_id(&after.id).await;
        }
        // The user's Move to next stage checked the statuses it starts from (Interrupted too).
        if trigger == ValidationTrigger::Agent
            && !LIVE.contains(&ticket.status)
            && ticket.status != Status::MergeConflictFixing
        {
            return Err(BoardError::refused(
                Refusal::MoveRefused,
                format!(
                    "{} is {}; only a ticket whose agent is working can be submitted",
                    ticket.key,
                    ticket.status.label()
                ),
            ));
        }
        let event = match trigger {
            ValidationTrigger::Agent => ValidationEvent::AgentSubmit,
            ValidationTrigger::User => ValidationEvent::UserNextStage,
        };
        let facts = self.check_facts(&ticket).await?;
        let step = transition(facts.state(&ticket), event)
            .map_err(|e| BoardError::refused(Refusal::MoveRefused, e.0))?;
        let head = board.submit_head(&ticket).await?;
        board
            .enter_validation(
                &ticket, &step, &head, summary, hand_over, trigger, false, actor,
            )
            .await?;
        self.forget_turns(&ticket.id);
        // Automatic, even after the user's Move to next stage: the check isn't the agent the user clicked (PAU-04).
        if let Err(e) = self.start_validator(&ticket.id, Start::Auto).await {
            // Shown, never retried by itself (P2): Retry validation is the user's click.
            tracing::warn!(key = %ticket.key, error = %e, "the validator didn't start");
            board
                .validator_did_not_start(&ticket.id, &e.to_string())
                .await?;
        }
        board.ticket_by_id(&ticket.id).await
    }

    /// A validator's hold lifted (T-0357): the open round's validator starts, checked once more; a start that fails
    /// ends the round without a result, as a submit's would.
    pub async fn start_held(&self, ticket_id: &str) -> Result<(), BoardError> {
        // Started meanwhile (the tool's Resume): kept, not started again.
        if let Err(e) = self.start_waiting_checker(ticket_id, Start::Auto).await {
            tracing::warn!(ticket = ticket_id, error = %e, "the held validator didn't start");
            self.inner
                .board
                .validator_did_not_start(ticket_id, &e.to_string())
                .await?;
        }
        Ok(())
    }

    /// The round waits for its held validator: "Waiting for limit reset" for a tool at its limit (marked as a
    /// validation that waits), Validating… otherwise.
    async fn hold_round(&self, ticket: &Ticket, held: &Held) -> Result<(), BoardError> {
        let limit = held.blocked == crate::agents::gate::Blocked::AtLimit;
        let (from, to, interrupted_from) = if limit {
            (
                Status::Checking,
                Status::WaitingForLimitReset,
                Some(Status::Checking.as_str()),
            )
        } else {
            (Status::WaitingForLimitReset, Status::Checking, None)
        };
        let now = crate::store::now_ms();
        sqlx::query(concat!(
            "UPDATE tickets SET state = ?, interrupted_from = ?, updated_at = ?, ",
            set_state_since!(),
            " WHERE id = ? AND state = ?"
        ))
        .bind(to.as_str())
        .bind(interrupted_from)
        .bind(now)
        .bind(to.as_str())
        .bind(now)
        .bind(&ticket.id)
        .bind(from.as_str())
        .execute(self.inner.board.store().pool())
        .await?;
        let project = ticket.key.project.to_string();
        self.inner.events.publish(Event::CardStatusChanged {
            project,
            id: ticket.id.clone(),
            key: ticket.key.clone(),
            status: if ticket.status == from {
                to
            } else {
                ticket.status
            },
        });
        tracing::info!(key = %ticket.key, message = held.message(), "the validator is held");
        Ok(())
    }

    /// Starts the validator of the ticket's open round (a submit, Retry validation, a tool's Resume after its limit, a
    /// lifted hold), after the check before every launch: held, the round waits without one. Any validator still
    /// running for the ticket is stopped first: one per ticket.
    pub async fn start_validator(
        &self,
        ticket_id: &str,
        start: Start,
    ) -> Result<ValidatorStart, BoardError> {
        let lock = self.start_lock(ticket_id);
        let _one_start = lock.lock().await;
        self.start_validator_locked(ticket_id, start).await
    }

    /// Starts the checkers of a round that waits for them (a lifted hold, a tool's Resume): the check agents whose
    /// checker already runs keep it, so the first of two overlapping starts wins and the second keeps its checkers.
    async fn start_waiting_checker(
        &self,
        ticket_id: &str,
        start: Start,
    ) -> Result<ValidatorStart, BoardError> {
        let lock = self.start_lock(ticket_id);
        let _one_start = lock.lock().await;
        self.start_validator_locked(ticket_id, start).await
    }

    fn start_lock(&self, ticket_id: &str) -> Arc<tokio::sync::Mutex<()>> {
        self.inner
            .starting
            .lock()
            .expect("starting")
            .entry(ticket_id.to_owned())
            .or_default()
            .clone()
    }

    fn end_lock(&self, ticket_id: &str) -> Arc<tokio::sync::Mutex<()>> {
        self.inner
            .ending
            .lock()
            .expect("ending")
            .entry(ticket_id.to_owned())
            .or_default()
            .clone()
    }

    /// Starts the open round's checkers (d0130): one fresh read-only session per check agent, together, in the
    /// ticket's worktree, each on its own agent and model. A check agent whose checker already runs keeps it; any
    /// other checker of the ticket stops first. Each goes through the check before every launch: a held one leaves
    /// its row waiting (the round waits for it). One that can't start ends its row without a verdict while the others
    /// run; when none starts, the error is the caller's (the round ends without a result).
    async fn start_validator_locked(
        &self,
        ticket_id: &str,
        start: Start,
    ) -> Result<ValidatorStart, BoardError> {
        let inner = &self.inner;
        let board = &inner.board;
        let ticket = board.ticket_by_id(ticket_id).await?;
        if self.kept(&ticket, start).await? {
            return Ok(ValidatorStart::Kept);
        }
        let flow = board.effective_flow(ticket.key.project.as_str()).await?;
        let skips = board.ticket_skips(&ticket, &flow).await?;
        let stage = resolve::check_stage_of(&flow, &ticket, &skips).cloned();
        if let Some(stage) = &stage {
            board.name_round_agents(&ticket, stage).await?;
        }
        let round = board.open_round(&ticket.id).await?;
        let running = inner
            .sessions
            .running_all_of(&ticket.id, SessionKind::Validator);
        let runs = |row: &crate::board::validation::RoundRow| {
            row.session
                .as_ref()
                .is_some_and(|s| running.iter().any(|r| &r.id == s))
        };
        // Each open row's check agent, without a running checker; a round-less start (tests) runs one checker.
        let (keep, todo): (Vec<_>, Vec<_>) = match &round {
            Some(round) => round.open().partition(|row| runs(row)),
            None => (Vec::new(), Vec::new()),
        };
        let todo: Vec<(Option<String>, Option<String>)> = match &round {
            Some(_) => todo
                .into_iter()
                .map(|row| (Some(row.id.clone()), row.agent.clone()))
                .collect(),
            None => vec![(None, None)],
        };
        let kept: Vec<&SessionInfo> = running
            .iter()
            .filter(|s| {
                keep.iter()
                    .any(|row| row.session.as_deref() == Some(s.id.as_str()))
            })
            .collect();
        for old in running
            .iter()
            .filter(|s| !kept.iter().any(|k| k.id == s.id))
        {
            inner.sessions.stop(&old.id).await.map_err(other)?;
        }
        let mut started: Vec<SessionInfo> = kept.into_iter().cloned().collect();
        let mut held: Option<Box<Held>> = None;
        let mut failed: Vec<(String, BoardError)> = Vec::new();
        let mut first_error: Option<BoardError> = None;
        for (row, name) in todo {
            match self
                .start_one(
                    &ticket,
                    stage.as_ref(),
                    row.as_deref(),
                    name.as_deref(),
                    start,
                )
                .await
            {
                Ok(ValidatorStart::Started(session)) => started.push(*session),
                Ok(ValidatorStart::Held(h)) => {
                    held.get_or_insert(h);
                }
                Ok(ValidatorStart::Kept) => return Ok(ValidatorStart::Kept),
                Err(e) => match row {
                    Some(row) => failed.push((row, e)),
                    None => first_error = Some(e),
                },
            }
        }
        if let Some(e) = first_error {
            return Err(e);
        }
        if started.is_empty() && held.is_none() {
            if let Some((_, e)) = failed.into_iter().next() {
                return Err(e);
            }
            return Ok(ValidatorStart::Kept);
        }
        // The others run: each one that couldn't start ends its own row without a verdict (Retry is the user's).
        if let Some(round) = &round {
            for (row, e) in &failed {
                tracing::warn!(key = %ticket.key, error = %e, "a check agent's checker didn't start");
                let reason = format!("The validator didn't start: {e}");
                let end = RowEnd {
                    row,
                    outcome: ValidationOutcome::NoResult,
                    reason: &reason,
                    hand_over: None,
                };
                board
                    .end_in_round(&ticket, round, Some(end), None, &Actor::current_user())
                    .await?;
            }
        }
        if let Some(held) = held {
            self.hold_round(&ticket, &held).await?;
            if started.is_empty() {
                return Ok(ValidatorStart::Held(held));
            }
        }
        Ok(ValidatorStart::Started(Box::new(started.remove(0))))
    }

    /// One check agent's checker (`name`, as the stage lists it; `None`: the ticket's check agent) for its row of
    /// the round (`row`), after the check before every launch.
    async fn start_one(
        &self,
        ticket: &Ticket,
        stage: Option<&crate::flow::Stage>,
        row: Option<&str>,
        name: Option<&str>,
        start: Start,
    ) -> Result<ValidatorStart, BoardError> {
        let inner = &self.inner;
        let board = &inner.board;
        // Every round resolves its agent again: the agent's model may change between rounds (AGM-05, d0088).
        let agent = match (name, stage) {
            (Some(name), Some(stage)) => {
                resolve::named(board.store(), name).await?.ok_or_else(|| {
                    BoardError::refused(
                        Refusal::UnknownAgent,
                        crate::flow::check::unknown_agent_message(stage, name),
                    )
                })?
            }
            _ => resolve::session_agent(board, ticket, SessionRole::Validator).await?,
        };
        let runs_on = match inner.gate.get() {
            Some(gate) => match gate
                .before_launch(SessionRole::Validator, Scope::ticket(ticket), &agent)
                .await?
            {
                Gated::Launch(runs_on) => runs_on,
                Gated::Held(held) => return Ok(ValidatorStart::Held(held)),
            },
            None => RunsOn::agent(&agent, board.store().default_model().await?.as_ref()),
        };
        match self
            .launch_validator(ticket, runs_on, &agent, row, start)
            .await
        {
            Ok(session) => Ok(ValidatorStart::Started(Box::new(session))),
            // The backstop refused it: the project was paused meanwhile.
            Err(_) if self.kept(ticket, start).await? => Ok(ValidatorStart::Kept),
            Err(e) => {
                // A launch that failed because of its tool: checked again, then held if it can't start now (ADP-10).
                if let Some(gate) = inner.gate.get()
                    && let Some(held) = gate
                        .after_failed_launch(SessionRole::Validator, Scope::ticket(ticket), &agent)
                        .await?
                {
                    return Ok(ValidatorStart::Held(Box::new(held)));
                }
                Err(e)
            }
        }
    }

    async fn launch_validator(
        &self,
        ticket: &Ticket,
        runs_on: RunsOn,
        agent: &crate::agents::Agent,
        row: Option<&str>,
        start: Start,
    ) -> Result<SessionInfo, BoardError> {
        let inner = &self.inner;
        let board = &inner.board;
        inner.adapters.unavailable(runs_on.tool)?;
        let worktree = ticket
            .work
            .as_ref()
            .and_then(|w| w.worktree.clone())
            .map(PathBuf::from)
            .ok_or_else(|| {
                BoardError::refused(
                    Refusal::MoveRefused,
                    format!("{} has no worktree to validate", ticket.key),
                )
            })?;
        // The check's own skill (Q261, every check agent's: Q516) and every skill of its agent, written into the
        // worktree first by one name (several check agents share the copies, d0118)
        // (AGM-07): only ignored paths change, so the round's recorded head and tracked files stay as they were
        // (VAL-02).
        let flow = board.effective_flow(ticket.key.project.as_str()).await?;
        let skips = board.ticket_skips(ticket, &flow).await?;
        let skill = resolve::check_stage_of(&flow, ticket, &skips)
            .and_then(|stage| stage.skill.clone())
            .unwrap_or_else(|| {
                crate::board::settings::WorkflowStep::Validate
                    .own_skill()
                    .to_owned()
            });
        let first_prompt = board
            .copy_for_session(
                &worktree,
                inner.adapters.shipped_skills(),
                Some(agent),
                Some(FirstPrompt::Skill {
                    name: skill,
                    message: None,
                }),
                crate::skills::Marker::Worktree,
            )
            .await
            .0;
        let launched = inner.adapters.launch(
            runs_on.tool,
            &LaunchRequest {
                role: LaunchRole::Validator,
                cwd: worktree,
                project: ticket.key.project.to_string(),
                ticket: Some(ticket.key.clone()),
                epic: None,
                port: 0,
                first_prompt,
                resume: None,
                model: runs_on.model,
                effort: runs_on.effort.clone(),
                title: Some(ticket.title.clone()),
                theme: board.theme_in_use().await,
            },
        )?;
        let project = board.project(ticket.key.project.as_str()).await?;
        let session = inner
            .sessions
            .prepared(launched.prepare)
            .spawn(SpawnSpec {
                kind: SessionKind::Validator,
                start,
                project_id: project.id,
                project_key: project.key,
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
            .map_err(other)?;
        board
            .validator_started(&ticket.id, row, &session.id)
            .await?;
        tracing::info!(key = %ticket.key, session = %session.id, "validator started");
        Ok(session)
    }

    /// A tool's Resume after its usage limit (SCH-04): starts the validators that waited for a reset. Returns their
    /// tickets' ids.
    pub async fn start_waiting(&self) -> Result<Vec<String>, BoardError> {
        let waiting = self.inner.board.validations_waiting_for_limit().await?;
        let mut started = Vec::new();
        for id in waiting {
            match self.start_waiting_checker(&id, Start::Auto).await {
                Ok(ValidatorStart::Started(_)) => started.push(id),
                Ok(ValidatorStart::Held(_) | ValidatorStart::Kept) => {}
                Err(e) => {
                    tracing::warn!(ticket = %id, error = %e, "can't start a waiting validator")
                }
            }
        }
        Ok(started)
    }
}

/// What the agent is told after a failed round of several check agents (VAL-04, d0130): each agent's verdict in the
/// stage's order, the reports being their comments. "Validate failed round 1: Security reviewer failed, Validator
/// passed. Run `otaq prime`."
pub fn checks_failed_text(
    check: &str,
    round: u32,
    verdicts: &[(String, Option<ValidationOutcome>)],
) -> String {
    let each: Vec<String> = verdicts
        .iter()
        .map(|(agent, outcome)| verdict_of(agent, *outcome))
        .collect();
    format!(
        "{check} failed round {round}: {}. Run `{} prime`.",
        each.join(", "),
        crate::naming::COMMAND_NAME
    )
}

/// The card's text for a round of several check agents that ended without a verdict (d0130), naming who gave none:
/// "Performance reviewer ended without a verdict", "Security reviewer and Validator ended without a verdict".
fn no_verdict_text(verdicts: &[(String, Option<ValidationOutcome>)]) -> String {
    let none: Vec<&str> = verdicts
        .iter()
        .filter(|(_, o)| !matches!(o, Some(ValidationOutcome::Pass | ValidationOutcome::Fail)))
        .map(|(agent, _)| agent.as_str())
        .collect();
    let names = match none.split_last() {
        Some((last, [])) => (*last).to_owned(),
        Some((last, rest)) => format!("{} and {last}", rest.join(", ")),
        None => "The check".to_owned(),
    };
    format!("{names} ended without a verdict")
}

/// What the agent gets on Give it another round or Answer the agent: the user's note first, then the findings.
fn user_findings_message(ticket: &Ticket, findings: &str, note: &str, extra: bool) -> String {
    let mut text = String::new();
    if !note.trim().is_empty() {
        text.push_str(&format!("The user answered: {}\n\n", note.trim()));
    }
    let what = if extra {
        "The user gave you another round"
    } else {
        "The validation's findings"
    };
    text.push_str(&format!(
        "{what} for {}. Fix these, commit, and submit again:\n\n{}",
        ticket.key,
        findings.trim()
    ));
    text
}

/// What the agent gets from the lead's note after its last round failed (LEAD-07): the note first, then the
/// findings.
fn lead_findings_message(ticket: &Ticket, findings: &str, lead: &str, note: &str) -> String {
    format!(
        "Note from {lead}: {}\n\nThe validation's findings for {}. Fix these, commit, and submit again:\n\n{}",
        note.trim(),
        ticket.key,
        findings.trim()
    )
}

/// Whether a hold offers **Skip this check** (FLW-13, Q277): an agent check's checker held for its tool's plan limit
/// with no usable fallback (none set, or the fallback at its limit too). An Agent stage held for the limit never
/// offers it, nor does any other hold.
pub fn skip_at_limit(hold: &AgentHold) -> bool {
    hold.role == SessionRole::Validator
        && hold.reason == HoldReason::ToolLimit
        && hold.fallback_tool.is_none()
}

/// What the rules need about a ticket's agent check: the flow, the stages it skips, and the counter of the check it
/// counts rounds for ([`CheckCounts::counting`]): its fails in a row and its rounds with the extra ones.
struct CheckFacts {
    flow: crate::flow::Flow,
    skips: Vec<String>,
    fails: u32,
    rounds: u32,
    counts: CheckCounts,
}

impl CheckFacts {
    fn state<'a>(&'a self, ticket: &'a Ticket) -> ValidationState<'a> {
        ValidationState {
            flow: &self.flow,
            place: &ticket.place,
            status: ticket.status,
            fails: self.fails,
            rounds: self.rounds,
            skips: &self.skips,
        }
    }
}

/// Validating…, or its validator waits on a prompt.
fn validating(status: Status) -> bool {
    matches!(status, Status::Checking | Status::CheckerNeedsYou)
}

fn other(e: impl std::fmt::Display) -> BoardError {
    BoardError::refused(Refusal::SessionEnded, e.to_string())
}
