//! `otaq uninstall [--keep-data]` (UNI-01…UNI-06, d0134): removes what Otaq left on this computer. It asks both
//! questions before anything changes (the worktrees, then the data folder); then the service removes the worktrees,
//! the merge worktrees and the skill copies and stops; then this command deletes the data folder, once the service
//! has let go of its lock, and prints what was removed and what was kept on purpose (the `otaq/` branches, the
//! folder trust). The scan and the removal are service requests: this command never opens the database.
//!
//! **The guard:** from a development build, `cargo run` or a test (`cargo test`, nextest), the command refuses to run
//! unless `OTAQ_HOME` names the data folder, so it can never reach the real `~/.otaq` by default.

use std::io::{BufRead, IsTerminal, Write};
use std::path::Path;
use std::process::ExitCode;
use std::time::Duration;

use otaq_core::board::reports::size_label;
use otaq_core::data_dir::DataDir;
use otaq_core::naming;
use otaq_core::protocol::{ErrorBody, ErrorKind, Reply, Request, UninstallRemoveParams};
use otaq_core::uninstall::{
    StepKind, UninstallOutcome, UninstallScan, UninstallStep, UninstallWorktree, WorktreeOwnerKind,
};

use crate::cli::GlobalArgs;
use crate::output::{EXIT_ERROR, Output};
use crate::service::{client_error, connect, internal, runtime};

/// How long the data folder's delete waits for the stopping service to let go of its lock.
const SERVICE_EXIT_WAIT: Duration = Duration::from_secs(30);

pub fn run(keep_data: bool, global: &GlobalArgs, out: &Output) -> ExitCode {
    match uninstall(keep_data, global) {
        Ok(Ended::Done) => ExitCode::SUCCESS,
        Ok(Ended::Declined(text)) | Ok(Ended::Failed(text)) => {
            println!("{text}");
            ExitCode::from(EXIT_ERROR)
        }
        Err(error) => out.error(&error, EXIT_ERROR),
    }
}

enum Ended {
    Done,
    /// The user answered no: nothing changed.
    Declined(String),
    /// The summary, with something that failed.
    Failed(String),
}

fn refused(message: impl Into<String>) -> ErrorBody {
    ErrorBody::new(ErrorKind::Refused, message)
}

/// Why the command can't run here: `OTAQ_HOME` unset in a development build or under cargo or a test runner.
fn guard(env: &dyn Fn(&str) -> Option<std::ffi::OsString>, debug_build: bool) -> Option<String> {
    let set = |name: &str| env(name).is_some_and(|v| !v.is_empty());
    if set(&DataDir::env_var()) {
        return None;
    }
    let under_cargo = set("CARGO_MANIFEST_DIR") || set("NEXTEST");
    (debug_build || under_cargo).then(|| {
        format!(
            "{} uninstall needs {} set when run from a development build or a test; set it to the data folder \
             to remove",
            naming::COMMAND_NAME,
            DataDir::env_var()
        )
    })
}

fn uninstall(keep_data: bool, global: &GlobalArgs) -> Result<Ended, ErrorBody> {
    if let Some(why) = guard(&|name| std::env::var_os(name), cfg!(debug_assertions)) {
        return Err(refused(why));
    }
    // Refusals that need nothing: no service is started for them.
    let in_terminal = format!("Run {} uninstall in a terminal", naming::COMMAND_NAME);
    if global.json || !std::io::stdin().is_terminal() {
        return Err(refused(in_terminal));
    }
    let data = DataDir::from_env().map_err(|e| internal(e.to_string()))?;
    crate::daemon::ensure_started(&data)?;
    let rt = runtime()?;
    let client = rt.block_on(connect(&data, global))?;
    let request = |request: Request| rt.block_on(client.request(request)).map_err(client_error);
    let scan = match request(Request::UninstallScan)? {
        Reply::UninstallScan(scan) => *scan,
        other => return Err(internal(format!("unexpected answer: {other:?}"))),
    };
    if scan.app_open {
        return Err(refused(format!(
            "Quit {} from its menu bar / tray icon first: the app would start the service again",
            naming::PRODUCT_NAME
        )));
    }
    let home = std::env::home_dir();
    let folder = shown_path(Path::new(&scan.data_folder), home.as_deref());

    // Both questions, before anything changes.
    let listed = listed_worktrees(&scan);
    let worktrees = !listed.is_empty();
    if worktrees {
        println!("{}", worktree_list(&listed));
        if !ask(&worktree_question(&listed))? {
            return Ok(Ended::Declined(
                "Nothing changed. Move them on or clear them first (Done, Close, the Cleanup view)"
                    .into(),
            ));
        }
    }
    let kept_size = otaq_core::uninstall::kept_size(&data);
    if !ask(&folder_question(&scan, &folder, keep_data, kept_size))? {
        return Ok(Ended::Declined("Nothing changed.".into()));
    }

    let outcome = match request(Request::UninstallRemove(UninstallRemoveParams {
        worktrees,
    }))? {
        Reply::UninstallOutcome(outcome) => *outcome,
        other => return Err(internal(format!("unexpected answer: {other:?}"))),
    };
    match request(Request::ServiceStop) {
        Ok(_) => {}
        // The service may close the connection before its answer arrives.
        Err(e) if e.kind == ErrorKind::Disconnected => {}
        Err(e) => return Err(e),
    }
    drop(client);
    drop(rt);
    let deleted = otaq_core::uninstall::delete_data_folder(&data, keep_data, SERVICE_EXIT_WAIT);
    let (text, failed) = summary(&scan, &outcome, &deleted, &folder, keep_data);
    Ok(if failed {
        Ended::Failed(text)
    } else {
        println!("{text}");
        Ended::Done
    })
}

/// Asks a yes/no question on the terminal; only `y` or `yes` (any case) is yes.
fn ask(question: &str) -> Result<bool, ErrorBody> {
    print!("{question} [y/N] ");
    std::io::stdout()
        .flush()
        .map_err(|e| internal(e.to_string()))?;
    let mut line = String::new();
    std::io::stdin()
        .lock()
        .read_line(&mut line)
        .map_err(|e| internal(e.to_string()))?;
    Ok(is_yes(&line))
}

fn is_yes(answer: &str) -> bool {
    matches!(answer.trim().to_lowercase().as_str(), "y" | "yes")
}

/// `~/.otaq` for a folder under the home folder, else the path as it is.
fn shown_path(path: &Path, home: Option<&Path>) -> String {
    match home.and_then(|h| path.strip_prefix(h).ok()) {
        Some(rest) if !rest.as_os_str().is_empty() => format!("~/{}", rest.display()),
        _ => path.display().to_string(),
    }
}

/// A line of the worktrees' list: what it is, its repository, its folder.
struct Listed {
    dirty: bool,
    label: String,
    repo: String,
    path: String,
}

/// What the worktrees' question is about (UNI-03): every folder under `wt/`, and each ✎ merge worktree.
fn listed_worktrees(scan: &UninstallScan) -> Vec<Listed> {
    let folder_name = |w: &UninstallWorktree| {
        Path::new(&w.path)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default()
    };
    scan.worktrees
        .iter()
        .map(|w| Listed {
            dirty: w.dirty,
            label: match (w.owner, &w.ticket) {
                (WorktreeOwnerKind::Helper, _) => format!("{} (helper)", folder_name(w)),
                (WorktreeOwnerKind::None, _) | (_, None) => {
                    format!("{} (no ticket)", folder_name(w))
                }
                (_, Some(key)) => key.to_string(),
            },
            repo: w.repo.clone(),
            path: w.path.clone(),
        })
        .chain(
            scan.merge_worktrees
                .iter()
                .filter(|m| m.dirty)
                .map(|m| Listed {
                    dirty: true,
                    label: "merge worktree".into(),
                    repo: m.repo.clone(),
                    path: m.path.clone(),
                }),
        )
        .collect()
}

fn worktree_list(listed: &[Listed]) -> String {
    listed
        .iter()
        .map(|l| {
            format!(
                "  {} {} · {} · {}",
                if l.dirty { "✎" } else { " " },
                l.label,
                l.repo,
                l.path
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// "Delete these 3 worktrees? Uncommitted changes in the 2 marked ✎ are lost. Their branches are kept." (UNI-03)
fn worktree_question(listed: &[Listed]) -> String {
    let dirty = listed.iter().filter(|l| l.dirty).count();
    if listed.len() == 1 {
        let lost = if dirty == 1 {
            " Its uncommitted changes are lost."
        } else {
            ""
        };
        return format!("Delete this worktree?{lost} Its branch is kept.");
    }
    let lost = match dirty {
        0 => String::new(),
        n => format!(" Uncommitted changes in the {n} marked ✎ are lost."),
    };
    format!(
        "Delete these {} worktrees?{lost} Their branches are kept.",
        listed.len()
    )
}

fn count(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

/// "Delete ~/.otaq (6.4 GB: 3 projects, 41 tickets, warm builds 5.9 GB)? 2 agents and 1 conversation with Noyan
/// end." (UNI-04)
fn folder_question(scan: &UninstallScan, folder: &str, keep_data: bool, kept_size: u64) -> String {
    let what = if keep_data {
        format!(
            "Delete everything in {folder} but the database and comment reports ({})?",
            size_label(scan.data_size_bytes.saturating_sub(kept_size))
        )
    } else {
        let mut parts = vec![
            count(scan.projects.len(), "project", "projects"),
            count(scan.tickets as usize, "ticket", "tickets"),
        ];
        if scan.warm_size_bytes > 0 {
            parts.push(format!("warm builds {}", size_label(scan.warm_size_bytes)));
        }
        format!(
            "Delete {folder} ({}: {})?",
            size_label(scan.data_size_bytes),
            parts.join(", ")
        )
    };
    format!("{what}{}", ending(scan))
}

/// " 2 agents and 1 conversation with Noyan end.", or nothing when none runs.
fn ending(scan: &UninstallScan) -> String {
    let (agents, noyan) = (
        scan.running.agents as usize,
        scan.running.noyan_conversations as usize,
    );
    let mut parts = Vec::new();
    if agents > 0 {
        parts.push(count(agents, "agent", "agents"));
    }
    if noyan > 0 {
        parts.push(format!(
            "{} with Noyan",
            count(noyan, "conversation", "conversations")
        ));
    }
    if parts.is_empty() {
        return String::new();
    }
    let verb = if agents + noyan == 1 { "ends" } else { "end" };
    format!(" {} {verb}.", parts.join(" and "))
}

/// "SHOP and PAY", "SHOP, PAY and WEB".
fn and_list(items: &[String]) -> String {
    match items {
        [] => String::new(),
        [one] => one.clone(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

/// The summary (UNI-06) and whether anything failed.
fn summary(
    scan: &UninstallScan,
    outcome: &UninstallOutcome,
    deleted: &Result<otaq_core::uninstall::FolderDeleted, String>,
    folder: &str,
    keep_data: bool,
) -> (String, bool) {
    let mut lines = Vec::new();
    let of =
        |kind: StepKind, steps: &[UninstallStep]| steps.iter().filter(|s| s.kind == kind).count();

    // Removed.
    let mut removed = Vec::new();
    let worktrees = of(StepKind::Worktree, &outcome.removed);
    if worktrees > 0 {
        removed.push(count(worktrees, "worktree", "worktrees"));
    }
    let merges = of(StepKind::MergeWorktree, &outcome.removed);
    if merges > 0 {
        removed.push(count(merges, "merge worktree", "merge worktrees"));
    }
    let copies = of(StepKind::SkillCopy, &outcome.removed);
    if copies > 0 {
        removed.push(count(copies, "skill copy", "skill copies"));
    }
    let resources = of(StepKind::Resources, &outcome.removed);
    if resources > 0 {
        removed.push(format!(
            "the resources of {}",
            count(resources, "ticket", "tickets")
        ));
    }
    lines.push("Removed:".to_owned());
    if !removed.is_empty() {
        lines.push(format!("  {}", removed.join(", ")));
    }
    if let Ok(deleted) = deleted {
        // Freed by the whole run: the worktrees went before the folder.
        let freed = scan.data_size_bytes.saturating_sub(deleted.left_bytes);
        lines.push(if keep_data {
            format!(
                "  Everything in {folder} but the database and comment reports ({} freed)",
                size_label(freed)
            )
        } else {
            format!("  {folder} ({} freed)", size_label(freed))
        });
    }

    // Kept on purpose.
    lines.push("Kept on purpose:".to_owned());
    let mut groups: Vec<(Vec<&str>, &Vec<String>)> = Vec::new();
    for trust in &scan.trust {
        match groups
            .iter_mut()
            .find(|(_, projects)| **projects == trust.projects)
        {
            Some((tools, _)) => tools.push(trust.tool.display_name()),
            None => groups.push((vec![trust.tool.display_name()], &trust.projects)),
        }
    }
    for (tools, projects) in groups {
        let verb = if projects.len() == 1 { "stays" } else { "stay" };
        lines.push(format!(
            "  {}: {} {verb} trusted",
            tools.join(", "),
            and_list(projects)
        ));
    }
    let prefix = naming::branch_prefix();
    for branches in scan.branches.iter().filter(|b| b.count > 0) {
        let word = if branches.count == 1 {
            "branch"
        } else {
            "branches"
        };
        lines.push(format!(
            "  {}: {} {prefix} {word} kept · git branch --list '{prefix}*'",
            branches.repo, branches.count
        ));
    }
    if keep_data && deleted.is_ok() {
        lines.push(format!("  The database and comment reports in {folder}"));
    }

    // Skipped and failed.
    if !outcome.skipped.is_empty() {
        lines.push("Skipped:".to_owned());
        for step in &outcome.skipped {
            lines.push(format!(
                "  {}: {}",
                step.path,
                step.message.as_deref().unwrap_or_default()
            ));
        }
    }
    let mut failures: Vec<String> = outcome
        .failed
        .iter()
        .map(|step| match step.kind {
            StepKind::Resources => format!(
                "  {}'s resources {}: {}",
                step.ticket
                    .as_ref()
                    .map(ToString::to_string)
                    .unwrap_or_default(),
                step.path,
                step.message.as_deref().unwrap_or_default()
            ),
            _ => format!(
                "  {}: {}",
                step.path,
                step.message.as_deref().unwrap_or_default()
            ),
        })
        .collect();
    match deleted {
        Ok(deleted) => failures.extend(
            deleted
                .failed
                .iter()
                .map(|(path, why)| format!("  {}: {why}", path.display())),
        ),
        Err(why) => failures.push(format!("  {}: {why}", scan.data_folder)),
    }
    let failed = !failures.is_empty();
    if failed {
        lines.push("Failed:".to_owned());
        lines.extend(failures);
    }
    lines.push(format!("Drag {}.app to the Trash", naming::PRODUCT_NAME));
    (lines.join("\n"), failed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsString;

    fn env<'a>(vars: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<OsString> + 'a {
        move |name| {
            vars.iter()
                .find(|(n, _)| *n == name)
                .map(|(_, v)| OsString::from(v))
        }
    }

    #[test]
    fn the_guard_needs_the_data_folder_under_cargo_and_in_debug_builds() {
        let home = DataDir::env_var();
        assert!(guard(&env(&[]), true).is_some());
        assert!(guard(&env(&[("NEXTEST", "1")]), false).is_some());
        assert!(guard(&env(&[("CARGO_MANIFEST_DIR", "/x")]), false).is_some());
        assert!(
            guard(&env(&[(&home, "")]), true).is_some(),
            "empty is unset"
        );
        assert!(guard(&env(&[(&home, "/tmp/m47"), ("NEXTEST", "1")]), true).is_none());
        // The installed app: a release build, outside cargo.
        assert!(guard(&env(&[]), false).is_none());
    }

    #[test]
    fn only_y_and_yes_are_yes() {
        for yes in ["y\n", "Y\n", "yes\n", "YES\r\n", " Yes "] {
            assert!(is_yes(yes), "{yes:?}");
        }
        for no in ["\n", "", "n\n", "no\n", "yep\n", "sure\n", "y es\n"] {
            assert!(!is_yes(no), "{no:?}");
        }
    }

    #[test]
    fn paths_under_home_are_shown_with_a_tilde() {
        let home = Path::new("/Users/me");
        assert_eq!(
            shown_path(Path::new("/Users/me/.data"), Some(home)),
            "~/.data"
        );
        assert_eq!(shown_path(Path::new("/tmp/m47"), Some(home)), "/tmp/m47");
        assert_eq!(shown_path(home, Some(home)), "/Users/me");
    }

    #[test]
    fn lists_read_naturally() {
        let names = |n: &[&str]| n.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(and_list(&names(&["SHOP"])), "SHOP");
        assert_eq!(and_list(&names(&["SHOP", "PAY"])), "SHOP and PAY");
        assert_eq!(
            and_list(&names(&["SHOP", "PAY", "WEB"])),
            "SHOP, PAY and WEB"
        );
    }
}
