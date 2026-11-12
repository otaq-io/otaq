//! The delivery's board side (GIT-03, EPC-03, FLW-15, FLW-17; d0015, d0038, d0059, d0108): where a ticket merges, the
//! squash message the confirmation proposes, the bookkeeping once it merged, and the delivery's mode and conflicts.
//! The delivery is a setting of the flow (`delivery:`), not a stage: a ticket being delivered stays in the flow's
//! last stage. The git work is [`crate::git::merge`]; the click is [`crate::review::Review::local_merge`], the
//! automatic delivery [`crate::review::delivery`].
//!
//! - **Click or automatic** ([`crate::flow::Delivery::is_automatic`]): the delivery does what the flow says. With
//!   `mode: automatic` the work is delivered with no click once the ticket passed its last stage, whatever happened
//!   to the checks before: a check the ticket skips by its choice (agent or human), or one the user moved it past
//!   (Move on anyway, Skip this check, Move to next stage or a drag past it), never turns it into a click. When a
//!   check didn't pass in its latest run (skipped, moved past, or none at all), the automatic delivery records
//!   "Delivered without all checks" ([`without_checks`]) for the card, Review and History.
//! - **Conflicts** (FLW-17, d0108 smaller rule 1): merging the target into the branch conflicted, in either mode. The ticket
//!   goes to the last Agent stage it runs, whose session fixes it with `restack` with no click ("Merge conflict ·
//!   fixing"); the conflicts count in a row and the third stops it there, turquoise ("Keeps conflicting with
//!   `main`"). A merge resets the count.

use std::future::Future;
use std::path::Path;
use std::pin::Pin;
use std::sync::{Arc, OnceLock};

use serde::{Deserialize, Serialize};
use serde_json::json;

use super::stage_runs::StageRun;
use super::tickets::{EventKind, Ticket, record_event};
use super::{Board, BoardError, Refusal};
use crate::domain::{Actor, AutoMergeStop, Outcome, Place, Status, TicketKey};
use crate::flow::engine::{self, Move, RunResult};
use crate::flow::{Delivery, DeliveryMode, Flow, Stage};
use crate::git::{GitStep, RefusedHook};
use crate::store::tickets::set_state_since;
use crate::store::{new_id, now_ms};

/// Delivery conflicts in a row before the ticket stops for the user (d0108 smaller rule 1, as 0064 did).
pub const DELIVERY_CONFLICT_LIMIT: u32 = 3;

/// Whether the ticket reaches the delivery without all its checks: a check of the flow didn't pass in its latest run
/// (`runs`, oldest first): the ticket skipped it, the user moved it past it, or the flow has no check at all. An
/// automatic delivery then says "Delivered without all checks".
pub fn without_checks(flow: &Flow, runs: &[StageRun]) -> bool {
    let mut checks = checks_of(flow).peekable();
    checks.peek().is_none() || checks.any(|check| !passed_latest(runs, &check.id))
}

/// Whether the latest run of the check `stage` (`runs`, oldest first) passed: one the ticket skipped, the user moved
/// it past (Move on anyway, Skip this check, a drag past it), or that never ran didn't. A check that failed and then
/// passed did. The ticket sheet's flow path shows a check before where the ticket is as skipped when it didn't.
pub fn passed_latest(runs: &[StageRun], stage: &str) -> bool {
    latest_run(runs, stage) == Some(RunResult::Passed)
}

/// The flow's check stages, in order: all of them come before the delivery.
fn checks_of(flow: &Flow) -> impl Iterator<Item = &Stage> {
    flow.stages
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

/// A ticket's delivery as it stands now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeliveryNow {
    /// The flow's last stage, which the ticket stays in while it is delivered.
    pub stage: Stage,
    /// What it delivers.
    pub delivery: Delivery,
    /// It delivers with no click ([`Delivery::is_automatic`]), as the flow says.
    pub automatic: bool,
    /// A check before it didn't pass ([`without_checks`]).
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

    /// The ticket `ticket_id` may be ready to deliver: an automatic delivery runs now. Nothing happens for a ticket
    /// elsewhere, one waiting for the user's delivery click, or before the service set the hook.
    pub async fn arrived(&self, ticket_id: &str) {
        if let Some(hook) = self.0.get() {
            hook(ticket_id.to_owned()).await;
        }
    }
}

/// Where a ticket's Local merge goes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MergeTarget {
    /// The branch merged: `otaq/SHOP-18`.
    pub branch: String,
    /// The branch merged into: `main`.
    pub target: String,
}

/// What the Local merge confirmation shows before the click: the target and an editable squash message.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MergeProposal {
    pub key: TicketKey,
    pub target: String,
    /// "SHOP-18: Checkout API" and the agent's summary.
    pub message: String,
    /// The commits on the ticket's branch beyond its base ([`Board::commits_beyond_base`]): with none, the Approve
    /// menu leads with "Done, nothing to merge".
    #[serde(default)]
    pub commits: u32,
    /// The delivery's agent that wrote `message` while the ticket waited in Review ("Prepared by Strong…").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prepared_by: Option<String>,
    /// The delivery's agent still writing it: `message` is the plain one until it arrives.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub writing: Option<String>,
    /// The agent still writing it waits for permission: its session, whose terminal answers the prompt (the service
    /// fills it in; the ticket in Review stays calm meanwhile).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub asks_in: Option<String>,
    /// The user's own edit of `message`, kept on the ticket until it lands or the user chooses Use the proposed
    /// message: Approve & merge shows it instead of `message`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub edited: Option<String>,
}

impl Board {
    /// Where the ticket merges: into its base branch.
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
        Ok(MergeTarget {
            branch,
            target: ticket.base.branch().to_owned(),
        })
    }

    /// The proposed squash message: "SHOP-18: Checkout API", then the agent's summary.
    pub async fn merge_proposal(&self, key: &str) -> Result<MergeProposal, BoardError> {
        let ticket = self.ticket(key).await?;
        let target = self.merge_target(&ticket).await?;
        let prepared = self.delivery_message(&ticket.id).await?;
        let mut message = format!("{}: {}", ticket.key, ticket.title);
        if let Some(body) = self.review_summary(&ticket.id).await? {
            message.push_str("\n\n");
            message.push_str(body.trim());
        }
        // The delivery's agent's message once it wrote it, else the plain one while it writes (or with no agent).
        let (prepared_by, writing) = match prepared {
            Some(m) if m.is_ready() => {
                message = m.squash();
                (Some(m.agent), None)
            }
            Some(m) => (None, Some(m.agent)),
            None => (None, None),
        };
        let commits = self.commits_beyond_base(&ticket).await?;
        let edited = self.edited_message(&ticket.id).await?.merge;
        Ok(MergeProposal {
            key: ticket.key,
            target: target.target,
            message,
            commits,
            prepared_by,
            writing,
            asks_in: None,
            edited,
        })
    }

    /// The commits on the ticket's branch beyond its base: in its worktree, else on the branch; none without either.
    pub async fn commits_beyond_base(&self, ticket: &Ticket) -> Result<u32, BoardError> {
        let Some(work) = &ticket.work else {
            return Ok(0);
        };
        let Some(base) = self.current_base_sha(ticket).await? else {
            return Ok(0);
        };
        let git = self.git()?;
        let (cwd, head) = match (&work.worktree, &work.branch) {
            (Some(worktree), _) if Path::new(worktree).is_dir() => {
                (worktree.clone(), "HEAD".to_owned())
            }
            (_, Some(branch)) => (
                self.project(ticket.key.project.as_str()).await?.repo_path,
                format!("refs/heads/{branch}"),
            ),
            _ => return Ok(0),
        };
        let range = format!("{base}..{head}");
        let count = git
            .run(Path::new(&cwd), &["rev-list", "--count", &range])
            .await?;
        Ok(count.trim().parse().unwrap_or(0))
    }

    /// After a Local merge: Done · Merged with the squash commit, and the review action recorded. `onto` is the
    /// target's commit merged into the branch: the ticket's diff stays its own changes. `hook_changed`: a hook changed
    /// the commit's files, which History's Merged row says, with Show changes. `post_merge`: the user's post-merge
    /// hook ran in their folder as the target moved, its own History line after the Merged one.
    #[allow(clippy::too_many_arguments)]
    pub async fn record_local_merge(
        &self,
        ticket: &Ticket,
        target: &str,
        commit: &str,
        onto: &str,
        hook_changed: Option<&crate::git::merge::HookChanges>,
        post_merge: Option<&crate::git::merge::PostMergeHook>,
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
                                auto_merge_stop = NULL, hook_refusal = NULL, signal_text = NULL, signal_since = NULL,
                                wakeup_at = NULL, merge_check_due = FALSE, updated_by = ?, updated_at = ?, ",
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
        // It landed: the user's kept edit of its message goes with it.
        sqlx::query(crate::pr::message::FORGET_EDITED)
            .bind(&ticket.id)
            .execute(&mut *tx)
            .await?;
        let mut payload = json!({ "from": ticket.status, "target": target, "commit": commit });
        if let Some(changes) = hook_changed {
            payload["hook_changed"] = json!(changes);
        }
        record_event(&mut tx, &ticket.id, EventKind::Merged, actor, payload).await?;
        if let Some(hook) = post_merge {
            let payload = json!({ "target": target, "hook": hook });
            record_event(
                &mut tx,
                &ticket.id,
                EventKind::PostMergeHook,
                actor,
                payload,
            )
            .await?;
        }
        tx.commit().await?;
        tracing::info!(key = %ticket.key, target, commit, "merged locally");
        self.ticket_by_id(&ticket.id).await
    }

    /// The user finished the ticket without a merge: Done with [`Outcome::DoneWithoutMerge`], which is
    /// [`Status::Merged`] (tickets waiting on it start) with no merge commit, and the review action recorded, as
    /// [`Self::record_local_merge`] does. `nothing_to_merge` is set when the branch had nothing beyond its base
    /// (empty, or a reason): the card then reads "Done · nothing to merge".
    pub async fn record_done_without_merge(
        &self,
        ticket: &Ticket,
        nothing_to_merge: Option<&str>,
        actor: &Actor,
    ) -> Result<Ticket, BoardError> {
        let now = now_ms();
        let to = Status::Merged;
        let outcome = Outcome::DoneWithoutMerge;
        let round = self.current_round(&ticket.id).await?;
        // The Delivery finished (STG-06), as a merge would have: from the human check that offered it, or from the
        // Delivery stage.
        let moved = self
            .moved(ticket, ticket.status, Move::Delivered)
            .await
            .ok();
        let mut tx = self.store.begin_write().await?;
        if let Some(moved) = &moved {
            moved.record(&mut tx, &ticket.id, None).await?;
        }
        sqlx::query(concat!(
            "UPDATE tickets SET stage_id = NULL, state = ?, outcome = ?, merge_commit_sha = NULL, nothing_to_merge = ?,
                                sent_back_at = NULL, resume_skill = NULL, conflicts_with = NULL,
                                conflict_files = NULL, conflict_reason = NULL, auto_merge_conflicts = 0,
                                auto_merge_stop = NULL, hook_refusal = NULL, signal_text = NULL, signal_since = NULL,
                                wakeup_at = NULL, merge_check_due = FALSE, updated_by = ?, updated_at = ?, ",
            set_state_since!(),
            " WHERE id = ?"
        ))
        .bind(to.as_str())
        .bind(outcome.as_str())
        .bind(nothing_to_merge)
        .bind(actor.to_string())
        .bind(now)
        .bind(to.as_str())
        .bind(now)
        .bind(&ticket.id)
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "INSERT INTO reviews (id, ticket_id, round, action, at) VALUES (?, ?, ?, ?, ?)",
        )
        .bind(new_id())
        .bind(&ticket.id)
        .bind(i64::from(round))
        .bind(outcome.as_str())
        .bind(now)
        .execute(&mut *tx)
        .await?;
        // It finished: the user's kept edit of its message goes with it.
        sqlx::query(crate::pr::message::FORGET_EDITED)
            .bind(&ticket.id)
            .execute(&mut *tx)
            .await?;
        let mut payload = json!({ "from": ticket.status });
        if let Some(reason) = nothing_to_merge {
            payload["nothing_to_merge"] = json!(true);
            if !reason.is_empty() {
                payload["reason"] = json!(reason);
            }
        }
        record_event(
            &mut tx,
            &ticket.id,
            EventKind::DoneWithoutMerge,
            actor,
            payload,
        )
        .await?;
        tx.commit().await?;
        tracing::info!(key = %ticket.key, "done without a merge");
        self.ticket_by_id(&ticket.id).await
    }
}

impl Board {
    /// The ticket's delivery now: the last stage it waits in, what it delivers, and whether it runs with no click.
    /// `None` only for a flow with no stages.
    pub async fn delivery_now(&self, ticket: &Ticket) -> Result<Option<DeliveryNow>, BoardError> {
        let flow = self.effective_flow(ticket.key.project.as_str()).await?;
        let Some(stage) = flow.last_stage().cloned() else {
            return Ok(None);
        };
        let runs = self.stage_runs(&ticket.key.to_string()).await?;
        Ok(Some(DeliveryNow {
            automatic: flow.delivery.is_automatic(),
            without_checks: without_checks(&flow, &runs),
            stage,
            delivery: flow.delivery.clone(),
        }))
    }

    /// Whose session opens the ticket's PR (d0108 second gap review rule 6): the Dev stage's (an agent session) when
    /// the delivery's agent is the agent that ran it, else a delivery session of the delivery's own agent. A flow
    /// whose agents can't be read keeps the ticket's agent.
    pub async fn delivery_session_kind(&self, ticket: &Ticket) -> crate::session::SessionKind {
        use crate::session::SessionKind;
        match self.delivery_agent_is_devs(ticket).await {
            Ok(false) => SessionKind::Delivery,
            Ok(true) => SessionKind::Agent,
            Err(e) => {
                tracing::warn!(key = %ticket.key, error = %e, "can't tell the Delivery's agent: the ticket's agent");
                SessionKind::Agent
            }
        }
    }

    /// Whether the delivery's agent is the agent that ran the ticket's last Agent stage (its choice for that stage,
    /// else the stage's agent).
    async fn delivery_agent_is_devs(&self, ticket: &Ticket) -> Result<bool, BoardError> {
        use crate::agents::resolve;
        let flow = self.effective_flow(ticket.key.project.as_str()).await?;
        let skips = self.ticket_skips(ticket, &flow).await?;
        let Some(dev) = engine::back_from_delivery(&flow, &skips) else {
            return Ok(false);
        };
        let delivery = resolve::delivery_agent(self, ticket.key.project.as_str()).await?;
        let dev = resolve::running_agent(self, ticket, &dev.id).await?;
        Ok(delivery.id == dev.id)
    }

    /// An automatic Delivery merged the ticket (AMG-02, FLW-15): "Merged automatically" (and "Delivered without
    /// checks" when a check before it didn't pass), the conflict count back at 0, a History line.
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
        self.stop_delivery(ticket_id, stop, message, conflicted)
            .await
    }

    /// A project's git hook refused a step of Approve & merge, the user's click or the automatic delivery alike: the
    /// ticket stays where it was ([`Self::stop_in_place`]), with "pre-commit hook refused the merge" as its text, the
    /// hook's whole output kept on the ticket for its sheet, and History's line holding that output. The way on is to
    /// edit the message and merge again (Approve & merge opens on the user's kept edit).
    pub(crate) async fn merge_refused_by_hook(
        &self,
        ticket_id: &str,
        refusal: &crate::git::HookRefusal,
    ) -> Result<RefusedHook, BoardError> {
        // Watched, git says the step; else the squash commit, where a refusal most often comes.
        let refused = RefusedHook::of(refusal, GitStep::Commit, "the merge");
        self.stop_in_place(
            ticket_id,
            AutoMergeStop::HookRefused,
            &refused.line(),
            Some(&refused),
        )
        .await?;
        tracing::warn!(ticket = ticket_id, hook = %refused.name, step = ?refused.step, "a hook refused the merge");
        Ok(refused)
    }

    /// The project's commit-msg hook refused a message the PR's commits were to get (the user's Approve & open PR, or
    /// the automatic delivery's): nothing was rewritten or pushed. The ticket stays where it was, as after a refused
    /// merge, with "commit-msg hook refused the PR" as its text and `refused` (the hook's output and the message) kept
    /// on the ticket and in History. The way on is to fix the message and open the PR again, on the user's kept edits.
    pub(crate) async fn pr_refused_by_hook(
        &self,
        ticket_id: &str,
        refused: &RefusedHook,
    ) -> Result<(), BoardError> {
        self.stop_in_place(
            ticket_id,
            AutoMergeStop::HookRefused,
            &refused.line(),
            Some(refused),
        )
        .await?;
        tracing::warn!(ticket = ticket_id, hook = %refused.name, "a hook refused the PR's commit messages");
        Ok(())
    }

    /// The delivery's agent reported its push or PR done but the remote doesn't have the branch where it should: the
    /// ticket waits where the Delivery started ([`Self::stop_in_place`]), with `message` (what the remote has) as its
    /// text. The way on is to push again (Push to PR).
    pub(crate) async fn push_not_landed(
        &self,
        ticket: &Ticket,
        message: &str,
        actor: &Actor,
    ) -> Result<(), BoardError> {
        // Reported again on an open PR, the ticket goes back to where its Delivery is offered first.
        if !matches!(ticket.status, Status::WaitingForYou | Status::ToDeliver) {
            self.to_delivery(ticket, actor).await?;
        }
        self.stop_in_place(&ticket.id, AutoMergeStop::PushNotLanded, message, None)
            .await
    }

    /// A Delivery stop that keeps the ticket where it is ([`AutoMergeStop::keeps_place`]): a hook's refusal or a push
    /// that didn't land. Its stage and status stay (the user's check, or To deliver); the stop, its text and the
    /// hook's refusal, if one, are kept on the ticket, which shows Needs you, and History's line is the refusal's or
    /// the stop's. The three share this one shape. A move on clears it, and so does the retry that goes through.
    async fn stop_in_place(
        &self,
        ticket_id: &str,
        stop: AutoMergeStop,
        message: &str,
        refused: Option<&RefusedHook>,
    ) -> Result<(), BoardError> {
        debug_assert!(stop.keeps_place());
        let now = now_ms();
        let mut tx = self.store.begin_write().await?;
        sqlx::query(
            "UPDATE tickets SET auto_merge_stop = ?, signal_text = ?, signal_since = ?, hook_refusal = ?,
                                updated_at = ?
             WHERE id = ?",
        )
        .bind(stop.as_str())
        .bind(message)
        .bind(now)
        .bind(refused.map(|r| serde_json::to_string(r).expect("serializable")))
        .bind(now)
        .bind(ticket_id)
        .execute(&mut *tx)
        .await?;
        record_stop(&mut tx, ticket_id, stop, message, refused).await?;
        tx.commit().await?;
        Ok(())
    }

    /// Clears a stop that kept the ticket where it was ([`Self::stop_in_place`]): its retry goes through now (every
    /// PR message passed, the push goes again). Nothing for a ticket without one.
    pub(crate) async fn clear_stop_in_place(&self, ticket: &Ticket) -> Result<(), BoardError> {
        if ticket.stopped_in_place().is_none() {
            return Ok(());
        }
        sqlx::query(
            "UPDATE tickets SET auto_merge_stop = NULL, hook_refusal = NULL, signal_text = NULL,
                                signal_since = NULL, updated_at = ?
             WHERE id = ?",
        )
        .bind(now_ms())
        .bind(&ticket.id)
        .execute(self.store.pool())
        .await?;
        Ok(())
    }

    /// [`Self::delivery_stopped`]: turquoise in the Agent stage whose session would fix it.
    async fn stop_delivery(
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
            _ => engine::back_for_conflict(&flow, &ticket.place, ticket.status, &[])
                .map(|s| s.id.clone()),
        };
        let mut tx = self.store.begin_write().await?;
        sqlx::query(concat!(
            "UPDATE tickets SET stage_id = ?, state = ?, auto_merge_stop = ?, signal_text = ?, signal_since = ?,
                                auto_merge_conflicts = auto_merge_conflicts + ?, hook_refusal = NULL, updated_at = ?, ",
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
        record_stop(&mut tx, ticket_id, stop, message, None).await?;
        tx.commit().await?;
        Ok(())
    }

    /// A delivery's conflict, or its failed Merge check (d0120), goes back to an Agent stage (FLW-17,
    /// [`engine::back_for_conflict`]):
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
        let stage = engine::back_for_conflict(&flow, &ticket.place, ticket.status, &skips)
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

    /// From a stopped delivery (Retry merge, Move to Review): back to where the delivery is offered, the flow's last
    /// stage. A check by you there with a click delivery waits for its Approve (the default flow's Review, Approve
    /// ▾); otherwise the ticket waits To deliver. The stop and its count clear. Returns where it went.
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

    /// Back to where the Delivery is offered ([`Self::back_to_delivery`]), from wherever the ticket is.
    pub(crate) async fn to_delivery(
        &self,
        ticket: &Ticket,
        actor: &Actor,
    ) -> Result<Ticket, BoardError> {
        let now_delivery = self
            .delivery_now(ticket)
            .await?
            .ok_or_else(|| BoardError::refused(Refusal::MoveRefused, "The flow has no stages"))?;
        // The last stage either way: waiting for the Approve when it is a check by you and the delivery waits for a
        // click, else To deliver.
        let last = &now_delivery.stage;
        let to = match last.is_human_check() && now_delivery.delivery.mode == DeliveryMode::Click {
            true => Status::WaitingForYou,
            false => Status::ToDeliver,
        };
        let place = Place::stage_of(&last.id);
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
                                hook_refusal = NULL, signal_text = NULL, signal_since = NULL, review_entered_at = ?,
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

/// History's line for a Delivery stop: the hook's refusal with its output, or the stop and its text.
async fn record_stop(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    ticket_id: &str,
    stop: AutoMergeStop,
    message: &str,
    refused: Option<&RefusedHook>,
) -> Result<(), BoardError> {
    let (kind, payload) = match refused {
        Some(refused) => (
            EventKind::HookRefused,
            json!({ "hook": refused, "stop": stop, "message": message }),
        ),
        None => (
            EventKind::AutoMergeStopped,
            json!({ "stop": stop, "message": message }),
        ),
    };
    record_event(tx, ticket_id, kind, &Actor::current_user(), payload).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::flow::default::{REVIEW, VALIDATE};
    use crate::flow::{Delivers, StageOption};

    /// The default flow delivering automatically (`delivers`), Review kept.
    fn automatic(delivers: Delivers) -> Flow {
        let mut flow = crate::flow::default_flow();
        flow.delivery.mode = DeliveryMode::Automatic;
        flow.delivery.delivers = delivers;
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

    fn is_automatic(flow: &Flow) -> bool {
        flow.delivery.is_automatic()
    }

    fn is_without_checks(flow: &Flow, runs: &[StageRun]) -> bool {
        without_checks(flow, runs)
    }

    /// FLW-15: the delivery does what the flow says. Automatic (`local` or `pr`) delivers with no click; click mode
    /// and `either` wait for the user.
    #[test]
    fn only_the_flow_decides_click_or_automatic() {
        assert!(is_automatic(&automatic(Delivers::Local)));
        assert!(is_automatic(&automatic(Delivers::Pr)));
        assert!(!is_automatic(&crate::flow::default_flow()), "click");
        assert!(!is_automatic(&automatic(Delivers::Either)));
    }

    /// Every check before the Delivery passed in its latest run: delivered with its checks. A check skipped by the
    /// ticket's choice (agent or human), moved past after a fail or no verdict (Move on anyway), skipped by the user
    /// (Skip this check) or never run: "Delivered without all checks", the Delivery still automatic.
    #[test]
    fn without_checks_when_a_check_didnt_pass() {
        let mut flow = automatic(Delivers::Local);
        for stage in &mut flow.stages {
            stage.options.push(StageOption::Skip);
        }
        let passed = [
            run(VALIDATE, RunResult::Passed),
            run(REVIEW, RunResult::Passed),
        ];
        assert!(!is_without_checks(&flow, &passed));
        for (latest, why) in [
            (RunResult::Failed, "Move on anyway"),
            (RunResult::NoResult, "moved on without a verdict"),
            (
                RunResult::Skipped,
                "Skip this check, or skipped by the ticket's choice",
            ),
        ] {
            let runs = [
                run(VALIDATE, RunResult::Passed),
                run(VALIDATE, latest),
                run(REVIEW, RunResult::Passed),
            ];
            assert!(is_without_checks(&flow, &runs), "{why}");
        }
        let review_skipped = [
            run(VALIDATE, RunResult::Passed),
            run(REVIEW, RunResult::Skipped),
        ];
        assert!(
            is_without_checks(&flow, &review_skipped),
            "a human check skipped"
        );
        // The latest run counts: a fail fixed and passed since is fine.
        let again = [
            run(VALIDATE, RunResult::Failed),
            run(REVIEW, RunResult::Passed),
            run(VALIDATE, RunResult::Passed),
        ];
        assert!(!is_without_checks(&flow, &again));
        assert!(
            is_without_checks(&flow, &[run(VALIDATE, RunResult::Passed)]),
            "Review never ran"
        );
        assert!(is_automatic(&flow), "whatever the checks did");

        let mut no_checks = automatic(Delivers::Local);
        no_checks
            .stages
            .retain(|s| !s.is_agent_check() && !s.is_human_check());
        assert!(is_automatic(&no_checks));
        assert!(is_without_checks(&no_checks, &[]));
    }
}
