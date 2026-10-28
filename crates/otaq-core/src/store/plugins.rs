//! The plugin tables (migration 0070, the otaq-marketplace README): marketplaces, installed plugins, each project's use of
//! one, and the tracker issues tickets were imported from.

use sqlx::FromRow;

use super::{Store, StoreError, new_id, now_ms};

#[derive(Debug, Clone, PartialEq, Eq, FromRow)]
pub struct MarketplaceRow {
    pub id: String,
    pub url: String,
    pub label: String,
    pub official: bool,
    pub index_json: Option<String>,
    pub etag: Option<String>,
    pub fetched_at: Option<i64>,
    pub fetch_error: Option<String>,
    pub added_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, FromRow)]
pub struct PluginRow {
    pub id: String,
    pub source: String,
    pub marketplace_id: Option<String>,
    pub local_path: Option<String>,
    pub version: String,
    pub manifest_json: String,
    pub sha256: Option<String>,
    pub fingerprint: String,
    pub consented: String,
    pub defaults_json: String,
    pub installed_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, FromRow)]
pub struct ProjectPluginRow {
    pub project_id: String,
    pub plugin_id: String,
    pub enabled: bool,
    pub values_json: String,
    pub auto_pull: bool,
    pub pull_minutes: i64,
    pub last_pull_at: Option<i64>,
    pub last_pull_error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, FromRow)]
pub struct LinkRow {
    pub id: String,
    /// `None` once the ticket was deleted: the issue is dismissed.
    pub ticket_id: Option<String>,
    pub project_id: String,
    pub plugin_id: String,
    /// The plugin's name when imported ("Jira"), kept for the badge after a removal.
    pub tracker: String,
    pub issue_id: String,
    pub issue_key: String,
    pub url: String,
    pub parent_key: Option<String>,
    pub parent_url: Option<String>,
    pub snapshot_title: String,
    pub snapshot_body: String,
    pub imported_at: i64,
    pub fetched_at: i64,
}

impl Store {
    pub async fn plugin_marketplaces(&self) -> Result<Vec<MarketplaceRow>, StoreError> {
        Ok(
            sqlx::query_as("SELECT * FROM plugin_marketplaces ORDER BY official DESC, added_at")
                .fetch_all(self.pool())
                .await?,
        )
    }

    pub async fn plugin_marketplace(&self, id: &str) -> Result<Option<MarketplaceRow>, StoreError> {
        Ok(
            sqlx::query_as("SELECT * FROM plugin_marketplaces WHERE id = ?")
                .bind(id)
                .fetch_optional(self.pool())
                .await?,
        )
    }

    /// Adds a marketplace, or returns the one with that URL.
    pub async fn add_plugin_marketplace(
        &self,
        url: &str,
        label: &str,
        official: bool,
    ) -> Result<MarketplaceRow, StoreError> {
        sqlx::query(
            "INSERT INTO plugin_marketplaces (id, url, label, official, added_at) VALUES (?, ?, ?, ?, ?)
             ON CONFLICT (url) DO NOTHING",
        )
        .bind(new_id())
        .bind(url)
        .bind(label)
        .bind(official)
        .bind(now_ms())
        .execute(self.pool())
        .await?;
        Ok(
            sqlx::query_as("SELECT * FROM plugin_marketplaces WHERE url = ?")
                .bind(url)
                .fetch_one(self.pool())
                .await?,
        )
    }

    /// Points the official marketplace at `url` (its address moved): the old copy of its index goes with it.
    pub async fn move_official_marketplace(
        &self,
        url: &str,
        label: &str,
    ) -> Result<(), StoreError> {
        sqlx::query(
            "UPDATE plugin_marketplaces SET url = ?, label = ?, index_json = NULL, etag = NULL, fetched_at = NULL,
             fetch_error = NULL WHERE official = 1 AND url <> ?",
        )
        .bind(url)
        .bind(label)
        .bind(url)
        .execute(self.pool())
        .await?;
        Ok(())
    }

    pub async fn remove_plugin_marketplace(&self, id: &str) -> Result<(), StoreError> {
        sqlx::query("DELETE FROM plugin_marketplaces WHERE id = ? AND official = 0")
            .bind(id)
            .execute(self.pool())
            .await?;
        Ok(())
    }

    /// Records a fetch of the marketplace's index: the new index (or `None` to keep the old one) and any error.
    pub async fn set_plugin_marketplace_fetch(
        &self,
        id: &str,
        index_json: Option<&str>,
        etag: Option<&str>,
        error: Option<&str>,
    ) -> Result<(), StoreError> {
        sqlx::query(
            "UPDATE plugin_marketplaces SET index_json = COALESCE(?, index_json), etag = COALESCE(?, etag),
             fetched_at = ?, fetch_error = ? WHERE id = ?",
        )
        .bind(index_json)
        .bind(etag)
        .bind(now_ms())
        .bind(error)
        .bind(id)
        .execute(self.pool())
        .await?;
        Ok(())
    }

    pub async fn plugins(&self) -> Result<Vec<PluginRow>, StoreError> {
        Ok(sqlx::query_as("SELECT * FROM plugins ORDER BY id")
            .fetch_all(self.pool())
            .await?)
    }

    pub async fn plugin(&self, id: &str) -> Result<Option<PluginRow>, StoreError> {
        Ok(sqlx::query_as("SELECT * FROM plugins WHERE id = ?")
            .bind(id)
            .fetch_optional(self.pool())
            .await?)
    }

    /// Stores an install or an update; the global values are kept.
    pub async fn put_plugin(&self, row: &PluginRow) -> Result<(), StoreError> {
        sqlx::query(
            "INSERT INTO plugins (id, source, marketplace_id, local_path, version, manifest_json, sha256, fingerprint,
                 consented, defaults_json, installed_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT (id) DO UPDATE SET source = excluded.source, marketplace_id = excluded.marketplace_id,
                 local_path = excluded.local_path, version = excluded.version, manifest_json = excluded.manifest_json,
                 sha256 = excluded.sha256, fingerprint = excluded.fingerprint, consented = excluded.consented,
                 updated_at = excluded.updated_at",
        )
        .bind(&row.id)
        .bind(&row.source)
        .bind(&row.marketplace_id)
        .bind(&row.local_path)
        .bind(&row.version)
        .bind(&row.manifest_json)
        .bind(&row.sha256)
        .bind(&row.fingerprint)
        .bind(&row.consented)
        .bind(&row.defaults_json)
        .bind(row.installed_at)
        .bind(row.updated_at)
        .execute(self.pool())
        .await?;
        Ok(())
    }

    pub async fn delete_plugin(&self, id: &str) -> Result<(), StoreError> {
        sqlx::query("DELETE FROM plugins WHERE id = ?")
            .bind(id)
            .execute(self.pool())
            .await?;
        Ok(())
    }

    pub async fn set_plugin_defaults(
        &self,
        id: &str,
        defaults_json: &str,
    ) -> Result<(), StoreError> {
        sqlx::query("UPDATE plugins SET defaults_json = ? WHERE id = ?")
            .bind(defaults_json)
            .bind(id)
            .execute(self.pool())
            .await?;
        Ok(())
    }

    pub async fn project_plugins(
        &self,
        project_id: &str,
    ) -> Result<Vec<ProjectPluginRow>, StoreError> {
        Ok(
            sqlx::query_as("SELECT * FROM project_plugins WHERE project_id = ? ORDER BY plugin_id")
                .bind(project_id)
                .fetch_all(self.pool())
                .await?,
        )
    }

    pub async fn project_plugin(
        &self,
        project_id: &str,
        plugin_id: &str,
    ) -> Result<Option<ProjectPluginRow>, StoreError> {
        Ok(
            sqlx::query_as("SELECT * FROM project_plugins WHERE project_id = ? AND plugin_id = ?")
                .bind(project_id)
                .bind(plugin_id)
                .fetch_optional(self.pool())
                .await?,
        )
    }

    /// Every enabled use with auto-pull on, across projects.
    pub async fn auto_pull_plugins(&self) -> Result<Vec<ProjectPluginRow>, StoreError> {
        Ok(
            sqlx::query_as("SELECT * FROM project_plugins WHERE enabled = 1 AND auto_pull = 1")
                .fetch_all(self.pool())
                .await?,
        )
    }

    pub async fn put_project_plugin(&self, row: &ProjectPluginRow) -> Result<(), StoreError> {
        sqlx::query(
            "INSERT INTO project_plugins (project_id, plugin_id, enabled, values_json, auto_pull, pull_minutes)
             VALUES (?, ?, ?, ?, ?, ?)
             ON CONFLICT (project_id, plugin_id) DO UPDATE SET enabled = excluded.enabled,
                 values_json = excluded.values_json, auto_pull = excluded.auto_pull,
                 pull_minutes = excluded.pull_minutes",
        )
        .bind(&row.project_id)
        .bind(&row.plugin_id)
        .bind(row.enabled)
        .bind(&row.values_json)
        .bind(row.auto_pull)
        .bind(row.pull_minutes)
        .execute(self.pool())
        .await?;
        Ok(())
    }

    pub async fn set_project_plugin_pull(
        &self,
        project_id: &str,
        plugin_id: &str,
        error: Option<&str>,
    ) -> Result<(), StoreError> {
        sqlx::query(
            "UPDATE project_plugins SET last_pull_at = ?, last_pull_error = ? WHERE project_id = ? AND plugin_id = ?",
        )
        .bind(now_ms())
        .bind(error)
        .bind(project_id)
        .bind(plugin_id)
        .execute(self.pool())
        .await?;
        Ok(())
    }

    pub async fn ticket_link(&self, ticket_id: &str) -> Result<Option<LinkRow>, StoreError> {
        Ok(
            sqlx::query_as("SELECT * FROM ticket_links WHERE ticket_id = ?")
                .bind(ticket_id)
                .fetch_optional(self.pool())
                .await?,
        )
    }

    /// Every link of a project: its imported tickets and its dismissed issues.
    pub async fn ticket_links(&self, project_id: &str) -> Result<Vec<LinkRow>, StoreError> {
        Ok(
            sqlx::query_as("SELECT * FROM ticket_links WHERE project_id = ? ORDER BY imported_at")
                .bind(project_id)
                .fetch_all(self.pool())
                .await?,
        )
    }

    pub async fn ticket_link_by_issue(
        &self,
        project_id: &str,
        plugin_id: &str,
        issue_id: &str,
    ) -> Result<Option<LinkRow>, StoreError> {
        Ok(sqlx::query_as(
            "SELECT * FROM ticket_links WHERE project_id = ? AND plugin_id = ? AND issue_id = ?",
        )
        .bind(project_id)
        .bind(plugin_id)
        .bind(issue_id)
        .fetch_optional(self.pool())
        .await?)
    }

    /// Stores a link; a dismissed issue imported again gets its new ticket on the same row.
    pub async fn put_ticket_link(&self, row: &LinkRow) -> Result<(), StoreError> {
        sqlx::query(
            "INSERT INTO ticket_links (id, ticket_id, project_id, plugin_id, tracker, issue_id, issue_key, url,
                 parent_key, parent_url, snapshot_title, snapshot_body, imported_at, fetched_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT (project_id, plugin_id, issue_id) DO UPDATE SET ticket_id = excluded.ticket_id,
                 tracker = excluded.tracker, issue_key = excluded.issue_key, url = excluded.url,
                 parent_key = excluded.parent_key, parent_url = excluded.parent_url,
                 snapshot_title = excluded.snapshot_title, snapshot_body = excluded.snapshot_body,
                 imported_at = excluded.imported_at, fetched_at = excluded.fetched_at",
        )
        .bind(&row.id)
        .bind(&row.ticket_id)
        .bind(&row.project_id)
        .bind(&row.plugin_id)
        .bind(&row.tracker)
        .bind(&row.issue_id)
        .bind(&row.issue_key)
        .bind(&row.url)
        .bind(&row.parent_key)
        .bind(&row.parent_url)
        .bind(&row.snapshot_title)
        .bind(&row.snapshot_body)
        .bind(row.imported_at)
        .bind(row.fetched_at)
        .execute(self.pool())
        .await?;
        Ok(())
    }

    /// After a refresh: the tracker's text becomes the snapshot.
    pub async fn set_ticket_link_snapshot(
        &self,
        id: &str,
        title: &str,
        body: &str,
    ) -> Result<(), StoreError> {
        sqlx::query(
            "UPDATE ticket_links SET snapshot_title = ?, snapshot_body = ?, fetched_at = ? WHERE id = ?",
        )
        .bind(title)
        .bind(body)
        .bind(now_ms())
        .bind(id)
        .execute(self.pool())
        .await?;
        Ok(())
    }
}
