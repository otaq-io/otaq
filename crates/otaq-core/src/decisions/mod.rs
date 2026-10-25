//! Recorded decisions (DEC-01, DEC-02; d0084). An agent that meets a product question the code and documents don't
//! settle, and can go on without the answer, builds one option and records the choice on its own ticket with
//! `otaq decision record --stdin`: the question, what it built, the other options and why. Nothing waits, nothing
//! is written into the user's repo (d0023), and recording changes no state, makes no needs-you item and notifies
//! nobody: it is information, listed in the review (T-0287), `otaq show` and `otaq prime`.
//!
//! Decisions are numbered per ticket from 1; the ID people and agents see is `<KEY>#<n>` (`SHOP-23#1`), built
//! from the ticket's key ([`DecisionId`]). The review keeps or changes them (T-0277).

pub mod outcome;
pub use outcome::{ChangeOutcome, keep_all, open_count};

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::board::tickets::{EventKind, key_from_parts, record_event};
use crate::board::{Board, BoardError, Refusal};
use crate::domain::{Actor, TicketKey, stored_enum};
use crate::store::{new_id, now_ms};

/// The refusal when a field is missing; `missing` lists them ("Why", "Built, Why").
pub fn missing_fields_message(missing: &[&str]) -> String {
    format!(
        "Recorded decisions need Question, Built, at least one Other option, and Why. Missing: {}.",
        missing.join(", ")
    )
}

stored_enum! {
    /// What the review did with a decision (`decisions.outcome`; DEC-05, DEC-06).
    pub enum Outcome ("decision outcome") {
        /// Not reviewed yet.
        Open = "open",
        /// Approve kept it: "Kept by you".
        Kept = "kept",
        /// Change… sent the ticket back with another option: "Changed by you → sent back".
        ChangedSentBack = "changed_sent_back",
    }
}

/// A decision's ID as people see it: the ticket's key and its number on that ticket (`SHOP-23#1`).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DecisionId {
    pub ticket: TicketKey,
    /// From 1, per ticket.
    pub ordinal: u32,
}

impl fmt::Display for DecisionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}#{}", self.ticket, self.ordinal)
    }
}

impl FromStr for DecisionId {
    type Err = BoardError;

    /// `SHOP-23#1`; surrounding spaces are ignored.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let s = s.trim();
        let bad = || {
            BoardError::NotFound(format!(
                "\"{s}\" isn't a decision ID: write it as the ticket's key and the decision's number, like SHOP-23#1"
            ))
        };
        let (key, ordinal) = s.rsplit_once('#').ok_or_else(bad)?;
        let ticket = key.parse().map_err(|_| bad())?;
        let ordinal = ordinal.parse().ok().filter(|n| *n > 0).ok_or_else(bad)?;
        Ok(Self { ticket, ordinal })
    }
}

impl Serialize for DecisionId {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for DecisionId {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = <std::borrow::Cow<'de, str>>::deserialize(d)?;
        s.parse().map_err(serde::de::Error::custom)
    }
}

/// What an agent records (DEC-02): the fields of `otaq decision record --stdin`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct NewDecision {
    #[serde(default)]
    pub question: String,
    #[serde(default)]
    pub built: String,
    /// At least one.
    #[serde(default)]
    pub other_options: Vec<String>,
    #[serde(default)]
    pub why: String,
}

impl NewDecision {
    /// Reads the labelled lines of `decision record --stdin`:
    ///
    /// ```text
    /// Question: Split fullName everywhere, or only in the profile form?
    /// Built: only in the profile form
    /// Other option: everywhere
    /// Why: the description only mentions the profile form
    /// ```
    ///
    /// Labels ignore case; `Other option:` may repeat (`Other options:` is read the same). A line without a label
    /// continues the field above it (joined with a space); blank lines are skipped. Text before the first label is
    /// refused. Missing fields are left empty: [`record`] refuses them with what's missing.
    pub fn parse(text: &str) -> Result<Self, BoardError> {
        let mut out = Self::default();
        let mut current: Option<Field> = None;
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            if let Some((field, rest)) = Field::split(line) {
                if field == Field::OtherOption {
                    out.other_options.push(String::new());
                }
                current = Some(field);
                out.append(field, rest.trim());
                continue;
            }
            let Some(field) = current else {
                return Err(BoardError::refused(
                    Refusal::EmptyField,
                    "Start each field with its label: Question:, Built:, Other option: (one per option) and Why:",
                ));
            };
            out.append(field, line);
        }
        Ok(out)
    }

    fn append(&mut self, field: Field, text: &str) {
        let target = match field {
            Field::Question => &mut self.question,
            Field::Built => &mut self.built,
            Field::Why => &mut self.why,
            Field::OtherOption => self
                .other_options
                .last_mut()
                .expect("an Other option line comes first"),
        };
        if text.is_empty() {
            return;
        }
        if !target.is_empty() {
            target.push(' ');
        }
        target.push_str(text);
    }

    /// The fields that are missing, in the order of the refusal ("Question", "Built", "Other option", "Why").
    pub fn missing(&self) -> Vec<&'static str> {
        let mut missing = Vec::new();
        if self.question.trim().is_empty() {
            missing.push("Question");
        }
        if self.built.trim().is_empty() {
            missing.push("Built");
        }
        if !self.other_options.iter().any(|o| !o.trim().is_empty()) {
            missing.push("Other option");
        }
        if self.why.trim().is_empty() {
            missing.push("Why");
        }
        missing
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Field {
    Question,
    Built,
    OtherOption,
    Why,
}

impl Field {
    /// The field a line starts with, and the text after its label.
    fn split(line: &str) -> Option<(Self, &str)> {
        let (label, rest) = line.split_once(':')?;
        let field = match label.trim().to_ascii_lowercase().as_str() {
            "question" => Self::Question,
            "built" => Self::Built,
            "other option" | "other options" => Self::OtherOption,
            "why" => Self::Why,
            _ => return None,
        };
        Some((field, rest))
    }
}

/// One recorded decision on a ticket.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Decision {
    /// `SHOP-23#1`.
    pub id: DecisionId,
    /// The row's id in the database.
    pub row_id: String,
    pub ticket_id: String,
    pub question: String,
    pub built: String,
    pub other_options: Vec<String>,
    pub why: String,
    pub recorded_by: Actor,
    pub recorded_at: i64,
    pub outcome: Outcome,
    /// The other option or the user's own words a Change… used.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outcome_text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outcome_at: Option<i64>,
}

#[derive(sqlx::FromRow)]
struct DecisionRow {
    id: String,
    ticket_id: String,
    ordinal: i64,
    question: String,
    built: String,
    other_options: String,
    why: String,
    recorded_by: String,
    recorded_at: i64,
    outcome: String,
    outcome_text: Option<String>,
    outcome_at: Option<i64>,
    project_key: String,
    number: i64,
}

const SELECT_DECISIONS: &str = "SELECT d.*, p.key AS project_key, t.number
     FROM decisions d
     JOIN tickets t ON t.id = d.ticket_id
     JOIN projects p ON p.id = t.project_id";

impl DecisionRow {
    fn key(&self) -> Option<TicketKey> {
        key_from_parts(&self.project_key, self.number)
    }

    fn into_decision(self) -> Option<Decision> {
        let ticket = self.key()?;
        Some(Decision {
            id: DecisionId {
                ticket,
                ordinal: u32::try_from(self.ordinal).ok()?,
            },
            row_id: self.id,
            ticket_id: self.ticket_id,
            question: self.question,
            built: self.built,
            other_options: serde_json::from_str(&self.other_options).unwrap_or_default(),
            why: self.why,
            recorded_by: self.recorded_by.parse().expect("infallible"),
            recorded_at: self.recorded_at,
            outcome: self.outcome.parse().unwrap_or(Outcome::Open),
            outcome_text: self.outcome_text,
            outcome_at: self.outcome_at,
        })
    }
}

/// Records a decision on the ticket `key` (DEC-02) with the next number, and its History line "Recorded a
/// decision: …". Who may record is checked before (the caller's own ticket, CLI-02); a missing field is refused
/// with [`missing_fields_message`]. The ticket's state doesn't change.
pub async fn record(
    board: &Board,
    key: &str,
    input: &NewDecision,
    actor: &Actor,
) -> Result<Decision, BoardError> {
    let missing = input.missing();
    if !missing.is_empty() {
        return Err(BoardError::refused(
            Refusal::EmptyField,
            missing_fields_message(&missing),
        ));
    }
    let ticket_id = board.ticket_id(key).await?;
    let question = input.question.trim();
    let built = input.built.trim();
    let why = input.why.trim();
    let others: Vec<&str> = input
        .other_options
        .iter()
        .map(|o| o.trim())
        .filter(|o| !o.is_empty())
        .collect();
    let id = new_id();
    let mut tx = board.store().begin_write().await?;
    let ordinal: i64 = sqlx::query_scalar(
        "SELECT COALESCE(MAX(ordinal), 0) + 1 FROM decisions WHERE ticket_id = ?",
    )
    .bind(&ticket_id)
    .fetch_one(&mut *tx)
    .await?;
    sqlx::query(
        "INSERT INTO decisions (id, ticket_id, ordinal, question, built, other_options, why, recorded_by,
                                recorded_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(&ticket_id)
    .bind(ordinal)
    .bind(question)
    .bind(built)
    .bind(json!(others).to_string())
    .bind(why)
    .bind(actor.to_string())
    .bind(now_ms())
    .execute(&mut *tx)
    .await?;
    record_event(
        &mut tx,
        &ticket_id,
        EventKind::DecisionRecorded,
        actor,
        json!({
            "ordinal": ordinal,
            "question": question,
            "built": built,
            "other_options": others,
            "why": why,
        }),
    )
    .await?;
    tx.commit().await?;
    let decision = by_row_id(board, &id).await?;
    tracing::info!(id = %decision.id, actor = %actor, "decision recorded");
    Ok(decision)
}

async fn by_row_id(board: &Board, id: &str) -> Result<Decision, BoardError> {
    let sql = format!("{SELECT_DECISIONS} WHERE d.id = ?");
    sqlx::query_as::<_, DecisionRow>(sqlx::AssertSqlSafe(sql))
        .bind(id)
        .fetch_optional(board.store().pool())
        .await?
        .and_then(DecisionRow::into_decision)
        .ok_or_else(|| BoardError::NotFound(format!("unknown decision {id}")))
}

/// A ticket's decisions (by key, `SHOP-23`), in the order they were recorded.
pub async fn list(board: &Board, key: &str) -> Result<Vec<Decision>, BoardError> {
    let ticket_id = board.ticket_id(key).await?;
    list_by_ticket_id(board, &ticket_id).await
}

/// A ticket's decisions by the ticket's id, in the order they were recorded.
pub async fn list_by_ticket_id(
    board: &Board,
    ticket_id: &str,
) -> Result<Vec<Decision>, BoardError> {
    let sql = format!("{SELECT_DECISIONS} WHERE d.ticket_id = ? ORDER BY d.ordinal");
    Ok(sqlx::query_as::<_, DecisionRow>(sqlx::AssertSqlSafe(sql))
        .bind(ticket_id)
        .fetch_all(board.store().pool())
        .await?
        .into_iter()
        .filter_map(DecisionRow::into_decision)
        .collect())
}

/// One decision by its ID (`SHOP-23#1`).
pub async fn get(board: &Board, id: &str) -> Result<Decision, BoardError> {
    let parsed: DecisionId = id.parse()?;
    let ticket_id = board.ticket_id(&parsed.ticket.to_string()).await?;
    let sql = format!("{SELECT_DECISIONS} WHERE d.ticket_id = ? AND d.ordinal = ?");
    sqlx::query_as::<_, DecisionRow>(sqlx::AssertSqlSafe(sql))
        .bind(&ticket_id)
        .bind(i64::from(parsed.ordinal))
        .fetch_optional(board.store().pool())
        .await?
        .and_then(DecisionRow::into_decision)
        .ok_or_else(|| BoardError::NotFound(format!("unknown decision {parsed}")))
}

/// The reply to `decision record`: "Recorded SHOP-23#1. Carry on; the user sees it in the review."
pub fn recorded_message(decision: &Decision) -> String {
    format!(
        "Recorded {}. Carry on; the user sees it in the review.",
        decision.id
    )
}

/// The decision as text, for `otaq show` and `otaq prime`: its ID and question, then Built, the other options, Why and
/// the outcome, each on its own line indented by `indent`.
pub fn render(decision: &Decision, indent: &str) -> String {
    let mut out = format!("{} {}\n", decision.id, decision.question);
    out.push_str(&format!("{indent}Built: {}\n", decision.built));
    for other in &decision.other_options {
        out.push_str(&format!("{indent}Other option: {other}\n"));
    }
    out.push_str(&format!("{indent}Why: {}\n", decision.why));
    let outcome = match decision.outcome {
        Outcome::Open => None,
        Outcome::Kept => Some("Kept by you".to_owned()),
        Outcome::ChangedSentBack => Some("Changed by you \u{2192} sent back".to_owned()),
    };
    if let Some(outcome) = outcome {
        out.push_str(&format!("{indent}{outcome}\n"));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_every_field_with_wrapped_lines_and_several_options() {
        let text = "Question: Split fullName everywhere,\n  or only in the profile form?\n\nBuilt: only in the profile \
                    form\nOther option: everywhere\nother options: in the API only\nWhy: the description only \
                    mentions\nthe profile form\n";
        let parsed = NewDecision::parse(text).unwrap();
        assert_eq!(
            parsed,
            NewDecision {
                question: "Split fullName everywhere, or only in the profile form?".into(),
                built: "only in the profile form".into(),
                other_options: vec!["everywhere".into(), "in the API only".into()],
                why: "the description only mentions the profile form".into(),
            }
        );
        assert!(parsed.missing().is_empty());
    }

    #[test]
    fn missing_fields_are_named_in_order() {
        let parsed = NewDecision::parse("Question: Which order?\nBuilt: newest first").unwrap();
        assert_eq!(parsed.missing(), ["Other option", "Why"]);
        assert_eq!(
            missing_fields_message(&["Why"]),
            "Recorded decisions need Question, Built, at least one Other option, and Why. Missing: Why."
        );
        let empty_other =
            NewDecision::parse("Question: q\nBuilt: b\nOther option:\nWhy: w").unwrap();
        assert_eq!(empty_other.missing(), ["Other option"]);
    }

    #[test]
    fn text_before_the_first_label_is_refused() {
        let error = NewDecision::parse("I chose newest first\nQuestion: q").unwrap_err();
        assert_eq!(error.refusal(), Some(Refusal::EmptyField));
    }

    #[test]
    fn decision_ids_parse_and_print() {
        let id: DecisionId = "SHOP-40#2".parse().unwrap();
        assert_eq!(id.ticket.to_string(), "SHOP-40");
        assert_eq!(id.ordinal, 2);
        assert_eq!(id.to_string(), "SHOP-40#2");
        assert_eq!(
            " SHOP-23#1 ".parse::<DecisionId>().unwrap().to_string(),
            "SHOP-23#1"
        );
        for bad in [
            "SHOP-23",
            "SHOP-23#0",
            "SHOP-23#x",
            "#1",
            "shop#1",
            "SHOP-40.1#1",
        ] {
            assert!(bad.parse::<DecisionId>().is_err(), "{bad}");
        }
    }
}
