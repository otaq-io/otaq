//! What uses an agent, for Delete (AGM-01, d0108): a stage or a stage option of any project's flow (in M32 each
//! project's flow derives from the default one), Otaq's default flow itself (every project's flow derives from it,
//! and a project that turns Validation on gets its Validate stage back), or a role. "Noyan has the Noyan role.
//! Give the role to another agent first"; "Griller is used in shop-web · Grill. Change that stage first."

use serde::{Deserialize, Serialize};

use super::{Agent, Role, resolve};
use crate::board::{Board, BoardError};
use crate::flow::{Stage, StageOption};

/// One stage that names the agent, as its usual agent or as an option.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StageUse {
    /// The project's name; `None` for Otaq's default flow.
    pub project: Option<String>,
    /// The stage's name ("Grill").
    pub stage: String,
    /// The agent is one of the stage's options rather than its usual agent.
    pub option: bool,
}

impl StageUse {
    /// "shop-web · Grill", "Otaq's default flow · Validate".
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
    /// The role it holds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<Role>,
    /// The stages that name it, project by project in key order, then the default flow's when no project names it.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub stages: Vec<StageUse>,
}

impl InUse {
    pub fn is_empty(&self) -> bool {
        self.role.is_none() && self.stages.is_empty()
    }

    /// Why Delete is refused (AGM-01), or `None` when nothing uses it: the role first, then the stages.
    pub fn refusal(&self) -> Option<String> {
        if let Some(role) = self.role {
            return Some(role.holder_refusal(&self.agent));
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
        let options = stage.options.iter().filter_map(|o| match o {
            StageOption::Agent(name) => Some(name),
            StageOption::Skip => None,
        });
        // An agent check's check agents each count (d0130).
        for (name, option) in stage
            .named_agents()
            .map(|n| (n, false))
            .chain(options.map(|n| (n, true)))
        {
            if resolve::named(board.store(), name)
                .await?
                .is_some_and(|a| a.id == agent.id)
            {
                uses.push(StageUse {
                    project: project.map(str::to_owned),
                    stage: stage.name.clone(),
                    option,
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
        role: agent.role,
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
            option: false,
        }
    }

    #[test]
    fn the_refusals_read_as_agm_01_says() {
        let griller = InUse {
            agent: "Griller".into(),
            role: None,
            stages: vec![stage("shop-web", "Grill")],
        };
        assert_eq!(
            griller.refusal().unwrap(),
            "Griller is used in shop-web \u{b7} Grill. Change that stage first."
        );
        let noyan = InUse {
            agent: "Noyan".into(),
            role: Some(Role::ProjectAgent),
            stages: Vec::new(),
        };
        assert_eq!(
            noyan.refusal().unwrap(),
            "Noyan has the Noyan role. Give the role to another agent first"
        );
        let developer = InUse {
            agent: "Developer".into(),
            role: None,
            stages: vec![
                stage("pocket-notes", "Dev"),
                stage("pocket-notes", "Done"),
                StageUse {
                    project: None,
                    stage: "Dev".into(),
                    option: false,
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
