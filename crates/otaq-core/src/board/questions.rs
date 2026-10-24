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
//! **An open question holds Needs you until it is closed** (d0119). A question closes ([`AnsweredVia`]) when the user
//! answers it from the answer box, presses Enter on a line in that agent's terminal (or the Ask bar's Continue to a
//! Project agent or lead), when the same agent asks a new question (the older one is replaced), or when its ticket is
//! closed, parked or delivered (a trigger, migration 0058). One agent has at most one open question.

use serde::{Deserialize, Serialize};

use super::{Board, BoardError, Refusal};
use crate::domain::stored_enum;
use crate::store::{new_id, now_ms};

stored_enum! {
    /// How a question was closed (`human_questions.answered_via`, d0119). A question its epic's lead answered has
    /// none: the lead's answer is in `lead_state` (LEAD-06).
    pub enum AnsweredVia ("answer source") {
        /// The user's answer from the strip, card or sheet (d0061).
        Box = "box",
        /// The line the user sent with Enter in that agent's terminal, or the Ask bar's Continue.
        Terminal = "terminal",
        /// The same agent asked a new question.
        Replaced = "replaced",
        /// Its ticket was closed, parked or delivered.
        Closed = "closed",
    }
}

/// The most options a question may carry.
pub const MAX_OPTIONS: usize = 4;
/// The longest option label, in characters: labels are short so they fit the inbox, card and panel.
pub const MAX_OPTION_LABEL: usize = 40;

/// A question an agent asked the user.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HumanQuestion {
    pub id: String,
    pub text: String,
    /// The answer options, in order; empty for a free-text question.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub options: Vec<String>,
    /// The recommended option, 1-based.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recommended: Option<u32>,
    /// Each option's note (ATT-17), in the options' order, `None` for an option without one; empty when no option
    /// has one.
    #[serde(default, skip_serializing_if = "no_notes")]
    pub notes: Vec<Option<String>>,
    pub asked_at: i64,
    /// Passed on by the epic's lead (LEAD-06): `recommended` is the lead's choice, and this names the lead and
    /// its reason ("SHOP-40.2 asks · passed on by Wishlist lead").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub via_lead: Option<ViaLead>,
}

/// The epic's lead that passed a child's question on to the user, with its reason for the recommended option
/// (LEAD-06, d0083).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ViaLead {
    /// "Wishlist lead".
    pub lead: String,
    /// "PAY-123 only mentions signed-in users."
    pub reason: String,
}

/// An unanswered question of a ticket, as the board reads it for the cards.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct OpenQuestion {
    pub ticket_id: String,
    pub question: HumanQuestion,
    /// The lead's name while the question waits for it ("asked lead · waiting", LEAD-06).
    pub with_lead: Option<String>,
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
    /// "Other…": free text.
    Other(String),
}

/// Checks the options of a question: none, or 2–4 short labels, and a recommendation among them.
pub fn validate_options(options: &[String], recommended: Option<u32>) -> Result<(), BoardError> {
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
    match recommended {
        Some(_) if options.is_empty() => {
            refuse("--recommended needs options to choose from".to_owned())
        }
        Some(n) if n == 0 || n as usize > options.len() => refuse(format!(
            "--recommended must be between 1 and {}",
            options.len()
        )),
        _ => Ok(()),
    }
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
/// "Answer to your question: <text>" for "Other…".
pub fn format_answer(question: &HumanQuestion, choice: &Choice) -> String {
    match choice {
        Choice::Option(n) => {
            let label = question
                .options
                .get((*n as usize).wrapping_sub(1))
                .map_or("?", String::as_str);
            format!("Answer to your question: {label} (option {n}).")
        }
        Choice::Other(text) => format!("Answer to your question: {}", text.trim()),
    }
}

#[derive(sqlx::FromRow)]
struct QuestionRow {
    id: String,
    question: String,
    options_json: String,
    recommended: Option<i64>,
    option_notes_json: Option<String>,
    asked_at: i64,
    lead_state: Option<String>,
    lead_reason: Option<String>,
    /// The title of the epic it was routed through: the lead's name.
    epic_title: Option<String>,
}

/// The columns of [`QuestionRow`], and where they come from (`human_questions q` and the epic `e` it was routed
/// through).
const QUESTION_COLUMNS: &str = "q.id, q.question, q.options_json, q.recommended, q.option_notes_json, q.asked_at, \
     q.lead_state, q.lead_reason, e.title AS epic_title";
const QUESTION_FROM: &str = "human_questions q LEFT JOIN tickets e ON e.id = q.lead_epic_id";

impl QuestionRow {
    fn lead_name(&self) -> Option<String> {
        self.epic_title
            .as_deref()
            .map(crate::project_agents::lead::lead_name)
    }
}

impl From<QuestionRow> for HumanQuestion {
    fn from(row: QuestionRow) -> Self {
        let via_lead = match (row.lead_state.as_deref(), row.lead_name()) {
            (Some(crate::epic_lead::questions::PASSED_ON), Some(lead)) => Some(ViaLead {
                lead,
                reason: row.lead_reason.clone().unwrap_or_default(),
            }),
            _ => None,
        };
        Self {
            id: row.id,
            text: row.question,
            options: serde_json::from_str(&row.options_json).unwrap_or_default(),
            recommended: row.recommended.and_then(|n| u32::try_from(n).ok()),
            notes: notes_from_json(row.option_notes_json.as_deref()),
            asked_at: row.asked_at,
            via_lead,
        }
    }
}

impl Board {
    /// Stores a question of `ticket_id` (options already validated).
    pub async fn store_question(
        &self,
        ticket_id: &str,
        text: &str,
        options: &[String],
        notes: &[Option<String>],
        recommended: Option<u32>,
    ) -> Result<HumanQuestion, BoardError> {
        self.insert_question("ticket_id", ticket_id, text, options, notes, recommended)
            .await
    }

    /// Stores a question of the Project agent `agent` (`ask human` from it, d0068).
    pub async fn store_agent_question(
        &self,
        agent: &str,
        text: &str,
        options: &[String],
        notes: &[Option<String>],
        recommended: Option<u32>,
    ) -> Result<HumanQuestion, BoardError> {
        self.insert_question("project_agent_id", agent, text, options, notes, recommended)
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

    /// The question the ticket's agent holds for the user (d0119): its open question, unless it waits for the epic's
    /// lead ("asked lead · waiting", LEAD-06).
    pub async fn open_user_question(
        &self,
        ticket_id: &str,
    ) -> Result<Option<HumanQuestion>, BoardError> {
        let row: Option<QuestionRow> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
            "SELECT {QUESTION_COLUMNS} FROM {QUESTION_FROM} WHERE q.ticket_id = ? AND q.answered_at IS NULL
               AND (q.lead_state IS NULL OR q.lead_state <> ?)
             ORDER BY q.asked_at DESC, q.id DESC LIMIT 1"
        )))
        .bind(ticket_id)
        .bind(crate::epic_lead::questions::WITH_LEAD)
        .fetch_optional(self.store().pool())
        .await?;
        Ok(row.map(Into::into))
    }

    /// The user sent `line` in the ticket's agent terminal while its question was open (d0119): the line is its
    /// answer. Returns the question it answered; `None` (nothing closed) for an empty line or no open question.
    pub async fn answer_in_terminal(
        &self,
        ticket_id: &str,
        line: &str,
    ) -> Result<Option<HumanQuestion>, BoardError> {
        let line = line.trim();
        if line.is_empty() {
            return Ok(None);
        }
        let Some(question) = self.open_user_question(ticket_id).await? else {
            return Ok(None);
        };
        self.close_question(&question.id, Some(line), AnsweredVia::Terminal)
            .await?;
        Ok(Some(question))
    }

    /// The same for a Project agent's or an epic lead's open question: `line` from its terminal or the Ask bar's
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

    async fn insert_question(
        &self,
        owner_column: &'static str,
        owner: &str,
        text: &str,
        options: &[String],
        notes: &[Option<String>],
        recommended: Option<u32>,
    ) -> Result<HumanQuestion, BoardError> {
        validate_options(options, recommended)?;
        validate_notes(options, notes)?;
        let question = HumanQuestion {
            id: new_id(),
            text: text.trim().to_owned(),
            options: options.iter().map(|o| o.trim().to_owned()).collect(),
            recommended,
            notes: clean_notes(notes),
            asked_at: now_ms(),
            via_lead: None,
        };
        let mut tx = self.store().begin_write().await?;
        // A new question replaces the agent's older open one (d0119).
        sqlx::query(sqlx::AssertSqlSafe(format!(
            "UPDATE human_questions SET answered_at = ?, answered_via = ?
             WHERE {owner_column} = ? AND answered_at IS NULL"
        )))
        .bind(question.asked_at)
        .bind(AnsweredVia::Replaced.as_str())
        .bind(owner)
        .execute(&mut *tx)
        .await?;
        sqlx::query(sqlx::AssertSqlSafe(format!(
            "INSERT INTO human_questions (id, {owner_column}, question, options_json, recommended, option_notes_json,
                                          asked_at)
             VALUES (?, ?, ?, ?, ?, ?, ?)"
        )))
        .bind(&question.id)
        .bind(owner)
        .bind(&question.text)
        .bind(serde_json::to_string(&question.options).expect("strings serialize"))
        .bind(question.recommended.map(i64::from))
        .bind(notes_json(&question.notes))
        .bind(question.asked_at)
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
            #[sqlx(flatten)]
            question: QuestionRow,
        }
        let rows: Vec<Row> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
            "SELECT {QUESTION_COLUMNS}, q.ticket_id FROM {QUESTION_FROM} JOIN tickets t ON t.id = q.ticket_id
             WHERE t.project_id = ? AND q.answered_at IS NULL ORDER BY q.asked_at DESC, q.id DESC"
        )))
        .bind(project_id)
        .fetch_all(self.store().pool())
        .await?;
        Ok(rows
            .into_iter()
            .map(|row| {
                let with_lead = (row.question.lead_state.as_deref()
                    == Some(crate::epic_lead::questions::WITH_LEAD))
                .then(|| row.question.lead_name())
                .flatten();
                OpenQuestion {
                    ticket_id: row.ticket_id,
                    question: row.question.into(),
                    with_lead,
                }
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
    /// (*10 → Answer · inline*: "Logged in SHOP-21 Messages"). A question its epic's lead answered isn't the
    /// user's: it is in the lead's log ([`crate::epic_lead::questions::log_of`]).
    pub async fn answered_human_questions(
        &self,
        ticket_id: &str,
    ) -> Result<Vec<HumanAnswer>, BoardError> {
        let rows: Vec<(String, String, i64, i64)> = sqlx::query_as(
            "SELECT question, answer, asked_at, answered_at FROM human_questions
             WHERE ticket_id = ? AND answered_at IS NOT NULL AND answer IS NOT NULL
               AND (lead_state IS NULL OR lead_state <> 'answered') ORDER BY asked_at, id",
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
             ORDER BY q.asked_at DESC, q.id DESC LIMIT 1"
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
        assert!(validate_options(&[], None).is_ok());
        assert!(validate_options(&opts(&["Postmark", "SES", "Resend"]), Some(1)).is_ok());
        assert!(validate_options(&opts(&["a", "b", "c", "d"]), None).is_ok());
        let wrong = "Give 2–4 options, or none for a free-text question";
        assert_eq!(refusal(validate_options(&opts(&["only"]), None)), wrong);
        assert_eq!(
            refusal(validate_options(&opts(&["a", "b", "c", "d", "e"]), None)),
            wrong
        );
        let long = "x".repeat(41);
        assert!(
            refusal(validate_options(&opts(&["a", &long]), None)).contains("at most 40 characters")
        );
        assert_eq!(
            refusal(validate_options(&opts(&["a", "b"]), Some(3))),
            "--recommended must be between 1 and 2"
        );
        assert_eq!(
            refusal(validate_options(&opts(&["a", "b"]), Some(0))),
            "--recommended must be between 1 and 2"
        );
        assert_eq!(
            refusal(validate_options(&[], Some(1))),
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
            recommended: Some(1),
            notes: vec![Some("One small library.".into()), None, None],
            asked_at: 0,
            via_lead: None,
        };
        assert_eq!(
            format_answer(&q, &Choice::Option(1)),
            "Answer to your question: Postmark (option 1)."
        );
        assert_eq!(
            format_answer(&q, &Choice::Other(" Mailgun, we have a contract ".into())),
            "Answer to your question: Mailgun, we have a contract"
        );
    }
}
