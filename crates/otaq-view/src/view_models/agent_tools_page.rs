//! Settings → Agent tools and the agent tools of onboarding's Tools step (§20 Agent tools, SET-02, ADP-10, ADP-11,
//! UI-08, UI-10; d0086, d0085, d0011; *15* `bGHVl`/`ehz4J`, *14* `y2FCwP`/`vnBdi`, `z8OVF`/`rZRuR`). One row per
//! tool in d0086's order, from the service's `agent_tools.list` rows ([`ToolStatus`]): the avatar (always shown on
//! this page, even with one tool connected: it is how the user connects more), the name, the state pill and its
//! line (version, account), the actions (Sign in…, How to install, Show, the update command), what Otaq can see,
//! what its setup added, a setup that failed (amber), the permission warning, and the … menu (Hide / Show,
//! Re-check this tool, Disconnect…). Disconnect… asks first ([`disconnect_dialog`]).
//!
//! Nothing here branches on which tool a row is: every difference comes from the row's own fields, filled by each
//! tool's adapter (ADP-01). The one exception is the permission docs link, which only Claude Code's docs have.

use otaq_core::agent_tools::connected::{
    AgentToolState, SetupAction, SetupLine, SetupRun, ToolStatus,
};
use otaq_core::agent_tools::{Tool, WindowsSource};
use otaq_core::naming::PRODUCT_NAME;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::banners::PERMISSION_DOCS_URL;
use super::tool_avatar::ToolAvatarView;
use super::warm_build::ago;

/// Where a row is drawn: the onboarding rows are shorter (no Disconnect…, no "Otaq added", no notes).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Place {
    Settings,
    Onboarding,
}

/// The state pill's colour: green Connected, amber the found states that need something, grey the rest. Never
/// turquoise (d0077).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename = "AgentToolPillLook")]
pub enum PillLook {
    Connected,
    NotSignedIn,
    UpdateNeeded,
    NotInstalled,
    Hidden,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "AgentToolPill")]
pub struct Pill {
    /// "Connected", "Found · not signed in", "Found · update needed", "Not installed", "Hidden".
    pub label: String,
    pub look: PillLook,
}

/// One chip of "Otaq can see": ticked, or crossed out ("limits not reported").
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "AgentToolSeeChip")]
pub struct SeeChip {
    pub label: String,
    pub seen: bool,
}

/// "Otaq can see" and its chips: permission prompts, questions, idle, tokens and limits (CHANGES-v §9).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "AgentToolCanSee")]
pub struct CanSee {
    pub label: String,
    pub chips: Vec<SeeChip>,
}

/// **How to install**: the command for this OS, to copy; the row's **Install** runs it on the user's click.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "AgentToolCommand")]
pub struct CommandView {
    /// "How to install (macOS)", "Update GitHub Copilot to a version Otaq runs".
    pub label: String,
    pub command: String,
    pub copy: String,
}

/// What Otaq's setup wrote into the tool's own config, as quiet text, and which part stays on Disconnect.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "AgentToolAdded")]
pub struct Added {
    /// "Otaq added: trust for your projects, its hooks, otaq pre-approved, its skills".
    pub text: String,
    /// "Trust for your projects stays when you disconnect."
    pub stays: Option<String>,
}

/// The permission warning of one tool (SET-02, d0011), amber: its title (the adapter's), what it means, and the
/// docs link where the tool has one (`agent_tools.open_docs`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "AgentToolWarning")]
pub struct Warning {
    pub title: String,
    pub text: String,
    pub link: Option<String>,
}

/// An entry of the row's … menu, and what it sends.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename = "AgentToolMenuAction")]
pub enum MenuAction {
    /// `agent_tools.set_hidden {hidden: true}`.
    Hide,
    /// `agent_tools.set_hidden {hidden: false}`.
    Show,
    /// `agent_tools.recheck {tool}`.
    Recheck,
    /// Opens [`disconnect_dialog`] (`agent_tools.disconnect_dialog`).
    Disconnect,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "AgentToolMenuItem")]
pub struct MenuItem {
    pub action: MenuAction,
    pub label: String,
    pub destructive: bool,
}

/// One tool's row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "AgentToolRow")]
pub struct Row {
    /// The tool as requests name it (`codex`).
    pub tool: String,
    pub avatar: ToolAvatarView,
    /// "Codex", "GitHub Copilot".
    pub name: String,
    /// Settings only; the onboarding rows say the state in [`Self::detail`].
    pub pill: Option<Pill>,
    /// "v0.157.1 · signed in with ChatGPT", "v1.0.2 · not signed in", "not installed · optional now".
    pub detail: String,
    /// **Sign in…** (`agent_tools.sign_in`).
    pub sign_in: Option<String>,
    /// **Show** for a hidden tool (`agent_tools.set_hidden {hidden: false}`).
    pub show: Option<String>,
    pub install: Option<CommandView>,
    /// **Install** (`agent_tools.install`): runs the tool's install command in a terminal under the row, on the user's
    /// click. Every missing tool has it, also an optional one on onboarding whose [`Self::install`] isn't shown.
    pub install_button: Option<String>,
    /// Why there is no Install: "Needs Node.js (npm) first…".
    pub install_note: Option<String>,
    /// Found · update needed: the command that updates it (Q-G).
    pub update: Option<CommandView>,
    pub can_see: Option<CanSee>,
    /// Quiet lines under the chips: where the limits come from, a reader's note ("Codex: limits not reported").
    pub notes: Vec<String>,
    pub added: Option<Added>,
    /// The last setup or Disconnect that couldn't write an entry, amber.
    pub setup_failed: Option<String>,
    pub warning: Option<Warning>,
    /// The … menu; empty when it has nothing.
    pub menu: Vec<MenuItem>,
}

/// Settings → Agent tools.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "AgentToolsPage")]
pub struct Page {
    pub title: String,
    pub intro: String,
    pub rows: Vec<Row>,
    /// Re-check for the page (`agent_tools.recheck {}`).
    pub recheck: String,
    /// "Checked 2 minutes ago"; none before the first check.
    pub checked: Option<String>,
    /// The … button's name.
    pub menu_label: String,
    pub not_shown_title: String,
    pub not_shown: String,
    /// The last request failed (the service is unreachable).
    pub error: Option<String>,
    /// The words of Sign in… and Install's terminal under a row.
    pub terminal: super::onboarding_page::ToolTerminalTexts,
}

/// The rows of `agent_tools.list` as the page draws them; `now` (Unix ms) dates "Checked …".
pub fn page(statuses: &[ToolStatus], now: i64) -> Page {
    let checked = statuses.iter().filter_map(|s| s.checked_at).max();
    Page {
        title: "Agent tools".to_owned(),
        intro: format!(
            "The AI tools on your machine. {PRODUCT_NAME} never asks for passwords or keys and never changes their permission settings. Install and Sign in\u{2026} run the tool\u{2019}s own command in a terminal here, on your click."
        ),
        rows: statuses
            .iter()
            .map(|s| row(s, Place::Settings, false))
            .collect(),
        recheck: "Re-check".to_owned(),
        checked: checked.map(|at| format!("Checked {}", ago(now - at))),
        menu_label: "More actions".to_owned(),
        not_shown_title: "Not shown here".to_owned(),
        not_shown: "git, gh and az stay under Tools: they aren\u{2019}t agents.".to_owned(),
        error: None,
        terminal: super::onboarding_page::tool_terminal_texts(),
    }
}

/// The page when `agent_tools.list` failed: no rows, and why.
pub fn failed(error: String) -> Page {
    Page {
        error: Some(error),
        ..page(&[], 0)
    }
}

/// One tool's row. `none_connected`: onboarding's "none connected" card, where a missing tool shows its install
/// command (with one connected, the others are "optional now").
pub fn row(s: &ToolStatus, place: Place, none_connected: bool) -> Row {
    let tool = s.tool;
    let name = tool.display_name();
    let version = s.state.version().map(version_label);
    let with_version = |text: &str| match &version {
        Some(v) => format!("{v} \u{b7} {text}"),
        None => text.to_owned(),
    };
    let settings = place == Place::Settings;
    let (look, pill, detail) = match &s.state {
        AgentToolState::Connected { account, .. } => (
            PillLook::Connected,
            "Connected",
            with_version(account.as_deref().unwrap_or("signed in")),
        ),
        AgentToolState::FoundNotSignedIn { .. } => (
            PillLook::NotSignedIn,
            "Found \u{b7} not signed in",
            if settings {
                with_version("not signed in")
            } else {
                "found \u{b7} not signed in".to_owned()
            },
        ),
        AgentToolState::UpdateNeeded { .. } => (
            PillLook::UpdateNeeded,
            "Found \u{b7} update needed",
            with_version("too old for this app"),
        ),
        AgentToolState::NotInstalled => (
            PillLook::NotInstalled,
            "Not installed",
            if settings || none_connected {
                "not installed"
            } else {
                "not installed \u{b7} optional now"
            }
            .to_owned(),
        ),
        AgentToolState::Hidden { .. } => (
            PillLook::Hidden,
            "Hidden",
            if settings {
                "Found, hidden by you"
            } else {
                "hidden by you"
            }
            .to_owned(),
        ),
    };
    let connected = s.state.is_connected();
    let install = (s.state == AgentToolState::NotInstalled && (settings || none_connected))
        .then(|| s.install_command.clone())
        .flatten()
        .map(|command| CommandView {
            label: format!("How to install ({})", os_name()),
            command,
            copy: "Copy".to_owned(),
        });
    // Install is offered for every missing tool, also an optional one on onboarding (where its command isn't shown).
    let installable = s.state == AgentToolState::NotInstalled && s.install_command.is_some();
    let install_button = (installable && s.can_install).then(|| "Install".to_owned());
    let install_note = installable
        .then_some(s.install_needs.as_deref())
        .flatten()
        .map(|manager| match manager {
            "npm" => "Needs Node.js (npm) first. Install it, then Re-check.".to_owned(),
            other => format!("Needs {other} first. Install it, then Re-check."),
        });
    let update = matches!(s.state, AgentToolState::UpdateNeeded { .. })
        .then(|| s.install_command.clone())
        .flatten()
        .map(|command| CommandView {
            label: format!("Update {name} to a version {PRODUCT_NAME} runs"),
            command,
            copy: "Copy".to_owned(),
        });
    let mut notes = Vec::new();
    if settings && connected {
        if s.limits == Some(WindowsSource::ToolFiles) {
            notes.push(format!(
                "Limits come from {name}\u{2019}s session file. If {PRODUCT_NAME} can\u{2019}t read it, Running shows \u{201c}Limits not reported\u{201d}."
            ));
        }
        notes.extend(s.notes.iter().map(|note| format!("{name}: {note}")));
    }
    Row {
        tool: tool.as_str().to_owned(),
        avatar: ToolAvatarView::of(tool, None),
        name: name.to_owned(),
        pill: settings.then(|| Pill {
            label: pill.to_owned(),
            look,
        }),
        detail,
        sign_in: s.can_sign_in.then(|| "Sign in\u{2026}".to_owned()),
        show: matches!(s.state, AgentToolState::Hidden { .. }).then(|| "Show".to_owned()),
        install,
        install_button,
        install_note,
        update,
        can_see: connected.then(|| can_see(s, place)),
        notes,
        added: (settings && connected)
            .then(|| added(&s.setup_entries))
            .flatten(),
        setup_failed: settings
            .then(|| s.setup.as_ref().and_then(setup_failed))
            .flatten(),
        warning: s.permission_warning.as_ref().map(|title| Warning {
            title: title.clone(),
            text: format!(
                "With 5\u{2013}10 agents, prompts will stop agents often. {PRODUCT_NAME} won\u{2019}t change your settings."
            ),
            link: docs_url(tool).map(|_| format!("{name} docs \u{2197}")),
        }),
        menu: menu(s, place),
    }
}

/// "2.1.4" → "v2.1.4"; a version printed with words before it ("Claude Code 2.1.4") keeps only its number.
fn version_label(version: &str) -> String {
    let number = version
        .split_whitespace()
        .find(|w| w.starts_with(|c: char| c.is_ascii_digit()))
        .unwrap_or(version);
    format!("v{number}")
}

fn os_name() -> &'static str {
    if cfg!(target_os = "macos") {
        "macOS"
    } else if cfg!(windows) {
        "Windows"
    } else {
        "Linux"
    }
}

fn can_see(s: &ToolStatus, place: Place) -> CanSee {
    let prompts = match place {
        Place::Settings => "permission prompts",
        Place::Onboarding => "prompts",
    };
    let limits_seen = s.limits != Some(WindowsSource::NotReported);
    let mut chips: Vec<SeeChip> = [prompts, "questions", "idle", "tokens"]
        .into_iter()
        .map(|label| SeeChip {
            label: label.to_owned(),
            seen: true,
        })
        .collect();
    chips.push(SeeChip {
        label: if limits_seen {
            "limits"
        } else {
            "limits not reported"
        }
        .to_owned(),
        seen: limits_seen,
    });
    CanSee {
        label: format!("{PRODUCT_NAME} can see"),
        chips,
    }
}

/// "Otaq added: …" from the adapter's entries, with the part Disconnect leaves; none without entries.
fn added(entries: &[SetupLine]) -> Option<Added> {
    if entries.is_empty() {
        return None;
    }
    let all: Vec<&str> = entries.iter().map(|e| e.text.as_str()).collect();
    let stays: Vec<&str> = entries
        .iter()
        .filter(|e| !e.removed_on_disconnect)
        .map(|e| e.text.as_str())
        .collect();
    Some(Added {
        text: format!("{PRODUCT_NAME} added: {}", all.join(", ")),
        stays: (!stays.is_empty()).then(|| {
            format!(
                "{} stays when you disconnect.",
                capitalized(&and_list(&stays))
            )
        }),
    })
}

/// The entries the last setup or Disconnect couldn't write, with their reasons.
fn setup_failed(run: &SetupRun) -> Option<String> {
    if run.failed.is_empty() {
        return None;
    }
    let verb = match run.action {
        SetupAction::SetUp => "add",
        SetupAction::Removed => "remove",
    };
    let each: Vec<String> = run
        .failed
        .iter()
        .map(|f| format!("{} ({})", f.text, f.reason))
        .collect();
    Some(format!(
        "{PRODUCT_NAME} couldn\u{2019}t {verb} {}.",
        and_list(&each)
    ))
}

fn menu(s: &ToolStatus, place: Place) -> Vec<MenuItem> {
    let item = |action, label: &str, destructive| MenuItem {
        action,
        label: label.to_owned(),
        destructive,
    };
    let hide = item(
        MenuAction::Hide,
        &format!("Hide from {PRODUCT_NAME}"),
        false,
    );
    let recheck = item(MenuAction::Recheck, "Re-check this tool", false);
    let mut entries = match s.state {
        AgentToolState::NotInstalled => vec![recheck],
        AgentToolState::Hidden { .. } => vec![item(MenuAction::Show, "Show", false), recheck],
        _ => vec![hide, recheck],
    };
    if place == Place::Settings && s.can_disconnect {
        entries.push(item(MenuAction::Disconnect, "Disconnect\u{2026}", true));
    }
    entries
}

/// The docs a permission warning links to, where the tool has a page about allowlists and auto mode.
pub fn docs_url(tool: Tool) -> Option<&'static str> {
    (tool == Tool::Claude).then_some(PERMISSION_DOCS_URL)
}

/// Disconnect…'s confirmation (*15* `bGHVl` → … menu → Note; d0086, Q-F), or its refusal while agents of the tool
/// run ("Codex has 2 agents running. End them first.", the service's words).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "AgentToolDisconnectDialog")]
pub struct DisconnectDialog {
    pub tool: String,
    pub title: String,
    /// "Removes only what Otaq added (its hooks, otaq pre-approved, its skills)."
    pub text: String,
    /// "Otaq added trust for your projects; it stays when you disconnect."
    pub stays: Option<String>,
    /// "Codex is then hidden: it doesn't count until you show it again."
    pub hidden: String,
    pub cancel: String,
    pub confirm: String,
    /// The refusal, in the dialog's error style; Disconnect is then off.
    pub error: Option<String>,
    pub can_confirm: bool,
}

pub fn disconnect_dialog(s: &ToolStatus, error: Option<String>) -> DisconnectDialog {
    let name = s.tool.display_name();
    let removed: Vec<&str> = s
        .setup_entries
        .iter()
        .filter(|e| e.removed_on_disconnect)
        .map(|e| e.text.as_str())
        .collect();
    let stays: Vec<&str> = s
        .setup_entries
        .iter()
        .filter(|e| !e.removed_on_disconnect)
        .map(|e| e.text.as_str())
        .collect();
    let text = if removed.is_empty() {
        format!("Removes only what {PRODUCT_NAME} added.")
    } else {
        format!(
            "Removes only what {PRODUCT_NAME} added ({}).",
            removed.join(", ")
        )
    };
    DisconnectDialog {
        tool: s.tool.as_str().to_owned(),
        title: format!("Disconnect {name}?"),
        text,
        stays: (!stays.is_empty()).then(|| {
            format!(
                "{PRODUCT_NAME} added {}; it stays when you disconnect.",
                and_list(&stays)
            )
        }),
        hidden: format!("{name} is then hidden: it doesn\u{2019}t count until you show it again."),
        cancel: "Cancel".to_owned(),
        confirm: "Disconnect".to_owned(),
        can_confirm: error.is_none() && s.can_disconnect,
        error,
    }
}

/// "a", "a and b", "a, b and c".
pub fn and_list<S: AsRef<str>>(items: &[S]) -> String {
    match items {
        [] => String::new(),
        [one] => one.as_ref().to_owned(),
        [rest @ .., last] => format!(
            "{} and {}",
            rest.iter()
                .map(AsRef::as_ref)
                .collect::<Vec<_>>()
                .join(", "),
            last.as_ref()
        ),
    }
}

fn capitalized(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

/// One cell of the gallery: a name and the rows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "AgentToolsGalleryCell")]
pub struct GalleryCell {
    pub name: String,
    pub page: Page,
}

/// The gallery (T-0365 draws it): every state for each tool, the notes, the Disconnect dialog and its refusal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "AgentToolsGallery")]
pub struct Gallery {
    pub pages: Vec<GalleryCell>,
    pub dialogs: Vec<DisconnectDialog>,
}

pub mod fixtures {
    //! Made-up rows in the shapes the adapters fill, for the gallery and the tests (the frame's samples).
    use otaq_core::agent_tools::connected::{SetupFailure, SetupLine, SetupRun};
    use otaq_core::signals::watch::LIMITS_NOT_REPORTED;

    use super::*;

    pub const NOW: i64 = 1_800_000_000_000;

    /// What each tool's adapter gives, in [`Tool::ALL`]'s order (Claude Code, Codex, Copilot, OpenCode): its
    /// install command, where its limits come from, Otaq's entries (text, removed on Disconnect; `{cmd}` is the
    /// command's name) and whether it has a sign-in command.
    type AdapterSample = (
        &'static str,
        WindowsSource,
        &'static [(&'static str, bool)],
        bool,
    );
    const TRUST: (&str, bool) = ("trust for your projects", false);
    const ADAPTERS: [AdapterSample; 4] = [
        (
            "npm install -g @anthropic-ai/claude-code",
            WindowsSource::Report,
            &[TRUST],
            false,
        ),
        (
            "brew install codex",
            WindowsSource::ToolFiles,
            &[
                TRUST,
                ("its hooks", true),
                ("{cmd} pre-approved", true),
                ("its skills", true),
            ],
            true,
        ),
        (
            "npm install -g @github/copilot",
            WindowsSource::ToolFiles,
            &[TRUST],
            true,
        ),
        (
            "brew install opencode",
            WindowsSource::NotReported,
            &[],
            true,
        ),
    ];

    /// The four tools, in [`Tool::ALL`]'s order.
    pub fn tools() -> [Tool; 4] {
        let &[claude, codex, copilot, opencode] = Tool::ALL else {
            unreachable!("four agent tools")
        };
        [claude, codex, copilot, opencode]
    }

    /// A row in `state`, checked two minutes ago, with what each tool's adapter gives for it.
    pub fn status(tool: Tool, state: AgentToolState) -> ToolStatus {
        let i = tools().iter().position(|&t| t == tool).unwrap_or_default();
        let (install, limits, entries, signs_in) = ADAPTERS[i];
        let connected = state.is_connected();
        let found = state != AgentToolState::NotInstalled;
        ToolStatus {
            tool,
            checked_at: Some(NOW - 120_000),
            notes: Vec::new(),
            install_command: Some(install.to_owned()),
            setup_entries: entries
                .iter()
                .map(|(text, removed)| SetupLine {
                    text: text.replace("{cmd}", otaq_core::naming::COMMAND_NAME),
                    removed_on_disconnect: *removed,
                })
                .collect(),
            setup: None,
            permission_warning: None,
            can_sign_in: matches!(state, AgentToolState::FoundNotSignedIn { .. }) && signs_in,
            can_install: matches!(state, AgentToolState::NotInstalled),
            install_needs: None,
            can_disconnect: connected,
            limits: found.then_some(limits),
            state,
        }
    }

    pub fn connected(version: &str, account: Option<&str>) -> AgentToolState {
        AgentToolState::Connected {
            version: Some(version.to_owned()),
            account: account.map(str::to_owned),
        }
    }

    /// The frame's sample: Claude Code and Codex connected, Copilot found but signed out, OpenCode connected.
    pub fn sample() -> Vec<ToolStatus> {
        let [claude, codex, copilot, opencode] = tools();
        let mut codex = status(codex, connected("0.157.1", Some("signed in with ChatGPT")));
        codex.permission_warning = Some("Your Codex settings ask for permission prompts".into());
        vec![
            status(claude, connected("Claude Code 2.1.301", None)),
            codex,
            status(
                copilot,
                AgentToolState::FoundNotSignedIn {
                    version: Some("1.0.2".into()),
                },
            ),
            status(
                opencode,
                connected("2.0.17", Some("2 providers (OpenAI, Google)")),
            ),
        ]
    }

    /// Every state for `tool`.
    pub fn every_state(tool: Tool) -> Vec<ToolStatus> {
        let v = |v: &str| Some(v.to_owned());
        vec![
            status(tool, connected("1.0.98", Some("signed in as mira-dev"))),
            status(
                tool,
                AgentToolState::FoundNotSignedIn {
                    version: v("1.0.98"),
                },
            ),
            status(
                tool,
                AgentToolState::UpdateNeeded {
                    version: v("0.0.404"),
                },
            ),
            status(tool, AgentToolState::NotInstalled),
            status(
                tool,
                AgentToolState::Hidden {
                    version: v("1.0.98"),
                },
            ),
        ]
    }

    /// Codex with its reader's note and a setup that couldn't write its skills.
    pub fn codex_with_notes() -> ToolStatus {
        let mut codex = status(
            tools()[1],
            connected("0.157.1", Some("signed in with ChatGPT")),
        );
        codex.notes = vec![LIMITS_NOT_REPORTED.to_owned()];
        codex.setup = Some(SetupRun {
            action: SetupAction::SetUp,
            at: NOW - 60_000,
            done: Vec::new(),
            failed: vec![SetupFailure {
                text: "its skills".into(),
                reason: "the plugin folder couldn\u{2019}t be written".into(),
            }],
        });
        codex
    }
}

pub fn gallery() -> Gallery {
    use fixtures::*;
    let mut pages = vec![GalleryCell {
        name: "15 · Agent tools · sample".to_owned(),
        page: page(&sample(), NOW),
    }];
    for tool in Tool::ALL.iter().copied() {
        pages.push(GalleryCell {
            name: format!("{} · every state", tool.display_name()),
            page: page(&every_state(tool), NOW),
        });
    }
    pages.push(GalleryCell {
        name: "Codex · limits not reported, a setup that failed".to_owned(),
        page: page(&[codex_with_notes()], NOW),
    });
    let codex = status(
        tools()[1],
        connected("0.157.1", Some("signed in with ChatGPT")),
    );
    Gallery {
        pages,
        dialogs: vec![
            disconnect_dialog(&codex, None),
            disconnect_dialog(
                &codex,
                Some("Codex has 2 agents running. End them first.".into()),
            ),
        ],
    }
}

#[cfg(test)]
mod tests {
    use otaq_core::naming::COMMAND_NAME;

    use super::fixtures::*;
    use super::*;

    fn settings_row(s: &ToolStatus) -> Row {
        row(s, Place::Settings, false)
    }

    fn menu_actions(r: &Row) -> Vec<MenuAction> {
        r.menu.iter().map(|m| m.action).collect()
    }

    #[test]
    fn each_state_has_its_pill_line_and_actions_for_every_tool() {
        use MenuAction::*;
        for tool in Tool::ALL.iter().copied() {
            let rows: Vec<Row> = every_state(tool).iter().map(settings_row).collect();
            let pills: Vec<(&str, PillLook)> = rows
                .iter()
                .map(|r| {
                    let p = r.pill.as_ref().unwrap();
                    (p.label.as_str(), p.look)
                })
                .collect();
            assert_eq!(
                pills,
                [
                    ("Connected", PillLook::Connected),
                    ("Found · not signed in", PillLook::NotSignedIn),
                    ("Found · update needed", PillLook::UpdateNeeded),
                    ("Not installed", PillLook::NotInstalled),
                    ("Hidden", PillLook::Hidden),
                ],
                "{tool:?}"
            );
            let details: Vec<&str> = rows.iter().map(|r| r.detail.as_str()).collect();
            assert_eq!(
                details,
                [
                    "v1.0.98 · signed in as mira-dev",
                    "v1.0.98 · not signed in",
                    "v0.0.404 · too old for this app",
                    "not installed",
                    "Found, hidden by you",
                ]
            );
            let [connected, signed_out, update, missing, hidden] = &rows[..] else {
                unreachable!()
            };
            assert_eq!(connected.name, tool.display_name());
            assert_eq!(connected.tool, tool.as_str());
            assert_eq!(connected.avatar.letters.len(), 2, "always shown here");
            assert_eq!(menu_actions(connected), [Hide, Recheck, Disconnect]);
            assert!(connected.sign_in.is_none() && connected.install.is_none());
            // Sign in… only where the service says it applies (Claude Code has no sign-in command).
            assert_eq!(signed_out.sign_in.is_some(), tool != Tool::Claude);
            assert_eq!(menu_actions(signed_out), [Hide, Recheck]);
            assert!(signed_out.can_see.is_none());
            let cmd = update.update.as_ref().unwrap();
            assert_eq!(
                cmd.label,
                format!(
                    "Update {} to a version {PRODUCT_NAME} runs",
                    tool.display_name()
                )
            );
            assert!(update.install.is_none());
            let install = missing.install.as_ref().unwrap();
            assert!(install.label.starts_with("How to install ("));
            assert_eq!(&install.command, cmd.command.as_str());
            assert_eq!(menu_actions(missing), [Recheck]);
            assert_eq!(hidden.show.as_deref(), Some("Show"));
            assert_eq!(menu_actions(hidden), [Show, Recheck]);
            assert!(hidden.can_see.is_none() && hidden.added.is_none());
        }
    }

    #[test]
    fn what_otaq_can_see_per_tool() {
        let rows: Vec<Row> = sample()
            .iter()
            .map(|s| {
                let mut s = s.clone();
                s.state = connected("1", None);
                settings_row(&s)
            })
            .collect();
        let chips = |r: &Row| -> Vec<(String, bool)> {
            r.can_see
                .as_ref()
                .unwrap()
                .chips
                .iter()
                .map(|c| (c.label.clone(), c.seen))
                .collect()
        };
        let seen = |limits: &str, seen: bool| {
            ["permission prompts", "questions", "idle", "tokens", limits]
                .iter()
                .enumerate()
                .map(|(i, l)| (l.to_string(), i < 4 || seen))
                .collect::<Vec<_>>()
        };
        for r in &rows[..3] {
            assert_eq!(chips(r), seen("limits", true), "{}", r.name);
        }
        assert_eq!(chips(&rows[3]), seen("limits not reported", false));
        assert_eq!(
            rows[0].can_see.as_ref().unwrap().label,
            format!("{PRODUCT_NAME} can see")
        );
        // Onboarding's shorter chip.
        let onboarding = row(&sample()[0], Place::Onboarding, false);
        assert_eq!(onboarding.can_see.unwrap().chips[0].label, "prompts");
    }

    #[test]
    fn codex_says_where_its_limits_come_from_and_when_they_are_not_reported() {
        let quiet = settings_row(&sample()[1]);
        assert_eq!(
            quiet.notes,
            [format!(
                "Limits come from Codex’s session file. If {PRODUCT_NAME} can’t read it, Running shows “Limits not reported”."
            )]
        );
        let noted = settings_row(&codex_with_notes());
        assert_eq!(noted.notes[1], "Codex: limits not reported");
        assert_eq!(
            noted.setup_failed.as_deref(),
            Some(
                format!(
                    "{PRODUCT_NAME} couldn’t add its skills (the plugin folder couldn’t be written)."
                )
                .as_str()
            )
        );
        // Claude Code's limits come from its report: no note; OpenCode's aren't reported: the chip says so.
        assert!(settings_row(&sample()[0]).notes.is_empty());
        assert!(settings_row(&sample()[3]).notes.is_empty());
    }

    #[test]
    fn otaq_added_names_what_stays_on_disconnect() {
        let codex = settings_row(&sample()[1]).added.unwrap();
        assert_eq!(
            codex.text,
            format!(
                "{PRODUCT_NAME} added: trust for your projects, its hooks, {COMMAND_NAME} pre-approved, its skills"
            )
        );
        assert_eq!(
            codex.stays.as_deref(),
            Some("Trust for your projects stays when you disconnect.")
        );
        // OpenCode's setup writes nothing; onboarding rows don't list it.
        assert!(settings_row(&sample()[3]).added.is_none());
        assert!(row(&sample()[1], Place::Onboarding, false).added.is_none());
    }

    #[test]
    fn the_warning_is_per_tool_with_the_docs_link_where_there_are_docs() {
        let rows: Vec<Row> = sample().iter().map(settings_row).collect();
        let codex = rows[1].warning.as_ref().unwrap();
        assert_eq!(
            codex.title,
            "Your Codex settings ask for permission prompts"
        );
        assert_eq!(codex.link, None);
        assert!(rows[0].warning.is_none() && rows[2].warning.is_none());
        let mut claude = sample()[0].clone();
        claude.permission_warning =
            Some("Your Claude Code settings ask for permission prompts".into());
        let warning = settings_row(&claude).warning.unwrap();
        assert_eq!(warning.link.as_deref(), Some("Claude Code docs ↗"));
        assert_eq!(
            warning.text,
            format!(
                "With 5–10 agents, prompts will stop agents often. {PRODUCT_NAME} won’t change your settings."
            )
        );
        assert_eq!(docs_url(Tool::Claude), Some(PERMISSION_DOCS_URL));
        assert_eq!(docs_url(Tool::Codex), None);
    }

    #[test]
    fn update_needed_offers_the_command() {
        let copilot = status(
            Tool::Copilot,
            AgentToolState::UpdateNeeded {
                version: Some("0.0.404".into()),
            },
        );
        let r = settings_row(&copilot);
        assert_eq!(r.detail, "v0.0.404 · too old for this app");
        let update = r.update.unwrap();
        assert_eq!(update.command, "npm install -g @github/copilot");
        assert_eq!(update.copy, "Copy");
        assert!(r.sign_in.is_none() && r.can_see.is_none());
    }

    #[test]
    fn the_page_lists_all_four_and_says_when_it_checked() {
        let p = page(&sample(), NOW);
        assert_eq!(p.title, "Agent tools");
        assert_eq!(p.rows.len(), 4);
        assert_eq!(p.checked.as_deref(), Some("Checked 2 minutes ago"));
        assert_eq!(p.rows[0].detail, "v2.1.301 · signed in");
        assert_eq!(p.rows[1].detail, "v0.157.1 · signed in with ChatGPT");
        assert_eq!(
            p.not_shown,
            "git, gh and az stay under Tools: they aren’t agents."
        );
        // With one tool connected the page still lists every tool.
        let one: Vec<ToolStatus> = Tool::ALL
            .iter()
            .map(|&t| {
                if t == Tool::Claude {
                    status(t, connected("2.1.4", None))
                } else {
                    status(t, AgentToolState::NotInstalled)
                }
            })
            .collect();
        let p = page(&one, NOW);
        assert_eq!(p.rows.len(), 4);
        assert!(p.rows.iter().all(|r| r.avatar.letters.len() == 2));
        assert_eq!(failed("down".into()).error.as_deref(), Some("down"));
        assert_eq!(page(&[], NOW).checked, None);
    }

    #[test]
    fn disconnect_asks_first_and_says_what_stays() {
        let codex = &sample()[1];
        let d = disconnect_dialog(codex, None);
        assert_eq!(d.title, "Disconnect Codex?");
        assert_eq!(
            d.text,
            format!(
                "Removes only what {PRODUCT_NAME} added (its hooks, {COMMAND_NAME} pre-approved, its skills)."
            )
        );
        assert_eq!(
            d.stays.as_deref(),
            Some(
                format!(
                    "{PRODUCT_NAME} added trust for your projects; it stays when you disconnect."
                )
                .as_str()
            )
        );
        assert_eq!(
            d.hidden,
            "Codex is then hidden: it doesn’t count until you show it again."
        );
        assert_eq!(
            (d.cancel.as_str(), d.confirm.as_str()),
            ("Cancel", "Disconnect")
        );
        assert!(d.can_confirm);
        // Refused while agents run: the service's words, Disconnect off.
        let refused = disconnect_dialog(
            codex,
            Some("Codex has 2 agents running. End them first.".into()),
        );
        assert_eq!(
            refused.error.as_deref(),
            Some("Codex has 2 agents running. End them first.")
        );
        assert!(!refused.can_confirm);
        // A tool whose setup wrote only folder trust.
        let claude = disconnect_dialog(&sample()[0], None);
        assert_eq!(
            claude.text,
            format!("Removes only what {PRODUCT_NAME} added.")
        );
        assert!(claude.stays.is_some());
    }

    #[test]
    fn onboarding_rows_are_shorter() {
        let rows: Vec<Row> = sample()
            .iter()
            .map(|s| row(s, Place::Onboarding, false))
            .collect();
        assert!(rows.iter().all(|r| r.pill.is_none()));
        assert!(
            rows.iter()
                .all(|r| !r.menu.iter().any(|m| m.action == MenuAction::Disconnect))
        );
        assert_eq!(rows[2].detail, "found · not signed in");
        assert!(rows[2].sign_in.is_some());
        let missing = status(Tool::Codex, AgentToolState::NotInstalled);
        let optional = row(&missing, Place::Onboarding, false);
        assert_eq!(optional.detail, "not installed · optional now");
        assert!(optional.install.is_none());
        assert_eq!(
            optional.install_button.as_deref(),
            Some("Install"),
            "an optional tool can still be installed"
        );
        let no_npm = ToolStatus {
            can_install: false,
            install_needs: Some("npm".to_owned()),
            ..missing.clone()
        };
        let waiting = row(&no_npm, Place::Onboarding, false);
        assert!(waiting.install_button.is_none());
        assert_eq!(
            waiting.install_note.as_deref(),
            Some("Needs Node.js (npm) first. Install it, then Re-check.")
        );
        let needed = row(&missing, Place::Onboarding, true);
        assert_eq!(needed.detail, "not installed");
        assert_eq!(needed.install.unwrap().command, "brew install codex");
        let hidden = row(
            &status(Tool::OpenCode, AgentToolState::Hidden { version: None }),
            Place::Onboarding,
            false,
        );
        assert_eq!(hidden.detail, "hidden by you");
        assert_eq!(hidden.show.as_deref(), Some("Show"));
    }

    #[test]
    fn lists_read_as_sentences() {
        assert_eq!(and_list::<&str>(&[]), "");
        assert_eq!(and_list(&["a"]), "a");
        assert_eq!(and_list(&["a", "b"]), "a and b");
        assert_eq!(and_list(&["a", "b", "c"]), "a, b and c");
        assert_eq!(version_label("Claude Code 2.1.4"), "v2.1.4");
        assert_eq!(version_label("0.157.1"), "v0.157.1");
    }

    #[test]
    fn the_gallery_has_every_state_per_tool_and_both_dialogs() {
        let g = gallery();
        assert_eq!(g.pages.len(), 6);
        assert!(g.pages[1..5].iter().all(|c| c.page.rows.len() == 5));
        assert_eq!(g.dialogs.len(), 2);
        assert!(g.dialogs[1].error.is_some());
    }
}
