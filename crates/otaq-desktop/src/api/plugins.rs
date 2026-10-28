//! `plugins.*`: Settings → Plugins, a project's Plugins section, the board's Import dialog and a ticket's Refresh from
//! tracker (the otaq-marketplace README). Each answer is the service's, in the page's typed shape; the page refetches on the
//! service's `plugins_changed` and `ticket_changed` events. The words are here, in [`Texts`].

use std::collections::BTreeMap;

use otaq_core::plugins::params::{
    ConsentParams, DefaultsParams, ImportParams, InstallParams, LocalFolderParams, MarketplaceAddParams,
    PluginIdParams, PluginTerminalParams, ProjectPluginParams, ProjectPluginSetParams, ResolveParams, SearchParams,
};
use otaq_core::naming::PRODUCT_NAME;
use otaq_core::protocol::{KeyParams, ProjectParams, Reply, Request};
use otaq_view::view_models::onboarding_page::{ToolTerminalTexts, tool_terminal_texts};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use ts_rs::TS;

use super::{RequestError, reply, unknown};
use crate::App;

pub async fn request(app: &App, method: &str, params: Value) -> Result<Value, RequestError> {
    let send = |r: Request| async move { app.service.send(r).await };
    match method {
        "plugins.view" => reply(&page(send(Request::PluginsList).await)?),
        "plugins.refresh_index" => reply(&page(send(Request::PluginsRefreshIndex).await)?),
        "plugins.marketplace_add" => {
            let p: MarketplaceAddParams = parse(params)?;
            reply(&page(send(Request::PluginsMarketplaceAdd(p)).await)?)
        }
        "plugins.marketplace_remove" => {
            let p: PluginIdParams = parse(params)?;
            reply(&page(send(Request::PluginsMarketplaceRemove(p)).await)?)
        }
        "plugins.consent" => {
            let p: ConsentParams = parse(params)?;
            reply(&consent(send(Request::PluginsConsent(p)).await?)?)
        }
        "plugins.consent_local" => {
            let p: FolderParams = parse(params)?;
            let request = Request::PluginsLocal(LocalFolderParams {
                folder: p.folder,
                digest: None,
            });
            reply(&consent(send(request).await?)?)
        }
        "plugins.install" => {
            let p: InstallParams = parse(params)?;
            reply(&page(send(Request::PluginsInstall(p)).await)?)
        }
        "plugins.add_local" => {
            let p: AddLocalParams = parse(params)?;
            let request = Request::PluginsLocal(LocalFolderParams {
                folder: p.folder,
                digest: Some(p.digest),
            });
            reply(&page(send(request).await)?)
        }
        "plugins.remove" => {
            let p: PluginIdParams = parse(params)?;
            reply(&page(send(Request::PluginsRemove(p)).await)?)
        }
        "plugins.defaults_set" => {
            let p: DefaultsParams = parse(params)?;
            reply(&page(send(Request::PluginsDefaultsSet(p)).await)?)
        }
        "plugins.check" => {
            let p: PluginIdParams = parse(params)?;
            match send(Request::PluginsCheck(p)).await? {
                Reply::PluginChecks(checks) => reply(&mirror::<Vec<RequirementView>>(&checks)?),
                other => Err(unexpected(method, &other)),
            }
        }
        "plugins.terminal" => {
            let p: PluginTerminalParams = parse(params)?;
            match send(Request::PluginsTerminal(p)).await? {
                Reply::Session(session) => reply(&TerminalOpened { session: session.id }),
                other => Err(unexpected(method, &other)),
            }
        }
        "plugins.project" => {
            let p: ProjectParams = parse(params)?;
            reply(&project(p.project.clone(), send(Request::ProjectPluginsGet(p)).await)?)
        }
        "plugins.project_set" => {
            let p: ProjectPluginSetParams = parse(params)?;
            reply(&project(p.project.clone(), send(Request::ProjectPluginsSet(p)).await)?)
        }
        "plugins.pull_now" => {
            let p: ProjectPluginParams = parse(params)?;
            match send(Request::PluginsPullNow(p)).await? {
                Reply::PluginImported(imported) => reply(&imported_view(&imported)),
                other => Err(unexpected(method, &other)),
            }
        }
        "plugins.search" => {
            let p: SearchParams = parse(params)?;
            match send(Request::PluginsSearch(p)).await? {
                Reply::PluginSearch(result) => reply(&mirror::<SearchView>(&*result)?),
                other => Err(unexpected(method, &other)),
            }
        }
        "plugins.import" => {
            let p: ImportParams = parse(params)?;
            match send(Request::PluginsImport(p)).await? {
                Reply::PluginImported(imported) => reply(&imported_view(&imported)),
                other => Err(unexpected(method, &other)),
            }
        }
        "plugins.links" => {
            let p: ProjectParams = parse(params)?;
            match send(Request::PluginsLinks(p)).await? {
                Reply::TrackerLinks(links) => reply(&mirror::<Vec<TrackerLinkView>>(&links)?),
                other => Err(unexpected(method, &other)),
            }
        }
        "plugins.refresh_ticket" => {
            let p: KeyParams = parse(params)?;
            match send(Request::TicketTrackerRefresh(p)).await? {
                Reply::TrackerRefresh(outcome) => reply(&mirror::<RefreshView>(&*outcome)?),
                other => Err(unexpected(method, &other)),
            }
        }
        "plugins.resolve" => {
            let p: ResolveParams = parse(params)?;
            match send(Request::TicketTrackerResolve(p)).await? {
                Reply::Ok => reply(&()),
                other => Err(unexpected(method, &other)),
            }
        }
        _ => Err(unknown(method)),
    }
}

fn parse<T: DeserializeOwned>(params: Value) -> Result<T, RequestError> {
    serde_json::from_value(params).map_err(|e| RequestError::plain(e.to_string()))
}

fn unexpected(method: &str, other: &Reply) -> RequestError {
    RequestError::plain(format!("unexpected answer to {method}: {other:?}"))
}

/// The service's value in the page's shape: the same fields, typed for the page.
fn mirror<T: DeserializeOwned>(value: &impl Serialize) -> Result<T, RequestError> {
    let json = serde_json::to_value(value).map_err(|e| RequestError::plain(e.to_string()))?;
    serde_json::from_value(json).map_err(|e| RequestError::plain(e.to_string()))
}

fn page(answer: Result<Reply, RequestError>) -> Result<Page, RequestError> {
    match answer? {
        Reply::Plugins(view) => Ok(Page {
            installed: mirror(&view.installed)?,
            available: mirror(&view.available)?,
            marketplaces: mirror(&view.marketplaces)?,
            texts: Texts::new(),
            terminal: tool_terminal_texts(),
        }),
        other => Err(unexpected("plugins.view", &other)),
    }
}

fn project(key: String, answer: Result<Reply, RequestError>) -> Result<ProjectView, RequestError> {
    match answer? {
        Reply::ProjectPlugins(plugins) => Ok(ProjectView {
            project: key,
            plugins: mirror(&plugins)?,
            texts: Texts::new(),
            terminal: tool_terminal_texts(),
        }),
        other => Err(unexpected("plugins.project", &other)),
    }
}

fn consent(answer: Reply) -> Result<ConsentView, RequestError> {
    match answer {
        Reply::PluginConsent(consent) => mirror(&*consent),
        other => Err(unexpected("plugins.consent", &other)),
    }
}

fn imported_view(imported: &otaq_core::plugins::registry::Imported) -> Imported {
    Imported {
        created: imported
            .tickets
            .iter()
            .map(|t| CreatedTicket {
                key: t.key.to_string(),
                title: t.title.clone(),
                external_id: t.external_id.clone(),
            })
            .collect(),
        skipped: imported.skipped.clone(),
    }
}

#[derive(Debug, Clone, Deserialize, TS)]
#[ts(rename = "PluginsFolderParams")]
pub struct FolderParams {
    pub folder: String,
}

#[derive(Debug, Clone, Deserialize, TS)]
#[ts(rename = "PluginsAddLocalParams")]
pub struct AddLocalParams {
    pub folder: String,
    pub digest: String,
}

/// Where a plugin came from: its badge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename = "PluginLabel")]
pub enum Label {
    Official,
    Community,
    Local,
}

/// Settings → Plugins.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(rename = "PluginsPage")]
pub struct Page {
    pub installed: Vec<InstalledView>,
    pub available: Vec<AvailableView>,
    pub marketplaces: Vec<MarketplaceRow>,
    pub texts: Texts,
    /// The Log in terminal's words.
    pub terminal: ToolTerminalTexts,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(rename = "PluginInstalled")]
pub struct InstalledView {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: String,
    pub license: String,
    pub label: Label,
    pub source: String,
    pub capabilities: Vec<String>,
    pub settings: Vec<SettingView>,
    #[ts(type = "Record<string, unknown>")]
    pub defaults: Map<String, Value>,
    #[serde(default)]
    pub update: Option<String>,
    #[serde(default)]
    pub warnings: Vec<String>,
    #[serde(default)]
    pub problem: Option<String>,
}

/// One field of a plugin's settings form.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(rename = "PluginSetting")]
pub struct SettingView {
    pub key: String,
    pub label: String,
    /// `string` or `map`.
    #[serde(rename = "type")]
    pub kind: String,
    pub required: bool,
    #[serde(default)]
    #[ts(type = "unknown")]
    pub default: Option<Value>,
    /// `project` (a project sets it) or `any`.
    pub scope: String,
    #[serde(default)]
    pub placeholder: Option<String>,
    #[serde(default)]
    pub help: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(rename = "PluginAvailable")]
pub struct AvailableView {
    pub marketplace: String,
    pub label: Label,
    pub id: String,
    pub name: String,
    pub description: String,
    pub license: String,
    pub version: String,
    #[serde(default)]
    pub installed: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(rename = "PluginMarketplace")]
pub struct MarketplaceRow {
    pub id: String,
    pub url: String,
    pub label: String,
    pub official: bool,
    #[serde(default)]
    pub fetched_at: Option<i64>,
    #[serde(default)]
    pub error: Option<String>,
}

/// The consent screen.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(rename = "PluginConsent")]
pub struct ConsentView {
    pub id: String,
    pub name: String,
    pub version: String,
    pub label: Label,
    pub source: String,
    pub license: String,
    pub requires: Vec<String>,
    pub commands: Vec<String>,
    pub capabilities: Vec<String>,
    #[serde(default)]
    pub source_url: Option<String>,
    #[serde(default)]
    pub diff_url: Option<String>,
    #[serde(default)]
    pub installed: Option<String>,
    #[serde(default)]
    pub warnings: Vec<String>,
    pub digest: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(rename = "PluginRequirement")]
pub struct RequirementView {
    pub id: String,
    pub name: String,
    pub installed: bool,
    #[serde(default)]
    pub logged_in: Option<bool>,
    #[serde(default)]
    pub extension_added: Option<bool>,
    #[serde(default)]
    pub install_hint: Option<String>,
    #[serde(default)]
    pub login: Option<Vec<String>>,
    #[serde(default)]
    pub extension_install: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(rename = "PluginTerminalOpened")]
pub struct TerminalOpened {
    pub session: String,
}

/// A project's Plugins section.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(rename = "ProjectPluginsView")]
pub struct ProjectView {
    pub project: String,
    pub plugins: Vec<ProjectPluginView>,
    pub texts: Texts,
    pub terminal: ToolTerminalTexts,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(rename = "ProjectPlugin")]
pub struct ProjectPluginView {
    pub plugin: String,
    pub name: String,
    pub label: Label,
    pub enabled: bool,
    pub settings: Vec<SettingView>,
    #[ts(type = "Record<string, unknown>")]
    pub values: Map<String, Value>,
    #[ts(type = "Record<string, unknown>")]
    pub defaults: Map<String, Value>,
    /// `[key, message]` pairs.
    pub problems: Vec<(String, String)>,
    pub can_auto_pull: bool,
    pub auto_pull: bool,
    pub pull_minutes: i64,
    #[serde(default)]
    pub last_pull_at: Option<i64>,
    #[serde(default)]
    pub last_pull_error: Option<String>,
    #[serde(default)]
    pub query_label: Option<String>,
}

/// The Import dialog's results.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(rename = "PluginSearch")]
pub struct SearchView {
    pub hits: Vec<SearchHitView>,
    pub truncated: bool,
    pub hidden: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(rename = "PluginSearchHit")]
pub struct SearchHitView {
    pub issue: IssueView,
    #[serde(default)]
    pub ticket: Option<String>,
    #[serde(default)]
    pub dismissed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(rename = "TrackerIssue")]
pub struct IssueView {
    pub id: String,
    pub key: String,
    pub url: String,
    pub title: String,
    #[serde(rename = "type", default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub state: Option<String>,
    #[serde(default)]
    pub parent: Option<IssueRefView>,
    /// 1 Urgent … 4 Low.
    #[serde(default)]
    pub priority: Option<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(rename = "TrackerIssueRef")]
pub struct IssueRefView {
    pub key: String,
    #[serde(default)]
    pub url: Option<String>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(rename = "PluginImported")]
pub struct Imported {
    pub created: Vec<CreatedTicket>,
    pub skipped: Vec<String>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(rename = "PluginCreatedTicket")]
pub struct CreatedTicket {
    pub key: String,
    pub title: String,
    pub external_id: Option<String>,
}

/// An imported ticket's tracker issue: the card's and the editor's badge.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(rename = "TrackerLink")]
pub struct TrackerLinkView {
    pub ticket_id: String,
    pub ticket: String,
    pub plugin: String,
    pub tracker: String,
    pub key: String,
    pub url: String,
    #[serde(default)]
    pub parent_key: Option<String>,
    #[serde(default)]
    pub parent_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(rename = "TrackerRefresh")]
pub struct RefreshView {
    /// `title`, `body`.
    pub updated: Vec<String>,
    pub conflicts: Vec<ConflictView>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(rename = "TrackerConflict")]
pub struct ConflictView {
    /// `title` or `body`.
    pub field: String,
    pub local: String,
    pub remote: String,
}

/// The words of the plugin screens.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(rename = "PluginTexts")]
pub struct Texts {
    pub intro: String,
    pub installed_heading: String,
    pub none_installed: String,
    pub browse_heading: String,
    pub none_available: String,
    pub marketplaces_heading: String,
    pub add_marketplace: String,
    pub add_marketplace_placeholder: String,
    pub add_local: String,
    pub refresh: String,
    pub install: String,
    pub update: String,
    pub remove: String,
    pub installed_version: String,
    pub defaults_heading: String,
    pub defaults_hint: String,
    pub save: String,
    pub check: String,
    pub log_in: String,
    pub add_extension: String,
    pub ready: String,
    pub not_installed: String,
    pub not_logged_in: String,
    pub extension_missing: String,
    pub labels: BTreeMap<String, String>,
    pub label_notes: BTreeMap<String, String>,
    pub consent_title: String,
    pub consent_intro: String,
    pub consent_commands: String,
    pub consent_requires: String,
    pub consent_polls: String,
    pub consent_source: String,
    pub consent_changes: String,
    pub consent_license: String,
    pub consent_warnings: String,
    pub consent_confirm: String,
    pub cancel: String,
    pub project_intro: String,
    pub project_none: String,
    pub enable: String,
    pub auto_pull: String,
    pub auto_pull_hint: String,
    pub every_minutes: String,
    pub pull_now: String,
    pub last_pull: String,
    pub import_title: String,
    pub search: String,
    pub search_placeholder: String,
    pub import_button: String,
    pub already_imported: String,
    pub dismissed: String,
    pub truncated: String,
    pub hidden: String,
    pub no_results: String,
    pub parent: String,
    pub refresh_ticket: String,
    pub refresh_title: String,
    pub refresh_intro: String,
    pub use_tracker: String,
    pub keep_mine: String,
    pub refreshed: String,
    pub up_to_date: String,
    pub open_in_tracker: String,
}

impl Texts {
    pub fn new() -> Self {
        let s = |t: &str| t.to_owned();
        Self {
            intro: format!(
                "Plugins extend {PRODUCT_NAME}: they connect it to the tools your team already uses. They run those \
                 tools' own command-line programs with your logins; {PRODUCT_NAME} stores no tokens."
            ),
            installed_heading: s("Installed"),
            none_installed: s("No plugins installed yet."),
            browse_heading: s("Browse"),
            none_available: s("Nothing to show: the marketplaces can't be read now, or offer nothing for this version."),
            marketplaces_heading: s("Marketplaces"),
            add_marketplace: s("Add marketplace"),
            add_marketplace_placeholder: s("https://github.com/owner/repo"),
            add_local: s("Add local folder…"),
            refresh: s("Refresh"),
            install: s("Install…"),
            update: s("Update to"),
            remove: s("Remove"),
            installed_version: s("Installed"),
            defaults_heading: s("Defaults for every project"),
            defaults_hint: s("A project's own value wins; empty fields use these."),
            save: s("Save"),
            check: s("Check tools"),
            log_in: s("Log in"),
            add_extension: s("Add extension"),
            ready: s("Ready"),
            not_installed: s("Not installed"),
            not_logged_in: s("Not logged in"),
            extension_missing: s("Extension missing"),
            labels: [
                ("official", "Official"),
                ("community", "Community"),
                ("local", "Local"),
            ]
            .into_iter()
            .map(|(k, v)| (k.to_owned(), v.to_owned()))
            .collect(),
            label_notes: [
                ("official", format!("From the {PRODUCT_NAME} marketplace; every change is reviewed.")),
                ("community", format!("From a marketplace you added; not reviewed by the {PRODUCT_NAME} project.")),
                ("local", "From a folder on this computer.".to_owned()),
            ]
            .into_iter()
            .map(|(k, v)| (k.to_owned(), v))
            .collect(),
            consent_title: s("Install"),
            consent_intro: s("This plugin runs these command lines with your permissions. Nothing installs until you agree."),
            consent_commands: s("Commands it can run"),
            consent_requires: s("Tools it needs"),
            consent_polls: s("Can pull new issues in the background, when you turn auto-pull on for a project."),
            consent_source: s("Source"),
            consent_changes: s("Changes since your version"),
            consent_license: s("License"),
            consent_warnings: s("Breaks the open-source rules (allowed for a local folder only)"),
            consent_confirm: s("Agree and install"),
            cancel: s("Cancel"),
            project_intro: s("Turn a plugin on to import from its tracker on the board (New ticket ▾ → Import)."),
            project_none: s("No plugins installed. Install one in Settings → Plugins."),
            enable: s("Use in this project"),
            auto_pull: s("Auto-pull"),
            auto_pull_hint: s("Runs the saved search and adds new open work items to the Backlog. Never moves or deletes tickets."),
            every_minutes: s("Every (minutes)"),
            pull_now: s("Pull now"),
            last_pull: s("Last pull"),
            import_title: s("Import from"),
            search: s("Search"),
            search_placeholder: s("The tracker's search"),
            import_button: s("Import"),
            already_imported: s("already imported"),
            dismissed: s("dismissed · import again"),
            truncated: s("Showing the first 100: refine the search for more."),
            hidden: s("left out: epics and finished issues"),
            no_results: s("No open work items match."),
            parent: s("parent"),
            refresh_ticket: s("Refresh from tracker"),
            refresh_title: s("Both sides changed"),
            refresh_intro: s("The ticket and the tracker changed this since the import. Pick which to keep."),
            use_tracker: s("Use tracker"),
            keep_mine: s("Keep mine"),
            refreshed: s("Updated from the tracker"),
            up_to_date: s("Already up to date"),
            open_in_tracker: s("Open in"),
        }
    }
}

#[cfg(test)]
pub fn export_types(cfg: &ts_rs::Config) -> Result<(), ts_rs::ExportError> {
    Page::export_all(cfg)?;
    ProjectView::export_all(cfg)?;
    ConsentView::export_all(cfg)?;
    RequirementView::export_all(cfg)?;
    TerminalOpened::export_all(cfg)?;
    SearchView::export_all(cfg)?;
    Imported::export_all(cfg)?;
    TrackerLinkView::export_all(cfg)?;
    RefreshView::export_all(cfg)?;
    FolderParams::export_all(cfg)?;
    AddLocalParams::export_all(cfg)?;
    Ok(())
}
