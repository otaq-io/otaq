//! `settings.*`: Settings, global and per project (T-0234; SET-01, PRJ-03, PRJ-04, d0023). The page asks for the whole
//! view (`settings.view`), sends one edit at a time (`settings.edit`: a field and its value; the rules are in
//! `otaq_view::view_models::settings_page`) and gets the fresh view back. The slow parts load on their own: the
//! tools check, the PR provider, the branches. Everything is stored by the service, never in a repository.

use otaq_core::agent_tools::Tool;
use otaq_core::agents::{AgentList, InUse, ModelRef};
use otaq_core::board::Refusal;
use otaq_core::board::settings::{GlobalSettings, ProjectSettings, SettingProblem};
use otaq_core::board::snapshot::ProjectInfo;
use otaq_core::protocol::{
    AgentEditParams, DefaultModelParams, FallbackParams, IdParams, ImportSkillParams, KeyParams, ModelsParams,
    MoveRoleParams, ProjectParams, ProjectSettingsParams, ProjectUpdateParams, ReimportSkillParams, Reply, Request,
};
use otaq_view::view_models::about;
use otaq_view::view_models::editor_fields::branch_items;
use otaq_view::view_models::licenses;
use otaq_view::view_models::{agent_page, agents};
use otaq_view::view_models::settings::data_paths;
use otaq_view::view_models::settings_page::{
    self as page, GlobalInputs, Os, ProjectChange, ProjectInputs, Reveal, WarmInputs,
};
use otaq_view::view_models::warm_build;
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
        "settings.view" => {
            let p: SettingsOpen = parse(params)?;
            reply(&view(app, p.project.as_deref()).await?)
        }
        "settings.edit" => edit(app, parse(params)?).await,
        "settings.tools" => reply(&page::tools(match app.service.send(Request::ToolsCheck).await {
            Ok(Reply::ToolsCheck(report)) => Ok(report),
            Ok(other) => Err(unexpected("tools.check", &other).message),
            Err(e) => Err(e.message),
        })),
        "settings.provider" => {
            let p: SettingsProject = parse(params)?;
            let request = Request::ProjectPrProvider(ProjectParams { project: p.project });
            match app.service.send(request).await? {
                Reply::PrProvider(provider) => reply(&page::provider(Some(&provider))),
                other => Err(unexpected("project.pr_provider", &other)),
            }
        }
        "settings.branches" => branches(app, parse(params)?).await,
        "settings.remove_preview" => remove_preview(app, parse(params)?).await,
        "settings.remove" => {
            let p: SettingsProject = parse(params)?;
            app.service
                .send(Request::ProjectRemove(KeyParams { key: p.project }))
                .await?;
            reply(&view(app, None).await?)
        }
        // Settings → Agents (T-0410; AGM-02, AGM-03, AGM-04): the page, its default model and fallbacks;
        // one agent's page, its changes, its role, its skills, Delete.
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
        "settings.agents_fallback" => {
            let p: AgentsFallback = parse(params)?;
            app.service
                .send(Request::AgentsFallback(FallbackParams {
                    tool: p.tool,
                    fallback: p.fallback,
                }))
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
        "settings.agent_role" => {
            let p: AgentRoleSet = parse(params)?;
            reply(&agent_role(app, &p.id, &p.role).await?)
        }
        "settings.agent_import" => {
            let p: AgentImport = parse(params)?;
            let request = Request::AgentsImportSkill(ImportSkillParams {
                id: p.id.clone(),
                folder: p.folder.into(),
            });
            reply(&agent_skill(app, &p.id, request).await?)
        }
        "settings.agent_reimport" => {
            let p: AgentReimport = parse(params)?;
            let request = Request::AgentsReimportSkill(ReimportSkillParams {
                id: p.id.clone(),
                skill: p.skill,
            });
            reply(&agent_skill(app, &p.id, request).await?)
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

/// `settings.view`: the global page, or a project's page when `project` names one.
#[derive(Debug, Clone, Default, Deserialize, TS)]
pub struct SettingsOpen {
    #[serde(default)]
    pub project: Option<String>,
}

/// One edit: `project` names the project page it was made on (none: the global page).
#[derive(Debug, Clone, Deserialize, TS)]
pub struct SettingsEdit {
    #[serde(default)]
    pub project: Option<String>,
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

/// `settings.agents_fallback`: a tool's fallback; none clears it.
#[derive(Debug, Clone, Deserialize)]
pub struct AgentsFallback {
    pub tool: Tool,
    #[serde(default)]
    pub fallback: Option<ModelRef>,
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

/// `settings.agent_role`: the role as stored (`noyan`, `epic-lead`), or empty for None.
#[derive(Debug, Clone, Deserialize)]
pub struct AgentRoleSet {
    pub id: String,
    pub role: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AgentImport {
    pub id: String,
    pub folder: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AgentReimport {
    pub id: String,
    pub skill: String,
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
    moved: Option<String>,
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
        moved,
        now: otaq_core::store::now_ms(),
    }))
}

/// `settings.agent`: a saved agent's change is saved at once (`agents.edit`); a draft's stays in the draft.
async fn agent_page(app: &App, p: AgentOpen) -> Result<agent_page::Page, RequestError> {
    if let Some(id) = &p.id {
        let Some(change) = p.change else {
            return page_of(app, Some(id), None, None, None).await;
        };
        let list = agent_list(app).await?;
        let agent = list
            .agents
            .iter()
            .find(|a| a.id == *id)
            .ok_or_else(|| RequestError::plain(format!("no agent {id}")))?;
        let draft = agent_page::apply(agent_page::Draft::of(agent), change);
        let request = Request::AgentsEdit(AgentEditParams {
            id: id.clone(),
            fields: draft.fields(),
        });
        return match app.service.send(request).await {
            Ok(_) => page_of(app, Some(id), None, None, None).await,
            Err(e) => page_of(app, Some(id), Some(draft), Some(refused(e)?), None).await,
        };
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
    page_of(app, None, Some(draft), None, None).await
}

async fn agent_add(app: &App, draft: agent_page::Draft) -> Result<AgentAddOutcome, RequestError> {
    match app.service.send(Request::AgentsAdd(draft.fields())).await {
        Ok(Reply::Agent(agent)) => Ok(AgentAddOutcome::Added {
            page: Box::new(page_of(app, Some(&agent.id), None, None, None).await?),
        }),
        Ok(other) => Err(unexpected("agents.add", &other)),
        Err(e) => Ok(AgentAddOutcome::Refused {
            page: Box::new(page_of(app, None, Some(draft), Some(refused(e)?), None).await?),
        }),
    }
}

/// The page's Role: another role is given to this agent (`agents.move_role`), and the page says whose it was; None
/// on the agent holding a role is refused as AGM-01 says.
async fn agent_role(app: &App, id: &str, role: &str) -> Result<agent_page::Page, RequestError> {
    let list = agent_list(app).await?;
    let agent = list
        .agents
        .iter()
        .find(|a| a.id == id)
        .ok_or_else(|| RequestError::plain(format!("no agent {id}")))?;
    let Some(role) = agent_page::role_of(role) else {
        let refusal = agent.role.map(|held| agent_page::Refused {
            field: agent_page::Field::Role,
            message: agent_page::sentence(held.holder_refusal(&agent.name)),
        });
        return page_of(app, Some(id), None, refusal, None).await;
    };
    if agent.role == Some(role) {
        return page_of(app, Some(id), None, None, None).await;
    }
    let before = list
        .agents
        .iter()
        .find(|a| a.role == Some(role))
        .map(|a| a.name.clone());
    let request = Request::AgentsMoveRole(MoveRoleParams {
        role,
        agent: id.to_owned(),
    });
    match app.service.send(request).await {
        Ok(_) => {
            let moved = agent_page::moved_line(role, &agent.name, before.as_deref());
            page_of(app, Some(id), None, None, Some(moved)).await
        }
        Err(e) => {
            let refusal = agent_page::Refused {
                field: agent_page::Field::Role,
                message: agent_page::sentence(refused(e)?.message),
            };
            page_of(app, Some(id), None, Some(refusal), None).await
        }
    }
}

/// Import skill folder… and Re-import: the page again, with a refused import's reason under Skills.
async fn agent_skill(app: &App, id: &str, request: Request) -> Result<agent_page::Page, RequestError> {
    match app.service.send(request).await {
        Ok(_) => page_of(app, Some(id), None, None, None).await,
        Err(e) => {
            let mut why = refused(e)?;
            why.field = agent_page::Field::Skills;
            why.message = agent_page::not_imported(&why.message);
            page_of(app, Some(id), None, Some(why), None).await
        }
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

async fn global(app: &App) -> Result<(GlobalSettings, Vec<SettingProblem>), RequestError> {
    match app.service.send(Request::SettingsGet).await? {
        Reply::GlobalSettings(r) => Ok((r.settings, r.problems)),
        other => Err(unexpected("settings.get", &other)),
    }
}

async fn projects(app: &App) -> Result<Vec<ProjectInfo>, RequestError> {
    match app.service.send(Request::ProjectList).await? {
        Reply::Projects(list) => Ok(list),
        other => Err(unexpected("project.list", &other)),
    }
}

async fn project_settings(
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

/// The nav and the page: the global page, or `project`'s page.
async fn view(app: &App, project: Option<&str>) -> Result<page::View, RequestError> {
    let (settings, problems) = global(app).await?;
    let list = projects(app).await?;
    // The nav marks the projects with a flagged value.
    let mut alerts = Vec::new();
    let mut open = None;
    for info in &list {
        let (p_settings, p_problems) = project_settings(app, &info.key).await?;
        if !p_problems.is_empty() {
            alerts.push(info.key.clone());
        }
        if project == Some(info.key.as_str()) {
            open = Some((info.clone(), p_settings, p_problems));
        }
    }
    let shown = match open {
        Some((info, p_settings, p_problems)) => {
            // The warm build switch (T-0301): whether the disk can clone, and the project's warm build.
            let warm = super::recovery::warm_list(app).await.ok();
            // Run command's Ask the agent names the project's Project agent (T-0342).
            let project_agent = otaq_view::view_models::agent_panel::new_agent_name(
                super::agent_panel::project_agent(app).await?.as_ref(),
                &super::agent_panel::tools(app).await,
            );
            // Ask the agent is hidden only when no agent tool is connected (RUN-02, d0086).
            let tool_connected = match super::agent_tools::list(app, Request::AgentToolsList).await {
                Ok(list) => list.tools.iter().any(|t| t.state.is_connected()),
                Err(_) => true,
            };
            page::project_page(&ProjectInputs {
                info: &info,
                settings: &p_settings,
                problems: &p_problems,
                global: &settings,
                warm: warm.as_ref().map(|list| WarmInputs {
                    list,
                    now: otaq_core::store::now_ms(),
                }),
                project_agent: &project_agent,
                tool_connected,
            })
        }
        None => {
            let scheduler = match app.service.send(Request::SchedulerStatus).await {
                Ok(Reply::SchedulerStatus(s)) => Some(s),
                _ => None,
            };
            let cleanup_count = match app.service.send(Request::CleanupScan).await {
                Ok(Reply::CleanupScan(items)) => items.len(),
                _ => 0,
            };
            // What macOS allows, for the status line under the notifications switch (ATT-14).
            let notification_permission = match app.service.send(Request::NotificationsState).await {
                Ok(Reply::NotificationsState(state)) => state.permission,
                _ => otaq_core::attention::notify::PermissionState::Unknown,
            };
            let fonts = app.fonts.get().await;
            page::global_page(&GlobalInputs {
                settings: &settings,
                problems: &problems,
                scheduler: scheduler.as_ref(),
                data: app.service.data().map(|d| data_paths(&d)),
                cleanup_count,
                os: Os::current(),
                notification_permission,
                fonts: &fonts,
            })
        }
    };
    Ok(page::view(shown, &list, &alerts))
}

/// Applies one edit and answers with the fresh view. A refused value comes back as the request's error.
async fn edit(app: &App, edit: SettingsEdit) -> Result<Value, RequestError> {
    match &edit.project {
        None => {
            let (mut settings, _) = global(app).await?;
            let fonts = app.fonts.get().await;
            page::edit_global(&mut settings, &edit.field, &edit.value, &fonts)
                .map_err(RequestError::plain)?;
            // The service applies it at once (limits, notifications) and announces `settings_changed`, on which
            // the theme and the terminals follow.
            app.service.send(Request::SettingsSet(settings)).await?;
        }
        Some(key) => {
            let (mut settings, _) = project_settings(app, key).await?;
            // Turning the warm build switch off clears the project's warm build after saving (the service does it;
            // the page asked first). Its toast comes once it is gone, after any copy it waits for (WRM-06).
            let clears = if edit.field == "warm_builds" && edit.value == page::Value::Flag(false) {
                warm_build_of(app, key).await
            } else {
                None
            };
            match page::edit_project(&mut settings, &edit.field, &edit.value)
                .map_err(RequestError::plain)?
            {
                ProjectChange::Settings => {
                    let request = Request::ProjectSettingsSet(ProjectSettingsParams {
                        project: key.clone(),
                        settings: Box::new(settings),
                    });
                    app.service.send(request).await?;
                    if let Some((repo, name)) = clears {
                        let toast = warm_build::switched_off_toast(&name);
                        super::recovery::follow_clear(app, &repo, &name, toast);
                    }
                }
                ProjectChange::Project(update) => {
                    let request = Request::ProjectUpdate(ProjectUpdateParams {
                        project: key.clone(),
                        update,
                    });
                    app.service.send(request).await?;
                }
                ProjectChange::Nothing => {}
            }
        }
    }
    reply(&view(app, edit.project.as_deref()).await?)
}

/// The project's own warm build, by its folder, and the project's name; `None` when it has none.
async fn warm_build_of(app: &App, key: &str) -> Option<(String, String)> {
    let list = super::recovery::warm_list(app).await.ok()?;
    let build = list
        .builds
        .into_iter()
        .find(|b| b.project.as_deref() == Some(key))?;
    let name = projects(app)
        .await
        .ok()?
        .into_iter()
        .find(|p| p.key == key)
        .map_or_else(|| build.repo.clone(), |p| p.name);
    Some((build.repo, name))
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

/// The Remove project confirmation, with the project's ticket count.
async fn remove_preview(app: &App, p: SettingsProject) -> Result<Value, RequestError> {
    let info = projects(app)
        .await?
        .into_iter()
        .find(|i| i.key == p.project)
        .ok_or_else(|| RequestError::plain(format!("no project {}", p.project)))?;
    let request = Request::BoardSnapshot(ProjectParams {
        project: p.project.clone(),
    });
    let tickets = match app.service.send(request).await? {
        Reply::Board(board) => board.tickets().count(),
        _ => 0,
    };
    reply(&page::remove_dialog(&info, tickets))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::api::request;
    use crate::service::testing::{Harness, connected};

    #[tokio::test(flavor = "multi_thread")]
    async fn the_global_page_loads_and_an_edit_is_stored() {
        let h = Harness::new(None);
        h.wait_for("connected", connected).await;
        let view = request(&h.app, "settings.view", Value::Null).await.unwrap();
        assert_eq!(view["page"]["page"], "global");
        assert_eq!(view["page"]["title"], "General");
        assert_eq!(view["nav"][0]["label"], "Agent tools");
        assert_eq!(view["nav"][1]["label"], "Agents");
        // Agent limits is a row of its own again (T-0410, `s15-58`); the workflow skills moved to the agents.
        assert_eq!(view["nav"][2]["label"], "Agent limits");
        // Validation & Auto-merge is gone (FLW-01).
        assert_eq!(view["nav"][3]["label"], "Usage");
        let data = &view["page"]["sections"][5];
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

    #[tokio::test(flavor = "multi_thread")]
    async fn environment_variables_are_saved_and_a_duplicate_is_marked() {
        use otaq_core::board::projects::AddProject;
        use otaq_core::board::settings::EnvVar;
        use otaq_core::git::testing::TempRepos;

        let h = Harness::new(None);
        h.wait_for("connected", connected).await;
        let repos = TempRepos::new();
        let path = repos.repo("shop-web", 1);
        let project = match h
            .app
            .service
            .send(Request::ProjectAdd(AddProject {
                path: path.display().to_string(),
                key: None,
                base_branch: None,
                flow_setup: false,
            }))
            .await
            .unwrap()
        {
            Reply::ProjectAdded { project: p, .. } => p.key,
            other => panic!("{other:?}"),
        };
        let edit = |field: &str, value: Value| {
            request(
                &h.app,
                "settings.edit",
                json!({ "project": project, "field": field, "value": value }),
            )
        };
        let env_row = |view: &Value| -> Value {
            view["page"]["sections"]
                .as_array()
                .unwrap()
                .iter()
                .flat_map(|s| s["rows"].as_array().unwrap())
                .find(|r| r["field"] == "env_vars")
                .unwrap()["control"]
                .clone()
        };
        let view = edit("env_vars.add", json!({ "name": "CI", "value": "true" }))
            .await
            .unwrap();
        assert_eq!(env_row(&view)["rows"][0]["name"], "CI");
        let view = edit("env_vars.add", json!({ "name": "CI", "value": "false" }))
            .await
            .unwrap();
        let rows = env_row(&view)["rows"].clone();
        assert_eq!(rows[0]["problem"], Value::Null);
        assert_eq!(
            rows[1]["problem"],
            "CI is already set above. Only the first row applies; remove one of them."
        );
        assert_eq!(view["nav"].as_array().unwrap().last().unwrap()["alert"], true);
        edit("env_vars.1.name", json!("DEBUG")).await.unwrap();
        edit("env_vars.0.value", json!("1")).await.unwrap();
        let (stored, problems) = project_settings(&h.app, &project).await.unwrap();
        assert_eq!(stored.env_vars, [EnvVar::new("CI", "1"), EnvVar::new("DEBUG", "false")]);
        assert!(problems.is_empty(), "{problems:?}");
        let view = edit("env_vars.remove", json!(0)).await.unwrap();
        assert_eq!(env_row(&view)["rows"][0]["name"], "DEBUG");
        h.service.stop_all().await;
    }

    /// The warm build switch (T-0301; WRM-05): turning it off with a warm build shows the confirmation's texts,
    /// saves, clears it and toasts; turning it on asks nothing.
    #[tokio::test(flavor = "multi_thread")]
    async fn turning_the_warm_build_switch_off_clears_the_warm_build() {
        use otaq_core::board::projects::AddProject;
        use otaq_core::git::testing::TempRepos;

        let h = Harness::new(None);
        h.wait_for("connected", connected).await;
        let repos = TempRepos::new();
        let path = repos.repo("shop-web", 1);
        let project = match h
            .app
            .service
            .send(Request::ProjectAdd(AddProject {
                path: path.display().to_string(),
                key: None,
                base_branch: None,
                flow_setup: false,
            }))
            .await
            .unwrap()
        {
            Reply::ProjectAdded { project: p, .. } => p.key,
            other => panic!("{other:?}"),
        };
        let open = || request(&h.app, "settings.view", json!({ "project": project }));
        let edit = |value: bool| {
            request(
                &h.app,
                "settings.edit",
                json!({ "project": project, "field": "warm_builds", "value": value }),
            )
        };
        let switch = |view: &Value| -> Value {
            view["page"]["sections"]
                .as_array()
                .unwrap()
                .iter()
                .flat_map(|s| s["rows"].as_array().unwrap())
                .find(|r| r["field"] == "warm_builds")
                .unwrap()["control"]
                .clone()
        };
        let view = open().await.unwrap();
        // No agent tool is connected in the test service: Run command has no Ask the agent (RUN-02, d0086).
        let run = view["page"]["sections"]
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|s| s["rows"].as_array().unwrap())
            .find(|r| r["field"] == "run_command")
            .unwrap()
            .clone();
        assert_eq!(run["button"], Value::Null);
        let control = switch(&view);
        assert_eq!(control["kind"], "warm_build");
        // On every disk (M19-b): where it can't clone, the build is handed over instead.
        assert_eq!(control["disabled"], false);
        assert_eq!(control["reason"], Value::Null);
        assert_eq!(control["on"], true);
        assert_eq!(control["confirm"], Value::Null, "nothing to delete: no question");

        // A warm build of the project's (its folder is the repo's name).
        let data = h.app.service.data().unwrap();
        let folder = data.warm_entry_path("shop-web", 0).unwrap();
        std::fs::create_dir_all(folder.join("out")).unwrap();
        std::fs::write(folder.join("out").join("built.bin"), vec![0u8; 4096]).unwrap();
        let control = switch(&open().await.unwrap());
        assert_eq!(control["confirm"]["title"], "Clear shop-web’s warm build (up to 4 KB)?");
        assert_eq!(control["confirm"]["confirm"], "Turn off and clear up to 4 KB");

        let control = switch(&edit(false).await.unwrap());
        assert_eq!(control["on"], false);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while !h.app.notices.view().toasts.iter().any(|t| t.title == "Cleared shop-web’s warm build") {
            assert!(std::time::Instant::now() < deadline, "no toast");
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
        assert!(!folder.exists());
        let (stored, _) = project_settings(&h.app, &project).await.unwrap();
        assert!(!stored.warm_builds);

        let control = switch(&edit(true).await.unwrap());
        assert_eq!(control["on"], true);
        assert_eq!(control["confirm"], Value::Null);
        h.service.stop_all().await;
    }

    /// Settings → Agents through the service (T-0410; AGM-01, AGM-03, AGM-04): a fresh install's four agents with
    /// their roles, skills and where they are used, and no default model before a tool connects; one agent's
    /// page and a saved change; None refused on the role's agent; a role moved; New agent's draft added; Delete
    /// refused for a stage's agent and allowed for an unused one.
    #[tokio::test(flavor = "multi_thread")]
    async fn the_agents_page_lists_edits_moves_a_role_adds_and_deletes() {
        let h = Harness::new(None);
        h.wait_for("connected", connected).await;
        let ask = |method: &'static str, params: Value| request(&h.app, method, params);
        let section = ask("settings.agents", json!({ "refresh": true })).await.unwrap();
        assert_eq!(section["title"], "Agents");
        assert!(section.get("notice").is_none(), "no move-over notice");
        let rows: Vec<(&str, &Value)> = section["rows"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| (r["name"].as_str().unwrap(), &r["role"]))
            .collect();
        assert_eq!(
            rows,
            [
                ("Noyan", &json!("Noyan")),
                ("Epic lead", &json!("Epic lead")),
                ("Developer", &Value::Null),
                ("Validator", &Value::Null),
            ]
        );
        assert_eq!(section["rows"][2]["skills"], json!(["work-ticket", "commit"]));
        assert_eq!(section["rows"][2]["model"], "default");
        assert_eq!(section["rows"][0]["used_in"], "every project \u{b7} Noyan");
        assert_eq!(section["default_model"]["dropdown"]["value"], "The first agent tool you connect");
        let id = |n: usize| section["rows"][n]["id"].as_str().unwrap().to_owned();
        let (noyan, developer) = (id(0), id(2));

        // One agent's page, and a change saved at once.
        let page = ask("settings.agent", json!({ "id": developer })).await.unwrap();
        assert_eq!(page["title"], "Developer");
        assert_eq!(page["model"]["tool_value"], "Default model");
        let page = ask(
            "settings.agent",
            json!({ "id": developer, "change": { "field": "instructions", "value": "Run the tests." } }),
        )
        .await
        .unwrap();
        assert_eq!(page["instructions"]["value"], "Run the tests.");
        let page = ask("settings.agent", json!({ "id": developer, "change": { "field": "name", "value": "Noyan" } }))
            .await
            .unwrap();
        assert_eq!(page["name"]["problem"], "Another agent is already called Noyan");
        assert_eq!(page["draft"]["name"], "Noyan", "the refused name stays typed");

        // The role: None · Noyan · Epic lead; None is refused on its agent; another agent takes it, and the page says
        // whose it was.
        let page = ask("settings.agent_role", json!({ "id": noyan, "role": "" })).await.unwrap();
        let options: Vec<(&str, &str)> = page["role"]["options"]
            .as_array()
            .unwrap()
            .iter()
            .map(|o| (o["id"].as_str().unwrap(), o["label"].as_str().unwrap()))
            .collect();
        assert_eq!(options, [("", "None"), ("noyan", "Noyan"), ("epic-lead", "Epic lead")]);
        assert_eq!(
            page["role"]["problem"],
            "Noyan has the Noyan role. Give the role to another agent first."
        );
        let page = ask("settings.agent_role", json!({ "id": developer, "role": "noyan" }))
            .await
            .unwrap();
        assert_eq!(page["role"]["moved"], "Developer is now Noyan \u{b7} the agent Noyan has no role");
        assert_eq!(page["role_tag"], "Noyan");

        // New agent: a draft until Add agent.
        let draft = ask("settings.agent", json!({ "change": { "field": "add_skill", "value": "work-ticket" } }))
            .await
            .unwrap();
        assert_eq!(draft["title"], "New agent");
        assert_eq!(draft["draft"]["skills"], json!(["shipped:work-ticket"]));
        let mut fields = draft["draft"].clone();
        fields["name"] = json!("Quick");
        let added = ask("settings.agent_add", json!({ "draft": fields })).await.unwrap();
        assert_eq!(added["outcome"], "added");
        assert_eq!(added["page"]["came_from"], "Added by you");
        let quick = added["page"]["id"].as_str().unwrap().to_owned();

        // Delete: refused for a stage's agent (the default flow's Dev), the plain question for an unused one.
        let validator = id(3);
        let refused = ask("settings.agent_delete_dialog", json!({ "id": validator })).await.unwrap();
        assert_eq!(refused["kind"], "refused");
        assert_eq!(refused["title"], "Can\u{2019}t delete Validator");
        let confirm = ask("settings.agent_delete_dialog", json!({ "id": quick })).await.unwrap();
        assert_eq!(confirm["kind"], "confirm");
        let deleted = ask("settings.agent_delete", json!({ "id": quick })).await.unwrap();
        assert_eq!(deleted["outcome"], "deleted");
        assert_eq!(deleted["section"]["rows"].as_array().unwrap().len(), 4);
        h.service.stop_all().await;
    }
}
