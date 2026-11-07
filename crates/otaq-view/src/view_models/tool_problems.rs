//! The agent tools as the sheet, the agent panel and recovery see them, and what they say about a tool that
//! can't start or continue an agent (AGT-04, AGT-05, AGT-08, SHL-04, USG-10, §7.2; d0086, d0087, d0088, d0077;
//! *05 → Column 3 · several tools (M23)*, *18 → Row · Pane: empty, hidden, tool problems, menu (0087)*, *16 → Row ·
//! Recovery per tool (0086)*).
//!
//! - **The tools** ([`Tools`]): which are connected, each one's state and active limit, and each one's cached model
//!   list. With one tool connected every screen reads as M22: no tool names, no avatars ([`Tools::several`]).
//! - **A hold** ([`hold_callout`]): a role that would start but waits for its tool or its model (SCH-04 `[M23]`):
//!   "Codex isn't available · logged out" with Open agent… and Sign in…; "Claude Code limit reached · resets 3:45
//!   pm"; "Model unavailable" with Open agent…. It waits for its own tool: nothing moves to another one.
//! - **A running or ended agent** whose tool isn't available or is at its limit ([`agent_notice`]):
//!   it waits, since a conversation never moves tools; the agent panel offers Sign in… (or Resume after reset), and
//!   never another agent in its place.
//!
//! Unavailable, a model no longer listed and moves are amber; limits are neutral; nothing here is turquoise (d0077).

use otaq_core::agent_tools::Tool;
use otaq_core::agent_tools::connected::{AgentToolState, NotAvailable, ToolStatus};
use otaq_core::agent_tools::models::ModelList;
use otaq_core::agents::{Agent, AgentHold, HoldReason, ModelRef, SessionRole};
use otaq_core::usage::plan::ActiveLimit;
use serde::Serialize;
use ts_rs::TS;

use crate::view_models::agents::{RoleId, model_label};
use crate::view_models::clock::reset_label;
use crate::view_models::tool_avatar::{AvatarTool, ToolAvatarView, shown_with};

/// What the views know about the agent tools: the connected ones (Connected only counts, d0086), every tool's state
/// (`agent_tools.list`), the active limits per tool, each tool's cached model list, the default model, and the time
/// now.
#[derive(Debug, Clone, Default)]
pub struct Tools {
    pub connected: Vec<Tool>,
    pub statuses: Vec<ToolStatus>,
    pub limits: Vec<ActiveLimit>,
    pub lists: Vec<(Tool, ModelList)>,
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
    /// Open agent…: Settings → Agents on `agent`'s page (AGM-03, d0108 second gap review rule 7; replaces Change
    /// agent, Q280); the list when the agent isn't known.
    OpenAgent { agent: Option<String> },
    /// Change agent…: the ticket's dialog that picks another agent for the waiting stage, one whose tool has the
    /// stage's skill and can run now (`sheet.change_agent_dialog`, then `sheet.set_choice`).
    ChangeAgent,
    /// Check again on a stage held because its tool doesn't have the stage's skill: Otaq's finder is asked again and
    /// the stage starts if the skill is found (`ticket.check_again`).
    CheckAgain,
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
pub const CHANGE_AGENT: &str = "Change agent\u{2026}";
pub const CHECK_AGAIN: &str = "Check again";
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

/// Change agent… on a ticket's waiting stage.
fn change_agent() -> Button {
    Button {
        label: CHANGE_AGENT.to_owned(),
        action: Action::ChangeAgent,
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

/// The callout of a held role (§7.2, SCH-04 `[M23]`, AGT-08; *05 → Model unavailable (SHOP-49)*): `agents` is every
/// agent (the held one). It waits for its own tool; Open agent… leads, and a ticket's waiting stage offers Change
/// agent… (it hasn't started, so its agent can still change).
pub fn hold_callout(hold: &AgentHold, agents: &[Agent], tools: &Tools) -> Callout {
    let tool = hold.tool;
    let name = find(agents, hold.agent_id.as_deref()).map_or("The agent", |a| a.name.as_str());
    let problem = tools.problem(tool);
    // A stage held for its skill: Check again leads, then Change agent… (only agents whose tool has the skill).
    if hold.reason == HoldReason::MissingSkill {
        let skill = hold.skill.as_deref().unwrap_or_default();
        return Callout {
            role: Some(hold.role.into()),
            tone: Tone::Amber,
            avatar: tools.avatar(tool, hold.model.as_deref()),
            heading: otaq_core::flow::proposal::missing_skill_text(tool, skill),
            text: Some(format!(
                "Nothing started. Link the skill into {}\u{2019}s skills folder or commit it to the project, then \
                 Check again; or give the stage an agent whose tool has it.",
                tool_word(tool)
            )),
            model: None,
            buttons: [
                Some(Button {
                    label: CHECK_AGAIN.to_owned(),
                    action: Action::CheckAgain,
                    primary: true,
                }),
                hold.ticket_id.is_some().then(change_agent),
            ]
            .into_iter()
            .flatten()
            .collect(),
        };
    }
    let mut open = open_agent(hold.agent_id.as_deref());
    open.primary = true;
    let mut buttons = vec![open];
    let (tone, heading, text) = match hold.reason {
        HoldReason::ModelUnavailable => {
            let model = model_text(hold, tools);
            (
                Tone::Amber,
                MODEL_UNAVAILABLE.to_owned(),
                Some(format!(
                    "{} no longer lists {model}, the model of {name}. Give {name} another model in Settings \u{2192} \
                     Agents.",
                    tool_word(tool),
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
                Some(format!(
                    "Nothing started and nothing is locked. {name} starts once {} is available again.",
                    tool_word(tool)
                )),
            )
        }
        HoldReason::MissingSkill => unreachable!("answered above"),
        HoldReason::ToolLimit => {
            let resets_at = match problem {
                Some(Problem::Limit { resets_at }) => resets_at,
                _ => None,
            };
            (
                Tone::Neutral,
                limit_text(tool, resets_at, tools.now),
                Some(format!(
                    "Nothing started and nothing is locked. {name} starts when you resume {} after its reset.",
                    tool_word(tool)
                )),
            )
        }
    };
    if hold.ticket_id.is_some() {
        buttons.push(change_agent());
    }
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

/// Every held role's callout, in the order of the rows (Developer, Validator).
pub fn hold_callouts(holds: &[AgentHold], agents: &[Agent], tools: &Tools) -> Vec<Callout> {
    let mut holds: Vec<&AgentHold> = holds.iter().collect();
    holds.sort_by_key(|h| SessionRole::ALL.iter().position(|r| *r == h.role));
    holds
        .into_iter()
        .map(|h| hold_callout(h, agents, tools))
        .collect()
}

/// The agent panel's notice for a running or ended Noyan conversation whose tool isn't available or is at its limit
/// (18 `VDJkG`): it waits, since a conversation never moves tools, and no other agent is offered in its place (no
/// fallback agents: if the tool can't run, the conversation waits).
pub fn agent_notice(tool: Tool, tools: &Tools) -> Option<Callout> {
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
/// agent it would start on; a tool that can't run waits like a running conversation.
pub fn first_message_notice(
    hold: &AgentHold,
    project: &str,
    agents: &[Agent],
    tools: &Tools,
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
    let mut notice = agent_notice(tool, tools).unwrap_or_else(|| Callout {
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
    use otaq_core::agents::{AgentHold, HoldReason, SessionRole};
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

    /// Claude Code, Codex and Copilot connected; OpenCode not installed.
    pub fn several() -> Tools {
        Tools::from_statuses(vec![
            connected(CLAUDE),
            connected(CODEX),
            connected(COPILOT),
            status(OPENCODE, AgentToolState::NotInstalled),
        ])
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

    /// A hold of `role` on `tool`.
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
            role,
            reason,
            tool,
            model: model.map(str::to_owned),
            agent_id: Some(agent.into()),
            skill: None,
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

    /// A stage held because its tool doesn't have its skill: the heading names the tool and the skill, and the
    /// buttons are Check again (first) and Change agent….
    #[test]
    fn a_stage_held_for_its_skill_offers_check_again() {
        let mut held = hold(
            SessionRole::Validator,
            HoldReason::MissingSkill,
            CODEX,
            "standard",
            None,
        );
        held.skill = Some("team-validate".into());
        let c = hold_callout(&held, &agents(), &several());
        assert_eq!(
            c.heading,
            "Codex doesn\u{2019}t have the skill `/team-validate`"
        );
        assert_eq!(c.tone, Tone::Amber);
        assert_eq!(labels(&c), [CHECK_AGAIN, CHANGE_AGENT]);
        assert_eq!(c.buttons[0].action, Action::CheckAgain);
        assert!(c.buttons[0].primary);
    }

    /// *SHOP-49 · Model unavailable*: amber, the model on hover, Open agent… on the held agent's page, for any held session (d0108 second gap review rule 7).
    #[test]
    fn a_model_no_longer_listed_offers_open_agent() {
        let hold = hold(
            SessionRole::Developer,
            HoldReason::ModelUnavailable,
            CODEX,
            "old-gpt",
            Some("gpt-5.4"),
        );
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
        // A ticket's waiting stage hasn't started: Change agent… picks another agent for it.
        assert_eq!(c.buttons[1].action, Action::ChangeAgent);
        assert_eq!(labels(&c), ["Open agent\u{2026}", "Change agent\u{2026}"]);
        assert!(c.buttons[0].primary);
        assert!(c.avatar.is_some());
        let validator = AgentHold {
            role: SessionRole::Validator,
            ..hold.clone()
        };
        let c = hold_callout(&validator, &agents(), &several());
        assert_eq!(labels(&c), ["Open agent\u{2026}", "Change agent\u{2026}"]);
        assert_eq!(
            c.buttons[0].action,
            Action::OpenAgent {
                agent: Some("old-gpt".into())
            }
        );
        // One tool connected: no avatar.
        assert!(
            hold_callout(&hold, &agents(), &Tools::one(None))
                .avatar
                .is_none()
        );
    }

    /// "Codex isn't available · logged out" · Open agent… · Sign in…: it waits for Codex, whatever other tool runs.
    #[test]
    fn a_tool_that_isnt_available_offers_open_agent_and_sign_in() {
        let hold = hold(
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
                "Open agent\u{2026}",
                "Sign in\u{2026}",
                "Change agent\u{2026}"
            ]
        );
        assert_eq!(
            c.buttons.iter().map(|b| b.primary).collect::<Vec<_>>(),
            [true, false, false]
        );
        assert_eq!(
            c.buttons[1].action,
            Action::SignIn {
                tool: CODEX.as_str().into()
            }
        );
        assert_eq!(
            c.text.as_deref(),
            Some(
                "Nothing started and nothing is locked. Standard starts once Codex is available again."
            )
        );
        // Hidden in Settings (Q-E): Show in Agent tools, no Sign in….
        let mut hidden = several();
        hidden.statuses[1] = status(CODEX, AgentToolState::Hidden { version: None });
        let c = hold_callout(&hold, &agents(), &hidden);
        assert_eq!(
            c.heading,
            "Codex isn\u{2019}t available \u{b7} hidden in Settings"
        );
        assert_eq!(
            labels(&c),
            [
                "Open agent\u{2026}",
                "Show in Agent tools",
                "Change agent\u{2026}"
            ]
        );
    }

    /// A limit is neutral with its reset; it waits for its own tool, with Open agent….
    #[test]
    fn a_limit_is_neutral_and_waits_for_its_tool() {
        let hold = hold(
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
        assert_eq!(labels(&c), ["Open agent\u{2026}", "Change agent\u{2026}"]);
        assert_eq!(
            c.text.as_deref(),
            Some(
                "Nothing started and nothing is locked. Strong starts when you resume Claude Code after its reset."
            )
        );
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

    /// 18 `VDJkG`: the notice for unavailable (Sign in…) and for a limit (Resume after reset). No other agent is
    /// offered in its place: the conversation waits for its own tool.
    #[test]
    fn the_agent_panels_notice_for_unavailable_and_limit() {
        let tools = codex_logged_out();
        let c = agent_notice(CODEX, &tools).unwrap();
        assert_eq!(
            c.heading,
            "Codex isn\u{2019}t available \u{b7} signed out. This agent continues after you sign in."
        );
        assert_eq!(labels(&c), ["Sign in\u{2026}"]);
        assert_eq!(c.tone, Tone::Amber);
        let tools = at_limit(CODEX, None);
        let c = agent_notice(CODEX, &tools).unwrap();
        assert_eq!(
            c.heading,
            "Codex limit reached. This agent continues after the reset."
        );
        assert_eq!(labels(&c), ["Resume after reset"]);
        assert_eq!(c.tone, Tone::Neutral);
        assert_eq!(agent_notice(CLAUDE, &tools), None, "its tool runs");
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
        let c = first_message_notice(&h, "SHOP", &agents(), &several());
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
        let c = first_message_notice(&h, "SHOP", &agents(), &codex_logged_out());
        assert_eq!(
            c.heading,
            "Codex isn\u{2019}t available \u{b7} signed out. Your message is kept: it starts after you sign in."
        );
        assert_eq!(labels(&c), ["Sign in\u{2026}"], "no other agent is offered");
    }

    #[test]
    fn recovery_rows_name_the_tools_problem() {
        let (text, button) = recovery_problem(CODEX, &codex_logged_out()).unwrap();
        assert_eq!(text, "Codex isn\u{2019}t available (signed out)");
        assert_eq!(button.unwrap().label, "Sign in\u{2026}");
        assert_eq!(recovery_problem(CLAUDE, &several()), None);
    }
}
