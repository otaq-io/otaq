//! The shared pieces the screens use to show who does the work (UX-65; d0108, d0088, d0086): the formatters of a model
//! ([`model_label`], [`who_model`], [`short_model`]), the agent tools' lists ([`AgentTools`]), the amber mark of a
//! model ([`mark_view`]) and the session roles as a page sends them back ([`RoleId`]). There are no named agents: a
//! stage of a flow says its own model, effort and instructions.

use otaq_core::agent_tools::follow::FollowChoice;
use otaq_core::agent_tools::{ModelKind, ModelList, Tool};
use otaq_core::agents::{AgentModel, Mark, ModelRef, SessionRole};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::tool_avatar::{AvatarTool, ToolAvatarView, shown_with};

/// The tool default as a field or a table cell shows it.
pub const TOOL_DEFAULT_LABEL: &str = "Tool default";
/// The tool default beside an agent's name ("Claude Code · tool default").
pub const TOOL_DEFAULT: &str = "tool default";
/// An agent on the account's default model (AGM-03): "default · Claude Code · sonnet".
pub const DEFAULT: &str = "default";

/// Whether `model` means the tool default: nothing, or the entry the tool lists as its own default.
pub fn is_tool_default(model: Option<&str>, list: Option<&ModelList>) -> bool {
    match model.map(str::trim) {
        None | Some("") => true,
        Some(id) => match list.and_then(|l| l.entry(id)) {
            Some(entry) => entry.kind == ModelKind::ToolDefault,
            None => id == "default" && list.is_none(),
        },
    }
}

/// A model as Settings shows it (AGT-02, AUDIT-y C2): "tool default", "<Display> · always the newest" for a follow
/// choice that resolves to one model, just its name for one that resolves per task (Copilot's "Auto · balance", Q-A),
/// else the exact id. A model the list doesn't have (or no list yet) shows as saved.
pub fn model_label(model: Option<&str>, list: Option<&ModelList>) -> String {
    if is_tool_default(model, list) {
        return TOOL_DEFAULT.to_owned();
    }
    let id = model.unwrap_or_default().trim();
    match list.and_then(|l| l.entry(id)) {
        Some(e) if e.kind == ModelKind::Follow && e.resolves_to.is_none() => e.display_name.clone(),
        Some(e) if e.kind == ModelKind::Follow => {
            format!("{} \u{b7} always the newest", e.display_name)
        }
        _ => id.to_owned(),
    }
}

/// The tool's name as people read it.
fn tool_name(tool: Tool) -> &'static str {
    AvatarTool::from(tool).name()
}

/// The tool's name in a sentence ("Copilot", "Claude Code"), as the sheet's lines say it.
pub use super::tool_problems::tool_word;

/// The tool a new agent's model gets before the service said which tools are connected: the first.
pub fn new_entry_tool() -> Tool {
    Tool::ALL[0]
}

/// Where a tool's model list stands for the picker (`GTuGI`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListState<'a> {
    /// Otaq's copy (the last one, also when a later read failed).
    Listed(&'a ModelList),
    /// No copy yet and the tool is connected: its first read runs.
    Loading,
    /// No copy, and the last read gave none: "Couldn't load … · Retry".
    Failed,
    /// No copy of a tool that isn't connected: nothing to say (as M22 before the first read).
    Unread,
}

impl<'a> ListState<'a> {
    pub fn list(self) -> Option<&'a ModelList> {
        match self {
            Self::Listed(list) => Some(list),
            _ => None,
        }
    }
}

/// What Settings → Agents knows about the agent tools (d0086): the connected ones (only Connected counts), each
/// tool's cached model list, and the tools whose last read gave no list.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AgentTools {
    pub connected: Vec<Tool>,
    pub lists: Vec<(Tool, ModelList)>,
    pub failed: Vec<Tool>,
}

impl AgentTools {
    /// The first tool connected with its list: the tests' default.
    pub fn one(list: Option<ModelList>) -> Self {
        let tool = Tool::ALL[0];
        Self {
            connected: vec![tool],
            lists: list.map(|l| (tool, l)).into_iter().collect(),
            failed: Vec::new(),
        }
    }

    /// Two or more tools connected: tool names and avatars show.
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

    pub fn is_connected(&self, tool: Tool) -> bool {
        self.connected.contains(&tool)
    }

    /// `tool`'s cached list.
    pub fn list(&self, tool: Tool) -> Option<&ModelList> {
        self.lists.iter().find(|(t, _)| *t == tool).map(|(_, l)| l)
    }

    /// Where `tool`'s list stands.
    pub fn state(&self, tool: Tool) -> ListState<'_> {
        match self.list(tool) {
            Some(list) => ListState::Listed(list),
            None if self.failed.contains(&tool) => ListState::Failed,
            None if self.is_connected(tool) => ListState::Loading,
            None => ListState::Unread,
        }
    }

    /// The first connected tool (in d0086's order).
    pub fn first(&self) -> Tool {
        Tool::ALL
            .iter()
            .copied()
            .find(|t| self.is_connected(*t))
            .unwrap_or_else(new_entry_tool)
    }

    /// The avatar of `tool`, only with several tools connected (SHL-08).
    pub fn avatar(&self, tool: Tool) -> Option<ToolAvatarView> {
        self.several().then(|| ToolAvatarView::of(tool, None))
    }

    /// A tool and model as Settings shows it: with several tools its tool first ("Claude Code · Opus · always the
    /// newest").
    pub fn model_ref(&self, model: &ModelRef) -> String {
        let label = model_label(model.model.as_deref(), self.list(model.tool));
        if self.several() {
            format!("{} \u{b7} {label}", tool_name(model.tool))
        } else {
            label
        }
    }
}

/// A model as Settings shows it (AGM-01, AGM-03, UX-65): its tool's model with the effort ("Opus · always the newest ·
/// effort high", with several tools the tool first), or "default" with what it is today ("default · Claude Code ·
/// sonnet").
pub fn who_model(
    model: &AgentModel,
    effort: Option<&str>,
    tools: &AgentTools,
    default: Option<&ModelRef>,
) -> String {
    match model {
        AgentModel::Default => match default {
            Some(model) => {
                let label = model_label(model.model.as_deref(), tools.list(model.tool));
                format!("{DEFAULT} \u{b7} {} \u{b7} {label}", tool_name(model.tool))
            }
            None => DEFAULT.to_owned(),
        },
        AgentModel::Tool { tool, model } => {
            let mut text = tools.model_ref(&ModelRef {
                tool: *tool,
                model: model.clone(),
            });
            if let Some(effort) = effort {
                text.push_str(&format!(" \u{b7} effort {effort}"));
            }
            text
        }
    }
}

/// A session role as the page sends it back, as stored (`developer`, `validator`, `project_agent`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename = "AgentRole")]
pub enum RoleId {
    Developer,
    Validator,
    ProjectAgent,
}

impl From<SessionRole> for RoleId {
    fn from(role: SessionRole) -> Self {
        match role {
            SessionRole::Developer => Self::Developer,
            SessionRole::Validator => Self::Validator,
            SessionRole::ProjectAgent => Self::ProjectAgent,
        }
    }
}

impl From<RoleId> for SessionRole {
    fn from(role: RoleId) -> Self {
        match role {
            RoleId::Developer => Self::Developer,
            RoleId::Validator => Self::Validator,
            RoleId::ProjectAgent => Self::ProjectAgent,
        }
    }
}

/// The amber mark (AGT-05, `st-paused`): "Claude Code couldn't run `claude-fable-5-1` on SHOP-48", the model in mono.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "AgentMarkView")]
pub struct MarkView {
    pub before: String,
    /// The refused model, drawn in mono between `before` and `after`.
    pub model: Option<String>,
    pub after: String,
    /// The tool's own message, in quotes.
    pub message: Option<String>,
}

/// The mark as shown, from T-0337's line: the model is cut out so it can be drawn in mono.
pub fn mark_view(mark: &Mark) -> MarkView {
    let (before, model, after) = match mark.model.as_deref().and_then(|m| {
        let at = mark.text.rfind(m)?;
        Some((at, m))
    }) {
        Some((at, m)) => (
            mark.text[..at].to_owned(),
            Some(m.to_owned()),
            mark.text[at + m.len()..].to_owned(),
        ),
        None => (mark.text.clone(), None, String::new()),
    };
    MarkView {
        before,
        model,
        after,
        message: mark.message.clone(),
    }
}

/// A model the short way the Agents page writes it (`s15-58`): a follow choice by its alias ("sonnet"), a family by
/// "newest <family>" ("newest GPT Sol"), Copilot's tiers by their name ("Auto · balance"), an exact model by its id,
/// else "tool default".
pub fn short_model(model: Option<&str>, list: Option<&ModelList>) -> String {
    if is_tool_default(model, list) {
        return TOOL_DEFAULT.to_owned();
    }
    let id = model.unwrap_or_default().trim();
    match list.and_then(|l| l.entry(id)) {
        Some(e) if e.kind == ModelKind::Follow => match FollowChoice::parse(id) {
            Some(_) if e.resolves_to.is_some() => format!("newest {}", e.display_name),
            Some(_) => e.display_name.clone(),
            None => id.to_owned(),
        },
        _ => id.to_owned(),
    }
}

/// A tool and a model the Agents page's way: "Claude Code · sonnet", "Codex · newest GPT Sol".
pub fn tool_model(model: &ModelRef, tools: &AgentTools) -> String {
    format!(
        "{} \u{b7} {}",
        tool_name(model.tool),
        short_model(model.model.as_deref(), tools.list(model.tool))
    )
}

/// A model the short way (`s15-58`), always with its tool: "Claude Code · opus · effort high", or on the default model
/// "default · Claude Code · sonnet" (just "default" before a tool connects).
pub fn model_line(
    model: &AgentModel,
    effort: Option<&str>,
    tools: &AgentTools,
    default: Option<&ModelRef>,
) -> String {
    match model {
        AgentModel::Default => match default {
            Some(model) => format!("{DEFAULT} \u{b7} {}", tool_model(model, tools)),
            None => DEFAULT.to_owned(),
        },
        AgentModel::Tool { tool, model } => {
            let mut text = tool_model(
                &ModelRef {
                    tool: *tool,
                    model: model.clone(),
                },
                tools,
            );
            if let Some(effort) = effort {
                text.push_str(&format!(" \u{b7} effort {effort}"));
            }
            text
        }
    }
}

/// [`model_line`] of a flow's model text (`claude:opus`, `default`), with its effort; "—" for none.
pub fn model_text_line(
    model: Option<&str>,
    effort: Option<&str>,
    tools: &AgentTools,
    default: Option<&ModelRef>,
) -> String {
    match model.map(AgentModel::parse) {
        Some(Ok(parsed)) => model_line(&parsed, effort, tools, default),
        Some(Err(_)) => model.unwrap_or_default().to_owned(),
        None => NOTHING.to_owned(),
    }
}

/// The tool a flow's model text runs on (`claude:opus` → Claude Code; `default`: the default model's tool). `None`
/// when it can't be told.
pub fn model_tool(model: Option<&str>, default: Option<&ModelRef>) -> Option<Tool> {
    AgentModel::parse(model?).ok()?.on(default).map(|m| m.tool)
}

/// The tool a stage an agent works or checks runs on ([`model_tool`]); `None` for a check by you.
pub fn stage_tool(stage: &otaq_core::flow::Stage, default: Option<&ModelRef>) -> Option<Tool> {
    model_tool(stage.model(), default)
}

/// One choice of a small select (a model, an effort). `AgentOption` on the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "AgentOption")]
pub struct Choice {
    /// What the page sends back; empty for "none" (the tool's effort).
    pub id: String,
    pub label: String,
    pub selected: bool,
}

pub(crate) fn choice(id: &str, label: &str, selected: bool) -> Choice {
    Choice {
        id: id.to_owned(),
        label: label.to_owned(),
        selected,
    }
}

/// An empty cell.
pub const NOTHING: &str = "\u{2014}";

/// "a", "a and b", "a, b and c".
pub fn and_list(items: &[String]) -> String {
    match items {
        [] => String::new(),
        [one] => one.clone(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

/// The agent tools' model lists the tests share.
pub mod fixtures {
    use otaq_core::agent_tools::models::ModelEntry;
    use otaq_core::agent_tools::{ModelKind, ModelList, Tool};

    pub fn entry(id: &str, display: &str, kind: ModelKind, now: Option<&str>) -> ModelEntry {
        ModelEntry {
            id: id.to_owned(),
            display_name: display.to_owned(),
            description: None,
            resolves_to: now.map(str::to_owned),
            kind,
            family: None,
            released_at: None,
        }
    }

    /// Claude Code's list as its `initialize` request gives it: its default and its four aliases.
    pub fn claude_list() -> ModelList {
        ModelList {
            entries: vec![
                entry(
                    "default",
                    "Default (recommended)",
                    ModelKind::ToolDefault,
                    Some("claude-opus-5-5"),
                ),
                entry("opus", "Opus", ModelKind::Follow, Some("claude-opus-5-5")),
                entry(
                    "sonnet",
                    "Sonnet",
                    ModelKind::Follow,
                    Some("claude-sonnet-5-5"),
                ),
                entry(
                    "haiku",
                    "Haiku",
                    ModelKind::Follow,
                    Some("claude-haiku-5-5"),
                ),
                entry(
                    "fable",
                    "Fable",
                    ModelKind::Follow,
                    Some("claude-fable-5-1"),
                ),
            ],
        }
    }

    pub fn codex_list() -> ModelList {
        ModelList {
            entries: vec![
                entry(
                    "newest:gpt-sol",
                    "GPT Sol",
                    ModelKind::Follow,
                    Some("gpt-6-sol"),
                ),
                entry(
                    "default",
                    "Default",
                    ModelKind::ToolDefault,
                    Some("gpt-6-astra"),
                ),
                entry("gpt-6-sol", "GPT-6-Sol", ModelKind::Exact, None),
                entry("gpt-5.5", "GPT-5.5", ModelKind::Exact, None),
                entry("gpt-5.5-pro", "GPT-5.5-Pro", ModelKind::Exact, None),
                entry("gpt-5.5-mini", "GPT-5.5-Mini", ModelKind::Exact, None),
            ],
        }
    }

    /// Copilot's list (T-0353): the Auto tiers it runs (resolved per task; no efficiency since Copilot 1.0.112,
    /// decision 117) with Copilot's descriptions, and one exact model a paid account lists.
    pub fn copilot_list() -> ModelList {
        let auto = |tier: &str, display: &str, description: &str| ModelEntry {
            description: Some(description.to_owned()),
            ..entry(&format!("auto@{tier}"), display, ModelKind::Follow, None)
        };
        ModelList {
            entries: vec![
                auto(
                    "balance",
                    "Auto \u{b7} balance",
                    "A balance of speed and quality",
                ),
                auto(
                    "intelligence",
                    "Auto \u{b7} intelligence",
                    "The most capable model for the task",
                ),
                entry("gpt-5.5", "GPT-5.5", ModelKind::Exact, None),
            ],
        }
    }

    /// OpenCode's list (T-0355): its default, a family, exact `provider/id` models.
    pub fn opencode_list() -> ModelList {
        ModelList {
            entries: vec![
                entry(
                    "newest:glm-flash",
                    "GLM Flash",
                    ModelKind::Follow,
                    Some("opencode/glm-5-flash"),
                ),
                entry(
                    "default",
                    "Default",
                    ModelKind::ToolDefault,
                    Some("opencode/glm-5-flash"),
                ),
                entry(
                    "opencode/glm-5-flash",
                    "GLM 5 Flash",
                    ModelKind::Exact,
                    None,
                ),
                entry(
                    "google/gemini-3-pro",
                    "Gemini 3 Pro",
                    ModelKind::Exact,
                    None,
                ),
            ],
        }
    }

    /// Every tool's list.
    pub fn all_lists() -> Vec<(Tool, ModelList)> {
        vec![
            (Tool::Claude, claude_list()),
            (Tool::Codex, codex_list()),
            (Tool::Copilot, copilot_list()),
            (Tool::OpenCode, opencode_list()),
        ]
    }

    /// `connected` tools, with every tool's list.
    pub fn tools(connected: &[Tool]) -> super::AgentTools {
        super::AgentTools {
            connected: connected.to_vec(),
            lists: all_lists(),
            failed: Vec::new(),
        }
    }

    /// Claude Code and Codex connected (`soGMU`).
    pub fn two_tools() -> super::AgentTools {
        tools(&[Tool::Claude, Tool::Codex])
    }
}

#[cfg(test)]
mod tests {
    use super::fixtures::*;
    use super::*;

    #[test]
    fn models_read_the_short_way() {
        let list = claude_list();
        assert_eq!(
            model_label(Some("opus"), Some(&list)),
            "Opus \u{b7} always the newest"
        );
        assert_eq!(short_model(None, Some(&list)), "tool default");
        assert_eq!(short_model(Some("opus"), Some(&list)), "opus");
        assert_eq!(short_model(Some("claude-x"), Some(&list)), "claude-x");
        assert_eq!(
            short_model(Some("newest:gpt-sol"), Some(&codex_list())),
            "newest GPT Sol"
        );
        assert_eq!(
            short_model(Some("auto@balance"), Some(&copilot_list())),
            "Auto \u{b7} balance"
        );
        let tools = AgentTools::one(Some(list));
        assert_eq!(
            model_text_line(Some("claude:opus"), Some("high"), &tools, None),
            "Claude Code \u{b7} opus \u{b7} effort high"
        );
        assert_eq!(
            model_text_line(Some("default"), None, &tools, None),
            "default"
        );
        let sonnet = ModelRef {
            tool: Tool::Claude,
            model: Some("sonnet".into()),
        };
        assert_eq!(
            model_text_line(Some("default"), None, &tools, Some(&sonnet)),
            "default \u{b7} Claude Code \u{b7} sonnet"
        );
        assert_eq!(model_text_line(None, None, &tools, None), NOTHING);
    }
}
