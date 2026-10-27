//! `move_app.*`: the Move Otaq to Applications window (T-0579; UPD-06; page 32 item 6). `move_app.view` is `null`
//! when the app runs from an Applications folder (the page then shows the board), else the window. `move_app.move`
//! stops the service of an older copy it replaces, moves the bundle ([`crate::install_place`]), opens the moved copy
//! and answers `{"moved": true}`; the page then quits the app. A failed move answers the window in its failure state.
//! `move_app.show_in_finder` shows the running bundle in Finder.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use otaq_core::client::{ClientError, request_once};
use otaq_core::data_dir::DataDir;
use otaq_core::protocol::{Reply, Request};
use otaq_view::view_models::move_app::{self, Failure};
use serde::Serialize;
use serde_json::Value;

use super::{RequestError, reply, unknown};
use crate::App;
use crate::install_place::{self, Gate};

/// How long the move waits for the replaced copy's service to exit.
const STOP_WAIT: Duration = Duration::from_secs(20);

#[derive(Serialize)]
struct Moved {
    moved: bool,
}

pub async fn request(app: &App, method: &str, _params: Value) -> Result<Value, RequestError> {
    let Gate::MoveFirst { bundle, place } = &app.install.gate else {
        return match method {
            "move_app.view" => reply(&Option::<move_app::View>::None),
            _ => Err(unknown(method)),
        };
    };
    match method {
        "move_app.view" => reply(&Some(move_app::view(place.source(), None))),
        "move_app.move" => match move_now(bundle, app.install.data.as_ref()).await {
            Ok(()) => reply(&Moved { moved: true }),
            Err(failure) => reply(&move_app::view(place.source(), Some(&failure))),
        },
        "move_app.show_in_finder" => {
            reveal(bundle).map_err(|e| RequestError::plain(e.to_string()))?;
            reply(&Value::Null)
        }
        _ => Err(unknown(method)),
    }
}

/// Moves the bundle, stopping the service of an older copy it replaces, then opens the moved copy.
async fn move_now(bundle: &Path, data: Option<&DataDir>) -> Result<(), Failure> {
    let home = std::env::home_dir();
    let roots = install_place::roots(home.as_deref());
    // The running service's program, asked before the blocking move: the service stops only when the copy it runs
    // from is replaced.
    let program = match data {
        Some(data) => service_program(data).await,
        None => None,
    };
    let (source, data, runtime) = (bundle.to_path_buf(), data.cloned(), tokio::runtime::Handle::current());
    let moved = tokio::task::spawn_blocking(move || {
        install_place::move_into(&source, &roots, &mut |target| {
            if let (Some(program), Some(data)) = (&program, &data)
                && install_place::stops_service(program.as_deref(), target)
            {
                runtime.block_on(stop_service(data));
            }
        })
    })
    .await
    .map_err(|e| Failure::Other(e.to_string()))??;
    let home_var = std::env::var_os(DataDir::env_var());
    let status = install_place::reopen_command(&moved, home_var.as_deref())
        .status()
        .map_err(|e| Failure::Other(e.to_string()))?;
    if status.success() {
        Ok(())
    } else {
        Err(Failure::Other(format!("open {} failed ({status})", moved.display())))
    }
}

/// The running service's program: `None` when no service runs; `Some(None)` when one runs but its program can't be
/// told.
async fn service_program(data: &DataDir) -> Option<Option<PathBuf>> {
    match request_once(&data.socket_path(), Request::ServiceStatus).await {
        Ok(Reply::ServiceStatus(status)) => Some(program_of(status.pid)),
        Err(ClientError::NotRunning(_)) => None,
        _ => Some(None),
    }
}

/// A process's program, from `ps` (macOS lists its full path).
fn program_of(pid: u32) -> Option<PathBuf> {
    let out = std::process::Command::new("ps")
        .args(["-o", "comm=", "-p", &pid.to_string()])
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&out.stdout).trim().to_owned();
    (out.status.success() && text.starts_with('/')).then(|| PathBuf::from(text))
}

/// Stops this data folder's service through its socket (agents interrupted, as a restart does) and waits for it to
/// exit. Never anything by process name.
async fn stop_service(data: &DataDir) {
    if let Err(e) = request_once(&data.socket_path(), Request::ServiceStop).await {
        eprintln!("couldn't stop the older copy's service: {e}");
        return;
    }
    let lock = data.service_lock_path();
    let deadline = Instant::now() + STOP_WAIT;
    while otaq_core::service::lock::service_alive(&lock) && Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

/// Shows the bundle in Finder (`open -R`): in the disk image's window, next to its Applications shortcut.
fn reveal(bundle: &Path) -> std::io::Result<()> {
    std::process::Command::new("open")
        .arg("-R")
        .arg(bundle)
        .status()
        .map(drop)
}
