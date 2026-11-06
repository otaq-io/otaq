//! The `merged_into` table (migration 0075): which ticket was merged into which other ticket's branch (its host), with
//! the squash commit on the host's branch. A ticket merged into a host waits as [`crate::domain::Status::MergedInto`]
//! until the host lands or is closed; its row says which host that is, and a host's rows list every ticket that went
//! into its branch, oldest first.

use sqlx::{Sqlite, SqlitePool, Transaction};

use super::StoreError;
use crate::domain::TicketKey;

/// One ticket merged into a host.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MergedIn {
    /// The ticket merged in.
    pub ticket_id: String,
    pub key: TicketKey,
    /// The host: the ticket whose branch took it.
    pub into_id: String,
    pub into: TicketKey,
    /// The squash commit on the host's branch.
    pub commit: String,
    pub merged_at: i64,
}

const SELECT: &str = "SELECT m.ticket_id, p.key, t.number, m.into_id, hp.key, h.number, m.commit_sha, \
                      m.merged_at \
                      FROM merged_into m \
                      JOIN tickets t ON t.id = m.ticket_id JOIN projects p ON p.id = t.project_id \
                      JOIN tickets h ON h.id = m.into_id JOIN projects hp ON hp.id = h.project_id";

type Row = (String, String, i64, String, String, i64, String, i64);

fn merged_in(row: Row) -> Option<MergedIn> {
    let (ticket_id, project, number, into_id, into_project, into_number, commit, merged_at) = row;
    Some(MergedIn {
        ticket_id,
        key: crate::board::tickets::key_from_parts(&project, number)?,
        into_id,
        into: crate::board::tickets::key_from_parts(&into_project, into_number)?,
        commit,
        merged_at,
    })
}

async fn select(pool: &SqlitePool, filter: &str, id: &str) -> Result<Vec<MergedIn>, StoreError> {
    let sql = format!("{SELECT} WHERE {filter} ORDER BY m.merged_at, m.rowid");
    let rows: Vec<Row> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(id)
        .fetch_all(pool)
        .await?;
    Ok(rows.into_iter().filter_map(merged_in).collect())
}

/// Records `ticket_id` as merged into `into_id` by `commit`, in the transaction that moves it. A ticket merged into a
/// host again (after the host was closed) replaces its old row.
pub async fn record(
    tx: &mut Transaction<'_, Sqlite>,
    ticket_id: &str,
    into_id: &str,
    commit: &str,
    at: i64,
) -> Result<(), StoreError> {
    sqlx::query(
        "INSERT INTO merged_into (ticket_id, into_id, commit_sha, merged_at) VALUES (?, ?, ?, ?)
         ON CONFLICT (ticket_id) DO UPDATE SET into_id = excluded.into_id, commit_sha = excluded.commit_sha,
                                               merged_at = excluded.merged_at",
    )
    .bind(ticket_id)
    .bind(into_id)
    .bind(commit)
    .bind(at)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

/// The host `ticket_id` was merged into, if it was.
pub async fn of_ticket(pool: &SqlitePool, ticket_id: &str) -> Result<Option<MergedIn>, StoreError> {
    Ok(select(pool, "m.ticket_id = ?", ticket_id).await?.pop())
}

/// Every ticket merged into the host `into_id`, oldest first.
pub async fn of_host(pool: &SqlitePool, into_id: &str) -> Result<Vec<MergedIn>, StoreError> {
    select(pool, "m.into_id = ?", into_id).await
}

/// Every row whose ticket is in the project `project_id` (the board's cards).
pub async fn of_project(pool: &SqlitePool, project_id: &str) -> Result<Vec<MergedIn>, StoreError> {
    select(pool, "t.project_id = ?", project_id).await
}

/// Forgets that `ticket_id` was merged into a host (it went back to review on its own branch).
pub async fn remove(tx: &mut Transaction<'_, Sqlite>, ticket_id: &str) -> Result<(), StoreError> {
    sqlx::query("DELETE FROM merged_into WHERE ticket_id = ?")
        .bind(ticket_id)
        .execute(&mut **tx)
        .await?;
    Ok(())
}
