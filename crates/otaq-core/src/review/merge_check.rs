//! **The Merge check** (d0120, Q475): the project's command (`npm test`) that Local merge runs on the merged result
//! in Otaq's scratch worktree before the target moves, when the rebase brought in commits the branch didn't have
//! ([`crate::git::merge`] decides when; this module runs it).
//!
//! - **The build:** the scratch worktree is seeded copy-on-write from the ticket's worktree build, else the warm
//!   build, else nothing ([`crate::warm::seed::Seeder::seed_scratch`]; WRM-01, never a plain copy). The project's
//!   env files are copied in as a start copies them.
//! - **The environment:** the project's variables (ENV-02) and Otaq's own for the ticket (`OTAQ_TICKET`, its resource
//!   variables), through the user's shell like the setup command, in its own process group.
//! - **The output** goes to `logs/merge-check/<KEY>.log` in Otaq's data folder, replaced by every run; a failure
//!   attaches it to the hand-back comment as a report (d0109), its last 20 MB when it is bigger.
//! - **While it runs** the card says "Merging · running the merge check" ([`crate::board::merge_check`]); the user's
//!   Stop ends the command and merges nothing. One merge at a time per service: the checks of two merges into one
//!   branch never run side by side, and the second runs on the first's result.
//!
//! Nothing is written to the user's checkout: the scratch worktree is cleaned after the check, whatever happened.

use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::process::Stdio;

use crate::board::Board;
use crate::board::reports::REPORT_LIMIT_BYTES;
use crate::board::settings::ProjectSettings;
use crate::board::tickets::Ticket;
use crate::data_dir::DataDir;
use crate::domain::BuildState;
use crate::git::merge::{CheckRun, MergeCheck};
use crate::git::{Git, GitError};
use crate::store::projects::Project;
use crate::tools::Os;
use crate::warm::seed::Seeder;

/// The Merge check of `ticket`'s Local merge: `command` with the project's settings.
#[allow(clippy::too_many_arguments)]
pub(crate) fn merge_check(
    board: &Board,
    data: &DataDir,
    seeder: &Seeder,
    git: &Git,
    project: &Project,
    settings: &ProjectSettings,
    ticket: &Ticket,
    command: &str,
) -> MergeCheck {
    let (board, data, seeder, git) = (board.clone(), data.clone(), seeder.clone(), git.clone());
    let (project, settings, ticket) = (project.clone(), settings.clone(), ticket.clone());
    let command = command.to_owned();
    MergeCheck::new(move |scratch| {
        let (board, data, seeder, git) = (board.clone(), data.clone(), seeder.clone(), git.clone());
        let (project, settings, ticket) = (project.clone(), settings.clone(), ticket.clone());
        let command = command.clone();
        Box::pin(async move {
            run(
                &board, &data, &seeder, &git, &project, &settings, &ticket, &command, &scratch,
            )
            .await
        })
    })
}

#[allow(clippy::too_many_arguments)]
async fn run(
    board: &Board,
    data: &DataDir,
    seeder: &Seeder,
    git: &Git,
    project: &Project,
    settings: &ProjectSettings,
    ticket: &Ticket,
    command: &str,
    scratch: &Path,
) -> Result<CheckRun, GitError> {
    let key = ticket.key.to_string();
    let (_running, mut stopped) = board.merge_checks().begin(ticket);
    // The ticket's own build when it is in its worktree (not set aside at Review), else the warm build.
    let own_build = ticket
        .work
        .as_ref()
        .filter(|w| w.build_state == BuildState::Present)
        .and_then(|w| w.worktree.as_deref())
        .map(Path::new);
    let seeded = seeder
        .seed_scratch(board, git, project, &key, own_build, scratch)
        .await;
    let repo = PathBuf::from(&project.repo_path);
    if let Err(e) = crate::start::setup::copy_env_files(&repo, scratch, &settings.env_files) {
        tracing::warn!(ticket = %key, error = %e, "can't copy the env files for the merge check");
    }
    let spawn_error = |e: std::io::Error| GitError::Spawn {
        args: format!("merge check `{command}`"),
        error: e,
    };
    let log = data
        .merge_check_log_path(&key)
        .map_err(|e| spawn_error(std::io::Error::other(e.to_string())))?;
    if let Some(parent) = log.parent() {
        DataDir::ensure_dir(parent).map_err(spawn_error)?;
    }
    let mut file = std::fs::File::create(&log).map_err(spawn_error)?;
    writeln!(file, "$ {command}").map_err(spawn_error)?;
    let stderr = file.try_clone().map_err(spawn_error)?;
    let recorded = ticket.work.as_ref().and_then(|w| w.resource_name.clone());
    let own = crate::ticket_env::resources::command_vars(
        data,
        project,
        &ticket.key,
        recorded.as_deref(),
        settings,
    );
    let shell = crate::shell::one_line(command);
    let own: Vec<(String, String)> = own.into_iter().chain(shell.env).collect();
    let project_env = crate::session::env::applied(&settings.env_vars, Os::current());
    let mut process = tokio::process::Command::new(&shell.program);
    process
        .args(&shell.args)
        .envs(crate::session::env::layered(
            &project_env,
            &own,
            None,
            Os::current(),
        ))
        .current_dir(scratch)
        .stdin(Stdio::null())
        .stdout(Stdio::from(file))
        .stderr(Stdio::from(stderr))
        .kill_on_drop(true);
    // Its own process group, so a Stop ends what the command started too (a test runner's workers).
    #[cfg(unix)]
    process.process_group(0);
    tracing::info!(ticket = %key, %command, ?seeded, "running the merge check");
    let mut child = process.spawn().map_err(spawn_error)?;
    let group = crate::process::Group::of_leader(child.id());
    let status = tokio::select! {
        status = child.wait() => status.map_err(spawn_error)?,
        Ok(()) = &mut stopped => {
            group.kill();
            let _ = child.wait().await;
            tracing::info!(ticket = %key, "merge check stopped by the user");
            return Ok(CheckRun::Stopped);
        }
    };
    if status.success() {
        tracing::info!(ticket = %key, "merge check passed");
        return Ok(CheckRun::Passed);
    }
    tracing::info!(ticket = %key, code = ?status.code(), "merge check failed");
    Ok(CheckRun::Failed {
        exit: status.code(),
        output: log,
    })
}

/// The check's output as a report (d0109): the file itself, or its last 20 MB in a file next to it when it is bigger
/// (a runaway log keeps the failures at its end).
pub(crate) fn capped_output(output: &Path) -> std::io::Result<PathBuf> {
    let size = std::fs::metadata(output)?.len();
    if size <= REPORT_LIMIT_BYTES {
        return Ok(output.to_owned());
    }
    const NOTE: &str = "[The start of the merge check's output was cut: this is its last 20 MB.]\n";
    let keep = REPORT_LIMIT_BYTES - NOTE.len() as u64;
    let mut from = std::fs::File::open(output)?;
    from.seek(SeekFrom::Start(size - keep))?;
    let mut name = output.as_os_str().to_owned();
    name.push(".last-20MB");
    let capped = PathBuf::from(name);
    let mut to = std::fs::File::create(&capped)?;
    to.write_all(NOTE.as_bytes())?;
    std::io::copy(&mut from.take(keep), &mut to)?;
    Ok(capped)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_report_over_the_limit_keeps_its_last_20_mb() {
        let dir = tempfile::tempdir().unwrap();
        let small = dir.path().join("small.log");
        std::fs::write(&small, "FAIL a.test.ts\n").unwrap();
        assert_eq!(capped_output(&small).unwrap(), small);

        let big = dir.path().join("big.log");
        let mut file = std::fs::File::create(&big).unwrap();
        let line = vec![b'x'; 1024 * 1024];
        for _ in 0..21 {
            file.write_all(&line).unwrap();
        }
        file.write_all(b"\nFAIL the end\n").unwrap();
        drop(file);
        let capped = capped_output(&big).unwrap();
        assert_ne!(capped, big);
        let text = std::fs::read(&capped).unwrap();
        assert_eq!(text.len() as u64, REPORT_LIMIT_BYTES);
        assert!(text.starts_with(b"[The start of the merge check's output was cut"));
        assert!(text.ends_with(b"FAIL the end\n"));
    }
}
