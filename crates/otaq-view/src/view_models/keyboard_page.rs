//! Settings → Keyboard shortcuts (T-0395; KEY-05, KEY-04, KEY-02's switch; d0097, d0089; *15 · Keyboard shortcuts*,
//! `s15-46`…`s15-55`): the single-letter switch, then every action grouped as the `?` list shows them, one row each:
//! its name, each key its own cell ("no key" when it has none), "Default: …" under a changed row's name with its
//! changed keys in the accent tint, and the row's reset. Reset all in the header asks first. On Windows/Linux a row
//! with a modifier key has "Works inside terminals" and, while that's off, its in-terminal form; on macOS only a
//! recorded ⌃ key has it (d0097 changelog 2026-10-19).
//!
//! Every check and every answer's text is [`keymap_edit`]'s; this only lays out the user's keys and turns a pressed
//! key into the map's text ([`press_text`]) so the page never formats one. Rows whose actions come in pairs (Next /
//! previous ticket) show both in one cell, `⌘] / ⌘[`, each half recorded on its own; numbered actions (Project 1–9,
//! Switch panel tab) show their range and aren't recorded from here (deferred question, built as option 1).

use otaq_core::board::settings::KeyboardSettings;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::keymap::{
    Binding, CustomKeys, Key, KeyAction, KeyGroup, KeyPlace, Keystroke, Modifiers, Platform,
    action_infos, user_keys,
};
use crate::keymap_edit::{
    self, Edited, KeyEdit, KeyEditAnswer, changed_count, custom_keys, offers_works_in_terminals,
    os_refusal, reset_all_question,
};

/// One key of an action: which action and which of its keys (the map's [`Binding::slot`]). `ShortcutsSlot` on the
/// page.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(rename = "ShortcutsSlot")]
pub struct KeySlot {
    pub action: KeyAction,
    pub slot: u8,
}

/// The whole page. `ShortcutsPage` on the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "ShortcutsPage")]
pub struct Page {
    /// The header's title, the nav label (d0089).
    pub title: String,
    pub intro: String,
    pub reset_all: ResetAll,
    pub letters: LettersSwitch,
    /// Windows/Linux: the note above the rows about "Works inside terminals" (`s15-54`).
    pub terminals_note: Option<String>,
    /// Windows/Linux: the rows have the "Works inside terminals" column (`s15-54`'s widths).
    pub terminals_column: bool,
    pub groups: Vec<Group>,
    pub texts: Texts,
}

/// Reset all in the header and its question (d0097: it asks first; `s15-48`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "ShortcutsResetAll")]
pub struct ResetAll {
    pub label: String,
    /// Only while something is changed.
    pub enabled: bool,
    /// "Reset all 3 changed shortcuts to their defaults?"
    pub question: String,
    /// "Activity, New ticket and Next item that needs you go back to their default keys."
    pub body: String,
    pub cancel: String,
}

/// The single-letter switch (KEY-02).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "ShortcutsLetters")]
pub struct LettersSwitch {
    pub label: String,
    pub description: String,
    pub on: bool,
}

/// A titled group of rows (the `?` list's groups).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "ShortcutsGroup")]
pub struct Group {
    pub title: String,
    pub rows: Vec<Row>,
}

/// One action's row (or a pair's, or a numbered action's).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "ShortcutsRow")]
pub struct Row {
    /// The first action's name (`open_activity`, `next_ticket`, `project`): what the page keys a row's state by.
    pub id: String,
    pub title: String,
    /// "Default: ⇧⌘U or g a", shown under the name while the row is changed (or a new key waits for "Use here").
    pub default_line: String,
    /// Some key of the row differs from the base map.
    pub changed: bool,
    /// Every key of the row is a single letter (or ↵ / Esc) and the switch is off: the name is muted too (`s15-48`).
    pub dimmed: bool,
    pub cells: Vec<Cell>,
    /// The row's reset ("Reset to default"), with the keys it resets; only while it is changed.
    pub reset: Option<Vec<KeySlot>>,
    pub terminals: Option<TerminalsCheck>,
}

/// One cell of a row: a key (a pair's two halves, `⌘] / ⌘[`), or a numbered action's range (`⌘1–9`), which isn't
/// recorded from here.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(rename = "ShortcutsCell")]
pub enum Cell {
    Keys {
        halves: Vec<CellKey>,
    },
    Range {
        text: String,
        changed: bool,
        dimmed: bool,
    },
}

/// One recordable key: click it (or focus it and press ↵) to record a new one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "ShortcutsKey")]
pub struct CellKey {
    pub slot: KeySlot,
    /// The Kbd chips: one ("⌘K", "j"), or `g` and its letter (drawn "g then b"); none: "no key".
    pub chips: Vec<String>,
    /// Recorded by the user: drawn in the accent tint.
    pub changed: bool,
    /// A single letter or sequence while the switch is off: dimmed in place, kept.
    pub dimmed: bool,
}

/// "Works inside terminals" for one key of the row (`s15-54`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "ShortcutsTerminals")]
pub struct TerminalsCheck {
    pub slot: KeySlot,
    pub on: bool,
    /// The key reaches the app in a terminal whatever the box says (a Ctrl+Shift key on Windows/Linux, or one that
    /// works only there): drawn on and disabled.
    pub fixed: bool,
    /// While off: "· in terminals Ctrl+Shift+N".
    pub in_terminals: Option<String>,
}

/// The words the page draws around the rows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "ShortcutsTexts")]
pub struct Texts {
    pub or: String,
    pub then: String,
    pub no_key: String,
    pub recording: String,
    pub after_g: String,
    /// Under the recording row: "Esc cancels · Remove key takes this key away"; without a key: "Esc cancels".
    pub recording_hint: String,
    pub recording_hint_no_key: String,
    pub remove_key: String,
    pub reset: String,
    pub works_in_terminals: String,
    pub use_here: String,
    pub cancel: String,
    pub let_through: String,
}

const INTRO: &str = "Every action and its keys. Click a key to record new ones: any key, a single letter, or g then a letter.";
const LETTERS_DESCRIPTION: &str = "Keys like j, k, / and g then b. They work only while you aren\u{2019}t typing in a \
     field, the editor, the Project agent\u{2019}s box or a terminal. Turn them off if you press keys by accident or \
     use speech input.";
const TERMINALS_NOTE: &str = "A shortcut that works inside terminals takes that key from the shell or agent while a \
     terminal has focus.";

fn texts() -> Texts {
    Texts {
        or: "or".into(),
        then: "then".into(),
        no_key: "no key".into(),
        recording: "Press the new keys\u{2026}".into(),
        after_g: "g, then a letter\u{2026}".into(),
        recording_hint: "Esc cancels \u{b7} Remove key takes this key away".into(),
        recording_hint_no_key: "Esc cancels".into(),
        remove_key: "Remove key".into(),
        reset: "Reset to default".into(),
        works_in_terminals: "Works inside terminals".into(),
        use_here: "Use here".into(),
        cancel: "Cancel".into(),
        let_through: "Let it through".into(),
    }
}

/// The column the page is drawn for: macOS, or Windows/Linux (whose keys carry their terminal forms).
fn column(platform: Platform) -> Platform {
    match platform {
        Platform::WinLinuxTerminal => Platform::WinLinux,
        other => other,
    }
}

/// The actions listed on one row: one, a pair sharing a title (Next / previous ticket), or a numbered action's
/// numbers (Project 1–9).
fn row_actions() -> Vec<Vec<KeyAction>> {
    let mut rows: Vec<Vec<KeyAction>> = Vec::new();
    for info in action_infos() {
        match rows.last_mut() {
            Some(last) if last[0].title() == info.title => last.push(info.action),
            _ => rows.push(vec![info.action]),
        }
    }
    rows
}

fn numbered(action: KeyAction) -> bool {
    matches!(action, KeyAction::Project(_) | KeyAction::SwitchTab(_))
}

/// The action's name as the map writes it (`open_activity`).
fn action_id(action: KeyAction) -> String {
    serde_json::to_value(action)
        .ok()
        .and_then(|v| v["action"].as_str().map(str::to_owned))
        .unwrap_or_default()
}

/// The base map's keys of an action in this column, by slot; on Windows/Linux a key that only works in terminals
/// (Find in terminal scrollback) is its terminal form.
fn default_texts(platform: Platform, action: KeyAction) -> Vec<String> {
    action
        .base_keys()
        .into_iter()
        .map(|t| {
            if platform == Platform::Mac {
                t[0].clone()
            } else if t[1].is_empty() {
                t[2].clone()
            } else {
                t[1].clone()
            }
        })
        .filter(|t| !t.is_empty())
        .collect()
}

/// The action's slots: its base keys and the ones the user added.
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

/// The binding a cell shows for a slot: the one outside terminals, else its terminal form.
fn shown(bindings: &[Binding], action: KeyAction, slot: u8) -> Option<&Binding> {
    let mine = || {
        bindings
            .iter()
            .filter(|b| b.action == action && b.slot == slot)
    };
    mine()
        .find(|b| b.place.outside_terminals())
        .or_else(|| mine().next())
}

/// A key the letters switch turns off: a single letter, a sequence, or a key with no modifier (↵ / Esc, Open / close
/// the sheet), as the dispatcher and the `?` list treat them (KEY-02).
fn plain(key: &Key) -> bool {
    key.stroke()
        .is_none_or(|s| s.modifiers == Modifiers::default())
}

fn chips(key: &Key, text: &str) -> Vec<String> {
    match key {
        Key::Sequence { then } => vec!["g".to_owned(), then.clone()],
        _ => vec![text.to_owned()],
    }
}

struct Ctx<'a> {
    platform: Platform,
    custom: &'a CustomKeys,
    bindings: &'a [Binding],
    letters: bool,
}

impl Ctx<'_> {
    fn changed(&self, action: KeyAction, slot: u8) -> bool {
        self.custom
            .0
            .iter()
            .any(|c| c.action == action && c.slot == slot)
    }

    fn key(&self, action: KeyAction, slot: u8) -> Option<CellKey> {
        let b = shown(self.bindings, action, slot)?;
        Some(CellKey {
            slot: KeySlot { action, slot },
            chips: chips(&b.key, &b.text),
            changed: self.changed(action, slot),
            dimmed: !self.letters && plain(&b.key),
        })
    }

    /// "no key" for an action: records into its first removed key, or a new one.
    fn no_key(&self, action: KeyAction, at: usize) -> CellKey {
        let slot = (0..slot_count(self.custom, action))
            .find(|s| {
                u8::try_from(*s).is_ok_and(|s| shown(self.bindings, action, s).is_none())
                    && *s >= at
            })
            .unwrap_or_else(|| slot_count(self.custom, action));
        CellKey {
            slot: KeySlot {
                action,
                slot: u8::try_from(slot).unwrap_or(u8::MAX),
            },
            chips: Vec::new(),
            changed: false,
            dimmed: false,
        }
    }

    /// The cells of one action or of a pair, slot by slot; "no key" when none is left.
    fn cells(&self, actions: &[KeyAction]) -> Vec<Cell> {
        let cells = self.cells_from(actions, 0);
        if cells.is_empty() {
            return vec![Cell::Keys {
                halves: actions.iter().map(|a| self.no_key(*a, 0)).collect(),
            }];
        }
        cells
    }

    /// The cells of the slots from `from` on, each recordable member by member (a numbered row's after its range:
    /// the sheet's tabs `g o / s / f / a`).
    fn cells_from(&self, actions: &[KeyAction], from: usize) -> Vec<Cell> {
        let slots = actions
            .iter()
            .map(|a| slot_count(self.custom, *a))
            .max()
            .unwrap_or(0);
        let mut cells = Vec::new();
        for slot in from..slots {
            let Ok(s) = u8::try_from(slot) else { break };
            let halves: Vec<Option<CellKey>> = actions.iter().map(|a| self.key(*a, s)).collect();
            if halves.iter().all(Option::is_none) {
                continue;
            }
            cells.push(Cell::Keys {
                halves: halves
                    .into_iter()
                    .zip(actions)
                    .map(|(h, a)| h.unwrap_or_else(|| self.no_key(*a, slot)))
                    .collect(),
            });
        }
        cells
    }

    /// A numbered action's range: "⌘1–9" while every number has its default key, else each number's key.
    fn range(&self, actions: &[KeyAction]) -> Cell {
        let changed = actions
            .iter()
            .any(|a| self.custom.0.iter().any(|c| c.action == *a));
        let dimmed = !self.letters
            && actions
                .iter()
                .any(|a| shown(self.bindings, *a, 0).is_some_and(|b| plain(&b.key)));
        let text = if changed {
            actions
                .iter()
                .flat_map(|a| {
                    self.bindings
                        .iter()
                        .filter(|b| b.action == *a && b.place.outside_terminals())
                        .map(|b| b.text.clone())
                })
                .collect::<Vec<_>>()
                .join(" ")
        } else {
            range_text(self.platform, actions)
        };
        Cell::Range {
            text,
            changed,
            dimmed,
        }
    }

    /// "Works inside terminals" for the row's first key that has it: on Windows/Linux a key with a modifier, on
    /// macOS a recorded ⌃ key.
    fn terminals(&self, actions: &[KeyAction]) -> Option<TerminalsCheck> {
        let first = actions[0];
        let b = (0..slot_count(self.custom, first))
            .filter_map(|s| u8::try_from(s).ok())
            .filter_map(|s| shown(self.bindings, first, s))
            .find(|b| {
                offers_works_in_terminals(self.platform, &b.key)
                    && (self.platform != Platform::Mac || b.custom)
            })?;
        let mine = || {
            self.bindings
                .iter()
                .filter(|o| o.action == first && o.slot == b.slot)
        };
        // On: the shown key itself reaches the app in a terminal (not only its Shift form).
        let on = mine().any(|o| o.key == b.key && o.place.inside_terminals());
        let only_inside = !mine().any(|o| o.place.outside_terminals());
        let always = self.platform != Platform::Mac
            && b.key.stroke().is_some_and(|s| {
                (s.modifiers.control && s.modifiers.shift) || s.modifiers.platform
            });
        let inside = mine()
            .find(|o| o.place == KeyPlace::InsideTerminals)
            .map(|o| format!("\u{b7} in terminals {}", o.text));
        Some(TerminalsCheck {
            slot: KeySlot {
                action: first,
                slot: b.slot,
            },
            on,
            fixed: only_inside || always,
            in_terminals: if on { None } else { inside },
        })
    }

    fn row(&self, actions: &[KeyAction]) -> Row {
        let first = actions[0];
        let range = numbered(first);
        let changed_slots: Vec<KeySlot> = self
            .custom
            .0
            .iter()
            .filter(|c| actions.contains(&c.action))
            .map(|c| KeySlot {
                action: c.action,
                slot: c.slot,
            })
            .collect();
        let changed = !changed_slots.is_empty();
        let cells = if range {
            std::iter::once(self.range(actions))
                .chain(self.cells_from(actions, 1))
                .collect()
        } else {
            self.cells(actions)
        };
        let dimmed = cells.iter().all(|c| match c {
            Cell::Range { dimmed, .. } => *dimmed,
            Cell::Keys { halves } => halves.iter().all(|h| h.dimmed),
        });
        Row {
            id: action_id(first),
            title: first.title().to_owned(),
            default_line: format!("Default: {}", default_line(self.platform, actions)),
            changed,
            dimmed,
            cells,
            reset: changed.then_some(changed_slots),
            terminals: if range { None } else { self.terminals(actions) },
        }
    }
}

/// `⌘1–9`, `Ctrl+1–9`, `1–6`: the first number's default key with the range in place of its number.
fn range_text(platform: Platform, actions: &[KeyAction]) -> String {
    let first = default_texts(platform, actions[0])
        .into_iter()
        .next()
        .unwrap_or_default();
    format!("{}1\u{2013}{}", first.trim_end_matches('1'), actions.len())
}

/// The row's default keys: "⇧⌘U or g a", "⌘] / ⌘[ or j / k", "⌘1–9", or "no key".
fn default_line(platform: Platform, actions: &[KeyAction]) -> String {
    if numbered(actions[0]) {
        return range_text(platform, actions);
    }
    let each: Vec<Vec<String>> = actions
        .iter()
        .map(|a| default_texts(platform, *a))
        .collect();
    let slots = each.iter().map(Vec::len).max().unwrap_or(0);
    let keys: Vec<String> = (0..slots)
        .map(|i| {
            each.iter()
                .filter_map(|keys| keys.get(i).cloned())
                .collect::<Vec<_>>()
                .join(" / ")
        })
        .collect();
    if keys.is_empty() {
        texts().no_key
    } else {
        keys.join(" or ")
    }
}

/// "A, B and C".
fn listed(names: &[String]) -> String {
    match names {
        [] => String::new(),
        [one] => one.clone(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

/// The page for `platform` from the stored keyboard settings.
pub fn page(platform: Platform, stored: &KeyboardSettings) -> Page {
    let platform = column(platform);
    let custom = custom_keys(stored);
    let bindings = user_keys(platform, &custom);
    let ctx = Ctx {
        platform,
        custom: &custom,
        bindings: &bindings,
        letters: stored.letters,
    };
    let rows: Vec<(KeyGroup, Row)> = row_actions()
        .iter()
        .map(|actions| (actions[0].group(), ctx.row(actions)))
        .collect();
    let groups = KeyGroup::ALL
        .iter()
        .map(|group| Group {
            title: group.title().to_owned(),
            rows: rows
                .iter()
                .filter(|(g, _)| g == group)
                .map(|(_, r)| r.clone())
                .collect(),
        })
        .collect();
    let mut changed_rows: Vec<String> = rows
        .iter()
        .filter(|(_, r)| r.changed)
        .map(|(_, r)| r.title.clone())
        .collect();
    changed_rows.sort();
    let count = changed_count(stored);
    let body = match changed_rows.as_slice() {
        [] => String::new(),
        [one] => format!("{one} goes back to its default keys."),
        many => format!("{} go back to their default keys.", listed(many)),
    };
    let win = platform != Platform::Mac;
    Page {
        title: "Keyboard shortcuts".into(),
        intro: INTRO.into(),
        reset_all: ResetAll {
            label: "Reset all".into(),
            enabled: count > 0,
            question: reset_all_question(count),
            body,
            cancel: "Cancel".into(),
        },
        letters: LettersSwitch {
            label: "Single-letter shortcuts".into(),
            description: LETTERS_DESCRIPTION.into(),
            on: stored.letters,
        },
        terminals_note: win.then(|| TERMINALS_NOTE.into()),
        terminals_column: win,
        groups,
        texts: texts(),
    }
}

/// A key as the page reads it from a key event (`KeyboardEvent.key`, `.code` and the modifiers held), while a cell
/// records. `ShortcutsPress` on the page.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS)]
#[ts(rename = "ShortcutsPress")]
pub struct Press {
    pub key: String,
    pub code: String,
    pub modifiers: Modifiers,
}

/// What a pressed key is as a key of the map.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PressText {
    /// A lone `g`: the cell waits for the letter ("g, then a letter…").
    ThenLetter,
    /// The key's text as the map writes it ("⌘J", "Ctrl+R", "n", "g p", "g" when `g` is followed by no letter),
    /// or an OS key's ("⌘Space", "Alt+F4") so it is refused with its reason.
    Text(String),
    /// Nothing the map can hold (an arrow, F5 alone): the cell keeps recording.
    Nothing,
}

/// The US keyboard's character of a physical key (`KeyboardEvent.code`), for keys held with modifiers: ⌘[ stays ⌘[
/// on a German keyboard, ⌥1 is ⌥1 though a Mac types `¡`.
fn char_of_code(code: &str) -> Option<String> {
    if let Some(l) = code.strip_prefix("Key")
        && l.len() == 1
    {
        return Some(l.to_ascii_lowercase());
    }
    if let Some(d) = code.strip_prefix("Digit")
        && d.len() == 1
    {
        return Some(d.to_owned());
    }
    Some(
        match code {
            "BracketLeft" => "[",
            "BracketRight" => "]",
            "Backquote" => "`",
            "Backslash" => "\\",
            "Comma" => ",",
            "Period" => ".",
            "Slash" => "/",
            "Semicolon" => ";",
            "Quote" => "'",
            "Equal" => "=",
            "Minus" => "-",
            _ => return None,
        }
        .to_owned(),
    )
}

/// The letter a key types: a Latin letter by the layout (⌘K is K on a Dvorak keyboard too), or where the layout
/// types none (Cyrillic), the physical key's.
fn latin_letter(press: &Press) -> Option<String> {
    let mut chars = press.key.chars();
    if let (Some(c), None) = (chars.next(), chars.next()) {
        if c.is_ascii_alphabetic() {
            return Some(c.to_ascii_lowercase().to_string());
        }
        if c.is_ascii() {
            return None;
        }
    }
    press
        .code
        .strip_prefix("Key")
        .filter(|l| l.len() == 1)
        .map(str::to_ascii_lowercase)
}

/// A named key as the map writes it, for the ones it can hold or that an OS key needs (Space, F4).
fn named(key: &str) -> Option<&'static str> {
    Some(match key {
        "Enter" => "Enter",
        "Backspace" => "Backspace",
        "Tab" => "Tab",
        "Escape" => "Escape",
        " " => "Space",
        "F4" => "F4",
        _ => return None,
    })
}

/// A pressed key as the map's text (the page sends the event, the text is the map's, [`Keystroke::text`]). Letters
/// match by the character typed (KEY-02): a Latin letter without Shift is the letter, any other character is itself;
/// `after_g`: the press follows a lone `g`, so a letter makes a sequence and anything else leaves plain `g`.
pub fn press_text(platform: Platform, press: &Press, after_g: bool) -> PressText {
    let platform = column(platform);
    let m = press.modifiers;
    let plain = !m.control && !m.alt && !m.platform;
    let letter = if plain && !m.shift {
        latin_letter(press).or_else(|| {
            let mut chars = press.key.chars();
            match (chars.next(), chars.next()) {
                (Some(c), None) if c.is_ascii_graphic() => Some(c.to_string()),
                _ => None,
            }
        })
    } else if plain && latin_letter(press).is_none() {
        // Shift and a sign: the sign typed (`?` is ⇧/ on a US keyboard).
        let mut chars = press.key.chars();
        match (chars.next(), chars.next()) {
            (Some(c), None) if c.is_ascii_graphic() => Some(c.to_string()),
            _ => None,
        }
    } else {
        None
    };
    if after_g {
        return PressText::Text(match letter {
            Some(l) if Key::parse(&format!("g {l}")).is_some() => format!("g {l}"),
            _ => "g".to_owned(),
        });
    }
    if let Some(l) = letter {
        if l == "g" {
            return PressText::ThenLetter;
        }
        return if Key::parse(&l).is_some() {
            PressText::Text(l)
        } else {
            PressText::Nothing
        };
    }
    let key = latin_letter(press)
        .or_else(|| named(&press.key).map(str::to_owned))
        .or_else(|| char_of_code(&press.code));
    let Some(key) = key else {
        return PressText::Nothing;
    };
    let text = Keystroke {
        key,
        code: press.code.clone(),
        modifiers: Modifiers {
            function: false,
            ..m
        },
    }
    .text(platform);
    if os_refusal(platform, &text).is_some() || Key::parse(&text).is_some() {
        PressText::Text(text)
    } else {
        PressText::Nothing
    }
}

/// What recording one pressed key answers. `ShortcutsRecordAnswer` on the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(tag = "outcome", rename_all = "snake_case")]
#[ts(rename = "ShortcutsRecordAnswer")]
pub enum RecordAnswer {
    /// A lone `g`: the cell now waits for the letter.
    ThenLetter,
    /// Nothing the map can hold: the cell keeps recording.
    Ignored,
    /// The key as recorded ("⌘J"; what "Use here" sends again) and the edit's answer.
    Edited { key: String, answer: KeyEditAnswer },
}

/// Records a pressed key for `slot`: its text ([`press_text`]), then the edit ([`keymap_edit::edit`]).
pub fn record(
    platform: Platform,
    stored: &KeyboardSettings,
    slot: KeySlot,
    press: &Press,
    after_g: bool,
) -> Result<(RecordAnswer, Option<KeyboardSettings>), String> {
    match press_text(platform, press, after_g) {
        PressText::ThenLetter => Ok((RecordAnswer::ThenLetter, None)),
        PressText::Nothing => Ok((RecordAnswer::Ignored, None)),
        PressText::Text(key) => {
            let edited = keymap_edit::edit(
                platform,
                stored,
                &KeyEdit::Record {
                    action: slot.action,
                    slot: slot.slot,
                    key: key.clone(),
                    confirmed: false,
                },
            )?;
            Ok((
                RecordAnswer::Edited {
                    key,
                    answer: edited.answer,
                },
                edited.store,
            ))
        }
    }
}

/// A row's reset: each of its changed keys back to its default, all or none. A default another action has now asks
/// first, as recording does (`confirmed` once the user said "Use here").
pub fn reset_row(
    platform: Platform,
    stored: &KeyboardSettings,
    keys: &[KeySlot],
    confirmed: bool,
) -> Result<Edited, String> {
    if keys.is_empty() {
        return Err("nothing to reset".into());
    }
    let mut now = stored.clone();
    let mut last = None;
    for key in keys {
        let edited = keymap_edit::edit(
            platform,
            &now,
            &KeyEdit::Reset {
                action: key.action,
                slot: key.slot,
                confirmed,
            },
        )?;
        match edited.store {
            Some(store) => now = store,
            None => return Ok(edited),
        }
        last = Some(edited.answer);
    }
    Ok(Edited {
        answer: last.unwrap_or(KeyEditAnswer::Done {
            changed: changed_count(&now),
        }),
        store: Some(now),
    })
}

/// What a cell shows besides the page: the key being recorded, and an answer under its row (the question, the
/// reason, the warning). The page keeps it while you interact; the gallery draws the frames' moments with it.
/// `ShortcutsMoment` on the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "ShortcutsMoment")]
pub struct Moment {
    pub recording: Option<KeySlot>,
    pub after_g: bool,
    pub answer: Option<RowAnswer>,
}

/// An answer shown under its row. `ShortcutsRowAnswer` on the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "ShortcutsRowAnswer")]
pub struct RowAnswer {
    pub row: String,
    pub slot: KeySlot,
    /// The recorded key that waits for "Use here", drawn in its cell.
    pub key: Option<String>,
    pub answer: KeyEditAnswer,
}

/// One frame of the gallery: a page (or some of its rows) in one moment. `ShortcutsGalleryCell` on the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "ShortcutsGalleryCell")]
pub struct GalleryCell {
    pub name: String,
    /// The frame's caption.
    pub note: String,
    pub page: Page,
    /// The rows drawn, by id; empty: the whole page with its header and switch.
    pub rows: Vec<String>,
    pub moment: Moment,
    /// The header (its title and Reset all) is drawn above the rows.
    pub header: bool,
}

/// `s15-46`…`s15-55` for the gallery. `ShortcutsGallery` on the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "ShortcutsGallery")]
pub struct Gallery {
    pub cells: Vec<GalleryCell>,
}

fn done(platform: Platform, stored: &KeyboardSettings, edit: KeyEdit) -> KeyboardSettings {
    keymap_edit::edit(platform, stored, &edit)
        .ok()
        .and_then(|e| e.store)
        .unwrap_or_else(|| stored.clone())
}

fn record_edit(action: KeyAction, slot: u8, key: &str, confirmed: bool) -> KeyEdit {
    KeyEdit::Record {
        action,
        slot,
        key: key.into(),
        confirmed,
    }
}

fn at(action: KeyAction, slot: u8) -> KeySlot {
    KeySlot { action, slot }
}

/// The frames' samples (d0097's example): Activity → ⌘J from Next item that needs you, New ticket's `c` → `n`; the refusals,
/// the Windows / Linux rows with Ctrl+R and Ctrl+D.
pub fn gallery() -> Gallery {
    use KeyAction::*;
    const MAC: Platform = Platform::Mac;
    const WIN: Platform = Platform::WinLinux;
    let none = KeyboardSettings::default();
    let changed = {
        let s = done(MAC, &none, record_edit(OpenActivity, 0, "⌘J", true));
        done(MAC, &s, record_edit(NewTicket, 1, "n", false))
    };
    let activity_j = done(MAC, &none, record_edit(OpenActivity, 0, "⌘J", true));
    let n_only = done(MAC, &none, record_edit(NewTicket, 1, "n", false));
    let needs_you_removed = done(
        MAC,
        &none,
        KeyEdit::Remove {
            action: NextNeedsYouItem,
            slot: 0,
        },
    );
    let letters_off = KeyboardSettings {
        letters: false,
        ..none.clone()
    };
    let moment = |recording: Option<KeySlot>, after_g, answer| Moment {
        recording,
        after_g,
        answer,
    };
    let idle = moment(None, false, None);
    let answer_of = |platform, stored: &KeyboardSettings, edit: KeyEdit| {
        keymap_edit::edit(platform, stored, &edit)
            .map(|e| e.answer)
            .unwrap_or(KeyEditAnswer::Refused {
                reason: String::new(),
            })
    };
    let under = |row: &str, slot: KeySlot, key: Option<&str>, answer| {
        Some(RowAnswer {
            row: row.into(),
            slot,
            key: key.map(str::to_owned),
            answer,
        })
    };
    let refused = |row: &str, action, key: &str| {
        moment(
            Some(at(action, 0)),
            false,
            under(
                row,
                at(action, 0),
                None,
                answer_of(MAC, &none, record_edit(action, 0, key, false)),
            ),
        )
    };
    let cell = |name: &str,
                note: &str,
                platform: Platform,
                stored: &KeyboardSettings,
                rows: &[&str],
                moment: Moment| GalleryCell {
        name: name.into(),
        note: note.into(),
        page: page(platform, stored),
        rows: rows.iter().map(|r| (*r).to_owned()).collect(),
        header: rows.is_empty(),
        moment,
    };
    let win_ctrl_r = done(WIN, &none, record_edit(OpenActivity, 0, "Ctrl+R", false));
    let cells = vec![
        cell(
            "All defaults",
            "s15-46 · the switch on, every action with its default keys; Reset all unavailable",
            MAC,
            &none,
            &[],
            idle.clone(),
        ),
        cell(
            "Some shortcuts changed",
            "s15-46 · Activity has ⌘J (Use here took it from Next item that needs you), New ticket c → n",
            MAC,
            &changed,
            &[],
            idle.clone(),
        ),
        cell(
            "Single letters switched off",
            "s15-48 · every letter dimmed in place, kept",
            MAC,
            &letters_off,
            &[],
            idle.clone(),
        ),
        GalleryCell {
            header: true,
            ..cell(
                "Reset all asks first",
                "s15-48 · Reset all opens the question, with how many and which",
                MAC,
                &changed,
                &["next_needs_you_item", "open_activity", "new_ticket"],
                idle.clone(),
            )
        },
        cell(
            "Recording a new key",
            "s15-50 · only the clicked cell records; Remove key beside it",
            MAC,
            &none,
            &["new_ticket", "open_activity", "terminals"],
            moment(Some(at(NewTicket, 1)), false, None),
        ),
        cell(
            "After recording the letter",
            "s15-50 · c → n: New ticket is ⌘N or n",
            MAC,
            &n_only,
            &["new_ticket", "open_activity"],
            idle.clone(),
        ),
        cell(
            "Remove key",
            "s15-50 · recording Next item that needs you's ⌘J",
            MAC,
            &none,
            &["next_needs_you_item"],
            moment(Some(at(NextNeedsYouItem, 0)), false, None),
        ),
        cell(
            "After Remove key",
            "s15-50 · no key, with its reset",
            MAC,
            &needs_you_removed,
            &["next_needs_you_item"],
            idle.clone(),
        ),
        cell(
            "Recording g, then a letter",
            "s15-50 · after g the cell waits for the letter",
            MAC,
            &none,
            &["open_activity", "terminals"],
            moment(Some(at(OpenActivity, 1)), true, None),
        ),
        cell(
            "A key that's already taken",
            "s15-50 · Activity is given ⌘J: the question in place",
            MAC,
            &none,
            &["open_activity", "next_needs_you_item"],
            moment(
                None,
                false,
                under(
                    "open_activity",
                    at(OpenActivity, 0),
                    Some("⌘J"),
                    answer_of(MAC, &none, record_edit(OpenActivity, 0, "⌘J", false)),
                ),
            ),
        ),
        cell(
            "After \u{201c}Use here\u{201d}",
            "s15-50 · Activity ⌘J, Next item that needs you no key",
            MAC,
            &activity_j,
            &["open_activity", "next_needs_you_item"],
            idle.clone(),
        ),
        cell(
            "Refused: a key the system uses",
            "s15-52 · ⌘Q",
            MAC,
            &none,
            &["open_activity"],
            refused("open_activity", OpenActivity, "⌘Q"),
        ),
        cell(
            "Refused: a text-editing key",
            "s15-52 · ⌘C",
            MAC,
            &none,
            &["open_activity"],
            refused("open_activity", OpenActivity, "⌘C"),
        ),
        cell(
            "Refused: g on its own",
            "s15-52 · plain g",
            MAC,
            &none,
            &["park"],
            refused("park", Park, "g"),
        ),
        cell(
            "Actions with no key",
            "s15-52 · the palette runs them; click no key to record one",
            MAC,
            &none,
            &[
                "park",
                "close_ticket",
                "open_pr",
                "rerun_validation",
                "open_diff",
            ],
            idle.clone(),
        ),
        cell(
            "Windows and Linux: Works inside terminals",
            "s15-54 · on, the key reaches the app in a terminal; off, its in-terminal form",
            WIN,
            &none,
            &[
                "command_palette",
                "next_needs_you_item",
                "new_ticket",
                "open_activity",
                "terminals",
                "find_in_terminal",
            ],
            idle.clone(),
        ),
        cell(
            "Letting a Ctrl+letter key through",
            "s15-54 · Activity re-recorded as Ctrl+R, the warning",
            WIN,
            &win_ctrl_r,
            &["open_activity"],
            moment(
                None,
                false,
                under(
                    "open_activity",
                    at(OpenActivity, 0),
                    None,
                    answer_of(
                        WIN,
                        &win_ctrl_r,
                        KeyEdit::WorksInTerminals {
                            action: OpenActivity,
                            slot: 0,
                            on: true,
                            let_through: false,
                            confirmed: false,
                        },
                    ),
                ),
            ),
        ),
        cell(
            "Refused inside terminals",
            "s15-54 · Ctrl+D can’t work inside terminals",
            WIN,
            &none,
            &["start_dev"],
            moment(
                None,
                false,
                under(
                    "start_dev",
                    at(StartDev, 0),
                    None,
                    answer_of(
                        WIN,
                        &none,
                        KeyEdit::WorksInTerminals {
                            action: StartDev,
                            slot: 0,
                            on: true,
                            let_through: false,
                            confirmed: false,
                        },
                    ),
                ),
            ),
        ),
        cell(
            "macOS: a recorded ⌃ key",
            "0097 changelog · the checkbox only on a ⌃ row (Activity → ⌃R)",
            MAC,
            &done(MAC, &none, record_edit(OpenActivity, 0, "⌃R", false)),
            &["open_activity", "terminals"],
            idle,
        ),
    ];
    Gallery { cells }
}

#[cfg(test)]
mod tests {
    use super::*;
    use KeyAction::*;

    const MAC: Platform = Platform::Mac;
    const WIN: Platform = Platform::WinLinux;

    fn row<'a>(page: &'a Page, id: &str) -> &'a Row {
        page.groups
            .iter()
            .flat_map(|g| &g.rows)
            .find(|r| r.id == id)
            .unwrap_or_else(|| panic!("no row {id}"))
    }

    /// The row's cells as the page draws them: "⌘K | ⇧⌘P", "g then a", "⌘] / ⌘[ | j / k", "no key".
    fn drawn(r: &Row) -> Vec<String> {
        r.cells
            .iter()
            .map(|c| match c {
                Cell::Range { text, .. } => text.clone(),
                Cell::Keys { halves } => halves
                    .iter()
                    .map(|h| {
                        if h.chips.is_empty() {
                            "no key".to_owned()
                        } else {
                            h.chips.join(" then ")
                        }
                    })
                    .collect::<Vec<_>>()
                    .join(" / "),
            })
            .collect()
    }

    fn keys(c: &Cell) -> &[CellKey] {
        match c {
            Cell::Keys { halves } => halves,
            Cell::Range { .. } => panic!("a range"),
        }
    }

    fn ok(platform: Platform, stored: &KeyboardSettings, edit: KeyEdit) -> KeyboardSettings {
        let e = keymap_edit::edit(platform, stored, &edit).unwrap();
        e.store
            .unwrap_or_else(|| panic!("{edit:?}: {:?}", e.answer))
    }

    fn rec(action: KeyAction, slot: u8, key: &str, confirmed: bool) -> KeyEdit {
        record_edit(action, slot, key, confirmed)
    }

    fn press(key: &str, code: &str, m: Modifiers) -> Press {
        Press {
            key: key.into(),
            code: code.into(),
            modifiers: m,
        }
    }

    const NONE: Modifiers = Modifiers {
        control: false,
        alt: false,
        shift: false,
        platform: false,
        function: false,
    };
    const CMD: Modifiers = Modifiers {
        platform: true,
        ..NONE
    };
    const CTRL: Modifiers = Modifiers {
        control: true,
        ..NONE
    };
    const SHIFT: Modifiers = Modifiers {
        shift: true,
        ..NONE
    };

    /// `s15-46`: every group and row in the design's order with the macOS defaults; no row changed, no reset,
    /// Reset all unavailable; Files (⇧⌘O, g f) under Views; Open diff last under The ticket (Q222).
    #[test]
    fn all_defaults() {
        let p = page(MAC, &KeyboardSettings::default());
        assert_eq!(p.intro, INTRO);
        assert!(!p.reset_all.enabled);
        assert!(p.letters.on);
        assert_eq!(p.letters.label, "Single-letter shortcuts");
        assert_eq!(p.terminals_note, None);
        let titles: Vec<&str> = p.groups.iter().map(|g| g.title.as_str()).collect();
        assert_eq!(
            titles,
            ["Moving around", "Views", "The ticket", "The terminal"]
        );
        let all: Vec<(String, Vec<String>)> = p
            .groups
            .iter()
            .flat_map(|g| &g.rows)
            .map(|r| (r.title.clone(), drawn(r)))
            .collect();
        let want: Vec<(&str, Vec<&str>)> = vec![
            ("Command palette", vec!["⌘K", "⇧⌘P"]),
            ("Switch project", vec!["⌘P"]),
            ("Project 1–9", vec!["⌘1–9"]),
            ("Next item that needs you", vec!["⌘J"]),
            ("Next / previous ticket", vec!["⌘] / ⌘[", "j / k"]),
            ("Filter the board", vec!["/"]),
            ("Focus the epic of the selected card", vec!["g then e"]),
            ("Show all shortcuts", vec!["?"]),
            ("Board", vec!["⇧⌘B", "g then b"]),
            ("List", vec!["g then l"]),
            ("Files", vec!["⇧⌘O", "g then f"]),
            ("Pause / Resume agents", vec!["⇧⌘."]),
            ("Activity", vec!["⇧⌘U", "g then a"]),
            ("Settings", vec!["⌘,"]),
            ("Open / close the Project agent", vec!["⌘L"]),
            ("New Project agent conversation", vec!["⇧⌘L"]),
            ("New ticket", vec!["⌘N", "c"]),
            ("Open / close the ticket", vec!["↵ / Esc"]),
            ("Full page / back to the sheet", vec!["e"]),
            (
                "Ticket tabs",
                vec!["1–4", "g then o / g then s / g then f / g then a"],
            ),
            ("Go to file", vec!["⌥⌘P"]),
            ("Next / previous ticket tab", vec!["⌃Tab / ⌃⇧Tab"]),
            ("Approve ▾", vec!["⌘↵"]),
            ("Send back", vec!["⌘⌫"]),
            ("Start / move to Dev", vec!["⌘D"]),
            ("Park", vec!["no key"]),
            ("Close ticket", vec!["no key"]),
            ("Open PR", vec!["no key"]),
            ("Re-run validation", vec!["no key"]),
            ("Open diff", vec!["no key"]),
            (
                "Terminals: open, close, leave the terminal",
                vec!["⌃`", "g then t"],
            ),
            ("Find in terminal scrollback", vec!["⌘F"]),
        ];
        let want: Vec<(String, Vec<String>)> = want
            .into_iter()
            .map(|(t, c)| (t.to_owned(), c.into_iter().map(str::to_owned).collect()))
            .collect();
        assert_eq!(all, want);
        for r in p.groups.iter().flat_map(|g| &g.rows) {
            assert!(!r.changed, "{}", r.title);
            assert_eq!(r.reset, None);
            assert_eq!(r.terminals, None, "macOS: no checkbox on a default key");
            for c in &r.cells {
                if let Cell::Keys { halves } = c {
                    assert!(halves.iter().all(|h| !h.changed && !h.dimmed));
                }
            }
        }
        assert_eq!(row(&p, "open_activity").default_line, "Default: ⇧⌘U or g a");
        assert_eq!(
            row(&p, "next_ticket").default_line,
            "Default: ⌘] / ⌘[ or j / k"
        );
        assert_eq!(row(&p, "park").default_line, "Default: no key");
    }

    /// `s15-47`: Activity took ⌘J (Use here), New ticket's `c` → `n`: Default lines, the accent on the changed
    /// cells only, the resets, Reset all with its count and names.
    #[test]
    fn some_changed() {
        let s = ok(
            MAC,
            &KeyboardSettings::default(),
            rec(OpenActivity, 0, "⌘J", true),
        );
        let s = ok(MAC, &s, rec(NewTicket, 1, "n", false));
        let p = page(MAC, &s);
        let activity = row(&p, "open_activity");
        assert!(activity.changed);
        assert_eq!(drawn(activity), ["⌘J", "g then a"]);
        assert!(keys(&activity.cells[0])[0].changed);
        assert!(!keys(&activity.cells[1])[0].changed);
        assert_eq!(activity.reset, Some(vec![at(OpenActivity, 0)]));
        let needs_you = row(&p, "next_needs_you_item");
        assert!(needs_you.changed);
        assert_eq!(drawn(needs_you), ["no key"]);
        assert_eq!(needs_you.default_line, "Default: ⌘J");
        assert_eq!(keys(&needs_you.cells[0])[0].slot, at(NextNeedsYouItem, 0));
        assert_eq!(needs_you.reset, Some(vec![at(NextNeedsYouItem, 0)]));
        let new = row(&p, "new_ticket");
        assert_eq!(drawn(new), ["⌘N", "n"]);
        assert_eq!(new.default_line, "Default: ⌘N or c");
        assert!(!keys(&new.cells[0])[0].changed);
        assert!(keys(&new.cells[1])[0].changed);
        assert!(p.reset_all.enabled);
        assert_eq!(
            p.reset_all.question,
            "Reset all 3 changed shortcuts to their defaults?"
        );
        assert_eq!(
            p.reset_all.body,
            "Activity, New ticket and Next item that needs you go back to their default keys."
        );
        assert!(!row(&p, "settings").changed);
    }

    /// `s15-48`: letters off dims every single letter and sequence in place (kept), not the modifier keys.
    #[test]
    fn letters_off() {
        let p = page(
            MAC,
            &KeyboardSettings {
                letters: false,
                ..KeyboardSettings::default()
            },
        );
        assert!(!p.letters.on);
        let board = row(&p, "show_board");
        assert_eq!(drawn(board), ["⇧⌘B", "g then b"]);
        assert!(!keys(&board.cells[0])[0].dimmed);
        assert!(keys(&board.cells[1])[0].dimmed);
        assert!(keys(&row(&p, "filter_board").cells[0])[0].dimmed);
        assert!(row(&p, "filter_board").dimmed);
        assert!(row(&p, "open_sheet").dimmed, "↵ / Esc are off too");
        assert!(!board.dimmed, "a row with a modifier key isn't");
        assert!(!row(&p, "park").dimmed, "no key isn't a letter");
        let tabs = &row(&p, "switch_tab").cells[0];
        assert_eq!(
            tabs,
            &Cell::Range {
                text: "1–4".into(),
                changed: false,
                dimmed: true
            }
        );
        let projects = &row(&p, "project").cells[0];
        assert!(matches!(projects, Cell::Range { dimmed: false, .. }));
        assert!(!p.reset_all.enabled, "the switch isn't a changed shortcut");
    }

    /// Recording turns the pressed key into the map's text: modifier keys in the platform's form, letters by the
    /// character, `g` waits for its letter, plain `g` (then no letter) is refused with the reason; ⌘Q and ⌘C are
    /// refused, an arrow is ignored; a taken key asks (with the key), and Use here moves it.
    #[test]
    fn recording_taken_refused() {
        let none = KeyboardSettings::default();
        let t = |p, pr: &Press, g| press_text(p, pr, g);
        assert_eq!(
            t(MAC, &press("j", "KeyJ", CMD), false),
            PressText::Text("⌘J".into())
        );
        assert_eq!(
            t(WIN, &press("r", "KeyR", CTRL), false),
            PressText::Text("Ctrl+R".into())
        );
        assert_eq!(
            t(
                MAC,
                &press("¡", "Digit1", Modifiers { alt: true, ..CMD }),
                false
            ),
            PressText::Text("⌥⌘1".into())
        );
        assert_eq!(
            t(MAC, &press("n", "KeyN", NONE), false),
            PressText::Text("n".into())
        );
        assert_eq!(
            t(MAC, &press("?", "Slash", SHIFT), false),
            PressText::Text("?".into())
        );
        assert_eq!(
            t(MAC, &press("т", "KeyN", NONE), false),
            PressText::Text("n".into()),
            "a Cyrillic layout: the physical key's letter"
        );
        assert_eq!(
            t(MAC, &press("g", "KeyG", NONE), false),
            PressText::ThenLetter
        );
        assert_eq!(
            t(MAC, &press("p", "KeyP", NONE), true),
            PressText::Text("g p".into())
        );
        assert_eq!(
            t(MAC, &press("Enter", "Enter", NONE), true),
            PressText::Text("g".into())
        );
        assert_eq!(
            t(MAC, &press(" ", "Space", CMD), false),
            PressText::Text("⌘Space".into())
        );
        assert_eq!(
            t(
                WIN,
                &press("F4", "F4", Modifiers { alt: true, ..NONE }),
                false
            ),
            PressText::Text("Alt+F4".into())
        );
        assert_eq!(
            t(MAC, &press("ArrowUp", "ArrowUp", NONE), false),
            PressText::Nothing
        );

        let slot = at(OpenActivity, 0);
        let (a, store) = record(MAC, &none, slot, &press("q", "KeyQ", CMD), false).unwrap();
        assert_eq!(
            a,
            RecordAnswer::Edited {
                key: "⌘Q".into(),
                answer: KeyEditAnswer::Refused {
                    reason: format!(
                        "⌘Q quits {}. Press another key.",
                        otaq_core::naming::PRODUCT_NAME
                    )
                }
            }
        );
        assert_eq!(store, None);
        let (a, _) = record(MAC, &none, slot, &press("c", "KeyC", CMD), false).unwrap();
        assert!(matches!(
            a,
            RecordAnswer::Edited { answer: KeyEditAnswer::Refused { reason }, .. }
                if reason == "⌘C copies text. Press another key."
        ));
        let (a, _) = record(MAC, &none, at(Park, 0), &press("x", "KeyX", CMD), true).unwrap();
        assert!(matches!(
            a,
            RecordAnswer::Edited { answer: KeyEditAnswer::Refused { reason }, .. }
                if reason == keymap_edit::G_REFUSED
        ));
        let (a, _) = record(MAC, &none, slot, &press("ArrowUp", "ArrowUp", NONE), false).unwrap();
        assert_eq!(a, RecordAnswer::Ignored);

        // Taken: the question, nothing stored; Use here (the edit again, confirmed) moves the key.
        let (a, store) = record(
            MAC,
            &none,
            at(OpenActivity, 0),
            &press("j", "KeyJ", CMD),
            false,
        )
        .unwrap();
        assert_eq!(store, None);
        let RecordAnswer::Edited {
            key,
            answer: KeyEditAnswer::Taken { question, by, .. },
        } = a
        else {
            panic!("{a:?}")
        };
        assert_eq!(
            question,
            "⌘J is used by Next item that needs you. Use it here instead?"
        );
        assert_eq!(by, NextNeedsYouItem);
        let s = ok(MAC, &none, rec(OpenActivity, 0, &key, true));
        let p = page(MAC, &s);
        assert_eq!(drawn(row(&p, "open_activity")), ["⌘J", "g then a"]);
        assert_eq!(drawn(row(&p, "next_needs_you_item")), ["no key"]);

        // A recorded sequence for an action with no key (Park → g p) and a letter in one slot (c → n).
        let (a, store) = record(MAC, &none, at(Park, 0), &press("p", "KeyP", NONE), true).unwrap();
        assert!(matches!(
            a,
            RecordAnswer::Edited {
                answer: KeyEditAnswer::Done { changed: 1 },
                ..
            }
        ));
        let p = page(MAC, &store.unwrap());
        let park = row(&p, "park");
        assert_eq!(drawn(park), ["g then p"]);
        assert_eq!(park.default_line, "Default: no key");
        assert_eq!(park.reset, Some(vec![at(Park, 0)]));
    }

    /// Remove key leaves "no key" with the reset, which gives the key back; removing one of two keys keeps the
    /// other; recording into "no key" goes to the removed slot.
    #[test]
    fn removed_and_reset() {
        let none = KeyboardSettings::default();
        let s = ok(
            MAC,
            &none,
            KeyEdit::Remove {
                action: NextNeedsYouItem,
                slot: 0,
            },
        );
        let p = page(MAC, &s);
        let needs_you = row(&p, "next_needs_you_item");
        assert_eq!(drawn(needs_you), ["no key"]);
        assert_eq!(keys(&needs_you.cells[0])[0].slot, at(NextNeedsYouItem, 0));
        let back = reset_row(MAC, &s, needs_you.reset.as_ref().unwrap(), false).unwrap();
        assert_eq!(back.answer, KeyEditAnswer::Done { changed: 0 });
        assert_eq!(back.store, Some(none.clone()));

        let s = ok(
            MAC,
            &none,
            KeyEdit::Remove {
                action: NewTicket,
                slot: 1,
            },
        );
        assert_eq!(drawn(row(&page(MAC, &s), "new_ticket")), ["⌘N"]);

        // A row's reset whose default another action took since asks first, then takes it back.
        let s = ok(MAC, &none, rec(OpenActivity, 0, "⌘J", true));
        let taken = reset_row(MAC, &s, &[at(NextNeedsYouItem, 0)], false).unwrap();
        assert_eq!(taken.store, None);
        assert!(matches!(
            taken.answer,
            KeyEditAnswer::Taken {
                by: OpenActivity,
                ..
            }
        ));
        let moved = reset_row(MAC, &s, &[at(NextNeedsYouItem, 0)], true).unwrap();
        let p = page(MAC, &moved.store.unwrap());
        assert_eq!(drawn(row(&p, "next_needs_you_item")), ["⌘J"]);
        assert_eq!(drawn(row(&p, "open_activity")), ["g then a"]);
        assert!(reset_row(MAC, &none, &[], false).is_err());
    }

    /// Pairs: each half records on its own; a changed half is tinted, the row's reset resets both.
    #[test]
    fn a_pair_changes_one_half() {
        let s = ok(
            MAC,
            &KeyboardSettings::default(),
            rec(PreviousTicket, 1, "h", false),
        );
        let p = page(MAC, &s);
        let r = row(&p, "next_ticket");
        assert_eq!(drawn(r), ["⌘] / ⌘[", "j / h"]);
        let halves = keys(&r.cells[1]);
        assert_eq!(halves[0].slot, at(NextTicket, 1));
        assert!(!halves[0].changed);
        assert_eq!(halves[1].slot, at(PreviousTicket, 1));
        assert!(halves[1].changed);
        assert_eq!(r.reset, Some(vec![at(PreviousTicket, 1)]));
        // A number taken from Project 1–9 shows each number's key.
        let s = ok(MAC, &KeyboardSettings::default(), rec(Park, 0, "⌘3", true));
        let p = page(MAC, &s);
        let projects = row(&p, "project");
        assert_eq!(drawn(projects), ["⌘1 ⌘2 ⌘4 ⌘5 ⌘6 ⌘7 ⌘8 ⌘9"]);
        assert!(projects.changed);
        assert_eq!(projects.reset, Some(vec![at(Project(3), 0)]));
    }

    /// `s15-54`: on Windows/Linux the note, the checkbox on rows with a modifier key, the in-terminal form while
    /// it's off, Ctrl+R's warning, Ctrl+D refused; Ctrl+Shift keys and terminal-only ones are fixed on.
    #[test]
    fn windows_linux_rows() {
        let none = KeyboardSettings::default();
        let p = page(WIN, &none);
        assert_eq!(p.terminals_note.as_deref(), Some(TERMINALS_NOTE));
        assert!(p.terminals_column);
        let check = |id: &str| row(&p, id).terminals.clone();
        let palette = check("command_palette").unwrap();
        assert!(palette.on && !palette.fixed && palette.in_terminals.is_none());
        let needs_you = check("next_needs_you_item").unwrap();
        assert!(!needs_you.on);
        assert_eq!(
            needs_you.in_terminals.as_deref(),
            Some("· in terminals Ctrl+Shift+J")
        );
        assert_eq!(
            check("new_ticket").unwrap().in_terminals.as_deref(),
            Some("· in terminals Ctrl+Shift+N")
        );
        let i = check("open_activity").unwrap();
        assert!(i.on && i.fixed);
        assert!(check("terminals").unwrap().on);
        let find = check("find_in_terminal").unwrap();
        assert!(find.on && find.fixed);
        assert_eq!(drawn(row(&p, "find_in_terminal")), ["Ctrl+Shift+F"]);
        assert_eq!(check("filter_board"), None, "letters have none");
        assert_eq!(check("park"), None);
        assert_eq!(check("project"), None);
        assert_eq!(drawn(row(&p, "project")), ["Ctrl+1–9"]);
        let board = check("show_board").unwrap();
        assert!(board.on || board.in_terminals.is_none());

        // Activity → Ctrl+R: its terminal form, then the warning on ticking; Let it through stores it.
        let s = ok(WIN, &none, rec(OpenActivity, 0, "Ctrl+R", false));
        let p = page(WIN, &s);
        let a = row(&p, "open_activity").terminals.clone().unwrap();
        assert_eq!(
            a.in_terminals.as_deref(),
            Some("· in terminals Ctrl+Shift+R")
        );
        let tick = |let_through| KeyEdit::WorksInTerminals {
            action: OpenActivity,
            slot: 0,
            on: true,
            let_through,
            confirmed: false,
        };
        let warned = keymap_edit::edit(WIN, &s, &tick(false)).unwrap();
        assert_eq!(
            warned.answer,
            KeyEditAnswer::Warn {
                warning: "Ctrl+R: the shell’s history search won’t reach the terminal.".into()
            }
        );
        let s = ok(WIN, &s, tick(true));
        let a = row(&page(WIN, &s), "open_activity")
            .terminals
            .clone()
            .unwrap();
        assert!(a.on && a.in_terminals.is_none());

        // Ctrl+D (Start / move to Dev) is refused with the reason and stays off.
        let refused = keymap_edit::edit(
            WIN,
            &none,
            &KeyEdit::WorksInTerminals {
                action: StartDev,
                slot: 0,
                on: true,
                let_through: false,
                confirmed: false,
            },
        )
        .unwrap();
        assert_eq!(
            refused.answer,
            KeyEditAnswer::Refused {
                reason: "Ctrl+D ends the shell or agent in a terminal, so it can’t work inside terminals."
                    .into()
            }
        );
        assert!(!check_of(&page(WIN, &none), "start_dev").on);
    }

    fn check_of(p: &Page, id: &str) -> TerminalsCheck {
        row(p, id).terminals.clone().unwrap()
    }

    /// macOS: the checkbox shows only on a row whose recorded key uses ⌃, never for ⌘ keys or the base map's ⌃`.
    #[test]
    fn mac_checkbox_only_on_a_control_row() {
        let none = KeyboardSettings::default();
        let s = ok(MAC, &none, rec(OpenActivity, 0, "⌃R", false));
        let p = page(MAC, &s);
        let a = check_of(&p, "open_activity");
        assert!(!a.on && !a.fixed);
        assert_eq!(a.in_terminals, None, "macOS has no in-terminal form");
        assert_eq!(row(&p, "terminals").terminals, None);
        let s = ok(MAC, &none, rec(OpenActivity, 0, "⌘J", true));
        assert_eq!(row(&page(MAC, &s), "open_activity").terminals, None);
        assert_eq!(p.terminals_note, None);
    }

    /// The gallery has every frame of `s15-46`…`s15-55`, light and dark drawn by the page.
    #[test]
    fn the_gallery_has_every_frame() {
        let g = gallery();
        assert_eq!(g.cells.len(), 19);
        let taken = g
            .cells
            .iter()
            .find(|c| c.name == "A key that's already taken")
            .unwrap();
        assert!(matches!(
            taken.moment.answer.as_ref().unwrap().answer,
            KeyEditAnswer::Taken { .. }
        ));
        let warn = g
            .cells
            .iter()
            .find(|c| c.name == "Letting a Ctrl+letter key through")
            .unwrap();
        assert!(matches!(
            warn.moment.answer.as_ref().unwrap().answer,
            KeyEditAnswer::Warn { .. }
        ));
        for c in &g.cells {
            for id in &c.rows {
                let _ = row(&c.page, id);
            }
        }
    }
}
