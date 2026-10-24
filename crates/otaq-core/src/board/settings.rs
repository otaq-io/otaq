//! Typed settings, global and per project (SET-01; d0003, d0023; SDD §20). Everything lives in the app's
//! database and nothing is ever written into a repository (d0023). Invalid values are stored as given and
//! flagged ([`SettingProblem`]), never silently fixed.
//!
//! Global settings are rows of the `settings` table, one per top-level field below (JSON values; a missing row
//! means the default). Project settings are the project's `settings_json`. From M32 the agent instructions, the
//! Project agent instructions and the workflow skills are the agents' own (AGM-02, d0108); a project keeps its notes.
//! From M33 validation, its rounds and the Delivery's mode are the project's flow's (FLW-01, d0108), not settings.
//!
//! The features that use these values arrive in their own milestones (limits T-0080, notifications T-0117,
//! Run T-0133, workflow skills T-0139, "let finished tickets answer" T-0153).

use std::path::{Component, Path};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use super::{Board, BoardError};
use crate::attention::notify::Kind;
use crate::domain::stored_enum;
use crate::naming;
use crate::store::now_ms;
use crate::tools::Os;

/// The optional agent limit and back-pressure (d0016): off by default; when on, at most 5 agents, and new starts
/// wait while 3 items need the user.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct AgentLimits {
    pub enabled: bool,
    pub max_concurrency: u32,
    pub back_pressure: u32,
}

impl Default for AgentLimits {
    fn default() -> Self {
        Self {
            enabled: false,
            max_concurrency: 5,
            back_pressure: 3,
        }
    }
}

/// A workflow step of M31 whose skill an agent's imported skill can stand for (d0050, d0063; AGM-02). On the wire as
/// its field name (`open_pr`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowStep {
    Commit,
    Push,
    OpenPr,
    Validate,
}

impl WorkflowStep {
    pub const ALL: [WorkflowStep; 4] = [Self::Commit, Self::Push, Self::OpenPr, Self::Validate];

    /// The settings field: `commit`, `push`, `open_pr`, `validate`.
    pub fn field(self) -> &'static str {
        match self {
            Self::Commit => "commit",
            Self::Push => "push",
            Self::OpenPr => "open_pr",
            Self::Validate => "validate",
        }
    }

    /// Otaq's own skill for the step, by its name in the plugin: `commit`, `push`, `open-pr`, `validate`.
    pub fn own_skill(self) -> &'static str {
        match self {
            Self::Commit => "commit",
            Self::Push => "push",
            Self::OpenPr => "open-pr",
            Self::Validate => "validate",
        }
    }
}

stored_enum! {
    /// One of the 15 bundled themes (d0069, d0070), stored as its name: the design's palette key, with Otaq's
    /// two versions told apart. The dark themes first, then the light ones, as the pickers list them.
    pub enum Theme ("theme") {
        OtaqDark = naming::THEME_DARK,
        CatppuccinFrappe = "catppuccin-frappe",
        CatppuccinMacchiato = "catppuccin-macchiato",
        CatppuccinMocha = "catppuccin-mocha",
        TokyoNight = "tokyo-night",
        Dracula = "dracula",
        Nord = "nord",
        GruvboxDark = "gruvbox-dark",
        RosePine = "rose-pine",
        OneDark = "one-dark",
        OtaqLight = naming::THEME_LIGHT,
        CatppuccinLatte = "catppuccin-latte",
        Daylight = "daylight",
        SolarizedLight = "solarized-light",
        RosePineDawn = "rose-pine-dawn",
    }
}

impl Theme {
    /// Whether the theme draws a dark window.
    pub fn is_dark(self) -> bool {
        !matches!(
            self,
            Self::OtaqLight
                | Self::CatppuccinLatte
                | Self::Daylight
                | Self::SolarizedLight
                | Self::RosePineDawn
        )
    }

    /// Light or dark (d0070): what every agent tool is told at start (d0117 point 4).
    pub fn kind(self) -> ThemeKind {
        if self.is_dark() {
            ThemeKind::Dark
        } else {
            ThemeKind::Light
        }
    }

    /// The terminal's text and background (`term-fg`, `term-bg`) as `0xRRGGBB`: what the service answers a
    /// program's colour questions with while no terminal shows the session (d0117 point 4). The design's values,
    /// copied from the app's tokens (`otaq-view`'s theme, which checks they stay the same).
    pub fn terminal_colors(self) -> TerminalColors {
        let (fg, bg) = match self {
            Self::OtaqDark => (0xD6D8DD, 0x0A0B0D),
            Self::CatppuccinFrappe => (0xC6D0F5, 0x303446),
            Self::CatppuccinMacchiato => (0xCAD3F5, 0x24273A),
            Self::CatppuccinMocha => (0xCDD6F4, 0x1E1E2E),
            Self::TokyoNight => (0xC0CAF5, 0x1A1B26),
            Self::Dracula => (0xF8F8F2, 0x282A36),
            Self::Nord => (0xD8DEE9, 0x2E3440),
            Self::GruvboxDark => (0xEBDBB2, 0x282828),
            Self::RosePine => (0xE0DEF4, 0x191724),
            Self::OneDark => (0xABB2BF, 0x282C34),
            Self::OtaqLight => (0x2A2C31, 0xFAFAF8),
            Self::CatppuccinLatte => (0x4C4F69, 0xEFF1F5),
            Self::Daylight => (0x1F2328, 0xF6F8FA),
            Self::SolarizedLight => (0x657B83, 0xFDF6E3),
            Self::RosePineDawn => (0x575279, 0xFAF4ED),
        };
        TerminalColors { fg, bg }
    }

    /// A theme stored by d0069 as a dark or light theme, where both Otaq versions were `otaq`. A name
    /// this version doesn't know, or one of the other kind, reads as Otaq.
    fn from_0069(value: Option<&Value>, dark: bool) -> Self {
        let otaq = if dark {
            Self::OtaqDark
        } else {
            Self::OtaqLight
        };
        match value.and_then(Value::as_str) {
            Some(naming::THEME_KEY) | None => otaq,
            Some(name) => name
                .parse()
                .ok()
                .filter(|theme: &Self| theme.is_dark() == dark)
                .unwrap_or(otaq),
        }
    }
}

/// The theme setting (d0070): one theme, used all the time. Until one is picked (a fresh install, or System mode
/// from before d0070) it holds a theme for a dark OS and one for a light OS; the app picks by the OS at its next
/// start and stores that theme.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ThemeSetting {
    Picked(Theme),
    ByOs { dark: Theme, light: Theme },
}

impl Default for ThemeSetting {
    /// Otaq Dark on a dark OS, Otaq Light on a light one.
    fn default() -> Self {
        Self::ByOs {
            dark: Theme::OtaqDark,
            light: Theme::OtaqLight,
        }
    }
}

/// A theme's kind (d0070).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThemeKind {
    Light,
    Dark,
}

/// A theme's terminal text and background colours, as `0xRRGGBB` ([`Theme::terminal_colors`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TerminalColors {
    pub fg: u32,
    pub bg: u32,
}

impl ThemeSetting {
    /// The theme in use, as the service sees it: the picked one. Before one is picked (a fresh install, until the
    /// app's first start stores the theme for the OS) the service, which can't see the OS, takes the dark one.
    pub fn in_use(self) -> Theme {
        self.resolve(true)
    }

    /// The theme to use, given whether the OS is dark.
    pub fn resolve(self, os_dark: bool) -> Theme {
        match self {
            Self::Picked(theme) => theme,
            Self::ByOs { dark, light } => {
                if os_dark {
                    dark
                } else {
                    light
                }
            }
        }
    }

    /// The setting d0069 stored as a mode (`appearance`) and two themes: Dark and Light mode keep the theme they
    /// showed, System mode keeps both until the OS decides. `None` when nothing of it was stored.
    fn from_0069(stored: &Map<String, Value>) -> Option<Self> {
        if !THEME_KEYS_0069.iter().any(|key| stored.contains_key(*key)) {
            return None;
        }
        let dark = Theme::from_0069(stored.get("dark_theme"), true);
        let light = Theme::from_0069(stored.get("light_theme"), false);
        Some(match stored.get("appearance").and_then(Value::as_str) {
            Some("dark") => Self::Picked(dark),
            Some("light") => Self::Picked(light),
            _ => Self::ByOs { dark, light },
        })
    }
}

/// The settings rows d0069 used for the theme, replaced by `theme` (d0070).
const THEME_KEYS_0069: [&str; 3] = ["appearance", "dark_theme", "light_theme"];

/// Settings for every project (SDD §20, left column).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct GlobalSettings {
    pub agent_limits: AgentLimits,
    /// OS notifications for items that block an agent (d0016). On by default.
    pub notifications: bool,
    /// One checkbox per kind of notification under the main switch (d0090): all on by default.
    pub notification_kinds: NotificationKinds,
    /// Notifications play the OS's default sound (d0090). On by default.
    pub notification_sound: bool,
    /// One theme, used all the time (d0070).
    pub theme: ThemeSetting,
    pub terminal_font_size: u32,
    /// The terminal font (d0117 point 5): an installed monospace family; `None` is Geist Mono. Checked against the
    /// installed fonts when set ([`Self::set_terminal_font_family`]); one no longer installed reads as Geist Mono
    /// ([`Self::terminal_font_family_in`]).
    pub terminal_font_family: Option<String>,
    /// How terminals draw (Q37, Q38): one setting for every terminal.
    pub terminal_renderer: TerminalRenderer,
    /// The description editor shows the raw Markdown source instead of formatted text (d0078 D9): every editor opens
    /// in the mode last chosen with its "Markdown" toggle. Off by default; not a row of the Settings page.
    pub description_source_mode: bool,
    /// Anonymous usage data (d0085): asked once, after first run; nothing is ever sent unless the answer was yes.
    pub usage_data: UsageData,
    /// The single-letter switch and the user's custom shortcuts (d0097, KEY-02, KEY-04): global, not per project.
    pub keyboard: KeyboardSettings,
    /// The Project agent panel's width dragged at its edge (T-0502, d0113), in px; `None` is the default width.
    /// `otaq-view`'s `layout.rs` keeps it inside its bounds. Not a row of the Settings page; Otaq has one window.
    pub agent_panel_width: Option<u32>,
}

/// The terminal font size when none is set.
pub const DEFAULT_TERMINAL_FONT_SIZE: u32 = 13;
/// The terminal font sizes accepted without a warning.
pub const TERMINAL_FONT_SIZES: std::ops::RangeInclusive<u32> = 8..=32;
/// The terminal font when none is chosen (d0117 point 5): the app's own monospace font, shipped with the page.
pub const DEFAULT_TERMINAL_FONT_FAMILY: &str = "Geist Mono";

impl GlobalSettings {
    /// Chooses the terminal font from the installed monospace families: `""` (or Geist Mono) goes back to the
    /// default, a name not in `installed` is refused. Stored with the installed family's own spelling.
    pub fn set_terminal_font_family(
        &mut self,
        family: &str,
        installed: &[String],
    ) -> Result<(), String> {
        let family = family.trim();
        if family.is_empty() || family.eq_ignore_ascii_case(DEFAULT_TERMINAL_FONT_FAMILY) {
            self.terminal_font_family = None;
            return Ok(());
        }
        let found = installed
            .iter()
            .find(|name| name.eq_ignore_ascii_case(family))
            .ok_or_else(|| format!("{family} isn't an installed monospace font"))?;
        self.terminal_font_family = Some(found.clone());
        Ok(())
    }

    /// The chosen terminal font when it is still installed; `None` (Geist Mono) otherwise, quietly: a font removed
    /// since it was chosen is no error.
    pub fn terminal_font_family_in(&self, installed: &[String]) -> Option<&str> {
        let family = self.terminal_font_family.as_deref()?;
        installed
            .iter()
            .any(|name| name.eq_ignore_ascii_case(family))
            .then_some(family)
    }
}

impl Default for GlobalSettings {
    fn default() -> Self {
        Self {
            agent_limits: AgentLimits::default(),
            notifications: true,
            notification_kinds: NotificationKinds::default(),
            notification_sound: true,
            theme: ThemeSetting::default(),
            terminal_font_size: DEFAULT_TERMINAL_FONT_SIZE,
            terminal_font_family: None,
            terminal_renderer: TerminalRenderer::default(),
            description_source_mode: false,
            usage_data: UsageData::NotAsked,
            keyboard: KeyboardSettings::default(),
            agent_panel_width: None,
        }
    }
}

/// Which kinds of OS notification go out (d0090, one Settings checkbox each). An unticked kind still turns turquoise
/// and lands in the inbox; only the OS notification is left out. All on by default, also for settings stored before
/// the checkboxes existed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct NotificationKinds {
    pub permission: bool,
    pub question: bool,
    pub turn_ended: bool,
    pub interrupted: bool,
    pub validation_decision: bool,
    pub auto_merge_stopped: bool,
    pub tool_unavailable: bool,
}

impl Default for NotificationKinds {
    fn default() -> Self {
        Self {
            permission: true,
            question: true,
            turn_ended: true,
            interrupted: true,
            validation_decision: true,
            auto_merge_stopped: true,
            tool_unavailable: true,
        }
    }
}

impl NotificationKinds {
    /// The checkbox of `kind`.
    pub fn allows(&self, kind: Kind) -> bool {
        match kind {
            Kind::Permission => self.permission,
            Kind::Question => self.question,
            Kind::TurnEnded => self.turn_ended,
            Kind::Interrupted => self.interrupted,
            Kind::ValidationDecision => self.validation_decision,
            Kind::AutoMergeStopped => self.auto_merge_stopped,
            Kind::ToolUnavailable => self.tool_unavailable,
        }
    }

    /// Ticks or unticks the checkbox of `kind`.
    pub fn set(&mut self, kind: Kind, on: bool) {
        let slot = match kind {
            Kind::Permission => &mut self.permission,
            Kind::Question => &mut self.question,
            Kind::TurnEnded => &mut self.turn_ended,
            Kind::Interrupted => &mut self.interrupted,
            Kind::ValidationDecision => &mut self.validation_decision,
            Kind::AutoMergeStopped => &mut self.auto_merge_stopped,
            Kind::ToolUnavailable => &mut self.tool_unavailable,
        };
        *slot = on;
    }
}

/// Whether the app may send anonymous usage data (d0085). Until the user answers, it never sends anything.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UsageData {
    /// Not asked yet: the "Help improve Otaq" dialog shows once; nothing is sent meanwhile.
    #[default]
    NotAsked,
    On,
    Off,
}

impl UsageData {
    /// Only an explicit yes sends anything.
    pub fn sends(self) -> bool {
        self == Self::On
    }
}

/// The keyboard settings (d0097): the single-letter switch (KEY-02) and the user's changes to the base map (KEY-04).
/// The core only stores them; the map, and the checks an edit goes through before it is stored, are `otaq-view`'s
/// (`keymap_edit`), which also drops a change naming an action or key it doesn't know.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct KeyboardSettings {
    /// Single letters and `g` sequences, built-in and custom, run (KEY-02). On by default.
    pub letters: bool,
    /// One per changed key, in the order they were made. A change that can't be read is left out, keeping the others.
    #[serde(deserialize_with = "readable_changes")]
    pub changes: Vec<KeyboardChange>,
}

impl Default for KeyboardSettings {
    fn default() -> Self {
        Self {
            letters: true,
            changes: Vec::new(),
        }
    }
}

/// One changed key of an action (d0097: each key is changed on its own).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeyboardChange {
    /// The action's name as the map writes it (`new_ticket`, `project`).
    pub action: String,
    /// Project 1–9's and Switch panel tab 1–6's number.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub number: Option<u8>,
    /// Which of the action's keys: its index in the base map, or past them for a key the user added.
    pub slot: u8,
    /// The new key as the map writes it ("⌘J", "Ctrl+R", "n", "g p"); `None`: the key was removed.
    pub key: Option<String>,
    /// "Works inside terminals" (d0097).
    #[serde(default)]
    pub works_in_terminals: bool,
}

/// The stored changes, skipping any entry that isn't a change.
fn readable_changes<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Vec<KeyboardChange>, D::Error> {
    let values = Vec::<Value>::deserialize(deserializer)?;
    Ok(values
        .into_iter()
        .filter_map(|v| serde_json::from_value(v).ok())
        .collect())
}

/// The terminal renderer: xterm.js's two (UIT-05). WebGL draws on the GPU and keeps the CPU low; DOM draws with the
/// page's elements. A broken WebGL driver draws garbage without any error (the M14 test build, in a Linux VM), so
/// nothing falls back by itself: this setting picks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TerminalRenderer {
    Webgl,
    Dom,
}

impl Default for TerminalRenderer {
    /// DOM on Linux, WebGL on macOS and Windows (Q37, deferred as option 1). The service and the app run on the same
    /// machine, so the service's platform is the app's.
    fn default() -> Self {
        if cfg!(target_os = "linux") {
            Self::Dom
        } else {
            Self::Webgl
        }
    }
}

/// Settings of one project (SDD §20, right column). Name and base branch are project fields (`update_project`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ProjectSettings {
    pub setup_command: String,
    /// "Merge check" (d0120): the command Local merge runs on the merged result in its scratch worktree before the
    /// target branch moves, when the rebase brought in new commits (`npm test`). Empty by default; never guessed
    /// (d0023). Read it with [`Self::merge_check`].
    pub merge_check: Option<String>,
    pub run_command: String,
    pub open_url: String,
    /// Files copied from the repository into each worktree, relative to the repository root.
    pub env_files: Vec<String>,
    /// "Notes every agent in this project gets" (d0040's per-project Agent instructions, kept by AGM-02): given to
    /// every agent that works in the project, after its own instructions (AGM-06). Empty by default.
    pub project_notes: String,
    /// Let finished tickets answer other agents' questions (d0030). On by default.
    pub let_finished_tickets_answer: bool,
    /// Set in everything Otaq starts in the project, in row order (ENV-01, d0082). Empty by default.
    pub env_vars: Vec<EnvVar>,
    /// "Start new worktrees from a warm build" (WRM-05, d0082). On by default; warm builds are on only where the
    /// data folder's disk can clone too.
    pub warm_builds: bool,
    /// Ticket resources (M19-d, d0087): the command that creates a ticket's own resource (a database, say) before
    /// setup, in the ticket's worktree. Empty: none, and Otaq sets no resource variables.
    pub resource_create: String,
    /// Removes what Create made, at Done and Close; runs only for a resource Otaq recorded.
    pub resource_remove: String,
    /// Refreshes the project's template (in the main checkout) after Done; Creates wait meanwhile.
    pub resource_template_refresh: String,
    /// Globs of the files that change the schema (`db/migrations/**`), relative to the repository root (M19-e).
    pub schema_paths: Vec<String>,
    /// How the board looks in this project (SHB-05, d0113): written by the board as the user changes it, never by
    /// the Settings page ([`Board::set_project_view`]; saving the settings keeps the stored one).
    pub view: ViewPrefs,
}

impl Default for ProjectSettings {
    fn default() -> Self {
        Self {
            setup_command: String::new(),
            merge_check: None,
            run_command: String::new(),
            open_url: String::new(),
            env_files: Vec::new(),
            project_notes: String::new(),
            let_finished_tickets_answer: true,
            env_vars: Vec::new(),
            warm_builds: true,
            resource_create: String::new(),
            resource_remove: String::new(),
            resource_template_refresh: String::new(),
            schema_paths: Vec::new(),
            view: ViewPrefs::default(),
        }
    }
}

impl ProjectSettings {
    /// The Merge check command, trimmed; `None` when it is empty (d0120).
    pub fn merge_check(&self) -> Option<&str> {
        self.merge_check
            .as_deref()
            .map(str::trim)
            .filter(|c| !c.is_empty())
    }
}

/// A project's view preferences (SHB-05, BRD-01…BRD-04, LST-01, EPL-01, DPV-02): each project opens as it was
/// left, restarts included. Only a view: nothing in the flow changes. Unknown or unreadable values fall back to
/// their defaults one by one (`read_or_default`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ViewPrefs {
    /// Board, List or Files (BRD-01, FIL-01).
    pub view: BoardView,
    /// The Board's lanes (EPL-01).
    pub lanes: Lanes,
    /// The List's grouping (LST-01): Stage by default.
    pub group_by: GroupBy,
    /// "Fold empty columns" in the Board's toolbar (BRD-02).
    pub fold_empty_columns: bool,
    /// "Hide empty stages" in the List's toolbar (LST-01).
    pub hide_empty_stages: bool,
    /// Dependencies on the board: On hover or Always (DPV-02).
    pub dependencies: Dependencies,
    /// The Board's columns folded to their rail by their ⋯ (BRD-02), by column id (`"done"`, a stage's id). Done is
    /// folded at first: its "Done 4 →" line under the last column.
    pub folded_columns: Vec<String>,
    /// The epic lanes folded by their head (EPL-01), by epic key.
    pub folded_lanes: Vec<String>,
    /// A human check column's sections folded by their label (Review's Approved, d0087), by section id.
    pub folded_sections: Vec<String>,
    /// An agent column shows every card instead of its "4 more · working" fold (T-0027).
    pub dev_unfolded: bool,
}

/// The column id of Done ([`ViewPrefs::folded_columns`]).
pub const DONE_COLUMN: &str = "done";

impl Default for ViewPrefs {
    fn default() -> Self {
        Self {
            view: BoardView::Board,
            lanes: Lanes::None,
            group_by: GroupBy::Stage,
            fold_empty_columns: false,
            hide_empty_stages: false,
            dependencies: Dependencies::OnHover,
            folded_columns: vec![DONE_COLUMN.to_owned()],
            folded_lanes: Vec::new(),
            folded_sections: Vec::new(),
            dev_unfolded: false,
        }
    }
}

/// The project's three views (BRD-01, FIL-01): Board, List and the project Files view.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BoardView {
    #[default]
    Board,
    List,
    Files,
}

/// The Board's lanes (EPL-01): none, or one per epic.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Lanes {
    #[default]
    None,
    Epic,
}

/// The List's grouping (LST-01).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GroupBy {
    #[default]
    Stage,
    Status,
    Epic,
    Agent,
}

/// When the board draws dependencies (DPV-02).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Dependencies {
    #[default]
    OnHover,
    Always,
}

/// One project environment variable row, `NAME = value` (ENV-01). Stored and shown as written, in plain text.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct EnvVar {
    pub name: String,
    pub value: String,
}

impl EnvVar {
    pub fn new(name: &str, value: &str) -> Self {
        Self {
            name: name.to_owned(),
            value: value.to_owned(),
        }
    }
}

/// The problem field of the environment variable row at `index`: `env_vars.2`.
pub fn env_var_field(index: usize) -> String {
    format!("env_vars.{index}")
}

/// The problem of a row with an empty name (*15 · Settings → Row · Environment variables · edit, remove, errors*).
pub const ENV_VAR_EMPTY_NAME: &str = "Add a name, or remove the row";

/// The problem of a name containing `=`, ending with the fix (d0093): "Remove “=” from the name. This row isn’t set
/// until then." The name itself is in the row's field.
pub fn env_var_equals_message(_name: &str) -> String {
    "Remove \u{201c}=\u{201d} from the name. This row isn\u{2019}t set until then.".to_owned()
}

/// The problem of a name containing a null character.
pub fn env_var_null_message(_name: &str) -> String {
    "Remove the null character from the name. This row isn\u{2019}t set until then.".to_owned()
}

/// The problem of a row repeating an earlier row's name: "NODE_OPTIONS is already set above. Only the first row
/// applies; remove one of them."
pub fn env_var_duplicate_message(name: &str) -> String {
    format!("{name} is already set above. Only the first row applies; remove one of them.")
}

/// Whether two variable names are the same on `os`: Windows compares them ignoring case (ENV-03).
pub fn same_env_name(a: &str, b: &str, os: Os) -> bool {
    if os == Os::Windows {
        a.to_lowercase() == b.to_lowercase()
    } else {
        a == b
    }
}

/// Each row's problem, if it has one, by row index (ENV-03): an empty name, a name containing `=` or a null
/// character, or a name an earlier row already has (the first applies; on Windows ignoring case). Nothing else is
/// a problem: spaces, a leading digit and any value are allowed.
pub fn env_var_problems(env_vars: &[EnvVar], os: Os) -> Vec<Option<String>> {
    env_vars
        .iter()
        .enumerate()
        .map(|(index, row)| {
            let name = row.name.as_str();
            if name.is_empty() {
                Some(ENV_VAR_EMPTY_NAME.to_owned())
            } else if name.contains('=') {
                Some(env_var_equals_message(name))
            } else if name.contains('\0') {
                Some(env_var_null_message(name))
            } else if env_vars[..index]
                .iter()
                .any(|earlier| same_env_name(&earlier.name, name, os))
            {
                Some(env_var_duplicate_message(name))
            } else {
                None
            }
        })
        .collect()
}

/// The key of [`ProjectSettings::project_notes`] in a project's stored settings.
pub const PROJECT_NOTES: &str = "project_notes";

/// The values a project's agents actually use.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EffectiveSettings {
    /// The project's notes for every agent (AGM-02, AGM-06).
    pub project_notes: String,
}

/// A stored value that is flagged as invalid. `field` names the setting (`env_files`, `open_url`, …).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SettingProblem {
    pub field: String,
    pub message: String,
}

impl Board {
    pub async fn global_settings(&self) -> Result<GlobalSettings, BoardError> {
        let rows: Vec<(String, String)> = sqlx::query_as("SELECT key, value_json FROM settings")
            .fetch_all(self.store.pool())
            .await?;
        let mut object = Map::new();
        for (key, value) in rows {
            if let Ok(value) = serde_json::from_str::<Value>(&value) {
                object.insert(key, value);
            }
        }
        if !object.contains_key("theme")
            && let Some(theme) = ThemeSetting::from_0069(&object)
        {
            object.insert(
                "theme".to_owned(),
                serde_json::to_value(theme).expect("theme"),
            );
        }
        Ok(read_or_default(Value::Object(object)))
    }

    /// The theme in use now ([`ThemeSetting::in_use`]), read when asked: what a session or launch starting now is
    /// told (d0117 point 4). Settings that can't be read count as the default.
    pub async fn theme_in_use(&self) -> Theme {
        self.global_settings()
            .await
            .unwrap_or_else(|e| {
                tracing::warn!(error = %e, "can't read the theme");
                GlobalSettings::default()
            })
            .theme
            .in_use()
    }

    /// Stores every global setting.
    pub async fn set_global_settings(&self, settings: &GlobalSettings) -> Result<(), BoardError> {
        let Value::Object(fields) = serde_json::to_value(settings).expect("settings serialize")
        else {
            unreachable!("settings are an object")
        };
        let mut tx = self.store.begin_write().await?;
        for (key, value) in fields {
            sqlx::query(
                "INSERT INTO settings (key, value_json, updated_at) VALUES (?, ?, ?)
                 ON CONFLICT (key) DO UPDATE SET value_json = excluded.value_json, updated_at = excluded.updated_at",
            )
            .bind(key)
            .bind(value.to_string())
            .bind(now_ms())
            .execute(&mut *tx)
            .await?;
        }
        for key in THEME_KEYS_0069 {
            sqlx::query("DELETE FROM settings WHERE key = ?")
                .bind(key)
                .execute(&mut *tx)
                .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    pub async fn project_settings(&self, project: &str) -> Result<ProjectSettings, BoardError> {
        let project = self.project(project).await?;
        let value: Value = serde_json::from_str(&project.settings_json).unwrap_or_default();
        let view = read_or_default(value.get("view").cloned().unwrap_or_default());
        Ok(ProjectSettings {
            view,
            ..read_or_default(value)
        })
    }

    pub async fn set_project_settings(
        &self,
        project: &str,
        settings: &ProjectSettings,
    ) -> Result<(), BoardError> {
        let project = self.project(project).await?;
        let json = serde_json::to_value(settings)
            .expect("settings serialize")
            .to_string();
        sqlx::query("UPDATE projects SET settings_json = ? WHERE id = ?")
            .bind(json)
            .bind(&project.id)
            .execute(self.store.pool())
            .await?;
        Ok(())
    }

    /// The project variables that apply (ENV-02), of the project with this id, read now (never cached): a
    /// change reaches what starts after it. An unknown project has none.
    pub async fn project_env(&self, project_id: &str) -> Result<Vec<(String, String)>, BoardError> {
        let json: Option<String> =
            sqlx::query_scalar("SELECT settings_json FROM projects WHERE id = ?")
                .bind(project_id)
                .fetch_optional(self.store.pool())
                .await?;
        let own: ProjectSettings =
            read_or_default(serde_json::from_str(&json.unwrap_or_default()).unwrap_or_default());
        Ok(crate::session::env::applied(&own.env_vars, Os::current()))
    }

    /// The user's `git` with the project's variables set in every command it runs (ENV-02), for the git
    /// commands Otaq runs in the project's repository and worktrees, so the user's hooks see them. Otaq sets
    /// none of its own variables there, so a `${…}` naming one becomes empty ([`crate::session::env::substitute`]).
    pub async fn project_git(&self, project: &str) -> Result<crate::git::Git, BoardError> {
        let git = self.git()?;
        let settings = self.project_settings(project).await?;
        let os = Os::current();
        Ok(git.with_env(crate::session::env::substitute(
            &crate::session::env::applied(&settings.env_vars, os),
            &[],
            os,
        )))
    }

    /// The values a project's agents, validators and Project agents use.
    pub async fn effective_settings(&self, project: &str) -> Result<EffectiveSettings, BoardError> {
        let own = self.project_settings(project).await?;
        Ok(effective(&own))
    }

    /// `settings.set`: stores the global settings.
    pub async fn save_global_settings(&self, settings: &GlobalSettings) -> Result<(), BoardError> {
        self.set_global_settings(settings).await
    }

    /// `project_settings.set`: stores a project's settings.
    /// Saves the Settings page's project settings. The view preferences are the board's: the stored ones stay, so a
    /// page opened before a board change never puts the old view back (SHB-05).
    pub async fn save_project_settings(
        &self,
        project: &str,
        settings: &ProjectSettings,
    ) -> Result<(), BoardError> {
        let view = self.project_settings(project).await?.view;
        let settings = ProjectSettings {
            view,
            ..settings.clone()
        };
        self.set_project_settings(project, &settings).await
    }

    /// Stores a project's view preferences (SHB-05), the rest of its settings unchanged.
    pub async fn set_project_view(
        &self,
        project: &str,
        view: &ViewPrefs,
    ) -> Result<(), BoardError> {
        let mut settings = self.project_settings(project).await?;
        settings.view = view.clone();
        self.set_project_settings(project, &settings).await
    }

    /// Problems with the global settings' values.
    pub async fn global_setting_problems(&self) -> Result<Vec<SettingProblem>, BoardError> {
        let settings = self.global_settings().await?;
        let mut problems = Vec::new();
        if settings.agent_limits.max_concurrency == 0 {
            problems.push(problem(
                "agent_limits.max_concurrency",
                "Allow at least 1 agent",
            ));
        }
        if settings.agent_limits.back_pressure == 0 {
            problems.push(problem(
                "agent_limits.back_pressure",
                "Back-pressure needs at least 1 item",
            ));
        }
        if !TERMINAL_FONT_SIZES.contains(&settings.terminal_font_size) {
            problems.push(problem(
                "terminal_font_size",
                &format!(
                    "Use a size from {} to {}",
                    TERMINAL_FONT_SIZES.start(),
                    TERMINAL_FONT_SIZES.end()
                ),
            ));
        }
        Ok(problems)
    }

    /// Problems with a project's settings: env files missing from the repository, an Open URL that isn't a URL.
    /// Reads the repository, never writes to it.
    pub async fn project_setting_problems(
        &self,
        project: &str,
    ) -> Result<Vec<SettingProblem>, BoardError> {
        let settings = self.project_settings(project).await?;
        let repo = self.project(project).await?.repo_path;
        let mut problems = Vec::new();
        for file in &settings.env_files {
            if let Some(message) = env_file_problem(Path::new(&repo), file) {
                problems.push(problem("env_files", &message));
            }
        }
        for (index, message) in env_var_problems(&settings.env_vars, Os::current())
            .into_iter()
            .enumerate()
        {
            if let Some(message) = message {
                problems.push(problem(&env_var_field(index), &message));
            }
        }
        problems.extend(resource_problems(&settings));
        if !settings.open_url.trim().is_empty() && !looks_like_url(settings.open_url.trim()) {
            problems.push(problem(
                "open_url",
                &format!(
                    "{} isn't a URL; use something like http://localhost:$PORT",
                    settings.open_url.trim()
                ),
            ));
        }
        Ok(problems)
    }
}

/// What a project's own settings give its agents ([`Board::effective_settings`] reads them).
pub fn effective(own: &ProjectSettings) -> EffectiveSettings {
    EffectiveSettings {
        project_notes: own.project_notes.clone(),
    }
}

/// Reads settings from stored JSON; anything missing or unreadable gets its default.
fn read_or_default<T: Default + serde::de::DeserializeOwned + Serialize>(stored: Value) -> T {
    // Merge field by field so one bad value doesn't reset the others.
    let mut merged = serde_json::to_value(T::default()).expect("settings serialize");
    if let (Value::Object(merged), Value::Object(stored)) = (&mut merged, stored) {
        for (key, value) in stored {
            if let Some(slot) = merged.get_mut(&key) {
                let previous = std::mem::replace(slot, value);
                let candidate = Value::Object(merged.clone());
                if serde_json::from_value::<T>(candidate).is_err() {
                    merged.insert(key, previous);
                }
            }
        }
    }
    serde_json::from_value(merged).unwrap_or_default()
}

fn problem(field: &str, message: &str) -> SettingProblem {
    SettingProblem {
        field: field.to_owned(),
        message: message.to_owned(),
    }
}

/// Why an env file can't be copied, if it can't: it must be a relative path to a file inside the repository.
fn env_file_problem(repo: &Path, file: &str) -> Option<String> {
    let path = Path::new(file.trim());
    let escapes =
        path.is_absolute() || path.components().any(|c| matches!(c, Component::ParentDir));
    if file.trim().is_empty() {
        Some("An env file entry is empty".to_owned())
    } else if escapes {
        Some(format!(
            "{file} must be a path inside the repository, like .env.local"
        ))
    } else if !repo.join(path).is_file() {
        Some(missing_env_file(file.trim(), repo))
    } else {
        None
    }
}

/// An env file that isn't in the repository, with the fix (d0093: one wording on every surface): "Can’t find
/// .env.local in ~/code/billing-api. Add it to the repo root, or remove it from this list."
pub fn missing_env_file(file: &str, repo: &Path) -> String {
    format!(
        "Can\u{2019}t find {file} in {}. Add it to the repo root, or remove it from this list.",
        crate::start::setup::home_relative(repo)
    )
}

/// The message on a Remove or Template refresh command set without a Create command.
pub fn resource_without_create() -> String {
    format!(
        "Remove runs only for resources {} created",
        naming::PRODUCT_NAME
    )
}

/// Problems with a project's ticket resource settings (d0087): a schema path that isn't a relative glob, and a
/// Remove or Template refresh command with no Create command (a warning: nothing is refused).
pub fn resource_problems(settings: &ProjectSettings) -> Vec<SettingProblem> {
    let mut problems: Vec<SettingProblem> = settings
        .schema_paths
        .iter()
        .filter_map(|glob| crate::glob::problem(glob))
        .map(|message| problem("schema_paths", &message))
        .collect();
    if settings.resource_create.trim().is_empty() {
        for (field, value) in [
            ("resource_remove", &settings.resource_remove),
            (
                "resource_template_refresh",
                &settings.resource_template_refresh,
            ),
        ] {
            if !value.trim().is_empty() {
                problems.push(problem(field, &resource_without_create()));
            }
        }
    }
    problems
}

/// A light check: `http://` or `https://` followed by a host, and no spaces. `$PORT` is allowed (d0035).
fn looks_like_url(url: &str) -> bool {
    let rest = url
        .strip_prefix("http://")
        .or_else(|| url.strip_prefix("https://"));
    rest.is_some_and(|rest| {
        let host = rest.split(['/', '?', '#']).next().unwrap_or("");
        !host.is_empty() && !url.contains(char::is_whitespace)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn urls() {
        for good in [
            "http://localhost:3000",
            "https://example.com/path?q=1",
            "http://localhost:$PORT/",
        ] {
            assert!(looks_like_url(good), "{good}");
        }
        for bad in [
            "localhost:3000",
            "http://",
            "ftp://x",
            "http://a b",
            "https:///x",
        ] {
            assert!(!looks_like_url(bad), "{bad}");
        }
    }

    #[test]
    fn theme_names_are_the_designs_palette_keys() {
        let names: Vec<&str> = Theme::ALL.iter().map(|t| t.as_str()).collect();
        assert_eq!(
            names,
            [
                naming::THEME_DARK,
                "catppuccin-frappe",
                "catppuccin-macchiato",
                "catppuccin-mocha",
                "tokyo-night",
                "dracula",
                "nord",
                "gruvbox-dark",
                "rose-pine",
                "one-dark",
                naming::THEME_LIGHT,
                "catppuccin-latte",
                "daylight",
                "solarized-light",
                "rose-pine-dawn",
            ]
        );
        let dark = Theme::ALL.iter().filter(|t| t.is_dark()).count();
        assert_eq!(dark, 10);
        assert!(Theme::ALL[..dark].iter().all(|t| t.is_dark()));
        for theme in Theme::ALL {
            let json = serde_json::to_value(ThemeSetting::Picked(*theme)).unwrap();
            assert_eq!(json, serde_json::json!(theme.as_str()));
            assert_eq!(
                serde_json::from_value::<ThemeSetting>(json).unwrap(),
                ThemeSetting::Picked(*theme)
            );
        }
        let defaults = GlobalSettings::default();
        assert_eq!(defaults.theme.resolve(true), Theme::OtaqDark);
        assert_eq!(defaults.theme.resolve(false), Theme::OtaqLight);
        let by_os = serde_json::to_value(defaults.theme).unwrap();
        assert_eq!(
            by_os,
            serde_json::json!({"dark": naming::THEME_DARK, "light": naming::THEME_LIGHT})
        );
        assert_eq!(
            serde_json::from_value::<ThemeSetting>(by_os).unwrap(),
            defaults.theme
        );
    }

    #[test]
    fn the_merge_check_is_empty_by_default_and_read_trimmed() {
        assert_eq!(ProjectSettings::default().merge_check(), None);
        let set: ProjectSettings =
            read_or_default(serde_json::json!({"merge_check": "  npm test \n"}));
        assert_eq!(set.merge_check(), Some("npm test"));
        let blank: ProjectSettings = read_or_default(serde_json::json!({"merge_check": "   "}));
        assert_eq!(blank.merge_check(), None);
    }

    #[test]
    fn warm_builds_are_on_unless_turned_off() {
        assert!(ProjectSettings::default().warm_builds);
        // A project saved before the switch existed reads as on.
        let old: ProjectSettings = read_or_default(serde_json::json!({"setup_command": "make"}));
        assert!(old.warm_builds);
        assert_eq!(old.setup_command, "make");
        let off: ProjectSettings = read_or_default(serde_json::json!({"warm_builds": false}));
        assert!(!off.warm_builds);
    }

    #[test]
    fn resource_settings_default_to_empty_in_old_settings() {
        let old: ProjectSettings = read_or_default(serde_json::json!({"setup_command": "make"}));
        assert_eq!(old.resource_create, "");
        assert_eq!(old.resource_remove, "");
        assert_eq!(old.resource_template_refresh, "");
        assert!(old.schema_paths.is_empty());
        assert!(resource_problems(&old).is_empty());
        let set: ProjectSettings = read_or_default(serde_json::json!({
            "resource_create": "createdb -T shop_template shop_1",
            "schema_paths": ["db/migrations/**"],
            "warm_builds": "not a flag",
        }));
        assert_eq!(set.schema_paths, ["db/migrations/**"]);
        assert!(set.resource_create.starts_with("createdb"));
        assert!(set.warm_builds, "one bad value keeps the others");
    }

    #[test]
    fn a_picked_theme_is_used_whatever_the_os() {
        let picked = ThemeSetting::Picked(Theme::Dracula);
        assert_eq!(picked.resolve(true), Theme::Dracula);
        assert_eq!(picked.resolve(false), Theme::Dracula);
    }

    #[test]
    fn an_unknown_theme_reads_as_unpicked_and_keeps_the_others() {
        let stored = serde_json::json!({"theme": "monokai", "terminal_font_size": 15});
        let settings: GlobalSettings = read_or_default(stored);
        assert_eq!(settings.theme, ThemeSetting::default());
        assert_eq!(settings.terminal_font_size, 15);
    }

    #[test]
    fn a_0069_mode_keeps_the_theme_it_showed() {
        let read = |stored: Value| {
            let Value::Object(stored) = stored else {
                unreachable!()
            };
            ThemeSetting::from_0069(&stored)
        };
        let both = |appearance: &str| {
            read(serde_json::json!({
                "appearance": appearance,
                "dark_theme": "tokyo-night",
                "light_theme": "daylight",
            }))
        };
        assert_eq!(both("dark"), Some(ThemeSetting::Picked(Theme::TokyoNight)));
        assert_eq!(both("light"), Some(ThemeSetting::Picked(Theme::Daylight)));
        assert_eq!(
            both("system"),
            Some(ThemeSetting::ByOs {
                dark: Theme::TokyoNight,
                light: Theme::Daylight
            })
        );
        // Both Otaq versions were stored as `otaq`; unknown names read as Otaq.
        assert_eq!(
            read(serde_json::json!({"appearance": "light", "light_theme": naming::THEME_KEY})),
            Some(ThemeSetting::Picked(Theme::OtaqLight))
        );
        assert_eq!(
            read(serde_json::json!({"appearance": "dark", "dark_theme": "monokai"})),
            Some(ThemeSetting::Picked(Theme::OtaqDark))
        );
        assert_eq!(
            read(serde_json::json!({"dark_theme": "nord"})),
            Some(ThemeSetting::ByOs {
                dark: Theme::Nord,
                light: Theme::OtaqLight
            })
        );
        assert_eq!(read(serde_json::json!({"notifications": false})), None);
    }

    #[test]
    fn the_terminal_renderer_defaults_to_dom_on_linux_only() {
        let default = GlobalSettings::default().terminal_renderer;
        if cfg!(target_os = "linux") {
            assert_eq!(default, TerminalRenderer::Dom);
        } else {
            assert_eq!(default, TerminalRenderer::Webgl);
        }
        let stored = serde_json::json!({"terminal_renderer": "dom"});
        let settings: GlobalSettings = read_or_default(stored);
        assert_eq!(settings.terminal_renderer, TerminalRenderer::Dom);
        let stored = serde_json::json!({"terminal_renderer": "vulkan", "terminal_font_size": 15});
        let settings: GlobalSettings = read_or_default(stored);
        assert_eq!(settings.terminal_renderer, default);
        assert_eq!(settings.terminal_font_size, 15);
    }

    #[test]
    fn the_terminal_font_is_geist_mono_until_an_installed_one_is_chosen() {
        let installed = ["MesloLGS NF".to_owned(), "JetBrains Mono".to_owned()];
        let mut settings = GlobalSettings::default();
        assert_eq!(settings.terminal_font_family, None);
        assert_eq!(settings.terminal_font_family_in(&installed), None);

        // An installed family is kept with its own spelling and round-trips through the stored file.
        settings
            .set_terminal_font_family("meslolgs nf", &installed)
            .unwrap();
        assert_eq!(
            settings.terminal_font_family.as_deref(),
            Some("MesloLGS NF")
        );
        let stored = serde_json::to_value(&settings).unwrap();
        assert_eq!(stored["terminal_font_family"], "MesloLGS NF");
        let read: GlobalSettings = read_or_default(stored);
        assert_eq!(read, settings);
        assert_eq!(
            read.terminal_font_family_in(&installed),
            Some("MesloLGS NF")
        );

        // An unknown name is refused and changes nothing.
        let refused = settings.set_terminal_font_family("Comic Mono", &installed);
        assert_eq!(
            refused,
            Err("Comic Mono isn't an installed monospace font".into())
        );
        assert_eq!(
            settings.terminal_font_family.as_deref(),
            Some("MesloLGS NF")
        );

        // Uninstalled since: the stored name stays, it reads as Geist Mono.
        let now = ["JetBrains Mono".to_owned()];
        assert_eq!(settings.terminal_font_family_in(&now), None);
        assert_eq!(
            settings.terminal_font_family.as_deref(),
            Some("MesloLGS NF")
        );

        // Empty, or Geist Mono by name, goes back to the default.
        for default in ["", "Geist Mono"] {
            settings
                .set_terminal_font_family("JetBrains Mono", &now)
                .unwrap();
            settings.set_terminal_font_family(default, &now).unwrap();
            assert_eq!(settings.terminal_font_family, None);
        }
    }

    #[test]
    fn the_description_source_mode_is_off_until_chosen() {
        assert!(!GlobalSettings::default().description_source_mode);
        // Settings stored before the field existed load with it off and keep their own values.
        let settings: GlobalSettings =
            read_or_default(serde_json::json!({"terminal_font_size": 15}));
        assert!(!settings.description_source_mode);
        assert_eq!(settings.terminal_font_size, 15);
        let old: GlobalSettings =
            serde_json::from_value(serde_json::json!({"notifications": false})).unwrap();
        assert!(!old.description_source_mode);
        let settings: GlobalSettings =
            read_or_default(serde_json::json!({"description_source_mode": true}));
        assert!(settings.description_source_mode);
    }

    /// KEY-02, KEY-04: letters on and no changes by default, also for settings stored before the field existed; a
    /// stored change that can't be read is left out, keeping the others and the switch.
    #[test]
    fn the_keyboard_has_letters_on_and_no_changes_until_changed() {
        let fresh = GlobalSettings::default().keyboard;
        assert!(fresh.letters);
        assert!(fresh.changes.is_empty());
        let old: GlobalSettings = read_or_default(serde_json::json!({"terminal_font_size": 15}));
        assert_eq!(old.keyboard, KeyboardSettings::default());
        assert_eq!(old.terminal_font_size, 15);

        let stored: GlobalSettings = read_or_default(serde_json::json!({"keyboard": {
            "letters": false,
            "changes": [
                {"action": "open_activity", "slot": 0, "key": "⌘J"},
                {"action": "new_ticket", "slot": "one", "key": "n"},
                "nonsense",
                {"action": "next_inbox_item", "slot": 0, "key": null, "works_in_terminals": false},
                {"action": "project", "number": 3, "slot": 0, "key": "⌃3", "works_in_terminals": true},
            ],
        }}));
        let keyboard = stored.keyboard;
        assert!(!keyboard.letters);
        assert_eq!(
            keyboard.changes,
            [
                KeyboardChange {
                    action: "open_activity".into(),
                    number: None,
                    slot: 0,
                    key: Some("⌘J".into()),
                    works_in_terminals: false,
                },
                KeyboardChange {
                    action: "next_inbox_item".into(),
                    number: None,
                    slot: 0,
                    key: None,
                    works_in_terminals: false,
                },
                KeyboardChange {
                    action: "project".into(),
                    number: Some(3),
                    slot: 0,
                    key: Some("⌃3".into()),
                    works_in_terminals: true,
                },
            ]
        );
        // A keyboard value that isn't one keeps the defaults; the round trip keeps everything.
        let broken: GlobalSettings = read_or_default(serde_json::json!({"keyboard": 7}));
        assert_eq!(broken.keyboard, KeyboardSettings::default());
        let back: KeyboardSettings =
            serde_json::from_value(serde_json::to_value(&keyboard).unwrap()).unwrap();
        assert_eq!(back, keyboard);
    }

    #[test]
    fn usage_data_is_not_asked_and_never_sends_until_the_user_says_yes() {
        let fresh = GlobalSettings::default();
        assert_eq!(fresh.usage_data, UsageData::NotAsked);
        assert!(!fresh.usage_data.sends());
        // Settings stored before the field existed load as not asked.
        let old: GlobalSettings = read_or_default(serde_json::json!({"terminal_font_size": 15}));
        assert_eq!(old.usage_data, UsageData::NotAsked);
        let on: GlobalSettings = read_or_default(serde_json::json!({"usage_data": "on"}));
        assert!(on.usage_data.sends());
        let off: GlobalSettings = read_or_default(serde_json::json!({"usage_data": "off"}));
        assert!(!off.usage_data.sends());
        // A value this version doesn't know never turns sending on.
        let odd: GlobalSettings = read_or_default(serde_json::json!({"usage_data": "maybe"}));
        assert!(!odd.usage_data.sends());
    }

    /// T-0371 (d0090): settings stored before the per-kind checkboxes and the sound read with every kind and the
    /// sound on; one kind turned off is kept, the others stay on.
    #[test]
    fn old_settings_read_with_every_kind_and_the_sound_on() {
        let old: GlobalSettings =
            read_or_default(serde_json::json!({"notifications": false, "terminal_font_size": 15}));
        assert!(!old.notifications);
        assert_eq!(old.notification_kinds, NotificationKinds::default());
        assert!(Kind::ALL.iter().all(|k| old.notification_kinds.allows(*k)));
        assert!(old.notification_sound);
        let fresh = GlobalSettings::default();
        assert!(fresh.notifications && fresh.notification_sound);
        assert!(
            Kind::ALL
                .iter()
                .all(|k| fresh.notification_kinds.allows(*k))
        );

        let one_off: GlobalSettings = read_or_default(serde_json::json!({
            "notification_kinds": {"turn_ended": false},
            "notification_sound": false,
        }));
        assert!(!one_off.notification_kinds.allows(Kind::TurnEnded));
        assert!(one_off.notification_kinds.allows(Kind::Permission));
        assert!(!one_off.notification_sound);
        let mut kinds = NotificationKinds::default();
        kinds.set(Kind::ToolUnavailable, false);
        assert!(!kinds.allows(Kind::ToolUnavailable));
    }

    #[test]
    fn a_bad_stored_value_keeps_the_others() {
        let stored = serde_json::json!({"notifications": "loud", "terminal_font_size": 15});
        let settings: GlobalSettings = read_or_default(stored);
        assert!(settings.notifications);
        assert_eq!(settings.terminal_font_size, 15);
    }
}
