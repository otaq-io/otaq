//! "What this agent reads" (AGR-01…AGR-03; d0108 rule 12 and its changelog of 2026-10-21): what one agent is given
//! in one project, read-only, for Settings → Agents. Built from the same functions the agent's own start uses, so the
//! preview and the agent can't differ: [`Board::instructions`] (its instructions, then the project's notes, as
//! `otaq prime` gives them, AGM-06), [`crate::skills::Sources::runs`] for each skill it runs first (AGM-07: its
//! imported skill standing for the stage's), the commit skill as prime names it, and the Project agent's `flow` start
//! ([`crate::project_agents::first_skill`]).
//!
//! The parts: one **skill part** per stage of the project's flow whose own agent this is (a ticket's stage choice,
//! AGM-08, doesn't move it: it is per ticket, and the per-ticket context is left out), then one for the role it holds
//! ("noyan · as Noyan"); then its **other skills** by name, the commit skill and the Project agent's
//! `flow` marked. An agent in no stage and with no role has no skill part.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::agent_tools::{EPIC_LEAD_SKILL, PROJECT_AGENT_SKILL};
use crate::agents::{Agent, Role};
use crate::board::settings::WorkflowStep;
use crate::board::{Board, BoardError};
use crate::flow::Stage;
use crate::skills::{Source, Sources};

/// `agents.reads`: the agent (its id) and the project (its key).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentReadsParams {
    pub agent: String,
    pub project: String,
}

/// What one agent reads in one project.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentReads {
    pub agent_id: String,
    /// "Rust developer".
    pub agent: String,
    /// The project's key ("SHOP") and name ("shop-web").
    pub project: String,
    pub project_name: String,
    /// The agent's own instructions, as prime gives them; empty when it has none.
    pub instructions: String,
    /// The project's notes, as prime gives them; empty when it has none.
    pub notes: String,
    /// One per stage it runs, then one for its role.
    pub skills: Vec<SkillPart>,
    /// Its other skills, in its order.
    pub others: Vec<OtherSkill>,
}

/// A skill the agent runs first: in a stage, or as its role.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillPart {
    /// The skill's own name as resolved for the agent ("work-ticket", "team-validate").
    pub skill: String,
    pub runs: RunsAs,
    /// Its SKILL.md in full; `None` when it can't be read (the plugin wasn't found, a copy went missing).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// The folder it is read from; `None` when there is none to show.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub folder: Option<PathBuf>,
}

/// Where a skill part runs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "name", rename_all = "snake_case")]
pub enum RunsAs {
    /// In the stage of that name ("Dev").
    Stage(String),
    /// As the role's agent.
    Role(Role),
}

/// One of the agent's other skills.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OtherSkill {
    pub skill: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub folder: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mark: Option<SkillMark>,
}

/// Why an other skill is marked.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SkillMark {
    /// The commit skill prime names (d0050).
    Commits,
    /// The Project agent's `flow` start (FLW-11).
    FlowChanges,
}

impl Board {
    /// What the agent `agent_id` reads in `project`, its shipped skills read from `shipped` (the installed app's
    /// bundle, AGM-07).
    pub async fn agent_reads(
        &self,
        agent_id: &str,
        project: &str,
        shipped: Option<&Path>,
    ) -> Result<AgentReads, BoardError> {
        let agent = self
            .store()
            .agent(agent_id)
            .await?
            .ok_or_else(|| BoardError::NotFound(format!("no agent {agent_id}")))?;
        let info = self.project(project).await?;
        let told = self.instructions(&info.key, Some(&agent)).await?;
        let sources = Sources {
            shipped,
            data: self.data(),
        };
        let mut skills = Vec::new();
        let flow = self.effective_flow(&info.key).await?;
        for stage in flow.stages.iter().filter(|s| s.agent.is_some()) {
            let runs_it = crate::agents::resolve::stage_agent(self, stage)
                .await
                .is_ok_and(|a| a.id == agent.id);
            if !runs_it {
                continue;
            }
            let name = self.stage_skill(&info.key, stage).await?;
            skills.push(part(
                sources.runs(Some(&agent), &name),
                RunsAs::Stage(stage.name.clone()),
            ));
        }
        if let Some(role) = agent.role {
            skills.push(part(
                sources.runs(Some(&agent), role_skill(role)),
                RunsAs::Role(role),
            ));
        }
        let others = others(&sources, &agent, &skills);
        Ok(AgentReads {
            agent_id: agent.id.clone(),
            agent: agent.name.clone(),
            project: info.key,
            project_name: info.name,
            instructions: told.as_ref().map(|t| t.own.clone()).unwrap_or_default(),
            notes: told.map(|t| t.notes).unwrap_or_default(),
            skills,
            others,
        })
    }

    /// The skill a session in `stage` runs first, by the flow's name for it: an Agent stage's and an agent check's
    /// own (the flow file requires one), a Delivery's Open PR skill.
    async fn stage_skill(&self, project: &str, stage: &Stage) -> Result<String, BoardError> {
        match &stage.skill {
            Some(skill) if !stage.is_delivery() => Ok(skill.clone()),
            _ => {
                self.delivery_skill_name(project, WorkflowStep::OpenPr)
                    .await
            }
        }
    }
}

/// The skill a role's agent runs first: the Project agent's own (a `/flow` start runs `flow`, listed with the
/// others), the epic lead's.
pub fn role_skill(role: Role) -> &'static str {
    match role {
        Role::ProjectAgent => PROJECT_AGENT_SKILL,
        Role::EpicLead => EPIC_LEAD_SKILL,
    }
}

fn part(source: Source, runs: RunsAs) -> SkillPart {
    let folder = Some(source.folder).filter(|f| !f.as_os_str().is_empty());
    SkillPart {
        text: folder
            .as_ref()
            .and_then(|f| std::fs::read_to_string(f.join(crate::skills::folder::SKILL_FILE)).ok()),
        skill: source.own,
        runs,
        folder,
    }
}

/// The agent's skills that no part shows, each once, the commit skill (as prime names it) and the Project agent's
/// `flow` marked.
fn others(sources: &Sources<'_>, agent: &Agent, parts: &[SkillPart]) -> Vec<OtherSkill> {
    let commit = crate::prime::commit_skill(sources, Some(agent));
    let flow = (agent.role == Some(Role::ProjectAgent)).then(|| {
        let (skill, _) = crate::project_agents::first_skill(crate::project_agents::FLOW_COMMAND);
        sources.runs(Some(agent), skill).own
    });
    let mut out: Vec<OtherSkill> = Vec::new();
    for source in sources.of_agent(agent) {
        if parts.iter().any(|p| p.skill == source.own) || out.iter().any(|o| o.skill == source.own)
        {
            continue;
        }
        let mark = if commit.as_deref() == Some(source.own.as_str()) {
            Some(SkillMark::Commits)
        } else if flow.as_deref() == Some(source.own.as_str()) {
            Some(SkillMark::FlowChanges)
        } else {
            None
        };
        out.push(OtherSkill {
            folder: Some(source.folder).filter(|f| !f.as_os_str().is_empty()),
            skill: source.own,
            mark,
        });
    }
    out
}
