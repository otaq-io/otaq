//! The checkout copy (AGM-07; d0098's copy and marker, brought back by d0108 for the Project agent, an exception to
//! d0023): it runs in the project's own folder, the user's checkout, not a worktree. Before
//! one starts (or resumes), its agent's skills are copied there as `otaq-<name>` with Otaq's marker and a `.gitignore`
//! holding `*` ([`crate::skills::copy`]), so `git status` and `git add -A` never see them. When its session ends, its
//! copies go, unless another running Project agent of that folder still holds one of the same name. Copies
//! a crash left are removed at service start ([`ProjectAgents::remove_checkout_leftovers`]). Nothing else in the
//! repository or its `.git` folder is written, and no launch flag is added (d0011).

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use super::ProjectAgents;
use crate::agent_tools::FirstPrompt;
use crate::agents::Agent;
use crate::skills::{Marker, copy};

/// The checkout copies each running Project agent holds, by its id: the folder and the copies' names.
pub(super) type Held = HashMap<String, (PathBuf, Vec<String>)>;

impl ProjectAgents {
    /// Copies `agent`'s skills, and the skill `first_prompt` runs, into `repo` before the agent `id`'s session
    /// starts, and records them as held by `id` (replacing what it held). Returns the prompt naming the skill as
    /// resolved for `agent`. A copy that can't be written is logged; the agent still starts.
    pub(super) async fn copy_into_checkout(
        &self,
        id: &str,
        repo: &Path,
        agent: Option<&Agent>,
        first_prompt: Option<FirstPrompt>,
    ) -> Option<FirstPrompt> {
        let mut held = self.inner.checkout.lock().await;
        let (prompt, written) = self
            .inner
            .board
            .copy_for_session(
                repo,
                self.inner.adapters.shipped_skills(),
                agent,
                first_prompt,
                Marker::Checkout,
            )
            .await;
        let before = held.insert(id.to_owned(), (repo.to_owned(), written.names));
        if let Some((folder, names)) = before {
            remove_unheld(&held, &folder, names).await;
        }
        prompt
    }

    /// The agent `id`'s session ended (or never started): the copies it held go, but those another running agent
    /// of the same folder holds.
    pub(super) async fn release_checkout(&self, id: &str) {
        let mut held = self.inner.checkout.lock().await;
        if let Some((folder, names)) = held.remove(id) {
            remove_unheld(&held, &folder, names).await;
        }
    }

    /// At service start, before any Project agent runs: every marked copy left in a project's folder (a crash, a
    /// killed service) is removed.
    pub(super) async fn remove_checkout_leftovers(&self) {
        let projects = match self.inner.board.projects().await {
            Ok(projects) => projects,
            Err(e) => {
                tracing::warn!(error = %e, "can't list the projects to tidy their skill copies");
                return;
            }
        };
        let folders: Vec<PathBuf> = projects
            .into_iter()
            .map(|p| PathBuf::from(p.repo_path))
            .collect();
        let removed = tokio::task::spawn_blocking(move || {
            folders
                .iter()
                .map(|folder| copy::remove_marked_copies(folder).len())
                .sum::<usize>()
        })
        .await
        .unwrap_or_default();
        if removed > 0 {
            tracing::info!(removed, "skill copies left in project folders removed");
        }
    }
}

/// Removes `names` from `folder` except those still `held` there by another agent.
async fn remove_unheld(held: &Held, folder: &Path, names: Vec<String>) {
    let gone: Vec<String> = names
        .into_iter()
        .filter(|name| {
            !held
                .values()
                .any(|(other, others)| other == folder && others.contains(name))
        })
        .collect();
    if gone.is_empty() {
        return;
    }
    let folder = folder.to_owned();
    let _ = tokio::task::spawn_blocking(move || {
        for name in gone {
            copy::remove_copy(&folder, &name);
        }
    })
    .await;
}
