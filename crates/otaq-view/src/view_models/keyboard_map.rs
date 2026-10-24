//! The keyboard map as the design shows it (*01 · Components → Keyboard map (update AF · 0097)*): the single letters
//! (the same on every platform), then the modifier keys in three columns, macOS, Windows / Linux, and Windows / Linux
//! while a terminal has focus, then three notes. Titles and keys come from the keymap ([`KeyAction::title`], the base
//! map), so the map shown is the map bound. This is the gallery's map of the defaults, not the `?` list ([`super::shortcut_list`]), which shows the user's keys.

use serde::Serialize;
use ts_rs::TS;

use super::shortcut_list::{ShortcutKey, row_keys};
use crate::keymap::{Binding, KeyAction, Platform, bindings};

pub const LETTERS_TITLE: &str = "Single letters";
pub const LETTERS_NOTE: &str = "all platforms · while not typing";
pub const MODIFIERS_TITLE: &str = "Modifier keys";
pub const COLUMNS: [&str; 3] = [
    "macOS",
    "Windows / Linux",
    "In a terminal (Windows / Linux)",
];
/// Between an action's keys, and between a sequence's two.
pub const OR: &str = "or";
pub const THEN: &str = "then";

/// The notes under the map, as drawn: the first stands out.
pub const NOTES: [&str; 3] = [
    "Single letters work on every platform, only while nobody is typing (a field, the description editor, the Project \
     agent’s box or a terminal) and no menu or dialog is open; Settings → Keyboard shortcuts can turn them off. “—” \
     means no key there: the command palette covers it. In the description editor ⌘K inserts a link and ⌘B is Bold.",
    "While a terminal has focus every key goes to the shell or agent. On macOS ⌘ keys keep working (the terminal \
     doesn’t use ⌘). On Windows and Linux most of the app’s keys move to Ctrl+Shift+… inside a terminal (the third \
     column); Ctrl+1–9, Ctrl+`, Ctrl+, , Ctrl+Tab, Ctrl+K and Ctrl+P stay as they are. ⌃` / Ctrl+` opens, closes and \
     leaves the terminal.",
    "Update AO (0087): the Files view is back as the third view, ⇧⌘O or g f (g f is the Files tab inside a ticket), \
     and ⌥⌘P is Go to file in both. Replaces 0058’s map (decision 0097). Open diff has no key (the palette runs it); ⌘D is Start / move to Dev. ⇧⌘T → ⌃`, ⇧⌘A → ⇧⌘L, ⇧⌘E → e, ⌘⌥1–6 → \
     1–6, ⇧⌘G → g e, ⇧⌘D → ⌘D, ⌘K search → / (⌘K is the command palette).",
];

/// One single-letter row: its name and its key. `KeyMapLetterRow` on the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "KeyMapLetterRow")]
pub struct LetterRow {
    pub label: &'static str,
    pub key: ShortcutKey,
}

/// One modifier-key row: the action's title and its keys per column; none is "—". `KeyMapRow` on the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "KeyMapRow")]
pub struct KeyRow {
    pub label: &'static str,
    pub mac: Vec<ShortcutKey>,
    pub win_linux: Vec<ShortcutKey>,
    pub in_terminal: Vec<ShortcutKey>,
}

/// The whole map. `KeyMapView` on the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "KeyMapView")]
pub struct KeyMap {
    pub letters_title: &'static str,
    pub letters_note: &'static str,
    pub letters: Vec<LetterRow>,
    pub modifiers_title: &'static str,
    pub columns: [&'static str; 3],
    pub modifiers: Vec<KeyRow>,
    pub or: &'static str,
    pub then: &'static str,
    pub notes: [&'static str; 3],
}

use KeyAction::*;

/// The single letters in the drawn order, with the drawn names where they say more than the title (where they work,
/// "Go to").
const LETTERS: [(Option<&str>, &[KeyAction]); 12] = [
    (
        Some("Next / previous ticket (panel)"),
        &[NextTicket, PreviousTicket],
    ),
    (
        Some("Ticket tabs (sheet open)"),
        &[SwitchTab(1), SwitchTab(2), SwitchTab(3), SwitchTab(4)],
    ),
    (None, &[FilterBoard]),
    (None, &[ShowShortcuts]),
    (None, &[ExpandSheet]),
    (Some("Go to Board"), &[ShowBoard]),
    (Some("Go to List"), &[ShowList]),
    (Some("Go to Activity"), &[OpenActivity]),
    (Some("Go to Terminals"), &[Terminals]),
    (None, &[FocusEpic]),
    (None, &[NewTicket]),
    (None, &[OpenSheet, CloseSheet]),
];

/// The modifier keys in the drawn order (0097's base map table).
const MODIFIERS: [&[KeyAction]; 20] = [
    &[CommandPalette],
    &[SwitchProject],
    &[
        Project(1),
        Project(2),
        Project(3),
        Project(4),
        Project(5),
        Project(6),
        Project(7),
        Project(8),
        Project(9),
    ],
    &[NewTicket],
    &[Settings],
    &[FocusAgentPanel],
    &[NewProjectAgent],
    &[Terminals],
    &[NextNeedsYouItem],
    &[NextTab, PreviousTab],
    &[NextTicket, PreviousTicket],
    &[ShowBoard],
    &[OpenActivity],
    &[Approve],
    &[SendBack],
    &[StartDev],
    &[FindInTerminal],
    // 0097's Stop all row is Pause's (d0112), before the Files view's two.
    &[Pause],
    &[ShowFiles],
    &[GoToFile],
];

/// The keys of `members` among `bindings`: the plain ones (letters, sequences, ↵ / Esc) or the modifier ones.
fn keys(members: &[KeyAction], bindings: &[&Binding], plain: bool) -> Vec<ShortcutKey> {
    row_keys(members, bindings)
        .into_iter()
        .filter(|k| k.plain == plain)
        .map(|k| ShortcutKey { kbds: k.kbds })
        .collect()
}

/// The modifier row's name where the map draws another than the title: "Files view" (ds-01, update AO).
fn modifier_label(action: KeyAction) -> &'static str {
    match action {
        ShowFiles => "Files view",
        other => other.title(),
    }
}

/// The map of the base keys.
pub fn map() -> KeyMap {
    let mac = bindings(Platform::Mac);
    let win = bindings(Platform::WinLinux);
    let mac: Vec<&Binding> = mac.iter().collect();
    let outside: Vec<&Binding> = win.iter().filter(|b| b.place.outside_terminals()).collect();
    let inside: Vec<&Binding> = win.iter().filter(|b| b.place.inside_terminals()).collect();
    let letters = LETTERS
        .iter()
        .filter_map(|(label, members)| {
            let key = keys(members, &mac, true).into_iter().next()?;
            Some(LetterRow {
                label: label.unwrap_or(members[0].title()),
                key,
            })
        })
        .collect();
    KeyMap {
        letters_title: LETTERS_TITLE,
        letters_note: LETTERS_NOTE,
        letters,
        modifiers_title: MODIFIERS_TITLE,
        columns: COLUMNS,
        modifiers: MODIFIERS
            .iter()
            .map(|members| KeyRow {
                label: modifier_label(members[0]),
                mac: keys(members, &mac, false),
                win_linux: keys(members, &outside, false),
                in_terminal: keys(members, &inside, false),
            })
            .collect(),
        or: OR,
        then: THEN,
        notes: NOTES,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kbds(keys: &[ShortcutKey]) -> Vec<Vec<&str>> {
        keys.iter()
            .map(|k| k.kbds.iter().map(String::as_str).collect())
            .collect()
    }

    /// ds-01's Single letters table, in its order and words.
    #[test]
    fn the_single_letters_are_the_drawn_ones() {
        let map = map();
        assert_eq!(
            (map.letters_title, map.letters_note),
            ("Single letters", "all platforms · while not typing")
        );
        let shown: Vec<(&str, Vec<&str>)> = map
            .letters
            .iter()
            .map(|r| (r.label, r.key.kbds.iter().map(String::as_str).collect()))
            .collect();
        assert_eq!(
            shown,
            [
                ("Next / previous ticket (panel)", vec!["j / k"]),
                ("Ticket tabs (sheet open)", vec!["1–4"]),
                ("Filter the board", vec!["/"]),
                ("Show all shortcuts", vec!["?"]),
                ("Full page / back to the sheet", vec!["e"]),
                ("Go to Board", vec!["g", "b"]),
                ("Go to List", vec!["g", "l"]),
                ("Go to Activity", vec!["g", "a"]),
                ("Go to Terminals", vec!["g", "t"]),
                ("Focus the epic of the selected card", vec!["g", "e"]),
                ("New ticket", vec!["c"]),
                ("Open / close the ticket", vec!["↵ / Esc"]),
            ]
        );
    }

    type Row<'a> = (
        &'a str,
        Vec<Vec<&'a str>>,
        Vec<Vec<&'a str>>,
        Vec<Vec<&'a str>>,
    );

    /// ds-01's Modifier keys table: every row with its three columns as drawn, the Files view's two at the end.
    #[test]
    fn the_modifier_keys_are_the_drawn_ones() {
        let map = map();
        assert_eq!(map.modifiers_title, "Modifier keys");
        assert_eq!(
            map.columns,
            [
                "macOS",
                "Windows / Linux",
                "In a terminal (Windows / Linux)"
            ]
        );
        let shown: Vec<Row> = map
            .modifiers
            .iter()
            .map(|r| {
                (
                    r.label,
                    kbds(&r.mac),
                    kbds(&r.win_linux),
                    kbds(&r.in_terminal),
                )
            })
            .collect();
        let one = |k: &'static str| vec![vec![k]];
        let none = Vec::<Vec<&str>>::new;
        let row = |label, mac, win, term| -> Row { (label, one(mac), one(win), one(term)) };
        assert_eq!(
            shown,
            [
                (
                    "Command palette",
                    vec![vec!["⌘K"], vec!["⇧⌘P"]],
                    vec![vec!["Ctrl+K"], vec!["Ctrl+Shift+P"]],
                    vec![vec!["Ctrl+K"], vec!["Ctrl+Shift+P"]],
                ),
                row("Switch project", "⌘P", "Ctrl+P", "Ctrl+P"),
                row("Project 1–9", "⌘1–9", "Ctrl+1–9", "Ctrl+1–9"),
                row("New ticket", "⌘N", "Ctrl+N", "Ctrl+Shift+N"),
                row("Settings", "⌘,", "Ctrl+,", "Ctrl+,"),
                row(
                    "Open / close the Project agent",
                    "⌘L",
                    "Ctrl+L",
                    "Ctrl+Shift+L"
                ),
                (
                    "New Project agent conversation",
                    one("⇧⌘L"),
                    one("Ctrl+Shift+L"),
                    none()
                ),
                row(
                    "Terminals: open, close, leave the terminal",
                    "⌃`",
                    "Ctrl+`",
                    "Ctrl+`"
                ),
                row("Next item that needs you", "⌘J", "Ctrl+J", "Ctrl+Shift+J"),
                row(
                    "Next / previous ticket tab",
                    "⌃Tab / ⌃⇧Tab",
                    "Ctrl+Tab / Ctrl+Shift+Tab",
                    "Ctrl+Tab / Ctrl+Shift+Tab"
                ),
                row(
                    "Next / previous ticket",
                    "⌘] / ⌘[",
                    "Ctrl+] / Ctrl+[",
                    "Ctrl+Shift+] / Ctrl+Shift+["
                ),
                ("Board", one("⇧⌘B"), one("Ctrl+Shift+B"), none()),
                row("Activity", "⇧⌘U", "Ctrl+Shift+U", "Ctrl+Shift+U"),
                row("Approve ▾", "⌘↵", "Ctrl+↵", "Ctrl+Shift+↵"),
                row("Send back", "⌘⌫", "Ctrl+⌫", "Ctrl+Shift+⌫"),
                row("Start / move to Dev", "⌘D", "Ctrl+D", "Ctrl+Shift+D"),
                (
                    "Find in terminal scrollback",
                    one("⌘F"),
                    none(),
                    one("Ctrl+Shift+F")
                ),
                row(
                    "Pause / Resume agents",
                    "⇧⌘.",
                    "Ctrl+Shift+.",
                    "Ctrl+Shift+."
                ),
                row("Files view", "⇧⌘O", "Ctrl+Shift+O", "Ctrl+Shift+O"),
                row("Go to file", "⌥⌘P", "Ctrl+Alt+P", "Ctrl+Shift+Alt+P"),
            ]
        );
        assert!(NOTES[0].starts_with("Single letters work on every platform"));
        assert!(NOTES[2].starts_with("Update AO (0087): the Files view is back"));
    }

    /// Every action with a key is in the map: the keyless ones (the palette runs them) aren't.
    #[test]
    fn every_action_with_a_key_is_shown() {
        let listed: Vec<KeyAction> = LETTERS
            .iter()
            .flat_map(|(_, m)| m.iter())
            .chain(MODIFIERS.iter().flat_map(|m| m.iter()))
            .copied()
            .collect();
        for action in KeyAction::all() {
            assert_eq!(
                listed.contains(&action),
                !action.base_keys().is_empty(),
                "{action:?}"
            );
        }
    }
}
