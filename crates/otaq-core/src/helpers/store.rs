//! The `helpers` table (migration 0067): one row per helper of a ticket, by piece.

use sqlx::SqlitePool;

use super::{Helper, HelperStatus};
use crate::domain::{ProjectKey, TicketKey};
use crate::store::{StoreError, new_id, now_ms};

const COLUMNS: &str = "h.id, h.piece, h.title, h.branch, h.worktree, h.session_id, h.status, h.note, \
                       h.held_reason, h.port, h.started_at, h.updated_at, h.merged_at, h.files_json, \
                       p.key, t.number";

type Row = (
    String,
    i64,
    String,
    String,
    String,
    Option<String>,
    String,
    Option<String>,
    Option<String>,
    Option<i64>,
    i64,
    i64,
    Option<i64>,
    Option<String>,
    String,
    i64,
);

/// A row as a [`Helper`]; `None` for a value no build writes (a row is skipped then, as other stores do).
fn helper(row: Row) -> Option<Helper> {
    let (
        id,
        piece,
        title,
        branch,
        worktree,
        session,
        status,
        note,
        held_reason,
        port,
        started_at,
        updated_at,
        merged_at,
        files,
        project,
        number,
    ) = row;
    Some(Helper {
        ticket: TicketKey::new(ProjectKey::new(&project).ok()?, u32::try_from(number).ok()?),
        id,
        piece: u32::try_from(piece).ok()?,
        title,
        branch,
        worktree,
        session,
        status: status.parse().ok()?,
        note,
        held_reason,
        port: port.and_then(|p| u16::try_from(p).ok()),
        started_at,
        updated_at,
        merged_at,
        files: files
            .and_then(|json| serde_json::from_str(&json).ok())
            .unwrap_or_default(),
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
