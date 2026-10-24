//! Custom shortcuts (d0097, KEY-04, KEY-02's switch): the checks an edit goes through before it is stored, and their
//! texts. The settings keep the plain shape ([`KeyboardSettings`], `otaq-core`); [`custom_keys`] reads it as the map's
//! [`CustomKeys`] and [`edit`] answers one edit: done (with the settings to store), a key taken by another action
//! (the question), a refused key (the reason), or the warning "Works inside terminals" asks to confirm.
//!
//! The rules, in the order they apply to a recorded key:
//! - OS and text-editing keys are refused (⌘Q, ⌘W, ⌘H, ⌘M, ⌘Tab, ⌘Space, ⌘`; Alt+F4, Alt+Tab, Windows-key keys; copy,
//!   paste, cut, undo, select all), and so is plain `g`, which starts every sequence;
//! - a key another action has (in the same place: outside terminals, or inside them, where a custom Ctrl+letter's
//!   Shift form counts) asks first; on yes the other action's key becomes "removed". Two actions never share a key;
//! - anything else is taken as it is, single letters and `g` sequences included, for any action, with no warning.
//!
//! "Works inside terminals" (Windows/Linux; on macOS for ⌃ keys only) is refused for the keys a terminal can't lose
//! ([`never_in_terminals`]) and otherwise warns what the shell or agent loses before it is ticked.

use otaq_core::board::settings::{KeyboardChange, KeyboardSettings};
use otaq_core::naming;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::keymap::{
    Binding, CustomKey, CustomKeys, Key, KeyAction, KeyChange, KeyPlace, Platform,
    never_in_terminals, user_keys,
};

/// One edit of the keyboard settings. `KeyEdit` on the page: `{ edit: "record", action: { action: "park" }, slot: 0,
/// key: "g p" }`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS)]
#[serde(tag = "edit", rename_all = "snake_case")]
pub enum KeyEdit {
    /// A new key for one of the action's keys (`slot`, as the map's [`Binding::slot`]; one past them adds a key, as
    /// for an action with no key). `key` is the key's text as the map writes it ("⌘J", "Ctrl+R", "n", "g p").
    /// `confirmed`: the user answered "Use here" to the question about a key that is taken.
    Record {
        action: KeyAction,
        slot: u8,
        key: String,
        #[serde(default)]
        confirmed: bool,
    },
    /// "Remove key": the action keeps no key there (it stays in the palette).
    Remove { action: KeyAction, slot: u8 },
    /// The key's default back. `confirmed` as for [`KeyEdit::Record`], when another action has that key now.
    Reset {
        action: KeyAction,
        slot: u8,
        #[serde(default)]
        confirmed: bool,
    },
    /// Every key back to the base map (the page asks first, [`reset_all_question`]).
    ResetAll,
    /// Ticks or unticks "Works inside terminals" for one key. `let_through`: the user confirmed the warning
    /// ("Let it through"); `confirmed`: as for [`KeyEdit::Record`].
    WorksInTerminals {
        action: KeyAction,
        slot: u8,
        on: bool,
        #[serde(default)]
        let_through: bool,
        #[serde(default)]
        confirmed: bool,
    },
    /// The single-letter switch (KEY-02).
    Letters { on: bool },
}

/// What an edit answers. `KeyEditAnswer` on the page; the Settings page shows it in place (design `s15-50`…`s15-55`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum KeyEditAnswer {
    /// Stored. `changed`: how many keys differ from the base map now (Reset all's count).
    Done { changed: u32 },
    /// The key is another action's: nothing changed; the question asks whether to move it ("Use here" sends the edit
    /// again with `confirmed`).
    Taken {
        /// The key as it clashes ("⌘J"; a custom Ctrl+letter's Shift form when that is what clashes).
        key: String,
        by: KeyAction,
        question: String,
    },
    /// The key can't be used (there): nothing changed. The recording cell keeps recording.
    Refused { reason: String },
    /// Ticking "Works inside terminals" takes a key from the shell or agent: nothing changed until confirmed
    /// (Cancel / Let it through, which sends the edit again with `let_through`).
    Warn { warning: String },
}

/// An answered edit, with the settings to store when it is done.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Edited {
    pub answer: KeyEditAnswer,
    /// `Some` only for [`KeyEditAnswer::Done`].
    pub store: Option<KeyboardSettings>,
}

/// The stored changes as the map's [`CustomKeys`]: a change naming an action or a key the map doesn't know is left
/// out.
pub fn custom_keys(stored: &KeyboardSettings) -> CustomKeys {
    CustomKeys(
        stored
            .changes
            .iter()
            .filter_map(|c| {
                let mut action = serde_json::json!({ "action": c.action });
                if let Some(n) = c.number {
                    action["number"] = n.into();
                }
                let change = match &c.key {
                    Some(text) => KeyChange::Replaced {
                        key: Key::parse(text)?,
                    },
                    None => KeyChange::Removed,
                };
                Some(CustomKey {
                    action: serde_json::from_value(action).ok()?,
                    slot: c.slot,
                    change,
                    works_in_terminals: c.works_in_terminals,
                })
            })
            .collect(),
    )
}

fn stored_changes(platform: Platform, custom: &CustomKeys) -> Vec<KeyboardChange> {
    custom
        .0
        .iter()
        .map(|c| {
            let action = serde_json::to_value(c.action).expect("an action serializes");
            KeyboardChange {
                action: action["action"].as_str().unwrap_or_default().to_owned(),
                number: action["number"].as_u64().and_then(|n| u8::try_from(n).ok()),
                slot: c.slot,
                key: match &c.change {
                    KeyChange::Replaced { key } => Some(key.text(platform)),
                    KeyChange::Removed => None,
                },
                works_in_terminals: c.works_in_terminals,
            }
        })
        .collect()
}

/// How many keys differ from the base map ("Reset all 3 changed shortcuts…").
pub fn changed_count(stored: &KeyboardSettings) -> u32 {
    u32::try_from(custom_keys(stored).0.len()).unwrap_or(u32::MAX)
}

/// Reset all's question (d0097), asked by the page before it sends [`KeyEdit::ResetAll`].
pub fn reset_all_question(changed: u32) -> String {
    if changed == 1 {
        "Reset 1 changed shortcut to its default?".to_owned()
    } else {
        format!("Reset all {changed} changed shortcuts to their defaults?")
    }
}

/// The question for a key another action has (design `s15-50`).
pub fn taken_question(key: &str, by: KeyAction) -> String {
    format!("{key} is used by {}. Use it here instead?", by.title())
}

/// Plain `g`'s refusal: it starts every sequence (d0097).
pub const G_REFUSED: &str = "g starts the g shortcuts (g b Board, g l List…). Press another key.";

/// The keys the OS or text editing has on this platform, with what they do (d0097, KEY-04); `{app}` is the product
/// name (d0028).
fn os_keys(platform: Platform) -> &'static [(&'static str, &'static str)] {
    if platform == Platform::Mac {
        &[
            ("⌘Q", "quits {app}"),
            ("⌘W", "closes the window"),
            ("⌘H", "hides {app}"),
            ("⌘M", "minimizes the window"),
            ("⌘Tab", "switches apps"),
            ("⌘Space", "opens Spotlight"),
            ("⌘`", "switches windows"),
            ("⌘C", "copies text"),
            ("⌘V", "pastes text"),
            ("⌘X", "cuts text"),
            ("⌘Z", "undoes the last edit"),
            ("⌘A", "selects all text"),
        ]
    } else {
        &[
            ("Alt+F4", "closes the window"),
            ("Alt+Tab", "switches windows"),
            ("Ctrl+C", "copies text"),
            ("Ctrl+V", "pastes text"),
            ("Ctrl+X", "cuts text"),
            ("Ctrl+Z", "undoes the last edit"),
            ("Ctrl+A", "selects all text"),
        ]
    }
}

/// Why a key can't be recorded at all, or `None`: the OS's and text editing's keys ("⌘Q quits Otaq. Press another
/// key."), and on Windows/Linux every Windows-key combination. Keys the map can't write (⌘Space, Alt+F4) are matched
/// by their text.
pub fn os_refusal(platform: Platform, text: &str) -> Option<String> {
    let platform = column(platform);
    let same = |a: &str, b: &str| match (Key::parse(a), Key::parse(b)) {
        (Some(a), Some(b)) => a == b,
        _ => a == b,
    };
    if let Some((key, does)) = os_keys(platform).iter().find(|(key, _)| same(key, text)) {
        let does = does.replace("{app}", naming::PRODUCT_NAME);
        return Some(format!("{key} {does}. Press another key."));
    }
    let windows_key = text.split('+').any(|part| part == "Super");
    (platform != Platform::Mac && windows_key)
        .then(|| format!("{text} belongs to the system. Press another key."))
}

/// What the terminal does with the keys "Works inside terminals" refuses ([`never_in_terminals`]).
fn terminal_meaning(text: &str) -> &'static str {
    let key = text.rsplit(['+', '⌃']).next().unwrap_or_default();
    match (text.contains("Shift"), key) {
        (true, "C") => "is the terminal’s copy",
        (true, "V") => "is the terminal’s paste",
        (_, "C") => "stops the running command in a terminal",
        (_, "D") => "ends the shell or agent in a terminal",
        (_, "Z") => "suspends the running command in a terminal",
        (_, "[") => "is Escape in a terminal",
        _ => "force-quits the running command in a terminal",
    }
}

/// Why `key` can't work inside terminals, or `None` ("Ctrl+D ends the shell or agent in a terminal, so it can't work
/// inside terminals.").
pub fn terminal_refusal(platform: Platform, key: &Key) -> Option<String> {
    let platform = column(platform);
    let stroke = key.stroke()?;
    let text = never_in_terminals(platform)
        .iter()
        .find(|t| Key::parse(t).as_ref().and_then(Key::stroke) == Some(stroke))?;
    Some(format!(
        "{text} {}, so it can’t work inside terminals.",
        terminal_meaning(text)
    ))
}

/// What the shell loses when a Ctrl+letter (⌃letter on macOS) works inside terminals ("Ctrl+R: the shell's history
/// search won't reach the terminal."), or `None` for a key the shell doesn't read that way.
pub fn terminal_warning(platform: Platform, key: &Key) -> Option<String> {
    let platform = column(platform);
    let stroke = key.stroke()?;
    let m = &stroke.modifiers;
    if !m.control || m.shift || m.alt || m.platform {
        return None;
    }
    let mut letters = stroke.key.chars();
    let (Some(letter), None) = (letters.next(), letters.next()) else {
        return None;
    };
    if !letter.is_ascii_lowercase() {
        return None;
    }
    let text = key.text(platform);
    let loses = match letter {
        'a' => "the shell’s start-of-line key",
        'b' => "the shell’s back-one-character key",
        'e' => "the shell’s end-of-line key",
        'f' => "the shell’s forward-one-character key",
        'h' => "the shell’s backspace",
        'i' => "the shell’s Tab completion",
        'j' => "the new line in an agent’s prompt",
        'k' => "the shell’s delete to the end of the line",
        'l' => "the shell’s clear screen",
        'm' => "the shell’s Enter",
        'n' => "the shell’s next command",
        'p' => "the shell’s previous command",
        'q' => "the shell’s resume output",
        'r' => "the shell’s history search",
        's' => "the shell’s pause output",
        't' => "the shell’s swap characters",
        'u' => "the shell’s delete to the start of the line",
        'w' => "the shell’s delete the word before",
        'y' => "the shell’s paste of deleted text",
        _ => {
            return Some(format!(
                "{text}: the shell or agent won’t get this key in a terminal."
            ));
        }
    };
    Some(format!("{text}: {loses} won’t reach the terminal."))
}

/// Whether a key has the "Works inside terminals" option: on Windows/Linux any key with a modifier, on macOS a ⌃ key
/// without ⌘ (d0097 changelog 2026-10-19). Letters and sequences never work in a terminal.
pub fn offers_works_in_terminals(platform: Platform, key: &Key) -> bool {
    let Some(stroke) = key.stroke() else {
        return false;
    };
    let m = &stroke.modifiers;
    if column(platform) == Platform::Mac {
        m.control && !m.platform
    } else {
        m.control || m.alt || m.platform
    }
}

/// The "Works inside terminals" checkbox of one key: `None` where it isn't offered (or the action has no key there),
/// else whether the key reaches the app unchanged while a terminal has focus (the base map's Ctrl+K and Ctrl+` do).
pub fn works_in_terminals(
    platform: Platform,
    stored: &KeyboardSettings,
    action: KeyAction,
    slot: u8,
) -> Option<bool> {
    let platform = column(platform);
    let custom = custom_keys(stored);
    let key = current_key(platform, &custom, action, slot)?;
    offers_works_in_terminals(platform, &key).then(|| {
        user_keys(platform, &custom)
            .iter()
            .any(|b| b.action == action && b.slot == slot && b.place == KeyPlace::Everywhere)
    })
}

/// Answers one edit of `stored` on `platform`. An edit the page can't send (no such key or slot) is an error.
pub fn edit(
    platform: Platform,
    stored: &KeyboardSettings,
    edit: &KeyEdit,
) -> Result<Edited, String> {
    let platform = column(platform);
    let mut custom = custom_keys(stored);
    let checked = match edit {
        KeyEdit::Letters { on } => {
            return Ok(done(KeyboardSettings {
                letters: *on,
                ..stored.clone()
            }));
        }
        KeyEdit::ResetAll => {
            return Ok(done(KeyboardSettings {
                changes: Vec::new(),
                ..stored.clone()
            }));
        }
        KeyEdit::Record {
            action,
            slot,
            key,
            confirmed,
        } => {
            check_slot(&custom, *action, *slot, true)?;
            if let Some(reason) = os_refusal(platform, key) {
                return Ok(refused(reason));
            }
            let key = Key::parse(key).ok_or_else(|| format!("not a key: {key}"))?;
            if key == (Key::Letter { letter: "g".into() }) {
                return Ok(refused(G_REFUSED.to_owned()));
            }
            // Recording the default key back is its reset.
            if base_key(platform, *action, *slot).as_ref() == Some(&key) {
                forget(&mut custom, *action, *slot);
            } else {
                set(
                    &mut custom,
                    CustomKey {
                        action: *action,
                        slot: *slot,
                        change: KeyChange::Replaced { key },
                        works_in_terminals: false,
                    },
                );
            }
            take_from_others(platform, &mut custom, *action, *slot, *confirmed)
        }
        KeyEdit::Remove { action, slot } => {
            check_slot(&custom, *action, *slot, false)?;
            remove(&mut custom, *action, *slot);
            Ok(custom)
        }
        KeyEdit::Reset {
            action,
            slot,
            confirmed,
        } => {
            check_slot(&custom, *action, *slot, false)?;
            forget(&mut custom, *action, *slot);
            take_from_others(platform, &mut custom, *action, *slot, *confirmed)
        }
        KeyEdit::WorksInTerminals {
            action,
            slot,
            on,
            let_through,
            confirmed,
        } => {
            check_slot(&custom, *action, *slot, false)?;
            let key = current_key(platform, &custom, *action, *slot)
                .ok_or_else(|| format!("{} has no key there", action.title()))?;
            if !offers_works_in_terminals(platform, &key) {
                return Err(format!(
                    "{} has no Works inside terminals",
                    key.text(platform)
                ));
            }
            let reaches = works_in_terminals(platform, stored, *action, *slot) == Some(true);
            if *on && !reaches {
                if let Some(reason) = terminal_refusal(platform, &key) {
                    return Ok(refused(reason));
                }
                if !let_through && let Some(warning) = terminal_warning(platform, &key) {
                    return Ok(Edited {
                        answer: KeyEditAnswer::Warn { warning },
                        store: None,
                    });
                }
            }
            set(
                &mut custom,
                CustomKey {
                    action: *action,
                    slot: *slot,
                    change: KeyChange::Replaced { key },
                    works_in_terminals: *on,
                },
            );
            take_from_others(platform, &mut custom, *action, *slot, *confirmed)
        }
    };
    Ok(match checked {
        Ok(custom) => done(KeyboardSettings {
            changes: stored_changes(platform, &tidy(platform, custom)),
            ..stored.clone()
        }),
        Err(answer) => Edited {
            answer,
            store: None,
        },
    })
}

/// The column the edits are checked in: macOS, or Windows/Linux (whose bindings hold the terminal forms too).
fn column(platform: Platform) -> Platform {
    match platform {
        Platform::WinLinuxTerminal => Platform::WinLinux,
        other => other,
    }
}

fn done(store: KeyboardSettings) -> Edited {
    Edited {
        answer: KeyEditAnswer::Done {
            changed: changed_count(&store),
        },
        store: Some(store),
    }
}

fn refused(reason: String) -> Edited {
    Edited {
        answer: KeyEditAnswer::Refused { reason },
        store: None,
    }
}

/// The action's slots: its base keys, and the keys the user added past them.
fn slot_count(custom: &CustomKeys, action: KeyAction) -> usize {
    custom
        .0
        .iter()
        .filter(|c| c.action == action)
        .map(|c| usize::from(c.slot) + 1)
        .chain([action.base_keys().len()])
        .max()
        .unwrap_or(0)
}

/// The slot exists (or, recording, is the next one: a new key).
fn check_slot(
    custom: &CustomKeys,
    action: KeyAction,
    slot: u8,
    adding: bool,
) -> Result<(), String> {
    let count = slot_count(custom, action);
    let slot = usize::from(slot);
    if slot < count || (adding && slot == count) {
        Ok(())
    } else {
        Err(format!("{} has no key {slot}", action.title()))
    }
}

/// The base map's key of a slot in this column; on Windows/Linux its terminal form where it has no other (Find in
/// terminal scrollback).
fn base_key(platform: Platform, action: KeyAction, slot: u8) -> Option<Key> {
    let texts = action.base_keys().into_iter().nth(usize::from(slot))?;
    let text = match platform {
        Platform::Mac => &texts[0],
        _ if texts[1].is_empty() => &texts[2],
        _ => &texts[1],
    };
    Key::parse(text)
}

/// The slot's key now: the user's, or the base map's; `None` when removed or never had one.
fn current_key(
    platform: Platform,
    custom: &CustomKeys,
    action: KeyAction,
    slot: u8,
) -> Option<Key> {
    match custom
        .0
        .iter()
        .find(|c| c.action == action && c.slot == slot)
    {
        Some(CustomKey {
            change: KeyChange::Replaced { key },
            ..
        }) => Some(key.clone()),
        Some(_) => None,
        None => base_key(platform, action, slot),
    }
}

/// Keeps one change per key.
fn set(custom: &mut CustomKeys, change: CustomKey) {
    forget(custom, change.action, change.slot);
    custom.0.push(change);
}

fn forget(custom: &mut CustomKeys, action: KeyAction, slot: u8) {
    custom.0.retain(|c| !(c.action == action && c.slot == slot));
}

/// "Remove key": a base key is marked removed; a key the user added is just dropped.
fn remove(custom: &mut CustomKeys, action: KeyAction, slot: u8) {
    if usize::from(slot) < action.base_keys().len() {
        set(
            custom,
            CustomKey {
                action,
                slot,
                change: KeyChange::Removed,
                works_in_terminals: false,
            },
        );
    } else {
        forget(custom, action, slot);
    }
}

/// Whether two bindings can be pressed in the same place.
fn overlap(a: KeyPlace, b: KeyPlace) -> bool {
    (a.outside_terminals() && b.outside_terminals())
        || (a.inside_terminals() && b.inside_terminals())
}

/// The other keys the action's `slot` shares a key with: (their binding, the action's clashing binding).
fn clashes(
    platform: Platform,
    custom: &CustomKeys,
    action: KeyAction,
    slot: u8,
) -> Vec<(Binding, Binding)> {
    let all = user_keys(platform, custom);
    let mine = |b: &Binding| b.action == action && b.slot == slot;
    let mut found = Vec::new();
    for m in all.iter().filter(|b| mine(b)) {
        for o in all.iter().filter(|b| !mine(b)) {
            if o.key == m.key && overlap(o.place, m.place) {
                found.push((o.clone(), m.clone()));
            }
        }
    }
    found
}

/// The first rule (d0097): a key another action has is moved only once the user said "Use here"; the other action's
/// key becomes "removed". Answers the changed keys, or the question.
fn take_from_others(
    platform: Platform,
    custom: &mut CustomKeys,
    action: KeyAction,
    slot: u8,
    confirmed: bool,
) -> Result<CustomKeys, KeyEditAnswer> {
    let found = clashes(platform, custom, action, slot);
    if let Some((other, mine)) = found.first()
        && !confirmed
    {
        return Err(KeyEditAnswer::Taken {
            key: mine.text.clone(),
            by: other.action,
            question: taken_question(&mine.text, other.action),
        });
    }
    for (other, _) in found {
        remove(custom, other.action, other.slot);
    }
    Ok(custom.clone())
}

/// Leaves out the changes that bind nothing differently from the base map (a key recorded back to its default, an
/// added key removed), so they don't count as changed.
fn tidy(platform: Platform, custom: CustomKeys) -> CustomKeys {
    let of = |custom: &CustomKeys, action: KeyAction, slot: u8| -> Vec<(String, Key, KeyPlace)> {
        user_keys(platform, custom)
            .into_iter()
            .filter(|b| b.action == action && b.slot == slot)
            .map(|b| (b.text, b.key, b.place))
            .collect()
    };
    let mut kept = custom.clone();
    for change in &custom.0 {
        let mut without = kept.clone();
        forget(&mut without, change.action, change.slot);
        if of(&kept, change.action, change.slot) == of(&without, change.action, change.slot) {
            kept = without;
        }
    }
    kept
}

#[cfg(test)]
mod tests {
    use super::*;

    const MAC: Platform = Platform::Mac;
    const WIN: Platform = Platform::WinLinux;

    fn record(action: KeyAction, slot: u8, key: &str) -> KeyEdit {
        KeyEdit::Record {
            action,
            slot,
            key: key.into(),
            confirmed: false,
        }
    }

    fn confirmed(edit: KeyEdit) -> KeyEdit {
        match edit {
            KeyEdit::Record {
                action, slot, key, ..
            } => KeyEdit::Record {
                action,
                slot,
                key,
                confirmed: true,
            },
            KeyEdit::Reset { action, slot, .. } => KeyEdit::Reset {
                action,
                slot,
                confirmed: true,
            },
            other => other,
        }
    }

    fn inside(action: KeyAction, slot: u8, on: bool, let_through: bool) -> KeyEdit {
        KeyEdit::WorksInTerminals {
            action,
            slot,
            on,
            let_through,
            confirmed: false,
        }
    }

    /// Applies an edit that must be done and answers the stored settings.
    fn ok(platform: Platform, stored: &KeyboardSettings, e: KeyEdit) -> KeyboardSettings {
        let edited = edit(platform, stored, &e).unwrap();
        assert!(
            matches!(edited.answer, KeyEditAnswer::Done { .. }),
            "{e:?}: {:?}",
            edited.answer
        );
        edited.store.unwrap()
    }

    fn answer(platform: Platform, stored: &KeyboardSettings, e: KeyEdit) -> KeyEditAnswer {
        let edited = edit(platform, stored, &e).unwrap();
        if !matches!(edited.answer, KeyEditAnswer::Done { .. }) {
            assert_eq!(edited.store, None, "nothing changes");
        }
        edited.answer
    }

    /// The keys of an action as shown, outside terminals and (Windows/Linux) inside them.
    fn keys(
        platform: Platform,
        stored: &KeyboardSettings,
        action: KeyAction,
    ) -> Vec<(u8, String, KeyPlace)> {
        user_keys(platform, &custom_keys(stored))
            .into_iter()
            .filter(|b| b.action == action)
            .map(|b| (b.slot, b.text, b.place))
            .collect()
    }

    fn texts(platform: Platform, stored: &KeyboardSettings, action: KeyAction) -> Vec<String> {
        keys(platform, stored, action)
            .into_iter()
            .filter(|(_, _, place)| place.outside_terminals())
            .map(|(_, text, _)| text)
            .collect()
    }

    fn k(slot: u8, text: &str, place: KeyPlace) -> (u8, String, KeyPlace) {
        (slot, text.to_owned(), place)
    }

    fn refused_with(reason: &str) -> KeyEditAnswer {
        KeyEditAnswer::Refused {
            reason: reason.into(),
        }
    }

    #[test]
    fn a_taken_key_asks_and_moves_only_once_confirmed() {
        let fresh = KeyboardSettings::default();
        let edit = record(KeyAction::OpenActivity, 0, "⌘J");
        assert_eq!(
            answer(MAC, &fresh, edit.clone()),
            KeyEditAnswer::Taken {
                key: "⌘J".into(),
                by: KeyAction::NextNeedsYouItem,
                question: "⌘J is used by Next item that needs you. Use it here instead?".into(),
            }
        );
        let moved = ok(MAC, &fresh, confirmed(edit));
        assert_eq!(texts(MAC, &moved, KeyAction::OpenActivity), ["⌘J", "g a"]);
        assert!(
            texts(MAC, &moved, KeyAction::NextNeedsYouItem).is_empty(),
            "no key"
        );
        assert_eq!(changed_count(&moved), 2);
        // The stored shape: Activity's ⌘J, Next item that needs you's key removed.
        assert_eq!(
            moved.changes,
            [
                KeyboardChange {
                    action: "open_activity".into(),
                    number: None,
                    slot: 0,
                    key: Some("⌘J".into()),
                    works_in_terminals: false,
                },
                KeyboardChange {
                    action: "next_needs_you_item".into(),
                    number: None,
                    slot: 0,
                    key: None,
                    works_in_terminals: false,
                },
            ]
        );
        // Its reset gives ⌘J back, which Activity has now: the same question.
        let reset = KeyEdit::Reset {
            action: KeyAction::NextNeedsYouItem,
            slot: 0,
            confirmed: false,
        };
        assert!(matches!(
            answer(MAC, &moved, reset.clone()),
            KeyEditAnswer::Taken {
                by: KeyAction::OpenActivity,
                ..
            }
        ));
        let back = ok(MAC, &moved, confirmed(reset));
        assert_eq!(texts(MAC, &back, KeyAction::NextNeedsYouItem), ["⌘J"]);
        assert_eq!(texts(MAC, &back, KeyAction::OpenActivity), ["g a"]);

        // A taken letter or sequence counts the same.
        assert!(matches!(
            answer(MAC, &fresh, record(KeyAction::Park, 0, "g b")),
            KeyEditAnswer::Taken {
                by: KeyAction::ShowBoard,
                ..
            }
        ));
        assert!(matches!(
            answer(MAC, &fresh, record(KeyAction::Park, 0, "j")),
            KeyEditAnswer::Taken {
                by: KeyAction::NextTicket,
                ..
            }
        ));
    }

    #[test]
    fn os_and_editing_keys_are_refused_with_the_reason() {
        let fresh = KeyboardSettings::default();
        let quits = format!("⌘Q quits {}. Press another key.", naming::PRODUCT_NAME);
        for (key, reason) in [
            ("⌘Q", quits.as_str()),
            ("⌘C", "⌘C copies text. Press another key."),
            ("⌘W", "⌘W closes the window. Press another key."),
            ("⌘Space", "⌘Space opens Spotlight. Press another key."),
            ("⌘`", "⌘` switches windows. Press another key."),
            ("⌘A", "⌘A selects all text. Press another key."),
        ] {
            assert_eq!(
                answer(MAC, &fresh, record(KeyAction::OpenActivity, 0, key)),
                refused_with(reason),
                "{key}"
            );
        }
        for (key, reason) in [
            ("Alt+F4", "Alt+F4 closes the window. Press another key."),
            ("Ctrl+V", "Ctrl+V pastes text. Press another key."),
            (
                "Super+E",
                "Super+E belongs to the system. Press another key.",
            ),
        ] {
            assert_eq!(
                answer(WIN, &fresh, record(KeyAction::OpenActivity, 0, key)),
                refused_with(reason),
                "{key}"
            );
        }
        // ⌘Q's Windows/Linux twin is free there; the webview's keys Otaq switches off are free.
        ok(WIN, &fresh, record(KeyAction::OpenActivity, 0, "Ctrl+Q"));
        ok(MAC, &fresh, record(KeyAction::OpenActivity, 0, "⌘R"));
    }

    #[test]
    fn plain_g_is_refused_and_a_custom_g_sequence_is_taken() {
        let fresh = KeyboardSettings::default();
        assert_eq!(
            answer(MAC, &fresh, record(KeyAction::Park, 0, "g")),
            refused_with("g starts the g shortcuts (g b Board, g l List…). Press another key.")
        );
        let park = ok(MAC, &fresh, record(KeyAction::Park, 0, "g p"));
        assert_eq!(texts(MAC, &park, KeyAction::Park), ["g p"]);
        assert_eq!(changed_count(&park), 1);
        // A single letter for an action that changes a ticket: the user's choice, no warning (d0097).
        let approve = ok(WIN, &fresh, record(KeyAction::Approve, 1, "a"));
        assert_eq!(texts(WIN, &approve, KeyAction::Approve), ["Ctrl+↵", "a"]);
        // An edit the page can't send.
        assert!(edit(MAC, &fresh, &record(KeyAction::Park, 3, "g p")).is_err());
        assert!(edit(MAC, &fresh, &record(KeyAction::Park, 0, "nonsense")).is_err());
    }

    #[test]
    fn each_key_is_changed_removed_and_reset_on_its_own() {
        let fresh = KeyboardSettings::default();
        let n = ok(MAC, &fresh, record(KeyAction::NewTicket, 1, "n"));
        assert_eq!(texts(MAC, &n, KeyAction::NewTicket), ["⌘N", "n"]);
        assert_eq!(n.changes.len(), 1);

        let removed = ok(
            MAC,
            &n,
            KeyEdit::Remove {
                action: KeyAction::NewTicket,
                slot: 0,
            },
        );
        assert_eq!(texts(MAC, &removed, KeyAction::NewTicket), ["n"]);
        assert_eq!(changed_count(&removed), 2);

        let reset = KeyEdit::Reset {
            action: KeyAction::NewTicket,
            slot: 1,
            confirmed: false,
        };
        let one = ok(MAC, &removed, reset);
        assert_eq!(
            texts(MAC, &one, KeyAction::NewTicket),
            ["c"],
            "⌘N stays removed"
        );
        // Recording the default back is its reset: nothing is changed then.
        let again = ok(MAC, &one, record(KeyAction::NewTicket, 0, "⌘N"));
        assert_eq!(again.changes, []);
        // A key added to an action with none, then removed, leaves nothing changed.
        let park = ok(MAC, &fresh, record(KeyAction::Park, 0, "g p"));
        let gone = ok(
            MAC,
            &park,
            KeyEdit::Remove {
                action: KeyAction::Park,
                slot: 0,
            },
        );
        assert_eq!(gone.changes, []);
    }

    #[test]
    fn reset_all_puts_every_key_back_and_says_how_many_were_changed() {
        let mut stored = KeyboardSettings::default();
        stored = ok(
            MAC,
            &stored,
            confirmed(record(KeyAction::OpenActivity, 0, "⌘J")),
        );
        stored = ok(MAC, &stored, record(KeyAction::NewTicket, 1, "n"));
        let edited = edit(MAC, &stored, &KeyEdit::Letters { on: false }).unwrap();
        assert_eq!(edited.answer, KeyEditAnswer::Done { changed: 3 });
        stored = edited.store.unwrap();
        assert!(!stored.letters);
        assert_eq!(
            reset_all_question(changed_count(&stored)),
            "Reset all 3 changed shortcuts to their defaults?"
        );

        let edited = edit(MAC, &stored, &KeyEdit::ResetAll).unwrap();
        assert_eq!(edited.answer, KeyEditAnswer::Done { changed: 0 });
        let all = edited.store.unwrap();
        assert_eq!(all.changes, []);
        assert!(!all.letters, "the switch isn't a shortcut");
        assert_eq!(texts(MAC, &all, KeyAction::NextNeedsYouItem), ["⌘J"]);
        let on = ok(MAC, &all, KeyEdit::Letters { on: true });
        assert!(on.letters);
    }

    #[test]
    fn stored_changes_that_the_map_doesnt_know_are_left_out() {
        let stored = KeyboardSettings {
            letters: true,
            changes: vec![
                KeyboardChange {
                    action: "fly".into(),
                    number: None,
                    slot: 0,
                    key: Some("⌘J".into()),
                    works_in_terminals: false,
                },
                KeyboardChange {
                    action: "park".into(),
                    number: None,
                    slot: 0,
                    key: Some("⌘⌘⌘".into()),
                    works_in_terminals: false,
                },
                KeyboardChange {
                    action: "project".into(),
                    number: Some(3),
                    slot: 0,
                    key: Some("⌃3".into()),
                    works_in_terminals: false,
                },
            ],
        };
        let custom = custom_keys(&stored);
        assert_eq!(custom.0.len(), 1);
        assert_eq!(custom.0[0].action, KeyAction::Project(3));
        assert_eq!(stored_changes(MAC, &custom), stored.changes[2..]);
    }

    #[test]
    fn works_inside_terminals_on_windows_and_linux() {
        let fresh = KeyboardSettings::default();
        // Refused for every key a terminal can't lose, with the reason.
        for (text, reason) in [
            (
                "Ctrl+C",
                "Ctrl+C stops the running command in a terminal, so it can’t work inside terminals.",
            ),
            (
                "Ctrl+D",
                "Ctrl+D ends the shell or agent in a terminal, so it can’t work inside terminals.",
            ),
            (
                "Ctrl+Z",
                "Ctrl+Z suspends the running command in a terminal, so it can’t work inside terminals.",
            ),
            (
                "Ctrl+[",
                "Ctrl+[ is Escape in a terminal, so it can’t work inside terminals.",
            ),
            (
                "Ctrl+\\",
                "Ctrl+\\ force-quits the running command in a terminal, so it can’t work inside terminals.",
            ),
            (
                "Ctrl+Shift+C",
                "Ctrl+Shift+C is the terminal’s copy, so it can’t work inside terminals.",
            ),
            (
                "Ctrl+Shift+V",
                "Ctrl+Shift+V is the terminal’s paste, so it can’t work inside terminals.",
            ),
        ] {
            assert_eq!(
                terminal_refusal(WIN, &Key::parse(text).unwrap()).as_deref(),
                Some(reason)
            );
        }
        // Through the edit: Start / move to Dev's Ctrl+D (design s15-54), Previous ticket's Ctrl+[, a recorded
        // Ctrl+Shift+V; each stays off.
        assert_eq!(
            answer(WIN, &fresh, inside(KeyAction::StartDev, 0, true, true)),
            refused_with(
                "Ctrl+D ends the shell or agent in a terminal, so it can’t work inside terminals."
            )
        );
        assert!(matches!(
            answer(
                WIN,
                &fresh,
                inside(KeyAction::PreviousTicket, 0, true, true)
            ),
            KeyEditAnswer::Refused { .. }
        ));
        let paste = ok(WIN, &fresh, record(KeyAction::Park, 0, "Ctrl+Shift+V"));
        assert!(matches!(
            answer(WIN, &paste, inside(KeyAction::Park, 0, true, true)),
            KeyEditAnswer::Refused { .. }
        ));

        // A custom Ctrl+R gets Shift in terminals; ticking warns, then lets it through.
        let r = ok(WIN, &fresh, record(KeyAction::OpenActivity, 0, "Ctrl+R"));
        assert_eq!(
            keys(WIN, &r, KeyAction::OpenActivity)[..2],
            [
                k(0, "Ctrl+R", KeyPlace::OutsideTerminals),
                k(0, "Ctrl+Shift+R", KeyPlace::InsideTerminals),
            ]
        );
        assert_eq!(
            works_in_terminals(WIN, &r, KeyAction::OpenActivity, 0),
            Some(false)
        );
        assert_eq!(
            answer(WIN, &r, inside(KeyAction::OpenActivity, 0, true, false)),
            KeyEditAnswer::Warn {
                warning: "Ctrl+R: the shell’s history search won’t reach the terminal.".into()
            }
        );
        let through = ok(WIN, &r, inside(KeyAction::OpenActivity, 0, true, true));
        assert_eq!(
            keys(WIN, &through, KeyAction::OpenActivity)[0],
            k(0, "Ctrl+R", KeyPlace::Everywhere)
        );
        assert_eq!(
            works_in_terminals(WIN, &through, KeyAction::OpenActivity, 0),
            Some(true)
        );
        assert!(through.changes[0].works_in_terminals);
        // Unticked again: back to the Shift form.
        let off = ok(
            WIN,
            &through,
            inside(KeyAction::OpenActivity, 0, false, false),
        );
        assert_eq!(off, r);
        // A base key ticked: Next item that needs you's Ctrl+J, after the warning; the base map's Ctrl+K is already on.
        assert!(matches!(
            answer(
                WIN,
                &fresh,
                inside(KeyAction::NextNeedsYouItem, 0, true, false)
            ),
            KeyEditAnswer::Warn { .. }
        ));
        let j = ok(
            WIN,
            &fresh,
            inside(KeyAction::NextNeedsYouItem, 0, true, true),
        );
        assert_eq!(
            keys(WIN, &j, KeyAction::NextNeedsYouItem),
            [k(0, "Ctrl+J", KeyPlace::Everywhere)]
        );
        assert_eq!(
            works_in_terminals(WIN, &fresh, KeyAction::CommandPalette, 0),
            Some(true)
        );
        // Letters have no such option.
        assert_eq!(
            works_in_terminals(WIN, &fresh, KeyAction::NewTicket, 1),
            None
        );
        assert!(edit(WIN, &fresh, &inside(KeyAction::NewTicket, 1, true, true)).is_err());
    }

    #[test]
    fn a_taken_shift_form_in_terminals_asks_the_same_question() {
        let fresh = KeyboardSettings::default();
        // Ctrl+U's terminal form is Ctrl+Shift+U, Activity's key.
        let edit = record(KeyAction::Park, 0, "Ctrl+U");
        assert_eq!(
            answer(WIN, &fresh, edit.clone()),
            KeyEditAnswer::Taken {
                key: "Ctrl+Shift+U".into(),
                by: KeyAction::OpenActivity,
                question: "Ctrl+Shift+U is used by Activity. Use it here instead?".into(),
            }
        );
        let moved = ok(WIN, &fresh, confirmed(edit));
        assert_eq!(texts(WIN, &moved, KeyAction::OpenActivity), ["g a"]);
        assert_eq!(texts(WIN, &moved, KeyAction::Park), ["Ctrl+U"]);
    }

    #[test]
    fn works_inside_terminals_on_macos_is_for_control_keys_only() {
        let fresh = KeyboardSettings::default();
        let r = ok(MAC, &fresh, record(KeyAction::OpenActivity, 0, "⌃R"));
        // A ⌃ key stays the terminal's there until ticked.
        assert_eq!(
            keys(MAC, &r, KeyAction::OpenActivity)[0],
            k(0, "⌃R", KeyPlace::OutsideTerminals)
        );
        assert_eq!(
            works_in_terminals(MAC, &r, KeyAction::OpenActivity, 0),
            Some(false)
        );
        assert_eq!(
            answer(MAC, &r, inside(KeyAction::OpenActivity, 0, true, false)),
            KeyEditAnswer::Warn {
                warning: "⌃R: the shell’s history search won’t reach the terminal.".into()
            }
        );
        let through = ok(MAC, &r, inside(KeyAction::OpenActivity, 0, true, true));
        assert_eq!(
            keys(MAC, &through, KeyAction::OpenActivity)[0],
            k(0, "⌃R", KeyPlace::Everywhere)
        );

        let d = ok(MAC, &fresh, record(KeyAction::Park, 0, "⌃D"));
        assert_eq!(
            answer(MAC, &d, inside(KeyAction::Park, 0, true, true)),
            refused_with(
                "⌃D ends the shell or agent in a terminal, so it can’t work inside terminals."
            )
        );
        // A ⌘ key has no such option (it works in terminals anyway).
        assert_eq!(
            works_in_terminals(MAC, &fresh, KeyAction::NextNeedsYouItem, 0),
            None
        );
        assert!(
            edit(
                MAC,
                &fresh,
                &inside(KeyAction::NextNeedsYouItem, 0, true, true)
            )
            .is_err()
        );
        // The base map's ⌃` is on.
        assert_eq!(
            works_in_terminals(MAC, &fresh, KeyAction::Terminals, 0),
            Some(true)
        );
    }

    #[test]
    fn the_page_sends_and_gets_the_shapes_it_names() {
        let e: KeyEdit = serde_json::from_value(serde_json::json!({
            "edit": "record", "action": { "action": "park" }, "slot": 0, "key": "g p"
        }))
        .unwrap();
        assert_eq!(e, record(KeyAction::Park, 0, "g p"));
        let e: KeyEdit =
            serde_json::from_value(serde_json::json!({ "edit": "reset_all" })).unwrap();
        assert_eq!(e, KeyEdit::ResetAll);
        assert_eq!(
            serde_json::to_value(KeyEditAnswer::Refused { reason: "x".into() }).unwrap(),
            serde_json::json!({ "outcome": "refused", "reason": "x" })
        );
    }
}
