//! Keyboard modality: while a modal is open (a confirm dialog, or the editor, Settings, Add project,
//! Cleanup or restart dialog over the window) the app's shortcuts (d0097) don't run, so nothing behind the
//! dialog starts, moves, opens or switches. The dialog's own keys (Esc, ↵, ⌘↵ to save, text editing) are bound
//! in its own key context, which wins over the app's, so they keep working. Decision 0097 names no app
//! shortcut that works over a dialog.

use crate::keymap::KeyAction;

/// Whether the app shortcut `action` runs.
pub fn shortcut_runs(action: KeyAction, modal_open: bool) -> bool {
    match action {
        // The terminal's own keys: they act on a focused terminal, never on what's behind a dialog (⌃` leaves it).
        KeyAction::Terminals | KeyAction::FindInTerminal => true,
        _ => !modal_open,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_app_shortcut_runs_over_a_modal() {
        for action in KeyAction::all() {
            assert!(
                shortcut_runs(action, false),
                "{action:?} runs without a modal"
            );
        }
        for action in [
            KeyAction::StartDev,
            KeyAction::NewTicket,
            KeyAction::Approve,
            KeyAction::SendBack,
            KeyAction::OpenSheet,
            KeyAction::CloseSheet,
            KeyAction::CommandPalette,
            KeyAction::FilterBoard,
            KeyAction::SwitchProject,
            KeyAction::Project(1),
            KeyAction::SwitchTab(2),
            KeyAction::Settings,
            KeyAction::NextNeedsYouItem,
            KeyAction::Park,
        ] {
            assert!(
                !shortcut_runs(action, true),
                "{action:?} is held back by a modal"
            );
        }
    }

    #[test]
    fn only_the_terminals_own_keys_pass_a_modal() {
        let passing: Vec<KeyAction> = KeyAction::all()
            .into_iter()
            .filter(|a| shortcut_runs(*a, true))
            .collect();
        assert_eq!(
            passing,
            vec![KeyAction::Terminals, KeyAction::FindInTerminal]
        );
    }
}
