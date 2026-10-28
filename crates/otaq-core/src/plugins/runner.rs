//! Runs a plugin's command lines (the otaq-marketplace README, "How a plugin works"): the program found on
//! the service's `PATH` (with `PATHEXT` on Windows, so `az.cmd`), started directly with no shell, no input, a time
//! limit and capped output. Plugins get the service's own environment, never a project's variables, so the CLIs'
//! logins are the user's.

use std::ffi::OsString;
use std::process::Stdio;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tokio::io::AsyncReadExt;

use crate::tools::find_on_path;

/// A search or a fetch.
pub const COMMAND_TIMEOUT: Duration = Duration::from_secs(30);
/// An installed or logged-in check.
pub const CHECK_TIMEOUT: Duration = Duration::from_secs(15);
const MAX_STDOUT: usize = 10 * 1024 * 1024;
const MAX_STDERR: usize = 64 * 1024;

/// Why a plugin command didn't give an answer, in the words the import dialog and the settings show.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunError {
    pub kind: RunErrorKind,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunErrorKind {
    NotInstalled,
    NotLoggedIn,
    BadSettings,
    NotFound,
    RateLimited,
    Failed,
}

impl RunError {
    pub fn new(kind: RunErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
            hint: None,
        }
    }

    pub fn with_hint(mut self, hint: Option<String>) -> Self {
        self.hint = hint;
        self
    }
}

impl std::fmt::Display for RunError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)?;
        if let Some(hint) = &self.hint {
            write!(f, " (try: {hint})")?;
        }
        Ok(())
    }
}

/// What a finished command printed.
#[derive(Debug, Clone)]
pub struct Output {
    pub success: bool,
    pub stdout: String,
    pub stderr: String,
}

/// Where commands run: the service's `PATH`.
#[derive(Debug, Clone)]
pub struct Runner {
    pub path: OsString,
}

impl Runner {
    /// Runs `argv` (the program first). `NotInstalled` when the program isn't on `PATH`.
    pub async fn run(&self, argv: &[String], timeout: Duration) -> Result<Output, RunError> {
        let (program, args) = argv
            .split_first()
            .ok_or_else(|| RunError::new(RunErrorKind::Failed, "The command is empty"))?;
        let resolved = find_on_path(program, &self.path).ok_or_else(|| {
            RunError::new(
                RunErrorKind::NotInstalled,
                format!(
                    "{program} isn't installed (or isn't on the PATH {} sees)",
                    crate::naming::PRODUCT_NAME
                ),
            )
        })?;
        let mut command = tokio::process::Command::new(&resolved);
        command
            .args(args)
            .env("PATH", &self.path)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        #[cfg(windows)]
        {
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            command.creation_flags(CREATE_NO_WINDOW);
        }
        let mut child = command.spawn().map_err(|e| {
            // Rust refuses arguments it can't pass safely to a .cmd/.bat program (`InvalidInput`).
            let kind = if e.kind() == std::io::ErrorKind::InvalidInput {
                RunErrorKind::BadSettings
            } else {
                RunErrorKind::Failed
            };
            RunError::new(kind, format!("{program} can't start: {e}"))
        })?;
        let mut stdout = child.stdout.take().expect("piped");
        let mut stderr = child.stderr.take().expect("piped");
        let finished = async {
            let mut out = Vec::new();
            let mut err = Vec::new();
            let mut limited_out = (&mut stdout).take(MAX_STDOUT as u64 + 1);
            let mut limited_err = (&mut stderr).take(MAX_STDERR as u64);
            let (read_out, read_err) = tokio::join!(
                limited_out.read_to_end(&mut out),
                limited_err.read_to_end(&mut err),
            );
            read_out.and(read_err)?;
            let status = child.wait().await?;
            Ok::<_, std::io::Error>((status, out, err))
        };
        let (status, out, err) = match tokio::time::timeout(timeout, finished).await {
            Ok(Ok(done)) => done,
            Ok(Err(e)) => {
                return Err(RunError::new(
                    RunErrorKind::Failed,
                    format!("{program} failed: {e}"),
                ));
            }
            Err(_) => {
                return Err(RunError::new(
                    RunErrorKind::Failed,
                    format!("{program} took longer than {} seconds", timeout.as_secs()),
                ));
            }
        };
        if out.len() > MAX_STDOUT {
            return Err(RunError::new(
                RunErrorKind::Failed,
                format!("{program} printed more than 10 MB"),
            ));
        }
        Ok(Output {
            success: status.success(),
            stdout: String::from_utf8_lossy(&out).into_owned(),
            stderr: String::from_utf8_lossy(&err).into_owned(),
        })
    }

    /// Runs a search or fetch and reads its JSON. A failed run is classified from its error output.
    pub async fn run_json(&self, argv: &[String]) -> Result<serde_json::Value, RunError> {
        let output = self.run(argv, COMMAND_TIMEOUT).await?;
        let program = &argv[0];
        if !output.success {
            return Err(classify(program, &output.stderr, &output.stdout));
        }
        serde_json::from_str(output.stdout.trim()).map_err(|e| {
            RunError::new(
                RunErrorKind::Failed,
                format!("{program}'s output isn't JSON: {e}"),
            )
        })
    }
}

/// A failed command's kind and one-line message, from what it printed.
pub fn classify(program: &str, stderr: &str, stdout: &str) -> RunError {
    let text = if stderr.trim().is_empty() {
        stdout
    } else {
        stderr
    };
    let lower = text.to_ascii_lowercase();
    let line = text
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or("it gave no reason");
    let has = |words: &[&str]| words.iter().any(|w| lower.contains(w));
    let kind = if has(&[
        "not logged",
        "auth login",
        "az login",
        "please login",
        "please log in",
        "unauthorized",
        "authentication",
        "401",
        "token has expired",
    ]) {
        RunErrorKind::NotLoggedIn
    } else if has(&["rate limit", "429", "too many requests"]) {
        RunErrorKind::RateLimited
    } else if has(&["not found", "could not resolve", "does not exist", "404"]) {
        RunErrorKind::NotFound
    } else {
        RunErrorKind::Failed
    };
    RunError::new(kind, format!("{program}: {line}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failures_are_classified() {
        let k = |stderr: &str| classify("gh", stderr, "").kind;
        assert_eq!(
            k("To get started with GitHub CLI, please run:  gh auth login"),
            RunErrorKind::NotLoggedIn
        );
        assert_eq!(
            k("ERROR: Please run 'az login' to setup account."),
            RunErrorKind::NotLoggedIn
        );
        assert_eq!(
            k("HTTP 429: API rate limit exceeded"),
            RunErrorKind::RateLimited
        );
        assert_eq!(
            k("GraphQL: Could not resolve to a Repository"),
            RunErrorKind::NotFound
        );
        assert_eq!(k("boom"), RunErrorKind::Failed);
        assert_eq!(
            classify("gh", "\n  first line\nsecond", "").message,
            "gh: first line"
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn runs_a_program_from_path_without_a_shell() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let program = dir.path().join("fake-cli");
        std::fs::write(&program, "#!/bin/sh\nprintf '%s\\n' \"$1\"\n").unwrap();
        std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
        let runner = Runner {
            path: dir.path().as_os_str().to_owned(),
        };
        let argv = vec!["fake-cli".to_owned(), "a b; echo pwned".to_owned()];
        let out = runner.run(&argv, CHECK_TIMEOUT).await.unwrap();
        assert_eq!(
            out.stdout, "a b; echo pwned\n",
            "one argument, never split or run"
        );
        let missing = runner
            .run(&["nope".to_owned()], CHECK_TIMEOUT)
            .await
            .unwrap_err();
        assert_eq!(missing.kind, RunErrorKind::NotInstalled);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn a_slow_command_is_stopped() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let program = dir.path().join("slow");
        std::fs::write(&program, "#!/bin/sh\nexec sleep 30\n").unwrap();
        std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
        let runner = Runner {
            path: dir.path().as_os_str().to_owned(),
        };
        let started = std::time::Instant::now();
        let e = runner
            .run(&["slow".to_owned()], Duration::from_millis(200))
            .await
            .unwrap_err();
        assert!(e.message.contains("longer than"));
        assert!(started.elapsed() < Duration::from_secs(5));
    }
}
