//! A helper's questions go to the agent in charge first (d0133 audit 3; d0048, d0030, d0061, d0119).
//!
//! - **`otaq ask human` from a helper** ([`Helpers::ask`]): the question is stored on the ticket, asked by the helper
//!   and marked as with the agent in charge, and the agent in charge is told as a note typed when it is idle (d0048):
//!   "Piece 3's helper asks: … Answer with `otaq helper answer 3 --stdin`, or pass it on with `otaq helper pass-on
//!   <id>`." The ticket stays Working (it never holds Asks you, [`crate::board::questions::FOR_THE_USER`]); the
//!   helper's row reads "asked · in charge is answering". With no agent in charge running, it goes to the user at once.
//! - **`otaq helper answer <n> --stdin`** ([`Helpers::answer`]): the agent in charge's answer is recorded as its and
//!   typed into the helper in 0061's form ("Answer to your question: …"). Activity has it; the card never showed it
//!   (d0042, d0114).
//! - **`otaq helper pass-on <id> [--note …]`** ([`Helpers::pass_on`]): the question becomes the user's. The ticket
//!   holds Asks you while it is open (d0119), its Needs you item reads "SHOP-40 · 40.2 asks" with the agent in
//!   charge's note ("passed this to you: …"); the user's answer from the answer box or a line sent with Enter in the
//!   helper's terminal is typed into (or was typed in) the helper and closes it ([`Helpers::user_answered`],
//!   [`Helpers::enter`]).
//! - **Logged** (d0030): every step is a Helper entry in the ticket's History, naming who answered.

use super::{Helper, Helpers, store};
use crate::board::questions::{AnsweredVia, Choice, HumanQuestion, format_answer};
use crate::board::tickets::Ticket;
use crate::board::{BoardError, Refusal};
use crate::caller::Caller;
use crate::domain::{PieceWait, PieceWaitKind, TicketKey, stored_enum};
use crate::naming;
use crate::session::SessionKind;
use crate::session::deliveries::DeliveryKind;
use crate::store::{new_id, now_ms};

stored_enum! {
    /// Where a helper's question stands (`human_questions.helper_state`).
    pub enum HelperQuestionState ("helper question state") {
        /// The agent in charge has it: "asked · in charge is answering".
        WithInCharge = "with_in_charge",
        /// The agent in charge answered it.
        Answered = "answered",
        /// The agent in charge passed it on: the user's, holding the ticket's Asks you until answered (d0119).
        PassedOn = "passed_on",
    }
}

/// The note typed into the agent in charge: "Piece 3's helper asks: Can guests save items? (options: 1. Logged-in
/// users only, 2. Guests too). Answer with `otaq helper answer 3 --stdin`, or pass it on with `otaq helper pass-on
/// q1`."
pub fn ask_note(piece: u32, question: &HumanQuestion) -> String {
    let cmd = naming::COMMAND_NAME;
    let text = question.text.trim();
    let mut asked = if question.options.is_empty() {
        text.to_owned()
    } else {
        let listed: Vec<String> = question
            .options
            .iter()
            .enumerate()
            .map(|(n, label)| format!("{}. {label}", n + 1))
            .collect();
        format!("{text} (options: {})", listed.join(", "))
    };
    if !asked.ends_with(['.', '?', '!']) {
        asked.push('.');
    }
    format!(
        "Piece {piece}'s helper asks: {asked} Answer with `{cmd} helper answer {piece} --stdin`, or pass it on with \
         `{cmd} helper pass-on {}`.",
        question.id
    )
}

/// The Needs you item's note: "passed this to you: it's a product question the plan doesn't answer".
pub fn passed_on_note(note: Option<&str>) -> String {
    match note.map(str::trim).filter(|n| !n.is_empty()) {
        Some(note) => format!("passed this to you: {note}"),
        None => "passed this to you".to_owned(),
    }
}

/// The chip and the Needs you item: "SHOP-40 · 40.2 asks".
pub fn asks_chip(ticket: &TicketKey, label: &str) -> String {
    format!("{ticket} \u{b7} {label} asks")
}

/// The card's line: "40.2 asks: Can guests save items?".
pub fn asks_line(label: &str, question: &str) -> String {
    format!("{label} asks: {}", question.trim())
}

/// A helper's open question to the user, as the ticket's [`PieceWait`].
pub fn asks_wait(helper: &Helper, question: &str, note: Option<&str>) -> PieceWait {
    let label = helper.label();
    PieceWait {
        piece: helper.piece,
        chip: asks_chip(&helper.ticket, &label),
        line: asks_line(&label, question),
        label,
        kind: PieceWaitKind::Asks,
        session: helper.session.clone(),
        note: Some(passed_on_note(note)),
    }
}

/// `otaq ask human`'s answer to a helper: what it is told.
pub fn asked_reply(label: &str, in_charge: bool) -> String {
    if in_charge {
        format!(
            "Asked the agent in charge ({label}'s question): its answer, or the user's if it passes it on, is typed into \
             your session. End your turn."
        )
    } else {
        format!(
            "The agent in charge isn't running: {label}'s question went to the user. Their answer is typed into your \
             session. End your turn."
        )
    }
}

/// A helper's question as stored, with where it stands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HelperQuestion {
    pub question: HumanQuestion,
    pub helper_id: String,
    pub ticket_id: String,
    pub state: HelperQuestionState,
    pub note: Option<String>,
    pub answered: bool,
}

#[derive(sqlx::FromRow)]
struct Row {
    id: String,
    ticket_id: String,
    helper_id: String,
    question: String,
    options_json: String,
    recommended: Option<i64>,
    option_notes_json: Option<String>,
    asked_at: i64,
    helper_state: String,
    pass_on_note: Option<String>,
    answered_at: Option<i64>,
}

const SELECT: &str = "SELECT id, ticket_id, helper_id, question, options_json, recommended, option_notes_json, \
                      asked_at, helper_state, pass_on_note, answered_at FROM human_questions";

fn of_row(row: Row) -> Option<HelperQuestion> {
    Some(HelperQuestion {
        question: HumanQuestion {
            id: row.id,
            text: row.question,
            options: serde_json::from_str(&row.options_json).unwrap_or_default(),
            recommended: row.recommended.and_then(|n| u32::try_from(n).ok()),
            notes: crate::board::questions::notes_from_json(row.option_notes_json.as_deref()),
            asked_at: row.asked_at,
        },
        helper_id: row.helper_id,
        ticket_id: row.ticket_id,
        state: row.helper_state.parse().ok()?,
        note: row.pass_on_note,
        answered: row.answered_at.is_some(),
    })
}

/// A helper's question by its id; `None` for any other question.
pub async fn find(pool: &sqlx::SqlitePool, id: &str) -> Result<Option<HelperQuestion>, BoardError> {
    let row: Option<Row> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "{SELECT} WHERE id = ? AND helper_id IS NOT NULL AND helper_state IS NOT NULL"
    )))
    .bind(id.trim())
    .fetch_optional(pool)
    .await?;
    Ok(row.and_then(of_row))
}

/// The helper's open question, if any.
pub async fn open_of(
    pool: &sqlx::SqlitePool,
    helper_id: &str,
) -> Result<Option<HelperQuestion>, BoardError> {
    let row: Option<Row> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "{SELECT} WHERE helper_id = ? AND helper_state IS NOT NULL AND answered_at IS NULL
         ORDER BY asked_at DESC, id DESC LIMIT 1"
    )))
    .bind(helper_id)
    .fetch_optional(pool)
    .await?;
    Ok(row.and_then(of_row))
}

/// The ticket's questions still with the agent in charge, oldest first (told again to a resumed agent in charge).
pub async fn with_in_charge(
    pool: &sqlx::SqlitePool,
    ticket_id: &str,
) -> Result<Vec<HelperQuestion>, BoardError> {
    let rows: Vec<Row> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "{SELECT} WHERE ticket_id = ? AND helper_id IS NOT NULL AND helper_state = ? AND answered_at IS NULL
         ORDER BY asked_at, id"
    )))
    .bind(ticket_id)
    .bind(HelperQuestionState::WithInCharge.as_str())
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().filter_map(of_row).collect())
}

impl Helpers {
    /// `otaq ask human` from a helper (see [`self`]): stored, and the agent in charge is told; with none running the
    /// question goes to the user at once. Returns the helper and what it is told.
    pub async fn ask(
        &self,
        caller: &Caller,
        text: &str,
        options: &[String],
        notes: &[Option<String>],
        recommended: Option<u32>,
    ) -> Result<super::HelperReply, BoardError> {
        let (ticket, helper) = self.helper_of(caller).await?;
        if text.trim().is_empty() {
            return Err(BoardError::refused(
                Refusal::BadQuestion,
                "Write the question on stdin",
            ));
        }
        crate::board::questions::validate_options(options, recommended)?;
        crate::board::questions::validate_notes(options, notes)?;
        let in_charge = self
            .inner
            .sessions
            .running_of(&ticket.id, SessionKind::Agent);
        let state = match in_charge {
            Some(_) => HelperQuestionState::WithInCharge,
            None => HelperQuestionState::PassedOn,
        };
        let question = HumanQuestion {
            id: new_id(),
            text: text.trim().to_owned(),
            options: options.iter().map(|o| o.trim().to_owned()).collect(),
            recommended,
            notes: crate::board::questions::clean_notes(notes),
            asked_at: now_ms(),
        };
        let mut tx = self.inner.board.store().begin_write().await?;
        // A helper has one open question: a new one replaces its older (d0119).
        sqlx::query(
            "UPDATE human_questions SET answered_at = ?, answered_via = ? WHERE helper_id = ? AND answered_at IS NULL",
        )
        .bind(question.asked_at)
        .bind(AnsweredVia::Replaced.as_str())
        .bind(&helper.id)
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "INSERT INTO human_questions (id, ticket_id, helper_id, helper_state, question, options_json, recommended,
                                          option_notes_json, asked_at, helper_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&question.id)
        .bind(&ticket.id)
        .bind(&helper.id)
        .bind(state.as_str())
        .bind(&question.text)
        .bind(serde_json::to_string(&question.options).expect("strings serialize"))
        .bind(question.recommended.map(i64::from))
        .bind(crate::board::questions::notes_json(&question.notes))
        .bind(question.asked_at)
        .bind((state == HelperQuestionState::PassedOn).then_some(question.asked_at))
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        self.record(&ticket, &helper, "asked", Some(&question.text))
            .await?;
        let label = helper.label();
        match in_charge {
            // Typed when the agent in charge is idle, never into its running turn (d0048).
            Some(in_charge) => self
                .inner
                .deliveries
                .queue_delivery(
                    &in_charge.id,
                    DeliveryKind::AgentMessage,
                    &ask_note(helper.piece, &question),
                )
                .await
                .map_err(|e| BoardError::refused(Refusal::SessionEnded, e.to_string()))?,
            None => self.wait_changed(&ticket.id, None).await?,
        }
        tracing::info!(ticket = %ticket.key, piece = helper.piece, ?state, "a helper asked");
        let helper = self.piece(&ticket, helper.piece).await?;
        Ok(super::HelperReply {
            text: asked_reply(&label, state == HelperQuestionState::WithInCharge),
            helper,
        })
    }

    /// The question of piece `piece` still with the agent in charge `caller`.
    async fn waiting_question(
        &self,
        ticket: &Ticket,
        helper: &Helper,
    ) -> Result<HelperQuestion, BoardError> {
        let label = helper.label();
        let open = open_of(self.inner.board.store().pool(), &helper.id).await?;
        match open {
            Some(q) if q.state == HelperQuestionState::WithInCharge => Ok(q),
            Some(_) => Err(BoardError::refused(
                Refusal::BadQuestion,
                format!("{label}'s question went to the user: it no longer waits for you"),
            )),
            None => Err(BoardError::refused(
                Refusal::BadQuestion,
                format!("{label} has no question waiting for you ({})", ticket.key),
            )),
        }
    }

    /// `otaq helper answer <n> --stdin` from the agent in charge: recorded as its answer and typed into the helper when
    /// it is idle (d0048), in 0061's form.
    pub async fn answer(
        &self,
        caller: &Caller,
        key: &str,
        piece: u32,
        answer: &str,
    ) -> Result<super::HelperReply, BoardError> {
        let answer = answer.trim();
        if answer.is_empty() {
            return Err(BoardError::refused(
                Refusal::BadQuestion,
                "Write the answer on stdin",
            ));
        }
        let (ticket, _) = self.in_charge(caller, key).await?;
        let helper = self.piece(&ticket, piece).await?;
        let q = self.waiting_question(&ticket, &helper).await?;
        let label = helper.label();
        let live = self.live_session(&helper).ok_or_else(|| {
            BoardError::refused(
                Refusal::SessionEnded,
                format!(
                    "{label}'s helper isn't running: start it again with `{} helper start {piece}`",
                    naming::COMMAND_NAME
                ),
            )
        })?;
        let now = now_ms();
        let done = sqlx::query(
            "UPDATE human_questions SET answer = ?, answered_at = ?, helper_state = ?, helper_at = ?
             WHERE id = ? AND helper_state = ? AND answered_at IS NULL",
        )
        .bind(answer)
        .bind(now)
        .bind(HelperQuestionState::Answered.as_str())
        .bind(now)
        .bind(&q.question.id)
        .bind(HelperQuestionState::WithInCharge.as_str())
        .execute(self.inner.board.store().pool())
        .await?;
        if done.rows_affected() != 1 {
            return Err(BoardError::refused(
                Refusal::BadQuestion,
                format!("{label}'s question was answered already"),
            ));
        }
        self.inner
            .deliveries
            .queue_delivery(
                &live.id,
                DeliveryKind::AgentMessage,
                &format_answer(&q.question, &Choice::Other(answer.to_owned())),
            )
            .await
            .map_err(|e| BoardError::refused(Refusal::SessionEnded, e.to_string()))?;
        self.record_answer(&ticket, &helper, "answered", answer)
            .await?;
        let helper = self.piece(&ticket, piece).await?;
        Ok(super::HelperReply {
            text: format!(
                "Answered {label}'s question: it is typed into its session as soon as it is idle."
            ),
            helper,
        })
    }

    /// `otaq helper pass-on <id> [--note …]` from the agent in charge: the question becomes the user's, with the note.
    pub async fn pass_on(
        &self,
        caller: &Caller,
        key: &str,
        id: &str,
        note: Option<&str>,
    ) -> Result<super::HelperReply, BoardError> {
        let (ticket, _) = self.in_charge(caller, key).await?;
        let pool = self.inner.board.store().pool();
        let q = find(pool, id)
            .await?
            .filter(|q| q.ticket_id == ticket.id)
            .ok_or_else(|| {
                BoardError::NotFound(format!(
                    "{} has no helper question {}",
                    ticket.key,
                    id.trim()
                ))
            })?;
        let helper = store::by_id(pool, &q.helper_id)
            .await?
            .ok_or_else(|| BoardError::NotFound(format!("question {id}")))?;
        let q = self.waiting_question(&ticket, &helper).await?;
        if q.question.id != id.trim() {
            return Err(BoardError::refused(
                Refusal::BadQuestion,
                format!("Question {} was replaced by a newer one", id.trim()),
            ));
        }
        let note = note.map(str::trim).filter(|n| !n.is_empty());
        let done = sqlx::query(
            "UPDATE human_questions SET helper_state = ?, pass_on_note = ?, helper_at = ?
             WHERE id = ? AND helper_state = ? AND answered_at IS NULL",
        )
        .bind(HelperQuestionState::PassedOn.as_str())
        .bind(note)
        .bind(now_ms())
        .bind(&q.question.id)
        .bind(HelperQuestionState::WithInCharge.as_str())
        .execute(pool)
        .await?;
        let label = helper.label();
        if done.rows_affected() != 1 {
            return Err(BoardError::refused(
                Refusal::BadQuestion,
                format!("{label}'s question no longer waits for you"),
            ));
        }
        self.record(&ticket, &helper, "passed_on", note).await?;
        // Only now does the ticket turn turquoise (d0133 audit 3): the question holds its Asks you (d0119).
        self.wait_changed(&ticket.id, None).await?;
        let helper = self.piece(&ticket, helper.piece).await?;
        Ok(super::HelperReply {
            text: format!(
                "Passed {label}'s question on to the user. Their answer is typed into {label}'s session; carry on \
                 with the rest meanwhile."
            ),
            helper,
        })
    }

    /// The helper that asked the user's open question `question_id`, if a helper did.
    pub async fn of_question(&self, question_id: &str) -> Result<Option<Helper>, BoardError> {
        let pool = self.inner.board.store().pool();
        let Some(q) = find(pool, question_id).await? else {
            return Ok(None);
        };
        Ok(store::by_id(pool, &q.helper_id).await?)
    }

    /// The user answered a helper's passed-on question (`text` as typed into the helper; from the answer box, or the
    /// line sent with Enter in its terminal): closed with the answer, logged (d0030), and the ticket's status follows.
    pub async fn user_answered(
        &self,
        helper: &Helper,
        question_id: &str,
        text: &str,
        via: AnsweredVia,
    ) -> Result<bool, BoardError> {
        let board = &self.inner.board;
        if !board.close_question(question_id, Some(text), via).await? {
            return Ok(false);
        }
        let ticket = board.ticket(&helper.ticket.to_string()).await?;
        self.record_answer(&ticket, helper, "you_answered", text)
            .await?;
        self.wait_changed(&ticket.id, None).await?;
        Ok(true)
    }

    /// Records who answered in History (d0030): "answered" (the agent in charge), "you_answered" (the user).
    async fn record_answer(
        &self,
        ticket: &Ticket,
        helper: &Helper,
        what: &str,
        answer: &str,
    ) -> Result<(), BoardError> {
        self.record(ticket, helper, what, Some(answer)).await
    }

    /// Re-runs the ticket's status with its helpers' waits (a passed-on question, a prompt): `ended_prompt` is the
    /// line of a helper prompt that just closed.
    pub(super) async fn wait_changed(
        &self,
        ticket_id: &str,
        ended_prompt: Option<String>,
    ) -> Result<(), BoardError> {
        if let Some(signals) = self.inner.signals.get() {
            signals.helper_wait_changed(ticket_id, ended_prompt).await?;
        }
        Ok(())
    }

    /// The questions still with the agent in charge go to a new agent in charge again (after a restart, its old
    /// session's deliveries are gone).
    pub(super) async fn tell_in_charge_again(
        &self,
        ticket: &Ticket,
        in_charge: &str,
    ) -> Result<(), BoardError> {
        let pool = self.inner.board.store().pool();
        for q in with_in_charge(pool, &ticket.id).await? {
            let Some(helper) = store::by_id(pool, &q.helper_id).await? else {
                continue;
            };
            if let Err(e) = self
                .inner
                .deliveries
                .queue_delivery(
                    in_charge,
                    DeliveryKind::AgentMessage,
                    &ask_note(helper.piece, &q.question),
                )
                .await
            {
                tracing::warn!(ticket = %ticket.key, error = %e, "can't tell the agent in charge again");
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn question(options: &[&str]) -> HumanQuestion {
        HumanQuestion {
            id: "q7".into(),
            text: "Can guests save items without logging in?".into(),
            options: options.iter().map(|o| (*o).to_owned()).collect(),
            recommended: None,
            notes: Vec::new(),
            asked_at: 0,
        }
    }

    #[test]
    fn texts() {
        let cmd = naming::COMMAND_NAME;
        assert_eq!(
            ask_note(
                3,
                &question(&["Logged-in users only", "Guests too, kept in the browser"])
            ),
            format!(
                "Piece 3's helper asks: Can guests save items without logging in? (options: 1. Logged-in users only, \
                 2. Guests too, kept in the browser). Answer with `{cmd} helper answer 3 --stdin`, or pass it on with \
                 `{cmd} helper pass-on q7`."
            )
        );
        assert_eq!(
            ask_note(3, &question(&[])),
            format!(
                "Piece 3's helper asks: Can guests save items without logging in? Answer with `{cmd} helper answer 3 \
                 --stdin`, or pass it on with `{cmd} helper pass-on q7`."
            )
        );
        assert_eq!(
            passed_on_note(Some(" it's a product question the plan doesn't answer ")),
            "passed this to you: it's a product question the plan doesn't answer"
        );
        assert_eq!(passed_on_note(Some(" ")), "passed this to you");
        let key: TicketKey = "SHOP-40".parse().unwrap();
        assert_eq!(asks_chip(&key, "40.2"), "SHOP-40 \u{b7} 40.2 asks");
        assert_eq!(
            asks_line("40.2", " Can guests save items? "),
            "40.2 asks: Can guests save items?"
        );
    }
}
