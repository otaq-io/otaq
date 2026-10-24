//! Skills by one name, in the worktree and in the checkout (AGM-07; d0108, d0092, d0098).
//!
//! Before a session starts, **every skill of its agent** is copied into its working folder as
//! `otaq-<the skill's own name>` ([`copy`]), shipped and imported alike, for all four tools; the skill the session
//! is told to run first (the stage's own skill, Q261, or Otaq's `address-review`, `open-pr`…) is copied with them.
//! A ticket's agent session also gets **every hand-off skill** ([`Board::handoff_sources`]: `address-review`,
//! `restack` and the flow's Delivery skills) at each launch or resume (d0118): a tool reads its skill folders when it
//! starts, so a skill copied while it runs isn't one it can run by name. The copies a session started with are
//! recorded with it ([`crate::session::Sessions::set_started_skills`]); a hand-off names any other skill by its file
//! ([`crate::session::deliveries::Deliveries::skill_line`]).
//!
//! Shipped skills are read from the installed app's bundle (`<plugin>/skills/<name>/`, [`SHIPPED_FOLDER`]), so they
//! update with Otaq; an imported skill from Otaq's own copy (`$OTAQ_HOME/skills/<id>/`, [`folder`]), with its own
//! folder's contents. The tools' plugins carry only Otaq's hooks, so a skill appears once, under its copy's name.
//!
//! A copy that can't be written is logged and the session still starts (the tool then says it has no such skill).

pub mod copy;
pub mod folder;
pub mod testing;

use std::path::{Path, PathBuf};

pub use copy::{Marker, marked_copies, remove_copy, write_copy};

use crate::agent_tools::FirstPrompt;
use crate::agents::{Agent, SkillRef};
use crate::board::Board;
use crate::data_dir::DataDir;
use crate::naming;

/// The folder of Otaq's shipped skills inside its plugin folder ([`crate::agent_tools::LaunchContext::plugin_dir`]):
/// `<plugin>/skills/<name>/`.
pub const SHIPPED_FOLDER: &str = "skills";

/// A skill to copy: its own name (its SKILL.md's `name:`, a shipped skill's folder) and the folder it is read from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Source {
    pub own: String,
    pub folder: PathBuf,
}

impl Source {
    /// The copy's name: `otaq-commit-compact`.
    pub fn copy_name(&self) -> String {
        naming::copy_name(&self.own)
    }
}

/// Where skills are read from: the bundle's shipped skills (`None` when Otaq's plugin wasn't found) and Otaq's
/// copies of imported ones.
#[derive(Debug, Clone, Copy)]
pub struct Sources<'a> {
    pub shipped: Option<&'a Path>,
    pub data: &'a DataDir,
}

impl Sources<'_> {
    /// The shipped skill `name`: `<plugin>/skills/<name>/` (a folder that isn't there when the plugin wasn't
    /// found: its copy fails and is logged).
    pub fn shipped(&self, name: &str) -> Source {
        Source {
            own: name.to_owned(),
            folder: self.shipped.map(|dir| dir.join(name)).unwrap_or_default(),
        }
    }

    /// The imported skill `id` of `agent`, read from Otaq's copy, when that copy is there and reads (WSK-04).
    fn imported(&self, agent: &Agent, id: &str) -> Option<Source> {
        let skill = agent.imported_skill(id)?;
        let folder = self.data.skill_path(id).ok()?;
        folder::copy_problem(&folder).is_none().then(|| Source {
            own: skill.name.clone(),
            folder,
        })
    }

    /// Every skill of `agent`, in its order.
    pub fn of_agent(&self, agent: &Agent) -> Vec<Source> {
        agent
            .skills
            .iter()
            .filter_map(|skill| match skill {
                SkillRef::Shipped(name) => Some(self.shipped(name)),
                SkillRef::Imported(id) => self.imported(agent, id),
            })
            .collect()
    }

    /// The skill a session runs for a stage's or step's skill `name` (Q261): `agent`'s imported skill of that name
    /// (its SKILL.md's `name:`), else the shipped one. A flow names the stage's skill (`skill: team-validate`, Q296).
    pub fn runs(&self, agent: Option<&Agent>, name: &str) -> Source {
        agent
            .and_then(|agent| {
                agent.skills.iter().find_map(|skill| match skill {
                    SkillRef::Imported(id) => self.imported(agent, id).filter(|s| s.own == name),
                    SkillRef::Shipped(_) => None,
                })
            })
            .unwrap_or_else(|| self.shipped(name))
    }
}

/// What a session's copies came to.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Written {
    /// The skill the session runs first, by its own name as resolved for the agent (`team-validate` for a stage's
    /// `validate`, [`Sources::runs`]); its prompt names its copy (`/otaq-team-validate`).
    pub runs: Option<String>,
    /// The copies written.
    pub names: Vec<String>,
    /// The copies that couldn't be written, and why.
    pub failures: Vec<(String, String)>,
}

/// Writes `runs` (the skill the session runs first) and `skills` into `root`, each once: when two skills would get
/// the same copy name, the first wins (`runs` first). Removes M27's `otaq-custom-*` copies. Blocking.
pub fn write_session(
    root: &Path,
    runs: Option<Source>,
    skills: Vec<Source>,
    marker: Marker,
) -> Written {
    copy::remove_old_copies(root);
    let mut written = Written {
        runs: runs.as_ref().map(|s| s.own.clone()),
        ..Written::default()
    };
    let mut seen: Vec<(String, PathBuf)> = Vec::new();
    for source in runs.into_iter().chain(skills) {
        let name = source.copy_name();
        if let Some((_, folder)) = seen.iter().find(|(n, _)| *n == name) {
            if *folder != source.folder {
                tracing::warn!(
                    copy = name,
                    kept = %folder.display(),
                    left_out = %source.folder.display(),
                    "two skills share a copy name; the first is copied"
                );
            }
            continue;
        }
        seen.push((name.clone(), source.folder.clone()));
        match write_copy(root, &name, &source.folder, marker) {
            Ok(()) => written.names.push(name),
            Err(e) => written.failures.push((name, e.to_string())),
        }
    }
    written
}

impl Board {
    /// The copies right before a session starts in (or is told to run a skill in) `root` (AGM-07): every skill of
    /// `agent` and the skill `runs`, resolved for it ([`Sources::runs`]), from the bundle's shipped skills at
    /// `shipped`. A copy that can't be written is logged; the session still starts.
    pub async fn copy_skills(
        &self,
        root: &Path,
        shipped: Option<&Path>,
        agent: Option<&Agent>,
        runs: Option<&str>,
        marker: Marker,
    ) -> Written {
        self.copy_with(root, shipped, agent, runs, None, marker)
            .await
    }

    /// [`Self::copy_skills`], with the hand-off skills of `handoffs_of`'s flow after the agent's own when it names a
    /// project (d0118).
    async fn copy_with(
        &self,
        root: &Path,
        shipped: Option<&Path>,
        agent: Option<&Agent>,
        runs: Option<&str>,
        handoffs_of: Option<&str>,
        marker: Marker,
    ) -> Written {
        let sources = Sources {
            shipped,
            data: self.data(),
        };
        let runs = runs.map(|name| sources.runs(agent, name));
        let mut skills = agent.map(|a| sources.of_agent(a)).unwrap_or_default();
        if let Some(project) = handoffs_of {
            skills.extend(self.handoff_sources(project, &sources).await);
        }
        let at = root.to_owned();
        let written = tokio::task::spawn_blocking(move || write_session(&at, runs, skills, marker))
            .await
            .expect("skill copy panicked");
        for (name, why) in &written.failures {
            tracing::warn!(
                folder = %root.display(),
                copy = name,
                error = %why,
                "a skill wasn't copied for the session"
            );
        }
        written
    }

    /// [`Self::copy_skills`] for a session about to start with `first_prompt`: a skill prompt runs the skill as
    /// resolved for `agent` (its imported skill standing for the stage's skill), with the message kept; text and no
    /// prompt copy only the agent's skills. Returns the prompt to launch with and what was written.
    pub async fn copy_for_session(
        &self,
        root: &Path,
        shipped: Option<&Path>,
        agent: Option<&Agent>,
        first_prompt: Option<FirstPrompt>,
        marker: Marker,
    ) -> (Option<FirstPrompt>, Written) {
        self.copy_prompted(root, shipped, agent, first_prompt, None, marker)
            .await
    }

    /// [`Self::copy_for_session`] for a ticket's agent session in its worktree (a launch or a resume, the Dev
    /// stage's or the Delivery's, d0118): every hand-off skill of `project`'s flow is copied too
    /// ([`Self::handoff_sources`]), so a later hand-off types one the session started with. The caller records
    /// [`Written::names`] with the session once it runs ([`crate::session::Sessions::set_started_skills`]).
    pub async fn copy_for_ticket_session(
        &self,
        root: &Path,
        shipped: Option<&Path>,
        agent: Option<&Agent>,
        first_prompt: Option<FirstPrompt>,
        project: &str,
    ) -> (Option<FirstPrompt>, Written) {
        self.copy_prompted(
            root,
            shipped,
            agent,
            first_prompt,
            Some(project),
            Marker::Worktree,
        )
        .await
    }

    async fn copy_prompted(
        &self,
        root: &Path,
        shipped: Option<&Path>,
        agent: Option<&Agent>,
        first_prompt: Option<FirstPrompt>,
        handoffs_of: Option<&str>,
        marker: Marker,
    ) -> (Option<FirstPrompt>, Written) {
        let runs = match &first_prompt {
            Some(FirstPrompt::Skill { name, .. }) => Some(name.as_str()),
            _ => None,
        };
        let written = self
            .copy_with(root, shipped, agent, runs, handoffs_of, marker)
            .await;
        let prompt = match (first_prompt, &written.runs) {
            (Some(FirstPrompt::Skill { message, .. }), Some(name)) => Some(FirstPrompt::Skill {
                name: name.clone(),
                message,
            }),
            (prompt, _) => prompt,
        };
        (prompt, written)
    }

    /// The skills Otaq may hand a ticket's running agent session later (d0118): Otaq's `address-review` (Send back,
    /// a decision's Change…, a drag back, Apply now) and `restack` (a merge conflict at Delivery), and the flow's
    /// Delivery skills for Open PR and Push as the Delivery stage's agent has them ([`Self::delivery_skill`]: an
    /// imported skill standing for `open-pr` or the one the flow names, else Otaq's `open-pr` and `push`). Settings →
    /// Agents doesn't list them: they aren't the agent's own.
    pub async fn handoff_sources(&self, project: &str, sources: &Sources<'_>) -> Vec<Source> {
        let mut list = vec![
            sources.shipped(crate::review::ADDRESS_REVIEW_SKILL),
            sources.shipped(crate::review::RESTACK_SKILL),
        ];
        let agent = crate::agents::resolve::delivery_agent(self, project)
            .await
            .ok();
        for step in [
            crate::board::settings::WorkflowStep::OpenPr,
            crate::board::settings::WorkflowStep::Push,
        ] {
            let name = match self.delivery_skill_name(project, step).await {
                Ok(name) => name,
                Err(e) => {
                    tracing::warn!(%project, error = %e, "the flow's Delivery skill isn't known; the built-in one is copied");
                    step.own_skill().to_owned()
                }
            };
            list.push(sources.runs(agent.as_ref(), &name));
        }
        list
    }

    /// The skill the PR path runs for `step` (Open PR, Push), by its own name: for Open PR the skill the flow's
    /// Delivery stage names (`skill: my-pr`, Q296), else Otaq's (`open-pr`, `push`), as the Delivery stage's agent
    /// has it. The Open PR flow's lines and REV-11's "use your own Open PR skill" name it (WSK-08).
    pub async fn delivery_skill(
        &self,
        project: &str,
        step: crate::board::settings::WorkflowStep,
    ) -> Result<String, crate::board::BoardError> {
        let agent = crate::agents::resolve::delivery_agent(self, project)
            .await
            .ok();
        let name = self.delivery_skill_name(project, step).await?;
        let sources = Sources {
            shipped: None,
            data: self.data(),
        };
        Ok(sources.runs(agent.as_ref(), &name).own)
    }

    /// The skill name the PR path runs for `step`: for Open PR the one the flow's Delivery stage names (Q296), else
    /// the step's own (`open-pr`, `push`).
    pub async fn delivery_skill_name(
        &self,
        project: &str,
        step: crate::board::settings::WorkflowStep,
    ) -> Result<String, crate::board::BoardError> {
        let named = match step {
            crate::board::settings::WorkflowStep::OpenPr => self
                .effective_flow(project)
                .await?
                .stages
                .iter()
                .find(|s| s.is_delivery())
                .and_then(|s| s.skill.clone()),
            _ => None,
        };
        Ok(named.unwrap_or_else(|| step.own_skill().to_owned()))
    }
}
