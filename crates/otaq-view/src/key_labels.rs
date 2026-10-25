//! The keys the labels show (KEY-08, d0097): every tooltip, menu item, Kbd chip and sentence that names an action's
//! key takes it from the **user's keys** ([`user_keys`]: the base map with their changes) and the single-letter switch,
//! never from the base map. The view models get a [`KeyLabels`] where they used to get only the platform; the desktop
//! api builds it from the stored keyboard settings for each view, and each view is asked again on `settings_changed`,
//! so a key changed in Settings shows at once.
//!
//! One label per action: its first modifier key (⌘N for New ticket's ⌘N and `c`, ⌃` for Terminals' ⌃` and `g` `t`),
//! else its first single letter or `g` sequence (`e`, `g` `e`) while letters are on, else none (a removed key, or a
//! letter-only action with letters off): then the label shows no chip.

use otaq_core::board::settings::KeyboardSettings;

use crate::keymap::{Binding, CustomKeys, KeyAction, Platform, user_keys};
use crate::keymap_edit::custom_keys;

/// The user's keys on one platform, as the labels read them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyLabels {
    platform: Platform,
    bindings: Vec<Binding>,
    letters: bool,
}

impl KeyLabels {
    /// `custom` applied to the base map; `letters`: the single-letter switch.
    pub fn new(platform: Platform, custom: &CustomKeys, letters: bool) -> Self {
        Self {
            platform,
            bindings: user_keys(platform, custom),
            letters,
        }
    }

    /// The labels for the stored keyboard settings (T-0390).
    pub fn stored(platform: Platform, keyboard: &KeyboardSettings) -> Self {
        Self::new(platform, &custom_keys(keyboard), keyboard.letters)
    }

    /// The base map with letters on: what a label shows before any change (and without the service).
    pub fn base(platform: Platform) -> Self {
        Self::new(platform, &CustomKeys::default(), true)
    }

    pub fn platform(&self) -> Platform {
        self.platform
    }

    /// The action's key outside terminals ("⌘B", "Ctrl+B", "e", "g e"); `None`: no chip.
    pub fn label(&self, action: KeyAction) -> Option<String> {
        self.pick(action, false)
    }

    /// Every key of the action outside terminals, its modifier keys first, then its letters while they are on: "⇧⌘O ·
    /// g f" in the Files segment's tooltip (`s28-02`).
    pub fn all_labels(&self, action: KeyAction) -> Vec<String> {
        let keys: Vec<&Binding> = self
            .bindings
            .iter()
            .filter(|b| b.action == action && b.place.outside_terminals())
            .collect();
        let modifier = keys.iter().filter(|b| !b.key.is_letter());
        let letters = keys.iter().filter(|b| self.letters && b.key.is_letter());
        modifier.chain(letters).map(|b| b.text.clone()).collect()
    }

    /// The action's key while a terminal has focus, for a label shown in one (the terminal tab's ⌃` and Find, the
    /// drawer's hint): the Windows/Linux terminal column ("Ctrl+Shift+F", "Ctrl+Shift+L"); on macOS ⌘ keys and ⌃`.
    /// Letters never work there, so a letter-only action has none.
    pub fn in_terminal(&self, action: KeyAction) -> Option<String> {
        self.pick(action, true)
    }

    fn pick(&self, action: KeyAction, in_terminal: bool) -> Option<String> {
        let keys: Vec<&Binding> = self
            .bindings
            .iter()
            .filter(|b| b.action == action)
            .filter(|b| {
                if in_terminal {
                    b.place.inside_terminals()
                } else {
                    b.place.outside_terminals()
                }
            })
            .collect();
        keys.iter()
            .find(|b| !b.key.is_letter())
            .or_else(|| {
                (self.letters && !in_terminal)
                    .then(|| keys.iter().find(|b| b.key.is_letter()))
                    .flatten()
            })
            .map(|b| b.text.clone())
    }
}

impl Default for KeyLabels {
    /// [`KeyLabels::base`] on this platform.
    fn default() -> Self {
        Self::base(Platform::current())
    }
}

/// Keyboard settings for the label tests, from the M29 example: Activity → ⌘J (which takes it from Next item that needs you);
/// one key recorded or removed; letters off.
pub mod fixtures {
    use otaq_core::board::settings::{KeyboardChange, KeyboardSettings};

    use super::KeyLabels;
    use crate::keymap::Platform;

    fn change(action: &str, number: Option<u8>, slot: u8, key: Option<&str>) -> KeyboardChange {
        KeyboardChange {
            action: action.to_owned(),
            number,
            slot,
            key: key.map(str::to_owned),
            works_in_terminals: false,
        }
    }

    /// Activity's ⇧⌘U recorded as ⌘J, confirmed: Next item that needs you's ⌘J is removed.
    pub fn activity_on_cmd_j() -> KeyboardSettings {
        KeyboardSettings {
            letters: true,
            changes: vec![
                change("next_needs_you_item", None, 0, None),
                change("open_activity", None, 0, Some("⌘J")),
            ],
        }
    }

    /// The user's keys on macOS for `settings`.
    pub fn mac(settings: &KeyboardSettings) -> KeyLabels {
        KeyLabels::stored(Platform::Mac, settings)
    }

    /// The base map on macOS with letters off.
    pub fn letters_off() -> KeyLabels {
        mac(&KeyboardSettings {
            letters: false,
            changes: Vec::new(),
        })
    }

    /// The base map on macOS with `action`'s key in `slot` removed.
    pub fn removed(action: &str, number: Option<u8>, slot: u8) -> KeyLabels {
        mac(&KeyboardSettings {
            letters: true,
            changes: vec![change(action, number, slot, None)],
        })
    }

    /// The macOS keys with the letters switch and `changes` (action, number, slot, the new key or `None` for removed).
    pub fn with(letters: bool, changes: &[(&str, Option<u8>, u8, Option<&str>)]) -> KeyLabels {
        mac(&KeyboardSettings {
            letters,
            changes: changes
                .iter()
                .map(|(action, number, slot, key)| change(action, *number, *slot, *key))
                .collect(),
        })
    }

    /// The base map on macOS with `action`'s key in `slot` recorded as `key`.
    pub fn replaced(action: &str, number: Option<u8>, slot: u8, key: &str) -> KeyLabels {
        mac(&KeyboardSettings {
            letters: true,
            changes: vec![change(action, number, slot, Some(key))],
        })
    }

    /// The base map on macOS.
    pub fn base() -> KeyLabels {
        KeyLabels::base(Platform::Mac)
    }
}

#[cfg(test)]
mod tests {
    use super::fixtures::*;
    use super::*;

    #[test]
    fn a_label_is_the_first_modifier_key_else_the_first_letter() {
        let mac = base();
        assert_eq!(mac.label(KeyAction::NewTicket).as_deref(), Some("⌘N"));
        assert_eq!(mac.label(KeyAction::Terminals).as_deref(), Some("⌃`"));
        assert_eq!(mac.label(KeyAction::ExpandSheet).as_deref(), Some("e"));
        assert_eq!(mac.label(KeyAction::ShowList).as_deref(), Some("g l"));
        assert_eq!(mac.label(KeyAction::FilterBoard).as_deref(), Some("/"));
        assert_eq!(mac.label(KeyAction::SwitchTab(3)).as_deref(), Some("3"));
        assert_eq!(mac.label(KeyAction::Project(2)).as_deref(), Some("⌘2"));
        assert_eq!(mac.label(KeyAction::Park), None);
        let win = KeyLabels::base(Platform::WinLinux);
        assert_eq!(
            win.label(KeyAction::FocusAgentPanel).as_deref(),
            Some("Ctrl+L")
        );
        assert_eq!(win.label(KeyAction::FindInTerminal), None);
    }

    /// Activity → ⌘J takes the key from Next item that needs you, which then has none (no chip).
    #[test]
    fn a_label_follows_the_users_keys() {
        let keys = mac(&activity_on_cmd_j());
        assert_eq!(keys.label(KeyAction::OpenActivity).as_deref(), Some("⌘J"));
        assert_eq!(keys.label(KeyAction::NextNeedsYouItem), None);
        // Its other key stays: Board's `g` `b` shows once its ⇧⌘B is removed.
        let board = removed("show_board", None, 0);
        assert_eq!(board.label(KeyAction::ShowBoard).as_deref(), Some("g b"));
    }

    #[test]
    fn with_letters_off_a_letter_only_action_shows_none() {
        let off = letters_off();
        assert_eq!(off.label(KeyAction::ExpandSheet), None);
        assert_eq!(off.label(KeyAction::ShowList), None);
        assert_eq!(off.label(KeyAction::SwitchTab(1)), None);
        assert_eq!(off.label(KeyAction::Terminals).as_deref(), Some("⌃`"));
    }

    /// Inside a terminal: the Windows/Linux terminal column; on macOS ⌘ keys and ⌃`; never a letter.
    #[test]
    fn a_label_in_a_terminal_reads_the_terminal_column() {
        let win = KeyLabels::base(Platform::WinLinux);
        assert_eq!(
            win.in_terminal(KeyAction::FindInTerminal).as_deref(),
            Some("Ctrl+Shift+F")
        );
        assert_eq!(
            win.in_terminal(KeyAction::FocusAgentPanel).as_deref(),
            Some("Ctrl+Shift+L")
        );
        assert_eq!(
            win.in_terminal(KeyAction::Terminals).as_deref(),
            Some("Ctrl+`")
        );
        assert_eq!(win.in_terminal(KeyAction::ShowBoard), None);
        let mac = base();
        assert_eq!(
            mac.in_terminal(KeyAction::FindInTerminal).as_deref(),
            Some("⌘F")
        );
        assert_eq!(mac.in_terminal(KeyAction::Terminals).as_deref(), Some("⌃`"));
        assert_eq!(mac.in_terminal(KeyAction::ExpandSheet), None);
    }

    #[test]
    fn without_changes_the_labels_are_the_base_maps() {
        assert_eq!(
            KeyLabels::stored(Platform::Mac, &KeyboardSettings::default()),
            base()
        );
    }
}
