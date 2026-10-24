//! The empty board of a project with no tickets yet (UI-08, SHL-06; *14 · First run → 4 · Done · empty board*,
//! *03 → Empty column & empty board*): when it shows, its button and what it asks of the shell. Since d0087 the
//! Project agent pane is always beside the board, so the card points at it (and at ⌘L) instead of offering a button.

use serde::Serialize;
use ts_rs::TS;

use crate::icons::IconName;
use crate::key_labels::KeyLabels;
use crate::keymap::KeyAction;
use crate::shell::ShellRequest;

/// Whether the project's empty board shows instead of the board. New ticket's title field sits at the top of
/// Backlog (*08 · Ticket editor → Quick-create*), so the board shows while it is open; Esc with no ticket brings
/// the empty board back.
pub fn shows(tickets: usize, quick_creating: bool) -> bool {
    tickets == 0 && !quick_creating
}

/// The empty board's button.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Action {
    pub id: &'static str,
    pub label: &'static str,
    pub icon: IconName,
    pub primary: bool,
    pub request: ShellRequest,
}

/// "New ticket" (⌘N): the same request as the header's button.
pub fn actions() -> [Action; 1] {
    [Action {
        id: "empty-new-ticket",
        label: "New ticket",
        icon: IconName::Plus,
        primary: true,
        request: ShellRequest::NewTicket,
    }]
}

/// A button of the empty board as the page draws it: its shortcut action is what a click runs, so the button and
/// the key do the same thing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "EmptyBoardButton")]
pub struct Button {
    pub label: &'static str,
    pub icon: IconName,
    pub primary: bool,
    pub action: KeyAction,
}

/// "⌘N new ticket" under the button: a button too, running the action its key runs (d0126: a hint that looked like a
/// link but wasn't one left the keyboard on the board, finding 5 of the 2026-10-24 dogfood report).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "EmptyBoardHint")]
pub struct Hint {
    pub keys: Option<String>,
    pub label: &'static str,
    pub action: KeyAction,
}

/// The empty board's centred card (*14 → 4 · Done · empty board*; *03 → Empty column & empty board*): New ticket, the
/// text and a hint pointing at the pane's Project agent (d0068, d0087).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "EmptyBoardView")]
pub struct View {
    pub title: String,
    pub text: String,
    pub buttons: Vec<Button>,
    pub hints: Vec<Hint>,
}

/// "shop-web is ready" for the project `name`. The hints name the user's keys; a hint whose action has no key is left
/// out (KEY-08).
pub fn view(name: &str, keys: &KeyLabels) -> View {
    View {
        title: format!("{name} is ready"),
        text: "Tell the Project agent on the left what you want: it reads the code and writes the tickets. Or write the first one yourself."
            .to_owned(),
        buttons: actions()
            .iter()
            .map(|a| Button {
                label: a.label,
                icon: a.icon,
                primary: a.primary,
                action: KeyAction::NewTicket,
            })
            .collect(),
        hints: [
            (KeyAction::NewTicket, "new ticket"),
            (KeyAction::FocusPane, "ask the Project agent"),
        ]
        .into_iter()
        .filter_map(|(action, label)| {
            keys.label(action).map(|keys| Hint {
                keys: Some(keys),
                label,
                action,
            })
        })
        .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::key_labels::fixtures;
    use crate::keymap::Platform;

    #[test]
    fn new_ticket_leaves_the_empty_board_while_its_title_field_is_open() {
        assert!(shows(0, false));
        assert!(!shows(0, true), "the title field is on the board");
        assert!(!shows(1, false));
        assert!(!shows(1, true));
    }

    fn hints(keys: &KeyLabels) -> Vec<(String, &'static str)> {
        view("shop-web", keys)
            .hints
            .into_iter()
            .map(|h| (h.keys.unwrap(), h.label))
            .collect()
    }

    /// *03 → Empty column & empty board* (`kItEl`), *14 → 4 · Done · empty board → Board empty* (`ugHTD`), d0087: the
    /// text and the ⌘L hint point at the pane; New ticket is the only button.
    #[test]
    fn the_page_view_names_the_project_and_runs_the_shortcuts() {
        let v = view("shop-web", &fixtures::base());
        assert_eq!(v.title, "shop-web is ready");
        assert_eq!(
            v.text,
            "Tell the Project agent on the left what you want: it reads the code and writes the tickets. Or write the first one yourself."
        );
        let actions: Vec<KeyAction> = v.buttons.iter().map(|b| b.action).collect();
        assert_eq!(actions, [KeyAction::NewTicket]);
        let labels: Vec<&str> = v.buttons.iter().map(|b| b.label).collect();
        assert_eq!(labels, ["New ticket"]);
        assert!(v.buttons[0].primary);
        assert_eq!(
            hints(&fixtures::base()),
            [
                ("⌘N".to_owned(), "new ticket"),
                ("⌘L".to_owned(), "ask the Project agent")
            ]
        );
    }

    /// d0126: each hint is a button that runs its key's action (⌘L opens the Project agent, focused).
    #[test]
    fn the_hints_run_their_keys_actions() {
        let actions: Vec<KeyAction> = view("shop-web", &fixtures::base())
            .hints
            .iter()
            .map(|h| h.action)
            .collect();
        assert_eq!(actions, [KeyAction::NewTicket, KeyAction::FocusPane]);
    }

    #[test]
    fn the_hints_use_the_platforms_keys() {
        assert_eq!(
            hints(&KeyLabels::base(Platform::WinLinux)),
            [
                ("Ctrl+N".to_owned(), "new ticket"),
                ("Ctrl+L".to_owned(), "ask the Project agent")
            ]
        );
    }

    /// KEY-08: the hints name the user's keys; New ticket's ⌘N removed leaves its `c`, and with letters off too the hint
    /// goes.
    #[test]
    fn the_hints_use_the_users_keys() {
        assert_eq!(
            hints(&fixtures::replaced("focus_pane", None, 0, "⌥L")),
            [
                ("⌘N".to_owned(), "new ticket"),
                ("⌥L".to_owned(), "ask the Project agent")
            ]
        );
        assert_eq!(
            hints(&fixtures::removed("new_ticket", None, 0)),
            [
                ("c".to_owned(), "new ticket"),
                ("⌘L".to_owned(), "ask the Project agent")
            ]
        );
        assert_eq!(hints(&fixtures::letters_off()), hints(&fixtures::base()));
        let neither = fixtures::with(false, &[("new_ticket", None, 0, None)]);
        assert_eq!(
            hints(&neither),
            [("⌘L".to_owned(), "ask the Project agent")]
        );
        assert_eq!(
            hints(&fixtures::removed("focus_pane", None, 0)),
            [("⌘N".to_owned(), "new ticket")]
        );
    }

    #[test]
    fn the_button_asks_for_a_ticket() {
        let requests: Vec<ShellRequest> = actions().into_iter().map(|a| a.request).collect();
        assert_eq!(requests, [ShellRequest::NewTicket]);
    }
}
