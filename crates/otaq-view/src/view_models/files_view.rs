//! The project **Files view** (FIL-01, FIL-03, FIL-04, FIL-07, FIL-10; d0087 and its 2026-10-23 lines; canvas
//! `s28-03`, `s28-04`, `s28-08`, `s28-09`): the toolbar's third view, Board · List · Files. On the left the source bar
//! ("main · every ticket’s changes" over "the base branch · 8 tickets, 11 files"), the search box (⌥⌘P, Go to file),
//! All · Changed 11 · Needs you 6 · Same file 1, and the tree of the project's base branch with every ticket's changes
//! on top: a changed file carries one chip per ticket, its mark and +/−, a folder counts the changed files below it, a
//! ticket on another base is tagged with that branch. The source bar opens the picker ("Find a ticket or epic"): BASE
//! BRANCH, EPICS · THE EPIC’S BRANCH, TICKETS · THEIR WORKTREE and the tickets with no worktree yet; a ticket or epic
//! picked there shows its own Files tab's tree, read-only, with "Open SHOP-29" and the back link.
//!
//! The rows are the tab's ([`TreeRow`], [`SearchView`]), widened with the chips; the data is `board/project_files.rs`'s
//! (`files.project_*`). The base branch is the project's setting (d0051): every label names it. The page keeps the
//! filter, the folders opened, the search being typed and the picked source; it asks again every 5 s while an agent
//! works ([`Page::working`], FIL-10) and on the tickets' events.
//!
//! Implementation choice: folders are listed one level at a time as the tab does; `s28-03`'s "api/checkout/coupon"
//! (single-child folders drawn as one row) isn't a drawn rule and isn't done.

use std::collections::{BTreeMap, HashSet};

use otaq_core::board::files::EntryKind;
use otaq_core::board::project_files::{
    EpicSource, FileOwner, LineRange, NoWorktree, OwnerLines, OwnerTicket, ProjectChanges,
    ProjectCounts, ProjectEntry, ProjectFile, ProjectSearch, ProjectSource, ProjectSources,
    UserWorktree,
};
use otaq_core::board::tickets::Ticket;
use otaq_core::domain::{Status, StatusSet, TicketKey};
use otaq_core::git::diff::{FileDiff, FileStatus};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::card::CardViewModel;
use super::diff::{self as diff_vm, ReviewMemory};
use super::diff_page::{self as diff_page, SpanView};
use super::files_tab::{
    self as tab, ChangeRow, ChipView, EmptyView, EntryKindView, FileMode, MarkView, SearchBox,
    SearchRow, SearchView, TabView, TreeRow,
};
use super::flow_columns::Columns;
use super::menu::MenuItem;
use super::tool_avatar::ToolAvatarView;
use crate::icons::IconName;
use crate::shell::tilde;
use crate::theme::Palette;

// ---- Words ----

pub const LOADING: &str = "Loading the tickets\u{2019} changes\u{2026}";
/// A filter's count while the tickets' changes load ("Changed …").
pub const COUNTING: &str = "\u{2026}";
pub const EMPTY_TITLE: &str = "No ticket is changing files right now.";
pub const EMPTY_TEXT: &str = "Start one from the board, or ask the Project agent.";
pub const FIND: &str = "Find a ticket or epic";
pub const BASE_SECTION: &str = "BASE BRANCH";
pub const THE_DEFAULT: &str = "the default";
pub const EPICS_SECTION: &str = "EPICS \u{b7} THE EPIC\u{2019}S BRANCH";
pub const TICKETS_SECTION: &str = "TICKETS \u{b7} THEIR WORKTREE";
/// Your own git worktrees of the repository (#43), shown read-only.
pub const YOURS_SECTION: &str = "YOUR WORKTREES";
pub const YOUR_WORKTREE: &str = "your worktree";
pub const READ_ONLY: &str = "read-only";

/// "main · every ticket’s changes": the view's default source, and the picker's back link.
pub fn every_change(base: &str) -> String {
    format!("{base} \u{b7} every ticket\u{2019}s changes")
}

fn plural(n: u32, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

// ---- The source bar and the filters ----

/// What the left column shows (FIL-04). `FilesViewFilter` on the page.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename = "FilesViewFilter")]
pub enum Filter {
    /// The base's tree, folder by folder.
    All,
    /// Every changed file.
    Changed,
    /// The files changed by tickets that need you.
    NeedsYou,
    /// The files two tickets on the same base change (FIL-06).
    SameFile,
}

/// The bar on the tree that opens the picker: "main · every ticket’s changes" over "the base branch · 8 tickets, 11
/// files". `FilesViewSource` on the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "FilesViewSource")]
pub struct SourceBar {
    pub title: String,
    pub subtitle: String,
}

/// `counts` is `None` while the tickets' changes load (the bare base shows at once, FIL-02). One of your worktrees:
/// "experiment/accent · your worktree" over "~/code/habitat-experiment · read-only" (#43).
pub fn source_bar(source: &ProjectSource, counts: Option<&ProjectCounts>) -> SourceBar {
    if let Some(folder) = &source.worktree {
        return SourceBar {
            title: format!("{} \u{b7} {YOUR_WORKTREE}", source.base_branch),
            subtitle: format!("{} \u{b7} {READ_ONLY}", tilde(folder)),
        };
    }
    let subtitle = match counts {
        None => "the base branch".to_owned(),
        Some(c) if c.tickets == 0 => {
            "the base branch \u{b7} no ticket is changing files".to_owned()
        }
        Some(c) => format!(
            "the base branch \u{b7} {}, {}",
            plural(c.tickets, "ticket", "tickets"),
            plural(c.changed, "file", "files")
        ),
    };
    SourceBar {
        title: every_change(&source.base_branch),
        subtitle,
    }
}

/// One filter: "Changed 11"; Needs you's tooltip "Needs you · 6 files" over "changed by the 4 tickets that need
/// you". `FilesViewFilterOption` on the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "FilesViewFilterOption")]
pub struct FilterOption {
    pub filter: Filter,
    pub label: String,
    pub tooltip: Option<String>,
    pub detail: Option<String>,
}

pub fn filters(counts: Option<&ProjectCounts>) -> Vec<FilterOption> {
    let count =
        |n: fn(&ProjectCounts) -> u32| counts.map_or(COUNTING.to_owned(), |c| n(c).to_string());
    let option = |filter, label: String| FilterOption {
        filter,
        label,
        tooltip: None,
        detail: None,
    };
    vec![
        option(Filter::All, "All".to_owned()),
        option(Filter::Changed, format!("Changed {}", count(|c| c.changed))),
        FilterOption {
            tooltip: counts
                .map(|c| format!("Needs you \u{b7} {}", plural(c.needs_you, "file", "files"))),
            detail: counts.map(|c| match c.needs_you_tickets {
                1 => "changed by the ticket that needs you".to_owned(),
                n => format!("changed by the {n} tickets that need you"),
            }),
            ..option(
                Filter::NeedsYou,
                format!("Needs you {}", count(|c| c.needs_you)),
            )
        },
        option(
            Filter::SameFile,
            format!("Same file {}", count(|c| c.same_file)),
        ),
    ]
}

// ---- A ticket's colour ----

/// The colours a ticket takes in the view, from the terminal palette (`ansi-*`, every theme has them): its chip in the
/// tree, its bar beside the lines it changes and its edge in Changed by and All changes, so chip and bar match
/// (`s28-03`; Q456, built as option 2). Two tickets working on one file stay apart, which status colours can't do.
/// Picked by the ticket's number, in the order that gives page 28's tickets their drawn colours (SHOP-29 blue,
/// SHOP-19 violet, SHOP-22 teal, SHOP-36 amber).
pub const TICKET_COLOURS: [&str; 12] = [
    "ansi-3", "ansi-12", "ansi-11", "ansi-10", "ansi-14", "ansi-4", "ansi-13", "ansi-5", "ansi-1",
    "ansi-2", "ansi-6", "ansi-9",
];

fn colour_index(key: &TicketKey) -> usize {
    (key.number as usize + 5 * key.child.unwrap_or(0) as usize) % TICKET_COLOURS.len()
}

/// A ticket's own colour, the same wherever it shows.
pub fn ticket_colour(key: &TicketKey) -> &'static str {
    TICKET_COLOURS[colour_index(key)]
}

/// One colour per ticket of a file, in order: each its own, or the next free one when an earlier ticket of the same
/// file already has it.
pub fn owner_colours<'a>(keys: impl IntoIterator<Item = &'a TicketKey>) -> Vec<&'static str> {
    let mut taken: Vec<usize> = Vec::new();
    keys.into_iter()
        .map(|key| {
            let mut i = colour_index(key);
            for _ in 0..TICKET_COLOURS.len() {
                if !taken.contains(&i) {
                    break;
                }
                i = (i + 1) % TICKET_COLOURS.len();
            }
            taken.push(i);
            TICKET_COLOURS[i]
        })
        .collect()
}

// ---- The tree ----

fn chips(owners: &[FileOwner]) -> Vec<ChipView> {
    owners
        .iter()
        .zip(owner_colours(owners.iter().map(|o| &o.key)))
        .map(|(o, colour)| ChipView {
            key: o.key.to_string(),
            colour,
        })
        .collect()
}

/// A file's mark from its owners: the first one's letter (the one that needs you, else flow order), +/− only while
/// one ticket changes it (`s28-03`: pricing.ts's two chips have none), uncommitted when any of it is.
fn mark(owners: &[FileOwner]) -> Option<MarkView> {
    let first = owners.first()?;
    let mut mark = tab::mark(&otaq_core::board::files::FileChange {
        status: first.file.status,
        added: first.file.added,
        removed: first.file.removed,
        binary: first.file.binary,
        uncommitted: owners.iter().any(|o| o.uncommitted),
    });
    if owners.len() > 1 {
        mark.counts = None;
    }
    Some(mark)
}

/// The other base branch a ticket changing the file is on ("release/2.3").
fn other_branch(owners: &[FileOwner]) -> Option<String> {
    owners
        .iter()
        .find(|o| o.other_base)
        .map(|o| o.base_branch.clone())
}

/// "3 changed".
fn changed_below(n: u32) -> Option<String> {
    (n > 0).then(|| format!("{n} changed"))
}

fn file_row(
    name: String,
    path: String,
    kind: EntryKindView,
    depth: u32,
    owners: &[FileOwner],
) -> TreeRow {
    TreeRow {
        name,
        path,
        kind,
        depth,
        mark: mark(owners),
        quiet: owners.is_empty(),
        opens: kind != EntryKindView::Submodule,
        tags: Vec::new(),
        child: None,
        chips: chips(owners),
        changed_below: None,
        branch: other_branch(owners),
        struck: owners
            .first()
            .is_some_and(|o| o.file.status == FileStatus::Deleted),
    }
}

fn folder_row(name: String, path: String, depth: u32, changed: u32) -> TreeRow {
    TreeRow {
        name,
        path,
        kind: EntryKindView::Folder,
        depth,
        mark: None,
        quiet: false,
        opens: true,
        tags: Vec::new(),
        child: None,
        chips: Vec::new(),
        changed_below: changed_below(changed),
        branch: None,
        struck: false,
    }
}

/// A folder's entries at `depth` (`files.project_tree`), as the tab's rows with the chips.
pub fn entry_rows(entries: &[ProjectEntry], depth: u32) -> Vec<TreeRow> {
    entries
        .iter()
        .map(|e| match e.kind {
            EntryKind::Folder => folder_row(e.name.clone(), e.path.clone(), depth, e.changed_below),
            kind => file_row(
                e.name.clone(),
                e.path.clone(),
                kind.into(),
                depth,
                &e.owners,
            ),
        })
        .collect()
}

/// Two owners of one file on the same base (FIL-06), as `files.project_*` counts Same file.
fn same_file(owners: &[FileOwner]) -> bool {
    let mut bases = HashSet::new();
    owners.iter().any(|o| !bases.insert(o.base_branch.as_str()))
}

/// Every changed path with its owners, in the groups' order (needs you first, then flow order).
fn by_path(changes: &ProjectChanges) -> BTreeMap<String, Vec<FileOwner>> {
    let mut map: BTreeMap<String, Vec<FileOwner>> = BTreeMap::new();
    for group in &changes.groups {
        let t = &group.ticket;
        for entry in &group.files {
            map.entry(entry.file.path.clone())
                .or_default()
                .push(FileOwner {
                    key: t.key.clone(),
                    status: t.status,
                    branch: t.branch.clone(),
                    base_branch: t.base_branch.clone(),
                    other_base: t.other_base,
                    needs_you: t.needs_you,
                    file: entry.file.clone(),
                    uncommitted: entry.uncommitted,
                });
        }
    }
    map
}

/// The changed files under `filter` (Changed, Needs you, Same file) as a tree, every folder open with its count:
/// folders first, then by name ignoring case, as the base's tree orders them. `All` lists nothing here (the base's
/// tree is asked for folder by folder).
pub fn changed_rows(changes: &ProjectChanges, filter: Filter) -> Vec<TreeRow> {
    #[derive(Default)]
    struct Folder {
        folders: BTreeMap<(String, String), Folder>,
        files: BTreeMap<(String, String), (String, Vec<FileOwner>)>,
        count: u32,
    }
    let order = |name: &str| (name.to_lowercase(), name.to_owned());
    let mut top = Folder::default();
    for (path, owners) in by_path(changes) {
        let keep = match filter {
            Filter::All => false,
            Filter::Changed => true,
            Filter::NeedsYou => owners.iter().any(|o| o.needs_you),
            Filter::SameFile => same_file(&owners),
        };
        if !keep {
            continue;
        }
        let mut parts: Vec<&str> = path.split('/').collect();
        let Some(name) = parts.pop() else { continue };
        let mut folder = &mut top;
        for part in parts {
            folder = folder.folders.entry(order(part)).or_default();
            folder.count += 1;
        }
        folder.files.insert(order(name), (path.clone(), owners));
    }
    fn walk(folder: &Folder, prefix: &str, depth: u32, rows: &mut Vec<TreeRow>) {
        for ((_, name), inner) in &folder.folders {
            let path = format!("{prefix}{name}");
            rows.push(folder_row(name.clone(), path.clone(), depth, inner.count));
            walk(inner, &format!("{path}/"), depth + 1, rows);
        }
        for ((_, name), (path, owners)) in &folder.files {
            rows.push(file_row(
                name.clone(),
                path.clone(),
                EntryKindView::File,
                depth,
                owners,
            ));
        }
    }
    let mut rows = Vec::new();
    walk(&top, "", 0, &mut rows);
    rows
}

// ---- The view ----

/// The Files view's left column and frame (`files_view.view`). `FilesViewPage` on the page.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(rename = "FilesViewPage")]
pub struct Page {
    pub project: String,
    /// The project's base branch ("develop"): every label names it.
    pub base_branch: String,
    pub source: SourceBar,
    pub search: SearchBox,
    pub filters: Vec<FilterOption>,
    pub filter: Filter,
    /// "Loading the tickets’ changes…" while only the base is listed (`s28-09`).
    pub loading: Option<&'static str>,
    /// All: the top folder's entries; the other filters: their files as a tree, every folder open.
    pub rows: Vec<TreeRow>,
    /// No ticket is changing files (`s28-09`): the file pane says so.
    pub empty: Option<EmptyView>,
    /// The file pane before a file is picked.
    pub pick: &'static str,
    /// A picked file's subtitle while it is read: "on main · checking which tickets change it…" (`s28-09`).
    pub checking: String,
    /// Right-click on a file (FIL-09): New ticket from this file ⌘N, Copy path.
    pub menu: Vec<MenuItem>,
    /// A ticket's agent is working: the page asks again every 5 s while the view is shown (commits have no event,
    /// FIL-10).
    pub working: bool,
}

/// The keys the view shows: Go to file's in the search box, New ticket's in a file's menu.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ViewKeys {
    pub go_to_file: Option<String>,
    pub new_ticket: Option<String>,
}

/// The view for `rows`: `counts` is `None` while only the base is listed.
pub fn page(
    source: &ProjectSource,
    counts: Option<&ProjectCounts>,
    filter: Filter,
    rows: Vec<TreeRow>,
    keys: ViewKeys,
    working: bool,
) -> Page {
    let mut page = Page {
        project: source.project.clone(),
        base_branch: source.base_branch.clone(),
        source: source_bar(source, counts),
        search: SearchBox {
            placeholder: tab::SEARCH_PLACEHOLDER,
            keys: keys.go_to_file,
        },
        filters: filters(counts),
        filter,
        loading: counts.is_none().then_some(LOADING),
        rows,
        empty: counts.filter(|c| c.tickets == 0).map(|_| EmptyView {
            title: EMPTY_TITLE,
            text: EMPTY_TEXT.to_owned(),
        }),
        pick: tab::PICK_A_FILE,
        checking: format!(
            "on {} \u{b7} checking which tickets change it\u{2026}",
            source.base_branch
        ),
        menu: file_menu(keys.new_ticket),
        working,
    };
    if source.worktree.is_some() {
        // One of your worktrees (#43): its tree alone, no ticket's changes to filter, count or wait for.
        page.filters = Vec::new();
        page.loading = None;
        page.empty = None;
        page.checking = format!("on {} \u{b7} {READ_ONLY}", source.base_branch);
        page.working = false;
    }
    page
}

pub const COPY_PATH: &str = "Copy path";

/// A file's menu (FIL-09, `s28-07`): New ticket from this file with its key, Copy path.
pub fn file_menu(new_ticket_keys: Option<String>) -> Vec<MenuItem> {
    let new_ticket = MenuItem::new("new_ticket", tab::NEW_TICKET).icon(IconName::Plus);
    vec![
        match new_ticket_keys {
            Some(keys) => new_ticket.shortcut(keys),
            None => new_ticket,
        },
        MenuItem::new("copy_path", COPY_PATH).icon(IconName::Copy),
    ]
}

/// An agent is at work on the ticket right now, so its changes move without an event (FIL-10).
pub fn agent_works(status: Status) -> bool {
    matches!(
        status,
        Status::Working
            | Status::WorkingInBackground
            | Status::MergeConflictFixing
            | Status::Checking
    )
}

/// The search's matches (`files.project_search`), as the tab's with the chips.
pub fn search_view(query: &str, found: &ProjectSearch) -> SearchView {
    let rows: Vec<SearchRow> = found
        .matches
        .iter()
        .map(|m| {
            let name_from = if m.folder.is_empty() {
                0
            } else {
                m.folder.chars().count() + 1
            };
            SearchRow {
                path: m.path.clone(),
                name: tab::hit_parts(&m.name, name_from, m.hit),
                folder: tab::hit_parts(&m.folder, 0, m.hit),
                mark: mark(&m.owners),
                quiet: m.owners.is_empty(),
                tags: Vec::new(),
                child: None,
                chips: chips(&m.owners),
                branch: other_branch(&m.owners),
            }
        })
        .collect();
    let query = query.trim();
    SearchView {
        query: query.to_owned(),
        empty: (rows.is_empty() && !query.is_empty())
            .then(|| format!("No file name matches \u{201c}{query}\u{201d}")),
        more: (found.total as usize > rows.len()).then(|| {
            format!(
                "{} of {} shown \u{b7} type more of the name",
                rows.len(),
                found.total
            )
        }),
        rows,
    }
}

// ---- The picker ----

/// What a picked row shows. `FilesViewPick` on the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(rename = "FilesViewPick")]
pub enum Pick {
    /// The base branch with every ticket's changes (the default).
    Base,
    /// A ticket's worktree or an epic's branch: its own Files tab's tree (`files.*`).
    Ticket { key: String },
    /// One of your own worktrees (#43): its folder, the view's requests then read it instead of the base.
    Worktree { path: String },
}

/// One row of the picker: "SHOP-29" "Coupon field" "Dev · 2 files". `FilesViewPickerRow` on the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "FilesViewPickerRow")]
pub struct PickerRow {
    pub pick: Pick,
    /// The ticket's key; `None` for the base branch and an epic (its key is in its title).
    pub key: Option<String>,
    pub title: String,
    pub detail: Option<String>,
}

/// A labelled group of rows. `FilesViewPickerSection` on the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "FilesViewPickerSection")]
pub struct PickerSection {
    pub title: &'static str,
    pub rows: Vec<PickerRow>,
}

/// The picker (FIL-07, `s28-04`). `FilesViewPicker` on the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "FilesViewPicker")]
pub struct Picker {
    pub placeholder: &'static str,
    /// BASE BRANCH, EPICS · THE EPIC’S BRANCH, TICKETS · THEIR WORKTREE and YOUR WORKTREES (each when there are any).
    pub sections: Vec<PickerSection>,
    /// "No worktree yet: SHOP-12, SHOP-44 (Grill) … and 3 in Backlog. A ticket gets its worktree when it reaches Dev."
    pub no_worktree: Option<String>,
}

/// "40.1".
fn short(key: &TicketKey) -> String {
    match key.child {
        Some(child) => format!("{}.{child}", key.number),
        None => key.to_string(),
    }
}

fn epic_row(epic: &EpicSource) -> PickerRow {
    let list = |keys: &[TicketKey]| keys.iter().map(short).collect::<Vec<_>>().join(", ");
    let detail: Vec<String> = [
        (!epic.merged.is_empty()).then(|| format!("{} merged", list(&epic.merged))),
        (!epic.on_top.is_empty()).then(|| format!("{} on top", list(&epic.on_top))),
    ]
    .into_iter()
    .flatten()
    .collect();
    PickerRow {
        pick: Pick::Ticket {
            key: epic.key.to_string(),
        },
        key: None,
        title: format!("{} \u{b7} {}", epic.title, epic.key),
        detail: (!detail.is_empty()).then(|| detail.join(" \u{b7} ")),
    }
}

/// "Dev · 3 files", "Dev · release/2.3 · 2 files", "PR open · 1 file".
fn ticket_row(t: &OwnerTicket) -> PickerRow {
    let place = match t.status {
        Status::PrOpen => Some("PR open".to_owned()),
        _ => t.stage.clone(),
    };
    let detail: Vec<String> = [
        place,
        t.other_base.then(|| t.base_branch.clone()),
        Some(plural(t.files, "file", "files")),
    ]
    .into_iter()
    .flatten()
    .collect();
    PickerRow {
        pick: Pick::Ticket {
            key: t.key.to_string(),
        },
        key: Some(t.key.to_string()),
        title: t.title.clone(),
        detail: Some(detail.join(" \u{b7} ")),
    }
}

/// "experiment/accent" "~/code/habitat-experiment · read-only".
fn worktree_row(w: &UserWorktree) -> PickerRow {
    PickerRow {
        pick: Pick::Worktree {
            path: w.path.clone(),
        },
        key: None,
        title: w.label().to_owned(),
        detail: Some(format!("{} \u{b7} {READ_ONLY}", tilde(&w.path))),
    }
}

/// The tickets with no worktree yet, by where they are: "SHOP-12, SHOP-44 (Grill), SHOP-35, SHOP-20 (Ready) and 3 in
/// Backlog", then why ("when it reaches Dev": `first_stage`, the flow's first agent stage).
pub fn no_worktree_line(tickets: &[NoWorktree], first_stage: &str) -> Option<String> {
    let mut groups: Vec<(String, Vec<String>)> = Vec::new();
    let mut backlog = 0;
    for t in tickets {
        let place = match &t.stage {
            Some(stage) => stage.clone(),
            None if t.status.set() == StatusSet::Backlog => {
                backlog += 1;
                continue;
            }
            None => "Ready".to_owned(),
        };
        match groups.iter_mut().find(|(p, _)| *p == place) {
            Some((_, keys)) => keys.push(t.key.to_string()),
            None => groups.push((place, vec![t.key.to_string()])),
        }
    }
    if groups.is_empty() && backlog == 0 {
        return None;
    }
    let named = groups
        .iter()
        .map(|(place, keys)| format!("{} ({place})", keys.join(", ")))
        .collect::<Vec<_>>()
        .join(", ");
    let list = match (named.is_empty(), backlog) {
        (_, 0) => named,
        (true, n) => format!("{n} in Backlog"),
        (false, n) => format!("{named} and {n} in Backlog"),
    };
    Some(format!(
        "No worktree yet: {list}. A ticket gets its worktree when it reaches {first_stage}."
    ))
}

pub fn picker(sources: &ProjectSources, first_stage: &str) -> Picker {
    let mut sections = vec![PickerSection {
        title: BASE_SECTION,
        rows: vec![PickerRow {
            pick: Pick::Base,
            key: None,
            title: every_change(&sources.source.base_branch),
            detail: Some(THE_DEFAULT.to_owned()),
        }],
    }];
    if !sources.epics.is_empty() {
        sections.push(PickerSection {
            title: EPICS_SECTION,
            rows: sources.epics.iter().map(epic_row).collect(),
        });
    }
    if !sources.tickets.is_empty() {
        sections.push(PickerSection {
            title: TICKETS_SECTION,
            rows: sources.tickets.iter().map(ticket_row).collect(),
        });
    }
    if !sources.yours.is_empty() {
        sections.push(PickerSection {
            title: YOURS_SECTION,
            rows: sources.yours.iter().map(worktree_row).collect(),
        });
    }
    Picker {
        placeholder: FIND,
        sections,
        no_worktree: no_worktree_line(&sources.no_worktree, first_stage),
    }
}

/// A ticket or an epic picked in the picker (`s28-04`, "One ticket’s worktree"): its own Files tab, read-only, with
/// the back link to the base and "Open SHOP-29". `FilesViewPicked` on the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "FilesViewPicked")]
pub struct Picked {
    pub key: String,
    /// "main · every ticket’s changes": back to the base.
    pub back: String,
    /// The tab's frame, its subtitle ending "· read-only".
    pub tab: TabView,
    /// "Open SHOP-29": its sheet on the Files tab.
    pub open: String,
    /// "Only SHOP-29’s changes are marked: the same tree as its Files tab. Open SHOP-29 opens its sheet on that tab."
    pub note: String,
}

pub fn picked(base_branch: &str, mut tab: TabView) -> Picked {
    let key = tab.key.clone();
    tab.source.subtitle = format!("{} \u{b7} {READ_ONLY}", tab.source.subtitle);
    Picked {
        back: every_change(base_branch),
        open: format!("Open {key}"),
        note: format!(
            "Only {key}\u{2019}s changes are marked: the same tree as its Files tab. Open {key} opens its sheet on \
             that tab."
        ),
        tab,
        key,
    }
}

// ---- A selected file ----

pub const CHANGED_BY: &str = "Changed by";
pub const CHANGED_BY_HINT: &str = "Click a ticket to open its sheet on the Files tab. Diff shows one ticket\u{2019}s change at a time.";
pub const OPEN_ON_FILES: &str = "Open on Files";
/// The narrow line of chips' link (`s28-08`).
pub const OPEN: &str = "Open";
pub const OF: &str = "of";
pub const NO_PREVIEW: &str = "No preview";
/// Over this many lines, a file some ticket changes shows only the parts tickets change, the rest folded (FIL-05).
pub const HUGE_LINES: usize = 3_000;
/// The lines kept around each change of a huge file.
pub const FOLD_CONTEXT: u32 = 3;
/// A path longer than this many characters is cut in the middle, the full path on hover (FIL-05, `s28-10`).
pub const LONG_PATH: usize = 48;

/// What the view knows of a ticket changing files besides `files.project_*`: its title, where it is, its card's
/// line and tool (as the board's card says them), and when it last changed. Built by the API from the board's cards.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct OwnerFacts {
    pub title: String,
    /// "Dev", "Security", "PR open".
    pub place: Option<String>,
    /// The card's one line ("Working · back from Security", "Needs you: allow rm -rf .next?") and its colour.
    pub line: Option<(String, Palette)>,
    /// The tool mark of whoever has the turn (`null` while only one tool is connected, d0086).
    pub avatar: Option<ToolAvatarView>,
    /// When the ticket last changed (ms): Diff starts on the latest when none needs you.
    pub changed_at: i64,
}

/// A ticket's facts from its board card (`board::ticket_card`): where it is ("PR open" for an open PR, else its stage
/// or column), the card's line and avatar.
pub fn owner_facts(columns: &Columns, ticket: &Ticket, card: CardViewModel) -> OwnerFacts {
    let place = match ticket.status {
        Status::PrOpen => "PR open".to_owned(),
        _ => super::dependencies::place_name(columns, ticket),
    };
    OwnerFacts {
        title: ticket.title.clone(),
        place: Some(place),
        line: card.one_line.line.map(|l| (l.text, l.dot)),
        avatar: card.avatar,
        changed_at: ticket.updated_at,
    }
}

/// "Dev · Working · back from Security": where it is, then its card's line.
fn status_words(facts: &OwnerFacts) -> Option<String> {
    let parts: Vec<&str> = [
        facts.place.as_deref(),
        facts.line.as_ref().map(|(t, _)| t.as_str()),
    ]
    .into_iter()
    .flatten()
    .collect();
    (!parts.is_empty()).then(|| parts.join(" \u{b7} "))
}

/// "12,400".
fn thousands(n: usize) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// "src/app/(storefront)/…/checkout/[step]/AddressField.tsx": a long path keeps its first three and last three parts.
pub fn short_path(path: &str) -> Option<String> {
    let parts: Vec<&str> = path.split('/').collect();
    (path.chars().count() > LONG_PATH && parts.len() > 6).then(|| {
        format!(
            "{}/\u{2026}/{}",
            parts[..3].join("/"),
            parts[parts.len() - 3..].join("/")
        )
    })
}

/// The last base line a range touches; an addition, the line it follows (line 1 for one before the first line).
fn range_last(r: LineRange) -> u32 {
    if r.lines == 0 {
        r.start.max(1)
    } else {
        r.start + r.lines - 1
    }
}

fn range_first(r: LineRange) -> u32 {
    r.start.max(1)
}

/// "lines 41–52", "line 7".
fn lines_label(first: u32, last: u32) -> String {
    if first == last {
        format!("line {first}")
    } else {
        format!("lines {first}\u{2013}{last}")
    }
}

/// The lines an owner touches on its base, first to last; for the file shown as a diff, the diff's new lines.
fn owner_lines(ranges: Option<&OwnerLines>, diff: Option<&FileDiff>) -> Option<String> {
    if let Some(ranges) = ranges.filter(|r| !r.ranges.is_empty()) {
        let first = ranges.ranges.iter().map(|r| range_first(*r)).min()?;
        let last = ranges.ranges.iter().map(|r| range_last(*r)).max()?;
        return Some(lines_label(first, last));
    }
    let hunks = &diff?.hunks;
    let first = hunks.iter().map(|h| h.new_start.max(1)).min()?;
    let last = hunks
        .iter()
        .map(|h| {
            (h.new_start + h.new_lines)
                .saturating_sub(1)
                .max(h.new_start)
        })
        .max()?;
    Some(lines_label(first, last))
}

/// How a bar sits beside one line: along it, or as a tick for lines a ticket adds after (or before) it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename = "FilesViewBar")]
pub enum Bar {
    None,
    Line,
    AddAfter,
    AddBefore,
}

/// One bar's column: a ticket on the project's base changing the file. `FilesViewBarColumn` on the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "FilesViewBarColumn")]
pub struct BarColumn {
    pub key: String,
    pub colour: &'static str,
}

/// A row of the file as it is on the base: a line with its bars, or the lines a huge file folds. `FilesViewRow` on
/// the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(rename = "FilesViewRow")]
pub enum FileRow {
    Line {
        number: u32,
        spans: Vec<SpanView>,
        /// One per bar column.
        bars: Vec<Bar>,
    },
    /// "⋯ 2,300 lines unchanged".
    Fold { label: String },
}

/// A ticket in Changed by (`s28-03`): its tool, key, title, "Dev · Working · back from Security", "otaq/SHOP-29 · from
/// main", "lines 41–52", "+7 −2" and Open on Files; the same facts name it in Diff's "of SHOP-29 ▾" and the narrow
/// line of chips. `FilesViewOwner` on the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "FilesViewOwner")]
pub struct OwnerCard {
    pub key: String,
    pub title: String,
    pub avatar: Option<ToolAvatarView>,
    pub colour: &'static str,
    pub status: Option<String>,
    /// The line's colour (the card's dot).
    pub palette: Option<Palette>,
    /// "otaq/SHOP-29 · from main".
    pub branch: String,
    /// "lines 41–52".
    pub lines: Option<String>,
    /// "+7 −2", "binary".
    pub counts: Option<String>,
    /// Diff's subtitle with this ticket's change: "SHOP-29’s diff against main · +7 −2".
    pub diff_subtitle: String,
    /// Diff's menu: "SHOP-29 · Coupon field" over "Dev · lines 41–52 · +7 −2".
    pub menu_label: String,
    pub menu_hint: String,
}

/// A file picked in the view (FIL-03, FIL-05, FIL-06, FIL-09; `s28-03`, `s28-05`, `s28-08`, `s28-09`, `s28-10`):
/// "src/lib/pricing.ts" over "on main · 2 tickets change it · read-only", File · Diff, New ticket from this file, the
/// same-file note, the file as it is on the base with a bar per ticket beside the lines it changes, and Changed by.
/// `FilesViewFile` on the page.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(rename = "FilesViewFile")]
pub struct FilePane {
    pub path: String,
    /// A long path cut in the middle; the full one on hover.
    pub short_path: Option<String>,
    /// A ticket deletes it: the path struck through.
    pub struck: bool,
    pub subtitle: String,
    /// At 1280 px or with the agent panel open, where the line of chips names the tickets: "on main · read-only".
    pub narrow_subtitle: String,
    pub modes: [&'static str; 2],
    /// File for a file on the base; Diff for one only in a ticket's changes.
    pub opens_in: FileMode,
    /// Off for a file that isn't on the base.
    pub can_file: bool,
    /// Some ticket changes it.
    pub can_diff: bool,
    pub new_ticket: &'static str,
    pub new_ticket_body: String,
    pub copy_path: &'static str,
    /// Same file (FIL-06): the notes as `files.project_read` words them, one per shared base.
    pub same_file: Vec<String>,
    /// A file only on another base: "SHOP-36 works on release/2.3, not main. …" (`s28-05`).
    pub other_base: Option<String>,
    /// "12,400 lines: only the parts tickets change are shown, with the lines between folded."
    pub huge: Option<String>,
    /// One column per ticket on the project's base, in Changed by's order; a ticket on another base has none (its
    /// lines are numbered on its own base, Q454).
    pub bars: Vec<BarColumn>,
    pub rows: Vec<FileRow>,
    /// "Showing the first 20000 lines: the file is too long to show in full."
    pub note: Option<String>,
    /// "Binary file · 182 KB on main, 164 KB in SHOP-26" with "No preview".
    pub binary: Option<String>,
    pub no_preview: Option<&'static str>,
    /// A file not on the base: the diff it previews as, and whose.
    pub diff: Option<diff_page::FileView>,
    pub diff_of: Option<String>,
    /// Changed by, the one that needs you first, then flow order.
    pub owners: Vec<OwnerCard>,
    pub changed_by: &'static str,
    pub changed_by_hint: &'static str,
    pub open_on_files: &'static str,
    pub open: &'static str,
    /// Diff's ticket at first: the one that needs you, else the last to change it.
    pub diff_default: Option<String>,
    pub of: &'static str,
    /// Diff's menu title: "Diff against main".
    pub diff_title: String,
}

/// Diff's ticket at first (FIL-05): the first that needs you, else the one that changed last (its ticket's last
/// change), else the first.
fn diff_default(owners: &[FileOwner], facts: &BTreeMap<String, OwnerFacts>) -> Option<String> {
    let changed_at = |o: &FileOwner| facts.get(&o.key.to_string()).map_or(0, |f| f.changed_at);
    owners
        .iter()
        .find(|o| o.needs_you)
        .or_else(|| owners.iter().rev().max_by_key(|o| changed_at(o)))
        .map(|o| o.key.to_string())
}

fn owner_card(
    owner: &FileOwner,
    colour: &'static str,
    facts: Option<&OwnerFacts>,
    lines: Option<String>,
) -> OwnerCard {
    let key = owner.key.to_string();
    let counts = if owner.file.binary {
        Some("binary".to_owned())
    } else {
        tab::counts(owner.file.added, owner.file.removed)
    };
    let mut diff_subtitle = format!("{key}\u{2019}s diff against {}", owner.base_branch);
    if let Some(c) = &counts {
        diff_subtitle.push_str(&format!(" \u{b7} {c}"));
    }
    let title = facts.map(|f| f.title.clone()).unwrap_or_default();
    let place = facts.and_then(|f| f.place.clone());
    let menu_hint: Vec<String> = [place, lines.clone(), counts.clone()]
        .into_iter()
        .flatten()
        .collect();
    OwnerCard {
        menu_label: if title.is_empty() {
            key.clone()
        } else {
            format!("{key} \u{b7} {title}")
        },
        menu_hint: menu_hint.join(" \u{b7} "),
        status: facts.and_then(status_words),
        palette: facts.and_then(|f| f.line.as_ref().map(|(_, p)| *p)),
        avatar: facts.and_then(|f| f.avatar.clone()),
        branch: format!("{} \u{b7} from {}", owner.branch, owner.base_branch),
        title,
        colour,
        lines,
        counts,
        diff_subtitle,
        key,
    }
}

/// "SHOP-26 changes it", "2 tickets change it".
fn who_changes(owners: &[FileOwner]) -> Option<String> {
    match owners {
        [] => None,
        [one] => Some(format!("{} changes it", one.key)),
        many => Some(format!("{} tickets change it", many.len())),
    }
}

/// "Binary file · 182 KB on main, 164 KB in SHOP-26".
fn binary_line(file: &ProjectFile) -> String {
    let mut sizes = vec![format!(
        "{} on {}",
        diff_vm::size_label(file.size),
        file.source.base_branch
    )];
    for o in &file.owners {
        match (o.file.status, o.file.size) {
            (FileStatus::Deleted, _) => sizes.push(format!("deleted in {}", o.key)),
            (_, Some(size)) => sizes.push(format!("{} in {}", diff_vm::size_label(size), o.key)),
            (_, None) => {}
        }
    }
    format!("Binary file \u{b7} {}", sizes.join(", "))
}

/// The base's lines with each column's bar; a huge file keeps only the lines around the changes, the rest folded.
fn file_rows(
    file: &ProjectFile,
    columns: &[(&FileOwner, &'static str)],
) -> (Vec<FileRow>, Option<String>) {
    let lines = tab::code_lines(&file.path, &file.text);
    let ranges: Vec<Vec<LineRange>> = columns
        .iter()
        .map(|(o, _)| {
            file.lines
                .iter()
                .find(|l| l.key == o.key)
                .map(|l| l.ranges.clone())
                .unwrap_or_default()
        })
        .collect();
    let bar = |ranges: &[LineRange], n: u32| -> Bar {
        if ranges
            .iter()
            .any(|r| r.lines > 0 && r.start <= n && n < r.start + r.lines)
        {
            Bar::Line
        } else if ranges.iter().any(|r| r.lines == 0 && r.start == n) {
            Bar::AddAfter
        } else if n == 1 && ranges.iter().any(|r| r.lines == 0 && r.start == 0) {
            Bar::AddBefore
        } else {
            Bar::None
        }
    };
    let every: Vec<LineRange> = ranges.iter().flatten().copied().collect();
    let huge = lines.len() > HUGE_LINES && !every.is_empty();
    let kept = |n: u32| {
        !huge
            || every.iter().any(|r| {
                let first = range_first(*r).saturating_sub(FOLD_CONTEXT);
                let last = range_last(*r) + u32::from(r.lines == 0) + FOLD_CONTEXT;
                first <= n && n <= last
            })
    };
    let mut rows = Vec::new();
    let mut folded = 0usize;
    let fold = |folded: &mut usize, rows: &mut Vec<FileRow>| {
        if *folded > 0 {
            rows.push(FileRow::Fold {
                label: format!(
                    "\u{22ef} {} {} unchanged",
                    thousands(*folded),
                    if *folded == 1 { "line" } else { "lines" }
                ),
            });
            *folded = 0;
        }
    };
    for line in lines.iter() {
        if !kept(line.number) {
            folded += 1;
            continue;
        }
        fold(&mut folded, &mut rows);
        rows.push(FileRow::Line {
            number: line.number,
            spans: line.spans.clone(),
            bars: ranges.iter().map(|r| bar(r, line.number)).collect(),
        });
    }
    fold(&mut folded, &mut rows);
    let note = huge.then(|| {
        let count = if file.truncated {
            format!("More than {}", thousands(lines.len()))
        } else {
            thousands(lines.len())
        };
        format!(
            "{count} lines: only the parts tickets change are shown, with the lines between folded."
        )
    });
    (rows, note)
}

/// A file read with `files.project_read`, with its owners' facts from their cards.
pub fn file_pane(file: &ProjectFile, facts: &BTreeMap<String, OwnerFacts>) -> FilePane {
    let base = &file.source.base_branch;
    let colours = owner_colours(file.owners.iter().map(|o| &o.key));
    let deleting = file
        .owners
        .iter()
        .find(|o| o.file.status == FileStatus::Deleted);
    let diff_of = file.diff_of.as_ref().map(ToString::to_string);
    let shown_as = file
        .diff_of
        .as_ref()
        .and_then(|k| file.owners.iter().find(|o| &o.key == k));
    let huge_lines = !file.binary && file.text.lines().count() > HUGE_LINES;
    // On the base: its lines with the bars of the tickets on the project's base.
    let columns: Vec<(&FileOwner, &'static str)> = file
        .owners
        .iter()
        .zip(colours.iter().copied())
        .filter(|(o, _)| !o.other_base)
        .collect();
    let (rows, huge) = if file.on_base && !file.binary {
        file_rows(file, &columns)
    } else {
        (Vec::new(), None)
    };
    let (subtitle, narrow_subtitle) = match (file.on_base, deleting, shown_as) {
        (false, _, owner) => {
            let line = match owner {
                Some(o) => format!(
                    "not on {base} \u{b7} only on {} \u{b7} {}\u{2019}s diff",
                    if o.other_base {
                        &o.base_branch
                    } else {
                        &o.branch
                    },
                    o.key
                ),
                None => format!("not on {base}"),
            };
            (line.clone(), line)
        }
        (true, Some(d), _) => {
            let line = format!("deleted by {} \u{b7} shown as it is on {base}", d.key);
            (line.clone(), line)
        }
        (true, None, _) => {
            let on = format!("on {base}");
            let huge = huge_lines.then_some("huge file");
            let wide: Vec<String> = [
                Some(on.clone()),
                huge.map(str::to_owned),
                who_changes(&file.owners),
            ]
            .into_iter()
            .flatten()
            .chain([READ_ONLY.to_owned()])
            .collect();
            let narrow: Vec<String> = [Some(on), huge.map(str::to_owned)]
                .into_iter()
                .flatten()
                .chain([READ_ONLY.to_owned()])
                .collect();
            (wide.join(" \u{b7} "), narrow.join(" \u{b7} "))
        }
    };
    let other_base = match shown_as {
        Some(o) if !file.on_base && o.other_base => Some(format!(
            "{key} works on {branch}, not {base}. Its files show here tagged with that branch. This one doesn\u{2019}t \
             exist on {base}, so it previews as {key}\u{2019}s diff. The same-file note only compares tickets on the \
             same base branch.",
            key = o.key,
            branch = o.base_branch,
        )),
        _ => None,
    };
    let owners = file
        .owners
        .iter()
        .zip(colours.iter().copied())
        .map(|(o, colour)| {
            let lines = owner_lines(
                file.lines.iter().find(|l| l.key == o.key),
                file.diff
                    .as_ref()
                    .filter(|_| shown_as.is_some_and(|s| s.key == o.key)),
            );
            owner_card(o, colour, facts.get(&o.key.to_string()), lines)
        })
        .collect();
    FilePane {
        path: file.path.clone(),
        short_path: short_path(&file.path),
        struck: deleting.is_some(),
        subtitle,
        narrow_subtitle,
        modes: tab::FILE_MODES,
        opens_in: if file.on_base {
            FileMode::File
        } else {
            FileMode::Diff
        },
        can_file: file.on_base,
        can_diff: !file.owners.is_empty(),
        new_ticket: tab::NEW_TICKET,
        new_ticket_body: tab::new_ticket_body(&file.path),
        copy_path: COPY_PATH,
        same_file: file.same_file.iter().map(|s| s.note.clone()).collect(),
        other_base,
        huge,
        bars: columns
            .iter()
            .map(|(o, colour)| BarColumn {
                key: o.key.to_string(),
                colour,
            })
            .collect(),
        rows,
        note: (file.on_base && !file.binary && file.truncated).then(|| {
            format!(
                "Showing the first {} lines: the file is too long to show in full.",
                file.text.lines().count()
            )
        }),
        binary: (file.on_base && file.binary).then(|| binary_line(file)),
        no_preview: (file.on_base && file.binary).then_some(NO_PREVIEW),
        diff: file.diff.as_ref().map(diff_page::file_view),
        diff_of,
        owners,
        changed_by: CHANGED_BY,
        changed_by_hint: CHANGED_BY_HINT,
        open_on_files: OPEN_ON_FILES,
        open: OPEN,
        diff_default: diff_default(&file.owners, facts),
        of: OF,
        diff_title: format!("Diff against {base}"),
    }
}

// ---- All changes ----

/// One ticket's group of All changes (FIL-08, `s28-06`): its tool, key, title, "Dev · Needs you: allow rm -rf .next?",
/// "3 files · +13 −5" and Open, then its files (the first group open, its first file unfolded). `FilesViewGroup` on
/// the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "FilesViewGroup")]
pub struct GroupView {
    pub key: String,
    pub title: String,
    pub avatar: Option<ToolAvatarView>,
    pub colour: &'static str,
    pub status: Option<String>,
    pub palette: Option<Palette>,
    /// "3 files · +13 −5", "2 files · on release/2.3 · +26 −2".
    pub summary: String,
    /// Open at first: the first group.
    pub open: bool,
    /// The file unfolded at first in an open group: its first.
    pub unfolded: Option<String>,
    /// The tab's rows, without line comments (d0017: the view has none).
    pub files: Vec<ChangeRow>,
}

/// All changes (FIL-08): Changed with no file picked. `FilesViewChanges` on the page.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(rename = "FilesViewChanges")]
pub struct ChangesPane {
    pub project: String,
    pub title: &'static str,
    /// "8 tickets · 11 files against main · grouped by ticket, the ones that need you first".
    pub summary: String,
    pub modes: [&'static str; 2],
    pub side_by_side: bool,
    pub side_by_side_from: f32,
    pub groups: Vec<GroupView>,
    pub empty: Option<EmptyView>,
    pub open: &'static str,
    pub open_file: &'static str,
    pub load_diff: &'static str,
}

pub fn changes_pane(
    changes: &ProjectChanges,
    facts: &BTreeMap<String, OwnerFacts>,
    memory: &ReviewMemory,
) -> ChangesPane {
    let c = &changes.counts;
    let groups = changes
        .groups
        .iter()
        .enumerate()
        .map(|(i, g)| {
            let t = &g.ticket;
            let f = facts.get(&t.key.to_string());
            let files: Vec<ChangeRow> = g
                .files
                .iter()
                .map(|e| ChangeRow {
                    commentable: false,
                    ..tab::change_row(e)
                })
                .collect();
            let summary: Vec<String> = [
                Some(tab::files_label(g.files.len())),
                t.other_base.then(|| format!("on {}", t.base_branch)),
                tab::counts(t.added, t.removed),
            ]
            .into_iter()
            .flatten()
            .collect();
            GroupView {
                key: t.key.to_string(),
                title: t.title.clone(),
                avatar: f.and_then(|f| f.avatar.clone()),
                colour: ticket_colour(&t.key),
                status: match f {
                    Some(f) => status_words(f),
                    None => t.stage.clone(),
                },
                palette: f.and_then(|f| f.line.as_ref().map(|(_, p)| *p)),
                summary: summary.join(" \u{b7} "),
                open: i == 0,
                unfolded: (i == 0)
                    .then(|| files.first().map(|r| r.id.clone()))
                    .flatten(),
                files,
            }
        })
        .collect();
    ChangesPane {
        project: changes.source.project.clone(),
        title: tab::ALL_CHANGES,
        summary: format!(
            "{} \u{b7} {} against {} \u{b7} grouped by ticket, the ones that need you first",
            plural(c.tickets, "ticket", "tickets"),
            plural(c.changed, "file", "files"),
            changes.source.base_branch
        ),
        modes: tab::MODES,
        side_by_side: memory.side_by_side,
        side_by_side_from: diff_vm::SIDE_BY_SIDE_FROM,
        groups,
        empty: changes.groups.is_empty().then(|| EmptyView {
            title: EMPTY_TITLE,
            text: EMPTY_TEXT.to_owned(),
        }),
        open: OPEN,
        open_file: tab::OPEN_FILE,
        load_diff: tab::LOAD_DIFF,
    }
}

// ---- The example and the gallery ----

/// The example's shop-web (page 28) as the view models read it: the tests' and the gallery's data.
pub mod example {
    use otaq_core::board::files::{ChangedEntry, FilesFrom};
    use otaq_core::board::project_files::ChangeGroup;
    use otaq_core::git::diff::{ChangedFile, DiffLine, Hunk, LineKind};

    use super::*;

    pub fn key(k: &str) -> TicketKey {
        k.parse().unwrap()
    }

    pub fn file(status: FileStatus, path: &str, added: u32, removed: u32) -> ChangedFile {
        ChangedFile {
            status,
            path: path.to_owned(),
            old_path: None,
            added,
            removed,
            binary: false,
            size: None,
            old_size: None,
            similarity: None,
        }
    }

    pub fn base(branch: &str) -> ProjectSource {
        ProjectSource {
            project: "SHOP".to_owned(),
            base_branch: branch.to_owned(),
            base_sha: "a1b2c3d4".to_owned(),
            base_short: "a1b2c3d".to_owned(),
            worktree: None,
        }
    }

    pub fn ticket(k: &str, title: &str, status: Status, stage: &str) -> OwnerTicket {
        OwnerTicket {
            key: key(k),
            title: title.to_owned(),
            status,
            stage: (!stage.is_empty()).then(|| stage.to_owned()),
            from: FilesFrom::Worktree,
            branch: otaq_core::naming::branch(k),
            base_branch: "main".to_owned(),
            other_base: false,
            needs_you: status.is_needs_you_family(),
            epic: None,
            files: 0,
            added: 0,
            removed: 0,
        }
    }

    pub fn group(mut t: OwnerTicket, files: Vec<ChangedFile>) -> ChangeGroup {
        t.files = u32::try_from(files.len()).unwrap();
        t.added = files.iter().map(|f| f.added).sum();
        t.removed = files.iter().map(|f| f.removed).sum();
        ChangeGroup {
            ticket: t,
            files: files
                .into_iter()
                .map(|file| ChangedEntry {
                    file,
                    uncommitted: false,
                    tags: Vec::new(),
                    child: None,
                })
                .collect(),
        }
    }

    /// The example's shop-web (page 28): SHOP-22 needs you (3 files, middleware uncommitted), SHOP-18 your check,
    /// SHOP-29 and SHOP-19 both on pricing.ts, SHOP-19 deleting legacy-price.ts, SHOP-36 on release/2.3.
    pub fn shop_web() -> ProjectChanges {
        let mut shop22 = group(
            ticket(
                "SHOP-22",
                "Fix flaky login redirect",
                Status::NeedsYou,
                "Dev",
            ),
            vec![
                file(FileStatus::Modified, "e2e/login.spec.ts", 6, 1),
                file(FileStatus::Modified, "src/app/login/page.tsx", 3, 3),
                file(FileStatus::Modified, "src/middleware.ts", 4, 1),
            ],
        );
        shop22.files[2].uncommitted = true;
        let mut shop18 = ticket("SHOP-18", "Checkout API", Status::WaitingForYou, "Review");
        shop18.needs_you = true;
        let mut shop36 = ticket("SHOP-36", "VAT rounding hotfix", Status::Working, "Dev");
        shop36.other_base = true;
        shop36.base_branch = "release/2.3".to_owned();
        ProjectChanges {
            source: base("main"),
            groups: vec![
                shop22,
                group(
                    shop18,
                    vec![
                        file(FileStatus::Modified, "prisma/schema.prisma", 1, 0),
                        file(
                            FileStatus::Modified,
                            "src/app/api/checkout/coupon/route.ts",
                            2,
                            1,
                        ),
                    ],
                ),
                group(
                    ticket("SHOP-29", "Coupon field", Status::Working, "Dev"),
                    vec![file(FileStatus::Modified, "src/lib/pricing.ts", 7, 2)],
                ),
                group(
                    ticket("SHOP-19", "Price table", Status::Checking, "Security"),
                    vec![
                        file(FileStatus::Deleted, "src/lib/legacy-price.ts", 0, 31),
                        file(FileStatus::Modified, "src/lib/pricing.ts", 5, 2),
                    ],
                ),
                group(
                    shop36,
                    vec![file(FileStatus::Modified, "src/lib/vat.ts", 3, 1)],
                ),
            ],
            counts: ProjectCounts {
                tickets: 5,
                changed: 8,
                needs_you: 5,
                needs_you_tickets: 2,
                same_file: 1,
            },
        }
    }

    /// The owners of `path` in [`shop_web`], in its groups' order.
    pub fn owners_of(path: &str) -> Vec<FileOwner> {
        by_path(&shop_web()).remove(path).unwrap_or_default()
    }

    /// What the cards say of the example's tickets: title, place, line and when each last changed.
    pub fn facts() -> BTreeMap<String, OwnerFacts> {
        let fact = |title: &str, place: &str, line: &str, palette, changed_at| OwnerFacts {
            title: title.to_owned(),
            place: Some(place.to_owned()),
            line: Some((line.to_owned(), palette)),
            avatar: None,
            changed_at,
        };
        [
            (
                "SHOP-22",
                fact(
                    "Fix flaky login redirect",
                    "Dev",
                    "Needs you: allow rm -rf .next?",
                    Palette::Attn,
                    50,
                ),
            ),
            (
                "SHOP-18",
                fact("Checkout API", "Review", "Your check", Palette::Review, 40),
            ),
            (
                "SHOP-29",
                fact(
                    "Coupon field",
                    "Dev",
                    "Working \u{b7} back from Security",
                    Palette::Working,
                    30,
                ),
            ),
            (
                "SHOP-19",
                fact(
                    "Price table",
                    "Security",
                    "Checking \u{b7} round 1 of 3",
                    Palette::Working,
                    20,
                ),
            ),
            (
                "SHOP-36",
                fact(
                    "VAT rounding hotfix",
                    "Dev",
                    "Working",
                    Palette::Working,
                    10,
                ),
            ),
        ]
        .into_iter()
        .map(|(k, f)| (k.to_owned(), f))
        .collect()
    }

    fn range(start: u32, lines: u32) -> LineRange {
        LineRange { start, lines }
    }

    /// pricing.ts on main (59 lines): SHOP-29 changes lines 41–52, SHOP-19 lines 47–58, both in applyDiscount.
    pub fn pricing() -> ProjectFile {
        let mut text: Vec<String> = vec![
            "// Prices for the cart and the checkout.".to_owned(),
            "import type { Coupon, Tier } from './types'".to_owned(),
            String::new(),
        ];
        text.extend((4..=37).map(|n| format!("export const RATE_{n} = {n}")));
        text.extend(
            [
                "import { Money } from './money'",
                "",
                "/** Applies a coupon, then the customer\u{2019}s tier, to a cart total. */",
                "export function applyDiscount(total: Money, coupon?: Coupon, tier?: Tier): Money {",
                "  if (!coupon) return applyTier(total, tier)",
                "  if (coupon.expiresAt && coupon.expiresAt < now()) {",
                "    throw new CouponExpired(coupon.code)",
                "  }",
                "  const off = coupon.kind === 'percent'",
                "    ? total.times(coupon.value / 100)",
                "    : Money.of(coupon.value, total.currency)",
                "  const capped = Money.min(off, total)",
                "  return applyTier(total.minus(capped), tier)",
                "}",
                "",
                "function applyTier(total: Money, tier?: Tier): Money {",
                "  if (!tier) return total",
                "  const rate = TIER_RATES[tier]",
                "  return total.times(1 - rate).round()",
                "}",
                "",
                "export const TIER_RATES = { silver: 0.05, gold: 0.1 } as const",
            ]
            .map(str::to_owned),
        );
        ProjectFile {
            source: base("main"),
            path: "src/lib/pricing.ts".to_owned(),
            on_base: true,
            size: 1_812,
            binary: false,
            text: text.join("\n"),
            truncated: false,
            owners: owners_of("src/lib/pricing.ts"),
            not_on_base: false,
            diff: None,
            diff_of: None,
            lines: vec![
                OwnerLines {
                    key: key("SHOP-29"),
                    ranges: vec![range(41, 12)],
                },
                OwnerLines {
                    key: key("SHOP-19"),
                    ranges: vec![range(47, 12)],
                },
            ],
            same_file: vec![otaq_core::board::project_files::SameFile {
                base_branch: "main".to_owned(),
                keys: vec![key("SHOP-19"), key("SHOP-29")],
                spans: vec![otaq_core::board::project_files::LineSpan {
                    first: 41,
                    last: 58,
                }],
                function: Some("applyDiscount".to_owned()),
                note: "SHOP-19 and SHOP-29 both edit applyDiscount (lines 41\u{2013}58). Whichever merges second \
                       will rebase on the other."
                    .to_owned(),
            }],
        }
    }

    fn diff_line(kind: LineKind, old: Option<u32>, new: Option<u32>, text: &str) -> DiffLine {
        DiffLine {
            kind,
            old,
            new,
            text: text.to_owned(),
            no_newline: false,
        }
    }

    /// SHOP-29's change of pricing.ts (`s28-03`'s Diff).
    pub fn pricing_diff() -> FileDiff {
        use LineKind::*;
        FileDiff {
            file: file(FileStatus::Modified, "src/lib/pricing.ts", 7, 2),
            hunks: vec![Hunk {
                old_start: 41,
                old_lines: 5,
                new_start: 41,
                new_lines: 11,
                header: "export function applyDiscount".to_owned(),
                lines: vec![
                    diff_line(
                        Context,
                        Some(41),
                        Some(41),
                        "export function applyDiscount(total: Money, coupon?: Coupon, tier?: Tier): Money {",
                    ),
                    diff_line(
                        Context,
                        Some(42),
                        Some(42),
                        "  if (!coupon) return applyTier(total, tier)",
                    ),
                    diff_line(
                        Added,
                        None,
                        Some(43),
                        "  if (coupon.expiresAt && coupon.expiresAt < now()) {",
                    ),
                    diff_line(
                        Added,
                        None,
                        Some(44),
                        "    throw new CouponExpired(coupon.code)",
                    ),
                    diff_line(Added, None, Some(45), "  }"),
                    diff_line(
                        Removed,
                        Some(43),
                        None,
                        "  const off = coupon.percent ? total.times(coupon.value / 100) : coupon.value",
                    ),
                    diff_line(
                        Added,
                        None,
                        Some(46),
                        "  const off = coupon.kind === 'percent'",
                    ),
                    diff_line(
                        Added,
                        None,
                        Some(47),
                        "    ? total.times(coupon.value / 100)",
                    ),
                    diff_line(
                        Added,
                        None,
                        Some(48),
                        "    : Money.of(coupon.value, total.currency)",
                    ),
                    diff_line(
                        Removed,
                        Some(44),
                        None,
                        "  return applyTier(total.minus(off), tier)",
                    ),
                    diff_line(
                        Added,
                        None,
                        Some(49),
                        "  const capped = Money.min(off, total)",
                    ),
                    diff_line(
                        Added,
                        None,
                        Some(50),
                        "  return applyTier(total.minus(capped), tier)",
                    ),
                    diff_line(Context, Some(45), Some(51), "}"),
                ],
            }],
            truncated: false,
        }
    }

    /// vat-table.ts, only on release/2.3 (`s28-05`): SHOP-36 adds it, so it previews as SHOP-36's diff.
    pub fn vat_table() -> ProjectFile {
        use LineKind::*;
        let mut owner = owners_of("src/lib/vat.ts").remove(0);
        owner.file = file(FileStatus::Added, "src/lib/vat-table.ts", 10, 0);
        let lines = [
            "// VAT rates as they were on release 2.3, kept for invoices issued then.",
            "export const VAT_TABLE_2_3 = {",
            "  DE: 0.19,",
            "  FR: 0.2,",
            "  NL: 0.21,",
            "} as const",
            "",
            "export function vatFor(country: keyof typeof VAT_TABLE_2_3) {",
            "  return VAT_TABLE_2_3[country]",
            "}",
        ];
        ProjectFile {
            source: base("main"),
            path: "src/lib/vat-table.ts".to_owned(),
            on_base: false,
            size: 0,
            binary: false,
            text: String::new(),
            truncated: false,
            owners: vec![owner],
            not_on_base: true,
            diff: Some(FileDiff {
                file: file(FileStatus::Added, "src/lib/vat-table.ts", 10, 0),
                hunks: vec![Hunk {
                    old_start: 0,
                    old_lines: 0,
                    new_start: 1,
                    new_lines: 10,
                    header: String::new(),
                    lines: lines
                        .iter()
                        .enumerate()
                        .map(|(i, l)| diff_line(Added, None, Some(i as u32 + 1), l))
                        .collect(),
                }],
                truncated: false,
            }),
            diff_of: Some(key("SHOP-36")),
            lines: Vec::new(),
            same_file: Vec::new(),
        }
    }

    /// legacy-price.ts, deleted by SHOP-19 (`s28-09`): shown as it is on main with SHOP-19's bar along it.
    pub fn legacy_price() -> ProjectFile {
        ProjectFile {
            source: base("main"),
            path: "src/lib/legacy-price.ts".to_owned(),
            on_base: true,
            size: 160,
            binary: false,
            text: "// Kept for orders placed before the tier rework.\nexport function legacyPrice(total: number, \
                   pct: number) {\n  return Math.round(total * (1 - pct / 100))\n}"
                .to_owned(),
            truncated: false,
            owners: owners_of("src/lib/legacy-price.ts"),
            not_on_base: false,
            diff: None,
            diff_of: None,
            lines: vec![OwnerLines {
                key: key("SHOP-19"),
                ranges: vec![range(1, 4)],
            }],
            same_file: Vec::new(),
        }
    }

    /// og-image.png, which SHOP-26 replaces (`s28-10`): sizes, no preview.
    pub fn og_image() -> ProjectFile {
        let mut owner = owners_of("src/lib/vat.ts").remove(0);
        owner.key = key("SHOP-26");
        owner.branch = otaq_core::naming::branch("SHOP-26");
        owner.base_branch = "main".to_owned();
        owner.other_base = false;
        owner.status = Status::PrOpen;
        owner.file = ChangedFile {
            binary: true,
            size: Some(164 * 1024),
            old_size: Some(182 * 1024),
            ..file(FileStatus::Modified, "public/og-image.png", 0, 0)
        };
        ProjectFile {
            source: base("main"),
            path: "public/og-image.png".to_owned(),
            on_base: true,
            size: 182 * 1024,
            binary: true,
            text: String::new(),
            truncated: false,
            owners: vec![owner],
            not_on_base: false,
            diff: None,
            diff_of: None,
            lines: Vec::new(),
            same_file: Vec::new(),
        }
    }

    /// schema.graphql, 12,400 lines, SHOP-18 adding after line 2301 and changing line 6418 (`s28-10`).
    pub fn schema() -> ProjectFile {
        let mut owner = owners_of("prisma/schema.prisma").remove(0);
        owner.file = file(FileStatus::Modified, "src/generated/schema.graphql", 9, 2);
        let text: Vec<String> = (1..=12_400)
            .map(|n| match n {
                2301 => "type Coupon {".to_owned(),
                2302 => "  code: String!".to_owned(),
                2303 => "}".to_owned(),
                6417 => "input ApplyCoupon {".to_owned(),
                6418 => "  code: String".to_owned(),
                6419 => "}".to_owned(),
                n => format!("# line {n}"),
            })
            .collect();
        ProjectFile {
            source: base("main"),
            path: "src/generated/schema.graphql".to_owned(),
            on_base: true,
            size: 300_000,
            binary: false,
            text: text.join("\n"),
            truncated: false,
            owners: vec![owner],
            not_on_base: false,
            diff: None,
            diff_of: None,
            lines: vec![OwnerLines {
                key: key("SHOP-18"),
                ranges: vec![range(2301, 0), range(6418, 1)],
            }],
            same_file: Vec::new(),
        }
    }

    /// A long path (`s28-10`): cut in the middle, the full path on hover.
    pub fn address_field() -> ProjectFile {
        let path = "src/app/(storefront)/[locale]/(shop)/checkout/[step]/AddressField.tsx";
        let mut owner = owners_of("src/middleware.ts").remove(0);
        owner.key = key("SHOP-32");
        owner.branch = otaq_core::naming::branch("SHOP-32");
        owner.needs_you = false;
        owner.status = Status::CheckFailed;
        owner.file = file(FileStatus::Modified, path, 1, 1);
        let text: Vec<String> = (1..=15)
            .map(|n| match n {
                12 => "export function AddressField({ step }: Props) {".to_owned(),
                13 => "  const q = useAddressQuery(step.country)".to_owned(),
                14 => "  // \u{2026}".to_owned(),
                15 => "}".to_owned(),
                n => format!("// {n}"),
            })
            .collect();
        ProjectFile {
            source: base("main"),
            path: path.to_owned(),
            on_base: true,
            size: 420,
            binary: false,
            text: text.join("\n"),
            truncated: false,
            owners: vec![owner],
            not_on_base: false,
            diff: None,
            diff_of: None,
            lines: vec![OwnerLines {
                key: key("SHOP-32"),
                ranges: vec![range(13, 1)],
            }],
            same_file: Vec::new(),
        }
    }

    pub fn tab_of(k: &str) -> TabView {
        TabView {
            key: k.to_owned(),
            source: tab::SourceView {
                kind: tab::SourceKind::Worktree,
                title: format!("{k}\u{2019}s worktree"),
                subtitle: format!("{} \u{b7} from main", otaq_core::naming::branch(k)),
                legend: Vec::new(),
            },
            search: SearchBox {
                placeholder: tab::SEARCH_PLACEHOLDER,
                keys: None,
            },
            filters: [tab::ALL_FILES.to_owned(), tab::changed_label(2)],
            changed: 2,
            opens_on: tab::Filter::All,
            rows: Vec::new(),
            pick: tab::PICK_A_FILE,
        }
    }
}

/// The gallery's Files view (`gallery.files_view`): shop-web on All with src/lib open and pricing.ts picked (its file,
/// its Diff of SHOP-29), Needs you, Changed with All changes, the loading and empty states, the picker, SHOP-29's
/// worktree, and the file states (another base, deleted, binary, huge, a long path). `FilesViewGallery` on the page.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(rename = "FilesViewGallery")]
pub struct Gallery {
    pub all: Page,
    /// The rows of All with src and src/lib open, as the page draws them.
    pub all_rows: Vec<TreeRow>,
    /// pricing.ts picked (`s28-03`).
    pub file: FilePane,
    /// Its Diff of SHOP-29.
    pub diff: diff_page::FileView,
    /// Changed with no file picked: All changes (`s28-06`).
    pub changed: Page,
    pub changes: ChangesPane,
    /// vat-table.ts, only on release/2.3 (`s28-05`).
    pub not_on_base: FilePane,
    /// `s28-09`, `s28-10`.
    pub deleted: FilePane,
    pub binary: FilePane,
    pub huge: FilePane,
    pub long_path: FilePane,
    pub needs_you: Page,
    pub loading: Page,
    /// The bare base's top rows while the tickets' changes load.
    pub loading_rows: Vec<TreeRow>,
    pub empty: Page,
    pub picker: Picker,
    pub picked: Picked,
}

pub fn gallery() -> Gallery {
    use example::*;
    let changes = shop_web();
    let owners = by_path(&changes);
    let entry = |name: &str, path: &str, kind, changed_below| ProjectEntry {
        name: name.to_owned(),
        path: path.to_owned(),
        kind,
        owners: owners.get(path).cloned().unwrap_or_default(),
        changed_below,
        not_on_base: false,
    };
    let top = entry_rows(
        &[
            entry("e2e", "e2e", EntryKind::Folder, 1),
            entry("prisma", "prisma", EntryKind::Folder, 1),
            entry("public", "public", EntryKind::Folder, 0),
            entry("src", "src", EntryKind::Folder, 6),
            entry("package.json", "package.json", EntryKind::File, 0),
            entry("README.md", "README.md", EntryKind::File, 0),
        ],
        0,
    );
    let src = entry_rows(
        &[
            entry("app", "src/app", EntryKind::Folder, 2),
            entry("lib", "src/lib", EntryKind::Folder, 3),
            entry("middleware.ts", "src/middleware.ts", EntryKind::File, 0),
        ],
        1,
    );
    let lib = entry_rows(
        &[
            entry(
                "legacy-price.ts",
                "src/lib/legacy-price.ts",
                EntryKind::File,
                0,
            ),
            entry("money.ts", "src/lib/money.ts", EntryKind::File, 0),
            entry("pricing.ts", "src/lib/pricing.ts", EntryKind::File, 0),
            entry("vat.ts", "src/lib/vat.ts", EntryKind::File, 0),
        ],
        2,
    );
    let bare = |e: &ProjectEntry| ProjectEntry {
        owners: Vec::new(),
        changed_below: 0,
        ..e.clone()
    };
    let loading_rows = entry_rows(
        &[
            bare(&entry("e2e", "e2e", EntryKind::Folder, 0)),
            bare(&entry("prisma", "prisma", EntryKind::Folder, 0)),
            bare(&entry("src", "src", EntryKind::Folder, 0)),
            bare(&entry("package.json", "package.json", EntryKind::File, 0)),
        ],
        0,
    );
    let mut all_rows = top[..3].to_vec();
    all_rows.push(top[3].clone());
    all_rows.push(src[0].clone());
    all_rows.push(src[1].clone());
    all_rows.extend(lib);
    all_rows.push(src[2].clone());
    all_rows.extend(top[4..].iter().cloned());
    let counts = changes.counts;
    let source = changes.source.clone();
    let keys = ViewKeys {
        go_to_file: Some("\u{2325}\u{2318}P".to_owned()),
        new_ticket: Some("\u{2318}N".to_owned()),
    };
    let facts = facts();
    let sources = ProjectSources {
        source: source.clone(),
        epics: vec![EpicSource {
            key: key("SHOP-40"),
            title: "Wishlist".to_owned(),
            status: Status::Working,
            branch: otaq_core::naming::branch("SHOP-40"),
            merged: vec![key("SHOP-40.1")],
            on_top: vec![key("SHOP-40.2")],
        }],
        tickets: changes.groups.iter().map(|g| g.ticket.clone()).collect(),
        no_worktree: vec![NoWorktree {
            key: key("SHOP-12"),
            title: String::new(),
            status: Status::Working,
            stage: Some("Grill".to_owned()),
        }],
        yours: vec![UserWorktree {
            path: "~/code/habitat-experiment".to_owned(),
            branch: Some("experiment/accent".to_owned()),
            head: "e4f5a6b7c8".to_owned(),
            head_short: "e4f5a6b".to_owned(),
        }],
    };
    Gallery {
        all: page(&source, Some(&counts), Filter::All, top, keys.clone(), true),
        all_rows,
        file: file_pane(&pricing(), &facts),
        diff: diff_page::file_view(&pricing_diff()),
        changed: page(
            &source,
            Some(&counts),
            Filter::Changed,
            changed_rows(&changes, Filter::Changed),
            keys.clone(),
            true,
        ),
        changes: changes_pane(&changes, &facts, &ReviewMemory::default()),
        not_on_base: file_pane(&vat_table(), &facts),
        deleted: file_pane(&legacy_price(), &facts),
        binary: file_pane(&og_image(), &facts),
        huge: file_pane(&schema(), &facts),
        long_path: file_pane(&address_field(), &facts),
        needs_you: page(
            &source,
            Some(&counts),
            Filter::NeedsYou,
            changed_rows(&changes, Filter::NeedsYou),
            keys.clone(),
            true,
        ),
        loading: page(&source, None, Filter::All, Vec::new(), keys.clone(), false),
        loading_rows,
        empty: page(
            &source,
            Some(&ProjectCounts::default()),
            Filter::All,
            Vec::new(),
            keys,
            false,
        ),
        picker: picker(&sources, "Dev"),
        picked: picked("main", tab_of("SHOP-29")),
    }
}

#[cfg(test)]
mod tests {
    use super::example::*;
    use super::*;

    fn texts(rows: &[TreeRow]) -> Vec<String> {
        rows.iter()
            .map(|r| {
                let mut parts = vec![format!("{}{}", "  ".repeat(r.depth as usize), r.name)];
                parts.extend(r.changed_below.clone());
                parts.extend(r.mark.as_ref().map(|m| m.letter.to_owned()));
                parts.extend(r.chips.iter().map(|c| c.key.clone()));
                parts.extend(r.branch.clone());
                parts.extend(r.mark.as_ref().and_then(|m| m.counts.clone()));
                parts.join(" ")
            })
            .collect()
    }

    /// `s28-03`'s source bar and filters; `s28-09`'s loading and empty ones; the Needs you tooltip.
    #[test]
    fn the_source_bar_and_the_filters_count() {
        let counts = ProjectCounts {
            tickets: 8,
            changed: 11,
            needs_you: 6,
            needs_you_tickets: 4,
            same_file: 1,
        };
        let bar = source_bar(&base("main"), Some(&counts));
        assert_eq!(bar.title, "main \u{b7} every ticket\u{2019}s changes");
        assert_eq!(bar.subtitle, "the base branch \u{b7} 8 tickets, 11 files");
        let shown: Vec<String> = filters(Some(&counts))
            .into_iter()
            .map(|f| f.label)
            .collect();
        assert_eq!(shown, ["All", "Changed 11", "Needs you 6", "Same file 1"]);
        let needs = &filters(Some(&counts))[2];
        assert_eq!(needs.tooltip.as_deref(), Some("Needs you \u{b7} 6 files"));
        assert_eq!(
            needs.detail.as_deref(),
            Some("changed by the 4 tickets that need you")
        );
        // Loading: the base at once, the counts as "…".
        let loading: Vec<String> = filters(None).into_iter().map(|f| f.label).collect();
        assert_eq!(
            loading,
            [
                "All",
                "Changed \u{2026}",
                "Needs you \u{2026}",
                "Same file \u{2026}"
            ]
        );
        let p = page(
            &base("main"),
            None,
            Filter::All,
            Vec::new(),
            ViewKeys::default(),
            false,
        );
        assert_eq!(
            p.loading,
            Some("Loading the tickets\u{2019} changes\u{2026}")
        );
        assert_eq!(p.empty, None);
        // Nothing changing.
        let none = ProjectCounts::default();
        assert_eq!(
            source_bar(&base("main"), Some(&none)).subtitle,
            "the base branch \u{b7} no ticket is changing files"
        );
        let p = page(
            &base("main"),
            Some(&none),
            Filter::All,
            Vec::new(),
            ViewKeys {
                go_to_file: Some("⌥⌘P".into()),
                new_ticket: None,
            },
            false,
        );
        let empty = p.empty.unwrap();
        assert_eq!(
            (empty.title, empty.text.as_str()),
            (
                "No ticket is changing files right now.",
                "Start one from the board, or ask the Project agent."
            )
        );
        assert_eq!(
            filters(Some(&none))
                .iter()
                .map(|f| f.label.as_str())
                .collect::<Vec<_>>(),
            ["All", "Changed 0", "Needs you 0", "Same file 0"]
        );
        assert_eq!(
            (p.search.placeholder, p.search.keys.as_deref()),
            ("Search files by name", Some("⌥⌘P"))
        );
    }

    /// billing-api's base is develop: every label says develop (FIL-02, `s28-08`).
    #[test]
    fn billing_api_says_develop() {
        let counts = ProjectCounts {
            tickets: 1,
            changed: 1,
            ..ProjectCounts::default()
        };
        let mut source = base("develop");
        source.project = "BILL".to_owned();
        let p = page(
            &source,
            Some(&counts),
            Filter::All,
            Vec::new(),
            ViewKeys::default(),
            false,
        );
        assert_eq!(p.base_branch, "develop");
        assert_eq!(
            p.source.title,
            "develop \u{b7} every ticket\u{2019}s changes"
        );
        assert_eq!(p.source.subtitle, "the base branch \u{b7} 1 ticket, 1 file");
        let sources = ProjectSources {
            source,
            epics: Vec::new(),
            tickets: Vec::new(),
            no_worktree: Vec::new(),
            yours: Vec::new(),
        };
        let pick = picker(&sources, "Dev");
        assert_eq!(
            pick.sections[0].rows[0].title,
            "develop \u{b7} every ticket\u{2019}s changes"
        );
        assert_eq!(pick.sections.len(), 1);
        assert_eq!(pick.no_worktree, None);
        let picked = picked("develop", tab_of("BILL-7"));
        assert_eq!(picked.back, "develop \u{b7} every ticket\u{2019}s changes");
    }

    /// The base's entries (`files.project_tree`): chips in status colours, marks, +/− for one owner only, folder
    /// counts, the other base's tag, a deleted file struck through, an unchanged file quieter.
    #[test]
    fn the_tree_has_chips_and_counts() {
        let changes = shop_web();
        let owners = by_path(&changes);
        let entry = |name: &str, path: &str, kind, changed_below| ProjectEntry {
            name: name.to_owned(),
            path: path.to_owned(),
            kind,
            owners: owners.get(path).cloned().unwrap_or_default(),
            changed_below,
            not_on_base: false,
        };
        let rows = entry_rows(
            &[
                entry(
                    "legacy-price.ts",
                    "src/lib/legacy-price.ts",
                    EntryKind::File,
                    0,
                ),
                entry("money.ts", "src/lib/money.ts", EntryKind::File, 0),
                entry("pricing.ts", "src/lib/pricing.ts", EntryKind::File, 0),
                entry("vat.ts", "src/lib/vat.ts", EntryKind::File, 0),
            ],
            2,
        );
        assert_eq!(
            texts(&rows),
            [
                "    legacy-price.ts D SHOP-19 \u{2212}31",
                "    money.ts",
                "    pricing.ts M SHOP-29 SHOP-19",
                "    vat.ts M SHOP-36 release/2.3 +3 \u{2212}1",
            ]
        );
        assert!(rows[0].struck && !rows[2].struck);
        assert!(rows[1].quiet && !rows[2].quiet);
        // Each ticket in its own colour, the colour of its bar (Q456).
        assert_eq!(
            rows[2].chips.iter().map(|c| c.colour).collect::<Vec<_>>(),
            owner_colours([&key("SHOP-29"), &key("SHOP-19")])
        );
        assert_ne!(rows[2].chips[0].colour, rows[2].chips[1].colour);
        let folder = entry_rows(&[entry("src", "src", EntryKind::Folder, 8)], 0);
        assert_eq!(texts(&folder), ["src 8 changed"]);
        assert_eq!(folder[0].mark, None);
        // SHOP-22's middleware is uncommitted.
        let mid = entry_rows(
            &[entry(
                "middleware.ts",
                "src/middleware.ts",
                EntryKind::File,
                0,
            )],
            1,
        );
        assert_eq!(mid[0].chips[0].colour, ticket_colour(&key("SHOP-22")));
        assert_eq!(
            mid[0].mark.as_ref().unwrap().uncommitted,
            Some("uncommitted")
        );
    }

    /// Changed, Needs you and Same file list their files as a tree, every folder open with its count.
    #[test]
    fn each_filter_lists_its_files() {
        let changes = shop_web();
        assert_eq!(
            texts(&changed_rows(&changes, Filter::Changed)),
            [
                "e2e 1 changed",
                "  login.spec.ts M SHOP-22 +6 \u{2212}1",
                "prisma 1 changed",
                "  schema.prisma M SHOP-18 +1",
                "src 6 changed",
                "  app 2 changed",
                "    api 1 changed",
                "      checkout 1 changed",
                "        coupon 1 changed",
                "          route.ts M SHOP-18 +2 \u{2212}1",
                "    login 1 changed",
                "      page.tsx M SHOP-22 +3 \u{2212}3",
                "  lib 3 changed",
                "    legacy-price.ts D SHOP-19 \u{2212}31",
                "    pricing.ts M SHOP-29 SHOP-19",
                "    vat.ts M SHOP-36 release/2.3 +3 \u{2212}1",
                "  middleware.ts M SHOP-22 +4 \u{2212}1",
            ]
        );
        assert_eq!(
            texts(&changed_rows(&changes, Filter::NeedsYou)),
            [
                "e2e 1 changed",
                "  login.spec.ts M SHOP-22 +6 \u{2212}1",
                "prisma 1 changed",
                "  schema.prisma M SHOP-18 +1",
                "src 3 changed",
                "  app 2 changed",
                "    api 1 changed",
                "      checkout 1 changed",
                "        coupon 1 changed",
                "          route.ts M SHOP-18 +2 \u{2212}1",
                "    login 1 changed",
                "      page.tsx M SHOP-22 +3 \u{2212}3",
                "  middleware.ts M SHOP-22 +4 \u{2212}1",
            ]
        );
        assert_eq!(
            texts(&changed_rows(&changes, Filter::SameFile)),
            [
                "src 1 changed",
                "  lib 1 changed",
                "    pricing.ts M SHOP-29 SHOP-19",
            ]
        );
        assert!(changed_rows(&changes, Filter::All).is_empty());
        // A ticket on another base never makes a Same file with one on the project's base.
        let mut apart = shop_web();
        apart.groups[4].files[0].file.path = "src/lib/pricing.ts".to_owned();
        assert_eq!(texts(&changed_rows(&apart, Filter::SameFile)).len(), 3);
    }

    /// The search's matches carry the owners' chips and the other base's tag.
    #[test]
    fn search_matches_have_chips() {
        let owners = by_path(&shop_web());
        let found = ProjectSearch {
            source: base("main"),
            query: "pri".to_owned(),
            matches: vec![otaq_core::board::project_files::ProjectMatch {
                path: "src/lib/pricing.ts".to_owned(),
                name: "pricing.ts".to_owned(),
                folder: "src/lib".to_owned(),
                hit: [8, 11],
                owners: owners["src/lib/pricing.ts"].clone(),
                not_on_base: false,
            }],
            total: 1,
        };
        let v = search_view("pri", &found);
        assert_eq!(v.rows[0].chips.len(), 2);
        assert_eq!(v.rows[0].name[0].text, "pri");
        assert!(v.rows[0].name[0].hit);
        assert_eq!(v.empty, None);
        let none = ProjectSearch {
            matches: Vec::new(),
            total: 0,
            ..found
        };
        assert_eq!(
            search_view("zzq", &none).empty.as_deref(),
            Some("No file name matches \u{201c}zzq\u{201d}")
        );
    }

    /// `s28-04`: BASE BRANCH, the epic with its children, the tickets in flow order with stage and files, your own
    /// worktrees (#43), the line naming the tickets with no worktree yet.
    #[test]
    fn the_picker_has_its_sections_in_order() {
        let mut shop36 = ticket("SHOP-36", "VAT rounding hotfix", Status::Working, "Dev");
        shop36.other_base = true;
        shop36.base_branch = "release/2.3".to_owned();
        let with_files = |mut t: OwnerTicket, n| {
            t.files = n;
            t
        };
        let sources = ProjectSources {
            source: base("main"),
            epics: vec![EpicSource {
                key: key("SHOP-40"),
                title: "Wishlist".to_owned(),
                status: Status::Working,
                branch: otaq_core::naming::branch("SHOP-40"),
                merged: vec![key("SHOP-40.1")],
                on_top: vec![key("SHOP-40.2")],
            }],
            tickets: vec![
                with_files(
                    ticket(
                        "SHOP-22",
                        "Fix flaky login redirect",
                        Status::NeedsYou,
                        "Dev",
                    ),
                    3,
                ),
                with_files(ticket("SHOP-29", "Coupon field", Status::Working, "Dev"), 2),
                with_files(
                    ticket("SHOP-19", "Price table", Status::Checking, "Security"),
                    2,
                ),
                with_files(ticket("SHOP-26", "Add sitemap", Status::PrOpen, "Ship"), 1),
                with_files(shop36, 2),
            ],
            no_worktree: vec![
                NoWorktree {
                    key: key("SHOP-12"),
                    title: String::new(),
                    status: Status::Working,
                    stage: Some("Grill".to_owned()),
                },
                NoWorktree {
                    key: key("SHOP-44"),
                    title: String::new(),
                    status: Status::Working,
                    stage: Some("Grill".to_owned()),
                },
                NoWorktree {
                    key: key("SHOP-35"),
                    title: String::new(),
                    status: Status::Queued,
                    stage: None,
                },
                NoWorktree {
                    key: key("SHOP-20"),
                    title: String::new(),
                    status: Status::Queued,
                    stage: None,
                },
            ]
            .into_iter()
            .chain((1..=3).map(|n| NoWorktree {
                key: key(&format!("SHOP-5{n}")),
                title: String::new(),
                status: Status::Backlog,
                stage: None,
            }))
            .collect(),
            yours: vec![
                UserWorktree {
                    path: "/work/habitat-experiment".to_owned(),
                    branch: Some("experiment/accent".to_owned()),
                    head: "e4f5a6b7c8".to_owned(),
                    head_short: "e4f5a6b".to_owned(),
                },
                UserWorktree {
                    path: "/work/bisect".to_owned(),
                    branch: None,
                    head: "0a1b2c3d4e".to_owned(),
                    head_short: "0a1b2c3".to_owned(),
                },
            ],
        };
        let p = picker(&sources, "Dev");
        assert_eq!(p.placeholder, "Find a ticket or epic");
        let shown: Vec<(&str, Vec<String>)> = p
            .sections
            .iter()
            .map(|s| {
                (
                    s.title,
                    s.rows
                        .iter()
                        .map(|r| {
                            [r.key.clone(), Some(r.title.clone()), r.detail.clone()]
                                .into_iter()
                                .flatten()
                                .collect::<Vec<_>>()
                                .join(" | ")
                        })
                        .collect(),
                )
            })
            .collect();
        assert_eq!(
            shown,
            [
                (
                    "BASE BRANCH",
                    vec!["main \u{b7} every ticket\u{2019}s changes | the default".to_owned()]
                ),
                (
                    "EPICS \u{b7} THE EPIC\u{2019}S BRANCH",
                    vec!["Wishlist \u{b7} SHOP-40 | 40.1 merged \u{b7} 40.2 on top".to_owned()]
                ),
                (
                    "TICKETS \u{b7} THEIR WORKTREE",
                    vec![
                        "SHOP-22 | Fix flaky login redirect | Dev \u{b7} 3 files".to_owned(),
                        "SHOP-29 | Coupon field | Dev \u{b7} 2 files".to_owned(),
                        "SHOP-19 | Price table | Security \u{b7} 2 files".to_owned(),
                        "SHOP-26 | Add sitemap | PR open \u{b7} 1 file".to_owned(),
                        "SHOP-36 | VAT rounding hotfix | Dev \u{b7} release/2.3 \u{b7} 2 files"
                            .to_owned(),
                    ]
                ),
                (
                    "YOUR WORKTREES",
                    vec![
                        "experiment/accent | /work/habitat-experiment \u{b7} read-only".to_owned(),
                        "0a1b2c3 | /work/bisect \u{b7} read-only".to_owned(),
                    ]
                ),
            ]
        );
        assert_eq!(
            p.sections[3].rows[0].pick,
            Pick::Worktree {
                path: "/work/habitat-experiment".to_owned()
            }
        );
        assert_eq!(
            p.no_worktree.as_deref(),
            Some(
                "No worktree yet: SHOP-12, SHOP-44 (Grill), SHOP-35, SHOP-20 (Ready) and 3 in Backlog. A ticket \
                 gets its worktree when it reaches Dev."
            )
        );
        assert_eq!(
            p.sections[2].rows[1].pick,
            Pick::Ticket {
                key: "SHOP-29".to_owned()
            }
        );
        assert_eq!(p.sections[0].rows[0].pick, Pick::Base);
        assert_eq!(
            no_worktree_line(&sources.no_worktree[4..], "Dev").as_deref(),
            Some("No worktree yet: 3 in Backlog. A ticket gets its worktree when it reaches Dev.")
        );
    }

    /// One of your worktrees picked (#43): the view's own frame on its tree, read-only, with no filters, counts or
    /// waiting, and a file of it reads "on experiment/accent · read-only" with nobody changing it.
    #[test]
    fn your_worktree_opens_read_only() {
        let source = ProjectSource {
            base_branch: "experiment/accent".to_owned(),
            worktree: Some("/work/habitat-experiment".to_owned()),
            ..base("main")
        };
        let p = page(
            &source,
            None,
            Filter::All,
            Vec::new(),
            ViewKeys::default(),
            true,
        );
        assert_eq!(p.source.title, "experiment/accent \u{b7} your worktree");
        assert_eq!(
            p.source.subtitle,
            "/work/habitat-experiment \u{b7} read-only"
        );
        assert!(p.filters.is_empty());
        assert_eq!((p.loading, p.empty, p.working), (None, None, false));
        assert_eq!(p.checking, "on experiment/accent \u{b7} read-only");
        let file = ProjectFile {
            source,
            path: "src/accent.css".to_owned(),
            on_base: true,
            size: 18,
            binary: false,
            text: ":root { --a: 1; }\n".to_owned(),
            truncated: false,
            owners: Vec::new(),
            not_on_base: false,
            diff: None,
            diff_of: None,
            lines: Vec::new(),
            same_file: Vec::new(),
        };
        let pane = file_pane(&file, &BTreeMap::new());
        assert_eq!(pane.subtitle, "on experiment/accent \u{b7} read-only");
        assert!(!pane.can_diff);
    }

    /// One ticket's worktree (`s28-04`): its tab's tree, read-only, the back link, Open SHOP-29 and the note.
    #[test]
    fn one_ticket_is_its_tab_with_the_back_link() {
        let p = picked("main", tab_of("SHOP-29"));
        assert_eq!(p.back, "main \u{b7} every ticket\u{2019}s changes");
        assert_eq!(p.tab.source.title, "SHOP-29\u{2019}s worktree");
        assert_eq!(
            p.tab.source.subtitle,
            format!(
                "{} \u{b7} from main \u{b7} read-only",
                otaq_core::naming::branch("SHOP-29")
            )
        );
        assert_eq!(p.tab.filters, ["All files", "Changed \u{b7} 2"]);
        assert_eq!(p.open, "Open SHOP-29");
        assert_eq!(
            p.note,
            "Only SHOP-29\u{2019}s changes are marked: the same tree as its Files tab. Open SHOP-29 opens its sheet \
             on that tab."
        );
    }

    /// FIL-10: the page asks again while an agent works, never for a ticket waiting on you or the PR.
    #[test]
    fn working_agents_keep_it_fresh() {
        assert!(agent_works(Status::Working));
        assert!(agent_works(Status::Checking));
        assert!(!agent_works(Status::NeedsYou));
        assert!(!agent_works(Status::WaitingForYou));
        assert!(!agent_works(Status::PrOpen));
    }

    /// The bars of a line, as `|` (along it), `^` (a tick after it), `v` (a tick before it) or `.`.
    fn bars_at(pane: &FilePane, number: u32) -> String {
        pane.rows
            .iter()
            .find_map(|r| match r {
                FileRow::Line {
                    number: n, bars, ..
                } if *n == number => Some(
                    bars.iter()
                        .map(|b| match b {
                            Bar::None => '.',
                            Bar::Line => '|',
                            Bar::AddAfter => '^',
                            Bar::AddBefore => 'v',
                        })
                        .collect(),
                ),
                _ => None,
            })
            .unwrap_or_else(|| panic!("no line {number}"))
    }

    fn row_texts(pane: &FilePane) -> Vec<String> {
        pane.rows
            .iter()
            .map(|r| match r {
                FileRow::Line { number, .. } => number.to_string(),
                FileRow::Fold { label } => label.clone(),
            })
            .collect()
    }

    /// `s28-03`: pricing.ts on main, a bar per ticket beside its lines, the same-file note word for word, Changed by
    /// with each ticket's line, branch, lines and +/−; the narrow subtitle (`s28-08`) and New ticket's body.
    #[test]
    fn a_file_has_a_bar_per_ticket_the_note_and_changed_by() {
        let p = file_pane(&pricing(), &facts());
        assert_eq!(p.path, "src/lib/pricing.ts");
        assert_eq!(
            p.subtitle,
            "on main \u{b7} 2 tickets change it \u{b7} read-only"
        );
        assert_eq!(p.narrow_subtitle, "on main \u{b7} read-only");
        assert_eq!(p.modes, ["File", "Diff"]);
        assert_eq!(p.opens_in, FileMode::File);
        assert!(p.can_file && p.can_diff && !p.struck);
        assert_eq!(
            p.same_file,
            [
                "SHOP-19 and SHOP-29 both edit applyDiscount (lines 41\u{2013}58). Whichever merges second will \
              rebase on the other."
            ]
        );
        assert_eq!(p.other_base, None);
        assert_eq!(p.huge, None);
        // One column per ticket, in Changed by's order, in the chips' colours.
        let colours = owner_colours([&key("SHOP-29"), &key("SHOP-19")]);
        assert_eq!(
            p.bars
                .iter()
                .map(|b| (b.key.as_str(), b.colour))
                .collect::<Vec<_>>(),
            [("SHOP-29", colours[0]), ("SHOP-19", colours[1])]
        );
        assert_eq!(bars_at(&p, 40), "..");
        assert_eq!(bars_at(&p, 41), "|.");
        assert_eq!(bars_at(&p, 47), "||");
        assert_eq!(bars_at(&p, 52), "||");
        assert_eq!(bars_at(&p, 53), ".|");
        assert_eq!(bars_at(&p, 59), "..");
        assert_eq!(p.rows.len(), 59, "a small file shows whole");
        // Changed by.
        assert_eq!(
            (p.changed_by, p.open_on_files),
            ("Changed by", "Open on Files")
        );
        assert_eq!(
            p.changed_by_hint,
            "Click a ticket to open its sheet on the Files tab. Diff shows one ticket\u{2019}s change at a time."
        );
        let card = |o: &OwnerCard| {
            [
                o.key.clone(),
                o.title.clone(),
                o.status.clone().unwrap_or_default(),
                o.branch.clone(),
                o.lines.clone().unwrap_or_default(),
                o.counts.clone().unwrap_or_default(),
            ]
            .join(" | ")
        };
        assert_eq!(
            p.owners.iter().map(card).collect::<Vec<_>>(),
            [
                format!(
                    "SHOP-29 | Coupon field | Dev \u{b7} Working \u{b7} back from Security | {} \u{b7} from main | \
                     lines 41\u{2013}52 | +7 \u{2212}2",
                    otaq_core::naming::branch("SHOP-29")
                ),
                format!(
                    "SHOP-19 | Price table | Security \u{b7} Checking \u{b7} round 1 of 3 | {} \u{b7} from main | \
                     lines 47\u{2013}58 | +5 \u{2212}2",
                    otaq_core::naming::branch("SHOP-19")
                ),
            ]
        );
        assert_eq!(p.owners[0].colour, colours[0]);
        assert_eq!(p.owners[0].palette, Some(Palette::Working));
        // New ticket from this file: the path in the description (FIL-09).
        assert_eq!(p.new_ticket, "New ticket from this file");
        assert_eq!(p.new_ticket_body, "`src/lib/pricing.ts`\n\n");
        assert_eq!(p.copy_path, "Copy path");
        assert_eq!(p.short_path, None);
    }

    /// FIL-05: Diff starts on the ticket that needs you, else the last to change it; "of SHOP-29 ▾" lists each with
    /// its stage, lines and +/−.
    #[test]
    fn diff_starts_on_the_one_that_needs_you_else_the_last_to_change_it() {
        let p = file_pane(&pricing(), &facts());
        assert_eq!(p.diff_default.as_deref(), Some("SHOP-29"), "changed last");
        assert_eq!((p.of, p.diff_title.as_str()), ("of", "Diff against main"));
        assert_eq!(
            p.owners
                .iter()
                .map(|o| (o.menu_label.as_str(), o.menu_hint.as_str()))
                .collect::<Vec<_>>(),
            [
                (
                    "SHOP-29 \u{b7} Coupon field",
                    "Dev \u{b7} lines 41\u{2013}52 \u{b7} +7 \u{2212}2"
                ),
                (
                    "SHOP-19 \u{b7} Price table",
                    "Security \u{b7} lines 47\u{2013}58 \u{b7} +5 \u{2212}2"
                ),
            ]
        );
        assert_eq!(
            p.owners[0].diff_subtitle,
            "SHOP-29\u{2019}s diff against main \u{b7} +7 \u{2212}2"
        );
        // SHOP-19 changed later: it starts there.
        let mut later = facts();
        later.get_mut("SHOP-19").unwrap().changed_at = 99;
        assert_eq!(
            file_pane(&pricing(), &later).diff_default.as_deref(),
            Some("SHOP-19")
        );
        // One needing you wins over the latest.
        let mut file = pricing();
        file.owners[1].needs_you = true;
        assert_eq!(
            file_pane(&file, &facts()).diff_default.as_deref(),
            Some("SHOP-19")
        );
        // The Diff's lines are SHOP-29's change, highlighted (diff_page).
        let diff = diff_page::file_view(&pricing_diff());
        assert_eq!(diff.hunks[0].lines.len(), 13);
    }

    /// `s28-05`: vat-table.ts exists only on release/2.3: "not on main", File off, SHOP-36's diff, the note; a ticket
    /// on another base never has a bar on main's lines (Q454).
    #[test]
    fn a_file_only_on_another_base_previews_as_its_diff() {
        let p = file_pane(&vat_table(), &facts());
        assert_eq!(
            p.subtitle,
            "not on main \u{b7} only on release/2.3 \u{b7} SHOP-36\u{2019}s diff"
        );
        assert_eq!(p.opens_in, FileMode::Diff);
        assert!(!p.can_file && p.can_diff);
        assert_eq!(p.diff_of.as_deref(), Some("SHOP-36"));
        assert_eq!(p.diff.as_ref().unwrap().hunks[0].lines.len(), 10);
        assert_eq!(
            p.other_base.as_deref(),
            Some(
                "SHOP-36 works on release/2.3, not main. Its files show here tagged with that branch. This one \
                 doesn\u{2019}t exist on main, so it previews as SHOP-36\u{2019}s diff. The same-file note only \
                 compares tickets on the same base branch."
            )
        );
        assert!(p.rows.is_empty() && p.bars.is_empty());
        assert_eq!(
            p.owners[0].branch,
            format!(
                "{} \u{b7} from release/2.3",
                otaq_core::naming::branch("SHOP-36")
            )
        );
        assert_eq!(p.owners[0].lines.as_deref(), Some("lines 1\u{2013}10"));
        assert_eq!(
            p.owners[0].diff_subtitle,
            "SHOP-36\u{2019}s diff against release/2.3 \u{b7} +10"
        );
        // On main with a ticket of another base too: only main's tickets get a bar.
        let mut file = pricing();
        let mut other = owners_of("src/lib/vat.ts").remove(0);
        other.file.path = "src/lib/pricing.ts".to_owned();
        file.owners.push(other);
        let p = file_pane(&file, &facts());
        assert_eq!(p.owners.len(), 3);
        assert_eq!(
            p.bars.iter().map(|b| b.key.as_str()).collect::<Vec<_>>(),
            ["SHOP-29", "SHOP-19"]
        );
        assert_eq!(bars_at(&p, 47), "||");
    }

    /// FIL-08, `s28-06`: grouped by ticket, the ones that need you first (SHOP-22), each with its line, files, +/−
    /// and Open; the first group open on its first file; no line comments.
    #[test]
    fn all_changes_are_grouped_by_ticket_the_ones_that_need_you_first() {
        let c = changes_pane(&shop_web(), &facts(), &ReviewMemory::default());
        assert_eq!(c.title, "All changes");
        assert_eq!(
            c.summary,
            "5 tickets \u{b7} 8 files against main \u{b7} grouped by ticket, the ones that need you first"
        );
        assert_eq!(c.modes, ["Unified", "Split"]);
        let groups: Vec<String> = c
            .groups
            .iter()
            .map(|g| {
                format!(
                    "{} {} \u{b7} {} \u{b7} {}",
                    g.key,
                    g.title,
                    g.status.clone().unwrap_or_default(),
                    g.summary
                )
            })
            .collect();
        assert_eq!(
            groups,
            [
                "SHOP-22 Fix flaky login redirect \u{b7} Dev \u{b7} Needs you: allow rm -rf .next? \u{b7} 3 files \
                 \u{b7} +13 \u{2212}5",
                "SHOP-18 Checkout API \u{b7} Review \u{b7} Your check \u{b7} 2 files \u{b7} +3 \u{2212}1",
                "SHOP-29 Coupon field \u{b7} Dev \u{b7} Working \u{b7} back from Security \u{b7} 1 file \u{b7} +7 \
                 \u{2212}2",
                "SHOP-19 Price table \u{b7} Security \u{b7} Checking \u{b7} round 1 of 3 \u{b7} 2 files \u{b7} +5 \
                 \u{2212}33",
                "SHOP-36 VAT rounding hotfix \u{b7} Dev \u{b7} Working \u{b7} 1 file \u{b7} on release/2.3 \u{b7} \
                 +3 \u{2212}1",
            ]
        );
        assert!(c.groups[0].open && !c.groups[1].open);
        assert_eq!(c.groups[0].unfolded.as_deref(), Some("e2e/login.spec.ts"));
        assert_eq!(c.groups[1].unfolded, None);
        assert!(
            c.groups
                .iter()
                .flat_map(|g| &g.files)
                .all(|f| !f.commentable)
        );
        assert_eq!(c.groups[0].files[2].mark.uncommitted, Some("uncommitted"));
        assert_eq!(c.groups[0].colour, ticket_colour(&key("SHOP-22")));
        assert!(
            !c.groups[3].files[0].can_open,
            "a deleted file has nothing to open"
        );
        assert_eq!((c.open, c.open_file), ("Open", "Open file"));
        assert_eq!(c.empty, None);
        let none = ProjectChanges {
            groups: Vec::new(),
            counts: ProjectCounts::default(),
            ..shop_web()
        };
        let empty = changes_pane(&none, &facts(), &ReviewMemory::default());
        assert_eq!(
            empty.empty.unwrap().text,
            "Start one from the board, or ask the Project agent."
        );
    }

    /// The view's page: the subtitle while a picked file is read (`s28-09`) and a file's menu (FIL-09, `s28-07`).
    #[test]
    fn a_files_menu_and_the_checking_line() {
        let p = page(
            &base("main"),
            None,
            Filter::All,
            Vec::new(),
            ViewKeys {
                go_to_file: None,
                new_ticket: Some("\u{2318}N".to_owned()),
            },
            false,
        );
        assert_eq!(
            p.checking,
            "on main \u{b7} checking which tickets change it\u{2026}"
        );
        assert_eq!(
            p.menu
                .iter()
                .map(|m| (m.id.as_str(), m.label.as_str(), m.shortcut.as_deref()))
                .collect::<Vec<_>>(),
            [
                ("new_ticket", "New ticket from this file", Some("\u{2318}N")),
                ("copy_path", "Copy path", None),
            ]
        );
    }

    /// `s28-09`, `s28-10`: deleted, binary, huge and long-path files.
    #[test]
    fn deleted_binary_huge_and_long_files() {
        let deleted = file_pane(&legacy_price(), &facts());
        assert!(deleted.struck);
        assert_eq!(
            deleted.subtitle,
            "deleted by SHOP-19 \u{b7} shown as it is on main"
        );
        assert_eq!(deleted.opens_in, FileMode::File);
        assert_eq!(row_texts(&deleted), ["1", "2", "3", "4"]);
        assert!((1..=4).all(|n| bars_at(&deleted, n) == "|"));

        let binary = file_pane(&og_image(), &facts());
        assert_eq!(
            binary.binary.as_deref(),
            Some("Binary file \u{b7} 182 KB on main, 164 KB in SHOP-26")
        );
        assert_eq!(binary.no_preview, Some("No preview"));
        assert!(binary.rows.is_empty());
        assert_eq!(
            binary.subtitle,
            "on main \u{b7} SHOP-26 changes it \u{b7} read-only"
        );
        assert_eq!(binary.owners[0].counts.as_deref(), Some("binary"));

        let huge = file_pane(&schema(), &facts());
        assert_eq!(
            huge.subtitle,
            "on main \u{b7} huge file \u{b7} SHOP-18 changes it \u{b7} read-only"
        );
        assert_eq!(
            huge.huge.as_deref(),
            Some(
                "12,400 lines: only the parts tickets change are shown, with the lines between folded."
            )
        );
        let rows = row_texts(&huge);
        assert_eq!(rows[0], "\u{22ef} 2,297 lines unchanged");
        assert_eq!(
            rows[1..9],
            [
                "2298", "2299", "2300", "2301", "2302", "2303", "2304", "2305"
            ]
        );
        assert_eq!(rows[9], "\u{22ef} 4,109 lines unchanged");
        assert_eq!(
            rows[10..17],
            ["6415", "6416", "6417", "6418", "6419", "6420", "6421"]
        );
        assert_eq!(rows[17], "\u{22ef} 5,979 lines unchanged");
        assert_eq!(rows.len(), 18);
        assert_eq!(
            bars_at(&huge, 2301),
            "^",
            "an addition after 2301 is a tick"
        );
        assert_eq!(bars_at(&huge, 2302), ".");
        assert_eq!(bars_at(&huge, 6418), "|");

        let long = file_pane(&address_field(), &facts());
        assert_eq!(
            long.short_path.as_deref(),
            Some("src/app/(storefront)/\u{2026}/checkout/[step]/AddressField.tsx")
        );
        assert_eq!(
            long.path,
            "src/app/(storefront)/[locale]/(shop)/checkout/[step]/AddressField.tsx"
        );
        assert_eq!(long.owners[0].lines.as_deref(), Some("line 13"));
        assert_eq!(short_path("src/lib/pricing.ts"), None);
        assert_eq!(thousands(999), "999");
        assert_eq!(thousands(1_000), "1,000");
    }

    /// A ticket's colour stays its own; two tickets of one file never share one.
    #[test]
    fn each_ticket_has_its_colour() {
        assert_eq!(
            ticket_colour(&key("SHOP-29")),
            ticket_colour(&key("SHOP-29"))
        );
        let same = key("SHOP-41"); // 41 and 29 fall on the same colour
        assert_eq!(ticket_colour(&same), ticket_colour(&key("SHOP-29")));
        let both = owner_colours([&key("SHOP-29"), &same]);
        assert_eq!(both[0], ticket_colour(&key("SHOP-29")));
        assert_ne!(both[0], both[1]);
    }

    /// billing-api on develop: the file's labels say develop.
    #[test]
    fn a_file_on_develop_says_develop() {
        let mut file = pricing();
        file.source = base("develop");
        for o in &mut file.owners {
            o.base_branch = "develop".to_owned();
        }
        let p = file_pane(&file, &facts());
        assert_eq!(
            p.subtitle,
            "on develop \u{b7} 2 tickets change it \u{b7} read-only"
        );
        assert_eq!(p.diff_title, "Diff against develop");
        assert!(p.owners[0].branch.ends_with("\u{b7} from develop"));
        assert_eq!(
            p.owners[0].diff_subtitle,
            "SHOP-29\u{2019}s diff against develop \u{b7} +7 \u{2212}2"
        );
    }
}
