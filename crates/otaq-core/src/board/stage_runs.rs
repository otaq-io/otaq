//! Stage runs (STG-05, d0108): every time a ticket runs a stage, the stage, the session, the agent and exact model,
//! an agent check's round, how it ended and the comment it ended with (CMT-05). Validation rounds are the runs of the
//! agent check.
//!
//! The rows follow the tickets by themselves (migration 0046's triggers): a ticket whose stage changes ends its open
//! run and opens one of the stage it entered, with that stage's live session; a session, a validation round and its
//! end fill in the rest. A move that knows how its run ended says so first ([`end_open`]), and the stages a ticket
//! skipped get a run of their own ([`record`]).

use serde::{Deserialize, Serialize};
use sqlx::{Sqlite, Transaction};

use super::tickets::Ticket;
use super::{Board, BoardError, Refusal};
use crate::domain::{Place, Status};
use crate::flow::StageKind;
use crate::flow::engine::{self, At, Move, RunResult, Step, run_kind};
use crate::flow::model::Delivers;
use crate::store::{new_id, now_ms};

/// One run of one stage, as History and the next stage's agent will read it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StageRun {
    pub stage: String,
    /// `agent`, `check` (an agent check), `you` (a human check) or `delivery`.
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session: Option<String>,
    /// The agent's name when the session started.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent: Option<String>,
    /// The exact model it ran on, once the tool reported it (else the model as passed).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// An agent check's round.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub round: Option<u32>,
    /// `None` while it runs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<RunResult>,
    /// The text of the comment the run ended with (the agent's `stage next`, the checker's verdict, the user's Send
    /// back note), unless it was deleted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    pub started_at: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ended_at: Option<i64>,
}

/// Ends the ticket's open run with `result` (and the hand-over `comment` it ended with, when one was posted), before
/// the move that leaves the stage.
pub(crate) async fn end_open(
    tx: &mut Transaction<'_, Sqlite>,
    ticket_id: &str,
    result: RunResult,
    comment: Option<&str>,
) -> Result<(), BoardError> {
    sqlx::query(
        "UPDATE stage_runs SET ended_at = ?, result = COALESCE(result, ?), comment_id = COALESCE(?, comment_id)
         WHERE ticket_id = ? AND ended_at IS NULL",
    )
    .bind(now_ms())
    .bind(result.as_str())
    .bind(comment)
    .bind(ticket_id)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

/// Otaq started or resumed the ticket's open run with a new comment (a Send back, a drag back, a failed check; CMT-05):
/// its agent has to run `prime` again before it ends its stage (CMT-08 reads `handed_over_at`). A ticket queued to
/// come back has no open run yet: the run its start opens is new anyway.
pub(crate) async fn mark_handed_over(
    tx: &mut Transaction<'_, Sqlite>,
    ticket_id: &str,
) -> Result<(), BoardError> {
    sqlx::query(
        "UPDATE stage_runs SET handed_over_at = ? WHERE ticket_id = ? AND ended_at IS NULL AND kind IN ('agent', 'check')",
    )
    .bind(now_ms())
    .bind(ticket_id)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

/// Where a move takes a ticket in its project's flow ([`Board::moved`]), owned so it outlives the flow.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Moved {
    pub place: Place,
    pub status: Status,
    /// How the run of the stage it leaves ends.
    pub ended: Option<RunResult>,
    /// Runs that start and end at once (skipped stages, a Delivery finished as it was entered).
    pub through: Vec<(String, StageKind, RunResult)>,
    /// A human check's Pass offers these delivery choices; the ticket stays put ([`Step::choose`]).
    pub choose: Option<Delivers>,
    /// It entered an agent check: a checker starts ([`Step::start_check`]).
    pub start_check: bool,
    /// It entered an automatic Delivery: it delivers with no click ([`Step::automatic`]).
    pub automatic: bool,
}

impl Moved {
    pub(crate) fn of(step: &Step<'_>) -> Self {
        Self {
            place: step.place.clone(),
            status: step.status,
            ended: step.ended,
            through: step
                .passed_through
                .iter()
                .map(|(stage, result)| (stage.id.clone(), stage.kind, *result))
                .collect(),
            choose: step.choose,
            start_check: step.start_check,
            automatic: step.automatic,
        }
    }

    /// The move's runs, before the move is written in the same transaction: the open run ends as the move says (with
    /// the hand-over `comment` it ended with), and the stages it walked over get theirs.
    pub(crate) async fn record(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        ticket_id: &str,
        comment: Option<&str>,
    ) -> Result<(), BoardError> {
        if let Some(result) = self.ended {
            end_open(tx, ticket_id, result, comment).await?;
        }
        for (stage, kind, result) in &self.through {
            record_kind(tx, ticket_id, stage, *kind, *result).await?;
        }
        Ok(())
    }
}

/// Records a run that starts and ends at once: a stage the ticket skipped, or a Delivery finished as it was entered.
pub(crate) async fn record_kind(
    tx: &mut Transaction<'_, Sqlite>,
    ticket_id: &str,
    stage_id: &str,
    kind: StageKind,
    result: RunResult,
) -> Result<(), BoardError> {
    let now = now_ms();
    sqlx::query(
        "INSERT INTO stage_runs (id, ticket_id, stage_id, kind, result, started_at, ended_at)
         VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(new_id())
    .bind(ticket_id)
    .bind(stage_id)
    .bind(run_kind(kind))
    .bind(result.as_str())
    .bind(now)
    .bind(now)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

impl Board {
    /// Where `mv` takes the ticket in its project's flow (the flow engine's step, STG-06), as if its status were
    /// `status` (the caller checked the status it accepts). Refused with the engine's reason.
    pub(crate) async fn moved(
        &self,
        ticket: &Ticket,
        status: Status,
        mv: Move,
    ) -> Result<Moved, BoardError> {
        let flow = self.effective_flow(ticket.key.project.as_str()).await?;
        // The stages it skips by its choices (AGM-08): walked over with a skipped run.
        let skips = self.ticket_skips(ticket, &flow).await?;
        let at = At {
            skips: &skips,
            ..At::new(&flow, &ticket.place, status)
        };
        engine::step(at, mv)
            .map(|step| Moved::of(&step))
            .map_err(|e| BoardError::refused(Refusal::MoveRefused, e.0))
    }

    /// Every stage run of the ticket, oldest first.
    pub async fn stage_runs(&self, key: &str) -> Result<Vec<StageRun>, BoardError> {
        let id = self.ticket_id(key).await?;
        type Row = (
            String,
            String,
            Option<String>,
            Option<String>,
            Option<String>,
            Option<i64>,
            Option<String>,
            Option<String>,
            i64,
            Option<i64>,
        );
        let rows: Vec<Row> = sqlx::query_as(
            "SELECT r.stage_id, r.kind, r.session_id, r.agent, r.model, r.round, r.result, c.body, r.started_at,
                    r.ended_at
             FROM stage_runs r
             LEFT JOIN ticket_comments c ON c.id = r.comment_id AND c.deleted_at IS NULL
             WHERE r.ticket_id = ? ORDER BY r.started_at, r.rowid",
        )
        .bind(&id)
        .fetch_all(self.store.pool())
        .await?;
        Ok(rows
            .into_iter()
            .map(
                |(
                    stage,
                    kind,
                    session,
                    agent,
                    model,
                    round,
                    result,
                    summary,
                    started_at,
                    ended_at,
                )| {
                    StageRun {
                        stage,
                        kind,
                        session,
                        agent,
                        model,
                        round: round.and_then(|r| u32::try_from(r).ok()),
                        result: result.and_then(|r| r.parse().ok()),
                        summary,
                        started_at,
                        ended_at,
                    }
                },
            )
            .collect())
    }
}
