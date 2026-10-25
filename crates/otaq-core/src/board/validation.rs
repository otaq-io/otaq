//! The M12 rows of SDD §7.3 as one pure function (d0063): validation before Review, on any agent check of a
//! project's flow (STG-06).
//!
//! [`transition`] takes where a ticket stands ([`ValidationState`]) and what happened ([`ValidationEvent`]) and
//! says where it goes, how it entered Review, what happens to the check counters, and whether the validator's
//! findings go to the agent. Where the ticket goes is the flow engine's ([`crate::flow::engine::step`]); this adds
//! the validation's own bookkeeping. What follows a pass into an automatic Delivery is the Delivery's
//! ([`super::merge`]).
//! It does no I/O: the service carries the result out (T-0183 … T-0189), and every row is tested on its own.
//!
//! **Each agent check counts its own fails in a row** (FLW-13, Q259; d0108 "After a Fail"): one row per ticket and
//! check stage in `check_counters` ([`CheckCounts`]), driven by the engine's [`Counters`]: a verdict counts on its
//! check, Answer the agent resets that check's, a human Fail or a drag back resets them all
//! ([`Board::reset_all_check_counters`]). The user moving a card past a check (Move on anyway, Skip this check) is
//! stored on its row too, and sends the card to a click-mode Delivery ([`Board::forced_click_reason`]).

use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::{Sqlite, Transaction};

use super::hand_over::HandOver;
use super::stage_runs;
use super::tickets::{EventKind, Ticket, record_event};
use super::{Board, BoardError, Refusal};
use crate::domain::{
    Actor, MovedPast, Place, ReviewEntry, Status, ValidationOutcome, ValidationTrigger,
};
use crate::flow::engine::{self, At, Counters, Move, RunResult};
use crate::flow::{Flow, Stage};
use crate::store::tickets::set_state_since;
use crate::store::{new_id, now_ms};

/// What the rules need to know about a ticket.
#[derive(Debug, Clone, Copy)]
pub struct ValidationState<'a> {
    /// The project's flow (the derived flow until M33: Validation off has no Validate stage).
    pub flow: &'a Flow,
    pub place: &'a Place,
    pub status: Status,
    /// The fails in a row of the check the ticket is in, or goes to ([`CheckCount::fails`]).
    pub fails: u32,
    /// The check's rounds plus the extra rounds the user allowed on it.
    pub rounds: u32,
    /// The stages the ticket skips by its choices ([`Board::ticket_skips`], AGM-08).
    pub skips: &'a [String],
}

/// What happened.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ValidationEvent {
    /// The agent's `review submit`.
    AgentSubmit,
    /// The user's Move to next stage: the agent's own `stage next` (d0108 rule 10), also from Interrupted.
    UserNextStage,
    Pass,
    Fail,
    /// The validator's turn ended without `check pass|fail`.
    NoVerdict,
    /// The validator changed tracked files or committed (VAL-02).
    ValidatorChangedFiles,
    /// The validator waits on a permission prompt.
    ValidatorNeedsYou,
    /// The user typed into the waiting validator.
    ValidatorContinues,
    /// Tracked files or the head changed through the agent, or the user edited the description (VAL-08).
    CodeChanged,
    /// Stop this check: the checker stops, the round doesn't count, back to the agent.
    StopValidation,
    /// Skip this check: on a running check, or on one held for the plan limit (the caller checked that no fallback
    /// can take it, Q277). On to the next stage; its Delivery in click mode.
    SkipCheck,
    /// After the last allowed fail or no result: on to the engine's next stage, whatever its kind (d0108).
    MoveOnAnyway,
    AnotherRound,
    AnswerAgent,
    RetryValidation,
}

/// The result of a row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Transition {
    pub to: Status,
    /// Where the ticket is afterwards (its stage).
    pub place: Place,
    /// Set when the ticket enters Ready for Review.
    pub entry: Option<ReviewEntry>,
    /// What the check counters do: the round that ended counts on its check, or they restart.
    pub counters: Counters,
    /// How the open validation round ends, if one does.
    pub outcome: Option<ValidationOutcome>,
    /// A fresh validator starts (submit, Retry validation).
    pub start_validator: bool,
    /// The findings go to the agent as an automatic message (VAL-04, Give it another round, Answer the agent).
    pub deliver_findings: bool,
    /// The agent is told the validation was cancelled because the code changed (VAL-08).
    pub tell_cancelled: bool,
    /// One more round is allowed (Give it another round).
    pub extra_round: bool,
    /// Stages walked over without running (a skip, or `Move::SubmitByUser` past them): each gets a skipped run.
    pub skipped: Vec<String>,
}

impl Transition {
    fn at(to: Status, place: Place) -> Self {
        Self {
            to,
            place,
            entry: None,
            counters: Counters::Keep,
            outcome: None,
            start_validator: false,
            deliver_findings: false,
            tell_cancelled: false,
            extra_round: false,
            skipped: Vec::new(),
        }
    }

    /// Where the engine's step goes.
    fn of(step: &engine::Step<'_>) -> Self {
        Self {
            start_validator: step.start_check,
            counters: step.counters,
            skipped: step
                .passed_through
                .iter()
                .filter(|(_, result)| *result == RunResult::Skipped)
                .map(|(stage, _)| stage.id.clone())
                .collect(),
            ..Self::at(step.status, step.place.clone())
        }
    }

    /// Staying where the ticket is, with another status.
    fn stay(s: &ValidationState<'_>, to: Status) -> Self {
        Self::at(to, s.place.clone())
    }

    fn review(mut self, entry: ReviewEntry) -> Self {
        self.entry = Some(entry);
        self
    }

    fn ended(mut self, outcome: ValidationOutcome) -> Self {
        self.outcome = Some(outcome);
        self
    }

    fn counters(mut self, counters: Counters) -> Self {
        self.counters = counters;
        self
    }

    /// Whether the ticket enters a human check (today's Ready for Review).
    pub fn enters_review(&self) -> bool {
        self.to == Status::WaitingForYou
    }
}

/// Why an event doesn't apply where the ticket stands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotNow(pub String);

/// Agent Working, with or without the needs-you family: where a submit is accepted.
fn agent_works(status: Status) -> bool {
    matches!(
        status,
        Status::Working
            | Status::WorkingInBackground
            | Status::AsksYou
            | Status::NeedsYou
            | Status::Idle
            | Status::MergeConflictFixing
    )
}

fn validating(status: Status) -> bool {
    matches!(status, Status::Checking | Status::CheckerNeedsYou)
}

/// One M12 row of SDD §7.3.
pub fn transition(s: ValidationState<'_>, event: ValidationEvent) -> Result<Transition, NotNow> {
    use ValidationEvent as E;
    let not_now = || {
        NotNow(format!(
            "{event:?} doesn't apply while the ticket is {}",
            s.status.label()
        ))
    };
    let at = At {
        fails: s.fails,
        rounds: s.rounds,
        skips: s.skips,
        ..At::new(s.flow, s.place, s.status)
    };
    let go = |mv: Move| {
        engine::step(at, mv)
            .map(|step| Transition::of(&step))
            .map_err(|_| not_now())
    };
    // An agent check whose checker waits for its tool's plan limit (SCH-04): skipped as if it were running.
    let held_check = s.status == Status::WaitingForLimitReset
        && s.place
            .stage()
            .and_then(|id| s.flow.stage(id))
            .is_some_and(Stage::is_agent_check);
    let go_checking = |mv: Move| {
        let checking = At {
            status: Status::Checking,
            ..at
        };
        engine::step(checking, mv)
            .map(|step| Transition::of(&step))
            .map_err(|_| not_now())
    };
    // The agent's (or the user's) next stage; the user's from Interrupted as if the agent were idle.
    let go_next = || -> Result<Transition, NotNow> {
        let t = if s.status == Status::Interrupted {
            let idle = At {
                status: Status::Idle,
                ..at
            };
            engine::step(idle, Move::NextStage)
                .map(|step| Transition::of(&step))
                .map_err(|_| not_now())?
        } else {
            go(Move::NextStage)?
        };
        // A fix after a Delivery's conflict validates again with the counters at 0 (FLW-17).
        Ok(
            if t.start_validator && s.status == Status::MergeConflictFixing {
                t.counters(Counters::ResetAll)
            } else {
                t
            },
        )
    };
    Ok(match event {
        E::AgentSubmit if agent_works(s.status) => go_next()?,
        E::UserNextStage if agent_works(s.status) || s.status == Status::Interrupted => go_next()?,
        E::Pass if validating(s.status) => {
            let mut t = go(Move::Pass)?;
            if t.enters_review() {
                t.entry = Some(ReviewEntry::Validated);
            }
            t.ended(ValidationOutcome::Pass)
        }
        E::Fail if validating(s.status) => {
            let mut t = go(Move::Fail)?;
            t.deliver_findings = t.to != Status::CheckFailed;
            t.ended(ValidationOutcome::Fail)
        }
        E::NoVerdict if validating(s.status) => {
            go(Move::NoResult)?.ended(ValidationOutcome::NoResult)
        }
        E::ValidatorChangedFiles if validating(s.status) => {
            go(Move::NoResult)?.ended(ValidationOutcome::Discarded)
        }
        E::ValidatorNeedsYou if s.status == Status::Checking => {
            Transition::stay(&s, Status::CheckerNeedsYou)
        }
        E::ValidatorContinues if s.status == Status::CheckerNeedsYou => {
            Transition::stay(&s, Status::Checking)
        }
        E::CodeChanged if validating(s.status) => Transition {
            tell_cancelled: true,
            ..go(Move::CancelCheck)?
        }
        .ended(ValidationOutcome::Cancelled),
        E::StopValidation if validating(s.status) => {
            go(Move::CancelCheck)?.ended(ValidationOutcome::Stopped)
        }
        E::SkipCheck if validating(s.status) || held_check => go_checking(Move::SkipCheck)?
            .review(ReviewEntry::Skipped)
            .ended(ValidationOutcome::Skipped),
        E::MoveOnAnyway if s.status == Status::CheckFailed => {
            go(Move::MoveOnAnyway)?.review(ReviewEntry::FailedMoved)
        }
        // No verdict: "Validation skipped by you" (d0063 changelog).
        E::MoveOnAnyway if s.status == Status::CheckNoResult => {
            go(Move::MoveOnAnyway)?.review(ReviewEntry::Skipped)
        }
        E::AnotherRound if s.status == Status::CheckFailed => Transition {
            deliver_findings: true,
            extra_round: true,
            ..go(Move::AnotherRound)?
        },
        E::AnswerAgent if s.status == Status::CheckFailed => Transition {
            deliver_findings: true,
            ..go(Move::AnswerAgent)?
        },
        E::RetryValidation if s.status == Status::CheckNoResult => go(Move::RetryCheck)?,
        _ => return Err(not_now()),
    })
}

impl Board {
    /// Every validation round of the ticket, newest first (History, VAL-12). Tokens are left for the service.
    pub async fn validations(&self, key: &str) -> Result<Vec<ValidationRecord>, BoardError> {
        let id = self.ticket_id(key).await?;
        type Row = (
            i64,
            String,
            Option<String>,
            i64,
            Option<i64>,
            Option<String>,
            Option<String>,
            Option<String>,
            Option<String>,
        );
        // Newest round first; a round's check agents in the stage's order (d0130).
        let rows: Vec<Row> = sqlx::query_as(
            "SELECT v.round, v.trigger, v.outcome, v.started_at, v.ended_at, v.session_id, COALESCE(c.body, v.reason),
                    v.notes_json, v.agent
             FROM validations v
             LEFT JOIN stage_runs r ON r.validation_id = v.id
             LEFT JOIN ticket_comments c ON c.id = r.comment_id
             WHERE v.ticket_id = ? ORDER BY v.started_at DESC, v.rowid",
        )
        .bind(&id)
        .fetch_all(self.store.pool())
        .await?;
        Ok(rows
            .into_iter()
            .map(
                |(round, trigger, outcome, started_at, ended_at, session, report, notes, agent)| {
                    ValidationRecord {
                        round: u32::try_from(round).unwrap_or(0),
                        agent,
                        trigger: trigger.parse().unwrap_or(ValidationTrigger::Agent),
                        outcome: outcome.and_then(|o| o.parse().ok()),
                        started_at,
                        duration_ms: ended_at.map(|end| (end - started_at).max(0)),
                        ended_at,
                        session,
                        report,
                        notes: notes
                            .and_then(|n| serde_json::from_str(&n).ok())
                            .unwrap_or_default(),
                        tokens: None,
                    }
                },
            )
            .collect())
    }

    /// **Create ticket** from a "Not covered by any ticket" note of the ticket's validation reports (VAL-12, the
    /// user's click; the validator stays read-only): a prefilled Backlog ticket in the same epic.
    pub async fn create_from_note(
        &self,
        key: &str,
        note: &str,
        actor: &Actor,
    ) -> Result<Ticket, BoardError> {
        let source = self.ticket(key).await?;
        let known = self
            .validations(key)
            .await?
            .iter()
            .any(|r| r.notes.iter().any(|n| n.trim() == note.trim()));
        let input = ticket_from_note(&source, note)
            .filter(|_| known)
            .ok_or_else(|| {
                BoardError::NotFound(format!(
                    "\"{}\" isn't a \"{}\" note of {}'s validation",
                    note.trim(),
                    NOT_COVERED.trim_end_matches(':'),
                    source.key
                ))
            })?;
        let created = self.create_ticket(&input, actor).await?;
        tracing::info!(source = %source.key, created = %created.key, "ticket created from a validation note");
        Ok(created)
    }

    /// Opens a validation round for a submit (VAL-01): posts the summary as the agent's comment (`hand_over`, CMT-05)
    /// and records the head like any submit, opens the round in `validations` and sets Validating… (or "Waiting for
    /// limit reset" while the usage limit holds, marked as a validation that waits). `summary` is History's line.
    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn enter_validation(
        &self,
        ticket: &Ticket,
        step: &Transition,
        head: &str,
        summary: &str,
        hand_over: Option<&HandOver>,
        trigger: ValidationTrigger,
        held: bool,
        actor: &Actor,
    ) -> Result<(), BoardError> {
        let now = now_ms();
        let check = step.place.stage_id().unwrap_or_default();
        let status = if held {
            Status::WaitingForLimitReset
        } else {
            step.to
        };
        // Into an agent check: "Finished Dev", not "Submitted for review" (Q263).
        let flow = self.effective_flow(ticket.key.project.as_str()).await?;
        let finished = super::review::stage_finished(&flow, ticket, summary, head);
        let mut tx = self.store.begin_write().await?;
        let posted = match hand_over {
            Some(h) => Some(h.post(&mut tx, &ticket.id, actor).await?),
            None => None,
        };
        stage_runs::end_open(
            &mut tx,
            &ticket.id,
            RunResult::Done,
            posted.as_ref().map(|p| p.id.as_str()),
        )
        .await?;
        if step.counters == Counters::ResetAll {
            reset_check_counters(&mut tx, &ticket.id).await?;
        }
        let counted = enter_check(&mut tx, &ticket.id, check).await?;
        sqlx::query(concat!(
            "UPDATE tickets SET stage_id = ?, state = ?, review_entry = NULL,
                                interrupted_from = ?, signal_text = NULL, signal_since = NULL, wakeup_at = NULL,
                                resume_skill = NULL, conflicts_with = NULL,
                                conflict_files = NULL, conflict_reason = NULL, auto_merge_stop = NULL,
                                updated_by = ?, updated_at = ?, ",
            set_state_since!(),
            " WHERE id = ?"
        ))
        .bind(step.place.stage_id())
        .bind(status.as_str())
        .bind(held.then_some(Status::Checking.as_str()))
        .bind(actor.to_string())
        .bind(now)
        .bind(status.as_str())
        .bind(now)
        .bind(&ticket.id)
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "INSERT INTO validations (id, ticket_id, round, trigger, started_at, head_sha) VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(new_id())
        .bind(&ticket.id)
        .bind(i64::from(counted + 1))
        .bind(trigger.as_str())
        .bind(now)
        .bind(head)
        .execute(&mut *tx)
        .await?;
        record_event(
            &mut tx,
            &ticket.id,
            EventKind::StageFinished,
            actor,
            finished,
        )
        .await?;
        tx.commit().await?;
        tracing::info!(key = %ticket.key, %actor, round = counted + 1, held, "validation opened");
        Ok(())
    }

    /// A checker of the ticket's open round started: its session is stored on its check agent's row (`row`; `None`:
    /// the round's newest open row), and a validation that waited for the usage limit shows Validating… again.
    pub(crate) async fn validator_started(
        &self,
        ticket_id: &str,
        row: Option<&str>,
        session_id: &str,
    ) -> Result<(), BoardError> {
        let now = now_ms();
        let mut tx = self.store.begin_write().await?;
        sqlx::query(
            "UPDATE validations SET session_id = ? WHERE id = COALESCE(?, (
                 SELECT id FROM validations WHERE ticket_id = ? AND ended_at IS NULL
                 ORDER BY started_at DESC, id DESC LIMIT 1))",
        )
        .bind(session_id)
        .bind(row)
        .bind(ticket_id)
        .execute(&mut *tx)
        .await?;
        let validating = Status::Checking;
        sqlx::query(concat!(
            "UPDATE tickets SET state = ?, interrupted_from = NULL, updated_at = ?, ",
            set_state_since!(),
            " WHERE id = ? AND state = ? AND interrupted_from = ?"
        ))
        .bind(validating.as_str())
        .bind(now)
        .bind(validating.as_str())
        .bind(now)
        .bind(ticket_id)
        .bind(Status::WaitingForLimitReset.as_str())
        .bind(validating.as_str())
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(())
    }

    /// The validator couldn't start: "Validation ended without a result" with the reason, the round ends without
    /// one (it doesn't count) and Retry validation or Move on anyway are the user's (VAL-07).
    pub(crate) async fn validator_did_not_start(
        &self,
        ticket_id: &str,
        reason: &str,
    ) -> Result<(), BoardError> {
        let now = now_ms();
        let to = Status::CheckNoResult;
        let mut tx = self.store.begin_write().await?;
        sqlx::query(
            "UPDATE validations SET ended_at = ?, outcome = ?, reason = ? WHERE ticket_id = ? AND ended_at IS NULL",
        )
        .bind(now)
        .bind(ValidationOutcome::NoResult.as_str())
        .bind(format!("The validator didn't start: {reason}"))
        .bind(ticket_id)
        .execute(&mut *tx)
        .await?;
        sqlx::query(concat!(
            "UPDATE tickets SET state = ?, signal_text = ?, signal_since = ?, updated_at = ?, ",
            set_state_since!(),
            " WHERE id = ?"
        ))
        .bind(to.as_str())
        .bind(format!("The validator didn't start: {reason}"))
        .bind(now)
        .bind(now)
        .bind(to.as_str())
        .bind(now)
        .bind(ticket_id)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(())
    }

    /// The ticket's open round (d0130): the round a row still runs in, with every check agent's row.
    pub(crate) async fn open_round(
        &self,
        ticket_id: &str,
    ) -> Result<Option<OpenRound>, BoardError> {
        let id: Option<String> = sqlx::query_scalar(
            "SELECT COALESCE(round_id, id) FROM validations WHERE ticket_id = ? AND ended_at IS NULL
             ORDER BY started_at DESC, id DESC LIMIT 1",
        )
        .bind(ticket_id)
        .fetch_optional(self.store.pool())
        .await?;
        match id {
            Some(id) => self.round(ticket_id, &id).await,
            None => Ok(None),
        }
    }

    /// The round `round_id` of the ticket, its rows oldest first (a round's rows in the stage's order).
    pub(crate) async fn round(
        &self,
        ticket_id: &str,
        round_id: &str,
    ) -> Result<Option<OpenRound>, BoardError> {
        type Row = (
            String,
            i64,
            Option<String>,
            Option<String>,
            Option<String>,
            Option<i64>,
            Option<String>,
        );
        let rows: Vec<Row> = sqlx::query_as(
            "SELECT id, round, agent, session_id, outcome, ended_at, head_sha FROM validations
             WHERE ticket_id = ? AND COALESCE(round_id, id) = ? ORDER BY started_at, rowid",
        )
        .bind(ticket_id)
        .bind(round_id)
        .fetch_all(self.store.pool())
        .await?;
        let Some(first) = rows.first() else {
            return Ok(None);
        };
        Ok(Some(OpenRound {
            id: round_id.to_owned(),
            round: u32::try_from(first.1).unwrap_or(0),
            head: first.6.clone(),
            rows: rows
                .into_iter()
                .map(|(id, _, agent, session, outcome, ended_at, _)| RoundRow {
                    id,
                    agent,
                    session,
                    outcome: outcome.and_then(|o| o.parse().ok()),
                    open: ended_at.is_none(),
                })
                .collect(),
        }))
    }

    /// Gives the ticket's open round one row per check agent of `stage` (d0130), once: a round opens with one row
    /// (a submit, a pass into the check, a drag, Retry), which takes the first check agent's name, and each other
    /// check agent gets its own row with the same number and head. Every row is a run of the stage (STG-05).
    pub(crate) async fn name_round_agents(
        &self,
        ticket: &Ticket,
        stage: &Stage,
    ) -> Result<(), BoardError> {
        let Some(round) = self.open_round(&ticket.id).await? else {
            return Ok(());
        };
        let [first] = round.rows.as_slice() else {
            return Ok(());
        };
        if first.agent.is_some() || !first.open {
            return Ok(());
        }
        let mut tx = self.store.begin_write().await?;
        let agents = round_agents(&mut tx, ticket, stage).await?;
        let Some((name, others)) = agents.split_first() else {
            return Ok(());
        };
        sqlx::query("UPDATE validations SET agent = ?, round_id = ? WHERE id = ?")
            .bind(name)
            .bind(&round.id)
            .bind(&first.id)
            .execute(&mut *tx)
            .await?;
        sqlx::query("UPDATE stage_runs SET agent = COALESCE(agent, ?) WHERE validation_id = ?")
            .bind(name)
            .bind(&first.id)
            .execute(&mut *tx)
            .await?;
        for agent in others {
            sqlx::query(
                "INSERT INTO validations (id, ticket_id, round, trigger, started_at, head_sha, agent, round_id)
                 SELECT ?, ticket_id, round, trigger, started_at, head_sha, ?, ? FROM validations WHERE id = ?",
            )
            .bind(new_id())
            .bind(agent)
            .bind(&round.id)
            .bind(&first.id)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    /// Ends the ticket's open round as `step` says without a verdict (a cancel, a discard, a stop…): every check
    /// agent's row still open ends; `reason` is why, in Otaq's words (History's line for a round that doesn't count),
    /// `signal` the card's text for the new status ("Validation discarded: the validator changed files").
    pub(crate) async fn end_round(
        &self,
        ticket: &Ticket,
        step: &Transition,
        reason: &str,
        signal: Option<&str>,
        actor: &Actor,
    ) -> Result<(), BoardError> {
        let Some(round) = self.open_round(&ticket.id).await? else {
            return Err(BoardError::refused(
                Refusal::MoveRefused,
                format!("{} has no validation running", ticket.key),
            ));
        };
        let close = RoundClose {
            step,
            reason,
            signal,
            resumed: false,
        };
        self.end_in_round(ticket, &round, None, Some(close), actor)
            .await
            .map(|_| ())
    }

    /// One check agent's row of `round` ends (`row`: its verdict, its text the checker's comment, CMT-05; or none),
    /// and the round closes as `close` says (VAL-03 … VAL-06): the ticket moves, the counter counts once for the
    /// round, and a pass enters Review with the round's head as the reviewed head. Its rows still open end with the
    /// step's outcome. A fail that sends the findings back to the agent marks its run as handed over (`resumed`,
    /// CMT-08). One transaction; `false` (nothing written) when `row` had already ended (a cancel came first).
    pub(crate) async fn end_in_round(
        &self,
        ticket: &Ticket,
        round: &OpenRound,
        row: Option<RowEnd<'_>>,
        close: Option<RoundClose<'_>>,
        actor: &Actor,
    ) -> Result<bool, BoardError> {
        let now = now_ms();
        let mut tx = self.store.begin_write().await?;
        if let Some(end) = &row {
            // "Note:" lines still become notes (d0063), from the comment's text.
            let notes: Vec<&str> = end
                .hand_over
                .map_or("", |h| h.body.as_str())
                .lines()
                .filter_map(note_of)
                .collect();
            let ended = sqlx::query(
                "UPDATE validations SET ended_at = ?, outcome = ?, reason = ?, notes_json = ?
                 WHERE id = ? AND ended_at IS NULL",
            )
            .bind(now)
            .bind(end.outcome.as_str())
            .bind(Some(end.reason.trim()).filter(|r| !r.is_empty()))
            .bind(serde_json::to_string(&notes).expect("notes serialize"))
            .bind(end.row)
            .execute(&mut *tx)
            .await?
            .rows_affected();
            if ended == 0 {
                return Ok(false);
            }
            if let Some(hand_over) = end.hand_over {
                let posted = hand_over.post(&mut tx, &ticket.id, actor).await?;
                sqlx::query("UPDATE stage_runs SET comment_id = ? WHERE validation_id = ?")
                    .bind(&posted.id)
                    .bind(end.row)
                    .execute(&mut *tx)
                    .await?;
            }
        }
        if let Some(close) = &close {
            self.close_round_in(&mut tx, ticket, round, close, now, actor)
                .await?;
        }
        tx.commit().await?;
        if let Some(close) = &close {
            tracing::info!(key = %ticket.key, round = round.round, outcome = ?close.step.outcome, to = %close.step.to,
                           "validation round ended");
        }
        Ok(true)
    }

    async fn close_round_in(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        ticket: &Ticket,
        round: &OpenRound,
        close: &RoundClose<'_>,
        now: i64,
        actor: &Actor,
    ) -> Result<(), BoardError> {
        let step = close.step;
        let outcome = step.outcome.unwrap_or(ValidationOutcome::NoResult);
        let review = step.enters_review();
        sqlx::query(
            "UPDATE validations SET ended_at = ?, outcome = ?, reason = ?, notes_json = COALESCE(notes_json, '[]')
             WHERE ticket_id = ? AND COALESCE(round_id, id) = ? AND ended_at IS NULL",
        )
        .bind(now)
        .bind(outcome.as_str())
        .bind(Some(close.reason.trim()).filter(|r| !r.is_empty()))
        .bind(&ticket.id)
        .bind(&round.id)
        .execute(&mut **tx)
        .await?;
        // The round counts once on the check it ran in (the stage the ticket is in until now), whichever agent failed.
        if let Some(check) = ticket.place.stage() {
            count_on(tx, &ticket.id, check, step.counters, outcome, round.round).await?;
        }
        sqlx::query(concat!(
            "UPDATE tickets SET stage_id = ?, state = ?, review_entry = ?, interrupted_from = NULL,
                                queued_at = CASE WHEN ? THEN ? ELSE queued_at END,
                                review_head_sha = CASE WHEN ? THEN ? ELSE review_head_sha END,
                                review_entered_at = CASE WHEN ? THEN ? ELSE review_entered_at END,
                                sent_back_at = CASE WHEN ? THEN NULL ELSE sent_back_at END,
                                pr_rework = CASE WHEN ? THEN FALSE ELSE pr_rework END,
                                signal_text = ?, signal_since = ?, updated_at = ?, ",
            set_state_since!(),
            " WHERE id = ?"
        ))
        .bind(step.place.stage_id())
        .bind(step.to.as_str())
        .bind(step.entry.map(|e| e.as_str()))
        .bind(step.to.is_queued())
        .bind(now)
        .bind(review)
        .bind(round.head.as_deref())
        .bind(review)
        .bind(now)
        .bind(review)
        .bind(review)
        .bind(close.signal)
        .bind(close.signal.map(|_| now))
        .bind(now)
        .bind(step.to.as_str())
        .bind(now)
        .bind(&ticket.id)
        .execute(&mut **tx)
        .await?;
        // The Agent stage it went back to resumes with the findings in a comment: its agent primes again (CMT-08).
        if close.resumed {
            stage_runs::mark_handed_over(tx, &ticket.id).await?;
        }
        record_event(
            tx,
            &ticket.id,
            EventKind::ValidationEnded,
            actor,
            json!({"round": round.round, "outcome": outcome, "counted": outcome.counts()}),
        )
        .await?;
        Ok(())
    }

    /// Whether tracked files or the head of the ticket's worktree differ from `head` (VAL-02, VAL-08). Files ignored
    /// by `.gitignore` and untracked files don't count; nothing in the worktree is touched (never a stash).
    pub(crate) async fn worktree_changed(
        &self,
        ticket: &Ticket,
        head: &str,
    ) -> Result<bool, BoardError> {
        let Some(worktree) = ticket.work.as_ref().and_then(|w| w.worktree.clone()) else {
            return Ok(false);
        };
        let git = self.git()?;
        let dir = std::path::Path::new(&worktree);
        let now = git.run(dir, &["rev-parse", "HEAD"]).await?;
        if now.trim() != head {
            return Ok(true);
        }
        let tracked = git
            .run(dir, &["status", "--porcelain", "--untracked-files=no"])
            .await?;
        Ok(!tracked.trim().is_empty())
    }

    /// Moves a validating ticket between the states its validator's hooks set (VAL-07, VAL-11), only if it is
    /// still in `from`. `behind` marks a validation that waits (the usage limit).
    pub(crate) async fn set_validation_status(
        &self,
        ticket_id: &str,
        from: &[Status],
        to: Status,
        text: Option<&str>,
        behind: bool,
    ) -> Result<bool, BoardError> {
        let now = now_ms();
        // A held stop handed back moves from the Agent stage into the check that failed it (the one that counted
        // last, FLW-13); a validation that waits stays.
        let ticket = self.ticket_by_id(ticket_id).await?;
        let flow = self.effective_flow(ticket.key.project.as_str()).await?;
        let counts = self.check_counts(ticket_id).await?;
        let failed_check = || {
            let at_work = ticket
                .place
                .stage()
                .and_then(|id| flow.stage(id))
                .is_some_and(Stage::is_agent_stage);
            (at_work && to.set() == crate::domain::StatusSet::AgentCheck)
                .then(|| counts.counting(&flow, &ticket.place))
                .flatten()
        };
        let stage = (!behind)
            .then(|| failed_check().or_else(|| engine::stage_for(&flow, &ticket.place, to)))
            .flatten()
            .map(|s| s.id.clone());
        let from: Vec<&str> = from.iter().map(|s| s.as_str()).collect();
        let sql = format!(
            concat!(
                "UPDATE tickets SET stage_id = COALESCE(?, stage_id), state = ?, signal_text = ?, signal_since = ?,
                                    interrupted_from = ?, updated_at = ?, ",
                set_state_since!(),
                " WHERE id = ? AND state IN ({})"
            ),
            vec!["?"; from.len()].join(", ")
        );
        let mut query = sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(stage)
            .bind(to.as_str())
            .bind(text)
            .bind(text.map(|_| now))
            .bind(behind.then_some(Status::Checking.as_str()))
            .bind(now)
            .bind(to.as_str())
            .bind(now)
            .bind(ticket_id);
        for status in from {
            query = query.bind(status);
        }
        Ok(query.execute(self.store.pool()).await?.rows_affected() > 0)
    }

    /// Into Ready for Review without a verdict, by the user (Move on anyway, VAL-06, VAL-07): the last
    /// round's head is the reviewed head; never auto-merged. The ticket goes to the first human check after its
    /// stage (the default flow's Review).
    pub(crate) async fn move_to_review(
        &self,
        ticket: &Ticket,
        entry: Option<ReviewEntry>,
        actor: &Actor,
    ) -> Result<(), BoardError> {
        let flow = self.effective_flow(ticket.key.project.as_str()).await?;
        let stage = ticket
            .place
            .stage()
            .and_then(|id| engine::next_human_check(&flow, id))
            .or_else(|| flow.stages.iter().rev().find(|s| s.is_human_check()))
            .map(|s| s.id.clone());
        let head: Option<Option<String>> = sqlx::query_scalar(
            "SELECT head_sha FROM validations WHERE ticket_id = ? ORDER BY started_at DESC, id DESC LIMIT 1",
        )
        .bind(&ticket.id)
        .fetch_optional(self.store.pool())
        .await?;
        let now = now_ms();
        let to = Status::WaitingForYou;
        let mut tx = self.store.begin_write().await?;
        sqlx::query(concat!(
            "UPDATE tickets SET stage_id = ?, state = ?, review_head_sha = COALESCE(?, review_head_sha),
                                review_entered_at = ?, review_entry = COALESCE(?, review_entry),
                                sent_back_at = NULL, pr_rework = FALSE, auto_merge_stop = NULL,
                                signal_text = NULL, signal_since = NULL, updated_by = ?, updated_at = ?, ",
            set_state_since!(),
            " WHERE id = ?"
        ))
        .bind(stage)
        .bind(to.as_str())
        .bind(head.flatten())
        .bind(now)
        .bind(entry.map(|e| e.as_str()))
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
            EventKind::Moved,
            actor,
            json!({"from": ticket.status, "to": to}),
        )
        .await?;
        tx.commit().await?;
        Ok(())
    }

    /// Back to the agent after the last failed round (Give it another round, Answer the agent): Agent Working in
    /// the stage `step` names, one more round allowed on the failed check or its counter at 0.
    pub(crate) async fn back_to_agent(
        &self,
        ticket: &Ticket,
        step: &Transition,
        actor: &Actor,
    ) -> Result<(), BoardError> {
        let now = now_ms();
        let to = Status::Working;
        let mut tx = self.store.begin_write().await?;
        if let Some(check) = ticket.place.stage() {
            if step.extra_round {
                add_round(&mut tx, &ticket.id, check).await?;
            }
            count_on(
                &mut tx,
                &ticket.id,
                check,
                step.counters,
                ValidationOutcome::NoResult,
                0,
            )
            .await?;
        }
        sqlx::query(concat!(
            "UPDATE tickets SET stage_id = ?, state = ?,
                                signal_text = NULL, signal_since = NULL, updated_by = ?, updated_at = ?, ",
            set_state_since!(),
            " WHERE id = ?"
        ))
        .bind(step.place.stage_id())
        .bind(to.as_str())
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
            EventKind::Moved,
            actor,
            json!({"from": ticket.status, "to": to}),
        )
        .await?;
        tx.commit().await?;
        Ok(())
    }

    /// The last ended round's report (its findings: the checker's comment), for Give it another round and Answer the
    /// agent. A round of several check agents gives every agent's report together, each under its verdict
    /// ("Security reviewer failed:"), in the stage's order (d0130).
    pub(crate) async fn last_report(&self, ticket_id: &str) -> Result<Option<String>, BoardError> {
        let rows: Vec<(Option<String>, Option<String>, Option<String>)> = sqlx::query_as(
            "SELECT v.agent, v.outcome, c.body FROM validations v
             LEFT JOIN stage_runs r ON r.validation_id = v.id
             LEFT JOIN ticket_comments c ON c.id = r.comment_id
             WHERE v.ticket_id = ?1 AND COALESCE(v.round_id, v.id) = (
                 SELECT COALESCE(round_id, id) FROM validations WHERE ticket_id = ?1 AND ended_at IS NOT NULL
                 ORDER BY started_at DESC, id DESC LIMIT 1)
             ORDER BY v.started_at, v.rowid",
        )
        .bind(ticket_id)
        .fetch_all(self.store.pool())
        .await?;
        if let [(_, _, body)] = rows.as_slice() {
            return Ok(body.clone());
        }
        let reports: Vec<String> = rows
            .into_iter()
            .filter_map(|(agent, outcome, body)| {
                let body = body?;
                let outcome: Option<ValidationOutcome> = outcome.and_then(|o| o.parse().ok());
                let agent = agent.unwrap_or_else(|| "The check".to_owned());
                Some(format!("{}:\n{}", verdict_of(&agent, outcome), body.trim()))
            })
            .collect();
        Ok((!reports.is_empty()).then(|| reports.join("\n\n")))
    }

    /// Ends whatever round is open with `outcome`, the ticket's status left alone (Park, Close).
    pub(crate) async fn abandon_round(
        &self,
        ticket_id: &str,
        outcome: ValidationOutcome,
    ) -> Result<bool, BoardError> {
        let ended = sqlx::query(
            "UPDATE validations SET ended_at = ?, outcome = ? WHERE ticket_id = ? AND ended_at IS NULL",
        )
        .bind(now_ms())
        .bind(outcome.as_str())
        .bind(ticket_id)
        .execute(self.store.pool())
        .await?
        .rows_affected();
        Ok(ended > 0)
    }

    /// A fresh round for Retry validation (VAL-07): the same round number, not counted; the ticket validates
    /// again.
    pub(crate) async fn reopen_round(
        &self,
        ticket: &Ticket,
        head: &str,
        actor: &Actor,
    ) -> Result<(), BoardError> {
        let now = now_ms();
        let to = Status::Checking;
        let mut tx = self.store.begin_write().await?;
        sqlx::query(concat!(
            "UPDATE tickets SET state = ?, signal_text = NULL, signal_since = NULL,
                                interrupted_from = NULL, updated_by = ?, updated_at = ?, ",
            set_state_since!(),
            " WHERE id = ?"
        ))
        .bind(to.as_str())
        .bind(actor.to_string())
        .bind(now)
        .bind(to.as_str())
        .bind(now)
        .bind(&ticket.id)
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "INSERT INTO validations (id, ticket_id, round, trigger, started_at, head_sha) VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(new_id())
        .bind(&ticket.id)
        .bind(i64::from(ticket.validation_round + 1))
        .bind(ValidationTrigger::User.as_str())
        .bind(now)
        .bind(head)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(())
    }

    /// Whether the round this validator session judged has ended (its verdict is in).
    pub(crate) async fn round_of_session_ended(&self, session: &str) -> Result<bool, BoardError> {
        let ended: Option<Option<i64>> =
            sqlx::query_scalar("SELECT ended_at FROM validations WHERE session_id = ?")
                .bind(session)
                .fetch_optional(self.store.pool())
                .await?;
        Ok(matches!(ended, Some(Some(_))))
    }

    /// Tickets whose validation waits for the usage limit to reset (SCH-04).
    pub(crate) async fn validations_waiting_for_limit(&self) -> Result<Vec<String>, BoardError> {
        let ids: Vec<(String,)> =
            sqlx::query_as("SELECT id FROM tickets WHERE state = ? AND interrupted_from = ?")
                .bind(Status::WaitingForLimitReset.as_str())
                .bind(Status::Checking.as_str())
                .fetch_all(self.store.pool())
                .await?;
        Ok(ids.into_iter().map(|(id,)| id).collect())
    }
}

/// One agent check's counter on a ticket (`check_counters`, FLW-13, Q259; d0108 "After a Fail" 2).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CheckCount {
    /// The check stage's id.
    pub stage: String,
    /// Its fails in a row; a pass ends them. At its rounds the card stops ("Failed ×3").
    pub fails: u32,
    /// The rounds Give it another round added to the stage's.
    pub extra: u32,
    /// The round it last passed in ("Validated · round 2"); `None` unless its last verdict was a pass.
    pub passed_round: Option<u32>,
    /// How the user moved the card past it in this walk, if they did.
    pub moved_past: Option<MovedPast>,
    /// When it last counted a round (or was reset).
    pub updated_at: i64,
}

impl CheckCount {
    /// The round its card markers name: the round it passed in, else its fails in a row.
    pub fn shown_round(&self) -> u32 {
        self.passed_round.unwrap_or(self.fails)
    }
}

/// A ticket's check counters, one per agent check that counted ([`CheckCount`]).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CheckCounts(pub Vec<CheckCount>);

impl CheckCounts {
    /// The counter of the check `stage` (all at 0 when it never counted).
    pub fn of(&self, stage: &str) -> CheckCount {
        self.0
            .iter()
            .find(|c| c.stage == stage)
            .cloned()
            .unwrap_or_else(|| CheckCount {
                stage: stage.to_owned(),
                ..CheckCount::default()
            })
    }

    /// The agent check a ticket at `place` counts rounds for (its card's "round 1 of 3", "Security failed · round 2
    /// of 3"): the check it is in; else the check that counted last (the one that sent it back); else the flow's
    /// [`Flow::round_check`].
    pub fn counting<'f>(&self, flow: &'f Flow, place: &Place) -> Option<&'f Stage> {
        if let Some(here) = place
            .stage()
            .and_then(|id| flow.stage(id))
            .filter(|s| s.is_agent_check())
        {
            return Some(here);
        }
        self.0
            .iter()
            .filter(|c| c.fails > 0 || c.passed_round.is_some())
            .filter_map(|c| Some((c.updated_at, flow.stage(&c.stage)?)))
            .filter(|(_, s)| s.is_agent_check())
            .max_by_key(|(at, _)| *at)
            .map(|(_, s)| s)
            .or_else(|| flow.round_check(place.stage()))
    }

    /// Why the card's Delivery waits for a click (d0108 "Move on anyway", Q277): each check the user moved it past
    /// in this walk, in flow order ("Failed Security 3 times · moved by you", "Skipped Security · by you"). `None`
    /// when it moved past none.
    pub fn moved_past_reason(&self, flow: &Flow) -> Option<String> {
        let mut past: Vec<(usize, String)> = self
            .0
            .iter()
            .filter_map(|c| {
                let how = c.moved_past?;
                let stage = flow.stage(&c.stage);
                let name = stage.map_or(c.stage.as_str(), |s| s.name.as_str());
                let at = flow.position(&c.stage).unwrap_or(usize::MAX);
                Some((at, how.reason(name, c.fails)))
            })
            .collect();
        past.sort();
        (!past.is_empty()).then(|| {
            past.into_iter()
                .map(|(_, reason)| reason)
                .collect::<Vec<_>>()
                .join(" · ")
        })
    }
}

type CountRow = (String, String, i64, i64, Option<i64>, Option<String>, i64);

fn count_of(row: CountRow) -> (String, CheckCount) {
    let (ticket, stage, fails, extra, passed, moved, updated_at) = row;
    let n = |v: i64| u32::try_from(v).unwrap_or(0);
    (
        ticket,
        CheckCount {
            stage,
            fails: n(fails),
            extra: n(extra),
            passed_round: passed.map(n),
            moved_past: moved.and_then(|m| m.parse().ok()),
            updated_at,
        },
    )
}

const SELECT_COUNTS: &str =
    "SELECT c.ticket_id, c.stage_id, c.fails, c.extra, c.passed_round, c.moved_past,
                                    c.updated_at FROM check_counters c";

/// Every ticket's check counters in a project, by ticket id (the board's cards).
pub(crate) async fn project_check_counts(
    pool: &sqlx::SqlitePool,
    project_id: &str,
) -> Result<std::collections::HashMap<String, CheckCounts>, BoardError> {
    let sql = format!(
        "{SELECT_COUNTS} JOIN tickets t ON t.id = c.ticket_id WHERE t.project_id = ? ORDER BY c.stage_id"
    );
    let rows: Vec<CountRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(project_id)
        .fetch_all(pool)
        .await?;
    let mut counts: std::collections::HashMap<String, CheckCounts> =
        std::collections::HashMap::new();
    for row in rows {
        let (ticket, count) = count_of(row);
        counts.entry(ticket).or_default().0.push(count);
    }
    Ok(counts)
}

/// Every check counter of a ticket restarts at 0 and its "moved past" marks go (a human Fail or a drag back: the
/// card walks every later check again, d0108 "After a Fail" 3; d0063's reset after Send back).
pub(crate) async fn reset_check_counters(
    tx: &mut Transaction<'_, Sqlite>,
    ticket_id: &str,
) -> Result<(), BoardError> {
    sqlx::query("DELETE FROM check_counters WHERE ticket_id = ?")
        .bind(ticket_id)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

/// The ticket enters the check `stage`: a fresh walk through it (its last pass and its "moved past" mark go; its
/// fails in a row stay). Returns its fails in a row: the new round is one more.
pub(crate) async fn enter_check(
    tx: &mut Transaction<'_, Sqlite>,
    ticket_id: &str,
    stage: &str,
) -> Result<u32, BoardError> {
    let fails: i64 = sqlx::query_scalar(
        "INSERT INTO check_counters (ticket_id, stage_id, updated_at) VALUES (?, ?, ?)
         ON CONFLICT (ticket_id, stage_id) DO UPDATE SET passed_round = NULL, moved_past = NULL
         RETURNING fails",
    )
    .bind(ticket_id)
    .bind(stage)
    .bind(now_ms())
    .fetch_one(&mut **tx)
    .await?;
    Ok(u32::try_from(fails).unwrap_or(0))
}

/// What the engine's [`Counters`] do to the check `stage` after a round numbered `round` ended with `outcome`.
async fn count_on(
    tx: &mut Transaction<'_, Sqlite>,
    ticket_id: &str,
    stage: &str,
    counters: Counters,
    outcome: ValidationOutcome,
    round: u32,
) -> Result<(), BoardError> {
    let sql = match (counters, outcome) {
        (Counters::Count, ValidationOutcome::Pass) => {
            "INSERT INTO check_counters (ticket_id, stage_id, passed_round, updated_at) VALUES (?, ?, ?, ?)
             ON CONFLICT (ticket_id, stage_id) DO UPDATE
             SET fails = 0, passed_round = excluded.passed_round, updated_at = excluded.updated_at"
        }
        (Counters::Count, ValidationOutcome::Fail) => {
            "INSERT INTO check_counters (ticket_id, stage_id, fails, updated_at) VALUES (?, ?, 1, ?)
             ON CONFLICT (ticket_id, stage_id) DO UPDATE
             SET fails = fails + 1, passed_round = NULL, updated_at = excluded.updated_at"
        }
        (Counters::ResetThis, _) => {
            "INSERT INTO check_counters (ticket_id, stage_id, updated_at) VALUES (?, ?, ?)
             ON CONFLICT (ticket_id, stage_id) DO UPDATE
             SET fails = 0, extra = 0, passed_round = NULL, updated_at = excluded.updated_at"
        }
        (Counters::ResetAll, _) => return reset_check_counters(tx, ticket_id).await,
        (Counters::Keep | Counters::Count, _) => return Ok(()),
    };
    let mut query = sqlx::query(sql).bind(ticket_id).bind(stage);
    if (counters, outcome) == (Counters::Count, ValidationOutcome::Pass) {
        query = query.bind(i64::from(round));
    }
    query.bind(now_ms()).execute(&mut **tx).await?;
    Ok(())
}

/// Give it another round: one more round on the check `stage`.
async fn add_round(
    tx: &mut Transaction<'_, Sqlite>,
    ticket_id: &str,
    stage: &str,
) -> Result<(), BoardError> {
    sqlx::query(
        "INSERT INTO check_counters (ticket_id, stage_id, extra, updated_at) VALUES (?, ?, 1, ?)
         ON CONFLICT (ticket_id, stage_id) DO UPDATE SET extra = extra + 1",
    )
    .bind(ticket_id)
    .bind(stage)
    .bind(now_ms())
    .execute(&mut **tx)
    .await?;
    Ok(())
}

/// The user moved the card past the check `stage` (Move on anyway, Skip this check; a forward drag past it): its
/// Delivery will be in click mode, saying why ([`CheckCounts::moved_past_reason`]).
pub(crate) async fn mark_moved_past(
    tx: &mut Transaction<'_, Sqlite>,
    ticket_id: &str,
    stage: &str,
    how: MovedPast,
) -> Result<(), BoardError> {
    sqlx::query(
        "INSERT INTO check_counters (ticket_id, stage_id, moved_past, updated_at) VALUES (?, ?, ?, ?)
         ON CONFLICT (ticket_id, stage_id) DO UPDATE SET moved_past = excluded.moved_past",
    )
    .bind(ticket_id)
    .bind(stage)
    .bind(how.as_str())
    .bind(now_ms())
    .execute(&mut **tx)
    .await?;
    Ok(())
}

impl Board {
    /// The ticket's check counters ([`CheckCounts`]).
    pub async fn check_counts(&self, ticket_id: &str) -> Result<CheckCounts, BoardError> {
        let sql = format!("{SELECT_COUNTS} WHERE c.ticket_id = ? ORDER BY c.stage_id");
        let rows: Vec<CountRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
            .bind(ticket_id)
            .fetch_all(self.store.pool())
            .await?;
        Ok(CheckCounts(
            rows.into_iter().map(|row| count_of(row).1).collect(),
        ))
    }

    /// **Every agent check's counter restarts at 0** (d0108 "After a Fail" 3, FLW-13): what a human Fail and a drag
    /// back do (the engine's [`Counters::ResetAll`]). The marks of checks the user moved the card past go too: the
    /// card walks every later check again.
    pub async fn reset_all_check_counters(&self, ticket_id: &str) -> Result<(), BoardError> {
        let mut tx = self.store.begin_write().await?;
        reset_check_counters(&mut tx, ticket_id).await?;
        tx.commit().await?;
        Ok(())
    }

    /// The check `stage`'s counter restarts at 0 (the engine's [`Counters::ResetThis`]: the epic lead's note, which
    /// counts as Answer the agent).
    pub(crate) async fn reset_check_counter(
        &self,
        ticket_id: &str,
        stage: &str,
    ) -> Result<(), BoardError> {
        let mut tx = self.store.begin_write().await?;
        count_on(
            &mut tx,
            ticket_id,
            stage,
            Counters::ResetThis,
            ValidationOutcome::NoResult,
            0,
        )
        .await?;
        tx.commit().await?;
        Ok(())
    }

    /// Marks that the user moved the ticket past the check `stage` (a forward drag past it, T-0421): its Delivery
    /// will be in click mode.
    pub async fn moved_past_check(
        &self,
        ticket_id: &str,
        stage: &str,
        how: MovedPast,
    ) -> Result<(), BoardError> {
        let mut tx = self.store.begin_write().await?;
        mark_moved_past(&mut tx, ticket_id, stage, how).await?;
        tx.commit().await?;
        Ok(())
    }

    /// **Why the ticket's Delivery must wait for a click** (d0108: "a ticket the user moved past a failed or skipped
    /// agent check always reaches Delivery in click mode, whatever the stage's mode"): "Failed Security 3 times ·
    /// moved by you", "Skipped Security · by you"; `None` when its checks ran (an automatic Delivery may deliver).
    pub async fn forced_click_reason(&self, ticket: &Ticket) -> Result<Option<String>, BoardError> {
        let flow = self.effective_flow(ticket.key.project.as_str()).await?;
        Ok(self
            .check_counts(&ticket.id)
            .await?
            .moved_past_reason(&flow))
    }

    /// The user moved the ticket on from its agent check (Move on anyway, Skip this check on a check held for the
    /// limit): `how` is marked on the check, and the ticket goes where `step` says, the engine's next stage
    /// (FLW-13): a human check as Ready for Review did (the last round's head is the reviewed head), another agent
    /// check (Checking, its round opened by the caller), the wait before an Agent stage, or the Delivery.
    pub(crate) async fn move_on_from_check(
        &self,
        ticket: &Ticket,
        step: &Transition,
        how: MovedPast,
        actor: &Actor,
    ) -> Result<(), BoardError> {
        if let Some(check) = ticket.place.stage() {
            let mut tx = self.store.begin_write().await?;
            mark_moved_past(&mut tx, &ticket.id, check, how).await?;
            tx.commit().await?;
        }
        if step.enters_review() {
            return self.move_to_review(ticket, step.entry, actor).await;
        }
        let now = now_ms();
        let queued = step.to.is_queued();
        let mut tx = self.store.begin_write().await?;
        sqlx::query(concat!(
            "UPDATE tickets SET stage_id = ?, state = ?, review_entry = COALESCE(?, review_entry),
                                interrupted_from = NULL, auto_merge_stop = NULL,
                                queued_at = CASE WHEN ? THEN ? ELSE queued_at END,
                                signal_text = NULL, signal_since = NULL, updated_by = ?, updated_at = ?, ",
            set_state_since!(),
            " WHERE id = ?"
        ))
        .bind(step.place.stage_id())
        .bind(step.to.as_str())
        .bind(step.entry.map(|e| e.as_str()))
        .bind(queued)
        .bind(now)
        .bind(actor.to_string())
        .bind(now)
        .bind(step.to.as_str())
        .bind(now)
        .bind(&ticket.id)
        .execute(&mut *tx)
        .await?;
        record_event(
            &mut tx,
            &ticket.id,
            EventKind::Moved,
            actor,
            json!({"from": ticket.status, "to": step.to}),
        )
        .await?;
        tx.commit().await?;
        Ok(())
    }

    /// Opens a round of the agent check the ticket has just entered from another check (a pass, a skip or Move on
    /// anyway into the next check, FLW-13), on the head the last round judged (else the worktree's head). The
    /// caller starts its checker.
    pub(crate) async fn open_next_check(
        &self,
        ticket: &Ticket,
        trigger: ValidationTrigger,
    ) -> Result<(), BoardError> {
        let Some(check) = ticket.place.stage() else {
            return Ok(());
        };
        let head: Option<Option<String>> = sqlx::query_scalar(
            "SELECT head_sha FROM validations WHERE ticket_id = ? ORDER BY started_at DESC, id DESC LIMIT 1",
        )
        .bind(&ticket.id)
        .fetch_optional(self.store.pool())
        .await?;
        let head = match head.flatten() {
            Some(head) => head,
            None => self.submit_head(ticket).await?,
        };
        let now = now_ms();
        let mut tx = self.store.begin_write().await?;
        let fails = enter_check(&mut tx, &ticket.id, check).await?;
        sqlx::query(
            "INSERT INTO validations (id, ticket_id, round, trigger, started_at, head_sha) VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(new_id())
        .bind(&ticket.id)
        .bind(i64::from(fails + 1))
        .bind(trigger.as_str())
        .bind(now)
        .bind(&head)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        tracing::info!(key = %ticket.key, check, round = fails + 1, "next check opened");
        Ok(())
    }
}

/// One check agent's record of a validation round as History shows it (VAL-12, d0063, d0130): its outcome, duration,
/// report and notes. Tokens are
/// its validator session's, added by the service from the usage counts (`ticket.validations`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ValidationRecord {
    pub round: u32,
    /// The check agent whose record it is (d0130: one per agent per round), as the stage lists it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent: Option<String>,
    pub trigger: ValidationTrigger,
    /// `None` while the round runs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outcome: Option<ValidationOutcome>,
    pub started_at: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ended_at: Option<i64>,
    /// From start to end, once the round ended.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<i64>,
    /// The validator's session.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session: Option<String>,
    /// The validator's whole report (a verdict's: its comment), else why the round ended without one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub report: Option<String>,
    /// The report's notes ("Not covered by any ticket: refunds endpoint").
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<String>,
    /// The validator's tokens; `None` when unknown.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tokens: Option<crate::usage::Tokens>,
}

/// The start of a note about a gap no ticket covers (the validate skill's wording, d0063).
pub const NOT_COVERED: &str = "Not covered by any ticket:";

/// The gap of a "Not covered by any ticket: refunds endpoint" note ("refunds endpoint"); `None` for other notes.
pub fn not_covered(note: &str) -> Option<&str> {
    let note = note.trim();
    let head = note.get(..NOT_COVERED.len())?;
    if !head.eq_ignore_ascii_case(NOT_COVERED) {
        return None;
    }
    let gap = note[NOT_COVERED.len()..]
        .trim()
        .trim_end_matches('.')
        .trim();
    (!gap.is_empty()).then_some(gap)
}

/// The prefilled ticket for a "Not covered by any ticket" note of `source`'s validation (VAL-12): the gap as the
/// title (first letter upper-case), a description naming the source ticket and quoting the note, in the source's
/// epic. `None` when the note isn't about an uncovered gap.
pub fn ticket_from_note(source: &Ticket, note: &str) -> Option<super::tickets::NewTicketInput> {
    let gap = not_covered(note)?;
    let mut chars = gap.chars();
    let title = chars
        .next()
        .map(|first| first.to_uppercase().chain(chars).collect::<String>())
        .unwrap_or_default();
    Some(super::tickets::NewTicketInput {
        project: source.key.project.to_string(),
        title,
        body: format!(
            "Noted while validating {} ({}):\n\n> {}\n",
            source.key,
            source.title,
            note.trim()
        ),
        epic: source.epic.as_ref().map(ToString::to_string),
        ..super::tickets::NewTicketInput::default()
    })
}

/// A report line that is a note, not a finding: "Note: covered by SHOP-40.2", "- Note: Not covered by any
/// ticket: removing items" (the validate skill's format).
pub fn note_of(line: &str) -> Option<&str> {
    let line = line.trim().trim_start_matches(['-', '*']).trim_start();
    let rest = line
        .strip_prefix("Note:")
        .or_else(|| line.strip_prefix("note:"))?;
    let note = rest.trim();
    (!note.is_empty()).then_some(note)
}

/// A ticket's validation round (d0130): one row per check agent, sharing the round's number and head.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct OpenRound {
    /// The round's id (its first row's).
    pub id: String,
    pub round: u32,
    /// The worktree's head when it opened (VAL-02).
    pub head: Option<String>,
    /// Its rows, oldest first.
    pub rows: Vec<RoundRow>,
}

/// One check agent's row of a round.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RoundRow {
    pub id: String,
    /// The check agent's name as the stage lists it; `None` for a round opened before its agents were named.
    pub agent: Option<String>,
    /// Its checker's session, once it started.
    pub session: Option<String>,
    pub outcome: Option<ValidationOutcome>,
    /// Still waiting for its verdict.
    pub open: bool,
}

impl OpenRound {
    /// The rows still waiting for a verdict.
    pub fn open(&self) -> impl Iterator<Item = &RoundRow> {
        self.rows.iter().filter(|r| r.open)
    }

    /// The open row `session` checks in.
    pub fn open_row_of(&self, session: &str) -> Option<&RoundRow> {
        self.open().find(|r| r.session.as_deref() == Some(session))
    }

    /// Each check agent's latest row, in the stage's order (a re-run adds a row for an agent).
    pub fn latest(&self) -> Vec<&RoundRow> {
        let mut latest: Vec<&RoundRow> = Vec::new();
        for row in &self.rows {
            match latest.iter_mut().find(|r| r.agent == row.agent) {
                Some(slot) => *slot = row,
                None => latest.push(row),
            }
        }
        latest
    }

    /// Whether more than one check agent checks in it.
    pub fn several(&self) -> bool {
        self.latest().len() > 1
    }
}

/// How one check agent's row of a round ends ([`Board::end_in_round`]).
pub(crate) struct RowEnd<'a> {
    pub row: &'a str,
    pub outcome: ValidationOutcome,
    /// Why it ended without a verdict, in Otaq's words.
    pub reason: &'a str,
    /// The verdict's comment.
    pub hand_over: Option<&'a HandOver>,
}

/// How a round closes ([`Board::end_in_round`]).
pub(crate) struct RoundClose<'a> {
    /// The rules' row it carries out.
    pub step: &'a Transition,
    /// Why its open rows end, in Otaq's words.
    pub reason: &'a str,
    /// The card's text for the new status.
    pub signal: Option<&'a str>,
    /// The findings go back to the agent, which resumes in its Agent stage.
    pub resumed: bool,
}

/// What a round's verdicts add up to once none of its check agents runs (d0130): every one passed, a pass; any
/// failed, a fail; else (one ended without a verdict, the others passed) none.
pub(crate) fn round_event(outcomes: &[Option<ValidationOutcome>]) -> ValidationEvent {
    if outcomes.contains(&Some(ValidationOutcome::Fail)) {
        ValidationEvent::Fail
    } else if outcomes.iter().all(|o| *o == Some(ValidationOutcome::Pass)) {
        ValidationEvent::Pass
    } else {
        ValidationEvent::NoVerdict
    }
}

/// "Security reviewer passed", "Security reviewer failed", "Performance reviewer ended without a verdict".
pub(crate) fn verdict_of(agent: &str, outcome: Option<ValidationOutcome>) -> String {
    match outcome {
        Some(ValidationOutcome::Pass) => format!("{agent} passed"),
        Some(ValidationOutcome::Fail) => format!("{agent} failed"),
        _ => format!("{agent} ended without a verdict"),
    }
}

/// The check agents of a round of `stage` for `ticket` (d0130), as the stage lists them: the ticket's chosen
/// option takes the first one's place (Q517). Empty for a stage that lists none.
async fn round_agents(
    tx: &mut Transaction<'_, Sqlite>,
    ticket: &Ticket,
    stage: &Stage,
) -> Result<Vec<String>, BoardError> {
    let mut agents = stage.check_agents().to_vec();
    if agents.is_empty() || ticket.is_epic() {
        return Ok(agents);
    }
    let chosen: Option<String> = sqlx::query_scalar(
        "SELECT a.name FROM ticket_stage_choices c JOIN agents a ON a.id = c.agent_id
         WHERE c.ticket_id = ? AND c.stage_id = ? AND c.choice = 'agent'",
    )
    .bind(&ticket.id)
    .bind(&stage.id)
    .fetch_optional(&mut **tx)
    .await?;
    let option = chosen.and_then(|chosen| {
        stage
            .option_agents()
            .find(|name| name.trim().to_lowercase() == chosen.trim().to_lowercase())
    });
    if let Some(option) = option {
        agents[0] = option.clone();
    }
    Ok(agents)
}

/// The name of the agent check a ticket at `place` is in ("Validate", "Security"); "The check" when it isn't in one.
pub fn check_name(flow: &Flow, place: &Place) -> String {
    place
        .stage()
        .and_then(|id| flow.stage(id))
        .filter(|s| s.is_agent_check())
        .map_or_else(|| "The check".to_owned(), |s| s.name.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ValidationEvent as E;

    fn state(status: Status) -> ValidationState<'static> {
        ValidationState {
            flow: &crate::flow::default_file().flow,
            place: Box::leak(Box::new(Place::of(status, None, None))),
            status,
            fails: 0,
            rounds: 3,
            skips: &[],
        }
    }

    /// Validation off: the derived flow has no Validate stage.
    fn without_validation(s: ValidationState<'static>) -> ValidationState<'static> {
        let mut flow = crate::flow::default_flow();
        flow.stages
            .retain(|s| s.id != crate::flow::default::VALIDATE);
        ValidationState {
            flow: Box::leak(Box::new(flow)),
            ..s
        }
    }

    /// T-0195 (VAL-12): only "Not covered by any ticket" notes make a ticket; the gap is its title.
    #[test]
    fn a_ticket_from_a_not_covered_note() {
        assert_eq!(
            not_covered("Not covered by any ticket: refunds endpoint."),
            Some("refunds endpoint")
        );
        assert_eq!(
            not_covered("not covered by any ticket:removing items"),
            Some("removing items")
        );
        assert_eq!(not_covered("covered by SHOP-40.2"), None);
        assert_eq!(not_covered("Not covered by any ticket:"), None);
        let source = Ticket {
            id: "t401".into(),
            key: "SHOP-40.1".parse().unwrap(),
            kind: crate::domain::Kind::Ticket,
            title: "Save items".into(),
            body: String::new(),
            priority: Default::default(),
            priority_inherited: true,
            epic: Some("SHOP-40".parse().unwrap()),
            place: Place::stage_of(crate::flow::default::REVIEW),
            status: Status::WaitingForYou,
            base: crate::board::tickets::Base::Branch("main".into()),
            base_locked: true,
            external_id: None,
            depends_on: vec![],
            aliases: vec![],
            next_child_number: None,
            flags: crate::domain::Flags::default(),
            created_by: Actor::User("me".into()),
            updated_by: None,
            started_at: Some(1),
            created_at: 0,
            updated_at: 0,
            status_since: None,
            queued_at: None,
            ready_since: None,
            validation_round: 1,
            auto_merge_conflicts: 0,
            auto_merge_stop: None,
            work: None,
            start_problem: None,
            interrupted_from: None,
            signal: None,
            question: None,
            pushed_differs: None,
            epic_queue_mode: Default::default(),
            joined_queue_by: None,
        };
        let input = ticket_from_note(&source, "Not covered by any ticket: removing items").unwrap();
        assert_eq!(input.title, "Removing items");
        assert_eq!(input.project, "SHOP");
        assert_eq!(input.epic.as_deref(), Some("SHOP-40"));
        assert!(input.body.contains("SHOP-40.1 (Save items)"));
        assert!(ticket_from_note(&source, "covered by SHOP-40.2").is_none());
    }

    fn go(s: ValidationState, event: ValidationEvent) -> Transition {
        transition(s, event).unwrap_or_else(|e| panic!("{e:?}"))
    }

    /// Working → Validating on the agent's `stage next` and on the user's Move to next stage; with validation off,
    /// straight to Ready for Review.
    #[test]
    fn submit_starts_a_validator() {
        for event in [E::AgentSubmit, E::UserNextStage] {
            let t = go(state(Status::Working), event);
            assert_eq!(t.to, Status::Checking);
            assert!(t.start_validator && t.counters == Counters::Keep && t.outcome.is_none());
        }
        let off = without_validation(state(Status::Working));
        let t = go(off, E::AgentSubmit);
        assert_eq!((t.to, t.entry), (Status::WaitingForYou, None));
        assert!(!t.start_validator);
        assert!(transition(state(Status::Checking), E::AgentSubmit).is_err());
    }

    /// T-0406 (d0108 rule 10): the user's Move to next stage counts as the agent's `stage next`: the check runs (it
    /// used to skip validation), also from Interrupted; not while it validates.
    #[test]
    fn the_users_move_to_next_stage_runs_the_check() {
        for status in [Status::Idle, Status::Interrupted] {
            let t = go(state(status), E::UserNextStage);
            assert_eq!((t.to, t.entry), (Status::Checking, None), "{status:?}");
            assert!(t.start_validator);
        }
        assert!(transition(state(Status::Interrupted), E::AgentSubmit).is_err());
        assert!(transition(state(Status::Checking), E::UserNextStage).is_err());
    }

    /// Validating → Ready for Review "Validated · round N" on pass.
    #[test]
    fn pass() {
        let t = go(state(Status::Checking), E::Pass);
        assert_eq!(
            (t.to, t.entry, t.counters, t.outcome),
            (
                Status::WaitingForYou,
                Some(ReviewEntry::Validated),
                Counters::Count,
                Some(ValidationOutcome::Pass),
            )
        );
    }

    /// Fail with rounds left → back to the agent with the findings; the last round → turquoise, nothing delivered.
    #[test]
    fn fail_and_the_last_fail() {
        let t = go(state(Status::Checking), E::Fail);
        assert_eq!(
            (t.to, t.deliver_findings, t.counters),
            (Status::Working, true, Counters::Count)
        );
        let last = ValidationState {
            fails: 2,
            ..state(Status::Checking)
        };
        let t = go(last, E::Fail);
        assert_eq!(
            (t.to, t.deliver_findings, t.counters, t.outcome),
            (
                Status::CheckFailed,
                false,
                Counters::Count,
                Some(ValidationOutcome::Fail)
            )
        );
        // One round: the first failure is the last; five rounds: the fourth isn't.
        let one = ValidationState {
            rounds: 1,
            ..state(Status::Checking)
        };
        assert_eq!(go(one, E::Fail).to, Status::CheckFailed);
        let five = ValidationState {
            rounds: 5,
            fails: 3,
            ..state(Status::Checking)
        };
        assert_eq!(go(five, E::Fail).to, Status::Working);
    }

    /// No verdict and a discarded verdict: turquoise, not a round.
    #[test]
    fn no_verdict() {
        for (event, outcome) in [
            (E::NoVerdict, ValidationOutcome::NoResult),
            (E::ValidatorChangedFiles, ValidationOutcome::Discarded),
        ] {
            let t = go(state(Status::Checking), event);
            assert_eq!(
                (t.to, t.counters, t.outcome),
                (Status::CheckNoResult, Counters::Keep, Some(outcome))
            );
            assert!(!outcome.counts());
        }
    }

    #[test]
    fn validator_permission_prompt_and_back() {
        assert_eq!(
            go(state(Status::Checking), E::ValidatorNeedsYou).to,
            Status::CheckerNeedsYou
        );
        assert_eq!(
            go(state(Status::CheckerNeedsYou), E::ValidatorContinues).to,
            Status::Checking
        );
        // A verdict also lands while the prompt shows.
        assert_eq!(
            go(state(Status::CheckerNeedsYou), E::Pass).to,
            Status::WaitingForYou
        );
    }

    /// Code changed, Stop this check and Skip this check: none counts as a round.
    #[test]
    fn changes_stop_and_skip() {
        let t = go(state(Status::Checking), E::CodeChanged);
        assert_eq!(
            (t.to, t.tell_cancelled, t.counters, t.outcome),
            (
                Status::Working,
                true,
                Counters::Keep,
                Some(ValidationOutcome::Cancelled)
            )
        );
        let t = go(state(Status::Checking), E::StopValidation);
        assert_eq!(
            (t.to, t.tell_cancelled, t.outcome),
            (Status::Working, false, Some(ValidationOutcome::Stopped))
        );
        let t = go(state(Status::Checking), E::SkipCheck);
        assert_eq!(
            (t.to, t.entry),
            (Status::WaitingForYou, Some(ReviewEntry::Skipped))
        );
        for event in [E::StopValidation, E::SkipCheck, E::CodeChanged] {
            assert!(
                transition(state(Status::Working), event).is_err(),
                "{event:?}"
            );
        }
    }

    /// After the last failed round: Move on anyway, Give it another round, Answer the agent.
    #[test]
    fn after_the_last_failed_round() {
        let failed = state(Status::CheckFailed);
        let t = go(failed, E::MoveOnAnyway);
        assert_eq!(
            (t.to, t.entry),
            (Status::WaitingForYou, Some(ReviewEntry::FailedMoved))
        );
        let t = go(failed, E::AnotherRound);
        assert_eq!(
            (t.to, t.deliver_findings, t.extra_round, t.counters),
            (Status::Working, true, true, Counters::Keep)
        );
        let t = go(failed, E::AnswerAgent);
        assert_eq!(
            (t.to, t.deliver_findings, t.counters),
            (Status::Working, true, Counters::ResetThis)
        );
    }

    /// Ended without a result: Retry validation (a fresh validator, not a round) or Move on anyway
    /// ("Validation skipped by you").
    #[test]
    fn after_no_result() {
        let t = go(state(Status::CheckNoResult), E::RetryValidation);
        assert_eq!(
            (t.to, t.start_validator, t.counters),
            (Status::Checking, true, Counters::Keep)
        );
        let t = go(state(Status::CheckNoResult), E::MoveOnAnyway);
        assert_eq!(t.entry, Some(ReviewEntry::Skipped));
        assert!(transition(state(Status::CheckFailed), E::RetryValidation).is_err());
    }

    /// The fix after a Delivery's conflict validates again with the counters at 0 (FLW-17).
    #[test]
    fn a_conflict_fix_validates_from_zero() {
        let t = go(state(Status::MergeConflictFixing), E::AgentSubmit);
        assert_eq!((t.to, t.counters), (Status::Checking, Counters::ResetAll));
    }

    fn every_kind() -> Flow {
        let text = include_str!("../../tests/flows/every-kind.yaml");
        crate::flow::yaml::read(text).unwrap().flow
    }

    fn count(stage: &str, fails: u32, passed: Option<u32>, at: i64) -> CheckCount {
        CheckCount {
            stage: stage.into(),
            fails,
            passed_round: passed,
            updated_at: at,
            ..CheckCount::default()
        }
    }

    /// T-0417 (FLW-13): the markers count for the check the card is in, else the one that counted last, else the
    /// next check.
    #[test]
    fn the_check_a_card_counts_for() {
        let flow = every_kind();
        let counts = CheckCounts(vec![
            count("validate", 0, Some(2), 10),
            count("security", 1, None, 20),
        ]);
        let id = |p: &Place| counts.counting(&flow, p).map(|s| s.id.clone());
        assert_eq!(
            id(&Place::stage_of("validate")).as_deref(),
            Some("validate")
        );
        assert_eq!(id(&Place::stage_of("dev")).as_deref(), Some("security"));
        assert_eq!(
            CheckCounts::default()
                .counting(&flow, &Place::stage_of("dev"))
                .map(|s| s.id.as_str()),
            Some("validate")
        );
        assert_eq!(counts.of("security").shown_round(), 1);
        assert_eq!(counts.of("validate").shown_round(), 2);
        assert_eq!(
            counts.of("qa"),
            CheckCount {
                stage: "qa".into(),
                ..CheckCount::default()
            }
        );
    }

    /// T-0417: every check the user moved the card past, in flow order.
    #[test]
    fn why_the_delivery_waits_for_a_click() {
        let flow = every_kind();
        assert_eq!(CheckCounts::default().moved_past_reason(&flow), None);
        let counts = CheckCounts(vec![
            CheckCount {
                moved_past: Some(MovedPast::Failed),
                ..count("security", 3, None, 1)
            },
            CheckCount {
                moved_past: Some(MovedPast::Skipped),
                ..count("validate", 0, None, 2)
            },
        ]);
        assert_eq!(
            counts.moved_past_reason(&flow).as_deref(),
            Some("Skipped Validate · by you · Failed Security 3 times · moved by you")
        );
    }

    /// T-0417 (Q277): Skip this check on a check held for the plan limit; never on a held Agent stage.
    #[test]
    fn skip_on_a_check_held_for_the_limit() {
        let flow = every_kind();
        let held = |stage: &str| {
            let place = Place::stage_of(stage);
            transition(
                ValidationState {
                    flow: &flow,
                    place: &place,
                    status: Status::WaitingForLimitReset,
                    fails: 0,
                    rounds: 2,
                    skips: &[],
                },
                E::SkipCheck,
            )
        };
        let t = held("security").unwrap();
        assert_eq!(
            (t.place, t.to, t.outcome),
            (
                Place::stage_of("qa"),
                Status::WaitingForYou,
                Some(ValidationOutcome::Skipped)
            )
        );
        assert!(held("dev").is_err());
    }
}
