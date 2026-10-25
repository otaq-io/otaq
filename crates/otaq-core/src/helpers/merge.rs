//! `otaq helper merge <n>` (d0133, d0038, d0018): the agent in charge merges a helper's piece into the ticket's
//! branch, one at a time (what 0124's merge turns became).
//!
//! 1. Refused while the ticket's worktree or the helper's has uncommitted files ("SHOP-40 has 2 uncommitted files",
//!    "40.3 has 1 uncommitted file"): nothing is checkpointed or stashed.
//! 2. **Free rebase** of the helper's branch onto the ticket's branch, in the helper's worktree, no tokens (d0038). A
//!    conflict puts everything back and merges nothing: the reply tells the agent in charge to send the helper back to
//!    rebase (`otaq helper back <n>`, with the sentence the restack skill would say).
//! 3. **Squash-merge** into the ticket's branch in the ticket's worktree (clean, by step 1): one commit named after the
//!    piece, the user's hooks running as for any commit; a hook that refuses it leaves the worktree as it was.
//! 4. The helper is `merged`: its session ends, its worktree is removed and its port freed; its branch stays until the
//!    ticket is Done (d0018); the files the squash touched are recorded for the Files tab's piece tags.

use std::path::Path;

use super::store;
use super::{HelperReply, HelperStatus, Helpers, ticket_branch, ticket_worktree};
use crate::board::{BoardError, Refusal};
use crate::caller::Caller;
use crate::git::checkpoint::{RebaseError, RebaseOutcome};
use crate::git::merge::uncommitted_files_message;
use crate::naming;

/// What the agent in charge is told when a helper's branch conflicts with the ticket's: nothing merged, and the note
/// to send the helper back with.
pub fn conflict_text(label: &str, piece: u32, against: &str, files: &[String]) -> String {
    let files = files.join(", ");
    let cmd = naming::COMMAND_NAME;
    format!(
        "{label} conflicts with {against} in {files}: nothing was merged. Send its helper back to rebase: `{cmd} helper back \
         {piece} --stdin` with \"Your branch conflicts with {against}. Rebase onto it (`git rebase {against}`), resolve the \
         conflicts in {files}, commit, then hand back with `{cmd} helper done --stdin`.\""
    )
}

impl Helpers {
    /// `otaq helper merge <n>` from the agent in charge (see the module).
    pub async fn merge(
        &self,
        caller: &Caller,
        key: &str,
        piece: u32,
    ) -> Result<HelperReply, BoardError> {
        let (ticket, _) = self.in_charge(caller, key).await?;
        let lock = self.lock_of(&ticket.id);
        let _one = lock.lock().await;
        let inner = &self.inner;
        let board = &inner.board;
        let pool = board.store().pool();
        let helper = self.piece(&ticket, piece).await?;
        let label = helper.label();
        if helper.status == HelperStatus::Merged {
            return Err(BoardError::refused(
                Refusal::MoveRefused,
                format!("{label} is merged already"),
            ));
        }
        let against = ticket_branch(&ticket)?;
        let ticket_wt = ticket_worktree(&ticket)?;
        let helper_wt = Path::new(&helper.worktree);
        if !helper_wt.is_dir() {
            return Err(BoardError::refused(
                Refusal::NoWorktree,
                format!(
                    "{label} has no worktree: start its helper again with `{} helper start {piece}`",
                    naming::COMMAND_NAME
                ),
            ));
        }
        let project = board.project(ticket.key.project.as_str()).await?;
        let repo = Path::new(&project.repo_path);
        let git = board.project_git(&project.key).await?;

        // 1. Both worktrees clean.
        for (name, folder) in [
            (ticket.key.to_string(), ticket_wt.as_path()),
            (label.clone(), helper_wt),
        ] {
            let files = git.uncommitted_files(folder).await?;
            if !files.is_empty() {
                return Err(BoardError::refused(
                    Refusal::Uncommitted,
                    uncommitted_files_message(&name, files.len()),
                ));
            }
        }

        // 2. The free rebase onto the ticket's branch, in the helper's worktree.
        let onto = git
            .resolve(repo, &format!("refs/heads/{against}"))
            .await?
            .ok_or_else(|| {
                BoardError::refused(
                    Refusal::NoBranch,
                    format!("{}'s branch {against} is gone", ticket.key),
                )
            })?;
        match git.rebase_with_checkpoint(helper_wt, &onto).await {
            Ok(RebaseOutcome::Clean) => {}
            Ok(RebaseOutcome::RebaseConflict { files } | RebaseOutcome::UndoConflict { files }) => {
                return Err(BoardError::refused(
                    Refusal::MoveRefused,
                    conflict_text(&label, piece, &against, &files),
                ));
            }
            Err(RebaseError::Git(e)) => return Err(e.into()),
            // Step 1 found it clean: no checkpoint was made.
            Err(e) => {
                return Err(BoardError::refused(Refusal::MoveRefused, e.to_string()));
            }
        }
        let range = format!("{onto}..{}", helper.branch);
        let subjects = git
            .run(repo, &["log", "--reverse", "--format=%s", &range])
            .await?;
        if subjects.trim().is_empty() {
            return Err(BoardError::refused(
                Refusal::MoveRefused,
                format!("{label} has nothing to merge: its branch has no commits of its own"),
            ));
        }
        let files: Vec<String> = git
            .run(repo, &["diff", "--name-only", "--no-renames", &range])
            .await?
            .lines()
            .filter(|l| !l.is_empty())
            .map(str::to_owned)
            .collect();

        // 3. The squash, in the ticket's worktree.
        let mut message = format!("{} ({label})\n\n", helper.title);
        for subject in subjects.lines().filter(|s| !s.trim().is_empty()) {
            message.push_str(&format!("- {subject}\n"));
        }
        squash(&git, &ticket_wt, &helper.branch, &message).await?;

        // 4. Merged: the session ends, the worktree goes, the branch stays.
        store::merged(pool, &helper.id, &files).await?;
        if let Some(live) = self.live_session(&helper)
            && let Err(e) = inner.sessions.stop(&live.id).await
        {
            tracing::warn!(helper = helper.id, error = %e, "can't end a merged helper's session");
        }
        if let Err(e) = crate::review::finish::remove_worktree(&git, repo, helper_wt).await {
            tracing::warn!(helper = helper.id, error = %e, "can't remove a merged helper's worktree");
        }
        let helper = self.piece(&ticket, piece).await?;
        self.record(&ticket, &helper, "merged", None).await?;
        let count = files.len();
        let plural = if count == 1 { "" } else { "s" };
        Ok(HelperReply {
            text: format!(
                "Merged {label} ({}) into {against}: {count} file{plural}. Its worktree is removed; its branch stays until {} is Done.",
                helper.title, ticket.key
            ),
            helper,
        })
    }
}

/// `git merge --squash` of `branch` in `worktree` (clean), committed with `message`; a refused commit (a hook) puts
/// the worktree back as it was.
async fn squash(
    git: &crate::git::Git,
    worktree: &Path,
    branch: &str,
    message: &str,
) -> Result<(), BoardError> {
    let undo = async || {
        let _ = git
            .run(worktree, &["reset", "--quiet", "--hard", "HEAD"])
            .await;
    };
    if let Err(e) = git
        .run(worktree, &["merge", "--quiet", "--squash", branch])
        .await
    {
        undo().await;
        return Err(e.into());
    }
    let file = tempfile::NamedTempFile::new().map_err(|e| {
        BoardError::Git(crate::git::GitError::Spawn {
            args: "commit -F".into(),
            error: e,
        })
    })?;
    std::fs::write(file.path(), message).map_err(|e| {
        BoardError::Git(crate::git::GitError::Spawn {
            args: "commit -F".into(),
            error: e,
        })
    })?;
    let path = file.path().display().to_string();
    if let Err(e) = git.run(worktree, &["commit", "--quiet", "-F", &path]).await {
        undo().await;
        return Err(e.into());
    }
    Ok(())
}
