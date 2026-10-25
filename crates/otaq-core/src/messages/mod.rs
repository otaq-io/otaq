//! Agent-to-agent questions (MSG-01…MSG-06; d0030, d0048). `otaq ask <KEY> --stdin` works only between
//! directly linked tickets (one depends on the other) and never blocks the asker. Every question and answer is
//! stored in `messages` and logged on both tickets.
//!
//! - The target has a **live session**: the question is an automatic delivery, typed when its agent is idle and
//!   its input line is empty (d0048); it answers with `otaq answer <ID> --stdin`.
//! - The target **never had a session**: the ask is refused ("SHOP-18 hasn't started yet …", MSG-04).
//! - The target has only a **stored conversation** (normally a merged parent): the question waits here until the
//!   answer-only resume (MSG-03) or Needs you (MSG-05) takes it.
//!
//! The answer goes back to the asker the same way, and "asked SHOP-41 · waiting" lasts until it was typed
//! there (MSG-06).

pub mod answering;

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::board::tickets::{EventKind, Ticket, record_event};
use crate::board::{Board, BoardError, Refusal};
use crate::domain::{Actor, TicketKey, stored_enum};
use crate::naming;
use crate::service::EventBus;
use crate::session::deliveries::Deliveries;
use crate::session::{SessionKind, SessionTicket, Sessions};
use crate::store::{new_id, now_ms};

stored_enum! {
    /// How a question reaches its target (`messages.delivery`).
    pub enum Route ("message route") {
        /// Typed into the target's live session.
        Live = "live",
        /// The target's stored conversation was resumed answer-only (MSG-03).
        Resumed = "resumed",
        /// The user answers from Needs you (MSG-05). Stored as `inbox`, its name before d0114 (migrations 0001, 0029).
        NeedsYou = "inbox",
    }
}

/// Where a question stands, for the Messages section and the cards (MSG-06).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageState {
    /// Waiting to be typed into the target (or for its answer-only resume).
    Asked,
    /// Typed into the target's session; no answer yet.
    Delivered,
    /// In the user's Needs you (the target has no live session and finished tickets don't answer).
    NeedsYou,
    /// Answered; the answer waits to be typed into the asker's session.
    Answered,
    /// The answer reached the asker.
    AnswerDelivered,
}

/// The Project agent that asked a question (d0068).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MessageAgent {
    pub id: String,
    /// Its current name ("Checkout slowness", or "New conversation").
    pub name: String,
}

/// One question between two linked tickets, or from a Project agent to a ticket.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Message {
    pub id: String,
    /// The asking ticket; none when a Project agent asked.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from: Option<TicketKey>,
    /// The asking Project agent (d0068).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from_agent: Option<MessageAgent>,
    pub to: TicketKey,
    pub question: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub answer: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub route: Option<Route>,
    pub asked_at: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delivered_at: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub answered_at: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub answer_delivered_at: Option<i64>,
}

impl Message {
    /// Who asked, for people: "SHOP-20", or "Noyan · Checkout slowness".
    pub fn asker(&self) -> String {
        match (&self.from, &self.from_agent) {
            (Some(key), _) => key.to_string(),
            (None, Some(agent)) => format!("Noyan · {}", agent.name),
            (None, None) => "someone".to_owned(),
        }
    }

    pub fn state(&self) -> MessageState {
        if self.answer_delivered_at.is_some() {
            MessageState::AnswerDelivered
        } else if self.answer.is_some() {
            MessageState::Answered
        } else if self.route == Some(Route::NeedsYou) {
            MessageState::NeedsYou
        } else if self.delivered_at.is_some() {
            MessageState::Delivered
        } else {
            MessageState::Asked
        }
    }
}

/// Who asks: a ticket's agent, or a Project agent (its id).
enum Asker<'a> {
    Ticket(&'a Ticket),
    ProjectAgent(&'a str),
}

/// The question as asked, trimmed; refused when empty.
fn checked_question(question: &str) -> Result<&str, BoardError> {
    let question = question.trim();
    if question.is_empty() {
        return Err(BoardError::refused(
            Refusal::BadQuestion,
            "Write the question on stdin",
        ));
    }
    Ok(question)
}

/// Which part of a message a delivery carries (`pending_deliveries.message_part`).
pub const PART_QUESTION: &str = "question";
pub const PART_ANSWER: &str = "answer";

/// Whether two tickets are directly linked: one depends on the other (MSG-01).
pub fn linked(a: &Ticket, b: &Ticket) -> bool {
    a.depends_on.contains(&b.key) || b.depends_on.contains(&a.key)
}

/// "SHOP-18 hasn't started yet, so there is no progress to ask about. Ask the human instead (`otaq ask
/// human`)." (MSG-04).
pub fn never_started_message(target: &TicketKey) -> String {
    format!(
        "{target} hasn't started yet, so there is no progress to ask about. Ask the human instead (`{} ask human`).",
        naming::COMMAND_NAME
    )
}

/// What is typed into the target's session.
pub fn question_text(m: &Message) -> String {
    let cmd = naming::COMMAND_NAME;
    format!(
        "{} asks you (question {}): {}\n\nAnswer only this, with `{cmd} answer {} --stdin`, then carry on with \
         your own work.",
        match &m.from {
            Some(key) => format!("{key}'s agent"),
            None => m.asker(),
        },
        m.id,
        m.question,
        m.id
    )
}

/// What is typed into the asker's session.
pub fn answer_text(m: &Message) -> String {
    format!(
        "{} answered your question {} (\u{201c}{}\u{201d}):\n\n{}",
        m.to,
        m.id,
        first_line(&m.question),
        m.answer.as_deref().unwrap_or_default()
    )
}

fn first_line(text: &str) -> &str {
    text.lines().next().unwrap_or_default()
}

type MessageRow = (
    String,
    Option<String>,
    Option<String>,
    String,
    String,
    Option<String>,
    Option<String>,
    i64,
    Option<i64>,
    Option<i64>,
    Option<i64>,
);

const SELECT_MESSAGES: &str = "SELECT m.id, m.from_ticket_id, m.from_agent_id, m.to_ticket_id, m.question, m.answer, m.delivery, \
     m.asked_at, m.delivered_at, m.answered_at, m.answer_delivered_at FROM messages m";

impl Board {
    async fn message_from_row(&self, row: MessageRow) -> Result<Message, BoardError> {
        let (
            id,
            from,
            from_agent,
            to,
            question,
            answer,
            route,
            asked_at,
            delivered_at,
            answered_at,
            answer_delivered_at,
        ) = row;
        let from = match from {
            Some(id) => Some(self.ticket_by_id(&id).await?.key),
            None => None,
        };
        let from_agent = match from_agent {
            Some(id) => {
                let name: Option<(Option<String>, Option<String>)> =
                    sqlx::query_as("SELECT name, name_source FROM project_agents WHERE id = ?")
                        .bind(&id)
                        .fetch_optional(self.store().pool())
                        .await?;
                let name = name
                    .and_then(|(name, source)| source.and(name))
                    .unwrap_or_else(|| crate::project_agents::NEW_AGENT.to_owned());
                Some(MessageAgent { id, name })
            }
            None => None,
        };
        Ok(Message {
            id,
            from,
            from_agent,
            to: self.ticket_by_id(&to).await?.key,
            question,
            answer,
            route: route.and_then(|r| r.parse().ok()),
            asked_at,
            delivered_at,
            answered_at,
            answer_delivered_at,
        })
    }

    async fn messages_where(&self, filter: &str, id: &str) -> Result<Vec<Message>, BoardError> {
        let rows: Vec<MessageRow> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
            "{SELECT_MESSAGES} WHERE {filter} ORDER BY m.asked_at, m.id"
        )))
        .bind(id)
        .bind(id)
        .fetch_all(self.store().pool())
        .await?;
        let mut out = Vec::new();
        for row in rows {
            out.push(self.message_from_row(row).await?);
        }
        Ok(out)
    }

    /// A message by its ID.
    pub async fn message(&self, id: &str) -> Result<Message, BoardError> {
        self.messages_where("m.id = ? OR m.id = ?", id.trim())
            .await?
            .into_iter()
            .next()
            .ok_or_else(|| BoardError::NotFound(format!("question {id}")))
    }

    /// Every question to or from a ticket, oldest first (the Messages section, MSG-01).
    pub async fn messages_of(&self, ticket_id: &str) -> Result<Vec<Message>, BoardError> {
        self.messages_where("m.from_ticket_id = ? OR m.to_ticket_id = ?", ticket_id)
            .await
    }

    /// The questions waiting for a ticket's answer (`otaq questions`, prime).
    pub async fn pending_questions(&self, ticket_id: &str) -> Result<Vec<Message>, BoardError> {
        let key = self.ticket_by_id(ticket_id).await?.key;
        Ok(self
            .messages_of(ticket_id)
            .await?
            .into_iter()
            .filter(|m| m.answer.is_none() && m.to == key)
            .collect())
    }
}

/// Runs the questions between agents. Cheap to clone.
#[derive(Clone)]
pub struct Messages {
    inner: Arc<Inner>,
}

struct Inner {
    board: Board,
    sessions: Sessions,
    deliveries: Deliveries,
    events: EventBus,
    data: crate::data_dir::DataDir,
    /// The agent tools: whether a stored conversation can still be resumed, and its launch. Without a launch
    /// context (tests) no target is resumed.
    adapters: crate::agent_tools::Adapters,
    /// One routing pass at a time.
    routing: tokio::sync::Mutex<()>,
}

impl Messages {
    pub fn new(
        board: Board,
        sessions: Sessions,
        deliveries: Deliveries,
        events: EventBus,
        data: crate::data_dir::DataDir,
        adapters: crate::agent_tools::Adapters,
    ) -> Self {
        Self {
            inner: Arc::new(Inner {
                board,
                sessions,
                deliveries,
                events,
                data,
                adapters,
                routing: tokio::sync::Mutex::new(()),
            }),
        }
    }

    pub fn board(&self) -> &Board {
        &self.inner.board
    }

    /// `otaq ask <TARGET> --stdin` from `asker`'s agent (MSG-01, MSG-02, MSG-04).
    pub async fn ask(
        &self,
        asker: &str,
        target: &str,
        question: &str,
        actor: &Actor,
    ) -> Result<Message, BoardError> {
        let board = &self.inner.board;
        let question = checked_question(question)?;
        let from = board.ticket(asker).await?;
        let to = board.ticket(target).await?;
        if from.id == to.id {
            return Err(BoardError::refused(
                Refusal::BadQuestion,
                format!("{} can't ask itself", from.key),
            ));
        }
        if !linked(&from, &to) {
            return Err(BoardError::refused(
                Refusal::NotAllowed,
                format!(
                    "{} and {} aren't linked: ask only a ticket yours depends on, or one that depends on yours. \
                     Ask the human instead (`{} ask human`).",
                    from.key,
                    to.key,
                    naming::COMMAND_NAME
                ),
            ));
        }
        self.store_question(Asker::Ticket(&from), &to, question, actor)
            .await
    }

    /// `otaq ask <TARGET> --stdin` from a Project agent (PAG-02, d0068): to any ticket, with the same
    /// delivery and resuming rules (§16); the answer is typed into the Project agent's session.
    pub async fn ask_from_agent(
        &self,
        agent: &str,
        target: &str,
        question: &str,
        actor: &Actor,
    ) -> Result<Message, BoardError> {
        let question = checked_question(question)?;
        let to = self.inner.board.ticket(target).await?;
        self.store_question(Asker::ProjectAgent(agent), &to, question, actor)
            .await
    }

    async fn store_question(
        &self,
        from: Asker<'_>,
        to: &Ticket,
        question: &str,
        actor: &Actor,
    ) -> Result<Message, BoardError> {
        let board = &self.inner.board;
        let sessions = self
            .inner
            .sessions
            .history(&SessionTicket {
                id: to.id.clone(),
                key: to.key.clone(),
            })
            .await
            .map_err(|e| BoardError::refused(Refusal::SessionEnded, e.to_string()))?;
        if !sessions.iter().any(|s| s.kind == SessionKind::Agent) {
            return Err(BoardError::refused(
                Refusal::NotAllowed,
                never_started_message(&to.key),
            ));
        }
        let live = self.inner.sessions.running_of(&to.id, SessionKind::Agent);
        // A finished ticket answers by resuming its stored conversation (MSG-03): it must still exist.
        if live.is_none()
            && board
                .project_settings(to.key.project.as_str())
                .await?
                .let_finished_tickets_answer
            && self.stored_conversation(to).await?.is_none()
        {
            return Err(BoardError::refused(
                Refusal::NotAllowed,
                answering::no_longer_available(&to.key),
            ));
        }
        let id = new_id();
        let now = now_ms();
        let route = live.as_ref().map(|_| Route::Live);
        let (from_ticket, from_agent) = match from {
            Asker::Ticket(t) => (Some(t), None),
            Asker::ProjectAgent(a) => (None, Some(a)),
        };
        let mut tx = board.store().begin_write().await?;
        sqlx::query(
            "INSERT INTO messages (id, from_ticket_id, from_agent_id, to_ticket_id, question, delivery, asked_at)
             VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(from_ticket.map(|t| t.id.clone()))
        .bind(from_agent)
        .bind(&to.id)
        .bind(question)
        .bind(route.map(|r| r.as_str()))
        .bind(now)
        .execute(&mut *tx)
        .await?;
        if let Some(from) = from_ticket {
            record_event(
                &mut tx,
                &from.id,
                EventKind::QuestionAsked,
                actor,
                json!({ "id": id, "to": to.key, "question": question }),
            )
            .await?;
        }
        let asked_by = match from_ticket {
            Some(t) => json!({ "id": id, "from": t.key, "question": question }),
            None => json!({ "id": id, "from_agent": from_agent, "question": question }),
        };
        record_event(&mut tx, &to.id, EventKind::QuestionAsked, actor, asked_by).await?;
        tx.commit().await?;
        let message = board.message(&id).await?;
        if let Some(session) = live {
            self.inner
                .deliveries
                .queue_message(&session.id, &question_text(&message), &id, PART_QUESTION)
                .await
                .map_err(|e| BoardError::refused(Refusal::SessionEnded, e.to_string()))?;
        }
        tracing::info!(id, from = %message.asker(), to = %to.key, "question asked");
        self.changed_message(&message).await;
        if route.is_none()
            && let Err(e) = self.route_waiting().await
        {
            tracing::warn!(id, error = %e, "can't route the question");
        }
        board.message(&id).await
    }

    /// `otaq answer <ID> --stdin` from the target's agent: stored, logged, and typed into the asker's session
    /// when it is idle with an empty input line.
    pub async fn answer(
        &self,
        id: &str,
        answer: &str,
        actor: &Actor,
    ) -> Result<Message, BoardError> {
        let board = &self.inner.board;
        let answer = answer.trim();
        if answer.is_empty() {
            return Err(BoardError::refused(
                Refusal::BadQuestion,
                "Write the answer on stdin",
            ));
        }
        let message = board.message(id).await?;
        if message.answer.is_some() {
            return Err(BoardError::refused(
                Refusal::BadQuestion,
                format!("Question {} was answered already", message.id),
            ));
        }
        let from = match &message.from {
            Some(key) => Some(board.ticket(&key.to_string()).await?),
            None => None,
        };
        let to = board.ticket(&message.to.to_string()).await?;
        let now = now_ms();
        let mut tx = board.store().begin_write().await?;
        sqlx::query(
            "UPDATE messages SET answer = ?, answered_at = ?, delivered_at = COALESCE(delivered_at, ?) WHERE id = ?",
        )
        .bind(answer)
        .bind(now)
        .bind(now)
        .bind(&message.id)
        .execute(&mut *tx)
        .await?;
        if let Some(from) = &from {
            record_event(
                &mut tx,
                &from.id,
                EventKind::QuestionAnswered,
                actor,
                json!({ "id": message.id, "with": to.key, "answer": answer }),
            )
            .await?;
        }
        let with = match &from {
            Some(from) => json!(from.key),
            None => json!(message.asker()),
        };
        record_event(
            &mut tx,
            &to.id,
            EventKind::QuestionAnswered,
            actor,
            json!({ "id": message.id, "with": with, "answer": answer }),
        )
        .await?;
        tx.commit().await?;
        let answered = board.message(&message.id).await?;
        let asker_session = match (&from, &answered.from_agent) {
            (Some(from), _) => self
                .inner
                .sessions
                .running_of(&from.id, SessionKind::Agent)
                .map(|s| s.id),
            (None, Some(agent)) => self.agent_session(&agent.id).await?,
            (None, None) => None,
        };
        if let Some(session) = asker_session {
            self.inner
                .deliveries
                .queue_message(&session, &answer_text(&answered), &answered.id, PART_ANSWER)
                .await
                .map_err(|e| BoardError::refused(Refusal::SessionEnded, e.to_string()))?;
        }
        tracing::info!(id = answered.id, from = %answered.asker(), to = %to.key, "question answered");
        // An answer-only resume has done its job (MSG-03).
        if let Err(e) = self.end_answer_session(&to).await {
            tracing::warn!(key = %to.key, error = %e, "can't end the answer session");
        }
        self.changed_message(&answered).await;
        if let Err(e) = self.route_waiting().await {
            tracing::warn!(error = %e, "can't route the waiting questions");
        }
        board.message(&answered.id).await
    }

    /// The running session of the Project agent `agent`, if any.
    async fn agent_session(&self, agent: &str) -> Result<Option<String>, BoardError> {
        let newest: Option<(String,)> = sqlx::query_as(
            "SELECT id FROM sessions WHERE project_agent_id = ? ORDER BY started_at DESC, id DESC LIMIT 1",
        )
        .bind(agent)
        .fetch_optional(self.inner.board.store().pool())
        .await?;
        Ok(newest
            .map(|(id,)| id)
            .filter(|id| self.inner.sessions.info(id).is_ok_and(|s| s.is_running())))
    }

    /// Clients refetch the message's tickets, and the asking Project agent's list.
    async fn changed_message(&self, message: &Message) {
        let keys: Vec<&TicketKey> = message.from.iter().chain([&message.to]).collect();
        self.changed_keys(&keys).await;
        if let Some(agent) = &message.from_agent {
            self.inner
                .events
                .publish(crate::protocol::Event::ProjectAgentChanged {
                    project: message.to.project.to_string(),
                    id: agent.id.clone(),
                });
        }
    }

    async fn changed_keys(&self, keys: &[&TicketKey]) {
        for key in keys {
            if let Ok(t) = self.inner.board.ticket(&key.to_string()).await {
                self.changed(&[&t]);
            }
        }
    }

    /// Clients refetch both tickets (their Messages sections and card flags).
    fn changed(&self, tickets: &[&Ticket]) {
        for t in tickets {
            self.inner
                .events
                .publish(crate::protocol::Event::TicketChanged {
                    project: t.key.project.to_string(),
                    id: t.id.clone(),
                    key: t.key.clone(),
                });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn message() -> Message {
        Message {
            id: "0199".into(),
            from: Some("SHOP-20".parse().unwrap()),
            from_agent: None,
            to: "SHOP-18".parse().unwrap(),
            question: "What does POST /checkout return on a declined card?".into(),
            answer: None,
            route: Some(Route::Live),
            asked_at: 1,
            delivered_at: None,
            answered_at: None,
            answer_delivered_at: None,
        }
    }

    #[test]
    fn states_follow_the_timestamps() {
        let mut m = message();
        assert_eq!(m.state(), MessageState::Asked);
        m.delivered_at = Some(2);
        assert_eq!(m.state(), MessageState::Delivered);
        m.answer = Some("402 with a code".into());
        assert_eq!(m.state(), MessageState::Answered);
        m.answer_delivered_at = Some(3);
        assert_eq!(m.state(), MessageState::AnswerDelivered);
        let mut needs_you = message();
        needs_you.route = Some(Route::NeedsYou);
        assert_eq!(needs_you.state(), MessageState::NeedsYou);
    }

    #[test]
    fn texts() {
        let cmd = naming::COMMAND_NAME;
        assert_eq!(
            never_started_message(&"SHOP-18".parse().unwrap()),
            format!(
                "SHOP-18 hasn't started yet, so there is no progress to ask about. Ask the human instead (`{cmd} ask human`)."
            )
        );
        let text = question_text(&message());
        assert!(text.starts_with("SHOP-20's agent asks you (question 0199): What does POST"));
        assert!(text.contains(&format!("`{cmd} answer 0199 --stdin`")));
        let mut answered = message();
        answered.answer = Some("402 with {code}".into());
        assert_eq!(
            answer_text(&answered),
            "SHOP-18 answered your question 0199 (\u{201c}What does POST /checkout return on a declined card?\u{201d}):\n\n402 with {code}"
        );
    }
}
