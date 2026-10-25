//! A stage's skill the Project agent writes (FED-06; d0115 "Stage templates fill the shape, the user supplies the
//! skill", d0092, d0023; `s27-06`): the user's **Ask the agent to write it** sends the Project agent "Write a skill
//! for the Plan stage: …", and the field says "Being written by Noyan…" ([`super::draft::SkillWriting`]).
//!
//! The agent writes **only in a folder Otaq gives it** (`otaq flow write-skill <stage>`, `flow.write_skill`):
//! `$OTAQ_HOME/skill-writes/<KEY>/<stage id>/`, never the project's checkout or a worktree. When it is done
//! (`otaq flow write-skill <stage> --done`) Otaq checks the folder as any import (a SKILL.md whose front matter
//! reads), keeps **its own copy** under `$OTAQ_HOME/skills/<id>/` ([`crate::agents::skills::import_folder`], d0092)
//! and deletes the folder. The draft then names the skill as the stage's, and the stage's agent brings the copy's
//! folder, which Apply gives it without copying again ([`super::apply`]); the field shows it tinted, "Written by the
//! agent", with Open and Undo ([`super::draft::WrittenSkill`]). Like proposing, only the user or a Project agent of
//! the project may (`policy::Action::ProposeFlow`).

use serde::{Deserialize, Serialize};

use super::draft::{SkillWriting, WrittenSkill, read};
use super::model::{FlowAgent, FlowFile, Stage};
use super::proposal::{Row, flow_agent_of};
use super::yaml;
use crate::agents::Agent;
use crate::board::{Board, BoardError, Refusal};
use crate::naming;
use crate::skills::folder::{self, SKILL_FILE};

/// `flow.write_skill`'s parameters: the stage (its id or name) whose skill the caller writes; `done` hands the
/// folder in.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WriteSkillParams {
    pub project: String,
    pub stage: String,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub done: bool,
}

/// `flow.write_skill`'s answer: the folder to write in, or the skill handed in.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillWrite {
    /// The stage's name.
    pub stage: String,
    /// The folder the agent writes in (gone once handed in).
    pub folder: String,
    /// The skill's own name, once handed in.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skill: Option<String>,
    /// What the CLI prints.
    pub text: String,
}

/// The draft's stage `stage`, by its id or its name.
pub(crate) fn stage_of<'a>(file: &'a FlowFile, stage: &str) -> Result<&'a Stage, BoardError> {
    file.flow
        .stage(stage)
        .or_else(|| {
            file.flow
                .stages
                .iter()
                .find(|s| s.name.eq_ignore_ascii_case(stage.trim()))
        })
        .ok_or_else(|| {
            BoardError::refused(
                Refusal::BadFlow,
                format!("The flow being edited has no stage {stage}"),
            )
        })
}

/// Takes the skill folder `path` out of every agent of the document.
fn detach(file: &mut FlowFile, path: &str) {
    for agent in &mut file.agents {
        agent.skills.retain(|s| s != path);
    }
}

/// Gives the skill folder `path` to the agent of the stage `stage_id` in the document (only to it): the stage's agent
/// joins the document's agents, as stored, when it isn't there.
fn attach(file: &mut FlowFile, stage_id: &str, path: &str, stored: &[Agent]) {
    detach(file, path);
    let Some(name) = file.flow.stage(stage_id).and_then(|s| s.agent.clone()) else {
        return;
    };
    let at = match file
        .agents
        .iter()
        .position(|a| a.name.eq_ignore_ascii_case(&name))
    {
        Some(at) => at,
        None => {
            let agent = stored
                .iter()
                .find(|a| a.name.eq_ignore_ascii_case(&name))
                .map_or_else(
                    || FlowAgent {
                        name: name.clone(),
                        model: "default".to_owned(),
                        ..FlowAgent::default()
                    },
                    flow_agent_of,
                );
            file.agents.push(agent);
            file.agents.len() - 1
        }
    };
    file.agents[at].skills.push(path.to_owned());
}

/// A stage an agent runs (only those have a skill).
fn runs_agent(stage: &Stage) -> Result<(), BoardError> {
    if stage.agent.is_some() {
        return Ok(());
    }
    Err(BoardError::refused(
        Refusal::BadFlow,
        format!("{} runs no agent, so it has no skill", stage.name),
    ))
}

impl Board {
    /// The folder of Otaq's copy `copy`, as the draft names it.
    fn copy_path(&self, copy: &str) -> Result<String, BoardError> {
        let path = self
            .data()
            .skill_path(copy)
            .map_err(|e| BoardError::refused(Refusal::BadSkillFolder, e.to_string()))?;
        Ok(path.display().to_string())
    }

    /// Deletes Otaq's copy `copy` (Undo, or replaced by another).
    pub(super) async fn drop_copy(&self, copy: &str) {
        if let Ok(path) = self.data().skill_path(copy) {
            let _ = tokio::task::spawn_blocking(move || folder::remove(&path)).await;
        }
    }

    /// Deletes the folder the agent writes `stage`'s skill in (Stop).
    pub(super) async fn drop_skill_folder(&self, project: &str, stage: &str) {
        if let Ok(path) = self.data().skill_writing_path(project, stage) {
            let _ = tokio::task::spawn_blocking(move || folder::remove(&path)).await;
        }
    }

    /// `file` without the skill `written`: its stage's skill as it was before, and no agent bringing the copy.
    pub(super) fn without_written(
        &self,
        mut file: FlowFile,
        written: &WrittenSkill,
    ) -> Result<FlowFile, BoardError> {
        detach(&mut file, &self.copy_path(&written.copy)?);
        if let Some(stage) = file
            .flow
            .stages
            .iter_mut()
            .find(|s| s.id == written.stage_id && s.skill.as_deref() == Some(&written.skill))
        {
            stage.skill.clone_from(&written.was);
        }
        Ok(file)
    }

    /// After a change to the draft `row`: the skills being written for stages it no longer has are forgotten; a written
    /// one its stage still names stays given to that stage's agent (an agent's proposal or a new agent doesn't lose it),
    /// one it no longer names goes, with Otaq's copy.
    pub(super) async fn keep_skills(
        &self,
        row: &mut Row,
        stored: &[Agent],
    ) -> Result<(), BoardError> {
        if row.draft.writing.is_empty() && row.draft.written.is_empty() {
            return Ok(());
        }
        let mut file = read(&row.yaml)?;
        row.draft
            .writing
            .retain(|w| file.flow.stage(&w.stage_id).is_some());
        let (kept, gone): (Vec<WrittenSkill>, Vec<WrittenSkill>) =
            row.draft.written.drain(..).partition(|w| {
                file.flow
                    .stage(&w.stage_id)
                    .is_some_and(|s| s.skill.as_deref() == Some(w.skill.as_str()))
            });
        for written in &gone {
            detach(&mut file, &self.copy_path(&written.copy)?);
            self.drop_copy(&written.copy).await;
        }
        for written in &kept {
            attach(
                &mut file,
                &written.stage_id,
                &self.copy_path(&written.copy)?,
                stored,
            );
        }
        row.draft.written = kept;
        row.yaml = yaml::write(&file);
        Ok(())
    }

    /// `otaq flow write-skill <stage>`: the folder to write `stage`'s skill in (made empty the first time, kept as it
    /// is after), and the field says it is being written, by `agent` when a Project agent asks.
    pub async fn skill_folder(
        &self,
        project: &str,
        stage: &str,
        agent: Option<&str>,
    ) -> Result<SkillWrite, BoardError> {
        let key = self.project(project).await?.key;
        let existing = self.draft_row(&key).await?;
        let mut row = match &existing {
            Some(row) => row.clone(),
            None => self.undrafted(&key).await?,
        };
        let file = read(&row.yaml)?;
        let target = stage_of(&file, stage)?.clone();
        runs_agent(&target)?;
        let path = self
            .data()
            .skill_writing_path(&key, &target.id)
            .map_err(|e| BoardError::refused(Refusal::BadSkillFolder, e.to_string()))?;
        let made = path.clone();
        tokio::task::spawn_blocking(move || std::fs::create_dir_all(&made))
            .await
            .expect("folder work panicked")
            .map_err(|e| {
                BoardError::refused(
                    Refusal::BadSkillFolder,
                    format!("Can't make the folder {}: {e}", path.display()),
                )
            })?;
        if !row.draft.writing.iter().any(|w| w.stage_id == target.id) {
            row.draft.writing.push(SkillWriting {
                stage_id: target.id.clone(),
                agent: agent.map(str::to_owned),
            });
            self.write_draft(&key, existing.map(|e| e.id), &row).await?;
        }
        let folder = path.display().to_string();
        let text = format!(
            "Write {name}'s skill in {folder}, and nowhere else: a {SKILL_FILE} whose front matter has `name:` (a short \
             name for the skill, like plan-ticket) and `description:` (one line; the editor shows it as {name}'s \
             caption), and any files it mentions. Then run `{cmd} flow write-skill {id} --done`.",
            name = target.name,
            id = target.id,
            cmd = naming::COMMAND_NAME,
        );
        Ok(SkillWrite {
            stage: target.name,
            folder,
            skill: None,
            text,
        })
    }

    /// `otaq flow write-skill <stage> --done`: the folder the agent wrote is imported as Otaq's own copy (d0092) and
    /// deleted; the draft names the skill as `stage`'s, given to its agent, tinted as written by `by`. A skill it
    /// wrote for the stage before goes. Refused, with the folder kept, when it doesn't read as a skill.
    pub async fn skill_written(
        &self,
        project: &str,
        stage: &str,
        by: &str,
    ) -> Result<SkillWrite, BoardError> {
        let key = self.project(project).await?.key;
        let existing = self.draft_row(&key).await?;
        let mut row = match &existing {
            Some(row) => row.clone(),
            None => self.undrafted(&key).await?,
        };
        let mut file = read(&row.yaml)?;
        let target = stage_of(&file, stage)?.clone();
        runs_agent(&target)?;
        let path = self
            .data()
            .skill_writing_path(&key, &target.id)
            .map_err(|e| BoardError::refused(Refusal::BadSkillFolder, e.to_string()))?;
        if !path.is_dir() {
            return Err(BoardError::refused(
                Refusal::BadSkillFolder,
                format!(
                    "Nothing to hand in for {}: run `{} flow write-skill {}` first, and write the skill in the folder \
                     it gives",
                    target.name,
                    naming::COMMAND_NAME,
                    target.id
                ),
            ));
        }
        let record = crate::agents::skills::import_folder(self, &path).await?;
        let written_in = path.clone();
        let _ = tokio::task::spawn_blocking(move || folder::remove(&written_in)).await;
        let stored = self.store().agents().await?;
        let mut was = target.skill.clone();
        if let Some(at) = row
            .draft
            .written
            .iter()
            .position(|w| w.stage_id == target.id)
        {
            let old = row.draft.written.remove(at);
            detach(&mut file, &self.copy_path(&old.copy)?);
            self.drop_copy(&old.copy).await;
            was = old.was;
        }
        if let Some(stage) = file.flow.stages.iter_mut().find(|s| s.id == target.id) {
            stage.skill = Some(record.name.clone());
        }
        attach(&mut file, &target.id, &self.copy_path(&record.id)?, &stored);
        row.yaml = yaml::write(&file);
        row.draft.writing.retain(|w| w.stage_id != target.id);
        row.draft.written.push(WrittenSkill {
            stage_id: target.id.clone(),
            skill: record.name.clone(),
            copy: record.id.clone(),
            by: by.to_owned(),
            was,
        });
        self.write_draft(&key, existing.map(|e| e.id), &row).await?;
        tracing::info!(project = %key, stage = %target.id, skill = %record.name, copy = %record.id, "a written skill is in the draft");
        let text = format!(
            "{skill} is in {name}'s skill field, as {product}'s own copy. Nothing changes on the board until the user \
             applies the flow.",
            skill = record.name,
            name = target.name,
            product = naming::PRODUCT_NAME,
        );
        Ok(SkillWrite {
            stage: target.name,
            folder: path.display().to_string(),
            skill: Some(record.name),
            text,
        })
    }
}
