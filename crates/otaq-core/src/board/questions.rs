//! Questions to the user (`otaq ask human`, ATT-01) with optional answer options (ATT-12, d0061).
//!
//! A question may carry **2–4 options** with short labels and, optionally, the recommended one (1-based); without
//! options it is free text. The UI always adds "Other…" (M7). Options apply only to questions to the human; a
//! question to another ticket (`ask <KEY>`, M10) stays free text. The answer is typed into the session as
//! [`format_answer`] words it.
//!
//! Each option may carry a **note**, its pros and cons (`--note` after its `--option`, ATT-17, d0061): stored whole,
//! never refused for its length or cut. The delivered answer doesn't name it.
//!
//! **A question can take several answers** (`--multiple`): the user ticks any of its options, Other… among them, and
//! it may recommend several (`--recommended 1,3`). The answer names every ticked option and Other…'s text
//! ([`Choice::Several`]). A question asked before reads as taking one answer.
//!
//! **An open question holds Needs you until it is closed** (d0119). A question closes ([`AnsweredVia`]) when the user
//! answers it from the answer box, presses Enter on a line in that agent's terminal (or the Ask bar's Continue to a
//! Project agent), when the same agent asks a new question (the older one is replaced), when its ticket is
//! closed, parked or delivered (a trigger, migration 0058), or when its ticket is sent back to an earlier stage
//! ([`close_on_send_back`]): the stage that asked is left behind. One agent has at most one open question.
//!
//! **A send-back question** ([`SendBackTo`]) names an earlier stage of the ticket's flow, the question being the
//! agent's reason: its two ways out are fixed, the user sends the ticket back there ([`Choice::SendBack`]) or answers
//! here (free text). It carries no options. The agent never moves a ticket back itself: only the user's answer does
//! ([`crate::board::send_back_question`]).

use serde::{Deserialize, Serialize};

use super::{Board, BoardError, Refusal};
use crate::domain::stored_enum;
use crate::store::{new_id, now_ms};

stored_enum! {
    /// How a question was closed (`human_questions.answered_via`, d0119).
    pub enum AnsweredVia ("answer source") {
        /// The user's answer from the strip, card or sheet (d0061).
        Box = "box",
        /// The line the user sent with Enter in that agent's terminal, or the Ask bar's Continue.
        Terminal = "terminal",
        /// The same agent asked a new question.
        Replaced = "replaced",
        /// Its ticket was closed, parked, delivered or sent back to an earlier stage.
        Closed = "closed",
    }
}

/// The most options a question may carry.
pub const MAX_OPTIONS: usize = 4;
/// The longest option label, in characters: labels are short so they fit Needs you, card and sheet.
pub const MAX_OPTION_LABEL: usize = 40;

/// A question an agent asked the user.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HumanQuestion {
    pub id: String,
    pub text: String,
    /// The answer options, in order; empty for a free-text question.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub options: Vec<String>,
    /// The recommended options, 1-based, in order: at most one unless the question takes several answers. One
    /// number in JSON when there is one, a list when there are several.
    #[serde(default, skip_serializing_if = "Vec::is_empty", with = "one_or_many")]
    pub recommended: Vec<u32>,
    /// The user may tick several of the options (and Other…); `false` for every question asked before.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub multiple: bool,
    /// Each option's note (ATT-17), in the options' order, `None` for an option without one; empty when no option
    /// has one.
    #[serde(default, skip_serializing_if = "no_notes")]
    pub notes: Vec<Option<String>>,
    pub asked_at: i64,
    /// The earlier stage the agent asks to send the ticket back to; `None` on every other question.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub send_back: Option<SendBackTo>,
}

/// The earlier stage a send-back question names: its id, and its name as it read when the agent asked.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SendBackTo {
    pub stage_id: String,
    pub stage: String,
}

/// An unanswered question of a ticket, as the board reads it for the cards.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct OpenQuestion {
    pub ticket_id: String,
    pub question: HumanQuestion,
    /// A helper's question the agent in charge passed on (d0133 audit 3): the helper, and the agent in charge's note.
    pub helper_id: Option<String>,
    pub pass_on_note: Option<String>,
}

/// An `ask human` question with the user's answer, for the Messages section.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HumanAnswer {
    pub question: String,
    pub answer: String,
    pub asked_at: i64,
    pub answered_at: i64,
}

/// The user's answer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Choice {
    /// One of the options, 1-based.
    Option(u32),
    /// "Other…": free text; on a send-back question, the answer here.
    Other(String),
    /// Send the ticket back to the stage a send-back question names.
    SendBack,
    /// A question that takes several answers: the ticked options (1-based) and Other…'s text when it is ticked.
    Several {
        options: Vec<u32>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        other: Option<String>,
    },
}

/// The recommended options in JSON: one number when there is one (as every question before several answers), a list
/// when there are several; either reads back.
pub mod one_or_many {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    #[derive(Deserialize)]
    #[serde(untagged)]
    enum OneOrMany {
        One(u32),
        Many(Vec<u32>),
    }

    pub fn serialize<S: Serializer>(numbers: &[u32], serializer: S) -> Result<S::Ok, S::Error> {
        match numbers {
            [one] => one.serialize(serializer),
            many => many.serialize(serializer),
        }
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<u32>, D::Error> {
        Ok(match Option::<OneOrMany>::deserialize(deserializer)? {
            None => Vec::new(),
            Some(OneOrMany::One(n)) => vec![n],
            Some(OneOrMany::Many(numbers)) => numbers,
        })
    }
}

/// Checks the options of a question: none, or 2–4 short labels, and recommendations among them: one, or several when
/// the question takes several answers (`multiple`), which needs options.
pub fn validate_options(
    options: &[String],
    recommended: &[u32],
    multiple: bool,
) -> Result<(), BoardError> {
    let refuse = |message: String| Err(BoardError::refused(Refusal::BadQuestion, message));
    if options.len() == 1 || options.len() > MAX_OPTIONS {
        return refuse("Give 2–4 options, or none for a free-text question".to_owned());
    }
    if options.iter().any(|o| o.trim().is_empty()) {
        return refuse("An option can't be empty".to_owned());
    }
    if let Some(long) = options
        .iter()
        .find(|o| o.trim().chars().count() > MAX_OPTION_LABEL)
    {
        return refuse(format!(
            "Option labels can be at most {MAX_OPTION_LABEL} characters: \"{}\" is too long",
            long.trim()
        ));
    }
    if multiple && options.is_empty() {
        return refuse("--multiple needs options to choose from".to_owned());
    }
    if !recommended.is_empty() && options.is_empty() {
        return refuse("--recommended needs options to choose from".to_owned());
    }
    if recommended
        .iter()
        .any(|&n| n == 0 || n as usize > options.len())
    {
        return refuse(format!(
            "--recommended must be between 1 and {}",
            options.len()
        ));
    }
    if clean_recommended(recommended).len() > 1 && !multiple {
        return refuse(
            "Recommend one option, or ask with --multiple to let the user pick several".to_owned(),
        );
    }
    Ok(())
}

/// The recommended options as stored: in order, each once.
pub fn clean_recommended(recommended: &[u32]) -> Vec<u32> {
    let mut numbers = recommended.to_vec();
    numbers.sort_unstable();
    numbers.dedup();
    numbers
}

/// `recommended_json`: the recommended options when there are several; NULL otherwise (`recommended` holds the one).
pub(crate) fn recommended_json(recommended: &[u32]) -> Option<String> {
    (recommended.len() > 1).then(|| serde_json::to_string(recommended).expect("numbers serialize"))
}

/// The recommended options read back: `recommended_json`'s list when there are several, else `recommended`'s one.
pub(crate) fn recommended_from(one: Option<i64>, json: Option<&str>) -> Vec<u32> {
    json.and_then(|j| serde_json::from_str::<Vec<u32>>(j).ok())
        .filter(|numbers| !numbers.is_empty())
        .unwrap_or_else(|| {
            one.and_then(|n| u32::try_from(n).ok())
                .into_iter()
                .collect()
        })
}

/// No option has a note.
pub fn no_notes(notes: &[Option<String>]) -> bool {
    notes.iter().all(Option::is_none)
}

/// Checks the notes of a question's options (ATT-17): none, or one entry per option (`None` for an option without
/// a note). Never refused for length.
pub fn validate_notes(options: &[String], notes: &[Option<String>]) -> Result<(), BoardError> {
    if no_notes(notes) {
        return Ok(());
    }
    let refuse = |message: &str| Err(BoardError::refused(Refusal::BadQuestion, message));
    if options.is_empty() {
        return refuse(
            "A free-text question takes no --note: a --note describes the --option before it",
        );
    }
    if notes.len() != options.len() {
        return refuse("Give the notes one per option, in the options' order");
    }
    Ok(())
}

/// The notes as stored: surrounding whitespace trimmed, an empty note none, and empty when no option has one.
pub fn clean_notes(notes: &[Option<String>]) -> Vec<Option<String>> {
    let notes: Vec<Option<String>> = notes
        .iter()
        .map(|n| {
            n.as_deref()
                .map(str::trim)
                .filter(|n| !n.is_empty())
                .map(str::to_owned)
        })
        .collect();
    if no_notes(&notes) { Vec::new() } else { notes }
}

/// `option_notes_json`: NULL when no option has a note.
pub(crate) fn notes_json(notes: &[Option<String>]) -> Option<String> {
    (!no_notes(notes)).then(|| serde_json::to_string(notes).expect("strings serialize"))
}

/// Reads `option_notes_json`; NULL (every question asked before M26) or unreadable is no notes.
pub(crate) fn notes_from_json(json: Option<&str>) -> Vec<Option<String>> {
    let notes: Vec<Option<String>> = json
        .and_then(|j| serde_json::from_str(j).ok())
        .unwrap_or_default();
    if no_notes(&notes) { Vec::new() } else { notes }
}

/// The text typed into the agent's session: "Answer to your question: Postmark (option 1)." for an option,
/// "Answer to your question: <text>" for "Other…". Several answers name every ticked option, then Other…'s text:
/// "Answer to your question: Postmark (option 1), SES (option 3). Also: <text>".
pub fn format_answer(question: &HumanQuestion, choice: &Choice) -> String {
    let named = |n: u32| {
        let label = question
            .options
            .get((n as usize).wrapping_sub(1))
            .map_or("?", String::as_str);
        format!("{label} (option {n})")
    };
    match choice {
        Choice::Option(n) => format!("Answer to your question: {}.", named(*n)),
        Choice::Other(text) => format!("Answer to your question: {}", text.trim()),
        Choice::Several { options, other } => {
            let other = other.as_deref().map(str::trim).filter(|t| !t.is_empty());
            if options.is_empty() {
                return format!("Answer to your question: {}", other.unwrap_or_default());
            }
            let picked = options.iter().map(|&n| named(n)).collect::<Vec<_>>();
            match other {
                Some(other) => format!(
                    "Answer to your question: {}. Also: {other}",
                    picked.join(", ")
                ),
                None => format!("Answer to your question: {}.", picked.join(", ")),
            }
        }
        Choice::SendBack => match &question.send_back {
            Some(to) => format!("Sent back to {}.", to.stage),
            None => "Sent back.".to_owned(),
        },
    }
}

#[derive(sqlx::FromRow)]
struct QuestionRow {
    id: String,
    question: String,
    options_json: String,
    recommended: Option<i64>,
    recommended_json: Option<String>,
    multiple: bool,
    option_notes_json: Option<String>,
    asked_at: i64,
    send_back_stage_id: Option<String>,
    send_back_stage: Option<String>,
}

/// The columns of [`QuestionRow`], and where they come from (`human_questions q`).
const QUESTION_COLUMNS: &str = "q.id, q.question, q.options_json, q.recommended, q.recommended_json, q.multiple, \
     q.option_notes_json, q.asked_at, q.send_back_stage_id, q.send_back_stage";
const QUESTION_FROM: &str = "human_questions q";
/// The ticket's questions to the user (d0133 audit 3): its agent's own, and a helper's once the agent in charge
/// passed it on. A helper's question still with the agent in charge, or answered by it, never is.
pub(crate) const FOR_THE_USER: &str = "(q.helper_id IS NULL OR q.helper_state = 'passed_on')";

impl From<QuestionRow> for HumanQuestion {
    fn from(row: QuestionRow) -> Self {
        Self {
            id: row.id,
            text: row.question,
            options: serde_json::from_str(&row.options_json).unwrap_or_default(),
            recommended: recommended_from(row.recommended, row.recommended_json.as_deref()),
            multiple: row.multiple,
            notes: notes_from_json(row.option_notes_json.as_deref()),
            asked_at: row.asked_at,
            send_back: row.send_back_stage_id.map(|stage_id| SendBackTo {
                stage: row.send_back_stage.unwrap_or_else(|| stage_id.clone()),
                stage_id,
            }),
        }
    }
}

/// The ticket was sent back to an earlier stage (a human check's Send back, a drag back, an accepted send-back
/// question), inside the move's transaction: its agent's open question for the user closes as `closed`, so it leaves
/// Needs you and the earlier stage's agent starts without it. A helper's questions are its helper's own (they go when
/// it ends).
pub(crate) async fn close_on_send_back(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    ticket_id: &str,
) -> Result<(), BoardError> {
    sqlx::query(
        "UPDATE human_questions SET answered_at = ?, answered_via = ?
         WHERE ticket_id = ? AND answered_at IS NULL AND helper_id IS NULL",
    )
    .bind(now_ms())
    .bind(AnsweredVia::Closed.as_str())
    .bind(ticket_id)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

impl Board {
    /// Stores a question of `ticket_id` (options already validated).
    pub async fn store_question(
        &self,
        ticket_id: &str,
        text: &str,
        options: &[String],
        notes: &[Option<String>],
        recommended: &[u32],
        multiple: bool,
    ) -> Result<HumanQuestion, BoardError> {
        self.insert_question(
            "ticket_id",
            ticket_id,
            text,
            (options, notes),
            (recommended, multiple),
            None,
        )
        .await
    }

    /// Stores the send-back question of `ticket_id` (the stage already checked to be an earlier one): `text` is the
    /// agent's reason; it carries no options.
    pub async fn store_send_back_question(
        &self,
        ticket_id: &str,
        text: &str,
        to: &SendBackTo,
    ) -> Result<HumanQuestion, BoardError> {
        self.insert_question(
            "ticket_id",
            ticket_id,
            text,
            (&[], &[]),
            (&[], false),
            Some(to),
        )
        .await
    }

    /// Stores a question of the Project agent `agent` (`ask human` from it, d0068).
    pub async fn store_agent_question(
        &self,
        agent: &str,
        text: &str,
        options: &[String],
        notes: &[Option<String>],
        recommended: &[u32],
        multiple: bool,
    ) -> Result<HumanQuestion, BoardError> {
        self.insert_question(
            "project_agent_id",
            agent,
            text,
            (options, notes),
            (recommended, multiple),
            None,
        )
        .await
    }

    /// The Project agent's unanswered question, if any (the newest).
    pub async fn open_agent_question(
        &self,
        agent: &str,
    ) -> Result<Option<HumanQuestion>, BoardError> {
        let row: Option<QuestionRow> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
            "SELECT {QUESTION_COLUMNS} FROM {QUESTION_FROM} WHERE q.project_agent_id = ? AND q.answered_at IS NULL
             ORDER BY q.asked_at DESC, q.id DESC LIMIT 1"
        )))
        .bind(agent)
        .fetch_optional(self.store().pool())
        .await?;
        Ok(row.map(Into::into))
    }

    /// The question the ticket's agent holds for the user (d0119): its open question.
    pub async fn open_user_question(
        &self,
        ticket_id: &str,
    ) -> Result<Option<HumanQuestion>, BoardError> {
        let row: Option<QuestionRow> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
            "SELECT {QUESTION_COLUMNS} FROM {QUESTION_FROM} WHERE q.ticket_id = ? AND q.answered_at IS NULL
             AND {FOR_THE_USER} ORDER BY q.asked_at DESC, q.id DESC LIMIT 1"
        )))
        .bind(ticket_id)
        .fetch_optional(self.store().pool())
        .await?;
        Ok(row.map(Into::into))
    }

    /// The user sent `line` in the ticket's agent terminal while its agent's question was open (d0119): the line is
    /// its answer. Returns the question it answered; `None` (nothing closed) for an empty line or no open question.
    pub async fn answer_in_terminal(
        &self,
        ticket_id: &str,
        line: &str,
    ) -> Result<Option<HumanQuestion>, BoardError> {
        let line = line.trim();
        if line.is_empty() {
            return Ok(None);
        }
        // Its own agent's question only: a helper's passed-on one is answered in the helper's terminal (d0133).
        let row: Option<QuestionRow> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
            "SELECT {QUESTION_COLUMNS} FROM {QUESTION_FROM} WHERE q.ticket_id = ? AND q.answered_at IS NULL
             AND q.helper_id IS NULL ORDER BY q.asked_at DESC, q.id DESC LIMIT 1"
        )))
        .bind(ticket_id)
        .fetch_optional(self.store().pool())
        .await?;
        let Some(question) = row.map(HumanQuestion::from) else {
            return Ok(None);
        };
        self.close_question(&question.id, Some(line), AnsweredVia::Terminal)
            .await?;
        Ok(Some(question))
    }

    /// The same for a Project agent's open question: `line` from its terminal or the Ask bar's
    /// Continue.
    pub async fn answer_agent_in_terminal(
        &self,
        agent: &str,
        line: &str,
    ) -> Result<Option<HumanQuestion>, BoardError> {
        let line = line.trim();
        if line.is_empty() {
            return Ok(None);
        }
        let Some(question) = self.open_agent_question(agent).await? else {
            return Ok(None);
        };
        self.close_question(&question.id, Some(line), AnsweredVia::Terminal)
            .await?;
        Ok(Some(question))
    }

    /// Closes an open question with how it was closed and, when there is one, its answer. Whether it was open.
    pub async fn close_question(
        &self,
        question_id: &str,
        answer: Option<&str>,
        via: AnsweredVia,
    ) -> Result<bool, BoardError> {
        let done = sqlx::query(
            "UPDATE human_questions SET answer = ?, answered_at = ?, answered_via = ?
             WHERE id = ? AND answered_at IS NULL",
        )
        .bind(answer)
        .bind(now_ms())
        .bind(via.as_str())
        .bind(question_id)
        .execute(self.store().pool())
        .await?;
        Ok(done.rows_affected() == 1)
    }

    /// The user's Send back on a send-back question: the send back already closed it ([`close_on_send_back`]), and
    /// the user's answer is recorded on it as the answer box's.
    pub(crate) async fn record_send_back_answer(
        &self,
        question_id: &str,
        answer: &str,
    ) -> Result<(), BoardError> {
        sqlx::query(
            "UPDATE human_questions SET answer = ?, answered_via = ?,
                                        answered_at = COALESCE(answered_at, ?)
             WHERE id = ? AND answer IS NULL AND (answered_at IS NULL OR answered_via = ?)",
        )
        .bind(answer)
        .bind(AnsweredVia::Box.as_str())
        .bind(now_ms())
        .bind(question_id)
        .bind(AnsweredVia::Closed.as_str())
        .execute(self.store().pool())
        .await?;
        Ok(())
    }

    /// Stores a question: the options come with their notes, the recommended ones with whether several may be
    /// picked.
    async fn insert_question(
        &self,
        owner_column: &'static str,
        owner: &str,
        text: &str,
        (options, notes): (&[String], &[Option<String>]),
        (recommended, multiple): (&[u32], bool),
        send_back: Option<&SendBackTo>,
    ) -> Result<HumanQuestion, BoardError> {
        validate_options(options, recommended, multiple)?;
        validate_notes(options, notes)?;
        let question = HumanQuestion {
            id: new_id(),
            text: text.trim().to_owned(),
            options: options.iter().map(|o| o.trim().to_owned()).collect(),
            recommended: clean_recommended(recommended),
            multiple,
            notes: clean_notes(notes),
            asked_at: now_ms(),
            send_back: send_back.cloned(),
        };
        let mut tx = self.store().begin_write().await?;
        // A new question replaces the agent's older open one (d0119); a helper's are its own (d0133).
        sqlx::query(sqlx::AssertSqlSafe(format!(
            "UPDATE human_questions SET answered_at = ?, answered_via = ?
             WHERE {owner_column} = ? AND answered_at IS NULL AND helper_id IS NULL"
        )))
        .bind(question.asked_at)
        .bind(AnsweredVia::Replaced.as_str())
        .bind(owner)
        .execute(&mut *tx)
        .await?;
        sqlx::query(sqlx::AssertSqlSafe(format!(
            "INSERT INTO human_questions (id, {owner_column}, question, options_json, recommended, recommended_json,
                                          multiple, option_notes_json, asked_at, send_back_stage_id, send_back_stage)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"
        )))
        .bind(&question.id)
        .bind(owner)
        .bind(&question.text)
        .bind(serde_json::to_string(&question.options).expect("strings serialize"))
        .bind(question.recommended.first().copied().map(i64::from))
        .bind(recommended_json(&question.recommended))
        .bind(question.multiple)
        .bind(notes_json(&question.notes))
        .bind(question.asked_at)
        .bind(send_back.map(|to| to.stage_id.as_str()))
        .bind(send_back.map(|to| to.stage.as_str()))
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(question)
    }

    /// Every unanswered question of a project's tickets, newest first.
    pub(crate) async fn open_questions(
        &self,
        project_id: &str,
    ) -> Result<Vec<OpenQuestion>, BoardError> {
        #[derive(sqlx::FromRow)]
        struct Row {
            ticket_id: String,
            helper_id: Option<String>,
            pass_on_note: Option<String>,
            #[sqlx(flatten)]
            question: QuestionRow,
        }
        let rows: Vec<Row> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
            "SELECT {QUESTION_COLUMNS}, q.ticket_id, q.helper_id, q.pass_on_note FROM {QUESTION_FROM} JOIN tickets t ON t.id = q.ticket_id
             WHERE t.project_id = ? AND q.answered_at IS NULL AND {FOR_THE_USER}
             ORDER BY q.asked_at DESC, q.id DESC"
        )))
        .bind(project_id)
        .fetch_all(self.store().pool())
        .await?;
        Ok(rows
            .into_iter()
            .map(|row| OpenQuestion {
                ticket_id: row.ticket_id,
                question: row.question.into(),
                helper_id: row.helper_id,
                pass_on_note: row.pass_on_note,
            })
            .collect())
    }

    /// Records the user's answer from the answer box (`otaq questions`, M10, reads it back).
    pub async fn record_answer(&self, question_id: &str, answer: &str) -> Result<(), BoardError> {
        self.close_question(question_id, Some(answer), AnsweredVia::Box)
            .await?;
        Ok(())
    }

    /// The ticket's answered `ask human` questions, oldest first: logged in its Messages with the user's answer
    /// (*10 → Answer · inline*: "Logged in SHOP-21 Messages").
    pub async fn answered_human_questions(
        &self,
        ticket_id: &str,
    ) -> Result<Vec<HumanAnswer>, BoardError> {
        let rows: Vec<(String, String, i64, i64)> = sqlx::query_as(
            "SELECT q.question, q.answer, q.asked_at, q.answered_at FROM human_questions q
             WHERE q.ticket_id = ? AND q.answered_at IS NOT NULL AND q.answer IS NOT NULL
               AND (q.helper_id IS NULL OR q.helper_state = 'passed_on')
             ORDER BY q.asked_at, q.id",
        )
        .bind(ticket_id)
        .fetch_all(self.store().pool())
        .await?;
        Ok(rows
            .into_iter()
            .map(|(question, answer, asked_at, answered_at)| HumanAnswer {
                question,
                answer,
                asked_at,
                answered_at,
            })
            .collect())
    }

    /// The ticket's latest unanswered question.
    pub async fn open_question(
        &self,
        ticket_id: &str,
    ) -> Result<Option<HumanQuestion>, BoardError> {
        let row: Option<QuestionRow> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
            "SELECT {QUESTION_COLUMNS} FROM {QUESTION_FROM} WHERE q.ticket_id = ? AND q.answered_at IS NULL
             AND {FOR_THE_USER} ORDER BY q.asked_at DESC, q.id DESC LIMIT 1"
        )))
        .bind(ticket_id)
        .fetch_optional(self.store().pool())
        .await?;
        Ok(row.map(Into::into))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opts(labels: &[&str]) -> Vec<String> {
        labels.iter().map(|l| (*l).to_owned()).collect()
    }

    fn refusal(result: Result<(), BoardError>) -> String {
        let e = result.unwrap_err();
        assert_eq!(e.refusal(), Some(Refusal::BadQuestion));
        e.to_string()
    }

    #[test]
    fn two_to_four_short_options() {
        assert!(validate_options(&[], &[], false).is_ok());
        assert!(validate_options(&opts(&["Postmark", "SES", "Resend"]), &[1], false).is_ok());
        assert!(validate_options(&opts(&["a", "b", "c", "d"]), &[], false).is_ok());
        let wrong = "Give 2–4 options, or none for a free-text question";
        assert_eq!(
            refusal(validate_options(&opts(&["only"]), &[], false)),
            wrong
        );
        assert_eq!(
            refusal(validate_options(
                &opts(&["a", "b", "c", "d", "e"]),
                &[],
                false
            )),
            wrong
        );
        let long = "x".repeat(41);
        assert!(
            refusal(validate_options(&opts(&["a", &long]), &[], false))
                .contains("at most 40 characters")
        );
        assert_eq!(
            refusal(validate_options(&opts(&["a", "b"]), &[3], false)),
            "--recommended must be between 1 and 2"
        );
        assert_eq!(
            refusal(validate_options(&opts(&["a", "b"]), &[0], false)),
            "--recommended must be between 1 and 2"
        );
        assert_eq!(
            refusal(validate_options(&[], &[1], false)),
            "--recommended needs options to choose from"
        );
    }

    /// ATT-17: notes are none, or one per option; never refused for length; stored trimmed, an empty one none.
    #[test]
    fn notes_pair_with_the_options() {
        let two = opts(&["Markdown files", "One ZIP"]);
        assert!(validate_notes(&two, &[]).is_ok());
        assert!(validate_notes(&two, &[None, None]).is_ok());
        assert!(validate_notes(&[], &[None]).is_ok(), "no notes at all");
        let long = "x".repeat(600);
        assert!(validate_notes(&two, &[Some(long.clone()), None]).is_ok());
        assert_eq!(
            validate_notes(&[], &[Some("a".into())])
                .unwrap_err()
                .to_string(),
            "A free-text question takes no --note: a --note describes the --option before it"
        );
        assert_eq!(
            validate_notes(&two, &[Some("a".into())])
                .unwrap_err()
                .to_string(),
            "Give the notes one per option, in the options' order"
        );
        assert_eq!(
            clean_notes(&[Some("  Opens anywhere. \n".into()), Some(" ".into())]),
            [Some("Opens anywhere.".to_owned()), None]
        );
        assert!(clean_notes(&[None, Some(String::new())]).is_empty());
        assert_eq!(
            clean_notes(&[Some(long.clone()), None])[0].as_deref(),
            Some(long.as_str())
        );
        assert_eq!(notes_json(&[None, None]), None);
        assert_eq!(
            notes_json(&[Some("a".into()), None]).as_deref(),
            Some(r#"["a",null]"#)
        );
        assert_eq!(notes_from_json(None), Vec::<Option<String>>::new());
        assert_eq!(
            notes_from_json(Some("[null,null]")),
            Vec::<Option<String>>::new()
        );
        assert_eq!(
            notes_from_json(Some(r#"["a",null]"#)),
            [Some("a".to_owned()), None]
        );
    }

    #[test]
    fn answers_name_the_choice() {
        let q = HumanQuestion {
            id: "q".into(),
            text: "Which email provider?".into(),
            options: opts(&["Postmark", "SES", "Resend"]),
            recommended: vec![1],
            multiple: false,
            notes: vec![Some("One small library.".into()), None, None],
            asked_at: 0,
            send_back: None,
        };
        assert_eq!(
            format_answer(&q, &Choice::Option(1)),
            "Answer to your question: Postmark (option 1)."
        );
        assert_eq!(
            format_answer(&q, &Choice::Other(" Mailgun, we have a contract ".into())),
            "Answer to your question: Mailgun, we have a contract"
        );
        let asks_back = HumanQuestion {
            options: Vec::new(),
            recommended: Vec::new(),
            multiple: false,
            notes: Vec::new(),
            send_back: Some(SendBackTo {
                stage_id: "product-spec".into(),
                stage: "Product spec".into(),
            }),
            ..q
        };
        assert_eq!(
            format_answer(&asks_back, &Choice::SendBack),
            "Sent back to Product spec."
        );
    }

    /// Several answers name every ticked option in the single answer's words, then Other…'s text; one ticked reads as
    /// one answer does.
    #[test]
    fn several_answers_name_each_choice() {
        let q = HumanQuestion {
            id: "q".into(),
            text: "Which email providers?".into(),
            options: opts(&["Postmark", "SES", "Resend"]),
            recommended: vec![1, 3],
            multiple: true,
            notes: Vec::new(),
            asked_at: 0,
            send_back: None,
        };
        let several = |options: &[u32], other: Option<&str>| {
            format_answer(
                &q,
                &Choice::Several {
                    options: options.to_vec(),
                    other: other.map(str::to_owned),
                },
            )
        };
        assert_eq!(
            several(&[1, 3], None),
            "Answer to your question: Postmark (option 1), Resend (option 3)."
        );
        assert_eq!(
            several(&[2], None),
            format_answer(&q, &Choice::Option(2)),
            "one ticked reads as one answer"
        );
        assert_eq!(
            several(&[1, 2], Some(" Mailgun, we have a contract ")),
            "Answer to your question: Postmark (option 1), SES (option 2). Also: Mailgun, we have a contract"
        );
        assert_eq!(
            several(&[], Some("Mailgun, we have a contract")),
            format_answer(&q, &Choice::Other("Mailgun, we have a contract".into())),
            "only Other… reads as Other…"
        );
    }

    /// `--multiple` needs options; several recommendations need `--multiple`; each is among the options.
    #[test]
    fn several_answers_need_options_and_may_recommend_several() {
        let three = opts(&["Postmark", "SES", "Resend"]);
        assert!(validate_options(&three, &[1, 3], true).is_ok());
        assert!(validate_options(&three, &[], true).is_ok());
        assert!(
            validate_options(&three, &[2, 2], false).is_ok(),
            "the same one twice is one"
        );
        assert_eq!(
            refusal(validate_options(&[], &[], true)),
            "--multiple needs options to choose from"
        );
        assert_eq!(
            refusal(validate_options(&three, &[1, 3], false)),
            "Recommend one option, or ask with --multiple to let the user pick several"
        );
        assert_eq!(
            refusal(validate_options(&three, &[1, 4], true)),
            "--recommended must be between 1 and 3"
        );
        assert_eq!(clean_recommended(&[3, 1, 3]), [1, 3]);
        assert_eq!(recommended_json(&[1]), None);
        assert_eq!(recommended_json(&[1, 3]).as_deref(), Some("[1,3]"));
        assert_eq!(recommended_from(Some(2), None), [2]);
        assert_eq!(recommended_from(Some(1), Some("[1,3]")), [1, 3]);
        assert_eq!(recommended_from(None, None), Vec::<u32>::new());
    }

    /// The recommended options in JSON: one number as before, a list when there are several; a question without the
    /// flag reads as one answer.
    #[test]
    fn recommended_reads_one_number_or_a_list() {
        let one: HumanQuestion = serde_json::from_str(
            r#"{"id": "q", "text": "Which?", "options": ["a", "b"], "recommended": 2, "asked_at": 0}"#,
        )
        .unwrap();
        assert_eq!(
            (one.recommended.as_slice(), one.multiple),
            (&[2][..], false)
        );
        assert_eq!(serde_json::to_value(&one).unwrap()["recommended"], 2);
        let several = HumanQuestion {
            recommended: vec![1, 2],
            multiple: true,
            ..one
        };
        let json = serde_json::to_value(&several).unwrap();
        assert_eq!(
            (json["recommended"].clone(), json["multiple"].clone()),
            (serde_json::json!([1, 2]), serde_json::json!(true))
        );
        assert_eq!(
            serde_json::from_value::<HumanQuestion>(json).unwrap(),
            several
        );
        assert_eq!(
            serde_json::to_value(Choice::Several {
                options: vec![1, 3],
                other: Some("x".into())
            })
            .unwrap(),
            serde_json::json!({"several": {"options": [1, 3], "other": "x"}})
        );
    }
}
