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
