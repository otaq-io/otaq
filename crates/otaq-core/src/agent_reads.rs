//! "What this agent reads" (AGR-01…AGR-03; d0108 rule 12 and its changelog of 2026-10-21): what one agent is given
//! in one project, read-only, for Settings → Agents. Built from the same functions the agent's own start uses, so the
//! preview and the agent can't differ: [`Board::instructions`] (its instructions, then the project's notes, as
//! `otaq prime` gives them, AGM-06), the skill it runs first by its name (the stage's `skill:`, else Otaq's own), the
//! commit skill as prime names it, and the Project agent's `flow` start ([`crate::project_agents::first_skill`]).
//!
//! The parts: one **skill part** per stage of the project's flow whose own agent this is (a ticket's stage choice,
//! AGM-08, doesn't move it: it is per ticket, and the per-ticket context is left out), then one when it is the agent
//! Noyan uses ("noyan · as Noyan"); then the **other skills** it is told about: the commit skill as prime names it (an agent
//! that runs a stage) and, for the Project agent, its `flow` start, each marked. An agent has no skill list of its own (T6). An agent in no stage
//! and with no role has no skill part.
//!
//! A user's skill (one not Otaq's own) is shown from where the skill finder found it ([`Board::found_skills`]): the
//! copy the agent's own tool loads, so its text and Open skill folder → are the user's own folder, never a copy. When
//! the agent's tool doesn't have the skill, the part says so instead of showing another tool's copy.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::agent_tools::{PROJECT_AGENT_SKILL, Tool};
use crate::board::settings::WorkflowStep;
use crate::board::{Board, BoardError};
use crate::flow::Stage;
use crate::skills::found::FoundSkill;

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
    /// The other skills it is told about: the commit skill, and the Project agent's `flow`.
    pub others: Vec<OtherSkill>,
}

/// A skill the agent runs first: in a stage, or as its role.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillPart {
    /// The skill's own name as resolved for the agent ("work-ticket", "team-validate").
    pub skill: String,
    pub runs: RunsAs,
    /// Its SKILL.md in full; `None` when it can't be read (the plugin wasn't found) or the agent's tool doesn't have it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// The folder it is read from; `None` when there is none to show.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub folder: Option<PathBuf>,
    /// The agent's tool, when it doesn't have this user's skill: the part says so and shows no other tool's copy.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub missing_in: Option<Tool>,
}

/// Where a skill part runs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "name", rename_all = "snake_case")]
pub enum RunsAs {
    /// In the stage of that name ("Dev").
    Stage(String),
    /// As Noyan: the agent Noyan uses (the app setting).
    Noyan,
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
        let mut skills = Vec::new();
        let default = self.store().default_model().await?;
        let tool = agent.model.on(default.as_ref()).map(|m| m.tool);
        let mut found: Option<Vec<FoundSkill>> = None;
        let flow = self.effective_flow(&info.key).await?;
        // Every stage naming it, an agent check's check agents each (d0130).
        for stage in &flow.stages {
            if !crate::agents::resolve::stage_names(stage, &agent) {
                continue;
            }
            let name = self.stage_skill(&info.key, stage).await?;
            let runs = RunsAs::Stage(stage.name.clone());
            if crate::agent_tools::is_own_skill(&name) {
                skills.push(part(shipped, name, runs));
            } else {
                if found.is_none() {
                    found = Some(self.found_skills(&info.key).await?);
                }
                let found = found.as_deref().unwrap_or_default();
                skills.push(user_part(found, tool, name, runs));
            }
        }
        let noyan = self.store().noyan_agent_id().await?.as_deref() == Some(agent.id.as_str());
        if noyan {
            skills.push(part(shipped, PROJECT_AGENT_SKILL.to_owned(), RunsAs::Noyan));
        }
        let others = others(shipped, noyan, &skills);
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

/// The skill part for the user's skill `skill` from `found` (the finder's list for the project): the first place,
/// in the order the folders are read, where the agent's `tool` finds it (with no tool known, the first any tool finds
/// it in). Its text is that place's SKILL.md and its folder that place's, the user's own; a tool that doesn't have
/// the skill gets no text and no folder, and is named.
pub fn user_part(
    found: &[FoundSkill],
    tool: Option<Tool>,
    skill: String,
    runs: RunsAs,
) -> SkillPart {
    let place = found
        .iter()
        .find(|f| f.name == skill && !f.otaq)
        .and_then(|f| {
            f.places.iter().find(|p| match tool {
                Some(tool) => p.tools.contains(&tool),
                None => !p.tools.is_empty(),
            })
        });
    let folder = place.map(|p| p.path.clone());
    SkillPart {
        text: folder
            .as_ref()
            .and_then(|f| std::fs::read_to_string(f.join(crate::skills::folder::SKILL_FILE)).ok()),
        missing_in: if place.is_none() { tool } else { None },
        skill,
        runs,
        folder,
    }
}

/// The skill part for `skill`: its text and folder when it is one of Otaq's own skills (read from the bundle at
/// `shipped`).
fn part(shipped: Option<&Path>, skill: String, runs: RunsAs) -> SkillPart {
    let folder = shipped
        .filter(|_| crate::agent_tools::is_own_skill(&skill))
        .map(|dir| dir.join(&skill));
    SkillPart {
        text: folder
            .as_ref()
            .and_then(|f| std::fs::read_to_string(f.join(crate::skills::folder::SKILL_FILE)).ok()),
        skill,
        runs,
        folder,
        missing_in: None,
    }
}

/// The skills the agent is told about that no part shows, each once: the commit skill (as prime names it, for an
/// agent that runs a stage) and the Project agent's `flow`, each marked; a folder only for one of Otaq's own (read from the bundle at `shipped`).
fn others(shipped: Option<&Path>, noyan: bool, parts: &[SkillPart]) -> Vec<OtherSkill> {
    let commit = crate::prime::commit_skill();
    let flow = noyan.then(|| {
        let (skill, _) = crate::project_agents::first_skill(crate::project_agents::FLOW_COMMAND);
        skill.to_owned()
    });
    // A ticket's session is told the commit skill (prime names it), so an agent that runs a stage is.
    let commits = parts.iter().any(|p| matches!(p.runs, RunsAs::Stage(_)));
    let told = commits
        .then_some((commit, SkillMark::Commits))
        .into_iter()
        .chain(flow.map(|name| (name, SkillMark::FlowChanges)));
    let mut out: Vec<OtherSkill> = Vec::new();
    for (name, mark) in told {
        if parts.iter().any(|p| p.skill == name) || out.iter().any(|o| o.skill == name) {
            continue;
        }
        let folder = shipped
            .filter(|_| crate::agent_tools::is_own_skill(&name))
            .map(|dir| dir.join(&name));
        out.push(OtherSkill {
            skill: name,
            folder,
            mark: Some(mark),
        });
    }
    out
}
