//! Settings → Agents (AGM-02, AGM-03, AGM-04; d0108, d0088, d0086; `s15-58` / `s15-59`): the default model for new agents, the fallback per tool and the list of agents (name, role tag, came-from line, model
//! with the tool's effort, skills, used in, the amber mark, ⋯ Duplicate / Delete…); one agent's page is
//! [`super::agent_page`]. Also the shared pieces the other screens use: the formatters of a model ([`model_label`],
//! [`agent_model`], [`short_model`]), the agent tools' lists ([`AgentTools`]) and the session roles as the page sends
//! them back ([`RoleId`]).

use otaq_core::agent_tools::follow::FollowChoice;
use otaq_core::agent_tools::{ModelKind, ModelList, Tool};
use otaq_core::agents::{
    Agent, AgentList, AgentModel, InUse, Mark, ModelRef, Role, SessionRole, SkillRef,
};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::menu::MenuItem;
use super::tool_avatar::{AvatarTool, ToolAvatarView, shown_with};
use crate::icons::IconName;

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

/// An agent's model as Settings shows it (AGM-01, AGM-03): its tool's model with the effort ("Opus · always the newest
/// · effort high", with several tools the tool first), or "default" with what it is today ("default · Claude Code ·
/// sonnet").
pub fn agent_model(agent: &Agent, tools: &AgentTools, default: Option<&ModelRef>) -> String {
    match &agent.model {
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
            if let Some(effort) = &agent.effort {
                text.push_str(&format!(" \u{b7} effort {effort}"));
            }
            text
        }
    }
}

/// A session role as the page sends it back, as stored (`developer`, `validator`, `lead`, `project_agent`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename = "AgentRole")]
pub enum RoleId {
    Developer,
    Validator,
    #[serde(rename = "lead")]
    EpicLead,
    ProjectAgent,
}

impl From<SessionRole> for RoleId {
    fn from(role: SessionRole) -> Self {
        match role {
            SessionRole::Developer => Self::Developer,
            SessionRole::Validator => Self::Validator,
            SessionRole::EpicLead => Self::EpicLead,
            SessionRole::ProjectAgent => Self::ProjectAgent,
        }
    }
}

impl From<RoleId> for SessionRole {
    fn from(role: RoleId) -> Self {
        match role {
            RoleId::Developer => Self::Developer,
            RoleId::Validator => Self::Validator,
            RoleId::EpicLead => Self::EpicLead,
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

/// An agent's model in the list and on its page (AGM-01, AGM-03): "Claude Code · opus · effort high", or on the
/// default model "default · Claude Code · sonnet" (just "default" before a tool connects).
pub fn list_model(agent: &Agent, tools: &AgentTools, default: Option<&ModelRef>) -> String {
    match &agent.model {
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
            if let Some(effort) = &agent.effort {
                text.push_str(&format!(" \u{b7} effort {effort}"));
            }
            text
        }
    }
}

/// What uses an agent, one place each (AGM-01): "every project · the Project agent", "every project · epics",
/// "pocket-api · Dev", "shop-web · Dev (option)".
pub fn used_in(uses: &InUse) -> Vec<String> {
    let role = uses.role.map(|role| {
        match role {
            Role::ProjectAgent => "every project \u{b7} the Project agent",
            Role::EpicLead => "every project \u{b7} epics",
        }
        .to_owned()
    });
    let stages = uses.stages.iter().map(|s| {
        let label = curly(&s.label());
        if s.option {
            format!("{label} (option)")
        } else {
            label
        }
    });
    role.into_iter().chain(stages).collect()
}

/// The list's Used in cell: the places joined, "—" for none.
fn used_in_cell(uses: Option<&InUse>) -> String {
    match uses.map(used_in).filter(|u| !u.is_empty()) {
        Some(places) => places.join(", "),
        None => NOTHING.to_owned(),
    }
}

/// An empty cell.
pub const NOTHING: &str = "\u{2014}";

/// One choice of a tool-and-model dropdown (the default model, a fallback): a tool's heading, a model, a note in
/// place of a tool's models (loading, couldn't load), or the fallback's "None".
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "AgentsModelChoice")]
pub struct ModelChoice {
    /// Unique in the list: what the page sends back is `tool` and `model`.
    pub id: String,
    pub kind: ChoiceKind,
    #[ts(type = "string | null")]
    pub tool: Option<Tool>,
    /// `None`: the tool's default.
    pub model: Option<String>,
    pub label: String,
    /// The muted part: "now claude-opus-5-5".
    pub detail: Option<String>,
    pub mono: bool,
    pub selected: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename = "AgentsChoiceKind")]
pub enum ChoiceKind {
    Heading,
    Option,
    Note,
}

/// A dropdown of a tool and a model from that tool's list (AGM-03): every connected tool under its name, each with
/// its own list as the model picker has it. Read-only while no tool is connected.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "AgentsModelDropdown")]
pub struct ModelDropdown {
    /// The field's text: "Claude Code · sonnet", "None".
    pub value: String,
    pub choices: Vec<ModelChoice>,
    pub read_only: bool,
}

/// The fallback's "None".
pub const NONE: &str = "None";

/// The dropdown over `tools` (in d0086's order), `picked` selected; `none` adds the fallback's "None" first.
fn dropdown(
    on: &[Tool],
    tools: &AgentTools,
    picked: Option<&ModelRef>,
    none: bool,
    empty: &str,
) -> ModelDropdown {
    let mut choices = Vec::new();
    if none {
        choices.push(ModelChoice {
            id: "none".to_owned(),
            kind: ChoiceKind::Option,
            tool: None,
            model: None,
            label: NONE.to_owned(),
            detail: None,
            mono: false,
            selected: picked.is_none(),
        });
    }
    for &tool in on {
        choices.push(ModelChoice {
            id: format!("heading:{tool}"),
            kind: ChoiceKind::Heading,
            tool: Some(tool),
            model: None,
            label: tool_name(tool).to_owned(),
            detail: None,
            mono: false,
            selected: false,
        });
        let mine = picked.filter(|p| p.tool == tool);
        let picker = super::model_picker::picker(
            tool,
            tools.state(tool),
            mine.and_then(|p| p.model.as_deref()),
            false,
            None,
        );
        for item in picker.groups.iter().flat_map(|g| &g.items) {
            choices.push(ModelChoice {
                id: format!("{tool}:{}", item.value.as_deref().unwrap_or_default()),
                kind: ChoiceKind::Option,
                tool: Some(tool),
                model: item.value.clone(),
                label: item.label.clone(),
                detail: item.detail.clone(),
                mono: item.mono,
                selected: mine.is_some() && item.selected,
            });
        }
        if let Some(status) = &picker.status {
            choices.push(ModelChoice {
                id: format!("status:{tool}"),
                kind: ChoiceKind::Note,
                tool: Some(tool),
                model: None,
                label: status.text.clone(),
                detail: None,
                mono: false,
                selected: false,
            });
        }
    }
    ModelDropdown {
        value: match picked {
            Some(model) => tool_model(model, tools),
            None if none => NONE.to_owned(),
            None => empty.to_owned(),
        },
        read_only: on.is_empty(),
        choices,
    }
}

/// The connected tools, in d0086's order.
fn connected(tools: &AgentTools) -> Vec<Tool> {
    Tool::ALL
        .iter()
        .copied()
        .filter(|t| tools.is_connected(*t))
        .collect()
}

/// Default model for new agents (AGM-03).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "AgentsDefaultModel")]
pub struct DefaultModel {
    pub label: String,
    pub description: String,
    pub dropdown: ModelDropdown,
}

/// One line of Fallback per tool: "When Claude Code hits its limit →" and its dropdown of the other tools.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "AgentsFallbackLine")]
pub struct FallbackLine {
    #[ts(type = "string")]
    pub tool: Tool,
    pub label: String,
    pub dropdown: ModelDropdown,
}

/// Fallback per tool (AGM-03), with two or more tools connected: a fallback must be on another tool.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "AgentsFallbacks")]
pub struct Fallbacks {
    pub label: String,
    pub description: String,
    pub lines: Vec<FallbackLine>,
}

/// One agent in the list.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(rename = "AgentListRow")]
pub struct TableRow {
    pub id: String,
    pub name: String,
    /// "Project agent", "Epic lead": the role tag.
    pub role: Option<String>,
    /// "From Otaq's agents file", "Added by the Otaq default preset", "Added by you".
    pub came_from: String,
    /// The model the Agents page's way ([`list_model`]).
    pub model: String,
    /// Its skills by name, in order: a shipped one by its name, an imported one by its `name:`.
    pub skills: Vec<String>,
    /// "pocket-api · Dev, pocket-api · Done", "—".
    pub used_in: String,
    pub mark: Option<MarkView>,
    /// After the mark: "Edit…" opens the agent's page (`s15-32`).
    pub mark_link: Option<String>,
    /// The ⋯ menu: Duplicate, Delete….
    pub menu: Vec<MenuItem>,
    /// The ⋯ button's name: "More for Strong".
    pub menu_label: String,
}

/// The Agents page (AGM-04; `s15-58` / `s15-59`): the default model, the fallbacks, the list.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(rename = "AgentsSection")]
pub struct Section {
    pub title: String,
    pub intro: String,
    pub new_agent_label: String,
    /// The header's ⋯ (FLW-10): Apply the shipped agents file again, which opens the flow editor for the project on
    /// screen.
    pub more_label: String,
    pub more: Vec<MenuItem>,
    pub default_model: DefaultModel,
    pub fallbacks: Option<Fallbacks>,
    /// NAME, MODEL, SKILLS, USED IN.
    pub columns: Vec<String>,
    pub rows: Vec<TableRow>,
}

/// The header's ⋯ item (FLW-10): the flow editor with `otaq-agents.yaml`, Keep mine · Take theirs per changed agent; the
/// way back after a broken edit.
pub const AGENTS_FILE: &str = "agents_file";
pub const APPLY_THE_SHIPPED_AGENTS_FILE_AGAIN: &str = "Apply the shipped agents file again";

/// The ⋯ menu's items, by the id the page sends back.
pub const DUPLICATE: &str = "duplicate";
pub const DELETE: &str = "delete";
pub const MENU_LABEL: &str = "More actions";

/// Duplicate, Delete… (AGT-02's menu, kept by AGM-04).
pub fn agent_menu() -> Vec<MenuItem> {
    vec![
        MenuItem {
            icon: Some(IconName::Copy),
            ..MenuItem::new(DUPLICATE, "Duplicate")
        },
        MenuItem {
            icon: Some(IconName::Trash2),
            danger: true,
            separated: true,
            ..MenuItem::new(DELETE, "Delete\u{2026}")
        },
    ]
}

/// The skills of `agent` by name.
pub fn skill_names(agent: &Agent) -> Vec<String> {
    agent
        .skills
        .iter()
        .map(|s| match s {
            SkillRef::Shipped(name) => name.clone(),
            SkillRef::Imported(id) => agent
                .imported_skill(id)
                .map_or_else(|| id.clone(), |i| i.name.clone()),
        })
        .collect()
}

/// "a", "a and b", "a, b and c".
pub fn and_list(items: &[String]) -> String {
    match items {
        [] => String::new(),
        [one] => one.clone(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

/// The core's words with the frames' apostrophe: "From Otaq’s agents file".
fn curly(text: &str) -> String {
    text.replace('\'', "\u{2019}")
}

/// An agent's came-from line: "From Otaq’s agents file", "Added by you".
pub fn came_from(agent: &Agent) -> String {
    curly(&agent.came_from.label())
}

/// What the page is built from: `agents.list`, the tools, and what uses each agent (`agents.in_use`).
pub struct Inputs<'a> {
    pub list: &'a AgentList,
    pub tools: &'a AgentTools,
    pub uses: &'a [InUse],
}

impl Inputs<'_> {
    /// What uses `agent` (by name: `InUse` names it).
    pub fn uses_of(&self, agent: &Agent) -> Option<&InUse> {
        self.uses.iter().find(|u| u.agent == agent.name)
    }
}

/// The page from `agents.list`.
pub fn section(input: &Inputs) -> Section {
    let (list, tools) = (input.list, input.tools);
    let default = list.default_model.as_ref();
    let on = connected(tools);
    let fallbacks = (on.len() > 1).then(|| Fallbacks {
        label: "Fallback per tool".to_owned(),
        description: "Used when a tool hits its plan limit.".to_owned(),
        lines: on
            .iter()
            .map(|&tool| {
                let others: Vec<Tool> = on.iter().copied().filter(|t| *t != tool).collect();
                let picked = list
                    .fallbacks
                    .iter()
                    .find(|f| f.tool == tool)
                    .map(|f| &f.fallback);
                FallbackLine {
                    tool,
                    label: format!("When {} hits its limit \u{2192}", tool_name(tool)),
                    dropdown: dropdown(&others, tools, picked, true, NONE),
                }
            })
            .collect(),
    });
    Section {
        title: "Agents".to_owned(),
        intro: "What each agent does: its instructions, its skills and its own model. Agents are shared by all your \
                projects. Two agents hold a role: the Project agent and the Epic lead. Edits apply to the next session \
                that starts."
            .to_owned(),
        new_agent_label: "New agent".to_owned(),
        more_label: "More agent actions".to_owned(),
        more: vec![MenuItem {
            icon: Some(IconName::RotateCcw),
            ..MenuItem::new(AGENTS_FILE, APPLY_THE_SHIPPED_AGENTS_FILE_AGAIN)
        }],
        default_model: DefaultModel {
            label: "Default model for new agents".to_owned(),
            description: "A new agent, or one a preset adds, starts with it.".to_owned(),
            dropdown: dropdown(&on, tools, default, false, NO_DEFAULT),
        },
        fallbacks,
        columns: ["Name", "Model", "Skills", "Used in"]
            .map(str::to_owned)
            .to_vec(),
        rows: list
            .agents
            .iter()
            .map(|a| TableRow {
                id: a.id.clone(),
                name: a.name.clone(),
                role: a.role.map(|r| r.label().to_owned()),
                came_from: came_from(a),
                model: list_model(a, tools, default),
                skills: skill_names(a),
                used_in: used_in_cell(input.uses_of(a)),
                mark: a.mark.as_ref().map(mark_view),
                mark_link: a.mark.as_ref().map(|_| "Edit\u{2026}".to_owned()),
                menu: agent_menu(),
                menu_label: format!("More for {}", a.name),
            })
            .collect(),
    }
}

/// The default model's field before a tool connects.
pub const NO_DEFAULT: &str = "The first agent tool you connect";

/// The agent tools' model lists the tests share.
pub mod fixtures {
    use otaq_core::agent_tools::models::ModelEntry;
    use otaq_core::agent_tools::{ModelKind, ModelList, Tool};
    use otaq_core::agents::in_use::StageUse;
    use otaq_core::agents::{
        Agent, AgentList, AgentModel, CameFrom, ImportedSkill, InUse, ModelRef, Role, SkillRef,
    };

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

    /// An agent on Claude Code with `model` (`None`: the tool's default).
    pub fn agent(id: &str, name: &str, model: Option<&str>) -> Agent {
        Agent {
            id: id.to_owned(),
            name: name.to_owned(),
            instructions: String::new(),
            skills: Vec::new(),
            imported: Vec::new(),
            model: AgentModel::Tool {
                tool: Tool::ALL[0],
                model: model.map(str::to_owned),
            },
            effort: None,
            role: None,
            came_from: CameFrom::You,
            mark: None,
            created_at: 1,
            updated_at: 1,
        }
    }

    /// `agent` on `tool` instead.
    pub fn on(tool: Tool, mut agent: Agent) -> Agent {
        let model = match agent.model {
            AgentModel::Tool { model, .. } => model,
            AgentModel::Default => None,
        };
        agent.model = AgentModel::Tool { tool, model };
        agent
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

    /// Copilot's list (T-0353): the three Auto tiers (resolved per task) with Copilot's descriptions, and one exact
    /// model a paid account lists.
    pub fn copilot_list() -> ModelList {
        let auto = |tier: &str, display: &str, description: &str| ModelEntry {
            description: Some(description.to_owned()),
            ..entry(&format!("auto@{tier}"), display, ModelKind::Follow, None)
        };
        ModelList {
            entries: vec![
                auto(
                    "efficiency",
                    "Auto \u{b7} efficiency",
                    "Fast and light on credits",
                ),
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

    /// The ticket file's example (T-0405, AGM-02): Planner, Epic lead, Developer, Validator (with the imported
    /// `team-validate`), Strong and Light, the default model Light's, and what uses each agent (pocket-notes
    /// validates, pocket-api doesn't).
    pub fn example() -> (AgentList, Vec<InUse>) {
        let with = |mut a: Agent, f: &dyn Fn(&mut Agent)| {
            f(&mut a);
            a
        };
        let instructions = "Run `just test` before committing";
        let skills = vec![
            SkillRef::Shipped("work-ticket".into()),
            SkillRef::Shipped("commit".into()),
        ];
        let agents = vec![
            with(agent("planner", "Planner", Some("haiku")), &|a| {
                a.role = Some(Role::ProjectAgent);
                a.came_from = CameFrom::AgentsFile;
                a.instructions = "Keep tickets small".into();
                a.skills = vec![SkillRef::Shipped("noyan".into())];
            }),
            with(agent("lead", "Epic lead", Some("opus")), &|a| {
                a.role = Some(Role::EpicLead);
                a.came_from = CameFrom::AgentsFile;
                a.skills = vec![SkillRef::Shipped("epic-lead".into())];
            }),
            with(agent("developer", "Developer", Some("haiku")), &|a| {
                a.came_from = CameFrom::DefaultPreset;
                a.instructions = instructions.into();
                a.skills.clone_from(&skills);
            }),
            with(agent("validator", "Validator", Some("opus")), &|a| {
                a.came_from = CameFrom::DefaultPreset;
                a.skills = vec![SkillRef::Imported("team-validate".into())];
                a.imported = vec![ImportedSkill {
                    id: "team-validate".into(),
                    name: "team-validate".into(),
                    original: "/team/validate".into(),
                    imported_at: 1,
                    fingerprint: String::new(),
                    warnings: Vec::new(),
                }];
            }),
            with(agent("strong", "Strong", Some("opus")), &|a| {
                a.instructions = instructions.into();
                a.skills.clone_from(&skills);
            }),
            with(agent("light", "Light", Some("haiku")), &|a| {
                a.instructions = instructions.into();
                a.skills.clone_from(&skills);
            }),
        ];
        let stage = |project: &str, stage: &str| StageUse {
            project: Some(project.into()),
            stage: stage.into(),
            option: false,
        };
        let uses = vec![
            InUse {
                agent: "Planner".into(),
                role: Some(Role::ProjectAgent),
                stages: Vec::new(),
            },
            InUse {
                agent: "Epic lead".into(),
                role: Some(Role::EpicLead),
                stages: Vec::new(),
            },
            InUse {
                agent: "Developer".into(),
                role: None,
                stages: vec![
                    stage("pocket-api", "Dev"),
                    stage("pocket-api", "Done"),
                    stage("pocket-notes", "Dev"),
                    stage("pocket-notes", "Done"),
                ],
            },
            InUse {
                agent: "Validator".into(),
                role: None,
                stages: vec![stage("pocket-notes", "Validate")],
            },
            InUse {
                agent: "Strong".into(),
                ..InUse::default()
            },
            InUse {
                agent: "Light".into(),
                ..InUse::default()
            },
        ];
        let list = AgentList {
            agents,
            default_model: Some(ModelRef {
                tool: Tool::Claude,
                model: Some("haiku".into()),
            }),
            fallbacks: Vec::new(),
        };
        (list, uses)
    }
}

#[cfg(test)]
mod tests {
    use otaq_core::naming::PRODUCT_NAME;

    use super::fixtures::*;
    use super::*;

    #[test]
    fn models_read_the_agents_pages_way() {
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
        let mut strong = agent("s", "Strong", Some("opus"));
        strong.effort = Some("high".into());
        assert_eq!(
            list_model(&strong, &tools, None),
            "Claude Code \u{b7} opus \u{b7} effort high"
        );
        let mut planner = agent("p", "Planner", None);
        planner.model = AgentModel::Default;
        assert_eq!(list_model(&planner, &tools, None), "default");
        let sonnet = ModelRef {
            tool: Tool::Claude,
            model: Some("sonnet".into()),
        };
        assert_eq!(
            list_model(&planner, &tools, Some(&sonnet)),
            "default \u{b7} Claude Code \u{b7} sonnet"
        );
    }

    /// The example (T-0410's Done-when): six agents with their roles, came-from lines, models, skills and where they
    /// are used.
    #[test]
    fn the_examples_agents_page() {
        let (list, uses) = example();
        let tools = AgentTools::one(Some(claude_list()));
        let page = section(&Inputs {
            list: &list,
            tools: &tools,
            uses: &uses,
        });
        assert_eq!(page.title, "Agents");
        assert_eq!(
            page.intro,
            "What each agent does: its instructions, its skills and its own model. Agents are shared by all your \
             projects. Two agents hold a role: the Project agent and the Epic lead. Edits apply to the next session \
             that starts."
        );
        assert_eq!(page.new_agent_label, "New agent");
        let more: Vec<(&str, &str)> = page
            .more
            .iter()
            .map(|m| (m.id.as_str(), m.label.as_str()))
            .collect();
        assert_eq!(
            more,
            [("agents_file", "Apply the shipped agents file again")]
        );
        assert_eq!(page.default_model.label, "Default model for new agents");
        assert_eq!(
            page.default_model.description,
            "A new agent, or one a preset adds, starts with it."
        );
        assert_eq!(
            page.default_model.dropdown.value,
            "Claude Code \u{b7} haiku"
        );
        assert_eq!(
            page.fallbacks, None,
            "one tool: a fallback must be on another tool"
        );
        assert_eq!(page.columns, ["Name", "Model", "Skills", "Used in"]);
        type Shown<'a> = (
            &'a str,
            Option<&'a str>,
            &'a str,
            &'a str,
            Vec<String>,
            &'a str,
        );
        let rows: Vec<Shown> = page
            .rows
            .iter()
            .map(|r| {
                (
                    r.name.as_str(),
                    r.role.as_deref(),
                    r.came_from.as_str(),
                    r.model.as_str(),
                    r.skills.clone(),
                    r.used_in.as_str(),
                )
            })
            .collect();
        let s = |names: &[&str]| names.iter().map(|n| (*n).to_owned()).collect::<Vec<_>>();
        assert_eq!(
            rows,
            [
                (
                    "Planner",
                    Some("Project agent"),
                    &*format!("From {PRODUCT_NAME}\u{2019}s agents file"),
                    "Claude Code \u{b7} haiku",
                    s(&["noyan"]),
                    "every project \u{b7} the Project agent"
                ),
                (
                    "Epic lead",
                    Some("Epic lead"),
                    &*format!("From {PRODUCT_NAME}\u{2019}s agents file"),
                    "Claude Code \u{b7} opus",
                    s(&["epic-lead"]),
                    "every project \u{b7} epics"
                ),
                (
                    "Developer",
                    None,
                    &*format!("Added by the {PRODUCT_NAME} default preset"),
                    "Claude Code \u{b7} haiku",
                    s(&["work-ticket", "commit"]),
                    "pocket-api \u{b7} Dev, pocket-api \u{b7} Done, pocket-notes \u{b7} Dev, pocket-notes \u{b7} Done"
                ),
                (
                    "Validator",
                    None,
                    &*format!("Added by the {PRODUCT_NAME} default preset"),
                    "Claude Code \u{b7} opus",
                    s(&["team-validate"]),
                    "pocket-notes \u{b7} Validate"
                ),
                (
                    "Strong",
                    None,
                    "Added by you",
                    "Claude Code \u{b7} opus",
                    s(&["work-ticket", "commit"]),
                    "\u{2014}"
                ),
                (
                    "Light",
                    None,
                    "Added by you",
                    "Claude Code \u{b7} haiku",
                    s(&["work-ticket", "commit"]),
                    "\u{2014}"
                ),
            ]
        );
        let menu: Vec<&str> = page.rows[0].menu.iter().map(|m| m.label.as_str()).collect();
        assert_eq!(menu, ["Duplicate", "Delete\u{2026}"]);
    }

    /// AGM-03: the default model and one Fallback line per connected tool, each offering the other tools' lists and
    /// None.
    #[test]
    fn the_default_model_and_a_fallback_per_tool() {
        let (mut list, uses) = example();
        list.fallbacks = vec![otaq_core::agents::ToolFallback {
            tool: Tool::Claude,
            fallback: ModelRef {
                tool: Tool::Codex,
                model: Some("newest:gpt-sol".into()),
            },
        }];
        let tools = two_tools();
        let page = section(&Inputs {
            list: &list,
            tools: &tools,
            uses: &uses,
        });
        let fallbacks = page.fallbacks.expect("two tools");
        assert_eq!(
            (fallbacks.label.as_str(), fallbacks.description.as_str()),
            ("Fallback per tool", "Used when a tool hits its plan limit.")
        );
        let lines: Vec<(&str, &str)> = fallbacks
            .lines
            .iter()
            .map(|l| (l.label.as_str(), l.dropdown.value.as_str()))
            .collect();
        assert_eq!(
            lines,
            [
                (
                    "When Claude Code hits its limit \u{2192}",
                    "Codex \u{b7} newest GPT Sol"
                ),
                ("When Codex hits its limit \u{2192}", "None")
            ]
        );
        // Claude Code's line offers None, then Codex's list only.
        let choices = &fallbacks.lines[0].dropdown.choices;
        assert_eq!(choices[0].label, "None");
        assert!(choices.iter().all(|c| c.tool != Some(Tool::Claude)));
        let picked: Vec<&str> = choices
            .iter()
            .filter(|c| c.selected)
            .map(|c| c.label.as_str())
            .collect();
        assert_eq!(picked, ["GPT Sol \u{b7} always the newest"]);
        // The default model offers both tools' lists, under their names.
        let headings: Vec<&str> = page
            .default_model
            .dropdown
            .choices
            .iter()
            .filter(|c| c.kind == ChoiceKind::Heading)
            .map(|c| c.label.as_str())
            .collect();
        assert_eq!(headings, ["Claude Code", "Codex"]);
        // Before a tool connects there is nothing to pick.
        list.default_model = None;
        let none = AgentTools::default();
        let page = section(&Inputs {
            list: &list,
            tools: &none,
            uses: &uses,
        });
        assert!(page.default_model.dropdown.read_only);
        assert_eq!(page.default_model.dropdown.value, NO_DEFAULT);
    }
}
