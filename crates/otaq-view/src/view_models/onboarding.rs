//! First run (UI-08, UI-11, SET-02; *14 · First run & onboarding*): Welcome, then the Theme step ("Pick a look",
//! d0069), then the tools checklist from `tools.check`. `git` missing, or no agent tool connected (SET-02 `[M23]`,
//! d0086), blocks Continue; gh and az are optional. The permission-mode
//! warning shows only when the check says "prompting", and stays away once dismissed. Nothing on these screens
//! installs a tool or changes a setting on its own (d0021, d0011): each command is shown for copying, and its
//! Install / Sign in… button runs exactly that command in a visible terminal under the row, on the user's click.
//!
//! After Tools comes 4 · Project (the add-project form, T-0034), then 5 · Flow (ONB-04: the project's flow, from an
//! agent, a preset or the default); once the flow is settled the onboarding steps disappear and the empty board shows.
//! The indicator lists only the five steps it shows (no "Done" step; dogfood report 2026-10-24, #3).

use otaq_core::naming;
use otaq_core::tools::{PermissionVerdict, Tool, ToolAction, ToolStatus, ToolsReport};

/// What the tools step needs besides git (SET-02 `[M23]`; *14* `z8OVF` "None connected · blocking").
const CONNECT_AN_AGENT_TOOL: &str = "connect one agent tool";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    Welcome,
    Theme,
    Tools,
    Project,
    Flow,
}

/// The step indicator in the title bar (ONB-10): only the steps onboarding shows; the board after Flow isn't one.
pub const STEPS: [&str; 5] = ["Welcome", "Theme", "Tools", "Project", "Flow"];

impl Step {
    /// The step's place in [`STEPS`] (0-based; the indicator shows it plus one).
    pub fn index(self) -> usize {
        match self {
            Self::Welcome => 0,
            Self::Theme => 1,
            Self::Tools => 2,
            Self::Project => 3,
            Self::Flow => 4,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowIcon {
    /// Found: green check.
    Found,
    /// A required tool is missing: red cross, red row.
    MissingRequired,
    /// An optional tool is missing: grey minus.
    MissingOptional,
}

/// One row of the checklist.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolRow {
    pub tool: Tool,
    pub name: &'static str,
    pub required: bool,
    pub icon: RowIcon,
    /// "2.46.0", "2.62.0 · logged in", "not found on PATH", "not found — needed for GitHub PRs".
    pub status: String,
    /// Why a missing required tool matters.
    pub explanation: Option<String>,
    /// A command to copy (install, add the extension, or sign in).
    pub command: Option<String>,
    /// The button that runs `command` in a terminal; none when its package manager is missing.
    pub button: Option<RowButton>,
    /// A line under the command: "Needs Homebrew first…".
    pub note: Option<String>,
}

/// Install, Add extension or Sign in… on a row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RowButton {
    pub action: ToolAction,
    /// "Install", "Add extension", "Sign in…".
    pub label: &'static str,
    /// The terminal's title while it runs: "Installing gh…".
    pub running: String,
}

/// The Continue button on the tools step.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContinueState {
    pub enabled: bool,
    /// Shown next to the disabled button: "Install claude to continue".
    pub reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct OnboardingModel {
    pub step: Step,
    pub report: Option<ToolsReport>,
    pub checking: bool,
    /// The last `tools.check` failed (service unreachable).
    pub error: Option<String>,
    pub permission_dismissed: bool,
}

impl OnboardingModel {
    pub fn new(permission_dismissed: bool) -> Self {
        Self {
            step: Step::Welcome,
            report: None,
            checking: false,
            error: None,
            permission_dismissed,
        }
    }

    /// "Choose a theme" on Welcome: the Theme step.
    pub fn choose_theme(&mut self) {
        self.step = Step::Theme;
    }

    /// "Check my tools" on the Theme step: go to the tools step and run the check.
    pub fn start_check(&mut self) {
        self.step = Step::Tools;
        self.checking = true;
        self.error = None;
    }

    pub fn finish_check(&mut self, result: Result<ToolsReport, String>) {
        self.checking = false;
        match result {
            Ok(report) => {
                self.report = Some(report);
                self.error = None;
            }
            Err(e) => self.error = Some(e),
        }
    }

    pub fn rows(&self) -> Vec<ToolRow> {
        self.report
            .as_ref()
            .map(|r| r.tools.iter().map(tool_row).collect())
            .unwrap_or_default()
    }

    pub fn continue_state(&self) -> ContinueState {
        let Some(report) = &self.report else {
            return ContinueState {
                enabled: false,
                reason: None,
            };
        };
        let missing: Vec<&str> = report
            .tools
            .iter()
            .filter(|t| t.required && !t.found)
            .map(|t| t.tool.command())
            .collect();
        let install = (!missing.is_empty()).then(|| format!("Install {}", missing.join(" and ")));
        let connect = (!report.agent_tool_connected()).then_some(CONNECT_AN_AGENT_TOOL);
        let reason = match (install, connect) {
            (None, None) => None,
            (Some(install), None) => Some(format!("{install} to continue")),
            (None, Some(_)) => Some("Connect one agent tool to continue".to_owned()),
            (Some(install), Some(connect)) => Some(format!("{install} and {connect} to continue")),
        };
        ContinueState {
            enabled: reason.is_none() && !self.checking,
            reason,
        }
    }

    /// Claude Code's dismissable warning above the list: only from a report whose agent tools carry no warning
    /// of their own (since M23 each tool's row has its own, SET-02).
    pub fn show_permission_warning(&self) -> bool {
        !self.permission_dismissed
            && self.report.as_ref().is_some_and(|r| {
                r.permission.verdict == PermissionVerdict::Prompting
                    && r.agent_tools.iter().all(|t| t.permission_warning.is_none())
            })
    }
}

fn tool_row(t: &ToolStatus) -> ToolRow {
    let name = match t.tool {
        Tool::Az => "az + azure-devops",
        other => other.command(),
    };
    if !t.found {
        // The agent tools aren't checklist rows since M23 (Settings → Agent tools); git is the only required one.
        let (status, explanation) = match t.tool {
            Tool::Gh => ("not found — needed for GitHub PRs".to_owned(), None),
            Tool::Az => ("not found — needed for Azure DevOps PRs".to_owned(), None),
            _ => (
                "not found on PATH".to_owned(),
                Some(format!(
                    "{} runs your own git for worktrees, branches and merges.",
                    naming::PRODUCT_NAME
                )),
            ),
        };
        return ToolRow {
            tool: t.tool,
            name,
            required: t.required,
            icon: if t.required {
                RowIcon::MissingRequired
            } else {
                RowIcon::MissingOptional
            },
            status,
            explanation,
            command: t.install_command.clone(),
            button: t.missing_package_manager.is_none().then(|| RowButton {
                action: ToolAction::Install,
                label: "Install",
                running: format!("Installing {}\u{2026}", t.tool.command()),
            }),
            note: t
                .missing_package_manager
                .as_deref()
                .map(|manager| match manager {
                    "brew" => {
                        "Needs Homebrew first (brew.sh). Install it, then Re-check.".to_owned()
                    }
                    other => format!("Needs {other} first. Install it, then Re-check."),
                }),
        };
    }
    let mut parts = vec![t.version.clone().unwrap_or_else(|| "found".to_owned())];
    if let Some(installed) = t.extension_installed {
        parts.push(
            if installed {
                "extension installed"
            } else {
                "extension missing"
            }
            .to_owned(),
        );
    }
    if let Some(logged_in) = t.logged_in {
        parts.push(
            if logged_in {
                "signed in"
            } else {
                "not signed in"
            }
            .to_owned(),
        );
    }
    ToolRow {
        tool: t.tool,
        name,
        required: t.required,
        icon: RowIcon::Found,
        status: parts.join(" · "),
        explanation: None,
        command: t
            .install_command
            .clone()
            .or_else(|| t.login_command.clone()),
        button: if t.install_command.is_some() {
            Some(RowButton {
                action: ToolAction::Install,
                label: "Add extension",
                running: "Adding the azure-devops extension\u{2026}".to_owned(),
            })
        } else {
            t.login_command.as_ref().map(|_| RowButton {
                action: ToolAction::Login,
                label: "Sign in\u{2026}",
                running: format!("Signing in to {}\u{2026}", t.tool.command()),
            })
        },
        note: None,
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use otaq_core::tools::{PermissionCheck, PermissionVerdict};

    use super::*;

    fn status(tool: Tool, found: bool) -> ToolStatus {
        ToolStatus {
            tool,
            required: tool.required(),
            found,
            version: None,
            logged_in: None,
            extension_installed: None,
            install_command: (!found).then(|| tool.install_command().to_owned()),
            login_command: None,
            missing_package_manager: None,
        }
    }

    fn report(tools: Vec<ToolStatus>, verdict: PermissionVerdict) -> ToolsReport {
        ToolsReport {
            tools,
            permission: PermissionCheck {
                verdict,
                mode: None,
                source: None,
            },
            agent_tools: agent_tools(&[otaq_core::agent_tools::Tool::Claude]),
        }
    }

    /// The agent tools' rows with `connected` Connected and the others not installed.
    pub(crate) fn agent_tools(
        connected: &[otaq_core::agent_tools::Tool],
    ) -> Vec<otaq_core::agent_tools::connected::ToolStatus> {
        use otaq_core::agent_tools::connected::{AgentToolState, ConnectedTools};
        let tools = ConnectedTools::new([]);
        for &tool in connected {
            let state = AgentToolState::Connected {
                version: Some("1.0".into()),
                account: None,
            };
            tools.set_detected(tool, state, 1);
        }
        tools.list().tools
    }

    fn everything_installed() -> Vec<ToolStatus> {
        vec![
            ToolStatus {
                version: Some("2.46.0".into()),
                ..status(Tool::Git, true)
            },
            ToolStatus {
                version: Some("2.62.0".into()),
                logged_in: Some(true),
                ..status(Tool::Gh, true)
            },
            ToolStatus {
                version: Some("2.67.0".into()),
                logged_in: Some(true),
                extension_installed: Some(true),
                ..status(Tool::Az, true)
            },
        ]
    }

    fn checked(tools: Vec<ToolStatus>, verdict: PermissionVerdict) -> OnboardingModel {
        let mut m = OnboardingModel::new(false);
        m.start_check();
        m.finish_check(Ok(report(tools, verdict)));
        m
    }

    #[test]
    fn welcome_then_theme_then_tools() {
        let mut m = OnboardingModel::new(false);
        assert_eq!(m.step, Step::Welcome);
        assert!(m.rows().is_empty());
        assert!(!m.continue_state().enabled);
        m.choose_theme();
        assert_eq!(m.step, Step::Theme);
        assert!(!m.checking, "the check starts with Check my tools");
        m.start_check();
        assert_eq!(m.step, Step::Tools);
        assert!(m.checking);
    }

    #[test]
    fn the_indicator_has_five_steps_ending_with_flow() {
        assert_eq!(STEPS, ["Welcome", "Theme", "Tools", "Project", "Flow"]);
        for (step, n) in [
            (Step::Welcome, 1),
            (Step::Theme, 2),
            (Step::Tools, 3),
            (Step::Project, 4),
            (Step::Flow, 5),
        ] {
            assert_eq!(step.index() + 1, n, "{step:?}");
            assert_eq!(STEPS[step.index()], format!("{step:?}"));
        }
    }

    #[test]
    fn everything_installed_matches_the_frame() {
        let m = checked(everything_installed(), PermissionVerdict::NotPrompting);
        let statuses: Vec<(&str, String, RowIcon)> = m
            .rows()
            .into_iter()
            .map(|r| (r.name, r.status, r.icon))
            .collect();
        assert_eq!(
            statuses,
            [
                ("git", "2.46.0".to_owned(), RowIcon::Found),
                ("gh", "2.62.0 · signed in".to_owned(), RowIcon::Found),
                (
                    "az + azure-devops",
                    "2.67.0 · extension installed · signed in".to_owned(),
                    RowIcon::Found
                ),
            ]
        );
        assert!(m.rows().iter().all(|r| r.command.is_none()));
        assert_eq!(
            m.continue_state(),
            ContinueState {
                enabled: true,
                reason: None
            }
        );
        assert!(!m.show_permission_warning());
    }

    #[test]
    fn no_agent_tool_connected_blocks_continue() {
        let mut m = checked(everything_installed(), PermissionVerdict::Unknown);
        m.report.as_mut().unwrap().agent_tools = agent_tools(&[]);
        let cont = m.continue_state();
        assert!(!cont.enabled);
        assert_eq!(
            cont.reason.as_deref(),
            Some("Connect one agent tool to continue")
        );

        // `claude` missing alone doesn't block when another tool (Codex) is connected (d0086).
        m.report.as_mut().unwrap().agent_tools =
            agent_tools(&[otaq_core::agent_tools::Tool::ALL[1]]);
        assert!(m.continue_state().enabled);

        // git missing too.
        let mut tools = everything_installed();
        tools[0] = status(Tool::Git, false);
        let mut m = checked(tools, PermissionVerdict::Unknown);
        assert_eq!(
            m.continue_state().reason.as_deref(),
            Some("Install git to continue")
        );
        m.report.as_mut().unwrap().agent_tools = agent_tools(&[]);
        assert_eq!(
            m.continue_state().reason.as_deref(),
            Some("Install git and connect one agent tool to continue")
        );
    }

    #[test]
    fn gh_and_az_missing_do_not_block() {
        let mut tools = everything_installed();
        tools[1] = status(Tool::Gh, false);
        tools[2] = status(Tool::Az, false);
        let m = checked(tools, PermissionVerdict::Prompting);
        let rows = m.rows();
        assert_eq!(rows[1].status, "not found — needed for GitHub PRs");
        assert_eq!(rows[1].icon, RowIcon::MissingOptional);
        // The command of the OS the app runs on ("brew install gh", "winget install GitHub.cli", …).
        assert_eq!(rows[1].command.as_deref(), Some(Tool::Gh.install_command()));
        assert_eq!(rows[2].status, "not found — needed for Azure DevOps PRs");
        assert!(m.continue_state().enabled);
        assert!(
            m.show_permission_warning(),
            "this step can show the warning"
        );
    }

    #[test]
    fn logged_out_and_missing_extension_offer_the_fix() {
        let mut tools = everything_installed();
        tools[1].logged_in = Some(false);
        tools[1].login_command = Some("gh auth login".into());
        tools[2].extension_installed = Some(false);
        tools[2].install_command = Some("az extension add --name azure-devops".into());
        let m = checked(tools, PermissionVerdict::NotPrompting);
        let rows = m.rows();
        assert_eq!(rows[1].status, "2.62.0 · not signed in");
        assert_eq!(rows[1].command.as_deref(), Some("gh auth login"));
        assert_eq!(rows[2].status, "2.67.0 · extension missing · signed in");
        assert_eq!(
            rows[2].command.as_deref(),
            Some("az extension add --name azure-devops")
        );
    }

    #[test]
    fn permission_warning_only_when_prompting_and_not_dismissed() {
        for (verdict, shown) in [
            (PermissionVerdict::Prompting, true),
            (PermissionVerdict::NotPrompting, false),
            (PermissionVerdict::Unknown, false),
        ] {
            assert_eq!(
                checked(everything_installed(), verdict).show_permission_warning(),
                shown,
                "{verdict:?}"
            );
        }
        let mut m = checked(everything_installed(), PermissionVerdict::Prompting);
        m.permission_dismissed = true;
        assert!(!m.show_permission_warning());
    }

    #[test]
    fn a_tool_row_with_its_own_warning_replaces_the_one_above_the_list() {
        let mut m = checked(everything_installed(), PermissionVerdict::Prompting);
        assert!(m.show_permission_warning());
        m.report.as_mut().unwrap().agent_tools[0].permission_warning =
            Some("Your Claude Code settings ask for permission prompts".into());
        assert!(!m.show_permission_warning());
    }

    #[test]
    fn a_failed_check_keeps_the_last_report_and_says_why() {
        let mut m = checked(everything_installed(), PermissionVerdict::Unknown);
        m.start_check();
        assert!(!m.continue_state().enabled, "not while re-checking");
        m.finish_check(Err("not connected".into()));
        assert_eq!(m.error.as_deref(), Some("not connected"));
        assert_eq!(m.rows().len(), 3);
        assert!(m.continue_state().enabled);
    }
}
