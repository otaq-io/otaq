//! "conflicts with `main`" (GIT-03, GIT-04, GIT-06; d0038, d0044, d0051): a fact about a ticket's branch and the
//! branch it merges into, shown on the card and in the sheet, and named by `otaq prime`.
//!
//! It comes from merging the target into a Delivery's branch (the ticket goes to its agent with `restack`, no click, FLW-17),
//! from the read-only test merge after another ticket merged (T-0096; the ticket goes to its agent the same way,
//! d0127), or from merging the base into a reopened ticket's branch on its way to Dev (T-0101). It clears when the agent submits
//! again, or when the next test merge is clean.

use serde::{Deserialize, Serialize};

use std::path::Path;

use super::tickets::Ticket;
use super::{Board, BoardError};
use crate::domain::stored_enum;

/// `git log --format=%x00%H --name-only`: each commit's id and the files it lists.
fn parse_log(log: &str) -> Vec<(String, Vec<String>)> {
    log.split('\0')
        .filter_map(|chunk| {
            let mut lines = chunk.lines().map(str::trim).filter(|l| !l.is_empty());
            let sha = lines.next()?.to_owned();
            Some((sha, lines.map(str::to_owned).collect()))
        })
        .collect()
}

stored_enum! {
    /// Where a conflict was found.
    pub enum ConflictReason ("conflict reason") {
        /// Merging the target into a Delivery's branch: its agent fixes it with `restack`.
        Merge = "merge",
        /// The read-only test merge after another ticket merged, while this one waited in Review (GIT-04, d0127):
        /// its agent fixes it with `restack`, no click, unless its PR is open or the project is paused. The card says
        /// "Conflicts with main · fixing".
        TestMerge = "test_merge",
        /// Merging the base into a reopened ticket's branch on its way to Dev: the agent resolves it first.
        Reopen = "reopen",
        /// Local merge's Merge check failed on the merged result (d0120): no conflicting files; its agent fixes what
        /// failed with `restack`. The card says "Merge check failed · fixing", never "conflicts with".
        MergeCheck = "merge_check",
    }
}

/// A ticket's conflict with the branch it merges into.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Conflict {
    /// `main` or `develop`.
    pub target: String,
    pub files: Vec<String>,
    pub reason: ConflictReason,
    /// What moved on `target` since the ticket's branch left it, in the conflicting files: "SHOP-6 changed
    /// README.md". Read from git when the sheet asks ([`Board::conflict_details`]); never stored.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub moved: Vec<Moved>,
}

/// Who changed which conflicting files on the target: the ticket whose merge made the commit, or the commit itself
/// when no ticket did (a commit made by hand).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Moved {
    /// "SHOP-6", or the commit's short id ("4fa97d6").
    pub by: String,
    pub files: Vec<String>,
}

impl Board {
    /// Records that the ticket's branch conflicts with `target`.
    pub async fn set_conflict(
        &self,
        ticket_id: &str,
        conflict: &Conflict,
    ) -> Result<(), BoardError> {
        sqlx::query(
            "UPDATE tickets SET conflicts_with = ?, conflict_files = ?, conflict_reason = ? WHERE id = ?",
        )
        .bind(&conflict.target)
        .bind(serde_json::to_string(&conflict.files).expect("paths serialize"))
        .bind(conflict.reason.as_str())
        .bind(ticket_id)
        .execute(self.store.pool())
        .await?;
        Ok(())
    }

    pub async fn clear_conflict(&self, ticket_id: &str) -> Result<(), BoardError> {
        sqlx::query(
            "UPDATE tickets SET conflicts_with = NULL, conflict_files = NULL, conflict_reason = NULL WHERE id = ?",
        )
        .bind(ticket_id)
        .execute(self.store.pool())
        .await?;
        Ok(())
    }

    /// Whether the ticket's next Local merge runs the Merge check even when its branch is current (d0120): an earlier
    /// merge brought new commits into it, or its check failed, and no check passed since.
    pub async fn merge_check_due(&self, ticket_id: &str) -> Result<bool, BoardError> {
        Ok(self
            .store
            .ticket(ticket_id)
            .await?
            .is_some_and(|row| row.merge_check_due))
    }

    pub(crate) async fn set_merge_check_due(
        &self,
        ticket_id: &str,
        due: bool,
    ) -> Result<(), BoardError> {
        sqlx::query("UPDATE tickets SET merge_check_due = ? WHERE id = ?")
            .bind(due)
            .bind(ticket_id)
            .execute(self.store.pool())
            .await?;
        Ok(())
    }

    /// The ticket's recorded conflict, if any.
    pub async fn conflict(&self, ticket_id: &str) -> Result<Option<Conflict>, BoardError> {
        let row = self.store.ticket(ticket_id).await?;
        Ok(row.and_then(|row| {
            Some(Conflict {
                target: row.conflicts_with?,
                files: row
                    .conflict_files
                    .and_then(|f| serde_json::from_str(&f).ok())
                    .unwrap_or_default(),
                reason: row
                    .conflict_reason
                    .and_then(|r| r.parse().ok())
                    .unwrap_or(ConflictReason::Merge),
                moved: Vec::new(),
            })
        }))
    }

    /// The ticket's recorded conflict with what moved on its target ([`Conflict::moved`]), for the sheet's banner.
    /// git failing to say leaves `moved` empty: the banner still has the rest.
    pub async fn conflict_details(&self, ticket: &Ticket) -> Result<Option<Conflict>, BoardError> {
        let Some(mut conflict) = self.conflict(&ticket.id).await? else {
            return Ok(None);
        };
        match self.conflict_moves(ticket, &conflict).await {
            Ok(moved) => conflict.moved = moved,
            Err(e) => {
                tracing::warn!(ticket = %ticket.key, error = %e, "can't tell what moved on the target");
            }
        }
        Ok(Some(conflict))
    }

    /// The commits on the target since its merge base with the ticket's branch that touch the conflicting files,
    /// oldest first, grouped by the ticket whose merge made each one.
    async fn conflict_moves(
        &self,
        ticket: &Ticket,
        conflict: &Conflict,
    ) -> Result<Vec<Moved>, BoardError> {
        if conflict.files.is_empty() {
            return Ok(Vec::new());
        }
        let Some(branch) = ticket.work.as_ref().and_then(|w| w.branch.clone()) else {
            return Ok(Vec::new());
        };
        let Some(row) = self.store.ticket(&ticket.id).await? else {
            return Ok(Vec::new());
        };
        let project = self.project(ticket.key.project.as_str()).await?;
        let repo = Path::new(&project.repo_path);
        let git = self.git()?;
        let base = git
            .run(repo, &["merge-base", &conflict.target, &branch])
            .await?;
        let range = format!("{}..{}", base.trim(), conflict.target);
        let mut args = vec![
            "log",
            "--reverse",
            "--format=%x00%H",
            "--name-only",
            range.as_str(),
            "--",
        ];
        args.extend(conflict.files.iter().map(String::as_str));
        let log = git.run(repo, &args).await?;
        let mut moved: Vec<Moved> = Vec::new();
        for (sha, files) in parse_log(&log) {
            let merged: Option<(i64,)> = sqlx::query_as(
                "SELECT number FROM tickets WHERE project_id = ? AND merge_commit_sha = ?",
            )
            .bind(&row.project_id)
            .bind(&sha)
            .fetch_optional(self.store.pool())
            .await?;
            let by = match merged {
                Some((number,)) => format!("{}-{number}", ticket.key.project),
                None => sha.chars().take(7).collect(),
            };
            let i = match moved.iter().position(|m| m.by == by) {
                Some(i) => i,
                None => {
                    moved.push(Moved {
                        by,
                        files: Vec::new(),
                    });
                    moved.len() - 1
                }
            };
            for file in files {
                if !moved[i].files.contains(&file) {
                    moved[i].files.push(file);
                }
            }
        }
        Ok(moved)
    }
}
