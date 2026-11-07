//! A project's view preferences as the page reads and changes them (SHB-05, BRD-01, BRD-02, BRD-04; T-0460): the
//! service keeps them in `ProjectSettings.view` ([`ViewPrefs`]); the page gets them with `board.view` ([`Prefs`]) and
//! changes one at a time with `board.set_view` ([`Change`], applied by [`apply`] and stored with
//! `project_view.set`). Nothing is kept in the page. The toolbar ([`Toolbar`]) is worded here: the views Board · Files
//! (Settings after them), then while the board shows, its layout switch (columns or list: Board and List are two
//! layouts of the same board, not two views), Edit flow, Fold empty columns, All tickets and New ticket; on Files
//! (FIL-01, `s28-03`) its subtitle and the Live dot instead. Each option carries the change it sends.

use otaq_core::board::settings::{self as core, ViewPrefs};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::icons::IconName;
use crate::key_labels::KeyLabels;
use crate::keymap::KeyAction;

/// The board in its columns or as a list, or the project Files view (BRD-01, FIL-01). `BoardViewKind` on the page.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename = "BoardViewKind")]
pub enum ViewKind {
    Board,
    List,
    Files,
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
    pub fold_empty_columns: bool,
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
            fold_empty_columns: p.fold_empty_columns,
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
    FoldEmptyColumns {
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
        Change::View { view } => {
            prefs.view = (*view).into();
            // The board's layout is remembered across a visit to Files.
            match view {
                ViewKind::Board => prefs.list_layout = false,
                ViewKind::List => prefs.list_layout = true,
                ViewKind::Files => {}
            }
        }
        Change::FoldEmptyColumns { on } => prefs.fold_empty_columns = *on,
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

/// A segmented switch: the views, or the board's layouts. `ToolbarGroup` on the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "ToolbarGroup")]
pub struct ToolbarGroup {
    /// The words before it; `null` for the views and the layouts.
    pub label: Option<String>,
    /// An icon before the words.
    pub icon: Option<IconName>,
    pub options: Vec<ToolbarOption>,
}

/// The toolbar at the top of the board, in every view (`s28-03`). `BoardToolbar` on the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "BoardToolbar")]
pub struct Toolbar {
    /// Board · Files (BRD-01, FIL-01): Board is chosen in either layout, and from Files it goes back to the layout
    /// last shown.
    pub views: ToolbarGroup,
    /// The board's layout switch beside the views, icons only: columns (Board) or rows (List). `null` on Files.
    pub layouts: Option<ToolbarGroup>,
    /// "Settings", after the views: the project's settings (they open over the board until they are a view of
    /// their own).
    pub settings: String,
    /// "Edit flow" on the board, in either layout: the project's flow, changed from the board (it opens the flow
    /// editor).
    pub edit_flow: Option<String>,
    /// Files: "What the base branch looks like, and which agents are changing which files", after a divider.
    pub subtitle: Option<String>,
    /// Files: "Live", with its dot on the right: the view follows the tickets' changes (FIL-10).
    pub live: Option<String>,
    /// Fold empty columns, in either layout (the List folds an empty stage's group): a switch; none on Files.
    pub switch: Option<ToolbarOption>,
    /// "All tickets": the board's filter (`/`); none on Files.
    pub all_tickets: Option<String>,
    /// "New ticket" (⌘N); none on Files.
    pub new_ticket: Option<String>,
    pub new_ticket_keys: Option<String>,
}

pub const BOARD: &str = "Board";
pub const BOARD_TOOLTIP: &str = "Board view";
pub const FILES: &str = "Files";
/// The layout switch's two icons, by their tooltips.
pub const BOARD_LAYOUT: &str = "Columns";
pub const LIST_LAYOUT: &str = "List";
pub const FOLD_EMPTY_COLUMNS: &str = "Fold empty columns";
pub const ALL_TICKETS: &str = "All tickets";
pub const SETTINGS: &str = "Settings";
pub const EDIT_FLOW: &str = "Edit flow";
pub const NEW_TICKET: &str = "New ticket";
/// The Files view's toolbar subtitle (`s28-03`).
pub const FILES_SUBTITLE: &str =
    "What the base branch looks like, and which agents are changing which files";
pub const LIVE: &str = "Live";

/// The toolbar for `prefs`, with the user's keys in the tooltips.
pub fn toolbar(prefs: &ViewPrefs, keys: &KeyLabels) -> Toolbar {
    let view: ViewKind = prefs.view.into();
    let board = view != ViewKind::Files;
    // The layout the board shows, or would show again on leaving Files.
    let layout = if view == ViewKind::List || (view == ViewKind::Files && prefs.list_layout) {
        ViewKind::List
    } else {
        ViewKind::Board
    };
    let option =
        |label: &str, icon: Option<IconName>, selected: bool, change: Change| ToolbarOption {
            label: label.to_owned(),
            icon,
            selected,
            change,
            tooltip: None,
            keys: None,
        };
    let layout_option =
        |kind: ViewKind, label: &str, icon: IconName, action: KeyAction| ToolbarOption {
            tooltip: Some(label.to_owned()),
            keys: keys.label(action),
            ..option(label, Some(icon), view == kind, Change::View { view: kind })
        };
    Toolbar {
        views: ToolbarGroup {
            label: None,
            icon: None,
            options: vec![
                ToolbarOption {
                    tooltip: Some(BOARD_TOOLTIP.to_owned()),
                    keys: keys.label(KeyAction::ShowBoard),
                    ..option(BOARD, None, board, Change::View { view: layout })
                },
                // `s28-02`: "Files" over both its keys, "⇧⌘O · g then f".
                ToolbarOption {
                    tooltip: Some(FILES.to_owned()),
                    keys: Some(keys.all_labels(KeyAction::ShowFiles).join(" \u{b7} "))
                        .filter(|k| !k.is_empty()),
                    ..option(
                        FILES,
                        None,
                        !board,
                        Change::View {
                            view: ViewKind::Files,
                        },
                    )
                },
            ],
        },
        layouts: board.then(|| ToolbarGroup {
            label: None,
            icon: None,
            options: vec![
                layout_option(
                    ViewKind::Board,
                    BOARD_LAYOUT,
                    IconName::Columns3,
                    KeyAction::ShowBoard,
                ),
                layout_option(
                    ViewKind::List,
                    LIST_LAYOUT,
                    IconName::List,
                    KeyAction::ShowList,
                ),
            ],
        }),
        settings: SETTINGS.to_owned(),
        edit_flow: board.then(|| EDIT_FLOW.to_owned()),
        subtitle: (!board).then(|| FILES_SUBTITLE.to_owned()),
        live: (!board).then(|| LIVE.to_owned()),
        switch: board.then(|| {
            option(
                FOLD_EMPTY_COLUMNS,
                Some(IconName::FoldHorizontal),
                prefs.fold_empty_columns,
                Change::FoldEmptyColumns {
                    on: !prefs.fold_empty_columns,
                },
            )
        }),
        all_tickets: board.then(|| ALL_TICKETS.to_owned()),
        new_ticket: board.then(|| NEW_TICKET.to_owned()),
        new_ticket_keys: board.then(|| keys.label(KeyAction::NewTicket)).flatten(),
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
        apply(&mut prefs, &fold("product-spec", true));
        apply(&mut prefs, &fold("product-spec", true));
        assert_eq!(prefs.folded_columns, ["done", "product-spec"]);
        apply(&mut prefs, &fold("done", false));
        assert_eq!(prefs.folded_columns, ["product-spec"]);
        apply(
            &mut prefs,
            &Change::View {
                view: ViewKind::List,
            },
        );
        apply(
            &mut prefs,
            &Change::Dependencies {
                dependencies: Dependencies::Always,
            },
        );
        apply(&mut prefs, &Change::FoldEmptyColumns { on: true });
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
            (page.view, page.dependencies),
            (ViewKind::List, Dependencies::Always)
        );
        assert!(prefs.list_layout);
        assert!(page.fold_empty_columns && page.dev_unfolded);
        assert_eq!(page.folded_sections, ["ship"]);
        // On the wire: `{"kind": "fold_column", "column": "product-spec", "folded": true}`.
        assert_eq!(
            serde_json::to_value(fold("product-spec", true)).unwrap(),
            serde_json::json!({"kind": "fold_column", "column": "product-spec", "folded": true})
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
        // The views are Board · Files; List is a layout of the board beside them.
        assert_eq!(
            labels(&board.views),
            [("Board".to_owned(), true), ("Files".to_owned(), false)]
        );
        let layouts = board.layouts.as_ref().unwrap();
        assert_eq!(
            labels(layouts),
            [("Columns".to_owned(), true), ("List".to_owned(), false)]
        );
        assert_eq!(layouts.options[1].icon, Some(IconName::List));
        assert_eq!(layouts.options[1].tooltip.as_deref(), Some("List"));
        assert_eq!(layouts.options[1].keys.as_deref(), Some("g l"));
        assert_eq!(
            layouts.options[1].change,
            Change::View {
                view: ViewKind::List
            }
        );
        assert_eq!((board.subtitle.as_ref(), board.live.as_ref()), (None, None));
        // The board's only option is Fold empty columns: no Dependencies switch, no Group by.
        let json = serde_json::to_value(&board).unwrap();
        assert!(json.get("dependencies").is_none() && json.get("group_by").is_none());
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

        // The List is the same board: Board stays chosen, the list icon is, and the toolbar is the board's.
        let list = toolbar(
            &ViewPrefs {
                view: core::BoardView::List,
                list_layout: true,
                fold_empty_columns: true,
                ..ViewPrefs::default()
            },
            &keys,
        );
        assert_eq!(
            labels(&list.views),
            [("Board".to_owned(), true), ("Files".to_owned(), false)]
        );
        assert_eq!(
            labels(list.layouts.as_ref().unwrap()),
            [("Columns".to_owned(), false), ("List".to_owned(), true)]
        );
        assert_eq!(list.edit_flow.as_deref(), Some("Edit flow"));
        let switch = list.switch.as_ref().unwrap();
        assert_eq!(
            (switch.label.as_str(), switch.selected, &switch.change),
            (
                "Fold empty columns",
                true,
                &Change::FoldEmptyColumns { on: false }
            )
        );
    }

    /// Board and List are layouts of one board: from Files, Board goes back to the layout last shown.
    #[test]
    fn board_goes_back_to_the_last_layout() {
        let keys = KeyLabels::default();
        let mut prefs = ViewPrefs::default();
        for view in [ViewKind::List, ViewKind::Files] {
            apply(&mut prefs, &Change::View { view });
        }
        let files = toolbar(&prefs, &keys);
        assert_eq!(files.layouts, None);
        assert_eq!(
            files.views.options[0].change,
            Change::View {
                view: ViewKind::List
            }
        );
        for view in [ViewKind::Board, ViewKind::Files] {
            apply(&mut prefs, &Change::View { view });
        }
        assert_eq!(
            toolbar(&prefs, &keys).views.options[0].change,
            Change::View {
                view: ViewKind::Board
            }
        );
    }

    /// FIL-01, `s28-02`, `s28-03`: Files is the second segment ("Files" over "⇧⌘O · g f"); on Files the toolbar has
    /// its subtitle and the Live dot and nothing of the Board's or the List's.
    #[test]
    fn the_files_view_has_its_toolbar() {
        let keys = KeyLabels::base(crate::keymap::Platform::Mac);
        let board = toolbar(&ViewPrefs::default(), &keys);
        let files = &board.views.options[1];
        assert_eq!(
            (files.label.as_str(), files.icon, files.selected),
            ("Files", None, false)
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
        assert!(shown.views.options[1].selected);
        assert_eq!(
            shown.subtitle.as_deref(),
            Some("What the base branch looks like, and which agents are changing which files")
        );
        assert_eq!(shown.live.as_deref(), Some("Live"));
        assert_eq!(shown.switch, None);
        assert_eq!((shown.all_tickets, shown.new_ticket), (None, None));
        // Letters off: only ⇧⌘O.
        let off = toolbar(
            &ViewPrefs::default(),
            &crate::key_labels::fixtures::letters_off(),
        );
        assert_eq!(off.views.options[1].keys.as_deref(), Some("⇧⌘O"));
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
