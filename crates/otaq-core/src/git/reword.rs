//! Rewording commits for the PR flow (REV-10, d0050): Otaq rewrites the commit messages the user confirmed,
//! with no tokens and no file changes. Each new commit keeps its tree and author; the committer is the user's git
//! identity (P4). Commits whose message and parent are unchanged keep their IDs, so already-pushed commits stay
//! as they are. Nothing here moves a ref: the caller does that once the whole chain is built and checked.

use std::path::Path;

use super::{Git, GitError, GitStep, HookRefusal};

/// One commit of a range.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitInfo {
    pub sha: String,
    /// More than one parent: a merge commit (main merged into the branch); rewording keeps its other parents.
    pub merge: bool,
    /// The full message, trimmed.
    pub message: String,
}

impl CommitInfo {
    /// The message's first line.
    pub fn subject(&self) -> &str {
        self.message.lines().next().unwrap_or_default()
    }
}

/// What the project's commit-msg hook made of a message ([`Git::commit_msg_hook`]).
#[derive(Debug)]
pub enum MessageCheck {
    /// Accepted: the message as the hook left it (a hook may add a line), or as it was without a hook.
    Passed(String),
    /// Refused: which hook and its whole output.
    Refused(Box<HookRefusal>),
}

impl Git {
    /// The commits after `from` up to `to`, oldest first (`from..to`, following first parents).
    pub async fn commits_between(
        &self,
        repo: &Path,
        from: &str,
        to: &str,
    ) -> Result<Vec<CommitInfo>, GitError> {
        let range = format!("{from}..{to}");
        let out = self
            .run(
                repo,
                &[
                    "log",
                    "--reverse",
                    "--first-parent",
                    "--format=%H%x00%P%x00%B%x1e",
                    &range,
                ],
            )
            .await?;
        Ok(out
            .split('\u{1e}')
            .filter_map(|record| {
                let mut fields = record.trim_start_matches('\n').splitn(3, '\0');
                let sha = fields.next()?.trim();
                if sha.is_empty() {
                    return None;
                }
                let parents = fields.next().unwrap_or_default();
                Some(CommitInfo {
                    sha: sha.to_owned(),
                    merge: parents.split_whitespace().count() > 1,
                    message: fields.next().unwrap_or_default().trim().to_owned(),
                })
            })
            .collect())
    }

    /// Builds the chain `parent` ← `commits[0]` ← … with the given messages and returns its tip. Trees and authors
    /// stay; a commit whose message and parent are unchanged keeps its ID. A merge commit (main merged into the branch)
    /// keeps its other parents.
    pub async fn reword(
        &self,
        repo: &Path,
        parent: &str,
        commits: &[(CommitInfo, String)],
    ) -> Result<String, GitError> {
        self.step(GitStep::Reword);
        let mut parent = parent.to_owned();
        let mut changed = false;
        for (commit, message) in commits {
            let message = message.trim();
            if !changed && message == commit.message {
                parent = commit.sha.clone();
                continue;
            }
            changed = true;
            let tree_ref = format!("{}^{{tree}}", commit.sha);
            let tree = self.run(repo, &["rev-parse", &tree_ref]).await?;
            let author = self
                .run(
                    repo,
                    &["log", "-1", "--format=%an%x00%ae%x00%aI", &commit.sha],
                )
                .await?;
            let mut author = author.trim_end_matches('\n').splitn(3, '\0');
            let (name, email, date) = (
                author.next().unwrap_or_default().to_owned(),
                author.next().unwrap_or_default().to_owned(),
                author.next().unwrap_or_default().to_owned(),
            );
            let mut args = vec!["commit-tree", tree.trim(), "-p", parent.as_str()];
            let parents = if commit.merge {
                self.run(repo, &["rev-list", "--parents", "-n", "1", &commit.sha])
                    .await?
            } else {
                String::new()
            };
            for other in parents.split_whitespace().skip(2) {
                args.extend(["-p", other]);
            }
            args.extend(["-m", message]);
            let new = self
                .run_with_env(
                    repo,
                    &args,
                    &[
                        ("GIT_AUTHOR_NAME", &name),
                        ("GIT_AUTHOR_EMAIL", &email),
                        ("GIT_AUTHOR_DATE", &date),
                    ],
                )
                .await?;
            parent = new.trim().to_owned();
        }
        Ok(parent)
    }

    /// Puts `message` through the project's commit-msg hook in `worktree`, as `git commit` would before it writes a
    /// commit, without making one: `git hook run commit-msg` (git 2.36 or newer) runs the user's own hook, found as
    /// git finds it (`core.hooksPath`, hooks set in the config), in the worktree's top folder, with the message in the
    /// worktree's `COMMIT_EDITMSG` and the variables a commit gives it (`GIT_INDEX_FILE`, and `GIT_EDITOR=:` since no
    /// editor opens). No commit-msg hook: it passes as it is. The hook may rewrite the file, as a hook adding a
    /// `Change-Id` does; a commit keeps what it wrote, and so does the answer. A git too old for `git hook` can't run
    /// it: the message passes unchecked, as before.
    pub async fn commit_msg_hook(
        &self,
        worktree: &Path,
        message: &str,
    ) -> Result<MessageCheck, GitError> {
        self.step(GitStep::Reword);
        let paths = self
            .run(
                worktree,
                &[
                    "rev-parse",
                    "--path-format=absolute",
                    "--git-path",
                    "COMMIT_EDITMSG",
                    "--git-path",
                    "index",
                ],
            )
            .await?;
        let mut paths = paths.lines();
        let (Some(file), Some(index)) = (paths.next(), paths.next()) else {
            return Ok(MessageCheck::Passed(message.to_owned()));
        };
        let file = std::path::PathBuf::from(file.trim());
        let write = |e: std::io::Error| GitError::Spawn {
            args: format!("hook run commit-msg -- {}", file.display()),
            error: e,
        };
        tokio::fs::write(&file, format!("{}\n", message.trim()))
            .await
            .map_err(write)?;
        let file_arg = file.display().to_string();
        let ran = self
            .run_with_env(
                worktree,
                &[
                    "hook",
                    "run",
                    "--ignore-missing",
                    "commit-msg",
                    "--",
                    &file_arg,
                ],
                &[("GIT_INDEX_FILE", index.trim()), ("GIT_EDITOR", ":")],
            )
            .await;
        match ran {
            Ok(_) => {
                let kept = tokio::fs::read_to_string(&file).await.map_err(write)?;
                Ok(MessageCheck::Passed(match kept.trim() {
                    "" => message.trim().to_owned(),
                    kept => kept.to_owned(),
                }))
            }
            Err(GitError::HookRefused(refusal)) => Ok(MessageCheck::Refused(refusal)),
            Err(GitError::Failed { stderr, .. }) if stderr.contains("is not a git command") => {
                tracing::warn!(
                    "git has no `git hook` (it needs 2.36): the PR's messages weren't checked by commit-msg"
                );
                Ok(MessageCheck::Passed(message.trim().to_owned()))
            }
            Err(e) => Err(e),
        }
    }

    /// The tree of a commit.
    pub async fn tree_of(&self, repo: &Path, commit: &str) -> Result<String, GitError> {
        let tree_ref = format!("{commit}^{{tree}}");
        Ok(self
            .run(repo, &["rev-parse", &tree_ref])
            .await?
            .trim()
            .to_owned())
    }
}
