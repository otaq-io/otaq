//! `otaq run …` (d{RUN}): a ticket's agent starts, reads and stops the ticket's Run, the project's run command in
//! the worktree with `$PORT` = the Run port. They work on `$OTAQ_TICKET` (or `-P`/the folder's ticket for the user).
//!
//! - `start [--wait] [--timeout S]`: starts the Run, or uses the one already up (whoever started it). With `--wait` it
//!   returns once the Run is running (printing the Open URL, or the port), or fails when it failed to start, exited,
//!   or is still starting after the timeout (default 300 s), printing the Run's last 40 lines. A port held by another
//!   program is an error, with or without `--wait`.
//! - `status`: not started · starting :4020 · running :4020 (started by Dev) · stopped (how).
//! - `log [--tail N]`: the Run's kept output as text, the last N lines (default 200). Otaq never writes it to disk
//!   (d0045): save it yourself (`otaq run log > "$OTAQ_RESOURCE_DIR/run.txt"`).
//! - `stop`: stops a Run an agent of this ticket started; one the user started is refused and stays up.
//!
//! `--json` prints the Run (`otaq_core::terminals::run::RunInfo`); `log --json` `{"key", "state", "text"}`.

use std::time::{Duration, Instant};

use clap::Subcommand;
use otaq_core::client::Client;
use otaq_core::data_dir::DataDir;
use otaq_core::naming;
use otaq_core::protocol::{ErrorBody, ErrorKind, KeyParams, Reply, Request, RunLogParams};
use otaq_core::terminals::run::{RunInfo, RunLog, RunState};

use crate::cli::GlobalArgs;
use crate::output::Output;
use crate::read::unexpected;
use crate::service::{self, client_error, internal};
use crate::target;

/// How often `start --wait` asks for the Run's state.
const POLL: Duration = Duration::from_millis(250);
/// The lines a failed `start --wait` prints.
const FAILED_TAIL: u32 = 40;

#[derive(Debug, Clone, PartialEq, Eq, Subcommand)]
pub enum RunCommand {
    /// Start the ticket's Run (or use the one already up); with --wait, return once it is running or failed
    Start {
        /// Wait until the Run is running (prints its URL or port) or failed to start (exits non-zero)
        #[arg(long)]
        wait: bool,
        /// With --wait: give up after this many seconds still starting
        #[arg(long, value_name = "SECONDS", default_value_t = 300, requires = "wait")]
        timeout: u64,
    },
    /// The Run's state: not started, starting, running :4020 (started by Dev), stopped (how)
    Status,
    /// The Run's output, kept in memory only: the last N lines (default 200)
    Log {
        /// How many lines, from the end
        #[arg(long, value_name = "N")]
        tail: Option<u32>,
    },
    /// Stop a Run an agent of this ticket started (one the user started stays running)
    Stop,
}

/// Runs one `otaq run` command.
pub fn run(command: RunCommand, global: &GlobalArgs, out: &Output) -> Result<(), ErrorBody> {
    let data = DataDir::from_env().map_err(|e| internal(e.to_string()))?;
    service::runtime()?.block_on(async {
        let client = service::connect(&data, global).await?;
        let key = target::resolve_key(&client, &data, None, global)
            .await?
            .to_string();
        match command {
            RunCommand::Start { wait, timeout } => {
                let info = run_reply(
                    client
                        .request(Request::RunStart(KeyParams { key: key.clone() }))
                        .await
                        .map_err(client_error)?,
                )?;
                let info = if wait {
                    wait_until_up(&client, info, Duration::from_secs(timeout)).await?
                } else {
                    info
                };
                if info.state == RunState::PortInUse {
                    return Err(port_in_use(&info));
                }
                out.print(&started_text(&info), &info, Some(&key));
            }
            RunCommand::Status => {
                let info = status(&client, &key).await?;
                let human = format!("{}'s Run: {}", info.key, info.status_text());
                out.print(&human, &info, Some(&key));
            }
            RunCommand::Log { tail } => {
                let log = log(&client, &key, tail).await?;
                out.print(&log.text, &log, Some(&key));
            }
            RunCommand::Stop => {
                let info = run_reply(
                    client
                        .request(Request::RunStop(KeyParams { key: key.clone() }))
                        .await
                        .map_err(client_error)?,
                )?;
                let human = format!("{}'s Run: {}", info.key, info.status_text());
                out.print(&human, &info, Some(&key));
            }
        }
        Ok(())
    })
}

fn run_reply(reply: Reply) -> Result<RunInfo, ErrorBody> {
    match reply {
        Reply::Run(info) => Ok(*info),
        other => Err(unexpected(&other)),
    }
}

async fn status(client: &Client, key: &str) -> Result<RunInfo, ErrorBody> {
    run_reply(
        client
            .request(Request::RunStatus(KeyParams {
                key: key.to_owned(),
            }))
            .await
            .map_err(client_error)?,
    )
}

async fn log(client: &Client, key: &str, tail: Option<u32>) -> Result<RunLog, ErrorBody> {
    match client
        .request(Request::RunLog(RunLogParams {
            key: key.to_owned(),
            tail,
        }))
        .await
        .map_err(client_error)?
    {
        Reply::RunLog(log) => Ok(*log),
        other => Err(unexpected(&other)),
    }
}

/// Follows a starting Run until it is running (its state is returned), or fails: it ended, failed to start, or is
/// still starting after `limit`. A failure's message ends with the Run's last lines.
async fn wait_until_up(
    client: &Client,
    mut info: RunInfo,
    limit: Duration,
) -> Result<RunInfo, ErrorBody> {
    let since = Instant::now();
    loop {
        match info.state {
            RunState::Running | RunState::PortInUse => return Ok(info),
            RunState::Starting if since.elapsed() < limit => {
                tokio::time::sleep(POLL).await;
                info = status(client, &info.key.to_string()).await?;
            }
            RunState::Starting => {
                let cmd = naming::COMMAND_NAME;
                let why = format!(
                    "{}'s Run is still starting after {} s: its port {} doesn't answer yet. It keeps running; \
                     check `{cmd} run log`, or stop it with `{cmd} run stop`",
                    info.key,
                    limit.as_secs(),
                    info.port.map_or_else(String::new, |p| p.to_string())
                );
                return Err(failed(client, &info, why).await);
            }
            _ => {
                let why = format!("{}'s Run {}", info.key, info.status_text());
                return Err(failed(client, &info, why).await);
            }
        }
    }
}

/// A failed `start --wait`: `why`, then the Run's last lines.
async fn failed(client: &Client, info: &RunInfo, why: String) -> ErrorBody {
    let key = info.key.to_string();
    let message = match log(client, &key, Some(FAILED_TAIL)).await {
        Ok(log) if !log.text.trim().is_empty() => {
            format!("{why}\nIts last lines:\n{}", log.text)
        }
        _ => why,
    };
    ErrorBody::new(ErrorKind::Refused, message)
}

fn port_in_use(info: &RunInfo) -> ErrorBody {
    let port = info.port.map_or_else(String::new, |p| p.to_string());
    ErrorBody::new(
        ErrorKind::Refused,
        format!(
            "{}'s Run port {port} is in use: another program is listening on it, and {} never stops it. To see \
             what it is: lsof -i :{port} (macOS/Linux) or netstat -ano | findstr {port} (Windows)",
            info.key,
            naming::PRODUCT_NAME
        ),
    )
}

/// What `start` prints: the Run's state, and where it answers once it is running.
fn started_text(info: &RunInfo) -> String {
    let mut text = format!("{}'s Run: {}", info.key, info.status_text());
    if info.state == RunState::Running {
        match info.url() {
            Some(url) => text.push_str(&format!("\n{url}")),
            None => {
                if let Some(port) = info.port {
                    text.push_str(&format!("\nport {port}"));
                }
            }
        }
    }
    text
}
