//! Human checks (FLW-14, STG-06; d0108 rule 6, "After a Fail", smaller rule 13): the user's **Pass** moves the card
//! on, and a card waiting in one says how it got there.
//!
//! - [`Board::pass_step`] is where a Pass goes, as the flow engine says: directly before a click Delivery it offers
//!   the delivery choices (today's Approve → Local merge / Open PR) and the card stays; otherwise it moves to the next
//!   stage the ticket runs. [`Board::record_pass`] writes a move that starts no checker (the next human check, the
//!   wait before an Agent stage, a Delivery); [`crate::review::Review::pass`] carries the Pass out, a checker included.
//! - The **Fail** is Send back's bookkeeping ([`super::send_back`]): the check's fails-go-to stage
//!   ([`crate::flow::engine::fail_to`], d0131) resumes with the comments, every agent check's counter restarts, and
//!   the card walks every later stage again.
//! - [`Board::arrival`]: "Passed Security · round 2", "Skipped Security · by you", "Moved back by you", read from the
//!   ticket's stage runs (STG-05), so every way in (a check's pass, a skip, a drag back, PR rework) is covered
//!   without a column of its own.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::json;

use super::stage_runs::{Moved, StageRun};
use super::tickets::{EventKind, Ticket, record_event};
use super::{Board, BoardError, Refusal};
use crate::domain::{Actor, Status, StatusSet};
use crate::flow::Flow;
use crate::flow::engine::{Move, RunResult};
use crate::store::now_ms;
use crate::store::tickets::set_state_since;

/// How a card came to wait in a human check (FLW-14), from the stage run before the check's own.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Arrival {
    /// An agent check passed it: "Passed Security · round 2"; a human check's Pass: "Passed QA".
    Passed {
        stage: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        round: Option<u32>,
    },
    /// The stage before was skipped (Skip this check, a skip choice, a check with no result moved on): "Skipped
    /// Security · by you"; `by` names the agent whose choice skipped it ("Skipped Grill · by Griller", Q305).
    Skipped {
        stage: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        by: Option<String>,
    },
    /// An agent check failed at its limit and the user moved it on: "Failed Security 3 times · moved by you".
    FailedMovedOn { stage: String, times: u32 },
    /// The stage before was a check of yours the ticket skipped on its own, since no Agent stage before it ran (0129
    /// row 4): "Skipped Spec review · nothing before it runs".
    SkippedOnItsOwn { stage: String },
    /// It came from a later stage (a drag back, PR rework): "Moved back by you".
    MovedBack,
    /// An Agent stage finished straight into the check: "Finished Dev".
    Finished { stage: String },
}

impl Arrival {
    /// The card's line.
    pub fn label(&self) -> String {
        match self {
            Self::Passed {
                stage,
                round: Some(round),
            } => format!("Passed {stage} \u{b7} round {round}"),
            Self::Passed { stage, round: None } => format!("Passed {stage}"),
            Self::Skipped { stage, by } => format!(
                "Skipped {stage} \u{b7} by {}",
                by.as_deref().unwrap_or("you")
            ),
            Self::FailedMovedOn { stage, times: 1 } => {
                format!("Failed {stage} 1 time \u{b7} moved by you")
            }
            Self::FailedMovedOn { stage, times } => {
                format!("Failed {stage} {times} times \u{b7} moved by you")
            }
            Self::SkippedOnItsOwn { stage } => format!(
                "Skipped {stage} \u{b7} {}",
                super::stage_choices::NOTHING_BEFORE_IT_RUNS
            ),
            Self::MovedBack => "Moved back by you".to_owned(),
            Self::Finished { stage } => format!("Finished {stage}"),
        }
    }
}

impl Board {
    /// Where the user's Pass takes `ticket` (STG-06): refused unless it waits in a human check.
    pub(crate) async fn pass_step(&self, ticket: &Ticket) -> Result<Moved, BoardError> {
        if ticket.status != Status::WaitingForYou {
            return Err(BoardError::refused(
                Refusal::MoveRefused,
                format!(
                    "{} is {}; Pass works on a ticket waiting in a human check",
                    ticket.key,
                    ticket.status.label()
                ),
            ));
        }
        self.moved(ticket, Status::WaitingForYou, Move::HumanPass)
            .await
    }

    /// A Pass that starts no checker (FLW-14): the human check's run ends Passed, the stages walked over get their
    /// skipped runs, and the ticket is in the next human check (waiting for the user again), in the wait before an
    /// Agent stage (Queued, counted from now, STG-07), or in its Delivery (To deliver).
    pub(crate) async fn record_pass(
        &self,
        ticket: &Ticket,
        moved: &Moved,
        actor: &Actor,
    ) -> Result<Ticket, BoardError> {
        let flow = self.effective_flow(ticket.key.project.as_str()).await?;
        let from = ticket.place.stage().unwrap_or_default();
        let name = flow
            .stage(from)
            .map_or(from, |s| s.name.as_str())
            .to_owned();
        let waits = moved.place.is_waiting();
        let into_check = moved.status.set() == StatusSet::HumanCheck;
        let now = now_ms();
        let mut tx = self.store.begin_write().await?;
        moved.record(&mut tx, &ticket.id, None).await?;
        sqlx::query(concat!(
            "UPDATE tickets SET stage_id = ?, state = ?, review_entry = NULL,
                                review_entered_at = CASE WHEN ? THEN ? ELSE review_entered_at END,
                                queued_at = CASE WHEN ? THEN ? ELSE queued_at END,
                                ready_since = CASE WHEN ? THEN NULL ELSE ready_since END,
                                signal_text = NULL, signal_since = NULL, wakeup_at = NULL,
                                updated_by = ?, updated_at = ?, ",
            set_state_since!(),
            " WHERE id = ?"
        ))
        .bind(moved.place.stage_id())
        .bind(moved.status.as_str())
        .bind(into_check)
        .bind(now)
        .bind(waits)
        .bind(now)
        .bind(waits)
        .bind(actor.to_string())
        .bind(now)
        .bind(moved.status.as_str())
        .bind(now)
        .bind(&ticket.id)
        .execute(&mut *tx)
        .await?;
        record_event(
            &mut tx,
            &ticket.id,
            EventKind::Passed,
            actor,
            json!({ "stage": name, "stage_id": from }),
        )
        .await?;
        tx.commit().await?;
        tracing::info!(key = %ticket.key, stage = from, to = %moved.status, "passed");
        self.ticket_by_id(&ticket.id).await
    }

    /// A Pass into an agent check, before the check opens its round ([`crate::review::Review::pass`]): the human
    /// check's run ends Passed (the round's own bookkeeping would end it as Done) and the stages walked over get
    /// their skipped runs.
    pub(crate) async fn record_pass_runs(
        &self,
        ticket: &Ticket,
        moved: &Moved,
    ) -> Result<(), BoardError> {
        let mut tx = self.store.begin_write().await?;
        moved.record(&mut tx, &ticket.id, None).await?;
        tx.commit().await?;
        Ok(())
    }

    /// How the ticket came to wait in the human check it is in (FLW-14): from the stage run before the check's
    /// latest. `None` when it isn't waiting in a human check, or nothing ran before.
    pub async fn arrival(&self, key: &str) -> Result<Option<Arrival>, BoardError> {
        let ticket = self.ticket(key).await?;
        if ticket.status.set() != StatusSet::HumanCheck {
            return Ok(None);
        }
        let Some(here) = ticket.place.stage() else {
            return Ok(None);
        };
        let flow = self.effective_flow(ticket.key.project.as_str()).await?;
        let runs = self.stage_runs(key).await?;
        let Some(at) = runs.iter().rposition(|r| r.stage == here) else {
            return Ok(None);
        };
        let Some(before) = runs[..at].last() else {
            return Ok(None);
        };
        let skipped_by: Vec<(String, String)> = sqlx::query_as(
            "SELECT stage_id, by_name FROM ticket_stage_choices
             WHERE ticket_id = ? AND choice = 'skip' AND set_by <> 'user' AND by_name IS NOT NULL",
        )
        .bind(&ticket.id)
        .fetch_all(self.store.pool())
        .await?;
        let arrival = with_chooser(
            arrival_from(&flow, here, before),
            &before.stage,
            &skipped_by,
        );
        self.on_its_own(&ticket.id, &flow, &before.stage, arrival)
            .await
            .map(Some)
    }

    /// A skip of a check the ticket skipped on its own says so (0129 row 4): "Skipped Spec review · nothing before
    /// it runs".
    async fn on_its_own(
        &self,
        ticket_id: &str,
        flow: &Flow,
        stage: &str,
        arrival: Arrival,
    ) -> Result<Arrival, BoardError> {
        let Arrival::Skipped {
            stage: name,
            by: None,
        } = arrival
        else {
            return Ok(arrival);
        };
        if flow
            .stage(stage)
            .is_some_and(|s| s.is_human_check() && s.can_skip())
            && self
                .skip_sets(ticket_id, flow)
                .await?
                .1
                .iter()
                .any(|s| s == stage)
        {
            return Ok(Arrival::SkippedOnItsOwn { stage: name });
        }
        Ok(Arrival::Skipped {
            stage: name,
            by: None,
        })
    }

    /// [`Board::arrival`] for every ticket of a project that waits in a human check or at its Delivery (To deliver),
    /// by ticket id: the cards' "Passed Security · round 2", "Skipped Security · by you" (FLW-14, FLW-15). `flow` is
    /// the project's applied flow.
    pub(crate) async fn project_arrivals(
        &self,
        project_id: &str,
        flow: &Flow,
    ) -> Result<HashMap<String, Arrival>, BoardError> {
        let waiting: Vec<&str> = Status::ALL
            .iter()
            .filter(|s| s.set() == StatusSet::HumanCheck || **s == Status::ToDeliver)
            .map(|s| s.as_str())
            .collect();
        let states = waiting
            .iter()
            .map(|s| format!("'{s}'"))
            .collect::<Vec<_>>()
            .join(", ");
        type Row = (String, Option<String>, String, Option<i64>, Option<String>);
        let sql = format!(
            "SELECT t.id, t.stage_id, r.stage_id, r.round, r.result
             FROM stage_runs r JOIN tickets t ON t.id = r.ticket_id
             WHERE t.project_id = ? AND t.state IN ({states})
             ORDER BY t.id, r.started_at, r.rowid"
        );
        let rows: Vec<Row> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
            .bind(project_id)
            .fetch_all(self.store.pool())
            .await?;
        let choices: Vec<(String, String, String)> = sqlx::query_as(
            "SELECT c.ticket_id, c.stage_id, c.by_name FROM ticket_stage_choices c
             JOIN tickets t ON t.id = c.ticket_id
             WHERE t.project_id = ? AND c.choice = 'skip' AND c.set_by <> 'user' AND c.by_name IS NOT NULL",
        )
        .bind(project_id)
        .fetch_all(self.store.pool())
        .await?;
        let mut runs: HashMap<String, (Option<String>, Vec<StageRun>)> = HashMap::new();
        for (id, here, stage, round, result) in rows {
            let entry = runs.entry(id).or_insert_with(|| (here, Vec::new()));
            entry.1.push(StageRun {
                stage,
                kind: String::new(),
                session: None,
                agent: None,
                model: None,
                round: round.and_then(|r| u32::try_from(r).ok()),
                result: result.and_then(|r| r.parse().ok()),
                summary: None,
                started_at: 0,
                ended_at: None,
            });
        }
        let mut arrivals = HashMap::new();
        for (id, (here, runs)) in runs {
            let Some(here) = here else { continue };
            let Some(at) = runs.iter().rposition(|r| r.stage == here) else {
                continue;
            };
            let Some(before) = runs[..at].last() else {
                continue;
            };
            let skipped_by: Vec<(String, String)> = choices
                .iter()
                .filter(|(t, _, _)| *t == id)
                .map(|(_, stage, by)| (stage.clone(), by.clone()))
                .collect();
            let arrival = with_chooser(
                arrival_from(flow, &here, before),
                &before.stage,
                &skipped_by,
            );
            let arrival = self.on_its_own(&id, flow, &before.stage, arrival).await?;
            arrivals.insert(id, arrival);
        }
        Ok(arrivals)
    }
}

/// A skip the ticket's own choice made names the agent that chose it (Q305); the user's skips and choices stay
/// "by you".
fn with_chooser(arrival: Arrival, stage: &str, skipped_by: &[(String, String)]) -> Arrival {
    match arrival {
        Arrival::Skipped {
            stage: name,
            by: None,
        } => Arrival::Skipped {
            stage: name,
            by: skipped_by
                .iter()
                .find(|(s, _)| s == stage)
                .map(|(_, by)| by.clone()),
        },
        other => other,
    }
}

/// [`Board::arrival`]'s rule, from the run `before` the human check `here`.
fn arrival_from(flow: &Flow, here: &str, before: &StageRun) -> Arrival {
    let later = matches!(
        (flow.position(&before.stage), flow.position(here)),
        (Some(b), Some(h)) if b > h
    );
    let stage = flow
        .stage(&before.stage)
        .map_or(before.stage.as_str(), |s| s.name.as_str())
        .to_owned();
    match before.result {
        _ if later => Arrival::MovedBack,
        Some(RunResult::Passed) => Arrival::Passed {
            stage,
            round: before.round,
        },
        Some(RunResult::Failed) => Arrival::FailedMovedOn {
            stage,
            times: before.round.unwrap_or(1).max(1),
        },
        Some(RunResult::Skipped | RunResult::NoResult) => Arrival::Skipped { stage, by: None },
        _ => Arrival::Finished { stage },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(stage: &str, result: RunResult, round: Option<u32>) -> StageRun {
        StageRun {
            stage: stage.into(),
            kind: String::new(),
            session: None,
            agent: None,
            model: None,
            round,
            result: Some(result),
            summary: None,
            started_at: 0,
            ended_at: Some(0),
        }
    }

    fn every_kind() -> Flow {
        let yaml = include_str!("../../tests/flows/every-kind.yaml");
        crate::flow::yaml::read(yaml).unwrap().flow
    }

    #[test]
    fn the_run_before_says_how_it_got_there() {
        let flow = every_kind();
        let label = |r: StageRun| arrival_from(&flow, "qa", &r).label();
        assert_eq!(
            label(run("security", RunResult::Passed, Some(2))),
            "Passed Security \u{b7} round 2"
        );
        assert_eq!(
            label(run("security", RunResult::Skipped, None)),
            "Skipped Security \u{b7} by you"
        );
        assert_eq!(
            label(run("security", RunResult::Failed, Some(2))),
            "Failed Security 2 times \u{b7} moved by you"
        );
        assert_eq!(
            label(run("ship", RunResult::Cancelled, None)),
            "Moved back by you"
        );
        assert_eq!(label(run("dev", RunResult::Done, None)), "Finished Dev");
    }
}
