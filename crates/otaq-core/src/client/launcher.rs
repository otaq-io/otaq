//! Starting the background service when it isn't running (d0055: the app starts it; nothing starts it at login).

use std::ffi::OsString;
use std::future::Future;
use std::io;
use std::path::PathBuf;
use std::pin::Pin;

use crate::naming;
use crate::protocol::ErrorBody;
use crate::store::NewerDatabase;
use crate::store::versions;

/// Starts the service. [`super::Client::connect_or_start`] calls it at most once, when no service answers.
pub trait Launcher: Send + Sync {
    fn launch(&self) -> Pin<Box<dyn Future<Output = io::Result<()>> + Send + '_>>;
}

/// Runs `otaq daemon start` and waits for it to finish. That command returns once the service answers,
/// and reports "already running" instead of starting a second one.
#[derive(Debug, Clone)]
pub struct CommandLauncher {
    pub program: PathBuf,
    /// Extra environment for the command, for example the data-folder override in tests.
    pub env: Vec<(OsString, OsString)>,
}

impl CommandLauncher {
    pub fn new(program: impl Into<PathBuf>) -> Self {
        Self {
            program: program.into(),
            env: Vec::new(),
        }
    }

    pub fn with_env(mut self, key: impl Into<OsString>, value: impl Into<OsString>) -> Self {
        self.env.push((key.into(), value.into()));
        self
    }

    /// The `otaq` command installed next to the running program (as `cargo build` and installs lay it out).
    pub fn next_to_current_exe() -> io::Result<Self> {
        let exe = std::env::current_exe()?;
        let dir = exe
            .parent()
            .ok_or_else(|| io::Error::other("program has no folder"))?;
        Ok(Self::new(dir.join(format!(
            "{}{}",
            naming::COMMAND_NAME,
            std::env::consts::EXE_SUFFIX
        ))))
    }
}

impl Launcher for CommandLauncher {
    fn launch(&self) -> Pin<Box<dyn Future<Output = io::Result<()>> + Send + '_>> {
        Box::pin(async move {
            let mut command = tokio::process::Command::new(&self.program);
            // The app has no console on Windows: without this the command would open a console window.
            #[cfg(windows)]
            command.creation_flags(crate::process::CREATE_NO_WINDOW);
            // `--json`: a start that fails prints its error body on stdout, which may carry a typed reason.
            let output = command
                .args(["daemon", "start", "--json"])
                .envs(self.env.iter().map(|(k, v)| (k, v)))
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::null())
                .output()
                .await?;
            if output.status.success() {
                return Ok(());
            }
            if let Some(newer) = newer_database(&output.stdout) {
                return Err(io::Error::other(newer));
            }
            Err(io::Error::other(format!(
                "{} daemon start failed ({})",
                self.program.display(),
                output.status
            )))
        })
    }
}

/// The reason in a failed `daemon start --json`'s error body, when its database is from a newer build (UPD-08).
fn newer_database(stdout: &[u8]) -> Option<NewerDatabase> {
    #[derive(serde::Deserialize)]
    struct Failed {
        error: ErrorBody,
    }
    let failed: Failed = serde_json::from_slice(stdout).ok()?;
    versions::from_error_data(failed.error.data.as_ref())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::ErrorKind;
    use crate::store::versions::error_data;

    #[test]
    fn a_failed_start_carries_the_newer_database() {
        let newer = NewerDatabase {
            last_opened_by: "0.2.0".into(),
            copy: None,
        };
        let body = ErrorBody {
            kind: ErrorKind::Refused,
            message: newer.message(),
            data: Some(error_data(&newer)),
        };
        let stdout = serde_json::to_vec(&serde_json::json!({ "error": body })).unwrap();
        assert_eq!(newer_database(&stdout), Some(newer));
        let other = serde_json::json!({"error": {"kind": "internal", "message": "didn't start"}});
        assert_eq!(newer_database(&serde_json::to_vec(&other).unwrap()), None);
        assert_eq!(newer_database(b"not json"), None);
    }
}
