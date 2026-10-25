//! The bookkeeping of a ticket returning from a human check to the check's fails-go-to stage (REV-04, GIT-03,
//! FLW-14; d0034, d0038, d0108 rule 6, d0131): on the default flow, Ready for Review to Dev.
//!
//! **Fail** (Send back on the default flow) bundles the round's pending line comments and the optional note: the note
//! is the user's ticket comment (CMT-05), posted first, the line comments get `sent_at`, a `reviews` row records the
//! action and the round, `sent_back_round` becomes that round and the card shows "Sent back · N comments · round K"
//! until the ticket next reaches Ready for Review. The ticket goes to that stage: Agent Working when its live session
//! gets the skill at once, otherwise Queued with the skill to run once the agent is live again (`resume_skill`) and
//! what it is told (`resume_prompt`: "SHOP-18 was sent back to Dev with 2 line comments and a comment. Run `otaq
//! prime`, then `otaq review feedback`.", never the note). With no line comments the skill doesn't go (d0131 Q514):
//! only the plain sentence ("Spec review sent SHOP-40 back to you with a comment. Run `otaq prime`."), typed or
//! resumed with. Typing and resuming are the caller's job ([`crate::review::Review`]).

use serde_json::json;

use super::hand_over::{HandOver, SentBack, sent_back_text};
use super::stage_runs;
use super::tickets::{EventKind, Ticket, record_event};
use super::{Board, BoardError};
use crate::domain::{Actor, Status};
use crate::flow::engine::{Move, RunResult};
use crate::store::tickets::set_state_since;
use crate::store::{Store, new_id, now_ms};

/// Where and how [`Board::send_back_to`] sends a ticket back.
pub(crate) struct SentBackTo<'a> {
    /// The user's note as their comment, posted first (CMT-05); `None` when the note was empty.
    pub hand_over: Option<&'a HandOver>,
    /// Working (its live session got the skill) or Queued.
    pub to: Status,
    /// The Agent stage it returns to, or waits before (`None`: Ready for Dev).
    pub stage: Option<&'a str>,
    pub resume_skill: Option<&'a str>,
    /// How the run of the stage it leaves ends.
    pub ended: RunResult,
    /// History's event in place of Sent back.
    pub event: Option<(EventKind, serde_json::Value)>,
    /// What its agent is told, when it is ([`sent_back_text`]).
    pub tell: Option<Tell<'a>>,
    /// The stage never ran (a drag back into a stage the ticket skips): its next start is a new conversation there,
    /// with the told text after the stage's skill (FLW-18).
    pub fresh: bool,
}

/// What [`sent_back_text`] needs besides the counts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Tell<'a> {
    /// The Agent stage's name ("Dev").
    pub stage: &'a str,
    pub by_you: bool,
    pub pr_comments: bool,
    /// The human check whose Send back it is ([`SentBack::from`]).
    pub from: Option<&'a str>,
}

/// What a Send back sent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SentRound {
    pub comments: u32,
    pub round: u32,
    /// What its agent is told ([`sent_back_text`]): typed into its live session by the caller, else already stored
    /// as the queued ticket's `resume_prompt`.
    pub told: Option<String>,
}

/// What `tickets.stage_id` holds for a ticket queued to come back to the Agent stage `agent_stage` (Resume, a Send
/// back with the limit full): that stage, which it waits before, or `None` (Ready for Dev) when it is the flow's
/// first Agent stage, so the default flow queues as today. The start continues its conversation there (STG-07,
/// STG-09).
pub(crate) fn waits_before(flow: &crate::flow::Flow, agent_stage: &str) -> Option<String> {
    let first = flow.first_agent_stage().map(|s| s.id.as_str());
    (Some(agent_stage) != first).then(|| agent_stage.to_owned())
}

impl Board {
    /// Posts the user's note (`hand_over`), marks the pending comments sent, records the review, and moves the ticket
    /// to `to` (Working or Queued) with `resume_skill` (the skill still to run, when it couldn't be typed now).
    pub async fn record_send_back(
        &self,
        ticket: &Ticket,
        hand_over: Option<&HandOver>,
        to: Status,
        resume_skill: Option<&str>,
        actor: &Actor,
    ) -> Result<SentRound, BoardError> {
        let stage = self.back_stage(ticket, to).await?;
        let name = self.back_stage_name(ticket).await?;
        let flow = self.effective_flow(ticket.key.project.as_str()).await?;
        let check = ticket
            .place
            .stage()
            .and_then(|id| flow.stage(id))
            .map(|s| s.name.clone());
        self.send_back_to(
            ticket,
            SentBackTo {
                hand_over,
                to,
                stage: stage.as_deref(),
                resume_skill,
                ended: RunResult::Failed,
                event: None,
                tell: Some(Tell {
                    stage: &name,
                    by_you: false,
                    pr_comments: false,
                    from: check.as_deref(),
                }),
                fresh: false,
            },
            actor,
        )
        .await
    }

    /// The name of the Agent stage a Fail returns the ticket to ("Dev"), for what its agent is told.
    pub(crate) async fn back_stage_name(&self, ticket: &Ticket) -> Result<String, BoardError> {
        let flow = self.effective_flow(ticket.key.project.as_str()).await?;
        let stage = self.back_stage(ticket, Status::Working).await?;
        Ok(stage
            .as_deref()
            .and_then(|id| flow.stage(id))
            .or_else(|| flow.first_agent_stage())
            .map_or_else(|| "Dev".to_owned(), |s| s.name.clone()))
    }

    /// A Send back's bookkeeping into `back.stage` (the Fail's fails-go-to stage, or the stage a drag back
    /// chose, FLW-18): the user's comment is posted, the pending line comments are sent and the review recorded (the
    /// agent reads them with `otaq review feedback`), the open run ends as `back.ended`, every agent check's counter
    /// restarts, and the ticket is `back.to` there. History gets Sent back, or the drag's own event.
    pub(crate) async fn send_back_to(
        &self,
        ticket: &Ticket,
        back: SentBackTo<'_>,
        actor: &Actor,
    ) -> Result<SentRound, BoardError> {
        let (to, stage) = (back.to, back.stage.map(str::to_owned));
        let round = self.current_round(&ticket.id).await?;
        let mut tx = self.store.begin_write().await?;
        let sent = send_back_in(&mut tx, ticket, back, round, actor).await?;
        tx.commit().await?;
        tracing::info!(key = %ticket.key, round, comments = sent.comments, %to, stage, "sent back");
        Ok(sent)
    }

    /// Where a Send back takes the ticket: the check's fails-go-to stage, as the flow engine says (a human check's
    /// Fail, STG-06, d0131). Queued (`to`, the limit is full), it waits before that stage instead
    /// ([`waits_before`]): Ready for Dev before the flow's first.
    pub(crate) async fn back_stage(
        &self,
        ticket: &Ticket,
        to: Status,
    ) -> Result<Option<String>, BoardError> {
        let flow = self.effective_flow(ticket.key.project.as_str()).await?;
        let stage = match self
            .moved(ticket, Status::WaitingForYou, Move::HumanFail)
            .await
        {
            Ok(moved) => moved.place.stage().map(str::to_owned),
            Err(_) => flow.first_agent_stage().map(|s| s.id.clone()),
        };
        Ok(match to.is_queued() {
            true => stage.and_then(|id| waits_before(&flow, &id)),
            false => stage,
        })
    }

    /// The line comments waiting to go with the next Send back: with any, it is a code review and `address-review`
    /// goes with it (d0131 Q514).
    pub async fn pending_line_comments(&self, ticket_id: &str) -> Result<u32, BoardError> {
        let n: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM review_comments WHERE ticket_id = ? AND sent_at IS NULL",
        )
        .bind(ticket_id)
        .fetch_one(self.store.pool())
        .await?;
        Ok(u32::try_from(n).unwrap_or(u32::MAX))
    }

    /// The round of the latest Send back, if there was one.
    pub async fn latest_send_back_round(&self, ticket_id: &str) -> Result<Option<u32>, BoardError> {
        latest_review(&self.store, ticket_id).await
    }
}

/// [`Board::send_back_to`]'s writes inside the caller's transaction, `round` being the ticket's current review round
/// (Apply now sends tickets back with its other moves and the flow in one, APN-04).
pub(crate) async fn send_back_in(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    ticket: &Ticket,
    back: SentBackTo<'_>,
    round: u32,
    actor: &Actor,
) -> Result<SentRound, BoardError> {
    let SentBackTo {
        hand_over,
        to,
        stage,
        resume_skill,
        ended,
        event,
        tell,
        fresh,
    } = back;
    let now = now_ms();
    // The note first: a resumed agent's `prime` already has it (CMT-05).
    let posted = match hand_over {
        Some(h) => Some(h.post(tx, &ticket.id, actor).await?),
        None => None,
    };
    let sent = sqlx::query(
        "UPDATE review_comments SET sent_at = ?, round = ? WHERE ticket_id = ? AND sent_at IS NULL",
    )
    .bind(now)
    .bind(i64::from(round))
    .bind(&ticket.id)
    .execute(&mut **tx)
    .await?
    .rows_affected();
    sqlx::query(
        "INSERT INTO reviews (id, ticket_id, round, action, at) VALUES (?, ?, ?, 'send_back', ?)",
    )
    .bind(new_id())
    .bind(&ticket.id)
    .bind(i64::from(round))
    .bind(now)
    .execute(&mut **tx)
    .await?;
    sqlx::query("UPDATE tickets SET sent_back_round = ?, sent_back_at = ? WHERE id = ?")
        .bind(i64::from(round))
        .bind(now)
        .bind(&ticket.id)
        .execute(&mut **tx)
        .await?;
    // A human check's Fail (STG-06), or how a drag back ends the run where the card was.
    let comment_id = posted.as_ref().map(|p| p.id.as_str());
    stage_runs::end_open(tx, &ticket.id, ended, comment_id).await?;
    return_to_dev(tx, ticket, to, stage, resume_skill, actor).await?;
    let comments = u32::try_from(sent).unwrap_or(u32::MAX);
    let told = tell.map(|tell| {
        sent_back_text(&SentBack {
            key: &ticket.key.to_string(),
            stage: tell.stage,
            line_comments: comments,
            pr_comments: tell.pr_comments,
            comment: posted.is_some(),
            by_you: tell.by_you,
            from: tell.from,
        })
    });
    // What a queued or fresh start begins with: the skill (or the stage's own) followed by the told text; with no
    // skill (no code review, d0131 Q514) the told text alone, as a resumed conversation's first prompt.
    let plain = resume_skill.is_none() && !fresh && to.is_queued();
    if resume_skill.is_some() || fresh || plain {
        sqlx::query(
            "UPDATE tickets SET resume_prompt = ?,
                                resume_skill = CASE WHEN ? THEN NULL ELSE resume_skill END,
                                return_mode = CASE WHEN ? THEN ? WHEN ? THEN ? ELSE return_mode END
             WHERE id = ?",
        )
        .bind(told.as_deref())
        .bind(fresh)
        .bind(fresh)
        .bind(crate::recovery::RETURN_SCRATCH)
        .bind(plain)
        .bind(crate::recovery::RETURN_RESUME)
        .bind(&ticket.id)
        .execute(&mut **tx)
        .await?;
    }
    // The run it resumes (a live session's) has a new comment: its agent primes again before it ends (CMT-08).
    if posted.is_some() {
        stage_runs::mark_handed_over(tx, &ticket.id).await?;
    }
    let note = hand_over.map_or("", |h| h.body.as_str());
    // History's "Sent back to Product spec · by you" names the stage (d0131).
    let named = tell.map(|t| t.stage);
    let (kind, payload) = event.unwrap_or_else(|| {
        (
            EventKind::SentBack,
            json!({ "round": round, "comments": comments, "note": note, "to": to, "stage": named }),
        )
    });
    record_event(tx, &ticket.id, kind, actor, payload).await?;
    Ok(SentRound {
        comments,
        round,
        told,
    })
}

async fn latest_review(store: &Store, ticket_id: &str) -> Result<Option<u32>, BoardError> {
    let round: Option<i64> = sqlx::query_scalar(
        "SELECT round FROM reviews WHERE ticket_id = ? AND action = 'send_back'
         ORDER BY at DESC, round DESC LIMIT 1",
    )
    .bind(ticket_id)
    .fetch_optional(store.pool())
    .await?;
    Ok(round.map(|round| u32::try_from(round).unwrap_or(0)))
}

/// Ready for Review → Dev: Agent Working in `stage` (the live session got the skill) or Queued (`queued_at` now)
/// with the skill still to run, waiting before `stage` (`None`: Ready for Dev, as today). The next submit validates again from round 1 (d0063,
/// VAL-10).
pub(crate) async fn return_to_dev(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    ticket: &Ticket,
    to: Status,
    stage: Option<&str>,
    resume_skill: Option<&str>,
    actor: &Actor,
) -> Result<(), BoardError> {
    let now = now_ms();
    sqlx::query(concat!(
        "UPDATE tickets SET stage_id = ?, state = ?, resume_skill = ?,
                            queued_at = CASE WHEN ? THEN ? ELSE queued_at END,
                            last_signal_at = CASE WHEN ? THEN ? ELSE last_signal_at END,
                            signal_text = NULL, signal_since = NULL, wakeup_at = NULL,
                            review_entry = NULL,
                            updated_by = ?, updated_at = ?, ",
        set_state_since!(),
        " WHERE id = ?"
    ))
    .bind(stage)
    .bind(to.as_str())
    .bind(resume_skill)
    .bind(to.is_queued())
    .bind(now)
    .bind(to == Status::Working)
    .bind(now)
    .bind(actor.to_string())
    .bind(now)
    .bind(to.as_str())
    .bind(now)
    .bind(&ticket.id)
    .execute(&mut **tx)
    .await?;
    // A human Fail: every agent check's counter restarts at 0 (FLW-13).
    super::validation::reset_check_counters(tx, &ticket.id).await?;
    record_event(
        tx,
        &ticket.id,
        EventKind::Moved,
        actor,
        json!({ "from": ticket.status, "to": to }),
    )
    .await
}

impl Board {
    /// The agent of `ticket_id` is alive in its conversation (a hook or a `otaq` command came from it): the
    /// first prompt it was started or resumed with (a Send back's skill, a needs-you answer) has reached it, so it is
    /// not sent again by a later Retry or Resume (T-0086, T-0112).
    pub async fn first_prompt_delivered(&self, ticket_id: &str) -> Result<(), BoardError> {
        sqlx::query(
            "UPDATE tickets SET resume_skill = NULL, resume_prompt = NULL
             WHERE id = ? AND (resume_skill IS NOT NULL OR resume_prompt IS NOT NULL)",
        )
        .bind(ticket_id)
        .execute(self.store().pool())
        .await?;
        Ok(())
    }
}
