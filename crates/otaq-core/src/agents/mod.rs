//! Agents (AGM-01…AGM-05; d0108, d0086, d0088, d0040, d0068, d0092): every session Otaq starts runs an agent, which
//! has a name, its own instructions, an ordered set of skills and its own model (a tool and a model from that tool's
//! list, or `default`), with the tool's own effort level where the tool has a documented one. Agents belong to the
//! account and are shared by every project; flows name them by name and Otaq keeps their ids, so a rename changes
//! nothing.
//!
//! - The types: [`Agent`], [`AgentModel`], [`SkillRef`], [`Role`] (the two roles an agent can hold), [`CameFrom`],
//!   [`ModelRef`] (a tool and a model: the default model, a tool's fallback) and [`SessionRole`] (which kind of
//!   session a hold or a token count is about).
//! - [`Agents`]: the requests' rules (add, edit, duplicate, delete, in use, move a role, the default model, the
//!   fallbacks, importing a skill).
//! - [`store`]: the tables; [`shipped`]: Otaq's agents file and the default flow's agents, applied at the first
//!   start; [`in_use`]: what uses an agent; [`resolve`]: which agent a session runs (AGM-05);
//!   [`gate`]: the check before every launch and the fallback (AGM-03); [`limits`]: the plan-limit banner's lines,
//!   Resume per tool and Use fallback now; [`mark`]: the amber mark of a model the tool refused; [`skills`]:
//!   importing a skill folder onto an agent (WSK-02…WSK-04); [`pause`]: Pause and Resume per project (d0112).

pub mod facts;
pub mod gate;
pub mod in_use;
pub mod kept;
pub mod limits;
pub mod mark;
pub mod pause;
pub mod resolve;
pub mod shipped;
pub mod skills;
pub mod store;

use std::collections::BTreeMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::agent_tools::Tool;
use crate::agent_tools::models::{ModelKind, ModelLists};
use crate::board::{Board, BoardError, Refusal};
use crate::domain::stored_enum;
use crate::session::{SessionAgent, StoredConversation};
pub use in_use::InUse;

stored_enum! {
    /// The role an agent can hold (AGM-01, d0108, d0133): Otaq starts "the agent with that role" for a project's
    /// Project agent, so a rename changes nothing. Exactly one agent holds it.
    pub enum Role ("agent role") {
        /// The Project agent (d0068, d0091, d0128): onboarding, Split, proposals.
        ProjectAgent = "noyan",
    }
}

impl Role {
    /// The role as people read it: "Noyan".
    pub fn label(self) -> &'static str {
        match self {
            Self::ProjectAgent => "Noyan",
        }
    }

    /// AGM-01's refusal of deleting the agent holding the role, or removing its role: "Noyan has the Noyan role.
    /// Give the role to another agent first".
    pub fn holder_refusal(self, name: &str) -> String {
        format!(
            "{name} has the {} role. Give the role to another agent first",
            self.label()
        )
    }
}

stored_enum! {
    /// Which kind of session something is about: a hold (stored as its `role`), a token count, a running session.
    /// The Developer is an Agent stage's session (and its answers and PR), the Validator an agent check's round, the
    /// Project agent runs its role's agent.
    pub enum SessionRole ("session role") {
        Developer = "developer",
        Validator = "validator",
        ProjectAgent = "project_agent",
    }
}

impl SessionRole {
    /// "Developer", "Validator", "Noyan".
    pub fn label(self) -> &'static str {
        match self {
            Self::Developer => "Developer",
            Self::Validator => "Validator",
            Self::ProjectAgent => "Noyan",
        }
    }

    /// The role whose agent this session runs: the Project agent's.
    pub fn role(self) -> Option<Role> {
        match self {
            Self::ProjectAgent => Some(Role::ProjectAgent),
            Self::Developer | Self::Validator => None,
        }
    }
}

stored_enum! {
    /// Why an agent that would start is held (SCH-04 `[M23]`, §7.2; d0086, d0088).
    pub enum HoldReason ("hold reason") {
        /// Its tool is at its limit: only that tool's agents wait.
        ToolLimit = "tool_limit",
        /// Its tool isn't available (not installed, logged out, hidden, update needed).
        ToolUnavailable = "tool_unavailable",
        /// Its model is no longer in its tool's list: "Model unavailable · Open agent…".
        ModelUnavailable = "model_unavailable",
    }
}

/// One of an agent's skills (AGM-01): a shipped skill by its name (`work-ticket`), or an imported folder by the id
/// of Otaq's copy (`skills/<id>/`, WSK-03), whose record is in [`Agent::imported`].
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SkillRef {
    Shipped(String),
    Imported(String),
}

/// What an agent remembers of an imported skill (WSK-03).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImportedSkill {
    /// Otaq's own id: the copy is `skills/<id>/`.
    pub id: String,
    /// The `name:` of its SKILL.md, the skill's label.
    pub name: String,
    /// The folder it was imported from (absolute).
    pub original: String,
    /// Unix milliseconds.
    pub imported_at: i64,
    /// A hash of the copy as imported ([`crate::skills::folder::fingerprint`]): the copy was edited when it
    /// differs.
    pub fingerprint: String,
    /// What the last import warned about (WSK-02: files the SKILL.md mentions that aren't in the folder).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
}

/// An agent's model (AGM-01): `default` (the account's default model, resolved when a session starts, AGM-03), or a
/// tool and a model from its list (`None`: the tool's own default; a follow choice such as `opus`; an exact id). Which
/// kind a model text is comes from the tool's list ([`Agent::model_kind`]); the text is passed to the tool unchanged.
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

/// A tool and a model from its list (`None`: the tool's default): the default model for new agents, a tool's
/// fallback (AGM-03).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ModelRef {
    pub tool: Tool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
}

/// Where an agent came from (AGM-01): "From Otaq's agents file", "Added by the Otaq default preset", "Added by
/// shop-web's flow", or the user's own.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "from", rename_all = "snake_case")]
pub enum CameFrom {
    /// Otaq's shipped `otaq-agents.yaml` (the Project agent).
    AgentsFile,
    /// The default flow's own `agents:` (Developer, Validator).
    DefaultPreset,
    /// Made by the user (New agent, Duplicate).
    You,
    /// A project's flow defined it.
    Flow { project: String },
}

impl CameFrom {
    /// The line under the agent's name.
    pub fn label(&self) -> String {
        match self {
            Self::AgentsFile => format!("From {}'s agents file", crate::naming::PRODUCT_NAME),
            Self::DefaultPreset => {
                format!(
                    "Added by the {} default preset",
                    crate::naming::PRODUCT_NAME
                )
            }
            Self::You => "Added by you".to_owned(),
            Self::Flow { project } => format!("Added by {project}'s flow"),
        }
    }
}

/// The amber mark on an agent (AGT-05): "Claude Code couldn't run `claude-fable-5-1` on SHOP-48". No dismiss; a
/// saved model change clears it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mark {
    /// The line as shown.
    pub text: String,
    /// The model that was refused.
    pub model: Option<String>,
    /// The ticket key it happened on.
    pub ticket: Option<String>,
    /// The tool's own message, shown in quotes on the agent's page.
    pub message: Option<String>,
    pub at: i64,
}

/// One agent (AGM-01).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Agent {
    /// Permanent: sessions, holds and the flows' names point at it.
    pub id: String,
    /// Unique ignoring case ("Developer", "Rust developer").
    pub name: String,
    /// Markdown; empty when it has none.
    pub instructions: String,
    /// In order.
    pub skills: Vec<SkillRef>,
    /// The records of its [`SkillRef::Imported`] skills.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub imported: Vec<ImportedSkill>,
    pub model: AgentModel,
    /// The tool's own effort level (`high`), only with a model on a tool that has a documented one
    /// ([`Tool::effort_levels`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effort: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<Role>,
    pub came_from: CameFrom,
    pub mark: Option<Mark>,
    pub created_at: i64,
    pub updated_at: i64,
}

impl Agent {
    /// The record of one of its imported skills.
    pub fn imported_skill(&self, id: &str) -> Option<&ImportedSkill> {
        self.imported.iter().find(|s| s.id == id)
    }

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

    /// The agent as a session stores it.
    pub fn session_agent(&self) -> SessionAgent {
        SessionAgent {
            id: self.id.clone(),
            name: self.name.clone(),
        }
    }
}

/// An agent that would start but doesn't (an Agent stage's start, an agent check's round, a Project agent's first
/// message), one per ticket and session role (and check agent, d0130 row 17), or per project for its
/// Project agent (`agent_holds`). It lifts when its cause goes, or on the user's Use fallback.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentHold {
    pub id: String,
    /// The held ticket; `None` for a Project agent's first message.
    pub ticket_id: Option<String>,
    /// The project of a held Project agent's first message; `None` for a ticket's hold.
    pub project_id: Option<String>,
    pub role: SessionRole,
    /// The held check agent of an agent check's round, as its row names it ("Security reviewer"): the others of the
    /// round run on (d0130 row 17). `None` for every other hold.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub check_agent: Option<String>,
    pub reason: HoldReason,
    /// The tool it would have started on.
    pub tool: Tool,
    /// The model it would have passed (`None`: the tool's default).
    pub model: Option<String>,
    /// The agent it would have started on.
    pub agent_id: Option<String>,
    /// The tool whose fallback could take it now ([`Fallbacks`]); `None` without one, or while that can't run either.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fallback_tool: Option<Tool>,
    /// The tool's fallback can't run either ("Claude Code and its fallback Codex are both at their limits").
    pub fallback_held: bool,
    /// The user asked for the fallback (Use fallback, Use fallback now): the held start launches there.
    #[serde(default)]
    pub fallback_asked: bool,
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

/// An agent's fields as its page sends them (`agents.add`, `agents.edit`) and as Duplicate's draft ("<name> copy",
/// not saved). A model text comes from the tool's list (`"default"` or empty = the tool's default). The role moves
/// with its own request (`agents.move_role`); imported skills come with `agents.import_skill`, and the list here
/// may keep, reorder or drop them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentFields {
    pub name: String,
    #[serde(default)]
    pub instructions: String,
    #[serde(default)]
    pub skills: Vec<SkillRef>,
    pub model: AgentModel,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effort: Option<String>,
}

/// Each tool's fallback (AGM-03): "When Claude Code hits its limit → Codex · newest GPT". Stored as one JSON object
/// in `settings` (`agents.fallback`), by tool.
pub type Fallbacks = BTreeMap<Tool, ModelRef>;

/// One line of Fallback per tool.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolFallback {
    pub tool: Tool,
    pub fallback: ModelRef,
}

/// The answer to `agents.list`: every agent (oldest first), the default model for new agents (`None` until a tool
/// connects) and each tool's fallback.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentList {
    pub agents: Vec<Agent>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_model: Option<ModelRef>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fallbacks: Vec<ToolFallback>,
}

/// A session that runs on its tool's fallback, by the user's click (AGM-03): the tool it would have run on and why
/// it couldn't ("on Codex · Claude Code limit").
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FallbackFrom {
    pub tool: Tool,
    pub reason: HoldReason,
}

/// What a session launches on (AGM-05, ADP-04; d0088 "The exact tool, model and agent are saved on the session"):
/// the tool (`None`: the new-session tool, while no default model is set), the model passed to it unchanged (`None`
/// passes no model option), the effort, the agent saved on the session, and the fallback it runs on.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RunsOn {
    pub tool: Option<Tool>,
    pub model: Option<String>,
    pub effort: Option<String>,
    pub agent: Option<SessionAgent>,
    pub fallback: Option<FallbackFrom>,
}

impl RunsOn {
    /// A new session on `agent` as it is now, without the check before a launch: its own model, or `default` (the
    /// new-session tool with its default model while none is set). The effort goes only with its own model.
    pub fn agent(agent: &Agent, default: Option<&ModelRef>) -> Self {
        let on = agent.model.on(default);
        Self {
            tool: on.as_ref().map(|m| m.tool),
            model: on.and_then(|m| m.model),
            effort: agent.model.tool().and(agent.effort.clone()),
            agent: Some(agent.session_agent()),
            fallback: None,
        }
    }

    /// A resumed conversation: the tool, model, effort and agent its session stored, whatever the agent is set to now
    /// (the models report: Claude Code doesn't keep the model, so it is passed again).
    pub fn resumed(conversation: &StoredConversation) -> Self {
        Self {
            tool: Some(conversation.tool),
            model: conversation.model.clone(),
            effort: conversation.effort.clone(),
            agent: conversation.agent.clone(),
            fallback: None,
        }
    }
}

/// The name rules (AGM-01, Q256): trimmed, not empty, not `skip` (a stage option's word) or starting with `_` (kept
/// for Otaq), and new ignoring case among `others`. The name as stored.
pub fn checked_name<'a>(
    name: &str,
    mut others: impl Iterator<Item = &'a str>,
) -> Result<String, BoardError> {
    let name = name.trim();
    if name.is_empty() {
        return Err(BoardError::refused(
            Refusal::EmptyField,
            "An agent needs a name",
        ));
    }
    if name.eq_ignore_ascii_case(crate::flow::StageOption::SKIP) {
        return Err(BoardError::refused(
            Refusal::BadName,
            format!("{name} is how a flow says a stage can be skipped: pick another name"),
        ));
    }
    if name.starts_with('_') {
        return Err(BoardError::refused(
            Refusal::BadName,
            format!(
                "Names starting with _ are kept for {}: pick another name",
                crate::naming::PRODUCT_NAME
            ),
        ));
    }
    let folded = name.to_lowercase();
    if others.any(|other| other.to_lowercase() == folded) {
        return Err(BoardError::refused(
            Refusal::NameTaken,
            format!("Another agent is already called {name}"),
        ));
    }
    Ok(name.to_owned())
}

/// The agents' rules, for the service's requests. Cheap to clone.
#[derive(Clone)]
pub struct Agents {
    board: Board,
    models: Arc<dyn ModelCatalog>,
}

impl Agents {
    pub fn new(board: Board, models: Arc<dyn ModelCatalog>) -> Self {
        Self { board, models }
    }

    /// The model list the rules check against.
    pub fn models(&self) -> &dyn ModelCatalog {
        self.models.as_ref()
    }

    pub async fn list(&self) -> Result<AgentList, BoardError> {
        let store = self.board.store();
        Ok(AgentList {
            agents: store.agents().await?,
            default_model: store.default_model().await?,
            fallbacks: store
                .fallbacks()
                .await?
                .into_iter()
                .map(|(tool, fallback)| ToolFallback { tool, fallback })
                .collect(),
        })
    }

    pub async fn agent(&self, id: &str) -> Result<Agent, BoardError> {
        self.board
            .store()
            .agent(id)
            .await?
            .ok_or_else(|| BoardError::NotFound(format!("no agent {id}")))
    }

    /// New agent (AGM-01): "Added by you".
    pub async fn add(&self, fields: &AgentFields) -> Result<Agent, BoardError> {
        let new = self.checked(fields, None).await?;
        let id = self
            .board
            .store()
            .insert_agent(&new, &CameFrom::You, None)
            .await?;
        self.agent(&id).await
    }

    /// Changes an agent (name, instructions, skills, model, effort). A saved model change clears the mark (AGT-05).
    /// The next session that starts uses it; running and parked sessions keep what they started with (0088).
    pub async fn edit(&self, id: &str, fields: &AgentFields) -> Result<Agent, BoardError> {
        let before = self.agent(id).await?;
        let new = self.checked(fields, Some(&before)).await?;
        let model_changed = before.model != new.model;
        self.board
            .store()
            .update_agent(id, &new, model_changed)
            .await?;
        self.agent(id).await
    }

    /// Duplicate's draft: the agent's fields named "<name> copy" (imported skills share Otaq's copy). Not saved; the
    /// page saves it with [`Self::add`].
    pub async fn duplicate(&self, id: &str) -> Result<AgentFields, BoardError> {
        let agent = self.agent(id).await?;
        Ok(AgentFields {
            name: format!("{} copy", agent.name),
            instructions: agent.instructions,
            skills: agent.skills,
            model: agent.model,
            effort: agent.effort,
        })
    }

    /// Deletes an agent nothing uses (AGM-01): refused for a role's agent ("Noyan has the Noyan role. Give the role
    /// to another agent first") and while a stage or a stage option names it ("Spec writer is used in shop-web ·
    /// Product spec. Change that stage first.").
    pub async fn delete(&self, id: &str) -> Result<(), BoardError> {
        let uses = self.in_use(id).await?;
        if let Some(message) = uses.refusal() {
            return Err(BoardError::refused(Refusal::AgentInUse, message));
        }
        self.board.store().delete_agent(id).await?;
        Ok(())
    }

    /// What uses an agent, before Delete asks: empty = the plain confirmation.
    pub async fn in_use(&self, id: &str) -> Result<InUse, BoardError> {
        let agent = self.agent(id).await?;
        in_use::in_use(&self.board, &agent).await
    }

    /// Gives `role` to the agent `to` (AGM-01: "the role can be moved to another agent"), in one step: the agent
    /// that held it loses it. Refused when `to` holds another role, which would be left without its agent.
    pub async fn move_role(&self, role: Role, to: &str) -> Result<Agent, BoardError> {
        let agent = self.agent(to).await?;
        if let Some(held) = agent.role.filter(|held| *held != role) {
            return Err(BoardError::refused(
                Refusal::AgentInUse,
                held.holder_refusal(&agent.name),
            ));
        }
        self.board.store().move_role(role, to).await?;
        self.agent(to).await
    }

    /// Default model for new agents (AGM-03): a tool and a model from its list.
    pub async fn set_default_model(&self, model: &ModelRef) -> Result<ModelRef, BoardError> {
        let model = self.checked_ref(model)?;
        self.board.store().set_default_model(&model).await?;
        Ok(model)
    }

    /// One line of Fallback per tool (AGM-03): another tool and a model from its list, or none.
    pub async fn set_fallback(
        &self,
        tool: Tool,
        fallback: Option<&ModelRef>,
    ) -> Result<Fallbacks, BoardError> {
        let store = self.board.store();
        let mut fallbacks = store.fallbacks().await?;
        match fallback {
            Some(fallback) if fallback.tool == tool => {
                return Err(BoardError::refused(
                    Refusal::BadFallback,
                    "A fallback must be on another tool",
                ));
            }
            Some(fallback) => {
                fallbacks.insert(tool, self.checked_ref(fallback)?);
            }
            None => {
                fallbacks.remove(&tool);
            }
        }
        store.set_fallbacks(&fallbacks).await?;
        Ok(fallbacks)
    }

    /// The fields as stored, after the rules: the name ([`checked_name`]), a model from its tool's list, an effort
    /// only where the tool has one (one of its levels), shipped skills Otaq ships and imported ones the agent has.
    async fn checked(
        &self,
        fields: &AgentFields,
        editing: Option<&Agent>,
    ) -> Result<store::NewAgent, BoardError> {
        let agents = self.board.store().agents().await?;
        let name = checked_name(
            &fields.name,
            agents
                .iter()
                .filter(|a| Some(a.id.as_str()) != editing.map(|e| e.id.as_str()))
                .map(|a| a.name.as_str()),
        )?;
        let model = match &fields.model {
            AgentModel::Default => AgentModel::Default,
            AgentModel::Tool { tool, model } => {
                let checked = self.checked_ref(&ModelRef {
                    tool: *tool,
                    model: model.clone(),
                })?;
                AgentModel::Tool {
                    tool: checked.tool,
                    model: checked.model,
                }
            }
        };
        let effort = match (fields.effort.as_deref().map(str::trim), model.tool()) {
            (None | Some(""), _) => None,
            (Some(level), Some(tool)) if tool.effort_levels().contains(&level) => {
                Some(level.to_owned())
            }
            (Some(level), Some(tool)) => {
                return Err(BoardError::refused(
                    Refusal::BadEffort,
                    match tool.effort_levels() {
                        [] => format!("{} has no effort level", tool.display_name()),
                        levels => format!(
                            "{level} isn't one of {}'s effort levels: {}",
                            tool.display_name(),
                            levels.join(", ")
                        ),
                    },
                ));
            }
            (Some(_), None) => {
                return Err(BoardError::refused(
                    Refusal::BadEffort,
                    "An effort level goes with a tool's model, not with default",
                ));
            }
        };
        // Every agent's imported records: Duplicate's draft keeps the copies of the agent it came from.
        let known: Vec<&ImportedSkill> = agents.iter().flat_map(|a| &a.imported).collect();
        let mut skills = Vec::new();
        let mut imported = Vec::new();
        for skill in &fields.skills {
            match skill {
                SkillRef::Shipped(name) if !shipped::SKILLS.contains(&name.as_str()) => {
                    return Err(BoardError::refused(
                        Refusal::BadSkill,
                        format!(
                            "{} doesn't ship a skill called {name}",
                            crate::naming::PRODUCT_NAME
                        ),
                    ));
                }
                SkillRef::Shipped(_) => {}
                SkillRef::Imported(id) => {
                    let record = known.iter().find(|s| &s.id == id).ok_or_else(|| {
                        BoardError::refused(
                            Refusal::BadSkill,
                            "That imported skill isn't there any more",
                        )
                    })?;
                    imported.push((*record).clone());
                }
            }
            if !skills.contains(skill) {
                skills.push(skill.clone());
            }
        }
        Ok(store::NewAgent {
            name,
            instructions: fields.instructions.clone(),
            skills,
            imported,
            model,
            effort,
            role: editing.and_then(|a| a.role),
        })
    }

    /// A tool and a model text as stored: the text from the tool's list (`"default"` or empty = the tool's default).
    fn checked_ref(&self, model: &ModelRef) -> Result<ModelRef, BoardError> {
        let picked = match model.model.as_deref().map(str::trim) {
            None | Some("") => ToolModel::ToolDefault,
            Some(text) => self.models.pick(model.tool, text).ok_or_else(|| {
                BoardError::refused(
                    Refusal::ModelNotListed,
                    format!("{text} isn't in the tool's model list"),
                )
            })?,
        };
        Ok(ModelRef {
            tool: model.tool,
            model: picked.passed().map(str::to_owned),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn refusal(result: Result<String, BoardError>) -> (Refusal, String) {
        match result {
            Err(BoardError::Refused { code, message }) => (code, message),
            other => panic!("refused: {other:?}"),
        }
    }

    #[test]
    fn names_are_trimmed_new_ignoring_case_and_not_skip_or_underscore() {
        let others = ["Developer", "Ünal"];
        assert_eq!(
            checked_name("  Rust developer ", others.iter().copied()).unwrap(),
            "Rust developer"
        );
        assert_eq!(
            refusal(checked_name("developer", others.iter().copied())),
            (
                Refusal::NameTaken,
                "Another agent is already called developer".to_owned()
            )
        );
        assert_eq!(
            refusal(checked_name("ünal", others.iter().copied())).0,
            Refusal::NameTaken
        );
        assert_eq!(
            refusal(checked_name("Skip", others.iter().copied())).0,
            Refusal::BadName
        );
        assert_eq!(
            refusal(checked_name("_x", others.iter().copied())),
            (
                Refusal::BadName,
                format!(
                    "Names starting with _ are kept for {}: pick another name",
                    crate::naming::PRODUCT_NAME
                )
            )
        );
        assert_eq!(
            refusal(checked_name(" ", others.iter().copied())).0,
            Refusal::EmptyField
        );
    }

    #[test]
    fn roles_are_stored_as_the_files_name_them() {
        let stored: Vec<&str> = Role::ALL.iter().map(|r| r.as_str()).collect();
        assert_eq!(stored, ["noyan"]);
        assert_eq!(
            Role::ProjectAgent.holder_refusal("Noyan"),
            "Noyan has the Noyan role. Give the role to another agent first"
        );
        let sessions: Vec<&str> = SessionRole::ALL.iter().map(|r| r.as_str()).collect();
        assert_eq!(sessions, ["developer", "validator", "project_agent"]);
    }

    #[test]
    fn a_default_model_resolves_to_the_default_and_carries_no_effort() {
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
        assert_eq!(
            CameFrom::Flow {
                project: "shop-web".into()
            }
            .label(),
            "Added by shop-web's flow"
        );
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
