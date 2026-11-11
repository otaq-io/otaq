//! `settings.*`: the app's Settings (T-0234; SET-01, d0023). The page asks for the whole view (`settings.view`),
//! sends one edit at a time (`settings.edit`: a field and its value; the rules are in
//! `otaq_view::view_models::settings_page`) and gets the fresh view back. The slow parts load on their own: the
//! tools check, the branches. Everything is stored by the service, never in a repository. A project's settings are
//! its own page (`project_settings.*`), which uses the branches and the Flow card's requests from here.

use otaq_core::agent_tools::Tool;
use otaq_core::agents::{AgentList, InUse, ModelRef};
use otaq_core::board::Refusal;
use otaq_core::board::settings::{GlobalSettings, ProjectSettings, SettingProblem};
use otaq_core::board::snapshot::ProjectInfo;
use otaq_core::protocol::{
    AgentEditParams, DefaultModelParams, IdParams, ModelsParams, NoyanAgentParams, ProjectParams,
    Reply, Request,
};
use otaq_view::view_models::about;
use otaq_view::view_models::git_settings;
use otaq_view::view_models::editor_fields::branch_items;
use otaq_view::view_models::licenses;
use otaq_view::view_models::{agent_page, agents};
use otaq_view::view_models::settings::data_paths;
use otaq_view::view_models::settings_page::{self as page, GlobalInputs, Os, Reveal};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;

use super::{RequestError, reply, unknown};
use crate::App;

mod agent_reads;
mod flow;
#[cfg(test)]
pub use flow::export_types as export_flow_types;
/// Import a flow…'s file read (the flow editor's too).
pub(super) use flow::import as read_flow_file;

pub async fn request(app: &App, method: &str, params: Value) -> Result<Value, RequestError> {
    match method {
        "settings.view" => reply(&view(app).await?),
        "settings.edit" => edit(app, parse(params)?).await,
        // Settings → Git & pull requests: git and the PR providers' tools, checked again on each call (Re-check).
        // Settings → Git & pull requests runs the tools check, which can take seconds (process starts are slow on
        // Windows). `settings.git_shown` answers at once with what to draw meanwhile: the last check's page, or
        // every row "Checking…" before the first.
        "settings.git" => {
            let view = git_settings::view(match app.service.send(Request::ToolsCheck).await {
                Ok(Reply::ToolsCheck(report)) => Ok(report),
                Ok(other) => Err(unexpected("tools.check", &other).message),
                Err(e) => Err(e.message),
            });
            if view.error.is_none() {
                *app.git_settings.lock().unwrap_or_else(|e| e.into_inner()) = Some(view.clone());
            }
            reply(&view)
        }
        "settings.git_shown" => reply(
            &app.git_settings
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .clone()
                .unwrap_or_else(git_settings::checking),
        ),
        "settings.branches" => branches(app, parse(params)?).await,
        // Settings → Agents (T-0410; AGM-02, AGM-03, AGM-04): the page, its default model, the agent Noyan uses;
        // one agent's page, its changes, Delete.
        "settings.agents" => {
            let p: AgentsOpen = parse(params)?;
            reply(&agents_section(app, p.refresh).await?)
        }
        "settings.agents_default_model" => {
            let p: ModelRef = parse(params)?;
            app.service
                .send(Request::AgentsDefaultModel(DefaultModelParams { model: p }))
                .await?;
            reply(&agents_section(app, false).await?)
        }
        // The model picker's Retry (`GTuGI`): reads the tool's list again; the page asks for its view after.
        "settings.agents_models_retry" => {
            let p: ModelsParams = parse(params)?;
            app.service.send(Request::AgentsModelsRetry(p)).await?;
            reply(&Value::Null)
        }
        "settings.agent" => reply(&agent_page(app, parse(params)?).await?),
        "settings.agent_add" => {
            let p: AgentAdd = parse(params)?;
            reply(&agent_add(app, p.draft).await?)
        }
        // Save on a saved agent's page: its edits are kept only now.
        "settings.agent_save" => {
            let p: AgentSave = parse(params)?;
            reply(&agent_save(app, p.id, p.draft).await?)
        }
        // "Noyan uses <agent>": the one app setting for Noyan's agent; the next conversation starts on it.
        "settings.noyan_agent" => {
            let p: AgentId = parse(params)?;
            app.service
                .send(Request::AgentsNoyan(NoyanAgentParams { agent: p.id }))
                .await?;
            reply(&agents_section(app, false).await?)
        }
        "settings.agent_delete_dialog" => {
            let p: AgentId = parse(params)?;
            reply(&delete_dialog(app, &p.id).await?)
        }
        "settings.agent_delete" => {
            let p: AgentId = parse(params)?;
            reply(&agent_delete(app, &p.id).await?)
        }
        // What this agent reads (T-0440; AGR-01…AGR-03): the view and its folders.
        m if m.starts_with("settings.agent_reads") => agent_reads::request(app, m, params).await,
        // Settings → Flow (T-0422; FLW-06…FLW-10): the section, the preset dialog, the actions.
        m if m.starts_with("settings.flow") => flow::request(app, m, params).await,
        "settings.credits" => reply(&page::credits()),
        // Settings → About (d0071): the section, Otaq's own license, the third-party notices.
        "settings.about" => reply(&crate::about::view()),
        // Use otaq in your own terminal (UPD-07, page 32 item 5): the row, and Add to my shell's PATH.
        "settings.about_path" | "settings.about_path_add" => {
            let data = app
                .service
                .data()
                .ok_or_else(|| RequestError::plain("no data folder"))?;
            let add = method == "settings.about_path_add";
            let row = tokio::task::spawn_blocking(move || {
                if add {
                    crate::shell_path::add(&data)
                } else {
                    crate::shell_path::row(&data)
                }
            })
            .await
            .map_err(|e| RequestError::plain(e.to_string()))?;
            reply(&row)
        }
        "settings.license" => reply(&about::license(crate::about::LICENSE)),
        "settings.licenses" => reply(&licenses::view(crate::about::NOTICES, cfg!(target_os = "macos"))),
        "settings.open_source" => {
            let p: SettingsLink = parse(params)?;
            // Only the links the credits and the notices show: the page can't make the app open anything else.
            if !page::is_credit_source(&p.url) && !crate::about::notices().links_to(&p.url) {
                return Err(RequestError::plain(format!("not a credits or notices link: {}", p.url)));
            }
            open::that_detached(&p.url).map_err(|e| RequestError::plain(e.to_string()))?;
            reply(&Value::Null)
        }
        "settings.reveal" => {
            let p: SettingsRevealParams = parse(params)?;
            let data = app
                .service
                .data()
                .ok_or_else(|| RequestError::plain("no data folder"))?;
            // The folder that holds it, in the file manager (the database file's own folder).
            let folder = match p.target {
                Reveal::Database => data.database_path().parent().map(ToOwned::to_owned),
                Reveal::Logs => Some(data.logs_dir()),
            }
            .ok_or_else(|| RequestError::plain("no folder to show"))?;
            open::that_detached(&folder).map_err(|e| RequestError::plain(e.to_string()))?;
            reply(&Value::Null)
        }
        _ => Err(unknown(method)),
    }
}

/// One edit of an app setting.
#[derive(Debug, Clone, Deserialize, TS)]
pub struct SettingsEdit {
    pub field: String,
    pub value: page::Value,
}

#[derive(Debug, Clone, Deserialize, TS)]
pub struct SettingsProject {
    pub project: String,
}

#[derive(Debug, Clone, Deserialize, TS)]
pub struct SettingsLink {
    pub url: String,
}

#[derive(Debug, Clone, Deserialize, TS)]
pub struct SettingsRevealParams {
    pub target: Reveal,
}

/// `settings.agents`: `refresh` reads the tool's model list again first (`agents.models_refresh`): the page asks
/// for it once when the section opens (d0086).
#[derive(Debug, Clone, Copy, Default, Deserialize, TS)]
pub struct AgentsOpen {
    #[serde(default)]
    pub refresh: bool,
}

/// `settings.agent`: a saved agent's page (`id`), or a draft's: New agent (nothing), Duplicate (`duplicate`: the
/// agent copied) or the draft the page holds (`draft`). `change` applies one change first: saved at once on a saved
/// agent, kept in the draft otherwise.
#[derive(Debug, Clone, Default, Deserialize, TS)]
#[ts(rename = "AgentPageParams")]
pub struct AgentOpen {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub duplicate: Option<String>,
    #[serde(default)]
    pub draft: Option<agent_page::Draft>,
    #[serde(default)]
    pub change: Option<agent_page::Change>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AgentAdd {
    pub draft: agent_page::Draft,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AgentSave {
    pub id: String,
    pub draft: agent_page::Draft,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AgentId {
    pub id: String,
}

/// How Add agent went: added (the new agent's page), or refused (the draft's page with the reason under its field).
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(tag = "outcome", rename_all = "snake_case")]
#[ts(rename = "AgentAddOutcome")]
pub enum AgentAddOutcome {
    Added { page: Box<agent_page::Page> },
    Refused { page: Box<agent_page::Page> },
}

/// How Delete went: deleted (the fresh list), or something started using it since the dialog asked.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(tag = "outcome", rename_all = "snake_case")]
#[ts(rename = "AgentDeleteOutcome")]
pub enum AgentDeleteOutcome {
    Deleted { section: Box<agents::Section> },
    Refused { dialog: agent_page::DeleteDialog },
}

/// The types Settings → Agents reads and sends, for `generated.rs` (T-0410).
#[cfg(test)]
pub fn export_agent_types(cfg: &ts_rs::Config) -> Result<(), ts_rs::ExportError> {
    agents::Section::export_all(cfg)?;
    agent_page::Page::export_all(cfg)?;
    agent_page::Change::export_all(cfg)?;
    agent_page::DeleteDialog::export_all(cfg)?;
    AgentOpen::export_all(cfg)?;
    AgentAddOutcome::export_all(cfg)?;
    AgentDeleteOutcome::export_all(cfg)?;
    agent_reads::export_types(cfg)
}

pub(super) fn parse<T: serde::de::DeserializeOwned>(params: Value) -> Result<T, RequestError> {
    let params = if params.is_null() {
        Value::Object(Default::default())
    } else {
        params
    };
    serde_json::from_value(params).map_err(|e| RequestError::plain(e.to_string()))
}

pub(super) async fn agent_list(app: &App) -> Result<AgentList, RequestError> {
    match app.service.send(Request::AgentsList).await? {
        Reply::Agents(list) => Ok(*list),
        other => Err(unexpected("agents.list", &other)),
    }
}

/// The agent tools (`agent_tools.list`; the avatars follow it), each tool's cached model list (T-0335), the connected
/// ones read again first when `refresh`, and the tools whose read gave none. When the tools can't be listed, the
/// first one counts as connected, as before M23.
pub(super) async fn agent_tools(app: &App, refresh: bool) -> Result<agents::AgentTools, RequestError> {
    let connected: Vec<Tool> = match super::agent_tools::list(app, Request::AgentToolsList).await {
        Ok(list) => list
            .tools
            .iter()
            .filter(|t| t.state.is_connected())
            .map(|t| t.tool)
            .collect(),
        Err(_) => vec![agents::new_entry_tool()],
    };
    let mut lists = Vec::new();
    let mut failed = Vec::new();
    for &tool in Tool::ALL {
        let read = refresh && connected.contains(&tool);
        let params = ModelsParams { tool };
        let request = if read {
            Request::AgentsModelsRefresh(params)
        } else {
            Request::AgentsModels(params)
        };
        let copy = match app.service.send(request).await? {
            Reply::Models(copy) => copy.map(|c| c.list),
            other => return Err(unexpected("agents.models", &other)),
        };
        if read && copy.is_none() {
            failed.push(tool);
        }
        lists.extend(copy.map(|list| (tool, list)));
    }
    Ok(agents::AgentTools {
        connected,
        lists,
        failed,
    })
}

/// What uses an agent (`agents.in_use`).
async fn in_use(app: &App, id: &str) -> Result<InUse, RequestError> {
    match app
        .service
        .send(Request::AgentsInUse(IdParams { id: id.to_owned() }))
        .await?
    {
        Reply::AgentInUse(uses) => Ok(*uses),
        other => Err(unexpected("agents.in_use", &other)),
    }
}

/// The page; `refresh` is the page opening: it reads the connected tools' lists again first.
async fn agents_section(app: &App, refresh: bool) -> Result<agents::Section, RequestError> {
    let list = agent_list(app).await?;
    let tools = agent_tools(app, refresh).await?;
    let mut uses = Vec::new();
    for agent in &list.agents {
        uses.push(in_use(app, &agent.id).await?);
    }
    Ok(agents::section(&agents::Inputs {
        list: &list,
        tools: &tools,
        uses: &uses,
    }))
}

/// A refusal the page writes under its field; anything else is the request's error.
fn refused(e: RequestError) -> Result<agent_page::Refused, RequestError> {
    match e.refusal.as_deref() {
        Some(code) => Ok(agent_page::Refused {
            field: agent_page::field_of(Some(code)),
            message: e.message,
        }),
        None => Err(e),
    }
}

/// One agent's page from what the service has now: `draft` (a draft, or a saved agent's change the service refused)
/// in place of the saved fields.
async fn page_of(
    app: &App,
    id: Option<&str>,
    draft: Option<agent_page::Draft>,
    refused: Option<agent_page::Refused>,
) -> Result<agent_page::Page, RequestError> {
    let list = agent_list(app).await?;
    let tools = agent_tools(app, false).await?;
    let agent = match id {
        Some(id) => Some(
            list.agents
                .iter()
                .find(|a| a.id == id)
                .ok_or_else(|| RequestError::plain(format!("no agent {id}")))?,
        ),
        None => None,
    };
    let uses = match agent {
        Some(a) => Some(in_use(app, &a.id).await?),
        None => None,
    };
    let draft = draft
        .or_else(|| agent.map(agent_page::Draft::of))
        .unwrap_or_default();
    Ok(agent_page::page(&agent_page::Inputs {
        list: &list,
        tools: &tools,
        id,
        draft: &draft,
        uses: uses.as_ref(),
        refused: refused.as_ref(),
    }))
}

/// `settings.agent`: a change goes into the page's draft, a saved agent's too: its edits are kept only on Save
/// (`settings.agent_save`).
async fn agent_page(app: &App, p: AgentOpen) -> Result<agent_page::Page, RequestError> {
    if let Some(id) = &p.id {
        let draft = match p.draft {
            Some(draft) => draft,
            None => {
                let list = agent_list(app).await?;
                let agent = list
                    .agents
                    .iter()
                    .find(|a| a.id == *id)
                    .ok_or_else(|| RequestError::plain(format!("no agent {id}")))?;
                agent_page::Draft::of(agent)
            }
        };
        let draft = match p.change {
            Some(change) => agent_page::apply(draft, change),
            None => draft,
        };
        return page_of(app, Some(id), Some(draft), None).await;
    }
    let draft = match (p.draft, p.duplicate) {
        (Some(draft), _) => draft,
        (None, Some(of)) => match app.service.send(Request::AgentsDuplicate(IdParams { id: of })).await? {
            Reply::AgentDraft(fields) => agent_page::Draft::from_fields(&fields),
            other => return Err(unexpected("agents.duplicate", &other)),
        },
        (None, None) => agent_page::Draft::default(),
    };
    let draft = match p.change {
        Some(change) => agent_page::apply(draft, change),
        None => draft,
    };
    page_of(app, None, Some(draft), None).await
}

/// Save on a saved agent's page: its draft becomes the agent (`agents.edit`), or the refusal is written under its
/// field with the draft kept.
async fn agent_save(
    app: &App,
    id: String,
    draft: agent_page::Draft,
) -> Result<AgentAddOutcome, RequestError> {
    let request = Request::AgentsEdit(AgentEditParams {
        id: id.clone(),
        fields: draft.fields(),
    });
    match app.service.send(request).await {
        Ok(_) => Ok(AgentAddOutcome::Added {
            page: Box::new(page_of(app, Some(&id), None, None).await?),
        }),
        Err(e) => Ok(AgentAddOutcome::Refused {
            page: Box::new(page_of(app, Some(&id), Some(draft), Some(refused(e)?)).await?),
        }),
    }
}

async fn agent_add(app: &App, draft: agent_page::Draft) -> Result<AgentAddOutcome, RequestError> {
    match app.service.send(Request::AgentsAdd(draft.fields())).await {
        Ok(Reply::Agent(agent)) => Ok(AgentAddOutcome::Added {
            page: Box::new(page_of(app, Some(&agent.id), None, None).await?),
        }),
        Ok(other) => Err(unexpected("agents.add", &other)),
        Err(e) => Ok(AgentAddOutcome::Refused {
            page: Box::new(page_of(app, None, Some(draft), Some(refused(e)?)).await?),
        }),
    }
}

/// What Delete… opens: refused with what uses it, or the plain question (`agents.in_use` asks first).
async fn delete_dialog(app: &App, id: &str) -> Result<agent_page::DeleteDialog, RequestError> {
    Ok(agent_page::delete_dialog(id, &in_use(app, id).await?))
}

async fn agent_delete(app: &App, id: &str) -> Result<AgentDeleteOutcome, RequestError> {
    match app.service.send(Request::AgentsDelete(IdParams { id: id.to_owned() })).await {
        Ok(_) => Ok(AgentDeleteOutcome::Deleted {
            section: Box::new(agents_section(app, false).await?),
        }),
        Err(e) if e.refusal.as_deref() == Some(Refusal::AgentInUse.as_str()) => Ok(AgentDeleteOutcome::Refused {
            dialog: delete_dialog(app, id).await?,
        }),
        Err(e) => Err(e),
    }
}

pub(super) fn unexpected(method: &str, reply: &Reply) -> RequestError {
    RequestError::plain(format!("unexpected answer to {method}: {reply:?}"))
}

pub(super) async fn global(app: &App) -> Result<(GlobalSettings, Vec<SettingProblem>), RequestError> {
    match app.service.send(Request::SettingsGet).await? {
        Reply::GlobalSettings(r) => Ok((r.settings, r.problems)),
        other => Err(unexpected("settings.get", &other)),
    }
}

pub(super) async fn projects(app: &App) -> Result<Vec<ProjectInfo>, RequestError> {
    match app.service.send(Request::ProjectList).await? {
        Reply::Projects(list) => Ok(list),
        other => Err(unexpected("project.list", &other)),
    }
}

pub(super) async fn project_settings(
    app: &App,
    key: &str,
) -> Result<(ProjectSettings, Vec<SettingProblem>), RequestError> {
    let request = Request::ProjectSettingsGet(ProjectParams {
        project: key.to_owned(),
    });
    match app.service.send(request).await? {
        Reply::ProjectSettings(r) => Ok((r.settings, r.problems)),
        other => Err(unexpected("project_settings.get", &other)),
    }
}

/// The nav and the app's page.
async fn view(app: &App) -> Result<page::View, RequestError> {
    let (settings, problems) = global(app).await?;
    let scheduler = match app.service.send(Request::SchedulerStatus).await {
        Ok(Reply::SchedulerStatus(s)) => Some(s),
        _ => None,
    };
    let (cleanup_count, cleanup_bytes) = match app.service.send(Request::CleanupScan).await {
        Ok(Reply::CleanupScan(items)) => (
            items.len(),
            items.iter().filter_map(|i| i.size_bytes).sum(),
        ),
        _ => (0, 0),
    };
    // What macOS allows, for the status line under the notifications switch (ATT-14).
    let notification_permission = match app.service.send(Request::NotificationsState).await {
        Ok(Reply::NotificationsState(state)) => state.permission,
        _ => otaq_core::attention::notify::PermissionState::Unknown,
    };
    let fonts = app.fonts.get().await;
    let shown = page::global_page(&GlobalInputs {
        settings: &settings,
        problems: &problems,
        scheduler: scheduler.as_ref(),
        data: app.service.data().map(|d| data_paths(&d)),
        cleanup_count,
        cleanup_bytes,
        os: Os::current(),
        notification_permission,
        fonts: &fonts,
    });
    Ok(page::view(shown))
}

/// Applies one edit and answers with the fresh view. A refused value comes back as the request's error.
async fn edit(app: &App, edit: SettingsEdit) -> Result<Value, RequestError> {
    let (mut settings, _) = global(app).await?;
    let fonts = app.fonts.get().await;
    page::edit_global(&mut settings, &edit.field, &edit.value, &fonts).map_err(RequestError::plain)?;
    // The service applies it at once (limits, notifications) and announces `settings_changed`, on which the theme
    // and the terminals follow.
    app.service.send(Request::SettingsSet(settings)).await?;
    reply(&view(app).await?)
}

/// The base branch dropdown: local branches, then remote ones not checked out, the current one checked.
async fn branches(app: &App, p: SettingsProject) -> Result<Value, RequestError> {
    let current = projects(app)
        .await?
        .into_iter()
        .find(|i| i.key == p.project)
        .map(|i| i.base_branch)
        .unwrap_or_default();
    let request = Request::ProjectBranches(ProjectParams { project: p.project });
    let Reply::Branches(branches) = app.service.send(request).await? else {
        return Err(RequestError::plain("unexpected answer to project.branches"));
    };
    reply(&branch_items(&branches.local, &branches.remote, &current))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::api::request;
    use crate::service::testing::{Harness, connected};

    /// Before any check, the pane is drawn at once with every row "Checking…"; after one, with what it found.
    #[tokio::test(flavor = "multi_thread")]
    async fn the_git_page_is_drawn_at_once_checking_then_as_last_found() {
        let h = Harness::new(None);
        h.wait_for("connected", connected).await;
        let shown = request(&h.app, "settings.git_shown", Value::Null).await.unwrap();
        assert_eq!(shown["git"]["state"], "Checking\u{2026}");
        assert_eq!(shown["providers"].as_array().unwrap().len(), 2);
        let view = request(&h.app, "settings.git", Value::Null).await.unwrap();
        if view["error"].is_null() {
            let shown = request(&h.app, "settings.git_shown", Value::Null).await.unwrap();
            assert_eq!(shown, view);
        }
        h.service.stop_all().await;
    }

    /// Git & pull requests runs the tools check and answers with git, then the PR providers (GitHub, Azure DevOps).
    #[tokio::test(flavor = "multi_thread")]
    async fn the_git_page_lists_git_then_the_providers() {
        let h = Harness::new(None);
        h.wait_for("connected", connected).await;
        let view = request(&h.app, "settings.git", Value::Null).await.unwrap();
        assert_eq!(view["git_title"], "Git");
        assert_eq!(view["providers_title"], "Pull request providers");
        if view["error"].is_null() {
            assert_eq!(view["git"]["name"], "git");
            let names: Vec<&str> =
                view["providers"].as_array().unwrap().iter().map(|p| p["name"].as_str().unwrap()).collect();
            assert_eq!(names, ["GitHub", "Azure DevOps"]);
        }
        h.service.stop_all().await;
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn the_global_page_loads_and_an_edit_is_stored() {
        let h = Harness::new(None);
        h.wait_for("connected", connected).await;
        let view = request(&h.app, "settings.view", Value::Null).await.unwrap();
        assert_eq!(view["page"]["page"], "global");
        assert_eq!(view["page"]["title"], "General");
        // Eight topics about you and your machine, no projects.
        let labels: Vec<&str> =
            view["nav"].as_array().unwrap().iter().map(|n| n["label"].as_str().unwrap()).collect();
        assert_eq!(
            labels,
            ["Agents", "Git & pull requests", "Limits & usage", "Notifications", "Appearance", "Keyboard", "Data", "About"]
        );
        let data = &view["page"]["sections"][8];
        assert_eq!(data["id"], "data");
        assert_eq!(data["rows"][0]["control"]["kind"], "path");

        let edited = request(
            &h.app,
            "settings.edit",
            json!({ "field": "terminal_renderer", "value": "dom" }),
        )
        .await
        .unwrap();
        let renderer = edited["page"]["sections"][6]["rows"][4]["control"]["value"].clone();
        assert_eq!(renderer, "dom");
        // No fonts in the test's folders: only Geist Mono is listed, and a family not installed is refused.
        let font = &edited["page"]["sections"][6]["rows"][3];
        assert_eq!(font["field"], "terminal_font_family");
        assert_eq!(font["control"]["kind"], "select");
        assert_eq!(font["control"]["choices"][0]["label"], "Geist Mono (default)");
        let refused = request(
            &h.app,
            "settings.edit",
            json!({ "field": "terminal_font_family", "value": "MesloLGS NF" }),
        )
        .await
        .unwrap_err();
        assert!(refused.message.contains("isn't an installed"), "{refused:?}");
        let (stored, _) = global(&h.app).await.unwrap();
        assert_eq!(
            stored.terminal_renderer,
            otaq_core::board::settings::TerminalRenderer::Dom
        );

        let refused = request(&h.app, "settings.edit", json!({ "field": "notifications", "value": "no" }))
            .await
            .unwrap_err();
        assert!(refused.message.contains("on or off"), "{refused:?}");
        let credits = request(&h.app, "settings.credits", Value::Null).await.unwrap();
        assert_eq!(credits["title"], "Theme credits");
        assert!(request(&h.app, "settings.open_source", json!({ "url": "https://example.com" }))
            .await
            .is_err());

        // Settings → About (d0071).
        let about = request(&h.app, "settings.about", Value::Null).await.unwrap();
        assert_eq!(about["product"], otaq_core::naming::PRODUCT_NAME);
        assert!(about["version_info"].as_str().unwrap().contains(env!("CARGO_PKG_VERSION")));
        let license = request(&h.app, "settings.license", Value::Null).await.unwrap();
        assert_eq!(license["title"], "Functional Source License");
        let notices = request(&h.app, "settings.licenses", Value::Null).await.unwrap();
        assert!(notices["rust"].as_array().unwrap().len() > 3);
        h.service.stop_all().await;
    }

    /// Settings → Agents through the service (T-0410; AGM-01, AGM-03, AGM-04): a fresh install's agents and where they
    /// are used, with no skills (T7) and no role, and no default model before a tool connects; "Noyan uses" set to
    /// another agent; one agent's page (no role on it) and a saved change; New agent's draft added; Delete refused for
    /// a stage's agent and for the agent Noyan uses, and allowed for an unused one.
    #[tokio::test(flavor = "multi_thread")]
    async fn the_agents_page_lists_edits_sets_noyans_agent_adds_and_deletes() {
        let h = Harness::new(None);
        h.wait_for("connected", connected).await;
        let ask = |method: &'static str, params: Value| request(&h.app, method, params);
        let section = ask("settings.agents", json!({ "refresh": true })).await.unwrap();
        assert_eq!(section["title"], "Agents");
        assert!(section.get("notice").is_none(), "no move-over notice");
        let rows: Vec<&str> = section["rows"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r["name"].as_str().unwrap())
            .collect();
        assert_eq!(rows, ["Noyan", "Developer", "Checker"]);
        assert!(section["rows"].as_array().unwrap().iter().all(|r| r.get("role").is_none()), "no row shows a role");
        assert_eq!(section["noyan"]["label"], "Noyan uses");
        assert_eq!(section["noyan"]["value"], "Noyan");
        // An agent has no skill list (T7): no Skills column, and no row names a skill.
        assert_eq!(section["columns"], json!(["Name", "Model", "Used in"]));
        assert!(section["rows"][1].get("skills").is_none());
        assert_eq!(section["rows"][1]["model"], "default");
        assert_eq!(section["rows"][0]["used_in"], "every project \u{b7} Noyan");
        assert_eq!(section["default_model"]["dropdown"]["value"], "The first agent tool you connect");
        let id = |n: usize| section["rows"][n]["id"].as_str().unwrap().to_owned();
        let (noyan, developer) = (id(0), id(1));

        // One agent's page: a change stays in its draft, and is kept only on Save.
        let page = ask("settings.agent", json!({ "id": developer })).await.unwrap();
        assert_eq!(page["title"], "Developer");
        assert!(page.get("skills").is_none(), "the page has no skills field");
        assert_eq!(page["model"]["tool_value"], "Default model");
        assert_eq!(page["add"]["add_label"], "Save");
        assert_eq!(page["add"]["unchanged"], true);
        let page = ask(
            "settings.agent",
            json!({ "id": developer, "change": { "field": "instructions", "value": "Run the tests." } }),
        )
        .await
        .unwrap();
        assert_eq!(page["instructions"]["value"], "Run the tests.");
        assert_eq!(page["add"]["unchanged"], false);
        // Not saved yet: the agent opens again as it was (Cancel).
        let again = ask("settings.agent", json!({ "id": developer })).await.unwrap();
        assert_ne!(again["instructions"]["value"], "Run the tests.");
        let saved = ask("settings.agent_save", json!({ "id": developer, "draft": page["draft"] })).await.unwrap();
        assert_eq!(saved["outcome"], "added");
        let again = ask("settings.agent", json!({ "id": developer })).await.unwrap();
        assert_eq!(again["instructions"]["value"], "Run the tests.");
        // A refused Save keeps the draft, the reason under its field.
        let mut taken = again["draft"].clone();
        taken["name"] = json!("Noyan");
        let refused = ask("settings.agent_save", json!({ "id": developer, "draft": taken })).await.unwrap();
        assert_eq!(refused["outcome"], "refused");
        assert_eq!(refused["page"]["name"]["problem"], "Another agent is already called Noyan");
        assert_eq!(refused["page"]["draft"]["name"], "Noyan", "the refused name stays typed");

        // No agent's page shows a role, Noyan's own included.
        let page = ask("settings.agent", json!({ "id": noyan })).await.unwrap();
        assert!(page.get("role").is_none() && page.get("role_tag").is_none());
        // The agent Noyan uses can't be deleted.
        let refused = ask("settings.agent_delete_dialog", json!({ "id": noyan })).await.unwrap();
        assert_eq!(refused["text"], "Noyan uses Noyan. Pick another agent for Noyan first.");

        // "Noyan uses" Developer: the setting changes, and Noyan's own agent is free.
        let section = ask("settings.noyan_agent", json!({ "id": developer })).await.unwrap();
        assert_eq!(section["noyan"]["value"], "Developer");
        let confirm = ask("settings.agent_delete_dialog", json!({ "id": noyan })).await.unwrap();
        assert_eq!(confirm["kind"], "confirm");

        // New agent: a draft until Add agent.
        let draft = ask("settings.agent", json!({ "change": { "field": "instructions", "value": "Be quick." } }))
            .await
            .unwrap();
        assert_eq!(draft["title"], "New agent");
        assert_eq!(draft["draft"]["instructions"], "Be quick.");
        assert!(draft["draft"].get("skills").is_none());
        let mut fields = draft["draft"].clone();
        fields["name"] = json!("Quick");
        let added = ask("settings.agent_add", json!({ "draft": fields })).await.unwrap();
        assert_eq!(added["outcome"], "added");
        assert_eq!(added["page"]["came_from"], "Added by you");
        let quick = added["page"]["id"].as_str().unwrap().to_owned();

        // Delete: refused for a stage's agent (the default flow's Dev), the plain question for an unused one.
        let validator = id(2);
        let refused = ask("settings.agent_delete_dialog", json!({ "id": validator })).await.unwrap();
        assert_eq!(refused["kind"], "refused");
        assert_eq!(refused["title"], "Can\u{2019}t delete Checker");
        let confirm = ask("settings.agent_delete_dialog", json!({ "id": quick })).await.unwrap();
        assert_eq!(confirm["kind"], "confirm");
        let deleted = ask("settings.agent_delete", json!({ "id": quick })).await.unwrap();
        assert_eq!(deleted["outcome"], "deleted");
        assert_eq!(deleted["section"]["rows"].as_array().unwrap().len(), 3);
        h.service.stop_all().await;
    }
}
