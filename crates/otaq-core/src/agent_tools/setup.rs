//! The tool-neutral side of the one-time setup (ADP-01, PRJ-06, PRJ-08; d0085, d0086, d0088): what recording an
//! added project's folder trust did, the projects whose trust couldn't be recorded, and the service's [`Trust`].
//!
//! [`Trust`] records the trust for **each tool in the registry** ([`super::AgentTool::record_project`]) when a
//! project is added and, in a startup pass, for the projects added earlier (PRJ-08), with the one-time note and the
//! projects whose trust couldn't be recorded; and for one tool when it becomes Connected ([`Trust::record_existing`],
//! d0086), with the note naming it, once per tool. How a tool's own config is written is its
//! adapter's (Claude Code: [`super::claude::trust`]).

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};

use serde::{Deserialize, Serialize};
use tokio::sync::watch;

use super::connected::{store_tools, stored_tools};
use super::{Adapters, Tool};
use crate::store::settings::{
    TRUST_NOTE_DISMISSED, TRUST_NOTE_PENDING, TRUST_NOTE_RAISED, TRUST_NOTE_TOOLS,
};
use crate::store::{Store, StoreError};

/// What recording one project's folder trust did for one tool ([`super::AgentTool::record_project`]).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SetupOutcome {
    /// The tool already trusted the repo; its config wasn't touched.
    AlreadyTrusted,
    /// The trust was written.
    Recorded,
    /// The trust couldn't be recorded. `reason` is a short sentence shown as it is (no final period).
    Failed { reason: String },
}

impl SetupOutcome {
    /// One outcome for several tools, for the add-project line: the first failure, else `Recorded` when any tool
    /// recorded it, else `AlreadyTrusted`. `None` when no tool keeps folder trust. With one tool, its outcome.
    fn combined(outcomes: impl IntoIterator<Item = Self>) -> Option<Self> {
        outcomes.into_iter().reduce(|a, b| match (a, b) {
            (failed @ Self::Failed { .. }, _) | (_, failed @ Self::Failed { .. }) => failed,
            (Self::Recorded, _) | (_, Self::Recorded) => Self::Recorded,
            (Self::AlreadyTrusted, Self::AlreadyTrusted) => Self::AlreadyTrusted,
        })
    }
}

/// A project whose folder trust couldn't be recorded by one tool: its warning banner (PRJ-08).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrustFailure {
    pub project_key: String,
    /// The tool whose config couldn't be written, so a banner can name it.
    pub tool: Tool,
    /// A short sentence shown as it is (no final period).
    pub reason: String,
}

/// The service's folder trust (PRJ-06, PRJ-08): the registry handed over once at service start, and the projects
/// whose trust couldn't be recorded (each project's and tool's latest outcome, in memory). The board's clones share
/// it; before the registry is set (and for a tool whose config can't be located) nothing is read or written.
#[derive(Clone, Default)]
pub struct Trust {
    inner: Arc<TrustInner>,
}

#[derive(Default)]
struct TrustInner {
    adapters: OnceLock<Adapters>,
    /// (Project key, tool) → why its trust couldn't be recorded.
    failed: Mutex<BTreeMap<(String, Tool), String>>,
    /// (Project key, tool) whose latest attempt recorded the trust or found it there: Otaq's own record, for
    /// `otaq uninstall`'s summary (d0134), never read back from the tools' files.
    trusted: Mutex<BTreeSet<(String, Tool)>>,
    /// Set once the startup pass is over.
    pass_done: watch::Sender<bool>,
}

impl std::fmt::Debug for Trust {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Trust")
            .field("adapters", &self.inner.adapters.get().is_some())
            .field("failed", &self.failed())
            .finish()
    }
}

impl Trust {
    /// The registry whose tools record folder trust. Set once, at service start; a second call changes nothing.
    pub fn set_adapters(&self, adapters: Adapters) {
        let _ = self.inner.adapters.set(adapters);
    }

    /// Records the trust for a project's repo with every tool, off the async threads: one outcome for the
    /// add-project line ([`SetupOutcome::combined`]), `None` when no tool keeps it. A `Failed` outcome lists the
    /// project until a later attempt records it.
    pub async fn record(&self, project_key: &str, repo_path: &str) -> Option<SetupOutcome> {
        let adapters = self.inner.adapters.get()?;
        let mut outcomes = Vec::new();
        for &tool in Tool::ALL {
            if let Some(outcome) = self
                .record_one(adapters, tool, project_key, repo_path)
                .await
            {
                outcomes.push(outcome);
            }
        }
        SetupOutcome::combined(outcomes)
    }

    async fn record_one(
        &self,
        adapters: &Adapters,
        tool: Tool,
        project_key: &str,
        repo_path: &str,
    ) -> Option<SetupOutcome> {
        let name = adapters.get(tool).display_name();
        let (adapters, repo) = (adapters.clone(), PathBuf::from(repo_path));
        let outcome = tokio::task::spawn_blocking(move || adapters.get(tool).record_project(&repo))
            .await
            .unwrap_or_else(|e| {
                tracing::warn!(project = project_key, error = %e, "recording {name}'s trust stopped");
                Some(SetupOutcome::Failed {
                    reason: format!("{name}'s config file couldn't be written"),
                })
            })?;
        let mut failed = self.inner.failed.lock().unwrap_or_else(|e| e.into_inner());
        let mut trusted = self.inner.trusted.lock().unwrap_or_else(|e| e.into_inner());
        let key = (project_key.to_owned(), tool);
        match &outcome {
            SetupOutcome::Failed { reason } => {
                trusted.remove(&key);
                failed.insert(key, reason.clone());
            }
            SetupOutcome::AlreadyTrusted | SetupOutcome::Recorded => {
                failed.remove(&key);
                trusted.insert(key);
            }
        };
        Some(outcome)
    }

    /// The projects whose latest attempt failed, by key, then tool.
    pub fn failed(&self) -> Vec<TrustFailure> {
        self.inner
            .failed
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .iter()
            .map(|((key, tool), reason)| TrustFailure {
                project_key: key.clone(),
                tool: *tool,
                reason: reason.clone(),
            })
            .collect()
    }

    /// The projects each tool trusts as far as this service recorded it (at Add project, the startup pass, a tool
    /// connecting): (project key, tool) pairs whose latest attempt recorded the trust or found it there, by key, then
    /// tool. Nothing is read from the tools' files.
    pub fn trusted(&self) -> Vec<(String, Tool)> {
        self.inner
            .trusted
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .iter()
            .cloned()
            .collect()
    }

    /// The startup pass (PRJ-08): records the trust for every stored project, oldest first. A repo folder that is
    /// gone is skipped; the first pass that records anything sets the one-time note, unless it was ever dismissed.
    /// Never fails: every problem is an outcome or a log line. True when the notices changed (the note was set or a
    /// project failed).
    pub async fn startup_pass(&self, store: &Store) -> bool {
        let changed = self.pass(store).await;
        self.inner.pass_done.send_replace(true);
        changed
    }

    /// Resolves once the startup pass is over.
    pub async fn pass_finished(&self) {
        let mut done = self.inner.pass_done.subscribe();
        let _ = done.wait_for(|d| *d).await;
    }

    async fn pass(&self, store: &Store) -> bool {
        let Some(adapters) = self.inner.adapters.get() else {
            tracing::debug!("no agent tools: folder trust not checked");
            return false;
        };
        let Some(projects) = existing_projects(store).await else {
            return false;
        };
        let mut recorded = Vec::new();
        for project in &projects {
            for &tool in Tool::ALL {
                let outcome = self
                    .record_one(adapters, tool, &project.key, &project.repo_path)
                    .await;
                if outcome == Some(SetupOutcome::Recorded) && !recorded.contains(&tool) {
                    recorded.push(tool);
                }
            }
        }
        let note = !recorded.is_empty() && set_note_pending(store, &recorded).await;
        note || !self.failed().is_empty()
    }

    /// A tool just connected (d0086): records its trust for every stored project, oldest first, skipping folders
    /// that are gone, and raises the one-time note naming it, once per tool, when it recorded any. True when the
    /// notices changed (the note was raised, or the projects whose trust failed changed).
    pub async fn record_existing(&self, store: &Store, tool: Tool) -> bool {
        let Some(adapters) = self.inner.adapters.get() else {
            return false;
        };
        let Some(projects) = existing_projects(store).await else {
            return false;
        };
        let failed_before = self.failed();
        let mut recorded = false;
        for project in &projects {
            let outcome = self
                .record_one(adapters, tool, &project.key, &project.repo_path)
                .await;
            recorded |= outcome == Some(SetupOutcome::Recorded);
        }
        let note = recorded && raise_note(store, tool).await;
        note || self.failed() != failed_before
    }
}

/// The stored projects whose repo folder is still there, oldest first; `None` when they can't be listed.
async fn existing_projects(store: &Store) -> Option<Vec<crate::store::projects::Project>> {
    let projects = match store.projects().await {
        Ok(projects) => projects,
        Err(e) => {
            tracing::warn!(error = %e, "can't list the projects to record their folder trust");
            return None;
        }
    };
    let mut existing = Vec::new();
    for project in projects {
        let is_dir = tokio::fs::metadata(&project.repo_path)
            .await
            .is_ok_and(|m| m.is_dir());
        if is_dir {
            existing.push(project);
        } else {
            tracing::info!(
                project = %project.key,
                repo = %project.repo_path,
                "the project's folder is gone: its folder trust not recorded"
            );
        }
    }
    Some(existing)
}

/// Sets the one-time note, naming `tools`, unless it was ever dismissed; true when it is set.
async fn set_note_pending(store: &Store, tools: &[Tool]) -> bool {
    let result = async {
        if setting_true(store, TRUST_NOTE_DISMISSED).await? {
            return Ok(false);
        }
        name_in_note(store, tools).await?;
        Ok::<_, StoreError>(true)
    }
    .await;
    result.unwrap_or_else(|e| {
        tracing::warn!(error = %e, "can't store the folder trust note");
        false
    })
}

/// The note for a tool connected later (d0086): raised once per tool, whatever an earlier note's dismissal; true
/// when it is raised now.
async fn raise_note(store: &Store, tool: Tool) -> bool {
    let result = async {
        let mut raised = stored_tools(store, TRUST_NOTE_RAISED).await?;
        if raised.contains(&tool) {
            return Ok(false);
        }
        raised.push(tool);
        store_tools(store, TRUST_NOTE_RAISED, &raised).await?;
        name_in_note(store, &[tool]).await?;
        Ok::<_, StoreError>(true)
    }
    .await;
    result.unwrap_or_else(|e| {
        tracing::warn!(error = %e, "can't store the folder trust note");
        false
    })
}

/// Shows the note and adds `tools` to the ones it names.
async fn name_in_note(store: &Store, tools: &[Tool]) -> Result<(), StoreError> {
    let mut named = stored_tools(store, TRUST_NOTE_TOOLS).await?;
    for tool in tools {
        if !named.contains(tool) {
            named.push(*tool);
        }
    }
    named.sort_by_key(|tool| Tool::ALL.iter().position(|t| t == tool));
    store_tools(store, TRUST_NOTE_TOOLS, &named).await?;
    store.set_setting(TRUST_NOTE_PENDING, "true").await
}

/// The tools the pending note names, in [`Tool::ALL`]'s order (d0086: "Otaq let Codex work in…").
pub async fn note_tools(store: &Store) -> Result<Vec<Tool>, StoreError> {
    if !note_pending(store).await? {
        return Ok(Vec::new());
    }
    stored_tools(store, TRUST_NOTE_TOOLS).await
}

/// Whether the one-time note shows: set by a startup pass, cleared for good by its dismissal.
pub async fn note_pending(store: &Store) -> Result<bool, StoreError> {
    setting_true(store, TRUST_NOTE_PENDING).await
}

/// The user dismissed the one-time note (`notices.dismiss_trust_note`): it never comes back.
pub async fn dismiss_note(store: &Store) -> Result<(), StoreError> {
    store.set_setting(TRUST_NOTE_DISMISSED, "true").await?;
    store_tools(store, TRUST_NOTE_TOOLS, &[]).await?;
    store.set_setting(TRUST_NOTE_PENDING, "false").await
}

async fn setting_true(store: &Store, key: &str) -> Result<bool, StoreError> {
    Ok(store.setting(key).await?.is_some_and(|v| v == "true"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn the_outcome_is_plain_data() {
        assert_eq!(
            serde_json::to_value(SetupOutcome::Recorded).unwrap(),
            json!({ "kind": "recorded" })
        );
        assert_eq!(
            serde_json::to_value(SetupOutcome::AlreadyTrusted).unwrap(),
            json!({ "kind": "already_trusted" })
        );
        assert_eq!(
            serde_json::to_value(SetupOutcome::Failed { reason: "r".into() }).unwrap(),
            json!({ "kind": "failed", "reason": "r" })
        );
    }

    #[test]
    fn several_tools_give_one_outcome() {
        use SetupOutcome::{AlreadyTrusted, Recorded};
        let failed = |r: &str| SetupOutcome::Failed { reason: r.into() };
        assert_eq!(SetupOutcome::combined([]), None);
        assert_eq!(SetupOutcome::combined([Recorded]), Some(Recorded));
        assert_eq!(
            SetupOutcome::combined([AlreadyTrusted, AlreadyTrusted]),
            Some(AlreadyTrusted)
        );
        assert_eq!(
            SetupOutcome::combined([AlreadyTrusted, Recorded]),
            Some(Recorded)
        );
        assert_eq!(
            SetupOutcome::combined([Recorded, failed("a"), failed("b")]),
            Some(failed("a"))
        );
    }

    #[test]
    fn a_failure_carries_the_tool() {
        assert_eq!(
            serde_json::to_value(TrustFailure {
                project_key: "NOTE".into(),
                tool: Tool::Claude,
                reason: "r".into(),
            })
            .unwrap(),
            json!({ "project_key": "NOTE", "tool": "claude", "reason": "r" })
        );
    }
}
