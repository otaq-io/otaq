//! The ticket's **Files** tab (FTB-01…FTB-05, SHT-06; d0017 as refined 2026-10-23, d0043, d0050, d0109; canvas
//! `s25-07`, `s25-08`, `s25-09`): where you read a ticket's work and review it.
//!
//! - **The tree** of the ticket's worktree (`files.tree`, T-0467), with **All files · Changed · 3** and the search box
//!   on top (⌥⌘P): changed files carry their letter and counts ("M +4 −1"), uncommitted ones are marked, unchanged
//!   ones are quieter. A ticket that never started shows its base branch, labelled so.
//! - **A file** opens as **File** (as it is, highlighted) or **Diff** (against the base), with **New ticket from this
//!   file**.
//! - **All changes** (the Changed filter): every changed file's diff in one scroll, **Unified · Split** (Split below
//!   900 px reads as unified, [`ReviewMemory`]), "3 files · +13 −5 against main", **Open file** per file; the hunks are
//!   [`super::diff_page`]'s.
//! - **Line comments** (FTB-05) on any line of All changes or a file's Diff while the ticket waits in your check: they
//!   wait "not sent yet" and go to the agent with **Send back** (`board/comments.rs`, REV-03).
//! - **The review bar** (SHT-06) under every tab while the check is yours: "Your check · Security passed · comment on
//!   any line to send it back", **Send back…** and **Approve** ⌘↵ (today's dialogs); with comments, "1 line comment ·
//!   they go to Dev with Send back" and **Send back with comments** first.
//!
//! The page keeps the moment of interaction only: the filter, the open file and its mode, the search being typed,
//! the folders opened, the comment box.
//!
//! [`ReviewMemory`]: super::diff::ReviewMemory

use std::collections::BTreeMap;

use otaq_core::board::comments::{ReviewComment, Side};
use otaq_core::board::files::{
    ChangedEntry, ChildTag, EntryKind, FileChange, FileContent, FileMatch, FilesChanges, FilesFrom,
    FilesSource, TagKind, TreeEntry,
};
use otaq_core::board::human_check::Arrival;
use otaq_core::board::tickets::Ticket;
use otaq_core::domain::{StatusSet, TicketKey};
use otaq_core::git::diff::FileStatus;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::highlight;
use crate::view_models::comments as cm;
use crate::view_models::diff::{self as vm, ReviewMemory};
use crate::view_models::diff_page::{AnchorView, SpanView, StatusView, TokenView};

/// What the left column shows (FTB-01, FTB-04).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename = "FilesFilter")]
pub enum Filter {
    /// The whole tree; a picked file opens on the right.
    All,
    /// The changed files; All changes on the right.
    Changed,
}

/// How a file shows (FTB-03).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename = "FilesFileMode")]
pub enum FileMode {
    File,
    Diff,
}

/// The tab opens on All changes while the ticket waits in your check (SHT-06), else on the tree.
pub fn opens_on(ticket: &Ticket) -> Filter {
    if waits_for_you(ticket) {
        Filter::Changed
    } else {
        Filter::All
    }
}

/// The ticket waits in one of your checks (any human check of its flow: their one status, `WaitingForYou`).
pub fn waits_for_you(ticket: &Ticket) -> bool {
    ticket.status.set() == StatusSet::HumanCheck
}

/// Line comments can be written: a ticket (never an epic, EPC-05) waiting in your check. The rule
/// `board.add_comment` applies (FTB-05).
pub fn can_comment(ticket: &Ticket) -> bool {
    waits_for_you(ticket) && !ticket.is_epic()
}

// ---- Words ----

pub const SEARCH_PLACEHOLDER: &str = "Search files by name";
pub const ALL_FILES: &str = "All files";
pub const ALL_CHANGES: &str = "All changes";
pub const MODES: [&str; 2] = ["Unified", "Split"];
pub const FILE_MODES: [&str; 2] = ["File", "Diff"];
pub const NEW_TICKET: &str = "New ticket from this file";
pub const OPEN_FILE: &str = "Open file";
pub const LOAD_DIFF: &str = "Load diff";
/// The right side of All files before a file is picked (not drawn: Q-C).
pub const PICK_A_FILE: &str = "Pick a file in the tree to read it, or search for one by name.";
/// After the summary line outside your check (`s25-08`).
pub const PR_LOOKS_LIKE_THIS: &str = "the PR will look like this";

/// "Changed · 3".
pub fn changed_label(changed: u32) -> String {
    format!("Changed \u{b7} {changed}")
}

/// "+4 −1", "+1", "−3"; `None` when no line changed (a rename, a mode change).
pub fn counts(added: u32, removed: u32) -> Option<String> {
    let parts: Vec<String> = [
        (added > 0).then(|| format!("+{added}")),
        (removed > 0).then(|| format!("\u{2212}{removed}")),
    ]
    .into_iter()
    .flatten()
    .collect();
    (!parts.is_empty()).then(|| parts.join(" "))
}

// ---- Where the files come from ----

/// The kind of source, for its icon.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename = "FilesSourceKind")]
pub enum SourceKind {
    Worktree,
    Branch,
    Base,
}

/// The card on top of the tree: "SHOP-22’s worktree" over "otaq/SHOP-22 · from main".
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "FilesSourceView")]
pub struct SourceView {
    pub kind: SourceKind,
    pub title: String,
    pub subtitle: String,
    /// An epic's key to its tags (FTB-06): "40.1 merged", "40.2 on top · its own worktree"; empty for a ticket.
    pub legend: Vec<TagView>,
}

pub fn source_view(source: &FilesSource) -> SourceView {
    let key = &source.key;
    match source.from {
        FilesFrom::Worktree => SourceView {
            kind: SourceKind::Worktree,
            title: format!("{key}\u{2019}s worktree"),
            subtitle: format!("{} \u{b7} from {}", source.branch, source.base_branch),
            legend: Vec::new(),
        },
        FilesFrom::Branch => SourceView {
            kind: SourceKind::Branch,
            title: format!("{key}\u{2019}s branch"),
            subtitle: format!("{} \u{b7} from {}", source.branch, source.base_branch),
            legend: Vec::new(),
        },
        FilesFrom::Base => SourceView {
            kind: SourceKind::Base,
            title: source.branch.clone(),
            subtitle: format!("the base branch \u{b7} {key} hasn\u{2019}t started"),
            legend: Vec::new(),
        },
    }
}

/// An epic's Files (FTB-06, `s25-11`): "Wishlist’s branch" over "otaq/PAY-123 · what its tickets merged", and the key
/// to its tags from its own children: the first merged one ("40.1 merged") and the first one running in its own
/// worktree ("40.2 on top · its own worktree"). Before it started, its base branch as for a ticket.
pub fn epic_source(epic: &Ticket, source: &FilesSource, tickets: &[Ticket]) -> SourceView {
    if source.from == FilesFrom::Base {
        return source_view(source);
    }
    let children = super::epic_sheet::children(epic, tickets);
    let merged = children
        .iter()
        .find(|c| c.status == otaq_core::domain::Status::Merged)
        .map(|c| TagView {
            label: format!("{} merged", super::epic_sheet::short_key(c)),
            writing: false,
        });
    let writing = children
        .iter()
        .find(|c| {
            otaq_core::epic_lead::overlap::is_running(c.status)
                && c.work.as_ref().is_some_and(|w| w.worktree.is_some())
        })
        .map(|c| TagView {
            label: format!(
                "{} on top \u{b7} its own worktree",
                super::epic_sheet::short_key(c)
            ),
            writing: true,
        });
    SourceView {
        kind: SourceKind::Branch,
        title: format!("{}\u{2019}s branch", epic.title),
        subtitle: format!("{} \u{b7} what its tickets merged", source.branch),
        legend: merged.into_iter().chain(writing).collect(),
    }
}

/// Where a file is read: "in SHOP-22’s worktree", "on otaq/SHOP-22", "on main".
fn read_in(source: &FilesSource) -> String {
    match source.from {
        FilesFrom::Worktree => format!("in {}\u{2019}s worktree", source.key),
        FilesFrom::Branch | FilesFrom::Base => format!("on {}", source.branch),
    }
}

// ---- The tree and the search ----

/// A changed file's mark: "M", "+4 −1", and whether some of it isn't committed yet.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "FilesMark")]
pub struct MarkView {
    /// M · A · D · R.
    pub letter: &'static str,
    pub status: StatusView,
    /// "+4 −1"; "binary" for a binary file.
    pub counts: Option<String>,
    /// "uncommitted": edited on disk, staged or untracked (FTB-01).
    pub uncommitted: Option<&'static str>,
}

pub const UNCOMMITTED: &str = "uncommitted";

pub fn mark(change: &FileChange) -> MarkView {
    MarkView {
        letter: vm::status_letter(change.status),
        status: change.status.into(),
        counts: if change.binary {
            Some("binary".to_owned())
        } else {
            counts(change.added, change.removed)
        },
        uncommitted: change.uncommitted.then_some(UNCOMMITTED),
    }
}

/// An epic child that changed a file: "40.1", "40.2 ✎" while it is writing (FTB-06).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "FilesTag")]
pub struct TagView {
    pub label: String,
    pub writing: bool,
}

fn tags(tags: &[ChildTag]) -> Vec<TagView> {
    tags.iter()
        .map(|t| {
            let short = match t.key.child {
                Some(child) => format!("{}.{child}", t.key.number),
                None => t.key.to_string(),
            };
            let writing = t.kind == TagKind::Writing;
            TagView {
                label: if writing {
                    format!("{short} \u{270e}")
                } else {
                    short
                },
                writing,
            }
        })
        .collect()
}

/// What a tree row is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename = "FilesEntryKind")]
pub enum EntryKindView {
    Folder,
    File,
    Link,
    Submodule,
}

impl From<EntryKind> for EntryKindView {
    fn from(kind: EntryKind) -> Self {
        match kind {
            EntryKind::Folder => Self::Folder,
            EntryKind::File => Self::File,
            EntryKind::Link => Self::Link,
            EntryKind::Submodule => Self::Submodule,
        }
    }
}

/// One row of the tree: a folder (opens below itself) or a file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "FilesTreeRow")]
pub struct TreeRow {
    pub name: String,
    pub path: String,
    pub kind: EntryKindView,
    /// How deep it sits (the top is 0): 14 px of indent each.
    pub depth: u32,
    pub mark: Option<MarkView>,
    /// A file nobody changed reads quieter (`s25-07`).
    pub quiet: bool,
    /// A submodule is shown, never opened.
    pub opens: bool,
    pub tags: Vec<TagView>,
    /// An epic's file a running child is writing: that child's key, so it opens from the child's worktree
    /// (`files.read`'s `child`, FTB-06).
    pub child: Option<String>,
    /// The project Files view (FIL-03): one chip per ticket changing the file; empty in a ticket's tab.
    pub chips: Vec<ChipView>,
    /// The project Files view: a folder's "3 changed".
    pub changed_below: Option<String>,
    /// The project Files view: the other base branch of a ticket changing it ("release/2.3", d0051).
    pub branch: Option<String>,
    /// A deleted file, struck through (FIL-05).
    pub struck: bool,
}

/// A ticket's chip on a file of the project Files view: its key in the ticket's colour, the colour of its bar beside
/// the lines it changes (FIL-03, FIL-05; Q456). `FilesChip` on the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "FilesChip")]
pub struct ChipView {
    pub key: String,
    /// The theme token: `ansi-4`.
    pub colour: &'static str,
}

/// The running child writing a file on top of the epic's branch, by key.
fn writing_child(tags: &[ChildTag]) -> Option<String> {
    tags.iter()
        .find(|t| t.kind == TagKind::Writing)
        .map(|t| t.key.to_string())
}

/// A folder's entries at `depth` (`files.tree`).
pub fn tree_rows(entries: &[TreeEntry], depth: u32) -> Vec<TreeRow> {
    entries
        .iter()
        .map(|e| TreeRow {
            name: e.name.clone(),
            path: e.path.clone(),
            kind: e.kind.into(),
            depth,
            mark: e.change.as_ref().map(mark),
            quiet: e.kind != EntryKind::Folder && e.change.is_none() && e.tags.is_empty(),
            opens: e.kind != EntryKind::Submodule,
            tags: tags(&e.tags),
            child: writing_child(&e.tags),
            chips: Vec::new(),
            changed_below: None,
            branch: None,
            struck: false,
        })
        .collect()
}

/// The changed files as a tree, every folder open (the Changed filter, `s25-08`): folders first, then by name
/// ignoring case, as `files.tree` orders them. A file an epic child changes on top shows once.
pub fn changed_tree(files: &[ChangedEntry]) -> Vec<TreeRow> {
    #[derive(Default)]
    struct Folder<'a> {
        folders: BTreeMap<(String, String), Folder<'a>>,
        files: BTreeMap<(String, String), &'a ChangedEntry>,
    }
    let order = |name: &str| (name.to_lowercase(), name.to_owned());
    let mut top = Folder::default();
    for entry in files {
        let mut parts: Vec<&str> = entry.file.path.split('/').collect();
        let Some(name) = parts.pop() else { continue };
        let mut folder = &mut top;
        for part in parts {
            folder = folder.folders.entry(order(part)).or_default();
        }
        folder.files.entry(order(name)).or_insert(entry);
    }
    fn walk(folder: &Folder, prefix: &str, depth: u32, rows: &mut Vec<TreeRow>) {
        for ((_, name), inner) in &folder.folders {
            let path = format!("{prefix}{name}");
            rows.push(TreeRow {
                name: name.clone(),
                path: path.clone(),
                kind: EntryKindView::Folder,
                depth,
                mark: None,
                quiet: false,
                opens: true,
                tags: Vec::new(),
                child: None,
                chips: Vec::new(),
                changed_below: None,
                branch: None,
                struck: false,
            });
            walk(inner, &format!("{path}/"), depth + 1, rows);
        }
        for ((_, name), entry) in &folder.files {
            rows.push(TreeRow {
                name: name.clone(),
                path: entry.file.path.clone(),
                kind: EntryKindView::File,
                depth,
                mark: Some(mark(&change_of(entry))),
                quiet: false,
                opens: true,
                tags: tags(&entry.tags),
                child: entry.child.as_ref().map(ToString::to_string),
                chips: Vec::new(),
                changed_below: None,
                branch: None,
                struck: false,
            });
        }
    }
    let mut rows = Vec::new();
    walk(&top, "", 0, &mut rows);
    rows
}

fn change_of(entry: &ChangedEntry) -> FileChange {
    FileChange {
        status: entry.file.status,
        added: entry.file.added,
        removed: entry.file.removed,
        binary: entry.file.binary,
        uncommitted: entry.uncommitted,
    }
}

/// A piece of a search match's name or folder; `hit` is the part that matched what was typed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "FilesHitPart")]
pub struct HitPart {
    pub text: String,
    pub hit: bool,
}

/// One search match, flat with its folder (FTB-02, `s25-08`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "FilesSearchRow")]
pub struct SearchRow {
    pub path: String,
    pub name: Vec<HitPart>,
    /// "src/app/login"; empty at the top.
    pub folder: Vec<HitPart>,
    pub mark: Option<MarkView>,
    pub quiet: bool,
    pub tags: Vec<TagView>,
    /// An epic's file a running child is writing: opens from that child's worktree.
    pub child: Option<String>,
    /// The project Files view: as [`TreeRow::chips`] and [`TreeRow::branch`].
    pub chips: Vec<ChipView>,
    pub branch: Option<String>,
}

/// The search's answer: the matches, best first, and what to say when there are none or too many.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "FilesSearchView")]
pub struct SearchView {
    pub query: String,
    pub rows: Vec<SearchRow>,
    /// "No file name matches “chekout”" (Q-C).
    pub empty: Option<String>,
    /// "50 of 120 shown · type more of the name" (Q-C).
    pub more: Option<String>,
}

/// `text`'s characters `[from, to)` cut into hit parts, `hit` being the matched `[start, end)` of the whole path.
pub(crate) fn hit_parts(text: &str, from: usize, hit: [u32; 2]) -> Vec<HitPart> {
    let chars: Vec<char> = text.chars().collect();
    let (start, end) = (hit[0] as usize, hit[1] as usize);
    let local = |at: usize| at.saturating_sub(from).min(chars.len());
    let (a, b) = if end <= from || start >= from + chars.len() {
        (chars.len(), chars.len())
    } else {
        (local(start), local(end))
    };
    [(0, a, false), (a, b, true), (b, chars.len(), false)]
        .into_iter()
        .filter(|(s, e, _)| s < e)
        .map(|(s, e, hit)| HitPart {
            text: chars[s..e].iter().collect(),
            hit,
        })
        .collect()
}

pub fn search_view(query: &str, matches: &[FileMatch], total: u32) -> SearchView {
    let rows: Vec<SearchRow> = matches
        .iter()
        .map(|m| {
            let folder_chars = m.folder.chars().count();
            // The name starts after the folder and its "/".
            let name_from = if m.folder.is_empty() {
                0
            } else {
                folder_chars + 1
            };
            SearchRow {
                path: m.path.clone(),
                name: hit_parts(&m.name, name_from, m.hit),
                folder: hit_parts(&m.folder, 0, m.hit),
                mark: m.change.as_ref().map(mark),
                quiet: m.change.is_none() && m.tags.is_empty(),
                tags: tags(&m.tags),
                child: writing_child(&m.tags),
                chips: Vec::new(),
                branch: None,
            }
        })
        .collect();
    let query = query.trim();
    SearchView {
        query: query.to_owned(),
        empty: (rows.is_empty() && !query.is_empty())
            .then(|| format!("No file name matches \u{201c}{query}\u{201d}")),
        more: (total as usize > rows.len()).then(|| {
            format!(
                "{} of {total} shown \u{b7} type more of the name",
                rows.len()
            )
        }),
        rows,
    }
}

// ---- The tab ----

/// The search box: "Search files by name" ⌥⌘P.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "FilesSearchBox")]
pub struct SearchBox {
    pub placeholder: &'static str,
    /// The user's key for Go to file (KEY-08).
    pub keys: Option<String>,
}

/// The Files tab's frame (`files.view`): the source, the search box, All files · Changed · N, the top of the tree,
/// and where it opens.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "FilesTabView")]
pub struct TabView {
    pub key: String,
    pub source: SourceView,
    pub search: SearchBox,
    /// "All files", "Changed · 3".
    pub filters: [String; 2],
    pub changed: u32,
    pub opens_on: Filter,
    /// The top folder's entries.
    pub rows: Vec<TreeRow>,
    pub pick: &'static str,
}

pub fn tab_view(
    ticket: &Ticket,
    source: &FilesSource,
    top: &[TreeEntry],
    changed: u32,
    search_keys: Option<String>,
) -> TabView {
    TabView {
        key: ticket.key.to_string(),
        source: source_view(source),
        search: SearchBox {
            placeholder: SEARCH_PLACEHOLDER,
            keys: search_keys,
        },
        filters: [ALL_FILES.to_owned(), changed_label(changed)],
        changed,
        opens_on: opens_on(ticket),
        rows: tree_rows(top, 0),
        pick: PICK_A_FILE,
    }
}

// ---- All changes ----

/// One changed file of All changes: its header and how its lines show.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "FilesChangeRow")]
pub struct ChangeRow {
    /// Unique in the list: the path, and the child for an epic child's change on top.
    pub id: String,
    pub path: String,
    /// "lib/money.ts → lib/format/money.ts" for a rename.
    pub label: String,
    /// Epics: the running child whose worktree it comes from (`files.diff`'s `child`).
    pub child: Option<String>,
    pub mark: MarkView,
    /// "new file", "deleted", "renamed", "binary file · 12 KB".
    pub note: Option<String>,
    /// Large and generated files wait for "Load diff".
    pub collapsed: Option<String>,
    /// "Binary file changed · 184 KB → 96 KB" in place of lines.
    pub binary_label: Option<String>,
    /// "Renamed with 3 changed lines (98 % similar). Showing only the changes."
    pub rename_label: Option<String>,
    /// About how many rows its lines take, for the room kept while they load.
    pub rows: u32,
    /// A deleted file has nothing to open.
    pub can_open: bool,
    /// Line comments go on the ticket's own changes, not on a child's on top.
    pub commentable: bool,
    pub tags: Vec<TagView>,
}

/// *Binary, renamed, empty → No changes*, for a worktree.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "FilesEmpty")]
pub struct EmptyView {
    pub title: &'static str,
    pub text: String,
}

/// All changes (`files.changes`, FTB-04): "All changes", "3 files · +13 −5 against main", why you are looking (your
/// check's line, or that the PR will look like this), Unified · Split, and every changed file.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(rename = "FilesChangesView")]
pub struct ChangesView {
    pub key: String,
    /// Changes when the list changes, so the page drops lines it loaded for an older one.
    pub revision: String,
    pub title: &'static str,
    /// "3 files · +13 −5 against main".
    pub summary: String,
    /// "· Security passed", "· the PR will look like this".
    pub note: Option<String>,
    pub modes: [&'static str; 2],
    pub side_by_side: bool,
    /// Below this width Split shows as Unified.
    pub side_by_side_from: f32,
    pub files: Vec<ChangeRow>,
    /// The left column under the Changed filter.
    pub tree: Vec<TreeRow>,
    pub empty: Option<EmptyView>,
    pub open_file: &'static str,
    pub load_diff: &'static str,
}

pub fn change_row(entry: &ChangedEntry) -> ChangeRow {
    let file = &entry.file;
    let child = entry.child.as_ref().map(ToString::to_string);
    ChangeRow {
        id: match &child {
            Some(c) => format!("{}\u{0}{c}", file.path),
            None => file.path.clone(),
        },
        path: file.path.clone(),
        label: vm::file_label(file),
        mark: mark(&change_of(entry)),
        note: vm::file_note(file),
        collapsed: vm::starts_collapsed(file).then(|| {
            if vm::is_generated(&file.path) {
                "Generated file: collapsed.".to_owned()
            } else {
                "Large change: collapsed.".to_owned()
            }
        }),
        binary_label: file.binary.then(|| vm::binary_label(file)),
        rename_label: vm::rename_label(file),
        rows: (file.added + file.removed).clamp(1, vm::LARGE_FILE_LINES) + 4,
        can_open: file.status != FileStatus::Deleted,
        commentable: child.is_none(),
        child,
        tags: tags(&entry.tags),
    }
}

/// "1 file", "3 files".
pub(crate) fn files_label(n: usize) -> String {
    if n == 1 {
        "1 file".to_owned()
    } else {
        format!("{n} files")
    }
}

/// How the ticket reached your check, as the bar and All changes say it: "Security passed", "Skipped Security · by
/// you", "Moved back by you".
pub fn arrival_line(arrival: &Arrival) -> String {
    match arrival {
        Arrival::Passed { stage, round: None } => format!("{stage} passed"),
        Arrival::Passed {
            stage,
            round: Some(round),
        } => format!("{stage} passed \u{b7} round {round}"),
        other => other.label(),
    }
}

fn revision(changes: &FilesChanges) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut h = DefaultHasher::new();
    changes.source.base_sha.hash(&mut h);
    for e in &changes.files {
        let f = &e.file;
        (
            &f.path,
            f.added,
            f.removed,
            f.binary,
            &f.size,
            e.uncommitted,
        )
            .hash(&mut h);
        e.child.as_ref().map(ToString::to_string).hash(&mut h);
    }
    format!("{:016x}", h.finish())
}

pub fn changes_view(ticket: &Ticket, changes: &FilesChanges, memory: &ReviewMemory) -> ChangesView {
    let source = &changes.source;
    let own = changes.files.iter().filter(|e| e.child.is_none()).count();
    let mut summary = files_label(own);
    if let Some(c) = counts(changes.added, changes.removed) {
        summary.push_str(&format!(" \u{b7} {c}"));
    }
    summary.push_str(&format!(" against {}", source.base_branch));
    let note = if waits_for_you(ticket) {
        ticket.flags.arrival.as_ref().map(arrival_line)
    } else {
        Some(PR_LOOKS_LIKE_THIS.to_owned())
    };
    let empty = changes.files.is_empty().then(|| EmptyView {
        title: vm::EMPTY_TITLE,
        text: match source.from {
            FilesFrom::Base => format!(
                "{} hasn\u{2019}t started: nothing has changed yet.",
                source.key
            ),
            FilesFrom::Worktree | FilesFrom::Branch => format!(
                "{} has no changes against {} yet.",
                source.key, source.base_branch
            ),
        },
    });
    ChangesView {
        key: ticket.key.to_string(),
        revision: revision(changes),
        title: ALL_CHANGES,
        summary,
        note: note
            .filter(|_| empty.is_none())
            .map(|n| format!("\u{b7} {n}")),
        modes: MODES,
        side_by_side: memory.side_by_side,
        side_by_side_from: vm::SIDE_BY_SIDE_FROM,
        files: changes.files.iter().map(change_row).collect(),
        tree: changed_tree(&changes.files),
        empty,
        open_file: OPEN_FILE,
        load_diff: LOAD_DIFF,
    }
}

/// The agent's summary above All changes (d0017), folded by a click.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "FilesAgentSummary")]
pub struct AgentSummary {
    pub title: &'static str,
    pub text: String,
    /// "no tests are run by Otaq" (P5).
    pub note: String,
}

/// The last summary the agent gave when its stage finished; none when it gave none.
pub fn agent_summary(text: Option<&str>) -> Option<AgentSummary> {
    text.filter(|t| !t.trim().is_empty())
        .map(|text| AgentSummary {
            title: "Agent summary",
            text: text.to_owned(),
            note: format!("no tests are run by {}", otaq_core::naming::PRODUCT_NAME),
        })
}

// ---- A file ----

/// One line of a file as it is, highlighted.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "FilesCodeLine")]
pub struct CodeLine {
    pub number: u32,
    pub spans: Vec<SpanView>,
}

/// A file opened from the tree (FTB-03): "src/middleware.ts" over "in SHOP-22’s worktree · +4 −1 against main", File ·
/// Diff, New ticket from this file, and its lines as it is (the Diff's come from `files.diff`).
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(rename = "FilesFilePage")]
pub struct FilePage {
    pub path: String,
    pub child: Option<String>,
    pub subtitle: String,
    pub modes: [&'static str; 2],
    /// File for an unchanged file; Diff for a changed one (`s25-07`) and a deleted one.
    pub opens_in: FileMode,
    /// Unified · Split on its Diff: the same choice as All changes', kept for every ticket ([`ReviewMemory`],
    /// dogfood #61).
    pub split_modes: [&'static str; 2],
    pub side_by_side: bool,
    /// Below this width Split shows as Unified.
    pub side_by_side_from: f32,
    /// Not for a deleted file.
    pub can_file: bool,
    /// Only for a changed file.
    pub can_diff: bool,
    pub new_ticket: &'static str,
    /// The description New ticket from this file starts with: the path.
    pub new_ticket_body: String,
    pub lines: Vec<CodeLine>,
    /// "Binary file · 184 KB", "Showing the first 20000 lines: the file is too long to show in full."
    pub note: Option<String>,
    /// An epic's file a running child is writing (FTB-06): "40.2’s agent (Rust developer) is writing this file in
    /// 40.2’s own worktree; you see its change on top of the epic’s branch."
    pub writing: Option<String>,
}

/// The note over a file `child` is writing on top of its epic's branch, naming its agent when known (FTB-06).
pub fn writing_note(child: &TicketKey, agent: Option<&str>) -> String {
    let short = match child.child {
        Some(n) => format!("{}.{n}", child.number),
        None => child.to_string(),
    };
    let who = match agent {
        Some(agent) => format!("{short}\u{2019}s agent ({agent})"),
        None => format!("{short}\u{2019}s agent"),
    };
    format!(
        "{who} is writing this file in {short}\u{2019}s own worktree; you see its change on top of the epic\u{2019}s \
         branch."
    )
}

/// The subtitle of a file `child` is writing: "40.2’s change, on top of the epic’s branch · not merged yet".
fn child_subtitle(child: &TicketKey) -> String {
    let short = match child.child {
        Some(n) => format!("{}.{n}", child.number),
        None => child.to_string(),
    };
    format!("{short}\u{2019}s change, on top of the epic\u{2019}s branch \u{b7} not merged yet")
}

/// A file's text as numbered lines, highlighted by its name (the project Files view's file too).
pub fn code_lines(path: &str, text: &str) -> Vec<CodeLine> {
    let texts: Vec<&str> = text.lines().collect();
    let spans = highlight::highlight(path, &texts);
    texts
        .iter()
        .enumerate()
        .map(|(i, text)| CodeLine {
            number: i as u32 + 1,
            spans: match spans.get(i) {
                Some(spans) => spans
                    .iter()
                    .filter_map(|(range, token)| {
                        Some(SpanView {
                            text: text.get(range.clone())?.to_owned(),
                            token: (*token).into(),
                        })
                    })
                    .collect(),
                None => vec![SpanView {
                    text: (*text).to_owned(),
                    token: TokenView::Plain,
                }],
            },
        })
        .collect()
}

/// A file read with `files.read`; `memory` holds Unified · Split.
pub fn file_page(content: &FileContent, memory: &ReviewMemory) -> FilePage {
    let changed = content.change.as_ref();
    let mut subtitle = read_in(&content.source);
    match changed {
        Some(c) => {
            let what = if c.binary {
                "binary file changed".to_owned()
            } else {
                counts(c.added, c.removed).unwrap_or_else(|| "changed".to_owned())
            };
            subtitle.push_str(&format!(
                " \u{b7} {what} against {}",
                content.source.base_branch
            ));
            if c.uncommitted {
                subtitle.push_str(&format!(" \u{b7} {UNCOMMITTED}"));
            }
        }
        None => subtitle.push_str(" \u{b7} not changed"),
    }
    if let Some(child) = &content.child {
        subtitle = child_subtitle(child);
    }
    let lines = if content.binary {
        Vec::new()
    } else {
        code_lines(&content.path, &content.text)
    };
    let note = if content.binary {
        Some(format!(
            "Binary file \u{b7} {}",
            vm::size_label(content.size)
        ))
    } else if content.truncated {
        Some(format!(
            "Showing the first {} lines: the file is too long to show in full.",
            lines.len()
        ))
    } else {
        None
    };
    FilePage {
        path: content.path.clone(),
        child: content.child.as_ref().map(ToString::to_string),
        subtitle,
        modes: FILE_MODES,
        opens_in: if changed.is_some() {
            FileMode::Diff
        } else {
            FileMode::File
        },
        split_modes: MODES,
        side_by_side: memory.side_by_side,
        side_by_side_from: vm::SIDE_BY_SIDE_FROM,
        can_file: true,
        can_diff: changed.is_some(),
        new_ticket: NEW_TICKET,
        new_ticket_body: new_ticket_body(&content.path),
        lines,
        note,
        writing: content.child.as_ref().map(|c| writing_note(c, None)),
    }
}

/// A file that is gone from the worktree (deleted): only its Diff shows what it was.
pub fn deleted_page(
    source: &FilesSource,
    path: &str,
    child: Option<String>,
    memory: &ReviewMemory,
) -> FilePage {
    FilePage {
        path: path.to_owned(),
        child,
        subtitle: format!("{} \u{b7} deleted", read_in(source)),
        modes: FILE_MODES,
        opens_in: FileMode::Diff,
        split_modes: MODES,
        side_by_side: memory.side_by_side,
        side_by_side_from: vm::SIDE_BY_SIDE_FROM,
        can_file: false,
        can_diff: true,
        new_ticket: NEW_TICKET,
        new_ticket_body: new_ticket_body(path),
        lines: Vec::new(),
        note: None,
        writing: None,
    }
}

/// What New ticket from this file puts in the description (FTB-03): the path.
pub fn new_ticket_body(path: &str) -> String {
    format!("`{path}`\n\n")
}

// ---- Line comments ----

/// One comment as its card shows it: "You · on line 21 · not sent yet" (`s25-09`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "FilesComment")]
pub struct CommentView {
    pub id: String,
    pub author: &'static str,
    /// "on line 21 · not sent yet", "on line 21 · sent · round 1".
    pub meta: String,
    pub pending: bool,
    /// Pending comments on a ticket in your check can be edited and deleted.
    pub editable: bool,
    pub body: String,
}

/// The comments on one line, oldest first.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "FilesThread")]
pub struct ThreadView {
    pub anchor: AnchorView,
    pub comments: Vec<CommentView>,
}

/// The comment box's words.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "FilesComposerTexts")]
pub struct ComposerTexts {
    pub placeholder: &'static str,
    /// "Goes with Send back, to Dev".
    pub hint: String,
    pub add: &'static str,
    pub save: &'static str,
    pub cancel: &'static str,
    pub reply: &'static str,
    pub edit: &'static str,
    pub delete: &'static str,
    pub add_on_line: &'static str,
}

/// A ticket's line comments (`files.comments`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "FilesComments")]
pub struct CommentsView {
    pub key: String,
    /// Comments can be added: the ticket waits in your check ([`can_comment`]).
    pub can_comment: bool,
    pub pending: u32,
    pub threads: Vec<ThreadView>,
    pub texts: ComposerTexts,
}

pub const NOT_SENT_YET: &str = "not sent yet";

/// "on line 21", "on removed line 20".
fn on_line(comment: &ReviewComment) -> String {
    match comment.side {
        Side::New => format!("on line {}", comment.line),
        Side::Old => format!("on removed line {}", comment.line),
    }
}

/// "not sent yet" until Send back sends it, then "sent · round 1".
pub fn comment_state(comment: &ReviewComment) -> String {
    if comment.is_pending() {
        NOT_SENT_YET.to_owned()
    } else {
        cm::badge(comment)
    }
}

/// The comments, grouped into threads in the order their first comment was written; `back_to` is the stage Send
/// back returns the ticket to ("Dev").
pub fn comments_view(
    key: &str,
    comments: &[ReviewComment],
    can_comment: bool,
    back_to: &str,
) -> CommentsView {
    let mut threads: Vec<ThreadView> = Vec::new();
    for c in comments {
        let view = CommentView {
            id: c.id.clone(),
            author: "You",
            meta: format!("{} \u{b7} {}", on_line(c), comment_state(c)),
            pending: c.is_pending(),
            editable: can_comment && cm::editable(c),
            body: c.body.clone(),
        };
        let anchor = AnchorView::new(&c.file, c.line, c.side.into());
        match threads.iter_mut().find(|t| t.anchor == anchor) {
            Some(thread) => thread.comments.push(view),
            None => threads.push(ThreadView {
                anchor,
                comments: vec![view],
            }),
        }
    }
    CommentsView {
        key: key.to_owned(),
        can_comment,
        pending: cm::pending(comments) as u32,
        threads,
        texts: ComposerTexts {
            placeholder: "Leave a comment",
            hint: format!("Goes with Send back, to {back_to}"),
            add: "Add comment",
            save: "Save",
            cancel: "Cancel",
            reply: "Reply",
            edit: "Edit",
            delete: "Delete",
            add_on_line: "Comment on this line",
        },
    }
}

// ---- The review bar ----

/// What a bar button opens: the Approve choice (`local_merge`, ⌘↵), the Pass a human check's Approve is in another
/// flow (`review.pass`, FLW-14; d0093), or the Send back dialog.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename = "FilesBarAction")]
pub enum BarAction {
    Approve,
    Pass,
    SendBack,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "FilesBarButton")]
pub struct BarButton {
    pub action: BarAction,
    /// "Approve", "Send back · to Product spec…" (d0093).
    pub label: String,
    pub primary: bool,
    /// The user's key, shown after the primary Approve (KEY-08).
    pub keys: Option<String>,
}

/// The bar under every tab while the check is yours (SHT-06, `s25-09`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "FilesReviewBar")]
pub struct ReviewBar {
    /// "Your check", or "1 line comment, 1 being written".
    pub title: String,
    /// "· Security passed · comment on any line to send it back", "· they go to Dev with Send back".
    pub line: String,
    pub buttons: Vec<BarButton>,
}

/// "1 line comment", "2 line comments, 1 being written", "1 line comment being written".
pub fn comments_line(pending: u32, writing: bool) -> String {
    let saved = match pending {
        1 => "1 line comment".to_owned(),
        n => format!("{n} line comments"),
    };
    match (pending, writing) {
        (0, _) => "1 line comment being written".to_owned(),
        (_, true) => format!("{saved}, 1 being written"),
        (_, false) => saved,
    }
}

/// The bar for `ticket`, or none when it doesn't wait in your check. `pending` counts its unsent line comments,
/// `writing` says a new one is being typed (the page's comment box), `back_to` names where Send back returns it
/// (d0131), `custom` says its flow isn't the default's shape (Approve is then its Pass, FLW-14).
pub fn review_bar(
    ticket: &Ticket,
    pending: u32,
    writing: bool,
    back_to: &str,
    approve_keys: Option<String>,
    custom: bool,
) -> Option<ReviewBar> {
    if !waits_for_you(ticket) {
        return None;
    }
    let send_back = crate::view_models::send_back::available(ticket);
    let approve = |primary: bool, keys: Option<String>| BarButton {
        // An epic's Approve is its delivery choice on every flow (EPC-05).
        action: match custom && !ticket.is_epic() {
            true => BarAction::Pass,
            false => BarAction::Approve,
        },
        label: crate::view_models::sheet_actions::APPROVE.to_owned(),
        primary,
        keys,
    };
    if send_back && (pending > 0 || writing) {
        return Some(ReviewBar {
            title: comments_line(pending, writing),
            line: format!("\u{b7} they go to {back_to} with Send back"),
            buttons: vec![
                approve(false, None),
                BarButton {
                    action: BarAction::SendBack,
                    label: "Send back with comments".to_owned(),
                    primary: true,
                    keys: None,
                },
            ],
        });
    }
    let line: Vec<String> = ticket
        .flags
        .arrival
        .as_ref()
        .map(arrival_line)
        .into_iter()
        .chain(send_back.then(|| "comment on any line to send it back".to_owned()))
        .collect();
    Some(ReviewBar {
        title: crate::view_models::card::YOUR_CHECK.to_owned(),
        line: line
            .iter()
            .map(|part| format!("\u{b7} {part}"))
            .collect::<Vec<_>>()
            .join(" "),
        buttons: send_back
            .then(|| BarButton {
                action: BarAction::SendBack,
                label: crate::view_models::send_back::button_label(back_to, 0),
                primary: false,
                keys: None,
            })
            .into_iter()
            .chain([approve(true, approve_keys)])
            .collect(),
    })
}

// ---- The gallery ----

/// The gallery's Files tab (T-0468; `s25-07`…`s25-09`): SHOP-22's tree with src/ open and middleware.ts as its Diff,
/// a search, All changes with a line comment not sent yet, and the review bar without and with comments.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(rename = "FilesGallery")]
pub struct Gallery {
    pub tab: TabView,
    /// src/'s rows, open under it.
    pub src: Vec<TreeRow>,
    pub file: FilePage,
    pub search: SearchView,
    pub changes: ChangesView,
    /// SHOP-18's route.ts, with the comment on line 21.
    pub diff: super::diff_page::FileView,
    pub comments: CommentsView,
    pub bars: Vec<ReviewBar>,
}

pub fn gallery() -> Gallery {
    use otaq_core::board::files::FilesFrom;
    use otaq_core::domain::{Place, Status};
    use otaq_core::git::diff::{ChangedFile, DiffLine, FileDiff, Hunk, LineKind};

    let source = |key: &str| FilesSource {
        key: key.parse().expect("fixture key"),
        from: FilesFrom::Worktree,
        branch: otaq_core::naming::branch(key),
        base_branch: "main".into(),
        base_sha: Some("a1b2c3d4e5".into()),
        base_short: Some("a1b2c3d".into()),
    };
    let modified = |added, removed, uncommitted| FileChange {
        status: FileStatus::Modified,
        added,
        removed,
        binary: false,
        uncommitted,
    };
    let entry = |name: &str, path: &str, kind, change| TreeEntry {
        name: name.into(),
        path: path.into(),
        kind,
        change,
        changed_below: 0,
        tags: Vec::new(),
    };
    let changed = |path: &str, added, removed, uncommitted| ChangedEntry {
        file: ChangedFile {
            status: FileStatus::Modified,
            path: path.into(),
            old_path: None,
            added,
            removed,
            binary: false,
            size: None,
            old_size: None,
            similarity: None,
        },
        uncommitted,
        tags: Vec::new(),
        child: None,
    };
    let mut shop_22 = crate::view_models::card_fixtures::ticket(
        "SHOP-22",
        "Fix flaky login redirect",
        Status::NeedsYou,
    );
    shop_22.place = Place::InStage("dev".into());
    let mut shop_18 =
        crate::view_models::card_fixtures::ticket("SHOP-18", "Checkout API", Status::WaitingForYou);
    shop_18.place = Place::InStage("review".into());
    shop_18.flags.arrival = Some(Arrival::Passed {
        stage: "Security".into(),
        round: None,
    });
    let top = [
        entry("e2e", "e2e", EntryKind::Folder, None),
        entry("src", "src", EntryKind::Folder, None),
        entry("public", "public", EntryKind::Folder, None),
        entry("next.config.ts", "next.config.ts", EntryKind::File, None),
        entry("package.json", "package.json", EntryKind::File, None),
        entry("README.md", "README.md", EntryKind::File, None),
    ];
    let src = [
        entry("app", "src/app", EntryKind::Folder, None),
        entry("components", "src/components", EntryKind::Folder, None),
        entry("lib", "src/lib", EntryKind::Folder, None),
        entry(
            "middleware.ts",
            "src/middleware.ts",
            EntryKind::File,
            Some(modified(4, 1, false)),
        ),
    ];
    let text = "import { NextResponse } from 'next/server'\nimport type { NextRequest } from 'next/server'\n\nexport function middleware(req: NextRequest) {\n  const session = req.cookies.get('sid') ?? req.headers.get('x-sid')\n  if (req.nextUrl.pathname === '/auth/callback') {\n    return NextResponse.next() // cookie is set on this response\n  }\n  return NextResponse.next()\n}\n";
    let file = file_page(
        &FileContent {
            source: source("SHOP-22"),
            path: "src/middleware.ts".into(),
            child: None,
            size: text.len() as u64,
            binary: false,
            text: text.into(),
            truncated: false,
            change: Some(modified(4, 1, false)),
            tags: Vec::new(),
        },
        &ReviewMemory::default(),
    );
    let search = search_view(
        "log",
        &[
            FileMatch {
                path: "e2e/login.spec.ts".into(),
                name: "login.spec.ts".into(),
                folder: "e2e".into(),
                hit: [4, 7],
                change: Some(modified(6, 1, false)),
                tags: Vec::new(),
            },
            FileMatch {
                path: "src/app/login/page.tsx".into(),
                name: "page.tsx".into(),
                folder: "src/app/login".into(),
                hit: [8, 11],
                change: Some(modified(3, 3, true)),
                tags: Vec::new(),
            },
        ],
        2,
    );
    let changes = changes_view(
        &shop_18,
        &FilesChanges {
            source: source("SHOP-18"),
            files: vec![
                changed("src/app/api/checkout/coupon/route.ts", 2, 1, false),
                changed("prisma/schema.prisma", 1, 0, true),
            ],
            added: 3,
            removed: 1,
        },
        &ReviewMemory::default(),
    );
    let line = |kind, old, new, text: &str| DiffLine {
        kind,
        old,
        new,
        text: text.into(),
        no_newline: false,
    };
    let route = "src/app/api/checkout/coupon/route.ts";
    let diff = super::diff_page::file_view(&FileDiff {
        file: changed(route, 2, 1, false).file,
        hunks: vec![Hunk {
            old_start: 18,
            old_lines: 5,
            new_start: 18,
            new_lines: 6,
            header: String::new(),
            lines: vec![
                line(
                    LineKind::Context,
                    Some(18),
                    Some(18),
                    "export async function POST(req: Request) {",
                ),
                line(
                    LineKind::Context,
                    Some(19),
                    Some(19),
                    "  const { code } = await req.json()",
                ),
                line(
                    LineKind::Removed,
                    Some(20),
                    None,
                    "  const coupon = await db.coupon.find(code)",
                ),
                line(
                    LineKind::Added,
                    None,
                    Some(20),
                    "  const coupon = await db.coupon.findValid(code)",
                ),
                line(
                    LineKind::Added,
                    None,
                    Some(21),
                    "  if (!coupon) return Response.json({ error: 'invalid' }, { status: 404 })",
                ),
                line(
                    LineKind::Context,
                    Some(21),
                    Some(22),
                    "  return Response.json(applyCoupon(coupon))",
                ),
            ],
        }],
        truncated: false,
    });
    let comments = comments_view(
        "SHOP-18",
        &[ReviewComment {
            id: "c1".into(),
            round: 1,
            file: route.into(),
            line: 21,
            side: Side::New,
            body: "Expired codes should say \u{201c}expired\u{201d}, not \u{201c}invalid\u{201d}: the cart shows the message.".into(),
            created_at: 0,
            sent_at: None,
        }],
        true,
        "Dev",
    );
    Gallery {
        tab: tab_view(
            &shop_22,
            &source("SHOP-22"),
            &top,
            3,
            Some("\u{2325}\u{2318}P".into()),
        ),
        src: tree_rows(&src, 1),
        file,
        search,
        changes,
        diff,
        comments,
        bars: [(0, false), (1, true)]
            .into_iter()
            .filter_map(|(pending, writing)| {
                review_bar(
                    &shop_18,
                    pending,
                    writing,
                    "Dev",
                    Some("\u{2318}\u{21b5}".into()),
                    false,
                )
            })
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use otaq_core::board::files::{FilesFrom, FilesSearch};
    use otaq_core::domain::{Place, Status, TicketKey};
    use otaq_core::git::diff::ChangedFile;

    use super::*;
    use crate::view_models::card_fixtures::ticket;

    fn key(k: &str) -> TicketKey {
        k.parse().unwrap()
    }

    fn source(k: &str, from: FilesFrom) -> FilesSource {
        FilesSource {
            key: key(k),
            from,
            branch: if from == FilesFrom::Base {
                "main".into()
            } else {
                otaq_core::naming::branch(k)
            },
            base_branch: "main".into(),
            base_sha: Some("a1b2c3d4e5".into()),
            base_short: Some("a1b2c3d".into()),
        }
    }

    fn change(status: FileStatus, added: u32, removed: u32, uncommitted: bool) -> FileChange {
        FileChange {
            status,
            added,
            removed,
            binary: false,
            uncommitted,
        }
    }

    fn entry(name: &str, path: &str, kind: EntryKind, change: Option<FileChange>) -> TreeEntry {
        TreeEntry {
            name: name.into(),
            path: path.into(),
            kind,
            change,
            changed_below: 0,
            tags: Vec::new(),
        }
    }

    fn changed(path: &str, added: u32, removed: u32, uncommitted: bool) -> ChangedEntry {
        ChangedEntry {
            file: ChangedFile {
                status: FileStatus::Modified,
                path: path.into(),
                old_path: None,
                added,
                removed,
                binary: false,
                size: None,
                old_size: None,
                similarity: None,
            },
            uncommitted,
            tags: Vec::new(),
            child: None,
        }
    }

    /// SHOP-22 "Fix flaky login redirect", in Dev, asking to run a command: three files changed, one of them not
    /// committed yet (the example).
    fn shop_22() -> Ticket {
        let mut t = ticket("SHOP-22", "Fix flaky login redirect", Status::NeedsYou);
        t.place = Place::InStage("dev".into());
        t
    }

    /// SHOP-18 "Checkout API" in Review, your check, after Security passed (`s25-09`).
    fn shop_18() -> Ticket {
        let mut t = ticket("SHOP-18", "Checkout API", Status::WaitingForYou);
        t.place = Place::InStage("review".into());
        t.flags.arrival = Some(Arrival::Passed {
            stage: "Security".into(),
            round: None,
        });
        t
    }

    fn shop_22_changes() -> FilesChanges {
        FilesChanges {
            source: source("SHOP-22", FilesFrom::Worktree),
            files: vec![
                changed("e2e/login.spec.ts", 6, 1, false),
                changed("src/app/login/page.tsx", 3, 3, true),
                changed("src/middleware.ts", 4, 1, false),
            ],
            added: 13,
            removed: 5,
        }
    }

    #[test]
    fn shop_22s_tree_reads_like_the_frame() {
        let t = shop_22();
        let top = [
            entry("e2e", "e2e", EntryKind::Folder, None),
            entry("src", "src", EntryKind::Folder, None),
            entry("next.config.ts", "next.config.ts", EntryKind::File, None),
        ];
        let tab = tab_view(
            &t,
            &source("SHOP-22", FilesFrom::Worktree),
            &top,
            3,
            Some("\u{2325}\u{2318}P".into()),
        );
        assert_eq!(tab.source.title, "SHOP-22\u{2019}s worktree");
        assert_eq!(
            tab.source.subtitle,
            format!("{} \u{b7} from main", otaq_core::naming::branch("SHOP-22"))
        );
        assert_eq!(tab.search.placeholder, "Search files by name");
        assert_eq!(tab.search.keys.as_deref(), Some("\u{2325}\u{2318}P"));
        assert_eq!(
            tab.filters,
            ["All files".to_owned(), "Changed \u{b7} 3".to_owned()]
        );
        assert_eq!(tab.opens_on, Filter::All, "SHOP-22 isn't in your check");
        assert!(!tab.rows[0].quiet, "folders read as usual");
        assert!(tab.rows[2].quiet, "an unchanged file is quieter");

        // Opening src/: its files at depth 1, the changed one marked "M +4 −1".
        let src = tree_rows(
            &[
                entry("app", "src/app", EntryKind::Folder, None),
                entry(
                    "middleware.ts",
                    "src/middleware.ts",
                    EntryKind::File,
                    Some(change(FileStatus::Modified, 4, 1, false)),
                ),
            ],
            1,
        );
        assert_eq!(src[1].depth, 1);
        let m = src[1].mark.as_ref().unwrap();
        assert_eq!(
            (m.letter, m.counts.as_deref(), m.uncommitted),
            ("M", Some("+4 \u{2212}1"), None)
        );
        assert!(!src[1].quiet);
    }

    #[test]
    fn a_ticket_that_never_started_shows_its_base_branch() {
        let s = source_view(&source("SHOP-31", FilesFrom::Base));
        assert_eq!(
            (s.kind, s.title.as_str(), s.subtitle.as_str()),
            (
                SourceKind::Base,
                "main",
                "the base branch \u{b7} SHOP-31 hasn\u{2019}t started"
            )
        );
    }

    #[test]
    fn shop_22s_search_shows_matches_flat_with_their_folder() {
        let found = FilesSearch {
            source: source("SHOP-22", FilesFrom::Worktree),
            query: "log".into(),
            matches: vec![
                FileMatch {
                    path: "e2e/login.spec.ts".into(),
                    name: "login.spec.ts".into(),
                    folder: "e2e".into(),
                    hit: [4, 7],
                    change: Some(change(FileStatus::Modified, 6, 1, false)),
                    tags: Vec::new(),
                },
                FileMatch {
                    path: "src/app/login/page.tsx".into(),
                    name: "page.tsx".into(),
                    folder: "src/app/login".into(),
                    hit: [8, 11],
                    change: Some(change(FileStatus::Modified, 3, 3, true)),
                    tags: Vec::new(),
                },
            ],
            total: 2,
        };
        let view = search_view("log", &found.matches, found.total);
        let text = |parts: &[HitPart]| {
            parts
                .iter()
                .map(|p| {
                    if p.hit {
                        format!("[{}]", p.text)
                    } else {
                        p.text.clone()
                    }
                })
                .collect::<String>()
        };
        assert_eq!(text(&view.rows[0].name), "[log]in.spec.ts");
        assert_eq!(text(&view.rows[0].folder), "e2e");
        assert_eq!(text(&view.rows[1].name), "page.tsx");
        assert_eq!(text(&view.rows[1].folder), "src/app/[log]in");
        assert_eq!(
            view.rows[1].mark.as_ref().unwrap().uncommitted,
            Some("uncommitted"),
            "changed files keep their mark"
        );
        assert_eq!((view.empty, view.more), (None, None));

        let none = search_view("chekout", &[], 0);
        assert_eq!(
            none.empty.as_deref(),
            Some("No file name matches \u{201c}chekout\u{201d}")
        );
        let many = search_view("s", &found.matches, 120);
        assert_eq!(
            many.more.as_deref(),
            Some("2 of 120 shown \u{b7} type more of the name")
        );
    }

    #[test]
    fn all_changes_include_the_uncommitted_file() {
        let view = changes_view(&shop_22(), &shop_22_changes(), &ReviewMemory::default());
        assert_eq!(view.title, "All changes");
        assert_eq!(view.summary, "3 files \u{b7} +13 \u{2212}5 against main");
        assert_eq!(
            view.note.as_deref(),
            Some("\u{b7} the PR will look like this")
        );
        assert_eq!(view.modes, ["Unified", "Split"]);
        let page = &view.files[1];
        assert_eq!(page.path, "src/app/login/page.tsx");
        assert_eq!(page.mark.uncommitted, Some("uncommitted"));
        assert!(page.commentable && page.can_open);
        // The left column: the changed files' folders, every one open.
        let tree: Vec<(u32, &str)> = view
            .tree
            .iter()
            .map(|r| (r.depth, r.name.as_str()))
            .collect();
        assert_eq!(
            tree,
            [
                (0, "e2e"),
                (1, "login.spec.ts"),
                (0, "src"),
                (1, "app"),
                (2, "login"),
                (3, "page.tsx"),
                (1, "middleware.ts"),
            ]
        );
        // Split is remembered for every ticket.
        let split = changes_view(
            &shop_22(),
            &shop_22_changes(),
            &ReviewMemory { side_by_side: true },
        );
        assert!(split.side_by_side);
        assert_ne!(view.revision, {
            let mut c = shop_22_changes();
            c.files[1].uncommitted = false;
            changes_view(&shop_22(), &c, &ReviewMemory::default()).revision
        });
    }

    #[test]
    fn no_changes_explain_themselves() {
        let mut c = shop_22_changes();
        c.files.clear();
        c.added = 0;
        c.removed = 0;
        let view = changes_view(&shop_22(), &c, &ReviewMemory::default());
        assert_eq!(view.summary, "0 files against main");
        assert_eq!(
            view.empty.unwrap().text,
            "SHOP-22 has no changes against main yet."
        );
        assert_eq!(view.note, None);
    }

    #[test]
    fn shop_18_opens_on_all_changes_with_the_bar() {
        let t = shop_18();
        assert_eq!(opens_on(&t), Filter::Changed);
        assert_eq!(
            crate::view_models::sheet::opens_on(&t),
            crate::view_models::sheet::SheetTab::Files
        );
        let mut c = shop_22_changes();
        c.source = source("SHOP-18", FilesFrom::Worktree);
        let view = changes_view(&t, &c, &ReviewMemory::default());
        assert_eq!(view.note.as_deref(), Some("\u{b7} Security passed"));

        let bar = review_bar(&t, 0, false, "Dev", Some("\u{2318}\u{21b5}".into()), false).unwrap();
        assert_eq!(bar.title, "Your check");
        assert_eq!(
            bar.line,
            "\u{b7} Security passed \u{b7} comment on any line to send it back"
        );
        let buttons: Vec<(&str, bool, Option<&str>)> = bar
            .buttons
            .iter()
            .map(|b| (b.label.as_str(), b.primary, b.keys.as_deref()))
            .collect();
        assert_eq!(
            buttons,
            [
                ("Send back \u{b7} to Dev\u{2026}", false, None),
                ("Approve", true, Some("\u{2318}\u{21b5}"))
            ]
        );
        // d0093, d0131 (`s30-02`): the stage the check's fails go to, not always the nearest.
        let bar = review_bar(&t, 0, false, "Product spec", None, true).unwrap();
        assert_eq!(
            bar.buttons[0].label,
            "Send back \u{b7} to Product spec\u{2026}"
        );
        // Another flow: its Approve is the human check's Pass, which moves it on (FLW-14, `s30-02`).
        assert_eq!(
            (bar.buttons[1].action, bar.buttons[1].label.as_str()),
            (BarAction::Pass, "Approve")
        );
        // Not in your check: no bar; SHOP-22 opens on its tree.
        assert_eq!(review_bar(&shop_22(), 0, false, "Dev", None, false), None);
        assert_eq!(
            crate::view_models::sheet::opens_on(&shop_22()),
            crate::view_models::sheet::SheetTab::Overview
        );
    }

    #[test]
    fn with_comments_the_bar_sends_them_back_first() {
        let t = shop_18();
        let bar = review_bar(&t, 1, true, "Dev", None, false).unwrap();
        assert_eq!(bar.title, "1 line comment, 1 being written");
        assert_eq!(bar.line, "\u{b7} they go to Dev with Send back");
        let buttons: Vec<(BarAction, &str, bool)> = bar
            .buttons
            .iter()
            .map(|b| (b.action, b.label.as_str(), b.primary))
            .collect();
        assert_eq!(
            buttons,
            [
                (BarAction::Approve, "Approve", false),
                (BarAction::SendBack, "Send back with comments", true)
            ]
        );
        assert_eq!(comments_line(2, false), "2 line comments");
        assert_eq!(comments_line(0, true), "1 line comment being written");
        // An epic takes no comments and isn't sent back (EPC-05): Approve only.
        let mut epic = shop_18();
        epic.kind = otaq_core::domain::Kind::Epic;
        let bar = review_bar(&epic, 0, false, "Dev", None, false).unwrap();
        assert_eq!(bar.line, "\u{b7} Security passed");
        assert_eq!(bar.buttons.len(), 1);
        assert!(!can_comment(&epic));
    }

    #[test]
    fn a_line_comment_waits_not_sent_yet_and_goes_out_with_send_back() {
        let t = shop_18();
        let mut c = ReviewComment {
            id: "c1".into(),
            round: 1,
            file: "src/app/api/checkout/coupon/route.ts".into(),
            line: 21,
            side: Side::New,
            body: "Expired codes should say \u{201c}expired\u{201d}, not \u{201c}invalid\u{201d}."
                .into(),
            created_at: 0,
            sent_at: None,
        };
        let view = comments_view("SHOP-18", std::slice::from_ref(&c), can_comment(&t), "Dev");
        let card = &view.threads[0].comments[0];
        assert_eq!(card.meta, "on line 21 \u{b7} not sent yet");
        assert!(card.pending && card.editable);
        assert_eq!(view.pending, 1);
        assert_eq!(view.texts.hint, "Goes with Send back, to Dev");
        // Send back lists it for the agent…
        let plan = crate::view_models::send_back::plan(&t, std::slice::from_ref(&c), &[], "Dev");
        assert_eq!(
            plan.comments,
            [(
                "src/app/api/checkout/coupon/route.ts \u{b7} L21".to_owned(),
                c.body.clone()
            )]
        );
        // …and once sent it reads so, and can't be changed.
        c.sent_at = Some(5);
        let view = comments_view("SHOP-18", std::slice::from_ref(&c), false, "Dev");
        let card = &view.threads[0].comments[0];
        assert_eq!(card.meta, "on line 21 \u{b7} sent \u{b7} round 1");
        assert!(!card.editable);
        assert_eq!(view.pending, 0);
    }

    #[test]
    fn the_gallery_draws_the_frames() {
        let g = gallery();
        assert_eq!(g.tab.source.title, "SHOP-22\u{2019}s worktree");
        assert_eq!(g.src[3].name, "middleware.ts");
        assert_eq!(g.file.opens_in, FileMode::Diff);
        assert_eq!(g.search.rows.len(), 2);
        assert_eq!(g.changes.note.as_deref(), Some("\u{b7} Security passed"));
        assert_eq!(g.diff.hunks[0].lines.len(), 6);
        assert_eq!(
            g.comments.threads[0].comments[0].meta,
            "on line 21 \u{b7} not sent yet"
        );
        let titles: Vec<&str> = g.bars.iter().map(|b| b.title.as_str()).collect();
        assert_eq!(titles, ["Your check", "1 line comment, 1 being written"]);
    }

    /// The Wishlist epic's Files (FTB-06, `s25-11`): its branch with "40.1 merged" and "40.2 on top · its own
    /// worktree", each file tagged with the child that changed it ("40.1", "40.2 ✎" opening 40.2's worktree), files no
    /// child wrote dimmed, and 40.2's file with its "being written" note.
    #[test]
    fn the_wishlist_epics_files_and_their_tags() {
        use otaq_core::board::files::{ChildTag, TagKind};
        let mut tickets = crate::view_models::epic_sheet::samples::wishlist();
        tickets[2].work = Some(otaq_core::board::tickets::Work {
            worktree: Some("/tmp/wt/SHOP-40.2".into()),
            ..Default::default()
        });
        let epic = &tickets[0];
        let mut branch = source("SHOP-40", FilesFrom::Branch);
        branch.branch = otaq_core::naming::branch("PAY-123");
        let view = epic_source(epic, &branch, &tickets);
        assert_eq!(view.title, "Wishlist\u{2019}s branch");
        assert_eq!(
            view.subtitle,
            format!("{} \u{b7} what its tickets merged", branch.branch)
        );
        let legend: Vec<(&str, bool)> = view
            .legend
            .iter()
            .map(|t| (t.label.as_str(), t.writing))
            .collect();
        assert_eq!(
            legend,
            [
                ("40.1 merged", false),
                ("40.2 on top \u{b7} its own worktree", true)
            ]
        );
        // A ticket's source has no legend; an epic that never started reads its base branch.
        assert!(source_view(&branch).legend.is_empty());
        assert_eq!(
            epic_source(epic, &source("SHOP-40", FilesFrom::Base), &tickets).title,
            "main"
        );

        let tag = |k: &str, kind| ChildTag { key: key(k), kind };
        let rows = tree_rows(
            &[
                TreeEntry {
                    tags: vec![tag("SHOP-40.1", TagKind::Merged)],
                    ..entry(
                        "route.ts",
                        "src/app/api/wishlist/route.ts",
                        EntryKind::File,
                        Some(change(FileStatus::Added, 12, 0, false)),
                    )
                },
                TreeEntry {
                    tags: vec![tag("SHOP-40.2", TagKind::Writing)],
                    ..entry(
                        "WishlistButton.tsx",
                        "src/components/WishlistButton.tsx",
                        EntryKind::File,
                        Some(change(FileStatus::Added, 12, 0, true)),
                    )
                },
                entry("package.json", "package.json", EntryKind::File, None),
            ],
            0,
        );
        let shown: Vec<(&str, Vec<&str>, bool, Option<&str>)> = rows
            .iter()
            .map(|r| {
                (
                    r.name.as_str(),
                    r.tags.iter().map(|t| t.label.as_str()).collect(),
                    r.quiet,
                    r.child.as_deref(),
                )
            })
            .collect();
        assert_eq!(
            shown,
            [
                ("route.ts", vec!["40.1"], false, None),
                (
                    "WishlistButton.tsx",
                    vec!["40.2 \u{270e}"],
                    false,
                    Some("SHOP-40.2")
                ),
                ("package.json", vec![], true, None),
            ]
        );

        let page = file_page(
            &FileContent {
                source: branch.clone(),
                path: "src/components/WishlistButton.tsx".into(),
                child: Some(key("SHOP-40.2")),
                size: 300,
                binary: false,
                text: "'use client'\n".into(),
                truncated: false,
                change: Some(change(FileStatus::Added, 12, 0, true)),
                tags: vec![tag("SHOP-40.2", TagKind::Writing)],
            },
            &ReviewMemory::default(),
        );
        assert_eq!(
            page.subtitle,
            "40.2\u{2019}s change, on top of the epic\u{2019}s branch \u{b7} not merged yet"
        );
        assert_eq!(page.opens_in, FileMode::Diff);
        assert_eq!(page.child.as_deref(), Some("SHOP-40.2"));
        assert_eq!(
            writing_note(&key("SHOP-40.2"), Some("Rust developer")),
            "40.2\u{2019}s agent (Rust developer) is writing this file in 40.2\u{2019}s own worktree; you see its \
             change on top of the epic\u{2019}s branch."
        );
        assert_eq!(page.writing, Some(writing_note(&key("SHOP-40.2"), None)));
    }

    #[test]
    fn a_file_opens_as_itself_or_its_diff() {
        let content = FileContent {
            source: source("SHOP-22", FilesFrom::Worktree),
            path: "src/middleware.ts".into(),
            child: None,
            size: 120,
            binary: false,
            text: "import { NextResponse } from 'next/server'\n\nexport function middleware() {}\n"
                .into(),
            truncated: false,
            change: Some(change(FileStatus::Modified, 4, 1, false)),
            tags: Vec::new(),
        };
        let page = file_page(&content, &ReviewMemory::default());
        assert_eq!(page.split_modes, ["Unified", "Split"]);
        assert!(!page.side_by_side);
        // Split chosen on All changes stays Split on a picked file's Diff (dogfood #61).
        let split = ReviewMemory { side_by_side: true };
        assert!(file_page(&content, &split).side_by_side);
        assert!(deleted_page(&content.source, "src/old.ts", None, &split).side_by_side);
        assert_eq!(
            page.subtitle,
            "in SHOP-22\u{2019}s worktree \u{b7} +4 \u{2212}1 against main"
        );
        assert_eq!((page.opens_in, page.can_diff), (FileMode::Diff, true));
        assert_eq!(page.lines.len(), 3);
        assert_eq!(page.lines[2].number, 3);
        assert_eq!(page.new_ticket, "New ticket from this file");

        let readme = FileContent {
            path: "README.md".into(),
            change: None,
            ..content.clone()
        };
        let page = file_page(&readme, &ReviewMemory::default());
        assert_eq!(
            page.subtitle,
            "in SHOP-22\u{2019}s worktree \u{b7} not changed"
        );
        assert_eq!((page.opens_in, page.can_diff), (FileMode::File, false));

        let logo = FileContent {
            path: "public/logo.png".into(),
            binary: true,
            text: String::new(),
            size: 2048,
            ..readme
        };
        assert_eq!(
            file_page(&logo, &ReviewMemory::default()).note.as_deref(),
            Some("Binary file \u{b7} 2 KB")
        );
        assert_eq!(
            new_ticket_body("src/middleware.ts"),
            "`src/middleware.ts`\n\n"
        );
        assert_eq!(
            file_page(&content, &ReviewMemory::default()).new_ticket_body,
            "`src/middleware.ts`\n\n"
        );
        let gone = deleted_page(
            &content.source,
            "src/old.ts",
            None,
            &ReviewMemory::default(),
        );
        assert_eq!(gone.subtitle, "in SHOP-22\u{2019}s worktree \u{b7} deleted");
        assert!(!gone.can_file && gone.opens_in == FileMode::Diff);
    }
}
