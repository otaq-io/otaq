//! Projects (PRJ-01…PRJ-04; d0023, d0024, d0029, d0059). A project is one git repository, added with nothing but
//! its folder. Adding reads the repository with git and writes nothing into it (d0023). The key is suggested
//! from the folder's name, can be edited while adding, and never changes afterwards (d0029). The base branch is
//! prefilled from the remote's default (`origin/HEAD`), or the current branch without a remote (d0059).

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::{Board, BoardError, Refusal};
use crate::agent_tools::SetupOutcome;
use crate::domain::ProjectKey;
use crate::store::projects::{NewProject, Project};

/// What adding a folder would give: shown by the add-project form before the user confirms.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FolderProbe {
    /// The repository's top folder; the project stores this.
    pub repo_path: String,
    /// The project's name: the repository folder's name.
    pub name: String,
    pub suggested_key: String,
    /// The detected base branch: the remote default, else the current branch.
    pub base_branch: String,
    /// The repository's branches, for the form's base-branch dropdown.
    #[serde(default)]
    pub branches: Branches,
}

/// Adds a project. `key` and `base_branch` default to the suggestions.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AddProject {
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_branch: Option<String>,
}

/// A repository's branches, for the base-branch dropdown (d0051).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Branches {
    /// Local branch names, sorted.
    pub local: Vec<String>,
    /// Remote-tracking branches as `origin/main`, sorted.
    pub remote: Vec<String>,
}

/// Changes to a project. A `key` different from the project's own is refused (d0029).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectUpdate {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_branch: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
}

impl Board {
    /// Reads a folder for the add-project form: its repository, suggested key and base branch. Refuses a folder
    /// that isn't a git repository or is already a project.
    pub async fn probe_folder(&self, path: &str) -> Result<FolderProbe, BoardError> {
        let root = self.repo_root(Path::new(path)).await?;
        let repo_path = root.display().to_string();
        if let Some(existing) = self.store.project_by_repo_path(&repo_path).await? {
            return Err(BoardError::refused(
                Refusal::AlreadyAdded,
                format!("{} is already added as {}", existing.name, existing.key),
            ));
        }
        let name = root
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| repo_path.clone());
        let taken: Vec<String> = self
            .store
            .projects()
            .await?
            .into_iter()
            .map(|p| p.key)
            .collect();
        Ok(FolderProbe {
            suggested_key: suggest_key(&name, &taken),
            base_branch: self.detect_base_branch(&root).await?,
            branches: self.repo_branches(&root).await?,
            repo_path,
            name,
        })
    }

    /// Adds a repository as a project (PRJ-01). Validates: a git repository, not already added, and a key in the
    /// right format that no other project uses. Records the agent tools' trust for it ([`Self::add_project_trusted`]).
    pub async fn add_project(&self, add: &AddProject) -> Result<Project, BoardError> {
        Ok(self.add_project_trusted(add).await?.0)
    }

    /// [`Self::add_project`] with the outcome of recording the agent tools' trust for the repo once the project is
    /// stored (PRJ-06, d0085): `None` when no tool's config file was located. A `Failed` outcome never fails the
    /// add.
    pub async fn add_project_trusted(
        &self,
        add: &AddProject,
    ) -> Result<(Project, Option<SetupOutcome>), BoardError> {
        let probe = self.probe_folder(&add.path).await?;
        let key = add.key.clone().unwrap_or(probe.suggested_key);
        let key = ProjectKey::new(&key)
            .map_err(|e| BoardError::refused(Refusal::BadKey, e.to_string()))?;
        if let Some(other) = self.store.project_by_key(key.as_str()).await? {
            return Err(BoardError::refused(
                Refusal::KeyTaken,
                format!("{key} is already used by {}", other.name),
            ));
        }
        let base_branch = match &add.base_branch {
            Some(b) => non_empty_branch(b)?,
            None => probe.base_branch,
        };
        let project = self
            .store
            .insert_project(&NewProject {
                key: key.to_string(),
                name: probe.name,
                repo_path: probe.repo_path,
                base_branch,
            })
            .await?;
        tracing::info!(key = %project.key, repo = %project.repo_path, "project added");
        let trust = self.trust.record(&project.key, &project.repo_path).await;
        Ok((project, trust))
    }

    /// The project repository's local and remote branches (read only).
    pub async fn branches(&self, key: &str) -> Result<Branches, BoardError> {
        let project = self.project(key).await?;
        self.repo_branches(Path::new(&project.repo_path)).await
    }

    /// A repository's local and remote-tracking branches, as they are (nothing is fetched).
    async fn repo_branches(&self, repo: &Path) -> Result<Branches, BoardError> {
        let git = self.git()?;
        Ok(Branches {
            local: git.local_branches(repo).await?,
            remote: git.remote_branches(repo).await?,
        })
    }

    /// Every project, oldest first.
    pub async fn projects(&self) -> Result<Vec<Project>, BoardError> {
        Ok(self.store.projects().await?)
    }

    pub async fn project(&self, key: &str) -> Result<Project, BoardError> {
        self.store
            .project_by_key(key)
            .await?
            .ok_or_else(|| BoardError::NotFound(format!("unknown project {key}")))
    }

    /// Changes the name or base branch. The key can't change (d0029).
    pub async fn update_project(
        &self,
        key: &str,
        update: &ProjectUpdate,
    ) -> Result<Project, BoardError> {
        let project = self.project(key).await?;
        if update.key.as_deref().is_some_and(|k| k != project.key) {
            return Err(BoardError::refused(
                Refusal::KeyLocked,
                format!(
                    "The project key {} can't be changed: it is part of every ticket key",
                    project.key
                ),
            ));
        }
        let name = match &update.name {
            Some(name) if name.trim().is_empty() => {
                return Err(BoardError::refused(
                    Refusal::EmptyField,
                    "The project name can't be empty",
                ));
            }
            Some(name) => name.trim().to_owned(),
            None => project.name.clone(),
        };
        let base_branch = match &update.base_branch {
            Some(b) => non_empty_branch(b)?,
            None => project.base_branch.clone(),
        };
        self.store
            .update_project(&project.id, &name, &base_branch)
            .await?;
        self.project(key).await
    }

    /// Removes a project from Otaq: its tickets, links, comments, sessions rows and settings are deleted
    /// from the database. The repository is never touched (PRJ-04).
    pub async fn remove_project(&self, key: &str) -> Result<(), BoardError> {
        let project = self.project(key).await?;
        self.store.delete_project(&project.id).await?;
        tracing::info!(key = %project.key, repo = %project.repo_path, "project removed");
        Ok(())
    }

    async fn repo_root(&self, path: &Path) -> Result<PathBuf, BoardError> {
        let not_a_repo = || {
            BoardError::refused(
                Refusal::NotAGitRepo,
                format!("{} isn't a git repository", path.display()),
            )
        };
        let folder = crate::paths::canonical(path).map_err(|_| {
            BoardError::refused(
                Refusal::NotAGitRepo,
                format!("{} doesn't exist", path.display()),
            )
        })?;
        let git = self.git()?;
        if !folder.is_dir() || !git.is_repo(&folder).await {
            return Err(not_a_repo());
        }
        let root = git.repo_root(&folder).await?;
        Ok(crate::paths::canonical(&root).unwrap_or(root))
    }

    /// The remote's default branch, else the current branch (PRJ-03).
    async fn detect_base_branch(&self, repo: &Path) -> Result<String, BoardError> {
        let git = self.git()?;
        if let Some(branch) = git.origin_head(repo).await? {
            return Ok(branch);
        }
        git.current_branch(repo).await?.ok_or_else(|| {
            BoardError::refused(
                Refusal::NoBaseBranch,
                "The repository has no remote default branch and no branch checked out; check out a branch first",
            )
        })
    }
}

fn non_empty_branch(branch: &str) -> Result<String, BoardError> {
    let branch = branch.trim();
    if branch.is_empty() {
        Err(BoardError::refused(
            Refusal::NoBaseBranch,
            "The base branch can't be empty",
        ))
    } else {
        Ok(branch.to_owned())
    }
}

/// The number of letters a suggested key takes from the repository name.
const SUGGESTED_KEY_LEN: usize = 4;

/// Suggests a project key from a repository name (d0029): the first word's first four letters, uppercase
/// (`shop-web` → `SHOP`, `billing-api` → `BILL`). If that key is taken, the words' initials (`SW`), then the
/// first suggestion with a number (`SHOP2`, `SHOP3`, …).
pub fn suggest_key(name: &str, taken: &[String]) -> String {
    let words: Vec<String> = name
        .split(|c: char| !c.is_ascii_alphanumeric())
        .map(|w| w.trim_start_matches(|c: char| c.is_ascii_digit()))
        .filter(|w| !w.is_empty())
        .map(str::to_ascii_uppercase)
        .collect();
    let first: String = words
        .first()
        .map(|w| w.chars().take(SUGGESTED_KEY_LEN).collect())
        .unwrap_or_else(|| "PROJ".to_owned());
    let initials: String = words.iter().filter_map(|w| w.chars().next()).collect();
    let is_free = |k: &str| !taken.iter().any(|t| t == k);
    if is_free(&first) {
        return first;
    }
    if initials.len() >= 2 && is_free(&initials) {
        return initials;
    }
    (2..)
        .map(|n| format!("{first}{n}"))
        .find(|k| is_free(k))
        .expect("some number is free")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn taken(keys: &[&str]) -> Vec<String> {
        keys.iter().map(|k| (*k).to_owned()).collect()
    }

    #[test]
    fn suggestions_from_repo_names() {
        assert_eq!(suggest_key("shop-web", &[]), "SHOP");
        assert_eq!(suggest_key("billing-api", &[]), "BILL");
        // A short one-word name is its own key: `otaq` → `OTAQ`.
        assert_eq!(
            suggest_key(crate::naming::COMMAND_NAME, &[]),
            crate::naming::COMMAND_NAME.to_uppercase()
        );
        assert_eq!(suggest_key("api", &[]), "API");
        assert_eq!(suggest_key("my_cool.app", &[]), "MY");
        assert_eq!(suggest_key("2048-game", &[]), "GAME");
        assert_eq!(suggest_key("---", &[]), "PROJ");
    }

    #[test]
    fn collisions_get_a_distinct_suggestion() {
        assert_eq!(suggest_key("shop-web", &taken(&["SHOP"])), "SW");
        assert_eq!(suggest_key("shop-web", &taken(&["SHOP", "SW"])), "SHOP2");
        assert_eq!(suggest_key("shopify", &taken(&["SHOP", "SHOP2"])), "SHOP3");
        let own_key = crate::naming::COMMAND_NAME.to_uppercase();
        for (name, keys) in [
            ("shop-web", &["SHOP"][..]),
            (crate::naming::COMMAND_NAME, &[own_key.as_str()][..]),
        ] {
            let key = suggest_key(name, &taken(keys));
            assert!(ProjectKey::new(&key).is_ok(), "{key}");
            assert!(!keys.contains(&key.as_str()));
        }
    }
}
