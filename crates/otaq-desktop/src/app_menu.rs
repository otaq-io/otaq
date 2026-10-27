//! Otaq's own macOS app menu (d0139), which replaces Tauri's default one: ⌘W closes the window (which hides it), ⌘Q
//! is Otaq's own Quit item (T-0569 asks first while agents work), and the standard Edit items stay so copy and paste
//! keep working in the webview (WKWebView takes them from the menu). The door turns these rows into native items.

/// One item of the app menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Item {
    About,
    Services,
    Hide,
    HideOthers,
    ShowAll,
    /// Otaq's own "Quit Otaq", ⌘Q ([`QUIT_ID`]).
    Quit,
    Undo,
    Redo,
    Cut,
    Copy,
    Paste,
    SelectAll,
    Minimize,
    Zoom,
    Fullscreen,
    /// ⌘W: closes the window, which hides it.
    CloseWindow,
    Separator,
}

/// The id of the Quit item's menu event.
pub const QUIT_ID: &str = "app-quit";
pub const QUIT_ACCELERATOR: &str = "CmdOrCtrl+Q";

/// The submenus, in order, with their titles (the first is the app's own, titled by macOS).
pub fn macos() -> Vec<(&'static str, Vec<Item>)> {
    use Item::*;
    vec![
        (
            otaq_core::naming::PRODUCT_NAME,
            vec![
                About, Separator, Services, Separator, Hide, HideOthers, ShowAll, Separator, Quit,
            ],
        ),
        (
            "Edit",
            vec![Undo, Redo, Separator, Cut, Copy, Paste, SelectAll],
        ),
        ("View", vec![Fullscreen]),
        ("Window", vec![Minimize, Zoom, Separator, CloseWindow]),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn items(title: &str) -> Vec<Item> {
        macos()
            .into_iter()
            .find(|(t, _)| *t == title)
            .map(|(_, items)| items)
            .unwrap_or_else(|| panic!("no {title} menu"))
    }

    /// Copy and paste in the webview need the standard Edit items (T-0566).
    #[test]
    fn the_edit_menu_keeps_the_standard_items() {
        let edit = items("Edit");
        for item in [
            Item::Undo,
            Item::Redo,
            Item::Cut,
            Item::Copy,
            Item::Paste,
            Item::SelectAll,
        ] {
            assert!(edit.contains(&item), "{item:?}");
        }
    }

    /// ⌘W closes (hides) the window; ⌘Q is Otaq's own Quit, once, in the app menu.
    #[test]
    fn close_window_and_quit_are_there_once() {
        let all: Vec<Item> = macos().into_iter().flat_map(|(_, items)| items).collect();
        assert_eq!(all.iter().filter(|i| **i == Item::CloseWindow).count(), 1);
        assert_eq!(all.iter().filter(|i| **i == Item::Quit).count(), 1);
        assert_eq!(
            macos()[0].1.last(),
            Some(&Item::Quit),
            "Quit ends the app menu"
        );
        assert!(items("Window").contains(&Item::CloseWindow));
    }
}
