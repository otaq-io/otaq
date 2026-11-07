//! A project's flow, stored (FLW-01, d0108): every project has an **applied flow** (the YAML document of STG-01, its
//! agents and its stages) and keeps every flow it applied before, numbered from 1, with who applied it and when. A
//! project gets the shipped `otaq-default.yaml` as version 1 when it is added. The board runs the current version
//! ([`Board::effective_flow`]); storing a new version makes it current ([`Board::store_flow`]). Rows are never changed
//! once written, except which one is current, so "Back to the previous flow" is a new version too, and except an
//! agent's rename: a flow names agents by name and resolves them by name, so renaming an agent renames it in every
//! stored version (AGM-01: "a rename changes nothing").
//!
//! [`Board::effective_flow`] is how everything else reads a project's flow; [`Board::current_flow_file`] adds its
//! agents; [`Board::current_flow`] and [`Board::flow_versions`] are the stored rows (`flow.current`,
//! `flow.versions`).

use serde::{Deserialize, Serialize};
use sqlx::{Sqlite, Transaction};

use super::model::{Flow, FlowFile};
use super::yaml;
use crate::board::{Board, BoardError};
use crate::domain::stored_enum;
use crate::naming;
use crate::store::{Store, StoreError, now_ms};

stored_enum! {
    /// Who applied a flow version ("Changed by Noyan · yesterday").
    pub enum AppliedBy ("applied by") {
        /// Nobody: the shipped default flow a project gets when it is added (version 1).
        Shipped = "shipped",
        /// The user (the flow editor's Apply, a test).
        User = "user",
        /// A Project agent's proposal the user applied; its name is [`FlowVersion::agent_name`].
        ProjectAgent = "project_agent",
    }
}

/// One stored version of a project's flow.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FlowVersion {
    /// The project's key.
    pub project: String,
    /// 1 for the flow the project got when it was added, then 2, 3… in the order they were applied.
    pub version: u32,
    /// The flow document as applied.
    pub yaml: String,
    pub applied_by: AppliedBy,
    /// The Project agent's name when it proposed it.
    pub agent_name: Option<String>,
    /// The shipped preset file it came from (`otaq-default.yaml`), if it came from one.
    pub preset: Option<String>,
    /// When it was applied, in Unix milliseconds.
    pub applied_at: i64,
    /// The flow the board runs now.
    pub current: bool,
}

impl FlowVersion {
    /// The document, read. Only documents that read are stored ([`Board::store_flow`]).
    pub fn file(&self) -> Result<FlowFile, yaml::FormatError> {
        yaml::read(&self.yaml)
    }
}

/// A version to store: the document and who applied it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewFlowVersion {
    pub yaml: String,
    pub applied_by: AppliedBy,
    pub agent_name: Option<String>,
    pub preset: Option<String>,
}

impl NewFlowVersion {
    /// The shipped default flow, as Otaq gives it to a new project.
    pub fn default_for_new_project() -> Self {
        Self {
            yaml: naming::DEFAULT_FLOW_YAML.to_owned(),
            applied_by: AppliedBy::Shipped,
            agent_name: None,
            preset: Some(naming::DEFAULT_FLOW_FILE.to_owned()),
        }
    }
}

type Row = (
    String,
    i64,
    String,
    String,
    Option<String>,
    Option<String>,
    i64,
    bool,
);

/// The columns of a [`Row`], from `project_flows f` joined with `projects p`.
macro_rules! select {
    ($rest:literal) => {
        concat!(
            "SELECT p.key, f.version, f.yaml, f.applied_by, f.agent_name, f.preset, f.applied_at, f.current
             FROM project_flows f JOIN projects p ON p.id = f.project_id ",
            $rest
        )
    };
}

fn version_of(row: Row) -> Result<FlowVersion, StoreError> {
    let (project, version, yaml, applied_by, agent_name, preset, applied_at, current) = row;
    Ok(FlowVersion {
        project,
        version: u32::try_from(version).unwrap_or(0),
        yaml,
        applied_by: applied_by
            .parse()
            .map_err(|e| StoreError::Database(sqlx::Error::Decode(Box::new(e))))?,
        agent_name,
        preset,
        applied_at,
        current,
    })
}

impl Store {
    /// The current flow version of the project with this key; `None` for an unknown project.
    pub async fn current_flow(&self, project_key: &str) -> Result<Option<FlowVersion>, StoreError> {
        let row: Option<Row> = sqlx::query_as(select!("WHERE p.key = ? AND f.current"))
            .bind(project_key)
            .fetch_optional(self.pool())
            .await?;
        row.map(version_of).transpose()
    }

    /// Every flow version of the project with this key, oldest first.
    pub async fn flow_versions(&self, project_key: &str) -> Result<Vec<FlowVersion>, StoreError> {
        let rows: Vec<Row> = sqlx::query_as(select!("WHERE p.key = ? ORDER BY f.version"))
            .bind(project_key)
            .fetch_all(self.pool())
            .await?;
        rows.into_iter().map(version_of).collect()
    }

    /// Stores `new` as the project's next version and makes it the current one. Its number.
    pub(crate) async fn insert_flow_version_in(
        tx: &mut Transaction<'_, Sqlite>,
        project_id: &str,
        new: &NewFlowVersion,
    ) -> Result<u32, StoreError> {
        let version: i64 = sqlx::query_scalar(
            "SELECT COALESCE(MAX(version), 0) + 1 FROM project_flows WHERE project_id = ?",
        )
        .bind(project_id)
        .fetch_one(&mut **tx)
        .await?;
        sqlx::query("UPDATE project_flows SET current = FALSE WHERE project_id = ? AND current")
            .bind(project_id)
            .execute(&mut **tx)
            .await?;
        sqlx::query(
            "INSERT INTO project_flows (project_id, version, yaml, applied_by, agent_name, preset, applied_at,
                                        current)
             VALUES (?, ?, ?, ?, ?, ?, ?, TRUE)",
        )
        .bind(project_id)
        .bind(version)
        .bind(&new.yaml)
        .bind(new.applied_by.as_str())
        .bind(&new.agent_name)
        .bind(&new.preset)
        .bind(now_ms())
        .execute(&mut **tx)
        .await?;
        carry_splits_in(tx, project_id, version, &new.yaml).await?;
        Ok(u32::try_from(version).unwrap_or(0))
    }
}

/// The `settings` key of the flow versions whose split checks the project's tickets were carried over to
/// ([`carry_splits_in`]): a JSON list of `[project id, version]`.
const SPLITS_CARRIED: &str = "flow_splits_carried";

/// When the version `version` of the project `project_id` (its document `text`) splits a check that listed several
/// agents ([`yaml::Split`]), carries the project's tickets over to its copies, once per version: a ticket's skip (or
/// its usual choice) on the stage applies to every copy, and so does its "moved past" (the counter's row, copied); a
/// ticket's chosen agent stays on the stage, its first copy; every copy's rounds start at 0 (the stage's fails in a
/// row and extra rounds go, its last pass stays as a record). A ticket in the middle of the stage's round is not
/// touched here: the upgrade restarts the service, which makes it Interrupted, and it restarts on the stage, its
/// first copy.
async fn carry_splits_in(
    tx: &mut Transaction<'_, Sqlite>,
    project_id: &str,
    version: i64,
    text: &str,
) -> Result<(), StoreError> {
    let Ok((_, splits)) = yaml::read_split(text) else {
        return Ok(());
    };
    if splits.is_empty() {
        return Ok(());
    }
    let stored: Option<String> =
        sqlx::query_scalar("SELECT value_json FROM settings WHERE key = ?")
            .bind(SPLITS_CARRIED)
            .fetch_optional(&mut **tx)
            .await?;
    let mut carried: Vec<(String, i64)> = stored
        .and_then(|json| serde_json::from_str(&json).ok())
        .unwrap_or_default();
    let this = (project_id.to_owned(), version);
    if carried.contains(&this) {
        return Ok(());
    }
    for split in &splits {
        for copy in &split.copies {
            sqlx::query(
                "INSERT OR IGNORE INTO ticket_stage_choices
                     (ticket_id, stage_id, choice, agent_id, set_by, by_name, reason, set_at)
                 SELECT c.ticket_id, ?, c.choice, c.agent_id, c.set_by, c.by_name, c.reason, c.set_at
                 FROM ticket_stage_choices c JOIN tickets t ON t.id = c.ticket_id
                 WHERE t.project_id = ? AND c.stage_id = ? AND c.choice IN ('skip', 'usual')",
            )
            .bind(copy)
            .bind(project_id)
            .bind(&split.stage)
            .execute(&mut **tx)
            .await?;
            sqlx::query(
                "INSERT OR IGNORE INTO check_counters
                     (ticket_id, stage_id, fails, extra, passed_round, moved_past, updated_at)
                 SELECT c.ticket_id, ?, c.fails, c.extra, c.passed_round, c.moved_past, c.updated_at
                 FROM check_counters c JOIN tickets t ON t.id = c.ticket_id
                 WHERE t.project_id = ? AND c.stage_id = ? AND c.moved_past IS NOT NULL",
            )
            .bind(copy)
            .bind(project_id)
            .bind(&split.stage)
            .execute(&mut **tx)
            .await?;
        }
        sqlx::query(
            "UPDATE check_counters SET fails = 0, extra = 0
             WHERE stage_id = ? AND moved_past IS NULL
               AND ticket_id IN (SELECT id FROM tickets WHERE project_id = ?)",
        )
        .bind(&split.stage)
        .bind(project_id)
        .execute(&mut **tx)
        .await?;
    }
    carried.push(this);
    sqlx::query(
        "INSERT INTO settings (key, value_json, updated_at) VALUES (?, ?, ?)
         ON CONFLICT (key) DO UPDATE SET value_json = excluded.value_json, updated_at = excluded.updated_at",
    )
    .bind(SPLITS_CARRIED)
    .bind(serde_json::to_string(&carried).unwrap_or_else(|_| "[]".to_owned()))
    .bind(now_ms())
    .execute(&mut **tx)
    .await?;
    Ok(())
}

/// Renames the agent `old` (ignoring case, as agent names are compared) to `new` in `file`: its entry in `agents:`,
/// the stages it runs and the stage options naming it. Whether anything changed.
pub fn rename_agent(file: &mut FlowFile, old: &str, new: &str) -> bool {
    let same = |name: &str| name.to_lowercase() == old.to_lowercase();
    let mut changed = false;
    let mut rename = |name: &mut String| {
        if same(name) && name != new {
            *name = new.to_owned();
            changed = true;
        }
    };
    for agent in &mut file.agents {
        rename(&mut agent.name);
    }
    for stage in &mut file.flow.stages {
        if let Some(agent) = &mut stage.agent {
            rename(agent);
        }
    }
    changed
}

impl Store {
    /// Renames the agent `old` to `new` in every stored flow version that names it ([`rename_agent`]), rewriting
    /// those documents in Otaq's layout.
    pub(crate) async fn rename_agent_in_flows_in(
        tx: &mut Transaction<'_, Sqlite>,
        old: &str,
        new: &str,
    ) -> Result<(), StoreError> {
        let rows: Vec<(String, i64, String)> =
            sqlx::query_as("SELECT project_id, version, yaml FROM project_flows")
                .fetch_all(&mut **tx)
                .await?;
        for (project_id, version, text) in rows {
            let Ok(mut file) = yaml::read(&text) else {
                continue;
            };
            if !rename_agent(&mut file, old, new) {
                continue;
            }
            sqlx::query("UPDATE project_flows SET yaml = ? WHERE project_id = ? AND version = ?")
                .bind(yaml::write(&file))
                .bind(&project_id)
                .bind(version)
                .execute(&mut **tx)
                .await?;
        }
        Ok(())
    }
}

impl Board {
    /// The flow the project (by key) runs now: its current stored version. Read on every call, so an applied change
    /// reaches the next read; each version is read from its YAML once.
    pub async fn effective_flow(&self, project: &str) -> Result<Flow, BoardError> {
        Ok(self.current_flow_file(project).await?.flow)
    }

    /// The project's current flow document, read (its agents and its stages).
    pub async fn current_flow_file(&self, project: &str) -> Result<FlowFile, BoardError> {
        let version = self.current_flow(project).await?;
        if let Some(file) = self.flows.lock().expect("flows").get(&version.yaml) {
            return Ok(file.clone());
        }
        let file = version.file().map_err(|e| {
            BoardError::NotFound(format!(
                "{}'s flow (version {}) doesn't read: {e}",
                version.project, version.version
            ))
        })?;
        self.flows
            .lock()
            .expect("flows")
            .insert(version.yaml, file.clone());
        Ok(file)
    }

    /// Carries every project's tickets over to the copies of its current flow's split checks ([`yaml::Split`]),
    /// once per version: what a flow stored before a check had one agent needs when this build first runs it. The
    /// service does it as it starts; versions stored after are carried as they are stored.
    pub async fn carry_split_checks(&self) -> Result<(), BoardError> {
        let rows: Vec<(String, i64, String)> =
            sqlx::query_as("SELECT project_id, version, yaml FROM project_flows WHERE current")
                .fetch_all(self.store().pool())
                .await?;
        for (project_id, version, text) in rows {
            if !text.contains("agents:") {
                continue;
            }
            let mut tx = self.store().begin_write().await?;
            carry_splits_in(&mut tx, &project_id, version, &text).await?;
            tx.commit().await?;
        }
        Ok(())
    }

    /// `flow.current`: the project's current flow version.
    pub async fn current_flow(&self, project: &str) -> Result<FlowVersion, BoardError> {
        self.store()
            .current_flow(project)
            .await?
            .ok_or_else(|| BoardError::NotFound(format!("unknown project {project}")))
    }

    /// `flow.versions`: every flow the project applied, oldest first; the last is the current one.
    pub async fn flow_versions(&self, project: &str) -> Result<Vec<FlowVersion>, BoardError> {
        let project = self.project(project).await?;
        Ok(self.store().flow_versions(&project.key).await?)
    }

    /// Stores `new` as the project's next flow version and makes it the one the board runs. `new.yaml` must read
    /// as a flow (what it says is checked before, by whoever proposes it), and every stage an agent runs has its
    /// skill: only the draft holds one without (FED-06). The stored version.
    pub async fn store_flow(
        &self,
        project: &str,
        new: &NewFlowVersion,
    ) -> Result<FlowVersion, BoardError> {
        let file = yaml::read(&new.yaml).map_err(|e| {
            BoardError::refused(
                crate::board::Refusal::BadProposal,
                format!("The flow doesn't read: {e}"),
            )
        })?;
        if let Some(stage) = file.flow.stages.iter().find(|s| s.needs_skill()) {
            return Err(BoardError::refused(
                crate::board::Refusal::BadProposal,
                super::check::needs_skill_message(stage),
            ));
        }
        let project = self.project(project).await?;
        let mut tx = self.store().begin_write().await?;
        Store::insert_flow_version_in(&mut tx, &project.id, new).await?;
        tx.commit().await?;
        self.current_flow(&project.key).await
    }

    /// Tests only: puts the project `key` on `file`'s flow (the test flow, `tests/flows/every-kind.yaml`) as a new
    /// version applied by the user, and creates the file's agents that aren't there yet ("Added by <key>'s flow").
    #[doc(hidden)]
    pub async fn use_flow_for_tests(&self, key: &str, file: &FlowFile) {
        let store = self.store();
        for agent in &file.agents {
            if store
                .agent_named(&agent.name)
                .await
                .expect("agents read")
                .is_some()
            {
                continue;
            }
            let new = crate::agents::shipped::new_agent(agent);
            store
                .insert_agent(
                    &new,
                    &crate::agents::CameFrom::Flow {
                        project: key.to_owned(),
                    },
                    None,
                )
                .await
                .expect("a test flow's agent added");
        }
        self.store_flow(
            key,
            &NewFlowVersion {
                yaml: yaml::write(file),
                applied_by: AppliedBy::User,
                agent_name: None,
                preset: None,
            },
        )
        .await
        .expect("a test flow stored");
    }

    /// Tests only: stores the project's current flow changed by `edit` as a new version (the default flow without
    /// Validate, with 5 rounds, with an automatic local Delivery).
    #[doc(hidden)]
    pub async fn edit_flow_for_tests(&self, key: &str, edit: impl FnOnce(&mut FlowFile)) {
        let mut file = self
            .current_flow_file(key)
            .await
            .expect("the project's flow");
        edit(&mut file);
        self.store_flow(
            key,
            &NewFlowVersion {
                yaml: yaml::write(&file),
                applied_by: AppliedBy::User,
                agent_name: None,
                preset: None,
            },
        )
        .await
        .expect("a test flow stored");
    }
}
