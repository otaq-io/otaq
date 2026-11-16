//! Who does an agent's work (UX-65…UX-75; AGM-03, AGM-05, d0086, d0088, d0068, d0092): every session Otaq starts runs
//! on a model (a tool and a model from that tool's list, or `default`), with the tool's own effort level where the tool
//! has a documented one, and optional instructions. There are no named agents: a stage of a project's flow says who does
//! it (its `model:`, `effort:` and `instructions:`), so does the flow's delivery (UX-69), and Noyan starts on an app
//! setting, "Noyan starts on" (UX-70).
//!
//! - The types: [`Who`] (what a session resolves to, and [`About`], where a mark about its model goes),
//!   [`AgentModel`], [`ModelRef`] (a tool and a model: the default model), [`SessionRole`] (which kind of session a
//!   hold or a token count is about), [`NoyanStartsOn`].
//! - [`store`]: the holds, the settings and the sessions' facts; [`shipped`]: Otaq's own skills and the account's
//!   default model; [`resolve`]: which model a session runs (AGM-05); [`gate`]: the check before every launch: a
//!   session whose tool can't run waits for it; [`limits`]: the plan-limit banner's lines and Resume per tool;
//!   [`mark`]: the amber mark of a model the tool refused, on the stage or Noyan's setting; [`pause`]: Pause and Resume
//!   per project (d0112).

pub mod facts;
pub mod gate;
pub mod kept;
pub mod limits;
pub mod mark;
pub mod pause;
pub mod resolve;
pub mod shipped;
pub mod store;
pub mod upgrade;

use serde::{Deserialize, Serialize};

use crate::agent_tools::Tool;
use crate::agent_tools::models::{ModelKind, ModelLists};
use crate::domain::stored_enum;
use crate::session::{SessionAgent, StoredConversation};

stored_enum! {
    /// Which kind of session something is about: a hold (stored as its `role`), a token count, a running session.
    /// The Developer is an Agent stage's session (and its answers and PR), the Validator an agent check's round, the
    /// Project agent is a Noyan conversation.
    pub enum SessionRole ("session role") {
        Developer = "developer",
        Validator = "validator",
        ProjectAgent = "project_agent",
    }
}

impl SessionRole {
    /// "Developer", "Checker", "Noyan".
    pub fn label(self) -> &'static str {
        match self {
            Self::Developer => "Developer",
            Self::Validator => "Checker",
            Self::ProjectAgent => "Noyan",
        }
    }
}

stored_enum! {
    /// Why a session that would start is held (SCH-04 `[M23]`, §7.2; d0086, d0088).
    pub enum HoldReason ("hold reason") {
        /// Its tool is at its limit: only that tool's sessions wait.
        ToolLimit = "tool_limit",
        /// Its tool isn't available (not installed, logged out, hidden, update needed).
        ToolUnavailable = "tool_unavailable",
        /// Its model is no longer in its tool's list: "Model unavailable · Edit flow…".
        ModelUnavailable = "model_unavailable",
        /// Its tool doesn't have the skill the stage names (a stage runs only on a tool that has its skill): the
        /// hold keeps the skill's name, and Check again asks the finder again.
        MissingSkill = "missing_skill",
    }
}

/// A model as a flow says it (UX-65, UX-68): `default` (the account's default model, AGM-03: only in shipped and
/// older files, made a tool when a flow is applied), or a tool and a model from its list (`None`: the tool's own
/// default; a follow choice such as `opus`; an exact id). Which kind a model text is comes from the tool's list
/// ([`Who::model_kind`]); the text is passed to the tool unchanged.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AgentModel {
    Default,
    Tool {
        tool: Tool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        model: Option<String>,
    },
}

impl AgentModel {
    /// A flow file's `model:`: `default`, a tool's stored name (`claude`: its default model) or `<tool>:<model>`
    /// (`claude:opus`); why it doesn't read otherwise.
    pub fn parse(text: &str) -> Result<Self, String> {
        let text = text.trim();
        if text == "default" {
            return Ok(Self::Default);
        }
        let (tool, model) = match text.split_once(':') {
            Some((tool, model)) => (
                tool,
                Some(model.trim().to_owned()).filter(|m| !m.is_empty()),
            ),
            None => (text, None),
        };
        let tool = tool.trim().parse().map_err(|_| {
            let tools: Vec<&str> = Tool::ALL.iter().map(|t| t.as_str()).collect();
            format!(
                "{text} isn't a model: say a tool ({}) or `tool:model` (`claude:opus`)",
                tools.join(", ")
            )
        })?;
        Ok(Self::Tool { tool, model })
    }

    /// The model as a flow file writes it: `default`, `<tool>` or `<tool>:<model>`.
    pub fn text(&self) -> String {
        match self {
            Self::Default => "default".to_owned(),
            Self::Tool { tool, model: None } => tool.as_str().to_owned(),
            Self::Tool {
                tool,
                model: Some(model),
            } => format!("{}:{model}", tool.as_str()),
        }
    }

    /// Its own tool; `None` for `default`.
    pub fn tool(&self) -> Option<Tool> {
        match self {
            Self::Default => None,
            Self::Tool { tool, .. } => Some(*tool),
        }
    }

    /// The tool and model it runs on now: its own, or the default model (`None` while none is set).
    pub fn on(&self, default: Option<&ModelRef>) -> Option<ModelRef> {
        match self {
            Self::Default => default.cloned(),
            Self::Tool { tool, model } => Some(ModelRef {
                tool: *tool,
                model: model.clone(),
            }),
        }
    }
}

/// Why `level` can't go with `model` (the refusals of an effort, UX-75): it isn't one of its tool's levels, or the tool
/// has none. `None` when it can, and for `default`, whose tool is known only once the flow is applied.
pub fn effort_refusal(model: &AgentModel, level: &str) -> Option<String> {
    let tool = model.tool()?;
    if tool.effort_levels().contains(&level) {
        return None;
    }
    Some(match tool.effort_levels() {
        [] => format!("{} has no effort level", tool.display_name()),
        levels => format!(
            "{level} isn't one of {}'s effort levels: {}",
            tool.display_name(),
            levels.join(", ")
        ),
    })
}

/// A model text by kind, as the tool's list says (d0088). Always one picked from the tool's own list.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", content = "id", rename_all = "snake_case")]
pub enum ToolModel {
    /// No model option is passed; the tool uses its own default (shown "tool default").
    ToolDefault,
    /// A follow choice: a tool alias that moves to the newest model ("opus", shown "Opus · always the newest").
    Follow(String),
    /// An exact model id from the tool's list ("claude-opus-5-5").
    Exact(String),
}

impl ToolModel {
    /// What goes to the tool: `None` passes no model option.
    pub fn passed(&self) -> Option<&str> {
        match self {
            Self::ToolDefault => None,
            Self::Follow(id) | Self::Exact(id) => Some(id),
        }
    }
}

/// A tool and a model from its list (`None`: the tool's default): the account's default model, what a session runs
/// on.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ModelRef {
    pub tool: Tool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
}

impl ModelRef {
    /// As a flow file writes it: `<tool>` or `<tool>:<model>`.
    pub fn text(&self) -> String {
        AgentModel::Tool {
            tool: self.tool,
            model: self.model.clone(),
        }
        .text()
    }
}

/// The amber mark on a stage's (or the delivery's, or Noyan's) model (AGT-05): "Claude Code couldn't run
/// `claude-fable-5-1` on SHOP-48". No dismiss; a saved model change clears it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mark {
    /// The line as shown.
    pub text: String,
    /// The model that was refused.
    pub model: Option<String>,
    /// The ticket key it happened on.
    pub ticket: Option<String>,
    /// The tool's own message.
    pub message: Option<String>,
    pub at: i64,
}

/// What a session's model came from, and so where a mark about it goes ([`mark`]): a stage of a project's flow, the
/// project's delivery, or Noyan's setting. Stored on the session as [`Self::scope`].
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum About {
    /// The stage `stage` (its id) of the project `project` (its key).
    Stage { project: String, stage: String },
    /// The delivery of the project `project` (its key).
    Delivery { project: String },
    /// Noyan's "Noyan starts on", or a conversation's own pick.
    Noyan,
}

impl About {
    /// As stored: `stage/<project>/<stage id>`, `delivery/<project>`, `noyan`.
    pub fn scope(&self) -> String {
        match self {
            Self::Stage { project, stage } => format!("stage/{project}/{stage}"),
            Self::Delivery { project } => format!("delivery/{project}"),
            Self::Noyan => "noyan".to_owned(),
        }
    }

    /// The stored scope read back; `None` for one that isn't.
    pub fn parse(scope: &str) -> Option<Self> {
        if scope == "noyan" {
            return Some(Self::Noyan);
        }
        if let Some(project) = scope.strip_prefix("delivery/") {
            return Some(Self::Delivery {
                project: project.to_owned(),
            });
        }
        let (project, stage) = scope.strip_prefix("stage/")?.split_once('/')?;
        Some(Self::Stage {
            project: project.to_owned(),
            stage: stage.to_owned(),
        })
    }
}

/// Who does a session's work (UX-65, UX-67, UX-69, UX-70): the model, effort and instructions of the stage it runs
/// in, of the flow's delivery, or of Noyan's setting, and what they came from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Who {
    pub model: AgentModel,
    /// The tool's own effort level (`high`), passed only when the tool it runs on has that level
    /// ([`Tool::effort_levels`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effort: Option<String>,
    /// Markdown added to the skill (UX-67); empty when there are none.
    #[serde(default)]
    pub instructions: String,
    pub about: About,
}

impl Who {
    /// Its model by kind, from its tool's list; `None` for `default`. A text the list no longer has counts as exact,
    /// or as a follow choice in one of Otaq's forms (`newest:gpt-sol`, `auto@balance`;
    /// [`crate::agent_tools::follow`]).
    pub fn model_kind(&self, models: &dyn ModelCatalog) -> Option<ToolModel> {
        let AgentModel::Tool { tool, model } = &self.model else {
            return None;
        };
        Some(match model {
            None => ToolModel::ToolDefault,
            Some(text) => models.pick(*tool, text).unwrap_or_else(|| {
                match crate::agent_tools::follow::FollowChoice::parse(text) {
                    Some(_) => ToolModel::Follow(text.clone()),
                    None => ToolModel::Exact(text.clone()),
                }
            }),
        })
    }

    /// Who a flow's stage or delivery `who` says, about `about`; `None` while it names no model (or one that doesn't
    /// read, which reading the flow refuses).
    pub fn of(who: &crate::flow::model::WhoDoesIt, about: About) -> Option<Self> {
        let model = AgentModel::parse(who.model.as_deref()?).ok()?;
        Some(Self {
            model,
            effort: who.effort.clone(),
            instructions: who.instructions.clone().unwrap_or_default(),
            about,
        })
    }

    /// As a session stores it: what it came from, and its tool's name as shown ("Claude Code"), the name History
    /// and the cards show for it.
    pub fn session_agent(&self, tool: Tool) -> SessionAgent {
        SessionAgent {
            runs_for: self.about.scope(),
            name: tool.display_name().to_owned(),
        }
    }
}

/// "Noyan starts on" (UX-70, UX-74): the model and effort every new Noyan conversation starts on, unless the
/// conversation picks another before its first message. `default` (the account's default model) while none was set.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct NoyanStartsOn {
    /// `default`, a tool (`claude`) or `tool:model` (`claude:opus`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effort: Option<String>,
}

impl NoyanStartsOn {
    /// Who a Noyan conversation on it runs as: no instructions (Noyan's behaviour comes from its skills, UX-74).
    pub fn who(&self) -> Who {
        Who {
            model: self
                .model
                .as_deref()
                .and_then(|m| AgentModel::parse(m).ok())
                .unwrap_or(AgentModel::Default),
            effort: self.effort.clone(),
            instructions: String::new(),
            about: About::Noyan,
        }
    }
}

/// A session that would start but doesn't (an Agent stage's start, an agent check's round, a Noyan conversation's
/// first message), one per ticket and session role, or per project for its Noyan conversation (`agent_holds`). It
/// lifts when its cause goes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentHold {
    pub id: String,
    /// The held ticket; `None` for a Noyan conversation's first message.
    pub ticket_id: Option<String>,
    /// The project of a held Noyan conversation's first message; `None` for a ticket's hold.
    pub project_id: Option<String>,
    pub role: SessionRole,
    pub reason: HoldReason,
    /// The tool it would have started on.
    pub tool: Tool,
    /// The model it would have passed (`None`: the tool's default).
    pub model: Option<String>,
    /// The skill its tool doesn't have, for a [`HoldReason::MissingSkill`] hold; `None` for every other.
    pub skill: Option<String>,
    /// When it was first held for this reason, in Unix milliseconds.
    pub since: i64,
}

/// A tool's own model list, as the rules ask it (AGT-02). Only what the list contains can be picked (d0086). The
/// service answers it from its copies of the tools' lists ([`ModelLists`]); tests from a [`FixedModels`].
pub trait ModelCatalog: Send + Sync {
    /// What picking `text` from `tool`'s list means (Claude Code's `default` is [`ToolModel::ToolDefault`], its
    /// aliases [`ToolModel::Follow`]), or `None` when the list doesn't have it.
    fn pick(&self, tool: Tool, text: &str) -> Option<ToolModel>;
}

impl ModelCatalog for ModelLists {
    /// From the copy of `tool`'s list: nothing can be picked before it was first read.
    fn pick(&self, tool: Tool, text: &str) -> Option<ToolModel> {
        let copy = self.copy(tool)?;
        let entry = copy.list.entry(text)?;
        Some(match entry.kind {
            ModelKind::ToolDefault => ToolModel::ToolDefault,
            ModelKind::Follow => ToolModel::Follow(entry.id.clone()),
            ModelKind::Exact => ToolModel::Exact(entry.id.clone()),
        })
    }
}

/// A fixed list, the same for every tool, for tests. [`Self::sample`] is Claude Code's list as its `initialize`
/// request gives it: `default` (tool default) and the aliases `opus`, `sonnet`, `haiku`, `fable`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FixedModels {
    entries: Vec<ToolModel>,
}

impl FixedModels {
    pub fn new(entries: Vec<ToolModel>) -> Self {
        Self { entries }
    }

    pub fn sample() -> Self {
        let mut entries = vec![ToolModel::ToolDefault];
        entries.extend(
            ["opus", "sonnet", "haiku", "fable"].map(|alias| ToolModel::Follow(alias.to_owned())),
        );
        Self { entries }
    }
}

impl ModelCatalog for FixedModels {
    fn pick(&self, _tool: Tool, text: &str) -> Option<ToolModel> {
        self.entries
            .iter()
            .find(|entry| match entry {
                ToolModel::ToolDefault => text == "default",
                ToolModel::Follow(id) | ToolModel::Exact(id) => id == text,
            })
            .cloned()
    }
}

/// What a session launches on (AGM-05, ADP-04; d0088 "The exact tool, model and agent are saved on the session"):
/// the tool (`None`: the new-session tool, while no default model is set), the model passed to it unchanged (`None`
/// passes no model option), the effort, and what the session runs for, saved on it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RunsOn {
    pub tool: Option<Tool>,
    pub model: Option<String>,
    pub effort: Option<String>,
    pub agent: Option<SessionAgent>,
}

impl RunsOn {
    /// A new session of `who` as it is now, without the check before a launch: its own model, or `default` (the
    /// new-session tool with its default model while none is set). The effort goes only where its tool has it.
    pub fn who(who: &Who, default: Option<&ModelRef>) -> Self {
        let on = who.model.on(default);
        let tool = on.as_ref().map(|m| m.tool);
        Self {
            tool,
            model: on.and_then(|m| m.model),
            effort: tool.and_then(|tool| effort_on(who, tool)),
            agent: tool.map(|tool| who.session_agent(tool)),
        }
    }

    /// A resumed conversation: the tool, model, effort and what it runs for its session stored, whatever the stage
    /// says now (the models report: Claude Code doesn't keep the model, so it is passed again).
    pub fn resumed(conversation: &StoredConversation) -> Self {
        Self {
            tool: Some(conversation.tool),
            model: conversation.model.clone(),
            effort: conversation.effort.clone(),
            agent: conversation.agent.clone(),
        }
    }
}

/// The effort `who` passes on `tool`: its own, when the tool has that level.
pub fn effort_on(who: &Who, tool: Tool) -> Option<String> {
    who.effort
        .clone()
        .filter(|level| tool.effort_levels().contains(&level.as_str()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_roles_are_stored_as_named() {
        let sessions: Vec<&str> = SessionRole::ALL.iter().map(|r| r.as_str()).collect();
        assert_eq!(sessions, ["developer", "validator", "project_agent"]);
    }

    #[test]
    fn a_default_model_resolves_to_the_default() {
        let tool = Tool::ALL[0];
        let default = ModelRef {
            tool,
            model: Some("haiku".into()),
        };
        assert_eq!(
            AgentModel::Default.on(Some(&default)),
            Some(default.clone())
        );
        assert_eq!(AgentModel::Default.on(None), None);
        let own = AgentModel::Tool {
            tool,
            model: Some("opus".into()),
        };
        assert_eq!(
            own.on(Some(&default)).unwrap().model.as_deref(),
            Some("opus")
        );
        assert_eq!(ToolModel::ToolDefault.passed(), None);
    }

    #[test]
    fn models_read_as_default_a_tool_or_tool_and_model_and_write_back() {
        assert_eq!(AgentModel::parse("default"), Ok(AgentModel::Default));
        assert_eq!(
            AgentModel::parse("claude"),
            Ok(AgentModel::Tool {
                tool: Tool::Claude,
                model: None
            })
        );
        let opencode = AgentModel::parse("opencode:anthropic/claude-sonnet-5-5#high").unwrap();
        assert_eq!(
            opencode,
            AgentModel::Tool {
                tool: Tool::OpenCode,
                model: Some("anthropic/claude-sonnet-5-5#high".into())
            }
        );
        assert_eq!(opencode.text(), "opencode:anthropic/claude-sonnet-5-5#high");
        assert!(AgentModel::parse("gpt").is_err());
        for text in ["default", "claude", "claude:opus"] {
            assert_eq!(AgentModel::parse(text).unwrap().text(), text);
        }
    }

    #[test]
    fn an_effort_is_one_of_its_tools_levels() {
        let claude = AgentModel::parse("claude:opus").unwrap();
        let level = Tool::Claude.effort_levels()[0];
        assert_eq!(effort_refusal(&claude, level), None);
        assert!(
            effort_refusal(&claude, "ludicrous")
                .unwrap()
                .starts_with("ludicrous isn't one of Claude Code's effort levels: ")
        );
        assert_eq!(effort_refusal(&AgentModel::Default, "ludicrous"), None);
    }

    #[test]
    fn what_a_session_runs_for_reads_back() {
        for about in [
            About::Stage {
                project: "SHOP".into(),
                stage: "dev".into(),
            },
            About::Delivery {
                project: "SHOP".into(),
            },
            About::Noyan,
        ] {
            assert_eq!(About::parse(&about.scope()), Some(about));
        }
        assert_eq!(About::parse("agent-1"), None);
    }

    #[test]
    fn the_fixed_list_knows_the_tool_default_and_the_aliases() {
        let models = FixedModels::sample();
        let tool = Tool::ALL[0];
        assert_eq!(models.pick(tool, "default"), Some(ToolModel::ToolDefault));
        assert_eq!(
            models.pick(tool, "opus"),
            Some(ToolModel::Follow("opus".into()))
        );
        assert_eq!(models.pick(tool, "claude-opus-5-5"), None);
    }
}
