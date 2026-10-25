//! A ticket's **Files** tab (FTB-01…FTB-04; d0017 as refined 2026-10-23, d0087, d0010, d0059): the
//! tree of what the ticket works on, its files by name, one file as it is, one file's diff and all its changes.
//!
//! **What is shown** ([`FilesFrom`]):
//! - A ticket whose worktree is on disk: **the worktree**, uncommitted edits and untracked files included (and
//!   marked), against where the ticket left its base (`base_sha`, as the Files tab measured it).
//! - A ticket that started but whose worktree is gone (Done): its **branch**, from git objects.
//! - A ticket that never started (Backlog, Ready for Dev): its **base branch**, flagged, with no changes.
//!
//! A file's [`PieceTag`]s name the pieces of the ticket's plan that changed it (d0133); none until helpers exist.
//!
//! **Read-only, and never the user's checkout** (d0087): files on disk are read only inside a ticket's worktree;
//! everything else comes from git objects of the project's repository (`ls-tree`, `cat-file`, `diff` between
//! commits). Paths are confined: relative, no `.`/`..`/`.git` parts, and a file on disk must resolve inside the
//! worktree (a symlink out is refused). Diffs and reads keep the Files tab's limits ([`MAX_FILE_LINES`],
//! [`MAX_READ_BYTES`], binary files by size).

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};
use tokio::io::AsyncReadExt;

use super::diff::short_sha;
use super::tickets::Ticket;
use super::{Board, BoardError, Refusal};
use crate::domain::TicketKey;
use crate::git::diff::{
    ChangedFile, DIFF_CONTEXT, DiffSpec, FileDiff, FileStatus, MAX_FILE_LINES, MAX_READ_BYTES,
    looks_binary, new_file_diff,
};
use crate::git::{Git, GitError};

/// The most matches `files.search` returns; [`FilesSearch::total`] counts them all.
pub const SEARCH_LIMIT: usize = 50;

/// Where the Files tab reads from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FilesFrom {
    /// The ticket's worktree on disk: "SHOP-22's worktree", uncommitted edits included.
    Worktree,
    /// The ticket's branch, from git (its worktree is gone).
    Branch,
    /// The ticket never started: its base branch, labelled so (FTB-01).
    Base,
}

/// What a Files reply shows: "SHOP-22's worktree · otaq/SHOP-22 · from main".
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FilesSource {
    pub key: TicketKey,
    pub from: FilesFrom,
    /// The branch shown: `otaq/SHOP-22`, or the base branch itself for [`FilesFrom::Base`].
    pub branch: String,
    /// What the changes are against: `main`.
    pub base_branch: String,
    /// Where the ticket left its base; `None` before it started.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_sha: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_short: Option<String>,
}

/// How a file changed, for its mark in the tree and the search: "M +4 −1".
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileChange {
    pub status: FileStatus,
    pub added: u32,
    pub removed: u32,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub binary: bool,
    /// Some of the change isn't committed yet (edited on disk, staged, or untracked).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub uncommitted: bool,
}

/// How a piece touched the ticket's file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TagKind {
    /// Its helper's work was merged into the ticket's branch and changed the file ("40.1").
    Merged,
    /// Its helper is working and has changed the file in its own worktree ("40.2 ✎").
    Writing,
}

/// A file's tag: the piece of the ticket's plan that changed it (d0133). Empty until helpers exist.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PieceTag {
    /// The piece's number within its plan: 1 for "40.1".
    pub piece: u32,
    pub kind: TagKind,
}

/// What a tree entry is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntryKind {
    Folder,
    File,
    /// A symbolic link (git stores its target as the content).
    Link,
    /// A submodule: shown, never opened.
    Submodule,
}

/// One folder's entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TreeEntry {
    /// `middleware.ts`.
    pub name: String,
    /// `src/middleware.ts`: what `files.read`, `files.diff` and a folder's `files.tree` take.
    pub path: String,
    pub kind: EntryKind,
    /// Files: how it changed; `None` when it didn't.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub change: Option<FileChange>,
    /// Folders: how many changed files are inside, at any depth.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub changed_below: u32,
    /// The pieces that changed the file, merged ones first.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<PieceTag>,
}

/// `files.tree`: one folder's entries, folders first, then by name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FilesTree {
    pub source: FilesSource,
    /// The folder: empty for the top.
    pub path: String,
    pub entries: Vec<TreeEntry>,
    /// The changed files in the whole tree: "Changed · 3".
    pub changed: u32,
}

/// One search match.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileMatch {
    pub path: String,
    /// `login.spec.ts`.
    pub name: String,
    /// `e2e`: the folder it is in, empty at the top.
    pub folder: String,
    /// The matched characters in `path`, as `[start, end)` character offsets.
    pub hit: [u32; 2],
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub change: Option<FileChange>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<PieceTag>,
}

/// `files.search`: file names across the whole tree, best first (FTB-02).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FilesSearch {
    pub source: FilesSource,
    pub query: String,
    /// At most [`SEARCH_LIMIT`].
    pub matches: Vec<FileMatch>,
    /// Every match, before the cut.
    pub total: u32,
}

/// One changed file of All changes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChangedEntry {
    pub file: ChangedFile,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub uncommitted: bool,
    /// The pieces that changed it.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<PieceTag>,
}

/// `files.changes`: every changed file, by path, without hunks; each file's diff comes from `files.diff`
/// (FTB-04: "3 files · +13 −5 against main").
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FilesChanges {
    pub source: FilesSource,
    pub files: Vec<ChangedEntry>,
    /// The sums over the ticket's files.
    pub added: u32,
    pub removed: u32,
}

/// `files.read`: one file as it is (FTB-03).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileContent {
    pub source: FilesSource,
    pub path: String,
    /// The whole file's size in bytes.
    pub size: u64,
    /// Binary files carry no text.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub binary: bool,
    /// The text, cut after [`MAX_FILE_LINES`] lines or [`MAX_READ_BYTES`] bytes.
    pub text: String,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub truncated: bool,
    /// "+4 −1 against main"; `None`: "not changed".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub change: Option<FileChange>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<PieceTag>,
}

fn is_zero(n: &u32) -> bool {
    *n == 0
}

/// Where a ticket's files come from (the project Files view reads its owners' the same way).
pub(super) struct Source {
    pub(super) info: FilesSource,
    /// Where git runs: the worktree for [`FilesFrom::Worktree`], else the project's repository (objects only).
    pub(super) cwd: PathBuf,
    /// The revision whose tree is listed: `HEAD` in the worktree, `refs/heads/<branch>` otherwise.
    pub(super) rev: String,
    /// The worktree on disk.
    pub(super) worktree: Option<PathBuf>,
    /// Worktrees: the commit the changes count from (where the ticket left `base_sha`).
    pub(super) from: Option<String>,
}

/// Every change of a source, by path, with the pieces' tags.
pub(super) struct Changes {
    pub(super) entries: Vec<ChangedEntry>,
    by_path: BTreeMap<String, (Option<FileChange>, Vec<PieceTag>)>,
}

impl Changes {
    fn new(entries: Vec<ChangedEntry>, mut tags: HashMap<String, Vec<PieceTag>>) -> Self {
        let mut by_path: BTreeMap<String, (Option<FileChange>, Vec<PieceTag>)> = BTreeMap::new();
        for entry in &entries {
            by_path.entry(entry.file.path.clone()).or_default().0 = Some(file_change(entry));
        }
        for (path, tags) in tags.drain() {
            by_path.entry(path).or_default().1 = tags;
        }
        Self { entries, by_path }
    }

    fn change(&self, path: &str) -> (Option<FileChange>, Vec<PieceTag>) {
        self.by_path.get(path).cloned().unwrap_or_default()
    }

    /// The changed files (a path tagged but not changed in the end doesn't count).
    fn changed_paths(&self) -> impl Iterator<Item = &String> {
        self.by_path
            .iter()
            .filter(|(_, (change, _))| change.is_some())
            .map(|(path, _)| path)
    }
}

pub(super) fn file_change(entry: &ChangedEntry) -> FileChange {
    FileChange {
        status: entry.file.status,
        added: entry.file.added,
        removed: entry.file.removed,
        binary: entry.file.binary,
        uncommitted: entry.uncommitted,
    }
}

impl Board {
    /// `files.tree`: the entries of `folder` (empty: the top).
    pub async fn files_tree(&self, key: &str, folder: &str) -> Result<FilesTree, BoardError> {
        let folder = clean_path(folder, true)?;
        let ticket = self.ticket(key).await?;
        let source = self.files_source(&ticket).await?;
        let changes = self.files_changes_of(&source).await?;
        let git = self.git()?;

        let mut entries = match list_folder(git, &source.cwd, &source.rev, &folder).await {
            Ok(entries) => entries,
            // A worktree with no commit of its own yet has a HEAD all the same; an unborn one lists nothing.
            Err(GitError::Failed { .. }) if source.info.from == FilesFrom::Worktree => {
                BTreeMap::new()
            }
            Err(e) => return Err(e.into()),
        };
        // Files git doesn't list at this revision: untracked or staged ones, deleted ones.
        let mut extra: Vec<&String> = changes.by_path.keys().collect();
        let untracked = match &source.worktree {
            Some(wt) if source.info.from == FilesFrom::Worktree => git.untracked_files(wt).await?,
            _ => Vec::new(),
        };
        extra.extend(untracked.iter());
        for path in extra {
            let Some(rest) = under(path, &folder) else {
                continue;
            };
            match rest.split_once('/') {
                Some((name, _)) => {
                    entries
                        .entry(join_path(&folder, name))
                        .or_insert(EntryKind::Folder);
                }
                None => {
                    entries.entry(path.clone()).or_insert(EntryKind::File);
                }
            }
        }
        if entries.is_empty() && !folder.is_empty() {
            return Err(BoardError::NotFound(format!(
                "{folder} isn't a folder in {}",
                ticket.key
            )));
        }

        let mut list: Vec<TreeEntry> = entries
            .into_iter()
            .map(|(path, kind)| {
                let name = path.rsplit('/').next().unwrap_or(&path).to_owned();
                let (change, tags, changed_below) = if kind == EntryKind::Folder {
                    let inside = changes
                        .changed_paths()
                        .filter(|p| under(p, &path).is_some())
                        .count();
                    (None, Vec::new(), u32::try_from(inside).unwrap_or(u32::MAX))
                } else {
                    let (change, tags) = changes.change(&path);
                    (change, tags, 0)
                };
                TreeEntry {
                    name,
                    path,
                    kind,
                    change,
                    changed_below,
                    tags,
                }
            })
            .collect();
        list.sort_by(|a, b| entry_order((a.kind, &a.name), (b.kind, &b.name)));
        Ok(FilesTree {
            changed: u32::try_from(changes.changed_paths().count()).unwrap_or(u32::MAX),
            source: source.info,
            path: folder,
            entries: list,
        })
    }

    /// `files.search`: file names matching `query` (ignoring case) across the whole tree, best first: the name
    /// starts with it, then a folder of the path does, then the name contains it, then the path does; changed
    /// files before unchanged ones, shorter paths first.
    pub async fn files_search(&self, key: &str, query: &str) -> Result<FilesSearch, BoardError> {
        let ticket = self.ticket(key).await?;
        let source = self.files_source(&ticket).await?;
        let query = query.trim().to_owned();
        if query.is_empty() {
            return Ok(FilesSearch {
                source: source.info,
                query,
                matches: Vec::new(),
                total: 0,
            });
        }
        let changes = self.files_changes_of(&source).await?;
        let git = self.git()?;
        let mut paths: HashSet<String> = HashSet::new();
        match (&source.worktree, source.info.from) {
            (Some(wt), FilesFrom::Worktree) => {
                let tracked = git
                    .run(wt, &["--no-optional-locks", "ls-files", "-z"])
                    .await?;
                paths.extend(nul_paths(&tracked));
                paths.extend(git.untracked_files(wt).await?);
            }
            _ => {
                let all = git
                    .run(
                        &source.cwd,
                        &["ls-tree", "-r", "--name-only", "-z", &source.rev],
                    )
                    .await?;
                paths.extend(nul_paths(&all));
            }
        }
        paths.extend(changes.by_path.keys().cloned());

        let needle = query.to_lowercase();
        let mut ranked: Vec<(u8, bool, usize, String, [u32; 2])> = paths
            .into_iter()
            .filter_map(|path| {
                let (rank, hit) = rank(&path, &needle)?;
                let unchanged = changes.change(&path).0.is_none();
                Some((rank, unchanged, path.chars().count(), path, hit))
            })
            .collect();
        ranked.sort();
        let total = u32::try_from(ranked.len()).unwrap_or(u32::MAX);
        let matches = ranked
            .into_iter()
            .take(SEARCH_LIMIT)
            .map(|(_, _, _, path, hit)| {
                let (folder, name) = match path.rsplit_once('/') {
                    Some((folder, name)) => (folder.to_owned(), name.to_owned()),
                    None => (String::new(), path.clone()),
                };
                let (change, tags) = changes.change(&path);
                FileMatch {
                    path,
                    name,
                    folder,
                    hit,
                    change,
                    tags,
                }
            })
            .collect();
        Ok(FilesSearch {
            source: source.info,
            query,
            matches,
            total,
        })
    }

    /// `files.changes`: every changed file against the base, uncommitted ones included; no hunks.
    pub async fn files_changes(&self, key: &str) -> Result<FilesChanges, BoardError> {
        let ticket = self.ticket(key).await?;
        let source = self.files_source(&ticket).await?;
        let changes = self.files_changes_of(&source).await?;
        let (added, removed) = changes.entries.iter().fold((0u32, 0u32), |(a, r), e| {
            (
                a.saturating_add(e.file.added),
                r.saturating_add(e.file.removed),
            )
        });
        Ok(FilesChanges {
            source: source.info,
            files: changes.entries,
            added,
            removed,
        })
    }

    /// `files.diff`: one file's hunks against the base, uncommitted edits included.
    pub async fn files_diff(&self, key: &str, path: &str) -> Result<FileDiff, BoardError> {
        let path = clean_path(path, false)?;
        let ticket = self.ticket(key).await?;
        let not_changed =
            || BoardError::NotFound(format!("{path} isn't changed in {}", ticket.key));
        self.ticket_file_diff(&ticket, &path, DIFF_CONTEXT)
            .await?
            .ok_or_else(not_changed)
    }

    /// The ticket's own change of `path` (already clean) against where it left its base, uncommitted edits
    /// included, with `context` unchanged lines around each change; `None` when it didn't change it.
    pub(crate) async fn ticket_file_diff(
        &self,
        ticket: &Ticket,
        path: &str,
        context: u32,
    ) -> Result<Option<FileDiff>, BoardError> {
        let source = self.files_source(ticket).await?;
        let git = self.git()?;
        Ok(match (source.info.from, &source.worktree, &source.from) {
            (FilesFrom::Worktree, Some(wt), Some(from)) => {
                self.worktree_file_diff(wt, from, path, context).await?
            }
            (FilesFrom::Branch, _, _) => {
                let base_sha = source.info.base_sha.as_deref().unwrap_or_default();
                let spec = DiffSpec::since(base_sha, &source.rev);
                git.file_diff_with(&source.cwd, &spec, path, context)
                    .await?
            }
            _ => None,
        })
    }

    /// `files.read`: the file as it is: on disk in a worktree, else from git.
    pub async fn files_read(&self, key: &str, path: &str) -> Result<FileContent, BoardError> {
        let path = clean_path(path, false)?;
        let ticket = self.ticket(key).await?;
        let source = self.files_source(&ticket).await?;
        let missing = || BoardError::NotFound(format!("{path} isn't a file in {}", ticket.key));
        let git = self.git()?;

        let changes = self.files_changes_of(&source).await?;
        let (change, tags) = changes.change(&path);
        let on_disk = match (&source.worktree, source.info.from) {
            (Some(wt), FilesFrom::Worktree) => Some(wt.clone()),
            _ => None,
        };

        let (bytes, size) = match &on_disk {
            Some(wt) => read_confined(wt, &path).await?.ok_or_else(missing)?,
            None => {
                let object = format!("{}:{path}", source.rev);
                let kind = git.run(&source.cwd, &["cat-file", "-t", &object]).await;
                if !matches!(kind.as_deref().map(str::trim), Ok("blob")) {
                    return Err(missing());
                }
                git.read_blob(&source.cwd, &source.rev, &path)
                    .await?
                    .ok_or_else(missing)?
            }
        };
        let (binary, text, truncated) = text_of(&bytes, size);
        Ok(FileContent {
            source: source.info,
            path,
            size,
            binary,
            text,
            truncated,
            change,
            tags,
        })
    }

    pub(super) async fn files_source(&self, ticket: &Ticket) -> Result<Source, BoardError> {
        let project = self.project(ticket.key.project.as_str()).await?;
        let repo = PathBuf::from(project.repo_path);
        let git = self.git()?;
        let work = ticket.work.clone().unwrap_or_default();
        let base_branch = ticket.base.branch().to_owned();

        if let (Some(branch), Some(base_sha)) = (work.branch, work.base_sha) {
            let info = FilesSource {
                key: ticket.key.clone(),
                from: FilesFrom::Branch,
                branch: branch.clone(),
                base_branch: base_branch.clone(),
                base_short: Some(short_sha(&base_sha)),
                base_sha: Some(base_sha.clone()),
            };
            let worktree = work.worktree.map(PathBuf::from).filter(|wt| wt.is_dir());
            if let Some(wt) = worktree {
                let from = match git.run(&wt, &["merge-base", &base_sha, "HEAD"]).await {
                    Ok(out) => out.trim().to_owned(),
                    Err(_) => base_sha,
                };
                return Ok(Source {
                    info: FilesSource {
                        from: FilesFrom::Worktree,
                        ..info
                    },
                    cwd: wt.clone(),
                    rev: "HEAD".to_owned(),
                    worktree: Some(wt),
                    from: Some(from),
                });
            }
            let rev = format!("refs/heads/{branch}");
            if branch_exists(git, &repo, &branch).await {
                return Ok(Source {
                    info,
                    cwd: repo,
                    rev,
                    worktree: None,
                    from: None,
                });
            }
        }
        Ok(Source {
            info: FilesSource {
                key: ticket.key.clone(),
                from: FilesFrom::Base,
                branch: base_branch.clone(),
                base_branch: base_branch.clone(),
                base_sha: None,
                base_short: None,
            },
            rev: format!("refs/heads/{base_branch}"),
            cwd: repo,
            worktree: None,
            from: None,
        })
    }

    /// Every change of `source`.
    pub(super) async fn files_changes_of(&self, source: &Source) -> Result<Changes, BoardError> {
        let git = self.git()?;
        let entries = match (source.info.from, &source.worktree, &source.from) {
            (FilesFrom::Worktree, Some(wt), Some(from)) => self.worktree_changes(wt, from).await?,
            (FilesFrom::Branch, _, _) => {
                let base_sha = source.info.base_sha.as_deref().unwrap_or_default();
                git.changed_files(&source.cwd, &DiffSpec::since(base_sha, &source.rev))
                    .await?
                    .into_iter()
                    .map(|file| ChangedEntry {
                        file,
                        uncommitted: false,
                        tags: Vec::new(),
                    })
                    .collect()
            }
            _ => Vec::new(),
        };
        Ok(Changes::new(entries, HashMap::new()))
    }

    /// A worktree's changes since `from`: its commits, its uncommitted edits and its untracked files.
    async fn worktree_changes(
        &self,
        worktree: &Path,
        from: &str,
    ) -> Result<Vec<ChangedEntry>, BoardError> {
        let git = self.git()?;
        let mut files = git
            .changed_files(worktree, &DiffSpec::working_tree(from))
            .await?;
        for path in git.untracked_files(worktree).await? {
            if let Some((bytes, size)) = read_entry(worktree, &path).await {
                files.push(new_file_diff(&path, &bytes, size).file);
            }
        }
        files.sort_by(|a, b| a.path.cmp(&b.path));
        let uncommitted: HashSet<String> =
            git.uncommitted_paths(worktree).await?.into_iter().collect();
        Ok(files
            .into_iter()
            .map(|file| ChangedEntry {
                uncommitted: uncommitted.contains(&file.path)
                    || file
                        .old_path
                        .as_ref()
                        .is_some_and(|p| uncommitted.contains(p)),
                file,
                tags: Vec::new(),
            })
            .collect())
    }

    /// One file's diff in a worktree since `from`, with `context` lines around each change; `None` when it
    /// didn't change.
    async fn worktree_file_diff(
        &self,
        worktree: &Path,
        from: &str,
        path: &str,
        context: u32,
    ) -> Result<Option<FileDiff>, BoardError> {
        let git = self.git()?;
        if git
            .untracked_files(worktree)
            .await?
            .iter()
            .any(|p| p == path)
        {
            return Ok(read_entry(worktree, path)
                .await
                .map(|(bytes, size)| new_file_diff(path, &bytes, size)));
        }
        Ok(git
            .file_diff_with(worktree, &DiffSpec::working_tree(from), path, context)
            .await?)
    }
}

/// A request's path, checked: relative, `/`-separated, no empty, `.`, `..` or `.git` part, no NUL or `\`.
/// A trailing `/` is dropped. Empty only where `top` allows it (the top folder).
pub fn clean_path(path: &str, top: bool) -> Result<String, BoardError> {
    let trimmed = path.trim_end_matches('/');
    let bad = |why: &str| {
        Err(BoardError::refused(
            Refusal::BadPath,
            format!("“{path}” can't be read: {why}"),
        ))
    };
    if trimmed.is_empty() {
        return if top {
            Ok(String::new())
        } else {
            bad("no path")
        };
    }
    if trimmed.starts_with('/') || Path::new(trimmed).is_absolute() {
        return bad("only paths inside the ticket's files");
    }
    if trimmed.contains('\0') || trimmed.contains('\\') {
        return bad("it has a character paths can't have");
    }
    for part in trimmed.split('/') {
        if part.is_empty() || part == "." || part == ".." {
            return bad("only paths inside the ticket's files");
        }
        if part.eq_ignore_ascii_case(".git") {
            return bad("git's own files aren't shown");
        }
    }
    Ok(trimmed.to_owned())
}

/// The file at `path` in `worktree`, refusing anything that resolves outside it (a symlink out, a symlinked
/// folder out). `None` when there is no such file. At most [`MAX_READ_BYTES`], and the whole size.
async fn read_confined(worktree: &Path, path: &str) -> Result<Option<(Vec<u8>, u64)>, BoardError> {
    let io_error = |e: std::io::Error| BoardError::NotFound(format!("{path} can't be read: {e}"));
    let root = tokio::fs::canonicalize(worktree).await.map_err(io_error)?;
    let full = match tokio::fs::canonicalize(worktree.join(path)).await {
        Ok(full) => full,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(io_error(e)),
    };
    let inside = full.starts_with(&root)
        && full
            .strip_prefix(&root)
            .is_ok_and(|rest| rest.components().all(|c| matches!(c, Component::Normal(_))));
    if !inside {
        return Err(BoardError::refused(
            Refusal::BadPath,
            format!("“{path}” points outside the ticket's worktree"),
        ));
    }
    let meta = tokio::fs::metadata(&full).await.map_err(io_error)?;
    if !meta.is_file() {
        return Ok(None);
    }
    let file = tokio::fs::File::open(&full).await.map_err(io_error)?;
    let mut bytes = Vec::new();
    file.take(MAX_READ_BYTES)
        .read_to_end(&mut bytes)
        .await
        .map_err(io_error)?;
    Ok(Some((bytes, meta.len())))
}

/// An untracked entry as git would store it, never following a link: a file's first [`MAX_READ_BYTES`] and
/// size, or a symlink's target.
async fn read_entry(worktree: &Path, path: &str) -> Option<(Vec<u8>, u64)> {
    let full = worktree.join(path);
    let meta = tokio::fs::symlink_metadata(&full).await.ok()?;
    if meta.file_type().is_symlink() {
        let target = tokio::fs::read_link(&full).await.ok()?;
        let bytes = target.to_string_lossy().into_owned().into_bytes();
        let size = bytes.len() as u64;
        return Some((bytes, size));
    }
    if !meta.is_file() {
        return None;
    }
    let file = tokio::fs::File::open(&full).await.ok()?;
    let mut bytes = Vec::new();
    file.take(MAX_READ_BYTES)
        .read_to_end(&mut bytes)
        .await
        .ok()?;
    Some((bytes, meta.len()))
}

/// One folder's entries at `rev` (`folder` empty: the top), by path: `git ls-tree` of that folder only, never the
/// whole tree.
pub(super) async fn list_folder(
    git: &Git,
    cwd: &Path,
    rev: &str,
    folder: &str,
) -> Result<BTreeMap<String, EntryKind>, GitError> {
    let mut args = vec!["--literal-pathspecs", "ls-tree", "-z", rev];
    let spec = format!("{folder}/");
    if !folder.is_empty() {
        args.extend(["--", spec.as_str()]);
    }
    let listed = git.run(cwd, &args).await?;
    let mut entries: BTreeMap<String, EntryKind> = BTreeMap::new();
    for record in listed.split('\0').filter(|r| !r.is_empty()) {
        let Some((meta, path)) = record.split_once('\t') else {
            continue;
        };
        let mut meta = meta.split(' ');
        let (mode, kind) = (meta.next().unwrap_or(""), meta.next().unwrap_or(""));
        let kind = match (kind, mode) {
            ("tree", _) => EntryKind::Folder,
            ("commit", _) => EntryKind::Submodule,
            (_, "120000") => EntryKind::Link,
            _ => EntryKind::File,
        };
        entries.insert(path.to_owned(), kind);
    }
    Ok(entries)
}

/// A tree's order: folders first, then by name ignoring case.
pub(super) fn entry_order(a: (EntryKind, &str), b: (EntryKind, &str)) -> std::cmp::Ordering {
    (a.0 != EntryKind::Folder)
        .cmp(&(b.0 != EntryKind::Folder))
        .then_with(|| a.1.to_lowercase().cmp(&b.1.to_lowercase()))
        .then_with(|| a.1.cmp(b.1))
}

/// A file's bytes as the File view shows them: binary or not, the text cut after [`MAX_FILE_LINES`] lines, and
/// whether it was cut (`size` is the whole file's).
pub(super) fn text_of(bytes: &[u8], size: u64) -> (bool, String, bool) {
    if looks_binary(bytes) {
        return (true, String::new(), false);
    }
    let text = String::from_utf8_lossy(bytes);
    let cut = text
        .match_indices('\n')
        .nth(MAX_FILE_LINES - 1)
        .map(|(at, _)| at + 1);
    let (text, truncated) = match cut {
        Some(at) if at < text.len() => (text[..at].to_owned(), true),
        _ => (text.into_owned(), (bytes.len() as u64) < size),
    };
    (false, text, truncated)
}

pub(super) async fn branch_exists(git: &Git, repo: &Path, branch: &str) -> bool {
    let spec = format!("refs/heads/{branch}^{{commit}}");
    git.run(repo, &["rev-parse", "--verify", "--quiet", &spec])
        .await
        .is_ok()
}

pub(super) fn nul_paths(out: &str) -> impl Iterator<Item = String> + '_ {
    out.split('\0').filter(|p| !p.is_empty()).map(str::to_owned)
}

/// `path` relative to `folder` when it is inside it (any depth); the top folder holds everything.
pub(super) fn under<'a>(path: &'a str, folder: &str) -> Option<&'a str> {
    if folder.is_empty() {
        return Some(path);
    }
    path.strip_prefix(folder)?.strip_prefix('/')
}

pub(super) fn join_path(folder: &str, name: &str) -> String {
    if folder.is_empty() {
        name.to_owned()
    } else {
        format!("{folder}/{name}")
    }
}

/// How well `path` matches `needle` (lowercase), lower is better, and the matched characters.
pub(super) fn rank(path: &str, needle: &str) -> Option<(u8, [u32; 2])> {
    let lower = path.to_lowercase();
    // Lowercasing can change lengths outside ASCII: count the hit in characters of the lowercased path, which
    // is the same as the path's for every name a repository usually has.
    let hit = |byte: usize| {
        let start = lower[..byte].chars().count() as u32;
        [start, start + needle.chars().count() as u32]
    };
    let name_at = lower.rfind('/').map_or(0, |i| i + 1);
    if lower[name_at..].starts_with(needle) {
        return Some((0, hit(name_at)));
    }
    let mut at = 0;
    for part in lower[..name_at].split('/') {
        if !part.is_empty() && part.starts_with(needle) {
            return Some((1, hit(at)));
        }
        at += part.len() + 1;
    }
    if let Some(i) = lower[name_at..].find(needle) {
        return Some((2, hit(name_at + i)));
    }
    lower.find(needle).map(|i| (3, hit(i)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_stay_inside() {
        assert_eq!(clean_path("src/lib/", false).unwrap(), "src/lib");
        assert_eq!(clean_path("", true).unwrap(), "");
        for bad in [
            "",
            "../etc/passwd",
            "src/../../x",
            "/etc/passwd",
            "./a",
            "a//b",
            ".git/config",
            "sub/.GIT/HEAD",
            "a\\b",
        ] {
            let err = clean_path(bad, false).unwrap_err();
            assert_eq!(err.refusal(), Some(Refusal::BadPath), "{bad}");
        }
    }

    #[test]
    fn names_rank_before_folders_and_contains() {
        let needle = "log";
        assert_eq!(rank("e2e/login.spec.ts", needle), Some((0, [4, 7])));
        assert_eq!(rank("src/app/login/page.tsx", needle), Some((1, [8, 11])));
        assert_eq!(rank("src/lib/catalog.ts", needle), Some((2, [12, 15])));
        assert_eq!(rank("src/LoginForm.tsx", needle), Some((0, [4, 7])));
        assert_eq!(rank("README.md", needle), None);
    }

    #[test]
    fn under_reads_any_depth() {
        assert_eq!(under("src/a/b.ts", "src"), Some("a/b.ts"));
        assert_eq!(under("srcx/b.ts", "src"), None);
        assert_eq!(under("b.ts", ""), Some("b.ts"));
    }
}
