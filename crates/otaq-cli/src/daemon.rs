//! `otaq daemon start | stop | status` (SVC-01). The service itself lives in `otaq_core::service`;
//! `start` launches this same binary as `daemon run` in the background, detached from the terminal: on Windows and
//! Linux its runtime copy (UPD-02, d0140, [`otaq_core::runtime`]), made first, so the installed program is never
//! locked; on macOS the program in `Otaq.app` (d0090).
//! Nothing here installs a launch agent or login item: the service runs only when started (d0055).

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use otaq_core::client::{ClientError, request_once};
use otaq_core::data_dir::DataDir;
use otaq_core::protocol::{ErrorBody, ErrorKind, Reply, Request, ServiceStatus};
use otaq_core::service::{self, ServiceConfig, ServiceError};
use otaq_core::store::{NewerDatabase, Store, StoreError, versions};
use otaq_core::{logging, naming};
use serde_json::json;

use crate::cli::DaemonCommand;
use crate::output::Output;

/// How long `start` waits for the service to come up: longer than the login shell's PATH may take
/// ([`otaq_core::tools::DAEMON_LOGIN_PATH_TIMEOUT`], read before the service listens), with room for a startup slowed
/// by a busy machine.
const START_WAIT: Duration = Duration::from_secs(60);
/// The variable whose seconds `start` waits instead of [`START_WAIT`] (`OTAQ_DAEMON_START_WAIT_SECS`): for tests of a
/// start that gives up.
fn start_wait_var() -> String {
    naming::env_var("DAEMON_START_WAIT_SECS")
}
/// How long `stop` waits for the service to go away.
const STOP_WAIT: Duration = Duration::from_secs(10);
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
    let (status, already) = ensure_started(data, out.is_human())?;
    print_started(out, &status, already);
    Ok(())
}

/// Starts the service in the background unless it runs: its status, and whether it already ran. `announce` says on
/// stderr that it is starting, when it does: the first start after an update migrates the database first, which can
/// take a while.
pub(crate) fn ensure_started(
    data: &DataDir,
    announce: bool,
) -> Result<(ServiceStatus, bool), ErrorBody> {
    if let Some(status) = query_status(data)? {
        return Ok((status, true));
    }
    let exe =
        std::env::current_exe().map_err(|e| internal(format!("can't find this program: {e}")))?;
    let installed = service_program(&exe);
    let copy = otaq_core::runtime::service_runs_from_copy_here()
        .then(|| runtime_copy(data, &installed))
        .flatten();
    let mut command = Command::new(copy.as_deref().unwrap_or(&installed));
    if copy.is_some()
        && let Some(dir) = installed.parent()
    {
        command.env(otaq_core::runtime::installed_dir_var(), dir);
    }
    command
        .args(["daemon", "run"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(unix)]
    {
        // A session of its own, so its own process group with no terminal: Ctrl+C in the terminal that ran `start`
        // doesn't reach the service, and a start that gives up ends the whole group. With no terminal the login shell
        // the service reads `PATH` from (`$SHELL -l -i`) can't turn on job control and leave the group, as an
        // interactive zsh does when it has one.
        use std::os::unix::process::CommandExt;
        // SAFETY: setsid is async-signal-safe; nothing else runs between fork and exec.
        unsafe {
            command.pre_exec(|| {
                rustix::process::setsid()?;
                Ok(())
            });
        }
    }
    if announce {
        eprintln!("Starting the background service…");
    }
    let mut child = match spawn_detached(&mut command) {
        Ok(child) => child,
        Err(e) => {
            // A start racing this one may have won and cleaned this build's runtime folder away.
            if let Some(status) = query_status(data)? {
                return Ok((status, true));
            }
            return Err(internal(format!("can't start the background service: {e}")));
        }
    };

    let waited = wait_for_service(data, &mut child, start_wait());
    if !matches!(waited, Ok(Some(_))) {
        // Not up, by any way out of the wait: ended if still starting, so a start that reports failure leaves no
        // service coming up behind it, which `daemon stop` couldn't reach until it listens. Its database is safe to
        // cut short: what a migration leaves to do after its SQL is marked in the database and done by the next start
        // (`Store::open`). The lock file and socket it may leave are taken over by the next start.
        end_starting(&mut child);
    }
    if let Some(started) = waited? {
        return Ok(started);
    }
    if let Some(newer) = newer_database(data) {
        return Err(newer_database_error(&newer));
    }
    Err(internal(format!(
        "the background service didn't start within {} s; see the logs in {}",
        start_wait().as_secs(),
        data.logs_dir().display()
    )))
}

/// Waits for the service `child` was launched as to answer, for up to `wait`: its status and whether another start's
/// service answered instead; `None` when `child` exited or the time ran out with none answering.
fn wait_for_service(
    data: &DataDir,
    child: &mut Child,
    wait: Duration,
) -> Result<Option<(ServiceStatus, bool)>, ErrorBody> {
    let deadline = Instant::now() + wait;
    loop {
        std::thread::sleep(POLL);
        if let Some(status) = query_status(data)? {
            // Another `start` may have won the race; either way one service runs.
            let ours = status.pid == child.id();
            return Ok(Some((status, !ours)));
        }
        let exited = child.try_wait().ok().flatten().is_some();
        if exited || Instant::now() > deadline {
            return Ok(query_status(data)?.map(|status| (status, true)));
        }
    }
}

/// Ends `child` and, on macOS and Linux, its process group (the login shell it may be reading `PATH` from), unless
/// it has exited already.
fn end_starting(child: &mut Child) {
    if !matches!(child.try_wait(), Ok(None)) {
        return;
    }
    #[cfg(unix)]
    let ended = match i32::try_from(child.id())
        .ok()
        .and_then(rustix::process::Pid::from_raw)
    {
        Some(group) => rustix::process::kill_process_group(group, rustix::process::Signal::KILL)
            .map_err(std::io::Error::from),
        None => child.kill(),
    };
    #[cfg(not(unix))]
    let ended = child.kill();
    match ended {
        Ok(()) => {
            let _ = child.wait();
        }
        Err(e) => {
            tracing::warn!(error = %e, "can't end the service that didn't start in time");
        }
    }
}

/// How long `start` waits: [`START_WAIT`], or the seconds in [`start_wait_var`] (the tests' short wait).
fn start_wait() -> Duration {
    std::env::var(start_wait_var())
        .ok()
        .and_then(|secs| secs.parse().ok())
        .map_or(START_WAIT, Duration::from_secs)
}

/// Why the service didn't start, when it's a database a newer build changed (UPD-08).
fn newer_database(data: &DataDir) -> Option<NewerDatabase> {
    runtime()
        .block_on(Store::newer_database(&data.database_path()))
        .unwrap_or_else(|e| {
            tracing::warn!(error = %e, "can't check the database's version");
            None
        })
}

/// The start's error for a database from a newer build: page 32's words, and the typed reason in `data` for the
/// app (`--json`).
pub(crate) fn newer_database_error(newer: &NewerDatabase) -> ErrorBody {
    ErrorBody {
        kind: ErrorKind::Refused,
        message: newer.message(),
        data: Some(versions::error_data(newer)),
    }
}

/// This build's runtime copy of `installed` (`otaq` and the plugin folder found next to it), made unless it is
/// there: the program the service runs as on Windows and Linux. `None` when it can't be made (no plugin folder, a
/// full disk): the service then runs from the installed program, as before.
fn runtime_copy(data: &DataDir, installed: &Path) -> Option<PathBuf> {
    let bin = installed.parent()?;
    let plugin = otaq_core::agent_tools::plugin_dir_next_to(bin)?;
    otaq_core::runtime::ensure(data, otaq_core::protocol::build_id(), bin, &plugin)
        .ok()
        .map(|folder| folder.program())
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
    match stop_service(data)? {
        Stopped::NotRunning => out.print(
            "Background service isn't running",
            &json!({"stopped": false, "running": false}),
            None,
        ),
        Stopped::Stopped { pid } => out.print(
            "Background service stopped",
            &json!({"stopped": true, "running": false, "pid": pid}),
            pid.map(|p| p.to_string()).as_deref(),
        ),
    }
    Ok(())
}

pub(crate) enum Stopped {
    NotRunning,
    Stopped { pid: Option<u32> },
}

/// Stops the service if one runs (working agents are interrupted) and waits until it has exited.
pub(crate) fn stop_service(data: &DataDir) -> Result<Stopped, ErrorBody> {
    let pid = match runtime().block_on(request_once(&data.socket_path(), Request::ServiceStop)) {
        Ok(Reply::Ok) => service::lock::read_pid(&data.service_lock_path()),
        Ok(other) => return Err(unexpected(&other)),
        Err(ClientError::NotRunning(_)) => return Ok(Stopped::NotRunning),
        Err(e) => return Err(client_error(&e)),
    };
    // The service removes its lock file as the very last step.
    if !wait_until_gone(&data.service_lock_path()) {
        return Err(internal("the background service didn't stop in time"));
    }
    Ok(Stopped::Stopped { pid })
}

fn wait_until_gone(path: &Path) -> bool {
    let deadline = Instant::now() + STOP_WAIT;
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
/// Returns it so its warning, if any, is logged once the log is open.
fn use_full_path() -> otaq_core::tools::DaemonPath {
    let inherited = std::env::var_os("PATH").unwrap_or_default();
    let shell = std::env::var_os("SHELL")
        .filter(|s| !s.is_empty())
        .map(std::path::PathBuf::from);
    let path = otaq_core::tools::daemon_path(
        &inherited,
        shell.as_deref(),
        std::env::home_dir().as_deref(),
        otaq_core::tools::DAEMON_LOGIN_PATH_TIMEOUT,
    );
    // SAFETY: called first in `foreground`, before the log and the runtime start any thread, so nothing else reads
    // or writes the environment at the same time. (The login shell's reader thread has ended or is only reading a
    // pipe.)
    unsafe { std::env::set_var("PATH", &path.path) };
    path
}

/// The installed app's folder `daemon start` passed to a service it started from the runtime copy, taken out of the
/// environment so agents and terminals don't inherit it.
fn take_installed_dir() -> Option<PathBuf> {
    let var = otaq_core::runtime::installed_dir_var();
    let dir = std::env::var_os(&var).filter(|v| !v.is_empty())?;
    // SAFETY: called in `foreground` beside `use_full_path`, before any thread starts.
    unsafe { std::env::remove_var(&var) };
    Some(PathBuf::from(dir))
}

/// Drops what an agent's session put in the environment of a service started from inside it
/// ([`otaq_core::agent_tools::parent_session_vars`]), so the agents and terminals the service starts aren't taken
/// for part of that session: Claude Code would stop saving their conversations and Resume would find nothing.
fn drop_parent_session_vars() {
    for var in otaq_core::agent_tools::parent_session_vars() {
        // SAFETY: called in `foreground` beside `use_full_path`, before any thread starts.
        unsafe { std::env::remove_var(var) };
    }
}

fn foreground(data: DataDir) -> Result<(), ErrorBody> {
    let path = use_full_path();
    drop_parent_session_vars();
    let installed = take_installed_dir();
    let _log = logging::init(&data).map_err(|e| internal(format!("can't open the log: {e}")))?;
    tracing::info!(
        command = naming::COMMAND_NAME,
        "starting the background service"
    );
    path.log_warning();
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|e| internal(e.to_string()))?;
    let serve = move || {
        runtime.block_on(service::run_until_signal(ServiceConfig::for_daemon(
            data, installed,
        )))
    };
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
        Err(ServiceError::Store(StoreError::NewerDatabase(newer))) => {
            tracing::warn!(last_opened_by = %newer.last_opened_by, "the database is from a newer build");
            Err(newer_database_error(&newer))
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
