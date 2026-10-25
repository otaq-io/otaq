//! The `helpers` table (migration 0067): one row per helper of a ticket, by piece.

use sqlx::SqlitePool;

use super::{Helper, HelperStatus};
use crate::domain::{ProjectKey, TicketKey};
use crate::store::{StoreError, new_id, now_ms};

const COLUMNS: &str = "h.id, h.piece, h.title, h.branch, h.worktree, h.session_id, h.status, h.note, \
                       h.held_reason, h.port, h.started_at, h.updated_at, h.merged_at, h.files_json, h.prompt, \
                       h.resume_prompt, \
                       (SELECT q.helper_state FROM human_questions q WHERE q.helper_id = h.id \
                        AND q.answered_at IS NULL ORDER BY q.asked_at DESC, q.id DESC LIMIT 1) AS asked, \
                       p.key, t.number";

#[derive(sqlx::FromRow)]
struct Row {
    id: String,
    piece: i64,
    title: String,
    branch: String,
    worktree: String,
    session_id: Option<String>,
    status: String,
    note: Option<String>,
    held_reason: Option<String>,
    port: Option<i64>,
    started_at: i64,
    updated_at: i64,
    merged_at: Option<i64>,
    files_json: Option<String>,
    prompt: Option<String>,
    resume_prompt: Option<String>,
    asked: Option<String>,
    key: String,
    number: i64,
}

/// A row as a [`Helper`]; `None` for a value no build writes (a row is skipped then, as other stores do).
fn helper(row: Row) -> Option<Helper> {
    Some(Helper {
        ticket: TicketKey::new(
            ProjectKey::new(&row.key).ok()?,
            u32::try_from(row.number).ok()?,
        ),
        id: row.id,
        piece: u32::try_from(row.piece).ok()?,
        title: row.title,
        branch: row.branch,
        worktree: row.worktree,
        session: row.session_id,
        status: row.status.parse().ok()?,
        note: row.note,
        held_reason: row.held_reason,
        port: row.port.and_then(|p| u16::try_from(p).ok()),
        started_at: row.started_at,
        updated_at: row.updated_at,
        merged_at: row.merged_at,
        files: row
            .files_json
            .and_then(|json| serde_json::from_str(&json).ok())
            .unwrap_or_default(),
        prompt: row.prompt,
        resume_prompt: row.resume_prompt,
        asked: row.asked.and_then(|a| a.parse().ok()),
    })
}

fn select(filter: &str) -> String {
    format!(
        "SELECT {COLUMNS} FROM helpers h JOIN tickets t ON t.id = h.ticket_id JOIN projects p ON p.id = t.project_id \
         WHERE {filter} ORDER BY h.piece"
    )
}

/// The ticket's helpers, by piece.
pub async fn of_ticket(pool: &SqlitePool, ticket_id: &str) -> Result<Vec<Helper>, StoreError> {
    let rows: Vec<Row> = sqlx::query_as(sqlx::AssertSqlSafe(select("h.ticket_id = ?")))
        .bind(ticket_id)
        .fetch_all(pool)
        .await?;
    Ok(rows.into_iter().filter_map(helper).collect())
}

/// The ticket's helper for `piece`.
pub async fn of_piece(
    pool: &SqlitePool,
    ticket_id: &str,
    piece: u32,
) -> Result<Option<Helper>, StoreError> {
    let row: Option<Row> = sqlx::query_as(sqlx::AssertSqlSafe(select(
        "h.ticket_id = ? AND h.piece = ?",
    )))
    .bind(ticket_id)
    .bind(i64::from(piece))
    .fetch_optional(pool)
    .await?;
    Ok(row.and_then(helper))
}

/// The helper whose latest session is `session`.
pub async fn of_session(pool: &SqlitePool, session: &str) -> Result<Option<Helper>, StoreError> {
    let row: Option<Row> = sqlx::query_as(sqlx::AssertSqlSafe(select("h.session_id = ?")))
        .bind(session)
        .fetch_optional(pool)
        .await?;
    Ok(row.and_then(helper))
}

/// Every helper not merged yet whose status is `status`, any ticket (the held ones a lifted limit starts).
pub async fn with_status(
    pool: &SqlitePool,
    status: HelperStatus,
) -> Result<Vec<Helper>, StoreError> {
    let rows: Vec<Row> = sqlx::query_as(sqlx::AssertSqlSafe(select("h.status = ?")))
        .bind(status.as_str())
        .fetch_all(pool)
        .await?;
    Ok(rows.into_iter().filter_map(helper).collect())
}

/// The new row of a helper about to start: its piece, title, branch, worktree and port.
pub struct NewHelper<'a> {
    pub ticket_id: &'a str,
    pub piece: u32,
    pub title: &'a str,
    pub branch: &'a str,
    pub worktree: &'a str,
    pub port: Option<u16>,
}

/// Inserts the helper of `piece` (status `held` until its session runs), or updates the title of the one there.
/// Returns its id.
pub async fn upsert(pool: &SqlitePool, new: &NewHelper<'_>) -> Result<String, StoreError> {
    let now = now_ms();
    let id: String = sqlx::query_scalar(
        "INSERT INTO helpers (id, ticket_id, piece, title, branch, worktree, status, port, started_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, 'held', ?, ?, ?)
         ON CONFLICT (ticket_id, piece) DO UPDATE SET title = excluded.title,
             port = COALESCE(helpers.port, excluded.port)
         RETURNING id",
    )
    .bind(new_id())
    .bind(new.ticket_id)
    .bind(i64::from(new.piece))
    .bind(new.title)
    .bind(new.branch)
    .bind(new.worktree)
    .bind(new.port.map(i64::from))
    .bind(now)
    .bind(now)
    .fetch_one(pool)
    .await?;
    Ok(id)
}

/// Sets a helper's status (and its note, its held reason), moving its last activity.
pub async fn set_status(
    pool: &SqlitePool,
    id: &str,
    status: HelperStatus,
    note: Option<&str>,
    held_reason: Option<&str>,
) -> Result<(), StoreError> {
    sqlx::query(
        "UPDATE helpers SET status = ?, note = COALESCE(?, note), held_reason = ?, updated_at = ? WHERE id = ?",
    )
    .bind(status.as_str())
    .bind(note)
    .bind(held_reason)
    .bind(now_ms())
    .bind(id)
    .execute(pool)
    .await?;
    Ok(())
}

/// The helper's latest session (`None`: none yet), named before the session starts.
pub async fn set_session(
    pool: &SqlitePool,
    id: &str,
    session: Option<&str>,
) -> Result<(), StoreError> {
    sqlx::query("UPDATE helpers SET session_id = ?, updated_at = ? WHERE id = ?")
        .bind(session)
        .bind(now_ms())
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

/// A helper's session started: working, with this session.
pub async fn started(
    pool: &SqlitePool,
    id: &str,
    session: &str,
    note: Option<&str>,
) -> Result<(), StoreError> {
    sqlx::query(
        "UPDATE helpers SET status = 'working', session_id = ?, note = COALESCE(?, note), held_reason = NULL,
                            updated_at = ?
         WHERE id = ?",
    )
    .bind(session)
    .bind(note)
    .bind(now_ms())
    .bind(id)
    .execute(pool)
    .await?;
    Ok(())
}

/// A helper's piece was merged: the paths its squash touched; its port goes back to the pool.
pub async fn merged(pool: &SqlitePool, id: &str, files: &[String]) -> Result<(), StoreError> {
    let now = now_ms();
    sqlx::query(
        "UPDATE helpers SET status = 'merged', merged_at = ?, updated_at = ?, files_json = ?, port = NULL
         WHERE id = ?",
    )
    .bind(now)
    .bind(now)
    .bind(serde_json::to_string(files).expect("paths serialize"))
    .bind(id)
    .execute(pool)
    .await?;
    Ok(())
}

/// Frees every helper port of the ticket (its Done).
pub async fn free_ports(pool: &SqlitePool, ticket_id: &str) -> Result<(), StoreError> {
    sqlx::query("UPDATE helpers SET port = NULL WHERE ticket_id = ?")
        .bind(ticket_id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Every helper of a project's tickets, by ticket and piece (the board's cards read their waits and Resume's set).
pub async fn of_project(pool: &SqlitePool, project_id: &str) -> Result<Vec<Helper>, StoreError> {
    let rows: Vec<Row> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "SELECT {COLUMNS} FROM helpers h JOIN tickets t ON t.id = h.ticket_id \
         JOIN projects p ON p.id = t.project_id WHERE t.project_id = ? ORDER BY t.number, h.piece"
    )))
    .bind(project_id)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().filter_map(helper).collect())
}

/// The helper's tool showed a permission prompt (d0133 audit 3): open until the helper moves on or its session ends.
pub async fn set_prompt(pool: &SqlitePool, id: &str, prompt: &str) -> Result<(), StoreError> {
    sqlx::query(
        "UPDATE helpers SET prompt = ?, prompt_at = COALESCE(CASE WHEN prompt = ? THEN prompt_at END, ?) WHERE id = ?",
    )
    .bind(prompt)
    .bind(prompt)
    .bind(now_ms())
    .bind(id)
    .execute(pool)
    .await?;
    Ok(())
}

/// The helper's prompt was answered (or its session ended): the prompt it had, if one was open.
pub async fn clear_prompt(pool: &SqlitePool, id: &str) -> Result<Option<String>, StoreError> {
    let prompt: Option<Option<String>> =
        sqlx::query_scalar("SELECT prompt FROM helpers WHERE id = ?")
            .bind(id)
            .fetch_optional(pool)
            .await?;
    let Some(prompt) = prompt.flatten() else {
        return Ok(None);
    };
    let cleared = sqlx::query(
        "UPDATE helpers SET prompt = NULL, prompt_at = NULL WHERE id = ? AND prompt IS NOT NULL",
    )
    .bind(id)
    .execute(pool)
    .await?;
    Ok((cleared.rows_affected() == 1).then_some(prompt))
}

/// Every open prompt of the ticket's helpers closes (their sessions stopped).
pub async fn clear_prompts(pool: &SqlitePool, ticket_id: &str) -> Result<(), StoreError> {
    sqlx::query("UPDATE helpers SET prompt = NULL, prompt_at = NULL WHERE ticket_id = ?")
        .bind(ticket_id)
        .execute(pool)
        .await?;
    Ok(())
}

/// The ticket's helpers with an open prompt, the longest open first (the one the ticket shows).
pub async fn open_prompts(pool: &SqlitePool, ticket_id: &str) -> Result<Vec<Helper>, StoreError> {
    let rows: Vec<Row> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "SELECT {COLUMNS} FROM helpers h JOIN tickets t ON t.id = h.ticket_id JOIN projects p ON p.id = t.project_id \
         WHERE h.ticket_id = ? AND h.prompt IS NOT NULL ORDER BY h.prompt_at, h.piece"
    )))
    .bind(ticket_id)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().filter_map(helper).collect())
}

/// What the helper is told first when it resumes (`None` clears it).
pub async fn set_resume_prompt(
    pool: &SqlitePool,
    id: &str,
    prompt: Option<&str>,
) -> Result<(), StoreError> {
    sqlx::query("UPDATE helpers SET resume_prompt = ? WHERE id = ?")
        .bind(prompt)
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

/// The helper `id`.
pub async fn by_id(pool: &SqlitePool, id: &str) -> Result<Option<Helper>, StoreError> {
    let row: Option<Row> = sqlx::query_as(sqlx::AssertSqlSafe(select("h.id = ?")))
        .bind(id)
        .fetch_optional(pool)
        .await?;
    Ok(row.and_then(helper))
}

/// A helper's conversation resumed in `session` (d0133 audit 5): its status stays (working or sent back), and what
/// waited to be told first is gone.
pub async fn resumed(pool: &SqlitePool, id: &str, session: &str) -> Result<(), StoreError> {
    sqlx::query(
        "UPDATE helpers SET session_id = ?, resume_prompt = NULL, prompt = NULL, prompt_at = NULL, updated_at = ?
         WHERE id = ?",
    )
    .bind(session)
    .bind(now_ms())
    .bind(id)
    .execute(pool)
    .await?;
    Ok(())
}
