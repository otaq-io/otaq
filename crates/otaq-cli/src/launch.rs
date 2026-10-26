//! `otaq` without a command opens the desktop app.
//!
//! Path resolution: the app binary (`naming::app_binary_name()`, `otaq-app`) is looked up **in the same
//! folder as the running `otaq` binary**. `cargo build` puts both in `target/<profile>/`, and an installed
//! build keeps them side by side. The app is started detached, so the command returns at once.
//!
//! With the app already running for the same data folder, the started copy hands over to it and exits at once, and
//! the running app's window comes forward (d0139 "Only one copy of Otaq runs", `otaq-desktop`'s `one_copy`): nothing
//! here checks for it first.

use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use otaq_core::naming;
use otaq_core::protocol::{ErrorBody, ErrorKind};

/// Where the app binary is expected for a CLI binary at `cli_exe`.
pub fn app_path_next_to(cli_exe: &Path) -> PathBuf {
    cli_exe
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(naming::app_binary_name())
}

pub fn launch_app() -> Result<(), ErrorBody> {
    let exe = std::env::current_exe().map_err(|e| internal(&e))?;
    // Through a link such as `~/.local/bin/otaq`, the app sits next to the real program (`Otaq.app`, d0072).
    let app = app_path_next_to(&crate::daemon::service_program(&exe));
    if !app.is_file() {
        return Err(ErrorBody::new(
            ErrorKind::Internal,
            format!(
                "the app isn't installed next to this command (looked for {})",
                app.display()
            ),
        ));
    }
    let mut command = Command::new(&app);
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        otaq_core::process::keep_std_handles_from_children();
        command.creation_flags(
            otaq_core::process::DETACHED_PROCESS | otaq_core::process::CREATE_NEW_PROCESS_GROUP,
        );
    }
    command.spawn().map_err(|e| internal(&e))?;
    Ok(())
}

fn internal(e: &io::Error) -> ErrorBody {
    ErrorBody::new(ErrorKind::Internal, format!("can't open the app: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_app_sits_next_to_the_command() {
        let cli = Path::new("/opt/cb/bin").join(naming::COMMAND_NAME);
        assert_eq!(
            app_path_next_to(&cli),
            Path::new("/opt/cb/bin").join(naming::app_binary_name())
        );
    }
}
