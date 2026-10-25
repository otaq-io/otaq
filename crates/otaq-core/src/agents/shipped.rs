//! What Otaq ships about agents (AGM-02, d0108): the account-level `otaq-agents.yaml` (the Project agent **Noyan**),
//! embedded at build time like the default flow, and the skills a shipped skill may name.
//! A flow file's agents (the default flow's Developer and Validator, the test flow's) become stored agents through
//! [`new_agent`].
//!
//! A file's `model:` is `default` (the account's default model; every shipped file says it), a tool's stored name
//! (`claude`: its default model) or `<tool>:<model>` (`claude:opus`).
//!
//! The first start ([`apply_on_first_start`]) applies the agents file and the default flow's agents, and nothing
//! else (FLW-01: nothing from M32 or earlier is moved over).

use std::sync::LazyLock;

use super::store::NewAgent;
use super::{AgentModel, CameFrom, ModelRef, Role, SkillRef};
use crate::agent_tools::Tool;
use crate::flow::FlowAgent;
use crate::flow::yaml;
use crate::store::{Store, StoreError};

pub use crate::naming::{AGENTS_FILE, AGENTS_YAML};

/// The skills Otaq ships (`plugin/skills/<name>/`): the names an agent's shipped skill can take.
pub const SKILLS: &[&str] = &[
    "address-review",
    "answer-question",
    "commit",
    "flow",
    "grill",
    "noyan",
    "open-pr",
    "push",
    "restack",
    "validate",
    "work-ticket",
];

static AGENTS: LazyLock<Vec<FlowAgent>> = LazyLock::new(|| {
    yaml::read_agents(AGENTS_YAML).expect("the shipped agents file reads (tested)")
});

/// The shipped agents file's agents, as read.
pub fn agents_file() -> &'static [FlowAgent] {
    &AGENTS
}

/// The model a file's `model:` says, or why it can't be read.
pub fn model_of(text: &str) -> Result<AgentModel, String> {
    let text = text.trim();
    if text == "default" {
        return Ok(AgentModel::Default);
    }
    let (tool, model) = match text.split_once(':') {
        Some((tool, model)) => (
            tool,
            Some(model.trim().to_owned()).filter(|m| !m.is_empty()),
        ),
        None => (text, None),
    };
    let tool = tool
        .trim()
        .parse()
        .map_err(|_| format!("{text} isn't a model: say `default`, a tool, or `tool:model`"))?;
    Ok(AgentModel::Tool { tool, model })
}

/// A file's agent as stored: its shipped skills by name, its role when it names one, and its model (`default` when
/// the file's doesn't read).
pub fn new_agent(agent: &FlowAgent) -> NewAgent {
    NewAgent {
        name: agent.name.trim().to_owned(),
        instructions: agent.instructions.clone(),
        skills: agent
            .skills
            .iter()
            .map(|s| SkillRef::Shipped(s.clone()))
            .collect(),
        imported: Vec::new(),
        model: model_of(&agent.model).unwrap_or(AgentModel::Default),
        effort: None,
        role: agent.role.as_deref().and_then(|r| r.parse::<Role>().ok()),
    }
}

/// Otaq's first start (AGM-02, FLW-01): with no agents stored yet, applies `otaq-agents.yaml` (Noyan)
/// and the default flow's `agents:` (Developer, Validator), in one transaction. A role agent can't be deleted, so
/// the table is never empty again and a later start changes nothing. Whether it applied them.
pub async fn apply_on_first_start(store: &Store) -> Result<bool, StoreError> {
    let mut tx = store.begin_write().await?;
    let any: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM agents)")
        .fetch_one(&mut *tx)
        .await?;
    if any {
        return Ok(false);
    }
    for agent in agents_file() {
        Store::insert_agent_in(&mut tx, &new_agent(agent), &CameFrom::AgentsFile, None).await?;
    }
    for agent in &crate::flow::default_file().agents {
        Store::insert_agent_in(&mut tx, &new_agent(agent), &CameFrom::DefaultPreset, None).await?;
    }
    tx.commit().await?;
    tracing::info!("the shipped agents applied (AGM-02)");
    Ok(true)
}

/// Sets the default model for new agents to the first connected tool with its default model when none is set (a
/// fresh install: AGM-02's "else the first starter entry's tool with no model"). `connected` in
/// [`Tool::ALL`]'s order. Whether it set one.
pub async fn ensure_default_model(store: &Store, connected: &[Tool]) -> Result<bool, StoreError> {
    let Some(&tool) = connected.first() else {
        return Ok(false);
    };
    if store.default_model().await?.is_some() {
        return Ok(false);
    }
    store
        .set_default_model(&ModelRef { tool, model: None })
        .await?;
    tracing::info!(%tool, "the default model for new agents is the first connected tool's");
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_agents_file_reads_and_writes_back() {
        let agents = agents_file();
        let names: Vec<&str> = agents.iter().map(|a| a.name.as_str()).collect();
        assert_eq!(names, ["Noyan"]);
        assert_eq!(yaml::write_agents(agents), AGENTS_YAML);
        let noyan = new_agent(&agents[0]);
        assert_eq!(noyan.role, Some(Role::ProjectAgent));
        // Noyan changes flows too (FLW-11).
        assert_eq!(
            noyan.skills,
            [
                SkillRef::Shipped("noyan".into()),
                SkillRef::Shipped("flow".into())
            ]
        );
        assert_eq!(noyan.model, AgentModel::Default);
    }

    #[test]
    fn models_read_as_default_a_tool_or_tool_and_model() {
        assert_eq!(model_of("default"), Ok(AgentModel::Default));
        assert_eq!(
            model_of("claude"),
            Ok(AgentModel::Tool {
                tool: Tool::Claude,
                model: None
            })
        );
        assert_eq!(
            model_of("opencode:anthropic/claude-sonnet-5-5#high"),
            Ok(AgentModel::Tool {
                tool: Tool::OpenCode,
                model: Some("anthropic/claude-sonnet-5-5#high".into())
            })
        );
        assert!(model_of("gpt").is_err());
    }
}
