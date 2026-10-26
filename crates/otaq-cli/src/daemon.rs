//! `otaq daemon start | stop | status` (SVC-01). The service itself lives in `otaq_core::service`;
//! `start` launches this same binary as `daemon run` in the background, detached from the terminal.
//! Nothing here installs a launch agent or login item: the service runs only when started (d0055).

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use otaq_core::client::{ClientError, request_once};
use otaq_core::data_dir::DataDir;
use otaq_core::protocol::{ErrorBody, ErrorKind, Reply, Request, ServiceStatus};
use otaq_core::service::{self, ServiceConfig, ServiceError};
use otaq_core::{logging, naming};
use serde_json::json;

use crate::cli::DaemonCommand;
use crate::output::Output;

/// How long `start` and `stop` wait for the service to come up or go away.
const WAIT: Duration = Duration::from_secs(10);
const POLL: Duration = Duration::from_millis(50);

pub fn run(command: DaemonCommand, out: &Output) -> Result<(), ErrorBody> {
    let data =
        DataDir::from_env().map_err(|e| ErrorBody::new(ErrorKind::Internal, e.to_string()))?;
    match command {
        DaemonCommand::Start => start(&data, out),
        DaemonCommand::Stop => stop(&data, out),
        DaemonCommand::Status => status(&data, out),
        DaemonCommand::Run => foreground(data),
    }
}

fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("a tokio runtime")
}

/// The running service's status, or `None` when nothing listens on the socket.
fn query_status(data: &DataDir) -> Result<Option<ServiceStatus>, ErrorBody> {
    match runtime().block_on(request_once(&data.socket_path(), Request::ServiceStatus)) {
        Ok(Reply::ServiceStatus(status)) => Ok(Some(status)),
        Ok(other) => Err(unexpected(&other)),
        Err(ClientError::NotRunning(_)) => Ok(None),
        Err(e) => Err(client_error(&e)),
    }
}

fn start(data: &DataDir, out: &Output) -> Result<(), ErrorBody> {
    let (status, already) = ensure_started(data)?;
    print_started(out, &status, already);
    Ok(())
}

/// Starts the service in the background unless it runs: its status, and whether it already ran.
pub(crate) fn ensure_started(data: &DataDir) -> Result<(ServiceStatus, bool), ErrorBody> {
    if let Some(status) = query_status(data)? {
        return Ok((status, true));
    }
    let exe =
        std::env::current_exe().map_err(|e| internal(format!("can't find this program: {e}")))?;
    let mut command = Command::new(service_program(&exe));
    command
        .args(["daemon", "run"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(unix)]
    {
        // Its own process group: Ctrl+C in the terminal that ran `start` doesn't reach the service.
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    let mut child = spawn_detached(&mut command)
        .map_err(|e| internal(format!("can't start the background service: {e}")))?;

    let deadline = Instant::now() + WAIT;
    loop {
        std::thread::sleep(POLL);
        if let Some(status) = query_status(data)? {
            // Another `start` may have won the race; either way one service runs.
            let ours = status.pid == child.id();
            return Ok((status, !ours));
        }
        let exited = child.try_wait().ok().flatten();
        if exited.is_some() || Instant::now() > deadline {
            if let Some(status) = query_status(data)? {
                return Ok((status, true));
            }
            return Err(internal(format!(
                "the background service didn't start; see the logs in {}",
                data.logs_dir().display()
            )));
        }
    }
}

/// The program the service runs as: `exe` with links resolved, so a start through `~/.local/bin/otaq` runs
/// `Otaq.app/Contents/MacOS/otaq` (macOS ties a process to its bundle by its real path, d0072, d0090). `exe` itself
/// when it can't be resolved.
pub(crate) fn service_program(exe: &Path) -> PathBuf {
    exe.canonicalize().unwrap_or_else(|_| exe.to_path_buf())
}

fn print_started(out: &Output, status: &ServiceStatus, already: bool) {
    let human = if already {
        format!("Background service already running (PID {})", status.pid)
    } else {
        format!("Background service started (PID {})", status.pid)
    };
    let mut value = status_json(Some(status));
    value["already_running"] = json!(already);
    out.print(&human, &value, Some(&status.pid.to_string()));
}

fn stop(data: &DataDir, out: &Output) -> Result<(), ErrorBody> {
    let pid = match runtime().block_on(request_once(&data.socket_path(), Request::ServiceStop)) {
        Ok(Reply::Ok) => service::lock::read_pid(&data.service_lock_path()),
        Ok(other) => return Err(unexpected(&other)),
        Err(ClientError::NotRunning(_)) => {
            out.print(
                "Background service isn't running",
                &json!({"stopped": false, "running": false}),
                None,
            );
            return Ok(());
        }
        Err(e) => return Err(client_error(&e)),
    };
    // The service removes its lock file as the very last step.
    if !wait_until_gone(&data.service_lock_path()) {
        return Err(internal("the background service didn't stop in time"));
    }
    out.print(
        "Background service stopped",
        &json!({"stopped": true, "running": false, "pid": pid}),
        pid.map(|p| p.to_string()).as_deref(),
    );
    Ok(())
}

fn wait_until_gone(path: &Path) -> bool {
    let deadline = Instant::now() + WAIT;
    while path.exists() {
        if Instant::now() > deadline {
            return false;
        }
        std::thread::sleep(POLL);
    }
    true
}

fn status(data: &DataDir, out: &Output) -> Result<(), ErrorBody> {
    let status = query_status(data)?;
    let human = match &status {
        Some(s) => format!(
            "Background service: running\nPID: {}\nBuild: {}\nProtocol: {}\nUptime: {}",
            s.pid,
            s.build_id,
            s.protocol_version,
            format_uptime(s.uptime_secs)
        ),
        None => "Background service: stopped".to_owned(),
    };
    let pid = status.as_ref().map(|s| s.pid.to_string());
    out.print(&human, &status_json(status.as_ref()), pid.as_deref());
    Ok(())
}

fn status_json(status: Option<&ServiceStatus>) -> serde_json::Value {
    match status {
        Some(s) => {
            let mut value = serde_json::to_value(s).expect("status serializes");
            value["running"] = json!(true);
            value
        }
        None => json!({"running": false}),
    }
}

/// `3d 4h`, `2h 5m`, `7m 12s`, `40s`.
pub fn format_uptime(secs: u64) -> String {
    let (d, h, m, s) = (secs / 86_400, secs / 3_600 % 24, secs / 60 % 60, secs % 60);
    if d > 0 {
        format!("{d}d {h}h")
    } else if h > 0 {
        format!("{h}h {m}m")
    } else if m > 0 {
        format!("{m}m {s}s")
    } else {
        format!("{s}s")
    }
}

/// `daemon run`: the service in the foreground, logging to the daily log file.
/// Gives the service the user's full `PATH` ([`otaq_core::tools::daemon_path`]): started from the app opened in
/// Finder it inherits launchd's short one, and the tools checklist, the Install terminals and the agents would miss
/// `brew`, `npm` and `~/.local/bin/claude`.
fn use_full_path() {
    let inherited = std::env::var_os("PATH").unwrap_or_default();
    let shell = std::env::var_os("SHELL")
        .filter(|s| !s.is_empty())
        .map(std::path::PathBuf::from);
    let path = otaq_core::tools::daemon_path(
        &inherited,
        shell.as_deref(),
        std::env::home_dir().as_deref(),
    );
    // SAFETY: called first in `foreground`, before the log and the runtime start any thread, so nothing else reads
    // or writes the environment at the same time.
    unsafe { std::env::set_var("PATH", path) };
}

fn foreground(data: DataDir) -> Result<(), ErrorBody> {
    use_full_path();
    let _log = logging::init(&data).map_err(|e| internal(format!("can't open the log: {e}")))?;
    tracing::info!(
        command = naming::COMMAND_NAME,
        "starting the background service"
    );
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|e| internal(e.to_string()))?;
    let serve =
        move || runtime.block_on(service::run_until_signal(ServiceConfig::for_daemon(data)));
    // Inside Otaq.app the service is the notification center's delegate (d0090): macOS may deliver a click through
    // the main queue, so the main thread runs its run loop and the service runs beside it.
    #[cfg(target_os = "macos")]
    let served = if otaq_core::attention::notification_center::in_bundle().is_some() {
        otaq_core::attention::notification_center::with_main_run_loop(serve)
    } else {
        serve()
    };
    #[cfg(not(target_os = "macos"))]
    let served = serve();
    match served {
        Ok(()) => Ok(()),
        Err(ServiceError::AlreadyRunning { pid }) => {
            tracing::info!(?pid, "another service is already running");
            Ok(())
        }
        Err(e) => {
            tracing::error!(error = %e, "service failed");
            Err(internal(e.to_string()))
        }
    }
}

fn internal(message: impl Into<String>) -> ErrorBody {
    ErrorBody::new(ErrorKind::Internal, message)
}

fn client_error(e: &ClientError) -> ErrorBody {
    match e {
        ClientError::Service(body) => body.clone(),
        ClientError::NotRunning(_) => ErrorBody::new(ErrorKind::Disconnected, e.to_string()),
        _ => internal(e.to_string()),
    }
}

fn unexpected(reply: &Reply) -> ErrorBody {
    internal(format!(
        "unexpected answer from the background service: {reply:?}"
    ))
}

/// Starts the service on its own. On Windows it gets a new process group (Ctrl+C in this console doesn't reach
/// it) and a hidden console of its own, and leaves this console's job when the job allows it, so closing the
/// terminal that ran `start` doesn't end it.
fn spawn_detached(command: &mut Command) -> std::io::Result<std::process::Child> {
    #[cfg(windows)]
    {
        use otaq_core::process::{
            CREATE_BREAKAWAY_FROM_JOB, CREATE_NEW_PROCESS_GROUP, CREATE_NO_WINDOW,
        };
        use std::os::windows::process::CommandExt;
        otaq_core::process::keep_std_handles_from_children();
        command.creation_flags(
            CREATE_NEW_PROCESS_GROUP | CREATE_NO_WINDOW | CREATE_BREAKAWAY_FROM_JOB,
        );
        if let Ok(child) = command.spawn() {
            return Ok(child);
        }
        command.creation_flags(CREATE_NEW_PROCESS_GROUP | CREATE_NO_WINDOW);
    }
    command.spawn()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn a_start_through_a_link_runs_the_real_program() {
        let dir = tempfile::tempdir().unwrap();
        let macos = dir
            .path()
            .join(otaq_core::naming::bundle_folder_name())
            .join("Contents")
            .join("MacOS");
        std::fs::create_dir_all(&macos).unwrap();
        let real = macos.join(otaq_core::naming::COMMAND_NAME);
        std::fs::write(&real, "").unwrap();
        let bin = dir.path().join("bin");
        std::fs::create_dir(&bin).unwrap();
        let link = bin.join(otaq_core::naming::COMMAND_NAME);
        std::os::unix::fs::symlink(&real, &link).unwrap();
        assert_eq!(service_program(&link), real.canonicalize().unwrap());
        assert!(
            service_program(&link).starts_with(
                dir.path()
                    .canonicalize()
                    .unwrap()
                    .join(otaq_core::naming::bundle_folder_name())
            )
        );
        let missing = bin.join("gone");
        assert_eq!(service_program(&missing), missing);
    }

    #[test]
    fn uptime_format() {
        assert_eq!(format_uptime(40), "40s");
        assert_eq!(format_uptime(7 * 60 + 12), "7m 12s");
        assert_eq!(format_uptime(2 * 3600 + 5 * 60 + 9), "2h 5m");
        assert_eq!(format_uptime(3 * 86_400 + 4 * 3600), "3d 4h");
    }
}
