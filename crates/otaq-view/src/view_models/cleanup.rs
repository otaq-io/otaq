//! The Cleanup view (REC-04; d0018; *16 → Cleanup · 3 items, Cleanup · 20 items, Cleanup · empty, Delete · confirm,
//! Success, Where Cleanup lives*): the service's leftovers grouped by kind with checkboxes and their size, path and
//! age. **Nothing is selected by default** (the user selects, d0018; Q24); Delete is disabled until something is;
//! the confirmation names every item and only Confirm calls `cleanup.delete`. [`CleanupModel::page`] is the overlay
//! as the Tauri page draws it (T-0235).
//!
//! From M18 (WRM-07, d0082) the kept **warm builds** are their own section under the leftovers' action bar
//! (*16 → Row · Cleanup with warm builds (0082)*): largest first, each with a Clear… and its own confirmation. They
//! aren't leftovers: not in "N items", never ticked or deleted by "Delete selected…", never on the rail's dot (that
//! counts `cleanup.scan`, which doesn't list them), and the section is hidden when there are none.

use std::collections::{BTreeMap, BTreeSet};

use otaq_core::recovery::cleanup::{CleanupGroup, CleanupItem, CleanupOutcome, CleanupTarget};
use otaq_core::warm::store::WarmBuildInfo;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::icons::IconName;
use crate::shell::tilde;
use crate::view_models::warm_build;

/// A group's heading and icon, for `n` rows (*Cleanup · 3 items*, *Cleanup · 20 items*).
pub fn group_title(group: CleanupGroup, n: usize) -> (&'static str, IconName) {
    match group {
        CleanupGroup::Sessions => ("Orphaned sessions", IconName::SquareTerminal),
        CleanupGroup::Worktrees => ("Worktrees", IconName::FolderGit2),
        CleanupGroup::Branches => ("Branches", IconName::GitBranch),
        CleanupGroup::Ports => ("Ports", IconName::Plug),
        CleanupGroup::KeptWorktrees if n == 1 => (
            "Done ticket with uncommitted changes",
            IconName::TriangleAlert,
        ),
        CleanupGroup::KeptWorktrees => (
            "Done tickets with uncommitted changes",
            IconName::TriangleAlert,
        ),
        CleanupGroup::CouldNotFree if n == 1 => {
            ("Review build that couldn’t be freed", IconName::FolderGit2)
        }
        CleanupGroup::CouldNotFree => {
            ("Review builds that couldn’t be freed", IconName::FolderGit2)
        }
        CleanupGroup::TicketResources if n == 1 => (
            "Ticket resource that couldn’t be removed",
            IconName::Database,
        ),
        CleanupGroup::TicketResources => (
            "Ticket resources that couldn’t be removed",
            IconName::Database,
        ),
    }
}

/// A group's name on the page and in its requests (`kept_worktrees`), as the service spells it.
pub fn group_id(group: CleanupGroup) -> &'static str {
    match group {
        CleanupGroup::Sessions => "sessions",
        CleanupGroup::Worktrees => "worktrees",
        CleanupGroup::Branches => "branches",
        CleanupGroup::Ports => "ports",
        CleanupGroup::KeptWorktrees => "kept_worktrees",
        CleanupGroup::CouldNotFree => "could_not_free",
        CleanupGroup::TicketResources => "ticket_resources",
    }
}

/// Longer lists start with their groups folded (*Cleanup · 20 items*: "Large lists collapse per group").
pub const FOLD_OVER: usize = 12;

/// What deleting an item does, for the confirmation ("(kill)", "(remove worktree)").
pub fn deletion(group: CleanupGroup) -> &'static str {
    match group {
        CleanupGroup::Sessions => "kill",
        CleanupGroup::Worktrees => "remove worktree",
        CleanupGroup::Branches => "delete branch",
        CleanupGroup::Ports => "free ports",
        CleanupGroup::KeptWorktrees => "remove worktree · its uncommitted changes are lost",
        // Not a deletion: the user's retry of the Free (M19-c).
        CleanupGroup::CouldNotFree => "free build · the worktree stays",
        // The user's retry of the project's Remove command (M19-d).
        CleanupGroup::TicketResources => "run Remove again",
    }
}

/// "412 MB", "1.2 GB", "880 KB".
pub fn size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;
    if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{} MB", bytes / MB)
    } else {
        format!("{} KB", bytes.div_ceil(KB))
    }
}

/// "9 d", "3 h", "5 min".
pub fn age(ms: i64) -> String {
    let minutes = ms.max(0) / 60_000;
    match minutes {
        m if m < 60 => format!("{m} min"),
        m if m < 24 * 60 => format!("{} h", m / 60),
        m => format!("{} d", m / (24 * 60)),
    }
}

/// One row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CleanupRow {
    pub target: CleanupTarget,
    /// The path with `~`, or the label.
    pub title: String,
    /// Why it is listed (and the uncommitted files of a kept worktree).
    pub detail: String,
    pub size: String,
    pub age: String,
    pub checked: bool,
    /// Deleting it loses uncommitted work.
    pub warning: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CleanupGroupView {
    pub group: CleanupGroup,
    pub title: &'static str,
    pub icon: IconName,
    pub rows: Vec<CleanupRow>,
    /// Every row of the group is checked.
    pub all_checked: bool,
}

/// The confirmation (*Delete · confirm*).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeleteConfirm {
    pub title: String,
    pub text: String,
    /// One line per item: "~/.otaq/wt/shop-web/SHOP-14 · 412 MB (remove worktree)".
    pub lines: Vec<String>,
    /// Each line's icon: its group's.
    pub icons: Vec<IconName>,
    pub confirm: String,
}

#[derive(Debug, Clone, Default)]
pub struct CleanupModel {
    items: Vec<CleanupItem>,
    selected: BTreeSet<(CleanupGroup, String)>,
    warm: WarmBuilds,
}

/// The warm builds as `warm.list` gave them, the project names they are shown by, and where focus goes once a
/// cleared row is gone.
#[derive(Debug, Clone, Default)]
struct WarmBuilds {
    builds: Vec<WarmBuildInfo>,
    /// Project key → name.
    names: BTreeMap<String, String>,
    now: i64,
    /// The row the user cleared and its place, until it is gone from the list.
    cleared: Option<(String, usize)>,
    focus: Option<WarmFocusPage>,
    focus_seq: u32,
}

fn key(t: &CleanupTarget) -> (CleanupGroup, String) {
    (t.group, t.id.clone())
}

impl CleanupModel {
    /// A new scan; the selection keeps only items still listed.
    pub fn set_items(&mut self, items: Vec<CleanupItem>) {
        self.selected
            .retain(|(g, id)| items.iter().any(|i| i.group == *g && &i.id == id));
        self.items = items;
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn toggle(&mut self, target: &CleanupTarget) {
        let k = key(target);
        if !self.selected.remove(&k) {
            self.selected.insert(k);
        }
    }

    /// The group's checkbox: all on, or all off when every row was on.
    pub fn toggle_group(&mut self, group: CleanupGroup) {
        let rows: Vec<(CleanupGroup, String)> = self
            .items
            .iter()
            .filter(|i| i.group == group)
            .map(|i| key(&i.target()))
            .collect();
        if rows.iter().all(|k| self.selected.contains(k)) {
            for k in &rows {
                self.selected.remove(k);
            }
        } else {
            self.selected.extend(rows);
        }
    }

    pub fn selected(&self) -> Vec<CleanupTarget> {
        self.items
            .iter()
            .filter(|i| self.selected.contains(&key(&i.target())))
            .map(CleanupItem::target)
            .collect()
    }

    fn selected_items(&self) -> Vec<&CleanupItem> {
        self.items
            .iter()
            .filter(|i| self.selected.contains(&key(&i.target())))
            .collect()
    }

    /// The size of what is selected.
    pub fn selected_bytes(&self) -> u64 {
        self.selected_items()
            .iter()
            .filter_map(|i| i.size_bytes)
            .sum()
    }

    /// "2 selected · 412 MB"; `None` while nothing is selected (Delete is disabled then).
    pub fn footer(&self) -> Option<String> {
        let chosen = self.selected_items();
        if chosen.is_empty() {
            return None;
        }
        let bytes = self.selected_bytes();
        Some(if bytes > 0 {
            format!("{} selected · {}", chosen.len(), size(bytes))
        } else {
            format!("{} selected", chosen.len())
        })
    }

    pub fn groups(&self) -> Vec<CleanupGroupView> {
        let mut out: Vec<CleanupGroupView> = Vec::new();
        for item in &self.items {
            let checked = self.selected.contains(&key(&item.target()));
            let title = match (&item.path, item.group) {
                (
                    Some(path),
                    CleanupGroup::Worktrees
                    | CleanupGroup::KeptWorktrees
                    | CleanupGroup::CouldNotFree,
                ) => tilde(path),
                _ => item.label.clone(),
            };
            let detail = if item.uncommitted.is_empty() {
                item.why.clone()
            } else {
                let n = item.uncommitted.len();
                let files = if n == 1 { "file" } else { "files" };
                format!(
                    "{} · {n} uncommitted {files}: {}",
                    item.why,
                    item.uncommitted.join(", ")
                )
            };
            let row = CleanupRow {
                target: item.target(),
                title,
                detail,
                size: item.size_bytes.map(size).unwrap_or_else(|| "—".into()),
                age: item.age_ms.map(age).unwrap_or_else(|| "—".into()),
                checked,
                warning: item.group == CleanupGroup::KeptWorktrees,
            };
            match out.iter_mut().find(|g| g.group == item.group) {
                Some(g) => g.rows.push(row),
                None => {
                    let n = self.items.iter().filter(|i| i.group == item.group).count();
                    let (title, icon) = group_title(item.group, n);
                    out.push(CleanupGroupView {
                        group: item.group,
                        title,
                        icon,
                        rows: vec![row],
                        all_checked: false,
                    });
                }
            }
        }
        for g in &mut out {
            g.all_checked = g.rows.iter().all(|r| r.checked);
        }
        out
    }

    /// The confirmation, naming every selected item; `None` while nothing is selected.
    pub fn confirm(&self) -> Option<DeleteConfirm> {
        let chosen = self.selected_items();
        if chosen.is_empty() {
            return None;
        }
        let n = chosen.len();
        let items = if n == 1 { "item" } else { "items" };
        Some(DeleteConfirm {
            title: format!("Delete {n} {items}?"),
            text: format!(
                "This can’t be undone. Nothing outside {} and {}’s own branches is touched.",
                tilde(&crate::view_models::cleanup::data_folder()),
                otaq_core::naming::PRODUCT_NAME
            ),
            lines: chosen
                .iter()
                .map(|i| {
                    let what = match (&i.path, i.group) {
                        (
                            Some(p),
                            CleanupGroup::Worktrees
                            | CleanupGroup::KeptWorktrees
                            | CleanupGroup::CouldNotFree,
                        ) => tilde(p),
                        _ => i.label.clone(),
                    };
                    match i.size_bytes {
                        Some(b) => format!("{what} · {} ({})", size(b), deletion(i.group)),
                        None => format!("{what} ({})", deletion(i.group)),
                    }
                })
                .collect(),
            icons: chosen.iter().map(|i| group_title(i.group, 1).1).collect(),
            confirm: format!("Delete {n} {items}"),
        })
    }
}

/// The data folder, as the confirmation names it.
pub fn data_folder() -> String {
    otaq_core::data_dir::DataDir::from_env()
        .map(|d| d.root().display().to_string())
        .unwrap_or_else(|_| format!("~/{}", otaq_core::naming::data_dir_name()))
}

/// The success toast (*Success*): "Cleaned up 2 items" · "Freed 412 MB · 1 item left in Cleanup".
pub fn success(outcome: &CleanupOutcome, freed: u64, left: usize) -> (String, Option<String>) {
    let n = outcome.deleted.len();
    let items = if n == 1 { "item" } else { "items" };
    let title = format!("{CLEANED_UP}{n} {items}");
    let mut parts = Vec::new();
    if freed > 0 {
        parts.push(format!("Freed {}", size(freed)));
    }
    if !outcome.failed.is_empty() {
        parts.push(format!("{} couldn’t be deleted", outcome.failed.len()));
    }
    if left > 0 {
        let things = if left == 1 { "item" } else { "items" };
        parts.push(format!("{left} {things} left in Cleanup"));
    }
    (title, (!parts.is_empty()).then(|| parts.join(" · ")))
}

/// The success toast's title starts so (the page gives it the broom icon).
pub const CLEANED_UP: &str = "Cleaned up ";

/// An item as the page names it in `recovery.cleanup_toggle`: `{"group": "worktrees", "id": "/…/SHOP-14"}`, the
/// service's own [`CleanupTarget`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(rename = "CleanupTarget")]
pub struct TargetPage {
    pub group: String,
    pub id: String,
}

impl TargetPage {
    fn of(target: &CleanupTarget) -> Self {
        Self {
            group: group_id(target.group).to_owned(),
            id: target.id.clone(),
        }
    }

    /// The service's target, or `None` for a group it doesn't know.
    pub fn target(&self) -> Option<CleanupTarget> {
        serde_json::from_value(serde_json::json!({ "group": self.group, "id": self.id })).ok()
    }
}

/// One row of the overlay. `CleanupRowPage` on the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "CleanupRowPage")]
pub struct RowPage {
    pub target: TargetPage,
    pub title: String,
    pub detail: String,
    pub size: String,
    pub age: String,
    pub checked: bool,
    /// Deleting it loses uncommitted work: the detail in the paused colour.
    pub warning: bool,
}

/// One group: its heading (fold arrow, checkbox, icon, title, count, "select all" while folded) and rows.
/// `CleanupGroupPage` on the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "CleanupGroupPage")]
pub struct GroupPage {
    pub group: String,
    pub title: &'static str,
    pub icon: IconName,
    pub count: String,
    pub all_checked: bool,
    /// The page folds it when the overlay opens; the arrow unfolds it.
    pub starts_folded: bool,
    pub select_all: &'static str,
    pub rows: Vec<RowPage>,
}

/// One line of the confirmation, with its group's icon.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "CleanupConfirmLine")]
pub struct ConfirmLine {
    pub icon: IconName,
    pub text: String,
}

/// *Delete · confirm*. `CleanupConfirm` on the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "CleanupConfirm")]
pub struct ConfirmPage {
    pub title: String,
    pub text: String,
    pub lines: Vec<ConfirmLine>,
    pub confirm: String,
}

/// *Cleanup · empty*.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "CleanupEmpty")]
pub struct EmptyPage {
    pub title: &'static str,
    pub text: &'static str,
}

/// The Cleanup overlay (*16 → Cleanup · 3 items, Cleanup · 20 items, Cleanup · empty, Delete · confirm*).
/// `CleanupPage` on the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "CleanupPage")]
pub struct Page {
    pub title: &'static str,
    /// "3 items".
    pub count: String,
    pub text: &'static str,
    pub groups: Vec<GroupPage>,
    pub empty: Option<EmptyPage>,
    /// "2 selected · 412 MB"; `None` while nothing is selected.
    pub footer: Option<String>,
    /// "Delete selected…", disabled while nothing is selected.
    pub delete: &'static str,
    /// The confirmation for what is selected; the page shows it after Delete selected….
    pub confirm: Option<ConfirmPage>,
    /// The warm builds' section under the action bar; `None` (not drawn) when there are none (WRM-07).
    pub warm: Option<WarmSectionPage>,
}

fn items_text(n: usize) -> String {
    if n == 1 {
        "1 item".to_owned()
    } else {
        format!("{n} items")
    }
}

impl CleanupModel {
    /// The overlay as the page draws it.
    pub fn page(&self) -> Page {
        let folded = self.len() > FOLD_OVER;
        Page {
            title: "Cleanup",
            count: items_text(self.len()),
            text: "Copies, branches and processes no ticket uses any more. Nothing is selected until you choose.",
            groups: self
                .groups()
                .into_iter()
                .map(|g| GroupPage {
                    group: group_id(g.group).to_owned(),
                    title: g.title,
                    icon: g.icon,
                    count: g.rows.len().to_string(),
                    all_checked: g.all_checked,
                    starts_folded: folded,
                    select_all: "select all",
                    rows: g
                        .rows
                        .into_iter()
                        .map(|r| RowPage {
                            target: TargetPage::of(&r.target),
                            title: r.title,
                            detail: r.detail,
                            size: r.size,
                            age: r.age,
                            checked: r.checked,
                            warning: r.warning,
                        })
                        .collect(),
                })
                .collect(),
            empty: self.is_empty().then_some(EmptyPage {
                title: "Nothing to clean up",
                text: if self.warm.builds.is_empty() {
                    "Every session, worktree, branch and port belongs to a ticket."
                } else {
                    "Every session, worktree, branch and port belongs to a ticket. Warm builds aren’t leftovers."
                },
            }),
            footer: self.footer(),
            delete: "Delete selected…",
            confirm: self.confirm().map(|c| ConfirmPage {
                title: c.title,
                text: c.text,
                lines: c
                    .icons
                    .into_iter()
                    .zip(c.lines)
                    .map(|(icon, text)| ConfirmLine { icon, text })
                    .collect(),
                confirm: c.confirm,
            }),
            warm: self.warm_section(),
        }
    }
}

/// The warm builds' section (*Cleanup · leftovers + warm builds (0082)*). `CleanupWarmSection` on the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "CleanupWarmSection")]
pub struct WarmSectionPage {
    pub title: &'static str,
    pub icon: IconName,
    /// How many: "3".
    pub count: String,
    /// "41.6 GB on disk": the full sizes added up (the "up to" is only in the confirmation).
    pub total: String,
    pub note: &'static str,
    /// Largest first.
    pub rows: Vec<WarmRowPage>,
    /// After a Clear, once its row is gone: the Clear… that takes focus (*Cleanup · right after Clear (0082)*).
    pub focus: Option<WarmFocusPage>,
}

/// One warm build. `CleanupWarmRow` on the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "CleanupWarmRow")]
pub struct WarmRowPage {
    /// Its folder in `warm/`, which names it in `recovery.warm_clear`.
    pub repo: String,
    /// The project's name, or the folder's when no project owns it.
    pub name: String,
    /// "~/.otaq/warm/shop-web".
    pub path: String,
    /// "built at 3e1f9a2"; `None` without a record.
    pub built_at: Option<String>,
    /// "Clearing… (waiting for SHOP-21’s copy)" while a Clear waits; in place of the built-at line, with a spinner.
    pub clearing: Option<String>,
    pub size: String,
    pub age: String,
    /// "Clear…", or "Clearing…" (disabled) while a Clear waits.
    pub clear: String,
    pub clear_enabled: bool,
    /// What Clear… asks.
    pub confirm: WarmConfirmPage,
}

/// *Warm build · Clear · confirm (0082)*. `CleanupWarmConfirm` on the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "CleanupWarmConfirm")]
pub struct WarmConfirmPage {
    /// "Clear otaq’s warm build (up to 36.1 GB)?"
    pub title: String,
    pub text: String,
    /// "Tickets copied from it share part of this space, so it may free less."
    pub shared: &'static str,
    /// "~/.otaq/warm/otaq · up to 36.1 GB (delete)".
    pub line: ConfirmLine,
    /// "Clear up to 36.1 GB".
    pub confirm: String,
}

/// The row whose Clear… takes focus; `seq` changes on every move, so the same row can take it twice.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "CleanupWarmFocus")]
pub struct WarmFocusPage {
    pub repo: String,
    pub seq: u32,
}

impl CleanupModel {
    /// A new `warm.list`, with the projects' names by key and the time now (for the ages). Once the row the user
    /// cleared is gone, focus goes to the row now in its place (the next one), else the last.
    pub fn set_warm(
        &mut self,
        builds: Vec<WarmBuildInfo>,
        names: BTreeMap<String, String>,
        now: i64,
    ) {
        let warm = &mut self.warm;
        warm.builds = builds;
        warm.names = names;
        warm.now = now;
        warm.builds.sort_by(|a, b| {
            b.size_bytes
                .cmp(&a.size_bytes)
                .then_with(|| a.repo.cmp(&b.repo))
        });
        warm.focus = None;
        if let Some((repo, at)) = warm.cleared.clone() {
            match warm.builds.iter().find(|b| b.repo == repo) {
                // Still waiting for a copy: focus moves once it goes.
                Some(b) if b.status.clearing => {}
                // The Clear didn't go through.
                Some(_) => warm.cleared = None,
                None => {
                    warm.cleared = None;
                    let next = warm.builds.get(at).or(warm.builds.last());
                    if let Some(next) = next {
                        warm.focus_seq = warm.focus_seq.wrapping_add(1);
                        warm.focus = Some(WarmFocusPage {
                            repo: next.repo.clone(),
                            seq: warm.focus_seq,
                        });
                    }
                }
            }
        }
    }

    /// The user confirmed Clear… on `repo`'s row.
    pub fn clearing(&mut self, repo: &str) {
        if let Some(at) = self.warm.builds.iter().position(|b| b.repo == repo) {
            self.warm.cleared = Some((repo.to_owned(), at));
        }
    }

    /// A listed warm build, by its folder.
    pub fn warm_build(&self, repo: &str) -> Option<&WarmBuildInfo> {
        self.warm.builds.iter().find(|b| b.repo == repo)
    }

    /// The name a warm build is shown by: its project's, or its folder's when no project owns it.
    pub fn warm_name(&self, build: &WarmBuildInfo) -> String {
        build
            .project
            .as_ref()
            .and_then(|key| self.warm.names.get(key))
            .cloned()
            .unwrap_or_else(|| build.repo.clone())
    }

    fn warm_section(&self) -> Option<WarmSectionPage> {
        let builds = &self.warm.builds;
        if builds.is_empty() {
            return None;
        }
        let total: u64 = builds.iter().map(|b| b.size_bytes).sum();
        Some(WarmSectionPage {
            title: "Warm builds",
            icon: IconName::Flame,
            count: builds.len().to_string(),
            total: format!("{} on disk", size(total)),
            note: "Kept to speed up new tickets. It grows over time; clear it to reclaim disk.",
            rows: builds.iter().map(|b| self.warm_row(b)).collect(),
            focus: self.warm.focus.clone(),
        })
    }

    fn warm_row(&self, b: &WarmBuildInfo) -> WarmRowPage {
        let name = self.warm_name(b);
        let path = tilde(&b.path);
        let age_ms = b.kept_at.map(|at| (self.warm.now - at).max(0));
        let clearing = b
            .status
            .clearing
            .then(|| warm_build::waiting(&b.status.copying));
        let built = match (&b.built_at, age_ms) {
            (Some(sha), Some(ms)) => format!(", built at {sha}, {}", warm_build::ago(ms)),
            (Some(sha), None) => format!(", built at {sha}"),
            _ => String::new(),
        };
        let up_to = warm_build::up_to(b.size_bytes);
        WarmRowPage {
            repo: b.repo.clone(),
            built_at: b.built_at.as_ref().map(|sha| format!("built at {sha}")),
            size: size(b.size_bytes),
            age: age_ms.map(age).unwrap_or_else(|| "—".into()),
            clear: if clearing.is_some() {
                warm_build::CLEARING.to_owned()
            } else {
                "Clear…".to_owned()
            },
            clear_enabled: clearing.is_none(),
            clearing,
            confirm: WarmConfirmPage {
                title: warm_build::confirm_title(&name, b.size_bytes),
                text: format!(
                    "Deletes {name}’s warm build{built}. The next new {name} worktree seeds from your own \
                     checkout or builds from nothing; the next Done ticket keeps a new warm build. Ticket \
                     worktrees and your repo aren’t touched."
                ),
                shared: warm_build::SHARED_SPACE,
                line: ConfirmLine {
                    icon: IconName::Flame,
                    text: format!("{path} · {up_to} (delete)"),
                },
                confirm: format!("Clear {up_to}"),
            },
            name,
            path,
        }
    }
}

/// One state of the Cleanup overlay in the gallery.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "CleanupGalleryCell")]
pub struct GalleryCell {
    pub name: String,
    pub page: Page,
}

/// *16 → Row · Cleanup with warm builds (0082)* and *… · no leftovers / no warm builds (0082)* for the gallery
/// (T-0302): the frames' samples (sizes, SHAs and ages are the frames'), each state as the page draws it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "CleanupGallery")]
pub struct Gallery {
    pub cells: Vec<GalleryCell>,
}

pub fn warm_gallery() -> Gallery {
    use otaq_core::naming::{COMMAND_NAME, data_dir_name};
    const GB: f64 = (1u64 << 30) as f64;
    const HOUR: i64 = 3_600_000;
    let now = 1_760_000_000_000;
    let build = |repo: &str, project: &str, gb: f64, sha: &str, hours: i64| WarmBuildInfo {
        repo: repo.to_owned(),
        path: format!("~/{}/warm/{repo}", data_dir_name()),
        project: Some(project.to_owned()),
        size_bytes: (gb * GB) as u64,
        built_at: Some(sha.to_owned()),
        from: None,
        kept_at: Some(now - hours * HOUR),
        status: Default::default(),
        entries: vec![],
        limit: None,
    };
    let three = || {
        vec![
            build(COMMAND_NAME, "OTQ", 36.1, "9c41e7b", 3 * 24),
            build("shop-web", "SHOP", 4.2, "3e1f9a2", 2),
            build("billing-api", "BILL", 1.3, "b7d20c4", 6 * 24),
        ]
    };
    let names: BTreeMap<String, String> = [
        ("OTQ", COMMAND_NAME),
        ("SHOP", "shop-web"),
        ("BILL", "billing-api"),
    ]
    .into_iter()
    .map(|(k, n)| (k.to_owned(), n.to_owned()))
    .collect();
    let wt = |key: &str| format!("~/{}/wt/shop-web/{key}", data_dir_name());
    let leftover = |group,
                    id: &str,
                    label: &str,
                    path: Option<String>,
                    mb: Option<u64>,
                    hours: i64,
                    why: &str| {
        CleanupItem {
            group,
            id: id.to_owned(),
            label: label.to_owned(),
            project: Some("SHOP".to_owned()),
            ticket: None,
            path,
            size_bytes: mb.map(|mb| mb << 20),
            age_ms: Some(hours * HOUR),
            why: why.to_owned(),
            uncommitted: Vec::new(),
        }
    };
    let leftovers = || {
        let mut kept = leftover(
            CleanupGroup::KeptWorktrees,
            "t17",
            "SHOP-17",
            Some(wt("SHOP-17")),
            Some(220),
            24,
            "SHOP-17 Done",
        );
        kept.uncommitted = vec!["src/lib/stripe/client.ts".into(), ".env.local".into()];
        vec![
            leftover(
                CleanupGroup::Sessions,
                "s1",
                "claude session 3f9c…",
                None,
                None,
                48,
                "no ticket · last output 2 days ago",
            ),
            leftover(
                CleanupGroup::Worktrees,
                &wt("SHOP-14"),
                "SHOP-14",
                Some(wt("SHOP-14")),
                Some(412),
                9 * 24,
                "no ticket owns it",
            ),
            kept,
        ]
    };
    let model = |items: Vec<CleanupItem>, builds: Vec<WarmBuildInfo>| {
        let mut m = CleanupModel::default();
        m.set_items(items);
        m.set_warm(builds, names.clone(), now);
        m
    };
    let mut clearing = three();
    clearing[1].status.copying = vec!["SHOP-21".into()];
    clearing[1].status.clearing = true;
    let mut after = model(leftovers(), three());
    after.clearing(COMMAND_NAME);
    after.set_warm(three().split_off(1), names.clone(), now);
    let cells = [
        ("leftovers + warm builds", model(leftovers(), three())),
        ("clearing, waiting for a copy", model(leftovers(), clearing)),
        ("right after Clear (focus on the next Clear…)", after),
        (
            "no leftovers, warm builds kept",
            model(Vec::new(), three().split_off(1)),
        ),
        (
            "no warm builds: the section is hidden",
            model(leftovers(), Vec::new()),
        ),
    ];
    Gallery {
        cells: cells
            .into_iter()
            .map(|(name, m)| GalleryCell {
                name: name.to_owned(),
                page: m.page(),
            })
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MB: u64 = 1024 * 1024;

    fn item(group: CleanupGroup, id: &str, label: &str, bytes: Option<u64>) -> CleanupItem {
        CleanupItem {
            group,
            id: id.into(),
            label: label.into(),
            project: None,
            ticket: None,
            path: matches!(group, CleanupGroup::Worktrees | CleanupGroup::KeptWorktrees)
                .then(|| format!("/data/wt/shop-web/{label}")),
            size_bytes: bytes,
            age_ms: Some(9 * 24 * 3_600_000),
            why: "no ticket uses this worktree".into(),
            uncommitted: if group == CleanupGroup::KeptWorktrees {
                vec!["src/lib/stripe/client.ts".into(), ".env.local".into()]
            } else {
                Vec::new()
            },
        }
    }

    fn sample() -> CleanupModel {
        let mut m = CleanupModel::default();
        m.set_items(vec![
            item(CleanupGroup::Sessions, "s1", "SHOP-9 · agent session", None),
            item(
                CleanupGroup::Worktrees,
                "/data/wt/shop-web/SHOP-14",
                "SHOP-14",
                Some(412 * MB),
            ),
            item(
                CleanupGroup::KeptWorktrees,
                "t17",
                "SHOP-17",
                Some(220 * MB),
            ),
        ]);
        m
    }

    #[test]
    fn nothing_is_selected_by_default_and_delete_waits_for_a_choice() {
        let m = sample();
        assert!(m.groups().iter().flat_map(|g| &g.rows).all(|r| !r.checked));
        assert_eq!(m.footer(), None);
        assert_eq!(m.confirm(), None);
        let groups = m.groups();
        let titles: Vec<&str> = groups.iter().map(|g| g.title).collect();
        assert_eq!(
            titles,
            [
                "Orphaned sessions",
                "Worktrees",
                "Done ticket with uncommitted changes"
            ]
        );
        let kept = &groups[2].rows[0];
        assert!(kept.warning);
        assert_eq!(
            kept.detail,
            "no ticket uses this worktree · 2 uncommitted files: src/lib/stripe/client.ts, .env.local"
        );
        assert_eq!((kept.size.as_str(), kept.age.as_str()), ("220 MB", "9 d"));
    }

    #[test]
    fn the_confirmation_names_every_selected_item() {
        let mut m = sample();
        let groups = m.groups();
        m.toggle(&groups[0].rows[0].target);
        m.toggle(&groups[1].rows[0].target);
        assert_eq!(m.footer().as_deref(), Some("2 selected · 412 MB"));
        let c = m.confirm().unwrap();
        assert_eq!(c.title, "Delete 2 items?");
        assert_eq!(c.confirm, "Delete 2 items");
        assert_eq!(c.lines[0], "SHOP-9 · agent session (kill)");
        assert!(
            c.lines[1].ends_with("SHOP-14 · 412 MB (remove worktree)"),
            "{}",
            c.lines[1]
        );
        assert_eq!(m.selected().len(), 2);
        // Unticking takes it out again.
        m.toggle(&groups[0].rows[0].target);
        assert_eq!(m.selected().len(), 1);
    }

    #[test]
    fn group_checkbox_and_a_new_scan() {
        let mut m = sample();
        m.toggle_group(CleanupGroup::Worktrees);
        assert!(m.groups()[1].all_checked);
        m.toggle_group(CleanupGroup::Worktrees);
        assert!(!m.groups()[1].all_checked);
        m.toggle_group(CleanupGroup::Sessions);
        m.set_items(vec![item(
            CleanupGroup::Worktrees,
            "/data/wt/x",
            "x",
            Some(MB),
        )]);
        assert!(m.selected().is_empty(), "gone items leave the selection");
    }

    #[test]
    fn the_page_draws_groups_the_confirmation_and_the_empty_state() {
        let mut m = sample();
        let page = m.page();
        assert_eq!((page.title, page.count.as_str()), ("Cleanup", "3 items"));
        assert_eq!(page.empty, None);
        assert_eq!(page.footer, None);
        assert_eq!(page.confirm, None);
        assert_eq!(page.groups[2].group, "kept_worktrees");
        assert_eq!(page.groups[2].title, "Done ticket with uncommitted changes");
        assert!(page.groups.iter().all(|g| !g.starts_folded));
        let target = page.groups[1].rows[0].target.target().unwrap();
        assert_eq!(target.group, CleanupGroup::Worktrees);
        m.toggle(&target);
        let confirm = m.page().confirm.unwrap();
        assert_eq!(confirm.lines[0].icon, IconName::FolderGit2);
        assert!(confirm.lines[0].text.ends_with("(remove worktree)"));
        assert_eq!(
            TargetPage {
                group: "nonsense".into(),
                id: "x".into()
            }
            .target(),
            None
        );

        let empty = CleanupModel::default().page();
        assert_eq!(empty.count, "0 items");
        assert_eq!(empty.empty.unwrap().title, "Nothing to clean up");
    }

    #[test]
    fn a_build_that_could_not_be_freed_is_retried_not_deleted() {
        let mut m = CleanupModel::default();
        m.set_items(vec![CleanupItem {
            path: Some("/data/wt/shop-web/SHOP-18".into()),
            why: "out/app.bin: the file is open in another program".into(),
            ..item(
                CleanupGroup::CouldNotFree,
                "t18",
                "SHOP-18 · Checkout API",
                Some(MB),
            )
        }]);
        let page = m.page();
        assert_eq!(page.groups[0].group, "could_not_free");
        assert_eq!(page.groups[0].title, "Review build that couldn’t be freed");
        let row = &page.groups[0].rows[0];
        assert!(!row.warning, "nothing is lost");
        assert!(row.title.ends_with("SHOP-18"), "{}", row.title);
        assert_eq!(
            row.detail,
            "out/app.bin: the file is open in another program"
        );
        let target = row.target.target().unwrap();
        assert_eq!(target.group, CleanupGroup::CouldNotFree);
        m.toggle(&target);
        let confirm = m.page().confirm.unwrap();
        assert!(
            confirm.lines[0]
                .text
                .ends_with("SHOP-18 · 1 MB (free build · the worktree stays)"),
            "{}",
            confirm.lines[0].text
        );
        assert_eq!(
            group_title(CleanupGroup::CouldNotFree, 2).0,
            "Review builds that couldn’t be freed"
        );
    }

    #[test]
    fn a_resource_that_could_not_be_removed_is_removed_again() {
        let mut m = CleanupModel::default();
        m.set_items(vec![CleanupItem {
            why: "remove command failed (exit 1):\ndropdb: database is being accessed".into(),
            ..item(CleanupGroup::TicketResources, "t18", "db_shop_18", None)
        }]);
        let page = m.page();
        assert_eq!(page.groups[0].group, "ticket_resources");
        assert_eq!(
            page.groups[0].title,
            "Ticket resource that couldn’t be removed"
        );
        let row = &page.groups[0].rows[0];
        assert_eq!(row.title, "db_shop_18", "the recorded name");
        assert!(!row.warning);
        assert_eq!(row.size, "—");
        let target = row.target.target().unwrap();
        assert_eq!(target.group, CleanupGroup::TicketResources);
        m.toggle(&target);
        let confirm = m.page().confirm.unwrap();
        assert_eq!(confirm.lines[0].text, "db_shop_18 (run Remove again)");
        assert_eq!(
            group_title(CleanupGroup::TicketResources, 2),
            (
                "Ticket resources that couldn’t be removed",
                IconName::Database
            )
        );
    }

    #[test]
    fn a_long_list_folds_its_groups_and_plural_titles_follow_the_count() {
        let mut m = CleanupModel::default();
        let mut items: Vec<CleanupItem> = (0..FOLD_OVER)
            .map(|i| item(CleanupGroup::Ports, &format!("p{i}"), "port", None))
            .collect();
        items.push(item(CleanupGroup::KeptWorktrees, "a", "SHOP-1", None));
        items.push(item(CleanupGroup::KeptWorktrees, "b", "SHOP-2", None));
        m.set_items(items);
        let page = m.page();
        assert!(page.groups.iter().all(|g| g.starts_folded));
        assert_eq!(page.groups[0].title, "Ports");
        assert_eq!(
            page.groups[1].title,
            "Done tickets with uncommitted changes"
        );
        assert_eq!(page.groups[1].count, "2");
    }

    const GB: u64 = 1024 * MB;
    const HOUR: i64 = 3_600_000;
    const NOW: i64 = 1_760_000_000_000;

    /// A warm build of `repo` (owned by `project`), kept `hours` ago at `sha`.
    fn warm(
        repo: &str,
        project: Option<&str>,
        tenths_gb: u64,
        sha: &str,
        hours: i64,
    ) -> WarmBuildInfo {
        WarmBuildInfo {
            repo: repo.into(),
            path: format!("/data/warm/{repo}"),
            project: project.map(Into::into),
            size_bytes: tenths_gb * GB / 10,
            built_at: Some(sha.into()),
            from: Some("SHOP-20".into()),
            kept_at: Some(NOW - hours * HOUR),
            status: Default::default(),
            entries: vec![],
            limit: None,
        }
    }

    fn names() -> BTreeMap<String, String> {
        [
            ("SHOP", "shop-web"),
            ("CORE", "core-app"),
            ("BILL", "billing-api"),
        ]
        .into_iter()
        .map(|(k, n)| (k.to_owned(), n.to_owned()))
        .collect()
    }

    /// The frame's three: core-app 36.1 GB, shop-web 4.2 GB, billing-api 1.3 GB (given out of order).
    fn three() -> Vec<WarmBuildInfo> {
        vec![
            warm("shop-web", Some("SHOP"), 42, "3e1f9a2", 2),
            warm("billing-api", Some("BILL"), 13, "b7d20c4", 6 * 24),
            warm("core-app", Some("CORE"), 361, "9c41e7b", 3 * 24),
        ]
    }

    #[test]
    fn warm_builds_are_their_own_section_largest_first() {
        let mut m = sample();
        m.set_warm(three(), names(), NOW);
        let page = m.page();
        let warm = page.warm.unwrap();
        assert_eq!((warm.title, warm.icon), ("Warm builds", IconName::Flame));
        assert_eq!(warm.count, "3");
        assert_eq!(warm.total, "41.6 GB on disk");
        assert_eq!(
            warm.note,
            "Kept to speed up new tickets. It grows over time; clear it to reclaim disk."
        );
        let rows: Vec<(&str, &str, &str)> = warm
            .rows
            .iter()
            .map(|r| (r.name.as_str(), r.size.as_str(), r.age.as_str()))
            .collect();
        assert_eq!(
            rows,
            [
                ("core-app", "36.1 GB", "3 d"),
                ("shop-web", "4.2 GB", "2 h"),
                ("billing-api", "1.3 GB", "6 d")
            ]
        );
        let first = &warm.rows[0];
        assert_eq!(first.built_at.as_deref(), Some("built at 9c41e7b"));
        assert_eq!(first.path, "/data/warm/core-app");
        assert_eq!(
            (first.clear.as_str(), first.clear_enabled),
            ("Clear…", true)
        );
        assert_eq!(first.clearing, None);

        // Not leftovers: the count, the selection and Delete selected… don't know them.
        assert_eq!(page.count, "3 items");
        m.toggle_group(CleanupGroup::Worktrees);
        assert_eq!(m.selected().len(), 1);
        assert!(
            m.confirm()
                .unwrap()
                .lines
                .iter()
                .all(|l| !l.contains("warm"))
        );
        assert_eq!(m.len(), 3);
    }

    #[test]
    fn the_clear_confirmation_says_up_to_and_keeps_the_switch() {
        let mut m = CleanupModel::default();
        m.set_warm(three(), names(), NOW);
        let c = &m.page().warm.unwrap().rows[0].confirm;
        assert_eq!(c.title, "Clear core-app’s warm build (up to 36.1 GB)?");
        assert_eq!(
            c.text,
            "Deletes core-app’s warm build, built at 9c41e7b, 3 days ago. The next new core-app worktree seeds \
             from your own checkout or builds from nothing; the next Done ticket keeps a new warm build. Ticket \
             worktrees and your repo aren’t touched."
        );
        assert_eq!(
            c.shared,
            "Tickets copied from it share part of this space, so it may free less."
        );
        assert_eq!(c.line.text, "/data/warm/core-app · up to 36.1 GB (delete)");
        assert_eq!(c.line.icon, IconName::Flame);
        assert_eq!(c.confirm, "Clear up to 36.1 GB");
    }

    #[test]
    fn no_warm_builds_hides_the_section_and_none_leaves_the_empty_state_with_it() {
        let mut m = sample();
        m.set_warm(Vec::new(), names(), NOW);
        assert_eq!(m.page().warm, None);
        assert_eq!(
            CleanupModel::default().page().empty.unwrap().text,
            "Every session, worktree, branch and port belongs to a ticket."
        );

        let mut m = CleanupModel::default();
        m.set_warm(three(), names(), NOW);
        let page = m.page();
        assert_eq!(page.count, "0 items");
        let empty = page.empty.unwrap();
        assert_eq!(empty.title, "Nothing to clean up");
        assert_eq!(
            empty.text,
            "Every session, worktree, branch and port belongs to a ticket. Warm builds aren’t leftovers."
        );
        assert_eq!(page.warm.unwrap().rows.len(), 3);
    }

    #[test]
    fn a_folder_no_project_owns_shows_its_folder_name() {
        let mut m = CleanupModel::default();
        let mut orphan = warm("old-app", None, 5, "", 0);
        orphan.built_at = None;
        orphan.from = None;
        orphan.kept_at = None;
        let mut gone = warm("legacy", Some("GONE"), 3, "1234567", 1);
        gone.kept_at = None;
        m.set_warm(vec![orphan, gone], names(), NOW);
        let warm = m.page().warm.unwrap();
        let row = &warm.rows[0];
        assert_eq!(row.name, "old-app");
        assert_eq!((row.built_at.clone(), row.age.as_str()), (None, "—"));
        assert_eq!(
            row.confirm.text,
            "Deletes old-app’s warm build. The next new old-app worktree seeds from your own checkout or \
             builds from nothing; the next Done ticket keeps a new warm build. Ticket worktrees and your repo \
             aren’t touched."
        );
        // A key with no project name left falls back on the folder too.
        assert_eq!(warm.rows[1].name, "legacy");
        assert!(warm.rows[1].confirm.text.contains(", built at 1234567. "));
        assert_eq!(m.warm_build("old-app").unwrap().repo, "old-app");
    }

    #[test]
    fn a_clear_waiting_for_a_copy_and_the_focus_after_it() {
        let mut m = CleanupModel::default();
        m.set_warm(three(), names(), NOW);
        // Clear… on shop-web while SHOP-21 seeds from it: it waits.
        m.clearing("shop-web");
        let mut builds = three();
        builds[0].status.copying = vec!["SHOP-21".into()];
        builds[0].status.clearing = true;
        m.set_warm(builds, names(), NOW);
        let warm = m.page().warm.unwrap();
        let row = &warm.rows[1];
        assert_eq!(
            row.clearing.as_deref(),
            Some("Clearing… (waiting for SHOP-21’s copy)")
        );
        assert_eq!(
            (row.clear.as_str(), row.clear_enabled),
            ("Clearing…", false)
        );
        assert_eq!(row.size, "4.2 GB", "the row keeps its full size");
        assert_eq!(warm.focus, None, "focus stays until the row goes");

        // The copy ended and the clear went through: focus moves to the row now in its place.
        let mut left = three();
        left.remove(0);
        m.set_warm(left.clone(), names(), NOW);
        let warm = m.page().warm.unwrap();
        assert_eq!(warm.total, "37.4 GB on disk");
        assert_eq!(warm.count, "2");
        let focus = warm.focus.unwrap();
        assert_eq!(focus.repo, "billing-api");
        // The next redraw doesn't move it again.
        m.set_warm(left, names(), NOW);
        assert_eq!(m.page().warm.unwrap().focus, None);

        // The last row cleared: the one before takes focus; the same row again gets a new seq.
        m.clearing("billing-api");
        m.set_warm(vec![three().remove(2)], names(), NOW);
        let again = m.page().warm.unwrap().focus.unwrap();
        assert_eq!(again.repo, "core-app");
        assert_ne!(again.seq, focus.seq);

        // Clearing the only one hides the section; a Clear that didn't go through moves nothing.
        m.clearing("core-app");
        m.set_warm(Vec::new(), names(), NOW);
        assert_eq!(m.page().warm, None);
        m.set_warm(three(), names(), NOW);
        m.clearing("shop-web");
        m.set_warm(three(), names(), NOW);
        m.set_warm(vec![three().remove(2)], names(), NOW);
        assert_eq!(m.page().warm.unwrap().focus, None);
    }

    #[test]
    fn the_gallery_draws_every_warm_build_state() {
        let cells = warm_gallery().cells;
        assert_eq!(cells.len(), 5);
        assert_eq!(
            cells[0].page.warm.as_ref().unwrap().total,
            "41.6 GB on disk"
        );
        assert!(
            cells[1].page.warm.as_ref().unwrap().rows[1]
                .clearing
                .is_some()
        );
        assert_eq!(
            cells[2]
                .page
                .warm
                .as_ref()
                .unwrap()
                .focus
                .as_ref()
                .unwrap()
                .repo,
            "shop-web"
        );
        assert!(cells[3].page.empty.is_some());
        assert_eq!(cells[4].page.warm, None);
    }

    #[test]
    fn sizes_ages_and_the_toast() {
        assert_eq!(size(412 * MB), "412 MB");
        assert_eq!(size(3 * 1024 * MB / 2), "1.5 GB");
        assert_eq!(size(10), "1 KB");
        assert_eq!(age(5 * 60_000), "5 min");
        assert_eq!(age(3 * 3_600_000), "3 h");
        let outcome = CleanupOutcome {
            deleted: vec![
                CleanupTarget {
                    group: CleanupGroup::Sessions,
                    id: "a".into(),
                },
                CleanupTarget {
                    group: CleanupGroup::Worktrees,
                    id: "b".into(),
                },
            ],
            failed: Vec::new(),
        };
        assert_eq!(
            success(&outcome, 412 * MB, 1),
            (
                "Cleaned up 2 items".to_owned(),
                Some("Freed 412 MB · 1 item left in Cleanup".to_owned())
            )
        );
    }
}
