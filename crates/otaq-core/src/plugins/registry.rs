//! The service's side of plugins (the otaq-marketplace README): installing (with consent and the open-source rules),
//! settings, search, import, refresh and auto-pull. Cheap to clone.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use tokio::sync::Mutex;

use super::bundle::{self, Bundle};
use super::checks::{self, RequirementStatus};
use super::index::{self, Fetched, Http, Index};
use super::manifest::{Capability, Manifest, PULL_QUERY, SettingSpec, otaq_version};
use super::mapping::{self, Issue};
use super::merge::{Merge, merge};
use super::runner::{RunError, RunErrorKind, Runner};
use super::template::{Template, TemplateScope, Values};
use super::values;
use crate::board::tickets::{NewTicketInput, Ticket, TicketUpdate};
use crate::board::{Board, BoardError, Refusal};
use crate::data_dir::DataDir;
use crate::domain::{Actor, TicketKey};
use crate::naming::PRODUCT_NAME;
use crate::store::plugins::{LinkRow, MarketplaceRow, PluginRow, ProjectPluginRow};
use crate::store::{Store, StoreError, new_id, now_ms};

/// The most issues a search lists (and an import or a pull takes).
pub const SEARCH_LIMIT: u32 = 100;
/// The shortest auto-pull interval.
const MIN_PULL_MINUTES: i64 = 5;

/// Where a plugin came from, as its badge says.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Label {
    /// The official marketplace: reviewed.
    Official,
    /// Another marketplace: not reviewed by the Otaq project.
    Community,
    /// A folder on disk.
    Local,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PluginsView {
    pub installed: Vec<InstalledPlugin>,
    pub marketplaces: Vec<MarketplaceView>,
    /// Plugins the marketplaces offer, newest compatible version each.
    pub available: Vec<AvailablePlugin>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InstalledPlugin {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: String,
    pub license: String,
    pub label: Label,
    /// The marketplace's name, or the folder.
    pub source: String,
    pub capabilities: Vec<Capability>,
    pub settings: Vec<SettingSpec>,
    /// The global values.
    pub defaults: Map<String, Value>,
    /// A newer compatible version in its marketplace.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub update: Option<String>,
    /// A Local folder's open-source problems (they block nothing there).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
    /// Why it can't run now: a Local folder whose manifest broke, or whose commands changed since consent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub problem: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MarketplaceView {
    pub id: String,
    pub url: String,
    pub label: String,
    pub official: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fetched_at: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AvailablePlugin {
    pub marketplace: String,
    pub label: Label,
    pub id: String,
    pub name: String,
    pub description: String,
    pub license: String,
    pub version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub installed: Option<String>,
}

/// What the consent screen shows before an install or update. `digest` comes back with the install, so what is
/// installed is what was shown.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Consent {
    pub id: String,
    pub name: String,
    pub version: String,
    pub label: Label,
    pub source: String,
    pub license: String,
    pub requires: Vec<String>,
    /// Every command line it can run, as text.
    pub commands: Vec<String>,
    pub capabilities: Vec<Capability>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_url: Option<String>,
    /// An update's changes since the installed version.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diff_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub installed: Option<String>,
    /// A Local folder's open-source problems.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
    pub digest: String,
}

/// A project's use of one installed plugin, for project settings.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProjectPlugin {
    pub plugin: String,
    pub name: String,
    pub label: Label,
    pub enabled: bool,
    pub settings: Vec<SettingSpec>,
    /// The project's own values.
    pub values: Map<String, Value>,
    /// What an empty field uses (the global value, else the manifest's default).
    pub defaults: Map<String, Value>,
    /// What's wrong with the effective values, `(key, message)`.
    pub problems: Vec<(String, String)>,
    pub can_auto_pull: bool,
    pub auto_pull: bool,
    pub pull_minutes: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_pull_at: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_pull_error: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub query_label: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchHit {
    pub issue: Issue,
    /// The ticket it was imported as.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ticket: Option<TicketKey>,
    /// Imported once, and that ticket deleted.
    #[serde(default)]
    pub dismissed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchResult {
    pub hits: Vec<SearchHit>,
    /// The tracker had at least [`SEARCH_LIMIT`]: refine the query.
    pub truncated: bool,
    /// Issues left out: containers (epics) and finished ones.
    pub hidden: usize,
}

/// An imported ticket's tracker issue, for its badge.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrackerLink {
    pub ticket_id: String,
    /// The ticket's key: `SHOP-12`.
    pub ticket: String,
    pub plugin: String,
    pub tracker: String,
    pub key: String,
    pub url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_url: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RefreshField {
    Title,
    Body,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RefreshConflict {
    pub field: RefreshField,
    pub local: String,
    pub remote: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RefreshOutcome {
    /// Fields that took the tracker's text silently.
    pub updated: Vec<RefreshField>,
    /// Fields both sides changed: the user picks.
    pub conflicts: Vec<RefreshConflict>,
}

/// A pick for a conflicting field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Pick {
    Tracker,
    Mine,
}

/// What an import made.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Imported {
    pub tickets: Vec<Ticket>,
    /// Issues left out: already imported, or not open work items.
    pub skipped: Vec<String>,
}

#[derive(Clone)]
pub struct Plugins {
    store: Store,
    data: DataDir,
    runner: Runner,
    http: Http,
    /// One command per plugin at a time, so a pull never overlaps a search on the tracker.
    locks: Arc<Mutex<BTreeMap<String, Arc<Mutex<()>>>>>,
    /// The official marketplace's address (tests: a local server).
    official_url: String,
}

fn refused(message: impl Into<String>) -> BoardError {
    BoardError::refused(Refusal::BadPlugin, message)
}

fn run_error(e: RunError) -> BoardError {
    BoardError::refused(Refusal::PluginFailed, e.to_string())
}

fn parse_map(json: &str) -> Map<String, Value> {
    serde_json::from_str(json).unwrap_or_default()
}

impl Plugins {
    pub fn new(store: Store, data: DataDir, path: std::ffi::OsString) -> Self {
        Self::with_official(store, data, path, index::OFFICIAL_URL)
    }

    pub fn with_official(
        store: Store,
        data: DataDir,
        path: std::ffi::OsString,
        official_url: &str,
    ) -> Self {
        Self {
            store,
            data,
            runner: Runner { path },
            http: Http::default(),
            locks: Arc::default(),
            official_url: official_url.to_owned(),
        }
    }

    /// The official marketplace is always listed, at this build's address (an older build may have stored another).
    async fn ensure_official(&self) -> Result<(), StoreError> {
        let rows = self.store.plugin_marketplaces().await?;
        if rows.iter().any(|m| m.official) {
            self.store
                .move_official_marketplace(&self.official_url, index::OFFICIAL_LABEL)
                .await?;
        } else {
            self.store
                .add_plugin_marketplace(&self.official_url, index::OFFICIAL_LABEL, true)
                .await?;
        }
        Ok(())
    }

    // ---- Listing and marketplaces

    pub async fn view(&self) -> Result<PluginsView, BoardError> {
        self.ensure_official().await?;
        let marketplaces = self.store.plugin_marketplaces().await?;
        let rows = self.store.plugins().await?;
        let otaq = otaq_version();
        let mut installed = Vec::new();
        for row in &rows {
            installed.push(self.installed(row, &marketplaces, &otaq).await?);
        }
        let mut available = Vec::new();
        for m in &marketplaces {
            let Some(index) = m.index_json.as_deref().and_then(|t| Index::read(t).ok()) else {
                continue;
            };
            for plugin in &index.plugins {
                let Some(newest) = plugin.newest_for(&otaq) else {
                    continue;
                };
                available.push(AvailablePlugin {
                    marketplace: m.id.clone(),
                    label: if m.official {
                        Label::Official
                    } else {
                        Label::Community
                    },
                    id: plugin.id.clone(),
                    name: plugin.name.clone(),
                    description: plugin.description.clone(),
                    license: plugin.license.clone(),
                    version: newest.version.clone(),
                    installed: rows
                        .iter()
                        .find(|r| r.id == plugin.id)
                        .map(|r| r.version.clone()),
                });
            }
        }
        Ok(PluginsView {
            installed,
            marketplaces: marketplaces
                .iter()
                .map(|m| MarketplaceView {
                    id: m.id.clone(),
                    url: m.url.clone(),
                    label: m.label.clone(),
                    official: m.official,
                    fetched_at: m.fetched_at,
                    error: m.fetch_error.clone(),
                })
                .collect(),
            available,
        })
    }

    async fn installed(
        &self,
        row: &PluginRow,
        marketplaces: &[MarketplaceRow],
        otaq: &semver::Version,
    ) -> Result<InstalledPlugin, BoardError> {
        let (manifest, problem, warnings) = match self.current(row).await {
            Ok((manifest, warnings)) => (manifest, None, warnings),
            Err(e) => (stored_manifest(row)?, Some(e.to_string()), Vec::new()),
        };
        let market = row
            .marketplace_id
            .as_ref()
            .and_then(|id| marketplaces.iter().find(|m| &m.id == id));
        let update = market
            .and_then(|m| m.index_json.as_deref())
            .and_then(|t| Index::read(t).ok())
            .and_then(|index| {
                let newest = index.plugin(&row.id)?.newest_for(otaq)?;
                let newer = semver::Version::parse(&newest.version).ok()?
                    > semver::Version::parse(&row.version).ok()?;
                newer.then(|| newest.version.clone())
            });
        Ok(InstalledPlugin {
            id: row.id.clone(),
            name: manifest.name.clone(),
            version: manifest.version.clone(),
            description: manifest.description.clone(),
            license: manifest.license.clone(),
            label: label(row, market),
            source: match (market, &row.local_path) {
                (Some(m), _) => m.label.clone(),
                (None, Some(path)) => path.clone(),
                (None, None) => "a removed marketplace".to_owned(),
            },
            capabilities: manifest.capabilities.clone(),
            settings: manifest.settings.clone(),
            defaults: parse_map(&row.defaults_json),
            update,
            warnings,
            problem,
        })
    }

    pub async fn add_marketplace(&self, url: &str) -> Result<MarketplaceView, BoardError> {
        let url = index::normalize_url(url).map_err(refused)?;
        let label = url
            .strip_prefix("https://github.com/")
            .unwrap_or(&url)
            .to_owned();
        let row = self
            .store
            .add_plugin_marketplace(&url, &label, false)
            .await?;
        self.fetch_index(&row).await;
        let row = self
            .store
            .plugin_marketplace(&row.id)
            .await?
            .ok_or_else(|| BoardError::NotFound("the marketplace".to_owned()))?;
        Ok(MarketplaceView {
            id: row.id,
            url: row.url,
            label: row.label,
            official: row.official,
            fetched_at: row.fetched_at,
            error: row.fetch_error,
        })
    }

    pub async fn remove_marketplace(&self, id: &str) -> Result<(), BoardError> {
        Ok(self.store.remove_plugin_marketplace(id).await?)
    }

    /// Reads every marketplace's index that is older than `max_age` (or never read). Failures are recorded on the
    /// marketplace; the last copy stays.
    pub async fn refresh_indexes(&self, max_age: Duration) -> Result<(), BoardError> {
        self.ensure_official().await?;
        let age = i64::try_from(max_age.as_millis()).unwrap_or(i64::MAX);
        let now = now_ms();
        for row in self.store.plugin_marketplaces().await? {
            if row
                .fetched_at
                .is_some_and(|at| now.saturating_sub(at) < age)
            {
                continue;
            }
            self.fetch_index(&row).await;
        }
        Ok(())
    }

    async fn fetch_index(&self, row: &MarketplaceRow) {
        let fetched = self.http.index(&row.url, row.etag.as_deref()).await;
        let result = match fetched {
            Ok(Fetched::New { text, etag }) => {
                self.store
                    .set_plugin_marketplace_fetch(&row.id, Some(&text), etag.as_deref(), None)
                    .await
            }
            Ok(Fetched::NotModified) => {
                self.store
                    .set_plugin_marketplace_fetch(&row.id, None, None, None)
                    .await
            }
            Err(e) => {
                tracing::warn!(marketplace = %row.url, error = %e, "plugin index not read; the last copy stays");
                self.store
                    .set_plugin_marketplace_fetch(&row.id, None, None, Some(&e))
                    .await
            }
        };
        if let Err(e) = result {
            tracing::error!(error = %e, "can't record a plugin index fetch");
        }
    }

    /// Whether any plugin came from a marketplace: only then does the daily check read indexes.
    pub async fn has_marketplace_plugins(&self) -> Result<bool, StoreError> {
        Ok(self
            .store
            .plugins()
            .await?
            .iter()
            .any(|p| p.source == "marketplace"))
    }

    // ---- Install, update, remove

    /// The consent screen for a marketplace plugin (an install, or an update to `version`, default the newest
    /// compatible one). The release is downloaded and checked now, and kept for the install.
    pub async fn consent(
        &self,
        marketplace: &str,
        id: &str,
        version: Option<&str>,
    ) -> Result<Consent, BoardError> {
        let (market, entry, bundle, manifest) = self.release(marketplace, id, version).await?;
        let installed = self.store.plugin(id).await?;
        let index = market
            .index_json
            .as_deref()
            .and_then(|t| Index::read(t).ok());
        let old_tag = installed.as_ref().and_then(|row| {
            index
                .as_ref()?
                .plugin(id)?
                .version(&row.version)
                .map(|v| v.tag.clone())
        });
        let mut consent = consent_view(
            &manifest,
            if market.official {
                Label::Official
            } else {
                Label::Community
            },
            &market.label,
            Vec::new(),
        );
        consent.source_url = index::source_url(&market.url, id, &entry.tag);
        consent.diff_url =
            old_tag.and_then(|old| index::compare_url(&market.url, &old, &entry.tag));
        consent.installed = installed.map(|r| r.version);
        let staged = self.staged_path(id, &manifest.version);
        let (bytes, _) = bundle.to_release();
        DataDir::ensure_dir(&self.data.plugin_staging_dir()).map_err(io_error)?;
        std::fs::write(&staged, bytes).map_err(io_error)?;
        Ok(consent)
    }

    /// Installs (or updates) what [`Self::consent`] showed: `digest` must be its.
    pub async fn install(
        &self,
        marketplace: &str,
        id: &str,
        version: &str,
        digest: &str,
    ) -> Result<InstalledPlugin, BoardError> {
        let market = self.marketplace(marketplace).await?;
        let staged = self.staged_path(id, version);
        let bundle = match std::fs::read(&staged) {
            Ok(bytes) => {
                let hash = bundle::sha256(&bytes);
                Bundle::read(&bytes, &hash).map_err(refused)?
            }
            Err(_) => self.release(marketplace, id, Some(version)).await?.2,
        };
        let manifest = bundle.manifest().map_err(refused)?;
        let label = if market.official {
            Label::Official
        } else {
            Label::Community
        };
        let shown = consent_view(&manifest, label, &market.label, Vec::new());
        if shown.digest != digest {
            return Err(refused(
                "The plugin isn't what the consent screen showed; open it again",
            ));
        }
        let entry_sha = self
            .index_entry(&market, id, version)?
            .map(|e| e.sha256)
            .unwrap_or_default();
        let folder = self
            .data
            .plugin_path(id)
            .map_err(|e| refused(e.to_string()))?;
        replace_folder(&self.data.plugin_staging_dir(), &folder, &bundle).map_err(io_error)?;
        let _ = std::fs::remove_file(&staged);
        let now = now_ms();
        let old = self.store.plugin(id).await?;
        self.store
            .put_plugin(&PluginRow {
                id: id.to_owned(),
                source: "marketplace".to_owned(),
                marketplace_id: Some(market.id.clone()),
                local_path: None,
                version: manifest.version.clone(),
                manifest_json: serde_json::to_string(&manifest).expect("manifests serialize"),
                sha256: Some(entry_sha),
                fingerprint: bundle.fingerprint(),
                consented: shown.digest,
                defaults_json: old
                    .as_ref()
                    .map_or_else(|| "{}".to_owned(), |o| o.defaults_json.clone()),
                installed_at: old.as_ref().map_or(now, |o| o.installed_at),
                updated_at: now,
            })
            .await?;
        let row = self.row(id).await?;
        let marketplaces = self.store.plugin_marketplaces().await?;
        self.installed(&row, &marketplaces, &otaq_version()).await
    }

    /// The consent screen for a Local folder. Its open-source problems are warnings.
    pub async fn consent_local(&self, folder: &Path) -> Result<Consent, BoardError> {
        let bundle = Bundle::from_dir(folder).map_err(refused)?;
        let manifest = bundle.manifest().map_err(refused)?;
        let warnings = bundle.open_source_problems(&manifest);
        let mut consent = consent_view(
            &manifest,
            Label::Local,
            &folder.display().to_string(),
            warnings,
        );
        consent.installed = self.store.plugin(&manifest.id).await?.map(|r| r.version);
        Ok(consent)
    }

    pub async fn add_local(
        &self,
        folder: &Path,
        digest: &str,
    ) -> Result<InstalledPlugin, BoardError> {
        let shown = self.consent_local(folder).await?;
        if shown.digest != digest {
            return Err(refused(
                "The folder changed since the consent screen; open it again",
            ));
        }
        let bundle = Bundle::from_dir(folder).map_err(refused)?;
        let manifest = bundle.manifest().map_err(refused)?;
        if let Some(old) = self.store.plugin(&manifest.id).await?
            && old.source != "local"
        {
            return Err(refused(format!(
                "{} is installed from a marketplace; remove it first",
                manifest.name
            )));
        }
        let now = now_ms();
        let old = self.store.plugin(&manifest.id).await?;
        self.store
            .put_plugin(&PluginRow {
                id: manifest.id.clone(),
                source: "local".to_owned(),
                marketplace_id: None,
                local_path: Some(folder.display().to_string()),
                version: manifest.version.clone(),
                manifest_json: serde_json::to_string(&manifest).expect("manifests serialize"),
                sha256: None,
                fingerprint: bundle.fingerprint(),
                consented: shown.digest,
                defaults_json: old
                    .as_ref()
                    .map_or_else(|| "{}".to_owned(), |o| o.defaults_json.clone()),
                installed_at: old.as_ref().map_or(now, |o| o.installed_at),
                updated_at: now,
            })
            .await?;
        let row = self.row(&manifest.id).await?;
        let marketplaces = self.store.plugin_marketplaces().await?;
        self.installed(&row, &marketplaces, &otaq_version()).await
    }

    /// Removes a plugin and every project's use of it. Imported tickets keep their links.
    pub async fn remove(&self, id: &str) -> Result<(), BoardError> {
        let row = self.row(id).await?;
        self.store.delete_plugin(id).await?;
        if row.source == "marketplace"
            && let Ok(folder) = self.data.plugin_path(id)
            && let Err(e) = std::fs::remove_dir_all(&folder)
            && e.kind() != std::io::ErrorKind::NotFound
        {
            tracing::warn!(plugin = id, error = %e, "can't remove a plugin's folder");
        }
        Ok(())
    }

    pub async fn set_defaults(
        &self,
        id: &str,
        defaults: &Map<String, Value>,
    ) -> Result<(), BoardError> {
        let row = self.row(id).await?;
        let manifest = stored_manifest(&row)?;
        let cleaned = values::clean(&manifest, defaults);
        Ok(self
            .store
            .set_plugin_defaults(id, &Value::Object(cleaned).to_string())
            .await?)
    }

    pub async fn check(&self, id: &str) -> Result<Vec<RequirementStatus>, BoardError> {
        let (manifest, _) = self.current(&self.row(id).await?).await?;
        Ok(checks::check(&manifest, &self.runner).await)
    }

    /// A command line **Log in** or **Add extension** may run in Otaq's terminal: one of the plugin's, which the
    /// user consented to.
    pub async fn terminal_command(
        &self,
        id: &str,
        requirement: &str,
        extension: bool,
    ) -> Result<Vec<String>, BoardError> {
        let (manifest, _) = self.current(&self.row(id).await?).await?;
        let r = manifest
            .requires
            .iter()
            .find(|r| r.id == requirement)
            .ok_or_else(|| {
                BoardError::NotFound(format!("{} needs no {requirement}", manifest.name))
            })?;
        let argv = if extension {
            r.extension.as_ref().map(|e| e.install.clone())
        } else {
            r.login.clone()
        };
        argv.filter(|a| !a.is_empty())
            .ok_or_else(|| refused(format!("{} has no command for that", r.name)))
    }

    // ---- Projects

    pub async fn project(&self, project_key: &str) -> Result<Vec<ProjectPlugin>, BoardError> {
        let project = self.project_row(project_key).await?;
        let uses = self.store.project_plugins(&project.id).await?;
        let mut out = Vec::new();
        for row in self.store.plugins().await? {
            let Ok(manifest) = stored_manifest(&row) else {
                continue;
            };
            let own = uses.iter().find(|u| u.plugin_id == row.id);
            let project_values = own.map(|u| parse_map(&u.values_json)).unwrap_or_default();
            let global = parse_map(&row.defaults_json);
            let defaults = values::effective(&manifest, &global, &Map::new());
            let effective = values::effective(&manifest, &global, &project_values);
            let market = match &row.marketplace_id {
                Some(id) => self.store.plugin_marketplace(id).await?,
                None => None,
            };
            out.push(ProjectPlugin {
                plugin: row.id.clone(),
                name: manifest.name.clone(),
                label: label(&row, market.as_ref()),
                enabled: own.is_some_and(|u| u.enabled),
                settings: manifest.settings.clone(),
                values: project_values,
                defaults,
                problems: values::problems(&manifest, &effective),
                can_auto_pull: manifest.has(Capability::AutoPull),
                auto_pull: own.is_some_and(|u| u.auto_pull),
                pull_minutes: own.map_or(15, |u| u.pull_minutes),
                last_pull_at: own.and_then(|u| u.last_pull_at),
                last_pull_error: own.and_then(|u| u.last_pull_error.clone()),
                query_label: manifest.query_label.clone(),
            });
        }
        Ok(out)
    }

    pub async fn set_project(
        &self,
        project_key: &str,
        plugin: &str,
        enabled: bool,
        project_values: &Map<String, Value>,
        auto_pull: bool,
        pull_minutes: i64,
    ) -> Result<(), BoardError> {
        let project = self.project_row(project_key).await?;
        let row = self.row(plugin).await?;
        let manifest = stored_manifest(&row)?;
        let cleaned = values::clean(&manifest, project_values);
        if auto_pull && !manifest.has(Capability::AutoPull) {
            return Err(refused(format!("{} can't auto-pull", manifest.name)));
        }
        if enabled {
            let effective = values::effective(&manifest, &parse_map(&row.defaults_json), &cleaned);
            if let Some((_, message)) = values::problems(&manifest, &effective).into_iter().next() {
                return Err(BoardError::refused(Refusal::EmptyField, message));
            }
        }
        Ok(self
            .store
            .put_project_plugin(&ProjectPluginRow {
                project_id: project.id,
                plugin_id: plugin.to_owned(),
                enabled,
                values_json: Value::Object(cleaned).to_string(),
                auto_pull,
                pull_minutes: pull_minutes.max(MIN_PULL_MINUTES),
                last_pull_at: None,
                last_pull_error: None,
            })
            .await?)
    }

    /// The plugins a project has turned on, `(id, name)`: the board's Import menu.
    pub async fn enabled(&self, project_key: &str) -> Result<Vec<(String, String)>, BoardError> {
        Ok(self
            .project(project_key)
            .await?
            .into_iter()
            .filter(|p| p.enabled)
            .map(|p| (p.plugin, p.name))
            .collect())
    }

    /// Every imported ticket's tracker issue in a project, for the badges.
    pub async fn links(
        &self,
        board: &Board,
        project_key: &str,
    ) -> Result<Vec<TrackerLink>, BoardError> {
        let project = self.project_row(project_key).await?;
        let mut out = Vec::new();
        for l in self.store.ticket_links(&project.id).await? {
            let Some(ticket_id) = l.ticket_id else {
                continue;
            };
            let Ok(ticket) = board.ticket_by_id(&ticket_id).await else {
                continue;
            };
            out.push(TrackerLink {
                ticket_id,
                ticket: ticket.key.to_string(),
                plugin: l.plugin_id,
                tracker: l.tracker,
                key: l.issue_key,
                url: l.url,
                parent_key: l.parent_key,
                parent_url: l.parent_url,
            });
        }
        Ok(out)
    }

    // ---- Search, import, refresh

    pub async fn search(
        &self,
        board: &Board,
        project_key: &str,
        plugin: &str,
        query: Option<&str>,
    ) -> Result<SearchResult, BoardError> {
        let ready = self.ready(project_key, plugin).await?;
        let query = query
            .map(str::trim)
            .filter(|q| !q.is_empty())
            .map(str::to_owned)
            .or_else(|| {
                ready
                    .values
                    .get(PULL_QUERY)
                    .and_then(Value::as_str)
                    .map(str::to_owned)
            })
            .ok_or_else(|| BoardError::refused(Refusal::EmptyField, "Type a search"))?;
        let _turn = self.turn(plugin).await;
        let output = self.run_search(&ready, &query).await?;
        let mut issues =
            mapping::search_results(&ready.manifest, &output, &ready.values).map_err(refused)?;
        // A CLI with no limit option (`az boards query`) is cut here.
        let truncated = issues.len() >= SEARCH_LIMIT as usize;
        issues.truncate(SEARCH_LIMIT as usize);
        let links = self.store.ticket_links(&ready.project.id).await?;
        let mut hits = Vec::new();
        let mut hidden = 0;
        for issue in issues {
            if !issue.importable() {
                hidden += 1;
                continue;
            }
            let link = links
                .iter()
                .find(|l| l.plugin_id == plugin && l.issue_id == issue.id);
            let ticket = match link.and_then(|l| l.ticket_id.as_deref()) {
                Some(id) => board.ticket_by_id(id).await.ok().map(|t| t.key),
                None => None,
            };
            hits.push(SearchHit {
                dismissed: link.is_some() && ticket.is_none(),
                ticket,
                issue,
            });
        }
        Ok(SearchResult {
            hits,
            truncated,
            hidden,
        })
    }

    /// Imports issues by id as Backlog tickets, all or nothing. Already-imported issues and ones that aren't open
    /// work items are skipped; a dismissed issue is imported again (the user picked it).
    pub async fn import(
        &self,
        board: &Board,
        project_key: &str,
        plugin: &str,
        ids: &[String],
        actor: &Actor,
    ) -> Result<Imported, BoardError> {
        if ids.is_empty() {
            return Err(BoardError::refused(
                Refusal::EmptyField,
                "Pick at least one issue",
            ));
        }
        if ids.len() > SEARCH_LIMIT as usize {
            return Err(refused(format!(
                "Import at most {SEARCH_LIMIT} issues at once"
            )));
        }
        let ready = self.ready(project_key, plugin).await?;
        let _turn = self.turn(plugin).await;
        let mut issues = Vec::new();
        let mut skipped = Vec::new();
        for id in ids {
            if let Some(link) = self
                .store
                .ticket_link_by_issue(&ready.project.id, plugin, id)
                .await?
                && link.ticket_id.is_some()
            {
                skipped.push(format!("{} is already imported", link.issue_key));
                continue;
            }
            let issue = self.fetch(&ready, id).await?;
            if !issue.importable() {
                skipped.push(format!("{} isn't an open work item", issue.key));
                continue;
            }
            issues.push(issue);
        }
        let tickets = self.create(board, &ready, &issues, actor).await?;
        Ok(Imported { tickets, skipped })
    }

    async fn create(
        &self,
        board: &Board,
        ready: &Ready,
        issues: &[Issue],
        actor: &Actor,
    ) -> Result<Vec<Ticket>, BoardError> {
        let mut created: Vec<Ticket> = Vec::new();
        let result = async {
            for issue in issues {
                let body = issue.body.clone().unwrap_or_default();
                let ticket = board
                    .create_ticket(
                        &NewTicketInput {
                            project: ready.project.key.clone(),
                            title: issue.title.clone(),
                            body: body.clone(),
                            priority: issue.priority,
                            external_id: Some(issue.key.clone()),
                            ..Default::default()
                        },
                        actor,
                    )
                    .await?;
                created.push(ticket.clone());
                let now = now_ms();
                self.store
                    .put_ticket_link(&LinkRow {
                        id: new_id(),
                        ticket_id: Some(ticket.id.clone()),
                        project_id: ready.project.id.clone(),
                        plugin_id: ready.manifest.id.clone(),
                        tracker: ready.manifest.name.clone(),
                        issue_id: issue.id.clone(),
                        issue_key: issue.key.clone(),
                        url: issue.url.clone(),
                        parent_key: issue.parent.as_ref().map(|p| p.key.clone()),
                        parent_url: issue.parent.as_ref().and_then(|p| p.url.clone()),
                        snapshot_title: issue.title.clone(),
                        snapshot_body: body,
                        imported_at: now,
                        fetched_at: now,
                    })
                    .await?;
            }
            Ok::<_, BoardError>(())
        }
        .await;
        match result {
            Ok(()) => Ok(created),
            Err(e) => {
                for t in created.iter().rev() {
                    if let Err(undo) = board.delete_ticket(&t.key.to_string()).await {
                        tracing::error!(key = %t.key, error = %undo, "can't undo an imported ticket");
                    }
                }
                Err(e)
            }
        }
    }

    /// Fetches an imported ticket's issue again: fields edited only on the tracker update silently, fields edited
    /// on both sides come back as conflicts.
    pub async fn refresh(
        &self,
        board: &Board,
        key: &str,
        actor: &Actor,
    ) -> Result<RefreshOutcome, BoardError> {
        let (ticket, link, ready) = self.linked(board, key).await?;
        let _turn = self.turn(&link.plugin_id).await;
        let issue = self.fetch(&ready, &link.issue_id).await?;
        let remote_body = issue.body.clone().unwrap_or_default();
        let mut update = TicketUpdate::default();
        let mut outcome = RefreshOutcome {
            updated: Vec::new(),
            conflicts: Vec::new(),
        };
        for (field, snapshot, local, remote) in [
            (
                RefreshField::Title,
                &link.snapshot_title,
                &ticket.title,
                &issue.title,
            ),
            (
                RefreshField::Body,
                &link.snapshot_body,
                &ticket.body,
                &remote_body,
            ),
        ] {
            match merge(snapshot, local, remote) {
                Merge::Same | Merge::KeepLocal => {}
                Merge::TakeRemote(text) => {
                    match field {
                        RefreshField::Title => update.title = Some(text),
                        RefreshField::Body => update.body = Some(text),
                    }
                    outcome.updated.push(field);
                }
                Merge::Conflict { local, remote } => {
                    outcome.conflicts.push(RefreshConflict {
                        field,
                        local,
                        remote,
                    });
                }
            }
        }
        if update != TicketUpdate::default() {
            board.update_ticket(key, &update, actor).await?;
        }
        if outcome.conflicts.is_empty() {
            self.store
                .set_ticket_link_snapshot(&link.id, &issue.title, &remote_body)
                .await?;
        }
        Ok(outcome)
    }

    /// Applies the user's picks for a refresh's conflicts, then takes the tracker's text as the new snapshot.
    pub async fn resolve(
        &self,
        board: &Board,
        key: &str,
        picks: &BTreeMap<String, Pick>,
        actor: &Actor,
    ) -> Result<(), BoardError> {
        let (_, link, ready) = self.linked(board, key).await?;
        let _turn = self.turn(&link.plugin_id).await;
        let issue = self.fetch(&ready, &link.issue_id).await?;
        let remote_body = issue.body.clone().unwrap_or_default();
        let take = |field: &str| picks.get(field) == Some(&Pick::Tracker);
        let update = TicketUpdate {
            title: take("title").then(|| issue.title.clone()),
            body: take("body").then(|| remote_body.clone()),
            ..Default::default()
        };
        if update != TicketUpdate::default() {
            board.update_ticket(key, &update, actor).await?;
        }
        self.store
            .set_ticket_link_snapshot(&link.id, &issue.title, &remote_body)
            .await?;
        Ok(())
    }

    async fn linked(
        &self,
        board: &Board,
        key: &str,
    ) -> Result<(Ticket, LinkRow, Ready), BoardError> {
        let ticket = board.ticket(key).await?;
        let link = self
            .store
            .ticket_link(&ticket.id)
            .await?
            .ok_or_else(|| refused(format!("{key} wasn't imported from a tracker")))?;
        let project = self
            .store
            .project(&link.project_id)
            .await?
            .ok_or_else(|| BoardError::NotFound("the project".to_owned()))?;
        let ready = self.ready(&project.key, &link.plugin_id).await?;
        Ok((ticket, link, ready))
    }

    // ---- Auto-pull

    /// Runs every pull that is due: the saved search, new open work items into the Backlog. Never deletes or moves
    /// anything; skips imported and dismissed issues. Answers the projects that got new tickets.
    pub async fn pull_due(&self, board: &Board) -> Result<Vec<(String, Vec<Ticket>)>, BoardError> {
        let now = now_ms();
        let mut out = Vec::new();
        for row in self.store.auto_pull_plugins().await? {
            let due = row.last_pull_at.is_none_or(|at| {
                now.saturating_sub(at) >= row.pull_minutes.max(MIN_PULL_MINUTES) * 60_000
            });
            if !due {
                continue;
            }
            let Some(project) = self.store.project(&row.project_id).await? else {
                continue;
            };
            match self.pull(board, &project.key, &row.plugin_id).await {
                Ok(tickets) => {
                    self.store
                        .set_project_plugin_pull(&row.project_id, &row.plugin_id, None)
                        .await?;
                    if !tickets.is_empty() {
                        out.push((project.key.clone(), tickets));
                    }
                }
                Err(e) => {
                    tracing::warn!(project = %project.key, plugin = %row.plugin_id, error = %e, "auto-pull failed");
                    self.store
                        .set_project_plugin_pull(
                            &row.project_id,
                            &row.plugin_id,
                            Some(&e.to_string()),
                        )
                        .await?;
                }
            }
        }
        Ok(out)
    }

    /// One pull now (also **Pull now**).
    pub async fn pull(
        &self,
        board: &Board,
        project_key: &str,
        plugin: &str,
    ) -> Result<Vec<Ticket>, BoardError> {
        let ready = self.ready(project_key, plugin).await?;
        if !ready.manifest.has(Capability::AutoPull) {
            return Err(refused(format!("{} can't auto-pull", ready.manifest.name)));
        }
        let query = ready
            .values
            .get(PULL_QUERY)
            .and_then(Value::as_str)
            .map(str::to_owned)
            .ok_or_else(|| {
                BoardError::refused(Refusal::EmptyField, "Set the auto-pull search first")
            })?;
        let _turn = self.turn(plugin).await;
        let output = self.run_search(&ready, &query).await?;
        let mut found =
            mapping::search_results(&ready.manifest, &output, &ready.values).map_err(refused)?;
        found.truncate(SEARCH_LIMIT as usize);
        let links = self.store.ticket_links(&ready.project.id).await?;
        let mut issues = Vec::new();
        for hit in found.into_iter().filter(Issue::importable) {
            if links
                .iter()
                .any(|l| l.plugin_id == plugin && l.issue_id == hit.id)
            {
                continue;
            }
            let issue = self.fetch(&ready, &hit.id).await?;
            if issue.importable() {
                issues.push(issue);
            }
        }
        self.create(board, &ready, &issues, &Actor::current_user())
            .await
    }

    // ---- Running commands

    async fn turn(&self, plugin: &str) -> tokio::sync::OwnedMutexGuard<()> {
        let lock = self
            .locks
            .lock()
            .await
            .entry(plugin.to_owned())
            .or_default()
            .clone();
        lock.lock_owned().await
    }

    async fn run_search(&self, ready: &Ready, query: &str) -> Result<Value, BoardError> {
        let argv = render(
            &ready.manifest,
            &ready.manifest.commands.search.run,
            TemplateScope::Search,
            &Values {
                settings: Some(&ready.values),
                query: Some(query),
                limit: Some(SEARCH_LIMIT),
                ..Default::default()
            },
        )?;
        self.runner.run_json(&argv).await.map_err(run_error)
    }

    async fn fetch(&self, ready: &Ready, id: &str) -> Result<Issue, BoardError> {
        let argv = render(
            &ready.manifest,
            &ready.manifest.commands.fetch.run,
            TemplateScope::Fetch,
            &Values {
                settings: Some(&ready.values),
                id: Some(id),
                ..Default::default()
            },
        )?;
        let output = self.runner.run_json(&argv).await.map_err(run_error)?;
        mapping::fetched(&ready.manifest, &output, &ready.values).map_err(refused)
    }

    /// A plugin turned on for a project, with its effective values, ready for a command.
    async fn ready(&self, project_key: &str, plugin: &str) -> Result<Ready, BoardError> {
        let project = self.project_row(project_key).await?;
        let row = self.row(plugin).await?;
        let (manifest, _) = self.current(&row).await?;
        let own = self
            .store
            .project_plugin(&project.id, plugin)
            .await?
            .filter(|u| u.enabled)
            .ok_or_else(|| {
                refused(format!(
                    "{} isn't turned on for {project_key}",
                    manifest.name
                ))
            })?;
        let values = values::effective(
            &manifest,
            &parse_map(&row.defaults_json),
            &parse_map(&own.values_json),
        );
        if let Some((_, message)) = values::problems(&manifest, &values).into_iter().next() {
            return Err(BoardError::refused(Refusal::EmptyField, message));
        }
        Ok(Ready {
            project,
            manifest,
            values,
        })
    }

    /// The plugin's manifest now: a Local folder is read again when its files changed, and refused when its
    /// commands changed since the user agreed to them. With a Local folder's open-source warnings.
    async fn current(&self, row: &PluginRow) -> Result<(Manifest, Vec<String>), BoardError> {
        let stored = stored_manifest(row)?;
        let Some(path) = row.local_path.as_deref().filter(|_| row.source == "local") else {
            if !stored.fits(&otaq_version()) {
                return Err(refused(format!(
                    "{} {} doesn't run on this {PRODUCT_NAME} version",
                    stored.name, stored.version
                )));
            }
            return Ok((stored, Vec::new()));
        };
        let bundle = Bundle::from_dir(Path::new(path)).map_err(refused)?;
        let manifest = bundle.manifest().map_err(refused)?;
        let warnings = bundle.open_source_problems(&manifest);
        if bundle.fingerprint() != row.fingerprint {
            let digest = consent_view(&manifest, Label::Local, path, Vec::new()).digest;
            if digest != row.consented || manifest.id != row.id {
                return Err(refused(format!(
                    "{}'s commands changed: add the folder again to review them",
                    manifest.name
                )));
            }
            let mut updated = row.clone();
            updated.version = manifest.version.clone();
            updated.manifest_json = serde_json::to_string(&manifest).expect("manifests serialize");
            updated.fingerprint = bundle.fingerprint();
            updated.updated_at = now_ms();
            self.store.put_plugin(&updated).await?;
        }
        Ok((manifest, warnings))
    }

    async fn row(&self, id: &str) -> Result<PluginRow, BoardError> {
        self.store
            .plugin(id)
            .await?
            .ok_or_else(|| BoardError::NotFound(format!("no plugin {id} is installed")))
    }

    async fn project_row(&self, key: &str) -> Result<crate::store::projects::Project, BoardError> {
        self.store
            .project_by_key(key)
            .await?
            .ok_or_else(|| BoardError::NotFound(format!("unknown project {key}")))
    }

    async fn marketplace(&self, id: &str) -> Result<MarketplaceRow, BoardError> {
        self.ensure_official().await?;
        self.store
            .plugin_marketplace(id)
            .await?
            .ok_or_else(|| BoardError::NotFound("no such marketplace".to_owned()))
    }

    fn index_entry(
        &self,
        market: &MarketplaceRow,
        id: &str,
        version: &str,
    ) -> Result<Option<index::IndexVersion>, BoardError> {
        let Some(text) = market.index_json.as_deref() else {
            return Ok(None);
        };
        let index = Index::read(text).map_err(refused)?;
        Ok(index.plugin(id).and_then(|p| p.version(version)).cloned())
    }

    /// Downloads and checks a marketplace release: its hash, its manifest, and the open-source rules.
    async fn release(
        &self,
        marketplace: &str,
        id: &str,
        version: Option<&str>,
    ) -> Result<(MarketplaceRow, index::IndexVersion, Bundle, Manifest), BoardError> {
        let market = self.marketplace(marketplace).await?;
        if market.index_json.is_none() {
            self.fetch_index(&market).await;
        }
        let market = self.marketplace(marketplace).await?;
        let text = market.index_json.as_deref().ok_or_else(|| {
            refused(format!(
                "{} can't be read now; try again later",
                market.label
            ))
        })?;
        let index = Index::read(text).map_err(refused)?;
        let plugin = index
            .plugin(id)
            .ok_or_else(|| BoardError::NotFound(format!("{} has no plugin {id}", market.label)))?;
        let entry = match version {
            Some(v) => plugin.version(v),
            None => plugin.newest_for(&otaq_version()),
        }
        .ok_or_else(|| {
            refused(format!(
                "No version of {} runs on this {PRODUCT_NAME}",
                plugin.name
            ))
        })?
        .clone();
        if !super::manifest::fits(&entry.otaq, entry.protocol, &otaq_version()) {
            return Err(refused(format!(
                "{} {} doesn't run on this {PRODUCT_NAME} version",
                plugin.name, entry.version
            )));
        }
        let bytes = self.http.download(&entry.url).await.map_err(refused)?;
        let bundle = Bundle::read(&bytes, &entry.sha256).map_err(refused)?;
        if bundle.id != id || bundle.version != entry.version {
            return Err(refused(
                "The download isn't the release the marketplace lists",
            ));
        }
        let manifest = bundle.manifest().map_err(refused)?;
        let problems = bundle.open_source_problems(&manifest);
        if let Some(first) = problems.first() {
            return Err(refused(format!(
                "{} breaks the open-source rules: {first}",
                manifest.name
            )));
        }
        Ok((market, entry, bundle, manifest))
    }

    fn staged_path(&self, id: &str, version: &str) -> PathBuf {
        self.data
            .plugin_staging_dir()
            .join(format!("{id}-{version}.json"))
    }
}

struct Ready {
    project: crate::store::projects::Project,
    manifest: Manifest,
    values: Map<String, Value>,
}

fn label(row: &PluginRow, market: Option<&MarketplaceRow>) -> Label {
    match (row.source.as_str(), market) {
        ("local", _) => Label::Local,
        (_, Some(m)) if m.official => Label::Official,
        _ => Label::Community,
    }
}

fn stored_manifest(row: &PluginRow) -> Result<Manifest, BoardError> {
    serde_json::from_str(&row.manifest_json)
        .map_err(|e| refused(format!("{}'s stored manifest can't be read: {e}", row.id)))
}

fn io_error(e: std::io::Error) -> BoardError {
    BoardError::Store(StoreError::Folder(e))
}

fn render(
    manifest: &Manifest,
    run: &[String],
    scope: TemplateScope,
    values: &Values,
) -> Result<Vec<String>, BoardError> {
    let keys: Vec<&str> = manifest.settings.iter().map(|s| s.key.as_str()).collect();
    run.iter()
        .map(|arg| {
            Template::parse(arg, scope, &keys)
                .and_then(|t| t.render(values))
                .map_err(|e| {
                    BoardError::refused(
                        Refusal::PluginFailed,
                        RunError::new(RunErrorKind::BadSettings, e).to_string(),
                    )
                })
        })
        .collect()
}

/// The consent screen's content, and the digest of what the user agrees to: the CLIs, the command lines and the
/// capabilities.
fn consent_view(manifest: &Manifest, label: Label, source: &str, warnings: Vec<String>) -> Consent {
    let commands: Vec<String> = manifest
        .command_lines()
        .iter()
        .map(|argv| argv.join(" "))
        .collect();
    let requires: Vec<String> = manifest.requires.iter().map(|r| r.name.clone()).collect();
    let digest = bundle::sha256(
        serde_json::json!({
            "id": manifest.id,
            "requires": manifest.requires.iter().map(|r| &r.id).collect::<Vec<_>>(),
            "commands": manifest.command_lines(),
            "capabilities": manifest.capabilities,
        })
        .to_string()
        .as_bytes(),
    );
    Consent {
        id: manifest.id.clone(),
        name: manifest.name.clone(),
        version: manifest.version.clone(),
        label,
        source: source.to_owned(),
        license: manifest.license.clone(),
        requires,
        commands,
        capabilities: manifest.capabilities.clone(),
        source_url: None,
        diff_url: None,
        installed: None,
        warnings,
        digest,
    }
}

/// Writes `bundle` as `folder`, replacing an older version in one rename.
fn replace_folder(staging: &Path, folder: &Path, bundle: &Bundle) -> std::io::Result<()> {
    DataDir::ensure_dir(staging)?;
    let fresh = staging.join(format!("{}.new", bundle.id));
    let old = staging.join(format!("{}.old", bundle.id));
    for leftover in [&fresh, &old] {
        if leftover.exists() {
            std::fs::remove_dir_all(leftover)?;
        }
    }
    bundle.write_to(&fresh)?;
    if folder.exists() {
        std::fs::rename(folder, &old)?;
    }
    if let Some(parent) = folder.parent() {
        DataDir::ensure_dir(parent)?;
    }
    std::fs::rename(&fresh, folder)?;
    if old.exists() {
        std::fs::remove_dir_all(&old)?;
    }
    Ok(())
}
