//! Ticket comments (CMT-01…CMT-04, CMT-14; d0109): the hand-over between stages. Every ticket has them, written by
//! the user and any stage's agent; every later agent reads them all in `prime`. Not the line comments
//! on a diff ([`super::comments`]), which stay as they are.
//!
//! - A comment's text is at most [`COMMENT_LIMIT_CHARS`] Unicode characters; longer text goes in an **attachment**
//!   ([`super::reports`]); a comment may carry any number, of any type (tuq 001, 002). It needs text, or at least one
//!   attachment, or both: a pasted screenshot is sent on its own; an empty comment is refused ([`check_comment`], tuq
//!   007).
//! - `n` numbers a ticket's comments 1, 2, 3…, never reused: a deleted comment keeps its row (History shows what it
//!   said) and no list shows it. Its attachments stay until the ticket is deleted.
//! - Editing a comment changes its text and can remove attachments and add new ones, all in one change (tuq 008,
//!   [`CommentEdit`]). A removed attachment leaves the comment (the thread, `prime`, `otaq comments`) but its row and
//!   file stay, marked `removed_at` (migration 0077): History's edit row names it and still opens it. An added one
//!   takes the next number after every one the comment ever had, so numbers and files are never reused.
//! - Posting, editing and deleting log `comment_posted`, `comment_edited` and `comment_deleted` with the actor.
//! - A comment the user wrote with a new ticket ([`super::tickets::NewTicketInput::first_comment`], RPT-03) is marked
//!   "at creation": its row's `stage_id` is [`AT_CREATION`] (a user's comment has no stage otherwise).
//! - The user's comment can also tell the ticket's running agent or check to read it ([`tell_agent_line`], d0123),
//!   only when the user picks "Tell <agent> now"; the service queues that line as an automatic delivery and records
//!   the agent told on the comment (`told`, migration 0108).
//! - Who may do what (CMT-04) is the policy's ([`crate::policy::Action::Comment`]); a check's checker posts only
//!   through its verdict, which calls [`post_in`] itself.

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::{FromRow, Sqlite, Transaction};

use super::reports::{
    Attachment, AttachmentKind, ReportSource, StoredAttachment, StoredAttachments, check_sources,
    count_label,
};
use super::tickets::{EventKind, record_event};
use super::{Board, BoardError, Refusal};
use crate::agent_tools::Tool;
use crate::caller::Caller;
use crate::domain::{Actor, TicketKey};
use crate::store::{new_id, now_ms};

/// The longest comment, in Unicode characters (CMT-02). The CLI, the app and every hand-over go through
/// [`check_body`].
pub const COMMENT_LIMIT_CHARS: usize = 2_000;

/// The refusal of an empty comment where a file could be attached: no text (or only spaces) and no attachment (tuq
/// 007).
pub const EMPTY_COMMENT: &str = "Write the comment or attach a file first";

/// The refusal of an empty text where nothing can be attached (a helper's note, a drop reason, the New ticket form's
/// line): [`check_body`].
pub const EMPTY_TEXT: &str = "Write the comment first";

/// The `stage_id` of the user's comment posted with a new ticket (RPT-03): no stage id can take it (a stage's id is
/// its name's lowercase letters and digits). Shown as "at creation".
pub const AT_CREATION: &str = "@creation";

/// What a comment posted with its ticket says where its stage would: "You · at creation".
pub const AT_CREATION_LABEL: &str = "at creation";

/// The line typed into the running agent's session when the user posts a comment with "Tell <agent> now" (d0123): "The user added comment 3. Run `otaq prime` to read it." The comment itself is never typed in.
pub fn tell_agent_line(n: u32) -> String {
    format!(
        "The user added comment {n}. Run `{} prime` to read it.",
        crate::naming::COMMAND_NAME
    )
}

/// The session a ticked comment tells (d0123): the check's only while the ticket is checking (its status says so), else
/// the ticket's agent's. A check that has just passed may still be closing while the ticket is in Review; it isn't told.
pub fn told_session_kind(status: crate::domain::Status) -> crate::session::SessionKind {
    use crate::domain::Status;
    use crate::session::SessionKind;
    match status {
        Status::Checking | Status::CheckerNeedsYou => SessionKind::Validator,
        _ => SessionKind::Agent,
    }
}

/// The first comment of a new ticket (RPT-03): the files the New ticket form took, several of any type (tuq 001,
/// 002), with an optional line: a comment needs text or an attachment (tuq 007).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FirstComment {
    pub body: String,
    /// Posted in this order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attachments: Vec<ReportSource>,
}

/// How many characters of a comment History's "commented" line keeps.
const EXCERPT_CHARS: usize = 120;

/// A text that takes no attachments (a helper's note, the New ticket form's line): never empty ([`EMPTY_TEXT`]), and
/// CMT-02's limit as [`check_comment`].
pub fn check_body(body: &str) -> Result<String, BoardError> {
    if body.trim().is_empty() {
        return Err(BoardError::refused(Refusal::EmptyField, EMPTY_TEXT));
    }
    check_comment(body, 0)
}

/// The comment's text as stored (trimmed, maybe empty), or the refusal: no text and none of its `attachments` (tuq
/// 007, [`EMPTY_COMMENT`]), or over the limit with CMT-02's text: "Comment is 48,210 characters; the limit is 2,000. Put the full text
/// in a report with `--report-file` and keep the comment to what the next agent needs."
pub fn check_comment(body: &str, attachments: usize) -> Result<String, BoardError> {
    let body = body.trim();
    if body.is_empty() && attachments == 0 {
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
}

impl CommentAuthor {
    fn kind(&self) -> &'static str {
        match self {
            Self::User { .. } => "user",
            Self::Agent { .. } => "agent",
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
    /// Its attachments, in order (tuq 001): attachment 1 first.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attachments: Vec<Attachment>,
    /// The user posted it with the new ticket (RPT-03): "You · at creation".
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub at_creation: bool,
    pub posted_at: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub edited_at: Option<i64>,
    /// The agent the user's comment was also sent to while it worked ("Strong"), when the user picked "Tell <agent>
    /// now" and an agent was running to get it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub told: Option<String>,
}

impl TicketComment {
    /// Who wrote it, for people and agents: "You", "Rust developer".
    pub fn author_label(&self) -> String {
        match &self.author {
            CommentAuthor::User { .. } => "You".to_owned(),
            CommentAuthor::Agent { name, .. } => name.clone().unwrap_or_else(|| "Agent".to_owned()),
        }
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

    /// The command that prints its attachment `a` (text): `otaq report SHOP-34 4`, with `--attachment 2` when the
    /// comment has several.
    pub fn report_command(&self, key: &TicketKey, a: u32) -> String {
        let pick = match self.attachments.len() {
            0 | 1 => String::new(),
            _ => format!(" --attachment {a}"),
        };
        format!(
            "{} report {key} {}{pick}",
            crate::naming::COMMAND_NAME,
            self.n
        )
    }

    /// One line per attachment under a comment (CMT-07; tuq 002, 006), never its bytes: "Report: test-run.txt · 48 KB,
    /// 2,310 lines · `otaq report SHOP-34 4`" for text; "Image: shot.png · 120 KB · open this path to view it:
    /// `/…/shot.png`" and "File: repro.zip · 2 MB · open this path to view it: `/…/repro.zip`" for the rest.
    pub fn attachment_lines(&self, key: &TicketKey) -> Vec<String> {
        self.attachments
            .iter()
            .map(|a| attachment_line(a, &self.report_command(key, a.n)))
            .collect()
    }
}

/// How agents are told to look at an image or another file: they open its path, never get its bytes (tuq 002, 006).
pub const OPEN_PATH: &str = "open this path to view it";

/// [`TicketComment::attachment_lines`]' line for one attachment; `command` prints a text one.
pub fn attachment_line(a: &Attachment, command: &str) -> String {
    let (label, open) = match a.kind {
        AttachmentKind::Text => ("Report", format!("`{command}`")),
        AttachmentKind::Image => ("Image", format!("{OPEN_PATH}: `{}`", a.path)),
        AttachmentKind::Other => ("File", format!("{OPEN_PATH}: `{}`", a.path)),
    };
    format!(
        "{label}: {} \u{b7} {} \u{b7} {open}",
        a.name,
        a.size_and_lines()
    )
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

/// One attachment of a comment (`otaq report`, the report view): the CLI prints or names its file, the app reads a
/// text one in pages.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReportFile {
    pub key: TicketKey,
    /// Its comment's number.
    pub n: u32,
    /// The attachment, with its full path in the data folder.
    pub attachment: Attachment,
    /// Its comment was deleted; History still opens its attachments (CMT-14).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub deleted: bool,
    /// An edit removed it from its comment; History still opens it (tuq 008).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub removed: bool,
}

/// What an edit changes on a posted comment (tuq 008), applied together: its new text (`None` keeps it), the
/// attachments it removes by their numbers, and the new ones it adds after them. The comment must keep text or an
/// attachment (tuq 007).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommentEdit {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub remove: Vec<u32>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub add: Vec<ReportSource>,
}

impl CommentEdit {
    /// An edit of the text alone.
    pub fn text(body: impl Into<String>) -> Self {
        Self {
            body: Some(body.into()),
            ..Self::default()
        }
    }
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
    posted_at: i64,
    edited_at: Option<i64>,
    deleted_at: Option<i64>,
    told: Option<String>,
}

const COLUMNS: &str = "id, n, author_kind, author_id, author_name, author_tool, stage_id, round, body, posted_at, \
                       edited_at, deleted_at, told";

/// One row of `comment_attachments` (migrations 0076, 0077).
#[derive(FromRow)]
struct AttachmentRow {
    comment_id: String,
    n: i64,
    file: String,
    name: String,
    kind: String,
    bytes: i64,
    lines: Option<i64>,
    removed_at: Option<i64>,
}

const ATTACHMENT_COLUMNS: &str =
    "a.comment_id, a.n, a.file, a.name, a.kind, a.bytes, a.lines, a.removed_at";

impl AttachmentRow {
    fn attachment(self, root: &std::path::Path) -> Attachment {
        Attachment {
            n: u32::try_from(self.n).unwrap_or(0),
            kind: AttachmentKind::parse(&self.kind),
            bytes: u64::try_from(self.bytes).unwrap_or(0),
            lines: self.lines.map(|l| u64::try_from(l).unwrap_or(0)),
            path: root.join(&self.file).display().to_string(),
            name: self.name,
        }
    }
}

/// The attachments of comments, by comment id, each comment's in order.
type AttachmentsById = std::collections::HashMap<String, Vec<Attachment>>;

fn by_comment(rows: Vec<AttachmentRow>, root: &std::path::Path) -> AttachmentsById {
    let mut all = AttachmentsById::new();
    for row in rows {
        all.entry(row.comment_id.clone())
            .or_default()
            .push(row.attachment(root));
    }
    all
}

/// Every attachment the ticket's comments carry now, deleted ones' too; not those an edit removed (tuq 008).
async fn ticket_attachments<'e>(
    executor: impl sqlx::SqliteExecutor<'e>,
    ticket_id: &str,
    root: &std::path::Path,
) -> Result<AttachmentsById, BoardError> {
    let rows: Vec<AttachmentRow> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "SELECT {ATTACHMENT_COLUMNS} FROM comment_attachments a
         JOIN ticket_comments c ON c.id = a.comment_id
         WHERE c.ticket_id = ? AND a.removed_at IS NULL ORDER BY a.comment_id, a.n"
    )))
    .bind(ticket_id)
    .fetch_all(executor)
    .await?;
    Ok(by_comment(rows, root))
}

/// One comment's attachments, in order, each with whether an edit removed it (tuq 008).
async fn comment_attachments<'e>(
    executor: impl sqlx::SqliteExecutor<'e>,
    comment_id: &str,
    root: &std::path::Path,
) -> Result<Vec<(Attachment, bool)>, BoardError> {
    let rows: Vec<AttachmentRow> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "SELECT {ATTACHMENT_COLUMNS} FROM comment_attachments a WHERE a.comment_id = ? ORDER BY a.n"
    )))
    .bind(comment_id)
    .fetch_all(executor)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| {
            let removed = r.removed_at.is_some();
            (r.attachment(root), removed)
        })
        .collect())
}

/// Stores `files` as attachments of the comment `comment_id` (posted, or edited: tuq 008).
async fn insert_attachments(
    tx: &mut Transaction<'_, Sqlite>,
    comment_id: &str,
    files: &[StoredAttachment],
) -> Result<(), BoardError> {
    for file in files {
        sqlx::query(
            "INSERT INTO comment_attachments (comment_id, n, file, name, kind, bytes, lines)
             VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(comment_id)
        .bind(i64::from(file.n))
        .bind(&file.relative)
        .bind(&file.name)
        .bind(file.kind.as_str())
        .bind(i64::try_from(file.bytes).unwrap_or(i64::MAX))
        .bind(file.lines.map(|l| i64::try_from(l).unwrap_or(i64::MAX)))
        .execute(&mut **tx)
        .await?;
    }
    Ok(())
}

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
            _ => CommentAuthor::User {
                name: self.author_name.clone().unwrap_or_default(),
            },
        }
    }

    fn n(&self) -> u32 {
        u32::try_from(self.n).unwrap_or(0)
    }

    /// A user's comment posted with its ticket (RPT-03).
    fn at_creation(&self) -> bool {
        self.author_kind == "user" && self.stage_id.as_deref() == Some(AT_CREATION)
    }

    fn into_comment(
        self,
        stage_name: Option<String>,
        attachments: Vec<Attachment>,
    ) -> TicketComment {
        TicketComment {
            n: self.n(),
            author: self.author(),
            stage_name,
            attachments,
            at_creation: self.at_creation(),
            body: self.body,
            posted_at: self.posted_at,
            edited_at: self.edited_at,
            told: self.told,
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

/// A comment's attachments as its events keep them (History): `[{"n", "name", "kind", "bytes", "lines"?}]`. Events
/// written before migration 0076 have `"report": {"bytes", "lines"} | null` instead.
pub fn attachments_json<'a>(attachments: impl IntoIterator<Item = &'a Attachment>) -> Value {
    Value::Array(
        attachments
            .into_iter()
            .map(|a| {
                let mut v = json!({"n": a.n, "name": a.name, "kind": a.kind, "bytes": a.bytes});
                if let Some(lines) = a.lines {
                    v["lines"] = json!(lines);
                }
                v
            })
            .collect(),
    )
}

fn stored_json(stored: Option<&StoredAttachments>) -> Value {
    let list: Vec<Attachment> = stored
        .map(|s| {
            s.files
                .iter()
                .map(|f| Attachment {
                    n: f.n,
                    name: f.name.clone(),
                    kind: f.kind,
                    bytes: f.bytes,
                    lines: f.lines,
                    path: String::new(),
                })
                .collect()
        })
        .unwrap_or_default();
    attachments_json(&list)
}

/// Posts a comment on the ticket `ticket_id` inside `tx`, with its `comment_posted` event: what the hand-overs call
/// (T-0443), so the comment and the move are one change. `body` must have passed [`check_comment`]; the attachments, if
/// any, were copied with [`Board::prepare_attachments`] and the comment takes their id. On a later failure of `tx` the
/// caller discards them ([`StoredAttachments::discard`]). No rights are checked here: the policy judged the request.
pub async fn post_in(
    tx: &mut Transaction<'_, Sqlite>,
    ticket_id: &str,
    author: &CommentAuthor,
    body: &str,
    attachments: Option<&StoredAttachments>,
    actor: &Actor,
) -> Result<Posted, BoardError> {
    post_comment_in(tx, ticket_id, author, body, attachments, false, actor).await
}

/// [`post_in`], marking a user's comment "at creation" when `at_creation` (the new ticket's first comment, RPT-03).
pub(crate) async fn post_comment_in(
    tx: &mut Transaction<'_, Sqlite>,
    ticket_id: &str,
    author: &CommentAuthor,
    body: &str,
    attachments: Option<&StoredAttachments>,
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
    let id = attachments.map_or_else(new_id, |a| a.comment_id.clone());
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
    };
    sqlx::query(
        "INSERT INTO ticket_comments (id, ticket_id, n, author_kind, author_id, author_name, author_tool, stage_id,
                                      round, body, posted_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
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
    .bind(now_ms())
    .execute(&mut **tx)
    .await?;
    insert_attachments(
        tx,
        &id,
        attachments.map(|a| a.files.as_slice()).unwrap_or_default(),
    )
    .await?;
    let mut detail = json!({
        "n": n,
        "excerpt": excerpt(body),
        "attachments": stored_json(attachments),
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
    /// Posts a comment (the user's or an agent's) with any number of attachments (tuq 001, 006): the text is checked
    /// first (it may be empty when something is attached, tuq 007), then every attachment checked and copied (a missing file or one over 20 MB refused by name before
    /// anything is posted, leaving nothing behind), then the comment stored. Rights are the policy's (CMT-04).
    pub async fn post_ticket_comment(
        &self,
        key: &str,
        author: &CommentAuthor,
        body: &str,
        attachments: Vec<ReportSource>,
        actor: &Actor,
    ) -> Result<TicketComment, BoardError> {
        let body = check_comment(body, attachments.len())?;
        let ticket = self.ticket(key).await?;
        let stored = self.prepare_attachments(&ticket.key, attachments).await?;
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

    /// Changes comment `n`'s text (the limit applies again), logging the before and after. The text may go only while
    /// the comment keeps an attachment (tuq 007). [`Self::edit_ticket_comment_with`] also changes its attachments.
    pub async fn edit_ticket_comment(
        &self,
        key: &str,
        n: u32,
        body: &str,
        actor: &Actor,
    ) -> Result<TicketComment, BoardError> {
        self.edit_ticket_comment_with(key, n, &CommentEdit::text(body), actor)
            .await
    }

    /// Edits comment `n` (tuq 008): its text (the limit applies again), the attachments `edit.remove` names (each one
    /// the comment carries now, refused by number otherwise) and the new ones `edit.add` copies (checked and refused by
    /// name before anything changes), all in one change logged as `comment_edited` with the text before and after and
    /// the attachments removed and added. The comment must keep text or an attachment (tuq 007). A removed attachment
    /// leaves the comment; its row and file stay for History. An edit that changes nothing returns the comment as it
    /// is.
    pub async fn edit_ticket_comment_with(
        &self,
        key: &str,
        n: u32,
        edit: &CommentEdit,
        actor: &Actor,
    ) -> Result<TicketComment, BoardError> {
        // The limit and the new files first, before anything is read or copied.
        if let Some(body) = &edit.body {
            check_comment(body, usize::MAX)?;
        }
        check_sources(&edit.add)?;
        let ticket = self.ticket(key).await?;
        let current = self.ticket_comment(&ticket.key.to_string(), n).await?;
        let mut remove: Vec<u32> = Vec::new();
        for a in &edit.remove {
            if !current.attachments.iter().any(|x| x.n == *a) {
                return Err(BoardError::NotFound(format!(
                    "{}'s comment {n} has no attachment {a}",
                    ticket.key
                )));
            }
            if !remove.contains(a) {
                remove.push(*a);
            }
        }
        if edit.body.is_none() && remove.is_empty() && edit.add.is_empty() {
            return Ok(current);
        }
        let keeps = current.attachments.len() - remove.len() + edit.add.len();
        let body = check_comment(edit.body.as_deref().unwrap_or(&current.body), keeps)?;
        let row = row(self.store.pool(), &ticket.id, n)
            .await?
            .ok_or_else(|| no_comment(&ticket.key, n))?;
        // After every number the comment ever had, removed ones included: never reused.
        let last: Option<i64> =
            sqlx::query_scalar("SELECT MAX(n) FROM comment_attachments WHERE comment_id = ?")
                .bind(&row.id)
                .fetch_one(self.store.pool())
                .await?;
        let first = u32::try_from(last.unwrap_or(0) + 1).unwrap_or(u32::MAX);
        let kept: Vec<String> = current
            .attachments
            .iter()
            .filter(|a| !remove.contains(&a.n))
            .map(|a| a.name.clone())
            .collect();
        let added = self
            .prepare_added_attachments(&ticket.key, &row.id, first, kept, edit.add.clone())
            .await?;
        let removed: Vec<&Attachment> = current
            .attachments
            .iter()
            .filter(|a| remove.contains(&a.n))
            .collect();
        let edited = async {
            let mut tx = self.store.begin_write().await?;
            let row = live_row(&mut tx, &ticket.id, &ticket.key, n).await?;
            let now = now_ms();
            sqlx::query("UPDATE ticket_comments SET body = ?, edited_at = ? WHERE id = ?")
                .bind(&body)
                .bind(now)
                .bind(&row.id)
                .execute(&mut *tx)
                .await?;
            for a in &remove {
                sqlx::query(
                    "UPDATE comment_attachments SET removed_at = ?
                     WHERE comment_id = ? AND n = ? AND removed_at IS NULL",
                )
                .bind(now)
                .bind(&row.id)
                .bind(i64::from(*a))
                .execute(&mut *tx)
                .await?;
            }
            insert_attachments(
                &mut tx,
                &row.id,
                added
                    .as_ref()
                    .map(|a| a.files.as_slice())
                    .unwrap_or_default(),
            )
            .await?;
            let mut detail =
                json!({"n": n, "before": row.body, "after": body, "author": row.author()});
            if !removed.is_empty() {
                detail["removed"] = attachments_json(removed.iter().copied());
            }
            if added.is_some() {
                detail["added"] = stored_json(added.as_ref());
            }
            record_event(&mut tx, &ticket.id, EventKind::CommentEdited, actor, detail).await?;
            tx.commit().await?;
            Ok::<_, BoardError>(())
        }
        .await;
        if let Err(e) = edited {
            if let Some(added) = &added {
                added.discard();
            }
            return Err(e);
        }
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
                "attachments": attachments_json(&deleted.attachments),
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
        let mut attachments =
            ticket_attachments(self.store.pool(), &ticket.id, self.data().root()).await?;
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
                    let files = attachments.remove(&row.id).unwrap_or_default();
                    row.into_comment(name, files)
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

    /// Records that comment `n` also went to `agent`, the agent working the ticket when the user picked "Tell <agent>
    /// now": the thread tags it "· told Strong now".
    pub async fn mark_comment_told(
        &self,
        key: &str,
        n: u32,
        agent: &str,
    ) -> Result<(), BoardError> {
        let ticket = self.ticket(key).await?;
        sqlx::query("UPDATE ticket_comments SET told = ? WHERE ticket_id = ? AND n = ?")
            .bind(agent)
            .bind(&ticket.id)
            .bind(i64::from(n))
            .execute(self.store.pool())
            .await?;
        Ok(())
    }

    /// Where attachment `attachment` (1, 2, 3…) of comment `n` is; with `None`, its only one. A comment with several
    /// refuses `None`, listing them, so the caller names one. A deleted comment's attachments still open (History,
    /// CMT-14), and so does one an edit removed (tuq 008), by its number.
    pub async fn ticket_report(
        &self,
        key: &str,
        n: u32,
        attachment: Option<u32>,
    ) -> Result<ReportFile, BoardError> {
        let ticket = self.ticket(key).await?;
        let row = row(self.store.pool(), &ticket.id, n)
            .await?
            .ok_or_else(|| no_comment(&ticket.key, n))?;
        let mut all = comment_attachments(self.store.pool(), &row.id, self.data().root()).await?;
        // Without a number, the comment's only one now: one an edit removed doesn't count (tuq 008).
        if attachment.is_none() {
            all.retain(|(_, removed)| !removed);
        }
        let picked = match (attachment, all.len()) {
            (_, 0) => {
                return Err(BoardError::NotFound(format!(
                    "{}'s comment {n} has no attachments",
                    ticket.key
                )));
            }
            (None, 1) => all.remove(0),
            (None, _) => {
                let list = all
                    .iter()
                    .map(|(a, _)| format!("{} {}", a.n, a.name))
                    .collect::<Vec<_>>()
                    .join(", ");
                return Err(BoardError::refused(
                    Refusal::BadReport,
                    format!(
                        "{}'s comment {n} has {} attachments; pick one with --attachment: {list}",
                        ticket.key,
                        all.len()
                    ),
                ));
            }
            (Some(a), _) => match all.iter().position(|(x, _)| x.n == a) {
                Some(i) => all.remove(i),
                None => {
                    return Err(BoardError::NotFound(format!(
                        "{}'s comment {n} has no attachment {a}",
                        ticket.key
                    )));
                }
            },
        };
        let (picked, removed) = picked;
        Ok(ReportFile {
            attachment: picked,
            deleted: row.deleted_at.is_some(),
            removed,
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
                                               ORDER BY rowid DESC LIMIT 1)
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
    /// ([`Self::session_author`]). A checker and a Project agent don't post here
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
            Caller::Agent { session, .. }
            | Caller::Validator { session, .. }
            | Caller::Helper { session, .. } => self.session_author(session).await,
            Caller::ProjectAgent { .. } => Err(BoardError::refused(
                Refusal::NotAllowed,
                crate::policy::PROJECT_AGENT_NO_COMMENTS,
            )),
        }
    }

    /// Whether `caller` wrote comment `n` (CMT-04): a ticket's agent when the comment is by the same agent (its
    /// session's). The user owns none and needs none.
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
            _ => false,
        })
    }

    /// The report files of these tickets' comments, deleted ones' and those an edit removed (tuq 008) included: what
    /// Delete removes with them (d0060).
    pub(crate) async fn report_files_of(
        &self,
        ticket_ids: &[&str],
    ) -> Result<Vec<String>, BoardError> {
        let mut files = Vec::new();
        for id in ticket_ids {
            let mine: Vec<String> = sqlx::query_scalar(
                "SELECT a.file FROM comment_attachments a JOIN ticket_comments c ON c.id = a.comment_id
                 WHERE c.ticket_id = ?",
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
    /// d0123: a ticked comment tells the check only while the ticket is checking; else the agent.
    #[test]
    fn the_told_session_follows_the_status() {
        use crate::domain::Status;
        use crate::session::SessionKind;
        assert_eq!(told_session_kind(Status::Checking), SessionKind::Validator);
        assert_eq!(
            told_session_kind(Status::CheckerNeedsYou),
            SessionKind::Validator
        );
        for status in [
            Status::Working,
            Status::WaitingForYou,
            Status::CheckFailed,
            Status::CheckNoResult,
        ] {
            assert_eq!(told_session_kind(status), SessionKind::Agent, "{status:?}");
        }
    }

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
            assert_eq!(refused.to_string(), "Write the comment first");
            let refused = check_comment(body, 0).unwrap_err();
            assert_eq!(refused.refusal(), Some(Refusal::EmptyField));
            assert_eq!(
                refused.to_string(),
                "Write the comment or attach a file first"
            );
            assert_eq!(check_comment(body, 1).unwrap(), "");
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
