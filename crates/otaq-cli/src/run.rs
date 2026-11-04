//! `otaq run …` (agents run the app): a ticket's agent starts, reads and stops the ticket's Run, the project's run command in
//! the worktree with `$PORT` = the Run port. They work on `$OTAQ_TICKET` (or `-P`/the folder's ticket for the user).
//!
//! **Apps** (monorepos): a project's run command is a list of named apps (`web`, `api`), each with its own Run and
//! port. Every command takes the app's name, `otaq run start web --wait`; it may be left out while the project has one
//! app, and with several the service refuses and names them. `status` without a name lists every app.
//!
//! - `start [<app>] [--wait] [--timeout S]`: starts the Run, or uses the one already up (whoever started it). With `--wait` it
//!   returns once the Run is running (printing the Open URL, or the port), or fails when it failed to start, exited,
//!   or is still starting after the timeout (default 300 s), printing the Run's last 40 lines. A port held by another
//!   program is an error, with or without `--wait`.
//! - `status [<app>]`: not started · starting :4020 · running :4020 (started by Dev) · stopped (how).
//! - `log [<app>] [--tail N]`: the Run's kept output as text, the last N lines (default 200). Otaq never writes it to disk
//!   (d0045): save it yourself (`otaq run log > "$OTAQ_RESOURCE_DIR/run.txt"`).
//! - `restart [<app>] [--wait] [--timeout S]`: stops the Run and starts it again with the same command and port, to run
//!   code changed since it started; waits like `start`. The user's Run stays the user's: an agent may restart it,
//!   never stop it.
//! - `stop [<app>]`: stops a Run an agent of this ticket started; one the user started is refused and stays up.
//! - `set [<app>] --command "<cmd>" [--open-url "<url>"]`: a project with no run command gets one, or a missing app
//!   (`set api --command …`). The command runs as this ticket's Run and is saved once it is running ("Set by Dev on
//!   SHOP-27"); it waits like `start --wait`. Failed to start: nothing is saved, and it prints the last lines. Refused
//!   for an app that has a run command.
//!
//! `--json` prints the Run (`otaq_core::terminals::run::RunInfo`); `log --json` `{"key", "state", "text"}`.

use std::time::{Duration, Instant};

use clap::Subcommand;
use otaq_core::board::settings::DEFAULT_APP;
use otaq_core::client::Client;
use otaq_core::data_dir::DataDir;
use otaq_core::naming;
use otaq_core::protocol::{
    ErrorBody, ErrorKind, KeyParams, Reply, Request, RunLogParams, RunParams, RunSetParams,
};
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
        /// The app, when the project has several (`web`, `api`)
        #[arg(value_name = "APP")]
        app: Option<String>,
        /// Wait until the Run is running (prints its URL or port) or failed to start (exits non-zero)
        #[arg(long)]
        wait: bool,
        /// With --wait: give up after this many seconds still starting
        #[arg(long, value_name = "SECONDS", default_value_t = 300, requires = "wait")]
        timeout: u64,
    },
    /// The Run's state: not started, starting, running :4020 (started by Dev), stopped (how); every app's without APP
    Status {
        /// The app; left out, every app of the project
        #[arg(value_name = "APP")]
        app: Option<String>,
    },
    /// The Run's output, kept in memory only: the last N lines (default 200)
    Log {
        /// The app, when the project has several
        #[arg(value_name = "APP")]
        app: Option<String>,
        /// How many lines, from the end
        #[arg(long, value_name = "N")]
        tail: Option<u32>,
    },
    /// Restart the Run to run changed code (same command and port); the user's Run stays the user's
    Restart {
        /// The app, when the project has several
        #[arg(value_name = "APP")]
        app: Option<String>,
        /// Wait until it is running again (prints its URL or port) or failed to start (exits non-zero)
        #[arg(long)]
        wait: bool,
        /// With --wait: give up after this many seconds still starting
        #[arg(long, value_name = "SECONDS", default_value_t = 300, requires = "wait")]
        timeout: u64,
    },
    /// Stop a Run an agent of this ticket started (one the user started stays running)
    Stop {
        /// The app, when the project has several
        #[arg(value_name = "APP")]
        app: Option<String>,
    },
    /// Add the project's missing run command, or a missing app: it runs as this ticket's Run and is saved once it runs
    Set {
        /// The app to add (`api`); left out, the one app of a project with none yet
        #[arg(value_name = "APP")]
        app: Option<String>,
        /// The command that starts the project; use $PORT where it listens (`pnpm dev --port $PORT`)
        #[arg(long, value_name = "CMD")]
        command: String,
        /// Where it answers, optional (`http://localhost:$PORT`)
        #[arg(long, value_name = "URL")]
        open_url: Option<String>,
        /// Give up after this many seconds still starting (nothing is saved)
        #[arg(long, value_name = "SECONDS", default_value_t = 300)]
        timeout: u64,
    },
}

/// Runs one `otaq run` command.
pub fn run(command: RunCommand, global: &GlobalArgs, out: &Output) -> Result<(), ErrorBody> {
    let data = DataDir::from_env().map_err(|e| internal(e.to_string()))?;
    service::runtime()?.block_on(async {
        let client = service::connect(&data, global).await?;
        let key = target::resolve_key(&client, &data, None, global)
            .await?
            .to_string();
        let restart = matches!(command, RunCommand::Restart { .. });
        match command {
            RunCommand::Start { app, wait, timeout }
            | RunCommand::Restart { app, wait, timeout } => {
                let params = RunParams {
                    key: key.clone(),
                    app,
                };
                let request = if restart {
                    Request::RunRestart(params)
                } else {
                    Request::RunStart(params)
                };
                let info = run_reply(client.request(request).await.map_err(client_error)?)?;
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
            RunCommand::Status { app: Some(app) } => {
                let info = status(&client, &key, &app).await?;
                out.print(&status_line(&info), &info, Some(&key));
            }
            RunCommand::Status { app: None } => {
                let all = match client
                    .request(Request::RunApps(KeyParams { key: key.clone() }))
                    .await
                    .map_err(client_error)?
                {
                    Reply::Runs(all) => all,
                    other => return Err(unexpected(&other)),
                };
                match all.as_slice() {
                    [only] => out.print(&status_line(only), only, Some(&key)),
                    _ => {
                        let mut human = format!("{key}'s Runs:");
                        for info in &all {
                            human.push_str(&format!("\n  {}: {}", info.app, info.status_text()));
                        }
                        out.print(&human, &all, Some(&key));
                    }
                }
            }
            RunCommand::Log { app, tail } => {
                let log = log(&client, &key, app, tail).await?;
                out.print(&log.text, &log, Some(&key));
            }
            RunCommand::Set {
                app,
                command,
                open_url,
                timeout,
            } => {
                let info = run_reply(
                    client
                        .request(Request::RunSet(RunSetParams {
                            key: key.clone(),
                            app,
                            command,
                            open_url,
                        }))
                        .await
                        .map_err(client_error)?,
                )?;
                let info = match wait_until_up(&client, info, Duration::from_secs(timeout)).await {
                    Ok(info) => info,
                    Err(mut e) => {
                        e.message = format!("Nothing saved: {}", e.message);
                        return Err(e);
                    }
                };
                if info.state == RunState::PortInUse {
                    return Err(port_in_use(&info));
                }
                let saved = match &info.command_set_by {
                    Some(by) if info.app == DEFAULT_APP => format!(
                        "Saved as {}'s run command ({}).",
                        info.project_name,
                        by.line()
                    ),
                    Some(by) => format!(
                        "Saved as {}'s app {} ({}).",
                        info.project_name,
                        info.app,
                        by.line()
                    ),
                    None => "Not saved: a run command was set meanwhile.".to_owned(),
                };
                out.print(
                    &format!("{saved}\n{}", started_text(&info)),
                    &info,
                    Some(&key),
                );
            }
            RunCommand::Stop { app } => {
                let info = run_reply(
                    client
                        .request(Request::RunStop(RunParams {
                            key: key.clone(),
                            app,
                        }))
                        .await
                        .map_err(client_error)?,
                )?;
                out.print(&status_line(&info), &info, Some(&key));
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

/// "SHOP-1's Run" for a project's one unnamed app; "SHOP-1's api" for a named one.
fn whose(info: &RunInfo) -> String {
    if info.app == DEFAULT_APP {
        format!("{}'s Run", info.key)
    } else {
        format!("{}'s {}", info.key, info.app)
    }
}

/// "SHOP-1's Run: running :4020 (started by Dev)".
fn status_line(info: &RunInfo) -> String {
    format!("{}: {}", whose(info), info.status_text())
}

async fn status(client: &Client, key: &str, app: &str) -> Result<RunInfo, ErrorBody> {
    run_reply(
        client
            .request(Request::RunStatus(RunParams {
                key: key.to_owned(),
                app: Some(app.to_owned()),
            }))
            .await
            .map_err(client_error)?,
    )
}

async fn log(
    client: &Client,
    key: &str,
    app: Option<String>,
    tail: Option<u32>,
) -> Result<RunLog, ErrorBody> {
    match client
        .request(Request::RunLog(RunLogParams {
            key: key.to_owned(),
            app,
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
                info = status(client, &info.key.to_string(), &info.app).await?;
            }
            RunState::Starting => {
                let cmd = naming::COMMAND_NAME;
                let app = if info.app == DEFAULT_APP {
                    String::new()
                } else {
                    format!(" {}", info.app)
                };
                let why = format!(
                    "{} is still starting after {} s: its port {} doesn't answer yet. It keeps running; check \
                     `{cmd} run log{app}`, or stop it with `{cmd} run stop{app}`",
                    whose(&info),
                    limit.as_secs(),
                    info.port.map_or_else(String::new, |p| p.to_string())
                );
                return Err(failed(client, &info, why).await);
            }
            _ => {
                let why = format!("{} {}", whose(&info), info.status_text());
                return Err(failed(client, &info, why).await);
            }
        }
    }
}

/// A failed `start --wait`: `why`, then the Run's last lines.
async fn failed(client: &Client, info: &RunInfo, why: String) -> ErrorBody {
    let key = info.key.to_string();
    let message = match log(client, &key, Some(info.app.clone()), Some(FAILED_TAIL)).await {
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
            "{} port {port} is in use: another program is listening on it, and {} never stops it. To see what it \
             is: lsof -i :{port} (macOS/Linux) or netstat -ano | findstr {port} (Windows)",
            whose(info),
            naming::PRODUCT_NAME
        ),
    )
}

/// What `start` prints: the Run's state, and where it answers once it is running.
fn started_text(info: &RunInfo) -> String {
    let mut text = status_line(info);
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
