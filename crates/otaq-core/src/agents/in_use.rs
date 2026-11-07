//! What uses an agent, for Delete (AGM-01, d0108): a stage of any project's flow (in M32 each
//! project's flow derives from the default one), Otaq's default flow itself (every project's flow derives from it,
//! and a project that turns Validation on gets its Validate stage back), or Noyan. "Noyan uses Strong. Pick another
//! agent for Noyan first"; "Spec writer is used in shop-web · Product spec. Change that stage first."

use serde::{Deserialize, Serialize};

use super::{Agent, resolve};
use crate::board::{Board, BoardError};
use crate::flow::Stage;

/// One stage that names the agent as its agent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StageUse {
    /// The project's name; `None` for Otaq's default flow.
    pub project: Option<String>,
    /// The stage's name ("Product spec").
    pub stage: String,
}

impl StageUse {
    /// "shop-web · Product spec", "Otaq's default flow · Validate".
    pub fn label(&self) -> String {
        let flow = self
            .project
            .clone()
            .unwrap_or_else(|| format!("{}'s default flow", crate::naming::PRODUCT_NAME));
        format!("{flow} \u{b7} {}", self.stage)
    }
}

/// Everything that uses an agent.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct InUse {
    /// The agent's name.
    pub agent: String,
    /// Noyan uses it (the app setting).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub noyan: bool,
    /// The stages that name it, project by project in key order, then the default flow's when no project names it.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub stages: Vec<StageUse>,
}

impl InUse {
    pub fn is_empty(&self) -> bool {
        !self.noyan && self.stages.is_empty()
    }

    /// Why Delete is refused (AGM-01), or `None` when nothing uses it: Noyan first, then the stages.
    pub fn refusal(&self) -> Option<String> {
        if self.noyan {
            return Some(noyan_refusal(&self.agent));
        }
        match self.stages.as_slice() {
            [] => None,
            [one] => Some(format!(
                "{} is used in {}. Change that stage first.",
                self.agent,
                one.label()
            )),
            many => {
                let labels: Vec<String> = many.iter().map(StageUse::label).collect();
                Some(format!(
                    "{} is used in {}. Change those stages first.",
                    self.agent,
                    join(&labels)
                ))
            }
        }
    }
}

/// The refusal of deleting the agent Noyan uses: "Noyan uses Strong. Pick another agent for Noyan first".
pub fn noyan_refusal(name: &str) -> String {
    format!("Noyan uses {name}. Pick another agent for Noyan first")
}

/// "a", "a and b", "a, b and c".
fn join(items: &[String]) -> String {
    match items {
        [] => String::new(),
        [one] => one.clone(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

/// The uses of `agent` in a flow's stages.
async fn uses_in(
    board: &Board,
    agent: &Agent,
    stages: &[Stage],
    project: Option<&str>,
) -> Result<Vec<StageUse>, BoardError> {
    let mut uses = Vec::new();
    for stage in stages {
        for name in stage.named_agents() {
            if resolve::named(board.store(), name)
                .await?
                .is_some_and(|a| a.id == agent.id)
            {
                uses.push(StageUse {
                    project: project.map(str::to_owned),
                    stage: stage.name.clone(),
                });
            }
        }
    }
    Ok(uses)
}

/// Reads what uses `agent`.
pub async fn in_use(board: &Board, agent: &Agent) -> Result<InUse, BoardError> {
    let mut stages = Vec::new();
    for project in board.projects().await? {
        let flow = board.effective_flow(&project.key).await?;
        stages.extend(uses_in(board, agent, &flow.stages, Some(&project.name)).await?);
    }
    if stages.is_empty() {
        let default = crate::flow::default_flow();
        stages = uses_in(board, agent, &default.stages, None).await?;
    }
    Ok(InUse {
        agent: agent.name.clone(),
        noyan: board.store().noyan_agent_id().await?.as_deref() == Some(agent.id.as_str()),
        stages,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stage(project: &str, stage: &str) -> StageUse {
        StageUse {
            project: Some(project.into()),
            stage: stage.into(),
        }
    }

    #[test]
    fn the_refusals_read_as_agm_01_says() {
        let spec_writer = InUse {
            agent: "Spec writer".into(),
            noyan: false,
            stages: vec![stage("shop-web", "Product spec")],
        };
        assert_eq!(
            spec_writer.refusal().unwrap(),
            "Spec writer is used in shop-web \u{b7} Product spec. Change that stage first."
        );
        let noyan = InUse {
            agent: "Noyan".into(),
            noyan: true,
            stages: Vec::new(),
        };
        assert_eq!(
            noyan.refusal().unwrap(),
            "Noyan uses Noyan. Pick another agent for Noyan first"
        );
        let developer = InUse {
            agent: "Developer".into(),
            noyan: false,
            stages: vec![
                stage("pocket-notes", "Dev"),
                stage("pocket-notes", "Done"),
                StageUse {
                    project: None,
                    stage: "Dev".into(),
                },
            ],
        };
        assert_eq!(
            developer.refusal().unwrap(),
            format!(
                "Developer is used in pocket-notes \u{b7} Dev, pocket-notes \u{b7} Done and {}'s default flow \u{b7} \
                 Dev. Change those stages first.",
                crate::naming::PRODUCT_NAME
            )
        );
        assert_eq!(InUse::default().refusal(), None);
    }
}
