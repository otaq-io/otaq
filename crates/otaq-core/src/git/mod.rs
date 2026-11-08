//! Runs the user's own `git` command, never a git library, so the user's config, hooks and credentials apply
//! (d0055, P4). Every call is logged at debug level.
//!
//! There is deliberately no stash helper: Otaq never stashes and never touches a dirty checkout (P3, d0038).

pub mod checkpoint;
pub mod diff;
pub mod merge;
pub mod reword;
mod sync;
pub mod testing;

pub use sync::{BaseSync, FetchFailure};

use std::ffi::OsStr;
use std::fmt;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Stdio;

use crate::tools::find_on_path;

/// The user's `git` program.
#[derive(Debug, Clone)]
pub struct Git {
    program: PathBuf,
    /// Set in every command, before a call's own variables (a project's variables, ENV-02).
    env: Vec<(String, String)>,
}

impl Git {
    /// Finds `git` on the process's `PATH`.
    pub fn find() -> Result<Self, GitError> {
        Self::find_on(&std::env::var_os("PATH").unwrap_or_default())
    }

    /// Finds `git` in the folders of `path`.
    pub fn find_on(path: &OsStr) -> Result<Self, GitError> {
        find_on_path("git", path)
            .map(|program| Self {
                program,
                env: Vec::new(),
            })
            .ok_or(GitError::NotFound)
    }

    pub fn program(&self) -> &Path {
        &self.program
    }

    /// The same `git`, with `env` set in every command it runs: a project's variables (ENV-02, d0082), so the
    /// user's hooks see them. A call's own variables still win over them.
    pub fn with_env(&self, env: Vec<(String, String)>) -> Self {
        Self {
            program: self.program.clone(),
            env,
        }
    }

    /// Runs `git <args>` in `cwd` and returns its standard output.
    pub async fn run(&self, cwd: &Path, args: &[&str]) -> Result<String, GitError> {
        self.run_with_env(cwd, args, &[]).await
    }

    /// Like [`run`](Self::run), with extra environment variables.
    pub async fn run_with_env(
        &self,
        cwd: &Path,
        args: &[&str],
        env: &[(&str, &str)],
    ) -> Result<String, GitError> {
        let mut command = tokio::process::Command::new(&self.program);
        command
            .args(args)
            .current_dir(cwd)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .envs(self.env.iter().map(|(k, v)| (k, v)));
        for (key, value) in env {
            command.env(key, value);
        }
        let output = command.output().await.map_err(|e| GitError::Spawn {
            args: join(args),
            error: e,
        })?;
        let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        tracing::debug!(
            cwd = %cwd.display(),
            args = %join(args),
            code = output.status.code(),
            stderr = %stderr,
            "git"
        );
        if output.status.success() {
            Ok(stdout)
        } else {
            Err(GitError::Failed {
                args: join(args),
                code: output.status.code(),
                stderr,
            })
        }
    }

    /// Runs `git <args>` and returns its exit code and standard output, for commands whose non-zero codes carry
    /// meaning (`merge-tree` exits 1 on conflicts). Only a git that can't run is an error.
    pub async fn run_with_code(
        &self,
        cwd: &Path,
        args: &[&str],
    ) -> Result<(i32, String), GitError> {
        let output = tokio::process::Command::new(&self.program)
            .args(args)
            .current_dir(cwd)
            .envs(self.env.iter().map(|(k, v)| (k, v)))
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .output()
            .await
            .map_err(|e| GitError::Spawn {
                args: join(args),
                error: e,
            })?;
        let code = output.status.code().unwrap_or(-1);
        tracing::debug!(cwd = %cwd.display(), args = %join(args), code, "git");
        Ok((code, String::from_utf8_lossy(&output.stdout).into_owned()))
    }

    /// Whether `path` is inside a git work tree.
    pub async fn is_repo(&self, path: &Path) -> bool {
        self.run(path, &["rev-parse", "--is-inside-work-tree"])
            .await
            .is_ok_and(|out| out.trim() == "true")
    }

    /// The top folder of the repository that contains `path`.
    pub async fn repo_root(&self, path: &Path) -> Result<PathBuf, GitError> {
        let out = self.run(path, &["rev-parse", "--show-toplevel"]).await?;
        Ok(PathBuf::from(out.trim()))
    }

    /// The checked-out branch, or `None` on a detached HEAD.
    pub async fn current_branch(&self, repo: &Path) -> Result<Option<String>, GitError> {
        match self
            .run(repo, &["symbolic-ref", "--quiet", "--short", "HEAD"])
            .await
        {
            Ok(out) => Ok(Some(out.trim().to_owned())),
            Err(GitError::Failed { code: Some(1), .. }) => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// The branch `origin/HEAD` points to (the remote's default branch, e.g. `develop`), if the repository
    /// has an `origin` with a known default.
    pub async fn origin_head(&self, repo: &Path) -> Result<Option<String>, GitError> {
        match self
            .run(
                repo,
                &[
                    "symbolic-ref",
                    "--quiet",
                    "--short",
                    "refs/remotes/origin/HEAD",
                ],
            )
            .await
        {
            Ok(out) => Ok(out
                .trim()
                .strip_prefix("origin/")
                .map(str::to_owned)
                .filter(|b| !b.is_empty())),
            Err(GitError::Failed { .. }) => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// Local branch names, sorted.
    pub async fn local_branches(&self, repo: &Path) -> Result<Vec<String>, GitError> {
        self.refs(repo, "refs/heads").await
    }

    /// Remote-tracking branches as `origin/main`, sorted; symbolic refs such as `origin/HEAD` are left out.
    pub async fn remote_branches(&self, repo: &Path) -> Result<Vec<String>, GitError> {
        let out = self
            .run(
                repo,
                &[
                    "for-each-ref",
                    "--format=%(refname:short)%09%(symref)",
                    "refs/remotes",
                ],
            )
            .await?;
        Ok(out
            .lines()
            .filter_map(|line| {
                let (name, symref) = line.split_once('\t').unwrap_or((line, ""));
                symref.is_empty().then(|| name.to_owned())
            })
            .collect())
    }

    /// What git still needs to sign a commit made in `cwd`, as git itself works it out (its config, the environment,
    /// or a guess from the account and the machine's name): `None` when it can commit; otherwise the name and email
    /// it has so far, to fill in. A fresh Mac often has neither, and every commit then fails with "Author identity
    /// unknown".
    pub async fn missing_identity(&self, cwd: &Path) -> Result<Option<Identity>, GitError> {
        let (author, _) = self
            .run_with_code(cwd, &["var", "GIT_AUTHOR_IDENT"])
            .await?;
        let (committer, _) = self
            .run_with_code(cwd, &["var", "GIT_COMMITTER_IDENT"])
            .await?;
        if author == 0 && committer == 0 {
            return Ok(None);
        }
        Ok(Some(Identity {
            name: self.config_value(cwd, "user.name").await,
            email: self.config_value(cwd, "user.email").await,
        }))
    }

    /// A config value as git reads it in `cwd`; `None` when unset or empty.
    async fn config_value(&self, cwd: &Path, key: &str) -> Option<String> {
        self.run(cwd, &["config", "--get", key])
            .await
            .ok()
            .map(|v| v.trim().to_owned())
            .filter(|v| !v.is_empty())
    }

    /// Writes the name and email every repository's commits carry into the user's own git settings, as
    /// `git config --global user.name` and `user.email` do. Only ever on the user's click.
    pub async fn set_global_identity(&self, name: &str, email: &str) -> Result<(), IdentityError> {
        let (name, email) = (name.trim(), email.trim());
        if name.is_empty() {
            return Err(IdentityError::Invalid("Type your name first.".to_owned()));
        }
        if name.contains(['\n', '\r', '<', '>']) {
            return Err(IdentityError::Invalid(
                "A name can\u{2019}t contain line breaks or < >.".to_owned(),
            ));
        }
        if email.is_empty() {
            return Err(IdentityError::Invalid("Type your email first.".to_owned()));
        }
        if !email.contains('@') || email.contains(char::is_whitespace) || email.contains(['<', '>'])
        {
            return Err(IdentityError::Invalid(format!(
                "\u{201c}{email}\u{201d} doesn\u{2019}t look like an email address."
            )));
        }
        // Outside any repository, so only the user's own settings are read and written.
        let cwd = std::env::temp_dir();
        self.run(&cwd, &["config", "--global", "user.name", name])
            .await?;
        self.run(&cwd, &["config", "--global", "user.email", email])
            .await?;
        Ok(())
    }

    async fn refs(&self, repo: &Path, prefix: &str) -> Result<Vec<String>, GitError> {
        let out = self
            .run(repo, &["for-each-ref", "--format=%(refname:short)", prefix])
            .await?;
        Ok(out.lines().map(str::to_owned).collect())
    }
}

fn join(args: &[&str]) -> String {
    args.join(" ")
}

/// The name and email git has for commits, either possibly missing ([`Git::missing_identity`]).
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Identity {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
}

/// Why [`Git::set_global_identity`] wrote nothing.
#[derive(Debug)]
pub enum IdentityError {
    /// What was typed can't be used; the words say what to fix.
    Invalid(String),
    Git(GitError),
}

impl From<GitError> for IdentityError {
    fn from(e: GitError) -> Self {
        Self::Git(e)
    }
}

impl fmt::Display for IdentityError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Invalid(why) => f.write_str(why),
            Self::Git(e) => write!(f, "git couldn\u{2019}t save your name and email: {e}"),
        }
    }
}

#[derive(Debug)]
pub enum GitError {
    /// No `git` on `PATH` (the tools checklist flags this too).
    NotFound,
    /// `git` couldn't be started.
    Spawn { args: String, error: io::Error },
    /// `git` ran and failed.
    Failed {
        args: String,
        code: Option<i32>,
        stderr: String,
    },
}

impl fmt::Display for GitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFound => write!(
                f,
                "git isn't installed or isn't on PATH; install it and try again"
            ),
            Self::Spawn { args, error } => write!(f, "can't run git {args}: {error}"),
            Self::Failed { args, code, stderr } => {
                write!(f, "git {args} failed")?;
                if let Some(code) = code {
                    write!(f, " (exit {code})")?;
                }
                if !stderr.is_empty() {
                    write!(f, ": {stderr}")?;
                }
                Ok(())
            }
        }
    }
}

impl std::error::Error for GitError {}
