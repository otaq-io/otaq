//! The box for new work (UX-22, UX-48; *New ticket · one box for new work*): one field docked at the bottom centre of
//! the board and the List, "New ticket, or ask Noyan…" with ⌘N. ⌘N, a click on it or the toolbar's New ticket opens
//! it: it grows upward over a light scrim. The first line typed is the ticket's title, the rest its description.
//!
//! - ↵ **Create**: the ticket exactly as typed, in Backlog, with no agent and no cost.
//! - ⌘↵ **Create & start**: the same ticket, then the user's own Start (`editor.create_and_start`).
//! - ⇧↵ **Ask Noyan to shape it**: the words start a conversation on Noyan's agent and its panel opens. Noyan reads the
//!   code and writes one or more tickets to Backlog; it never starts them.
//! - ⌥↵ a new line, since ↵ and ⇧↵ are taken. **More options…** opens the full form with what was typed.
//!
//! The box is on Board and List only: in Files ⌘N stays "New ticket from this file". It hides while Noyan's panel is
//! open, and wherever [`NewWorkView::shown`] says so.

use serde::Serialize;
use ts_rs::TS;

use crate::key_labels::KeyLabels;
use crate::keymap::{KeyAction, Platform};

/// The closed box's words.
pub const PLACEHOLDER: &str = "New ticket, or ask Noyan\u{2026}";

/// One of the open box's actions: its words and its key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "NewWorkAction")]
pub struct Action {
    pub label: &'static str,
    /// "↵", "⌘↵" ("Ctrl+↵" off the Mac).
    pub keys: &'static str,
}

/// Everything the box draws for a project. `NewWorkView` on the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
pub struct NewWorkView {
    /// Whether the box is on the board and the List at all.
    pub shown: bool,
    pub placeholder: &'static str,
    /// The user's New ticket key on the closed box ("⌘N"); `None`: no chip.
    pub keys: Option<String>,
    /// The second line's words while only a title is typed: "Add details (optional)".
    pub details: &'static str,
    /// How a new line is typed, beside `details`: "⌥↵ new line".
    pub new_line: Action,
    pub more_options: &'static str,
    pub ask: Action,
    pub create: Action,
    pub start: Action,
}

/// What [`view`] reads.
pub struct Facts<'a> {
    pub keys: &'a KeyLabels,
    /// Whether the box is on the board (always, today).
    pub shown: bool,
}

pub fn view(f: &Facts) -> NewWorkView {
    let mac = f.keys.platform() == Platform::Mac;
    let keys = |on_mac, elsewhere| if mac { on_mac } else { elsewhere };
    NewWorkView {
        shown: f.shown,
        placeholder: PLACEHOLDER,
        keys: f.keys.label(KeyAction::NewTicket),
        details: "Add details (optional)",
        new_line: Action {
            label: "new line",
            keys: keys("\u{2325}\u{21b5}", "Alt+\u{21b5}"),
        },
        more_options: "More options\u{2026}",
        ask: Action {
            label: "Ask Noyan to shape it",
            keys: keys("\u{21e7}\u{21b5}", "Shift+\u{21b5}"),
        },
        create: Action {
            label: "Create",
            keys: "\u{21b5}",
        },
        start: Action {
            label: "Create & start",
            keys: keys("\u{2318}\u{21b5}", "Ctrl+\u{21b5}"),
        },
    }
}

/// One state of the box in the gallery.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "NewWorkGalleryState")]
pub struct GalleryState {
    pub name: &'static str,
    /// What the box holds; open while `open`.
    pub typed: &'static str,
    pub open: bool,
    pub view: NewWorkView,
}

/// The gallery's box: closed, typing a ticket, and words for Noyan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "NewWorkGallery")]
pub struct Gallery {
    pub states: Vec<GalleryState>,
}

pub fn gallery() -> Gallery {
    let keys = KeyLabels::base(Platform::Mac);
    let v = view(&Facts {
        keys: &keys,
        shown: true,
    });
    Gallery {
        states: vec![
            GalleryState {
                name: "Closed",
                typed: "",
                open: false,
                view: v.clone(),
            },
            GalleryState {
                name: "Typing a ticket",
                typed: "Coupon field rejects lowercase codes\nAccept any case; codes are stored upper-case. Done when \
                        save10 applies SAVE10.",
                open: true,
                view: v.clone(),
            },
            GalleryState {
                name: "Ask Noyan to shape it (\u{21e7}\u{21b5})",
                typed: "Checkout feels slow on mobile, make tickets",
                open: true,
                view: v,
            },
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::key_labels::fixtures as keys;

    /// The closed box names both ways in and the user's New ticket key; open, its three actions and their keys.
    #[test]
    fn the_box_says_create_create_and_start_and_ask_noyan() {
        let k = keys::base();
        let v = view(&Facts {
            keys: &k,
            shown: true,
        });
        assert!(v.shown);
        assert_eq!(v.placeholder, "New ticket, or ask Noyan\u{2026}");
        assert_eq!(v.keys.as_deref(), Some("\u{2318}N"));
        assert_eq!(
            [
                (v.create.label, v.create.keys),
                (v.start.label, v.start.keys),
                (v.ask.label, v.ask.keys),
                (v.new_line.label, v.new_line.keys),
            ],
            [
                ("Create", "\u{21b5}"),
                ("Create & start", "\u{2318}\u{21b5}"),
                ("Ask Noyan to shape it", "\u{21e7}\u{21b5}"),
                ("new line", "\u{2325}\u{21b5}"),
            ]
        );
        assert_eq!(
            (v.details, v.more_options),
            ("Add details (optional)", "More options\u{2026}")
        );
    }

    /// Off the Mac the keys say Ctrl, Shift and Alt; a removed New ticket key shows no chip.
    #[test]
    fn off_the_mac_the_keys_are_spelled_out() {
        let k = KeyLabels::base(Platform::WinLinux);
        let v = view(&Facts {
            keys: &k,
            shown: true,
        });
        assert_eq!(v.keys.as_deref(), Some("Ctrl+N"));
        assert_eq!(
            (v.start.keys, v.ask.keys, v.new_line.keys),
            ("Ctrl+\u{21b5}", "Shift+\u{21b5}", "Alt+\u{21b5}")
        );
        let removed = keys::removed("new_ticket", None, 0);
        let v = view(&Facts {
            keys: &removed,
            shown: true,
        });
        // Its letter `c` is left: the chip shows it.
        assert_eq!(v.keys.as_deref(), Some("c"));
    }

    /// Whether the box shows is one flag, passed through.
    #[test]
    fn a_hidden_box_says_so() {
        let k = keys::base();
        assert!(
            !view(&Facts {
                keys: &k,
                shown: false
            })
            .shown
        );
    }

    #[test]
    fn the_gallery_draws_closed_typing_and_asking() {
        let g = gallery();
        assert_eq!(
            g.states
                .iter()
                .map(|s| (s.name, s.open))
                .collect::<Vec<_>>(),
            [
                ("Closed", false),
                ("Typing a ticket", true),
                ("Ask Noyan to shape it (\u{21e7}\u{21b5})", true)
            ]
        );
    }
}
