//! First run and Add project as the Tauri page draws them (T-0233; UI-08, UI-11, SET-02, PRJ-01…PRJ-03; *02 → No
//! projects yet*): the title bar's step indicator (Welcome · Tools · Project, ONB-10), Welcome's picture of the model,
//! the Tools step from [`OnboardingModel`] (the agent tools and git, Continue and why it is off), the Add project form
//! from [`AddProjectModel`] (the onboarding step and the later dialog, with its Flow choice), and the "No projects yet"
//! board. The form says which tools will run their hooks and settings in the folder, and why a tool's trust wasn't
//! recorded when it wasn't (PRJ-07, d0085). After Project the board opens on the default flow with the first ticket in
//! Backlog: there is no Theme or Flow step.

use otaq_core::agent_tools::connected::AgentToolState;
use otaq_core::board::snapshot::ProjectInfo;
use otaq_core::naming;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::icons::IconName;
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
    Tools,
    Project,
}

impl StepId {
    pub fn step(self) -> Step {
        match self {
            Self::Welcome => Step::Welcome,
            Self::Tools => Step::Tools,
            Self::Project => Step::Project,
        }
    }

    pub const ALL: [Self; 3] = [Self::Welcome, Self::Tools, Self::Project];
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

/// The window's title bar during onboarding: the product name and the three steps.
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

/// A sample card in Welcome's picture.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "OnboardingSampleCard")]
pub struct SampleCard {
    pub key: String,
    pub title: String,
    pub status: String,
    /// Turquoise: an agent needs the user.
    pub needs_you: bool,
}

/// One column of Welcome's picture: the default flow's columns, who acts in each.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "OnboardingColumn")]
pub struct Column {
    pub title: String,
    /// "An agent writes the code".
    pub who: String,
    pub card: Option<SampleCard>,
}

/// Welcome: what Otaq is, in one picture: the board itself, with the default flow the project will start on, who acts
/// in each column, and the turquoise card. One button.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "OnboardingWelcome")]
pub struct Welcome {
    pub title: String,
    pub text: String,
    pub columns: Vec<Column>,
    /// Under the picture: the turquoise card and the notification.
    pub needs_you: String,
    pub foot: String,
    pub button: String,
}

/// The ticket the picture follows.
const SAMPLE_KEY: &str = "SHOP-1";

pub fn welcome() -> Welcome {
    let flow = otaq_core::flow::default::default_flow();
    let card = |status: &str, needs_you: bool| SampleCard {
        key: SAMPLE_KEY.to_owned(),
        title: crate::view_models::empty_board::TITLE.to_owned(),
        status: status.to_owned(),
        needs_you,
    };
    let column = |title: &str, who: &str, card: Option<SampleCard>| Column {
        title: title.to_owned(),
        who: who.to_owned(),
        card,
    };
    let mut columns = vec![column(
        "Backlog",
        "You write a ticket",
        Some(card("Not started", false)),
    )];
    columns.extend(flow.stages.iter().map(|stage| {
        if stage.is_human_check() {
            column(&stage.name, "You review, then approve", None)
        } else if stage.is_agent_check() {
            column(
                &stage.name,
                "A checker tests it, and sends it back if it fails",
                None,
            )
        } else {
            let first = flow.first_agent_stage().is_some_and(|s| s.id == stage.id);
            column(
                &stage.name,
                "An agent writes the code",
                first.then(|| card("Asks you", true)),
            )
        }
    }));
    columns.push(column("Done", "Merged, or sent as a pull request", None));
    Welcome {
        title: "Coding agents that follow your process".to_owned(),
        text: "Your board\u{2019}s columns are your process. A ticket moves through them: agents do the work, other \
               agents check it, and you decide what gets merged."
            .to_owned(),
        columns,
        needs_you: format!(
            "When an agent needs you, its card turns turquoise and {} lets you know, even when the window is closed.",
            naming::PRODUCT_NAME
        ),
        foot: "Next: connect an agent tool and add a project. About 2 minutes.".to_owned(),
        button: "Get started".to_owned(),
    }
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
    /// git's row while it has no name and email for commits: the form that saves them.
    pub identity: Option<crate::view_models::git_identity::Form>,
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

/// The agent tools on the Tools step (SET-02 `[M23]`, d0086): the same rows as Settings → Agent tools without
/// Disconnect…, and the rule "one tool is enough".
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "OnboardingAgentTools")]
pub struct AgentTools {
    /// "AGENT TOOLS".
    pub heading: String,
    /// "1 ready", "none ready", "2 ready · 1 hidden".
    pub count: String,
    pub rows: Vec<agent_tools_page::Row>,
    /// With none ready, said once at the top: "No agent tool is ready yet. …".
    pub blocked: Option<String>,
    /// The … button's name.
    pub menu_label: String,
}

/// Tools: connect an agent tool. Only what's needed to start: the agent tools and git. The pull request tools (gh, az)
/// aren't asked for here; they're set up in Settings the first time a ticket opens a PR.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "OnboardingTools")]
pub struct Tools {
    pub title: String,
    pub text: String,
    pub agent_tools: AgentTools,
    /// "ALSO NEEDED": git.
    pub other_heading: String,
    pub rows: Vec<ToolRowView>,
    /// While no report is in: "Checking your tools…".
    pub waiting: Option<String>,
    pub error: Option<String>,
    pub recheck: String,
    pub continue_label: String,
    pub continue_enabled: bool,
    /// Next to a Continue that is off for another reason than the one said at the top: "Install git to continue".
    pub continue_reason: Option<String>,
    /// The user has projects already (Check tools from the board): Continue ends here, no Project step.
    pub has_projects: bool,
    pub copy: String,
    pub terminal: ToolTerminalTexts,
    /// Under the lists: where more tools are added later.
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
        0 => "none ready".to_owned(),
        n => format!("{n} ready"),
    };
    if hidden > 0 {
        count.push_str(&format!(" \u{b7} {hidden} hidden"));
    }
    let missing_identity = |tool: otaq_core::tools::Tool| {
        model
            .report
            .as_ref()
            .and_then(|r| r.tools.iter().find(|t| t.tool == tool))
            .and_then(|t| t.missing_identity.as_ref())
    };
    // git is the one tool besides an agent tool that's needed to start.
    let rows: Vec<ToolRowView> = model
        .rows()
        .into_iter()
        .filter(|r| r.required)
        .map(|r| {
            let identity = missing_identity(r.tool);
            let (look, icon) = match r.icon {
                // Found, but it can't commit yet: the form under the row says what's left (it doesn't block).
                RowIcon::Found if identity.is_some() => (RowLook::Found, IconName::CircleAlert),
                RowIcon::Found => (RowLook::Found, IconName::CircleCheck),
                RowIcon::MissingRequired => (RowLook::MissingRequired, IconName::CircleX),
                RowIcon::MissingOptional => (RowLook::MissingOptional, IconName::CircleMinus),
            };
            ToolRowView {
                tool: r.tool.command().to_owned(),
                button: tool_button(&r),
                note: r.note.clone(),
                name: r.name.to_owned(),
                badge: "required".to_owned(),
                look,
                icon,
                status: match identity {
                    Some(_) => format!(
                        "{} \u{b7} {}",
                        r.status,
                        crate::view_models::git_identity::STATE
                    ),
                    None => r.status,
                },
                explanation: r.explanation,
                command: r.command,
                identity: identity.map(crate::view_models::git_identity::form),
            }
        })
        .collect();
    let git_missing = rows.iter().any(|r| r.look == RowLook::MissingRequired);
    Tools {
        title: "Connect an agent tool".to_owned(),
        text: format!(
            "{product} runs the coding agents you already use, with your own sign-in and plan. You need one to start; \
             {product} never installs or signs in without your click.",
            product = naming::PRODUCT_NAME
        ),
        agent_tools: AgentTools {
            heading: "AGENT TOOLS".to_owned(),
            count,
            rows: statuses
                .iter()
                .map(|s| agent_tools_page::row(s, Place::Onboarding, none_connected))
                .collect(),
            blocked: none_connected.then(|| {
                "No agent tool is ready yet. Install one and sign in, then Continue.".to_owned()
            }),
            menu_label: "More actions".to_owned(),
        },
        other_heading: "ALSO NEEDED".to_owned(),
        rows,
        waiting: model.report.is_none().then(|| {
            if model.checking {
                CHECKING
            } else {
                "No result yet"
            }
            .to_owned()
        }),
        error: model.error.as_ref().map(|e| format!("Check failed: {e}")),
        recheck: "Check again".to_owned(),
        continue_label: "Continue".to_owned(),
        continue_enabled: state.enabled,
        // The missing agent tool is said once, at the top.
        continue_reason: state.reason.filter(|_| git_missing),
        has_projects,
        copy: "Copy".to_owned(),
        terminal: tool_terminal_texts(),
        agents_line: Some(agents_line()),
    }
}

/// The Tools step's last line: where more tools are added later.
pub fn agents_line() -> String {
    "You can add more tools later in Settings \u{2192} Agent tools.".to_owned()
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
    /// "SHOP-1, SHOP-2, … Can’t be changed later." while the key is fine.
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
    /// Under the field: "Agents start from here; work merges back here."
    pub note: String,
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
    /// "Claude Code, Codex and Copilot will run their hooks and settings in this folder."
    pub trust_line: Option<String>,
    /// The project was added but some tools' trust wasn't recorded: why, one sentence per tool, in the form's
    /// inline error style. The form then waits for [`Self::continue_label`] instead of Add.
    pub trust_error: Option<String>,
    pub continue_label: String,
    /// The dialog of a project added after the first: which flow it starts on.
    pub flow: Option<FlowChoice>,
}

pub const PICKER_TITLE: &str = "Add project";

/// Which flow a project added after the first starts on: Otaq default, the same as another project's, or the default
/// while Noyan's agent drafts a suggestion in Edit flow. `AddProjectFlowPick` on the page.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(rename = "AddProjectFlowPick")]
pub enum FlowPick {
    #[default]
    Default,
    SameAs {
        project: String,
    },
    Suggest,
}

/// One of the Flow choice's options.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "AddProjectFlowOption")]
pub struct FlowOption {
    pub pick: FlowPick,
    /// "Otaq default", "Same as shop-web", "Suggest one for billing-api".
    pub name: String,
    /// "Dev → Validate → Review · merge or PR on Approve", or what Suggest does.
    pub line: String,
}

/// The light Flow choice in Add project, after the first project: Otaq default selected.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "AddProjectFlow")]
pub struct FlowChoice {
    pub label: String,
    pub options: Vec<FlowOption>,
    /// "Change it any time with Edit flow."
    pub foot: String,
}

/// Another project whose flow a new one can copy: its key, name and flow's line.
pub struct OtherFlow<'a> {
    pub key: &'a str,
    pub name: &'a str,
    pub line: String,
}

/// The Flow choice: Otaq default, Same as `other` (the project added last), Suggest one for the new project (named
/// once its folder is picked). `suggester` is the agent that drafts it.
pub fn flow_choice(
    other: Option<OtherFlow>,
    new_name: Option<&str>,
    suggester: &str,
) -> FlowChoice {
    let default = otaq_core::flow::default::default_flow();
    let mut options = vec![FlowOption {
        pick: FlowPick::Default,
        name: format!("{} default", naming::PRODUCT_NAME),
        line: super::flow_board::chain(&default),
    }];
    options.extend(other.map(|o| FlowOption {
        pick: FlowPick::SameAs {
            project: o.key.to_owned(),
        },
        name: format!("Same as {}", o.name),
        line: o.line,
    }));
    options.push(FlowOption {
        pick: FlowPick::Suggest,
        name: match new_name {
            Some(name) => format!("Suggest one for {name}"),
            None => "Suggest one for this project".to_owned(),
        },
        line: format!(
            "After adding, {suggester} reads the project and drafts a flow in Edit flow. The board starts on {} \
             default meanwhile.",
            naming::PRODUCT_NAME
        ),
    });
    FlowChoice {
        label: "Flow".to_owned(),
        options,
        foot: "Change it any time with Edit flow.".to_owned(),
    }
}

/// The form. `flow` is the dialog's Flow choice (a project added after the first); none on the onboarding step,
/// where every project starts on Otaq default.
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
        title: "Add a project".to_owned(),
        text: first_run.then(|| {
            "A project is a git repository on your machine. Agents work in their own copies of it; only a merge you \
             approve updates your base branch."
                .to_owned()
        }),
        folder: model.folder.as_deref().map_or_else(
            || "Choose a folder, or drop one here".to_owned(),
            tilde,
        ),
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
        name: probe.map(|p| Detected {
            icon: IconName::Tag,
            label: "Name".to_owned(),
            value: p.name.clone(),
        }),
        base: probe.map(|p| BaseField {
            icon: IconName::GitBranch,
            label: "Base branch".to_owned(),
            value: model.base_branch.clone(),
            note: "Agents start from here; work merges back here.".to_owned(),
            items: branch_items(&p.branches.local, &p.branches.remote, &model.base_branch),
            search: MenuSearch::branches(),
        }),
        key: probe.map(|_| KeyField {
            label: "Ticket key".to_owned(),
            note: "prefilled from the repo name".to_owned(),
            value: model.key.clone(),
            placeholder: "KEY".to_owned(),
            hint: key_error.is_none().then(|| model.key_hint()),
            error: key_error,
        }),
        footnote: first_run.then(|| {
            "Run command and environment can wait: they\u{2019}re in the project\u{2019}s settings.".to_owned()
        }),
        add: "Add project".to_owned(),
        adding: "Adding\u{2026}".to_owned(),
        can_add: model.can_add(projects),
        add_error: model.add_error.clone(),
        cancel: "Cancel".to_owned(),
        trust_line: picked.then(|| model.trust_line()).flatten(),
        trust_error: Some(model.trust_errors().join(" ")).filter(|e| !e.is_empty()),
        continue_label: "Continue".to_owned(),
        flow: None,
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
            &crate::key_labels::KeyLabels::default(),
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
        path: None,
        account: None,
        missing_identity: None,
    };
    let step = |statuses| {
        let mut m = OnboardingModel::new();
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
    .collect::<Vec<_>>();
    // git found without a name and email for commits: the form under its row.
    let mut cells = cells;
    let mut no_identity = step(vec![claude_connected()]);
    if let Some(row) = no_identity.rows.first_mut() {
        row.icon = IconName::CircleAlert;
        row.status = format!("{} \u{b7} {}", row.status, super::git_identity::STATE);
        row.identity = Some(super::git_identity::form(
            &otaq_core::git::Identity::default(),
        ));
    }
    cells.push(ToolsGalleryCell {
        name: "git without a name and email".to_owned(),
        tools: no_identity,
    });
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
    /// The title bar at each step, in [`StepId::ALL`]'s order.
    pub title_bars: Vec<TitleBar>,
    pub no_projects: NoProjects,
    /// The tools list while the check runs.
    pub checking: String,
}

pub fn texts() -> Texts {
    Texts {
        welcome: welcome(),
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
            path: None,
            account: None,
            missing_identity: None,
        }
    }

    fn checked(claude: bool, verdict: PermissionVerdict) -> OnboardingModel {
        let mut m = OnboardingModel::new();
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
        assert_eq!(states, [DotState::Done, DotState::Current, DotState::Next]);
        let labels: Vec<(&str, &str)> = bar
            .steps
            .iter()
            .map(|d| (d.number.as_str(), d.label.as_str()))
            .collect();
        assert_eq!(labels, [("1", "Welcome"), ("2", "Tools"), ("3", "Project")]);
        assert_eq!(texts().title_bars.len(), 3);
    }

    /// Welcome teaches the model with the board itself: the default flow's columns, who acts in each, and the
    /// turquoise card.
    #[test]
    fn welcome_shows_the_default_flow_and_who_acts_in_each_column() {
        let w = welcome();
        assert_eq!(w.title, "Coding agents that follow your process");
        let columns: Vec<(&str, &str)> = w
            .columns
            .iter()
            .map(|c| (c.title.as_str(), c.who.as_str()))
            .collect();
        assert_eq!(
            columns,
            [
                ("Backlog", "You write a ticket"),
                ("Dev", "An agent writes the code"),
                (
                    "Validate",
                    "A checker tests it, and sends it back if it fails"
                ),
                ("Review", "You review, then approve"),
                ("Done", "Merged, or sent as a pull request"),
            ]
        );
        let asks = w.columns[1].card.as_ref().unwrap();
        assert!(asks.needs_you);
        assert_eq!(
            (asks.key.as_str(), asks.status.as_str()),
            ("SHOP-1", "Asks you")
        );
        assert_eq!(w.button, "Get started");
        assert!(
            w.foot
                .starts_with("Next: connect an agent tool and add a project.")
        );
    }

    #[test]
    fn git_without_a_name_and_email_asks_for_them_without_blocking_continue() {
        let mut m = checked(true, PermissionVerdict::NotPrompting);
        if let Some(report) = m.report.as_mut() {
            report.tools[0].version = Some("2.46.0".into());
            report.tools[0].missing_identity = Some(otaq_core::git::Identity::default());
        }
        let v = tools(&m, false);
        let git = &v.rows[0];
        assert_eq!(git.status, "2.46.0 \u{b7} Needs your name and email");
        assert_eq!(git.icon, IconName::CircleAlert);
        let form = git.identity.as_ref().unwrap();
        assert_eq!(
            (form.name_label.as_str(), form.email_label.as_str()),
            ("Name", "Email")
        );
        assert!(v.continue_enabled, "the merge asks again if it's skipped");
        let fine = tools(&checked(true, PermissionVerdict::NotPrompting), false);
        assert!(fine.rows[0].identity.is_none());
    }

    #[test]
    fn no_connected_agent_tool_blocks_continue_and_only_git_is_asked_besides() {
        let blocked = tools(&checked(false, PermissionVerdict::NotPrompting), false);
        assert!(!blocked.continue_enabled);
        assert_eq!(
            blocked.agent_tools.blocked.as_deref(),
            Some("No agent tool is ready yet. Install one and sign in, then Continue.")
        );
        assert_eq!(blocked.continue_reason, None, "said once, at the top");
        let fine = tools(&checked(true, PermissionVerdict::NotPrompting), false);
        assert!(fine.continue_enabled);
        assert_eq!(fine.other_heading, "ALSO NEEDED");
        let names: Vec<&str> = fine.rows.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(names, ["git"], "gh and az aren't asked for here");
        assert_eq!(
            fine.agents_line.as_deref(),
            Some("You can add more tools later in Settings → Agent tools.")
        );
        assert_eq!(fine.waiting, None);
        assert_eq!(fine.recheck, "Check again");
    }

    fn with_agent_tools(
        statuses: Vec<otaq_core::agent_tools::connected::ToolStatus>,
    ) -> OnboardingModel {
        let mut m = checked(false, PermissionVerdict::NotPrompting);
        m.report.as_mut().unwrap().agent_tools = statuses;
        m
    }

    #[test]
    fn the_agent_tools_block_with_none_ready_and_with_codex_only() {
        use crate::view_models::agent_tools_page::fixtures::{connected, status};
        use otaq_core::agent_tools::Tool;
        use otaq_core::agent_tools::connected::AgentToolState;
        let missing = |tool| status(tool, AgentToolState::NotInstalled);

        let none = tools(
            &with_agent_tools(Tool::ALL.iter().map(|&t| missing(t)).collect()),
            false,
        );
        let block = &none.agent_tools;
        assert_eq!(block.heading, "AGENT TOOLS");
        assert_eq!(block.count, "none ready");
        assert_eq!(block.rows.len(), 4);
        assert!(
            block
                .rows
                .iter()
                .all(|r| r.detail == "not installed" && r.install.is_some())
        );
        assert!(!none.continue_enabled);

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
        assert_eq!(block.count, "1 ready · 1 hidden");
        assert_eq!(block.blocked, None);
        assert_eq!(block.rows[1].detail, "v0.157.1 · signed in with ChatGPT");
        assert!(codex_only.continue_enabled);
    }

    /// The permission-mode warning left this step: it is a one-time hint at the first permission prompt.
    #[test]
    fn the_tools_step_says_nothing_about_permission_prompts() {
        let t = tools(&checked(true, PermissionVerdict::Prompting), false);
        let json = serde_json::to_string(&t).unwrap();
        assert!(!json.contains("permission prompts"), "{json}");
        let mut waiting = OnboardingModel::new();
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
        assert_eq!(empty.title, "Add a project");
        assert_eq!(empty.folder, "Choose a folder, or drop one here");
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
            Some("SHOP-1, SHOP-2, … Can’t be changed later.")
        );
        assert!(valid.can_add);
        assert_eq!(valid.repo_path.as_deref(), Some("/code/shop-web"));
        let dialog = add_project(&m, &[], false);
        assert_eq!(
            (dialog.title.as_str(), dialog.choose.as_str()),
            ("Add a project", "Change…")
        );
        assert!(dialog.footnote.is_none() && dialog.flow.is_none());
        assert_eq!(
            add_project(&m, &[], true).base.unwrap().note,
            "Agents start from here; work merges back here."
        );
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
        let line = "Claude Code will run its hooks and settings in this folder.";
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
            Some("Codex will run its hooks and settings in this folder.")
        );
        assert_eq!(
            line(&[Tool::Claude, Tool::Codex]).as_deref(),
            Some("Claude Code and Codex will run their hooks and settings in this folder.")
        );
        assert_eq!(
            line(&[Tool::Claude, Tool::Codex, Tool::Copilot]).as_deref(),
            Some(
                "Claude Code, Codex and Copilot will run their hooks and settings in this folder."
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
            ["3 ready", "none ready", "1 ready", "2 ready · 1 hidden"]
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
