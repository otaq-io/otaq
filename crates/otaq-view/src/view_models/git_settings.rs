//! Settings → Git & pull requests: git on this machine (its version and where it is, Re-check), then one row per pull
//! request provider with the tool it uses and whether that tool is signed in ("GitHub · uses gh 2.62.0 · Signed in as
//! nt"). Install and Sign in… run in a terminal under the row, as the agent tools' do. The providers are today's
//! built-in GitHub and Azure DevOps; each project picks its own in its Repository settings. Otaq pushes and opens
//! pull requests through them and never watches them.
//!
//! Everything comes from the git tools check (`tools.check_git`, onboarding's check without the agent tools): the
//! rows only say what it found.

use otaq_core::tools::{Tool, ToolStatus, ToolsReport};
use serde::Serialize;
use ts_rs::TS;

use super::git_identity;
use super::onboarding::{OnboardingModel, ToolRow};
use super::onboarding_page::{ToolButton, ToolTerminalTexts, tool_button, tool_terminal_texts};
use otaq_core::naming::PRODUCT_NAME;

/// How a row's state looks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename = "GitRowLook")]
pub enum Look {
    /// Found and, for a provider, signed in.
    Ready,
    /// Found, with something left to do (sign in, add the extension).
    Attention,
    /// Not found. Red for git, which every project needs; muted for a provider.
    Missing,
    /// Its check hasn't answered yet: "Checking…".
    Checking,
}

/// git's row, or one provider's.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "GitSettingsRow")]
pub struct Row {
    /// The tool the row's terminal is keyed by: "git", "gh", "az".
    pub tool: String,
    /// "git", "GitHub", "Azure DevOps".
    pub name: String,
    /// "2.46.0 · /usr/bin/git"; a provider's "uses gh 2.62.0".
    pub detail: String,
    /// "Needed by every project", "Signed in as nt", "Not signed in", "gh isn't installed".
    pub state: String,
    pub look: Look,
    /// The command Install or Sign in… runs, to copy.
    pub command: Option<String>,
    pub button: Option<ToolButton>,
    /// "Needs Homebrew first…".
    pub note: Option<String>,
    /// git's row while it has no name and email for commits: the form that saves them.
    pub identity: Option<crate::view_models::git_identity::Form>,
}

/// The whole page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "GitSettingsView")]
pub struct View {
    pub intro: String,
    pub git_title: String,
    /// `None` until the check answers (or when it failed: [`Self::error`]).
    pub git: Option<Row>,
    pub recheck_label: String,
    pub providers_title: String,
    pub providers: Vec<Row>,
    pub footnote: String,
    pub error: Option<String>,
    pub copy: String,
    pub terminal: ToolTerminalTexts,
}

/// A row's state while its check hasn't answered.
pub const CHECKING: &str = "Checking\u{2026}";

pub const INTRO: &str = "Installed once on this machine and used by every project. Each project chooses its provider in its Repository settings.";

/// The page before the tools check answers: its whole layout, git and each provider "Checking…", so the pane is
/// never empty while git, gh and az are looked up (slow to start on Windows).
pub fn checking() -> View {
    let row = |tool: Tool, name: &str, detail: String| Row {
        tool: tool.command().to_owned(),
        name: name.to_owned(),
        detail,
        state: CHECKING.to_owned(),
        look: Look::Checking,
        command: None,
        button: None,
        note: None,
        identity: None,
    };
    View {
        git: Some(row(Tool::Git, "git", String::new())),
        providers: [Tool::Gh, Tool::Az]
            .into_iter()
            .map(|tool| {
                row(
                    tool,
                    provider_name(tool),
                    format!("uses {}", tool.command()),
                )
            })
            .collect(),
        ..page(None, None)
    }
}

/// The page's fixed texts around its rows.
fn page(git: Option<Row>, error: Option<String>) -> View {
    View {
        intro: INTRO.to_owned(),
        git_title: "Git".to_owned(),
        git,
        recheck_label: "Re-check".to_owned(),
        providers_title: "Pull request providers".to_owned(),
        providers: Vec::new(),
        footnote: format!(
            "{PRODUCT_NAME} uses them to push and open pull requests; it never watches them."
        ),
        error,
        copy: "Copy".to_owned(),
        terminal: tool_terminal_texts(),
    }
}

/// The page from the tools check's answer, or why it failed.
pub fn view(result: Result<ToolsReport, String>) -> View {
    let error = result.as_ref().err().cloned();
    let mut model = OnboardingModel::new();
    model.start_check();
    let report = result.as_ref().ok().cloned();
    model.finish_check(result);
    let rows = model.rows();
    let status = |tool: Tool| {
        report
            .as_ref()
            .and_then(|r| r.tools.iter().find(|t| t.tool == tool))
    };
    let row_of = |tool: Tool| rows.iter().find(|r| r.tool == tool);
    let git = row_of(Tool::Git)
        .zip(status(Tool::Git))
        .map(|(r, s)| git_row(r, s));
    let providers = [Tool::Gh, Tool::Az]
        .into_iter()
        .filter_map(|tool| {
            row_of(tool)
                .zip(status(tool))
                .map(|(r, s)| provider_row(r, s))
        })
        .collect();
    View {
        providers,
        ..page(git, error)
    }
}

fn git_row(row: &ToolRow, s: &ToolStatus) -> Row {
    let (detail, state, look) = if s.found {
        let detail = [s.version.clone(), s.path.clone()]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join(" \u{b7} ");
        match &s.missing_identity {
            Some(_) => (detail, git_identity::STATE.to_owned(), Look::Attention),
            None => (detail, "Needed by every project".to_owned(), Look::Ready),
        }
    } else {
        (
            "Not found".to_owned(),
            row.explanation
                .clone()
                .unwrap_or_else(|| "Needed by every project".to_owned()),
            Look::Missing,
        )
    };
    Row {
        tool: Tool::Git.command().to_owned(),
        name: "git".to_owned(),
        detail,
        state,
        look,
        command: row.command.clone(),
        button: tool_button(row),
        note: row.note.clone(),
        identity: s
            .found
            .then_some(s.missing_identity.as_ref())
            .flatten()
            .map(git_identity::form),
    }
}

/// The provider a PR tool serves.
fn provider_name(tool: Tool) -> &'static str {
    match tool {
        Tool::Az => "Azure DevOps",
        _ => "GitHub",
    }
}

fn provider_row(row: &ToolRow, s: &ToolStatus) -> Row {
    let command = s.tool.command();
    let (detail, state, look) = if !s.found {
        (
            format!("uses {command}"),
            format!("{command} isn\u{2019}t installed"),
            Look::Missing,
        )
    } else {
        let detail = match &s.version {
            Some(v) => format!("uses {command} {v}"),
            None => format!("uses {command}"),
        };
        let (state, look) = if s.extension_installed == Some(false) {
            (
                "The azure-devops extension is missing".to_owned(),
                Look::Attention,
            )
        } else {
            match (s.logged_in, &s.account) {
                (Some(true), Some(account)) => (format!("Signed in as {account}"), Look::Ready),
                (Some(true), None) => ("Signed in".to_owned(), Look::Ready),
                (Some(false), _) => ("Not signed in".to_owned(), Look::Attention),
                (None, _) => ("Installed".to_owned(), Look::Ready),
            }
        };
        (detail, state, look)
    };
    Row {
        tool: command.to_owned(),
        name: provider_name(s.tool).to_owned(),
        detail,
        state,
        look,
        command: row.command.clone(),
        button: tool_button(row),
        note: row.note.clone(),
        identity: None,
    }
}

#[cfg(test)]
mod tests {
    use otaq_core::tools::{PermissionCheck, PermissionVerdict};

    use super::*;

    fn status(tool: Tool) -> ToolStatus {
        ToolStatus {
            tool,
            required: tool.required(),
            found: true,
            version: Some("2.62.0".into()),
            logged_in: None,
            extension_installed: None,
            install_command: None,
            login_command: None,
            missing_package_manager: None,
            path: Some(format!("/usr/bin/{}", tool.command())),
            account: None,
            missing_identity: None,
        }
    }

    fn report(tools: Vec<ToolStatus>) -> ToolsReport {
        ToolsReport {
            tools,
            permission: PermissionCheck {
                verdict: PermissionVerdict::Unknown,
                mode: None,
                source: None,
            },
            agent_tools: Vec::new(),
        }
    }

    #[test]
    fn git_shows_its_version_and_where_it_is_then_one_row_per_provider() {
        let git = ToolStatus {
            version: Some("2.46.0".into()),
            ..status(Tool::Git)
        };
        let gh = ToolStatus {
            logged_in: Some(true),
            account: Some("nt".into()),
            ..status(Tool::Gh)
        };
        let az = ToolStatus {
            version: Some("2.67.0".into()),
            extension_installed: Some(true),
            logged_in: Some(false),
            login_command: Some("az login".into()),
            ..status(Tool::Az)
        };
        let v = view(Ok(report(vec![git, gh, az])));
        let git = v.git.unwrap();
        assert_eq!(git.detail, "2.46.0 \u{b7} /usr/bin/git");
        assert_eq!(git.state, "Needed by every project");
        assert_eq!(git.look, Look::Ready);
        let rows: Vec<_> = v
            .providers
            .iter()
            .map(|r| (r.name.as_str(), r.detail.as_str(), r.state.as_str(), r.look))
            .collect();
        assert_eq!(
            rows,
            [
                ("GitHub", "uses gh 2.62.0", "Signed in as nt", Look::Ready),
                (
                    "Azure DevOps",
                    "uses az 2.67.0",
                    "Not signed in",
                    Look::Attention
                ),
            ]
        );
        // Sign in… runs the login in the row's terminal.
        assert_eq!(
            v.providers[1].button.as_ref().unwrap().label,
            "Sign in\u{2026}"
        );
    }

    #[test]
    fn a_provider_whose_tool_is_missing_offers_install() {
        let gh = ToolStatus {
            found: false,
            version: None,
            path: None,
            install_command: Some("brew install gh".into()),
            ..status(Tool::Gh)
        };
        let v = view(Ok(report(vec![status(Tool::Git), gh])));
        let gh = &v.providers[0];
        assert_eq!(
            (gh.state.as_str(), gh.look),
            ("gh isn\u{2019}t installed", Look::Missing)
        );
        assert_eq!(gh.button.as_ref().unwrap().label, "Install");
        assert_eq!(gh.command.as_deref(), Some("brew install gh"));
    }

    #[test]
    fn git_without_a_name_and_email_asks_for_them_on_its_row() {
        let git = ToolStatus {
            missing_identity: Some(otaq_core::git::Identity::default()),
            ..status(Tool::Git)
        };
        let v = view(Ok(report(vec![git])));
        let git = v.git.unwrap();
        assert_eq!(
            (git.state.as_str(), git.look),
            ("Needs your name and email", Look::Attention)
        );
        assert_eq!(git.identity.unwrap().save, "Save");
        // With them, no form.
        let v = view(Ok(report(vec![status(Tool::Git)])));
        assert!(v.git.unwrap().identity.is_none());
    }

    #[test]
    fn before_the_check_answers_every_row_is_drawn_checking() {
        let v = checking();
        let rows: Vec<_> = v
            .git
            .iter()
            .chain(&v.providers)
            .map(|r| (r.name.as_str(), r.detail.as_str(), r.state.as_str(), r.look))
            .collect();
        assert_eq!(
            rows,
            [
                ("git", "", CHECKING, Look::Checking),
                ("GitHub", "uses gh", CHECKING, Look::Checking),
                ("Azure DevOps", "uses az", CHECKING, Look::Checking),
            ]
        );
        assert!(v.git.unwrap().button.is_none() && v.error.is_none());
        assert_eq!(
            (v.git_title.as_str(), v.providers_title.as_str()),
            ("Git", "Pull request providers")
        );
    }

    #[test]
    fn a_failed_check_says_why_and_draws_no_rows() {
        let v = view(Err("the service is gone".into()));
        assert_eq!(v.error.as_deref(), Some("the service is gone"));
        assert!(v.git.is_none() && v.providers.is_empty());
    }
}
