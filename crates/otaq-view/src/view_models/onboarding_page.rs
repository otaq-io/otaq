//! First run and Add project as the Tauri page draws them (T-0233; UI-08, UI-11, SET-02, PRJ-01…PRJ-03; *14 · First
//! run & onboarding*, *02 → No projects yet*): the title bar's step indicator (Welcome · Theme · Tools · Project ·
//! Flow, ONB-10), the Welcome and Theme steps' words, the tools checklist from [`OnboardingModel`] (rows, the permission-mode
//! warning, Continue and why it is off), the Add project form from [`AddProjectModel`] (the onboarding card and the
//! later dialog), and the "No projects yet" board. Every text the GPUI app wrote in its drawing code lives here now.
//! The form says that adding a project lets Claude Code work in its folder, and why the trust wasn't recorded when it
//! wasn't (PRJ-07, d0085; *14 → step 4*, *Claude Code's trust couldn't be recorded*). The Flow step after Add your
//! first project is the flow editor (`flow_editor.rs`, FED-09, d0115) and stays until the flow is settled.

use otaq_core::agent_tools::connected::AgentToolState;
use otaq_core::board::snapshot::ProjectInfo;
use otaq_core::naming;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::icons::IconName;
use crate::key_labels::KeyLabels;
use crate::keymap::KeyAction;
use crate::shell::tilde;
use crate::view_models::add_project::AddProjectModel;
use crate::view_models::agent_tools_page::{self, Place};
use crate::view_models::banners::{self, BannerModel, BannerView, TrustFailure};
use crate::view_models::editor_fields::branch_items;
use crate::view_models::menu::{MenuItem, MenuSearch};
use crate::view_models::onboarding::{OnboardingModel, RowIcon, STEPS, Step, ToolRow};

/// An onboarding step as the page names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename = "OnboardingStepId")]
pub enum StepId {
    Welcome,
    Theme,
    Tools,
    Project,
    Flow,
}

impl StepId {
    pub fn step(self) -> Step {
        match self {
            Self::Welcome => Step::Welcome,
            Self::Theme => Step::Theme,
            Self::Tools => Step::Tools,
            Self::Project => Step::Project,
            Self::Flow => Step::Flow,
        }
    }

    pub const ALL: [Self; 5] = [
        Self::Welcome,
        Self::Theme,
        Self::Tools,
        Self::Project,
        Self::Flow,
    ];
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename = "OnboardingDotState")]
pub enum DotState {
    /// A step behind: a check.
    Done,
    Current,
    Next,
}

/// One step of the indicator: "1 Welcome", or a check once it is behind.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "OnboardingDot")]
pub struct Dot {
    pub number: String,
    pub label: String,
    pub state: DotState,
}

/// The window's title bar during onboarding: the product name and the five steps.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "OnboardingTitleBar")]
pub struct TitleBar {
    pub product: String,
    pub steps: Vec<Dot>,
}

pub fn title_bar(step: StepId) -> TitleBar {
    let current = step.step().index();
    TitleBar {
        product: naming::PRODUCT_NAME.to_owned(),
        steps: STEPS
            .iter()
            .enumerate()
            .map(|(i, label)| Dot {
                number: (i + 1).to_string(),
                label: (*label).to_owned(),
                state: match i.cmp(&current) {
                    std::cmp::Ordering::Less => DotState::Done,
                    std::cmp::Ordering::Equal => DotState::Current,
                    std::cmp::Ordering::Greater => DotState::Next,
                },
            })
            .collect(),
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "OnboardingPoint")]
pub struct Point {
    pub icon: IconName,
    pub title: String,
    pub text: String,
}

/// *1 · Welcome*.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "OnboardingWelcome")]
pub struct Welcome {
    pub title: String,
    pub text: String,
    pub points: Vec<Point>,
    pub foot: String,
    pub button: String,
}

pub fn welcome(keys: &KeyLabels) -> Welcome {
    // Without a key for Next item that needs you the sentence about it is left out.
    let next_key = keys
        .label(KeyAction::NextInboxItem)
        .map(|k| format!(" {k} opens the next."))
        .unwrap_or_default();
    Welcome {
        title: "Run coding agents from a board".to_owned(),
        text: format!(
            "{} turns tickets into real agent sessions (Claude Code, Codex, Copilot or OpenCode) \u{2014} one per ticket, each in its own git worktree. You watch and type into them from an embedded terminal, review the diff, and merge or open a PR.",
            naming::PRODUCT_NAME
        ),
        points: vec![
            Point {
                icon: IconName::SquareTerminal,
                title: "Real sessions".to_owned(),
                text: format!(
                    "Each agent is its tool's own CLI, not a wrapper. Attach from any terminal with {} attach SHOP-18.",
                    naming::COMMAND_NAME
                ),
            },
            Point {
                icon: IconName::Inbox,
                title: "Needs you".to_owned(),
                text: format!(
                    "Permission prompts, questions and reviews wait in one strip above the board.{next_key}"
                ),
            },
            Point {
                icon: IconName::ShieldCheck,
                title: "You stay in control".to_owned(),
                text: "Nothing starts, retries or deletes without a click. Your repos are never written to by settings."
                    .to_owned(),
            },
        ],
        foot: "Takes about a minute".to_owned(),
        button: "Choose a theme".to_owned(),
    }
}

/// *2 · Theme* around the picker (d0070).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "OnboardingTheme")]
pub struct ThemeStep {
    pub title: String,
    pub text: String,
    pub foot: String,
    pub button: String,
}

pub fn theme_step() -> ThemeStep {
    ThemeStep {
        title: "Pick a look".to_owned(),
        text: format!(
            "Pick one theme. {} uses it all the time, and the board, panels and terminal recolor as soon as you click.",
            naming::PRODUCT_NAME
        ),
        foot: "Change it anytime in Settings \u{2192} Appearance.".to_owned(),
        button: "Check my tools".to_owned(),
    }
}

/// The permission-mode warning on the tools step (d0011): a link to the docs in the middle of its text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "PermissionWarningView")]
pub struct PermissionWarning {
    pub title: String,
    pub before: String,
    pub link: String,
    pub after: String,
    /// The close button's name.
    pub close: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename = "ToolRowLook")]
pub enum RowLook {
    Found,
    MissingRequired,
    MissingOptional,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "ToolRowView")]
pub struct ToolRowView {
    /// "git", "gh", "az": what `tools.terminal` takes, and whose terminal shows under the row.
    pub tool: String,
    pub name: String,
    /// "required" or "optional".
    pub badge: String,
    pub look: RowLook,
    pub icon: IconName,
    pub status: String,
    pub explanation: Option<String>,
    /// The install or sign-in command, copyable.
    pub command: Option<String>,
    /// Install / Add extension / Sign in…: runs `command` in a terminal under the row.
    pub button: Option<ToolButton>,
    /// "Needs Homebrew first (brew.sh)…".
    pub note: Option<String>,
}

/// What a checklist row's button asks the service to run (`tools.terminal`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename = "ToolActionKind")]
pub enum ToolActionKind {
    Install,
    Login,
}

/// A checklist row's Install / Add extension / Sign in… button.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "ToolButton")]
pub struct ToolButton {
    /// The tool as `tools.terminal` takes it: "git", "gh", "az".
    pub tool: String,
    pub action: ToolActionKind,
    pub label: String,
    /// The terminal's title while it runs: "Installing gh…".
    pub running: String,
}

/// The words of the terminal under a row (the checklist and the agent tools, onboarding and Settings).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "ToolTerminalTexts")]
pub struct ToolTerminalTexts {
    /// Hides a running terminal; it keeps running, and the row's button shows it again.
    pub hide: String,
    pub close: String,
    pub retry: String,
    /// The command ended; the tools are checked again.
    pub checking: String,
    /// The check after it still offers the same fix (failed, cancelled).
    pub not_done: String,
    /// The check after it found the fix done: the terminal closes by itself.
    pub done: String,
}

pub fn tool_terminal_texts() -> ToolTerminalTexts {
    ToolTerminalTexts {
        hide: "Hide".to_owned(),
        close: "Close".to_owned(),
        retry: "Retry".to_owned(),
        checking: "Ended \u{b7} checking again\u{2026}".to_owned(),
        not_done: "Not done yet \u{2014} see the output above".to_owned(),
        done: "Done".to_owned(),
    }
}

/// A checklist row's button, as the page sends it back.
pub fn tool_button(row: &ToolRow) -> Option<ToolButton> {
    row.button.as_ref().map(|b| ToolButton {
        tool: row.tool.command().to_owned(),
        action: match b.action {
            otaq_core::tools::ToolAction::Install => ToolActionKind::Install,
            otaq_core::tools::ToolAction::Login => ToolActionKind::Login,
        },
        label: b.label.to_owned(),
        running: b.running.clone(),
    })
}

/// The checklist's intro (onboarding and Settings).
pub fn tools_intro() -> String {
    format!(
        "{} uses the tools already on your machine and never updates them. Install and Sign in\u{2026} run the \
         command shown, in a terminal here \u{2014} nothing installs without your click.",
        naming::PRODUCT_NAME
    )
}

/// The agent tools on the Tools step (*14* `y2FCwP`, `z8OVF`; SET-02 `[M23]`, d0086): the same rows as Settings →
/// Agent tools without Disconnect…, and the rule "at least one connected".
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "OnboardingAgentTools")]
pub struct AgentTools {
    /// "AGENT TOOLS · AT LEAST ONE".
    pub heading: String,
    /// "3 connected", "none connected", "2 connected · 1 hidden".
    pub count: String,
    pub rows: Vec<agent_tools_page::Row>,
    /// With none connected, above the list: "No agent tool is connected. …".
    pub blocked: Option<String>,
    /// The … button's name.
    pub menu_label: String,
}

/// *2 · Tool checklist*, *gh and az missing · non-blocking*, *14* `y2FCwP` / `z8OVF` (the agent tools).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "OnboardingTools")]
pub struct Tools {
    pub title: String,
    pub text: String,
    /// Claude Code's dismissable warning above the list, from a service whose tools carry no warning of their
    /// own; since M23 each agent tool's row has its own ([`agent_tools_page::Row::warning`]).
    pub warning: Option<PermissionWarning>,
    pub agent_tools: AgentTools,
    /// "OTHER TOOLS": git, gh and az.
    pub other_heading: String,
    pub rows: Vec<ToolRowView>,
    /// While no report is in: "Checking your tools…".
    pub waiting: Option<String>,
    pub error: Option<String>,
    pub recheck: String,
    pub continue_label: String,
    pub continue_enabled: bool,
    /// "Install claude to continue".
    pub continue_reason: Option<String>,
    /// The user has projects already (Check tools from the board): Continue ends here, no Project step.
    pub has_projects: bool,
    pub copy: String,
    pub terminal: ToolTerminalTexts,
    /// Under the list once a tool is connected (*14* `ODRG2`, M22): the starter entry and where to make others.
    pub agents_line: Option<String>,
}

/// The checklist's place while the check runs.
pub const CHECKING: &str = "Checking your tools\u{2026}";

pub fn tools(model: &OnboardingModel, has_projects: bool) -> Tools {
    let state = model.continue_state();
    let statuses = model
        .report
        .as_ref()
        .map(|r| r.agent_tools.as_slice())
        .unwrap_or_default();
    let connected = statuses.iter().filter(|s| s.state.is_connected()).count();
    let hidden = statuses
        .iter()
        .filter(|s| matches!(s.state, AgentToolState::Hidden { .. }))
        .count();
    let none_connected = model.report.is_some() && connected == 0;
    let mut count = match connected {
        0 => "none connected".to_owned(),
        n => format!("{n} connected"),
    };
    if hidden > 0 {
        count.push_str(&format!(" \u{b7} {hidden} hidden"));
    }
    Tools {
        title: "Tools".to_owned(),
        text: tools_intro(),
        warning: model.show_permission_warning().then(|| PermissionWarning {
            title: "Your Claude Code settings ask for permission prompts".to_owned(),
            before: "With 5\u{2013}10 agents, prompts will stop agents often. Consider allowlists or auto mode ("
                .to_owned(),
            link: "Claude Code docs \u{2197}".to_owned(),
            after: format!(
                "). {} won\u{2019}t change your settings.",
                naming::PRODUCT_NAME
            ),
            close: "Close".to_owned(),
        }),
        agent_tools: AgentTools {
            heading: "AGENT TOOLS \u{b7} AT LEAST ONE".to_owned(),
            count,
            rows: statuses
                .iter()
                .map(|s| agent_tools_page::row(s, Place::Onboarding, none_connected))
                .collect(),
            blocked: none_connected.then(|| {
                "No agent tool is connected. Install and sign in to at least one; agents can\u{2019}t start without it."
                    .to_owned()
            }),
            menu_label: "More actions".to_owned(),
        },
        other_heading: "OTHER TOOLS".to_owned(),
        rows: model
            .rows()
            .into_iter()
            .map(|r| {
                let (look, icon) = match r.icon {
                    RowIcon::Found => (RowLook::Found, IconName::CircleCheck),
                    RowIcon::MissingRequired => (RowLook::MissingRequired, IconName::CircleX),
                    RowIcon::MissingOptional => (RowLook::MissingOptional, IconName::CircleMinus),
                };
                ToolRowView {
                    tool: r.tool.command().to_owned(),
                    button: tool_button(&r),
                    note: r.note.clone(),
                    name: r.name.to_owned(),
                    badge: if r.required { "required" } else { "optional" }.to_owned(),
                    look,
                    icon,
                    status: r.status,
                    explanation: r.explanation,
                    command: r.command,
                }
            })
            .collect(),
        waiting: model.report.is_none().then(|| {
            if model.checking {
                CHECKING
            } else {
                "No result yet"
            }
            .to_owned()
        }),
        error: model.error.as_ref().map(|e| format!("Check failed: {e}")),
        recheck: "Re-check".to_owned(),
        continue_label: "Continue".to_owned(),
        continue_enabled: state.enabled,
        continue_reason: state.reason,
        has_projects,
        copy: "Copy".to_owned(),
        terminal: tool_terminal_texts(),
        agents_line: state.enabled.then(agents_line),
    }
}

/// The tools step's line about the starter entry (*14* `ODRG2`, `z8OVF`; d0086), after "You can connect more later in
/// Settings → Agent tools.".
pub fn agents_line() -> String {
    let tool = super::tool_avatar::AvatarTool::from(super::agents::new_entry_tool()).name();
    format!(
        "You can connect more later in Settings \u{2192} Agent tools. {} starts you with one agent, {tool} \u{b7} {} ({tool} picks its own model). Make others, like Strong or Light, in Settings \u{2192} Agents.",
        naming::PRODUCT_NAME,
        super::agents::TOOL_DEFAULT,
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename = "FolderLook")]
pub enum FolderLook {
    /// No folder yet.
    Empty,
    /// Being checked.
    Checking,
    /// A git repository, not added yet: the green outline.
    Valid,
    /// Refused: the red outline and the error box.
    Refused,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "AddProjectError")]
pub struct FolderErrorView {
    pub title: String,
    pub detail: String,
}

/// A detected value: "Name shop-web".
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "AddProjectDetected")]
pub struct Detected {
    pub icon: IconName,
    pub label: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "AddProjectKey")]
pub struct KeyField {
    pub label: String,
    pub note: String,
    pub value: String,
    pub placeholder: String,
    /// "Used in ticket IDs like SHOP-18. Can’t be changed later." while the key is fine.
    pub hint: Option<String>,
    pub error: Option<String>,
}

/// The base-branch field: the detected branch, changed before adding by picking another of the repository's local
/// or remote branches from a searchable dropdown (PRJ-03).
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(rename = "AddProjectBase")]
pub struct BaseField {
    pub icon: IconName,
    pub label: String,
    pub value: String,
    /// The dropdown's rows: LOCAL, then REMOTE branches, the current one checked.
    pub items: Vec<MenuItem>,
    pub search: MenuSearch,
}

/// The Add project form: the onboarding card (*3 · Add project · valid*, *Not a git repo*, *Already added*) and the
/// dialog of a later project (*Adding a second project later*).
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(rename = "AddProjectView")]
pub struct AddProjectPage {
    pub first_run: bool,
    pub title: String,
    /// The onboarding card's line under its title.
    pub text: Option<String>,
    /// The folder as shown ("~/code/shop-web"), or "No folder chosen yet".
    pub folder: String,
    pub folder_look: FolderLook,
    /// "Choose folder…", or "Change…" in the dialog once a folder is picked.
    pub choose: String,
    pub choose_quiet: bool,
    /// The picker's title.
    pub picker_title: String,
    pub error: Option<FolderErrorView>,
    /// The repository's top folder, sent with Add.
    pub repo_path: Option<String>,
    /// The onboarding card's detected name.
    pub name: Option<Detected>,
    pub base: Option<BaseField>,
    pub key: Option<KeyField>,
    /// The onboarding card's last line.
    pub footnote: Option<String>,
    pub add: String,
    pub adding: String,
    pub can_add: bool,
    pub add_error: Option<String>,
    pub cancel: String,
    /// Under the folder once one is picked (PRJ-07, d0086), naming the connected tools that keep folder trust:
    /// "Adding a project also lets Claude Code, Codex and GitHub Copilot work in this folder (their hooks and
    /// settings will run)."
    pub trust_line: Option<String>,
    /// The project was added but some tools' trust wasn't recorded: why, one sentence per tool, in the form's
    /// inline error style. The form then waits for [`Self::continue_label`] instead of Add.
    pub trust_error: Option<String>,
    pub continue_label: String,
}

pub const PICKER_TITLE: &str = "Add project";

pub fn add_project(
    model: &AddProjectModel,
    projects: &[ProjectInfo],
    first_run: bool,
) -> AddProjectPage {
    let folder_look = if model.probing {
        FolderLook::Checking
    } else if model.folder_error.is_some() {
        FolderLook::Refused
    } else if model.probe.is_some() {
        FolderLook::Valid
    } else {
        FolderLook::Empty
    };
    let picked = model.folder.is_some();
    let probe = model.probe.as_ref();
    let key_error = probe.and_then(|_| model.key_error(projects));
    AddProjectPage {
        first_run,
        title: if first_run {
            "Add your first project"
        } else {
            "Add project"
        }
        .to_owned(),
        text: first_run.then(|| {
            format!(
                "Pick a folder that is a git repository. {} keeps everything in its own database; nothing is written into the repo.",
                naming::PRODUCT_NAME
            )
        }),
        folder: model
            .folder
            .as_deref()
            .map_or_else(|| "No folder chosen yet".to_owned(), tilde),
        folder_look,
        choose: if !first_run && picked {
            "Change\u{2026}"
        } else {
            "Choose folder\u{2026}"
        }
        .to_owned(),
        choose_quiet: !first_run && picked,
        picker_title: PICKER_TITLE.to_owned(),
        error: model.folder_error.as_ref().map(|e| FolderErrorView {
            title: e.title.clone(),
            detail: e.detail.clone(),
        }),
        repo_path: probe.map(|p| p.repo_path.clone()),
        name: probe.filter(|_| first_run).map(|p| Detected {
            icon: IconName::Tag,
            label: "Name".to_owned(),
            value: p.name.clone(),
        }),
        base: probe.map(|p| BaseField {
            icon: IconName::GitBranch,
            label: "Base branch".to_owned(),
            value: model.base_branch.clone(),
            items: branch_items(&p.branches.local, &p.branches.remote, &model.base_branch),
            search: MenuSearch::branches(),
        }),
        key: probe.map(|_| KeyField {
            label: "Project key".to_owned(),
            note: "prefilled from the repo name".to_owned(),
            value: model.key.clone(),
            placeholder: "KEY".to_owned(),
            hint: key_error.is_none().then(|| model.key_hint()),
            error: key_error,
        }),
        footnote: first_run.then(|| {
            "Setup command, run command and env files can be added later in project settings."
                .to_owned()
        }),
        add: "Add project".to_owned(),
        adding: "Adding\u{2026}".to_owned(),
        can_add: model.can_add(projects),
        add_error: model.add_error.clone(),
        cancel: "Cancel".to_owned(),
        trust_line: picked.then(|| model.trust_line()).flatten(),
        trust_error: Some(model.trust_errors().join(" ")).filter(|e| !e.is_empty()),
        continue_label: "Continue".to_owned(),
    }
}

/// One form of the folder trust gallery.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(rename = "TrustFormCell")]
pub struct TrustFormCell {
    pub name: String,
    pub form: AddProjectPage,
}

/// One banner of the folder trust gallery.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "TrustBannerCell")]
pub struct TrustBannerCell {
    pub name: String,
    pub banners: Vec<BannerView>,
}

/// The folder trust in the component gallery (T-0306; *14 → step 4*, *11 → 4c*): the form with the line (onboarding
/// card and dialog), the form after a failed trust, the note and the failed warning for one and two projects, and
/// the warning and the note around the permission warning.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(rename = "TrustGallery")]
pub struct TrustGallery {
    pub forms: Vec<TrustFormCell>,
    pub banners: Vec<TrustBannerCell>,
}

pub fn trust_gallery() -> TrustGallery {
    use otaq_core::agent_tools::claude::trust::TrustError;
    use otaq_core::board::projects::{Branches, FolderProbe};
    let [claude, codex, copilot, _] = super::agent_tools_page::fixtures::tools();
    let picked = || {
        let mut m = AddProjectModel {
            trust_tools: vec![claude],
            ..AddProjectModel::default()
        };
        m.pick("~/code/pocket-notes".into());
        m.probed(
            Ok(FolderProbe {
                repo_path: "/Users/me/code/pocket-notes".into(),
                name: "pocket-notes".into(),
                suggested_key: "NOTE".into(),
                base_branch: "main".into(),
                branches: Branches::default(),
            }),
            &[],
            &KeyLabels::default(),
        );
        m
    };
    let mut failed = picked();
    failed.added_untrusted(vec![(claude, TrustError::Unwritable.reason().into())]);
    let three = AddProjectModel {
        trust_tools: vec![claude, codex, copilot],
        ..picked()
    };
    let mut codex_failed = three.clone();
    codex_failed.added_untrusted(vec![(
        codex,
        "Codex\u{2019}s config file couldn\u{2019}t be written".into(),
    )]);
    let forms = [
        ("onboarding · folder picked · the line", picked(), true),
        ("dialog · folder picked · the line", picked(), false),
        (
            "onboarding · trust couldn’t be recorded",
            failed.clone(),
            true,
        ),
        ("dialog · trust couldn’t be recorded", failed, false),
        ("onboarding · three tools keep folder trust", three, true),
        (
            "onboarding · Codex’s trust couldn’t be recorded",
            codex_failed,
            true,
        ),
    ]
    .into_iter()
    .map(|(name, model, first_run)| TrustFormCell {
        name: name.to_owned(),
        form: add_project(&model, &[], first_run),
    })
    .collect();
    let failure = |project: &str, reason: &str| TrustFailure {
        project: project.to_owned(),
        reason: reason.to_owned(),
    };
    let note = failure("NOTE", TrustError::Unwritable.reason());
    let shop = failure("SHOP", TrustError::HomeFolder.reason());
    let cells: [(&str, bool, Vec<TrustFailure>, bool); 5] = [
        ("the one-time note", true, Vec::new(), false),
        (
            "trust couldn’t be recorded · one project",
            false,
            vec![note.clone()],
            false,
        ),
        (
            "trust couldn’t be recorded · two projects (hover for the reasons)",
            false,
            vec![note.clone(), shop],
            false,
        ),
        (
            "stacking · the warning above the permission warning",
            false,
            vec![note],
            true,
        ),
        (
            "stacking · the note below the permission warning",
            true,
            Vec::new(),
            true,
        ),
    ];
    let banners = cells
        .into_iter()
        .map(|(name, pending, failed, permission)| {
            let mut model = BannerModel::default();
            if permission {
                model.raise(banners::Situation::PermissionWarning);
            }
            model.set_trust(pending, Vec::new(), failed);
            TrustBannerCell {
                name: name.to_owned(),
                banners: model.rows().iter().map(|r| r.view(false)).collect(),
            }
        })
        .collect();
    TrustGallery { forms, banners }
}

/// One Tools step of the gallery.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "OnboardingToolsGalleryCell")]
pub struct ToolsGalleryCell {
    pub name: String,
    pub tools: Tools,
}

/// The Tools step with agent tools in the component gallery (T-0365; *14* `y2FCwP`, `z8OVF`): the sample with three
/// connected, none connected, one connected, and a hidden tool beside a warning and what Otaq can see.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "OnboardingToolsGallery")]
pub struct ToolsGallery {
    pub cells: Vec<ToolsGalleryCell>,
}

pub fn tools_gallery() -> ToolsGallery {
    use agent_tools_page::fixtures::{connected, sample, status, tools as agent_tools};
    use otaq_core::tools::{PermissionCheck, PermissionVerdict, Tool, ToolStatus, ToolsReport};
    let [claude, codex, copilot, opencode] = agent_tools();
    let found = |tool: Tool, version: &str, logged_in: Option<bool>| ToolStatus {
        tool,
        required: tool.required(),
        found: true,
        version: Some(version.to_owned()),
        logged_in,
        extension_installed: (tool == Tool::Az).then_some(true),
        install_command: None,
        login_command: None,
        missing_package_manager: None,
    };
    let step = |statuses| {
        let mut m = OnboardingModel::new(false);
        m.start_check();
        m.finish_check(Ok(ToolsReport {
            tools: vec![
                found(Tool::Git, "2.46.0", None),
                found(Tool::Gh, "2.62.0", Some(true)),
                found(Tool::Az, "2.67.0", Some(true)),
            ],
            permission: PermissionCheck {
                verdict: PermissionVerdict::NotPrompting,
                mode: None,
                source: None,
            },
            agent_tools: statuses,
        }));
        tools(&m, false)
    };
    let claude_connected = || status(claude, connected("Claude Code 2.1.301", None));
    let missing = |tool| status(tool, AgentToolState::NotInstalled);
    let mut warned = status(codex, connected("0.157.1", Some("signed in with ChatGPT")));
    warned.permission_warning = Some("Your Codex settings ask for permission prompts".into());
    let cells = [
        ("3 · Tools · sample (three agent tools connected)", sample()),
        (
            "None connected · blocking",
            agent_tools().into_iter().map(missing).collect(),
        ),
        (
            "One connected · continue",
            vec![
                claude_connected(),
                missing(codex),
                missing(copilot),
                missing(opencode),
            ],
        ),
        (
            "Hidden, warnings, what {} can see",
            vec![
                claude_connected(),
                warned,
                status(
                    opencode,
                    AgentToolState::Hidden {
                        version: Some("2.0.17".into()),
                    },
                ),
            ],
        ),
    ]
    .into_iter()
    .map(|(name, statuses)| ToolsGalleryCell {
        name: name.replace("{}", naming::PRODUCT_NAME),
        tools: step(statuses),
    })
    .collect();
    ToolsGallery { cells }
}

/// *02 → No projects yet*: the board of an app with no project (onboarding skipped, or every project removed).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "NoProjectsView")]
pub struct NoProjects {
    pub title: String,
    pub text: String,
    pub add: String,
    pub check: String,
    pub drop: String,
}

pub fn no_projects() -> NoProjects {
    NoProjects {
        title: "No projects yet".to_owned(),
        text: format!(
            "A project is any folder that is a git repository. {} never writes into it \u{2014} all settings stay in the app.",
            naming::PRODUCT_NAME
        ),
        add: "Add project folder\u{2026}".to_owned(),
        check: "Check tools".to_owned(),
        drop: "or drop a folder anywhere in this window".to_owned(),
    }
}

/// Everything onboarding says that doesn't depend on the machine, asked once.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "OnboardingTexts")]
pub struct Texts {
    pub welcome: Welcome,
    pub theme: ThemeStep,
    /// The title bar at each step, in [`StepId::ALL`]'s order.
    pub title_bars: Vec<TitleBar>,
    pub no_projects: NoProjects,
    /// The tools list while the check runs.
    pub checking: String,
}

pub fn texts(keys: &KeyLabels) -> Texts {
    Texts {
        welcome: welcome(keys),
        theme: theme_step(),
        title_bars: StepId::ALL.iter().map(|s| title_bar(*s)).collect(),
        no_projects: no_projects(),
        checking: CHECKING.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use otaq_core::board::projects::{Branches, FolderProbe};
    use otaq_core::tools::{PermissionCheck, PermissionVerdict, Tool, ToolStatus, ToolsReport};

    use super::*;
    use crate::key_labels::fixtures;

    fn status(tool: Tool, found: bool) -> ToolStatus {
        ToolStatus {
            tool,
            required: tool.required(),
            found,
            version: found.then(|| "1.0".to_owned()),
            logged_in: None,
            extension_installed: None,
            install_command: (!found).then(|| tool.install_command().to_owned()),
            login_command: None,
            missing_package_manager: None,
        }
    }

    fn checked(claude: bool, verdict: PermissionVerdict) -> OnboardingModel {
        let mut m = OnboardingModel::new(false);
        m.start_check();
        let connected: &[otaq_core::agent_tools::Tool] = if claude {
            &[otaq_core::agent_tools::Tool::Claude]
        } else {
            &[]
        };
        m.finish_check(Ok(ToolsReport {
            tools: vec![
                status(Tool::Git, true),
                status(Tool::Gh, false),
                status(Tool::Az, false),
            ],
            permission: PermissionCheck {
                verdict,
                mode: None,
                source: None,
            },
            agent_tools: crate::view_models::onboarding::tests::agent_tools(connected),
        }));
        m
    }

    #[test]
    fn the_indicator_checks_the_steps_behind() {
        let bar = title_bar(StepId::Tools);
        assert_eq!(bar.product, naming::PRODUCT_NAME);
        let states: Vec<DotState> = bar.steps.iter().map(|d| d.state).collect();
        assert_eq!(
            states,
            [
                DotState::Done,
                DotState::Done,
                DotState::Current,
                DotState::Next,
                DotState::Next
            ]
        );
        let labels: Vec<(&str, &str)> = bar
            .steps
            .iter()
            .map(|d| (d.number.as_str(), d.label.as_str()))
            .collect();
        assert_eq!(
            labels,
            [
                ("1", "Welcome"),
                ("2", "Theme"),
                ("3", "Tools"),
                ("4", "Project"),
                ("5", "Flow")
            ]
        );
        // s14-12: on the Flow step the four before it are checked, and Flow is the last step.
        let flow = title_bar(StepId::Flow);
        let states: Vec<DotState> = flow.steps.iter().map(|d| d.state).collect();
        assert_eq!(
            states,
            [
                DotState::Done,
                DotState::Done,
                DotState::Done,
                DotState::Done,
                DotState::Current
            ]
        );
        assert_eq!(texts(&fixtures::base()).title_bars.len(), 5);
    }

    #[test]
    fn welcome_and_theme_words_match_the_frames() {
        let w = welcome(&fixtures::base());
        assert_eq!(w.title, "Run coding agents from a board");
        assert_eq!(
            w.text,
            format!(
                "{} turns tickets into real agent sessions (Claude Code, Codex, Copilot or OpenCode) — one per ticket, each in its own git worktree. You watch and type into them from an embedded terminal, review the diff, and merge or open a PR.",
                naming::PRODUCT_NAME
            )
        );
        assert_eq!(w.points.len(), 3);
        assert_eq!(
            w.points[0].text,
            format!(
                "Each agent is its tool's own CLI, not a wrapper. Attach from any terminal with {} attach SHOP-18.",
                naming::COMMAND_NAME
            )
        );
        assert!(
            w.points[1]
                .text
                .ends_with("above the board. ⌘J opens the next.")
        );
        // KEY-08: the user's key for Next item that needs you, and no sentence about it without one.
        let custom = fixtures::replaced("next_inbox_item", None, 0, "⇧⌘J");
        assert!(
            welcome(&custom).points[1]
                .text
                .ends_with(" ⇧⌘J opens the next.")
        );
        let moved = fixtures::mac(&fixtures::activity_on_cmd_j());
        assert!(
            welcome(&moved).points[1]
                .text
                .ends_with("in one strip above the board.")
        );
        assert!(
            welcome(&fixtures::letters_off()).points[1]
                .text
                .ends_with("⌘J opens the next.")
        );
        assert_eq!(w.foot, "Takes about a minute");
        assert_eq!(w.button, "Choose a theme");
        let t = theme_step();
        assert_eq!(t.title, "Pick a look");
        assert_eq!(t.button, "Check my tools");
    }

    #[test]
    fn no_connected_agent_tool_blocks_continue_and_optional_tools_dont() {
        let blocked = tools(&checked(false, PermissionVerdict::NotPrompting), false);
        assert!(!blocked.continue_enabled);
        assert_eq!(
            blocked.continue_reason.as_deref(),
            Some("Connect one agent tool to continue")
        );
        let git = &blocked.rows[0];
        assert_eq!(git.badge, "required");
        let fine = tools(&checked(true, PermissionVerdict::NotPrompting), false);
        assert!(fine.continue_enabled);
        // The starter entry's line (*14* `ODRG2`, `z8OVF`), after M23's "Agent tools" sentence.
        assert_eq!(blocked.agents_line, None);
        assert_eq!(
            fine.agents_line.as_deref(),
            Some(
                format!(
                    "You can connect more later in Settings → Agent tools. {} starts you with one agent, Claude Code · tool default (Claude Code picks its own model). Make others, like Strong or Light, in Settings → Agents.",
                    naming::PRODUCT_NAME
                )
                .as_str()
            )
        );
        assert_eq!(fine.rows[1].look, RowLook::MissingOptional);
        assert_eq!(fine.rows[2].name, "az + azure-devops");
        assert!(fine.rows[1].command.is_some());
        assert_eq!(fine.warning, None);
        assert_eq!(fine.waiting, None);
    }

    fn with_agent_tools(
        statuses: Vec<otaq_core::agent_tools::connected::ToolStatus>,
    ) -> OnboardingModel {
        let mut m = checked(false, PermissionVerdict::NotPrompting);
        m.report.as_mut().unwrap().agent_tools = statuses;
        m
    }

    #[test]
    fn the_agent_tools_block_with_none_connected_and_with_codex_only() {
        use crate::view_models::agent_tools_page::fixtures::{connected, status};
        use otaq_core::agent_tools::Tool;
        use otaq_core::agent_tools::connected::AgentToolState;
        let missing = |tool| status(tool, AgentToolState::NotInstalled);

        let none = tools(
            &with_agent_tools(Tool::ALL.iter().map(|&t| missing(t)).collect()),
            false,
        );
        let block = &none.agent_tools;
        assert_eq!(block.heading, "AGENT TOOLS · AT LEAST ONE");
        assert_eq!(block.count, "none connected");
        assert_eq!(
            block.blocked.as_deref(),
            Some(
                "No agent tool is connected. Install and sign in to at least one; agents can’t start without it."
            )
        );
        assert_eq!(block.rows.len(), 4);
        assert!(
            block
                .rows
                .iter()
                .all(|r| r.detail == "not installed" && r.install.is_some())
        );
        assert!(!none.continue_enabled);
        assert_eq!(
            none.continue_reason.as_deref(),
            Some("Connect one agent tool to continue")
        );
        assert_eq!(none.other_heading, "OTHER TOOLS");

        // Codex alone is enough (d0086: no longer claude).
        let codex_only = tools(
            &with_agent_tools(vec![
                missing(Tool::Claude),
                status(
                    Tool::Codex,
                    connected("0.157.1", Some("signed in with ChatGPT")),
                ),
                missing(Tool::Copilot),
                status(Tool::OpenCode, AgentToolState::Hidden { version: None }),
            ]),
            false,
        );
        let block = &codex_only.agent_tools;
        assert_eq!(block.count, "1 connected · 1 hidden");
        assert_eq!(block.blocked, None);
        assert_eq!(block.rows[0].detail, "not installed · optional now");
        assert_eq!(block.rows[1].detail, "v0.157.1 · signed in with ChatGPT");
        assert_eq!(block.rows[3].detail, "hidden by you");
        assert!(codex_only.continue_enabled);
        assert!(codex_only.agents_line.is_some());

        // A tool's own warning on its row; then no warning above the list.
        let mut codex = status(Tool::Codex, connected("0.157.1", None));
        codex.permission_warning = Some("Your Codex settings ask for permission prompts".into());
        let mut m = with_agent_tools(vec![codex]);
        m.report.as_mut().unwrap().permission.verdict = PermissionVerdict::Prompting;
        let warned = tools(&m, false);
        assert_eq!(warned.warning, None);
        assert_eq!(
            warned.agent_tools.rows[0].warning.as_ref().unwrap().title,
            "Your Codex settings ask for permission prompts"
        );
    }

    #[test]
    fn the_permission_warning_shows_while_prompting() {
        let t = tools(&checked(true, PermissionVerdict::Prompting), false);
        let w = t.warning.unwrap();
        assert_eq!(
            w.title,
            "Your Claude Code settings ask for permission prompts"
        );
        assert_eq!(w.link, "Claude Code docs ↗");
        let mut waiting = OnboardingModel::new(false);
        waiting.start_check();
        assert_eq!(
            tools(&waiting, false).waiting.as_deref(),
            Some("Checking your tools…")
        );
    }

    fn probe() -> FolderProbe {
        FolderProbe {
            repo_path: "/code/shop-web".into(),
            name: "shop-web".into(),
            suggested_key: "SHOP".into(),
            base_branch: "main".into(),
            branches: Branches {
                local: vec!["main".into(), "release/2.3".into()],
                remote: vec!["origin/main".into(), "origin/next".into()],
            },
        }
    }

    #[test]
    fn the_form_shows_the_detected_values_and_the_key_hint() {
        let mut m = AddProjectModel::default();
        let empty = add_project(&m, &[], true);
        assert_eq!(empty.title, "Add your first project");
        assert_eq!(empty.folder, "No folder chosen yet");
        assert_eq!(empty.folder_look, FolderLook::Empty);
        assert!(!empty.can_add && empty.key.is_none());
        m.pick("/code/shop-web".into());
        assert_eq!(add_project(&m, &[], true).folder_look, FolderLook::Checking);
        m.probed(Ok(probe()), &[], &fixtures::base());
        let valid = add_project(&m, &[], true);
        assert_eq!(valid.folder_look, FolderLook::Valid);
        assert_eq!(valid.name.unwrap().value, "shop-web");
        let base = valid.base.unwrap();
        assert_eq!(base.value, "main");
        let rows: Vec<_> = base
            .items
            .iter()
            .map(|i| (i.label.as_str(), i.checked))
            .collect();
        assert_eq!(
            rows,
            [
                ("LOCAL", false),
                ("main", true),
                ("release/2.3", false),
                ("REMOTE", false),
                ("next", false)
            ],
            "local and remote branches, the detected one checked"
        );
        let key = valid.key.unwrap();
        assert_eq!(key.value, "SHOP");
        assert_eq!(
            key.hint.as_deref(),
            Some("Used in ticket IDs like SHOP-18. Can’t be changed later.")
        );
        assert!(valid.can_add);
        assert_eq!(valid.repo_path.as_deref(), Some("/code/shop-web"));
        let dialog = add_project(&m, &[], false);
        assert_eq!(
            (dialog.title.as_str(), dialog.choose.as_str()),
            ("Add project", "Change…")
        );
        assert!(dialog.name.is_none() && dialog.footnote.is_none());
    }

    #[test]
    fn refused_folders_say_why() {
        let mut m = AddProjectModel::default();
        m.pick("/tmp/designs".into());
        m.probed(
            Err((
                Some(otaq_core::board::Refusal::NotAGitRepo.as_str().to_owned()),
                "not a git repository".into(),
            )),
            &[],
            &fixtures::base(),
        );
        let v = add_project(&m, &[], true);
        assert_eq!(v.folder_look, FolderLook::Refused);
        assert_eq!(v.error.unwrap().title, "Not a git repository");
        assert!(!v.can_add);
    }

    #[test]
    fn the_trust_line_shows_once_a_folder_is_picked() {
        let line = "Adding a project also lets Claude Code work in this folder (its hooks and settings will run).";
        let mut m = claude_trusts();
        for first_run in [true, false] {
            assert_eq!(add_project(&m, &[], first_run).trust_line, None);
        }
        m.pick("/code/shop-web".into());
        assert_eq!(add_project(&m, &[], true).trust_line.as_deref(), Some(line));
        m.probed(Ok(probe()), &[], &fixtures::base());
        for first_run in [true, false] {
            let v = add_project(&m, &[], first_run);
            assert_eq!(v.trust_line.as_deref(), Some(line), "trusted or not");
            assert_eq!(v.trust_error, None);
            assert!(v.can_add);
        }
    }

    fn claude_trusts() -> AddProjectModel {
        AddProjectModel {
            trust_tools: vec![otaq_core::agent_tools::Tool::Claude],
            ..AddProjectModel::default()
        }
    }

    #[test]
    fn the_line_names_the_connected_tools_that_keep_folder_trust() {
        use otaq_core::agent_tools::Tool;
        let line = |tools: &[Tool]| {
            let mut m = AddProjectModel {
                trust_tools: tools.to_vec(),
                ..AddProjectModel::default()
            };
            m.pick("/code/shop-web".into());
            add_project(&m, &[], true).trust_line
        };
        assert_eq!(
            line(&[Tool::Codex]).as_deref(),
            Some(
                "Adding a project also lets Codex work in this folder (its hooks and settings will run)."
            )
        );
        assert_eq!(
            line(&[Tool::Claude, Tool::Codex]).as_deref(),
            Some(
                "Adding a project also lets Claude Code and Codex work in this folder (their hooks and settings will run)."
            )
        );
        assert_eq!(
            line(&[Tool::Claude, Tool::Codex, Tool::Copilot]).as_deref(),
            Some(
                "Adding a project also lets Claude Code, Codex and GitHub Copilot work in this folder (their hooks and settings will run)."
            )
        );
        // Only OpenCode connected: it keeps no folder trust, so there is nothing to say.
        assert_eq!(line(&[]), None);
    }

    #[test]
    fn trust_that_failed_for_one_tool_names_that_tool() {
        use otaq_core::agent_tools::Tool;
        let mut m = AddProjectModel {
            trust_tools: vec![Tool::Claude, Tool::Codex],
            ..AddProjectModel::default()
        };
        m.pick("/code/shop-web".into());
        m.probed(Ok(probe()), &[], &fixtures::base());
        m.added_untrusted(vec![(Tool::Codex, "its config isn’t writable".into())]);
        let v = add_project(&m, &[], true);
        assert_eq!(
            v.trust_error.as_deref(),
            Some(
                "Codex’s trust couldn’t be recorded: its config isn’t writable. Its agents will wait on Codex’s trust screen."
            )
        );
        assert!(!v.can_add);
        m.added_untrusted(vec![(Tool::Claude, "a".into()), (Tool::Codex, "b".into())]);
        let both = add_project(&m, &[], true).trust_error.unwrap();
        assert!(both.starts_with("Claude Code’s trust couldn’t be recorded: a."));
        assert!(both.contains(" Codex’s trust couldn’t be recorded: b."));
    }

    #[test]
    fn a_failed_trust_says_why_and_waits_for_continue() {
        let mut m = claude_trusts();
        m.pick("/code/shop-web".into());
        m.probed(Ok(probe()), &[], &fixtures::base());
        m.adding = true;
        m.added_untrusted(vec![(
            otaq_core::agent_tools::Tool::Claude,
            "Claude Code's config file couldn't be written".into(),
        )]);
        for first_run in [true, false] {
            let v = add_project(&m, &[], first_run);
            assert_eq!(
                v.trust_error.as_deref(),
                Some(
                    "Claude Code’s trust couldn’t be recorded: Claude Code's config file couldn't be written. Its \
                     agents will wait on Claude Code’s trust screen."
                )
            );
            assert_eq!(v.continue_label, "Continue");
            assert!(!v.can_add, "added already: only Continue");
            assert!(v.trust_line.is_some());
            assert_eq!(v.folder_look, FolderLook::Valid);
        }
        assert!(!m.adding);
    }

    #[test]
    fn the_tools_gallery_has_the_four_frames() {
        let g = tools_gallery();
        let counts: Vec<&str> = g
            .cells
            .iter()
            .map(|c| c.tools.agent_tools.count.as_str())
            .collect();
        assert_eq!(
            counts,
            [
                "3 connected",
                "none connected",
                "1 connected",
                "2 connected · 1 hidden"
            ]
        );
        let enabled: Vec<bool> = g.cells.iter().map(|c| c.tools.continue_enabled).collect();
        assert_eq!(enabled, [true, false, true, true]);
        assert!(g.cells[1].tools.agent_tools.blocked.is_some());
        assert!(g.cells[0].tools.agent_tools.rows[2].sign_in.is_some());
        assert!(g.cells[3].tools.agent_tools.rows[1].warning.is_some());
        assert_eq!(
            g.cells[3].name,
            format!("Hidden, warnings, what {} can see", naming::PRODUCT_NAME)
        );
    }

    #[test]
    fn the_trust_gallery_has_the_line_the_failed_form_and_the_banners() {
        let g = trust_gallery();
        assert_eq!(g.forms.len(), 6);
        assert!(g.forms.iter().all(|c| c.form.trust_line.is_some()));
        assert_eq!(
            g.forms
                .iter()
                .filter(|c| c.form.trust_error.is_some())
                .count(),
            3
        );
        let kinds: Vec<Vec<banners::SituationKind>> = g
            .banners
            .iter()
            .map(|c| c.banners.iter().map(|b| b.kind).collect())
            .collect();
        use banners::SituationKind::{PermissionWarning, TrustFailed, TrustNote};
        assert_eq!(
            kinds,
            [
                vec![TrustNote],
                vec![TrustFailed],
                vec![TrustFailed],
                vec![TrustFailed, PermissionWarning],
                vec![PermissionWarning, TrustNote],
            ]
        );
        assert!(g.banners[2].banners[0].tooltip.is_some());
    }

    #[test]
    fn no_projects_yet_matches_the_frame() {
        let v = no_projects();
        assert_eq!(v.title, "No projects yet");
        assert_eq!(v.add, "Add project folder…");
        assert_eq!(v.check, "Check tools");
    }
}
