//! A ticket's diff for the Files tab (REV-01, REV-02; d0017, d0049, d0059).
//!
//! A ticket's diff is always **its branch against its `base_sha`** (`base_sha...branch`), where it left its
//! base, never against the base's current head: a ticket never shows someone else's changes. The reply names
//! the base branch and the short `base_sha` ("diff vs `main` at `a1b2c3d`").
//!
//! Everything is read from the project's repository by branch name, so a Done ticket whose worktree was
//! removed still shows its diff.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::tickets::Ticket;
use super::{Board, BoardError, Refusal};
use crate::domain::TicketKey;
use crate::git::diff::{ChangedFile, DiffSpec, FileDiff};

/// How many characters of `base_sha` the base indicator shows.
pub const SHORT_SHA: usize = 7;

/// The file list of a ticket's diff.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiffSummary {
    pub key: TicketKey,
    /// The base branch: `main`.
    pub base_branch: String,
    pub base_sha: String,
    /// The first [`SHORT_SHA`] characters of `base_sha`.
    pub base_short: String,
    /// The branch shown: `otaq/SHOP-18`.
    pub branch: String,
    pub files: Vec<ChangedFile>,
    /// "+412 −38": the sum over the files.
    pub added: u32,
    pub removed: u32,
    /// The review round (1 + the Send backs so far): the Files tab keeps its viewed ticks per round.
    pub round: u32,
    /// What the agent said at `review submit` (or "Submitted by you" and the note), shown above the diff.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub review_summary: Option<String>,
}

/// Where a ticket's diff comes from.
struct DiffSource {
    repo: PathBuf,
    base_branch: String,
    base_sha: String,
    branch: String,
}

impl Board {
    /// The changed files of a ticket's branch since its `base_sha`.
    pub async fn diff_summary(&self, key: &str) -> Result<DiffSummary, BoardError> {
        let ticket = self.ticket(key).await?;
        let source = self.diff_source(&ticket).await?;
        let git = self.git()?;
        let files = git
            .changed_files(
                &source.repo,
                &DiffSpec::since(&source.base_sha, &source.branch),
            )
            .await?;
        let (added, removed) = totals(&files);
        Ok(DiffSummary {
            round: self.current_round(&ticket.id).await?,
            review_summary: self.review_summary(&ticket.id).await?,
            key: ticket.key,
            base_short: short_sha(&source.base_sha),
            base_branch: source.base_branch,
            base_sha: source.base_sha,
            branch: source.branch,
            files,
            added,
            removed,
        })
    }

    /// One file's hunks.
    pub async fn diff_file(&self, key: &str, path: &str) -> Result<FileDiff, BoardError> {
        let ticket = self.ticket(key).await?;
        let source = self.diff_source(&ticket).await?;
        let spec = DiffSpec::since(&source.base_sha, &source.branch);
        self.git()?
            .file_diff(&source.repo, &spec, path)
            .await?
            .ok_or_else(|| BoardError::NotFound(format!("{path} isn't changed in {}", ticket.key)))
    }

    /// One file's change from `since` (a reviewed head) to the branch's head: only the new commits (REV-06).
    pub async fn diff_file_since(
        &self,
        key: &str,
        path: &str,
        since: &str,
    ) -> Result<FileDiff, BoardError> {
        let ticket = self.ticket(key).await?;
        let source = self.diff_source(&ticket).await?;
        self.git()?
            .file_diff(
                &source.repo,
                &DiffSpec::between(since, &source.branch),
                path,
            )
            .await?
            .ok_or_else(|| BoardError::NotFound(format!("{path} isn't changed in {}", ticket.key)))
    }

    async fn diff_source(&self, ticket: &Ticket) -> Result<DiffSource, BoardError> {
        let project = self.project(ticket.key.project.as_str()).await?;
        let work = ticket.work.clone().unwrap_or_default();
        let (Some(branch), Some(base_sha)) = (work.branch, work.base_sha) else {
            return Err(BoardError::refused(
                Refusal::NoBranch,
                format!("{} has no branch yet: it hasn't started", ticket.key),
            ));
        };
        Ok(DiffSource {
            repo: PathBuf::from(project.repo_path),
            base_branch: ticket.base.branch().to_owned(),
            base_sha,
            branch,
        })
    }
}

fn totals(files: &[ChangedFile]) -> (u32, u32) {
    files.iter().fold((0, 0), |(a, r), f| {
        (a.saturating_add(f.added), r.saturating_add(f.removed))
    })
}

pub fn short_sha(sha: &str) -> String {
    sha.chars().take(SHORT_SHA).collect()
}
