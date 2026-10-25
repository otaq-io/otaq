//! A project's view preferences as the page reads and changes them (SHB-05, BRD-01, BRD-02, BRD-04; T-0460): the
//! service keeps them in `ProjectSettings.view` ([`ViewPrefs`]); the page gets them with `board.view` ([`Prefs`]) and
//! changes one at a time with `board.set_view` ([`Change`], applied by [`apply`] and stored with
//! `project_view.set`). Nothing is kept in the page. The toolbar ([`Toolbar`], *24 · The two views*) is worded here:
//! Board · List · Files, Group by on the List, Fold empty columns or Hide empty
//! stages, All tickets and New ticket; on Files (FIL-01, `s28-03`) its subtitle and the Live dot instead. Each option
//! carries the change it sends.

use otaq_core::board::settings::{self as core, ViewPrefs};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::icons::IconName;
use crate::key_labels::KeyLabels;
use crate::keymap::KeyAction;

/// Board, List or the project Files view (BRD-01, FIL-01). `BoardViewKind` on the page.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename = "BoardViewKind")]
pub enum ViewKind {
    Board,
    List,
    Files,
}

/// The List's grouping (LST-01). `ListGroupBy` on the page.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename = "ListGroupBy")]
pub enum GroupBy {
    Stage,
    Status,
    Agent,
}

/// When dependencies are drawn (DPV-02). `DependenciesMode` on the page.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename = "DependenciesMode")]
pub enum Dependencies {
    OnHover,
    Always,
}

impl From<core::BoardView> for ViewKind {
    fn from(v: core::BoardView) -> Self {
        match v {
            core::BoardView::Board => Self::Board,
            core::BoardView::List => Self::List,
            core::BoardView::Files => Self::Files,
        }
    }
}

impl From<ViewKind> for core::BoardView {
    fn from(v: ViewKind) -> Self {
        match v {
            ViewKind::Board => Self::Board,
            ViewKind::List => Self::List,
            ViewKind::Files => Self::Files,
        }
    }
}

impl From<core::GroupBy> for GroupBy {
    fn from(v: core::GroupBy) -> Self {
        match v {
            core::GroupBy::Stage => Self::Stage,
            core::GroupBy::Status => Self::Status,
            core::GroupBy::Agent => Self::Agent,
        }
    }
}

impl From<GroupBy> for core::GroupBy {
    fn from(v: GroupBy) -> Self {
        match v {
            GroupBy::Stage => Self::Stage,
            GroupBy::Status => Self::Status,
            GroupBy::Agent => Self::Agent,
        }
    }
}

impl From<core::Dependencies> for Dependencies {
    fn from(v: core::Dependencies) -> Self {
        match v {
            core::Dependencies::OnHover => Self::OnHover,
            core::Dependencies::Always => Self::Always,
        }
    }
}

impl From<Dependencies> for core::Dependencies {
    fn from(v: Dependencies) -> Self {
        match v {
            Dependencies::OnHover => Self::OnHover,
            Dependencies::Always => Self::Always,
        }
    }
}

/// The project's view preferences as the page reads them (SHB-05). `ViewPrefs` on the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "ViewPrefs")]
pub struct Prefs {
    pub view: ViewKind,
    pub group_by: GroupBy,
    pub fold_empty_columns: bool,
    pub hide_empty_stages: bool,
    pub dependencies: Dependencies,
    /// Column ids folded from their ⋯ ("done" at first).
    pub folded_columns: Vec<String>,
    /// A human check column's sections folded by their label.
    pub folded_sections: Vec<String>,
    pub dev_unfolded: bool,
}

impl From<&ViewPrefs> for Prefs {
    fn from(p: &ViewPrefs) -> Self {
        Self {
            view: p.view.into(),
            group_by: p.group_by.into(),
            fold_empty_columns: p.fold_empty_columns,
            hide_empty_stages: p.hide_empty_stages,
            dependencies: p.dependencies.into(),
            folded_columns: p.folded_columns.clone(),
            folded_sections: p.folded_sections.clone(),
            dev_unfolded: p.dev_unfolded,
        }
    }
}

/// One change of the view preferences (`board.set_view`). `ViewChange` on the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(rename = "ViewChange")]
pub enum Change {
    View {
        view: ViewKind,
    },
    GroupBy {
        group_by: GroupBy,
    },
    FoldEmptyColumns {
        on: bool,
    },
    HideEmptyStages {
        on: bool,
    },
    Dependencies {
        dependencies: Dependencies,
    },
    /// A column folded from its ⋯ (`true`) or unfolded by a click on its rail (`false`), by its id.
    FoldColumn {
        column: String,
        folded: bool,
    },
    /// A section of a human check column folded or unfolded by its label, by its id.
    FoldSection {
        section: String,
        folded: bool,
    },
    /// An agent column's "4 more · working" fold opened (`true`) or back (`false`).
    DevUnfolded {
        on: bool,
    },
}

/// Puts `change` into `prefs`. Folding twice or unfolding what isn't folded changes nothing.
pub fn apply(prefs: &mut ViewPrefs, change: &Change) {
    fn set(list: &mut Vec<String>, id: &str, on: bool) {
        list.retain(|f| f != id);
        if on {
            list.push(id.to_owned());
        }
    }
    match change {
        Change::View { view } => prefs.view = (*view).into(),
        Change::GroupBy { group_by } => prefs.group_by = (*group_by).into(),
        Change::FoldEmptyColumns { on } => prefs.fold_empty_columns = *on,
        Change::HideEmptyStages { on } => prefs.hide_empty_stages = *on,
        Change::Dependencies { dependencies } => prefs.dependencies = (*dependencies).into(),
        Change::FoldColumn { column, folded } => set(&mut prefs.folded_columns, column, *folded),
        Change::FoldSection { section, folded } => {
            set(&mut prefs.folded_sections, section, *folded);
        }
        Change::DevUnfolded { on } => prefs.dev_unfolded = *on,
    }
}

/// One choice in the toolbar: its words, whether it is the current one, and the change a click sends.
/// `ToolbarOption` on the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "ToolbarOption")]
pub struct ToolbarOption {
    pub label: String,
    pub icon: Option<IconName>,
    /// The current one (a segment), or on (a switch).
    pub selected: bool,
    pub change: Change,
    /// "List view" over its key ("g l"), on hover.
    pub tooltip: Option<String>,
    pub keys: Option<String>,
}

/// A segmented switch: "Group by" Stage · Status · Agent. `ToolbarGroup` on the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "ToolbarGroup")]
pub struct ToolbarGroup {
    /// The words before it ("Group by"); `null` for Board · List · Files.
    pub label: Option<String>,
    /// An icon before the words: the link before "Dependencies".
    pub icon: Option<IconName>,
    pub options: Vec<ToolbarOption>,
}

/// The toolbar at the top of the board, in every view (*24 · The two views*, `s28-03`). `BoardToolbar` on the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "BoardToolbar")]
pub struct Toolbar {
    /// Board · List · Files (BRD-01, FIL-01).
    pub views: ToolbarGroup,
    /// Files: "What the base branch looks like, and which agents are changing which files", after a divider.
    pub subtitle: Option<String>,
    /// Files: "Live", with its dot on the right: the view follows the tickets' changes (FIL-10).
    pub live: Option<String>,
    /// The List's Group by Stage · Status · Agent (LST-01).
    pub group_by: Option<ToolbarGroup>,
    /// The Board's Dependencies On hover · Always (DPV-02), on the right before the switch.
    pub dependencies: Option<ToolbarGroup>,
    /// Fold empty columns (Board) or Hide empty stages (List): a switch; none on Files.
    pub switch: Option<ToolbarOption>,
    /// "All tickets": the board's filter (`/`); none on Files.
    pub all_tickets: Option<String>,
    /// "New ticket" (⌘N); none on Files.
    pub new_ticket: Option<String>,
    pub new_ticket_keys: Option<String>,
}

pub const ALL_TICKETS: &str = "All tickets";
pub const NEW_TICKET: &str = "New ticket";
/// The Files view's toolbar subtitle (`s28-03`).
pub const FILES_SUBTITLE: &str =
    "What the base branch looks like, and which agents are changing which files";
pub const LIVE: &str = "Live";

/// The toolbar for `prefs`, with the user's keys in the tooltips.
pub fn toolbar(prefs: &ViewPrefs, keys: &KeyLabels) -> Toolbar {
    let view: ViewKind = prefs.view.into();
    let option =
        |label: &str, icon: Option<IconName>, selected: bool, change: Change| ToolbarOption {
            label: label.to_owned(),
            icon,
            selected,
            change,
            tooltip: None,
            keys: None,
        };
    let view_option =
        |kind: ViewKind, label: &str, icon: IconName, action: KeyAction| ToolbarOption {
            tooltip: Some(format!("{label} view")),
            keys: keys.label(action),
            ..option(label, Some(icon), view == kind, Change::View { view: kind })
        };
    let group_by: GroupBy = prefs.group_by.into();
    let dependencies: Dependencies = prefs.dependencies.into();
    Toolbar {
        views: ToolbarGroup {
            label: None,
            icon: None,
            options: vec![
                view_option(
                    ViewKind::Board,
                    "Board",
                    IconName::Columns3,
                    KeyAction::ShowBoard,
                ),
                view_option(ViewKind::List, "List", IconName::List, KeyAction::ShowList),
                // `s28-02`: "Files" over both its keys, "⇧⌘O · g then f".
                ToolbarOption {
                    tooltip: Some("Files".to_owned()),
                    keys: Some(keys.all_labels(KeyAction::ShowFiles).join(" \u{b7} "))
                        .filter(|k| !k.is_empty()),
                    ..option(
                        "Files",
                        Some(IconName::FolderOpen),
                        view == ViewKind::Files,
                        Change::View {
                            view: ViewKind::Files,
                        },
                    )
                },
            ],
        },
        subtitle: (view == ViewKind::Files).then(|| FILES_SUBTITLE.to_owned()),
        live: (view == ViewKind::Files).then(|| LIVE.to_owned()),
        group_by: (view == ViewKind::List).then(|| ToolbarGroup {
            label: Some("Group by".to_owned()),
            icon: None,
            options: [
                (GroupBy::Stage, "Stage"),
                (GroupBy::Status, "Status"),
                (GroupBy::Agent, "Agent"),
            ]
            .into_iter()
            .map(|(g, label)| option(label, None, group_by == g, Change::GroupBy { group_by: g }))
            .collect(),
        }),
        dependencies: (view == ViewKind::Board).then(|| ToolbarGroup {
            label: Some("Dependencies".to_owned()),
            icon: Some(IconName::Link),
            options: [
                (Dependencies::OnHover, "On hover"),
                (Dependencies::Always, "Always"),
            ]
            .into_iter()
            .map(|(d, label)| {
                option(
                    label,
                    None,
                    dependencies == d,
                    Change::Dependencies { dependencies: d },
                )
            })
            .collect(),
        }),
        switch: match view {
            ViewKind::Board => Some(option(
                "Fold empty columns",
                Some(IconName::FoldHorizontal),
                prefs.fold_empty_columns,
                Change::FoldEmptyColumns {
                    on: !prefs.fold_empty_columns,
                },
            )),
            ViewKind::List => Some(option(
                "Hide empty stages",
                Some(IconName::FoldHorizontal),
                prefs.hide_empty_stages,
                Change::HideEmptyStages {
                    on: !prefs.hide_empty_stages,
                },
            )),
            ViewKind::Files => None,
        },
        all_tickets: (view != ViewKind::Files).then(|| ALL_TICKETS.to_owned()),
        new_ticket: (view != ViewKind::Files).then(|| NEW_TICKET.to_owned()),
        new_ticket_keys: (view != ViewKind::Files)
            .then(|| keys.label(KeyAction::NewTicket))
            .flatten(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn changes_fold_and_unfold_once() {
        let mut prefs = ViewPrefs::default();
        let fold = |column: &str, folded| Change::FoldColumn {
            column: column.into(),
            folded,
        };
        apply(&mut prefs, &fold("grill", true));
        apply(&mut prefs, &fold("grill", true));
        assert_eq!(prefs.folded_columns, ["done", "grill"]);
        apply(&mut prefs, &fold("done", false));
        assert_eq!(prefs.folded_columns, ["grill"]);
        apply(
            &mut prefs,
            &Change::View {
                view: ViewKind::List,
            },
        );
        apply(
            &mut prefs,
            &Change::GroupBy {
                group_by: GroupBy::Status,
            },
        );
        apply(
            &mut prefs,
            &Change::Dependencies {
                dependencies: Dependencies::Always,
            },
        );
        apply(&mut prefs, &Change::FoldEmptyColumns { on: true });
        apply(&mut prefs, &Change::HideEmptyStages { on: true });
        apply(
            &mut prefs,
            &Change::FoldSection {
                section: "ship".into(),
                folded: true,
            },
        );
        apply(&mut prefs, &Change::DevUnfolded { on: true });
        let page = Prefs::from(&prefs);
        assert_eq!(
            (page.view, page.group_by, page.dependencies),
            (ViewKind::List, GroupBy::Status, Dependencies::Always)
        );
        assert!(page.fold_empty_columns && page.hide_empty_stages && page.dev_unfolded);
        assert_eq!(page.folded_sections, ["ship"]);
        // On the wire: `{"kind": "fold_column", "column": "grill", "folded": true}`.
        assert_eq!(
            serde_json::to_value(fold("grill", true)).unwrap(),
            serde_json::json!({"kind": "fold_column", "column": "grill", "folded": true})
        );
    }

    #[test]
    fn the_toolbar_follows_the_view() {
        let keys = KeyLabels::default();
        let prefs = ViewPrefs::default();
        let board = toolbar(&prefs, &keys);
        let labels = |g: &ToolbarGroup| -> Vec<(String, bool)> {
            g.options
                .iter()
                .map(|o| (o.label.clone(), o.selected))
                .collect()
        };
        assert_eq!(
            labels(&board.views),
            [
                ("Board".to_owned(), true),
                ("List".to_owned(), false),
                ("Files".to_owned(), false)
            ]
        );
        assert_eq!((board.subtitle.as_ref(), board.live.as_ref()), (None, None));
        assert_eq!(board.views.options[1].tooltip.as_deref(), Some("List view"));
        assert_eq!(board.views.options[1].keys.as_deref(), Some("g l"));
        assert_eq!(board.group_by, None);
        // DPV-02: Dependencies On hover · Always, On hover by default, the link before its words; a click sends Always.
        let deps = board.dependencies.as_ref().unwrap();
        assert_eq!(
            (deps.label.as_deref(), deps.icon),
            (Some("Dependencies"), Some(IconName::Link))
        );
        assert_eq!(
            labels(deps),
            [("On hover".to_owned(), true), ("Always".to_owned(), false)]
        );
        assert_eq!(
            deps.options[1].change,
            Change::Dependencies {
                dependencies: Dependencies::Always
            }
        );
        let switch = board.switch.as_ref().unwrap();
        assert_eq!(
            (switch.label.as_str(), switch.selected, &switch.change),
            (
                "Fold empty columns",
                false,
                &Change::FoldEmptyColumns { on: true }
            )
        );
        assert_eq!(
            (board.all_tickets.as_deref(), board.new_ticket.as_deref()),
            (Some("All tickets"), Some("New ticket"))
        );

        let list = toolbar(
            &ViewPrefs {
                view: core::BoardView::List,
                hide_empty_stages: true,
                ..ViewPrefs::default()
            },
            &keys,
        );
        assert_eq!(list.dependencies, None, "the List draws no arrows");
        let group = list.group_by.as_ref().unwrap();
        assert_eq!(group.label.as_deref(), Some("Group by"));
        assert_eq!(
            labels(group),
            [
                ("Stage".to_owned(), true),
                ("Status".to_owned(), false),
                ("Agent".to_owned(), false)
            ]
        );
        let switch = list.switch.as_ref().unwrap();
        assert_eq!(
            (switch.label.as_str(), switch.selected, &switch.change),
            (
                "Hide empty stages",
                true,
                &Change::HideEmptyStages { on: false }
            )
        );
    }

    /// FIL-01, `s28-02`, `s28-03`: Files is the third segment ("Files" over "⇧⌘O · g f"); on Files the toolbar has
    /// its subtitle and the Live dot and nothing of the Board's or the List's.
    #[test]
    fn the_files_view_has_its_toolbar() {
        let keys = KeyLabels::base(crate::keymap::Platform::Mac);
        let board = toolbar(&ViewPrefs::default(), &keys);
        let files = &board.views.options[2];
        assert_eq!(
            (files.label.as_str(), files.icon, files.selected),
            ("Files", Some(IconName::FolderOpen), false)
        );
        assert_eq!(
            (files.tooltip.as_deref(), files.keys.as_deref()),
            (Some("Files"), Some("⇧⌘O \u{b7} g f"))
        );
        assert_eq!(
            files.change,
            Change::View {
                view: ViewKind::Files
            }
        );
        let shown = toolbar(
            &ViewPrefs {
                view: core::BoardView::Files,
                ..ViewPrefs::default()
            },
            &keys,
        );
        assert!(shown.views.options[2].selected);
        assert_eq!(
            shown.subtitle.as_deref(),
            Some("What the base branch looks like, and which agents are changing which files")
        );
        assert_eq!(shown.live.as_deref(), Some("Live"));
        assert_eq!(
            (&shown.group_by, &shown.dependencies, &shown.switch),
            (&None, &None, &None)
        );
        assert_eq!((shown.all_tickets, shown.new_ticket), (None, None));
        // Letters off: only ⇧⌘O.
        let off = toolbar(
            &ViewPrefs::default(),
            &crate::key_labels::fixtures::letters_off(),
        );
        assert_eq!(off.views.options[2].keys.as_deref(), Some("⇧⌘O"));
    }

    /// SHB-05: Files is stored and read back like the other two views (`ViewPrefs` round-trip).
    #[test]
    fn the_files_view_is_remembered() {
        let mut prefs = ViewPrefs::default();
        apply(
            &mut prefs,
            &Change::View {
                view: ViewKind::Files,
            },
        );
        assert_eq!(prefs.view, core::BoardView::Files);
        let stored = serde_json::to_string(&prefs).unwrap();
        assert!(stored.contains("\"view\":\"files\""), "{stored}");
        let back: ViewPrefs = serde_json::from_str(&stored).unwrap();
        assert_eq!(back, prefs);
        assert_eq!(Prefs::from(&back).view, ViewKind::Files);
        assert_eq!(
            serde_json::to_value(Change::View {
                view: ViewKind::Files
            })
            .unwrap(),
            serde_json::json!({"kind": "view", "view": "files"})
        );
    }
}
