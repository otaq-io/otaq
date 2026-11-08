//! Context menus and dropdown lists (*01 · Components → Overlays & feedback → Context menu*): items with an
//! icon, a label, an optional shortcut, a danger tone, a disabled state with a hint, and separators. Keyboard
//! navigation skips separators and disabled items and wraps around.

use serde::Serialize;
use ts_rs::TS;

use crate::icons::IconName;

/// One row of a menu. The page's menu (T-0217) draws a list of these and applies the same keyboard rules as
/// [`MenuModel`]: arrows skip headers and disabled rows and wrap; ↵ and clicks choose enabled rows only.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
pub struct MenuItem {
    /// What the owner gets back when the item is chosen.
    pub id: String,
    pub label: String,
    pub icon: Option<IconName>,
    pub shortcut: Option<String>,
    /// Destructive actions are drawn in the error colour.
    pub danger: bool,
    pub disabled: bool,
    /// A second, muted line ("Only for tickets that never started").
    pub hint: Option<String>,
    /// Draws a separator above the item (a header too).
    pub separated: bool,
    /// Shown with a check mark (the current value of a dropdown).
    pub checked: bool,
    /// A group label ("LOCAL", "REMOTE"); never chosen.
    pub header: bool,
}

impl MenuItem {
    pub fn new(id: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            icon: None,
            shortcut: None,
            danger: false,
            disabled: false,
            hint: None,
            separated: false,
            checked: false,
            header: false,
        }
    }

    /// A line of explanation between the rows, never chosen: a header with no label, only its words.
    pub fn note(text: impl Into<String>) -> Self {
        Self {
            hint: Some(text.into()),
            ..Self::header("")
        }
    }

    /// A group label in a list.
    pub fn header(label: impl Into<String>) -> Self {
        Self {
            header: true,
            ..Self::new("", label)
        }
    }

    pub fn icon(mut self, icon: IconName) -> Self {
        self.icon = Some(icon);
        self
    }

    pub fn shortcut(mut self, keys: impl Into<String>) -> Self {
        self.shortcut = Some(keys.into());
        self
    }

    pub fn danger(mut self) -> Self {
        self.danger = true;
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    pub fn hint(mut self, hint: impl Into<String>) -> Self {
        self.hint = Some(hint.into());
        self
    }

    pub fn separated(mut self) -> Self {
        self.separated = true;
        self
    }

    pub fn checked(mut self, checked: bool) -> Self {
        self.checked = checked;
        self
    }
}

/// A dropdown list with a search field on top (the base-branch dropdowns): the field's placeholder and the line
/// shown when nothing matches.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
pub struct MenuSearch {
    pub placeholder: String,
    pub empty: String,
}

impl MenuSearch {
    /// The base-branch dropdowns' search (Add project, Settings, the editor).
    pub fn branches() -> Self {
        Self {
            placeholder: "Search branches\u{2026}".to_owned(),
            empty: "No matching branches".to_owned(),
        }
    }
}

/// A menu's items and its highlighted row.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MenuModel {
    pub items: Vec<MenuItem>,
    pub highlighted: Option<usize>,
}

impl MenuModel {
    pub fn new(items: Vec<MenuItem>) -> Self {
        Self {
            items,
            highlighted: None,
        }
    }

    fn enabled(&self, index: usize) -> bool {
        self.items
            .get(index)
            .is_some_and(|i| !i.disabled && !i.header)
    }

    /// Highlights the next enabled item, wrapping around.
    pub fn next(&mut self) {
        self.step(1);
    }

    pub fn previous(&mut self) {
        self.step(-1);
    }

    fn step(&mut self, direction: isize) {
        let n = self.items.len();
        if n == 0 {
            return;
        }
        let start = match self.highlighted {
            Some(i) => i as isize,
            None if direction > 0 => -1,
            None => n as isize,
        };
        for k in 1..=n as isize {
            let i = (start + direction * k).rem_euclid(n as isize) as usize;
            if self.enabled(i) {
                self.highlighted = Some(i);
                return;
            }
        }
    }

    /// Highlights an item under the pointer (disabled items aren't highlighted).
    pub fn hover(&mut self, index: usize) {
        if self.enabled(index) {
            self.highlighted = Some(index);
        }
    }

    /// The chosen item's id (↵ or a click), if it can be chosen.
    pub fn activate(&self, index: Option<usize>) -> Option<String> {
        let index = index.or(self.highlighted)?;
        self.enabled(index).then(|| self.items[index].id.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn menu() -> MenuModel {
        MenuModel::new(vec![
            MenuItem::new("open", "Open panel").shortcut("↵"),
            MenuItem::new("pin", "Pin to top").separated(),
            MenuItem::new("close", "Close ticket…").danger(),
            MenuItem::new("delete", "Delete…")
                .danger()
                .disabled(true)
                .hint("Started tickets can only be closed."),
        ])
    }

    #[test]
    fn arrows_skip_disabled_items_and_wrap() {
        let mut m = menu();
        m.next();
        assert_eq!(m.highlighted, Some(0));
        m.next();
        m.next();
        assert_eq!(m.highlighted, Some(2));
        m.next();
        assert_eq!(
            m.highlighted,
            Some(0),
            "skips the disabled Delete and wraps"
        );
        m.previous();
        assert_eq!(m.highlighted, Some(2));

        let mut fresh = menu();
        fresh.previous();
        assert_eq!(
            fresh.highlighted,
            Some(2),
            "up from nothing starts at the bottom"
        );
    }

    #[test]
    fn enter_and_clicks_choose_enabled_items_only() {
        let mut m = menu();
        assert_eq!(m.activate(None), None);
        m.next();
        assert_eq!(m.activate(None).as_deref(), Some("open"));
        assert_eq!(m.activate(Some(1)).as_deref(), Some("pin"));
        assert_eq!(m.activate(Some(3)), None);
        m.hover(3);
        assert_eq!(m.highlighted, Some(0), "disabled items aren't highlighted");
    }

    #[test]
    fn headers_are_skipped() {
        let mut m = MenuModel::new(vec![
            MenuItem::header("LOCAL"),
            MenuItem::new("main", "main"),
            MenuItem::header("REMOTE"),
            MenuItem::new("next", "next"),
        ]);
        m.next();
        assert_eq!(m.highlighted, Some(1));
        m.next();
        assert_eq!(m.highlighted, Some(3));
        assert_eq!(m.activate(Some(0)), None);
    }

    #[test]
    fn an_empty_or_all_disabled_menu_highlights_nothing() {
        let mut m = MenuModel::new(vec![MenuItem::new("x", "X").disabled(true)]);
        m.next();
        assert_eq!(m.highlighted, None);
        let mut empty = MenuModel::default();
        empty.previous();
        assert_eq!(empty.highlighted, None);
    }
}
