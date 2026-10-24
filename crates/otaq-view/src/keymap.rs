//! The keyboard map of decision 0097 (SDD §21.3, KEY-01…KEY-03): every app action with its title, its group and its
//! keys. A key is a modifier key (⌘K, Ctrl+Shift+P, ⌃`), a single letter (`j`, `/`, `?`, `1`) or `g` then a letter
//! (`g` `b`). The **base map** ([`KeyAction::base_keys`]) holds each action's default keys in three columns: macOS,
//! Windows/Linux, and Windows/Linux while a terminal has focus. The **user's keys** ([`user_keys`]) are the base map
//! with the user's changes applied ([`CustomKeys`], stored by T-0390); everything the app binds comes from them.
//!
//! Where a key works ([`KeyPlace`]): on macOS ⌘ keys and the base map's ⌃` / ⌃Tab / ⌃⇧Tab work in terminals too, every
//! other key stays the terminal's there. On Windows/Linux only the terminal column applies inside a terminal (most keys
//! get Shift there; Ctrl+1–9, Ctrl+`, Ctrl+, , Ctrl+Tab / Ctrl+Shift+Tab, Ctrl+K and Ctrl+P are let through). Single
//! letters and sequences never work in a terminal. A custom key marked "Works inside terminals" is let through as it is.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// The ticket sheet's tabs (SHT-02): `SwitchTab(1..=4)`.
pub const SHEET_TABS: u8 = 4;
/// The second keys of their `g` sequences: Overview, Session, Files, Activity.
const SHEET_TAB_LETTERS: [&str; 4] = ["o", "s", "f", "a"];

/// Which column of the keyboard map applies. `KeyPlatform` on the page.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename = "KeyPlatform")]
pub enum Platform {
    Mac,
    WinLinux,
    /// Windows/Linux with the terminal focused.
    WinLinuxTerminal,
}

impl Platform {
    /// The platform this build runs on (outside the terminal).
    pub fn current() -> Self {
        if cfg!(target_os = "macos") {
            Self::Mac
        } else {
            Self::WinLinux
        }
    }

    fn is_mac(self) -> bool {
        self == Self::Mac
    }
}

/// The groups the Settings page and the `?` list show the actions in (design `s15-46`). `KeyGroup` on the page.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum KeyGroup {
    MovingAround,
    Views,
    TheTicket,
    TheTerminal,
}

impl KeyGroup {
    pub const ALL: [Self; 4] = [
        Self::MovingAround,
        Self::Views,
        Self::TheTicket,
        Self::TheTerminal,
    ];

    pub fn title(self) -> &'static str {
        match self {
            Self::MovingAround => "Moving around",
            Self::Views => "Views",
            Self::TheTicket => "The ticket",
            Self::TheTerminal => "The terminal",
        }
    }
}

/// An app action of the map. On the page it is `{ action: "open_activity" }`, or with its number
/// `{ action: "project", number: 3 }`; screens register their handlers by `action` (T-0212).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(tag = "action", content = "number", rename_all = "snake_case")]
pub enum KeyAction {
    /// The command palette: actions, tickets and projects (KEY-06).
    CommandPalette,
    SwitchProject,
    /// Project 1–9.
    Project(u8),
    /// The next item that needs you (d0114): the strip's Next, on to other projects after the open one's.
    NextInboxItem,
    NextTicket,
    PreviousTicket,
    /// Focus the header's board filter (KEY-09).
    FilterBoard,
    /// Focus the epic of the selected card (EPC-18, d0080): the same as its `epic:` filter.
    FocusEpic,
    /// The `?` shortcut list (KEY-07).
    ShowShortcuts,
    /// The Board view of the main area (d0087), its Board view (BRD-01).
    ShowBoard,
    /// The board's List view (BRD-01, SHB-04).
    ShowList,
    /// Pause / Resume agents (PAU-08, d0112): opens the title bar's scope menu (This project · All projects). 0097's
    /// Stop all row, reused.
    Pause,
    /// The Activity popover (d0087).
    OpenActivity,
    Settings,
    /// Open the Project agent panel and focus it, or close it when it has the keyboard (d0113).
    FocusPane,
    /// A new Project agent conversation: the panel's empty state; nothing starts until the first message is sent
    /// (d0068, d0087, d0113, PAG-01).
    NewProjectAgent,
    NewTicket,
    OpenPanel,
    ClosePanel,
    ExpandPanel,
    /// The ticket sheet's tabs 1–4: Overview, Session (an epic's Lead), Files, Activity (SHT-02); `1`–`4` and `g o` /
    /// `g s` / `g f` / `g a`.
    SwitchTab(u8),
    NextTab,
    PreviousTab,
    Approve,
    SendBack,
    /// Start / move to Dev (also queues an epic, d0080).
    StartDev,
    /// No key in the base map (the palette runs it), as the four after it.
    Park,
    CloseTicket,
    OpenPr,
    RerunValidation,
    OpenDiff,
    /// The Terminals drawer: open it, close it, and leave a focused terminal (d0068, d0087).
    Terminals,
    FindInTerminal,
    /// The project Files view, the toolbar's third view (FIL-01, FIL-11).
    ShowFiles,
    /// Go to file (FIL-11): the search box of the open ticket's Files tab, else of the project Files view.
    GoToFile,
}

impl KeyAction {
    /// Every action in the order Settings and the `?` list show them, numbered ones expanded (Project 1–9, tabs 1–4).
    pub fn all() -> Vec<KeyAction> {
        let mut all = vec![Self::CommandPalette, Self::SwitchProject];
        all.extend((1..=9).map(Self::Project));
        all.extend([
            Self::NextInboxItem,
            Self::NextTicket,
            Self::PreviousTicket,
            Self::FilterBoard,
            Self::FocusEpic,
            Self::ShowShortcuts,
            Self::ShowBoard,
            Self::ShowList,
            Self::ShowFiles,
            Self::Pause,
            Self::OpenActivity,
            Self::Settings,
            Self::FocusPane,
            Self::NewProjectAgent,
            Self::NewTicket,
            Self::OpenPanel,
            Self::ClosePanel,
            Self::ExpandPanel,
        ]);
        all.extend((1..=SHEET_TABS).map(Self::SwitchTab));
        all.extend([
            Self::GoToFile,
            Self::NextTab,
            Self::PreviousTab,
            Self::Approve,
            Self::SendBack,
            Self::StartDev,
            Self::Park,
            Self::CloseTicket,
            Self::OpenPr,
            Self::RerunValidation,
            Self::OpenDiff,
            Self::Terminals,
            Self::FindInTerminal,
        ]);
        all
    }

    /// A key of the open ticket sheet (its tabs): it comes before another action's same key while a sheet is open,
    /// and the other one's runs when none is (`g a`: the Activity tab, else Activity, SHB-04; `g f`: the Files tab,
    /// else the Files view, FIL-11).
    pub fn in_sheet(self) -> bool {
        matches!(self, Self::SwitchTab(_))
    }

    /// The action's name in Settings, the `?` list and the palette (design `s15-46`). The two halves of a pair drawn
    /// as one row (Next / previous ticket, Open / close the ticket, Next / previous ticket tab) and the numbered
    /// actions share their row's title.
    pub fn title(self) -> &'static str {
        match self {
            Self::CommandPalette => "Command palette",
            Self::SwitchProject => "Switch project",
            Self::Project(_) => "Project 1–9",
            Self::NextInboxItem => "Next item that needs you",
            Self::NextTicket | Self::PreviousTicket => "Next / previous ticket",
            Self::FilterBoard => "Filter the board",
            Self::FocusEpic => "Focus the epic of the selected card",
            Self::ShowShortcuts => "Show all shortcuts",
            Self::ShowBoard => "Board",
            Self::ShowList => "List",
            Self::Pause => "Pause / Resume agents",
            Self::OpenActivity => "Activity",
            Self::Settings => "Settings",
            Self::FocusPane => "Open / close the Project agent",
            Self::NewProjectAgent => "New Project agent conversation",
            Self::NewTicket => "New ticket",
            Self::OpenPanel | Self::ClosePanel => "Open / close the ticket",
            Self::ExpandPanel => "Full page / back to the sheet",
            Self::SwitchTab(_) => "Ticket tabs",
            Self::NextTab | Self::PreviousTab => "Next / previous ticket tab",
            Self::Approve => "Approve ▾",
            Self::SendBack => "Send back",
            Self::StartDev => "Start / move to Dev",
            Self::Park => "Park",
            Self::CloseTicket => "Close ticket",
            Self::OpenPr => "Open PR",
            Self::RerunValidation => "Re-run validation",
            Self::OpenDiff => "Open diff",
            Self::Terminals => "Terminals: open, close, leave the terminal",
            Self::FindInTerminal => "Find in terminal scrollback",
            Self::ShowFiles => "Files",
            Self::GoToFile => "Go to file",
        }
    }

    /// The action's name in the `g` hint (design `s02-30`): the view in a word ("Board", "Epic"), else its title (a
    /// custom `g` `p` shows "Park").
    pub fn hint_name(self) -> &'static str {
        match self {
            Self::Terminals => "Terminals",
            Self::FocusEpic => "Epic",
            _ => self.title(),
        }
    }

    /// The group it is listed in (design `s15-46`).
    pub fn group(self) -> KeyGroup {
        match self {
            Self::CommandPalette
            | Self::SwitchProject
            | Self::Project(_)
            | Self::NextInboxItem
            | Self::NextTicket
            | Self::PreviousTicket
            | Self::FilterBoard
            | Self::FocusEpic
            | Self::ShowShortcuts => KeyGroup::MovingAround,
            Self::ShowBoard
            | Self::ShowList
            | Self::Pause
            | Self::OpenActivity
            | Self::Settings
            | Self::FocusPane
            | Self::NewProjectAgent
            | Self::ShowFiles => KeyGroup::Views,
            Self::NewTicket
            | Self::OpenPanel
            | Self::ClosePanel
            | Self::ExpandPanel
            | Self::SwitchTab(_)
            | Self::GoToFile
            | Self::NextTab
            | Self::PreviousTab
            | Self::Approve
            | Self::SendBack
            | Self::StartDev
            | Self::Park
            | Self::CloseTicket
            | Self::OpenPr
            | Self::RerunValidation
            | Self::OpenDiff => KeyGroup::TheTicket,
            Self::Terminals | Self::FindInTerminal => KeyGroup::TheTerminal,
        }
    }

    /// **The base map** (KEY-01, KEY-02): the action's default keys in order, each as its three texts (macOS,
    /// Windows/Linux, Windows/Linux in a terminal), "" where that column has none. The index is the key's **slot**,
    /// which a custom change names ([`CustomKey::slot`]).
    pub fn base_keys(self) -> Vec<[String; 3]> {
        let k =
            |mac: &str, win: &str, term: &str| [mac.to_owned(), win.to_owned(), term.to_owned()];
        // A single letter or a `g` sequence: the same on every platform, never in a terminal.
        let letter = |l: &str| k(l, l, "");
        match self {
            Self::CommandPalette => vec![
                k("⌘K", "Ctrl+K", "Ctrl+K"),
                k("⇧⌘P", "Ctrl+Shift+P", "Ctrl+Shift+P"),
            ],
            Self::SwitchProject => vec![k("⌘P", "Ctrl+P", "Ctrl+P")],
            Self::Project(n) => {
                let n = n.clamp(1, 9);
                vec![k(
                    &format!("⌘{n}"),
                    &format!("Ctrl+{n}"),
                    &format!("Ctrl+{n}"),
                )]
            }
            Self::NextInboxItem => vec![k("⌘J", "Ctrl+J", "Ctrl+Shift+J")],
            Self::NextTicket => vec![k("⌘]", "Ctrl+]", "Ctrl+Shift+]"), letter("j")],
            Self::PreviousTicket => vec![k("⌘[", "Ctrl+[", "Ctrl+Shift+["), letter("k")],
            Self::FilterBoard => vec![letter("/")],
            Self::FocusEpic => vec![letter("g e")],
            Self::ShowShortcuts => vec![letter("?")],
            Self::ShowBoard => vec![k("⇧⌘B", "Ctrl+Shift+B", ""), letter("g b")],
            Self::ShowList => vec![letter("g l")],
            // 0097's Stop all row (d0112): in terminals too.
            Self::Pause => vec![k("⇧⌘.", "Ctrl+Shift+.", "Ctrl+Shift+.")],
            Self::OpenActivity => vec![k("⇧⌘U", "Ctrl+Shift+U", "Ctrl+Shift+U"), letter("g a")],
            Self::Settings => vec![k("⌘,", "Ctrl+,", "Ctrl+,")],
            Self::FocusPane => vec![k("⌘L", "Ctrl+L", "Ctrl+Shift+L")],
            Self::NewProjectAgent => vec![k("⇧⌘L", "Ctrl+Shift+L", "")],
            Self::NewTicket => vec![k("⌘N", "Ctrl+N", "Ctrl+Shift+N"), letter("c")],
            Self::OpenPanel => vec![k("↵", "↵", "")],
            Self::ClosePanel => vec![k("Esc", "Esc", "")],
            Self::ExpandPanel => vec![letter("e")],
            Self::SwitchTab(n) => {
                let n = n.clamp(1, SHEET_TABS);
                vec![
                    letter(&n.to_string()),
                    letter(&format!("g {}", SHEET_TAB_LETTERS[usize::from(n) - 1])),
                ]
            }
            Self::NextTab => vec![k("⌃Tab", "Ctrl+Tab", "Ctrl+Tab")],
            Self::PreviousTab => vec![k("⌃⇧Tab", "Ctrl+Shift+Tab", "Ctrl+Shift+Tab")],
            Self::Approve => vec![k("⌘↵", "Ctrl+↵", "Ctrl+Shift+↵")],
            Self::SendBack => vec![k("⌘⌫", "Ctrl+⌫", "Ctrl+Shift+⌫")],
            Self::StartDev => vec![k("⌘D", "Ctrl+D", "Ctrl+Shift+D")],
            Self::Park
            | Self::CloseTicket
            | Self::OpenPr
            | Self::RerunValidation
            | Self::OpenDiff => {
                vec![]
            }
            Self::Terminals => vec![k("⌃`", "Ctrl+`", "Ctrl+`"), letter("g t")],
            Self::FindInTerminal => vec![k("⌘F", "", "Ctrl+Shift+F")],
            Self::ShowFiles => vec![k("⇧⌘O", "Ctrl+Shift+O", "Ctrl+Shift+O"), letter("g f")],
            Self::GoToFile => vec![k("⌥⌘P", "Ctrl+Alt+P", "Ctrl+Shift+Alt+P")],
        }
    }

    /// The base map's keys in one column, in slot order, leaving out the slots that column has none for.
    pub fn default_keys(self, platform: Platform) -> Vec<String> {
        let column = column_index(platform);
        self.base_keys()
            .into_iter()
            .map(|texts| texts[column].clone())
            .filter(|t| !t.is_empty())
            .collect()
    }

    /// The base map's first key as shown in a Kbd, or `None` where the action has none. Only the gallery's keyboard
    /// map reads it; every label shows the user's keys ([`crate::key_labels::KeyLabels`], KEY-08).
    pub fn keys(self, platform: Platform) -> Option<String> {
        self.default_keys(platform).into_iter().next()
    }
}

fn column_index(platform: Platform) -> usize {
    match platform {
        Platform::Mac => 0,
        Platform::WinLinux => 1,
        Platform::WinLinuxTerminal => 2,
    }
}

/// An action's title and group as the page lists them. `KeyActionInfo` on the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "KeyActionInfo")]
pub struct ActionInfo {
    pub action: KeyAction,
    pub title: &'static str,
    pub group: KeyGroup,
}

/// Every listed action ([`KeyAction::all`]).
pub fn action_infos() -> Vec<ActionInfo> {
    KeyAction::all()
        .into_iter()
        .map(|action| ActionInfo {
            action,
            title: action.title(),
            group: action.group(),
        })
        .collect()
}

/// One second key of the `g` hint (KEY-02, design `s02-30`): the letter after `g` and what it goes to.
/// `SequenceHint` on the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
pub struct SequenceHint {
    pub then: String,
    pub name: &'static str,
}

/// The base map's sequences in the order the hint shows them ("g → b Board · l List · a Activity · t Terminals · e
/// Epic", 0097; Files after List, FIL-11); the user's own sequences for other actions follow, in the map's order.
const HINT_ORDER: [KeyAction; 6] = [
    KeyAction::ShowBoard,
    KeyAction::ShowList,
    KeyAction::ShowFiles,
    KeyAction::OpenActivity,
    KeyAction::Terminals,
    KeyAction::FocusEpic,
];

/// **The typing hint** (d0126): shown when letters came too fast with nothing focused, so they ran nothing.
pub const TYPING_HINT: &str = "Typing? Click where you want to type";

/// **The `g` hint** for the user's keys (`bindings`, [`user_keys`]): every `g` sequence with its action's hint name,
/// custom ones included, removed ones left out. The sheet's tab keys aren't in it (`s02-30`): they work in an open
/// sheet only.
pub fn sequence_hint(bindings: &[Binding]) -> Vec<SequenceHint> {
    let mut hint: Vec<(usize, SequenceHint)> = bindings
        .iter()
        .filter(|b| !b.action.in_sheet())
        .filter_map(|b| match &b.key {
            Key::Sequence { then } => Some((
                HINT_ORDER
                    .iter()
                    .position(|a| *a == b.action)
                    .unwrap_or(HINT_ORDER.len()),
                SequenceHint {
                    then: then.clone(),
                    name: b.action.hint_name(),
                },
            )),
            _ => None,
        })
        .collect();
    // Stable: the user's own sequences keep the map's order after the base ones.
    hint.sort_by_key(|(order, _)| *order);
    hint.into_iter().map(|(_, h)| h).collect()
}

/// The modifier keys held with a keystroke, as the UI toolkit reports them. `KeyModifiers` on the page.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[ts(rename = "KeyModifiers")]
pub struct Modifiers {
    pub control: bool,
    pub alt: bool,
    pub shift: bool,
    /// ⌘ on macOS, the Windows key on Windows, Super on Linux.
    pub platform: bool,
    pub function: bool,
}

impl Modifiers {
    const NONE: Self = Self {
        control: false,
        alt: false,
        shift: false,
        platform: false,
        function: false,
    };

    /// Whether every modifier of `other` is held here (others may be held too).
    pub fn holds(&self, other: &Modifiers) -> bool {
        (!other.control || self.control)
            && (!other.alt || self.alt)
            && (!other.shift || self.shift)
            && (!other.platform || self.platform)
            && (!other.function || self.function)
    }

    /// The same four modifiers held (Fn isn't compared: the toolkit doesn't report it on every platform).
    fn same(&self, other: &Modifiers) -> bool {
        self.control == other.control
            && self.alt == other.alt
            && self.shift == other.shift
            && self.platform == other.platform
    }
}

/// Where a keystroke goes (SES-10).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyRoute {
    /// To the program in the focused terminal (Claude Code): Esc, Ctrl+C, Ctrl+J, arrows, Tab, letters…
    Terminal,
    /// To the app's shortcuts and views.
    App,
}

/// The modifier sets that take every key holding them to the app while a terminal has focus, bound or not: on macOS
/// ⌘ (the terminal doesn't use ⌘); on Windows and Linux Ctrl+Shift (the terminal column's form, and the terminal's own
/// copy and paste, which it handles itself), or the Windows / Super key. Other keys reach the app only when a binding
/// that works in terminals has them ([`route`]).
pub fn terminal_app_modifiers(platform: Platform) -> &'static [Modifiers] {
    const PLATFORM: Modifiers = Modifiers {
        platform: true,
        ..Modifiers::NONE
    };
    const CONTROL_SHIFT: Modifiers = Modifiers {
        control: true,
        shift: true,
        ..Modifiers::NONE
    };
    match platform {
        Platform::Mac => &[PLATFORM],
        Platform::WinLinux | Platform::WinLinuxTerminal => &[CONTROL_SHIFT, PLATFORM],
    }
}

/// Routes one key press. While a terminal has focus **every key goes to the program in it, Esc included**, except
/// keys holding [`terminal_app_modifiers`] and the keys of `bindings` that work in terminals ([`KeyPlace`]): on macOS
/// the base map's ⌃` and ⌃Tab / ⌃⇧Tab, and a custom key marked "Works inside terminals". Without terminal focus
/// everything is the app's.
pub fn route(
    platform: Platform,
    terminal_focused: bool,
    press: &KeyPress,
    bindings: &[Binding],
) -> KeyRoute {
    if !terminal_focused {
        return KeyRoute::App;
    }
    let held = terminal_app_modifiers(platform)
        .iter()
        .any(|m| press.modifiers.holds(m));
    let bound = bindings
        .iter()
        .any(|b| b.place.inside_terminals() && b.key.matches(press, false));
    if held || bound {
        KeyRoute::App
    } else {
        KeyRoute::Terminal
    }
}

/// One key event as the page gets it (`KeyboardEvent.key`, `.code` and the modifiers held): what [`Key::matches`]
/// and [`route`] read. The page's dispatcher applies the same rules (`ui/src/app/keys.ts`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyPress {
    pub key: String,
    pub code: String,
    pub modifiers: Modifiers,
}

impl KeyPress {
    /// The letter the key types, or else its name: letters follow the keyboard layout (⌘K is K on a Dvorak keyboard
    /// too); where the layout types no Latin letter (Cyrillic), the physical key's.
    fn letter(&self) -> String {
        let mut chars = self.key.chars();
        if let (Some(c), None) = (chars.next(), chars.next())
            && c.is_ascii_alphabetic()
        {
            return c.to_ascii_lowercase().to_string();
        }
        match self.code.strip_prefix("Key") {
            Some(l) if l.len() == 1 => l.to_ascii_lowercase(),
            _ => self.key.to_lowercase(),
        }
    }
}

/// One modifier key as the page matches it against a key event (T-0215). `Keystroke` on the page.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
pub struct Keystroke {
    /// The key as the web's `KeyboardEvent.key` names it: a lowercase letter, a digit or a sign (`k`, `1`, `[`), or a
    /// named key (`Enter`, `Backspace`, `Escape`, `Tab`).
    pub key: String,
    /// The physical key as `KeyboardEvent.code` names it (`KeyK`, `Digit1`, `BracketLeft`, `Enter`), for keys whose
    /// character depends on the keyboard layout or on the modifiers held (⌥1 types `¡` on a Mac).
    pub code: String,
    pub modifiers: Modifiers,
}

impl Keystroke {
    /// A shortcut text as a keystroke: "⇧⌘I", "⌃Tab", "⌃`", "Ctrl+Shift+↵", "Esc". `None` for text that isn't one
    /// shortcut.
    pub fn parse(text: &str) -> Option<Self> {
        let mut modifiers = Modifiers::default();
        let key = if text.contains('+') && text.len() > 1 {
            let mut pieces: Vec<&str> = text.split('+').collect();
            let key = pieces.pop()?;
            for modifier in pieces {
                match modifier {
                    "Ctrl" => modifiers.control = true,
                    "Shift" => modifiers.shift = true,
                    "Alt" => modifiers.alt = true,
                    "Super" => modifiers.platform = true,
                    _ => return None,
                }
            }
            key
        } else {
            let mut rest = text;
            loop {
                let mut chars = rest.chars();
                match chars.next() {
                    Some('⌘') => modifiers.platform = true,
                    Some('⇧') => modifiers.shift = true,
                    Some('⌥') => modifiers.alt = true,
                    Some('⌃') => modifiers.control = true,
                    _ => break,
                }
                rest = chars.as_str();
            }
            rest
        };
        let (key, code) = match key {
            "↵" => ("Enter".to_owned(), "Enter".to_owned()),
            "⌫" => ("Backspace".to_owned(), "Backspace".to_owned()),
            "Esc" => ("Escape".to_owned(), "Escape".to_owned()),
            "Tab" => ("Tab".to_owned(), "Tab".to_owned()),
            k => {
                let mut chars = k.chars();
                let (Some(c), None) = (chars.next(), chars.next()) else {
                    return None;
                };
                (c.to_ascii_lowercase().to_string(), code_of(c)?)
            }
        };
        Some(Self {
            key,
            code,
            modifiers,
        })
    }

    /// Whether `press` is this keystroke: the same modifiers, and the same letter (by the layout), named key or (digits
    /// and signs) physical key.
    pub fn matches(&self, press: &KeyPress) -> bool {
        if !press.modifiers.same(&self.modifiers) {
            return false;
        }
        if self.key.chars().count() > 1 {
            return press.key == self.key;
        }
        if self.key.chars().all(|c| c.is_ascii_lowercase()) {
            return press.letter() == self.key;
        }
        press.code == self.code
    }

    /// As shown in a Kbd: "⇧⌘B" (⌃⌥⇧⌘ order) on macOS, "Ctrl+Shift+Alt+P" on Windows/Linux, as the base map writes them.
    pub fn text(&self, platform: Platform) -> String {
        let m = &self.modifiers;
        let key = match self.key.as_str() {
            "Enter" => "↵".to_owned(),
            "Backspace" => "⌫".to_owned(),
            "Escape" => "Esc".to_owned(),
            k if k.chars().count() == 1 => k.to_uppercase(),
            k => k.to_owned(),
        };
        if platform.is_mac() {
            let mut s = String::new();
            for (held, sign) in [
                (m.control, '⌃'),
                (m.alt, '⌥'),
                (m.shift, '⇧'),
                (m.platform, '⌘'),
            ] {
                if held {
                    s.push(sign);
                }
            }
            s + &key
        } else {
            let mut parts = Vec::new();
            for (held, name) in [
                (m.control, "Ctrl"),
                (m.shift, "Shift"),
                (m.alt, "Alt"),
                (m.platform, "Super"),
            ] {
                if held {
                    parts.push(name);
                }
            }
            parts.push(&key);
            parts.join("+")
        }
    }
}

/// The physical key of a character on a US keyboard, as `KeyboardEvent.code` names it.
fn code_of(c: char) -> Option<String> {
    Some(match c {
        'a'..='z' | 'A'..='Z' => format!("Key{}", c.to_ascii_uppercase()),
        '0'..='9' => format!("Digit{c}"),
        '[' => "BracketLeft".to_owned(),
        ']' => "BracketRight".to_owned(),
        '`' => "Backquote".to_owned(),
        '\\' => "Backslash".to_owned(),
        ',' => "Comma".to_owned(),
        '.' => "Period".to_owned(),
        '/' => "Slash".to_owned(),
        ';' => "Semicolon".to_owned(),
        '\'' => "Quote".to_owned(),
        '=' => "Equal".to_owned(),
        '-' => "Minus".to_owned(),
        _ => return None,
    })
}

/// One key of an action (0097): a modifier key, a single letter, or `g` then a letter. Letters and sequences work
/// only while nobody is typing and while the single-letter switch is on (KEY-02; the page's dispatcher checks both),
/// never in a terminal. `Key` on the page.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Key {
    /// A key with modifiers (⌘K, Ctrl+Shift+P, ⌃`), or ↵ / Esc alone (Open / close the ticket, which stay keys of
    /// the focused control as before, not single letters).
    Stroke { stroke: Keystroke },
    /// A single character with no modifier: `j`, `/`, `?`, `1`. Matched by the character it types, on any layout.
    Letter { letter: String },
    /// `g`, let go, then `then` (a single character, matched as a [`Key::Letter`]).
    Sequence { then: String },
}

impl Key {
    /// A key text of the map: "j", "/", "?", "1", "g b" (a sequence), or a modifier key ([`Keystroke::parse`]).
    /// `None` for text that isn't one key.
    pub fn parse(text: &str) -> Option<Self> {
        if let Some(then) = text.strip_prefix("g ") {
            return single_letter(then).map(|then| Self::Sequence { then });
        }
        if let Some(letter) = single_letter(text) {
            return Some(Self::Letter { letter });
        }
        // A key alone is a letter (above) or a named key (↵, Esc); "J" is neither.
        Keystroke::parse(text)
            .filter(|s| s.modifiers != Modifiers::NONE || s.key.chars().count() > 1)
            .map(|stroke| Self::Stroke { stroke })
    }

    /// As shown in a Kbd: "j", "g b", "⇧⌘L", "Ctrl+Shift+L".
    pub fn text(&self, platform: Platform) -> String {
        match self {
            Self::Stroke { stroke } => stroke.text(platform),
            Self::Letter { letter } => letter.clone(),
            Self::Sequence { then } => format!("g {then}"),
        }
    }

    /// A single letter or a `g` sequence (the kinds the letters switch and the "nobody is typing" rule apply to).
    pub fn is_letter(&self) -> bool {
        !matches!(self, Self::Stroke { .. })
    }

    /// The keystroke, for a modifier key.
    pub fn stroke(&self) -> Option<&Keystroke> {
        match self {
            Self::Stroke { stroke } => Some(stroke),
            _ => None,
        }
    }

    /// Whether `press` presses this key. `after_g`: the press follows a lone `g` (only a sequence's second key matches
    /// then; the dispatcher keeps that state, T-0391).
    pub fn matches(&self, press: &KeyPress, after_g: bool) -> bool {
        match self {
            Self::Stroke { stroke } => !after_g && stroke.matches(press),
            Self::Letter { letter } => !after_g && letter_matches(letter, press),
            Self::Sequence { then } => after_g && letter_matches(then, press),
        }
    }
}

/// A single character a letter key may be: a lowercase Latin letter, a digit or a sign.
fn single_letter(text: &str) -> Option<String> {
    let mut chars = text.chars();
    let (Some(c), None) = (chars.next(), chars.next()) else {
        return None;
    };
    (c.is_ascii_lowercase() || c.is_ascii_digit() || c.is_ascii_punctuation())
        .then(|| c.to_string())
}

/// A single letter by the character typed: a Latin letter by the layout's letter without Shift (`J` isn't `j`), any
/// other character as typed, Shift or not (`?` is ⇧/ on a US keyboard, `/` is ⇧7 on a German one).
fn letter_matches(letter: &str, press: &KeyPress) -> bool {
    let m = &press.modifiers;
    if m.control || m.alt || m.platform {
        return false;
    }
    if letter.chars().all(|c| c.is_ascii_lowercase()) {
        !m.shift && press.letter() == letter
    } else {
        press.key == letter
    }
}

/// Where a binding works. `KeyPlace` on the page.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum KeyPlace {
    /// Outside terminals and inside them.
    Everywhere,
    /// Not while a terminal has focus (there the key is the terminal's).
    OutsideTerminals,
    /// Only while a terminal has focus (the Windows/Linux terminal column).
    InsideTerminals,
}

impl KeyPlace {
    pub fn inside_terminals(self) -> bool {
        self != Self::OutsideTerminals
    }

    pub fn outside_terminals(self) -> bool {
        self != Self::InsideTerminals
    }
}

/// One key of an action as the page binds it. `KeyBinding` on the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "KeyBinding")]
pub struct Binding {
    pub action: KeyAction,
    /// Which of the action's keys this is ([`KeyAction::base_keys`]' index, or past them for a key the user added).
    /// On Windows/Linux one slot may give two bindings: its key outside terminals and its terminal form.
    pub slot: u8,
    /// The key as shown in a Kbd (`⌘K`, `Ctrl+Shift+K`, `j`, `g b`).
    pub text: String,
    pub key: Key,
    pub place: KeyPlace,
    /// A key the user recorded ([`CustomKeys`]), not the base map's.
    pub custom: bool,
}

/// The base map's macOS keys without ⌘ that work in terminals too (0097): Terminals (also how you leave one) and
/// Next / previous ticket tab. Every other macOS key without ⌘ stays the terminal's there.
const MAC_TERMINAL_KEYS: [&str; 3] = ["⌃`", "⌃Tab", "⌃⇧Tab"];

/// Keys an app shortcut never takes from a terminal, on Windows/Linux and on macOS (0097): stop, exit, suspend,
/// Escape, force quit, and on Windows/Linux the terminal's copy and paste. T-0390 refuses "Works inside terminals"
/// for them; [`user_keys`] never binds them there.
pub fn never_in_terminals(platform: Platform) -> &'static [&'static str] {
    if platform.is_mac() {
        &["⌃C", "⌃D", "⌃Z", "⌃[", "⌃\\"]
    } else {
        &[
            "Ctrl+C",
            "Ctrl+D",
            "Ctrl+Z",
            "Ctrl+[",
            "Ctrl+\\",
            "Ctrl+Shift+C",
            "Ctrl+Shift+V",
        ]
    }
}

fn is_never_in_terminals(platform: Platform, key: &Key) -> bool {
    key.stroke().is_some_and(|stroke| {
        never_in_terminals(platform)
            .iter()
            .any(|t| Keystroke::parse(t).as_ref() == Some(stroke))
    })
}

/// The user's changes to the base map (KEY-04), one per changed key; empty means the base map. T-0390 stores them in
/// the settings and converts them to this.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct CustomKeys(pub Vec<CustomKey>);

/// One changed key: the action, which of its keys, and what became of it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct CustomKey {
    pub action: KeyAction,
    /// The key's slot: an index into the action's base keys ([`KeyAction::base_keys`]; New ticket's ⌘N is 0, its `c`
    /// is 1). A slot past them is a key the user added (Park has none, so its first recorded key, `g` `p`, is slot 0).
    pub slot: u8,
    pub change: KeyChange,
    /// "Works inside terminals" (Windows/Linux; on macOS for ⌃ keys): the key is let through to the app unchanged
    /// while a terminal has focus, instead of getting Shift (Windows/Linux) or staying the terminal's (macOS). Ignored
    /// for single letters, sequences, ⌘ keys and the keys of [`never_in_terminals`].
    pub works_in_terminals: bool,
}

/// What became of a key. A removed key leaves its action with no key there (it stays in the palette).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "change", rename_all = "snake_case")]
pub enum KeyChange {
    /// Recorded anew, in this platform's form.
    Replaced {
        key: Key,
    },
    Removed,
}

impl CustomKeys {
    fn get(&self, action: KeyAction, slot: u8) -> Option<&CustomKey> {
        self.0
            .iter()
            .rev()
            .find(|c| c.action == action && c.slot == slot)
    }

    /// One past the highest slot changed for `action`, so added keys are reached.
    fn slots(&self, action: KeyAction) -> usize {
        self.0
            .iter()
            .filter(|c| c.action == action)
            .map(|c| usize::from(c.slot) + 1)
            .max()
            .unwrap_or(0)
    }
}

/// **The user's keys** on `platform`: the base map with `custom` applied, every key with where it works. The page's
/// whole map (`keymap.map`).
pub fn user_keys(platform: Platform, custom: &CustomKeys) -> Vec<Binding> {
    let mut all = Vec::new();
    for action in KeyAction::all() {
        let base = action.base_keys();
        for slot in 0..base.len().max(custom.slots(action)) {
            let Ok(slot_number) = u8::try_from(slot) else {
                break;
            };
            match custom.get(action, slot_number) {
                Some(CustomKey {
                    change: KeyChange::Removed,
                    ..
                }) => {}
                Some(CustomKey {
                    change: KeyChange::Replaced { key },
                    works_in_terminals,
                    ..
                }) => all.extend(custom_bindings(
                    platform,
                    action,
                    slot_number,
                    key,
                    *works_in_terminals,
                )),
                None => {
                    if let Some(texts) = base.get(slot) {
                        all.extend(base_bindings(platform, action, slot_number, texts));
                    }
                }
            }
        }
    }
    all
}

/// The base map's bindings: [`user_keys`] with no changes.
pub fn bindings(platform: Platform) -> Vec<Binding> {
    user_keys(platform, &CustomKeys::default())
}

fn binding(
    action: KeyAction,
    slot: u8,
    platform: Platform,
    key: Key,
    place: KeyPlace,
    custom: bool,
) -> Binding {
    Binding {
        action,
        slot,
        text: key.text(platform),
        key,
        place,
        custom,
    }
}

/// One base slot: on macOS its one key, working in terminals when it holds ⌘ or is one of [`MAC_TERMINAL_KEYS`]; on
/// Windows/Linux its key outside terminals and its terminal column's key inside them (one binding when they're equal).
fn base_bindings(
    platform: Platform,
    action: KeyAction,
    slot: u8,
    texts: &[String; 3],
) -> Vec<Binding> {
    let parsed = |text: &str| (!text.is_empty()).then(|| Key::parse(text)).flatten();
    if platform.is_mac() {
        let Some(key) = parsed(&texts[0]) else {
            return vec![];
        };
        let reaches = key.stroke().is_some_and(|s| s.modifiers.platform)
            || MAC_TERMINAL_KEYS.contains(&texts[0].as_str());
        let place = if reaches {
            KeyPlace::Everywhere
        } else {
            KeyPlace::OutsideTerminals
        };
        return vec![binding(action, slot, platform, key, place, false)];
    }
    match (parsed(&texts[1]), parsed(&texts[2])) {
        (Some(out), Some(inside)) if out == inside => {
            vec![binding(
                action,
                slot,
                platform,
                out,
                KeyPlace::Everywhere,
                false,
            )]
        }
        (out, inside) => out
            .map(|k| binding(action, slot, platform, k, KeyPlace::OutsideTerminals, false))
            .into_iter()
            .chain(
                inside
                    .map(|k| binding(action, slot, platform, k, KeyPlace::InsideTerminals, false)),
            )
            .collect(),
    }
}

/// A recorded key. Letters and sequences: outside terminals. Works inside terminals: everywhere, unchanged. Otherwise
/// on macOS a ⌘ key works everywhere and any other stays the terminal's; on Windows/Linux a Ctrl key gets Shift inside
/// terminals, as the base map's do (Ctrl+R → Ctrl+Shift+R), and a key without Ctrl stays the terminal's there. Keys of
/// [`never_in_terminals`] are never bound inside them.
fn custom_bindings(
    platform: Platform,
    action: KeyAction,
    slot: u8,
    key: &Key,
    works_in_terminals: bool,
) -> Vec<Binding> {
    let make = |key: Key, place| binding(action, slot, platform, key, place, true);
    let Some(stroke) = key.stroke() else {
        return vec![make(key.clone(), KeyPlace::OutsideTerminals)];
    };
    let inside = if is_never_in_terminals(platform, key) {
        None
    } else if works_in_terminals || (platform.is_mac() && stroke.modifiers.platform) {
        Some(key.clone())
    } else if !platform.is_mac() && stroke.modifiers.control {
        let shifted = Key::Stroke {
            stroke: Keystroke {
                modifiers: Modifiers {
                    shift: true,
                    ..stroke.modifiers
                },
                ..stroke.clone()
            },
        };
        (!is_never_in_terminals(platform, &shifted)).then_some(shifted)
    } else {
        None
    };
    match inside {
        Some(inside) if &inside == key => vec![make(inside, KeyPlace::Everywhere)],
        Some(inside) => vec![
            make(key.clone(), KeyPlace::OutsideTerminals),
            make(inside, KeyPlace::InsideTerminals),
        ],
        None => vec![make(key.clone(), KeyPlace::OutsideTerminals)],
    }
}

/// The description editor's own keys that may also be app keys (0097, d0078): while its text has the focus, ⌘K /
/// Ctrl+K inserts a link (not the palette) and ⌘B / Ctrl+B is Bold (the base map has no ⌘B since d0113; a key the
/// user records there stays Bold in the editor). The only keys allowed two meanings, by focus.
pub fn description_editor_keys(platform: Platform) -> [&'static str; 2] {
    if platform.is_mac() {
        ["⌘K", "⌘B"]
    } else {
        ["Ctrl+K", "Ctrl+B"]
    }
}

/// A focused terminal's own keys, outside the app's map (as the GPUI terminal bound them): copy and paste, and ⌘+ /
/// ⌘− for the font size. On Windows and Linux Ctrl+letter belongs to the program in the terminal, so they are on
/// Ctrl+Shift there, as in Windows Terminal and Linux terminals. ⇧↵ is a new line in the prompt: xterm.js sends the
/// same `\r` as ↵, so the terminal sends ESC + CR instead (OT-5). `TerminalKey` on the page.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum TerminalKey {
    Copy,
    Paste,
    FontBigger,
    FontSmaller,
    /// A new line in the program's prompt instead of sending it: ESC + CR (⌥↵, what Claude Code's `/terminal-setup`
    /// makes ⇧↵ send). Claude Code, Codex, Copilot CLI and OpenCode all take it as a new line; zsh adds a line to
    /// the command and bash ignores it, so a Project terminal gets it too.
    NewLine,
}

impl TerminalKey {
    pub const ALL: [Self; 5] = [
        Self::Copy,
        Self::Paste,
        Self::FontBigger,
        Self::FontSmaller,
        Self::NewLine,
    ];

    /// Its shortcuts on `platform` (Windows/Linux in or out of the terminal alike). On macOS copy and paste have
    /// none here: the Edit menu's ⌘C / ⌘V reach the terminal's own copy and paste.
    pub fn keys(self, platform: Platform) -> &'static [&'static str] {
        match (self, platform) {
            (Self::Copy | Self::Paste, Platform::Mac) => &[],
            (Self::Copy, _) => &["Ctrl+Shift+C"],
            (Self::Paste, _) => &["Ctrl+Shift+V"],
            (Self::FontBigger, Platform::Mac) => &["⌘=", "⇧⌘="],
            (Self::FontBigger, _) => &["Ctrl+Shift+="],
            (Self::FontSmaller, Platform::Mac) => &["⌘-"],
            (Self::FontSmaller, _) => &["Ctrl+Shift+-"],
            (Self::NewLine, Platform::Mac) => &["⇧↵"],
            (Self::NewLine, _) => &["Shift+↵"],
        }
    }
}

/// One of a terminal's own shortcuts as the page matches it. `TerminalKeyBinding` on the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "TerminalKeyBinding")]
pub struct TerminalBinding {
    pub key: TerminalKey,
    pub keystroke: Keystroke,
}

/// Every terminal shortcut on `platform`.
pub fn terminal_bindings(platform: Platform) -> Vec<TerminalBinding> {
    TerminalKey::ALL
        .into_iter()
        .flat_map(|key| {
            key.keys(platform).iter().filter_map(move |text| {
                Some(TerminalBinding {
                    key,
                    keystroke: Keystroke::parse(text)?,
                })
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    type Row = (KeyAction, Vec<String>, Vec<String>, Vec<String>);

    /// Decision 0097's base map (KEY-01) and its single letters (KEY-02), one row per action: macOS, Windows/Linux, in
    /// a terminal (Windows/Linux). Pairs are split into their two actions, Project 1–9 and the sheet tabs 1–4 expanded;
    /// `[]` where the decision has no key ("—", and letters in a terminal).
    fn table_0097() -> Vec<Row> {
        use KeyAction::*;
        let v = |keys: &[&str]| keys.iter().map(|k| (*k).to_owned()).collect::<Vec<_>>();
        let row = |a, mac: &[&str], win: &[&str], term: &[&str]| (a, v(mac), v(win), v(term));
        let mut table = vec![
            row(
                CommandPalette,
                &["⌘K", "⇧⌘P"],
                &["Ctrl+K", "Ctrl+Shift+P"],
                &["Ctrl+K", "Ctrl+Shift+P"],
            ),
            row(SwitchProject, &["⌘P"], &["Ctrl+P"], &["Ctrl+P"]),
            row(NextInboxItem, &["⌘J"], &["Ctrl+J"], &["Ctrl+Shift+J"]),
            row(
                NextTicket,
                &["⌘]", "j"],
                &["Ctrl+]", "j"],
                &["Ctrl+Shift+]"],
            ),
            row(
                PreviousTicket,
                &["⌘[", "k"],
                &["Ctrl+[", "k"],
                &["Ctrl+Shift+["],
            ),
            row(FilterBoard, &["/"], &["/"], &[]),
            row(FocusEpic, &["g e"], &["g e"], &[]),
            row(ShowShortcuts, &["?"], &["?"], &[]),
            row(ShowBoard, &["⇧⌘B", "g b"], &["Ctrl+Shift+B", "g b"], &[]),
            row(ShowList, &["g l"], &["g l"], &[]),
            // Stop all's row, reused for Pause (d0112).
            row(Pause, &["⇧⌘."], &["Ctrl+Shift+."], &["Ctrl+Shift+."]),
            row(
                OpenActivity,
                &["⇧⌘U", "g a"],
                &["Ctrl+Shift+U", "g a"],
                &["Ctrl+Shift+U"],
            ),
            row(Settings, &["⌘,"], &["Ctrl+,"], &["Ctrl+,"]),
            row(FocusPane, &["⌘L"], &["Ctrl+L"], &["Ctrl+Shift+L"]),
            row(NewProjectAgent, &["⇧⌘L"], &["Ctrl+Shift+L"], &[]),
            row(NewTicket, &["⌘N", "c"], &["Ctrl+N", "c"], &["Ctrl+Shift+N"]),
            row(OpenPanel, &["↵"], &["↵"], &[]),
            row(ClosePanel, &["Esc"], &["Esc"], &[]),
            row(ExpandPanel, &["e"], &["e"], &[]),
            row(NextTab, &["⌃Tab"], &["Ctrl+Tab"], &["Ctrl+Tab"]),
            row(
                PreviousTab,
                &["⌃⇧Tab"],
                &["Ctrl+Shift+Tab"],
                &["Ctrl+Shift+Tab"],
            ),
            row(Approve, &["⌘↵"], &["Ctrl+↵"], &["Ctrl+Shift+↵"]),
            row(SendBack, &["⌘⌫"], &["Ctrl+⌫"], &["Ctrl+Shift+⌫"]),
            row(StartDev, &["⌘D"], &["Ctrl+D"], &["Ctrl+Shift+D"]),
            row(Park, &[], &[], &[]),
            row(CloseTicket, &[], &[], &[]),
            row(OpenPr, &[], &[], &[]),
            row(RerunValidation, &[], &[], &[]),
            row(OpenDiff, &[], &[], &[]),
            row(Terminals, &["⌃`", "g t"], &["Ctrl+`", "g t"], &["Ctrl+`"]),
            row(FindInTerminal, &["⌘F"], &[], &["Ctrl+Shift+F"]),
            row(
                ShowFiles,
                &["⇧⌘O", "g f"],
                &["Ctrl+Shift+O", "g f"],
                &["Ctrl+Shift+O"],
            ),
            row(GoToFile, &["⌥⌘P"], &["Ctrl+Alt+P"], &["Ctrl+Shift+Alt+P"]),
        ];
        for n in 1..=9 {
            let (mac, win) = (format!("⌘{n}"), format!("Ctrl+{n}"));
            table.push(row(Project(n), &[&mac], &[&win], &[&win]));
        }
        // The sheet's tabs (SHT-02, SHB-04): 1–4 and g o / s / f / a.
        for (n, then) in (1..=4).zip(["o", "s", "f", "a"]) {
            let (digit, sequence) = (n.to_string(), format!("g {then}"));
            table.push(row(
                SwitchTab(n),
                &[&digit, &sequence],
                &[&digit, &sequence],
                &[],
            ));
        }
        table
    }

    const COLUMNS: [Platform; 3] = [
        Platform::Mac,
        Platform::WinLinux,
        Platform::WinLinuxTerminal,
    ];

    fn column(row: &Row, platform: Platform) -> &[String] {
        match platform {
            Platform::Mac => &row.1,
            Platform::WinLinux => &row.2,
            Platform::WinLinuxTerminal => &row.3,
        }
    }

    /// The key event that presses `text` (a key of the map), as a US keyboard reports it.
    fn press(text: &str) -> KeyPress {
        match Key::parse(text).unwrap_or_else(|| panic!("{text} parses")) {
            Key::Stroke { stroke } => KeyPress {
                key: stroke.key,
                code: stroke.code,
                modifiers: stroke.modifiers,
            },
            Key::Letter { letter } | Key::Sequence { then: letter } => typed(&letter),
        }
    }

    /// The key event of a typed character on a US keyboard (`?` is ⇧/).
    fn typed(c: &str) -> KeyPress {
        let shifted = c == "?";
        KeyPress {
            key: c.to_owned(),
            code: if shifted {
                "Slash".to_owned()
            } else {
                code_of(c.chars().next().unwrap()).unwrap_or_default()
            },
            modifiers: Modifiers {
                shift: shifted,
                ..Modifiers::default()
            },
        }
    }

    /// The action a key runs, as the page's dispatcher finds it (`ui/src/app/keys.ts`): a focused terminal keeps what
    /// [`route`] gives it; otherwise the first binding of the key that works where it was pressed. `after_g`: pressed
    /// after a lone `g`.
    fn action_for(
        platform: Platform,
        in_terminal: bool,
        press: &KeyPress,
        after_g: bool,
        bindings: &[Binding],
    ) -> Option<KeyAction> {
        if in_terminal && route(platform, true, press, bindings) == KeyRoute::Terminal {
            return None;
        }
        bindings
            .iter()
            .find(|b| {
                let here = if in_terminal {
                    b.place.inside_terminals() && !b.key.is_letter()
                } else {
                    b.place.outside_terminals()
                };
                here && b.key.matches(press, after_g)
            })
            .map(|b| b.action)
    }

    /// Runs `text` (a sequence's second key after `g`).
    fn runs(
        platform: Platform,
        in_terminal: bool,
        text: &str,
        bindings: &[Binding],
    ) -> Option<KeyAction> {
        let sequence = matches!(Key::parse(text), Some(Key::Sequence { .. }));
        action_for(platform, in_terminal, &press(text), sequence, bindings)
    }

    /// Every action has 0097's keys in each column, and `all()` lists exactly the table's actions.
    #[test]
    fn every_action_has_its_0097_keys() {
        let table = table_0097();
        for row in &table {
            for platform in COLUMNS {
                assert_eq!(
                    row.0.default_keys(platform),
                    column(row, platform),
                    "{:?} on {platform:?}",
                    row.0
                );
            }
        }
        let mut listed = KeyAction::all();
        let mut tabled: Vec<KeyAction> = table.iter().map(|r| r.0).collect();
        let order = |a: &KeyAction| format!("{a:?}");
        listed.sort_by_key(order);
        tabled.sort_by_key(order);
        assert_eq!(listed, tabled);
    }

    /// The bindings as the dispatcher tries them while a ticket sheet is open: the sheet's keys first.
    fn sheet_first(bindings: &[Binding]) -> Vec<Binding> {
        let (sheet, rest): (Vec<Binding>, Vec<Binding>) =
            bindings.iter().cloned().partition(|b| b.action.in_sheet());
        sheet.into_iter().chain(rest).collect()
    }

    /// Each key of the table, pressed where its column applies, runs its action: macOS on the board, and in a terminal
    /// when it holds ⌘ or is ⌃` / ⌃Tab / ⌃⇧Tab; Windows/Linux on the board; the terminal column in a Windows/Linux
    /// terminal, where a Windows/Linux key outside that column never runs its action. The sheet's keys run theirs in
    /// an open sheet (`g a`: Activity without one).
    #[test]
    fn each_0097_key_runs_its_action_where_it_applies() {
        for row in table_0097() {
            let action = row.0;
            let (mac, win) = match action.in_sheet() {
                true => (
                    sheet_first(&bindings(Platform::Mac)),
                    sheet_first(&bindings(Platform::WinLinux)),
                ),
                false => (bindings(Platform::Mac), bindings(Platform::WinLinux)),
            };
            for text in &row.1 {
                assert_eq!(
                    runs(Platform::Mac, false, text, &mac),
                    Some(action),
                    "{text}"
                );
                let in_terminal =
                    text.contains('⌘') || ["⌃`", "⌃Tab", "⌃⇧Tab"].contains(&text.as_str());
                assert_eq!(
                    runs(Platform::Mac, true, text, &mac),
                    in_terminal.then_some(action),
                    "{text} in a Mac terminal"
                );
            }
            for text in &row.2 {
                assert_eq!(
                    runs(Platform::WinLinux, false, text, &win),
                    Some(action),
                    "{text}"
                );
                if !row.3.contains(text) {
                    assert_ne!(
                        runs(Platform::WinLinux, true, text, &win),
                        Some(action),
                        "{text} isn't in the terminal column"
                    );
                }
            }
            for text in &row.3 {
                assert_eq!(
                    runs(Platform::WinLinux, true, text, &win),
                    Some(action),
                    "{text} in a terminal"
                );
            }
        }
    }

    /// KEY-03's examples in a Windows/Linux terminal.
    #[test]
    fn a_windows_and_linux_terminal_uses_the_terminal_column() {
        let win = bindings(Platform::WinLinux);
        let in_terminal = |text: &str| runs(Platform::WinLinux, true, text, &win);
        assert_eq!(in_terminal("Ctrl+K"), Some(KeyAction::CommandPalette));
        assert_eq!(in_terminal("Ctrl+`"), Some(KeyAction::Terminals));
        assert_eq!(in_terminal("Ctrl+Shift+D"), Some(KeyAction::StartDev));
        assert_eq!(in_terminal("Ctrl+D"), None);
        assert_eq!(in_terminal("Ctrl+L"), None);
        // Ctrl+Shift+L belongs to the Project agent, so New conversation has no key there; Board has none either, and
        // Ctrl+Shift+B is free since the pane's hide / show key went (d0113).
        assert_eq!(in_terminal("Ctrl+Shift+B"), None);
        assert_eq!(in_terminal("Ctrl+Shift+L"), Some(KeyAction::FocusPane));
        for action in [KeyAction::ShowBoard, KeyAction::NewProjectAgent] {
            assert!(
                win.iter()
                    .all(|b| b.action != action || !b.place.inside_terminals()),
                "{action:?}"
            );
        }
        // Single letters never work in a terminal.
        assert!(
            win.iter()
                .all(|b| !b.key.is_letter() || b.place == KeyPlace::OutsideTerminals)
        );
        assert_eq!(in_terminal("j"), None);
        assert_eq!(
            route(Platform::WinLinux, true, &typed("j"), &win),
            KeyRoute::Terminal
        );
    }

    /// KEY-03: in a Windows/Linux terminal Ctrl+1–9, Ctrl+`, Ctrl+, , Ctrl+Tab / Ctrl+Shift+Tab, Ctrl+K and Ctrl+P
    /// (with Ctrl+Shift+P, the palette's other key) work unchanged; every other key there is its Windows/Linux key with
    /// Shift added. Find has no Windows/Linux key outside a terminal.
    #[test]
    fn the_terminal_lets_these_through_and_adds_shift_to_the_rest() {
        let mut let_through: Vec<String> = (1..=9).map(|n| format!("Ctrl+{n}")).collect();
        let_through.extend(["Ctrl+`", "Ctrl+,", "Ctrl+Tab", "Ctrl+K", "Ctrl+P"].map(String::from));
        let_through.sort();
        let win = bindings(Platform::WinLinux);
        let everywhere = |shift: bool| -> Vec<String> {
            let mut keys: Vec<String> = win
                .iter()
                .filter(|b| b.place == KeyPlace::Everywhere)
                .filter(|b| b.key.stroke().unwrap().modifiers.shift == shift)
                .map(|b| b.text.clone())
                .collect();
            keys.sort();
            keys
        };
        // The Ctrl keys a terminal lets through; the rest that work in one already hold Shift.
        assert_eq!(everywhere(false), let_through);
        for text in ["Ctrl+Shift+Tab", "Ctrl+Shift+P", "Ctrl+Shift+U"] {
            assert!(everywhere(true).contains(&text.to_owned()), "{text}");
        }
        for b in win.iter().filter(|b| b.place == KeyPlace::InsideTerminals) {
            let stroke = b.key.stroke().unwrap();
            assert!(
                stroke.modifiers.control && stroke.modifiers.shift,
                "{}",
                b.text
            );
            let outside = win.iter().find(|o| {
                o.action == b.action && o.slot == b.slot && o.place == KeyPlace::OutsideTerminals
            });
            match outside {
                Some(o) => {
                    let mut shifted = o.key.stroke().unwrap().clone();
                    shifted.modifiers.shift = true;
                    assert_eq!(&shifted, stroke, "{} is {} with Shift", b.text, o.text);
                }
                None => assert_eq!(b.action, KeyAction::FindInTerminal),
            }
        }
    }

    /// KEY-03: Ctrl+[ and Ctrl+\ (Escape, force quit) are never taken in a terminal, and Ctrl+Shift+C / V stay the
    /// terminal's copy and paste, not even by a custom key marked Works inside terminals.
    #[test]
    fn the_terminals_own_ctrl_keys_are_never_bound_there() {
        let custom = CustomKeys(
            ["Ctrl+C", "Ctrl+[", "Ctrl+\\", "Ctrl+Shift+V"]
                .iter()
                .enumerate()
                .map(|(i, text)| CustomKey {
                    action: [
                        KeyAction::Park,
                        KeyAction::CloseTicket,
                        KeyAction::OpenPr,
                        KeyAction::OpenDiff,
                    ][i],
                    slot: 0,
                    change: KeyChange::Replaced {
                        key: Key::parse(text).unwrap(),
                    },
                    works_in_terminals: true,
                })
                .collect(),
        );
        for keys in [
            bindings(Platform::WinLinux),
            user_keys(Platform::WinLinux, &custom),
        ] {
            for text in [
                "Ctrl+[",
                "Ctrl+\\",
                "Ctrl+C",
                "Ctrl+D",
                "Ctrl+Shift+C",
                "Ctrl+Shift+V",
            ] {
                let key = Key::parse(text).unwrap();
                assert!(
                    keys.iter()
                        .all(|b| b.key != key || !b.place.inside_terminals()),
                    "{text} bound in a terminal"
                );
            }
            for text in ["Ctrl+[", "Ctrl+\\", "Ctrl+C"] {
                assert_eq!(
                    route(Platform::WinLinux, true, &press(text), &keys),
                    KeyRoute::Terminal,
                    "{text}"
                );
            }
        }
        let base = bindings(Platform::WinLinux);
        for text in ["Ctrl+Shift+C", "Ctrl+Shift+V"] {
            let key = Key::parse(text).unwrap();
            assert!(base.iter().all(|b| b.key != key), "{text}");
        }
    }

    /// The keys of one place: macOS or Windows/Linux, outside or inside a terminal.
    fn place_keys(
        platform: Platform,
        inside: bool,
        bindings: &[Binding],
    ) -> Vec<(Key, KeyAction, String)> {
        bindings
            .iter()
            .filter(|b| {
                if inside {
                    b.place.inside_terminals()
                } else {
                    b.place.outside_terminals()
                }
            })
            .map(|b| {
                (
                    b.key.clone(),
                    b.action,
                    format!("{} on {platform:?}", b.text),
                )
            })
            .collect()
    }

    /// 0097: no key means two things in the same place: macOS (outside and inside terminals), Windows/Linux, and
    /// Windows/Linux while a terminal has focus.
    #[test]
    fn no_key_means_two_things_in_one_place() {
        for platform in [Platform::Mac, Platform::WinLinux] {
            let all = bindings(platform);
            for inside in [false, true] {
                let mut seen: HashMap<Key, KeyAction> = HashMap::new();
                for (key, action, text) in place_keys(platform, inside, &all) {
                    if let Some(other) = seen.insert(key, action) {
                        // `g a` is the sheet's Activity tab while a sheet is open, else Activity (SHB-04); `g f`
                        // its Files tab, else the Files view (FIL-11).
                        let sheet_first = (other.in_sheet() || action.in_sheet())
                            && [other, action].iter().any(|a| {
                                matches!(a, KeyAction::OpenActivity | KeyAction::ShowFiles)
                            });
                        assert!(other == action || sheet_first, "{text} (inside: {inside})");
                    }
                }
            }
        }
    }

    /// 0097, d0078, d0113: the description editor's toolbar keys share only ⌘K (Link) with the map, which the editor
    /// keeps while its text has the focus (⌘B is Bold alone since the pane's key went). ⇧⌘B is the editor's
    /// Blockquote too: Board takes it first, as under 0058 (a deferred question of T-0389).
    #[test]
    fn only_link_and_bold_share_a_key_with_the_description_editor() {
        // `ui/src/markdown/toolbar/buttons.ts`: Bold, Italic, Strikethrough, Code, Link, lists, Blockquote, Code block.
        let editor = ["B", "I", "⇧X", "E", "K", "⇧8", "⇧7", "⇧9", "⇧B", "⌥C"];
        for platform in [Platform::Mac, Platform::WinLinux] {
            let editor: Vec<Key> = editor
                .iter()
                .map(|k| {
                    let mac = format!("⌘{k}").replace("⌘⇧", "⇧⌘").replace("⌘⌥", "⌥⌘");
                    let mut stroke = Keystroke::parse(&mac).unwrap();
                    if platform == Platform::WinLinux {
                        stroke.modifiers.platform = false;
                        stroke.modifiers.control = true;
                    }
                    Key::Stroke { stroke }
                })
                .collect();
            let mut shared: Vec<String> = bindings(platform)
                .iter()
                .filter(|b| b.place.outside_terminals() && editor.contains(&b.key))
                .map(|b| b.text.clone())
                .collect();
            shared.sort();
            let mut want: Vec<String> = vec![description_editor_keys(platform)[0].to_owned()];
            want.push(
                if platform == Platform::Mac {
                    "⇧⌘B"
                } else {
                    "Ctrl+Shift+B"
                }
                .to_owned(),
            );
            want.sort();
            assert_eq!(shared, want, "{platform:?}");
        }
        let mac = bindings(Platform::Mac);
        assert_eq!(
            runs(Platform::Mac, false, "⌘K", &mac),
            Some(KeyAction::CommandPalette)
        );
        // ⌘B is Bold in the editor only: the app has no ⌘B since the pane went (d0113).
        assert_eq!(runs(Platform::Mac, false, "⌘B", &mac), None);
    }

    /// 0097 on macOS: ⌃`, ⌃Tab and ⌃⇧Tab reach the app from a focused terminal; ⌃C, ⌃D, ⌃R and a bare letter stay the
    /// terminal's; ⌘ keys reach the app as before; a custom ⌃R stays the terminal's until marked Works inside
    /// terminals.
    #[test]
    fn a_mac_terminal_gives_the_app_ctrl_backquote_and_ctrl_tab_only() {
        let mac = bindings(Platform::Mac);
        let to = |text: &str, keys: &[Binding]| route(Platform::Mac, true, &press(text), keys);
        for text in ["⌃`", "⌃Tab", "⌃⇧Tab"] {
            assert_eq!(to(text, &mac), KeyRoute::App, "{text}");
        }
        assert_eq!(
            runs(Platform::Mac, true, "⌃`", &mac),
            Some(KeyAction::Terminals)
        );
        assert_eq!(
            runs(Platform::Mac, true, "⌃Tab", &mac),
            Some(KeyAction::NextTab)
        );
        assert_eq!(
            runs(Platform::Mac, true, "⌃⇧Tab", &mac),
            Some(KeyAction::PreviousTab)
        );
        for text in ["⌃C", "⌃D", "⌃R", "⌃J", "⌃[", "j", "e", "?", "↵", "Esc"] {
            assert_eq!(to(text, &mac), KeyRoute::Terminal, "{text}");
        }
        for text in ["⌘K", "⌘J", "⇧⌘I", "⌘D", "⌘F", "⌘B", "⌘X", "⇧⌘Z"] {
            assert_eq!(to(text, &mac), KeyRoute::App, "{text}");
        }
        let custom = |works_in_terminals| {
            CustomKeys(vec![CustomKey {
                action: KeyAction::OpenActivity,
                slot: 0,
                change: KeyChange::Replaced {
                    key: Key::parse("⌃R").unwrap(),
                },
                works_in_terminals,
            }])
        };
        let mine = user_keys(Platform::Mac, &custom(false));
        assert_eq!(to("⌃R", &mine), KeyRoute::Terminal);
        assert_eq!(
            runs(Platform::Mac, false, "⌃R", &mine),
            Some(KeyAction::OpenActivity)
        );
        let through = user_keys(Platform::Mac, &custom(true));
        assert_eq!(to("⌃R", &through), KeyRoute::App);
        assert_eq!(
            runs(Platform::Mac, true, "⌃R", &through),
            Some(KeyAction::OpenActivity)
        );
        // ⌃D can't be let through (T-0390 refuses it; the map never binds it there).
        let exit = CustomKeys(vec![CustomKey {
            action: KeyAction::Park,
            slot: 0,
            change: KeyChange::Replaced {
                key: Key::parse("⌃D").unwrap(),
            },
            works_in_terminals: true,
        }]);
        assert_eq!(
            to("⌃D", &user_keys(Platform::Mac, &exit)),
            KeyRoute::Terminal
        );
    }

    #[test]
    fn without_terminal_focus_everything_is_the_apps() {
        let mac = bindings(Platform::Mac);
        for text in ["Esc", "↵", "j", "⌘K", "⌃C"] {
            assert_eq!(
                route(Platform::Mac, false, &press(text), &mac),
                KeyRoute::App
            );
        }
    }

    #[test]
    fn the_terminal_app_modifiers_still_take_command_and_ctrl_shift_keys() {
        assert_eq!(terminal_app_modifiers(Platform::Mac).len(), 1);
        assert!(terminal_app_modifiers(Platform::Mac)[0].platform);
        let held = Modifiers {
            control: true,
            shift: true,
            alt: true,
            ..Modifiers::default()
        };
        assert!(held.holds(&terminal_app_modifiers(Platform::WinLinux)[0]));
        assert!(!Modifiers::default().holds(&terminal_app_modifiers(Platform::WinLinux)[0]));
        // Unbound, they still leave the terminal's own keys (copy, paste) to the terminal's handler.
        assert_eq!(
            route(Platform::WinLinux, true, &press("Ctrl+Shift+C"), &[]),
            KeyRoute::App
        );
        assert_eq!(
            route(Platform::WinLinux, true, &press("Ctrl+R"), &[]),
            KeyRoute::Terminal
        );
    }

    /// The new kinds of key parse, and a letter is matched by the character typed, whatever the layout.
    #[test]
    fn letters_sequences_and_ctrl_keys_parse() {
        let letter = |l: &str| {
            Some(Key::Letter {
                letter: l.to_owned(),
            })
        };
        assert_eq!(Key::parse("j"), letter("j"));
        assert_eq!(Key::parse("/"), letter("/"));
        assert_eq!(Key::parse("?"), letter("?"));
        assert_eq!(Key::parse("1"), letter("1"));
        assert_eq!(
            Key::parse("g b"),
            Some(Key::Sequence {
                then: "b".to_owned()
            })
        );
        let stroke = |text| Key::parse(text).and_then(|k| k.stroke().cloned()).unwrap();
        let tab = stroke("⌃Tab");
        assert_eq!((tab.key.as_str(), tab.code.as_str()), ("Tab", "Tab"));
        assert!(tab.modifiers.control && !tab.modifiers.platform && !tab.modifiers.shift);
        let back = stroke("⌃`");
        assert_eq!((back.key.as_str(), back.code.as_str()), ("`", "Backquote"));
        assert!(back.modifiers.control);
        let new_agent = stroke("⇧⌘L");
        assert_eq!(new_agent.code, "KeyL");
        assert!(
            new_agent.modifiers.shift
                && new_agent.modifiers.platform
                && !new_agent.modifiers.control
        );
        assert!(stroke("⌃⇧Tab").modifiers.shift);
        assert_eq!(stroke("Ctrl+Shift+Tab").key, "Tab");
        for wrong in [
            "",
            "J",
            "g",
            "g bb",
            "g ",
            "Ctrl+",
            "Meta+K",
            "⌘",
            "⌘KK",
            "Ctrl+Shift+F5",
        ] {
            let parsed = Key::parse(wrong);
            assert!(
                parsed.is_none() || wrong == "g",
                "{wrong} parsed as {parsed:?}"
            );
        }
        // Plain `g` is a letter key (T-0390 refuses recording it: it starts every sequence).
        assert_eq!(Key::parse("g"), letter("g"));

        let at = |key: &str, code: &str, shift: bool| KeyPress {
            key: key.to_owned(),
            code: code.to_owned(),
            modifiers: Modifiers {
                shift,
                ..Modifiers::default()
            },
        };
        let j = Key::parse("j").unwrap();
        // Dvorak types j where QWERTY has C; AZERTY types a where QWERTY has Q; a Cyrillic layout types о on J's key.
        assert!(j.matches(&at("j", "KeyC", false), false));
        assert!(!j.matches(&at("c", "KeyJ", false), false));
        assert!(
            Key::parse("a")
                .unwrap()
                .matches(&at("a", "KeyQ", false), false)
        );
        assert!(j.matches(&at("о", "KeyJ", false), false));
        assert!(!j.matches(&at("J", "KeyJ", true), false), "⇧J isn't j");
        assert!(
            !j.matches(&at("j", "KeyJ", false), true),
            "after g only a sequence matches"
        );
        // `/` is ⇧7 on a German keyboard, `?` ⇧/ on a US one: by the character, Shift or not.
        assert!(
            Key::parse("/")
                .unwrap()
                .matches(&at("/", "Digit7", true), false)
        );
        assert!(
            Key::parse("?")
                .unwrap()
                .matches(&at("?", "Slash", true), false)
        );
        let gb = Key::parse("g b").unwrap();
        assert!(
            gb.matches(&at("b", "KeyN", false), true),
            "Dvorak b after g"
        );
        assert!(
            !gb.matches(&at("b", "KeyB", false), false),
            "b alone isn't g b"
        );
        // Modifier keys match letters by the layout too: ⌘K on Dvorak.
        let cmd_k = Key::parse("⌘K").unwrap();
        let mut dvorak_k = at("k", "KeyV", false);
        dvorak_k.modifiers.platform = true;
        assert!(cmd_k.matches(&dvorak_k, false));
    }

    /// Every key of the table parses, and shows as the table writes it.
    #[test]
    fn every_key_parses_and_shows_as_written() {
        for row in table_0097() {
            for platform in COLUMNS {
                for text in column(&row, platform) {
                    let key = Key::parse(text).unwrap_or_else(|| panic!("{text}"));
                    let shown = if platform == Platform::WinLinuxTerminal {
                        Platform::WinLinux
                    } else {
                        platform
                    };
                    assert_eq!(&key.text(shown), text);
                }
            }
        }
    }

    #[test]
    fn shortcut_texts_parse_into_the_web_key_names() {
        let parse = |text: &str| Keystroke::parse(text).unwrap();
        let mods = |control, alt, shift, platform| Modifiers {
            control,
            alt,
            shift,
            platform,
            function: false,
        };
        let activity = parse("⇧⌘U");
        assert_eq!(
            (activity.key.as_str(), activity.code.as_str()),
            ("u", "KeyU")
        );
        assert_eq!(activity.modifiers, mods(false, false, true, true));
        let approve = parse("Ctrl+Shift+↵");
        assert_eq!(
            (approve.key.as_str(), approve.code.as_str()),
            ("Enter", "Enter")
        );
        assert_eq!(approve.modifiers, mods(true, false, true, false));
        assert_eq!(parse("Ctrl+⌫").key, "Backspace");
        assert_eq!(parse("Esc").modifiers, Modifiers::default());
        assert_eq!(parse("Esc").code, "Escape");
        assert_eq!(parse("Ctrl+`").code, "Backquote");
        assert_eq!(parse("⌘,").code, "Comma");
        assert_eq!(parse("Ctrl+Shift+[").code, "BracketLeft");
        assert_eq!(parse("⌘]").code, "BracketRight");
        let go = parse("Ctrl+Shift+Alt+P");
        assert!(go.modifiers.control && go.modifiers.shift && go.modifiers.alt);
        assert!(parse("⌥⌘P").modifiers.alt);
    }

    /// The user's keys: a replaced key, a removed key, a key added to an action with none, and Works inside terminals
    /// on Windows/Linux.
    #[test]
    fn the_users_keys_apply_each_change_to_its_slot() {
        let change = |action, slot, text: Option<&str>, works_in_terminals| CustomKey {
            action,
            slot,
            change: match text {
                Some(t) => KeyChange::Replaced {
                    key: Key::parse(t).unwrap(),
                },
                None => KeyChange::Removed,
            },
            works_in_terminals,
        };
        // The example's custom keys (tickets/m29-tickets.md): Activity → ⌘J taking it from Next item that needs you, New
        // ticket's `c` → `n`, Park → `g` `p`.
        let custom = CustomKeys(vec![
            change(KeyAction::OpenActivity, 0, Some("⌘J"), false),
            change(KeyAction::NextInboxItem, 0, None, false),
            change(KeyAction::NewTicket, 1, Some("n"), false),
            change(KeyAction::Park, 0, Some("g p"), false),
        ]);
        let mac = user_keys(Platform::Mac, &custom);
        let keys_of = |action: KeyAction| -> Vec<(u8, String, bool)> {
            mac.iter()
                .filter(|b| b.action == action)
                .map(|b| (b.slot, b.text.clone(), b.custom))
                .collect()
        };
        assert_eq!(
            keys_of(KeyAction::NewTicket),
            [(0, "⌘N".to_owned(), false), (1, "n".to_owned(), true)]
        );
        assert_eq!(
            keys_of(KeyAction::OpenActivity),
            [(0, "⌘J".to_owned(), true), (1, "g a".to_owned(), false)]
        );
        assert_eq!(keys_of(KeyAction::NextInboxItem), []);
        assert!(
            action_infos()
                .iter()
                .any(|a| a.action == KeyAction::NextInboxItem),
            "it stays listed"
        );
        assert_eq!(keys_of(KeyAction::Park), [(0, "g p".to_owned(), true)]);
        assert_eq!(
            runs(Platform::Mac, false, "⌘J", &mac),
            Some(KeyAction::OpenActivity)
        );
        assert_eq!(
            runs(Platform::Mac, false, "g p", &mac),
            Some(KeyAction::Park)
        );
        assert_eq!(runs(Platform::Mac, false, "c", &mac), None);
        assert_eq!(
            runs(Platform::Mac, false, "n", &mac),
            Some(KeyAction::NewTicket)
        );
        // Untouched actions keep the base map's keys.
        let base = bindings(Platform::Mac);
        let rest = |keys: &[Binding]| -> Vec<Binding> {
            keys.iter()
                .filter(|b| {
                    ![
                        KeyAction::OpenActivity,
                        KeyAction::NextInboxItem,
                        KeyAction::NewTicket,
                        KeyAction::Park,
                    ]
                    .contains(&b.action)
                })
                .cloned()
                .collect()
        };
        assert_eq!(rest(&mac), rest(&base));
        assert_eq!(user_keys(Platform::Mac, &CustomKeys::default()), base);

        // Windows/Linux: a custom Ctrl+R gets Shift in a terminal; marked Works inside terminals, it's let through.
        let ctrl_r = |works| {
            CustomKeys(vec![change(
                KeyAction::OpenActivity,
                0,
                Some("Ctrl+R"),
                works,
            )])
        };
        let win = user_keys(Platform::WinLinux, &ctrl_r(false));
        let activity: Vec<(String, KeyPlace)> = win
            .iter()
            .filter(|b| b.action == KeyAction::OpenActivity && b.slot == 0)
            .map(|b| (b.text.clone(), b.place))
            .collect();
        assert_eq!(
            activity,
            [
                ("Ctrl+R".to_owned(), KeyPlace::OutsideTerminals),
                ("Ctrl+Shift+R".to_owned(), KeyPlace::InsideTerminals)
            ]
        );
        assert_eq!(runs(Platform::WinLinux, true, "Ctrl+R", &win), None);
        assert_eq!(
            runs(Platform::WinLinux, true, "Ctrl+Shift+R", &win),
            Some(KeyAction::OpenActivity)
        );
        let through = user_keys(Platform::WinLinux, &ctrl_r(true));
        let activity: Vec<(String, KeyPlace)> = through
            .iter()
            .filter(|b| b.action == KeyAction::OpenActivity && b.slot == 0)
            .map(|b| (b.text.clone(), b.place))
            .collect();
        assert_eq!(activity, [("Ctrl+R".to_owned(), KeyPlace::Everywhere)]);
        assert_eq!(
            route(Platform::WinLinux, true, &press("Ctrl+R"), &through),
            KeyRoute::App
        );
        assert_eq!(
            runs(Platform::WinLinux, true, "Ctrl+R", &through),
            Some(KeyAction::OpenActivity)
        );
        // A custom letter never works in a terminal, marked or not.
        let letter = user_keys(
            Platform::WinLinux,
            &CustomKeys(vec![change(KeyAction::Park, 0, Some("p"), true)]),
        );
        let park: Vec<KeyPlace> = letter
            .iter()
            .filter(|b| b.action == KeyAction::Park)
            .map(|b| b.place)
            .collect();
        assert_eq!(park, [KeyPlace::OutsideTerminals]);
    }

    /// The stored changes read back as written (T-0390 keeps them in the settings).
    #[test]
    fn custom_keys_round_trip_through_json() {
        let custom = CustomKeys(vec![
            CustomKey {
                action: KeyAction::Project(3),
                slot: 0,
                change: KeyChange::Replaced {
                    key: Key::parse("⌥⌘3").unwrap(),
                },
                works_in_terminals: false,
            },
            CustomKey {
                action: KeyAction::NextInboxItem,
                slot: 0,
                change: KeyChange::Removed,
                works_in_terminals: false,
            },
        ]);
        let json = serde_json::to_value(&custom).unwrap();
        assert_eq!(
            json[1]["change"],
            serde_json::json!({ "change": "removed" })
        );
        assert_eq!(json[0]["change"]["key"]["kind"], "stroke");
        assert_eq!(serde_json::from_value::<CustomKeys>(json).unwrap(), custom);
    }

    /// Tauri's default macOS menu keeps these keys (its app, File, Edit, View and Window menus), and the OS keeps ⌘`,
    /// ⌘Tab and ⌘Space; none may be a key of the map.
    #[test]
    fn the_default_mac_menu_and_os_keys_are_not_in_the_map() {
        let taken = [
            "⌘Q", "⌘W", "⌘H", "⌥⌘H", "⌘M", "⌃⌘F", "⌘Z", "⇧⌘Z", "⌘X", "⌘C", "⌘V", "⌘A", "⌘`", "⌘Tab",
        ];
        let taken: Vec<Key> = taken.iter().map(|t| Key::parse(t).unwrap()).collect();
        for b in bindings(Platform::Mac) {
            assert!(!taken.contains(&b.key), "{} is a menu or OS key", b.text);
            assert!(b.key.stroke().is_none_or(|s| s.key != " "), "{}", b.text);
        }
    }

    #[test]
    fn a_terminals_own_keys_parse_and_stay_off_the_app_map() {
        for platform in [Platform::Mac, Platform::WinLinux] {
            let own = terminal_bindings(platform);
            let texts: usize = TerminalKey::ALL
                .iter()
                .map(|k| k.keys(platform).len())
                .sum();
            assert_eq!(own.len(), texts, "every text parses on {platform:?}");
            let map = bindings(platform);
            for b in &own {
                // Taken from the program in the terminal, as the app's own keys are (SES-10); ⇧↵ stays the
                // program's, only its bytes change.
                let to = if b.key == TerminalKey::NewLine {
                    KeyRoute::Terminal
                } else {
                    KeyRoute::App
                };
                let press = KeyPress {
                    key: b.keystroke.key.clone(),
                    code: b.keystroke.code.clone(),
                    modifiers: b.keystroke.modifiers,
                };
                assert_eq!(route(platform, true, &press, &map), to, "{:?}", b.key);
                assert!(
                    !map.iter().any(|a| a.key.stroke() == Some(&b.keystroke)),
                    "{:?} clashes with the map on {platform:?}",
                    b.key
                );
            }
        }
        let mac = terminal_bindings(Platform::Mac);
        assert!(
            mac.iter()
                .all(|b| !matches!(b.key, TerminalKey::Copy | TerminalKey::Paste))
        );
        assert_eq!(mac[0].keystroke.code, "Equal");
        let win = terminal_bindings(Platform::WinLinux);
        assert_eq!(win[0].key, TerminalKey::Copy);
        assert_eq!(win[0].keystroke.code, "KeyC");
        for own in [&mac, &win] {
            let new_line = own.iter().find(|b| b.key == TerminalKey::NewLine).unwrap();
            assert_eq!(new_line.keystroke.code, "Enter");
            assert_eq!(
                new_line.keystroke.modifiers,
                Modifiers {
                    shift: true,
                    ..Modifiers::default()
                }
            );
        }
    }

    #[test]
    fn the_page_reads_an_action_by_its_name_and_number() {
        let json = |a: KeyAction| serde_json::to_value(a).unwrap();
        assert_eq!(
            json(KeyAction::OpenActivity),
            serde_json::json!({ "action": "open_activity" })
        );
        assert_eq!(
            json(KeyAction::CommandPalette),
            serde_json::json!({ "action": "command_palette" })
        );
        assert_eq!(
            json(KeyAction::SwitchTab(2)),
            serde_json::json!({ "action": "switch_tab", "number": 2 })
        );
        for action in KeyAction::all() {
            assert_eq!(
                serde_json::from_value::<KeyAction>(json(action)).unwrap(),
                action
            );
        }
        let b = serde_json::to_value(&bindings(Platform::Mac)[0]).unwrap();
        assert_eq!(
            b,
            serde_json::json!({
                "action": { "action": "command_palette" },
                "slot": 0,
                "text": "⌘K",
                "key": { "kind": "stroke", "stroke": { "key": "k", "code": "KeyK", "modifiers": {
                    "control": false, "alt": false, "shift": false, "platform": true, "function": false } } },
                "place": "everywhere",
                "custom": false,
            })
        );
        let j = bindings(Platform::Mac)
            .into_iter()
            .find(|b| b.text == "j")
            .unwrap();
        assert_eq!(
            serde_json::to_value(&j.key).unwrap(),
            serde_json::json!({ "kind": "letter", "letter": "j" })
        );
        let gb = bindings(Platform::Mac)
            .into_iter()
            .find(|b| b.text == "g b")
            .unwrap();
        assert_eq!(
            serde_json::to_value(&gb.key).unwrap(),
            serde_json::json!({ "kind": "sequence", "then": "b" })
        );
    }

    /// The base map's first key (⌘N, not `c`; `e` where there is only a letter).
    #[test]
    fn the_first_default_key() {
        assert_eq!(
            KeyAction::NewTicket.keys(Platform::Mac).as_deref(),
            Some("⌘N")
        );
        assert_eq!(
            KeyAction::ExpandPanel.keys(Platform::Mac).as_deref(),
            Some("e")
        );
        assert_eq!(
            KeyAction::Terminals.keys(Platform::WinLinux).as_deref(),
            Some("Ctrl+`")
        );
        assert_eq!(KeyAction::FindInTerminal.keys(Platform::WinLinux), None);
        assert_eq!(KeyAction::Park.keys(Platform::Mac), None);
        assert_eq!(
            KeyAction::SwitchTab(3).keys(Platform::Mac).as_deref(),
            Some("3")
        );
        assert_eq!(
            KeyAction::Project(2).keys(Platform::Mac).as_deref(),
            Some("⌘2")
        );
    }

    /// Each action's title and group as Settings draws them (`s15-46`, all defaults, macOS). Open diff isn't drawn there; it is listed last in The ticket.
    #[test]
    fn the_titles_and_groups_are_the_designs() {
        let mut drawn: Vec<(&str, Vec<&str>)> = Vec::new();
        for info in action_infos() {
            let group = info.group.title();
            if drawn.last().is_none_or(|(g, _)| *g != group) {
                drawn.push((group, Vec::new()));
            }
            let titles = &mut drawn.last_mut().unwrap().1;
            if titles.last() != Some(&info.title) {
                titles.push(info.title);
            }
        }
        assert_eq!(
            drawn,
            [
                (
                    "Moving around",
                    vec![
                        "Command palette",
                        "Switch project",
                        "Project 1–9",
                        "Next item that needs you",
                        "Next / previous ticket",
                        "Filter the board",
                        "Focus the epic of the selected card",
                        "Show all shortcuts",
                    ]
                ),
                (
                    "Views",
                    vec![
                        "Board",
                        "List",
                        "Files",
                        "Pause / Resume agents",
                        "Activity",
                        "Settings",
                        "Open / close the Project agent",
                        "New Project agent conversation",
                    ]
                ),
                (
                    "The ticket",
                    vec![
                        "New ticket",
                        "Open / close the ticket",
                        "Full page / back to the sheet",
                        "Ticket tabs",
                        "Go to file",
                        "Next / previous ticket tab",
                        "Approve ▾",
                        "Send back",
                        "Start / move to Dev",
                        "Park",
                        "Close ticket",
                        "Open PR",
                        "Re-run validation",
                        "Open diff",
                    ]
                ),
                (
                    "The terminal",
                    vec![
                        "Terminals: open, close, leave the terminal",
                        "Find in terminal scrollback"
                    ]
                ),
            ]
        );
        assert_eq!(
            KeyGroup::ALL.map(KeyGroup::title),
            ["Moving around", "Views", "The ticket", "The terminal"]
        );
    }

    /// FIL-11: ⇧⌘O and `g f` open the Files view (Ctrl+Shift+O on Windows/Linux, in terminals too); inside a sheet
    /// `g f` is still its Files tab, tried first as `g a` is (the dispatcher's sheet-first order), and ⌥⌘P is Go to
    /// file wherever it is pressed.
    #[test]
    fn files_is_bound_and_g_f_stays_the_files_tab_in_a_sheet() {
        let mac = bindings(Platform::Mac);
        assert_eq!(
            runs(Platform::Mac, false, "⇧⌘O", &mac),
            Some(KeyAction::ShowFiles)
        );
        assert_eq!(
            runs(Platform::Mac, false, "g f", &mac),
            Some(KeyAction::ShowFiles)
        );
        assert_eq!(
            runs(Platform::Mac, false, "g f", &sheet_first(&mac)),
            Some(KeyAction::SwitchTab(3))
        );
        assert_eq!(
            runs(Platform::Mac, false, "⌥⌘P", &mac),
            Some(KeyAction::GoToFile)
        );
        let win = bindings(Platform::WinLinux);
        assert_eq!(
            runs(Platform::WinLinux, true, "Ctrl+Shift+O", &win),
            Some(KeyAction::ShowFiles)
        );
        assert!(
            action_infos()
                .iter()
                .any(|a| a.action == KeyAction::ShowFiles)
        );
        assert_eq!(KeyAction::GoToFile.title(), "Go to file");
    }

    /// Windows/Linux in or out of a terminal binds the same map (the place of each key says where it works).
    #[test]
    fn the_windows_and_linux_columns_make_one_map() {
        assert_eq!(
            bindings(Platform::WinLinuxTerminal),
            bindings(Platform::WinLinux)
        );
        assert!(
            bindings(Platform::Mac)
                .iter()
                .all(|b| b.place != KeyPlace::InsideTerminals)
        );
    }

    /// The `g` hint (design `s02-30`): the base map's sequences in the drawn order, a recorded `g` `p` Park at the
    /// end, a removed `g` `a` left out, a replaced one in its action's place.
    #[test]
    fn the_g_hint_lists_the_users_sequences() {
        let texts = |custom: &CustomKeys| -> String {
            sequence_hint(&user_keys(Platform::Mac, custom))
                .iter()
                .map(|h| format!("{} {}", h.then, h.name))
                .collect::<Vec<_>>()
                .join(" · ")
        };
        assert_eq!(
            texts(&CustomKeys::default()),
            "b Board · l List · f Files · a Activity · t Terminals · e Epic"
        );
        let change = |action, slot, change| CustomKey {
            action,
            slot,
            change,
            works_in_terminals: false,
        };
        let park = change(
            KeyAction::Park,
            0,
            KeyChange::Replaced {
                key: Key::parse("g p").unwrap(),
            },
        );
        assert_eq!(
            texts(&CustomKeys(vec![park.clone()])),
            "b Board · l List · f Files · a Activity · t Terminals · e Epic · p Park"
        );
        let custom = CustomKeys(vec![
            park,
            change(KeyAction::OpenActivity, 1, KeyChange::Removed),
            change(
                KeyAction::Terminals,
                1,
                KeyChange::Replaced {
                    key: Key::parse("g n").unwrap(),
                },
            ),
        ]);
        assert_eq!(
            texts(&custom),
            "b Board · l List · f Files · n Terminals · e Epic · p Park"
        );
        // Windows/Linux: the same letters (they never work in a terminal).
        assert_eq!(
            sequence_hint(&bindings(Platform::WinLinux)),
            sequence_hint(&bindings(Platform::Mac))
        );
    }
}
