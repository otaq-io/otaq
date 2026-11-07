//! A stage's skill the Project agent writes (FED-06; d0115 "Stage templates fill the shape, the user supplies the
//! skill"; `s27-06`): the user's **Ask the agent to write it** sends the Project agent "Write a skill for the Plan
//! stage: …", and the field says "Being written by Noyan…" ([`super::draft::SkillWriting`]).
//!
//! "Ask the agent to write it" saves into the project (T6). The agent names the skill first and gets its folder
//! (`otaq flow write-skill <stage> --name <name>`, `flow.write_skill`): inside the project's checkout, in the skills
//! folder the stage's agent's tool reads, `.agents/skills/<name>/` when that tool is Codex and `.claude/skills/<name>/`
//! otherwise (Claude Code, Copilot and OpenCode all read it). Otaq makes the folder empty and refuses one that is
//! already there. When the agent is done (`otaq flow write-skill <stage> --done`) Otaq checks that a SKILL.md whose
//! front matter reads is there and names it as the stage's skill (its `name:`, else the folder's name); the field
//! shows it tinted, "Written by the agent", with Open and Undo ([`super::draft::WrittenSkill`]), and says
//! [`saved_text`]. Otaq doesn't commit it, and copies or imports nothing: a ticket's worktree gets the skill once the
//! user commits it.
//!
//! **Undo** and **Stop** delete the folder only while git says it is untracked and it holds only what was written;
//! otherwise they just clear the field ([`Board::remove_written_folder`]). The folders above it that the write made
//! (`.claude/` and `.claude/skills/` in a checkout that had neither) go with it when nothing else is left in them; a
//! folder that was there before the write is never removed. Like proposing, only the user or a
//! Project agent of the project may (`policy::Action::ProposeFlow`).

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::draft::{SkillWriting, WrittenSkill, read};
use super::model::{FlowFile, Stage};
use super::proposal::Row;
use super::yaml;
use crate::agent_tools::Tool;
use crate::agents::AgentModel;
use crate::board::{Board, BoardError, Refusal};
use crate::naming;
use crate::skills::folder::{self, SKILL_FILE};

/// The skills folder Codex reads in a project: a written skill goes in `.agents/skills/<name>/` when the stage's agent
/// runs on Codex.
pub const AGENTS_SKILLS: &str = ".agents/skills";
/// The skills folder Claude Code, Copilot and OpenCode all read in a project.
pub const CLAUDE_SKILLS: &str = ".claude/skills";

/// What the skill field says once the skill is written: "Saved in `<folder>` in this project. Commit it so agents
/// get it in their worktrees." `folder` is relative to the checkout (`.claude/skills/plan-ticket`).
pub fn saved_text(folder: &str) -> String {
    format!("Saved in `{folder}` in this project. Commit it so agents get it in their worktrees.")
}

/// `flow.write_skill`'s parameters: the stage (its id or name) whose skill the caller writes; `name` the skill's name,
/// which names its folder (needed the first time); `done` hands the folder in.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WriteSkillParams {
    pub project: String,
    pub stage: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub done: bool,
}

/// `flow.write_skill`'s answer: the folder to write in, or the skill handed in.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillWrite {
    /// The stage's name.
    pub stage: String,
    /// The folder the agent writes in (absolute).
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

/// A stage an agent runs (only those have a skill).
fn runs_agent(stage: &Stage) -> Result<(), BoardError> {
    if stage.has_agent() {
        return Ok(());
    }
    Err(BoardError::refused(
        Refusal::BadFlow,
        format!("{} runs no agent, so it has no skill", stage.name),
    ))
}

/// A skill name as a folder takes it: lowercase letters, digits and single hyphens inside, at most 64 characters,
/// and not one of Otaq's own skills (a name that matches one always means Otaq's).
fn checked_name(name: &str) -> Result<String, BoardError> {
    let name = name.trim();
    let refused = |why: String| Err(BoardError::refused(Refusal::BadSkillFolder, why));
    let fits = !name.is_empty()
        && name.len() <= naming::SKILL_NAME_MAX
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        && !name.starts_with('-')
        && !name.ends_with('-')
        && !name.contains("--");
    if !fits {
        return refused(format!(
            "`{name}` can't name a skill: use lowercase letters, digits and hyphens, like plan-ticket"
        ));
    }
    if crate::agent_tools::is_own_skill(name) {
        return refused(format!(
            "`{name}` is one of {}'s own skills: pick another name",
            naming::PRODUCT_NAME
        ));
    }
    Ok(name.to_owned())
}

/// The folder (relative to the checkout, `/`-separated) a skill `name` is written in for an agent on `tool`.
pub fn folder_for(tool: Option<Tool>, name: &str) -> String {
    let base = match tool {
        Some(Tool::Codex) => AGENTS_SKILLS,
        _ => CLAUDE_SKILLS,
    };
    format!("{base}/{name}")
}

/// `rel` (`/`-separated) inside `checkout`.
fn inside(checkout: &Path, rel: &str) -> PathBuf {
    rel.split('/')
        .fold(checkout.to_owned(), |p, part| p.join(part))
}

/// The folders above `rel`'s last part (`.claude`, `.claude/skills` for `.claude/skills/plan-ticket`) that aren't in
/// `checkout` yet, outermost first: the ones making `rel` would make.
fn missing_parents(checkout: &Path, rel: &str) -> Vec<String> {
    let parts: Vec<&str> = rel.split('/').collect();
    (1..parts.len())
        .map(|n| parts[..n].join("/"))
        .filter(|parent| std::fs::symlink_metadata(inside(checkout, parent)).is_err())
        .collect()
}

/// Removes the folders `made` (relative to `checkout`) that are empty, innermost first; one that holds anything, or
/// isn't a real folder, stays.
fn remove_empty(checkout: &Path, made: &[String]) {
    let mut made: Vec<&String> = made.iter().collect();
    made.sort_by_key(|rel| std::cmp::Reverse(rel.matches('/').count()));
    for rel in made {
        let path = inside(checkout, rel);
        let is_folder = std::fs::symlink_metadata(&path).is_ok_and(|m| m.is_dir());
        // `remove_dir` removes only an empty folder.
        if is_folder && std::fs::remove_dir(&path).is_ok() {
            tracing::info!(folder = %rel, "an empty folder a skill write made removed");
        }
    }
}

/// The written skill's name: its SKILL.md's `name:`, else the folder's name. Refused when there is no SKILL.md or
/// its front matter doesn't read.
fn written_name(dir: &Path) -> Result<String, String> {
    let file = dir.join(SKILL_FILE);
    if !file.is_file() {
        return Err(format!("The folder has no {SKILL_FILE}"));
    }
    let text =
        std::fs::read_to_string(&file).map_err(|e| format!("{SKILL_FILE} can't be read: {e}"))?;
    let front = crate::skills::found::front_matter(&text)?;
    Ok(front.name.unwrap_or_else(|| {
        dir.file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default()
    }))
}

impl Board {
    /// The project's checkout (where a written skill is saved).
    async fn checkout(&self, project: &str) -> Result<PathBuf, BoardError> {
        Ok(PathBuf::from(self.project(project).await?.repo_path))
    }

    /// The tool of the agent that runs `stage` (its first named agent: the document's own when it defines it, else
    /// the stored one; `default` is the account's default model's tool). `None` when it can't be told.
    async fn stage_tool(&self, file: &FlowFile, stage: &Stage) -> Result<Option<Tool>, BoardError> {
        let Some(name) = stage.named_agents().next() else {
            return Ok(None);
        };
        let documented = file
            .agents
            .iter()
            .find(|a| a.name.eq_ignore_ascii_case(name))
            .and_then(|a| crate::agents::shipped::model_of(&a.model).ok());
        let model = match documented {
            Some(model) => Some(model),
            None => self
                .store()
                .agents()
                .await?
                .into_iter()
                .find(|a| a.name.eq_ignore_ascii_case(name))
                .map(|a| a.model),
        };
        Ok(match model {
            Some(AgentModel::Tool { tool, .. }) => Some(tool),
            Some(AgentModel::Default) => self.store().default_model().await?.map(|m| m.tool),
            None => None,
        })
    }

    /// Deletes the written folder `rel` of `project`'s checkout, but only while git says nothing in it is tracked and
    /// it holds only what was written (its fingerprint is still `fingerprint`, when given); otherwise it stays, and
    /// only the field is cleared. Then the folders above it that the write made (`made`) go too, those left empty.
    /// Whether `rel` was deleted.
    pub(super) async fn remove_written_folder(
        &self,
        project: &str,
        rel: &str,
        fingerprint: Option<&str>,
        made: &[String],
    ) -> bool {
        let gone = self.remove_written(project, rel, fingerprint).await;
        if !made.is_empty()
            && let Ok(checkout) = self.checkout(project).await
        {
            let made = made.to_vec();
            let _ = tokio::task::spawn_blocking(move || remove_empty(&checkout, &made)).await;
        }
        gone
    }

    /// [`Self::remove_written_folder`]'s `rel` alone.
    async fn remove_written(&self, project: &str, rel: &str, fingerprint: Option<&str>) -> bool {
        if rel.is_empty() {
            return false;
        }
        let Ok(checkout) = self.checkout(project).await else {
            return false;
        };
        let path = inside(&checkout, rel);
        if !path.is_dir() {
            return false;
        }
        let Ok(git) = self.git() else {
            return false;
        };
        // An error (not a repository, git missing) keeps the folder: only what is surely untracked is deleted.
        let untracked = git
            .run(&checkout, &["ls-files", "--", rel])
            .await
            .is_ok_and(|tracked| tracked.trim().is_empty());
        if !untracked {
            return false;
        }
        if let Some(fingerprint) = fingerprint {
            let at = path.clone();
            let now = tokio::task::spawn_blocking(move || folder::fingerprint(&at))
                .await
                .ok()
                .and_then(Result::ok);
            if now.as_deref() != Some(fingerprint) {
                return false;
            }
        }
        let gone = path.clone();
        let _ = tokio::task::spawn_blocking(move || folder::remove(&gone)).await;
        tracing::info!(project, folder = rel, "a written skill's folder removed");
        !path.exists()
    }

    /// `file` without the skill `written`: its stage's skill as it was before.
    pub(super) fn without_written(mut file: FlowFile, written: &WrittenSkill) -> FlowFile {
        if let Some(stage) = file
            .flow
            .stages
            .iter_mut()
            .find(|s| s.id == written.stage_id && s.skill.as_deref() == Some(&written.skill))
        {
            stage.skill.clone_from(&written.was);
        }
        file
    }

    /// After a change to the draft `row`: the skills being written for stages it no longer has are forgotten, and so
    /// are written ones their stage no longer names. Their folders stay in the checkout: only Undo and Stop delete one.
    pub(super) fn keep_skills(row: &mut Row) -> Result<(), BoardError> {
        if row.draft.writing.is_empty() && row.draft.written.is_empty() {
            return Ok(());
        }
        let file = read(&row.yaml)?;
        row.draft
            .writing
            .retain(|w| file.flow.stage(&w.stage_id).is_some());
        row.draft.written.retain(|w| {
            file.flow
                .stage(&w.stage_id)
                .is_some_and(|s| s.skill.as_deref() == Some(w.skill.as_str()))
        });
        Ok(())
    }

    /// `otaq flow write-skill <stage> --name <name>`: the folder to write `stage`'s skill in, inside the project's
    /// checkout ([`folder_for`]), made empty the first time and kept as it is after; the field says it is being
    /// written, by `agent` when a Project agent asks. `name` may be left out once the stage has its folder. Refused for
    /// a folder that is already there and wasn't made by this write.
    pub async fn skill_folder(
        &self,
        project: &str,
        stage: &str,
        name: Option<&str>,
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
        let checkout = self.checkout(&key).await?;
        let (had, had_made) = row
            .draft
            .writing
            .iter()
            .find(|w| w.stage_id == target.id)
            .map(|w| (w.folder.clone(), w.made.clone()))
            .unwrap_or_default();
        let rel = match (name, &had) {
            (Some(name), _) => {
                let tool = self.stage_tool(&file, &target).await?;
                folder_for(tool, &checked_name(name)?)
            }
            (None, Some(had)) => had.clone(),
            (None, None) => {
                return Err(BoardError::refused(
                    Refusal::BadSkillFolder,
                    format!(
                        "Name the skill first: `{cmd} flow write-skill {id} --name <name>` (a short name, like \
                         plan-ticket)",
                        cmd = naming::COMMAND_NAME,
                        id = target.id,
                    ),
                ));
            }
        };
        let path = inside(&checkout, &rel);
        if had.as_deref() != Some(rel.as_str()) {
            if std::fs::symlink_metadata(&path).is_ok() {
                return Err(BoardError::refused(
                    Refusal::BadSkillFolder,
                    format!(
                        "`{rel}` is already in this project, and {} won't write over it: pick another name",
                        naming::PRODUCT_NAME
                    ),
                ));
            }
            // A folder this write made under another name goes, while nothing in it is tracked.
            if let Some(had) = &had {
                self.remove_written_folder(&key, had, None, &had_made).await;
            }
        }
        // The folders above it this write makes, kept so Stop and Undo can take them away again once empty.
        let made = if had.as_deref() == Some(rel.as_str()) {
            had_made
        } else {
            missing_parents(&checkout, &rel)
        };
        let making = path.clone();
        tokio::task::spawn_blocking(move || std::fs::create_dir_all(&making))
            .await
            .expect("folder work panicked")
            .map_err(|e| {
                BoardError::refused(
                    Refusal::BadSkillFolder,
                    format!("Can't make the folder {}: {e}", path.display()),
                )
            })?;
        let agent = row
            .draft
            .writing
            .iter()
            .find(|w| w.stage_id == target.id)
            .and_then(|w| w.agent.clone())
            .or_else(|| agent.map(str::to_owned));
        row.draft.writing.retain(|w| w.stage_id != target.id);
        row.draft.writing.push(SkillWriting {
            stage_id: target.id.clone(),
            agent,
            folder: Some(rel.clone()),
            made,
        });
        self.write_draft(&key, existing.map(|e| e.id), &row).await?;
        let skill = rel.rsplit('/').next().unwrap_or_default().to_owned();
        let folder = path.display().to_string();
        let text = format!(
            "Write {name}'s skill in {folder}, and nowhere else: a {SKILL_FILE} whose front matter has `name: \
             {skill}` and `description:` (one line; the editor shows it as {name}'s caption), and any files it \
             mentions. Then run `{cmd} flow write-skill {id} --done`.",
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

    /// `otaq flow write-skill <stage> --done`: the folder the agent wrote must hold a SKILL.md whose front matter
    /// reads; the draft then names the skill (its `name:`, else the folder's name) as `stage`'s, tinted as written by
    /// `by`. Nothing is copied or imported: the skill stays where it was written. Refused, with the folder kept, when
    /// it doesn't read as a skill.
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
        let checkout = self.checkout(&key).await?;
        let nothing = || {
            BoardError::refused(
                Refusal::BadSkillFolder,
                format!(
                    "Nothing to hand in for {}: run `{} flow write-skill {} --name <name>` first, and write the \
                     skill in the folder it gives",
                    target.name,
                    naming::COMMAND_NAME,
                    target.id
                ),
            )
        };
        let (rel, made) = row
            .draft
            .writing
            .iter()
            .find(|w| w.stage_id == target.id)
            .and_then(|w| Some((w.folder.clone()?, w.made.clone())))
            .ok_or_else(nothing)?;
        let path = inside(&checkout, &rel);
        if !path.is_dir() {
            return Err(nothing());
        }
        let at = path.clone();
        let (name, fingerprint) = tokio::task::spawn_blocking(move || {
            let name = written_name(&at)?;
            let fingerprint = folder::fingerprint(&at).map_err(|e| e.to_string())?;
            Ok::<_, String>((name, fingerprint))
        })
        .await
        .expect("folder work panicked")
        .map_err(|why| BoardError::refused(Refusal::BadSkillFolder, why))?;
        let mut was = target.skill.clone();
        if let Some(at) = row
            .draft
            .written
            .iter()
            .position(|w| w.stage_id == target.id)
        {
            let old = row.draft.written.remove(at);
            if old.folder != rel {
                self.remove_written_folder(&key, &old.folder, Some(&old.fingerprint), &old.made)
                    .await;
            }
            was = old.was;
        }
        if let Some(stage) = file.flow.stages.iter_mut().find(|s| s.id == target.id) {
            stage.skill = Some(name.clone());
        }
        row.yaml = yaml::write(&file);
        row.draft.writing.retain(|w| w.stage_id != target.id);
        row.draft.written.push(WrittenSkill {
            stage_id: target.id.clone(),
            skill: name.clone(),
            folder: rel.clone(),
            fingerprint,
            made,
            by: by.to_owned(),
            was,
        });
        self.write_draft(&key, existing.map(|e| e.id), &row).await?;
        tracing::info!(project = %key, stage = %target.id, skill = %name, folder = %rel, "a written skill is in the draft");
        let text = format!(
            "{name} is in {stage}'s skill field. {saved} Nothing changes on the board until the user applies the flow.",
            stage = target.name,
            saved = saved_text(&rel),
        );
        Ok(SkillWrite {
            stage: target.name,
            folder: path.display().to_string(),
            skill: Some(name),
            text,
        })
    }
}
