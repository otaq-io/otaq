//! The project **Files** view's data (FIL-02…FIL-08, FIL-10; d0087, d0051, d0059, d0010, d0023, d0079): the
//! project's base branch with every ticket's changes on top, keyed by project.
//!
//! **The base** is the project's setting (`projects.base_branch`, d0051: `main`, or `develop` where the project is
//! set so), read as `refs/heads/<base>` in the project's repository: the tree with `git ls-tree` of one folder at a
//! time, files with `git cat-file`. It never reads the user's checkout, whatever branch it is on or edits it has.
//!
//! **Owners** are the project's tickets that have started and aren't Done or closed (epics aren't: the picker offers
//! an epic's branch as its own source), each compared with where it left its own base, as its Files tab measures it
//! (`board/files.rs`): the worktree while it is on disk, uncommitted and untracked files included and marked, else the
//! branch from git. An epic child's base is the epic's branch. A ticket on another base than the project's (SHOP-36 on
//! `release/2.3`, an epic child) is tagged with it; a file that exists only there is "not on <base>" and reads as that
//! ticket's diff. A ticket without a branch or worktree yet isn't an owner; the picker names it.
//!
//! **Who changes which lines** (FIL-05, FIL-06): for the file read, each owner's changed lines on its base's
//! numbering, from its own `git diff -U0` (one hunk per changed run; an addition is a point between two lines).
//! Owners on one base that change the file get a note: the overlap of their lines merged into spans, and the
//! function when git's hunk context names one for every overlapping change. The Same file count comes from paths
//! alone (`same_file`); owners on other bases are never compared.
//!
//! **No file watcher** (FIL-10): each owner's changes are cached in memory per ticket and read again only when the
//! ticket changed (its status, place or work, the points that publish `TicketChanged` / `CardStatusChanged`), its
//! worktree's HEAD or index moved, or its branch's tip moved, and, for a ticket whose agent is working, when they are
//! older than [`WORKING_FRESH`] (the view asks every 5 s while it is open; commits and unstaged edits have no event).
//! The owners are read side by side, and one request at a time reads them: the view's first requests arrive
//! together (the tree, the open folders, All changes), and the later ones then find the cache filled.
//! Nothing runs while no client asks. Every path goes through [`clean_path`] / the Files tab's reads; nothing here
//! writes to a worktree, an index or the checkout.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};

use serde::{Deserialize, Serialize};
use tokio::task::JoinSet;

use super::diff::short_sha;
use super::epic_flight::{REVIEW_EPIC, epic_stop};
use super::files::{
    ChangedEntry, EntryKind, FilesFrom, SEARCH_LIMIT, clean_path, entry_order, join_path,
    list_folder, nul_paths, rank, text_of, under,
};
use super::needs_you::{ProjectTickets, needs_you_items_with};
use super::tickets::{Base, Ticket, Work};
use super::{Board, BoardError};
use crate::domain::{Place, Status, StatusSet, TicketKey};
use crate::epic_lead::overlap::is_running;
use crate::flow::Flow;
use crate::git::Git;
use crate::git::diff::{ChangedFile, FileDiff};
use crate::store::now_ms;
use crate::store::projects::Project;

/// How long a working ticket's cached changes are trusted: under the view's 5 s refresh, so each refresh reads a
/// working agent's uncommitted edits again (FIL-10).
pub const WORKING_FRESH: Duration = Duration::from_secs(4);

/// What the view shows: "main · every ticket's changes".
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectSource {
    /// The project's key.
    pub project: String,
    /// The project's base branch: every label names it ("on develop", "not on develop").
    pub base_branch: String,
    /// Its tip now.
    pub base_sha: String,
    pub base_short: String,
    /// One of your own worktrees is shown instead of the base (#43): its folder. `base_branch` is then its branch
    /// (its commit when detached), `base_sha` its HEAD, and nothing of any ticket is on top.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub worktree: Option<String>,
}

/// One of the user's own git worktrees of the project's repository, in the picker's YOUR WORKTREES (#43): every
/// worktree `git worktree list` gives except the main checkout and Otaq's own (in its data folder, or a ticket's).
/// It shows read-only from git objects at its HEAD, as the base does (d0087: never the checkout's files on disk).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UserWorktree {
    /// Its folder, as git lists it.
    pub path: String,
    /// The branch checked out there; `None` when detached.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    /// Its HEAD commit.
    pub head: String,
    pub head_short: String,
}

impl UserWorktree {
    /// What its labels name: its branch, or its commit when detached.
    pub fn label(&self) -> &str {
        self.branch.as_deref().unwrap_or(&self.head_short)
    }
}

/// One ticket changing a file: one chip ("SHOP-36 · release/2.3"), its mark and +/−.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileOwner {
    pub key: TicketKey,
    /// The ticket's status (its chip's colour).
    pub status: Status,
    /// `otaq/SHOP-29`.
    pub branch: String,
    /// The ticket's own base: the project's, another branch (d0051) or its epic's branch.
    pub base_branch: String,
    /// Its base isn't the project's: the chip is tagged with `base_branch`.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub other_base: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub needs_you: bool,
    /// How the ticket changed the file: M · A · D, +/−, sizes of a binary file.
    pub file: ChangedFile,
    /// Some of the change isn't committed yet (edited on disk, staged, or untracked).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub uncommitted: bool,
}

/// A ticket that changes files (an owner), as the picker, a group of All changes and Changed by name it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OwnerTicket {
    pub key: TicketKey,
    pub title: String,
    pub status: Status,
    /// The stage it is in or waits before ("Dev", "Security"); `None` in Backlog or Ready for Dev.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stage: Option<String>,
    /// Its worktree on disk, or its branch from git (the worktree is gone).
    pub from: FilesFrom,
    pub branch: String,
    pub base_branch: String,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub other_base: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub needs_you: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub epic: Option<TicketKey>,
    /// "3 files · +13 −5".
    pub files: u32,
    pub added: u32,
    pub removed: u32,
}

/// One folder's entry of the view.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectEntry {
    pub name: String,
    pub path: String,
    pub kind: EntryKind,
    /// Files: the tickets changing it, the one that needs you first, then flow order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub owners: Vec<FileOwner>,
    /// Folders: how many changed files are inside, at any depth.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub changed_below: u32,
    /// The file isn't on the base: only tickets on other bases have it ("not on main").
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub not_on_base: bool,
}

/// The filters' counts: "Changed 11 · Needs you 6 · Same file 1", "8 tickets, 11 files".
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectCounts {
    /// Owners with at least one changed file.
    pub tickets: u32,
    /// Changed files, each counted once.
    pub changed: u32,
    /// Files changed by tickets that need you.
    pub needs_you: u32,
    /// The tickets that need you and change files ("changed by the 4 tickets that need you").
    pub needs_you_tickets: u32,
    /// Files two tickets on the same base change (FIL-06).
    pub same_file: u32,
}

/// `files.project_tree`: one folder of the base with every ticket's changes on top.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectTree {
    pub source: ProjectSource,
    pub path: String,
    /// Folders first, then by name.
    pub entries: Vec<ProjectEntry>,
    /// `None` when only the base was asked for (`base_only`: the tree shows at once, the marks follow).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub counts: Option<ProjectCounts>,
}

/// One search match of the view.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectMatch {
    pub path: String,
    pub name: String,
    pub folder: String,
    /// The matched characters in `path`, as `[start, end)` character offsets.
    pub hit: [u32; 2],
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub owners: Vec<FileOwner>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub not_on_base: bool,
}

/// `files.project_search`: names on the base and the owners' files, best first, as `files.search` ranks them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectSearch {
    pub source: ProjectSource,
    pub query: String,
    /// At most [`SEARCH_LIMIT`].
    pub matches: Vec<ProjectMatch>,
    pub total: u32,
}

/// `files.project_read`: a file as it is on the base, with the tickets changing it. A file that isn't on the base
/// carries a ticket's diff instead ("not on main · only on release/2.3 · SHOP-36's diff").
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectFile {
    pub source: ProjectSource,
    pub path: String,
    /// The file exists on the base: `size`, `binary`, `text` and `truncated` are its.
    pub on_base: bool,
    pub size: u64,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub binary: bool,
    /// Cut after `MAX_FILE_LINES` lines or `MAX_READ_BYTES` bytes, as the Files tab does.
    pub text: String,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub truncated: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub owners: Vec<FileOwner>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub not_on_base: bool,
    /// Not on the base: the first owner's diff (the one that needs you, else flow order).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diff: Option<FileDiff>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diff_of: Option<TicketKey>,
    /// On the base, a text file: each owner's changed lines, in `owners` order (FIL-05: a bar per ticket, the
    /// lines Changed by names, and what a huge file keeps unfolded). Each owner's lines are numbered as on its own
    /// base, where it left it.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub lines: Vec<OwnerLines>,
    /// Same file (FIL-06): one note per base that two or more owners share.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub same_file: Vec<SameFile>,
}

/// Lines a ticket changes on its base's numbering, as `git diff -U0` gives them: `lines` lines from `start`, or,
/// when `lines` is 0, new lines added after line `start` (0: before the first line).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct LineRange {
    pub start: u32,
    pub lines: u32,
}

impl LineRange {
    /// The last line it changes; for an addition, the line it follows.
    fn last(self) -> u32 {
        (self.start + self.lines).saturating_sub(1).max(self.start)
    }

    /// The two share a line, or an addition falls between two of the other's lines, or both add at one place.
    fn overlaps(self, other: LineRange) -> bool {
        // New lines after line `at` land inside `range` when both lines around them are in it.
        let inside = |at: u32, range: LineRange| range.start <= at && at < range.last();
        match (self.lines, other.lines) {
            (0, 0) => self.start == other.start,
            (0, _) => inside(self.start, other),
            (_, 0) => inside(other.start, self),
            _ => self.start <= other.last() && other.start <= self.last(),
        }
    }
}

/// One owner's changed lines of the file read.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OwnerLines {
    pub key: TicketKey,
    pub ranges: Vec<LineRange>,
}

/// Lines `first` to `last` of the base, both included.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct LineSpan {
    pub first: u32,
    pub last: u32,
}

/// Owners on one base changing the file (FIL-06): "SHOP-19 and SHOP-29 both edit applyDiscount (lines 41–58).
/// Whichever merges second will rebase on the other."
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SameFile {
    pub base_branch: String,
    /// The tickets the note names, furthest along first (later stage first, ties by key): those whose lines
    /// overlap, else every owner on the base.
    pub keys: Vec<TicketKey>,
    /// The lines they both touch: the overlapping changes, merged. Empty without overlapping lines.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub spans: Vec<LineSpan>,
    /// The function every overlapping change is in, from git's hunk context.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub function: Option<String>,
    pub note: String,
}

/// One ticket's group of All changes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChangeGroup {
    pub ticket: OwnerTicket,
    /// By path; each file's hunks come from `files.project_diff`.
    pub files: Vec<ChangedEntry>,
}

/// `files.project_changes`: every owner's changed files grouped by ticket, the ones that need you first, then flow
/// order ("8 tickets · 11 files against main").
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectChanges {
    pub source: ProjectSource,
    pub groups: Vec<ChangeGroup>,
    pub counts: ProjectCounts,
}

/// An epic in the picker: "Wishlist · SHOP-40 · 40.1 merged · 40.2 on top".
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EpicSource {
    pub key: TicketKey,
    pub title: String,
    pub status: Status,
    pub branch: String,
    /// Children merged into the epic's branch.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub merged: Vec<TicketKey>,
    /// Running children whose worktree changes come on top.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub on_top: Vec<TicketKey>,
}

/// A ticket without a worktree yet: "No worktree yet: SHOP-12, SHOP-44 (Grill), … and 3 in Backlog".
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NoWorktree {
    pub key: TicketKey,
    pub title: String,
    pub status: Status,
    /// The stage it is in or waits before; `None` in Backlog or Ready for Dev.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stage: Option<String>,
}

/// `files.project_sources`: the picker's rows (FIL-07). A ticket or an epic picked there reads with the Files tab's
/// own requests (`files.tree` …).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectSources {
    /// BASE BRANCH, the default.
    pub source: ProjectSource,
    /// EPICS · THE EPIC'S BRANCH: open epics whose branch exists.
    pub epics: Vec<EpicSource>,
    /// TICKETS · THEIR WORKTREE: every owner in flow order (other bases after the project's).
    pub tickets: Vec<OwnerTicket>,
    /// Open tickets with no worktree or branch yet, by key.
    pub no_worktree: Vec<NoWorktree>,
    /// YOUR WORKTREES: the user's own worktrees of the repository, by folder (#43).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub yours: Vec<UserWorktree>,
}

fn is_zero(n: &u32) -> bool {
    *n == 0
}

fn count(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

/// The per-ticket cache of owners' changes. Cheap to clone; lives in memory.
#[derive(Debug, Clone, Default)]
pub struct ProjectFilesCache {
    inner: Arc<Mutex<CacheState>>,
    /// Held while the owners' changes are read, so requests that arrive together read git once.
    reading: Arc<tokio::sync::Mutex<()>>,
}

#[derive(Debug, Default)]
struct CacheState {
    /// By ticket id.
    owners: HashMap<String, Cached>,
    /// How many times an owner's changes were read from git (the cache's misses).
    recomputes: u64,
}

#[derive(Debug)]
struct Cached {
    stamp: Stamp,
    at: Instant,
    owned: Arc<Owned>,
}

/// What must stay the same for an owner's cached changes to stay right.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Stamp {
    status: Status,
    status_since: Option<i64>,
    updated_at: i64,
    place: Place,
    base: Base,
    work: Work,
    /// The worktree's HEAD, or the branch's tip when the worktree is gone.
    head: String,
    /// The worktree's index (staging moves it).
    index_mtime: Option<SystemTime>,
}

/// One owner's changes, as read from git.
#[derive(Debug)]
struct Owned {
    from: FilesFrom,
    branch: String,
    base_branch: String,
    entries: Vec<ChangedEntry>,
}

impl ProjectFilesCache {
    /// How many times an owner's changes were read from git so far (for tests and logs).
    pub fn recomputes(&self) -> u64 {
        self.lock().recomputes
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, CacheState> {
        self.inner.lock().expect("project files")
    }
}

/// A project's tickets with what the view needs of each: owners with their changes, in group order.
struct View {
    project: Project,
    source: ProjectSource,
    flow: Flow,
    tickets: Vec<Ticket>,
    /// In group order: needs you first (inbox order), then flow order, other bases after the project's.
    owners: Vec<Owner>,
}

struct Owner {
    ticket: OwnerTicket,
    owned: Arc<Owned>,
    /// Flow order for the picker.
    flow_rank: (bool, u32),
}

impl Owner {
    fn file_owner(&self, entry: &ChangedEntry) -> FileOwner {
        FileOwner {
            key: self.ticket.key.clone(),
            status: self.ticket.status,
            branch: self.ticket.branch.clone(),
            base_branch: self.ticket.base_branch.clone(),
            other_base: self.ticket.other_base,
            needs_you: self.ticket.needs_you,
            file: entry.file.clone(),
            uncommitted: entry.uncommitted,
        }
    }
}

impl View {
    /// Every changed path with its owners, in group order.
    fn by_path(&self) -> BTreeMap<&str, Vec<FileOwner>> {
        let mut map: BTreeMap<&str, Vec<FileOwner>> = BTreeMap::new();
        for owner in &self.owners {
            for entry in &owner.owned.entries {
                map.entry(entry.file.path.as_str())
                    .or_default()
                    .push(owner.file_owner(entry));
            }
        }
        map
    }

    fn counts(&self, by_path: &BTreeMap<&str, Vec<FileOwner>>) -> ProjectCounts {
        let changing = |o: &&Owner| !o.owned.entries.is_empty();
        ProjectCounts {
            tickets: count(self.owners.iter().filter(changing).count()),
            changed: count(by_path.len()),
            needs_you: count(
                by_path
                    .values()
                    .filter(|owners| owners.iter().any(|o| o.needs_you))
                    .count(),
            ),
            needs_you_tickets: count(
                self.owners
                    .iter()
                    .filter(changing)
                    .filter(|o| o.ticket.needs_you)
                    .count(),
            ),
            same_file: count(by_path.values().filter(|o| same_file(o)).count()),
        }
    }
}

/// Two owners of one file on the same base (FIL-06: tickets on other bases are never compared).
fn same_file(owners: &[FileOwner]) -> bool {
    !same_base(owners).is_empty()
}

/// The owners of one file grouped by base, each base two or more of them share, in the owners' order.
fn same_base(owners: &[FileOwner]) -> Vec<(&str, Vec<&FileOwner>)> {
    let mut bases: Vec<(&str, Vec<&FileOwner>)> = Vec::new();
    for owner in owners {
        match bases.iter_mut().find(|(b, _)| *b == owner.base_branch) {
            Some((_, group)) => group.push(owner),
            None => bases.push((&owner.base_branch, vec![owner])),
        }
    }
    bases.retain(|(_, group)| group.len() > 1);
    bases
}

/// The Same file notes of a file read: one per base its owners share, each naming them furthest along first. Owners
/// without changes in `changed` (the base has no such text file) are compared by path alone.
fn same_file_notes(
    view: &View,
    owners: &[FileOwner],
    changed: &[(TicketKey, Vec<Change>)],
) -> Vec<SameFile> {
    let rank = |key: &TicketKey| {
        view.owners
            .iter()
            .find(|o| &o.ticket.key == key)
            .map(|o| o.flow_rank)
    };
    same_base(owners)
        .into_iter()
        .map(|(base, mut group)| {
            // Furthest along first: SHOP-19 in Security before SHOP-29 in Dev (page 28).
            group.sort_by_key(|o| (std::cmp::Reverse(rank(&o.key)), o.key.clone()));
            let group: Vec<_> = group
                .iter()
                .map(|o| {
                    let changes = changed.iter().find(|(k, _)| *k == o.key);
                    (&o.key, changes.map(|(_, c)| c.as_slice()))
                })
                .collect();
            same_file_note(base, &group)
        })
        .collect()
}

/// One change of an owner: its lines and the function git names in its hunk header.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Change {
    range: LineRange,
    function: Option<String>,
}

/// The changes of `-U0` hunks: one per changed run, each with its own function context.
fn changes(diff: &FileDiff) -> Vec<Change> {
    diff.hunks
        .iter()
        .map(|h| Change {
            range: LineRange {
                start: h.old_start,
                lines: h.old_lines,
            },
            function: function_name(&h.header),
        })
        .collect()
}

/// The identifier git's hunk header names (`export function applyDiscount(total: Money) {` → `applyDiscount`,
/// `impl Board {` → `Board`), or `None` when it names none plainly.
fn function_name(header: &str) -> Option<String> {
    const KEYWORDS: &[&str] = &[
        "async",
        "class",
        "const",
        "def",
        "enum",
        "export",
        "fn",
        "func",
        "function",
        "impl",
        "interface",
        "let",
        "module",
        "mod",
        "namespace",
        "pub",
        "static",
        "struct",
        "trait",
        "type",
        "var",
        "if",
        "for",
        "while",
        "switch",
        "catch",
        "return",
        "else",
        "match",
    ];
    const DECLARES: &[&str] = &[
        "class",
        "enum",
        "impl",
        "interface",
        "module",
        "mod",
        "namespace",
        "struct",
        "trait",
        "type",
    ];
    let ident = |c: char| c.is_alphanumeric() || c == '_' || c == '$';
    let plain = |word: &str| {
        word.chars().next().is_some_and(|c| !c.is_ascii_digit())
            && word.chars().all(ident)
            && !KEYWORDS.contains(&word)
    };
    // The name just before a `(` (or before `= (` for an arrow function), skipping generics.
    for (at, _) in header.match_indices('(') {
        let mut before = header[..at].trim_end();
        if let Some(rest) = before.strip_suffix('>')
            && let Some(open) = rest.rfind('<')
        {
            before = rest[..open].trim_end();
        }
        if let Some(rest) = before.strip_suffix('=') {
            before = rest.trim_end();
        }
        let word = &before[before.rfind(|c: char| !ident(c)).map_or(0, |i| i + 1)..];
        if plain(word) {
            return Some(word.to_owned());
        }
    }
    if header.contains('(') {
        return None;
    }
    // A declaration without parentheses: the name after `class`, `impl`, `struct` …
    let words: Vec<&str> = header
        .split(|c: char| !ident(c))
        .filter(|w| !w.is_empty())
        .collect();
    let at = words.iter().position(|w| DECLARES.contains(w))?;
    words
        .get(at + 1)
        .filter(|w| plain(w))
        .map(|w| (*w).to_owned())
}

/// The note of owners on one base (furthest along first, with their changes; `None`: not compared by lines, as for a
/// file the base doesn't have or a binary one).
fn same_file_note(base_branch: &str, owners: &[(&TicketKey, Option<&[Change]>)]) -> SameFile {
    let mut keys = Vec::new();
    let mut overlapping: Vec<&Change> = Vec::new();
    for (i, (key, changes)) in owners.iter().enumerate() {
        let mut named = false;
        for change in changes.unwrap_or_default() {
            let hits = owners.iter().enumerate().any(|(j, (_, other))| {
                j != i
                    && other
                        .unwrap_or_default()
                        .iter()
                        .any(|o| change.range.overlaps(o.range))
            });
            if hits {
                overlapping.push(change);
                named = true;
            }
        }
        if named {
            keys.push((*key).clone());
        }
    }
    if keys.is_empty() {
        keys = owners.iter().map(|(k, _)| (*k).clone()).collect();
    }
    let mut spans: Vec<LineSpan> = overlapping
        .iter()
        .map(|c| match c.range.lines {
            0 => LineSpan {
                first: c.range.start.max(1),
                last: c.range.start.max(1),
            },
            _ => LineSpan {
                first: c.range.start,
                last: c.range.last(),
            },
        })
        .collect();
    spans.sort_by_key(|s| (s.first, s.last));
    spans.dedup();
    let mut merged: Vec<LineSpan> = Vec::new();
    for span in spans {
        match merged.last_mut() {
            Some(last) if span.first <= last.last => last.last = last.last.max(span.last),
            _ => merged.push(span),
        }
    }
    let function = overlapping
        .first()
        .and_then(|c| c.function.clone())
        .filter(|name| {
            overlapping
                .iter()
                .all(|c| c.function.as_ref() == Some(name))
        });
    let note = note_text(&keys, &merged, function.as_deref());
    SameFile {
        base_branch: base_branch.to_owned(),
        keys,
        spans: merged,
        function,
        note,
    }
}

/// "SHOP-19 and SHOP-29 both edit applyDiscount (lines 41–58). Whichever merges second will rebase on the
/// other." / "SHOP-19 and SHOP-29 both change this file."; three or more: "SHOP-19, SHOP-29 and SHOP-33 all …".
fn note_text(keys: &[TicketKey], spans: &[LineSpan], function: Option<&str>) -> String {
    let keys: Vec<String> = keys.iter().map(ToString::to_string).collect();
    let (names, all) = match keys.split_last() {
        Some((last, rest)) if !rest.is_empty() => {
            (format!("{} and {last}", rest.join(", ")), keys.len() > 2)
        }
        _ => (keys.concat(), false),
    };
    let both = if all { "all" } else { "both" };
    if spans.is_empty() {
        return format!("{names} {both} change this file.");
    }
    let each: Vec<String> = spans
        .iter()
        .map(|s| match s.first == s.last {
            true => s.first.to_string(),
            false => format!("{}\u{2013}{}", s.first, s.last),
        })
        .collect();
    let list = match each.split_last() {
        Some((last, rest)) if !rest.is_empty() => format!("{} and {last}", rest.join(", ")),
        _ => each.concat(),
    };
    let one_line = spans.len() == 1 && spans[0].first == spans[0].last;
    let lines = format!("{} {list}", if one_line { "line" } else { "lines" });
    let what = match function {
        Some(name) => format!("{name} ({lines})"),
        None => lines,
    };
    let then = if all {
        "Each one that merges later will rebase on the ones before it."
    } else {
        "Whichever merges second will rebase on the other."
    };
    format!("{names} {both} edit {what}. {then}")
}

/// Only tickets on other bases have it, and the base doesn't.
fn not_on_base(on_base: bool, owners: &[FileOwner]) -> bool {
    !on_base && !owners.is_empty() && owners.iter().all(|o| o.other_base)
}

impl Board {
    /// The cache of owners' changes the project Files view reads.
    pub fn project_files(&self) -> &ProjectFilesCache {
        &self.project_files
    }

    /// `files.project_tree`: the entries of `folder` (empty: the top) on the base, with every ticket's changes on
    /// top. `base_only`: the base's entries alone, without reading any ticket's changes.
    pub async fn files_project_tree(
        &self,
        project: &str,
        folder: &str,
        base_only: bool,
    ) -> Result<ProjectTree, BoardError> {
        let folder = clean_path(folder, true)?;
        let (project, source) = self.project_source(project).await?;
        let git = self.git()?;
        let repo = PathBuf::from(&project.repo_path);
        let base_ref = format!("refs/heads/{}", source.base_branch);
        let mut entries = list_folder(git, &repo, &base_ref, &folder).await?;
        let on_base: HashSet<String> = entries.keys().cloned().collect();
        if base_only {
            if entries.is_empty() && !folder.is_empty() {
                return Err(not_a_folder(&folder, &source));
            }
            let mut list: Vec<ProjectEntry> = entries
                .into_iter()
                .map(|(path, kind)| entry(path, kind, Vec::new(), 0, false))
                .collect();
            list.sort_by(|a, b| entry_order((a.kind, &a.name), (b.kind, &b.name)));
            return Ok(ProjectTree {
                source,
                path: folder,
                entries: list,
                counts: None,
            });
        }

        let view = self.project_view(project, source).await?;
        let by_path = view.by_path();
        // Files the base doesn't have: added ones, and a ticket's on another base.
        for path in by_path.keys() {
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
                    entries.entry((*path).to_owned()).or_insert(EntryKind::File);
                }
            }
        }
        if entries.is_empty() && !folder.is_empty() {
            return Err(not_a_folder(&folder, &view.source));
        }
        let mut list: Vec<ProjectEntry> = entries
            .into_iter()
            .map(|(path, kind)| {
                if kind == EntryKind::Folder {
                    let inside = by_path.keys().filter(|p| under(p, &path).is_some()).count();
                    entry(path, kind, Vec::new(), count(inside), false)
                } else {
                    let owners = by_path.get(path.as_str()).cloned().unwrap_or_default();
                    let not_on_base = not_on_base(on_base.contains(&path), &owners);
                    entry(path, kind, owners, 0, not_on_base)
                }
            })
            .collect();
        list.sort_by(|a, b| entry_order((a.kind, &a.name), (b.kind, &b.name)));
        Ok(ProjectTree {
            counts: Some(view.counts(&by_path)),
            source: view.source,
            path: folder,
            entries: list,
        })
    }

    /// `files.project_search`: file names on the base and the owners' changed files matching `query`, ranked as
    /// `files.search` ranks them (changed files before unchanged ones).
    pub async fn files_project_search(
        &self,
        project: &str,
        query: &str,
    ) -> Result<ProjectSearch, BoardError> {
        let (project, source) = self.project_source(project).await?;
        let query = query.trim().to_owned();
        if query.is_empty() {
            return Ok(ProjectSearch {
                source,
                query,
                matches: Vec::new(),
                total: 0,
            });
        }
        let git = self.git()?;
        let repo = PathBuf::from(&project.repo_path);
        let base_ref = format!("refs/heads/{}", source.base_branch);
        let all = git
            .run(&repo, &["ls-tree", "-r", "--name-only", "-z", &base_ref])
            .await?;
        let on_base: HashSet<String> = nul_paths(&all).collect();
        let view = self.project_view(project, source).await?;
        let by_path = view.by_path();
        let mut paths: HashSet<&str> = on_base.iter().map(String::as_str).collect();
        paths.extend(by_path.keys().copied());

        let needle = query.to_lowercase();
        let mut ranked: Vec<(u8, bool, usize, &str, [u32; 2])> = paths
            .into_iter()
            .filter_map(|path| {
                let (rank, hit) = rank(path, &needle)?;
                let unchanged = !by_path.contains_key(path);
                Some((rank, unchanged, path.chars().count(), path, hit))
            })
            .collect();
        ranked.sort();
        let total = count(ranked.len());
        let matches = ranked
            .into_iter()
            .take(SEARCH_LIMIT)
            .map(|(_, _, _, path, hit)| {
                let (folder, name) = match path.rsplit_once('/') {
                    Some((folder, name)) => (folder.to_owned(), name.to_owned()),
                    None => (String::new(), path.to_owned()),
                };
                let owners = by_path.get(path).cloned().unwrap_or_default();
                ProjectMatch {
                    not_on_base: not_on_base(on_base.contains(path), &owners),
                    path: path.to_owned(),
                    name,
                    folder,
                    hit,
                    owners,
                }
            })
            .collect();
        Ok(ProjectSearch {
            source: view.source,
            query,
            matches,
            total,
        })
    }

    /// `files.project_read`: the file as it is on the base, with its owners. A file the base doesn't have (only
    /// on another base, or added by a ticket) carries its first owner's diff.
    pub async fn files_project_read(
        &self,
        project: &str,
        path: &str,
    ) -> Result<ProjectFile, BoardError> {
        let path = clean_path(path, false)?;
        let (project, source) = self.project_source(project).await?;
        let git = self.git()?;
        let repo = PathBuf::from(&project.repo_path);
        let base_ref = format!("refs/heads/{}", source.base_branch);
        let object = format!("{base_ref}:{path}");
        let kind = git.run(&repo, &["cat-file", "-t", &object]).await;
        let blob = match kind.as_deref().map(str::trim) {
            Ok("blob") => git.read_blob(&repo, &base_ref, &path).await?,
            _ => None,
        };
        let view = self.project_view(project, source).await?;
        let owners = view.by_path().remove(path.as_str()).unwrap_or_default();
        let mut file = ProjectFile {
            path: path.clone(),
            on_base: blob.is_some(),
            size: 0,
            binary: false,
            text: String::new(),
            truncated: false,
            not_on_base: not_on_base(blob.is_some(), &owners),
            diff: None,
            diff_of: None,
            lines: Vec::new(),
            same_file: Vec::new(),
            owners,
            source: view.source.clone(),
        };
        let mut changed = Vec::new();
        match blob {
            Some((bytes, size)) => {
                (file.binary, file.text, file.truncated) = text_of(&bytes, size);
                file.size = size;
                if !file.binary {
                    changed = self.owner_changes(&view, &file.owners, &path).await?;
                }
            }
            None => {
                let Some(first) = file.owners.first() else {
                    return Err(BoardError::NotFound(format!(
                        "{path} isn't a file on {} or in any ticket's changes",
                        file.source.base_branch
                    )));
                };
                let key = first.key.clone();
                file.diff = Some(self.files_diff(&key.to_string(), &path, None).await?);
                file.diff_of = Some(key);
            }
        }
        file.same_file = same_file_notes(&view, &file.owners, &changed);
        file.lines = changed
            .into_iter()
            .map(|(key, changes)| OwnerLines {
                key,
                ranges: changes.into_iter().map(|c| c.range).collect(),
            })
            .collect();
        Ok(file)
    }

    /// Each owner's changes of `path` (clean), from its own `-U0` diff, so each changed run is a hunk with its own
    /// function context. Read only for the file asked for, and not cached: one `git diff` per owner of it.
    async fn owner_changes(
        &self,
        view: &View,
        owners: &[FileOwner],
        path: &str,
    ) -> Result<Vec<(TicketKey, Vec<Change>)>, BoardError> {
        let mut changed = Vec::new();
        for owner in owners {
            let Some(ticket) = view.tickets.iter().find(|t| t.key == owner.key) else {
                continue;
            };
            let diff = self.ticket_file_diff(ticket, path, 0).await?;
            changed.push((
                owner.key.clone(),
                diff.as_ref().map(changes).unwrap_or_default(),
            ));
        }
        Ok(changed)
    }

    /// `files.project_diff`: one owner's change of `path`, as `files.diff` gives it on the ticket's Files tab.
    pub async fn files_project_diff(
        &self,
        project: &str,
        path: &str,
        ticket: &str,
    ) -> Result<FileDiff, BoardError> {
        let path = clean_path(path, false)?;
        let (project, source) = self.project_source(project).await?;
        let wanted = self.ticket(ticket).await?;
        let view = self.project_view(project, source).await?;
        if !view.owners.iter().any(|o| o.ticket.key == wanted.key) {
            return Err(BoardError::NotFound(format!(
                "{} isn't changing files in {}",
                wanted.key, view.project.name
            )));
        }
        self.files_diff(&wanted.key.to_string(), &path, None).await
    }

    /// `files.project_changes`: every owner's changed files, grouped by ticket.
    pub async fn files_project_changes(&self, project: &str) -> Result<ProjectChanges, BoardError> {
        let (project, source) = self.project_source(project).await?;
        let view = self.project_view(project, source).await?;
        let counts = view.counts(&view.by_path());
        let groups = view
            .owners
            .iter()
            .filter(|o| !o.owned.entries.is_empty())
            .map(|o| ChangeGroup {
                ticket: o.ticket.clone(),
                files: o.owned.entries.clone(),
            })
            .collect();
        Ok(ProjectChanges {
            source: view.source,
            groups,
            counts,
        })
    }

    /// `files.project_sources`: the picker's rows.
    pub async fn files_project_sources(&self, project: &str) -> Result<ProjectSources, BoardError> {
        let (project, source) = self.project_source(project).await?;
        let git = self.git()?;
        let repo = PathBuf::from(&project.repo_path);
        let mut view = self.project_view(project, source).await?;
        let owning: HashSet<&TicketKey> = view.owners.iter().map(|o| &o.ticket.key).collect();

        let mut epics = Vec::new();
        for epic in view.tickets.iter().filter(|t| t.is_epic() && open(t)) {
            let Some(branch) = epic.work.as_ref().and_then(|w| w.branch.clone()) else {
                continue;
            };
            if !super::files::branch_exists(git, &repo, &branch).await {
                continue;
            }
            let children = view
                .tickets
                .iter()
                .filter(|t| t.epic.as_ref() == Some(&epic.key));
            epics.push(EpicSource {
                key: epic.key.clone(),
                title: epic.title.clone(),
                status: epic.status,
                merged: children
                    .clone()
                    .filter(|c| c.status == Status::Merged)
                    .map(|c| c.key.clone())
                    .collect(),
                on_top: children
                    .filter(|c| is_running(c.status) && worktree_on_disk(c).is_some())
                    .map(|c| c.key.clone())
                    .collect(),
                branch,
            });
        }
        let no_worktree = view
            .tickets
            .iter()
            .filter(|t| !t.is_epic() && open(t) && !owning.contains(&t.key))
            .map(|t| NoWorktree {
                key: t.key.clone(),
                title: t.title.clone(),
                status: t.status,
                stage: stage_name(t, &view.flow),
            })
            .collect();
        view.owners
            .sort_by(|a, b| (a.flow_rank, &a.ticket.key).cmp(&(b.flow_rank, &b.ticket.key)));
        let yours = self.user_worktrees(&view.project, &view.tickets).await?;
        Ok(ProjectSources {
            tickets: view.owners.into_iter().map(|o| o.ticket).collect(),
            source: view.source,
            epics,
            no_worktree,
            yours,
        })
    }

    /// The user's own worktrees of the project's repository (#43), by folder: `git worktree list` without the main
    /// checkout, bare or prunable entries, folders that are gone, and Otaq's own (under its data folder, or a
    /// ticket's worktree wherever it is).
    async fn user_worktrees(
        &self,
        project: &Project,
        tickets: &[Ticket],
    ) -> Result<Vec<UserWorktree>, BoardError> {
        let git = self.git()?;
        let repo = Path::new(&project.repo_path);
        let list = git.run(repo, &["worktree", "list", "--porcelain"]).await?;
        let real = |p: &Path| crate::paths::canonical(p).unwrap_or_else(|_| p.to_owned());
        let main = real(repo);
        let ours = real(self.data.root());
        let tickets: HashSet<PathBuf> = tickets
            .iter()
            .filter_map(worktree_on_disk)
            .map(|p| real(&p))
            .collect();
        let mut found = Vec::new();
        for block in list.split("\n\n") {
            let (mut path, mut head, mut branch, mut skip) = (None, None, None, false);
            for line in block.lines() {
                if let Some(p) = line.strip_prefix("worktree ") {
                    path = Some(PathBuf::from(p));
                } else if let Some(h) = line.strip_prefix("HEAD ") {
                    head = Some(h.to_owned());
                } else if let Some(b) = line.strip_prefix("branch ") {
                    branch = Some(b.strip_prefix("refs/heads/").unwrap_or(b).to_owned());
                } else if line == "bare" || line.starts_with("prunable") {
                    skip = true;
                }
            }
            let (Some(path), Some(head)) = (path, head) else {
                continue;
            };
            let at = real(&path);
            if skip
                || !path.is_dir()
                || at == main
                || at.starts_with(&ours)
                || tickets.contains(&at)
            {
                continue;
            }
            found.push(UserWorktree {
                path: path.display().to_string(),
                branch,
                head_short: short_sha(&head),
                head,
            });
        }
        found.sort_by(|a, b| a.path.cmp(&b.path));
        Ok(found)
    }

    /// One of the user's worktrees as the view's source (`worktree`: its folder, as the picker gave it); refused for
    /// any other folder.
    async fn worktree_source(
        &self,
        key: &str,
        worktree: &str,
    ) -> Result<(Project, ProjectSource), BoardError> {
        let project = self.project(key).await?;
        let tickets = self.tickets(&project.key).await?;
        let real = |p: &Path| crate::paths::canonical(p).unwrap_or_else(|_| p.to_owned());
        let wanted = real(Path::new(worktree));
        let found = self
            .user_worktrees(&project, &tickets)
            .await?
            .into_iter()
            .find(|w| real(Path::new(&w.path)) == wanted)
            .ok_or_else(|| {
                BoardError::NotFound(format!(
                    "{worktree} isn't one of your worktrees of {}",
                    project.name
                ))
            })?;
        let source = ProjectSource {
            project: project.key.clone(),
            base_branch: found.label().to_owned(),
            base_sha: found.head.clone(),
            base_short: found.head_short.clone(),
            worktree: Some(found.path),
        };
        Ok((project, source))
    }

    /// `files.project_tree` of one of your worktrees (#43): the entries of `folder` at its HEAD, from git objects,
    /// with no ticket's changes.
    pub async fn files_worktree_tree(
        &self,
        project: &str,
        worktree: &str,
        folder: &str,
    ) -> Result<ProjectTree, BoardError> {
        let folder = clean_path(folder, true)?;
        let (project, source) = self.worktree_source(project, worktree).await?;
        let repo = PathBuf::from(&project.repo_path);
        let entries = list_folder(self.git()?, &repo, &source.base_sha, &folder).await?;
        if entries.is_empty() && !folder.is_empty() {
            return Err(not_a_folder(&folder, &source));
        }
        let mut list: Vec<ProjectEntry> = entries
            .into_iter()
            .map(|(path, kind)| entry(path, kind, Vec::new(), 0, false))
            .collect();
        list.sort_by(|a, b| entry_order((a.kind, &a.name), (b.kind, &b.name)));
        Ok(ProjectTree {
            source,
            path: folder,
            entries: list,
            counts: None,
        })
    }

    /// `files.project_search` of one of your worktrees: file names at its HEAD, ranked as `files.search` ranks them.
    pub async fn files_worktree_search(
        &self,
        project: &str,
        worktree: &str,
        query: &str,
    ) -> Result<ProjectSearch, BoardError> {
        let (project, source) = self.worktree_source(project, worktree).await?;
        let query = query.trim().to_owned();
        let mut ranked: Vec<(u8, usize, String, [u32; 2])> = Vec::new();
        if !query.is_empty() {
            let repo = PathBuf::from(&project.repo_path);
            let all = self
                .git()?
                .run(
                    &repo,
                    &["ls-tree", "-r", "--name-only", "-z", &source.base_sha],
                )
                .await?;
            let needle = query.to_lowercase();
            ranked = nul_paths(&all)
                .filter_map(|path| {
                    let (rank, hit) = rank(&path, &needle)?;
                    Some((rank, path.chars().count(), path, hit))
                })
                .collect();
            ranked.sort();
        }
        let total = count(ranked.len());
        let matches = ranked
            .into_iter()
            .take(SEARCH_LIMIT)
            .map(|(_, _, path, hit)| {
                let (folder, name) = match path.rsplit_once('/') {
                    Some((folder, name)) => (folder.to_owned(), name.to_owned()),
                    None => (String::new(), path.clone()),
                };
                ProjectMatch {
                    path,
                    name,
                    folder,
                    hit,
                    owners: Vec::new(),
                    not_on_base: false,
                }
            })
            .collect();
        Ok(ProjectSearch {
            source,
            query,
            matches,
            total,
        })
    }

    /// `files.project_read` of one of your worktrees: the file at its HEAD, from git objects, read-only.
    pub async fn files_worktree_read(
        &self,
        project: &str,
        worktree: &str,
        path: &str,
    ) -> Result<ProjectFile, BoardError> {
        let path = clean_path(path, false)?;
        let (project, source) = self.worktree_source(project, worktree).await?;
        let git = self.git()?;
        let repo = PathBuf::from(&project.repo_path);
        let object = format!("{}:{path}", source.base_sha);
        let kind = git.run(&repo, &["cat-file", "-t", &object]).await;
        let blob = match kind.as_deref().map(str::trim) {
            Ok("blob") => git.read_blob(&repo, &source.base_sha, &path).await?,
            _ => None,
        };
        let Some((bytes, size)) = blob else {
            return Err(BoardError::NotFound(format!(
                "{path} isn't a file on {}",
                source.base_branch
            )));
        };
        let (binary, text, truncated) = text_of(&bytes, size);
        Ok(ProjectFile {
            source,
            path,
            on_base: true,
            size,
            binary,
            text,
            truncated,
            owners: Vec::new(),
            not_on_base: false,
            diff: None,
            diff_of: None,
            lines: Vec::new(),
            same_file: Vec::new(),
        })
    }

    /// The project and its base: `refs/heads/<base>` must exist in its repository.
    async fn project_source(&self, key: &str) -> Result<(Project, ProjectSource), BoardError> {
        let project = self.project(key).await?;
        let git = self.git()?;
        let spec = format!("refs/heads/{}^{{commit}}", project.base_branch);
        let sha = git
            .run(
                Path::new(&project.repo_path),
                &["rev-parse", "--verify", "--quiet", &spec],
            )
            .await
            .map_err(|_| {
                BoardError::NotFound(format!(
                    "{}'s base branch {} isn't in its repository",
                    project.name, project.base_branch
                ))
            })?;
        let sha = sha.trim().to_owned();
        let source = ProjectSource {
            project: project.key.clone(),
            base_branch: project.base_branch.clone(),
            base_short: short_sha(&sha),
            base_sha: sha,
            worktree: None,
        };
        Ok((project, source))
    }

    /// The project's tickets with each owner's changes (from the cache where they still hold), in group order.
    async fn project_view(
        &self,
        project: Project,
        source: ProjectSource,
    ) -> Result<View, BoardError> {
        let git = self.git()?;
        let tickets = self.tickets(&project.key).await?;
        let flow = self.effective_flow(&project.key).await?;
        let needs_you: HashMap<TicketKey, usize> = needs_you_items_with(
            &[ProjectTickets {
                key: &project.key,
                name: &project.name,
                tickets: &tickets,
                flow: &flow,
            }],
            now_ms(),
            &self.limit_stops().snapshot(),
        )
        .into_iter()
        .enumerate()
        .map(|(i, item)| (item.key, i))
        .collect();
        let tips = Arc::new(branch_tips(git, Path::new(&project.repo_path)).await?);

        // Every owner's changes side by side (each is several `git` calls, and a slow one in a big repository).
        let candidates: Vec<&Ticket> = tickets.iter().filter(|t| !t.is_epic() && open(t)).collect();
        let mut read: Vec<Option<Arc<Owned>>> = vec![None; candidates.len()];
        {
            let _reading = self.project_files().reading.lock().await;
            let mut reads = JoinSet::new();
            for (i, ticket) in candidates.iter().enumerate() {
                let (board, ticket, tips) = (self.clone(), (*ticket).clone(), tips.clone());
                reads.spawn(async move { (i, board.owned(&ticket, &tips).await) });
            }
            while let Some(done) = reads.join_next().await {
                let (i, owned) = done.unwrap_or_else(|e| std::panic::resume_unwind(e.into_panic()));
                read[i] = owned?;
            }
        }

        let mut owners = Vec::new();
        for (ticket, owned) in candidates.into_iter().zip(read) {
            let Some(owned) = owned else {
                continue;
            };
            let (files, added, removed) = owned.entries.iter().fold((0, 0u32, 0u32), |acc, e| {
                (
                    acc.0 + 1,
                    acc.1.saturating_add(e.file.added),
                    acc.2.saturating_add(e.file.removed),
                )
            });
            let other_base = owned.base_branch != source.base_branch;
            owners.push((
                needs_you.get(&ticket.key).copied(),
                Owner {
                    ticket: OwnerTicket {
                        key: ticket.key.clone(),
                        title: ticket.title.clone(),
                        status: ticket.status,
                        stage: stage_name(ticket, &flow),
                        from: owned.from,
                        branch: owned.branch.clone(),
                        base_branch: owned.base_branch.clone(),
                        other_base,
                        needs_you: needs_you.contains_key(&ticket.key),
                        epic: ticket.epic.clone(),
                        files,
                        added,
                        removed,
                    },
                    flow_rank: (other_base, flow_rank(ticket, &flow)),
                    owned,
                },
            ));
        }
        // Needs you first, in inbox order; then flow order, other bases after the project's.
        owners.sort_by(|(a_needs, a), (b_needs, b)| {
            (a_needs.is_none(), a_needs, a.flow_rank, &a.ticket.key).cmp(&(
                b_needs.is_none(),
                b_needs,
                b.flow_rank,
                &b.ticket.key,
            ))
        });
        // Forget the project's tickets that stopped being owners (Done, closed).
        let owning: HashSet<&TicketKey> = owners.iter().map(|(_, o)| &o.ticket.key).collect();
        let gone: Vec<&str> = tickets
            .iter()
            .filter(|t| !owning.contains(&t.key))
            .map(|t| t.id.as_str())
            .collect();
        {
            let mut state = self.project_files().lock();
            for id in gone {
                state.owners.remove(id);
            }
        }
        Ok(View {
            project,
            source,
            flow,
            tickets,
            owners: owners.into_iter().map(|(_, o)| o).collect(),
        })
    }

    /// One ticket's changes: cached while its stamp holds (and, while its agent works, for [`WORKING_FRESH`]),
    /// else read from git as its Files tab reads them. `None`: it has no worktree or branch to read yet.
    async fn owned(
        &self,
        ticket: &Ticket,
        tips: &HashMap<String, String>,
    ) -> Result<Option<Arc<Owned>>, BoardError> {
        let Some(stamp) = stamp(self.git()?, ticket, tips).await else {
            return Ok(None);
        };
        let cache = self.project_files();
        let cached = cache
            .lock()
            .owners
            .get(&ticket.id)
            .filter(|c| {
                c.stamp == stamp && (!is_running(ticket.status) || c.at.elapsed() < WORKING_FRESH)
            })
            .map(|c| c.owned.clone());
        if let Some(owned) = cached {
            return Ok(Some(owned));
        }
        let source = self.files_source(ticket).await?;
        if source.info.from == FilesFrom::Base {
            cache.lock().owners.remove(&ticket.id);
            return Ok(None);
        }
        let entries = self.files_changes_of(&source).await?.entries;
        let owned = Arc::new(Owned {
            from: source.info.from,
            branch: source.info.branch,
            base_branch: source.info.base_branch,
            entries,
        });
        let mut state = cache.lock();
        state.recomputes += 1;
        state.owners.insert(
            ticket.id.clone(),
            Cached {
                stamp,
                at: Instant::now(),
                owned: owned.clone(),
            },
        );
        Ok(Some(owned))
    }
}

fn entry(
    path: String,
    kind: EntryKind,
    owners: Vec<FileOwner>,
    changed_below: u32,
    not_on_base: bool,
) -> ProjectEntry {
    ProjectEntry {
        name: path.rsplit('/').next().unwrap_or(&path).to_owned(),
        path,
        kind,
        owners,
        changed_below,
        not_on_base,
    }
}

fn not_a_folder(folder: &str, source: &ProjectSource) -> BoardError {
    BoardError::NotFound(format!("{folder} isn't a folder on {}", source.base_branch))
}

/// Not Done or closed.
fn open(ticket: &Ticket) -> bool {
    ticket.status.set() != StatusSet::Done
}

fn worktree_on_disk(ticket: &Ticket) -> Option<PathBuf> {
    ticket
        .work
        .as_ref()
        .and_then(|w| w.worktree.as_deref())
        .map(PathBuf::from)
        .filter(|wt| wt.is_dir())
}

/// Every branch's tip in the repository, by name, in one `git` call.
async fn branch_tips(git: &Git, repo: &Path) -> Result<HashMap<String, String>, BoardError> {
    let out = git
        .run(
            repo,
            &[
                "for-each-ref",
                "--format=%(objectname) %(refname:strip=2)",
                "refs/heads",
            ],
        )
        .await?;
    Ok(out
        .lines()
        .filter_map(|line| line.split_once(' '))
        .map(|(sha, name)| (name.to_owned(), sha.to_owned()))
        .collect())
}

/// The ticket's stamp: its fields, and its worktree's HEAD and index (one `git` call) or its branch's tip. `None`
/// when it has nothing to read yet: no branch, no `base_sha`, or neither a worktree nor the branch.
async fn stamp(git: &Git, ticket: &Ticket, tips: &HashMap<String, String>) -> Option<Stamp> {
    let work = ticket.work.as_ref()?;
    let branch = work.branch.as_deref()?;
    work.base_sha.as_ref()?;
    let (head, index_mtime) = match worktree_on_disk(ticket) {
        Some(wt) => {
            let out = git
                .run(&wt, &["rev-parse", "HEAD", "--git-path", "index"])
                .await
                .unwrap_or_default();
            let mut lines = out.lines();
            let head = lines.next().unwrap_or_default().to_owned();
            let index_mtime = match lines.next() {
                Some(index) => tokio::fs::metadata(wt.join(index))
                    .await
                    .and_then(|m| m.modified())
                    .ok(),
                None => None,
            };
            (head, index_mtime)
        }
        None => (tips.get(branch)?.clone(), None),
    };
    Some(Stamp {
        status: ticket.status,
        status_since: ticket.status_since,
        updated_at: ticket.updated_at,
        place: ticket.place.clone(),
        base: ticket.base.clone(),
        work: work.clone(),
        head,
        index_mtime,
    })
}

/// The stage a ticket is in or waits before, by name ("Dev"); `None` in Backlog, Ready for Dev or Done.
fn stage_name(ticket: &Ticket, flow: &Flow) -> Option<String> {
    let id = ticket.place.stage_id()?;
    match flow.stage(id) {
        Some(stage) => Some(stage.name.clone()),
        None if id == REVIEW_EPIC => Some(epic_stop(flow).name),
        None => None,
    }
}

/// Where a ticket is in its flow, left to right: Backlog, Ready for Dev, then each stage (waiting before it, in
/// it), Done last.
fn flow_rank(ticket: &Ticket, flow: &Flow) -> u32 {
    let at = |id: &str| {
        flow.position(id)
            .map_or(u32::MAX - 1, |i| 2 + 2 * count(i).min(u32::MAX / 4))
    };
    match &ticket.place {
        Place::Backlog => 0,
        Place::Waiting(None) => 1,
        Place::Waiting(Some(id)) => at(id),
        Place::InStage(id) => at(id).saturating_add(1),
        Place::Done => u32::MAX,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::diff::FileStatus;

    fn owner(key: &str, base: &str, other_base: bool) -> FileOwner {
        FileOwner {
            key: key.parse().unwrap(),
            status: Status::Working,
            branch: crate::naming::branch(key),
            base_branch: base.to_owned(),
            other_base,
            needs_you: false,
            file: ChangedFile {
                status: FileStatus::Modified,
                path: "src/lib/pricing.ts".into(),
                old_path: None,
                added: 1,
                removed: 0,
                binary: false,
                size: None,
                old_size: None,
                similarity: None,
            },
            uncommitted: false,
        }
    }

    #[test]
    fn same_file_only_counts_tickets_on_one_base() {
        let both = [
            owner("SHOP-19", "main", false),
            owner("SHOP-29", "main", false),
        ];
        assert!(same_file(&both));
        let apart = [
            owner("SHOP-29", "main", false),
            owner("SHOP-36", "release/2.3", true),
        ];
        assert!(!same_file(&apart));
        assert!(!same_file(&both[..1]));
    }

    fn key(key: &str) -> TicketKey {
        key.parse().unwrap()
    }

    fn change(start: u32, lines: u32, function: Option<&str>) -> Change {
        Change {
            range: LineRange { start, lines },
            function: function.map(str::to_owned),
        }
    }

    #[test]
    fn function_names_come_from_the_hunk_header_alone() {
        let name = |h: &str| function_name(h);
        assert_eq!(
            name("export function applyDiscount(total: Money, coupon?: Coupon): Money {"),
            Some("applyDiscount".into())
        );
        assert_eq!(
            name("pub async fn total<T>(x: T) -> u32 {"),
            Some("total".into())
        );
        assert_eq!(
            name("def apply_tier(self, tier):"),
            Some("apply_tier".into())
        );
        assert_eq!(
            name("func (s *Server) Handle(w http.ResponseWriter) {"),
            Some("Handle".into())
        );
        assert_eq!(
            name("const applyTier = (total) => {"),
            Some("applyTier".into())
        );
        assert_eq!(name("impl Board {"), Some("Board".into()));
        assert_eq!(name("class Cart extends Base {"), Some("Cart".into()));
        // Nothing named plainly: no name, never a guess.
        assert_eq!(name(""), None);
        assert_eq!(name("import { Money } from './money'"), None);
        assert_eq!(name("if (coupon) {"), None);
        assert_eq!(name("export const TIER_RATES = { silver: 0.05 }"), None);
    }

    #[test]
    fn ranges_overlap_on_shared_lines_and_additions_inside() {
        let r = |start, lines| LineRange { start, lines };
        assert!(r(41, 12).overlaps(r(47, 12)));
        assert!(r(41, 12).overlaps(r(52, 1)));
        assert!(!r(41, 12).overlaps(r(53, 6)));
        // New lines after line 45 land between two of 41–52's lines.
        assert!(r(45, 0).overlaps(r(41, 12)));
        assert!(r(41, 12).overlaps(r(41, 0)));
        // Right before or right after the range: next to it, not inside.
        assert!(!r(40, 0).overlaps(r(41, 12)));
        assert!(!r(52, 0).overlaps(r(41, 12)));
        // Both adding at one place.
        assert!(r(10, 0).overlaps(r(10, 0)));
        assert!(!r(10, 0).overlaps(r(11, 0)));
    }

    #[test]
    fn the_note_names_the_function_and_the_merged_lines() {
        let (k19, k29) = (key("SHOP-19"), key("SHOP-29"));
        let a = [
            change(47, 12, Some("applyDiscount")),
            change(70, 1, Some("applyTier")),
        ];
        let b = [change(41, 12, Some("applyDiscount"))];
        let note = same_file_note("main", &[(&k19, Some(&a[..])), (&k29, Some(&b[..]))]);
        assert_eq!(
            note.note,
            "SHOP-19 and SHOP-29 both edit applyDiscount (lines 41\u{2013}58). Whichever merges second will rebase on the other."
        );
        assert_eq!(
            note.spans,
            [LineSpan {
                first: 41,
                last: 58
            }]
        );
        assert_eq!(note.function.as_deref(), Some("applyDiscount"));
        assert_eq!(note.keys, [k19.clone(), k29.clone()]);

        // Overlaps in two functions: no single name.
        let a = [
            change(47, 12, Some("applyDiscount")),
            change(70, 2, Some("applyTier")),
        ];
        let b = [
            change(41, 12, Some("applyDiscount")),
            change(71, 1, Some("applyTier")),
        ];
        let note = same_file_note("main", &[(&k19, Some(&a[..])), (&k29, Some(&b[..]))]);
        assert_eq!(
            note.note,
            "SHOP-19 and SHOP-29 both edit lines 41\u{2013}58 and 70\u{2013}71. Whichever merges second will rebase on the other."
        );
        assert_eq!(note.function, None);

        // A header without a plain name on one side: no name.
        let a = [change(5, 1, Some("round"))];
        let b = [change(5, 1, None)];
        let note = same_file_note("main", &[(&k19, Some(&a[..])), (&k29, Some(&b[..]))]);
        assert_eq!(
            note.note,
            "SHOP-19 and SHOP-29 both edit line 5. Whichever merges second will rebase on the other."
        );
    }

    #[test]
    fn without_overlapping_lines_the_note_is_plain() {
        let (k19, k29) = (key("SHOP-19"), key("SHOP-29"));
        let a = [change(2, 1, None)];
        let b = [change(9, 3, None), change(1, 0, None)];
        let note = same_file_note("main", &[(&k19, Some(&a[..])), (&k29, Some(&b[..]))]);
        assert_eq!(note.note, "SHOP-19 and SHOP-29 both change this file.");
        assert!(note.spans.is_empty() && note.function.is_none());
        // Not compared by lines (a file the base doesn't have): plain too.
        let note = same_file_note("main", &[(&k19, None), (&k29, None)]);
        assert_eq!(note.note, "SHOP-19 and SHOP-29 both change this file.");
    }

    #[test]
    fn three_owners_are_listed_with_all() {
        let (k1, k2, k3) = (key("SHOP-29"), key("SHOP-33"), key("SHOP-19"));
        let a = [change(5, 1, Some("round"))];
        let b = [change(5, 0, Some("round"))];
        let c = [change(4, 3, Some("round"))];
        let note = same_file_note(
            "main",
            &[
                (&k1, Some(&a[..])),
                (&k2, Some(&b[..])),
                (&k3, Some(&c[..])),
            ],
        );
        assert_eq!(
            note.note,
            "SHOP-29, SHOP-33 and SHOP-19 all edit round (lines 4\u{2013}6). Each one that merges later will rebase on the ones before it."
        );
        let plain = same_file_note("main", &[(&k1, None), (&k2, None), (&k3, None)]);
        assert_eq!(
            plain.note,
            "SHOP-29, SHOP-33 and SHOP-19 all change this file."
        );
        // Only the two whose lines overlap are named; the third changes elsewhere.
        let far = [change(90, 1, None)];
        let note = same_file_note(
            "main",
            &[
                (&k1, Some(&a[..])),
                (&k2, Some(&far[..])),
                (&k3, Some(&c[..])),
            ],
        );
        assert_eq!(note.keys, [k1, k3]);
        assert!(
            note.note
                .starts_with("SHOP-29 and SHOP-19 both edit round (lines 4")
        );
    }

    #[test]
    fn same_base_groups_only_shared_bases() {
        let owners = [
            owner("SHOP-19", "main", false),
            owner("SHOP-36", "release/2.3", true),
            owner("SHOP-29", "main", false),
        ];
        let groups: Vec<_> = same_base(&owners)
            .into_iter()
            .map(|(b, g)| (b, g.iter().map(|o| o.key.to_string()).collect::<Vec<_>>()))
            .collect();
        assert_eq!(
            groups,
            [("main", vec!["SHOP-19".to_owned(), "SHOP-29".to_owned()])]
        );
    }

    #[test]
    fn not_on_base_needs_every_owner_elsewhere() {
        let elsewhere = [owner("SHOP-36", "release/2.3", true)];
        assert!(not_on_base(false, &elsewhere));
        assert!(!not_on_base(true, &elsewhere));
        // A ticket on the base adding the file: an added file, not "not on main".
        let added = [owner("SHOP-33", "main", false)];
        assert!(!not_on_base(false, &added));
        assert!(!not_on_base(false, &[]));
    }
}
