//! Thin access to the `tickets` table. Key allocation, fields and events come in M2 (T-0022).

use sqlx::FromRow;

use super::{Store, StoreError, new_id, now_ms};
use crate::domain::{BuildState, ResourceState};

/// A full `tickets` row.
#[derive(Debug, Clone, PartialEq, Eq, FromRow)]
pub struct TicketRow {
    pub id: String,
    pub project_id: String,
    pub number: i64,
    pub epic_id: Option<String>,
    pub child_number: i64,
    pub next_child_number: i64,
    pub kind: String,
    pub title: String,
    pub body: String,
    /// 1–4 (d0065); `NULL` for an epic child that follows its epic (migration 0023).
    pub priority: Option<i64>,
    pub external_id: Option<String>,
    pub state: String,
    pub outcome: Option<String>,
    pub pinned_at: Option<i64>,
    pub parked: bool,
    pub base_branch: Option<String>,
    pub branch_name: Option<String>,
    pub branch_locked: bool,
    pub base_sha: Option<String>,
    pub review_head_sha: Option<String>,
    pub review_entered_at: Option<i64>,
    pub worktree_path: Option<String>,
    pub agent_port: Option<i64>,
    pub run_port: Option<i64>,
    pub pr_url: Option<String>,
    pub pr_number: Option<i64>,
    pub sent_back_round: i64,
    pub created_by: String,
    pub updated_by: Option<String>,
    pub started_at: Option<i64>,
    pub created_at: i64,
    pub updated_at: i64,
    pub start_problem_step: Option<String>,
    pub start_problem_message: Option<String>,
    pub signal_text: Option<String>,
    pub signal_since: Option<i64>,
    pub wakeup_at: Option<i64>,
    pub last_signal_at: Option<i64>,
    pub state_since: Option<i64>,
    pub queued_at: Option<i64>,
    pub merge_commit_sha: Option<String>,
    pub sent_back_at: Option<i64>,
    pub resume_skill: Option<String>,
    pub conflicts_with: Option<String>,
    pub conflict_files: Option<String>,
    pub conflict_reason: Option<String>,
    pub worktree_kept: bool,
    pub reopened: bool,
    pub interrupted_from: Option<String>,
    pub return_mode: Option<String>,
    pub resume_prompt: Option<String>,
    /// GIT-07: the branch was pushed; its name is locked (migration 0018).
    pub branch_pushed: bool,
    /// T-0145: the pushed content differs from the reviewed one (JSON, migration 0019).
    pub pushed_differs: Option<String>,
    /// T-0150: back from Approved for PR rework, not submitted again yet (migration 0020).
    pub pr_rework: bool,
    /// T-0178: when this queued ticket became ready to start (migration 0024); set by the scheduler.
    pub ready_since: Option<i64>,
    /// T-0182 (migration 0025): how it reached Review, Auto-merge (the rounds are per check from migration 0051).
    pub review_entry: Option<String>,
    pub auto_merge_conflicts: i64,
    pub auto_merge_stop: Option<String>,
    pub merged_automatically: bool,
    /// T-0411 (migration 0049): the reason of an agent's `stage done` close (Closed · nothing to merge).
    pub nothing_to_merge: Option<String>,
    /// T-0419 (migration 0053): an automatic Delivery delivered it with no check passed before it.
    pub delivered_without_checks: bool,
    /// T-0262 (migration 0030): an epic's queue mode, `none` / `whole` / `single` (d0080), and when it was set.
    pub epic_queue_mode: String,
    pub epic_queue_mode_at: Option<i64>,
    /// T-0262 (migration 0030): a child that joined a whole-queued epic's queue: who added it (an actor).
    pub joined_queue_by: Option<String>,
    /// T-0304 (migration 0041): where the build output is (`present` / `freed` / `fetching`, d0085, d0086), and
    /// why the last Free failed.
    pub build_state: String,
    pub build_problem: Option<String>,
    /// T-0304 (migration 0041): the ticket resource Otaq created (d0087): its name, state and last problem.
    pub resource_name: Option<String>,
    pub resource_state: String,
    pub resource_problem: Option<String>,
    /// T-0304 (migration 0041): JSON `[{other, path}]`, Review tickets adding files under the same schema path.
    pub schema_overlap: Option<String>,
    /// T-0400 (migration 0046): the flow stage the ticket is in, or the Agent stage it waits before; `NULL` in
    /// Backlog, Ready for Dev and Done (STG-04).
    pub stage_id: Option<String>,
    /// T-0491 (migration 0059): the next Local merge runs the Merge check even when the branch is current: an
    /// earlier one rebased it onto new commits and no check passed on them (d0120).
    pub merge_check_due: bool,
}

/// The SQL that keeps `tickets.state_since` (when the ticket entered its status) in an `UPDATE tickets SET …`
/// that writes `state`: kept when the state stays the same, else now. Binds: the new state, then now. Expands to
/// a literal, for `concat!`.
macro_rules! set_state_since {
    () => {
        "state_since = CASE WHEN state = ? THEN state_since ELSE ? END"
    };
}
pub(crate) use set_state_since;

/// The fields needed to insert a ticket. The caller picks the number (allocation is T-0022's job).
#[derive(Debug, Clone)]
pub struct NewTicket {
    pub project_id: String,
    pub number: i64,
    pub epic_id: Option<String>,
    pub child_number: i64,
    pub kind: String,
    pub title: String,
    pub body: String,
    /// The stage it is in (`None` in Backlog, Ready for Dev and Done).
    pub stage_id: Option<String>,
    pub state: String,
    pub created_by: String,
}

impl Store {
    pub async fn insert_ticket(&self, new: &NewTicket) -> Result<TicketRow, StoreError> {
        let id = new_id();
        let now = now_ms();
        sqlx::query(
            "INSERT INTO tickets (id, project_id, number, epic_id, child_number, kind, title, body,
                                  stage_id, state, created_by, created_at, updated_at, state_since)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(&new.project_id)
        .bind(new.number)
        .bind(&new.epic_id)
        .bind(new.child_number)
        .bind(&new.kind)
        .bind(&new.title)
        .bind(&new.body)
        .bind(&new.stage_id)
        .bind(&new.state)
        .bind(&new.created_by)
        .bind(now)
        .bind(now)
        .bind(now)
        .execute(self.pool())
        .await?;
        self.ticket(&id)
            .await?
            .ok_or(StoreError::Database(sqlx::Error::RowNotFound))
    }

    pub async fn ticket(&self, id: &str) -> Result<Option<TicketRow>, StoreError> {
        Ok(sqlx::query_as("SELECT * FROM tickets WHERE id = ?")
            .bind(id)
            .fetch_optional(self.pool())
            .await?)
    }

    /// Records who added a child to its whole-queued epic's queue (EPC-13, migration 0030), or clears it (`None`).
    pub async fn set_joined_queue_by(
        &self,
        id: &str,
        actor: Option<&str>,
    ) -> Result<(), StoreError> {
        sqlx::query("UPDATE tickets SET joined_queue_by = ? WHERE id = ?")
            .bind(actor)
            .bind(id)
            .execute(self.pool())
            .await?;
        Ok(())
    }

    /// Records where the ticket's build output is (d0085, d0086) and why the last Free failed (`None` clears it).
    pub async fn set_build_state(
        &self,
        id: &str,
        state: BuildState,
        problem: Option<&str>,
    ) -> Result<(), StoreError> {
        sqlx::query("UPDATE tickets SET build_state = ?, build_problem = ? WHERE id = ?")
            .bind(state.as_str())
            .bind(problem)
            .bind(id)
            .execute(self.pool())
            .await?;
        Ok(())
    }

    /// Records the ticket resource Otaq created (d0087): its name, state and last problem.
    pub async fn set_resource(
        &self,
        id: &str,
        name: Option<&str>,
        state: ResourceState,
        problem: Option<&str>,
    ) -> Result<(), StoreError> {
        sqlx::query(
            "UPDATE tickets SET resource_name = ?, resource_state = ?, resource_problem = ? WHERE id = ?",
        )
        .bind(name)
        .bind(state.as_str())
        .bind(problem)
        .bind(id)
        .execute(self.pool())
        .await?;
        Ok(())
    }

    /// Stores the schema-path overlaps (M19-e) as JSON `[{other, path}]`, or clears them (`None`).
    pub async fn set_schema_overlap(&self, id: &str, json: Option<&str>) -> Result<(), StoreError> {
        sqlx::query("UPDATE tickets SET schema_overlap = ? WHERE id = ?")
            .bind(json)
            .bind(id)
            .execute(self.pool())
            .await?;
        Ok(())
    }

    /// A ticket by project and number: `(40, 0)` is SHOP-40, `(40, 1)` is SHOP-40.1.
    pub async fn ticket_by_number(
        &self,
        project_id: &str,
        number: i64,
        child_number: i64,
    ) -> Result<Option<TicketRow>, StoreError> {
        Ok(sqlx::query_as(
            "SELECT * FROM tickets WHERE project_id = ? AND number = ? AND child_number = ?",
        )
        .bind(project_id)
        .bind(number)
        .bind(child_number)
        .fetch_optional(self.pool())
        .await?)
    }
}
