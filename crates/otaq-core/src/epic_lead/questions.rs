//! A child's question goes to its epic's lead first (LEAD-06, LEAD-04; d0083, d0030, d0061, d0011, d0016, d0048).
//!
//! - **Routing** ([`LeadQuestions::route`]): `otaq ask human` from a child whose epic has a **live** lead stores the
//!   question as always, marked as waiting for that lead. No needs-you item, no notification, and the child isn't
//!   turquoise: its card shows "asked lead · waiting" ([`crate::domain::Flags::asked_lead`]). The lead is woken
//!   ([`wake_text`]; its status reads "answering SHOP-40.2") and a handover is registered. With no live lead (also
//!   after a restart, until Resume) the question is a plain "Asks you" as before. Permission prompts never come here:
//!   the hook path doesn't change (d0011).
//! - **`otaq answer <QUESTION-ID> --stdin`** by the lead ([`LeadQuestions::answer`]): recorded as the answer and typed
//!   into the child as an automatic message ([`answer_text`], §9.3). Needs you never saw it.
//! - **`otaq ask human --pass-on <QUESTION-ID> [--option …] [--recommended N] --stdin`** by the lead
//!   ([`LeadQuestions::pass_on`]): the question becomes the child's "Asks you" item "SHOP-40.2 asks · passed on by
//!   Wishlist lead", with the lead's options (the child's by default), its recommended option (d0061) and its reason
//!   ([`crate::board::questions::ViaLead`]). The options' notes (ATT-17) go with them: the lead's own with its own
//!   options (none if it gave none), the child's with the child's. The child turns turquoise with an OS notification, as for any Asks you;
//!   the user's answer reaches it as today (ATT-12).
//! - **Hand-back** ([`hand_back`]): the lead's turn ended without either, or the lead ended: the question becomes a
//!   plain "Asks you", as if no lead were live. Handovers live in memory, so on startup every question still waiting
//!   for a lead is handed back ([`hand_back_after_restart`]) before the reconcile keeps the child's label (d0045).
//! - **Messages** (MSG-01): every step is logged on the child **and** the epic (History [`EventKind::LeadQuestion`]),
//!   and [`log_of`] lists the questions with where they stand for both tickets' Messages sections.
//! - **`otaq questions`** from the lead lists the questions waiting for it ([`waiting`]).

use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::board::questions::{
    HumanQuestion, clean_notes, no_notes, notes_from_json, notes_json, validate_notes,
    validate_options,
};
use crate::board::tickets::{EventKind, Ticket, record_event};
use crate::board::{Board, BoardError, Refusal};
use crate::caller::Caller;
use crate::domain::{Actor, TicketKey, stored_enum};
use crate::epic_lead::wake::Handover;
use crate::naming;
use crate::project_agents::lead::{EpicLeads, Wake, lead_name};
use crate::protocol::Event;
use crate::service::EventBus;
use crate::session::deliveries::{Deliveries, DeliveryKind};
use crate::session::{SessionKind, Sessions};
use crate::signals::Signals;
use crate::store::now_ms;

/// The handover owner of routed questions ([`EpicLeads::on_hand_back`]).
pub const OWNER: &str = "question";

/// The stored forms of [`LeadQuestionState`].
pub const WITH_LEAD: &str = "with_lead";
pub const ANSWERED: &str = "answered";
pub const PASSED_ON: &str = "passed_on";
pub const HANDED_BACK: &str = "handed_back";

/// The refusal of `answer` on a question waiting for a lead, from anyone but that lead.
pub const ONLY_ITS_LEAD_ANSWERS: &str =
    "Only the epic's lead can answer a question that waits for it";
/// The refusal of `ask human --pass-on` from anyone but an epic's lead.
pub const ONLY_A_LEAD_PASSES_ON: &str = "Only an epic's lead can pass a child's question on";

stored_enum! {
    /// Where a question routed to a lead stands (`human_questions.lead_state`).
    pub enum LeadQuestionState ("lead question state") {
        /// Waits for the lead: "asked lead · waiting".
        WithLead = WITH_LEAD,
        /// The lead answered it (`answer` is the lead's text).
        Answered = ANSWERED,
        /// The lead passed it on: an "Asks you" item with its recommendation (`answer`, once there, is the user's).
        PassedOn = PASSED_ON,
        /// The lead did neither in its turn, or ended: a plain "Asks you" (`answer` is the user's).
        HandedBack = HANDED_BACK,
    }
}

/// A child's question that went to its epic's lead, with where it stands: the Messages log, `otaq questions` for the
/// lead, and the reply of `answer` / `ask human --pass-on`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LeadQuestion {
    /// The question's id, as `otaq answer` and `--pass-on` take it.
    pub id: String,
    pub child: TicketKey,
    pub child_title: String,
    pub epic: TicketKey,
    /// "Wishlist lead".
    pub lead: String,
    /// The lead's Project agent id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lead_agent: Option<String>,
    pub question: String,
    /// The answer options: the child's, or the lead's after a pass-on that replaced them.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub options: Vec<String>,
    /// The recommended option, 1-based: the child's, or the lead's after a pass-on.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recommended: Option<u32>,
    /// Each option's note (ATT-17), in the options' order: the child's, or the lead's with its own options. Empty
    /// when no option has one.
    #[serde(default, skip_serializing_if = "no_notes")]
    pub notes: Vec<Option<String>>,
    pub state: LeadQuestionState,
    /// The lead's reason, after a pass-on.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// The answer: the lead's when `state` is answered, else the user's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub answer: Option<String>,
    pub asked_at: i64,
    /// When it left the lead (answered, passed on or handed back).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lead_at: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub answered_at: Option<i64>,
}

impl LeadQuestion {
    /// The option the lead recommended, by its label ("Logged-in users only"), after a pass-on.
    pub fn recommended_label(&self) -> Option<&str> {
        let n = self.recommended? as usize;
        self.options.get(n.checked_sub(1)?).map(String::as_str)
    }
}

/// The wake-up typed into the lead: "SHOP-40.2 asks (question 7): Can guests save items without logging in?
/// Options: 1. Logged-in users only 2. Guests too, kept in the browser. Answer with `otaq answer 7 --stdin`, or
/// pass it on with `otaq ask human --pass-on 7 …`."
pub fn wake_text(child: &TicketKey, question: &HumanQuestion) -> String {
    let cmd = naming::COMMAND_NAME;
    let id = &question.id;
    let options = if question.options.is_empty() {
        String::new()
    } else {
        let listed: Vec<String> = question
            .options
            .iter()
            .enumerate()
            .map(|(n, label)| format!("{}. {label}", n + 1))
            .collect();
        format!(" Options: {}.", listed.join(" "))
    };
    format!(
        "{child} asks (question {id}): {}{options} Answer with `{cmd} answer {id} --stdin`, or pass it on with \
         `{cmd} ask human --pass-on {id} \u{2026}`.",
        question.text.trim()
    )
}

/// What is typed into the child: "Answer from Wishlist lead: Yes: filled heart means saved …".
pub fn answer_text(lead: &str, answer: &str) -> String {
    format!("Answer from {lead}: {}", answer.trim())
}

/// "Answered SHOP-40.2's question 7. It is typed into SHOP-40.2's session when its agent is idle.": `answer`'s
/// output.
pub fn answered_message(q: &LeadQuestion) -> String {
    format!(
        "Answered {}'s question {}. It is typed into {}'s session when its agent is idle.",
        q.child, q.id, q.child
    )
}

/// `ask human --pass-on`'s output.
pub fn passed_on_message(q: &LeadQuestion) -> String {
    format!(
        "Passed {}'s question {} on to the user, with your recommendation. End your turn.",
        q.child, q.id
    )
}

#[derive(sqlx::FromRow)]
struct Row {
    id: String,
    ticket_id: String,
    lead_epic_id: String,
    lead_id: Option<String>,
    question: String,
    options_json: String,
    recommended: Option<i64>,
    option_notes_json: Option<String>,
    asked_at: i64,
    answer: Option<String>,
    answered_at: Option<i64>,
    lead_state: String,
    lead_reason: Option<String>,
    lead_at: Option<i64>,
}

const SELECT: &str = "SELECT id, ticket_id, lead_epic_id, lead_id, question, options_json, recommended, \
     option_notes_json, asked_at, answer, answered_at, lead_state, lead_reason, lead_at FROM human_questions";

async fn rows(board: &Board, filter: &str, binds: &[&str]) -> Result<Vec<Row>, BoardError> {
    let sql = format!(
        "{SELECT} WHERE lead_state IS NOT NULL AND lead_epic_id IS NOT NULL AND ticket_id IS NOT NULL AND ({filter}) \
         ORDER BY asked_at, id"
    );
    let mut query = sqlx::query_as::<_, Row>(sqlx::AssertSqlSafe(sql));
    for bind in binds {
        query = query.bind(*bind);
    }
    Ok(query.fetch_all(board.store().pool()).await?)
}

async fn view(board: &Board, row: Row) -> Result<LeadQuestion, BoardError> {
    let child = board.ticket_by_id(&row.ticket_id).await?;
    let epic = board.ticket_by_id(&row.lead_epic_id).await?;
    Ok(LeadQuestion {
        id: row.id,
        child: child.key,
        child_title: child.title,
        lead: lead_name(&epic.title),
        lead_agent: row.lead_id,
        epic: epic.key,
        question: row.question,
        options: serde_json::from_str(&row.options_json).unwrap_or_default(),
        recommended: row.recommended.and_then(|n| u32::try_from(n).ok()),
        notes: notes_from_json(row.option_notes_json.as_deref()),
        state: row
            .lead_state
            .parse()
            .map_err(|e: crate::domain::UnknownValue| BoardError::NotFound(e.to_string()))?,
        reason: row.lead_reason,
        answer: row.answer,
        asked_at: row.asked_at,
        lead_at: row.lead_at,
        answered_at: row.answered_at,
    })
}

async fn views(board: &Board, rows: Vec<Row>) -> Result<Vec<LeadQuestion>, BoardError> {
    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        out.push(view(board, row).await?);
    }
    Ok(out)
}

/// A question that went to a lead, by its id; `None` for any other id (a question between tickets, say).
pub async fn find(board: &Board, id: &str) -> Result<Option<LeadQuestion>, BoardError> {
    match rows(board, "id = ?", &[id.trim()]).await?.pop() {
        Some(row) => Ok(Some(view(board, row).await?)),
        None => Ok(None),
    }
}

/// The questions waiting for the lead of the epic `epic_id`, oldest first (`otaq questions` from the lead; T-0283's
/// `prime --epic`).
pub async fn waiting(board: &Board, epic_id: &str) -> Result<Vec<LeadQuestion>, BoardError> {
    let rows = rows(
        board,
        "lead_epic_id = ? AND lead_state = ? AND answered_at IS NULL",
        &[epic_id, WITH_LEAD],
    )
    .await?;
    views(board, rows).await
}

/// Every question a child of the epic asked its lead, oldest first, for the Messages section (MSG-01): of the child
/// when `ticket_id` is a child, of every child when it is the epic.
pub async fn log_of(board: &Board, ticket_id: &str) -> Result<Vec<LeadQuestion>, BoardError> {
    let rows = rows(
        board,
        "ticket_id = ? OR lead_epic_id = ?",
        &[ticket_id, ticket_id],
    )
    .await?;
    views(board, rows).await
}

/// Logs one step on the child and the epic (History; their Messages read the row).
async fn log(
    board: &Board,
    q: &LeadQuestion,
    step: &str,
    extra: serde_json::Value,
    actor: &Actor,
) -> Result<(), BoardError> {
    let child = board.ticket(&q.child.to_string()).await?;
    let epic = board.ticket(&q.epic.to_string()).await?;
    let mut payload = json!({
        "id": q.id, "child": q.child, "lead": q.lead, "step": step, "question": q.question,
    });
    if let (Some(payload), Some(extra)) = (payload.as_object_mut(), extra.as_object()) {
        payload.extend(extra.clone());
    }
    let mut tx = board.store().begin_write().await?;
    for id in [&child.id, &epic.id] {
        record_event(&mut tx, id, EventKind::LeadQuestion, actor, payload.clone()).await?;
    }
    tx.commit().await?;
    Ok(())
}

/// Hands a question back from `with_lead`; false when it had left already (answered, passed on, handed back).
async fn leave_lead(board: &Board, id: &str) -> Result<bool, BoardError> {
    let done = sqlx::query(
        "UPDATE human_questions SET lead_state = ?, lead_at = ? WHERE id = ? AND lead_state = ? AND answered_at IS NULL",
    )
    .bind(HANDED_BACK)
    .bind(now_ms())
    .bind(id)
    .bind(WITH_LEAD)
    .execute(board.store().pool())
    .await?;
    Ok(done.rows_affected() == 1)
}

/// Hands one question back to the user (the lead's turn ended without acting on it, or the lead ended): a plain
/// "Asks you" on the child, as if no lead were live. False when it no longer waited for the lead.
pub async fn hand_back(board: &Board, signals: &Signals, id: &str) -> Result<bool, BoardError> {
    let Some(q) = find(board, id).await? else {
        return Ok(false);
    };
    if q.state != LeadQuestionState::WithLead || !leave_lead(board, id).await? {
        return Ok(false);
    }
    let actor = match &q.lead_agent {
        Some(lead) => Actor::ProjectAgent(lead.clone()),
        None => Actor::current_user(),
    };
    log(board, &q, HANDED_BACK, json!({}), &actor).await?;
    let child = board.ticket(&q.child.to_string()).await?;
    signals
        .ask_human(&Caller::User, &child.id, &q.question)
        .await?;
    tracing::info!(id, child = %q.child, "question handed back by the lead");
    Ok(true)
}

/// On startup, before the reconcile: every question still waiting for a lead goes to the user (no lead is live after
/// a restart, and handovers don't survive one). The child shows Asks you, which the reconcile then keeps as its
/// label (d0045). How many were handed back.
pub async fn hand_back_after_restart(
    board: &Board,
    signals: &Signals,
) -> Result<usize, BoardError> {
    let waiting: Vec<(String,)> = sqlx::query_as(
        "SELECT id FROM human_questions WHERE lead_state = ? AND answered_at IS NULL ORDER BY asked_at, id",
    )
    .bind(WITH_LEAD)
    .fetch_all(board.store().pool())
    .await?;
    let mut handed = 0;
    for (id,) in waiting {
        if hand_back(board, signals, &id).await? {
            handed += 1;
        }
    }
    Ok(handed)
}

/// Routes children's questions to their leads and carries out the lead's answer or pass-on. Cheap to clone.
#[derive(Clone)]
pub struct LeadQuestions {
    board: Board,
    signals: Signals,
    leads: EpicLeads,
    deliveries: Deliveries,
    sessions: Sessions,
    events: EventBus,
}

impl LeadQuestions {
    pub fn new(
        board: Board,
        signals: Signals,
        leads: EpicLeads,
        deliveries: Deliveries,
        sessions: Sessions,
        events: EventBus,
    ) -> Self {
        Self {
            board,
            signals,
            leads,
            deliveries,
            sessions,
            events,
        }
    }

    /// Registers the hand-back of routed questions (once, at service start).
    pub fn register_hand_back(&self) {
        let this = self.clone();
        self.leads.on_hand_back(
            OWNER,
            std::sync::Arc::new(move |handover: Handover| {
                let this = this.clone();
                Box::pin(async move {
                    match hand_back(&this.board, &this.signals, &handover.reference).await {
                        Ok(true) => this.changed(&[&handover.child, &handover.epic]).await,
                        Ok(false) => {}
                        Err(e) => {
                            tracing::warn!(child = %handover.child, error = %e, "can't hand the question back");
                        }
                    }
                })
            }),
        );
    }

    /// `ask human` from `ticket` stored `question`: sends it to the epic's live lead instead of the user. Whether it
    /// went there; false (no epic, no live lead) leaves it to the caller, which shows Asks you as before.
    pub async fn route(
        &self,
        caller: &Caller,
        ticket: &Ticket,
        question: &HumanQuestion,
        actor: &Actor,
    ) -> Result<bool, BoardError> {
        let Some(epic) = &ticket.epic else {
            return Ok(false);
        };
        let epic = epic.to_string();
        let Some(lead) = self.leads.live_lead(&epic).await? else {
            return Ok(false);
        };
        let pool = self.board.store().pool();
        // Marked before the lead is woken, so its answer finds the question waiting for it.
        sqlx::query(
            "UPDATE human_questions SET lead_id = ?, lead_epic_id = ?, lead_state = ? WHERE id = ?",
        )
        .bind(&lead.agent.id)
        .bind(&lead.epic_id)
        .bind(WITH_LEAD)
        .bind(&question.id)
        .execute(pool)
        .await?;
        let woken = self
            .leads
            .wake_with_handover(
                &epic,
                Wake::Question {
                    ticket: ticket.key.clone(),
                },
                &wake_text(&ticket.key, question),
                OWNER,
                &ticket.key,
                &question.id,
            )
            .await;
        // The lead couldn't be woken (it ended meanwhile, or its session refused the wake-up): the user is asked.
        if !matches!(woken, Ok(Some(_))) {
            if let Err(e) = woken {
                tracing::warn!(child = %ticket.key, error = %e, "can't wake the lead: the user is asked");
            }
            sqlx::query(
                "UPDATE human_questions SET lead_id = NULL, lead_epic_id = NULL, lead_state = NULL WHERE id = ?",
            )
            .bind(&question.id)
            .execute(pool)
            .await?;
            return Ok(false);
        }
        let routed = find(&self.board, &question.id)
            .await?
            .ok_or_else(|| BoardError::NotFound(format!("question {}", question.id)))?;
        log(&self.board, &routed, "asked", json!({}), actor).await?;
        // The agent ends its turn and waits: its Stop isn't Idle, and nothing turns it turquoise.
        if let Caller::Agent { session, .. } = caller {
            self.signals.mark_submitted_or_asked(session);
        }
        tracing::info!(child = %ticket.key, lead = %routed.lead, "question sent to the lead");
        self.changed(&[&routed.child, &routed.epic]).await;
        Ok(true)
    }

    /// `otaq answer <QUESTION-ID> --stdin` from the lead: recorded and typed into the child (§9.3).
    pub async fn answer(
        &self,
        caller: &Caller,
        id: &str,
        answer: &str,
        actor: &Actor,
    ) -> Result<LeadQuestion, BoardError> {
        let answer = answer.trim();
        if answer.is_empty() {
            return Err(BoardError::refused(
                Refusal::BadQuestion,
                "Write the answer on stdin",
            ));
        }
        let q = self.waiting_for(caller, id, ONLY_ITS_LEAD_ANSWERS).await?;
        let pool = self.board.store().pool();
        let now = now_ms();
        let done = sqlx::query(
            "UPDATE human_questions SET answer = ?, answered_at = ?, lead_state = ?, lead_at = ?
             WHERE id = ? AND lead_state = ? AND answered_at IS NULL",
        )
        .bind(answer)
        .bind(now)
        .bind(ANSWERED)
        .bind(now)
        .bind(&q.id)
        .bind(WITH_LEAD)
        .execute(pool)
        .await?;
        if done.rows_affected() != 1 {
            return Err(gone(&q));
        }
        self.leads.settle_handover(OWNER, &q.id);
        log(
            &self.board,
            &q,
            ANSWERED,
            json!({ "answer": answer }),
            actor,
        )
        .await?;
        let child = self.board.ticket(&q.child.to_string()).await?;
        if let Some(session) = self.sessions.running_of(&child.id, SessionKind::Agent) {
            self.deliveries
                .queue_delivery(
                    &session.id,
                    DeliveryKind::AgentMessage,
                    &answer_text(&q.lead, answer),
                )
                .await
                .map_err(|e| BoardError::refused(Refusal::SessionEnded, e.to_string()))?;
        }
        tracing::info!(child = %q.child, lead = %q.lead, "the lead answered");
        self.changed(&[&q.child, &q.epic]).await;
        self.reread(&q.id).await
    }

    /// `otaq ask human --pass-on <QUESTION-ID> [--option … [--note …]] [--recommended N] --stdin` from the lead: the
    /// child's "Asks you", with the lead's options and their notes (the child's when no options are given),
    /// recommendation and reason.
    #[allow(clippy::too_many_arguments)]
    pub async fn pass_on(
        &self,
        caller: &Caller,
        id: &str,
        options: &[String],
        notes: &[Option<String>],
        recommended: Option<u32>,
        reason: &str,
        actor: &Actor,
    ) -> Result<LeadQuestion, BoardError> {
        let reason = reason.trim();
        if reason.is_empty() {
            return Err(BoardError::refused(
                Refusal::BadQuestion,
                "Write your reason on stdin",
            ));
        }
        let q = self.waiting_for(caller, id, ONLY_A_LEAD_PASSES_ON).await?;
        // Its own options take its own notes (none if it gave none); without options the child's both stay.
        let (options, notes): (Vec<String>, Vec<Option<String>>) = if options.is_empty() {
            if !no_notes(notes) {
                return Err(BoardError::refused(
                    Refusal::BadQuestion,
                    "--note must follow the --option it describes: without --option the child's options and \
                     notes stay",
                ));
            }
            (q.options.clone(), q.notes.clone())
        } else {
            validate_notes(options, notes)?;
            (
                options.iter().map(|o| o.trim().to_owned()).collect(),
                clean_notes(notes),
            )
        };
        validate_options(&options, recommended)?;
        let done = sqlx::query(
            "UPDATE human_questions SET options_json = ?, option_notes_json = ?, recommended = ?, lead_reason = ?,
                    lead_state = ?, lead_at = ?
             WHERE id = ? AND lead_state = ? AND answered_at IS NULL",
        )
        .bind(serde_json::to_string(&options).expect("strings serialize"))
        .bind(notes_json(&notes))
        .bind(recommended.map(i64::from))
        .bind(reason)
        .bind(PASSED_ON)
        .bind(now_ms())
        .bind(&q.id)
        .bind(WITH_LEAD)
        .execute(self.board.store().pool())
        .await?;
        if done.rows_affected() != 1 {
            return Err(gone(&q));
        }
        self.leads.settle_handover(OWNER, &q.id);
        let passed = self.reread(&q.id).await?;
        log(
            &self.board,
            &passed,
            PASSED_ON,
            json!({ "reason": reason, "recommended": passed.recommended_label() }),
            actor,
        )
        .await?;
        let child = self.board.ticket(&q.child.to_string()).await?;
        self.signals
            .ask_human(&Caller::User, &child.id, &q.question)
            .await?;
        tracing::info!(child = %q.child, lead = %q.lead, "the lead passed the question on");
        self.changed(&[&q.epic]).await;
        Ok(passed)
    }

    /// The question `id`, waiting for the calling lead; refused for anyone else, and once it left the lead.
    async fn waiting_for(
        &self,
        caller: &Caller,
        id: &str,
        only: &str,
    ) -> Result<LeadQuestion, BoardError> {
        let Caller::EpicLead { agent, epic_id, .. } = caller else {
            return Err(BoardError::refused(Refusal::NotAllowed, only));
        };
        let q = find(&self.board, id)
            .await?
            .ok_or_else(|| BoardError::NotFound(format!("question {id}")))?;
        let epic = self.board.ticket(&q.epic.to_string()).await?;
        if q.lead_agent.as_deref() != Some(agent.as_str()) || epic.id != *epic_id {
            return Err(BoardError::refused(Refusal::NotAllowed, only));
        }
        if q.state != LeadQuestionState::WithLead {
            return Err(gone(&q));
        }
        Ok(q)
    }

    async fn reread(&self, id: &str) -> Result<LeadQuestion, BoardError> {
        find(&self.board, id)
            .await?
            .ok_or_else(|| BoardError::NotFound(format!("question {id}")))
    }

    /// Clients refetch these tickets (cards, Messages, History).
    async fn changed(&self, keys: &[&TicketKey]) {
        for key in keys {
            if let Ok(t) = self.board.ticket(&key.to_string()).await {
                self.events.publish(Event::TicketChanged {
                    project: t.key.project.to_string(),
                    id: t.id,
                    key: t.key,
                });
            }
        }
    }
}

/// The refusal once a question left the lead: "Question 7 already went to the user" / "… was answered already".
fn gone(q: &LeadQuestion) -> BoardError {
    let message = match q.state {
        LeadQuestionState::Answered => format!("Question {} was answered already", q.id),
        _ => format!(
            "Question {} already went to the user: it no longer waits for you",
            q.id
        ),
    };
    BoardError::refused(Refusal::BadQuestion, message)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn question(options: &[&str]) -> HumanQuestion {
        HumanQuestion {
            id: "7".into(),
            text: "Can guests save items without logging in?".into(),
            options: options.iter().map(|o| (*o).to_owned()).collect(),
            recommended: None,
            notes: Vec::new(),
            asked_at: 0,
            via_lead: None,
        }
    }

    #[test]
    fn texts() {
        let cmd = naming::COMMAND_NAME;
        let child: TicketKey = "SHOP-40.2".parse().unwrap();
        assert_eq!(
            wake_text(
                &child,
                &question(&["Logged-in users only", "Guests too, kept in the browser"])
            ),
            format!(
                "SHOP-40.2 asks (question 7): Can guests save items without logging in? Options: 1. Logged-in \
                 users only 2. Guests too, kept in the browser. Answer with `{cmd} answer 7 --stdin`, or pass it on \
                 with `{cmd} ask human --pass-on 7 \u{2026}`."
            )
        );
        assert_eq!(
            wake_text(&child, &question(&[])),
            format!(
                "SHOP-40.2 asks (question 7): Can guests save items without logging in? Answer with `{cmd} answer 7 \
                 --stdin`, or pass it on with `{cmd} ask human --pass-on 7 \u{2026}`."
            )
        );
        assert_eq!(
            answer_text(
                "Wishlist lead",
                " Yes: filled heart means saved, outline means not saved (plan, step 2). "
            ),
            "Answer from Wishlist lead: Yes: filled heart means saved, outline means not saved (plan, step 2)."
        );
    }

    #[test]
    fn the_recommended_label() {
        let mut q = LeadQuestion {
            id: "7".into(),
            child: "SHOP-40.2".parse().unwrap(),
            child_title: "Wishlist button".into(),
            epic: "SHOP-40".parse().unwrap(),
            lead: "Wishlist lead".into(),
            lead_agent: Some("l1".into()),
            question: "Can guests save items?".into(),
            options: vec!["Logged-in users only".into(), "Guests too".into()],
            recommended: Some(1),
            notes: Vec::new(),
            state: LeadQuestionState::PassedOn,
            reason: Some("PAY-123 only mentions signed-in users.".into()),
            answer: None,
            asked_at: 0,
            lead_at: Some(1),
            answered_at: None,
        };
        assert_eq!(q.recommended_label(), Some("Logged-in users only"));
        q.recommended = Some(3);
        assert_eq!(q.recommended_label(), None);
        q.recommended = Some(0);
        assert_eq!(q.recommended_label(), None);
    }
}
