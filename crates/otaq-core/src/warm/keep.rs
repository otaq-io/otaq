//! Keeping the build at Done (WRM-03, REC-05; d0082, d0085): before a clean worktree is removed, its gitignored files
//! are **moved** (renamed, never copied) into the project's stock, so a later worktree starts from them.
//!
//! - Only when warm builds are on for the project (its switch), on any disk (M19-b): where the disk can clone the
//!   stock keeps one build, forward only; where it can't, the build is added and the stock limit applied
//!   ([`Stock::add`]).
//! - The gitignored paths come from `git ls-files --others --ignored --exclude-standard --directory`: a folder is one
//!   entry, so a folder of a hundred thousand files is one rename. Every gitignored file is treated the same.
//! - The project's env files are left out, and stay in the worktree (a folder holding one moves around it): they are
//!   copied fresh at each start (SES-01 step 4). So are Otaq's skill copies (marked folders under
//!   `.agents/skills/` and `.claude/skills/`, AGM-07, and M27's `otaq-custom-*`): each session writes its own, and they
//!   are never kept or seeded (d0082).
//! - The record: built at (`HEAD`), base (`merge-base HEAD <base branch>`, for "forward only"), from (the ticket)
//!   and the time. The stock decides whether it is kept.
//! - The times file ([`TrackedTimes`]): every tracked file's modified time in the worktree (a symlink's own), so a
//!   worktree seeded from this build gives its unchanged files the times they had here (Q71): a file committed after
//!   the last build still looks newer than the build's output.
//! - **Nothing here can make Done fail.** Any failure is logged and the worktree is removed as before. A rename that
//!   fails moves back what it moved; a half-filled `.incoming` is deleted (dropping the store's
//!   [`Incoming`](super::store::Incoming) does that).

use std::fmt;
use std::path::{Component, Path, PathBuf};

use super::clone::move_tree_or_back;
use super::store::{TrackedTimes, WarmBuilds, WarmRecord};
use crate::board::Board;
use crate::domain::TicketKey;
use crate::store::projects::Project;
use crate::ticket_env::stock::{Added, Stock};

/// Why a build couldn't be kept (logged; never a reason for Done to fail).
#[derive(Debug)]
pub(crate) struct KeepError(String);

impl fmt::Display for KeepError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

fn failed(e: impl fmt::Display) -> KeepError {
    KeepError(e.to_string())
}

/// What a worktree's build is made of, read before it moves.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Collected {
    /// The gitignored paths to move (relative; a folder is one), without the env files.
    pub(crate) paths: Vec<PathBuf>,
    /// The project's env files and the workflow skills' worktree copies (relative): they stay in the worktree, also
    /// inside a moved folder.
    pub(crate) env_files: Vec<PathBuf>,
    /// The tracked files, whose times are recorded.
    pub(crate) tracked: Vec<String>,
    /// `HEAD`.
    pub(crate) built_at: String,
    /// `merge-base HEAD <base branch>`.
    pub(crate) base: String,
}

/// Keeps the gitignored files of `worktree`, the clean worktree of `ticket` about to be removed, in the project's
/// stock. Returns what the stock did with them; `None` when nothing was kept (warm builds off, nothing gitignored,
/// or a failure, which is logged).
pub(crate) async fn keep(
    board: &Board,
    warm: &WarmBuilds,
    project: &Project,
    ticket: &TicketKey,
    worktree: &Path,
) -> Option<Added> {
    let result = async {
        // Off: Done removes the worktree and its build with it, as before.
        if !warm.enabled(&project.key).await.map_err(failed)? {
            return Ok(None);
        }
        let collected = collect(board, project, worktree).await?;
        if collected.paths.is_empty() {
            tracing::debug!(ticket = %ticket, "nothing gitignored to keep");
            return Ok(None);
        }
        let stock = Stock::new(warm.clone(), board.clone());
        move_to_stock(&stock, warm, project, ticket, worktree, collected)
            .await
            .map(Some)
    }
    .await;
    match result {
        Ok(added) => added,
        Err(error) => {
            tracing::warn!(
                ticket = %ticket,
                project = %project.key,
                error = %error,
                "the build isn't kept; the worktree is removed as before"
            );
            None
        }
    }
}

/// Reads what the build in `worktree` is made of: its gitignored paths (without the env files), its tracked files,
/// its commit and its base.
pub(crate) async fn collect(
    board: &Board,
    project: &Project,
    worktree: &Path,
) -> Result<Collected, KeepError> {
    let settings = board.project_settings(&project.key).await.map_err(failed)?;
    let env_files: Vec<PathBuf> = settings
        .env_files
        .iter()
        .map(|f| normal(Path::new(f.trim())))
        .filter(|f| !f.as_os_str().is_empty())
        // Otaq's skill copies are never kept (d0082): each session writes its own (AGM-07).
        .chain(crate::skills::copy::copies_to_skip(worktree))
        .collect();
    let git = board.project_git(&project.key).await.map_err(failed)?;
    let listed = git
        .run(
            worktree,
            &[
                "ls-files",
                "-z",
                "--others",
                "--ignored",
                "--exclude-standard",
                "--directory",
            ],
        )
        .await
        .map_err(failed)?;
    let paths = ignored_paths(&listed, &env_files);
    if paths.is_empty() {
        return Ok(Collected {
            paths,
            env_files,
            tracked: Vec::new(),
            built_at: String::new(),
            base: String::new(),
        });
    }
    let built_at = git
        .run(worktree, &["rev-parse", "HEAD"])
        .await
        .map_err(failed)?
        .trim()
        .to_owned();
    let base = git
        .run(worktree, &["merge-base", "HEAD", &project.base_branch])
        .await
        .map_err(failed)?
        .trim()
        .to_owned();
    let tracked: Vec<String> = git
        .run(worktree, &["ls-files", "-z"])
        .await
        .map_err(failed)?
        .split('\0')
        .filter(|f| !f.is_empty())
        .map(str::to_owned)
        .collect();
    Ok(Collected {
        paths,
        env_files,
        tracked,
        built_at,
        base,
    })
}

/// Renames the `collected` paths of `worktree` into the project's `.incoming`, with the tracked files' times, and
/// adds them to the stock ([`Stock::add`]) as `ticket`'s build. A rename that fails moves back what it moved.
pub(crate) async fn move_to_stock(
    stock: &Stock,
    warm: &WarmBuilds,
    project: &Project,
    ticket: &TicketKey,
    worktree: &Path,
    collected: Collected,
) -> Result<Added, KeepError> {
    let Collected {
        paths,
        env_files,
        tracked,
        built_at,
        base,
    } = collected;
    // On an error from here on, dropping `incoming` deletes what is in it.
    let incoming = warm.incoming(project).await.map_err(failed)?;
    let (from, to) = (worktree.to_owned(), incoming.path().to_owned());
    tokio::task::spawn_blocking(move || -> Result<(), KeepError> {
        move_out(&from, &to, &paths, &env_files)?;
        TrackedTimes::measure(&from, &tracked)
            .write(&to)
            .map_err(|e| KeepError(format!("{}: {e}", to.display())))
    })
    .await
    .map_err(failed)??;
    let record = WarmRecord {
        built_at,
        base,
        from: ticket.to_string(),
        kept_at: crate::store::now_ms(),
        project: String::new(),
    };
    stock.add(project, incoming, record).await.map_err(failed)
}

/// Renames the `collected` paths of `worktree` into the project's `.incoming` and deletes them there (dropping the
/// [`Incoming`](super::store::Incoming) does that in the background): the Free of a ticket whose project has warm
/// builds off (d0086). A rename that fails moves back what it moved. The env files stay in the worktree.
pub(crate) async fn move_to_trash(
    warm: &WarmBuilds,
    project: &Project,
    worktree: &Path,
    collected: Collected,
) -> Result<(), KeepError> {
    let incoming = warm.incoming(project).await.map_err(failed)?;
    let (from, to) = (worktree.to_owned(), incoming.path().to_owned());
    let Collected {
        paths, env_files, ..
    } = collected;
    tokio::task::spawn_blocking(move || move_out(&from, &to, &paths, &env_files))
        .await
        .map_err(failed)??;
    drop(incoming);
    Ok(())
}

/// Renames `paths` (relative) from `from` into `to`, all or nothing, leaving the env files where they are: a folder
/// holding one is moved part by part around it ([`around_env_files`]).
fn move_out(
    from: &Path,
    to: &Path,
    paths: &[PathBuf],
    env_files: &[PathBuf],
) -> Result<(), KeepError> {
    let paths = around_env_files(paths, env_files, |dir| {
        std::fs::read_dir(from.join(dir))
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| dir.join(e.file_name()))
            .collect()
    });
    move_tree_or_back(from, to, Some(&paths)).map_err(failed)
}

/// `paths` without the env files: a path that is an env file is left out, and a folder holding one is replaced by
/// what `children` lists in it (relative paths), the same way down to the env file. Sorted.
fn around_env_files(
    paths: &[PathBuf],
    env_files: &[PathBuf],
    children: impl Fn(&Path) -> Vec<PathBuf>,
) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut todo: Vec<PathBuf> = paths.to_vec();
    while let Some(path) = todo.pop() {
        if env_files.iter().any(|e| path.starts_with(e)) {
            continue;
        }
        if env_files.iter().any(|e| e.starts_with(&path)) {
            todo.extend(children(&path));
        } else {
            out.push(path);
        }
    }
    out.sort();
    out
}

/// The paths of `git ls-files -z --others --ignored --directory` output, relative to the worktree: folders (listed
/// with a trailing `/`) as one entry, without what lies inside a folder already listed, and without the env files.
fn ignored_paths(listed: &str, env_files: &[PathBuf]) -> Vec<PathBuf> {
    let mut paths: Vec<PathBuf> = Vec::new();
    for entry in listed.split('\0').filter(|e| !e.is_empty()) {
        let path = normal(Path::new(entry));
        // A file inside a left-out folder is listed on its own when the folder has its own `.gitignore` (each skill
        // copy does): it stays too.
        if path.as_os_str().is_empty()
            || env_files.iter().any(|e| path.starts_with(e))
            || paths.iter().any(|p| path.starts_with(p))
        {
            continue;
        }
        paths.push(path);
    }
    paths
}

/// `path` with only its plain parts (`./.env.local` and `.env.local` are the same file; `out/` is `out`).
fn normal(path: &Path) -> PathBuf {
    path.components()
        .filter_map(|c| match c {
            Component::Normal(part) => Some(part),
            _ => None,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folders_are_one_entry_and_env_files_stay_out() {
        let listed = ".env.local\0out/\0sub/\0sub/y.log\0x.log\0config/.env\0";
        let env = [normal(Path::new("./.env.local"))];
        assert_eq!(
            ignored_paths(listed, &env),
            ["out", "sub", "x.log", "config/.env"]
                .iter()
                .map(PathBuf::from)
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn a_folder_holding_an_env_file_moves_around_it() {
        let tree = |dir: &Path| -> Vec<PathBuf> {
            match dir.to_str().unwrap() {
                "config" => vec![
                    "config/.env".into(),
                    "config/cache.json".into(),
                    "config/sub".into(),
                ],
                _ => vec![],
            }
        };
        let paths: Vec<PathBuf> = ["out", "config", ".env.local"]
            .iter()
            .map(PathBuf::from)
            .collect();
        let env = [PathBuf::from(".env.local"), PathBuf::from("config/.env")];
        assert_eq!(
            around_env_files(&paths, &env, tree),
            ["config/cache.json", "config/sub", "out"]
                .iter()
                .map(PathBuf::from)
                .collect::<Vec<_>>()
        );
        assert_eq!(around_env_files(&paths, &[], tree), {
            let mut sorted = paths.clone();
            sorted.sort();
            sorted
        });
    }

    #[test]
    fn the_files_of_a_left_out_folder_stay_too() {
        // Each skill copy has its own `.gitignore`, so git lists its files one by one, not the folder.
        let listed = ".claude/skills/a-copy/.gitignore\0.claude/skills/a-copy/SKILL.md\0target/\0";
        let skip = [PathBuf::from(".claude/skills/a-copy")];
        assert_eq!(ignored_paths(listed, &skip), [PathBuf::from("target")]);
        let paths = [
            PathBuf::from(".claude/skills/a-copy/SKILL.md"),
            PathBuf::from("target"),
        ];
        assert_eq!(
            around_env_files(&paths, &skip, |_| Vec::new()),
            [PathBuf::from("target")]
        );
    }

    #[test]
    fn a_sibling_with_a_longer_name_is_its_own_entry() {
        let listed = "out/\0out-old/\0";
        assert_eq!(
            ignored_paths(listed, &[]),
            [PathBuf::from("out"), PathBuf::from("out-old")]
        );
    }
}
