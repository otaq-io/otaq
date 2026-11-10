//! The **project switcher** under the project's name in the title bar: a click on the name lists every project, in
//! the rail's order, each with its tile, name and folder, a dot on one that needs you and a check on the one shown,
//! then **Add project…**. The name only switches projects or adds one: changing the flow lives on the board (Edit
//! flow), the project's settings in its Settings view, and exporting the flow in its Flow settings.
//!
//! The rail's own menu (a right-click on a tile) keeps Project settings…, Rename… and Remove project….

use otaq_core::board::snapshot::ProjectInfo;
use serde::Serialize;
use ts_rs::TS;

use crate::icons::IconName;
use crate::key_labels::KeyLabels;
use crate::keymap::KeyAction;
use crate::view_models::menu::MenuItem;

/// The rail menu's items' ids, which the page acts on.
pub const PROJECT_SETTINGS: &str = "project_settings";
/// The rail's menu only: the project's Settings at its Name field, and at Remove project.
pub const RENAME_PROJECT: &str = "rename_project";
pub const REMOVE_PROJECT: &str = "remove_project";

/// The switcher's last row.
pub const ADD_PROJECT: &str = "Add project\u{2026}";

/// One project in the switcher. `ProjectSwitchRow` on the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "ProjectSwitchRow")]
pub struct SwitchRow {
    /// The project's key, which a click selects; also its tile's letters, as the rail draws them.
    pub key: String,
    /// "shop-web".
    pub name: String,
    /// "~/code/shop-web".
    pub path: String,
    /// The project needs you: the turquoise dot.
    pub needs_you: bool,
    /// The project on screen: the check mark.
    pub current: bool,
    /// What a screen reader hears: "shop-web, needs you, current".
    pub label: String,
}

/// The switcher. `ProjectMenuView` on the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "ProjectMenuView")]
pub struct ProjectMenu {
    /// "Switch project": the menu's name for screen readers.
    pub title: String,
    pub rows: Vec<SwitchRow>,
    /// "Add project…".
    pub add: String,
}

/// The switcher for `projects` in the rail's order, `shown` the project on screen; `tilde` words each folder as the
/// title bar shows it. A project needs you while its needs-you count is above zero.
pub fn switcher(
    projects: &[ProjectInfo],
    shown: Option<&str>,
    tilde: impl Fn(&str) -> String,
) -> ProjectMenu {
    ProjectMenu {
        title: "Switch project".to_owned(),
        rows: projects
            .iter()
            .map(|p| {
                let needs_you = p.needs_you > 0;
                let current = shown == Some(p.key.as_str());
                let mut label = p.name.clone();
                if needs_you {
                    label.push_str(", needs you");
                }
                if current {
                    label.push_str(", current");
                }
                SwitchRow {
                    key: p.key.clone(),
                    name: p.name.clone(),
                    path: tilde(&p.repo_path),
                    needs_you,
                    current,
                    label,
                }
            })
            .collect(),
        add: ADD_PROJECT.to_owned(),
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

/// The switcher in the component gallery: shop-web shown and needing you with billing-api beside it, and one project
/// alone.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "ProjectMenuGallery")]
pub struct Gallery {
    pub cells: Vec<(String, ProjectMenu)>,
}

/// A project of the gallery and the tests.
pub fn example(key: &str, name: &str, needs_you: u32) -> ProjectInfo {
    ProjectInfo {
        key: key.into(),
        name: name.into(),
        repo_path: format!("~/code/{name}"),
        base_branch: "main".into(),
        agents_working: 0,
        needs_you,
        paused_since: None,
        paused_by: None,
    }
}

pub fn gallery() -> Gallery {
    let two = [
        example("SH", "shop-web", 2),
        example("BI", "billing-api", 0),
    ];
    let one = [example("SH", "shop-web", 0)];
    let same = |p: &str| p.to_owned();
    Gallery {
        cells: vec![
            (
                "Two projects, one needs you".to_owned(),
                switcher(&two, Some("SH"), same),
            ),
            ("One project".to_owned(), switcher(&one, Some("SH"), same)),
        ],
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

    /// The name only switches projects or adds one: every project in the rail's order, a dot on the one that needs
    /// you, a check on the one shown, then Add project….
    #[test]
    fn the_name_lists_the_projects_and_adds_one() {
        let projects = [
            example("SH", "shop-web", 2),
            example("BI", "billing-api", 0),
        ];
        let menu = switcher(&projects, Some("BI"), |p| p.replace("/code", "/src"));
        let rows: Vec<(&str, &str, bool, bool)> = menu
            .rows
            .iter()
            .map(|r| (r.key.as_str(), r.path.as_str(), r.needs_you, r.current))
            .collect();
        assert_eq!(
            rows,
            [
                ("SH", "~/src/shop-web", true, false),
                ("BI", "~/src/billing-api", false, true),
            ]
        );
        assert_eq!(menu.rows[0].label, "shop-web, needs you");
        assert_eq!(menu.rows[1].label, "billing-api, current");
        assert_eq!(menu.add, "Add project\u{2026}");
        let none = switcher(&projects, None, str::to_owned);
        assert!(none.rows.iter().all(|r| !r.current));
        assert_eq!(gallery().cells.len(), 2);
    }
}
