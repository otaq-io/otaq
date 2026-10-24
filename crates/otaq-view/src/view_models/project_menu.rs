//! The **project menu** (FED-09, d0115; *26 → Row · Changing the flow later: where it is* `s26-08`, *Row · …: waiting
//! to apply* `s26-12`): a click on the project's name in the title bar. Its head is the project's name and folder;
//! then **Change flow…** with the flow in use under it ("shop-web’s flow · 7 steps", or "shop-web’s flow → Grill first
//! · waiting" while a change waits), **Export flow…**, **Project settings…** with the user's key, **Open in Finder**
//! and **Open a terminal here**. Changing the flow is rare, so it lives here and not on the board.

use otaq_core::flow::apply::WaitingChange;
use otaq_core::flow::model::FlowFile;
use otaq_core::flow::presets::preset;
use otaq_core::tools::Os;
use serde::Serialize;
use ts_rs::TS;

use crate::icons::IconName;
use crate::key_labels::KeyLabels;
use crate::keymap::KeyAction;
use crate::view_models::flow_settings::export_file;
use crate::view_models::menu::MenuItem;

/// The items' ids, which the page acts on.
pub const CHANGE_FLOW: &str = "change_flow";
pub const EXPORT_FLOW: &str = "export_flow";
pub const PROJECT_SETTINGS: &str = "project_settings";
pub const OPEN_FOLDER: &str = "open_folder";
pub const OPEN_TERMINAL: &str = "open_terminal";
/// The rail's menu only: the project's Settings at its Name field, and at Remove project.
pub const RENAME_PROJECT: &str = "rename_project";
pub const REMOVE_PROJECT: &str = "remove_project";

/// The project menu. `ProjectMenuView` on the page.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(rename = "ProjectMenuView")]
pub struct ProjectMenu {
    /// "shop-web".
    pub title: String,
    /// "~/code/shop-web".
    pub path: String,
    pub items: Vec<MenuItem>,
    /// Export flow…'s save dialog: its title and the file it offers.
    pub export_title: String,
    pub export_file: String,
}

/// What the menu is built from.
pub struct Facts<'a> {
    pub name: &'a str,
    /// The folder as the title bar shows it ("~/code/shop-web").
    pub path: &'a str,
    /// The flow in use; `None` when it couldn't be read (Change flow… then has no second line).
    pub flow: Option<&'a FlowFile>,
    pub waiting: Option<&'a WaitingChange>,
    pub os: Os,
    pub keys: &'a KeyLabels,
}

/// "Open in Finder" as each OS calls it.
pub fn open_folder_label(os: Os) -> &'static str {
    match os {
        Os::Mac => "Open in Finder",
        Os::Windows => "Open in File Explorer",
        Os::Linux => "Open in Files",
    }
}

/// Change flow…'s second line: the flow in use and its steps (Ready for Dev counted, as the editor counts them), or
/// where a waiting change goes ("→ Grill first · waiting" for a preset, else "· a change waiting").
pub fn flow_line(
    name: &str,
    flow: Option<&FlowFile>,
    waiting: Option<&WaitingChange>,
) -> Option<String> {
    let own = format!("{name}\u{2019}s flow");
    if let Some(waiting) = waiting {
        return Some(match waiting.preset.as_deref().and_then(preset) {
            Some(p) => format!("{own} \u{2192} {} \u{b7} waiting", p.name),
            None => format!("{own} \u{b7} a change waiting"),
        });
    }
    let steps = flow?.flow.stages.len() + 1;
    Some(format!("{own} \u{b7} {steps} steps"))
}

pub fn menu(f: &Facts) -> ProjectMenu {
    let mut change = MenuItem::new(CHANGE_FLOW, "Change flow\u{2026}").icon(IconName::Workflow);
    change.hint = flow_line(f.name, f.flow, f.waiting);
    let mut settings =
        MenuItem::new(PROJECT_SETTINGS, "Project settings\u{2026}").icon(IconName::Settings);
    settings.shortcut = f.keys.label(KeyAction::Settings);
    settings.separated = true;
    ProjectMenu {
        title: f.name.to_owned(),
        path: f.path.to_owned(),
        items: vec![
            change,
            MenuItem::new(EXPORT_FLOW, "Export flow\u{2026}").icon(IconName::ArrowDownToLine),
            settings,
            MenuItem::new(OPEN_FOLDER, open_folder_label(f.os)).icon(IconName::FolderOpen),
            MenuItem::new(OPEN_TERMINAL, "Open a terminal here").icon(IconName::SquareTerminal),
        ],
        export_title: format!("Export {}\u{2019}s flow", f.name),
        export_file: export_file(f.name),
    }
}

/// A right-click on a project's tile in the rail (#11): Project settings…, Rename… and Remove project…, the same for
/// every project. Rename… and Remove project… open the project's Settings at its Name field and at Remove project.
pub fn rail_menu(keys: &KeyLabels) -> Vec<MenuItem> {
    let mut settings =
        MenuItem::new(PROJECT_SETTINGS, "Project settings\u{2026}").icon(IconName::Settings);
    settings.shortcut = keys.label(KeyAction::Settings);
    vec![
        settings,
        MenuItem::new(RENAME_PROJECT, "Rename\u{2026}").icon(IconName::Pencil),
        MenuItem::new(REMOVE_PROJECT, "Remove project\u{2026}")
            .icon(IconName::Trash2)
            .danger()
            .separated(),
    ]
}

/// The menu in the component gallery: shop-web's flow in use, and with Grill first waiting.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(rename = "ProjectMenuGallery")]
pub struct Gallery {
    pub cells: Vec<(String, ProjectMenu)>,
}

pub fn gallery() -> Gallery {
    let keys = &KeyLabels::default();
    let flow = fixtures::seven_steps();
    let waiting = fixtures::waiting_for("grill-first.yaml");
    let facts = |waiting| Facts {
        name: "shop-web",
        path: "~/code/shop-web",
        flow: Some(&flow),
        waiting,
        os: Os::Mac,
        keys,
    };
    Gallery {
        cells: vec![
            ("The flow in use".to_owned(), menu(&facts(None))),
            ("A change waiting".to_owned(), menu(&facts(Some(&waiting)))),
        ],
    }
}

/// shop-web's flow (Grill first with a Security check: 7 steps) and a waiting swap.
pub mod fixtures {
    use otaq_core::flow::apply::{Hold, Wait, WaitingChange};
    use otaq_core::flow::model::{FlowFile, Stage};
    use otaq_core::flow::presets::preset;
    use otaq_core::flow::store::AppliedBy;

    /// Grill first (Ready, Grill, Dev, Validate, Review, Done) with Security after Validate.
    pub fn seven_steps() -> FlowFile {
        let mut file = preset("grill-first.yaml").expect("a shipped preset").read();
        let validate = file
            .flow
            .stages
            .iter()
            .position(|s| s.id == "validate")
            .expect("Grill first has Validate");
        let mut security: Stage = file.flow.stages[validate].clone();
        security.id = "security".into();
        security.name = "Security".into();
        file.flow.stages.insert(validate + 1, security);
        file
    }

    /// A swap to the preset `file`, waiting for the whole board.
    pub fn waiting_for(file: &str) -> WaitingChange {
        WaitingChange {
            project: "SHOP".into(),
            yaml: String::new(),
            applied_by: AppliedBy::User,
            agent_name: None,
            preset: Some(file.into()),
            changes: vec!["Grill first".into()],
            since: 0,
            wait: Wait::WholeBoard,
            hold: Hold::READY_FOR_DEV,
            waiting_for: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::key_labels::fixtures as keys;

    #[test]
    fn the_rail_menu_offers_settings_rename_and_remove() {
        let keys = keys::base();
        let menu = rail_menu(&keys);
        let rows: Vec<(&str, &str, bool, bool)> = menu
            .iter()
            .map(|i| (i.id.as_str(), i.label.as_str(), i.danger, i.separated))
            .collect();
        assert_eq!(
            rows,
            [
                (PROJECT_SETTINGS, "Project settings\u{2026}", false, false),
                (RENAME_PROJECT, "Rename\u{2026}", false, false),
                (REMOVE_PROJECT, "Remove project\u{2026}", true, true),
            ]
        );
        assert_eq!(menu[0].shortcut, keys.label(KeyAction::Settings));
    }

    #[test]
    fn the_menu_matches_the_frame_s26_08() {
        let keys = keys::base();
        let flow = fixtures::seven_steps();
        let m = menu(&Facts {
            name: "shop-web",
            path: "~/code/shop-web",
            flow: Some(&flow),
            waiting: None,
            os: Os::Mac,
            keys: &keys,
        });
        assert_eq!(
            (m.title.as_str(), m.path.as_str()),
            ("shop-web", "~/code/shop-web")
        );
        let rows: Vec<(&str, &str, Option<&str>)> = m
            .items
            .iter()
            .map(|i| (i.id.as_str(), i.label.as_str(), i.hint.as_deref()))
            .collect();
        assert_eq!(
            rows,
            [
                (
                    CHANGE_FLOW,
                    "Change flow\u{2026}",
                    Some("shop-web\u{2019}s flow \u{b7} 7 steps")
                ),
                (EXPORT_FLOW, "Export flow\u{2026}", None),
                (PROJECT_SETTINGS, "Project settings\u{2026}", None),
                (OPEN_FOLDER, "Open in Finder", None),
                (OPEN_TERMINAL, "Open a terminal here", None),
            ]
        );
        assert_eq!(m.items[2].shortcut.as_deref(), Some("\u{2318},"));
        assert!(
            m.items[2].separated,
            "Project settings… starts the project's own items"
        );
        assert_eq!(m.export_file, "shop-web-flow.yaml");
    }

    #[test]
    fn a_waiting_swap_shows_where_the_flow_goes_s26_12() {
        let flow = fixtures::seven_steps();
        let waiting = fixtures::waiting_for("grill-first.yaml");
        assert_eq!(
            flow_line("shop-web", Some(&flow), Some(&waiting)).as_deref(),
            Some("shop-web\u{2019}s flow \u{2192} Grill first \u{b7} waiting")
        );
        let mut edit = waiting.clone();
        edit.preset = None;
        assert_eq!(
            flow_line("shop-web", Some(&flow), Some(&edit)).as_deref(),
            Some("shop-web\u{2019}s flow \u{b7} a change waiting")
        );
        assert_eq!(flow_line("shop-web", None, None), None);
        assert_eq!(gallery().cells.len(), 2);
    }

    #[test]
    fn open_in_finder_as_each_os_calls_it() {
        assert_eq!(open_folder_label(Os::Mac), "Open in Finder");
        assert_eq!(open_folder_label(Os::Windows), "Open in File Explorer");
        assert_eq!(open_folder_label(Os::Linux), "Open in Files");
    }
}
