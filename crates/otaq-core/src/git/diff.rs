//! Reading diffs with the user's `git` (REV-01, REV-02; d0017): the changed files with their status and counts,
//! and one file's hunks with line numbers on both sides. Read-only: nothing here touches a worktree, the index
//! or a ref.
//!
//! A diff is always between two revisions of the repository, so it works from the branch alone (a Done
//! ticket whose worktree was removed still has its branch). Large diffs are read **one file at a time**: the
//! file list is cheap (`--numstat`), hunks come per file. A single file is cut after [`MAX_FILE_LINES`] lines
//! (`truncated`), so one message never carries an unbounded diff (implementation choice, SDD §26).
//!
//! A ticket's Files tab (FTB-01…FTB-04, T-0467) also compares a commit with **the files on disk** in a ticket's
//! worktree ([`DiffSpec::working_tree`]), uncommitted edits included; files git doesn't track yet come from
//! [`untracked_files`](Git::untracked_files) and [`new_file_diff`]. Still read-only: every command runs with
//! `--no-optional-locks`, so git never refreshes the worktree's index.

use std::collections::HashMap;
use std::path::Path;
use std::process::Stdio;

use tokio::io::AsyncReadExt;

use serde::{Deserialize, Serialize};

use super::{Git, GitError};
use crate::domain::stored_enum;

/// The most diff lines returned for one file; the rest is reported as `truncated`.
pub const MAX_FILE_LINES: usize = 20_000;

/// The most bytes of one file read for its text (a file's File view, a new file's diff); the rest is reported as
/// `truncated`, as [`MAX_FILE_LINES`] is.
pub const MAX_READ_BYTES: u64 = 8 * 1024 * 1024;

/// The unchanged lines shown around each change of a diff.
pub const DIFF_CONTEXT: u32 = 3;

/// How far git looks for a NUL byte to call a file binary (git's own `FIRST_FEW_BYTES`).
const BINARY_PROBE: usize = 8000;

/// Which two revisions to compare.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffSpec {
    pub from: String,
    /// `None`: the files on disk in the folder git runs in (a ticket's worktree), uncommitted edits included.
    pub to: Option<String>,
    /// `from...to`: compare `to` with where it left `from` (a ticket's branch with its `base_sha`). Otherwise
    /// `from` and `to` are compared directly (one squash commit with its parent, a review head with the head).
    pub merge_base: bool,
}

impl DiffSpec {
    /// `from...to`.
    pub fn since(from: &str, to: &str) -> Self {
        Self {
            from: from.to_owned(),
            to: Some(to.to_owned()),
            merge_base: true,
        }
    }

    /// `from` to `to`, directly.
    pub fn between(from: &str, to: &str) -> Self {
        Self {
            from: from.to_owned(),
            to: Some(to.to_owned()),
            merge_base: false,
        }
    }

    /// `from` to the files on disk: run in a worktree, its commits and its uncommitted edits since `from`.
    /// Untracked files aren't part of it ([`Git::untracked_files`]).
    pub fn working_tree(from: &str) -> Self {
        Self {
            from: from.to_owned(),
            to: None,
            merge_base: false,
        }
    }

    /// One commit against its first parent.
    pub fn commit(sha: &str) -> Self {
        Self::between(&format!("{sha}^"), sha)
    }

    fn revs(&self) -> Vec<String> {
        match &self.to {
            Some(to) if self.merge_base => vec![format!("{}...{to}", self.from)],
            Some(to) => vec![self.from.clone(), to.clone()],
            None => vec![self.from.clone()],
        }
    }
}

stored_enum! {
    /// How a file changed.
    pub enum FileStatus ("file status") {
        Added = "added",
        Modified = "modified",
        Deleted = "deleted",
        Renamed = "renamed",
    }
}

/// One changed file in the file list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChangedFile {
    pub status: FileStatus,
    /// The path after the change (the old path for a deleted file).
    pub path: String,
    /// Renames: the path before.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub old_path: Option<String>,
    pub added: u32,
    pub removed: u32,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub binary: bool,
    /// Binary files: the size in bytes after the change (`None` for a deleted file).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
    /// Binary files: the size before the change (`None` for an added file).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub old_size: Option<u64>,
    /// Renames: how similar the two versions are, in percent (git's rename score).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub similarity: Option<u8>,
}

stored_enum! {
    /// A line of a hunk.
    pub enum LineKind ("diff line kind") {
        Context = "context",
        Added = "added",
        Removed = "removed",
    }
}

/// One line of a hunk, with its number on each side it exists on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiffLine {
    pub kind: LineKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub old: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub new: Option<u32>,
    pub text: String,
    /// The file has no newline at its end after this line ("\ No newline at end of file").
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub no_newline: bool,
}

/// `@@ -12,7 +12,9 @@ fn total()`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Hunk {
    pub old_start: u32,
    pub old_lines: u32,
    pub new_start: u32,
    pub new_lines: u32,
    /// The text after the second `@@` (git's function context), possibly empty.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub header: String,
    pub lines: Vec<DiffLine>,
}

/// One file's diff.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileDiff {
    pub file: ChangedFile,
    pub hunks: Vec<Hunk>,
    /// More than [`MAX_FILE_LINES`] lines: the rest was left out.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub truncated: bool,
}

impl Git {
    /// The changed files between the two revisions, in git's order (by path), renames detected.
    pub async fn changed_files(
        &self,
        repo: &Path,
        spec: &DiffSpec,
    ) -> Result<Vec<ChangedFile>, GitError> {
        let revs = spec.revs();
        let mut args = vec![
            "--no-optional-locks",
            "diff",
            "--no-color",
            "--no-ext-diff",
            "-M",
            "-z",
        ];
        let rev_args: Vec<&str> = revs.iter().map(String::as_str).collect();
        let mut status_args = args.clone();
        status_args.push("--name-status");
        status_args.extend(&rev_args);
        args.push("--numstat");
        args.extend(&rev_args);
        // Both passes side by side: each walks the worktree, a slow walk in a big one.
        let (statuses, counts) = tokio::join!(self.run(repo, &status_args), self.run(repo, &args));
        let statuses = parse_name_status(&statuses?);
        let counts = parse_numstat(&counts?);
        let mut files = Vec::with_capacity(statuses.len());
        for (status, path, old_path, similarity) in statuses {
            let count = counts.get(&path).copied();
            let binary = count.is_some_and(|c| c.is_none());
            let (added, removed) = count.flatten().unwrap_or((0, 0));
            let (size, old_size) = if binary {
                let old = old_path.as_deref().unwrap_or(&path);
                let size = match &spec.to {
                    Some(to) => self.blob_size(repo, to, &path).await?,
                    None => tokio::fs::metadata(repo.join(&path))
                        .await
                        .ok()
                        .map(|m| m.len()),
                };
                (size, self.blob_size(repo, &spec.from, old).await?)
            } else {
                (None, None)
            };
            files.push(ChangedFile {
                status,
                path,
                old_path,
                added,
                removed,
                binary,
                size,
                old_size,
                similarity,
            });
        }
        Ok(files)
    }

    /// One file's hunks. `path` is the file's path in the list (its new path for a rename). `None` when the
    /// file didn't change.
    pub async fn file_diff(
        &self,
        repo: &Path,
        spec: &DiffSpec,
        path: &str,
    ) -> Result<Option<FileDiff>, GitError> {
        self.file_diff_with(repo, spec, path, DIFF_CONTEXT).await
    }

    /// [`Git::file_diff`] with `context` unchanged lines around each change: 0 gives one hunk per changed run,
    /// each with its own function context in `header`.
    pub async fn file_diff_with(
        &self,
        repo: &Path,
        spec: &DiffSpec,
        path: &str,
        context: u32,
    ) -> Result<Option<FileDiff>, GitError> {
        let files = self.changed_files(repo, spec).await?;
        let Some(file) = files.into_iter().find(|f| f.path == path) else {
            return Ok(None);
        };
        if file.binary {
            return Ok(Some(FileDiff {
                file,
                hunks: Vec::new(),
                truncated: false,
            }));
        }
        let revs = spec.revs();
        let unified = format!("--unified={context}");
        let mut args = vec![
            "--no-optional-locks",
            "diff",
            "--no-color",
            "--no-ext-diff",
            "-M",
            &unified,
        ];
        args.extend(revs.iter().map(String::as_str));
        args.push("--");
        if let Some(old) = &file.old_path {
            args.push(old);
        }
        args.push(&file.path);
        let patch = self.run(repo, &args).await?;
        let (hunks, truncated) = parse_hunks(&patch, MAX_FILE_LINES);
        Ok(Some(FileDiff {
            file,
            hunks,
            truncated,
        }))
    }

    /// The size of `path` at `rev`, or `None` if it isn't there.
    pub async fn blob_size(
        &self,
        repo: &Path,
        rev: &str,
        path: &str,
    ) -> Result<Option<u64>, GitError> {
        let object = format!("{rev}:{path}");
        match self.run(repo, &["cat-file", "-s", &object]).await {
            Ok(out) => Ok(out.trim().parse().ok()),
            Err(GitError::Failed { .. }) => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// The files of a worktree git doesn't track and doesn't ignore, by path from its top (`git ls-files
    /// --others --exclude-standard`). Sorted.
    pub async fn untracked_files(&self, worktree: &Path) -> Result<Vec<String>, GitError> {
        let out = self
            .run(
                worktree,
                &[
                    "--no-optional-locks",
                    "ls-files",
                    "--others",
                    "--exclude-standard",
                    "-z",
                ],
            )
            .await?;
        Ok(nul_list(&out))
    }

    /// The paths whose files on disk differ from the worktree's `HEAD`: edits not committed yet, staged or not
    /// (`git diff --name-only HEAD`), plus untracked files. Sorted, without duplicates.
    pub async fn uncommitted_paths(&self, worktree: &Path) -> Result<Vec<String>, GitError> {
        let (tracked, untracked) = tokio::join!(
            self.uncommitted_tracked(worktree),
            self.untracked_files(worktree)
        );
        let mut paths = tracked?;
        paths.extend(untracked?);
        paths.sort();
        paths.dedup();
        Ok(paths)
    }

    /// The tracked files whose content on disk differs from the worktree's `HEAD`, staged or not (`git diff
    /// --name-only HEAD`); untracked files aren't among them.
    pub async fn uncommitted_tracked(&self, worktree: &Path) -> Result<Vec<String>, GitError> {
        let tracked = self
            .run(
                worktree,
                &[
                    "--no-optional-locks",
                    "diff",
                    "--name-only",
                    "--no-renames",
                    "--no-ext-diff",
                    "-z",
                    "HEAD",
                ],
            )
            .await?;
        Ok(nul_list(&tracked))
    }

    /// The first [`MAX_READ_BYTES`] of `path` at `rev`, and the blob's whole size; `None` if it isn't there.
    /// Read through a pipe, so a huge blob is never held whole.
    pub async fn read_blob(
        &self,
        repo: &Path,
        rev: &str,
        path: &str,
    ) -> Result<Option<(Vec<u8>, u64)>, GitError> {
        let Some(size) = self.blob_size(repo, rev, path).await? else {
            return Ok(None);
        };
        let object = format!("{rev}:{path}");
        let args = ["cat-file", "blob", object.as_str()];
        let spawn_error = |error| GitError::Spawn {
            args: super::join(&args),
            error,
        };
        let mut child = tokio::process::Command::new(&self.program)
            .args(args)
            .current_dir(repo)
            .envs(self.env.iter().map(|(k, v)| (k, v)))
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .map_err(spawn_error)?;
        let mut bytes = Vec::new();
        if let Some(stdout) = child.stdout.take() {
            stdout
                .take(MAX_READ_BYTES)
                .read_to_end(&mut bytes)
                .await
                .map_err(spawn_error)?;
        }
        // Reading stopped at the cap: the rest isn't wanted.
        let _ = child.start_kill();
        let _ = child.wait().await;
        tracing::debug!(cwd = %repo.display(), args = %super::join(&args), "git");
        Ok(Some((bytes, size)))
    }

    /// The paths a worktree has changed so far against where it left `branch`: its commits and its
    /// uncommitted changes since the merge base, plus untracked files that aren't ignored. A rename counts as
    /// both paths. Sorted, without duplicates. Read-only: `--no-optional-locks` keeps git from refreshing the
    /// index, so the index's mtime moves only when the agent stages or commits.
    pub async fn worktree_changed_paths(
        &self,
        worktree: &Path,
        branch: &str,
    ) -> Result<Vec<String>, GitError> {
        let branch_ref = format!("refs/heads/{branch}");
        let merge_base = self
            .run(worktree, &["merge-base", "HEAD", &branch_ref])
            .await?;
        let tracked = self
            .run(
                worktree,
                &[
                    "--no-optional-locks",
                    "diff",
                    "--name-only",
                    "--no-renames",
                    "--no-ext-diff",
                    "-z",
                    merge_base.trim(),
                ],
            )
            .await?;
        let untracked = self
            .run(
                worktree,
                &[
                    "--no-optional-locks",
                    "ls-files",
                    "--others",
                    "--exclude-standard",
                    "-z",
                ],
            )
            .await?;
        let mut paths: Vec<String> = tracked
            .split('\0')
            .chain(untracked.split('\0'))
            .filter(|p| !p.is_empty())
            .map(str::to_owned)
            .collect();
        paths.sort();
        paths.dedup();
        Ok(paths)
    }
}

/// A NUL-separated list (`-z`), sorted.
fn nul_list(out: &str) -> Vec<String> {
    let mut paths: Vec<String> = out
        .split('\0')
        .filter(|p| !p.is_empty())
        .map(str::to_owned)
        .collect();
    paths.sort();
    paths
}

/// Whether `bytes` read as binary: a NUL among the first bytes, as git decides.
pub fn looks_binary(bytes: &[u8]) -> bool {
    bytes[..bytes.len().min(BINARY_PROBE)].contains(&0)
}

/// An untracked file's diff: every line added, as `git diff` shows a new file. `content` is the start of the
/// file (at most [`MAX_READ_BYTES`]) and `size` its whole size; past either cap the diff is `truncated` and the
/// counts are those of what was read.
pub fn new_file_diff(path: &str, content: &[u8], size: u64) -> FileDiff {
    let binary = looks_binary(content);
    let mut file = ChangedFile {
        status: FileStatus::Added,
        path: path.to_owned(),
        old_path: None,
        added: 0,
        removed: 0,
        binary,
        size: binary.then_some(size),
        old_size: None,
        similarity: None,
    };
    if binary {
        return FileDiff {
            file,
            hunks: Vec::new(),
            truncated: false,
        };
    }
    let text = String::from_utf8_lossy(content);
    let ends_with_newline = text.ends_with('\n');
    let all: Vec<&str> = text.lines().collect();
    let kept = all.len().min(MAX_FILE_LINES);
    let truncated = kept < all.len() || (content.len() as u64) < size;
    let mut lines: Vec<DiffLine> = all[..kept]
        .iter()
        .enumerate()
        .map(|(n, line)| DiffLine {
            kind: LineKind::Added,
            old: None,
            new: Some(n as u32 + 1),
            text: (*line).to_owned(),
            no_newline: false,
        })
        .collect();
    if !ends_with_newline
        && !truncated
        && let Some(last) = lines.last_mut()
    {
        last.no_newline = true;
    }
    file.added = u32::try_from(all.len()).unwrap_or(u32::MAX);
    let hunks = if lines.is_empty() {
        Vec::new()
    } else {
        vec![Hunk {
            old_start: 0,
            old_lines: 0,
            new_start: 1,
            new_lines: file.added,
            header: String::new(),
            lines,
        }]
    };
    FileDiff {
        file,
        hunks,
        truncated,
    }
}

/// `git diff --name-status -z`: `M\0path\0`, `R100\0old\0new\0` (with the rename's similarity). Copies (`C`)
/// read as added; type changes (`T`) as modified.
fn parse_name_status(out: &str) -> Vec<(FileStatus, String, Option<String>, Option<u8>)> {
    let mut fields = out.split('\0').filter(|f| !f.is_empty());
    let mut files = Vec::new();
    while let Some(code) = fields.next() {
        let letter = code.chars().next().unwrap_or('M');
        let Some(first) = fields.next() else { break };
        match letter {
            'R' => {
                let Some(new) = fields.next() else { break };
                files.push((
                    FileStatus::Renamed,
                    new.to_owned(),
                    Some(first.to_owned()),
                    code[1..].parse().ok(),
                ));
            }
            'C' => {
                let Some(new) = fields.next() else { break };
                files.push((FileStatus::Added, new.to_owned(), None, None));
            }
            'A' => files.push((FileStatus::Added, first.to_owned(), None, None)),
            'D' => files.push((FileStatus::Deleted, first.to_owned(), None, None)),
            _ => files.push((FileStatus::Modified, first.to_owned(), None, None)),
        }
    }
    files
}

/// `git diff --numstat -z`: `added\tremoved\tpath\0`, or for a rename `added\tremoved\t\0old\0new\0`; binary
/// files have `-` counts. Keyed by the new path; `None` counts mean binary.
fn parse_numstat(out: &str) -> HashMap<String, Option<(u32, u32)>> {
    let mut counts = HashMap::new();
    let mut fields = out.split('\0');
    while let Some(record) = fields.next() {
        if record.is_empty() {
            continue;
        }
        let mut parts = record.splitn(3, '\t');
        let (Some(added), Some(removed), Some(path)) = (parts.next(), parts.next(), parts.next())
        else {
            continue;
        };
        let path = if path.is_empty() {
            // A rename: the old and the new path follow as their own fields.
            let _old = fields.next();
            fields.next().unwrap_or_default().to_owned()
        } else {
            path.to_owned()
        };
        let count = match (added.parse(), removed.parse()) {
            (Ok(a), Ok(r)) => Some((a, r)),
            _ => None,
        };
        counts.insert(path, count);
    }
    counts
}

/// Turns a unified diff of one file into hunks, keeping at most `max_lines` lines.
fn parse_hunks(patch: &str, max_lines: usize) -> (Vec<Hunk>, bool) {
    let mut hunks: Vec<Hunk> = Vec::new();
    let mut old = 0;
    let mut new = 0;
    let mut kept = 0;
    for line in patch.lines() {
        if let Some(rest) = line.strip_prefix("@@ ") {
            if kept >= max_lines {
                return (hunks, true);
            }
            let Some(hunk) = parse_hunk_header(rest) else {
                continue;
            };
            old = hunk.old_start;
            new = hunk.new_start;
            hunks.push(hunk);
            continue;
        }
        let Some(hunk) = hunks.last_mut() else {
            // The header lines before the first hunk (diff --git, index, ---, +++).
            continue;
        };
        if line.starts_with('\\') {
            if let Some(last) = hunk.lines.last_mut() {
                last.no_newline = true;
            }
            continue;
        }
        if kept >= max_lines {
            return (hunks, true);
        }
        let (kind, text) = match line.split_at_checked(1) {
            Some(("+", text)) => (LineKind::Added, text),
            Some(("-", text)) => (LineKind::Removed, text),
            Some((_, text)) => (LineKind::Context, text),
            None => (LineKind::Context, ""),
        };
        let (o, n) = match kind {
            LineKind::Added => {
                new += 1;
                (None, Some(new - 1))
            }
            LineKind::Removed => {
                old += 1;
                (Some(old - 1), None)
            }
            LineKind::Context => {
                old += 1;
                new += 1;
                (Some(old - 1), Some(new - 1))
            }
        };
        hunk.lines.push(DiffLine {
            kind,
            old: o,
            new: n,
            text: text.to_owned(),
            no_newline: false,
        });
        kept += 1;
    }
    (hunks, false)
}

/// `-12,7 +12,9 @@ fn total()` (after the leading `@@ `).
fn parse_hunk_header(rest: &str) -> Option<Hunk> {
    let (ranges, header) = rest.split_once(" @@").unwrap_or((rest, ""));
    let mut parts = ranges.split_whitespace();
    let old = parts.next()?.strip_prefix('-')?;
    let new = parts.next()?.strip_prefix('+')?;
    let range = |r: &str| -> Option<(u32, u32)> {
        match r.split_once(',') {
            Some((start, len)) => Some((start.parse().ok()?, len.parse().ok()?)),
            None => Some((r.parse().ok()?, 1)),
        }
    };
    let (old_start, old_lines) = range(old)?;
    let (new_start, new_lines) = range(new)?;
    Some(Hunk {
        old_start,
        old_lines,
        new_start,
        new_lines,
        header: header.trim().to_owned(),
        lines: Vec::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn name_status_reads_every_kind() {
        let out = "M\0lib/a.ts\0A\0b.ts\0D\0c.ts\0R087\0old.ts\0new.ts\0";
        assert_eq!(
            parse_name_status(out),
            [
                (FileStatus::Modified, "lib/a.ts".into(), None, None),
                (FileStatus::Added, "b.ts".into(), None, None),
                (FileStatus::Deleted, "c.ts".into(), None, None),
                (
                    FileStatus::Renamed,
                    "new.ts".into(),
                    Some("old.ts".into()),
                    Some(87)
                ),
            ]
        );
    }

    #[test]
    fn numstat_reads_renames_and_binary_files() {
        let out = "3\t1\tlib/a.ts\0-\t-\tlogo.png\0\x30\t0\t\0old.ts\0new.ts\0";
        let counts = parse_numstat(out);
        assert_eq!(counts["lib/a.ts"], Some((3, 1)));
        assert_eq!(counts["logo.png"], None);
        assert_eq!(counts["new.ts"], Some((0, 0)));
    }

    #[test]
    fn hunks_number_both_sides() {
        let patch = "diff --git a/f b/f\nindex 1..2 100644\n--- a/f\n+++ b/f\n@@ -1,3 +1,3 @@ fn x()\n a\n-b\n+B\n c\n\\ No newline at end of file\n";
        let (hunks, truncated) = parse_hunks(patch, 100);
        assert!(!truncated);
        assert_eq!(hunks.len(), 1);
        let h = &hunks[0];
        assert_eq!(
            (h.old_start, h.old_lines, h.new_start, h.new_lines),
            (1, 3, 1, 3)
        );
        assert_eq!(h.header, "fn x()");
        let numbers: Vec<_> = h.lines.iter().map(|l| (l.kind, l.old, l.new)).collect();
        assert_eq!(
            numbers,
            [
                (LineKind::Context, Some(1), Some(1)),
                (LineKind::Removed, Some(2), None),
                (LineKind::Added, None, Some(2)),
                (LineKind::Context, Some(3), Some(3)),
            ]
        );
        assert!(h.lines[3].no_newline);
    }

    #[test]
    fn long_files_are_cut() {
        let mut patch = String::from("@@ -0,0 +1,10 @@\n");
        for n in 0..10 {
            patch.push_str(&format!("+line {n}\n"));
        }
        let (hunks, truncated) = parse_hunks(&patch, 4);
        assert!(truncated);
        assert_eq!(hunks[0].lines.len(), 4);
    }

    #[test]
    fn single_line_ranges_have_an_implicit_length() {
        let h = parse_hunk_header("-5 +5,2 @@").unwrap();
        assert_eq!(
            (h.old_start, h.old_lines, h.new_start, h.new_lines),
            (5, 1, 5, 2)
        );
    }
}
