//! `settings.*`: the app's Settings (T-0234; SET-01, d0023). The page asks for the whole view (`settings.view`),
//! sends one edit at a time (`settings.edit`: a field and its value; the rules are in
//! `otaq_view::view_models::settings_page`) and gets the fresh view back. The slow parts load on their own: the
//! tools check, the branches. Everything is stored by the service, never in a repository. A project's settings are
//! its own page (`project_settings.*`), which uses the branches and the Flow card's requests from here.

use otaq_core::agent_tools::Tool;
use otaq_core::agents::NoyanStartsOn;
use otaq_core::board::settings::{GlobalSettings, ProjectSettings, SettingProblem};
use otaq_core::board::snapshot::ProjectInfo;
use otaq_core::protocol::{ModelsParams, ProjectParams, Reply, Request};
use otaq_view::view_models::about;
use otaq_view::view_models::git_settings;
use otaq_view::view_models::editor_fields::branch_items;
use otaq_view::view_models::licenses;
use otaq_view::view_models::agents;
use otaq_view::view_models::settings::data_paths;
use otaq_view::view_models::settings_page::{self as page, GlobalInputs, Os, Reveal};
use serde::Deserialize;
use serde_json::Value;
use ts_rs::TS;

use super::{RequestError, reply, unknown};
use crate::App;

mod flow;
#[cfg(test)]
pub use flow::export_types as export_flow_types;
/// Import a flow…'s file read (the flow editor's too).
pub(super) use flow::import as read_flow_file;

pub async fn request(app: &App, method: &str, params: Value) -> Result<Value, RequestError> {
    match method {
        "settings.view" => reply(&view(app).await?),
        "settings.edit" => edit(app, parse(params)?).await,
        // Settings → Git & pull requests: git and the PR providers' tools, checked again on each call (Re-check); not
        // the agent tools, which the page doesn't show.
        // Settings → Git & pull requests runs the tools check, which can take seconds (process starts are slow on
        // Windows). `settings.git_shown` answers at once with what to draw meanwhile: the last check's page, or
        // every row "Checking…" before the first.
        "settings.git" => {
            let view = git_settings::view(match app.service.send(Request::GitToolsCheck).await {
                Ok(Reply::ToolsCheck(report)) => Ok(report),
                Ok(other) => Err(unexpected("tools.check_git", &other).message),
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
        // The model picker's Retry (`GTuGI`): reads the tool's list again; the page asks for its view after.
        "settings.agents_models_retry" => {
            let p: ModelsParams = parse(params)?;
            app.service.send(Request::AgentsModelsRetry(p)).await?;
            reply(&Value::Null)
        }
        // Settings → Flow (T-0422; FLW-06…FLW-10): the section, the preset dialog, the actions.
        m if m.starts_with("settings.flow") => flow::request(app, m, params).await,
        "settings.credits" => reply(&page::credits()),
        // Settings → About (d0071): the section, Otaq's own license, the third-party notices.
        "settings.about" => reply(&crate::about::view(app).await),
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

pub(super) fn parse<T: serde::de::DeserializeOwned>(params: Value) -> Result<T, RequestError> {
    let params = if params.is_null() {
        Value::Object(Default::default())
    } else {
        params
    };
    serde_json::from_value(params).map_err(|e| RequestError::plain(e.to_string()))
}

/// "Noyan starts on" (`noyan.starts_on`, UX-70): the model and effort new Noyan conversations start on; `default`
/// when it can't be read.
pub(super) async fn noyan_starts_on(app: &App) -> NoyanStartsOn {
    match app.service.send(Request::NoyanStartsOn).await {
        Ok(Reply::NoyanStartsOn(on)) => *on,
        _ => NoyanStartsOn::default(),
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
        let data = &view["page"]["sections"][7];
        assert_eq!(data["id"], "data");
        assert_eq!(data["rows"][0]["control"]["kind"], "path");

        let edited = request(
            &h.app,
            "settings.edit",
            json!({ "field": "terminal_renderer", "value": "dom" }),
        )
        .await
        .unwrap();
        let renderer = edited["page"]["sections"][5]["rows"][4]["control"]["value"].clone();
        assert_eq!(renderer, "dom");
        // No fonts in the test's folders: only Geist Mono is listed, and a family not installed is refused.
        let font = &edited["page"]["sections"][5]["rows"][3];
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
        // No connected tool has tuq here: About says nothing about it (decisions 100, 102).
        assert_eq!(about["tuq"], Value::Null);
        assert!(!about["version_info"].as_str().unwrap().contains("tuq"));
        let license = request(&h.app, "settings.license", Value::Null).await.unwrap();
        assert_eq!(license["title"], "Functional Source License");
        let notices = request(&h.app, "settings.licenses", Value::Null).await.unwrap();
        assert!(notices["rust"].as_array().unwrap().len() > 3);
        h.service.stop_all().await;
    }

}
