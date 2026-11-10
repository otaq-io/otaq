//! Which tools have a skill, for a project or one of its tickets (S6, "Which tool has a skill").
//!
//! A tool has a skill when it finds it in the ticket's worktree ([`crate::skills::found`] says which folders each tool
//! reads): its own personal folders, the project's folders in the worktree, or, for Claude Code only, the checkout's
//! `.claude/skills`. Before a ticket has a worktree (and for the project as a whole, as the flow editor asks), the
//! project's skills count when they are committed at the base the worktree will be made from: the ticket's base, or
//! the project's. Otaq's own skills are had by every tool.
//!
//! Each tool's folder variables are read from the environment Otaq launches it with: the service's own, with the
//! project's variables on top ([`crate::session::env`]'s order rule; Otaq's own variables never name a tool's
//! folder).
//!
//! Every answer reads the folders again (nothing is cached): a skill added or committed shows at the next question.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use super::{Board, BoardError};
use crate::agent_tools::{self, Tool};
use crate::board::settings::same_env_name;
use crate::skills::found::{self, FoundSkill, Homes, PROJECT, ProjectFolders};
use crate::tools::Os;

impl Board {
    /// Every skill the user already has for `project` plus Otaq's own, read only, each with the tools that have it
    /// for the project's tickets ([`found::list_in`]): its project skills count when committed at the project's base.
    pub async fn found_skills(&self, project: &str) -> Result<Vec<FoundSkill>, BoardError> {
        self.found_skills_for(project, None).await
    }

    /// Every skill the user already has plus Otaq's own, each with the tools that have it for `ticket` (a key) of
    /// `project`, or for the project as a whole without one.
    pub async fn found_skills_for(
        &self,
        project: &str,
        ticket: Option<&str>,
    ) -> Result<Vec<FoundSkill>, BoardError> {
        let (homes, folders) = self.skill_folders(project, ticket).await?;
        let listed = tokio::task::spawn_blocking(move || found::list_in(&homes, &folders)).await;
        Ok(listed.unwrap_or_else(|e| {
            tracing::warn!(project, error = %e, "listing the skills stopped");
            Vec::new()
        }))
    }

    /// The tools that have the skill `name` for `ticket` (a key) of `project`, or for the project as a whole without
    /// one, in [`Tool::ALL`]'s order: every tool for one of Otaq's own. A project that isn't found has only the
    /// personal folders and plugins.
    pub async fn skill_tools(&self, project: &str, ticket: Option<&str>, name: &str) -> Vec<Tool> {
        if found::is_shipped(name) {
            return Tool::ALL.to_vec();
        }
        self.found_skill(project, ticket, name)
            .await
            .map(|skill| skill.tools)
            .unwrap_or_default()
    }

    /// The skill `name` as the tools find it for `ticket` (a key) of `project`, or for the project as a whole without
    /// one ([`found::find`]): who has it, and who finds it but skips it. A project that isn't found has only the
    /// personal folders and plugins.
    pub async fn found_skill(
        &self,
        project: &str,
        ticket: Option<&str>,
        name: &str,
    ) -> Option<FoundSkill> {
        let (homes, folders) = match self.skill_folders(project, ticket).await {
            Ok(found) => found,
            Err(e) => {
                tracing::debug!(project, ticket, error = %e, "the project's skills folders aren't known");
                (self.skill_homes(None).await, ProjectFolders::default())
            }
        };
        let name = name.to_owned();
        tokio::task::spawn_blocking(move || found::find(&homes, &folders, &name))
            .await
            .ok()
            .flatten()
    }

    /// Whether `tool` has the skill `name` for `ticket` (a key) of `project` ([`Self::skill_tools`]).
    pub async fn tool_has_skill(
        &self,
        project: &str,
        ticket: Option<&str>,
        tool: Tool,
        name: &str,
    ) -> bool {
        self.skill_tools(project, ticket, name)
            .await
            .contains(&tool)
    }

    /// Whether some tool has the skill `name` for the tickets of `project` ([`Self::skill_tools`]).
    pub async fn has_skill(&self, project: &str, name: &str) -> bool {
        !self.skill_tools(project, None, name).await.is_empty()
    }

    /// The personal folders as the tools launched for the project with id `project_id` find them (`None`: the
    /// service's own environment only).
    async fn skill_homes(&self, project_id: Option<&str>) -> Homes {
        let Some(home) = self.skill_home.get().cloned().flatten() else {
            return Homes::default();
        };
        let rows = match project_id {
            Some(id) => self.project_env(id).await.unwrap_or_default(),
            None => Vec::new(),
        };
        Homes::with_env(Some(&home), &launch_var(&rows))
    }

    /// The personal folders and the project's folders for `ticket` of `project` (or the project as a whole).
    async fn skill_folders(
        &self,
        project: &str,
        ticket: Option<&str>,
    ) -> Result<(Homes, ProjectFolders), BoardError> {
        let project = self.project(project).await?;
        let homes = self.skill_homes(Some(&project.id)).await;
        let checkout = PathBuf::from(&project.repo_path);
        let mut base = project.base_branch.clone();
        if let Some(key) = ticket {
            let ticket = self.ticket(key).await?;
            let worktree = ticket
                .work
                .as_ref()
                .and_then(|work| work.worktree.as_deref())
                .map(PathBuf::from)
                .filter(|path| path.is_dir());
            if let Some(worktree) = worktree {
                return Ok((
                    homes,
                    ProjectFolders {
                        checkout: Some(checkout),
                        worktree: Some(worktree),
                        committed: None,
                    },
                ));
            }
            ticket.base.branch().clone_into(&mut base);
        }
        let committed = self.committed_skills(&checkout, &base).await;
        Ok((
            homes,
            ProjectFolders {
                checkout: Some(checkout),
                worktree: None,
                committed,
            },
        ))
    }

    /// The skill folders committed at `base` in the repository at `checkout`, relative to it
    /// (`.agents/skills/team-validate`): a skill's folder, or a link committed in its place. `None` when git can't
    /// say (no git, no such branch): then every skill in the checkout counts.
    async fn committed_skills(&self, checkout: &Path, base: &str) -> Option<HashSet<PathBuf>> {
        let git = self.git().ok()?;
        let folders: Vec<String> = PROJECT.iter().map(|parts| parts.join("/")).collect();
        for rev in [format!("refs/heads/{base}"), base.to_owned()] {
            let mut args = vec!["ls-tree", "-r", "--name-only", rev.as_str(), "--"];
            args.extend(folders.iter().map(String::as_str));
            match git.run(checkout, &args).await {
                Ok(out) => return Some(committed_folders(&out)),
                Err(e) => {
                    tracing::debug!(checkout = %checkout.display(), rev, error = %e, "can't list the committed skills");
                }
            }
        }
        None
    }
}

/// The skill folders in `git ls-tree -r --name-only` output: each path's first three parts (`.claude`, `skills`,
/// the skill's folder), joined as the platform joins them.
fn committed_folders(out: &str) -> HashSet<PathBuf> {
    out.lines()
        .filter_map(|line| {
            let parts: Vec<&str> = line.trim().split('/').collect();
            (parts.len() >= 3).then(|| parts[..3].iter().collect::<PathBuf>())
        })
        .collect()
}

/// How a launched tool reads `name`: the project's variable (`rows`) of that name, else the service's.
fn launch_var(rows: &[(String, String)]) -> impl Fn(&str) -> Option<std::ffi::OsString> + '_ {
    move |name| {
        rows.iter()
            .find(|(row, _)| same_env_name(row, name, Os::current()))
            .map(|(_, value)| value.into())
            .or_else(|| agent_tools::process_var(name))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn committed_folders_are_the_skill_folders() {
        let out =
            ".agents/skills/team/SKILL.md\n.agents/skills/team/ref/a.md\n.claude/skills/linked\n";
        let found = committed_folders(out);
        assert_eq!(found.len(), 2);
        assert!(found.contains(&Path::new(".agents").join("skills").join("team")));
        assert!(found.contains(&Path::new(".claude").join("skills").join("linked")));
    }

    #[test]
    fn a_project_variable_wins_over_the_services() {
        let rows = vec![("CODEX_HOME".to_owned(), "/team/codex".to_owned())];
        let var = launch_var(&rows);
        assert_eq!(var("CODEX_HOME"), Some("/team/codex".into()));
        assert_eq!(
            var("SURELY_UNSET_FOR_THIS_SKILL_TEST"),
            agent_tools::process_var("SURELY_UNSET_FOR_THIS_SKILL_TEST")
        );
    }
}
