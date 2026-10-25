//! The `?` shortcut list (KEY-07, d0097; design `s02-24`…`s02-27`): an overlay over the window with every action that
//! has a key, in the user's keys, in four groups drawn as three columns. A row holds one action, or the actions that
//! share its title (Next / previous ticket, Project 1–9); its keys are joined by "or", a sequence's two keys by
//! "then". A row with a changed key is drawn in the accent tint with "changed"; with the letters off, a key with no
//! modifier is left out of a row that has another, and a row with only such keys stays, dimmed, with "off". Keyless
//! actions aren't listed.

use serde::Serialize;
use ts_rs::TS;

use crate::keymap::{Binding, Key, KeyAction, KeyGroup, Modifiers, action_infos};

pub const TITLE: &str = "Keyboard shortcuts";
/// Opens Settings on Keyboard shortcuts.
pub const SETTINGS_LINK: &str = "Change shortcuts in Settings →";
/// After the Esc key in the head.
pub const CLOSE: &str = "close";
pub const NOTE: &str = "Single letters work while you aren’t typing in a field, the editor, Noyan’s box \
                        or a terminal.";
pub const NOTE_OFF: &str =
    "Single letters (j, /, g then b…) are off. Turn them on in Settings → Keyboard shortcuts.";
pub const CHANGED: &str = "changed";
pub const OFF: &str = "off";
/// Between an action's keys ("⌘N or c").
pub const OR: &str = "or";
/// Between a sequence's two keys ("g then b").
pub const THEN: &str = "then";

/// The groups in their columns, as drawn: Moving around over The terminal, then Views, then The ticket.
pub const COLUMNS: [&[KeyGroup]; 3] = [
    &[KeyGroup::MovingAround, KeyGroup::TheTerminal],
    &[KeyGroup::Views],
    &[KeyGroup::TheTicket],
];

/// The whole list. `ShortcutListView` on the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "ShortcutListView")]
pub struct ShortcutList {
    pub title: &'static str,
    pub settings_link: &'static str,
    pub close: &'static str,
    /// The note under the head: where single letters work, or that they are off.
    pub note: &'static str,
    pub or: &'static str,
    pub then: &'static str,
    /// The three columns of groups ([`COLUMNS`]).
    pub columns: Vec<Vec<ShortcutGroup>>,
}

/// One group with its rows. `ShortcutGroup` on the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
pub struct ShortcutGroup {
    pub group: KeyGroup,
    pub title: &'static str,
    pub rows: Vec<ShortcutRow>,
}

/// One row: the action's title and its keys. `ShortcutRow` on the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
pub struct ShortcutRow {
    pub title: &'static str,
    pub keys: Vec<ShortcutKey>,
    /// One of its keys is the user's (drawn: the row's keys in the accent tint).
    pub changed: bool,
    /// The letters are off and the row has only keys without a modifier (drawn dimmed).
    pub off: bool,
    /// "changed" or "off" after the keys.
    pub mark: Option<&'static str>,
}

/// One key of a row, as Kbds: one ("⌘K", "⌘] / ⌘[", "1–4"), or a sequence's two ("g", "b") shown with "then" (a
/// run's sequences share the "g": "g", "o / s / f / a").
/// `ShortcutKey` on the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
pub struct ShortcutKey {
    pub kbds: Vec<String>,
}

/// One key of a row, before it's shown: what the [`keyboard_map`](super::keyboard_map) also draws.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RowKey {
    pub kbds: Vec<String>,
    /// A key with no modifier: a single letter, a `g` sequence, ↵ or Esc (the single-letter keys of 0097's table).
    pub plain: bool,
    pub custom: bool,
}

/// No modifier: what the single-letter switch turns off (0097's single-letter table has `↵` / `Esc` too).
fn is_plain(key: &Key) -> bool {
    key.stroke()
        .is_none_or(|s| s.modifiers == Modifiers::default())
}

/// The keys of a row of `members` (one action, a pair, or a numbered run), one per slot, from `bindings` (the ones
/// shown in this place, at most one per action and slot). The members' keys of one slot are one Kbd: "⌘] / ⌘[",
/// "↵ / Esc", or a numbered run's range ("⌘1–9", "1–6") when each member has its number.
pub(crate) fn row_keys(members: &[KeyAction], bindings: &[&Binding]) -> Vec<RowKey> {
    let slots = bindings
        .iter()
        .filter(|b| members.contains(&b.action))
        .map(|b| usize::from(b.slot) + 1)
        .max()
        .unwrap_or(0);
    let mut keys = Vec::new();
    for slot in 0..slots {
        let parts: Vec<&Binding> = members
            .iter()
            .filter_map(|a| {
                bindings
                    .iter()
                    .find(|b| b.action == *a && usize::from(b.slot) == slot)
                    .copied()
            })
            .collect();
        let Some(first) = parts.first() else {
            continue;
        };
        let thens: Vec<&str> = parts
            .iter()
            .filter_map(|b| match &b.key {
                Key::Sequence { then } => Some(then.as_str()),
                _ => None,
            })
            .collect();
        let kbds = match (&first.key, parts.len()) {
            (Key::Sequence { then }, 1) => vec!["g".to_owned(), then.clone()],
            // The sheet's tabs: "g" then "o / s / f / a" (`s02-24`).
            _ if thens.len() == parts.len() => vec!["g".to_owned(), thens.join(" / ")],
            _ => vec![range(&parts, members.len()).unwrap_or_else(|| {
                parts
                    .iter()
                    .map(|b| b.text.as_str())
                    .collect::<Vec<_>>()
                    .join(" / ")
            })],
        };
        keys.push(RowKey {
            kbds,
            plain: parts.iter().all(|b| is_plain(&b.key)),
            custom: parts.iter().any(|b| b.custom),
        });
    }
    keys
}

/// "⌘1–9" for ⌘1…⌘9: every member present, each the same text but for its number at the end.
fn range(parts: &[&Binding], members: usize) -> Option<String> {
    if members < 3 || parts.len() != members {
        return None;
    }
    let prefix = parts[0].text.strip_suffix('1')?;
    parts
        .iter()
        .enumerate()
        .all(|(i, b)| b.text.strip_prefix(prefix) == Some(&(i + 1).to_string()))
        .then(|| format!("{prefix}1–{members}"))
}

/// The bindings a list shows: those outside terminals: on Windows/Linux an action whose keys work only inside
/// a terminal (Find in terminal scrollback, Ctrl+Shift+F) shows those.
fn shown_bindings(bindings: &[Binding]) -> Vec<&Binding> {
    bindings
        .iter()
        .filter(|b| {
            b.place.outside_terminals()
                || !bindings
                    .iter()
                    .any(|o| o.action == b.action && o.place.outside_terminals())
        })
        .collect()
}

/// The listed actions in rows: one row per title, in the map's order ([`action_infos`]), with its group.
pub(crate) fn rows_of_actions() -> Vec<(KeyGroup, &'static str, Vec<KeyAction>)> {
    let mut rows: Vec<(KeyGroup, &'static str, Vec<KeyAction>)> = Vec::new();
    for info in action_infos() {
        match rows.iter_mut().find(|(_, title, _)| *title == info.title) {
            Some((_, _, members)) => members.push(info.action),
            None => rows.push((info.group, info.title, vec![info.action])),
        }
    }
    rows
}

/// **The `?` list** for the user's keys (`bindings`, [`crate::keymap::user_keys`], whose texts are the platform's)
/// and the letters switch.
pub fn shortcut_list(bindings: &[Binding], letters: bool) -> ShortcutList {
    let shown = shown_bindings(bindings);
    let rows = rows_of_actions();
    let group = |group: KeyGroup| ShortcutGroup {
        group,
        title: group.title(),
        rows: rows
            .iter()
            .filter(|(g, _, _)| *g == group)
            .filter_map(|(_, title, members)| row(title, &row_keys(members, &shown), letters))
            .collect(),
    };
    ShortcutList {
        title: TITLE,
        settings_link: SETTINGS_LINK,
        close: CLOSE,
        note: if letters { NOTE } else { NOTE_OFF },
        or: OR,
        then: THEN,
        columns: COLUMNS
            .iter()
            .map(|groups| groups.iter().map(|g| group(*g)).collect())
            .collect(),
    }
}

/// A row, or `None` for an action with no key.
fn row(title: &'static str, keys: &[RowKey], letters: bool) -> Option<ShortcutRow> {
    if keys.is_empty() {
        return None;
    }
    let with_modifier: Vec<&RowKey> = keys.iter().filter(|k| !k.plain).collect();
    let (shown, off): (Vec<&RowKey>, bool) = if letters {
        (keys.iter().collect(), false)
    } else if with_modifier.is_empty() {
        (keys.iter().collect(), true)
    } else {
        (with_modifier, false)
    };
    let changed = !off && shown.iter().any(|k| k.custom);
    Some(ShortcutRow {
        title,
        keys: shown
            .into_iter()
            .map(|k| ShortcutKey {
                kbds: k.kbds.clone(),
            })
            .collect(),
        changed,
        off,
        mark: if off {
            Some(OFF)
        } else if changed {
            Some(CHANGED)
        } else {
            None
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keymap::{CustomKey, CustomKeys, KeyChange, Platform, bindings, user_keys};

    type Drawn = Vec<(&'static str, Vec<Vec<String>>, Option<&'static str>)>;

    fn drawn(list: &ShortcutList, group: KeyGroup) -> Drawn {
        list.columns
            .iter()
            .flatten()
            .find(|g| g.group == group)
            .unwrap()
            .rows
            .iter()
            .map(|r| {
                (
                    r.title,
                    r.keys.iter().map(|k| k.kbds.clone()).collect(),
                    r.mark,
                )
            })
            .collect()
    }

    fn keys(list: &[&[&str]]) -> Vec<Vec<String>> {
        list.iter()
            .map(|k| k.iter().map(|s| (*s).to_owned()).collect())
            .collect()
    }

    fn mac(custom: &CustomKeys, letters: bool) -> ShortcutList {
        shortcut_list(&user_keys(Platform::Mac, custom), letters)
    }

    /// `s02-24`: the four groups in three columns, every row with its keys as drawn.
    #[test]
    fn the_groups_and_rows_are_the_drawn_ones() {
        let list = mac(&CustomKeys::default(), true);
        assert_eq!(list.title, "Keyboard shortcuts");
        assert_eq!(list.settings_link, "Change shortcuts in Settings →");
        assert_eq!(
            list.note,
            "Single letters work while you aren’t typing in a field, the editor, Noyan’s box or a terminal."
        );
        let titles: Vec<Vec<&str>> = list
            .columns
            .iter()
            .map(|c| c.iter().map(|g| g.title).collect())
            .collect();
        assert_eq!(
            titles,
            [
                vec!["Moving around", "The terminal"],
                vec!["Views"],
                vec!["The ticket"]
            ]
        );
        assert_eq!(
            drawn(&list, KeyGroup::MovingAround),
            [
                ("Command palette", keys(&[&["⌘K"], &["⇧⌘P"]]), None),
                ("Switch project", keys(&[&["⌘P"]]), None),
                ("Project 1–9", keys(&[&["⌘1–9"]]), None),
                ("Next item that needs you", keys(&[&["⌘J"]]), None),
                (
                    "Next / previous ticket",
                    keys(&[&["⌘] / ⌘["], &["j / k"]]),
                    None
                ),
                ("Filter the board", keys(&[&["/"]]), None),
                (
                    "Focus the epic of the selected card",
                    keys(&[&["g", "e"]]),
                    None
                ),
                ("Show all shortcuts", keys(&[&["?"]]), None),
            ]
        );
        assert_eq!(
            drawn(&list, KeyGroup::TheTerminal),
            [
                (
                    "Terminals: open, close, leave the terminal",
                    keys(&[&["⌃`"], &["g", "t"]]),
                    None
                ),
                ("Find in terminal scrollback", keys(&[&["⌘F"]]), None),
            ]
        );
        assert_eq!(
            drawn(&list, KeyGroup::Views),
            [
                ("Board", keys(&[&["⇧⌘B"], &["g", "b"]]), None),
                ("List", keys(&[&["g", "l"]]), None),
                ("Files", keys(&[&["⇧⌘O"], &["g", "f"]]), None),
                ("Pause / Resume agents", keys(&[&["⇧⌘."]]), None),
                ("Activity", keys(&[&["⇧⌘U"], &["g", "a"]]), None),
                ("Settings", keys(&[&["⌘,"]]), None),
                ("Open / close Noyan", keys(&[&["⌘L"]]), None),
                ("New conversation with Noyan", keys(&[&["⇧⌘L"]]), None),
            ]
        );
        assert_eq!(
            drawn(&list, KeyGroup::TheTicket),
            [
                ("New ticket", keys(&[&["⌘N"], &["c"]]), None),
                ("Open / close the ticket", keys(&[&["↵ / Esc"]]), None),
                ("Full page / back to the sheet", keys(&[&["e"]]), None),
                (
                    "Ticket tabs",
                    keys(&[&["1–4"], &["g", "o / s / f / a"]]),
                    None
                ),
                ("Go to file", keys(&[&["⌥⌘P"]]), None),
                (
                    "Next / previous ticket tab",
                    keys(&[&["⌃Tab / ⌃⇧Tab"]]),
                    None
                ),
                ("Approve ▾", keys(&[&["⌘↵"]]), None),
                ("Send back\u{2026}", keys(&[&["⌘⌫"]]), None),
                ("Start / move to Dev", keys(&[&["⌘D"]]), None),
            ]
        );
    }

    fn replaced(action: KeyAction, slot: u8, key: &str) -> CustomKey {
        CustomKey {
            action,
            slot,
            change: KeyChange::Replaced {
                key: Key::parse(key).unwrap(),
            },
            works_in_terminals: false,
        }
    }

    fn removed(action: KeyAction, slot: u8) -> CustomKey {
        CustomKey {
            action,
            slot,
            change: KeyChange::Removed,
            works_in_terminals: false,
        }
    }

    fn row_of<'a>(list: &'a ShortcutList, title: &str) -> Option<&'a ShortcutRow> {
        list.columns
            .iter()
            .flatten()
            .flat_map(|g| &g.rows)
            .find(|r| r.title == title)
    }

    /// `s02-24`'s second card (Pause / Resume agents on ⌘I, New ticket's letter n) and the ticket's example (Activity on
    /// ⌘J, which takes it from Next item that needs you): a changed row says "changed"; an action left with no key
    /// leaves the list.
    #[test]
    fn a_changed_key_is_marked() {
        let list = mac(
            &CustomKeys(vec![
                replaced(KeyAction::Pause, 0, "⌘I"),
                replaced(KeyAction::NewTicket, 1, "n"),
            ]),
            true,
        );
        let pause = row_of(&list, "Pause / Resume agents").unwrap();
        assert_eq!(pause.keys[0].kbds, ["⌘I"]);
        assert!(pause.changed);
        assert_eq!(pause.mark, Some("changed"));
        let new_ticket = row_of(&list, "New ticket").unwrap();
        let shown: Vec<&Vec<String>> = new_ticket.keys.iter().map(|k| &k.kbds).collect();
        assert_eq!(shown, [&["⌘N"], &["n"]]);
        assert_eq!(new_ticket.mark, Some("changed"));
        assert_eq!(row_of(&list, "Board").unwrap().mark, None);

        let list = mac(
            &CustomKeys(vec![
                replaced(KeyAction::OpenActivity, 0, "⌘J"),
                removed(KeyAction::NextNeedsYouItem, 0),
            ]),
            true,
        );
        let activity = row_of(&list, "Activity").unwrap();
        assert_eq!(activity.keys[0].kbds, ["⌘J"]);
        assert_eq!(activity.mark, Some("changed"));
        assert!(row_of(&list, "Next item that needs you").is_none());
    }

    /// `s02-26`: with the letters off, rows with a modifier key show just that key; rows with only letters stay,
    /// dimmed and marked "off" (↵ / Esc too, as drawn and as 0097's single-letter table lists them).
    #[test]
    fn letters_off() {
        let list = mac(&CustomKeys::default(), false);
        assert_eq!(list.note, NOTE_OFF);
        assert_eq!(
            drawn(&list, KeyGroup::MovingAround),
            [
                ("Command palette", keys(&[&["⌘K"], &["⇧⌘P"]]), None),
                ("Switch project", keys(&[&["⌘P"]]), None),
                ("Project 1–9", keys(&[&["⌘1–9"]]), None),
                ("Next item that needs you", keys(&[&["⌘J"]]), None),
                ("Next / previous ticket", keys(&[&["⌘] / ⌘["]]), None),
                ("Filter the board", keys(&[&["/"]]), Some("off")),
                (
                    "Focus the epic of the selected card",
                    keys(&[&["g", "e"]]),
                    Some("off")
                ),
                ("Show all shortcuts", keys(&[&["?"]]), Some("off")),
            ]
        );
        assert_eq!(
            drawn(&list, KeyGroup::TheTerminal)[0],
            (
                "Terminals: open, close, leave the terminal",
                keys(&[&["⌃`"]]),
                None
            )
        );
        let ticket = drawn(&list, KeyGroup::TheTicket);
        assert_eq!(ticket[0], ("New ticket", keys(&[&["⌘N"]]), None));
        assert_eq!(
            ticket[1],
            (
                "Open / close the ticket",
                keys(&[&["↵ / Esc"]]),
                Some("off")
            )
        );
        assert_eq!(ticket[2].2, Some("off"));
        assert_eq!(
            ticket[3],
            (
                "Ticket tabs",
                keys(&[&["1–4"], &["g", "o / s / f / a"]]),
                Some("off")
            )
        );
        let filter = row_of(&list, "Filter the board").unwrap();
        assert!(filter.off && !filter.changed);
        // A changed letter hidden by the switch doesn't mark its row.
        let list = mac(
            &CustomKeys(vec![replaced(KeyAction::NewTicket, 1, "n")]),
            false,
        );
        assert_eq!(row_of(&list, "New ticket").unwrap().mark, None);
    }

    /// Park, Close ticket, Open PR, Re-run validation and Open diff have no key: not listed until the user gives one.
    #[test]
    fn no_keyless_rows() {
        let titles = |list: &ShortcutList| -> Vec<&str> {
            list.columns
                .iter()
                .flatten()
                .flat_map(|g| g.rows.iter().map(|r| r.title))
                .collect()
        };
        for platform in [Platform::Mac, Platform::WinLinux] {
            let list = shortcut_list(&bindings(platform), true);
            for title in [
                "Park",
                "Close ticket",
                "Open PR",
                "Re-run validation",
                "Open diff",
            ] {
                assert!(!titles(&list).contains(&title), "{title}");
            }
            assert_eq!(titles(&list).len(), 27);
        }
        let list = mac(&CustomKeys(vec![replaced(KeyAction::Park, 0, "g p")]), true);
        let park = row_of(&list, "Park").unwrap();
        assert_eq!(park.keys[0].kbds, ["g", "p"]);
        assert_eq!(park.mark, Some("changed"));
        assert!(
            drawn(&list, KeyGroup::TheTicket)
                .iter()
                .any(|r| r.0 == "Park")
        );
        // An action whose every key was removed leaves the list.
        let list = mac(&CustomKeys(vec![removed(KeyAction::Settings, 0)]), true);
        assert!(row_of(&list, "Settings").is_none());
    }

    /// Windows/Linux: the keys outside terminals; Find in terminal scrollback, which has a key only inside one, shows
    /// that key.
    #[test]
    fn windows_and_linux_show_the_keys_outside_terminals() {
        let list = shortcut_list(&bindings(Platform::WinLinux), true);
        let shown = |title| -> Vec<Vec<String>> {
            row_of(&list, title)
                .unwrap()
                .keys
                .iter()
                .map(|k| k.kbds.clone())
                .collect()
        };
        assert_eq!(shown("New ticket"), keys(&[&["Ctrl+N"], &["c"]]));
        assert_eq!(
            shown("Next / previous ticket"),
            keys(&[&["Ctrl+] / Ctrl+["], &["j / k"]])
        );
        assert_eq!(shown("Project 1–9"), keys(&[&["Ctrl+1–9"]]));
        assert_eq!(
            shown("Find in terminal scrollback"),
            keys(&[&["Ctrl+Shift+F"]])
        );
        assert_eq!(shown("Board"), keys(&[&["Ctrl+Shift+B"], &["g", "b"]]));
    }

    /// A numbered run with one member changed shows each member's key, joined, and is marked.
    #[test]
    fn a_numbered_run_with_a_changed_member() {
        let list = mac(
            &CustomKeys(vec![replaced(KeyAction::Project(3), 0, "⌥⌘3")]),
            true,
        );
        let project = row_of(&list, "Project 1–9").unwrap();
        assert_eq!(
            project.keys[0].kbds,
            ["⌘1 / ⌘2 / ⌥⌘3 / ⌘4 / ⌘5 / ⌘6 / ⌘7 / ⌘8 / ⌘9"]
        );
        assert_eq!(project.mark, Some("changed"));
    }
}
