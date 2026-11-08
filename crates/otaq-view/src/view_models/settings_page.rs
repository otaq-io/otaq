//! Settings as the Tauri page draws it (T-0234; SET-01, PRJ-03, PRJ-04, SCH-03, USG-02, ATT-11, REV-09, REV-11, MSG-05,
//! UI-11; *15 · Settings*): the nav and the app's page as sections of rows, every row with its label, description and
//! control already worded, and the rules for applying an edit ([`edit_global`], [`edit_project`]). A project's rows
//! are built here too, for the project's own Settings page ([`super::project_settings`]). The page only lays the rows
//! out and sends back `field` + value; the service still flags invalid values (a missing env file, a URL that isn't
//! one), shown in red under their field.
//!
//! Section ids are the page registry's (`ui/src/app/registry/settings.ts`): the Settings screen draws the sections
//! it has registered, so a later ticket adds a section here and registers its id there. M33 removed Validation &
//! Auto-merge (FLW-01): validation, its rounds and the Delivery's mode are the project's flow's, which a project's Flow
//! section shows (FLW-07; [`super::flow_settings`], drawn from its own request).

use otaq_core::attention::notify::{Kind, PermissionState};
use otaq_core::board::projects::ProjectUpdate;
use otaq_core::board::settings::{
    DEFAULT_APP, EnvVar, GlobalSettings, ProjectSettings, RunApp, SettingProblem,
    TERMINAL_FONT_SIZES, TerminalRenderer, UsageData, env_var_field, env_var_problems,
};
use otaq_core::board::snapshot::ProjectInfo;
use otaq_core::naming::{self, PRODUCT_NAME};
use otaq_core::scheduler::SchedulerStatus;
use otaq_core::warm::clone::CloneSupport;
use otaq_core::warm::store::{WarmBuildInfo, WarmList};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::terminal_font::{self, InstalledFonts};
use crate::icons::IconName;
use crate::keymap::{Platform, TerminalKey};

/// The OS the app runs on: the file manager's name and the terminal's font keys differ.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Os {
    Mac,
    Windows,
    Linux,
}

impl Os {
    pub fn current() -> Self {
        if cfg!(target_os = "macos") {
            Self::Mac
        } else if cfg!(windows) {
            Self::Windows
        } else {
            Self::Linux
        }
    }

    fn keys(self) -> Platform {
        match self {
            Self::Mac => Platform::Mac,
            Self::Windows | Self::Linux => Platform::WinLinux,
        }
    }
}
use crate::shell::tilde;
use crate::view_models::cleanup::{ConfirmLine, WarmConfirmPage};
use crate::view_models::limits::{
    BACK_PRESSURE_ITEMS, MAX_AGENTS, right_now, settings_rows, toggled,
};
use crate::view_models::menu::MenuSearch;
use crate::view_models::settings::{problem_for, remove_text};
use crate::view_models::theme_credits;
use crate::view_models::usage_data;
use crate::view_models::warm_build;

/// A section of the global page; its id is the page registry's.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename = "GlobalSectionId")]
pub enum GlobalSection {
    /// M23, T-0360: the agent tools, drawn from `agent_tools.view` ([`super::agent_tools_page`]). The Agents topic's
    /// first section.
    AgentTools,
    /// T-0405: the agents, the default model and the agent Noyan uses, drawn from `settings.agents`
    /// ([`super::agents::section`]).
    Agents,
    /// Git and the pull request providers on this machine, drawn from `settings.git` ([`super::git_settings`]).
    /// Other screens open Settings at this topic by its id, `git`.
    Git,
    /// The optional agent limit: the Limits & usage topic's first section.
    AgentLimits,
    /// What is left of each tool's plan, drawn from the Running popover's usage.
    Usage,
    Notifications,
    Appearance,
    /// M29, T-0395: custom shortcuts and the single-letter switch, drawn from `keyboard.view` ([`super::keyboard_page`]).
    KeyboardShortcuts,
    Data,
    /// The last one: the mark, version, licenses and credits (d0071).
    About,
}

/// A card of the project's Settings page ([`super::project_settings`]), in the page's order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename = "ProjectSectionId")]
pub enum ProjectSection {
    /// The project's flow: drawn from `settings.flow` ([`super::flow_settings`]), no rows here.
    Flow,
    /// Name, base branch, the flow's delivery, the PR provider, the merge check.
    Repository,
    /// The apps: run commands and Open URLs.
    Run,
    /// Setup command, the variables the user sets, files to copy, ticket resources, finished tickets, warm build.
    Workspace,
    /// The notes every agent in the project gets (AGM-02).
    ProjectNotes,
    RemoveProject,
}

/// Where a nav row goes: one topic of the global page. A global topic is its own page: the window shows only its
/// `sections` (the first is `section`), never the whole global page scrolling. A project's settings are a view of the
/// project's own ([`super::project_settings`]), not a page here.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(tag = "page", rename_all = "snake_case")]
#[ts(rename = "SettingsTarget")]
pub enum Target {
    Global {
        section: GlobalSection,
        sections: Vec<GlobalSection>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "SettingsNavRow")]
pub struct NavRow {
    pub label: String,
    pub icon: IconName,
    pub target: Target,
}

/// One choice of a segmented control.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "SettingsChoice")]
pub struct Choice {
    pub id: String,
    pub label: String,
}

/// One env file, its field `env_files.<index>`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "SettingsEnvFile")]
pub struct EnvFileRow {
    pub value: String,
    pub problem: Option<String>,
}

/// One project environment variable, `NAME = value` (ENV-01; *15 → Row · Environment variables (0082)*). Its fields
/// are `env_vars.<index>.name` and `env_vars.<index>.value`; `problem` (ENV-03, from the service) marks the name red.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "SettingsEnvVar")]
pub struct EnvVarRow {
    pub name: String,
    pub value: String,
    pub problem: Option<String>,
    /// The × button's name and tooltip: "Remove NODE_OPTIONS".
    pub remove_label: String,
}

/// What a row edits or shows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(rename = "SettingsControl")]
pub enum Control {
    /// Saved on blur and on ↵ (a multiline field: on blur and ⌘↵).
    Text {
        value: String,
        placeholder: String,
        mono: bool,
        multiline: bool,
        problem: Option<String>,
    },
    Toggle {
        on: bool,
    },
    /// The main notifications switch with macOS's answer under the row's text (ATT-14, d0090): sent like a toggle.
    /// `status` is `None` where the OS state isn't known (no line).
    NotificationsSwitch {
        on: bool,
        status: Option<OsStatus>,
    },
    /// A checkbox row: a notification kind or Play a sound (ATT-16), indented under the main switch and disabled
    /// (not hidden) while it is off.
    Checkbox {
        on: bool,
        disabled: bool,
        indented: bool,
    },
    Stepper {
        value: u32,
        min: u32,
        max: u32,
        /// " px" after the number.
        suffix: String,
        disabled: bool,
        /// Shown in place of the value while a project uses the global one: "Use global (3)". − and + start from
        /// `value` (the global one).
        empty: Option<String>,
    },
    Segmented {
        choices: Vec<Choice>,
        value: String,
    },
    /// A dropdown of `choices`, `value` the chosen one's id (the Terminal font, d0117 point 5).
    Select {
        choices: Vec<Choice>,
        value: String,
    },
    /// A quiet text in place of a control ("Always on").
    Note {
        text: String,
    },
    /// A read-only value; `boxed` draws it in a field-like box (the project key).
    Value {
        text: String,
        mono: bool,
        boxed: bool,
    },
    /// A path with the button that shows it in the file manager (`settings.reveal`).
    Path {
        text: String,
        reveal: Reveal,
        reveal_label: String,
    },
    /// A button; `action` is what the Settings screen does with a click.
    Button {
        label: String,
        icon: IconName,
        destructive: bool,
        action: ButtonAction,
    },
    /// ↵ in the last field adds a file (`env_files.add`); × removes one (`env_files.remove`).
    EnvFiles {
        rows: Vec<EnvFileRow>,
        add_placeholder: String,
        remove_label: String,
    },
    /// The project environment variables (ENV-01, d0082), drawn across the row's width under its text. "Add
    /// variable" opens a new row (`env_vars.add` with a `{ name, value }` once anything is typed in it: one with a
    /// value but no name is kept and marked, T-0506); × removes one (`env_vars.remove`).
    EnvVars {
        rows: Vec<EnvVarRow>,
        /// The always-visible example: `CI = true`, then its reason.
        hint_example: String,
        hint: String,
        /// The box shown while there are no rows.
        empty: String,
        add_label: String,
        /// The × of a new line that isn't saved yet.
        drop_label: String,
        /// The line under the list: changes reach only what starts after them.
        applies: String,
        name_placeholder: String,
        value_placeholder: String,
    },
    /// "Start new worktrees from a warm build" (WRM-05, WRM-06, d0082). The switch sends `warm_builds` like a
    /// toggle; turning it off first asks `confirm` when there is one (only when it would delete a warm build).
    WarmBuild {
        on: bool,
        /// Drawn off and disabled, `reason` under it and in its tooltip. Never set since M19-b: the switch works on
        /// every disk (where the disk can't clone, the build is handed over).
        disabled: bool,
        reason: Option<String>,
        /// "Clearing… (waiting for SHOP-21’s copy)" while the cleared warm build waits for a seed.
        clearing: Option<String>,
        /// What turning it off asks (*Warm build · turning off · confirm (0082)*).
        confirm: Option<WarmConfirmPage>,
    },
    /// The base branch: a dropdown of the repository's branches (`settings.branches`).
    Branch {
        value: String,
        /// The dropdown's search field.
        search: MenuSearch,
    },
    /// The 15 themes (`theme.picker`, `theme.pick`).
    Themes,
    /// The PR provider line, detected when the page opens (`project_settings.provider`).
    Provider,
    /// How work leaves the flow: the flow's own `delivery:` setting, shown on the project's page and changed through
    /// the flow (`delivery.delivers`, `delivery.mode`; [`super::project_settings::deliver_as`]). Either is chosen at
    /// your click, so with Either the modes show On Approve, switched off (`modes_off`), and `either` says why.
    Delivery {
        delivers: Vec<Choice>,
        delivers_value: String,
        modes: Vec<Choice>,
        mode_value: String,
        modes_off: bool,
        either: Option<String>,
        /// "This is the flow's setting: changing it here changes the flow, and the flow file."
        note: String,
    },
    /// The About section (`settings.about`; d0071): the page draws it whole.
    About,
}

/// What the OS allows Otaq's notifications, as one line with the one action that fits (ATT-14, d0090).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "SettingsOsStatus")]
pub struct OsStatus {
    pub text: String,
    /// "Allow notifications" or "Open System Settings".
    pub action: Option<OsStatusAction>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "SettingsOsStatusAction")]
pub struct OsStatusAction {
    pub label: String,
    pub action: ButtonAction,
}

/// The status line under the main switch, by macOS's permission (ATT-14, d0090; *CHANGES-2026-10-18-aa* §4). Shown
/// whatever the switch's state; none where the state isn't read (Windows and Linux for now).
pub fn os_status(permission: PermissionState) -> Option<OsStatus> {
    let action = |label: &str, action| {
        Some(OsStatusAction {
            label: label.to_owned(),
            action,
        })
    };
    let (text, action) = match permission {
        PermissionState::NotAsked => (
            "macOS hasn\u{2019}t been asked yet.".to_owned(),
            action("Allow notifications", ButtonAction::AllowNotifications),
        ),
        PermissionState::Allowed => ("Allowed in macOS.".to_owned(), None),
        PermissionState::TurnedOff => (
            format!("Turned off in macOS, so {PRODUCT_NAME} can\u{2019}t notify you."),
            action(
                crate::view_models::banners::OPEN_SYSTEM_SETTINGS,
                ButtonAction::OpenSystemSettings,
            ),
        ),
        PermissionState::OutsideBundle => (
            format!(
                "Running outside {}: notifications appear as Script Editor, and {PRODUCT_NAME} can\u{2019}t check \
                 whether macOS allows them.",
                naming::bundle_folder_name()
            ),
            None,
        ),
        PermissionState::Unknown => return None,
    };
    Some(OsStatus { text, action })
}

/// Limits & usage's first line.
pub const LIMITS_INTRO: &str =
    "How many agents run at once, and how much of each tool\u{2019}s plan is left.";

/// Under the usage: what a tool's limit does.
pub const USAGE_NOTE: &str = "When a tool hits its limit, its agents stop and wait; other tools keep working. A banner says when it resets.";

/// The notification kinds as Settings → Notifications groups them: one checkbox per group, which turns every kind in
/// it on or off. On by default for the blocking groups only; a review blocks nothing, so its group starts off.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotificationGroup {
    /// A question, a permission prompt, an agent that ended its turn waiting for you.
    Asks,
    /// A session interrupted, a check that needs a decision, a delivery that stopped, a tool not available.
    Broke,
    /// A ticket reaching your review.
    Review,
}

impl NotificationGroup {
    pub const ALL: [Self; 3] = [Self::Asks, Self::Broke, Self::Review];

    pub fn kinds(self) -> &'static [Kind] {
        match self {
            Self::Asks => &[Kind::Permission, Kind::Question, Kind::TurnEnded],
            Self::Broke => &[
                Kind::Interrupted,
                Kind::ValidationDecision,
                Kind::AutoMergeStopped,
                Kind::ToolUnavailable,
            ],
            Self::Review => &[Kind::ReadyForReview],
        }
    }

    /// Its field, label and description.
    pub fn row(self) -> (&'static str, &'static str, &'static str) {
        match self {
            Self::Asks => (
                "notification_groups.asks",
                "An agent asks you something",
                "A question or a permission prompt, from an agent, a checker or Noyan.",
            ),
            Self::Broke => (
                "notification_groups.broke",
                "Something broke and waits for you",
                "A session stopped, a check needs a decision, a delivery stopped or an agent tool isn\u{2019}t available.",
            ),
            Self::Review => (
                "notification_groups.review",
                "A ticket is ready for your review",
                "Off by default: a review isn\u{2019}t blocking anything.",
            ),
        }
    }
}

/// The checkbox of each notification kind (ATT-16, d0090), in d0090's order: its field, label and who it covers
/// (d0090's parentheses; none where d0090 gives none).
pub fn notification_kind_row(kind: Kind) -> (&'static str, &'static str, &'static str) {
    match kind {
        Kind::Permission => (
            "notification_kinds.permission",
            "Permission prompts",
            "Agents, checkers and Noyan.",
        ),
        Kind::Question => (
            "notification_kinds.question",
            "Questions",
            "Agents and Noyan.",
        ),
        Kind::TurnEnded => ("notification_kinds.turn_ended", "Agent ended its turn", ""),
        Kind::Interrupted => ("notification_kinds.interrupted", "Session interrupted", ""),
        Kind::ValidationDecision => (
            "notification_kinds.validation_decision",
            "Validation needs a decision",
            "Failed N times, or no result.",
        ),
        Kind::AutoMergeStopped => (
            "notification_kinds.auto_merge_stopped",
            "Delivery stopped",
            "",
        ),
        Kind::ToolUnavailable => (
            "notification_kinds.tool_unavailable",
            "Agent tool not available",
            "",
        ),
        Kind::ReadyForReview => (
            "notification_kinds.ready_for_review",
            "Ready for your review",
            "A ticket reaches Review, or another check you do. Off by default: it blocks no agent.",
        ),
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename = "SettingsReveal")]
pub enum Reveal {
    Database,
    Logs,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename = "SettingsButtonAction")]
pub enum ButtonAction {
    /// Opens Recovery and Cleanup.
    Cleanup,
    /// Opens the Theme credits dialog.
    Credits,
    /// Asks before removing the project.
    RemoveProject,
    /// Empties the run command an agent set ("Set by Dev on SHOP-27 · Undo", agents run the app).
    UndoRunCommand,
    /// Sends the row's own field as an edit with an empty value (agents run the app): Add app (`apps.add`), Remove app
    /// (`apps.1.remove`).
    EditField,
    /// Brings up macOS's notifications question (in the app's process) and answers Otaq's ask (ATT-13, ATT-14).
    AllowNotifications,
    /// Opens Otaq's entry in System Settings → Notifications (ATT-14).
    OpenSystemSettings,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "SettingsRow")]
pub struct Row {
    /// What `settings.edit` names; empty for a row that edits nothing.
    pub field: String,
    pub label: String,
    pub description: String,
    pub control: Control,
    /// The limits are off: the row dims with its control.
    pub dimmed: bool,
    /// A project's own value: the link that empties it so the global one applies again (`settings.edit` with
    /// `""`).
    pub reset: Option<String>,
    /// A button after the control: Run command's **Ask the agent** (T-0232, T-0342), which opens the run-command
    /// dialog.
    pub button: Option<RowButton>,
    /// Run command's "Set by Dev on SHOP-27 · Undo" (agents run the app); Undo is [`ButtonAction::UndoRunCommand`].
    pub set_by: Option<crate::view_models::run_page::SetByView>,
}

/// A row's button and what it opens.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "SettingsRowButton")]
pub struct RowButton {
    pub label: String,
    pub icon: IconName,
    /// "Asks Noyan (Strong) to suggest a run command".
    pub tooltip: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "SettingsSection")]
pub struct Section<Id> {
    pub id: Id,
    /// The small title above the rows (the global page); a project page's card title.
    pub title: Option<String>,
    pub intro: Option<String>,
    pub rows: Vec<Row>,
    /// A line under the rows ("Right now: 3 running · …").
    pub note: Option<String>,
}

/// The page on the right: "General" (the global sections; the window shows one nav topic's at a time) or a project.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(tag = "page", rename_all = "snake_case")]
#[ts(rename = "SettingsPage")]
pub enum Page {
    Global {
        title: String,
        sections: Vec<Section<GlobalSection>>,
    },
    Project {
        key: String,
        title: String,
        intro: String,
        sections: Vec<Section<ProjectSection>>,
    },
}

/// The whole Settings view: the nav and the open page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "SettingsView")]
pub struct View {
    pub title: String,
    /// Under the nav: "Project settings live in each project's Settings tab."
    pub footnote: String,
    pub close_label: String,
    pub nav: Vec<NavRow>,
    pub page: Page,
}

/// What the global page is built from.
pub struct GlobalInputs<'a> {
    pub settings: &'a GlobalSettings,
    pub problems: &'a [SettingProblem],
    /// The scheduler's view, for the "Right now" line while the limits are on.
    pub scheduler: Option<&'a SchedulerStatus>,
    /// The database file and the logs folder (`settings::data_paths`).
    pub data: Option<(String, String)>,
    pub cleanup_count: usize,
    /// What the leftovers take on disk, as far as the scan could tell.
    pub cleanup_bytes: u64,
    pub os: Os,
    /// What the OS allows Otaq's notifications (`notifications.state`); `Unknown` when it couldn't be read.
    pub notification_permission: PermissionState,
    /// The installed fonts, for the Terminal font dropdown (d0117 point 5).
    pub fonts: &'a InstalledFonts,
}

/// What a project page is built from.
pub struct ProjectInputs<'a> {
    pub info: &'a ProjectInfo,
    pub settings: &'a ProjectSettings,
    pub problems: &'a [SettingProblem],
    pub global: &'a GlobalSettings,
    /// `warm.list` and the time now (for the warm build's age); `None` when it couldn't be read.
    pub warm: Option<WarmInputs<'a>>,
    /// The name of the project's Project agent (its role's named agent), which Ask the agent asks.
    pub project_agent: &'a str,
    /// An agent tool is connected: Run command's Ask the agent is hidden only without one (RUN-02, d0086).
    pub tool_connected: bool,
    /// The flow's delivery, which the Repository card shows as the flow's; `None` when the flow doesn't read.
    pub delivery: Option<&'a otaq_core::flow::model::Delivery>,
}

/// What the warm build switch is drawn from.
#[derive(Clone, Copy)]
pub struct WarmInputs<'a> {
    pub list: &'a WarmList,
    pub now: i64,
}

/// The global sections, in nav order.
const DRAWN: [GlobalSection; 10] = [
    GlobalSection::AgentTools,
    GlobalSection::Agents,
    GlobalSection::Git,
    GlobalSection::AgentLimits,
    GlobalSection::Usage,
    GlobalSection::Notifications,
    GlobalSection::Appearance,
    GlobalSection::KeyboardShortcuts,
    GlobalSection::Data,
    GlobalSection::About,
];

/// The nav row a global section is drawn under. App settings has eight topics: the agent tools sit on the Agents
/// page above the agents, and usage on one page with the agent limit; every other section is a topic of its own.
pub fn topic_of(id: GlobalSection) -> GlobalSection {
    match id {
        GlobalSection::AgentTools => GlobalSection::Agents,
        GlobalSection::Usage => GlobalSection::AgentLimits,
        other => other,
    }
}

/// The nav: one row per global topic the page has. App settings are about you and your machine: a project's
/// settings are its own page, not a row here.
pub fn nav(sections: &[GlobalSection]) -> Vec<NavRow> {
    sections
        .iter()
        .filter(|id| topic_of(**id) == **id)
        .map(|id| {
            let (label, icon) = nav_label(*id);
            NavRow {
                label: label.to_owned(),
                icon,
                target: Target::Global {
                    section: *id,
                    sections: sections
                        .iter()
                        .copied()
                        .filter(|s| topic_of(*s) == *id)
                        .collect(),
                },
            }
        })
        .collect()
}

/// A topic's nav label and icon; the sections inside a topic have none of their own.
fn nav_label(id: GlobalSection) -> (&'static str, IconName) {
    match id {
        GlobalSection::AgentTools | GlobalSection::Agents => ("Agents", IconName::Bot),
        GlobalSection::Git => ("Git & pull requests", IconName::GitPullRequest),
        GlobalSection::AgentLimits | GlobalSection::Usage => ("Limits & usage", IconName::Gauge),
        GlobalSection::Notifications => ("Notifications", IconName::Bell),
        GlobalSection::Appearance => ("Appearance", IconName::Palette),
        GlobalSection::KeyboardShortcuts => ("Keyboard", IconName::Keyboard),
        GlobalSection::Data => ("Data", IconName::Database),
        GlobalSection::About => ("About", IconName::Info),
    }
}

/// The nav's line under the topics: where a project's own settings are.
pub const PROJECT_SETTINGS_NOTE: &str =
    "Project settings live in each project\u{2019}s Settings tab.";

/// The view around a page.
pub fn view(page: Page) -> View {
    let sections: Vec<GlobalSection> = DRAWN.to_vec();
    View {
        title: "Settings".into(),
        footnote: PROJECT_SETTINGS_NOTE.into(),
        close_label: "Close settings".into(),
        nav: nav(&sections),
        page,
    }
}

pub(crate) fn row(
    field: &str,
    label: &str,
    description: impl Into<String>,
    control: Control,
) -> Row {
    Row {
        field: field.to_owned(),
        label: label.to_owned(),
        description: description.into(),
        control,
        dimmed: false,
        reset: None,
        button: None,
        set_by: None,
    }
}

/// The help under Merge check (d0120); the page shows `npm test` as code.
pub const MERGE_CHECK_HELP: &str = "Runs on the merged result when you Approve & merge, before it lands, if the base has commits the agent\u{2019}s work didn\u{2019}t have. For example `npm test`.";

/// "Merge check" (d0120), next to the setup command and built like it: a text field that saves as you type, empty by
/// default (never guessed, d0023).
pub(crate) fn merge_check_row(s: &ProjectSettings, p: &[SettingProblem]) -> Row {
    row(
        "merge_check",
        "Merge check \u{b7} optional",
        MERGE_CHECK_HELP,
        text(
            s.merge_check.as_deref().unwrap_or_default(),
            "none \u{2014} e.g. npm test",
            true,
            problem_for(p, "merge_check"),
        ),
    )
}

pub(crate) fn text(value: &str, placeholder: &str, mono: bool, problem: Option<String>) -> Control {
    Control::Text {
        value: value.to_owned(),
        placeholder: placeholder.to_owned(),
        mono,
        multiline: false,
        problem,
    }
}

pub(crate) fn multiline(value: &str, placeholder: &str) -> Control {
    Control::Text {
        value: value.to_owned(),
        placeholder: placeholder.to_owned(),
        mono: false,
        multiline: true,
        problem: None,
    }
}

fn section<Id>(id: Id, title: Option<&str>, rows: Vec<Row>) -> Section<Id> {
    Section {
        id,
        title: title.map(str::to_owned),
        intro: None,
        rows,
        note: None,
    }
}

/// "Reveal in Finder" as each OS calls it.
pub fn reveal_label(os: Os) -> &'static str {
    match os {
        Os::Mac => "Reveal in Finder",
        Os::Windows => "Show in Explorer",
        Os::Linux => "Show in Files",
    }
}

/// The terminal renderer's words (Q38, built as option 1).
pub fn renderer_choices() -> Vec<Choice> {
    [
        (TerminalRenderer::Webgl, "WebGL (faster)"),
        (TerminalRenderer::Dom, "DOM (safer)"),
    ]
    .into_iter()
    .map(|(r, label)| Choice {
        id: renderer_id(r).to_owned(),
        label: label.to_owned(),
    })
    .collect()
}

fn renderer_id(renderer: TerminalRenderer) -> &'static str {
    match renderer {
        TerminalRenderer::Webgl => "webgl",
        TerminalRenderer::Dom => "dom",
    }
}

pub fn global_page(input: &GlobalInputs) -> Page {
    let s = input.settings;
    let limits = s.agent_limits;
    let [toggle, max, pause] = settings_rows(&limits);
    let stepper = |value, range: &std::ops::RangeInclusive<u32>| Control::Stepper {
        value,
        min: *range.start(),
        max: *range.end(),
        suffix: String::new(),
        disabled: !limits.enabled,
        empty: None,
    };
    let limit_rows = vec![
        row(
            "agent_limits.enabled",
            toggle.label,
            toggle.description,
            Control::Toggle { on: limits.enabled },
        ),
        Row {
            dimmed: !max.enabled,
            ..row(
                "agent_limits.max_concurrency",
                max.label,
                max.description,
                stepper(limits.max_concurrency, &MAX_AGENTS),
            )
        },
        Row {
            dimmed: !pause.enabled,
            ..row(
                "agent_limits.back_pressure",
                pause.label,
                pause.description,
                stepper(limits.back_pressure, &BACK_PRESSURE_ITEMS),
            )
        },
    ];
    let agent_limits = Section {
        intro: Some(LIMITS_INTRO.into()),
        note: input.scheduler.filter(|_| limits.enabled).map(right_now),
        ..section(GlobalSection::AgentLimits, None, limit_rows)
    };

    // Its own view: each tool's plan windows as "% left", from the Running popover's usage.
    let usage = Section {
        note: Some(USAGE_NOTE.into()),
        ..section(GlobalSection::Usage, Some("Usage"), Vec::new())
    };

    // The main switch with macOS's answer under it, then a checkbox per group of kinds and Play a sound, disabled
    // (not hidden) while the switch is off (ATT-14, ATT-16; d0090). Only blocking things are on by default.
    let mut notification_rows = vec![row(
        "notifications",
        "Notifications",
        "",
        Control::NotificationsSwitch {
            on: s.notifications,
            status: os_status(input.notification_permission),
        },
    )];
    let checkbox = |field: &str, label: &str, description: &str, on: bool| Row {
        dimmed: !s.notifications,
        ..row(
            field,
            label,
            description,
            Control::Checkbox {
                on,
                disabled: !s.notifications,
                indented: true,
            },
        )
    };
    for group in NotificationGroup::ALL {
        let (field, label, description) = group.row();
        notification_rows.push(checkbox(
            field,
            label,
            description,
            group
                .kinds()
                .iter()
                .all(|k| s.notification_kinds.allows(*k)),
        ));
    }
    notification_rows.push(Row {
        dimmed: !s.notifications,
        ..row(
            "",
            "Not while you\u{2019}re looking at that ticket",
            "A ticket open in front of you doesn\u{2019}t notify.",
            Control::Note {
                text: "Always".into(),
            },
        )
    });
    notification_rows.push(checkbox(
        "notification_sound",
        "Play a sound",
        "The OS\u{2019}s default notification sound.",
        s.notification_sound,
    ));
    let notifications = Section {
        intro: Some(format!(
            "{PRODUCT_NAME} tells you when something is blocked on you, even with the window closed."
        )),
        ..section(GlobalSection::Notifications, None, notification_rows)
    };

    let reveal = reveal_label(input.os);
    let mut data_rows = Vec::new();
    if let Some((db, logs)) = &input.data {
        let db_folder = std::path::Path::new(db)
            .parent()
            .map_or_else(|| db.clone(), |p| p.to_string_lossy().into_owned());
        data_rows.push(row(
            "",
            "Data folder",
            format!(
                "Tickets, runs and settings. Your normal machine backup covers it — {PRODUCT_NAME} has no separate backup."
            ),
            Control::Path {
                text: db_folder,
                reveal: Reveal::Database,
                reveal_label: reveal.to_owned(),
            },
        ));
        let folder = logs.trim_end_matches(['/', '\\']).trim_end_matches("logs");
        data_rows.push(row(
            "",
            "Logs folder",
            format!(
                "Service and session logs. The same {folder} folder on macOS, Windows and Linux."
            ),
            Control::Path {
                text: logs.clone(),
                reveal: Reveal::Logs,
                reveal_label: reveal.to_owned(),
            },
        ));
    }
    // d0085: off until the user said yes, in the one-time question or here.
    data_rows.push(row(
        "usage_data",
        usage_data::ROW_LABEL,
        usage_data::row_description(),
        Control::Toggle {
            on: s.usage_data.sends(),
        },
    ));
    // Cleanup opens from here (it has no rail button).
    let count = input.cleanup_count;
    data_rows.push(row(
        "",
        "Cleanup",
        if count == 0 {
            "Nothing to clear: no leftover copies, branches, sessions or ports that no ticket uses.".to_owned()
        } else {
            let size = match input.cleanup_bytes {
                0 => String::new(),
                bytes => format!(" \u{b7} {}", crate::view_models::cleanup::size(bytes)),
            };
            format!(
                "{count} {}{size} \u{b7} copies and branches no ticket uses. Nothing is deleted without your click.",
                if count == 1 { "leftover" } else { "leftovers" }
            )
        },
        Control::Button {
            label: "Review\u{2026}".into(),
            icon: IconName::BrushCleaning,
            destructive: false,
            action: ButtonAction::Cleanup,
        },
    ));
    let data = Section {
        intro: Some(format!(
            "Where {PRODUCT_NAME} keeps its board, and the leftovers you can clear."
        )),
        ..section(GlobalSection::Data, None, data_rows)
    };

    let bigger = TerminalKey::FontBigger.keys(input.os.keys())[0];
    let smaller = TerminalKey::FontSmaller.keys(input.os.keys())[0];
    let appearance = section(
        GlobalSection::Appearance,
        Some("Appearance"),
        vec![
            row(
                "theme",
                "Theme",
                "Match system shows the light theme while your system is light and the dark one while it is dark. Pick one of each; it saves at once.",
                Control::Themes,
            ),
            row(
                "",
                "Theme credits",
                theme_credits::ROW_DESCRIPTION,
                Control::Button {
                    label: "View credits".into(),
                    icon: IconName::ScrollText,
                    destructive: false,
                    action: ButtonAction::Credits,
                },
            ),
            row(
                "terminal_font_size",
                "Terminal font size",
                format!("Also {bigger} / {smaller} inside a focused terminal."),
                Control::Stepper {
                    value: s.terminal_font_size,
                    min: *TERMINAL_FONT_SIZES.start(),
                    max: *TERMINAL_FONT_SIZES.end(),
                    suffix: " px".into(),
                    disabled: false,
                    empty: None,
                },
            ),
            // d0117 point 5 (T-0482): every terminal follows it at once. Its layout is provisional (designer notes).
            row(
                "terminal_font_family",
                "Terminal font",
                "Icons your font lacks come from an installed Nerd Font.",
                Control::Select {
                    choices: terminal_font::choices(input.fonts),
                    value: terminal_font::chosen(s, input.fonts),
                },
            ),
            // Q38, built as option 1: one global row, read by every terminal (T-0216).
            row(
                "terminal_renderer",
                "Terminal renderer",
                "WebGL draws on the GPU. Pick DOM if a terminal stays blank or garbled; every terminal redraws at once.",
                Control::Segmented {
                    choices: renderer_choices(),
                    value: renderer_id(s.terminal_renderer).to_owned(),
                },
            ),
        ],
    );

    Page::Global {
        title: "General".into(),
        sections: vec![
            // Its own view: the four tools' rows ([`super::agent_tools_page`]).
            section(GlobalSection::AgentTools, Some("Tools"), Vec::new()),
            // Its own view: the list, the default model and Noyan's agent ([`super::agents::section`]).
            section(GlobalSection::Agents, Some("Your agents"), Vec::new()),
            // Its own view: git and the PR providers ([`super::git_settings`]).
            section(GlobalSection::Git, None, Vec::new()),
            agent_limits,
            usage,
            notifications,
            appearance,
            // Its own view: the short list and every action's keys ([`super::keyboard_page`]).
            section(
                GlobalSection::KeyboardShortcuts,
                Some("Keyboard"),
                Vec::new(),
            ),
            data,
            section(
                GlobalSection::About,
                Some("About"),
                vec![row("", "", "", Control::About)],
            ),
        ],
    }
}

/// The ticket resources' rows (M19-d, M19-e; d0087), in the project's Workspace: the commands that make, remove and
/// refresh a ticket's own resources, and the schema paths. All empty by default, and then nothing changes.
pub fn ticket_resource_rows(s: &ProjectSettings, p: &[SettingProblem]) -> Vec<Row> {
    let (db_name, db_template, resource_dir) = (
        naming::db_name_env_var(),
        naming::db_template_env_var(),
        naming::resource_dir_env_var(),
    );
    let schema_problems: Vec<&str> = p
        .iter()
        .filter(|problem| problem.field == "schema_paths")
        .map(|problem| problem.message.as_str())
        .collect();
    vec![
        row(
            "resource_create",
            "Per-ticket resources \u{b7} optional",
            format!(
                "Creates what each ticket needs of its own, like a database named ${db_name} from \
                         ${db_template}, in each new copy after the files and before setup. {PRODUCT_NAME} then sets \
                         {db_name}, {db_template} and {resource_dir} in the ticket’s commands and sessions."
            ),
            text(
                &s.resource_create,
                "none",
                true,
                problem_for(p, "resource_create"),
            ),
        ),
        row(
            "resource_remove",
            "Remove command",
            "Runs at Done and Close in the repository, for a resource Create made.",
            text(
                &s.resource_remove,
                "none",
                true,
                problem_for(p, "resource_remove"),
            ),
        ),
        row(
            "resource_template_refresh",
            "Template refresh command",
            format!(
                "Runs in the repository after Done, so new tickets start from the merged schema. \
                         Creates wait while it runs. ${db_template} is set."
            ),
            text(
                &s.resource_template_refresh,
                "none",
                true,
                problem_for(p, "resource_template_refresh"),
            ),
        ),
        row(
            "schema_paths",
            "Schema paths",
            "One path per line, like db/migrations/**. Two tickets in Review adding files under the same \
                     path get a warning.",
            Control::Text {
                value: s.schema_paths.join("\n"),
                placeholder: "none".into(),
                mono: true,
                multiline: true,
                problem: (!schema_problems.is_empty()).then(|| schema_problems.join("; ")),
            },
        ),
    ]
}

/// The × of an environment variable row without a name.
const REMOVE_VARIABLE: &str = "Remove variable";

/// The Environment variables row's description: where the rows are set, and that a value can name Otaq's own
/// variables as `${…}` (d0087; [`otaq_core::session::env::substitute`]).
fn env_vars_description() -> String {
    format!(
        "Only the ones you set here: set in every agent session, Run and Terminal of this project. You can use ${{{}}} and {PRODUCT_NAME}’s other variables inside a value.",
        naming::db_name_env_var()
    )
}

/// The Environment variables row (ENV-01, ENV-03; d0082, d0087; *15 → Row · Environment variables (0082)*): every row as
/// written, in full and in plain text, each with the problem the service found (`env_vars.<index>`). There are no
/// built-in, read-only or computed rows; the hint is only an example.
/// The run command rows (agents run the app). With one app (or none yet) they look as before apps had names: Run command (with
/// Ask the agent and "Set by Dev on SHOP-27 · Undo"), Open URL, and Add app once there is a command. With several, each
/// app has its name, command and Open URL rows and Remove, then Add app.
pub(crate) fn app_rows(
    s: &ProjectSettings,
    p: &[SettingProblem],
    tool_connected: bool,
    project_agent: &str,
) -> Vec<Row> {
    use crate::view_models::run_page::{ASK_LABEL, ask_tooltip, set_by_view};
    use otaq_core::board::settings::app_field;
    let add = Row {
        description: format!(
            "A monorepo? Add another app (web, api, worker). Each runs with its own port; every app\u{2019}s command \
             also gets each app\u{2019}s port as {}<NAME>.",
            otaq_core::naming::env_var("PORT_")
        ),
        ..row(
            "apps.add",
            "More apps",
            "",
            Control::Button {
                label: "Add app".into(),
                icon: IconName::Plus,
                destructive: false,
                action: ButtonAction::EditField,
            },
        )
    };
    if s.apps.len() <= 1 {
        let only = s.apps.first();
        let mut rows = vec![
            Row {
                set_by: set_by_view(only.and_then(|a| a.set_by.as_ref()), None),
                button: tool_connected.then(|| RowButton {
                    label: ASK_LABEL.into(),
                    icon: IconName::Sparkles,
                    tooltip: Some(ask_tooltip(project_agent)),
                }),
                ..row(
                    "run_command",
                    "Run command",
                    "Runs in the ticket’s worktree when you press Run. Use $PORT if it listens on a port.",
                    text(
                        only.map_or("", |a| a.command.as_str()),
                        "not set",
                        true,
                        problem_for(p, &app_field(0, "command")),
                    ),
                )
            },
            row(
                "open_url",
                "Open URL",
                "Optional. Shows an Open button while it runs. Leave empty for backends, CLIs and libraries.",
                text(
                    only.map_or("", |a| a.open_url.as_str()),
                    "none",
                    true,
                    problem_for(p, &app_field(0, "open_url")),
                ),
            ),
        ];
        if only.is_some() {
            rows.push(add);
        }
        return rows;
    }
    let mut rows = Vec::new();
    for (i, app) in s.apps.iter().enumerate() {
        let name = if app.name.trim().is_empty() {
            "this app".to_owned()
        } else {
            app.name.trim().to_owned()
        };
        rows.push(row(
            &app_field(i, "name"),
            "App",
            format!(
                "Its port is $PORT in its own command and {} in every app\u{2019}s.",
                otaq_core::naming::app_port_env_var(&app.name)
            ),
            text(
                &app.name,
                "web",
                true,
                problem_for(p, &app_field(i, "name")),
            ),
        ));
        rows.push(Row {
            set_by: set_by_view(app.set_by.as_ref(), Some(&app.name)),
            ..row(
                &app_field(i, "command"),
                &format!("Run command \u{b7} {name}"),
                "Runs in the ticket’s worktree. Use $PORT where it listens.",
                text(
                    &app.command,
                    "not set",
                    true,
                    problem_for(p, &app_field(i, "command")),
                ),
            )
        });
        rows.push(row(
            &app_field(i, "open_url"),
            &format!("Open URL \u{b7} {name}"),
            "Optional. Shows an Open button while it runs.",
            text(
                &app.open_url,
                "none",
                true,
                problem_for(p, &app_field(i, "open_url")),
            ),
        ));
        rows.push(row(
            &app_field(i, "remove"),
            "",
            "",
            Control::Button {
                label: format!("Remove {name}"),
                icon: IconName::Trash2,
                destructive: true,
                action: ButtonAction::EditField,
            },
        ));
    }
    rows.push(add);
    rows
}

pub fn env_vars_row(s: &ProjectSettings, problems: &[SettingProblem]) -> Row {
    row(
        "env_vars",
        "Environment variables",
        env_vars_description(),
        Control::EnvVars {
            rows: s
                .env_vars
                .iter()
                .enumerate()
                .map(|(i, v)| EnvVarRow {
                    name: v.name.clone(),
                    value: v.value.clone(),
                    problem: problem_for(problems, &env_var_field(i)),
                    remove_label: if v.name.is_empty() {
                        REMOVE_VARIABLE.into()
                    } else {
                        format!("Remove {}", v.name)
                    },
                })
                .collect(),
            hint_example: "CI = true".into(),
            hint: "e.g. so tools never wait for input or start watch mode".into(),
            empty: "No variables. Sessions get your normal environment.".into(),
            add_label: "Add variable".into(),
            drop_label: REMOVE_VARIABLE.into(),
            applies: "Changes apply to sessions, Runs and Shells started after this.".into(),
            name_placeholder: "NAME".into(),
            value_placeholder: "value".into(),
        },
    )
}

/// The warm build switch (WRM-05, WRM-06; d0082; *15 → Row · Start new worktrees from a warm build (0082)*), on by
/// default, on every disk (M19-b, d0085). Where the data folder's disk can't clone, the description says the build is
/// handed over (renamed) to the next ticket instead. Turning it off asks only when it would delete the project's warm
/// build; while that Clear waits for a seed, the row says so. It names no build tool.
pub fn warm_builds_row(info: &ProjectInfo, s: &ProjectSettings, warm: Option<WarmInputs>) -> Row {
    let build = warm.and_then(|w| {
        w.list
            .builds
            .iter()
            .find(|b| b.project.as_deref() == Some(info.key.as_str()))
            .map(|b| (b, w.now))
    });
    let clearing = build
        .filter(|(b, _)| b.status.clearing)
        .map(|(b, _)| warm_build::waiting(&b.status.copying));
    let control = Control::WarmBuild {
        on: s.warm_builds,
        disabled: false,
        reason: None,
        confirm: build
            .filter(|_| s.warm_builds && clearing.is_none())
            .map(|(b, now)| turn_off_confirm(&info.name, b, now)),
        clearing,
    };
    let description = match warm {
        Some(w) if !w.list.support.available => HANDED_OVER,
        _ => {
            "New tickets start from the last finished ticket’s build, so their first build only redoes what changed."
        }
    };
    row(
        "warm_builds",
        "Start new worktrees from a warm build",
        description,
        control,
    )
}

/// The warm build switch's description where the disk can't clone (plan §4.3): builds are renamed, not cloned.
pub const HANDED_OVER: &str =
    "This disk can’t copy-on-write; the build is handed over to the next ticket.";

/// *Warm build · turning off · confirm (0082)*: the same shape as Cleanup's Clear… confirmation.
fn turn_off_confirm(name: &str, b: &WarmBuildInfo, now: i64) -> WarmConfirmPage {
    let built = match (&b.built_at, b.kept_at) {
        (Some(sha), Some(at)) => format!(", built at {sha}, {}", warm_build::ago(now - at)),
        (Some(sha), None) => format!(", built at {sha}"),
        _ => String::new(),
    };
    let up_to = warm_build::up_to(b.size_bytes);
    WarmConfirmPage {
        title: warm_build::confirm_title(name, b.size_bytes),
        text: format!(
            "Turning this off deletes {name}’s warm build{built}. New tickets will run setup and build from \
             nothing. Ticket worktrees and your repo aren’t touched."
        ),
        shared: warm_build::SHARED_SPACE,
        line: ConfirmLine {
            icon: IconName::Flame,
            text: format!("{} · {up_to} (delete)", tilde(&b.path)),
        },
        confirm: warm_build::turn_off_label(b.size_bytes),
    }
}

/// One state of the warm build switch in the gallery.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "WarmSwitchGalleryCell")]
pub struct WarmSwitchCell {
    pub name: String,
    pub project: String,
    pub row: Row,
}

/// The notification banners and Settings → Notifications in the component gallery (T-0372; ATT-13, ATT-14, ATT-16;
/// no frames, d0090): the ask, the blocked banner and their stacking with the folder trust banners, the permission
/// warning, the usage limit and Reconnecting; the section in every status-line state, and with the switch off.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "NotificationsGallery")]
pub struct NotificationsGallery {
    pub banners: Vec<NotificationsBannerCell>,
    pub pages: Vec<NotificationsPageCell>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "NotificationsBannerCell")]
pub struct NotificationsBannerCell {
    pub name: String,
    pub banners: Vec<crate::view_models::banners::BannerView>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "NotificationsPageCell")]
pub struct NotificationsPageCell {
    pub name: String,
    pub section: Section<GlobalSection>,
}

pub fn notifications_gallery() -> NotificationsGallery {
    use crate::view_models::banners::{BannerModel, Situation, SituationKind, TrustFailure};
    fn limit() -> Situation {
        Situation::UsageLimit {
            title: "Session limit reached".into(),
            text: "Resets at 3:00 pm \u{b7} 8 agents stopped".into(),
            resume: None,
            lines: Vec::new(),
            state: String::new(),
        }
    }
    fn failed() -> Vec<TrustFailure> {
        vec![TrustFailure {
            project: "NOTE".into(),
            reason: "Claude Code\u{2019}s config file couldn\u{2019}t be written".into(),
        }]
    }
    type Raise = fn(&mut BannerModel);
    let banner_cells: [(&str, Raise); 6] = [
        ("the ask · the first agent started, macOS not asked", |m| {
            m.set_notifications(true, false)
        }),
        ("macOS blocked a notification", |m| {
            m.set_notifications(false, true)
        }),
        (
            "stacking · blocked below the folder trust warning, above the permission warning",
            |m| {
                m.raise(Situation::PermissionWarning);
                m.set_trust(false, Vec::new(), failed());
                m.set_notifications(false, true);
            },
        ),
        ("stacking · blocked above the permission warning", |m| {
            m.raise(Situation::PermissionWarning);
            m.set_notifications(true, true);
        }),
        ("stacking · the usage limit on top, reconnecting", |m| {
            m.raise(limit());
            m.raise(Situation::Reconnecting);
            m.set_notifications(true, true);
        }),
        (
            "stacking · the ask below the folder trust note, reconnecting",
            |m| {
                m.set_trust(true, Vec::new(), Vec::new());
                m.set_notifications(true, false);
                m.raise(Situation::Reconnecting);
            },
        ),
    ];
    let banners = banner_cells
        .into_iter()
        .map(|(name, set)| {
            let mut model = BannerModel::default();
            set(&mut model);
            debug_assert!(
                model.is_active(SituationKind::NotificationsAsk)
                    || model.is_active(SituationKind::NotificationsBlocked)
            );
            NotificationsBannerCell {
                name: name.to_owned(),
                banners: model.rows().iter().map(|r| r.view(false)).collect(),
            }
        })
        .collect();
    let mut off = GlobalSettings {
        notifications: false,
        ..GlobalSettings::default()
    };
    off.notification_kinds.set(Kind::TurnEnded, false);
    let mut some = GlobalSettings::default();
    some.notification_kinds.set(Kind::Question, false);
    some.notification_sound = false;
    let on = GlobalSettings::default();
    let page_cells = [
        ("macOS not asked yet", &on, PermissionState::NotAsked),
        (
            "allowed · Questions and Play a sound unticked",
            &some,
            PermissionState::Allowed,
        ),
        ("turned off in macOS", &on, PermissionState::TurnedOff),
        (
            "running outside the app bundle",
            &on,
            PermissionState::OutsideBundle,
        ),
        ("state unknown · no line", &on, PermissionState::Unknown),
        (
            "the switch off · turned off in macOS",
            &off,
            PermissionState::TurnedOff,
        ),
    ];
    let pages = page_cells
        .into_iter()
        .map(|(name, settings, permission)| {
            let input = GlobalInputs {
                settings,
                problems: &[],
                scheduler: None,
                data: None,
                cleanup_count: 0,
                cleanup_bytes: 0,
                os: Os::Mac,
                notification_permission: permission,
                fonts: &InstalledFonts::default(),
            };
            let Page::Global { sections, .. } = global_page(&input) else {
                unreachable!("the global page")
            };
            NotificationsPageCell {
                name: name.to_owned(),
                section: sections
                    .into_iter()
                    .find(|s| s.id == GlobalSection::Notifications)
                    .expect("the Notifications section"),
            }
        })
        .collect();
    NotificationsGallery { banners, pages }
}

/// *15 → Row · Start new worktrees from a warm build (0082)* and the dark frame's Linux state for the gallery
/// (T-0301): the frames' samples (shop-web 4.2 GB at 3e1f9a2, 2 h; SHOP-21 copying).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "WarmSwitchGallery")]
pub struct WarmSwitchGallery {
    pub cells: Vec<WarmSwitchCell>,
}

pub fn warm_switch_gallery() -> WarmSwitchGallery {
    use otaq_core::naming::{COMMAND_NAME, data_dir_name};
    use otaq_core::warm::store::WarmStatus;
    let now = 1_760_000_000_000;
    let support = |available: bool, file_system: &str, volume: &str| CloneSupport {
        available,
        file_system: file_system.to_owned(),
        volume: volume.to_owned(),
    };
    let build = |status: WarmStatus| WarmBuildInfo {
        repo: "shop-web".into(),
        path: format!("~/{}/warm/shop-web", data_dir_name()),
        project: Some("SHOP".into()),
        size_bytes: 42 * (1u64 << 30) / 10,
        built_at: Some("3e1f9a2".into()),
        from: Some("SHOP-20".into()),
        kept_at: Some(now - 2 * 3_600_000),
        status,
        entries: vec![],
        limit: None,
    };
    let copying = WarmStatus {
        copying: vec!["SHOP-21".into()],
        clearing: true,
    };
    let apfs = || support(true, "apfs", "/");
    let cells = [
        (
            "on (the default) · no warm build yet",
            "shop-web",
            true,
            apfs(),
            None,
        ),
        (
            "on · with a warm build (turning it off asks)",
            "shop-web",
            true,
            apfs(),
            Some(build(WarmStatus::default())),
        ),
        (
            "off, right after the confirmation",
            "shop-web",
            false,
            apfs(),
            None,
        ),
        (
            "turning off while SHOP-21 is seeded",
            "shop-web",
            false,
            apfs(),
            Some(build(copying)),
        ),
        (
            "handed over · Windows NTFS",
            "shop-web",
            true,
            support(false, "NTFS", "C:"),
            None,
        ),
        (
            "handed over · Linux ext4",
            COMMAND_NAME,
            true,
            support(false, "ext4", "/home"),
            None,
        ),
        (
            "handed over, off · macOS exFAT disk",
            "shop-web",
            false,
            support(false, "exfat", "/Volumes/Untitled"),
            None,
        ),
    ];
    WarmSwitchGallery {
        cells: cells
            .into_iter()
            .map(|(name, project, on, support, build)| {
                let info = ProjectInfo {
                    key: "SHOP".into(),
                    name: project.to_owned(),
                    repo_path: String::new(),
                    base_branch: "main".into(),
                    agents_working: 0,
                    needs_you: 0,
                    flow_setup_since: None,
                    paused_since: None,
                    paused_by: None,
                };
                let settings = ProjectSettings {
                    warm_builds: on,
                    ..ProjectSettings::default()
                };
                let list = WarmList {
                    mode: otaq_core::ticket_env::handoff::HandoffMode::of(&support),
                    support,
                    builds: build.into_iter().collect(),
                };
                WarmSwitchCell {
                    name: name.to_owned(),
                    project: project.to_owned(),
                    row: warm_builds_row(&info, &settings, Some(WarmInputs { list: &list, now })),
                }
            })
            .collect(),
    }
}

/// The problems the service would find in `rows` on `os` (ENV-03), as it sends them: one per marked row.
pub fn env_var_setting_problems(rows: &[EnvVar], os: otaq_core::tools::Os) -> Vec<SettingProblem> {
    env_var_problems(rows, os)
        .into_iter()
        .enumerate()
        .filter_map(|(i, message)| {
            message.map(|message| SettingProblem {
                field: env_var_field(i),
                message,
            })
        })
        .collect()
}

/// One state of the Environment variables row in the gallery.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "EnvVarsGalleryCell")]
pub struct EnvVarsCell {
    pub name: String,
    pub row: Row,
}

/// *15 → Row · Environment variables (0082)* and *… · edit, remove, errors (0082)* for the gallery (T-0300): the
/// frames' sample rows, with the problems as the service finds them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "EnvVarsGallery")]
pub struct EnvVarsGallery {
    pub cells: Vec<EnvVarsCell>,
}

pub fn env_vars_gallery() -> EnvVarsGallery {
    use otaq_core::tools::Os as Platform;
    let shop = [
        EnvVar::new("NEXT_TELEMETRY_DISABLED", "1"),
        EnvVar::new("NODE_OPTIONS", "--max-old-space-size=4096"),
        EnvVar::new("DATABASE_URL", "postgres://localhost:5432/shop_dev"),
    ];
    let with = |extra: &[EnvVar]| [&shop[..], extra].concat();
    let cells = [
        ("empty (the default)", Vec::new(), Platform::Mac),
        ("a few rows", shop.to_vec(), Platform::Mac),
        (
            "invalid name",
            with(&[EnvVar::new("MAX=JOBS", "2")]),
            Platform::Mac,
        ),
        ("empty name", with(&[EnvVar::new("", "2")]), Platform::Mac),
        (
            "duplicate name",
            with(&[EnvVar::new("NODE_OPTIONS", "--inspect")]),
            Platform::Mac,
        ),
        (
            "differing case · Windows",
            with(&[EnvVar::new("node_options", "--inspect")]),
            Platform::Windows,
        ),
        (
            "differing case · macOS and Linux",
            with(&[EnvVar::new("node_options", "--inspect")]),
            Platform::Linux,
        ),
    ];
    EnvVarsGallery {
        cells: cells
            .into_iter()
            .map(|(name, rows, os)| {
                let settings = ProjectSettings {
                    env_vars: rows,
                    ..ProjectSettings::default()
                };
                let problems = env_var_setting_problems(&settings.env_vars, os);
                EnvVarsCell {
                    name: name.to_owned(),
                    row: env_vars_row(&settings, &problems),
                }
            })
            .collect(),
    }
}

/// An edit of the Environment variables row: `env_vars.add` (a new row; an empty one isn't saved, and one with a value
/// but no name is kept, marked "Add a name, or remove the row", so nothing typed is lost: T-0506),
/// `env_vars.remove` (no confirmation: nothing on disk is deleted), `env_vars.<index>.name` / `.value`. Names and
/// values are kept as typed: a space is allowed, and the service marks only what would break the start (ENV-03).
fn edit_env_var(
    s: &mut ProjectSettings,
    field: &str,
    value: &Value,
) -> Option<Result<ProjectChange, String>> {
    let rest = field.strip_prefix("env_vars.")?;
    let rows = &mut s.env_vars;
    let result = match rest {
        "add" => match value {
            Value::Var(v) if v.name.is_empty() && v.value.is_empty() => Ok(ProjectChange::Nothing),
            Value::Var(v) => {
                rows.push(EnvVar::new(&v.name, &v.value));
                Ok(ProjectChange::Settings)
            }
            other => Err(format!("expected a name and a value, got {other:?}")),
        },
        "remove" => value.number().map(|i| {
            if (i as usize) < rows.len() {
                rows.remove(i as usize);
            }
            ProjectChange::Settings
        }),
        _ => {
            let (index, part) = rest.split_once('.')?;
            let i: usize = index.parse().ok()?;
            let text = match value.text() {
                Ok(t) => t.to_owned(),
                Err(e) => return Some(Err(e)),
            };
            match (rows.get_mut(i), part) {
                (None, _) => Err(format!("no environment variable {i}")),
                (Some(row), "name") => {
                    row.name = text;
                    Ok(ProjectChange::Settings)
                }
                (Some(row), "value") => {
                    row.value = text;
                    Ok(ProjectChange::Settings)
                }
                _ => return None,
            }
        }
    };
    Some(result)
}

/// The Remove project confirmation (PRJ-04; *15 → Remove project · confirm*).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "RemoveProjectDialog")]
pub struct RemoveView {
    pub title: String,
    pub deleted_heading: String,
    pub deleted: Vec<String>,
    pub kept_heading: String,
    pub kept: Vec<String>,
    pub agents: Option<String>,
    pub confirm: String,
    pub cancel: String,
}

pub fn remove_dialog(info: &ProjectInfo, tickets: usize) -> RemoveView {
    let t = remove_text(info, tickets);
    RemoveView {
        title: t.title,
        deleted_heading: format!("Deleted from {PRODUCT_NAME}"),
        deleted: t.deleted,
        kept_heading: "Never touched".into(),
        kept: t.kept,
        agents: t.agents,
        confirm: "Remove project".into(),
        cancel: "Cancel".into(),
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "ThemeCredit")]
pub struct CreditView {
    pub project: String,
    /// "→ Catppuccin Latte, Frappé, …".
    pub themes: String,
    pub copyright: String,
    pub license: String,
    pub source: String,
}

/// *15 → Theme credits · dialog* (UI-11, d0069).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "ThemeCreditsDialog")]
pub struct CreditsView {
    pub title: String,
    pub description: String,
    pub credits: Vec<CreditView>,
    pub source_label: String,
    pub mit_title: String,
    pub mit_note: String,
    pub mit_text: Vec<String>,
    pub notices: String,
    pub copy_label: String,
    pub copied_label: String,
    pub done_label: String,
}

pub fn credits() -> CreditsView {
    CreditsView {
        title: "Theme credits".into(),
        description: theme_credits::description(),
        credits: theme_credits::CREDITS
            .iter()
            .map(|c| CreditView {
                project: c.project.to_owned(),
                themes: format!("→ {}", c.themes),
                copyright: c.copyright.to_owned(),
                license: c.license.to_owned(),
                source: c.source.to_owned(),
            })
            .collect(),
        source_label: "Source".into(),
        mit_title: "MIT License — full text".into(),
        mit_note: "Applies to each project above, with its own copyright line".into(),
        mit_text: theme_credits::MIT_TEXT
            .iter()
            .map(|p| (*p).to_owned())
            .collect(),
        notices: theme_credits::notices(),
        copy_label: "Copy notices".into(),
        copied_label: "Copied".into(),
        done_label: "Done".into(),
    }
}

/// Whether `url` is one of the credits' sources (`settings.open_source` opens nothing else).
pub fn is_credit_source(url: &str) -> bool {
    theme_credits::CREDITS.iter().any(|c| c.source == url)
}

/// An edited value as the page sends it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS)]
#[serde(untagged)]
#[ts(rename = "SettingsValue")]
pub enum Value {
    Flag(bool),
    Number(u32),
    Text(String),
    /// A new environment variable row (`env_vars.add`).
    Var(EnvVarValue),
}

/// A new environment variable as the page sends it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS)]
#[ts(rename = "SettingsEnvVarValue")]
pub struct EnvVarValue {
    pub name: String,
    pub value: String,
}

impl Value {
    fn text(&self) -> Result<&str, String> {
        match self {
            Self::Text(t) => Ok(t),
            other => Err(format!("expected a text, got {other:?}")),
        }
    }
    fn flag(&self) -> Result<bool, String> {
        match self {
            Self::Flag(b) => Ok(*b),
            other => Err(format!("expected on or off, got {other:?}")),
        }
    }
    fn number(&self) -> Result<u32, String> {
        match self {
            Self::Number(n) => Ok(*n),
            other => Err(format!("expected a number, got {other:?}")),
        }
    }
}

/// Applies one edit to the global settings. Steppers clamp to their range; a terminal font must be one of `fonts`;
/// the service flags the rest.
pub fn edit_global(
    s: &mut GlobalSettings,
    field: &str,
    value: &Value,
    fonts: &InstalledFonts,
) -> Result<(), String> {
    let clamp = |n: u32, r: std::ops::RangeInclusive<u32>| n.clamp(*r.start(), *r.end());
    match field {
        "agent_limits.enabled" => s.agent_limits = toggled(&s.agent_limits, value.flag()?),
        "agent_limits.max_concurrency" => {
            s.agent_limits.max_concurrency = clamp(value.number()?, MAX_AGENTS)
        }
        "agent_limits.back_pressure" => {
            s.agent_limits.back_pressure = clamp(value.number()?, BACK_PRESSURE_ITEMS)
        }
        "notifications" => s.notifications = value.flag()?,
        "notification_sound" => s.notification_sound = value.flag()?,
        kind if kind.starts_with("notification_kinds.") => {
            let kind = Kind::ALL
                .into_iter()
                .find(|k| notification_kind_row(*k).0 == kind)
                .ok_or_else(|| format!("unknown setting: {kind}"))?;
            s.notification_kinds.set(kind, value.flag()?);
        }
        group if group.starts_with("notification_groups.") => {
            let group = NotificationGroup::ALL
                .into_iter()
                .find(|g| g.row().0 == group)
                .ok_or_else(|| format!("unknown setting: {group}"))?;
            let on = value.flag()?;
            for kind in group.kinds() {
                s.notification_kinds.set(*kind, on);
            }
        }
        "usage_data" => {
            s.usage_data = if value.flag()? {
                UsageData::On
            } else {
                UsageData::Off
            }
        }
        "terminal_font_size" => s.terminal_font_size = clamp(value.number()?, TERMINAL_FONT_SIZES),
        "terminal_font_family" => s.set_terminal_font_family(value.text()?, &fonts.monospace)?,
        "terminal_renderer" => {
            s.terminal_renderer = match value.text()? {
                "webgl" => TerminalRenderer::Webgl,
                "dom" => TerminalRenderer::Dom,
                other => return Err(format!("unknown terminal renderer: {other}")),
            }
        }
        f => return Err(format!("unknown setting: {f}")),
    }
    Ok(())
}

/// What a project edit changes: its settings, or the project itself (name, base branch: `project.update`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectChange {
    Settings,
    Project(ProjectUpdate),
    /// An empty name: nothing to save.
    Nothing,
}

/// Applies one edit to a project's settings, or says which project update it is.
pub fn edit_project(
    s: &mut ProjectSettings,
    field: &str,
    value: &Value,
) -> Result<ProjectChange, String> {
    match field {
        "name" => {
            let name = value.text()?.trim();
            return Ok(if name.is_empty() {
                ProjectChange::Nothing
            } else {
                ProjectChange::Project(ProjectUpdate {
                    name: Some(name.to_owned()),
                    ..ProjectUpdate::default()
                })
            });
        }
        "base_branch" => {
            return Ok(ProjectChange::Project(ProjectUpdate {
                base_branch: Some(value.text()?.to_owned()),
                ..ProjectUpdate::default()
            }));
        }
        "project_notes" => s.project_notes = value.text()?.to_owned(),
        "setup_command" => s.setup_command = value.text()?.to_owned(),
        // Kept as typed while it saves on every key; empty is no Merge check (d0120). The core reads it trimmed.
        "merge_check" => {
            let text = value.text()?;
            s.merge_check = (!text.trim().is_empty()).then(|| text.to_owned());
        }
        // The one app's rows (agents run the app); emptied, the project has no run command again.
        "run_command" | "open_url" => {
            if s.apps.is_empty() {
                s.apps.push(RunApp {
                    name: DEFAULT_APP.into(),
                    command: String::new(),
                    open_url: String::new(),
                    set_by: None,
                });
            }
            let text = value.text()?;
            if field == "run_command" {
                s.apps[0].command = text.to_owned();
            } else {
                s.apps[0].open_url = text.trim().to_owned();
            }
            if s.apps.len() == 1
                && s.apps[0].command.trim().is_empty()
                && s.apps[0].open_url.is_empty()
            {
                s.apps.clear();
            }
        }
        // Another app (agents run the app): the first free name `app2`, `app3`… for the user to rename.
        "apps.add" => {
            let name = (2..)
                .map(|n| format!("{DEFAULT_APP}{n}"))
                .find(|n| s.app(n).is_none())
                .expect("a free name");
            s.apps.push(RunApp {
                name,
                command: String::new(),
                open_url: String::new(),
                set_by: None,
            });
        }
        other if other.starts_with("apps.") => {
            let mut parts = other.splitn(3, '.').skip(1);
            let index: usize = parts
                .next()
                .and_then(|i| i.parse().ok())
                .filter(|i| *i < s.apps.len())
                .ok_or_else(|| format!("no such field: {other}"))?;
            match parts.next() {
                Some("name") => s.apps[index].name = value.text()?.trim().to_owned(),
                Some("command") => s.apps[index].command = value.text()?.to_owned(),
                Some("open_url") => s.apps[index].open_url = value.text()?.trim().to_owned(),
                Some("remove") => {
                    s.apps.remove(index);
                }
                _ => return Err(format!("no such field: {other}")),
            }
        }
        "let_finished_tickets_answer" => s.let_finished_tickets_answer = value.flag()?,
        // Off clears the warm build after saving (the service; the page asked first when there was one).
        "warm_builds" => s.warm_builds = value.flag()?,
        "resource_create" => s.resource_create = value.text()?.to_owned(),
        "resource_remove" => s.resource_remove = value.text()?.to_owned(),
        "resource_template_refresh" => s.resource_template_refresh = value.text()?.to_owned(),
        // One glob per line; empty lines are dropped.
        "schema_paths" => {
            s.schema_paths = value
                .text()?
                .lines()
                .map(str::trim)
                .filter(|line| !line.is_empty())
                .map(str::to_owned)
                .collect()
        }
        "env_files.add" => {
            let file = value.text()?.trim();
            if file.is_empty() {
                return Ok(ProjectChange::Nothing);
            }
            s.env_files.push(file.to_owned());
        }
        "env_files.remove" => {
            let i = value.number()? as usize;
            if i < s.env_files.len() {
                s.env_files.remove(i);
            }
        }
        f => {
            if let Some(change) = edit_env_var(s, f, value) {
                return change;
            }
            if let Some(i) = f
                .strip_prefix("env_files.")
                .and_then(|i| i.parse::<usize>().ok())
            {
                let file = value.text()?.trim().to_owned();
                if i >= s.env_files.len() {
                    return Err(format!("no env file {i}"));
                }
                // An emptied field removes the file.
                if file.is_empty() {
                    s.env_files.remove(i);
                } else {
                    s.env_files[i] = file;
                }
            } else {
                return Err(format!("unknown setting: {f}"));
            }
        }
    }
    Ok(ProjectChange::Settings)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::view_models::project_settings::page as project_page;

    fn info() -> ProjectInfo {
        ProjectInfo {
            key: "BILL".into(),
            name: "billing-api".into(),
            repo_path: "/code/billing-api".into(),
            base_branch: "develop".into(),
            agents_working: 1,
            needs_you: 0,
            flow_setup_since: None,
            paused_since: None,
            paused_by: None,
        }
    }

    fn app_page_rows(s: &ProjectSettings) -> Vec<Row> {
        let Page::Project { sections, .. } = project_page(&ProjectInputs {
            info: &info(),
            settings: s,
            problems: &[],
            global: &GlobalSettings::default(),
            warm: None,
            project_agent: "Strong",
            tool_connected: true,
            delivery: None,
        }) else {
            panic!()
        };
        sections.into_iter().flat_map(|s| s.rows).collect()
    }

    /// Agents run the app (T4): one app (or none) shows Run command and Open URL as before, Add app once there is a command;
    /// several show each app's name, command, Open URL and Remove, then Add app; edits go to the app's fields, and
    /// emptying the one app's command leaves no app.
    #[test]
    fn apps_get_rows_of_their_own_and_edits_by_field() {
        let mut s = ProjectSettings::default();
        let fields = |s: &ProjectSettings| -> Vec<String> {
            app_page_rows(s)
                .into_iter()
                .map(|r| r.field)
                .filter(|f| f == "run_command" || f == "open_url" || f.starts_with("apps."))
                .collect()
        };
        assert_eq!(fields(&s), ["run_command", "open_url"]);
        edit_project(&mut s, "run_command", &Value::Text("pnpm dev".into())).unwrap();
        assert_eq!(s.apps, RunApp::only("pnpm dev", ""));
        assert_eq!(fields(&s), ["run_command", "open_url", "apps.add"]);

        edit_project(&mut s, "apps.add", &Value::Text(String::new())).unwrap();
        assert_eq!(
            s.apps.iter().map(|a| a.name.as_str()).collect::<Vec<_>>(),
            ["app", "app2"]
        );
        assert_eq!(
            fields(&s),
            [
                "apps.0.name",
                "apps.0.command",
                "apps.0.open_url",
                "apps.0.remove",
                "apps.1.name",
                "apps.1.command",
                "apps.1.open_url",
                "apps.1.remove",
                "apps.add"
            ]
        );
        edit_project(&mut s, "apps.0.name", &Value::Text(" web ".into())).unwrap();
        edit_project(&mut s, "apps.1.name", &Value::Text("api".into())).unwrap();
        edit_project(&mut s, "apps.1.command", &Value::Text("node api.js".into())).unwrap();
        edit_project(
            &mut s,
            "apps.1.open_url",
            &Value::Text(" http://localhost:$PORT ".into()),
        )
        .unwrap();
        let rows = app_page_rows(&s);
        let row = |field: &str| rows.iter().find(|r| r.field == field).unwrap();
        assert_eq!(row("apps.1.command").label, "Run command \u{b7} api");
        assert_eq!(
            row("apps.1.name").description,
            format!(
                "Its port is $PORT in its own command and {} in every app\u{2019}s.",
                otaq_core::naming::app_port_env_var("api")
            )
        );
        assert!(matches!(
            &row("apps.1.remove").control,
            Control::Button { label, action: ButtonAction::EditField, destructive: true, .. } if label == "Remove api"
        ));
        assert_eq!(s.apps[1].open_url, "http://localhost:$PORT");
        assert!(edit_project(&mut s, "apps.7.name", &Value::Text("x".into())).is_err());

        edit_project(&mut s, "apps.1.remove", &Value::Text(String::new())).unwrap();
        assert_eq!(
            s.apps.iter().map(|a| a.name.as_str()).collect::<Vec<_>>(),
            ["web"]
        );
        // Back to one app: the rows of before, editing that app.
        assert_eq!(fields(&s), ["run_command", "open_url", "apps.add"]);
        edit_project(&mut s, "run_command", &Value::Text(String::new())).unwrap();
        assert!(s.apps.is_empty());
    }

    fn global(settings: &GlobalSettings) -> Vec<Section<GlobalSection>> {
        let input = GlobalInputs {
            settings,
            problems: &[],
            scheduler: None,
            data: Some(("~/x/data.db".into(), "~/x/logs/".into())),
            cleanup_count: 0,
            cleanup_bytes: 0,
            os: Os::Mac,
            notification_permission: PermissionState::Unknown,
            fonts: &InstalledFonts::default(),
        };
        match global_page(&input) {
            Page::Global { sections, .. } => sections,
            Page::Project { .. } => unreachable!(),
        }
    }

    /// The Notifications section with macOS's permission `permission`.
    fn notifications_rows(settings: &GlobalSettings, permission: PermissionState) -> Vec<Row> {
        let input = GlobalInputs {
            settings,
            problems: &[],
            scheduler: None,
            data: None,
            cleanup_count: 0,
            cleanup_bytes: 0,
            os: Os::Mac,
            notification_permission: permission,
            fonts: &InstalledFonts::default(),
        };
        let Page::Global { sections, .. } = global_page(&input) else {
            unreachable!()
        };
        sections
            .into_iter()
            .find(|s| s.id == GlobalSection::Notifications)
            .expect("the Notifications section")
            .rows
    }

    /// T-0372 (ATT-14, d0090): the status line under the main switch in every macOS state, with its one action;
    /// none where the state isn't read; the same whatever the switch's state.
    #[test]
    fn the_notifications_status_line_follows_macos() {
        let product = PRODUCT_NAME;
        type Line = Option<(String, Option<(&'static str, ButtonAction)>)>;
        let cases: [(PermissionState, Line); 5] = [
            (
                PermissionState::NotAsked,
                Some((
                    "macOS hasn’t been asked yet.".into(),
                    Some(("Allow notifications", ButtonAction::AllowNotifications)),
                )),
            ),
            (
                PermissionState::Allowed,
                Some(("Allowed in macOS.".into(), None)),
            ),
            (
                PermissionState::TurnedOff,
                Some((
                    format!("Turned off in macOS, so {product} can’t notify you."),
                    Some(("Open System Settings", ButtonAction::OpenSystemSettings)),
                )),
            ),
            (
                PermissionState::OutsideBundle,
                Some((
                    format!(
                        "Running outside {product}.app: notifications appear as Script Editor, and {product} can’t \
                         check whether macOS allows them."
                    ),
                    None,
                )),
            ),
            (PermissionState::Unknown, None),
        ];
        for (permission, wanted) in cases {
            for on in [true, false] {
                let settings = GlobalSettings {
                    notifications: on,
                    ..GlobalSettings::default()
                };
                let rows = notifications_rows(&settings, permission);
                let Control::NotificationsSwitch { on: shown, status } = &rows[0].control else {
                    panic!("{:?}", rows[0].control)
                };
                assert_eq!(*shown, on);
                assert_eq!(rows[0].field, "notifications");
                let status = status.as_ref().map(|s| {
                    (
                        s.text.clone(),
                        s.action.as_ref().map(|a| (a.label.as_str(), a.action)),
                    )
                });
                assert_eq!(status, wanted, "{permission:?}, switch on = {on}");
            }
        }
    }

    /// T-0372 (ATT-16, d0090): the three groups, "not while you're looking at that ticket" and Play a sound, indented
    /// under the main switch, disabled and dimmed (not hidden) while it is off. Every group is ticked at first but
    /// the review's, which blocks nothing. A kind can still be edited on its own.
    #[test]
    fn the_notification_checkboxes_follow_the_switch() {
        let mut s = GlobalSettings::default();
        let rows = notifications_rows(&s, PermissionState::Allowed);
        let labels: Vec<&str> = rows[1..].iter().map(|r| r.label.as_str()).collect();
        assert_eq!(
            labels,
            [
                "An agent asks you something",
                "Something broke and waits for you",
                "A ticket is ready for your review",
                "Not while you\u{2019}re looking at that ticket",
                "Play a sound",
            ]
        );
        for row in rows[1..].iter().filter(|r| !r.field.is_empty()) {
            assert_eq!(
                row.control,
                Control::Checkbox {
                    on: row.label != "A ticket is ready for your review",
                    disabled: false,
                    indented: true
                },
                "{}",
                row.label
            );
            assert!(!row.dimmed);
        }

        edit_global(
            &mut s,
            "notification_kinds.question",
            &Value::Flag(false),
            &InstalledFonts::default(),
        )
        .unwrap();
        edit_global(
            &mut s,
            "notification_sound",
            &Value::Flag(false),
            &InstalledFonts::default(),
        )
        .unwrap();
        edit_global(
            &mut s,
            "notification_kinds.ready_for_review",
            &Value::Flag(true),
            &InstalledFonts::default(),
        )
        .unwrap();
        assert!(s.notification_kinds.allows(Kind::ReadyForReview));
        assert!(!s.notification_kinds.allows(Kind::Question));
        assert!(s.notification_kinds.allows(Kind::Permission));
        assert!(!s.notification_sound);
        assert!(
            edit_global(
                &mut s,
                "notification_kinds.nothing",
                &Value::Flag(false),
                &InstalledFonts::default()
            )
            .is_err()
        );
        assert!(
            edit_global(
                &mut s,
                "notification_sound",
                &Value::Text("no".into()),
                &InstalledFonts::default()
            )
            .is_err()
        );
        for kind in Kind::ALL {
            let field = notification_kind_row(kind).0;
            edit_global(
                &mut s,
                field,
                &Value::Flag(false),
                &InstalledFonts::default(),
            )
            .unwrap();
            assert!(!s.notification_kinds.allows(kind), "{field}");
        }

        s.notifications = false;
        let rows = notifications_rows(&s, PermissionState::Allowed);
        assert_eq!(rows.len(), 6, "not hidden");
        for row in &rows[1..] {
            assert!(row.dimmed, "{}", row.label);
            let Control::Checkbox { disabled, .. } = row.control else {
                continue;
            };
            assert!(disabled && row.dimmed, "{}", row.label);
        }
    }

    fn find<'a>(sections: &'a [Section<GlobalSection>], field: &str) -> &'a Row {
        sections
            .iter()
            .flat_map(|s| &s.rows)
            .find(|r| r.field == field)
            .unwrap_or_else(|| panic!("no row {field}"))
    }

    /// App settings are about you and your machine: eight topics, no projects in the nav.
    #[test]
    fn app_settings_has_eight_topics_and_no_projects() {
        let sections = global(&GlobalSettings::default());
        let ids: Vec<GlobalSection> = sections.iter().map(|s| s.id).collect();
        assert_eq!(ids, DRAWN);
        let v = view(Page::Global {
            title: String::new(),
            sections: Vec::new(),
        });
        let labels: Vec<&str> = v.nav.iter().map(|n| n.label.as_str()).collect();
        assert_eq!(
            labels,
            [
                "Agents",
                "Git & pull requests",
                "Limits & usage",
                "Notifications",
                "Appearance",
                "Keyboard",
                "Data",
                "About"
            ]
        );
        assert!(
            v.nav
                .iter()
                .all(|n| matches!(n.target, Target::Global { .. }))
        );
        assert_eq!(
            v.footnote,
            "Project settings live in each project\u{2019}s Settings tab."
        );
        // The agent tools sit above the agents; usage with the agent limit.
        let sections_of = |i: usize| {
            let Target::Global { sections, .. } = &v.nav[i].target;
            sections.clone()
        };
        assert_eq!(
            sections_of(0),
            [GlobalSection::AgentTools, GlobalSection::Agents]
        );
        assert_eq!(sections_of(1), [GlobalSection::Git]);
        assert_eq!(
            sections_of(2),
            [GlobalSection::AgentLimits, GlobalSection::Usage]
        );
        // Other screens open Settings at Git & pull requests by its id.
        assert_eq!(
            serde_json::to_value(GlobalSection::Git).unwrap(),
            serde_json::json!("git")
        );
    }

    /// The agent limit is optional and off by default; usage is drawn as "% left" from the Running popover's usage,
    /// with what a tool's limit does under it.
    #[test]
    fn limits_and_usage_start_with_the_limit_off() {
        let sections = global(&GlobalSettings::default());
        let limits = find(&sections, "agent_limits.enabled");
        assert_eq!(limits.control, Control::Toggle { on: false });
        assert!(find(&sections, "agent_limits.max_concurrency").dimmed);
        let usage = sections
            .iter()
            .find(|s| s.id == GlobalSection::Usage)
            .unwrap();
        assert!(usage.rows.is_empty());
        assert_eq!(usage.note.as_deref(), Some(USAGE_NOTE));
        let mut s = GlobalSettings::default();
        assert!(
            edit_global(
                &mut s,
                "dollar_estimate",
                &Value::Flag(true),
                &InstalledFonts::default()
            )
            .is_err()
        );
    }

    /// Notifications are grouped as the page draws them: a group's checkbox turns every kind in it on or off, and
    /// "ready for your review" starts off.
    #[test]
    fn notifications_come_in_groups() {
        let mut s = GlobalSettings::default();
        let sections = global(&s);
        let on = |field: &str| match &find(&sections, field).control {
            Control::Checkbox { on, .. } => *on,
            other => panic!("{field}: {other:?}"),
        };
        assert!(on("notification_groups.asks"));
        assert!(on("notification_groups.broke"));
        assert!(!on("notification_groups.review"));
        let fonts = InstalledFonts::default();
        edit_global(
            &mut s,
            "notification_groups.broke",
            &Value::Flag(false),
            &fonts,
        )
        .unwrap();
        for kind in NotificationGroup::Broke.kinds() {
            assert!(!s.notification_kinds.allows(*kind));
        }
        assert!(s.notification_kinds.allows(Kind::Question));
        edit_global(
            &mut s,
            "notification_groups.review",
            &Value::Flag(true),
            &fonts,
        )
        .unwrap();
        assert!(s.notification_kinds.allows(Kind::ReadyForReview));
    }

    /// FLW-01: Validation & Auto-merge is no section and no setting any more, global or per project.
    #[test]
    fn validation_and_auto_merge_are_gone() {
        let sections = global(&GlobalSettings::default());
        for field in ["validation", "validation_rounds", "auto_merge"] {
            assert!(
                sections
                    .iter()
                    .flat_map(|s| &s.rows)
                    .all(|r| r.field != field),
                "{field}"
            );
            assert!(
                edit_global(
                    &mut GlobalSettings::default(),
                    field,
                    &Value::Flag(false),
                    &InstalledFonts::default()
                )
                .is_err()
            );
            assert!(
                edit_project(&mut ProjectSettings::default(), field, &Value::Flag(false)).is_err()
            );
        }
    }

    fn project_rows(settings: &ProjectSettings, global: &GlobalSettings) -> Vec<Row> {
        let info = ProjectInfo {
            name: "web-shop".into(),
            ..info()
        };
        let Page::Project { sections, .. } = project_page(&ProjectInputs {
            info: &info,
            settings,
            problems: &[],
            global,
            warm: None,
            project_agent: "Claude Code",
            tool_connected: true,
            delivery: None,
        }) else {
            panic!()
        };
        let ids: Vec<ProjectSection> = sections.iter().map(|s| s.id).collect();
        assert_eq!(
            ids,
            [
                ProjectSection::Flow,
                ProjectSection::Repository,
                ProjectSection::Run,
                ProjectSection::Workspace,
                ProjectSection::ProjectNotes,
                ProjectSection::RemoveProject,
            ]
        );
        sections.into_iter().flat_map(|s| s.rows).collect()
    }

    /// Merge check (d0120): the last of the Repository's rows, built like Setup command (a mono text field saved as
    /// you type), empty by default; a blank value is no Merge check.
    #[test]
    fn the_merge_check_row_ends_the_repository_and_saves_as_typed() {
        let mut s = ProjectSettings::default();
        let rows = project_rows(&s, &GlobalSettings::default());
        let at = rows.iter().position(|r| r.field == "merge_check").unwrap();
        assert_eq!(rows[at + 1].field, "run_command", "the Run card follows");
        let row = &rows[at];
        assert_eq!(row.label, "Merge check \u{b7} optional");
        assert_eq!(
            row.description,
            "Runs on the merged result when you Approve & merge, before it lands, if the base has commits the \
             agent\u{2019}s work didn\u{2019}t have. For example `npm test`."
        );
        let Control::Text {
            value,
            mono,
            multiline,
            ..
        } = &row.control
        else {
            panic!("{:?}", row.control)
        };
        assert_eq!((value.as_str(), *mono, *multiline), ("", true, false));

        for typed in ["n", "npm", "npm test "] {
            assert_eq!(
                edit_project(&mut s, "merge_check", &Value::Text(typed.into())),
                Ok(ProjectChange::Settings)
            );
        }
        assert_eq!(s.merge_check.as_deref(), Some("npm test "), "as typed");
        assert_eq!(s.merge_check(), Some("npm test"));
        let rows = project_rows(&s, &GlobalSettings::default());
        let row = rows.iter().find(|r| r.field == "merge_check").unwrap();
        assert!(matches!(&row.control, Control::Text { value, .. } if value == "npm test "));
        edit_project(&mut s, "merge_check", &Value::Text("  ".into())).unwrap();
        assert_eq!(s.merge_check, None, "cleared");
    }

    /// Ticket resources (M19-d, d0087): empty by default, each field edits its setting, and the schema paths are one
    /// glob per line with empty lines dropped.
    #[test]
    fn ticket_resources_section_edits_each_field() {
        let mut s = ProjectSettings::default();
        let rows = ticket_resource_rows(&s, &[]);
        assert_eq!(rows[0].label, "Per-ticket resources \u{b7} optional");
        let intro = rows[0].description.clone();
        for var in [
            naming::db_name_env_var(),
            naming::db_template_env_var(),
            naming::resource_dir_env_var(),
        ] {
            assert!(intro.contains(&var), "{intro}");
        }
        let fields: Vec<&str> = rows.iter().map(|r| r.field.as_str()).collect();
        assert_eq!(
            fields,
            [
                "resource_create",
                "resource_remove",
                "resource_template_refresh",
                "schema_paths"
            ]
        );
        for row in &rows {
            let Control::Text {
                value,
                placeholder,
                mono,
                problem,
                ..
            } = &row.control
            else {
                panic!("{}", row.field)
            };
            assert_eq!((value.as_str(), placeholder.as_str()), ("", "none"));
            assert!(mono, "{}", row.field);
            assert_eq!(*problem, None);
        }

        for (field, text) in [
            ("resource_create", "createdb -T $T $N"),
            ("resource_remove", "dropdb $N"),
            ("resource_template_refresh", "make template"),
        ] {
            assert_eq!(
                edit_project(&mut s, field, &Value::Text(text.into())),
                Ok(ProjectChange::Settings)
            );
        }
        assert_eq!(s.resource_create, "createdb -T $T $N");
        assert_eq!(s.resource_remove, "dropdb $N");
        assert_eq!(s.resource_template_refresh, "make template");
        edit_project(
            &mut s,
            "schema_paths",
            &Value::Text(" db/migrations/** \n\n prisma/schema.prisma\n".into()),
        )
        .unwrap();
        assert_eq!(s.schema_paths, ["db/migrations/**", "prisma/schema.prisma"]);
        assert!(edit_project(&mut s, "schema_paths", &Value::Flag(true)).is_err());

        let problems = [
            SettingProblem {
                field: "schema_paths".into(),
                message: "a".into(),
            },
            SettingProblem {
                field: "schema_paths".into(),
                message: "b".into(),
            },
            SettingProblem {
                field: "resource_remove".into(),
                message: "c".into(),
            },
        ];
        let rows = ticket_resource_rows(&s, &problems);
        let control = |field: &str| {
            rows.iter()
                .find(|r| r.field == field)
                .unwrap()
                .control
                .clone()
        };
        let Control::Text {
            value,
            multiline,
            problem,
            ..
        } = control("schema_paths")
        else {
            panic!()
        };
        assert_eq!(value, "db/migrations/**\nprisma/schema.prisma");
        assert!(multiline);
        assert_eq!(problem.as_deref(), Some("a; b"));
        let Control::Text { problem, .. } = control("resource_remove") else {
            panic!()
        };
        assert_eq!(problem.as_deref(), Some("c"));
    }

    /// The project's notes edit their setting; no row offers a project value of a global setting any more.
    #[test]
    fn project_notes_edit_and_nothing_resets_to_global() {
        let global = GlobalSettings::default();
        let rows = project_rows(&ProjectSettings::default(), &global);
        let notes = rows.iter().find(|r| r.field == "project_notes").unwrap();
        assert!(matches!(
            &notes.control,
            Control::Text {
                multiline: true,
                ..
            }
        ));
        assert!(rows.iter().all(|r| r.reset.is_none()));
        let mut s = ProjectSettings::default();
        edit_project(&mut s, "project_notes", &Value::Text("x".into())).unwrap();
        assert_eq!(s.project_notes, "x");
    }

    #[test]
    fn limits_dim_while_off_and_say_right_now_while_on() {
        let mut s = GlobalSettings::default();
        let sections = global(&s);
        let max = find(&sections, "agent_limits.max_concurrency");
        assert!(max.dimmed);
        assert!(matches!(
            max.control,
            Control::Stepper {
                disabled: true,
                min: 1,
                max: 50,
                ..
            }
        ));
        edit_global(
            &mut s,
            "agent_limits.enabled",
            &Value::Flag(true),
            &InstalledFonts::default(),
        )
        .unwrap();
        assert!(s.agent_limits.enabled);
        assert!(!find(&global(&s), "agent_limits.max_concurrency").dimmed);
        edit_global(
            &mut s,
            "agent_limits.max_concurrency",
            &Value::Number(99),
            &InstalledFonts::default(),
        )
        .unwrap();
        assert_eq!(s.agent_limits.max_concurrency, 50, "a stepper clamps");
        let status = SchedulerStatus::default();
        let input = GlobalInputs {
            settings: &s,
            problems: &[],
            scheduler: Some(&status),
            data: None,
            cleanup_count: 2,
            cleanup_bytes: 432_013_312,
            os: Os::Linux,
            notification_permission: PermissionState::Unknown,
            fonts: &InstalledFonts::default(),
        };
        let Page::Global { sections, .. } = global_page(&input) else {
            unreachable!()
        };
        assert!(
            sections[3]
                .note
                .as_deref()
                .is_some_and(|n| n.starts_with("Right now:"))
        );
        let cleanup = sections
            .iter()
            .flat_map(|s| &s.rows)
            .find(|r| r.label == "Cleanup")
            .unwrap();
        assert_eq!(
            cleanup.description,
            "2 leftovers \u{b7} 412 MB \u{b7} copies and branches no ticket uses. Nothing is deleted without your \
             click."
        );
        let size = sections
            .iter()
            .flat_map(|s| &s.rows)
            .find(|r| r.field == "terminal_font_size")
            .unwrap();
        assert_eq!(
            size.description,
            "Also Ctrl+Shift+= / Ctrl+Shift+- inside a focused terminal."
        );
    }

    #[test]
    fn appearance_has_the_terminal_font_row_after_the_size_and_it_takes_only_installed_fonts() {
        // The fixture folder's faces, as the desktop reads them: a monospace font, a proportional one, a Nerd Font.
        let fonts = InstalledFonts::from_faces([
            ("Fixture Mono", true),
            ("Fixture Sans", false),
            ("Fixture Nerd Font Mono", true),
        ]);
        let mut s = GlobalSettings::default();
        let page = |s: &GlobalSettings| {
            let input = GlobalInputs {
                settings: s,
                problems: &[],
                scheduler: None,
                data: None,
                cleanup_count: 0,
                cleanup_bytes: 0,
                os: Os::Mac,
                notification_permission: PermissionState::Unknown,
                fonts: &fonts,
            };
            let Page::Global { sections, .. } = global_page(&input) else {
                unreachable!()
            };
            sections
        };
        let sections = page(&s);
        let appearance = sections
            .iter()
            .find(|s| s.id == GlobalSection::Appearance)
            .unwrap();
        let fields: Vec<_> = appearance.rows.iter().map(|r| r.field.as_str()).collect();
        assert_eq!(
            fields,
            [
                "theme",
                "",
                "terminal_font_size",
                "terminal_font_family",
                "terminal_renderer"
            ]
        );
        let row = find(&sections, "terminal_font_family");
        assert_eq!(row.label, "Terminal font");
        assert_eq!(
            row.description,
            "Icons your font lacks come from an installed Nerd Font."
        );
        let Control::Select { choices, value } = &row.control else {
            panic!("{:?}", row.control)
        };
        let labels: Vec<_> = choices.iter().map(|c| c.label.as_str()).collect();
        assert_eq!(
            labels,
            [
                "Geist Mono (default)",
                "Fixture Mono",
                "Fixture Nerd Font Mono"
            ]
        );
        assert_eq!(choices[0].id, "");
        assert_eq!(value, "");

        edit_global(
            &mut s,
            "terminal_font_family",
            &Value::Text("Fixture Mono".into()),
            &fonts,
        )
        .unwrap();
        assert_eq!(s.terminal_font_family.as_deref(), Some("Fixture Mono"));
        let row = find(&page(&s), "terminal_font_family").clone();
        assert!(matches!(&row.control, Control::Select { value, .. } if value == "Fixture Mono"));
        assert_eq!(
            terminal_font::stack(&s, &fonts),
            r#""Fixture Mono", "Fixture Nerd Font Mono", monospace"#
        );
        // A proportional or unknown font is refused; "" is Geist Mono again.
        for refused in ["Fixture Sans", "Nowhere Mono"] {
            let edit = edit_global(
                &mut s,
                "terminal_font_family",
                &Value::Text(refused.into()),
                &fonts,
            );
            assert!(edit.is_err(), "{refused}");
        }
        assert_eq!(s.terminal_font_family.as_deref(), Some("Fixture Mono"));
        edit_global(
            &mut s,
            "terminal_font_family",
            &Value::Text(String::new()),
            &fonts,
        )
        .unwrap();
        assert_eq!(s.terminal_font_family, None);
    }

    #[test]
    fn appearance_has_the_renderer_row_and_edits_apply() {
        let mut s = GlobalSettings::default();
        let sections = global(&s);
        let renderer = find(&sections, "terminal_renderer");
        let Control::Segmented { choices, .. } = &renderer.control else {
            panic!()
        };
        assert_eq!(choices.len(), 2);
        assert_eq!(choices[1].label, "DOM (safer)");
        edit_global(
            &mut s,
            "terminal_renderer",
            &Value::Text("dom".into()),
            &InstalledFonts::default(),
        )
        .unwrap();
        assert_eq!(s.terminal_renderer, TerminalRenderer::Dom);
        assert!(
            edit_global(
                &mut s,
                "terminal_renderer",
                &Value::Text("x".into()),
                &InstalledFonts::default()
            )
            .is_err()
        );
        edit_global(
            &mut s,
            "terminal_font_size",
            &Value::Number(3),
            &InstalledFonts::default(),
        )
        .unwrap();
        assert_eq!(s.terminal_font_size, 8);
        // The workflow skills moved to the agents (AGM-02): no such setting any more.
        assert!(
            edit_global(
                &mut s,
                "workflow_skills.open_pr",
                &Value::Text(" /my-pr ".into()),
                &InstalledFonts::default()
            )
            .is_err()
        );
        edit_global(
            &mut s,
            "usage_data",
            &Value::Flag(true),
            &InstalledFonts::default(),
        )
        .unwrap();
        assert_eq!(s.usage_data, UsageData::On);
        edit_global(
            &mut s,
            "usage_data",
            &Value::Flag(false),
            &InstalledFonts::default(),
        )
        .unwrap();
        assert_eq!(s.usage_data, UsageData::Off);
        assert!(
            edit_global(
                &mut s,
                "usage_data",
                &Value::Text("yes".into()),
                &InstalledFonts::default()
            )
            .is_err()
        );
        edit_global(
            &mut s,
            "notifications",
            &Value::Flag(false),
            &InstalledFonts::default(),
        )
        .unwrap();
        assert!(!s.notifications);
        assert!(
            edit_global(
                &mut s,
                "notifications",
                &Value::Text("no".into()),
                &InstalledFonts::default()
            )
            .is_err()
        );
        assert!(
            edit_global(
                &mut s,
                "nothing",
                &Value::Flag(true),
                &InstalledFonts::default()
            )
            .is_err()
        );
        let data = sections
            .iter()
            .find(|s| s.id == GlobalSection::Data)
            .unwrap();
        assert_eq!(
            data.rows[1].description,
            "Service and session logs. The same ~/x/ folder on macOS, Windows and Linux."
        );
        assert!(
            matches!(&data.rows[0].control, Control::Path { reveal_label, .. } if reveal_label == "Reveal in Finder")
        );
    }

    #[test]
    fn a_project_page_shows_flagged_values_and_routes_name_and_branch() {
        let mut s = ProjectSettings {
            env_files: vec!["appsettings.json".into(), ".env.local".into()],
            apps: vec![RunApp {
                name: DEFAULT_APP.into(),
                command: "dotnet run".into(),
                open_url: "localhost".into(),
                set_by: None,
            }],
            ..ProjectSettings::default()
        };
        let problems = vec![
            SettingProblem {
                field: "env_files".into(),
                message: "Can\u{2019}t find .env.local in ~/code/shop-web. Add it to the repo root, or remove it from this list."
                    .into(),
            },
            SettingProblem {
                field: "apps.0.open_url".into(),
                message: "localhost isn't a URL".into(),
            },
        ];
        let page = project_page(&ProjectInputs {
            info: &info(),
            settings: &s,
            problems: &problems,
            global: &GlobalSettings::default(),
            warm: None,
            project_agent: "Strong",
            tool_connected: true,
            delivery: None,
        });
        let Page::Project {
            title, sections, ..
        } = page
        else {
            panic!()
        };
        assert_eq!(title, "billing-api");
        let rows: Vec<&Row> = sections.iter().flat_map(|s| &s.rows).collect();
        let run = rows.iter().find(|r| r.field == "run_command").unwrap();
        assert_eq!(
            run.button,
            Some(RowButton {
                label: "Ask the agent".into(),
                icon: IconName::Sparkles,
                tooltip: Some("Asks Noyan (Strong) to suggest a run command".into()),
            }),
            "the Project agent's named agent is asked (d0086)"
        );
        // With no agent tool connected there is nobody to ask (RUN-02, d0086).
        let page = project_page(&ProjectInputs {
            info: &info(),
            settings: &s,
            problems: &problems,
            global: &GlobalSettings::default(),
            warm: None,
            project_agent: "Strong",
            tool_connected: false,
            delivery: None,
        });
        let Page::Project { sections, .. } = page else {
            panic!()
        };
        let run = sections
            .iter()
            .flat_map(|s| &s.rows)
            .find(|r| r.field == "run_command")
            .unwrap();
        assert_eq!(run.button, None);
        assert_eq!(run.set_by, None, "the user's own command has no line");

        // A command an agent set (agents run the app): "Set by Dev on SHOP-27 · Undo" under the row.
        let mut by_agent = s.clone();
        by_agent.apps[0].set_by = Some(otaq_core::board::settings::RunCommandSetBy {
            stage: "Dev".into(),
            ticket: "SHOP-27".parse().unwrap(),
            at: 0,
            open_url: false,
        });
        let page = project_page(&ProjectInputs {
            info: &info(),
            settings: &by_agent,
            problems: &problems,
            global: &GlobalSettings::default(),
            warm: None,
            project_agent: "Strong",
            tool_connected: true,
            delivery: None,
        });
        let Page::Project { sections, .. } = page else {
            panic!()
        };
        let run = sections
            .iter()
            .flat_map(|s| &s.rows)
            .find(|r| r.field == "run_command")
            .unwrap();
        let line = run.set_by.as_ref().expect("the set-by line");
        assert_eq!(
            (line.text.as_str(), line.undo),
            ("Set by Dev on SHOP-27", "Undo")
        );
        let url = rows.iter().find(|r| r.field == "open_url").unwrap();
        assert!(
            matches!(&url.control, Control::Text { problem: Some(p), .. } if p == "localhost isn't a URL")
        );
        let env = rows.iter().find(|r| r.field == "env_files").unwrap();
        let Control::EnvFiles { rows: files, .. } = &env.control else {
            panic!()
        };
        assert_eq!(files[0].problem, None);
        assert!(files[1].problem.is_some());

        assert_eq!(
            edit_project(&mut s, "name", &Value::Text("  ".into())),
            Ok(ProjectChange::Nothing)
        );
        assert!(
            matches!(edit_project(&mut s, "base_branch", &Value::Text("main".into())), Ok(ProjectChange::Project(u)) if u.base_branch.as_deref() == Some("main"))
        );
        edit_project(&mut s, "env_files.add", &Value::Text(" .env ".into())).unwrap();
        assert_eq!(s.env_files.last().map(String::as_str), Some(".env"));
        edit_project(&mut s, "env_files.remove", &Value::Number(0)).unwrap();
        assert_eq!(s.env_files[0], ".env.local");
        edit_project(&mut s, "env_files.0", &Value::Text("".into())).unwrap();
        assert_eq!(s.env_files, [".env"], "an emptied file is removed");
        edit_project(&mut s, "let_finished_tickets_answer", &Value::Flag(false)).unwrap();
        assert!(!s.let_finished_tickets_answer);
        assert!(edit_project(&mut s, "workflow_skills.commit", &Value::Text("/c".into())).is_err());
        assert!(edit_project(&mut s, "env_files.9", &Value::Text("x".into())).is_err());
    }

    #[test]
    fn dialogs_and_side_rows_are_worded_here() {
        let r = remove_dialog(&info(), 2);
        assert_eq!(r.deleted_heading, format!("Deleted from {PRODUCT_NAME}"));
        assert_eq!(
            r.agents.as_deref(),
            Some("1 agent is running. Removing stops it first.")
        );
        let c = credits();
        assert_eq!(c.credits.len(), theme_credits::CREDITS.len());
        assert!(c.credits[0].themes.starts_with("→ "));
        assert!(is_credit_source(&c.credits[0].source));
        assert!(!is_credit_source("https://example.com"));
        let json = serde_json::to_value(view(Page::Global {
            title: "General".into(),
            sections: global(&GlobalSettings::default()),
        }))
        .unwrap();
        assert_eq!(json["page"]["page"], "global");
        assert_eq!(json["page"]["sections"][0]["id"], "agent_tools");
        assert_eq!(json["page"]["sections"][1]["id"], "agents");
        assert_eq!(json["page"]["sections"][2]["id"], "git");
        assert_eq!(json["page"]["sections"][3]["id"], "agent_limits");
        assert_eq!(
            json["page"]["sections"][3]["rows"][0]["control"]["kind"],
            "toggle"
        );
        assert_eq!(
            json["nav"][0]["target"],
            serde_json::json!({ "page": "global", "section": "agents", "sections": ["agent_tools", "agents"] })
        );
        let v: Value = serde_json::from_value(serde_json::json!(true)).unwrap();
        assert_eq!(v, Value::Flag(true));
        let v: Value = serde_json::from_value(serde_json::json!(4)).unwrap();
        assert_eq!(v, Value::Number(4));
    }

    fn env_control(settings: &ProjectSettings, problems: &[SettingProblem]) -> Control {
        let Page::Project { sections, .. } = project_page(&ProjectInputs {
            info: &info(),
            settings,
            problems,
            global: &GlobalSettings::default(),
            warm: None,
            project_agent: "Claude Code",
            tool_connected: true,
            delivery: None,
        }) else {
            panic!()
        };
        let rows: Vec<&Row> = sections.iter().flat_map(|s| &s.rows).collect();
        let at = rows.iter().position(|r| r.field == "env_vars").unwrap();
        assert_eq!(
            rows[at - 1].field,
            "setup_command",
            "after the setup command"
        );
        assert_eq!(rows[at + 1].field, "env_files", "before the files to copy");
        let row = rows[at];
        assert_eq!(row.label, "Environment variables");
        assert_eq!(
            row.description,
            format!(
                "Only the ones you set here: set in every agent session, Run and Terminal of this project. You can use ${{{}}} and {PRODUCT_NAME}’s other variables inside a value.",
                naming::db_name_env_var()
            )
        );
        row.control.clone()
    }

    fn env_rows(control: &Control) -> &[EnvVarRow] {
        let Control::EnvVars { rows, .. } = control else {
            panic!("{control:?}")
        };
        rows
    }

    fn vars(rows: &[(&str, &str)]) -> ProjectSettings {
        ProjectSettings {
            env_vars: rows.iter().map(|(n, v)| EnvVar::new(n, v)).collect(),
            ..ProjectSettings::default()
        }
    }

    #[test]
    fn env_vars_empty_shows_the_box_the_hint_and_the_applies_line() {
        let control = env_control(&ProjectSettings::default(), &[]);
        let Control::EnvVars {
            rows,
            hint_example,
            hint,
            empty,
            add_label,
            drop_label,
            applies,
            name_placeholder,
            value_placeholder,
        } = control
        else {
            panic!()
        };
        assert!(rows.is_empty(), "no built-in, read-only or computed rows");
        assert_eq!(hint_example, "CI = true");
        assert_eq!(
            hint,
            "e.g. so tools never wait for input or start watch mode"
        );
        assert_eq!(empty, "No variables. Sessions get your normal environment.");
        assert_eq!(add_label, "Add variable");
        assert_eq!(drop_label, "Remove variable");
        assert_eq!(
            applies,
            "Changes apply to sessions, Runs and Shells started after this."
        );
        assert_eq!(
            (name_placeholder.as_str(), value_placeholder.as_str()),
            ("NAME", "value")
        );
    }

    #[test]
    fn env_vars_rows_are_shown_in_order_in_full_with_their_remove_label() {
        let s = vars(&[
            ("NEXT_TELEMETRY_DISABLED", "1"),
            ("DATABASE_URL", "postgres://localhost:5432/shop_dev"),
        ]);
        let control = env_control(&s, &[]);
        let rows = env_rows(&control);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].name, "NEXT_TELEMETRY_DISABLED");
        assert_eq!(
            rows[1].value, "postgres://localhost:5432/shop_dev",
            "plain, in full"
        );
        assert_eq!(rows[1].remove_label, "Remove DATABASE_URL");
        assert!(rows.iter().all(|r| r.problem.is_none()));
    }

    #[test]
    fn env_vars_each_problem_is_worded_as_the_frames_and_marks_only_its_row() {
        use otaq_core::tools::Os as Platform;
        let s = vars(&[
            ("NODE_OPTIONS", "--max-old-space-size=4096"),
            ("MAX=JOBS", "2"),
            ("", "3"),
            ("BAD\0NAME", "x"),
            ("NODE_OPTIONS", "--inspect"),
            ("1 SPACE OK", "y"),
        ]);
        let problems = env_var_setting_problems(&s.env_vars, Platform::Mac);
        let control = env_control(&s, &problems);
        let rows = env_rows(&control);
        assert_eq!(rows[0].problem, None, "the first NODE_OPTIONS applies");
        assert_eq!(
            rows[1].problem.as_deref(),
            Some("Remove “=” from the name. This row isn’t set until then.")
        );
        assert_eq!(
            rows[2].problem.as_deref(),
            Some("Add a name, or remove the row")
        );
        assert_eq!(rows[2].remove_label, "Remove variable");
        assert_eq!(
            rows[3].problem.as_deref(),
            Some("Remove the null character from the name. This row isn’t set until then.")
        );
        assert_eq!(
            rows[4].problem.as_deref(),
            Some(
                "NODE_OPTIONS is already set above. Only the first row applies; remove one of them."
            )
        );
        assert_eq!(
            rows[5].problem, None,
            "a space or a leading digit is allowed"
        );
    }

    #[test]
    fn env_vars_duplicates_ignore_case_on_windows_only() {
        use otaq_core::tools::Os as Platform;
        let s = vars(&[("NODE_OPTIONS", "a"), ("node_options", "b")]);
        for (os, marked) in [
            (Platform::Windows, true),
            (Platform::Mac, false),
            (Platform::Linux, false),
        ] {
            let problems = env_var_setting_problems(&s.env_vars, os);
            let control = env_control(&s, &problems);
            let rows = env_rows(&control);
            assert_eq!(rows[0].problem, None, "{os:?}");
            assert_eq!(
                rows[1].problem.as_deref(),
                marked.then_some(
                    "node_options is already set above. Only the first row applies; remove one of them."
                ),
                "{os:?}"
            );
        }
    }

    #[test]
    fn env_vars_edits_add_change_and_remove_rows() {
        let mut s = ProjectSettings::default();
        let var = |name: &str, value: &str| {
            Value::Var(EnvVarValue {
                name: name.into(),
                value: value.into(),
            })
        };
        assert_eq!(
            edit_project(&mut s, "env_vars.add", &var("", "")).unwrap(),
            ProjectChange::Nothing,
            "an empty row isn't saved"
        );
        assert!(s.env_vars.is_empty());
        edit_project(&mut s, "env_vars.add", &var("", "1")).unwrap();
        assert_eq!(
            s.env_vars,
            [EnvVar::new("", "1")],
            "a value with no name is kept (and marked), not dropped"
        );
        edit_project(&mut s, "env_vars.remove", &Value::Number(0)).unwrap();
        edit_project(&mut s, "env_vars.add", &var("CI", "true")).unwrap();
        edit_project(&mut s, "env_vars.add", &var("MY LOG ", " a b ")).unwrap();
        assert_eq!(
            s.env_vars[1],
            EnvVar::new("MY LOG ", " a b "),
            "kept as typed"
        );
        edit_project(&mut s, "env_vars.0.value", &Value::Text("false".into())).unwrap();
        edit_project(&mut s, "env_vars.1.name", &Value::Text("".into())).unwrap();
        assert_eq!(
            s.env_vars,
            [EnvVar::new("CI", "false"), EnvVar::new("", " a b ")],
            "an emptied name stays, marked"
        );
        edit_project(&mut s, "env_vars.remove", &Value::Number(0)).unwrap();
        assert_eq!(s.env_vars, [EnvVar::new("", " a b ")]);
        edit_project(&mut s, "env_vars.remove", &Value::Number(7)).unwrap();
        assert_eq!(s.env_vars.len(), 1);
        assert!(edit_project(&mut s, "env_vars.4.name", &Value::Text("X".into())).is_err());
        assert!(edit_project(&mut s, "env_vars.0.other", &Value::Text("X".into())).is_err());
        assert!(edit_project(&mut s, "env_vars.add", &Value::Text("X".into())).is_err());
        let v: Value =
            serde_json::from_value(serde_json::json!({ "name": "CI", "value": "1" })).unwrap();
        assert_eq!(v, var("CI", "1"));
    }

    #[test]
    fn the_env_vars_gallery_has_every_frame_state() {
        let g = env_vars_gallery();
        let problem = |name: &str| {
            let cell = g.cells.iter().find(|c| c.name == name).unwrap();
            env_rows(&cell.row.control)
                .iter()
                .filter_map(|r| r.problem.clone())
                .collect::<Vec<_>>()
        };
        assert!(env_rows(&g.cells[0].row.control).is_empty());
        assert!(problem("a few rows").is_empty());
        assert_eq!(problem("invalid name").len(), 1);
        assert_eq!(problem("duplicate name").len(), 1);
        assert_eq!(problem("differing case · Windows").len(), 1);
        assert!(problem("differing case · macOS and Linux").is_empty());
    }

    const GB: u64 = 1024 * 1024 * 1024;
    const NOW: i64 = 1_760_000_000_000;

    fn shop() -> ProjectInfo {
        ProjectInfo {
            key: "SHOP".into(),
            name: "shop-web".into(),
            ..info()
        }
    }

    fn apfs() -> CloneSupport {
        CloneSupport {
            available: true,
            file_system: "apfs".into(),
            volume: "/".into(),
        }
    }

    fn shop_build(copying: &[&str], clearing: bool) -> WarmBuildInfo {
        WarmBuildInfo {
            repo: "shop-web".into(),
            path: "/data/warm/shop-web".into(),
            project: Some("SHOP".into()),
            size_bytes: 42 * GB / 10,
            built_at: Some("3e1f9a2".into()),
            from: Some("SHOP-20".into()),
            kept_at: Some(NOW - 2 * 3_600_000),
            status: otaq_core::warm::store::WarmStatus {
                copying: copying.iter().map(|t| (*t).to_owned()).collect(),
                clearing,
            },
            entries: vec![],
            limit: None,
        }
    }

    /// The switch row as the project page draws it, checked to sit right after Environment variables.
    fn warm_row(on: bool, support: CloneSupport, builds: Vec<WarmBuildInfo>) -> Row {
        let settings = ProjectSettings {
            warm_builds: on,
            ..ProjectSettings::default()
        };
        let list = WarmList {
            support,
            mode: Default::default(),
            builds,
        };
        let Page::Project { sections, .. } = project_page(&ProjectInputs {
            info: &shop(),
            settings: &settings,
            problems: &[],
            global: &GlobalSettings::default(),
            warm: Some(WarmInputs {
                list: &list,
                now: NOW,
            }),
            project_agent: "Claude Code",
            tool_connected: true,
            delivery: None,
        }) else {
            panic!()
        };
        let rows: Vec<&Row> = sections.iter().flat_map(|s| &s.rows).collect();
        let at = rows.iter().position(|r| r.field == "warm_builds").unwrap();
        assert_eq!(
            rows[at - 1].field,
            "let_finished_tickets_answer",
            "right after Finished tickets"
        );
        assert_eq!(
            rows[at + 1].field,
            "project_notes",
            "the last of the Workspace, before Notes for agents"
        );
        rows[at].clone()
    }

    #[test]
    fn the_warm_build_switch_is_on_by_default_and_asks_only_when_it_would_delete_a_warm_build() {
        let row = warm_row(true, apfs(), Vec::new());
        assert_eq!(row.label, "Start new worktrees from a warm build");
        assert_eq!(
            row.description,
            "New tickets start from the last finished ticket’s build, so their first build only redoes what changed."
        );
        // On, with nothing to delete: turning it off asks nothing.
        assert_eq!(
            row.control,
            Control::WarmBuild {
                on: true,
                disabled: false,
                reason: None,
                clearing: None,
                confirm: None,
            }
        );
        // Another project's warm build isn't this one's.
        let other = WarmBuildInfo {
            project: Some("BILL".into()),
            ..shop_build(&[], false)
        };
        let Control::WarmBuild { confirm, .. } = warm_row(true, apfs(), vec![other]).control else {
            panic!()
        };
        assert_eq!(confirm, None);

        // With a warm build: the confirmation, "up to" the size, and the shared-space line.
        let Control::WarmBuild { on, confirm, .. } =
            warm_row(true, apfs(), vec![shop_build(&[], false)]).control
        else {
            panic!()
        };
        assert!(on);
        let confirm = confirm.expect("asks first");
        assert_eq!(confirm.title, "Clear shop-web’s warm build (up to 4.2 GB)?");
        assert_eq!(
            confirm.text,
            "Turning this off deletes shop-web’s warm build, built at 3e1f9a2, 2 hours ago. New tickets will run \
             setup and build from nothing. Ticket worktrees and your repo aren’t touched."
        );
        assert_eq!(
            confirm.shared,
            "Tickets copied from it share part of this space, so it may free less."
        );
        assert_eq!(
            confirm.line.text,
            "/data/warm/shop-web · up to 4.2 GB (delete)"
        );
        assert_eq!(confirm.line.icon, IconName::Flame);
        assert_eq!(confirm.confirm, "Turn off and clear up to 4.2 GB");

        // Off: turning it on never asks.
        let Control::WarmBuild { on, confirm, .. } = warm_row(false, apfs(), Vec::new()).control
        else {
            panic!()
        };
        assert!(!on);
        assert_eq!(confirm, None);
    }

    #[test]
    fn the_warm_build_switch_says_when_its_clear_waits_for_a_copy() {
        let Control::WarmBuild {
            on,
            clearing,
            confirm,
            ..
        } = warm_row(false, apfs(), vec![shop_build(&["SHOP-21"], true)]).control
        else {
            panic!()
        };
        assert!(!on);
        assert_eq!(
            clearing.as_deref(),
            Some("Clearing… (waiting for SHOP-21’s copy)")
        );
        assert_eq!(confirm, None);
        // A seed running without a Clear waiting: nothing to say.
        let Control::WarmBuild { clearing, .. } =
            warm_row(true, apfs(), vec![shop_build(&["SHOP-21"], false)]).control
        else {
            panic!()
        };
        assert_eq!(clearing, None);
    }

    #[test]
    fn where_the_disk_cant_clone_the_switch_works_and_the_build_is_handed_over() {
        let cannot = |file_system: &str, volume: &str| CloneSupport {
            available: false,
            file_system: file_system.into(),
            volume: volume.into(),
        };
        for support in [
            cannot("NTFS", "C:"),
            cannot("ext4", "/home"),
            cannot("exfat", "/Volumes/Untitled"),
            cannot("", ""),
        ] {
            // The switch is the stored setting, enabled; the description says the build is renamed (M19-b).
            let row = warm_row(true, support.clone(), Vec::new());
            assert_eq!(
                row.description,
                "This disk can’t copy-on-write; the build is handed over to the next ticket."
            );
            assert_eq!(
                row.control,
                Control::WarmBuild {
                    on: true,
                    disabled: false,
                    reason: None,
                    clearing: None,
                    confirm: None,
                }
            );
            let Control::WarmBuild { on, .. } =
                warm_row(false, support.clone(), Vec::new()).control
            else {
                panic!()
            };
            assert!(!on);
            // Turning it off with a build in the stock asks first, as where the disk can clone.
            let Control::WarmBuild { confirm, .. } =
                warm_row(true, support, vec![shop_build(&[], false)]).control
            else {
                panic!()
            };
            assert!(confirm.is_some());
        }
    }

    #[test]
    fn the_warm_build_switch_saves_as_a_flag() {
        let mut s = ProjectSettings::default();
        assert!(s.warm_builds);
        let change = edit_project(&mut s, "warm_builds", &Value::Flag(false)).unwrap();
        assert_eq!(change, ProjectChange::Settings);
        assert!(!s.warm_builds);
        edit_project(&mut s, "warm_builds", &Value::Flag(true)).unwrap();
        assert!(s.warm_builds);
        assert!(edit_project(&mut s, "warm_builds", &Value::Text("off".into())).is_err());

        let gallery = warm_switch_gallery();
        assert_eq!(gallery.cells.len(), 7);
    }
}
