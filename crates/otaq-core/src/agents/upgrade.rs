//! The rest of migration 0128 (`0128_agents_inside_the_flow.sql`) that SQL can't write (UX-71, UX-73, UX-74): every
//! stored flow document (each project's versions, the open proposals and the waiting changes) has each stage and its
//! delivery take the model, effort and instructions of the agent it named, as it was stored (a `default` model as the
//! account's default model, `tool:model`, so nothing changes in how tickets run); two stages naming one agent each get
//! their own copy. "Noyan starts on" takes the model and effort of the agent Noyan used; its instructions are dropped.
//! The agents themselves go (`agents_before_0128`, which the migration kept for this).
//!
//! Runs once, right after the migrations ([`crate::store::Store::open`]), in one transaction that drops the kept
//! table: a start cut short before it leaves the table, and the next start runs it.

use super::{AgentModel, ModelRef, NoyanStartsOn};
use crate::flow::model::WhoDoesIt;
use crate::flow::yaml;
use crate::store::settings::{DEFAULT_MODEL, NOYAN_STARTS_ON};
use crate::store::{Store, StoreError, now_ms};

/// The table migration 0128 keeps the agents in for this.
const KEPT: &str = "agents_before_0128";
/// The setting that named the agent Noyan used (its id as JSON), gone with the agents.
const NOYAN_AGENT: &str = "agents.noyan";

type AgentRow = (
    String,
    String,
    String,
    Option<String>,
    Option<String>,
    Option<String>,
);

/// One stored agent as a stage takes it.
struct Kept {
    id: String,
    name: String,
    who: WhoDoesIt,
}

/// Moves the agents into the flows and Noyan's setting ([`self`]). Whether it ran (the kept table was there).
pub async fn move_agents_into_flows(store: &Store) -> Result<bool, StoreError> {
    let mut tx = store.begin_write().await?;
    let kept: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?)",
    )
    .bind(KEPT)
    .fetch_one(&mut *tx)
    .await?;
    if !kept {
        return Ok(false);
    }
    let default: Option<ModelRef> =
        sqlx::query_scalar::<_, String>("SELECT value_json FROM settings WHERE key = ?")
            .bind(DEFAULT_MODEL)
            .fetch_optional(&mut *tx)
            .await?
            .and_then(|json| serde_json::from_str(&json).ok());
    let rows: Vec<AgentRow> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "SELECT id, name, instructions, tool, model, effort FROM {KEPT} ORDER BY created_at, rowid"
    )))
    .fetch_all(&mut *tx)
    .await?;
    let agents: Vec<Kept> = rows
        .into_iter()
        .map(|(id, name, instructions, tool, model, effort)| {
            let own = tool.as_deref().and_then(|t| t.parse().ok());
            let model = match own {
                Some(tool) => AgentModel::Tool { tool, model }.text(),
                // `default`: the account's default model as it is now, so nothing changes; `default` again while
                // none is set (the new-session tool's default, as before).
                None => default
                    .as_ref()
                    .map_or_else(|| "default".to_owned(), ModelRef::text),
            };
            Kept {
                id,
                name,
                who: WhoDoesIt {
                    model: Some(model),
                    // An agent on `default` had none.
                    effort: effort.filter(|_| own.is_some()),
                    instructions: Some(instructions.trim().to_owned()).filter(|i| !i.is_empty()),
                },
            }
        })
        .collect();
    let named = |name: &str| {
        let folded = name.trim().to_lowercase();
        agents
            .iter()
            .find(|a| a.name.to_lowercase() == folded)
            .map(|a| a.who.clone())
    };
    // An agent no stored agent nor the file names (a document that never passed the checks): the default model.
    let fallback = default
        .as_ref()
        .map_or_else(|| "default".to_owned(), ModelRef::text);
    let fallback = WhoDoesIt::on(&fallback);
    let moved = |text: &str| -> Option<String> {
        let mut file = yaml::read_older(text, &named, Some(&fallback)).ok()?;
        // An agent only the file defined, on `default`, is on the default model as it is now too.
        if let Some(default) = &default {
            let whos = file
                .flow
                .stages
                .iter_mut()
                .map(|s| &mut s.who)
                .chain(std::iter::once(&mut file.flow.delivery.who));
            for who in whos {
                if who.model.as_deref() == Some("default") {
                    who.model = Some(default.text());
                }
            }
        }
        let written = yaml::write(&file);
        (written != text).then_some(written)
    };

    let flows: Vec<(String, i64, String)> =
        sqlx::query_as("SELECT project_id, version, yaml FROM project_flows")
            .fetch_all(&mut *tx)
            .await?;
    for (project_id, version, text) in flows {
        let Some(new) = moved(&text) else { continue };
        sqlx::query("UPDATE project_flows SET yaml = ? WHERE project_id = ? AND version = ?")
            .bind(new)
            .bind(&project_id)
            .bind(version)
            .execute(&mut *tx)
            .await?;
    }
    // A proposal of the shipped agents file brought agents only: there is nothing left to propose.
    sqlx::query("DELETE FROM flow_proposals WHERE source_json LIKE '%\"agents_file\"%'")
        .execute(&mut *tx)
        .await?;
    let proposals: Vec<(i64, String)> = sqlx::query_as("SELECT id, yaml FROM flow_proposals")
        .fetch_all(&mut *tx)
        .await?;
    for (id, text) in proposals {
        let Some(new) = moved(&text) else { continue };
        sqlx::query("UPDATE flow_proposals SET yaml = ? WHERE id = ?")
            .bind(new)
            .bind(id)
            .execute(&mut *tx)
            .await?;
    }
    let waiting: Vec<(String, String)> =
        sqlx::query_as("SELECT project_id, yaml FROM flow_waiting")
            .fetch_all(&mut *tx)
            .await?;
    for (project_id, text) in waiting {
        let Some(new) = moved(&text) else { continue };
        sqlx::query("UPDATE flow_waiting SET yaml = ? WHERE project_id = ?")
            .bind(new)
            .bind(project_id)
            .execute(&mut *tx)
            .await?;
    }

    let noyan: Option<String> =
        sqlx::query_scalar::<_, String>("SELECT value_json FROM settings WHERE key = ?")
            .bind(NOYAN_AGENT)
            .fetch_optional(&mut *tx)
            .await?
            .and_then(|json| serde_json::from_str(&json).ok());
    if let Some(agent) = noyan.and_then(|id| agents.iter().find(|a| a.id == id)) {
        let starts_on = NoyanStartsOn {
            model: agent.who.model.clone(),
            effort: agent.who.effort.clone(),
        };
        sqlx::query(
            "INSERT INTO settings (key, value_json, updated_at) VALUES (?, ?, ?)
             ON CONFLICT (key) DO UPDATE SET value_json = excluded.value_json, updated_at = excluded.updated_at",
        )
        .bind(NOYAN_STARTS_ON)
        .bind(serde_json::to_string(&starts_on).expect("a setting serializes"))
        .bind(now_ms())
        .execute(&mut *tx)
        .await?;
    }
    sqlx::query("DELETE FROM settings WHERE key = ?")
        .bind(NOYAN_AGENT)
        .execute(&mut *tx)
        .await?;
    sqlx::query(sqlx::AssertSqlSafe(format!("DROP TABLE {KEPT}")))
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    tracing::info!(
        agents = agents.len(),
        "the agents moved into the flows (migration 0128)"
    );
    Ok(true)
}
