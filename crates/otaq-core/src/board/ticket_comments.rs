//! Ticket comments (CMT-01…CMT-04, CMT-14; d0109): the hand-over between stages. Every ticket has them, written by
//! the user, any stage's agent and the epic lead; every later agent reads them all in `prime`. Not the line comments
//! on a diff ([`super::comments`]), which stay as they are.
//!
//! - A comment's text is at most [`COMMENT_LIMIT_CHARS`] Unicode characters and never empty ([`check_body`]); longer
//!   text goes in a **report** ([`super::reports`]), at most one per comment.
//! - `n` numbers a ticket's comments 1, 2, 3…, never reused: a deleted comment keeps its row (History shows what it
//!   said) and no list shows it. Its report stays until the ticket is deleted.
//! - Posting, editing and deleting log `comment_posted`, `comment_edited` and `comment_deleted` with the actor.
//! - A comment the user wrote with a new ticket ([`super::tickets::NewTicketInput::first_comment`], RPT-03) is marked
//!   "at creation": its row's `stage_id` is [`AT_CREATION`] (a user's comment has no stage otherwise).
//! - The user's comment can also tell the ticket's running agent or check to read it ([`tell_agent_line`], d0123);
//!   the service queues that line as an automatic delivery.
//! - Who may do what (CMT-04) is the policy's ([`crate::policy::Action::Comment`]); a check's checker posts only
//!   through its verdict, which calls [`post_in`] itself.

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::{FromRow, Sqlite, Transaction};

use super::reports::{ReportInfo, ReportSource, StoredReport, count_label};
use super::tickets::{EventKind, record_event};
use super::{Board, BoardError, Refusal};
use crate::agent_tools::Tool;
use crate::caller::Caller;
use crate::domain::{Actor, TicketKey};
use crate::store::{new_id, now_ms};

/// The longest comment, in Unicode characters (CMT-02). The CLI, the app and every hand-over go through
/// [`check_body`].
pub const COMMENT_LIMIT_CHARS: usize = 2_000;

/// The refusal of an empty comment (or one of only spaces), with or without a report.
pub const EMPTY_COMMENT: &str = "Write the comment first";

/// The `stage_id` of the user's comment posted with a new ticket (RPT-03): no stage id can take it (a stage's id is
/// its name's lowercase letters and digits). Shown as "at creation".
pub const AT_CREATION: &str = "@creation";

/// What a comment posted with its ticket says where its stage would: "You · at creation".
pub const AT_CREATION_LABEL: &str = "at creation";

/// The line typed into the running agent's session when the user's comment is posted with "Also tell the agent now"
/// (d0123): "The user added comment 3. Run `otaq prime` to read it." The comment itself is never typed in.
pub fn tell_agent_line(n: u32) -> String {
    format!(
        "The user added comment {n}. Run `{} prime` to read it.",
        crate::naming::COMMAND_NAME
    )
}

/// The first comment of a new ticket (RPT-03): the user's line and, usually, the report the New ticket form took.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FirstComment {
    pub body: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub report: Option<ReportSource>,
}

/// How many characters of a comment History's "commented" line keeps.
const EXCERPT_CHARS: usize = 120;

/// The comment's text as stored (trimmed), or the refusal: empty, or over the limit with CMT-02's text: "Comment is
/// 48,210 characters; the limit is 2,000. Put the full text in a report with `--report-file` and keep the comment to
/// what the next agent needs."
pub fn check_body(body: &str) -> Result<String, BoardError> {
    let body = body.trim();
    if body.is_empty() {
        return Err(BoardError::refused(Refusal::EmptyField, EMPTY_COMMENT));
    }
    let chars = body.chars().count();
    if chars > COMMENT_LIMIT_CHARS {
        return Err(BoardError::refused(
            Refusal::TooLong,
            format!(
                "Comment is {} characters; the limit is {}. Put the full text in a report with `--report-file` and \
                 keep the comment to what the next agent needs.",
                count_label(chars as u64),
                count_label(COMMENT_LIMIT_CHARS as u64)
            ),
        ));
    }
    Ok(body.to_owned())
}

/// Who wrote a comment (CMT-01).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CommentAuthor {
    /// The user (the app or their own terminal), by OS user name.
    User { name: String },
    /// A stage's agent: the flow agent's id and name, its tool, and the stage and check round it wrote from.
    Agent {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        name: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        tool: Option<Tool>,
        /// The stage's id.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        stage: Option<String>,
        /// An agent check's round.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        round: Option<u32>,
    },
    /// An epic's lead ("Wishlist lead"): its comments are its epic's lead's, whichever session runs it.
    Lead {
        epic_id: String,
        name: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        tool: Option<Tool>,
    },
}

impl CommentAuthor {
    fn kind(&self) -> &'static str {
        match self {
            Self::User { .. } => "user",
            Self::Agent { .. } => "agent",
            Self::Lead { .. } => "lead",
        }
    }
}

/// One comment as the thread, `prime` and the CLI show it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TicketComment {
    /// Its number on the ticket: `otaq comment edit <N>`, `otaq report <KEY> <N>`.
    pub n: u32,
    pub author: CommentAuthor,
    /// The author's stage by its name in the ticket's flow now ("Validate"); its id when the flow no longer has it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stage_name: Option<String>,
    pub body: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub report: Option<ReportInfo>,
    /// The report's own name, its file's (`test-run-login.md`); `None` for pasted text ("Pasted text", RPT-02).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub report_name: Option<String>,
    /// The user posted it with the new ticket (RPT-03): "You · at creation".
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub at_creation: bool,
    pub posted_at: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub edited_at: Option<i64>,
}

impl TicketComment {
    /// Who wrote it, for people and agents: "You", "Rust developer", "Wishlist lead".
    pub fn author_label(&self) -> String {
        match &self.author {
            CommentAuthor::User { .. } => "You".to_owned(),
            CommentAuthor::Agent { name, .. } => name.clone().unwrap_or_else(|| "Agent".to_owned()),
            CommentAuthor::Lead { name, .. } => name.clone(),
        }
    }

    /// What its report is called (RPT-02): its file's name, "Pasted text" for pasted text; `None` without a report.
    pub fn report_label(&self) -> Option<String> {
        self.report?;
        Some(
            self.report_name
                .clone()
                .unwrap_or_else(|| super::reports::PASTED_TEXT.to_owned()),
        )
    }

    /// Its heading in `prime` and `otaq comments` (CMT-07): "Validator · Validate · round 2 · 3:05 pm", "You · 2:14
    /// pm", "You · at creation · 2:14 pm", local time.
    pub fn heading(&self) -> String {
        let mut parts = vec![self.author_label()];
        parts.extend(self.stage_name.clone());
        if self.at_creation {
            parts.push(AT_CREATION_LABEL.to_owned());
        }
        if let CommentAuthor::Agent {
            round: Some(round), ..
        } = &self.author
        {
            parts.push(format!("round {round}"));
        }
        parts.extend(clock(self.posted_at));
        parts.join(" \u{b7} ")
    }

    /// The line under a comment with a report (CMT-07): "Report: 48 KB, 2,310 lines · `otaq report SHOP-34 4`".
    pub fn report_line(&self, key: &TicketKey) -> Option<String> {
        let report = self.report?;
        Some(format!(
            "Report: {}, {} line{} \u{b7} `{} report {key} {}`",
            super::reports::size_label(report.bytes),
            count_label(report.lines),
            if report.lines == 1 { "" } else { "s" },
            crate::naming::COMMAND_NAME,
            self.n
        ))
    }
}

/// "3:05 pm", "12:40 am", local time (0093's form).
fn clock(at: i64) -> Option<String> {
    let t = crate::local_time::local(at)?;
    let (h, suffix) = match t.hour {
        0 => (12, "am"),
        h @ 1..=11 => (h, "am"),
        12 => (12, "pm"),
        h => (h - 12, "pm"),
    };
    Some(format!("{h}:{:02} {suffix}", t.minute))
}

/// A ticket's comments, oldest first; deleted ones are left out.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TicketComments {
    pub key: TicketKey,
    pub comments: Vec<TicketComment>,
}

/// Where a comment's report is (`otaq report`): the CLI prints or names the file, the app reads it in pages.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReportFile {
    pub key: TicketKey,
    pub n: u32,
    /// The full path in the data folder.
    pub path: String,
    pub info: ReportInfo,
    /// Its comment was deleted; History still opens the report (CMT-14).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub deleted: bool,
}

/// A comment just posted inside a transaction ([`post_in`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Posted {
    pub id: String,
    pub n: u32,
}

#[derive(FromRow)]
struct Row {
    id: String,
    n: i64,
    author_kind: String,
    author_id: Option<String>,
    author_name: Option<String>,
    author_tool: Option<String>,
    stage_id: Option<String>,
    round: Option<i64>,
    body: String,
    report_file: Option<String>,
    report_bytes: Option<i64>,
    report_lines: Option<i64>,
    posted_at: i64,
    edited_at: Option<i64>,
    deleted_at: Option<i64>,
}

const COLUMNS: &str = "id, n, author_kind, author_id, author_name, author_tool, stage_id, round, body, report_file, \
                       report_bytes, report_lines, posted_at, edited_at, deleted_at";

impl Row {
    fn author(&self) -> CommentAuthor {
        let tool = self.author_tool.as_deref().and_then(|t| t.parse().ok());
        match self.author_kind.as_str() {
            "agent" => CommentAuthor::Agent {
                id: self.author_id.clone(),
                name: self.author_name.clone(),
                tool,
                stage: self.stage_id.clone(),
                round: self.round.and_then(|r| u32::try_from(r).ok()),
            },
            "lead" => CommentAuthor::Lead {
                epic_id: self.author_id.clone().unwrap_or_default(),
                name: self.author_name.clone().unwrap_or_default(),
                tool,
            },
            _ => CommentAuthor::User {
                name: self.author_name.clone().unwrap_or_default(),
            },
        }
    }

    fn report(&self) -> Option<ReportInfo> {
        self.report_file.as_ref()?;
        Some(ReportInfo {
            bytes: u64::try_from(self.report_bytes.unwrap_or(0)).unwrap_or(0),
            lines: u64::try_from(self.report_lines.unwrap_or(0)).unwrap_or(0),
        })
    }

    fn n(&self) -> u32 {
        u32::try_from(self.n).unwrap_or(0)
    }

    /// A user's comment posted with its ticket (RPT-03).
    fn at_creation(&self) -> bool {
        self.author_kind == "user" && self.stage_id.as_deref() == Some(AT_CREATION)
    }

    fn into_comment(self, stage_name: Option<String>) -> TicketComment {
        TicketComment {
            n: self.n(),
            author: self.author(),
            stage_name,
            report: self.report(),
            report_name: self
                .report_file
                .as_deref()
                .and_then(|f| super::reports::stored_name(f, &self.id)),
            at_creation: self.at_creation(),
            body: self.body,
            posted_at: self.posted_at,
            edited_at: self.edited_at,
        }
    }
}

/// The start of a comment for History's "commented" line.
fn excerpt(body: &str) -> String {
    if body.chars().count() <= EXCERPT_CHARS {
        return body.to_owned();
    }
    let mut cut: String = body.chars().take(EXCERPT_CHARS).collect();
    cut.push('…');
    cut
}

fn report_json(info: Option<ReportInfo>) -> Value {
    info.map_or(Value::Null, |r| json!({"bytes": r.bytes, "lines": r.lines}))
}

/// Posts a comment on the ticket `ticket_id` inside `tx`, with its `comment_posted` event: what the hand-overs call
/// (T-0443), so the comment and the move are one change. `body` must have passed [`check_body`]; the report, if any,
/// was copied with [`Board::prepare_report`] and the comment takes its id. On a later failure of `tx` the caller
/// discards the report ([`StoredReport::discard`]). No rights are checked here: the policy judged the request.
pub async fn post_in(
    tx: &mut Transaction<'_, Sqlite>,
    ticket_id: &str,
    author: &CommentAuthor,
    body: &str,
    report: Option<&StoredReport>,
    actor: &Actor,
) -> Result<Posted, BoardError> {
    post_comment_in(tx, ticket_id, author, body, report, false, actor).await
}

/// [`post_in`], marking a user's comment "at creation" when `at_creation` (the new ticket's first comment, RPT-03).
pub(crate) async fn post_comment_in(
    tx: &mut Transaction<'_, Sqlite>,
    ticket_id: &str,
    author: &CommentAuthor,
    body: &str,
    report: Option<&StoredReport>,
    at_creation: bool,
    actor: &Actor,
) -> Result<Posted, BoardError> {
    // Never reused: deleted rows stay and count.
    let last: Option<i64> =
        sqlx::query_scalar("SELECT MAX(n) FROM ticket_comments WHERE ticket_id = ?")
            .bind(ticket_id)
            .fetch_one(&mut **tx)
            .await?;
    let n = last.unwrap_or(0) + 1;
    let id = report.map_or_else(new_id, |r| r.comment_id.clone());
    let (author_id, author_name, tool, stage, round) = match author {
        CommentAuthor::User { name } => (
            None,
            Some(name.clone()),
            None,
            at_creation.then(|| AT_CREATION.to_owned()),
            None,
        ),
        CommentAuthor::Agent {
            id,
            name,
            tool,
            stage,
            round,
        } => (id.clone(), name.clone(), *tool, stage.clone(), *round),
        CommentAuthor::Lead {
            epic_id,
            name,
            tool,
        } => (Some(epic_id.clone()), Some(name.clone()), *tool, None, None),
    };
    sqlx::query(
        "INSERT INTO ticket_comments (id, ticket_id, n, author_kind, author_id, author_name, author_tool, stage_id,
                                      round, body, report_file, report_bytes, report_lines, posted_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(ticket_id)
    .bind(n)
    .bind(author.kind())
    .bind(author_id)
    .bind(author_name)
    .bind(tool.map(Tool::as_str))
    .bind(stage)
    .bind(round.map(i64::from))
    .bind(body)
    .bind(report.map(|r| r.relative.clone()))
    .bind(report.map(|r| i64::try_from(r.info.bytes).unwrap_or(i64::MAX)))
    .bind(report.map(|r| i64::try_from(r.info.lines).unwrap_or(i64::MAX)))
    .bind(now_ms())
    .execute(&mut **tx)
    .await?;
    let mut detail = json!({
        "n": n,
        "excerpt": excerpt(body),
        "report": report_json(report.map(|r| r.info)),
        "author": author,
    });
    if at_creation {
        detail["at_creation"] = Value::Bool(true);
    }
    record_event(tx, ticket_id, EventKind::CommentPosted, actor, detail).await?;
    Ok(Posted {
        id,
        n: u32::try_from(n).unwrap_or(0),
    })
}

impl Board {
    /// Posts a comment (the user's, an agent's or a lead's) with an optional report: the text is checked first, then
    /// the report copied (a missing file or one over 20 MB refused before anything is posted), then the comment
    /// stored. Rights are the policy's (CMT-04).
    pub async fn post_ticket_comment(
        &self,
        key: &str,
        author: &CommentAuthor,
        body: &str,
        report: Option<ReportSource>,
        actor: &Actor,
    ) -> Result<TicketComment, BoardError> {
        let body = check_body(body)?;
        let ticket = self.ticket(key).await?;
        let stored = match report {
            Some(source) => {
                let project = self.project(ticket.key.project.as_str()).await?;
                let repo = crate::warm::store::repo_name(&project);
                Some(self.prepare_report(&repo, &ticket.key, source).await?)
            }
            None => None,
        };
        let posted = async {
            let mut tx = self.store.begin_write().await?;
            let posted =
                post_in(&mut tx, &ticket.id, author, &body, stored.as_ref(), actor).await?;
            tx.commit().await?;
            Ok::<_, BoardError>(posted)
        }
        .await;
        let posted = match posted {
            Ok(posted) => posted,
            Err(e) => {
                if let Some(stored) = &stored {
                    stored.discard();
                }
                return Err(e);
            }
        };
        self.ticket_comment(&ticket.key.to_string(), posted.n).await
    }

    /// Changes comment `n`'s text (the limit applies again), logging the before and after.
    pub async fn edit_ticket_comment(
        &self,
        key: &str,
        n: u32,
        body: &str,
        actor: &Actor,
    ) -> Result<TicketComment, BoardError> {
        let body = check_body(body)?;
        let ticket = self.ticket(key).await?;
        let mut tx = self.store.begin_write().await?;
        let row = live_row(&mut tx, &ticket.id, &ticket.key, n).await?;
        sqlx::query("UPDATE ticket_comments SET body = ?, edited_at = ? WHERE id = ?")
            .bind(&body)
            .bind(now_ms())
            .bind(&row.id)
            .execute(&mut *tx)
            .await?;
        record_event(
            &mut tx,
            &ticket.id,
            EventKind::CommentEdited,
            actor,
            json!({"n": n, "before": row.body, "after": body, "author": row.author()}),
        )
        .await?;
        tx.commit().await?;
        self.ticket_comment(&ticket.key.to_string(), n).await
    }

    /// Deletes comment `n`: it leaves the list and `prime`; its row and report stay for History (CMT-14). Returns it
    /// as it was.
    pub async fn delete_ticket_comment(
        &self,
        key: &str,
        n: u32,
        actor: &Actor,
    ) -> Result<TicketComment, BoardError> {
        let ticket = self.ticket(key).await?;
        let deleted = self.ticket_comment(&ticket.key.to_string(), n).await?;
        let mut tx = self.store.begin_write().await?;
        let row = live_row(&mut tx, &ticket.id, &ticket.key, n).await?;
        sqlx::query("UPDATE ticket_comments SET deleted_at = ? WHERE id = ?")
            .bind(now_ms())
            .bind(&row.id)
            .execute(&mut *tx)
            .await?;
        record_event(
            &mut tx,
            &ticket.id,
            EventKind::CommentDeleted,
            actor,
            json!({
                "n": n,
                "body": row.body,
                "report": report_json(row.report()),
                "author": row.author(),
            }),
        )
        .await?;
        tx.commit().await?;
        Ok(deleted)
    }

    /// The ticket's comments, oldest first, without the deleted ones (CMT-01).
    pub async fn ticket_comments(&self, key: &str) -> Result<TicketComments, BoardError> {
        let ticket = self.ticket(key).await?;
        let rows: Vec<Row> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
            "SELECT {COLUMNS} FROM ticket_comments WHERE ticket_id = ? AND deleted_at IS NULL ORDER BY n"
        )))
        .bind(&ticket.id)
        .fetch_all(self.store.pool())
        .await?;
        let flow = self.effective_flow(ticket.key.project.as_str()).await.ok();
        let stage_name = |stage: Option<&String>| {
            stage.map(|id| {
                flow.as_ref()
                    .and_then(|f| f.stage(id))
                    .map_or_else(|| id.clone(), |s| s.name.clone())
            })
        };
        Ok(TicketComments {
            comments: rows
                .into_iter()
                .map(|row| {
                    let name = if row.at_creation() {
                        None
                    } else {
                        stage_name(row.stage_id.as_ref())
                    };
                    row.into_comment(name)
                })
                .collect(),
            key: ticket.key,
        })
    }

    /// Comment `n` of the ticket, if it isn't deleted.
    pub async fn ticket_comment(&self, key: &str, n: u32) -> Result<TicketComment, BoardError> {
        let comments = self.ticket_comments(key).await?;
        let key = comments.key.clone();
        comments
            .comments
            .into_iter()
            .find(|c| c.n == n)
            .ok_or_else(|| no_comment(&key, n))
    }

    /// Where comment `n`'s report is. A deleted comment's report still opens (History, CMT-14).
    pub async fn ticket_report(&self, key: &str, n: u32) -> Result<ReportFile, BoardError> {
        let ticket = self.ticket(key).await?;
        let row = row(self.store.pool(), &ticket.id, n)
            .await?
            .ok_or_else(|| no_comment(&ticket.key, n))?;
        let (Some(file), Some(info)) = (row.report_file.as_deref(), row.report()) else {
            return Err(BoardError::NotFound(format!(
                "{}'s comment {n} has no report",
                ticket.key
            )));
        };
        Ok(ReportFile {
            path: self.report_file(file).display().to_string(),
            info,
            deleted: row.deleted_at.is_some(),
            key: ticket.key,
            n,
        })
    }

    /// Who comment `n` was written by, deleted or not: what the policy compares with the caller (CMT-04).
    pub async fn ticket_comment_author(
        &self,
        key: &str,
        n: u32,
    ) -> Result<CommentAuthor, BoardError> {
        let ticket = self.ticket(key).await?;
        row(self.store.pool(), &ticket.id, n)
            .await?
            .filter(|r| r.deleted_at.is_none())
            .map(|r| r.author())
            .ok_or_else(|| no_comment(&ticket.key, n))
    }

    /// The author a ticket session's comment gets: the agent its session runs, its tool, and the stage and check
    /// round of its latest stage run.
    pub async fn session_author(&self, session: &str) -> Result<CommentAuthor, BoardError> {
        type SessionRow = (
            Option<String>,
            Option<String>,
            Option<String>,
            Option<String>,
            Option<i64>,
        );
        let row: Option<SessionRow> = sqlx::query_as(
            "SELECT s.agent_id, s.agent_name, s.tool, r.stage_id, r.round
             FROM sessions s
             LEFT JOIN stage_runs r ON r.id = (SELECT id FROM stage_runs WHERE session_id = s.id
                                               ORDER BY started_at DESC, rowid DESC LIMIT 1)
             WHERE s.id = ?",
        )
        .bind(session)
        .fetch_optional(self.store.pool())
        .await?;
        let (id, name, tool, stage, round) =
            row.ok_or_else(|| BoardError::NotFound(format!("no session {session}")))?;
        Ok(CommentAuthor::Agent {
            id,
            name,
            tool: tool.and_then(|t| t.parse().ok()),
            stage,
            round: round.and_then(|r| u32::try_from(r).ok()),
        })
    }

    /// The author a caller's new comment gets: the user by name, a ticket's agent by its session
    /// ([`Self::session_author`]), an epic lead as its epic's lead. A checker and a Project agent don't post here
    /// (the policy refused them first).
    pub async fn caller_author(
        &self,
        caller: &Caller,
        actor: &Actor,
    ) -> Result<CommentAuthor, BoardError> {
        match caller {
            Caller::User => Ok(CommentAuthor::User {
                name: actor.to_string(),
            }),
            Caller::Agent { session, .. } | Caller::Validator { session, .. } => {
                self.session_author(session).await
            }
            Caller::EpicLead { epic_id, agent, .. } => {
                let epic = self.ticket_by_id(epic_id).await?;
                let tool: Option<String> =
                    sqlx::query_scalar("SELECT tool FROM project_agents WHERE id = ?")
                        .bind(agent)
                        .fetch_optional(self.store.pool())
                        .await?;
                Ok(CommentAuthor::Lead {
                    epic_id: epic_id.clone(),
                    name: crate::project_agents::lead::lead_name(&epic.title),
                    tool: tool.and_then(|t| t.parse().ok()),
                })
            }
            Caller::ProjectAgent { .. } => Err(BoardError::refused(
                Refusal::NotAllowed,
                crate::policy::PROJECT_AGENT_NO_COMMENTS,
            )),
        }
    }

    /// Whether `caller` wrote comment `n` (CMT-04): a ticket's agent when the comment is by the same agent (its
    /// session's), an epic lead when it is by its epic's lead. The user owns none and needs none.
    pub async fn is_own_comment(
        &self,
        caller: &Caller,
        key: &str,
        n: u32,
    ) -> Result<bool, BoardError> {
        let author = self.ticket_comment_author(key, n).await?;
        Ok(match (caller, &author) {
            (
                Caller::Agent { session, .. } | Caller::Validator { session, .. },
                CommentAuthor::Agent { id: Some(id), .. },
            ) => {
                let mine: Option<Option<String>> =
                    sqlx::query_scalar("SELECT agent_id FROM sessions WHERE id = ?")
                        .bind(session)
                        .fetch_optional(self.store.pool())
                        .await?;
                mine.flatten().as_ref() == Some(id)
            }
            (Caller::EpicLead { epic_id, .. }, CommentAuthor::Lead { epic_id: of, .. }) => {
                epic_id == of
            }
            _ => false,
        })
    }

    /// The report files of these tickets' comments, deleted ones included: what Delete removes with them (d0060).
    pub(crate) async fn report_files_of(
        &self,
        ticket_ids: &[&str],
    ) -> Result<Vec<String>, BoardError> {
        let mut files = Vec::new();
        for id in ticket_ids {
            let mine: Vec<String> = sqlx::query_scalar(
                "SELECT report_file FROM ticket_comments WHERE ticket_id = ? AND report_file IS NOT NULL",
            )
            .bind(id)
            .fetch_all(self.store.pool())
            .await?;
            files.extend(mine);
        }
        Ok(files)
    }
}

fn no_comment(key: &TicketKey, n: u32) -> BoardError {
    BoardError::NotFound(format!("{key} has no comment {n}"))
}

async fn row(pool: &sqlx::SqlitePool, ticket_id: &str, n: u32) -> Result<Option<Row>, BoardError> {
    Ok(sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "SELECT {COLUMNS} FROM ticket_comments WHERE ticket_id = ? AND n = ?"
    )))
    .bind(ticket_id)
    .bind(i64::from(n))
    .fetch_optional(pool)
    .await?)
}

/// Comment `n` inside `tx`, unless it is deleted.
async fn live_row(
    tx: &mut Transaction<'_, Sqlite>,
    ticket_id: &str,
    key: &TicketKey,
    n: u32,
) -> Result<Row, BoardError> {
    let row: Option<Row> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "SELECT {COLUMNS} FROM ticket_comments WHERE ticket_id = ? AND n = ? AND deleted_at IS NULL"
    )))
    .bind(ticket_id)
    .bind(i64::from(n))
    .fetch_optional(&mut **tx)
    .await?;
    row.ok_or_else(|| no_comment(key, n))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_limit_counts_unicode_characters() {
        let at_limit = "ş".repeat(COMMENT_LIMIT_CHARS);
        assert_eq!(check_body(&at_limit).unwrap(), at_limit);
        let emoji = "🦀".repeat(COMMENT_LIMIT_CHARS);
        assert_eq!(check_body(&emoji).unwrap(), emoji);
        let over = "ş".repeat(COMMENT_LIMIT_CHARS + 1);
        let refused = check_body(&over).unwrap_err();
        assert_eq!(refused.refusal(), Some(Refusal::TooLong));
        assert_eq!(
            refused.to_string(),
            "Comment is 2,001 characters; the limit is 2,000. Put the full text in a report with `--report-file` and \
             keep the comment to what the next agent needs."
        );
        assert_eq!(
            check_body(&"x".repeat(48_210)).unwrap_err().to_string(),
            "Comment is 48,210 characters; the limit is 2,000. Put the full text in a report with `--report-file` \
             and keep the comment to what the next agent needs."
        );
    }

    #[test]
    fn empty_and_blank_comments_are_refused() {
        for body in ["", "   ", "\n\t \n"] {
            let refused = check_body(body).unwrap_err();
            assert_eq!(refused.refusal(), Some(Refusal::EmptyField));
            assert_eq!(refused.to_string(), EMPTY_COMMENT);
        }
        // Stored trimmed: the newline stdin ends with isn't part of it.
        assert_eq!(check_body("  Looks good.\n").unwrap(), "Looks good.");
    }

    #[test]
    fn excerpts_keep_the_start() {
        assert_eq!(excerpt("short"), "short");
        let long = "a".repeat(200);
        assert_eq!(excerpt(&long).chars().count(), EXCERPT_CHARS + 1);
        assert!(excerpt(&long).ends_with('…'));
    }
}
