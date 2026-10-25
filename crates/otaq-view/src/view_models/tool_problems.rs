//! The agent tools as the sheet, the agent panel, the epic lead and recovery see them, and what they say about a tool that
//! can't start or continue an agent (AGT-04, AGT-05, AGT-08, SHL-04, USG-10, §7.2; d0086, d0087, d0088, d0077;
//! *05 → Column 3 · several tools (M23)*, *18 → Row · Pane: empty, hidden, tool problems, menu (0087)*, *08 → Row ·
//! Epic lead in the epic form and panel (0086 · V)*, *16 → Row · Recovery per tool (0086)*).
//!
//! - **The tools** ([`Tools`]): which are connected, each one's state and active limit, and each one's cached model
//!   list. With one tool connected every screen reads as M22: no tool names, no avatars ([`Tools::several`]).
//! - **A hold** ([`hold_callout`]): a role that would start but waits for its tool or its model (SCH-04 `[M23]`):
//!   "Codex isn't available · logged out" with Use fallback (Claude Code), Change agent and Sign in…; "Claude Code
//!   limit reached · resets 3:45 pm"; "Model unavailable" with Change agent (Change validator for the Validator).
//!   The fallback is the tool's (AGM-03, d0108): it takes a held start only when you ask.
//! - **A running or ended agent** whose tool isn't available or is at its limit ([`agent_notice`], [`lead_notice`]):
//!   it waits, since a conversation never moves tools; the agent panel offers Sign in… and "New agent on Sonnet".
//!
//! Unavailable, a model no longer listed and moves are amber; limits are neutral; nothing here is turquoise (d0077).

use otaq_core::agent_tools::Tool;
use otaq_core::agent_tools::connected::{AgentToolState, NotAvailable, ToolStatus};
use otaq_core::agent_tools::models::ModelList;
use otaq_core::agents::{Agent, AgentHold, Fallbacks, HoldReason, ModelRef, SessionRole};
use otaq_core::usage::plan::ActiveLimit;
use serde::Serialize;
use ts_rs::TS;

use crate::view_models::agents::{RoleId, model_label};
use crate::view_models::clock::reset_label;
use crate::view_models::tool_avatar::{AvatarTool, ToolAvatarView, shown_with};

/// What the views know about the agent tools: the connected ones (Connected only counts, d0086), every tool's state
/// (`agent_tools.list`), the active limits per tool, each tool's cached model list, each tool's fallback, the
/// default model, and the time now.
#[derive(Debug, Clone, Default)]
pub struct Tools {
    pub connected: Vec<Tool>,
    pub statuses: Vec<ToolStatus>,
    pub limits: Vec<ActiveLimit>,
    pub lists: Vec<(Tool, ModelList)>,
    pub fallbacks: Fallbacks,
    pub default_model: Option<ModelRef>,
    pub now: i64,
}

impl Tools {
    /// One tool connected (the first), with its list: what every screen read before M23, and the tests' default.
    pub fn one(list: Option<ModelList>) -> Self {
        let tool = Tool::ALL[0];
        Self {
            connected: vec![tool],
            lists: list.map(|l| (tool, l)).into_iter().collect(),
            ..Self::default()
        }
    }

    /// The tools of `statuses` that count (Connected), in [`Tool::ALL`] order.
    pub fn from_statuses(statuses: Vec<ToolStatus>) -> Self {
        Self {
            connected: statuses
                .iter()
                .filter(|s| s.state.is_connected())
                .map(|s| s.tool)
                .collect(),
            statuses,
            ..Self::default()
        }
    }

    /// Two or more tools connected: rows, lines and History name the tool, avatars show (AGT-04, SHL-08).
    pub fn several(&self) -> bool {
        shown_with(
            &self
                .connected
                .iter()
                .copied()
                .map(AvatarTool::from)
                .collect::<Vec<_>>(),
        )
    }

    /// At least one tool connected ("Ask the agent" is hidden only without one, RUN-02).
    pub fn any(&self) -> bool {
        !self.connected.is_empty()
    }

    /// `tool`'s cached model list.
    pub fn list(&self, tool: Tool) -> Option<&ModelList> {
        self.lists.iter().find(|(t, _)| *t == tool).map(|(_, l)| l)
    }

    pub fn status(&self, tool: Tool) -> Option<&ToolStatus> {
        self.statuses.iter().find(|s| s.tool == tool)
    }

    /// The tool `agent` runs on now: its own, or the default model's.
    pub fn tool_of(&self, agent: &Agent) -> Option<Tool> {
        agent.model.on(self.default_model.as_ref()).map(|m| m.tool)
    }

    /// The avatar of `tool`, only with several tools connected (SHL-08).
    pub fn avatar(&self, tool: Tool, model: Option<&str>) -> Option<ToolAvatarView> {
        self.several().then(|| ToolAvatarView::of(tool, model))
    }

    /// Why `tool` can't run an agent now: not available (its state), else at its limit. `None` when it can, or
    /// when its state wasn't read.
    pub fn problem(&self, tool: Tool) -> Option<Problem> {
        if let Some(status) = self.status(tool)
            && let Some(reason) = status.state.unavailable_reason()
        {
            return Some(Problem::Unavailable {
                reason: reason.to_owned(),
                sign_in: status.can_sign_in,
                hidden: matches!(status.state, AgentToolState::Hidden { .. }),
            });
        }
        self.limits
            .iter()
            .find(|l| l.tool == tool)
            .map(|l| Problem::Limit {
                resets_at: l.resets_at,
            })
    }
}

/// Why a tool can't run an agent now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Problem {
    /// Not installed, logged out, hidden or needs an update ("logged out"); Sign in… applies; Hidden (Q-E).
    Unavailable {
        reason: String,
        sign_in: bool,
        hidden: bool,
    },
    /// At its limit; `None`: resets at an unknown time.
    Limit { resets_at: Option<i64> },
}

/// The tool's name in a sentence or a line: its display name, shortened to the short name where that is its end
/// ("GitHub Copilot" → "Copilot"; "Claude Code" stays, since "Claude" isn't its end).
pub fn tool_word(tool: Tool) -> &'static str {
    let short = tool.short_name();
    if tool.display_name().ends_with(short) {
        short
    } else {
        tool.display_name()
    }
}

/// "Codex isn't available · logged out".
pub fn unavailable_text(tool: Tool, reason: &str) -> String {
    NotAvailable {
        reason: reason.to_owned(),
    }
    .message(tool)
    .replace(tool.display_name(), tool_word(tool))
}

/// "Codex limit reached · resets 5:10 pm"; "Codex limit reached" when the reset isn't known.
pub fn limit_text(tool: Tool, resets_at: Option<i64>, now: i64) -> String {
    match resets_at {
        Some(at) => format!(
            "{} limit reached \u{b7} resets {}",
            tool_word(tool),
            reset_label(at, now)
        ),
        None => format!("{} limit reached", tool_word(tool)),
    }
}

/// What a button of a callout or a notice does. `ToolProblemAction` on the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(rename = "ToolProblemAction")]
pub enum Action {
    /// `agent_tools.sign_in { tool }`: the tool's own login in a terminal (`tool` as stored, `codex`).
    SignIn { tool: String },
    /// Settings → Agent tools, for a tool hidden there (Q-E).
    ShowInAgentTools,
    /// `limits.resume_tool { tool }`.
    ResumeTool { tool: String },
    /// `ticket.use_fallback { key }`: the held start launches on its tool's fallback (`role` says which card action).
    UseFallback { role: RoleId },
    /// `limits.use_fallback_now { tool }`: every ticket waiting on `tool` starts on its fallback (AGM-03).
    UseFallbackNow { tool: String },
    /// Open agent…: Settings → Agents on `agent`'s page (AGM-03, d0108 second gap review rule 7; replaces Change
    /// agent, Q280); the list when the agent isn't known.
    OpenAgent { agent: Option<String> },
    /// The agent panel's empty state on `agent` (a named agent's id); it starts when you send (CHANGES-v §4).
    NewAgent { agent: String },
    /// Settings → Agents at the project's Project agent default, `agent` (Q-O).
    AgentSettings {
        project: String,
        agent: Option<String>,
    },
}

/// A button. `ToolProblemButton` on the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "ToolProblemButton")]
pub struct Button {
    pub label: String,
    pub action: Action,
    pub primary: bool,
}

/// The callout's colour: amber for a tool that isn't available, a model no longer listed and a move; neutral for a
/// limit (d0077: never turquoise). `ToolProblemTone` on the page.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename = "ToolProblemTone")]
pub enum Tone {
    Amber,
    Neutral,
}

/// A callout or a notice: its heading, the line under it, its buttons. `ToolProblemCallout` on the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "ToolProblemCallout")]
pub struct Callout {
    /// The held role, for a hold; `None` for a running agent's notice.
    pub role: Option<RoleId>,
    pub tone: Tone,
    /// The tool's avatar (several tools only).
    pub avatar: Option<ToolAvatarView>,
    /// "Model unavailable", "Codex isn't available · logged out", "Claude Code limit reached · resets 3:45 pm".
    pub heading: String,
    pub text: Option<String>,
    /// The model on hover ("gpt-5.4"), for Model unavailable.
    pub model: Option<String>,
    pub buttons: Vec<Button>,
}

pub const OPEN_AGENT: &str = "Open agent\u{2026}";
pub const SIGN_IN: &str = "Sign in\u{2026}";
pub const SHOW_IN_AGENT_TOOLS: &str = "Show in Agent tools";
pub const MODEL_UNAVAILABLE: &str = "Model unavailable";
pub const RESUME_AFTER_RESET: &str = "Resume after reset";

/// Open agent… on `agent` (its id).
fn open_agent(agent: Option<&str>) -> Button {
    Button {
        label: OPEN_AGENT.to_owned(),
        action: Action::OpenAgent {
            agent: agent.map(str::to_owned),
        },
        primary: false,
    }
}

fn sign_in(tool: Tool) -> Button {
    Button {
        label: SIGN_IN.to_owned(),
        action: Action::SignIn {
            tool: tool.as_str().to_owned(),
        },
        primary: false,
    }
}

/// The buttons of a tool that isn't available: Sign in… where it applies, Show in Agent tools for a hidden one.
fn unavailable_buttons(tool: Tool, problem: Option<&Problem>) -> Vec<Button> {
    match problem {
        Some(Problem::Unavailable { sign_in: true, .. }) => vec![self::sign_in(tool)],
        Some(Problem::Unavailable { hidden: true, .. }) => vec![Button {
            label: SHOW_IN_AGENT_TOOLS.to_owned(),
            action: Action::ShowInAgentTools,
            primary: false,
        }],
        _ => Vec::new(),
    }
}

fn find<'a>(agents: &'a [Agent], id: Option<&str>) -> Option<&'a Agent> {
    id.and_then(|id| agents.iter().find(|a| a.id == id))
}

/// The model as Settings shows it, for "Codex no longer lists gpt-5.4".
fn model_text(hold: &AgentHold, tools: &Tools) -> String {
    model_label(hold.model.as_deref(), tools.list(hold.tool))
}

/// "its fallback Claude Code can't start either": a held role whose tool's fallback is blocked too, on the card
/// ("Griller's model can't run: Codex isn't available · its fallback Claude Code can't start either") and in the sheet
/// (Q155: one wording).
pub fn fallback_blocked_too(fallback: &str) -> String {
    format!("its fallback {fallback} can\u{2019}t start either")
}

fn capitalized(text: &str) -> String {
    let mut chars = text.chars();
    chars
        .next()
        .map_or_else(String::new, |c| c.to_uppercase().chain(chars).collect())
}

/// The tool's fallback as a button or a line names it: "Claude Code", "Codex · gpt-5.5" with several tools.
fn fallback_word(fallback: &ModelRef, tools: &Tools) -> String {
    match &fallback.model {
        Some(_) => format!(
            "{} \u{b7} {}",
            tool_word(fallback.tool),
            model_label(fallback.model.as_deref(), tools.list(fallback.tool))
        ),
        None => tool_word(fallback.tool).to_owned(),
    }
}

/// The callout of a held role (§7.2, SCH-04 `[M23]`, AGT-08; *05 → Model unavailable (SHOP-49)*): `agents` is every
/// agent (the held one). Use fallback is offered while the tool's fallback can start (AGM-03).
pub fn hold_callout(hold: &AgentHold, agents: &[Agent], tools: &Tools) -> Callout {
    let tool = hold.tool;
    let name = find(agents, hold.agent_id.as_deref()).map_or("The agent", |a| a.name.as_str());
    let problem = tools.problem(tool);
    let fallback = tools.fallbacks.get(&tool).map(|f| fallback_word(f, tools));
    let mut buttons = Vec::new();
    if hold.fallback_tool.is_some() && !hold.fallback_held {
        buttons.push(Button {
            label: match &fallback {
                Some(word) => format!("Use fallback ({word})"),
                None => "Use fallback".to_owned(),
            },
            action: Action::UseFallback {
                role: hold.role.into(),
            },
            primary: true,
        });
    }
    let mut open = open_agent(hold.agent_id.as_deref());
    open.primary = buttons.is_empty();
    buttons.push(open);
    // "Claude Code and its fallback Codex are both at their limits".
    let both = hold
        .fallback_held
        .then_some(fallback.as_deref())
        .flatten()
        .map(|fallback| match hold.reason {
            HoldReason::ToolLimit => {
                format!(
                    "{} and its fallback {fallback} are both at their limits",
                    tool_word(tool)
                )
            }
            // Under the heading that names the cause ("Codex isn't available · logged out").
            _ => format!("{}.", capitalized(&fallback_blocked_too(fallback))),
        });
    let (tone, heading, text) = match hold.reason {
        HoldReason::ModelUnavailable => {
            let model = model_text(hold, tools);
            (
                Tone::Amber,
                MODEL_UNAVAILABLE.to_owned(),
                Some(format!(
                    "{} no longer lists {model}, the model of {name}. Give {name} another model in Settings \u{2192} \
                     Agents{}.",
                    tool_word(tool),
                    if hold.fallback_tool.is_some() && !hold.fallback_held {
                        ", or use the fallback"
                    } else {
                        ""
                    }
                )),
            )
        }
        HoldReason::ToolUnavailable => {
            buttons.extend(unavailable_buttons(tool, problem.as_ref()));
            let heading = match &problem {
                Some(Problem::Unavailable { reason, .. }) => unavailable_text(tool, reason),
                _ => format!("{} isn\u{2019}t available", tool_word(tool)),
            };
            (
                Tone::Amber,
                heading,
                Some(both.unwrap_or_else(|| {
                    format!(
                        "Nothing started and nothing is locked. {name} starts once {} is available again.",
                        tool_word(tool)
                    )
                })),
            )
        }
        HoldReason::ToolLimit => {
            let resets_at = match problem {
                Some(Problem::Limit { resets_at }) => resets_at,
                _ => None,
            };
            (
                Tone::Neutral,
                limit_text(tool, resets_at, tools.now),
                Some(both.unwrap_or_else(|| {
                    format!(
                        "Nothing started and nothing is locked. {name} starts when you resume {} after its reset.",
                        tool_word(tool)
                    )
                })),
            )
        }
    };
    Callout {
        role: Some(hold.role.into()),
        tone,
        avatar: tools.avatar(tool, hold.model.as_deref()),
        heading,
        text,
        model: (hold.reason == HoldReason::ModelUnavailable).then(|| model_text(hold, tools)),
        buttons,
    }
}

/// Every held role's callout, in the order of the rows (Developer, Validator, Epic lead).
pub fn hold_callouts(holds: &[AgentHold], agents: &[Agent], tools: &Tools) -> Vec<Callout> {
    let mut holds: Vec<&AgentHold> = holds.iter().collect();
    holds.sort_by_key(|h| SessionRole::ALL.iter().position(|r| *r == h.role));
    holds
        .into_iter()
        .map(|h| hold_callout(h, agents, tools))
        .collect()
}

/// Where "New agent on …" starts a Project agent whose tool can't run (CHANGES-v §4): the Project agent role's agent
/// (`default`), else the first agent on the tool's fallback, else the first on another tool, when it can run now.
pub fn new_agent_on<'a>(
    tool: Tool,
    default: Option<&'a Agent>,
    agents: &'a [Agent],
    tools: &Tools,
) -> Option<&'a Agent> {
    let runs = |a: &&Agent| {
        tools
            .tool_of(a)
            .is_some_and(|t| t != tool && tools.problem(t).is_none())
    };
    let fallback = tools.fallbacks.get(&tool).map(|f| f.tool);
    default
        .filter(runs)
        .or_else(|| {
            agents
                .iter()
                .filter(runs)
                .find(|a| fallback.is_some() && tools.tool_of(a) == fallback)
        })
        .or_else(|| agents.iter().find(runs))
}

/// The agent panel's notice for a running or ended Project agent whose tool isn't available or is at its limit (18
/// `VDJkG`): it waits, since its conversation can't move tools; `new_agent` is where "New agent on …" starts.
pub fn agent_notice(tool: Tool, tools: &Tools, new_agent: Option<&Agent>) -> Option<Callout> {
    let problem = tools.problem(tool)?;
    let (tone, heading, mut buttons) = match &problem {
        Problem::Unavailable {
            reason, sign_in, ..
        } => (
            Tone::Amber,
            format!(
                "{}. This agent continues {}.",
                unavailable_text(tool, reason),
                if *sign_in {
                    "after you sign in".to_owned()
                } else {
                    format!("once {} is available again", tool_word(tool))
                }
            ),
            unavailable_buttons(tool, Some(&problem)),
        ),
        Problem::Limit { resets_at } => (
            Tone::Neutral,
            format!(
                "{}. This agent continues after the reset.",
                limit_text(tool, *resets_at, tools.now)
            ),
            vec![Button {
                label: RESUME_AFTER_RESET.to_owned(),
                action: Action::ResumeTool {
                    tool: tool.as_str().to_owned(),
                },
                primary: false,
            }],
        ),
    };
    if let Some(first) = buttons.first_mut() {
        first.primary = true;
    }
    buttons.extend(new_agent.map(|a| Button {
        label: format!("New agent on {}", a.name),
        action: Action::NewAgent {
            agent: a.id.clone(),
        },
        primary: false,
    }));
    Some(Callout {
        role: None,
        tone,
        avatar: tools.avatar(tool, None),
        heading,
        text: None,
        model: None,
        buttons,
    })
}

/// The empty agent panel's notice for a held first message (Q-O): Model unavailable opens Settings → Agents at the
/// project's Project agent default; a tool that can't run waits like a running agent, with "New agent on …".
pub fn first_message_notice(
    hold: &AgentHold,
    project: &str,
    agents: &[Agent],
    tools: &Tools,
    new_agent: Option<&Agent>,
) -> Callout {
    let tool = hold.tool;
    let name = find(agents, hold.agent_id.as_deref()).map_or("The agent", |a| a.name.as_str());
    if hold.reason == HoldReason::ModelUnavailable {
        let model = model_text(hold, tools);
        return Callout {
            role: Some(RoleId::ProjectAgent),
            tone: Tone::Amber,
            avatar: tools.avatar(tool, hold.model.as_deref()),
            heading: MODEL_UNAVAILABLE.to_owned(),
            text: Some(format!(
                "{} no longer lists {model}, the model of {name}. Your message is kept: it starts once you edit \
                 {name} in Settings \u{2192} Agents.",
                tool_word(tool)
            )),
            model: Some(model),
            buttons: vec![Button {
                label: OPEN_AGENT.to_owned(),
                action: Action::AgentSettings {
                    project: project.to_owned(),
                    agent: hold.agent_id.clone(),
                },
                primary: true,
            }],
        };
    }
    let mut notice = agent_notice(tool, tools, new_agent).unwrap_or_else(|| Callout {
        role: None,
        tone: if hold.reason == HoldReason::ToolLimit {
            Tone::Neutral
        } else {
            Tone::Amber
        },
        avatar: tools.avatar(tool, None),
        heading: match hold.reason {
            HoldReason::ToolLimit => limit_text(tool, None, tools.now),
            _ => format!("{} isn\u{2019}t available", tool_word(tool)),
        },
        text: None,
        model: None,
        buttons: Vec::new(),
    });
    notice.heading = notice
        .heading
        .replace("This agent continues", "Your message is kept: it starts");
    notice.role = Some(RoleId::ProjectAgent);
    notice
}

/// The epic lead's waiting state (*08 → Epic lead · its tool isn't available / is at its limit*), on the Lead tab:
/// a lead held before its start (`hold`), or a running or ended lead on a tool that can't run now.
pub fn lead_notice(
    hold: Option<&AgentHold>,
    lead_tool: Option<Tool>,
    agents: &[Agent],
    tools: &Tools,
) -> Option<Callout> {
    if let Some(hold) = hold {
        let tool = hold.tool;
        let problem = tools.problem(tool);
        let (tone, heading, buttons) = match hold.reason {
            HoldReason::ToolUnavailable => (
                Tone::Amber,
                match &problem {
                    Some(Problem::Unavailable { reason, .. }) => unavailable_text(tool, reason),
                    _ => format!("{} isn\u{2019}t available", tool_word(tool)),
                },
                unavailable_buttons(tool, problem.as_ref()),
            ),
            HoldReason::ToolLimit => (
                Tone::Neutral,
                format!("Waiting for {}\u{2019}s limit reset", tool_word(tool)),
                Vec::new(),
            ),
            HoldReason::ModelUnavailable => (
                Tone::Amber,
                MODEL_UNAVAILABLE.to_owned(),
                vec![open_agent(hold.agent_id.as_deref())],
            ),
        };
        let callout = hold_callout(hold, agents, tools);
        return Some(Callout {
            heading,
            tone,
            buttons: buttons
                .into_iter()
                .enumerate()
                .map(|(i, b)| Button {
                    primary: i == 0,
                    ..b
                })
                .collect(),
            ..callout
        });
    }
    let tool = lead_tool?;
    let problem = tools.problem(tool)?;
    let (tone, heading, buttons) = match &problem {
        Problem::Unavailable { reason, .. } => (
            Tone::Amber,
            format!(
                "{}. The lead waits; children on {} wait too.",
                unavailable_text(tool, reason),
                tool_word(tool)
            ),
            unavailable_buttons(tool, Some(&problem)),
        ),
        Problem::Limit { resets_at } => (
            Tone::Neutral,
            format!(
                "{}. The lead continues after the reset.",
                limit_text(tool, *resets_at, tools.now)
            ),
            Vec::new(),
        ),
    };
    Some(Callout {
        role: None,
        tone,
        avatar: tools.avatar(tool, None),
        heading,
        text: None,
        model: None,
        buttons: buttons
            .into_iter()
            .map(|b| Button { primary: true, ..b })
            .collect(),
    })
}

/// "Codex isn't available (logged out)", for a recovery row whose tool can't resume now (*16 → Recovery per tool*).
pub fn recovery_problem(tool: Tool, tools: &Tools) -> Option<(String, Option<Button>)> {
    match tools.problem(tool)? {
        Problem::Unavailable {
            reason, sign_in, ..
        } => Some((
            format!("{} isn\u{2019}t available ({reason})", tool_word(tool)),
            sign_in.then(|| self::sign_in(tool)),
        )),
        Problem::Limit { resets_at } => Some((limit_text(tool, resets_at, tools.now), None)),
    }
}

/// Fixtures for the tests and the gallery: the tools of *update Y*'s example.
pub mod fixtures {
    use otaq_core::agent_tools::Tool;
    use otaq_core::agent_tools::connected::{AgentToolState, ToolStatus};
    use otaq_core::agents::{AgentHold, HoldReason, ModelRef, SessionRole};
    use otaq_core::usage::limit::LimitKind;
    use otaq_core::usage::plan::ActiveLimit;

    use super::Tools;

    pub const CLAUDE: Tool = Tool::ALL[0];
    pub const CODEX: Tool = Tool::ALL[1];
    pub const COPILOT: Tool = Tool::ALL[2];
    pub const OPENCODE: Tool = Tool::ALL[3];

    pub fn status(tool: Tool, state: AgentToolState) -> ToolStatus {
        ToolStatus {
            can_sign_in: matches!(state, AgentToolState::FoundNotSignedIn { .. }),
            can_install: false,
            install_needs: None,
            tool,
            state,
            checked_at: Some(0),
            notes: Vec::new(),
            install_command: None,
            setup_entries: Vec::new(),
            setup: None,
            permission_warning: None,
            can_disconnect: false,
            limits: None,
        }
    }

    pub fn connected(tool: Tool) -> ToolStatus {
        status(
            tool,
            AgentToolState::Connected {
                version: None,
                account: None,
            },
        )
    }

    /// Claude Code, Codex and Copilot connected; OpenCode not installed. Claude Code falls back to Codex and Codex
    /// to Claude Code.
    pub fn several() -> Tools {
        let mut tools = Tools::from_statuses(vec![
            connected(CLAUDE),
            connected(CODEX),
            connected(COPILOT),
            status(OPENCODE, AgentToolState::NotInstalled),
        ]);
        tools.fallbacks.insert(
            CLAUDE,
            ModelRef {
                tool: CODEX,
                model: None,
            },
        );
        tools.fallbacks.insert(
            CODEX,
            ModelRef {
                tool: CLAUDE,
                model: None,
            },
        );
        tools
    }

    /// [`several`] with Codex logged out.
    pub fn codex_logged_out() -> Tools {
        let mut tools = several();
        tools.statuses[1] = status(CODEX, AgentToolState::FoundNotSignedIn { version: None });
        tools.connected.retain(|t| *t != CODEX);
        // Claude Code and Copilot still count: several.
        tools
    }

    /// [`several`] with `tool` at its limit, resetting at `resets_at`.
    pub fn at_limit(tool: Tool, resets_at: Option<i64>) -> Tools {
        let mut tools = several();
        tools.limits.push(ActiveLimit {
            tool,
            kind: LimitKind::Session,
            text: String::new(),
            hit_at: 0,
            resets_at,
            reset_source: None,
            reset_seen_at: None,
        });
        tools
    }

    /// A hold whose tool's fallback can take it (`fallback_tool`).
    pub fn hold(
        role: SessionRole,
        reason: HoldReason,
        tool: Tool,
        agent: &str,
        model: Option<&str>,
    ) -> AgentHold {
        AgentHold {
            id: format!("h-{agent}"),
            ticket_id: Some("t".into()),
            project_id: None,
            check_agent: None,
            role,
            reason,
            tool,
            model: model.map(str::to_owned),
            agent_id: Some(agent.into()),
            fallback_tool: Some(tool),
            fallback_held: false,
            fallback_asked: false,
            since: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use otaq_core::agent_tools::connected::AgentToolState;

    use super::fixtures::*;
    use super::*;
    use crate::view_models::agents::fixtures::{agent, on};

    fn agents() -> Vec<Agent> {
        vec![
            agent("strong", "Strong", Some("opus")),
            on(CODEX, agent("standard", "Standard", None)),
            agent("sonnet", "Sonnet", Some("sonnet")),
            on(CODEX, agent("old-gpt", "Old GPT", Some("gpt-5.4"))),
            on(COPILOT, agent("verifier", "Verifier", Some("gpt-5.5"))),
        ]
    }

    fn labels(c: &Callout) -> Vec<&str> {
        c.buttons.iter().map(|b| b.label.as_str()).collect()
    }

    #[test]
    fn one_tool_reads_as_before_and_several_name_the_tool() {
        assert!(!Tools::one(None).several());
        assert!(Tools::one(None).any());
        assert!(several().several());
        assert!(!Tools::default().any(), "no tool connected");
        assert_eq!(tool_word(COPILOT), "Copilot");
        assert_eq!(tool_word(CLAUDE), "Claude Code");
        assert_eq!(tool_word(CODEX), "Codex");
        assert_eq!(
            unavailable_text(CODEX, "signed out"),
            "Codex isn\u{2019}t available \u{b7} signed out"
        );
        assert_eq!(limit_text(CODEX, None, 0), "Codex limit reached");
    }

    /// *SHOP-49 · Model unavailable*: amber, the model on hover, Use fallback when the tool's fallback can take it,
    /// Open agent… on the held agent's page, for any held session (d0108 second gap review rule 7).
    #[test]
    fn a_model_no_longer_listed_offers_open_agent() {
        let mut hold = hold(
            SessionRole::Developer,
            HoldReason::ModelUnavailable,
            CODEX,
            "old-gpt",
            Some("gpt-5.4"),
        );
        hold.fallback_tool = None;
        let c = hold_callout(&hold, &agents(), &several());
        assert_eq!(
            (c.tone, c.heading.as_str()),
            (Tone::Amber, "Model unavailable")
        );
        assert_eq!(
            c.text.as_deref(),
            Some(
                "Codex no longer lists gpt-5.4, the model of Old GPT. Give Old GPT another model in Settings \
                 \u{2192} Agents."
            )
        );
        assert_eq!(c.model.as_deref(), Some("gpt-5.4"));
        assert_eq!(labels(&c), ["Open agent\u{2026}"]);
        assert!(c.buttons[0].primary);
        assert!(c.avatar.is_some());
        let validator = AgentHold {
            role: SessionRole::Validator,
            ..hold.clone()
        };
        let c = hold_callout(&validator, &agents(), &several());
        assert_eq!(labels(&c), ["Open agent\u{2026}"]);
        assert_eq!(
            c.buttons[0].action,
            Action::OpenAgent {
                agent: Some("old-gpt".into())
            }
        );
        hold.fallback_tool = Some(CODEX);
        let c = hold_callout(&hold, &agents(), &several());
        assert_eq!(
            labels(&c),
            ["Use fallback (Claude Code)", "Open agent\u{2026}"]
        );
        assert!(c.text.unwrap().ends_with(", or use the fallback."));
        // One tool connected: no avatar.
        assert!(
            hold_callout(&hold, &agents(), &Tools::one(None))
                .avatar
                .is_none()
        );
    }

    /// "Codex isn't available · logged out" · Use fallback (Claude Code) · Change agent · Sign in….
    #[test]
    fn a_tool_that_isnt_available_offers_the_fallback_and_sign_in() {
        let mut hold = hold(
            SessionRole::Developer,
            HoldReason::ToolUnavailable,
            CODEX,
            "standard",
            None,
        );
        let c = hold_callout(&hold, &agents(), &codex_logged_out());
        assert_eq!(c.heading, "Codex isn\u{2019}t available \u{b7} signed out");
        assert_eq!(c.tone, Tone::Amber);
        assert_eq!(
            labels(&c),
            [
                "Use fallback (Claude Code)",
                "Open agent\u{2026}",
                "Sign in\u{2026}"
            ]
        );
        assert_eq!(
            c.buttons.iter().map(|b| b.primary).collect::<Vec<_>>(),
            [true, false, false]
        );
        assert_eq!(
            c.buttons[0].action,
            Action::UseFallback {
                role: RoleId::Developer
            }
        );
        assert_eq!(
            c.buttons[2].action,
            Action::SignIn {
                tool: CODEX.as_str().into()
            }
        );
        // Without a fallback that can take it: Change agent leads.
        hold.fallback_tool = None;
        let c = hold_callout(&hold, &agents(), &codex_logged_out());
        assert_eq!(labels(&c), ["Open agent\u{2026}", "Sign in\u{2026}"]);
        assert!(c.buttons[0].primary);
        // Its fallback is blocked too: the card's words (Q155).
        hold.fallback_held = true;
        let c = hold_callout(&hold, &agents(), &codex_logged_out());
        assert_eq!(
            c.text.as_deref(),
            Some("Its fallback Claude Code can\u{2019}t start either.")
        );
        hold.fallback_held = false;
        // Hidden in Settings (Q-E): Show in Agent tools, no Sign in….
        let mut hidden = several();
        hidden.statuses[1] = status(CODEX, AgentToolState::Hidden { version: None });
        let c = hold_callout(&hold, &agents(), &hidden);
        assert_eq!(
            c.heading,
            "Codex isn\u{2019}t available \u{b7} hidden in Settings"
        );
        assert_eq!(labels(&c), ["Open agent\u{2026}", "Show in Agent tools"]);
    }

    /// A limit is neutral with its reset; both blocked says so (*fallback at its limit*).
    #[test]
    fn a_limit_is_neutral_and_both_blocked_says_so() {
        let mut hold = hold(
            SessionRole::Developer,
            HoldReason::ToolLimit,
            CLAUDE,
            "strong",
            Some("opus"),
        );
        let c = hold_callout(&hold, &agents(), &at_limit(CLAUDE, None));
        assert_eq!(
            (c.tone, c.heading.as_str()),
            (Tone::Neutral, "Claude Code limit reached")
        );
        assert_eq!(labels(&c), ["Use fallback (Codex)", "Open agent\u{2026}"]);
        hold.fallback_tool = None;
        hold.fallback_held = true;
        let c = hold_callout(&hold, &agents(), &at_limit(CLAUDE, None));
        assert_eq!(
            c.text.as_deref(),
            Some("Claude Code and its fallback Codex are both at their limits")
        );
        assert_eq!(labels(&c), ["Open agent\u{2026}"]);
    }

    #[test]
    fn callouts_follow_the_rows_order() {
        let holds = [
            hold(
                SessionRole::Validator,
                HoldReason::ToolUnavailable,
                COPILOT,
                "verifier",
                None,
            ),
            hold(
                SessionRole::Developer,
                HoldReason::ModelUnavailable,
                CODEX,
                "old-gpt",
                Some("gpt-5.4"),
            ),
        ];
        let roles: Vec<_> = hold_callouts(&holds, &agents(), &several())
            .into_iter()
            .map(|c| c.role)
            .collect();
        assert_eq!(roles, [Some(RoleId::Developer), Some(RoleId::Validator)]);
    }

    /// 18 `VDJkG`: the notice for unavailable (Sign in…, New agent on Strong) and for a limit (Resume after reset).
    #[test]
    fn the_agent_panels_notice_for_unavailable_and_limit() {
        let all = agents();
        let tools = codex_logged_out();
        let on = new_agent_on(CODEX, Some(&all[1]), &all, &tools);
        assert_eq!(
            on.map(|a| a.name.as_str()),
            Some("Strong"),
            "the first on the fallback"
        );
        let c = agent_notice(CODEX, &tools, on).unwrap();
        assert_eq!(
            c.heading,
            "Codex isn\u{2019}t available \u{b7} signed out. This agent continues after you sign in."
        );
        assert_eq!(labels(&c), ["Sign in\u{2026}", "New agent on Strong"]);
        assert_eq!(
            c.buttons[1].action,
            Action::NewAgent {
                agent: "strong".into()
            }
        );
        assert_eq!(c.tone, Tone::Amber);
        let tools = at_limit(CODEX, None);
        let c = agent_notice(CODEX, &tools, on).unwrap();
        assert_eq!(
            c.heading,
            "Codex limit reached. This agent continues after the reset."
        );
        assert_eq!(labels(&c), ["Resume after reset", "New agent on Strong"]);
        assert_eq!(c.tone, Tone::Neutral);
        assert_eq!(agent_notice(CLAUDE, &tools, None), None, "its tool runs");
        // The Project agent's own agent first when it runs elsewhere; never one on the same tool.
        let on = new_agent_on(CODEX, Some(&all[4]), &all, &tools);
        assert_eq!(on.map(|a| a.name.as_str()), Some("Verifier"));
        assert_eq!(new_agent_on(CODEX, None, &all[1..2], &tools), None);
    }

    /// Q-O: a first message held for Model unavailable → Change agent opens Settings → Agents at its agent.
    #[test]
    fn a_held_first_message() {
        let mut h = hold(
            SessionRole::ProjectAgent,
            HoldReason::ModelUnavailable,
            CODEX,
            "standard",
            Some("gpt-5.4"),
        );
        h.ticket_id = None;
        h.project_id = Some("p".into());
        let c = first_message_notice(&h, "SHOP", &agents(), &several(), None);
        assert_eq!(c.heading, "Model unavailable");
        assert_eq!(labels(&c), ["Open agent\u{2026}"]);
        assert_eq!(
            c.buttons[0].action,
            Action::AgentSettings {
                project: "SHOP".into(),
                agent: Some("standard".into())
            }
        );
        h.reason = HoldReason::ToolUnavailable;
        let c = first_message_notice(
            &h,
            "SHOP",
            &agents(),
            &codex_logged_out(),
            Some(&agents()[2]),
        );
        assert_eq!(
            c.heading,
            "Codex isn\u{2019}t available \u{b7} signed out. Your message is kept: it starts after you sign in."
        );
        assert_eq!(labels(&c), ["Sign in\u{2026}", "New agent on Sonnet"]);
    }

    /// *08 → Epic lead*: the three waiting states of a lead held before its start, and a running lead's tool.
    #[test]
    fn the_leads_waiting_states() {
        let all = agents();
        let held = |reason, tools: &Tools| {
            lead_notice(
                Some(&hold(
                    SessionRole::EpicLead,
                    reason,
                    CODEX,
                    "standard",
                    Some("gpt-5.4"),
                )),
                None,
                &all,
                tools,
            )
            .unwrap()
        };
        let c = held(HoldReason::ToolUnavailable, &codex_logged_out());
        assert_eq!(
            (c.heading.as_str(), labels(&c)),
            (
                "Codex isn\u{2019}t available \u{b7} signed out",
                vec!["Sign in\u{2026}"]
            )
        );
        let c = held(HoldReason::ToolLimit, &at_limit(CODEX, None));
        assert_eq!(
            (c.heading.as_str(), c.tone),
            ("Waiting for Codex\u{2019}s limit reset", Tone::Neutral)
        );
        assert!(c.buttons.is_empty());
        let c = held(HoldReason::ModelUnavailable, &several());
        assert_eq!(
            (c.heading.as_str(), labels(&c)),
            ("Model unavailable", vec!["Open agent\u{2026}"])
        );
        assert!(matches!(c.buttons[0].action, Action::OpenAgent { .. }));
        // A running lead on a logged-out tool, and at its limit.
        let mut tools = several();
        tools.statuses[0] = status(CLAUDE, AgentToolState::FoundNotSignedIn { version: None });
        let c = lead_notice(None, Some(CLAUDE), &all, &tools).unwrap();
        assert_eq!(
            c.heading,
            "Claude Code isn\u{2019}t available \u{b7} signed out. The lead waits; children on Claude Code wait too."
        );
        assert_eq!(labels(&c), ["Sign in\u{2026}"]);
        let c = lead_notice(None, Some(CLAUDE), &all, &at_limit(CLAUDE, None)).unwrap();
        assert_eq!(
            c.heading,
            "Claude Code limit reached. The lead continues after the reset."
        );
        assert_eq!(lead_notice(None, Some(CLAUDE), &all, &several()), None);
    }

    #[test]
    fn recovery_rows_name_the_tools_problem() {
        let (text, button) = recovery_problem(CODEX, &codex_logged_out()).unwrap();
        assert_eq!(text, "Codex isn\u{2019}t available (signed out)");
        assert_eq!(button.unwrap().label, "Sign in\u{2026}");
        assert_eq!(recovery_problem(CLAUDE, &several()), None);
    }
}
