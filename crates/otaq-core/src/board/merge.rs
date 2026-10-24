//! The Delivery's board side (GIT-03, EPC-03, FLW-15, FLW-17; d0015, d0038, d0059, d0108): where a ticket merges, the
//! squash message the confirmation proposes, the bookkeeping once it merged, and the Delivery's mode and conflicts.
//! The git work is [`crate::git::merge`]; the click is [`crate::review::Review::local_merge`], the automatic
//! Delivery [`crate::review::delivery`].
//!
//! - **Click or automatic** ([`delivery_is_automatic`]): a Delivery stage with `mode: automatic` delivers with no
//!   click, but only work that passed its checks (d0108 "Move on anyway"): every check before it must have passed in
//!   its latest run. A check moved past after it failed or ended without a verdict (Move on anyway), skipped by the
//!   user (Skip this check, Move to next stage past it) or a human check the ticket skips by its choice makes it click
//!   mode. An agent check the ticket skips by its own choice doesn't count either way. With no check passed at all,
//!   the automatic Delivery records "Delivered without checks" (d0108 rule 8).
//! - **Conflicts** (FLW-17, d0108 smaller rule 1): the rebase onto the target conflicted, in either mode. The ticket
//!   goes to its nearest earlier Agent stage, whose session fixes it with `restack` with no click ("Merge conflict ·
//!   fixing"); the conflicts count in a row and the third stops it there, turquoise ("Keeps conflicting with
//!   `main`"). A merge resets the count.

use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, OnceLock};

use serde::{Deserialize, Serialize};
use serde_json::json;

use super::epic_flight::{delivery_of, epic_flow};
use super::stage_runs::StageRun;
use super::tickets::{Base, EventKind, Ticket, record_event};
use super::{Board, BoardError, Refusal};
use crate::domain::{Actor, AutoMergeStop, Place, Status, TicketKey};
use crate::flow::engine::{self, Move, RunResult};
use crate::flow::{Delivers, Delivery, DeliveryMode, Flow, Stage};
use crate::naming;
use crate::store::tickets::set_state_since;
use crate::store::{new_id, now_ms};

/// Delivery conflicts in a row before the ticket stops for the user (d0108 smaller rule 1, as 0064 did).
pub const DELIVERY_CONFLICT_LIMIT: u32 = 3;

/// Whether a Delivery delivers with no click: its mode is automatic and every check before it passed in its latest
/// run (`runs`, oldest first). `skips` are the stages the ticket skips by its choices.
pub fn delivery_is_automatic(
    flow: &Flow,
    delivery: &Stage,
    skips: &[String],
    runs: &[StageRun],
) -> bool {
    let automatic = delivery
        .delivery
        .is_some_and(|d| d.mode == DeliveryMode::Automatic && d.delivers != Delivers::Either);
    automatic
        && checks_before(flow, delivery).all(|check| {
            let skipped = check.can_skip() && skips.contains(&check.id);
            match latest_run(runs, &check.id) {
                // A human check skipped by the ticket's choice always leads to a click (d0108).
                _ if skipped && check.is_human_check() => false,
                _ if skipped => true,
                Some(RunResult::Passed) => true,
                _ => false,
            }
        })
}

/// Whether no check before the Delivery passed in its latest run: an automatic Delivery then says "Delivered
/// without checks" (d0108 rule 8).
pub fn without_checks(flow: &Flow, delivery: &Stage, runs: &[StageRun]) -> bool {
    !checks_before(flow, delivery)
        .any(|check| latest_run(runs, &check.id) == Some(RunResult::Passed))
}

/// The check stages before `delivery`, in order.
fn checks_before<'f>(flow: &'f Flow, delivery: &Stage) -> impl Iterator<Item = &'f Stage> {
    let end = flow.position(&delivery.id).unwrap_or(flow.stages.len());
    flow.stages[..end]
        .iter()
        .filter(|s| s.is_agent_check() || s.is_human_check())
}

/// How the latest run of `stage` ended, if it ran and ended.
fn latest_run(runs: &[StageRun], stage: &str) -> Option<RunResult> {
    runs.iter()
        .rev()
        .find(|r| r.stage == stage)
        .and_then(|r| r.result)
}

/// A ticket's Delivery as it stands now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeliveryNow {
    /// The Delivery stage of the ticket's flow (an epic's: its own walk, d0108 rule 7).
    pub stage: Stage,
    /// What it delivers: an epic child always Local-merges into its epic's branch.
    pub delivery: Delivery,
    /// It delivers with no click: [`delivery_is_automatic`], and the user didn't move it past a check.
    pub automatic: bool,
    /// Why it waits for a click although its mode is automatic: "Failed Security 3 times · moved by you",
    /// "Skipped Security · by you" ([`Board::forced_click_reason`], T-0417).
    pub forced: Option<String>,
    /// No check passed before it ([`without_checks`]).
    pub without_checks: bool,
}

/// What runs a ticket's automatic Delivery once it entered its Delivery stage (the service sets it).
pub type ArrivedHook =
    Arc<dyn Fn(String) -> Pin<Box<dyn Future<Output = ()> + Send>> + Send + Sync>;

/// The automatic Delivery's side of board moves: every move that can put a ticket into its Delivery stage (a check's
/// pass, a submit, a human check's Pass) calls [`DeliveryLink::arrived`] afterwards. Cheap to clone.
#[derive(Clone, Default)]
pub struct DeliveryLink(Arc<OnceLock<ArrivedHook>>);

impl std::fmt::Debug for DeliveryLink {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("DeliveryLink")
    }
}

impl DeliveryLink {
    /// How an automatic Delivery runs (the service sets it once).
    pub fn set(&self, hook: ArrivedHook) {
        let _ = self.0.set(hook);
    }

    /// The ticket `ticket_id` may have entered its Delivery stage: an automatic Delivery runs now. Nothing happens
    /// for a ticket elsewhere, in a click-mode Delivery, or before the service set the hook.
    pub async fn arrived(&self, ticket_id: &str) {
        if let Some(hook) = self.0.get() {
            hook(ticket_id.to_owned()).await;
        }
    }
}

/// Where a ticket's Local merge goes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MergeTarget {
    /// The branch merged: `otaq/SHOP-18`, or an epic's `otaq/SHOP-40`.
    pub branch: String,
    /// The branch merged into: `main`, or the epic's branch for an epic child (EPC-03).
    pub target: String,
    /// The target is the epic's local integration branch (no fetch, d0059).
    pub into_epic: bool,
}

/// What the Local merge confirmation shows before the click: the target and an editable squash message.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MergeProposal {
    pub key: TicketKey,
    pub target: String,
    pub into_epic: bool,
    /// "SHOP-18: Checkout API" and the agent's summary.
    pub message: String,
}

impl Board {
    /// Where the ticket merges: a standalone ticket or an epic into its base branch, an epic child into its
    /// epic's branch.
    pub async fn merge_target(&self, ticket: &Ticket) -> Result<MergeTarget, BoardError> {
        let branch = ticket
            .work
            .as_ref()
            .and_then(|w| w.branch.clone())
            .ok_or_else(|| {
                BoardError::refused(
                    Refusal::NoBranch,
                    format!("{} has no branch to merge", ticket.key),
                )
            })?;
        Ok(match &ticket.base {
            Base::Branch(target) => MergeTarget {
                branch,
                target: target.clone(),
                into_epic: false,
            },
            Base::Epic(epic) => {
                let epic = self.ticket(&epic.to_string()).await?;
                MergeTarget {
                    branch,
                    target: epic
                        .work
                        .and_then(|w| w.branch)
                        .unwrap_or_else(|| naming::branch(&epic.key.to_string())),
                    into_epic: true,
                }
            }
        })
    }

    /// The proposed squash message: "SHOP-18: Checkout API", then the agent's summary (an epic lists its merged
    /// children).
    pub async fn merge_proposal(&self, key: &str) -> Result<MergeProposal, BoardError> {
        let ticket = self.ticket(key).await?;
        let target = self.merge_target(&ticket).await?;
        let mut message = format!("{}: {}", ticket.key, ticket.title);
        let body = if ticket.is_epic() {
            let children: Vec<String> = self
                .epic_children(key)
                .await?
                .into_iter()
                .filter(|c| c.status == Status::Merged)
                .map(|c| format!("- {} {}", c.key, c.title))
                .collect();
            (!children.is_empty()).then(|| children.join("\n"))
        } else {
            self.review_summary(&ticket.id).await?
        };
        if let Some(body) = body {
            message.push_str("\n\n");
            message.push_str(body.trim());
        }
        Ok(MergeProposal {
            key: ticket.key,
            target: target.target,
            into_epic: target.into_epic,
            message,
        })
    }

    /// After a Local merge: Done · Merged with the squash commit, and the review action recorded. `onto` is the
    /// target's commit the branch was rebased onto: the ticket's diff stays its own changes.
    pub async fn record_local_merge(
        &self,
        ticket: &Ticket,
        target: &str,
        commit: &str,
        onto: &str,
        actor: &Actor,
    ) -> Result<Ticket, BoardError> {
        let now = now_ms();
        let to = Status::Merged;
        let round = self.current_round(&ticket.id).await?;
        // The Delivery finished (STG-06): from the human check whose Pass offered it, or from the Delivery stage.
        let moved = self
            .moved(ticket, ticket.status, Move::Delivered)
            .await
            .ok();
        let mut tx = self.store.begin_write().await?;
        if let Some(moved) = &moved {
            moved.record(&mut tx, &ticket.id, None).await?;
        }
        sqlx::query(concat!(
            "UPDATE tickets SET stage_id = NULL, state = ?, outcome = 'merged', merge_commit_sha = ?,
                                base_sha = ?, sent_back_at = NULL, resume_skill = NULL, conflicts_with = NULL,
                                conflict_files = NULL, conflict_reason = NULL, auto_merge_conflicts = 0,
                                auto_merge_stop = NULL, signal_text = NULL, signal_since = NULL, wakeup_at = NULL,
                                merge_check_due = FALSE, updated_by = ?, updated_at = ?, ",
            set_state_since!(),
            " WHERE id = ?"
        ))
        .bind(to.as_str())
        .bind(commit)
        .bind(onto)
        .bind(actor.to_string())
        .bind(now)
        .bind(to.as_str())
        .bind(now)
        .bind(&ticket.id)
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "INSERT INTO reviews (id, ticket_id, round, action, at) VALUES (?, ?, ?, 'approve_local', ?)",
        )
        .bind(new_id())
        .bind(&ticket.id)
        .bind(i64::from(round))
        .bind(now)
        .execute(&mut *tx)
        .await?;
        record_event(
            &mut tx,
            &ticket.id,
            EventKind::Merged,
            actor,
            json!({ "from": ticket.status, "target": target, "commit": commit }),
        )
        .await?;
        tx.commit().await?;
        tracing::info!(key = %ticket.key, target, commit, "merged locally");
        self.ticket_by_id(&ticket.id).await
    }
}

impl Board {
    /// The ticket's Delivery now: its stage, what it delivers, and whether it runs with no click.
    pub async fn delivery_now(&self, ticket: &Ticket) -> Result<Option<DeliveryNow>, BoardError> {
        let flow = self.effective_flow(ticket.key.project.as_str()).await?;
        // An epic walks its own flow: its stop, then the Delivery (d0108 rule 7).
        let flow = if ticket.is_epic() {
            epic_flow(&flow)
        } else {
            flow
        };
        let (Some(stage), Some(delivery)) = (
            flow.stages.iter().find(|s| s.is_delivery()).cloned(),
            delivery_of(&flow, ticket),
        ) else {
            return Ok(None);
        };
        let skips = self.ticket_skips(ticket, &flow).await?;
        let runs = self.stage_runs(&ticket.key.to_string()).await?;
        let forced = self.forced_click_reason(ticket).await?;
        Ok(Some(DeliveryNow {
            automatic: forced.is_none() && delivery_is_automatic(&flow, &stage, &skips, &runs),
            forced,
            without_checks: without_checks(&flow, &stage, &runs),
            stage,
            delivery,
        }))
    }

    /// Whose session runs the ticket's Delivery (d0108 second gap review rule 6): the Dev stage's (an agent
    /// session) when the Delivery's agent is the agent that ran it, else a delivery session of the Delivery's own
    /// agent; an epic's is always a delivery session (EPC-08). A flow whose agents can't be read keeps the ticket's
    /// agent.
    pub async fn delivery_session_kind(&self, ticket: &Ticket) -> crate::session::SessionKind {
        use crate::session::SessionKind;
        if ticket.is_epic() {
            return SessionKind::Delivery;
        }
        match self.delivery_agent_is_devs(ticket).await {
            Ok(false) => SessionKind::Delivery,
            Ok(true) => SessionKind::Agent,
            Err(e) => {
                tracing::warn!(key = %ticket.key, error = %e, "can't tell the Delivery's agent: the ticket's agent");
                SessionKind::Agent
            }
        }
    }

    /// Whether the Delivery's agent is the agent that ran the ticket's nearest Agent stage before the Delivery (its
    /// choice for that stage, else the stage's agent).
    async fn delivery_agent_is_devs(&self, ticket: &Ticket) -> Result<bool, BoardError> {
        use crate::agents::resolve;
        let flow = self.effective_flow(ticket.key.project.as_str()).await?;
        let skips = self.ticket_skips(ticket, &flow).await?;
        let Some(dev) = flow
            .stages
            .iter()
            .find(|s| s.is_delivery())
            .and_then(|d| engine::back_to(&flow, &d.id, &skips))
        else {
            return Ok(false);
        };
        let delivery = resolve::delivery_agent(self, ticket.key.project.as_str()).await?;
        let dev = resolve::running_agent(self, ticket, &dev.id).await?;
        Ok(delivery.id == dev.id)
    }

    /// An automatic Delivery merged the ticket (AMG-02, FLW-15): "Merged automatically" (and "Delivered without
    /// checks" when no check passed before it), the conflict count back at 0, a History line.
    pub(crate) async fn record_delivered_automatically(
        &self,
        ticket_id: &str,
        target: &str,
        without_checks: bool,
    ) -> Result<(), BoardError> {
        let mut tx = self.store.begin_write().await?;
        sqlx::query(
            "UPDATE tickets SET merged_automatically = TRUE, delivered_without_checks = ?, auto_merge_conflicts = 0,
                                auto_merge_stop = NULL
             WHERE id = ?",
        )
        .bind(without_checks)
        .bind(ticket_id)
        .execute(&mut *tx)
        .await?;
        record_event(
            &mut tx,
            ticket_id,
            EventKind::MergedAutomatically,
            &Actor::current_user(),
            json!({ "target": target, "without_checks": without_checks }),
        )
        .await?;
        tx.commit().await?;
        Ok(())
    }

    /// A Delivery's conflict went to the agent (FLW-17): "Merge conflict · fixing", one more in a row.
    pub(crate) async fn count_delivery_conflict(&self, ticket_id: &str) -> Result<(), BoardError> {
        let now = now_ms();
        let to = Status::MergeConflictFixing;
        sqlx::query(concat!(
            "UPDATE tickets SET auto_merge_conflicts = auto_merge_conflicts + 1,
                                state = CASE WHEN state = ? THEN ? ELSE state END, updated_at = ?, ",
            "state_since = CASE WHEN state = ? THEN ? ELSE state_since END",
            " WHERE id = ?"
        ))
        .bind(Status::Working.as_str())
        .bind(to.as_str())
        .bind(now)
        .bind(Status::Working.as_str())
        .bind(now)
        .bind(ticket_id)
        .execute(self.store.pool())
        .await?;
        Ok(())
    }

    /// The Delivery stopped (FLW-17; AMG-03, AMG-04): turquoise with the reason as its text, in the Agent stage whose
    /// session would fix it (d0108 second gap review rule 12). `conflicted` counts this conflict.
    pub(crate) async fn delivery_stopped(
        &self,
        ticket_id: &str,
        stop: AutoMergeStop,
        message: &str,
        conflicted: bool,
    ) -> Result<(), BoardError> {
        let now = now_ms();
        let to = Status::DeliveryStopped;
        let ticket = self.ticket_by_id(ticket_id).await?;
        let flow = self.effective_flow(ticket.key.project.as_str()).await?;
        let stage = match ticket.place.stage() {
            Some(id) if ticket.status.at_work() => Some(id.to_owned()),
            Some(id) => engine::back_to(&flow, id, &[]).map(|s| s.id.clone()),
            None => None,
        }
        .or_else(|| flow.first_agent_stage().map(|s| s.id.clone()));
        let mut tx = self.store.begin_write().await?;
        sqlx::query(concat!(
            "UPDATE tickets SET stage_id = ?, state = ?, auto_merge_stop = ?, signal_text = ?, signal_since = ?,
                                auto_merge_conflicts = auto_merge_conflicts + ?, updated_at = ?, ",
            set_state_since!(),
            " WHERE id = ?"
        ))
        .bind(stage)
        .bind(to.as_str())
        .bind(stop.as_str())
        .bind(message)
        .bind(now)
        .bind(i64::from(conflicted))
        .bind(now)
        .bind(to.as_str())
        .bind(now)
        .bind(ticket_id)
        .execute(&mut *tx)
        .await?;
        record_event(
            &mut tx,
            ticket_id,
            EventKind::AutoMergeStopped,
            &Actor::current_user(),
            json!({ "stop": stop, "message": message }),
        )
        .await?;
        tx.commit().await?;
        Ok(())
    }

    /// A Delivery's conflict, or its failed Merge check (d0120), goes to the nearest earlier Agent stage (FLW-17):
    /// Working there (its live session gets `restack`), or Queued before it with the skill still to run. The conflict
    /// stays on the ticket until the agent moves on again. A failed check's comment (`hand_over`, with the output as
    /// its report) is posted first, in the same change (CMT-05).
    pub(crate) async fn record_conflict_fix(
        &self,
        ticket: &Ticket,
        target: &str,
        hand_over: Option<&super::hand_over::HandOver>,
        to: Status,
        resume_skill: Option<&str>,
        actor: &Actor,
    ) -> Result<(), BoardError> {
        let flow = self.effective_flow(ticket.key.project.as_str()).await?;
        let skips = self.ticket_skips(ticket, &flow).await?;
        let stage = ticket
            .place
            .stage()
            .and_then(|id| engine::back_to(&flow, id, &skips))
            .or_else(|| flow.first_agent_stage())
            .map(|s| s.id.clone());
        let stage = match to.is_queued() {
            true => stage.and_then(|id| super::send_back::waits_before(&flow, &id)),
            false => stage,
        };
        let mut tx = self.store.begin_write().await?;
        if let Some(hand_over) = hand_over {
            hand_over.post(&mut tx, &ticket.id, actor).await?;
        }
        super::send_back::return_to_dev(&mut tx, ticket, to, stage.as_deref(), resume_skill, actor)
            .await?;
        record_event(
            &mut tx,
            &ticket.id,
            EventKind::SentToFix,
            actor,
            json!({ "target": target, "to": to }),
        )
        .await?;
        tx.commit().await?;
        tracing::info!(key = %ticket.key, target, %to, "conflict sent to the agent to fix");
        Ok(())
    }

    /// From a stopped Delivery (Retry merge, Move to Review): back to where the Delivery is offered. A click Delivery
    /// right after a human check waits in that check (the default flow's Review, Approve ▾); otherwise the ticket
    /// waits in its Delivery stage. The stop and its count clear. Returns where it went.
    pub(crate) async fn back_to_delivery(
        &self,
        ticket: &Ticket,
        actor: &Actor,
    ) -> Result<Ticket, BoardError> {
        if ticket.status != Status::DeliveryStopped {
            return Err(BoardError::refused(
                Refusal::MoveRefused,
                format!(
                    "{} is {}; only a stopped Delivery goes back to it",
                    ticket.key,
                    ticket.status.label()
                ),
            ));
        }
        self.to_delivery(ticket, actor).await
    }

    /// Back to where the Delivery is offered ([`Self::back_to_delivery`]), from wherever the ticket is: the epic
    /// lead's note on a child that kept conflicting (LEAD-07).
    pub(crate) async fn to_delivery(
        &self,
        ticket: &Ticket,
        actor: &Actor,
    ) -> Result<Ticket, BoardError> {
        let now_delivery = self.delivery_now(ticket).await?.ok_or_else(|| {
            BoardError::refused(Refusal::MoveRefused, "The flow has no Delivery stage")
        })?;
        let flow = self.effective_flow(ticket.key.project.as_str()).await?;
        let before = flow
            .position(&now_delivery.stage.id)
            .and_then(|i| i.checked_sub(1))
            .and_then(|i| flow.stages.get(i));
        let (place, to) = match before {
            Some(check)
                if check.is_human_check() && now_delivery.delivery.mode == DeliveryMode::Click =>
            {
                (Place::stage_of(&check.id), Status::WaitingForYou)
            }
            _ => (Place::stage_of(&now_delivery.stage.id), Status::ToDeliver),
        };
        // The last check's head is the reviewed head, as when it entered the check (REV-06).
        let head: Option<Option<String>> = sqlx::query_scalar(
            "SELECT head_sha FROM validations WHERE ticket_id = ? ORDER BY started_at DESC, id DESC LIMIT 1",
        )
        .bind(&ticket.id)
        .fetch_optional(self.store.pool())
        .await?;
        let now = now_ms();
        let mut tx = self.store.begin_write().await?;
        sqlx::query(concat!(
            "UPDATE tickets SET stage_id = ?, state = ?, auto_merge_stop = NULL, auto_merge_conflicts = 0,
                                signal_text = NULL, signal_since = NULL, review_entered_at = ?,
                                review_head_sha = COALESCE(?, review_head_sha), sent_back_at = NULL,
                                pr_rework = FALSE, updated_by = ?, updated_at = ?, ",
            set_state_since!(),
            " WHERE id = ?"
        ))
        .bind(place.stage_id())
        .bind(to.as_str())
        .bind(now)
        .bind(head.flatten())
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
            json!({ "from": ticket.status, "to": to }),
        )
        .await?;
        tx.commit().await?;
        self.ticket_by_id(&ticket.id).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::flow::StageOption;
    use crate::flow::default::{DELIVER, REVIEW, VALIDATE};

    /// The default flow with an automatic Delivery (`delivers`), Review kept.
    fn automatic(delivers: Delivers) -> Flow {
        let mut flow = crate::flow::default_flow();
        for stage in flow.stages.iter_mut().filter(|s| s.id == DELIVER) {
            stage.delivery = Some(Delivery {
                mode: DeliveryMode::Automatic,
                delivers,
            });
        }
        flow
    }

    fn run(stage: &str, result: RunResult) -> StageRun {
        StageRun {
            stage: stage.into(),
            kind: String::new(),
            session: None,
            agent: None,
            model: None,
            round: None,
            result: Some(result),
            summary: None,
            started_at: 0,
            ended_at: Some(0),
        }
    }

    fn is_automatic(flow: &Flow, skips: &[String], runs: &[StageRun]) -> bool {
        let delivery = flow.stages.iter().find(|s| s.is_delivery()).unwrap();
        delivery_is_automatic(flow, delivery, skips, runs)
    }

    /// FLW-15: only work whose every check passed in its latest run delivers by itself; anything moved past or
    /// skipped is click mode, as is a click or `either` Delivery.
    #[test]
    fn automatic_only_after_every_check_passed() {
        let flow = automatic(Delivers::Local);
        let passed = [
            run(VALIDATE, RunResult::Passed),
            run(REVIEW, RunResult::Passed),
        ];
        assert!(is_automatic(&flow, &[], &passed));
        for (latest, why) in [
            (RunResult::Failed, "Move on anyway"),
            (RunResult::NoResult, "moved on without a verdict"),
            (RunResult::Skipped, "Skip this check"),
        ] {
            let runs = [
                run(VALIDATE, RunResult::Passed),
                run(VALIDATE, latest),
                run(REVIEW, RunResult::Passed),
            ];
            assert!(!is_automatic(&flow, &[], &runs), "{why}");
        }
        // The latest run counts: a fail fixed and passed since is fine.
        let again = [
            run(VALIDATE, RunResult::Failed),
            run(REVIEW, RunResult::Passed),
            run(VALIDATE, RunResult::Passed),
        ];
        assert!(is_automatic(&flow, &[], &again));
        assert!(
            !is_automatic(&flow, &[], &[run(VALIDATE, RunResult::Passed)]),
            "Review never ran"
        );
        assert!(
            !is_automatic(&crate::flow::default_flow(), &[], &passed),
            "click"
        );
        assert!(!is_automatic(&automatic(Delivers::Either), &[], &passed));
    }

    /// d0108: a human check skipped by the ticket's choice always leads to a click; an agent check it skips doesn't
    /// count, and with no check passed at all the Delivery is "without checks".
    #[test]
    fn skips_by_choice() {
        let mut flow = automatic(Delivers::Pr);
        for stage in flow.stages.iter_mut().filter(|s| s.id != DELIVER) {
            stage.options.push(StageOption::Skip);
        }
        let delivery = flow.stages.iter().find(|s| s.is_delivery()).unwrap();
        let review_skipped = [
            run(VALIDATE, RunResult::Passed),
            run(REVIEW, RunResult::Skipped),
        ];
        assert!(!is_automatic(&flow, &[REVIEW.into()], &review_skipped));
        let validate_skipped = [
            run(VALIDATE, RunResult::Skipped),
            run(REVIEW, RunResult::Passed),
        ];
        assert!(is_automatic(&flow, &[VALIDATE.into()], &validate_skipped));
        assert!(!without_checks(&flow, delivery, &validate_skipped));

        let mut no_checks = automatic(Delivers::Local);
        no_checks
            .stages
            .retain(|s| !s.is_agent_check() && !s.is_human_check());
        assert!(is_automatic(&no_checks, &[], &[]));
        let delivery = no_checks.stages.iter().find(|s| s.is_delivery()).unwrap();
        assert!(without_checks(&no_checks, delivery, &[]));
    }
}
