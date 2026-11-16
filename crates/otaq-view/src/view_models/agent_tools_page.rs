//! Settings → Tools and the agent tools of onboarding's Tools step (§20 Agent tools, SET-02, ADP-10, ADP-11,
//! UI-08, UI-10; d0086, d0085, d0011; *15* `bGHVl`/`ehz4J`, *14* `y2FCwP`/`vnBdi`, `z8OVF`/`rZRuR`; UX-65, UX-70,
//! canvas A-8). One row per
//! tool in d0086's order, from the service's `agent_tools.list` rows ([`ToolStatus`]): the avatar (always shown on
//! this page, even with one tool connected: it is how the user connects more), the name, the state pill and its
//! line (version, account), the actions (Sign in…, How to install, Show, the update command), what Otaq can see,
//! what its setup added, a setup that failed (amber), the permission warning, and the … menu (Hide / Show,
//! Re-check this tool, Disconnect…). Disconnect… asks first ([`disconnect_dialog`]).
//!
//! Under the rows, Settings → Tools has **Noyan starts on** ([`Noyan`]: the model and effort every new Noyan
//! conversation starts on, the shared model field of [`super::model_effort`], UX-70) and **Where they're used**
//! ([`Used`], read-only: per tool, the projects' stages and Noyan on it). [`with_flows`] adds both to the page.
//!
//! Nothing here branches on which tool a row is: every difference comes from the row's own fields, filled by each
//! tool's adapter (ADP-01). The one exception is the permission docs link, which only Claude Code's docs have.

use otaq_core::agent_tools::connected::{
    AgentToolState, SetupAction, SetupLine, SetupRun, ToolStatus,
};
use otaq_core::agent_tools::{Tool, WindowsSource};
use otaq_core::agents::{AgentModel, NoyanStartsOn};
use otaq_core::naming::PRODUCT_NAME;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::machine::this_machine;

use super::banners::PERMISSION_DOCS_URL;
use super::model_effort::{self, Use};
use super::tool_avatar::ToolAvatarView;
use super::tool_problems::{Tools, tool_word};
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
    /// Its sign-in answer couldn't be read: amber, as signed out.
    CouldNotCheck,
    UpdateNeeded,
    /// Found, but this app won't use it (Codex with a data folder of its own and no `CODEX_HOME`): amber.
    NotUsable,
    NotInstalled,
    Hidden,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "AgentToolPill")]
pub struct Pill {
    /// "Connected", "Found · not signed in", "Found · couldn't check", "Found · update needed", "Found · not
    /// usable", "Not installed", "Hidden".
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

/// A row's state mark before its name: connected; something to do (sign in, update); missing while no tool is
/// connected yet; quiet for the rest (an optional tool, a hidden one).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename = "AgentToolMark")]
pub enum Mark {
    Connected,
    Attention,
    Missing,
    Quiet,
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
    /// The row's state mark.
    pub mark: Mark,
    /// Settings only; the onboarding rows say the state in [`Self::detail`].
    pub pill: Option<Pill>,
    /// "v0.157.1 · signed in with ChatGPT", "v1.0.2 · not signed in", "not installed · optional now".
    pub detail: String,
    /// **Sign in…** (`agent_tools.sign_in`).
    pub sign_in: Option<String>,
    /// **Re-check** for a tool whose sign-in couldn't be checked (`agent_tools.recheck {tool}`).
    pub recheck: Option<String>,
    /// **Show** for a hidden tool (`agent_tools.set_hidden {hidden: false}`).
    pub show: Option<String>,
    pub install: Option<CommandView>,
    /// **Install** (`agent_tools.install`): runs the tool's install command in a terminal under the row, on the user's
    /// click. Every missing tool has it.
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

/// Settings → Tools.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "AgentToolsPage")]
pub struct Page {
    pub title: String,
    pub intro: String,
    /// "TOOLS", over the rows.
    pub rows_heading: String,
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
    /// Noyan starts on; `None` until [`with_flows`] adds it (the gallery, a failed list).
    pub noyan: Option<Noyan>,
    /// Where they're used; `None` until [`with_flows`] adds it.
    pub used: Option<Used>,
}

/// "Noyan starts on" (UX-70, A-8): the model and effort every new Noyan conversation starts on. `ToolsNoyan` on the
/// page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "ToolsNoyan")]
pub struct Noyan {
    /// "NOYAN".
    pub heading: String,
    /// "Noyan starts on".
    pub label: String,
    /// The shared Model and Effort field; its hint is [`NOYAN_HINT`].
    pub picker: model_effort::View,
}

/// One tool of Where they're used. `ToolsUsedRow` on the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "ToolsUsedRow")]
pub struct UsedRow {
    /// The tool as requests name it (`codex`).
    pub tool: String,
    pub avatar: ToolAvatarView,
    /// "Claude Code".
    pub name: String,
    /// "shop-web: Dev · billing-api: Dev, Security review · Noyan".
    pub places: String,
    /// Amber, for a tool that isn't set up on this Mac: what waits for it.
    pub off: Option<String>,
}

/// Where they're used (A-8), read-only: per tool, the projects' stages (and Done) and Noyan on it. `ToolsUsed` on
/// the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "ToolsUsed")]
pub struct Used {
    /// "WHERE THEY’RE USED".
    pub heading: String,
    pub rows: Vec<UsedRow>,
    /// [`USED_NOTE`].
    pub note: String,
}

/// What Noyan starts on's field says under it (UX-70).
pub const NOYAN_HINT: &str =
    "For new conversations. Switch for one conversation in the Noyan panel.";
/// Under Where they're used (A-8).
pub const USED_NOTE: &str = "To change one, open its project\u{2019}s flow. Signing out of a tool makes its stages \
     wait until you pick another model.";
/// Where they're used names Noyan so.
pub const NOYAN: &str = super::agent_panel::LABEL;

/// What the page adds from the flows and the Noyan setting: Noyan starts on as stored, every project's [`Use`]s
/// ([`model_effort::uses_in`] over each project's flow in use) and the tools with their model lists.
pub struct Flows<'a> {
    pub noyan: &'a NoyanStartsOn,
    pub uses: &'a [Use],
    pub tools: &'a Tools,
}

/// The page with Noyan starts on and Where they're used.
pub fn with_flows(page: Page, f: &Flows) -> Page {
    Page {
        noyan: Some(noyan(f)),
        used: Some(used(f)),
        ..page
    }
}

/// Noyan starts on: the shared field, with the flows' model-and-effort pairs on top of its menu. No model stored yet
/// reads as `default` (the account's default model's tool), as the conversation starts on it.
pub fn noyan(f: &Flows) -> Noyan {
    let model = f.noyan.model.as_deref().unwrap_or(model_effort::DEFAULT);
    Noyan {
        heading: NOYAN.to_uppercase(),
        label: format!("{NOYAN} starts on"),
        picker: model_effort::view(
            Some(model),
            f.noyan.effort.as_deref(),
            f.uses,
            f.tools,
            model_effort::Place {
                hint: NOYAN_HINT,
                waits_at: None,
            },
        ),
    }
}

/// Where they're used: one row per tool something runs on, in [`Tool::ALL`]'s order; a row lists each project's
/// stages ("shop-web: Dev, Security review"), the projects in the order their uses come, then Noyan. `default` counts
/// as the default model's tool; a model that doesn't read is left out.
pub fn used(f: &Flows) -> Used {
    let noyan = Use {
        model: f
            .noyan
            .model
            .clone()
            .unwrap_or_else(|| model_effort::DEFAULT.to_owned()),
        effort: f.noyan.effort.clone(),
        what: NOYAN.to_owned(),
        project: None,
    };
    let all: Vec<&Use> = f.uses.iter().chain(std::iter::once(&noyan)).collect();
    let rows = Tool::ALL
        .iter()
        .copied()
        .filter_map(|tool| {
            let on: Vec<&Use> = all
                .iter()
                .copied()
                .filter(|u| model_effort::runs_on(&u.model, f.tools).is_some_and(|r| r.tool == tool))
                .collect();
            if on.is_empty() {
                return None;
            }
            let word = tool_word(tool);
            Some(UsedRow {
                tool: tool.as_str().to_owned(),
                avatar: ToolAvatarView::of(tool, None),
                name: tool.display_name().to_owned(),
                places: used_places(&on),
                off: (!f.tools.connected.contains(&tool)).then(|| {
                    format!(
                        "{word} isn\u{2019}t set up on {}: these wait until you set it up or pick another model.",
                        this_machine()
                    )
                }),
            })
        })
        .collect();
    Used {
        heading: "WHERE THEY\u{2019}RE USED".to_owned(),
        rows,
        note: USED_NOTE.to_owned(),
    }
}

/// "shop-web: Dev · billing-api: Dev, Security review · Noyan": each project once with its stages, then the uses
/// with no project.
fn used_places(uses: &[&Use]) -> String {
    let mut by: Vec<(Option<&str>, Vec<&str>)> = Vec::new();
    for u in uses {
        let project = u.project.as_deref();
        match by.iter_mut().find(|(p, _)| *p == project) {
            Some((_, what)) => {
                if !what.contains(&u.what.as_str()) {
                    what.push(&u.what);
                }
            }
            None => by.push((project, vec![&u.what])),
        }
    }
    // Noyan (no project) last.
    by.sort_by_key(|(p, _)| p.is_none());
    by.into_iter()
        .map(|(project, what)| match project {
            Some(p) => format!("{p}: {}", what.join(", ")),
            None => what.join(", "),
        })
        .collect::<Vec<_>>()
        .join(" \u{b7} ")
}

/// One change to Noyan starts on, as the field sends it: a model (its effort stays, or moves to the nearest level the
/// new tool offers, UX-75), a pair of "As in your other flows" (both), or an effort (`None`: the tool's own).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS)]
#[serde(tag = "edit", rename_all = "snake_case")]
#[ts(rename = "ToolsNoyanEdit")]
pub enum NoyanEdit {
    Model {
        model: String,
    },
    Pair {
        model: String,
        #[serde(default)]
        #[ts(optional)]
        effort: Option<String>,
    },
    Effort {
        #[serde(default)]
        #[ts(optional)]
        effort: Option<String>,
    },
}

/// Noyan starts on after `edit`, to store (`noyan.set_starts_on`; the service checks it again). A model that doesn't
/// read, or a pair's effort its tool doesn't have, is refused with why.
pub fn noyan_edit(on: &NoyanStartsOn, edit: &NoyanEdit) -> Result<NoyanStartsOn, String> {
    match edit {
        NoyanEdit::Model { model } => {
            let parsed = AgentModel::parse(model)?;
            // `default`'s tool is known only from the account, so its effort stays (as a stage's does).
            let effort = match (parsed.tool(), on.effort.as_deref()) {
                (Some(tool), Some(effort)) => {
                    otaq_core::agents::nearest_effort(tool, effort).map(str::to_owned)
                }
                (_, effort) => effort.map(str::to_owned),
            };
            Ok(NoyanStartsOn {
                model: Some(parsed.text()),
                effort,
            })
        }
        NoyanEdit::Pair { model, effort } => {
            let parsed = AgentModel::parse(model)?;
            if let Some(why) = effort
                .as_deref()
                .and_then(|e| otaq_core::agents::effort_refusal(&parsed, e))
            {
                return Err(why);
            }
            Ok(NoyanStartsOn {
                model: Some(parsed.text()),
                effort: effort.clone(),
            })
        }
        NoyanEdit::Effort { effort } => Ok(NoyanStartsOn {
            model: on.model.clone(),
            effort: effort.clone(),
        }),
    }
}

/// The rows of `agent_tools.list` as the page draws them; `now` (Unix ms) dates "Checked …".
pub fn page(statuses: &[ToolStatus], now: i64) -> Page {
    let checked = statuses.iter().filter_map(|s| s.checked_at).max();
    Page {
        title: "Tools".to_owned(),
        intro: format!(
            "The AI tools on {}. Which model and effort each stage uses is set in the flow, on the stage. {PRODUCT_NAME} never asks for passwords or keys and never changes their permission settings. Install and Sign in\u{2026} run the tool\u{2019}s own command in a terminal here, on your click.",
            this_machine()
        ),
        rows: statuses
            .iter()
            .map(|s| row(s, Place::Settings, false))
            .collect(),
        recheck: "Re-check".to_owned(),
        checked: checked.map(|at| format!("Checked {}", ago(now - at))),
        menu_label: "More actions".to_owned(),
        not_shown_title: "Not shown here".to_owned(),
        not_shown: "git, gh and az are under Git & pull requests: they aren\u{2019}t agents."
            .to_owned(),
        error: None,
        terminal: super::onboarding_page::tool_terminal_texts(),
        rows_heading: "TOOLS".to_owned(),
        noyan: None,
        used: None,
    }
}

/// The page when `agent_tools.list` failed: no rows, and why.
pub fn failed(error: String) -> Page {
    Page {
        error: Some(error),
        ..page(&[], 0)
    }
}

/// One tool's row. A missing tool shows its install command before its Install, also on onboarding, where
/// with one tool connected the others are "optional now" (`none_connected`: none is).
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
        AgentToolState::CouldNotCheck { .. } => (
            PillLook::CouldNotCheck,
            "Found \u{b7} couldn\u{2019}t check",
            if settings {
                with_version("couldn\u{2019}t check sign-in")
            } else {
                "found \u{b7} couldn\u{2019}t check sign-in".to_owned()
            },
        ),
        AgentToolState::UpdateNeeded { .. } => (
            PillLook::UpdateNeeded,
            "Found \u{b7} update needed",
            with_version("too old for this app"),
        ),
        AgentToolState::NotUsable { why, .. } => (
            PillLook::NotUsable,
            "Found \u{b7} not usable",
            with_version(why),
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
    let install = (s.state == AgentToolState::NotInstalled)
        .then(|| s.install_command.clone())
        .flatten()
        .map(|command| CommandView {
            label: format!("How to install ({})", os_name()),
            command,
            copy: "Copy".to_owned(),
        });
    // Install is offered for every missing tool, also an optional one on onboarding.
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
    let could_not_check = matches!(s.state, AgentToolState::CouldNotCheck { .. });
    let not_usable = matches!(s.state, AgentToolState::NotUsable { .. });
    let mark = if connected {
        Mark::Connected
    } else if s.can_sign_in || update.is_some() || could_not_check || not_usable {
        Mark::Attention
    } else if s.state == AgentToolState::NotInstalled && (settings || none_connected) {
        Mark::Missing
    } else {
        Mark::Quiet
    };
    Row {
        tool: tool.as_str().to_owned(),
        avatar: ToolAvatarView::of(tool, None),
        name: name.to_owned(),
        mark,
        pill: settings.then(|| Pill {
            label: pill.to_owned(),
            look,
        }),
        detail,
        sign_in: s.can_sign_in.then(|| "Sign in\u{2026}".to_owned()),
        recheck: could_not_check.then(|| "Re-check".to_owned()),
        show: matches!(s.state, AgentToolState::Hidden { .. }).then(|| "Show".to_owned()),
        install,
        install_button,
        install_note,
        update,
        // Onboarding's rows say only what the user needs to start: no "Otaq can see".
        can_see: (settings && connected).then(|| can_see(s)),
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

fn can_see(s: &ToolStatus) -> CanSee {
    let limits_seen = s.limits != Some(WindowsSource::NotReported);
    let mut chips: Vec<SeeChip> = ["permission prompts", "questions", "idle", "tokens"]
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
            true,
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
                AgentToolState::CouldNotCheck {
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

/// The stages of two projects' flows, as A-8 draws them: shop-web's Dev and Validate, billing-api's Dev, Security
/// review and Validate.
fn sample_uses() -> Vec<Use> {
    let at = |what: &str, project: &str, model: &str, effort: &str| Use {
        model: model.to_owned(),
        effort: Some(effort.to_owned()),
        what: what.to_owned(),
        project: Some(project.to_owned()),
    };
    vec![
        at("Dev", "shop-web", "claude:opus", "high"),
        at("Validate", "shop-web", "codex:gpt-5.5", "medium"),
        at("Dev", "billing-api", "claude:opus", "high"),
        at("Security review", "billing-api", "claude", "medium"),
        at("Validate", "billing-api", "codex:gpt-5.5", "medium"),
    ]
}

pub fn gallery() -> Gallery {
    use fixtures::*;
    let set_up = Tools {
        connected: vec![Tool::Claude, Tool::Codex],
        lists: super::agents::fixtures::all_lists(),
        ..Tools::default()
    };
    let uses = sample_uses();
    let mut pages = vec![GalleryCell {
        name: "15 · Tools · sample".to_owned(),
        page: with_flows(
            page(&sample(), NOW),
            &Flows {
                noyan: &NoyanStartsOn {
                    model: Some("claude:opus".into()),
                    effort: Some("high".into()),
                },
                uses: &uses,
                tools: &set_up,
            },
        ),
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
                    ("Found · couldn’t check", PillLook::CouldNotCheck),
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
                    "v1.0.98 · couldn’t check sign-in",
                    "v0.0.404 · too old for this app",
                    "not installed",
                    "Found, hidden by you",
                ]
            );
            let [connected, signed_out, unchecked, update, missing, hidden] = &rows[..] else {
                unreachable!()
            };
            assert_eq!(connected.name, tool.display_name());
            assert_eq!(connected.tool, tool.as_str());
            assert_eq!(connected.avatar.letters.len(), 2, "always shown here");
            assert_eq!(menu_actions(connected), [Hide, Recheck, Disconnect]);
            assert!(connected.sign_in.is_none() && connected.install.is_none());
            assert!(connected.recheck.is_none());
            // Signed out: Sign in… on every tool, Claude Code's included (`claude auth login`).
            assert_eq!(signed_out.sign_in.as_deref(), Some("Sign in…"));
            assert_eq!(signed_out.mark, Mark::Attention);
            assert_eq!(menu_actions(signed_out), [Hide, Recheck]);
            assert!(signed_out.can_see.is_none());
            // Its sign-in answer couldn't be read: Re-check on the row, never signed in or counted.
            assert_eq!(unchecked.recheck.as_deref(), Some("Re-check"));
            assert!(unchecked.sign_in.is_none() && unchecked.can_see.is_none());
            assert_eq!(unchecked.mark, Mark::Attention);
            assert_eq!(menu_actions(unchecked), [Hide, Recheck]);
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
        // Onboarding's rows don't list them.
        assert!(
            row(&sample()[0], Place::Onboarding, false)
                .can_see
                .is_none()
        );
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

    /// A second Otaq without `CODEX_HOME` (piece 3 of fixes-before-0-3): Codex is found but not usable, amber, and
    /// the line says why and what to set; nothing Otaq added, no Sign in…, no Disconnect….
    #[test]
    fn not_usable_says_why() {
        let codex = status(
            Tool::Codex,
            AgentToolState::NotUsable {
                version: None,
                why: otaq_core::agent_tools::codex::held_back_reason(),
            },
        );
        let r = settings_row(&codex);
        let pill = r.pill.as_ref().unwrap();
        assert_eq!(
            (pill.label.as_str(), pill.look),
            ("Found \u{b7} not usable", PillLook::NotUsable)
        );
        assert_eq!(
            r.detail,
            format!(
                "this {PRODUCT_NAME} has its own data folder: set CODEX_HOME to use Codex with this {PRODUCT_NAME}"
            )
        );
        assert_eq!(r.mark, Mark::Attention);
        assert!(r.sign_in.is_none() && r.can_see.is_none() && r.added.is_none());
        assert!(r.update.is_none() && r.install.is_none());
        assert!(!r.menu.iter().any(|m| m.action == MenuAction::Disconnect));
    }

    #[test]
    fn the_page_lists_all_four_and_says_when_it_checked() {
        let p = page(&sample(), NOW);
        assert_eq!(p.title, "Tools");
        assert_eq!(p.rows.len(), 4);
        assert_eq!(p.checked.as_deref(), Some("Checked 2 minutes ago"));
        assert_eq!(p.rows[0].detail, "v2.1.301 · signed in");
        assert_eq!(p.rows[1].detail, "v0.157.1 · signed in with ChatGPT");
        assert_eq!(
            p.not_shown,
            "git, gh and az are under Git & pull requests: they aren’t agents."
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
        assert_eq!(
            optional.install.unwrap().command,
            "brew install codex",
            "its install command shows before Install"
        );
        assert_eq!(optional.mark, Mark::Quiet, "optional with a tool connected");
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
        assert_eq!(needed.mark, Mark::Missing);
        assert_eq!(rows[2].mark, Mark::Attention);
        assert_eq!(rows[0].mark, Mark::Connected);
        assert_eq!(needed.install.unwrap().command, "brew install codex");
        let hidden = row(
            &status(Tool::OpenCode, AgentToolState::Hidden { version: None }),
            Place::Onboarding,
            false,
        );
        assert_eq!(hidden.detail, "hidden by you");
        assert_eq!(hidden.show.as_deref(), Some("Show"));
        let unchecked = row(
            &status(
                Tool::Claude,
                AgentToolState::CouldNotCheck {
                    version: Some("Claude Code 2.1.319".into()),
                },
            ),
            Place::Onboarding,
            false,
        );
        assert_eq!(unchecked.detail, "found · couldn’t check sign-in");
        assert_eq!(unchecked.recheck.as_deref(), Some("Re-check"));
        assert_eq!(unchecked.mark, Mark::Attention);
        let signed_out = row(
            &status(
                Tool::Claude,
                AgentToolState::FoundNotSignedIn {
                    version: Some("Claude Code 2.1.319".into()),
                },
            ),
            Place::Onboarding,
            false,
        );
        assert_eq!(signed_out.detail, "found · not signed in");
        assert_eq!(signed_out.sign_in.as_deref(), Some("Sign in…"));
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

    fn two_tools() -> Tools {
        Tools {
            connected: vec![Tool::Claude, Tool::Codex],
            lists: crate::view_models::agents::fixtures::all_lists(),
            ..Tools::default()
        }
    }

    /// A-8: under the tools, Noyan starts on (the shared field, UX-70) and Where they're used, read-only, per tool.
    #[test]
    fn the_tools_page_has_noyan_starts_on_and_where_each_tool_is_used() {
        let tools = two_tools();
        let uses = sample_uses();
        let on = NoyanStartsOn {
            model: Some("claude:opus".into()),
            effort: Some("high".into()),
        };
        let p = with_flows(
            page(&sample(), NOW),
            &Flows {
                noyan: &on,
                uses: &uses,
                tools: &tools,
            },
        );
        assert_eq!(p.rows_heading, "TOOLS");
        assert!(p.intro.starts_with(&format!(
            "The AI tools on {}. Which model and effort each stage uses is set in the flow, on the stage.",
            this_machine()
        )));
        let noyan = p.noyan.expect("Noyan starts on");
        assert_eq!(noyan.heading, "NOYAN");
        assert_eq!(noyan.label, "Noyan starts on");
        assert_eq!(noyan.picker.value.text, "Claude Code · opus");
        assert_eq!(
            noyan.picker.hint,
            "For new conversations. Switch for one conversation in the Noyan panel."
        );
        let effort = noyan.picker.effort.expect("Claude Code has effort levels");
        assert!(effort.levels.iter().any(|l| l.id == "high" && l.selected));
        // The menu's pairs are the flows' (Noyan isn't one of its own pairs).
        assert!(!noyan.picker.menu.pairs.is_empty());
        assert!(
            noyan
                .picker
                .menu
                .pairs
                .iter()
                .all(|p| !p.places.contains("Noyan"))
        );
        let used = p.used.expect("where they're used");
        assert_eq!(used.heading, "WHERE THEY’RE USED");
        let rows: Vec<(&str, &str)> = used
            .rows
            .iter()
            .map(|r| (r.name.as_str(), r.places.as_str()))
            .collect();
        assert_eq!(
            rows,
            [
                (
                    "Claude Code",
                    "shop-web: Dev · billing-api: Dev, Security review · Noyan"
                ),
                ("Codex", "shop-web: Validate · billing-api: Validate"),
            ]
        );
        assert!(used.rows.iter().all(|r| r.off.is_none()));
        assert_eq!(
            used.note,
            "To change one, open its project’s flow. Signing out of a tool makes its stages wait until you pick another \
             model."
        );
        // Without the flows (the gallery's other cells, a failed list) there is neither.
        assert!(page(&sample(), NOW).noyan.is_none());
        assert!(failed("down".into()).used.is_none());
    }

    /// A stage on a tool that isn't set up is still listed, marked: its tickets wait. `default` is the default model's
    /// tool; Noyan with nothing stored starts on `default`.
    #[test]
    fn where_theyre_used_marks_a_tool_that_isnt_set_up_and_reads_default() {
        let tools = Tools {
            connected: vec![Tool::Claude],
            lists: crate::view_models::agents::fixtures::all_lists(),
            default_model: Some(otaq_core::agents::ModelRef {
                tool: Tool::Claude,
                model: None,
            }),
            ..Tools::default()
        };
        let uses = vec![
            Use {
                model: "default".into(),
                effort: None,
                what: "Dev".into(),
                project: Some("shop-web".into()),
            },
            Use {
                model: "codex:gpt-5.5".into(),
                effort: None,
                what: "Validate".into(),
                project: Some("shop-web".into()),
            },
            Use {
                model: "codex".into(),
                effort: None,
                what: "Done".into(),
                project: Some("shop-web".into()),
            },
        ];
        let on = NoyanStartsOn::default();
        let f = Flows {
            noyan: &on,
            uses: &uses,
            tools: &tools,
        };
        let used = used(&f);
        assert_eq!(used.rows[0].places, "shop-web: Dev · Noyan");
        assert_eq!(used.rows[1].places, "shop-web: Validate, Done");
        assert_eq!(
            used.rows[1].off.as_deref(),
            Some(
                format!(
                    "Codex isn\u{2019}t set up on {}: these wait until you set it up or pick another model.",
                    this_machine()
                )
                .as_str()
            )
        );
        assert_eq!(noyan(&f).picker.value.text, "Claude Code · default");
    }

    /// Noyan starts on's edits: a model keeps the effort or moves it to the nearest level its tool has (UX-75), a pair
    /// sets both, an effort only the effort; a model that doesn't read is refused.
    #[test]
    fn noyan_starts_on_edits() {
        let on = NoyanStartsOn {
            model: Some("codex:gpt-5.5".into()),
            effort: Some("xhigh".into()),
        };
        let model = |m: &str| NoyanEdit::Model { model: m.into() };
        assert_eq!(
            noyan_edit(&on, &model("claude:opus")).unwrap(),
            NoyanStartsOn {
                model: Some("claude:opus".into()),
                effort: Some(
                    otaq_core::agents::nearest_effort(Tool::Claude, "xhigh")
                        .unwrap()
                        .to_owned()
                ),
            }
        );
        assert_eq!(
            noyan_edit(&on, &model("opencode")).unwrap().effort,
            None,
            "OpenCode has no effort"
        );
        assert_eq!(
            noyan_edit(&on, &model("default"))
                .unwrap()
                .effort
                .as_deref(),
            Some("xhigh"),
            "default's tool isn't known here: the effort stays"
        );
        assert_eq!(
            noyan_edit(
                &on,
                &NoyanEdit::Pair {
                    model: "claude:sonnet".into(),
                    effort: Some("medium".into()),
                }
            )
            .unwrap(),
            NoyanStartsOn {
                model: Some("claude:sonnet".into()),
                effort: Some("medium".into()),
            }
        );
        assert_eq!(
            noyan_edit(&on, &NoyanEdit::Effort { effort: None }).unwrap(),
            NoyanStartsOn {
                model: Some("codex:gpt-5.5".into()),
                effort: None,
            }
        );
        assert!(noyan_edit(&on, &model("nope:x")).is_err());
        assert!(
            noyan_edit(
                &on,
                &NoyanEdit::Pair {
                    model: "opencode".into(),
                    effort: Some("high".into()),
                }
            )
            .is_err()
        );
        // As the page sends them.
        let edit: NoyanEdit =
            serde_json::from_value(serde_json::json!({ "edit": "effort", "effort": "low" }))
                .unwrap();
        assert_eq!(
            edit,
            NoyanEdit::Effort {
                effort: Some("low".into())
            }
        );
    }

    #[test]
    fn the_gallery_has_every_state_per_tool_and_both_dialogs() {
        let g = gallery();
        assert_eq!(g.pages.len(), 6);
        assert!(g.pages[1..5].iter().all(|c| c.page.rows.len() == 6));
        assert_eq!(g.dialogs.len(), 2);
        assert!(g.dialogs[1].error.is_some());
    }
}
