//! `project_settings.*`: the project's Settings page, in its main area (`otaq-view`'s `project_settings.rs`). The page
//! asks for its view (`project_settings.view`), sends one edit at a time (`project_settings.edit`: a field and its
//! value) and gets the fresh view back. Most edits are the project's settings or the project itself (name, base
//! branch), as `settings_page::edit_project` says; Deliver as is the flow's own setting, so it writes a new flow
//! version the way the flow editor does: the flow with the new delivery as the project's draft
//! (`flow.draft_update`), then applied (`flow.apply`). The PR provider line (`project_settings.provider`) and Remove
//! project (`project_settings.remove_preview`, `project_settings.remove`) are here too; the Flow card's requests are
//! `settings.flow*`, the base branch's list `settings.branches`.

use otaq_core::flow::apply::ApplyParams;
use otaq_core::flow::draft::{Draft, DraftEdit, DraftUpdateParams};
use otaq_core::flow::model::Delivery;
use otaq_core::flow::yaml;
use otaq_core::protocol::{
    KeyParams, ProjectParams, ProjectSettingsParams, ProjectUpdateParams, Reply, Request,
};
use otaq_view::view_models::project_settings::{self as vm, deliver_as, is_delivery_field};
use otaq_view::view_models::settings_page::{self as page, ProjectChange, ProjectInputs, WarmInputs};
use otaq_view::view_models::warm_build;
use serde::Deserialize;
use serde_json::Value;
use ts_rs::TS;

use super::settings::{global, parse, project_settings, projects, unexpected};
use super::{RequestError, reply, unknown};
use crate::App;

pub async fn request(app: &App, method: &str, params: Value) -> Result<Value, RequestError> {
    match method {
        "project_settings.view" => {
            let p: ProjectSettingsOpen = parse(params)?;
            reply(&view(app, &p.project).await?)
        }
        "project_settings.edit" => {
            let p: ProjectSettingsEdit = parse(params)?;
            edit(app, &p).await?;
            reply(&view(app, &p.project).await?)
        }
        "project_settings.provider" => {
            let p: ProjectSettingsOpen = parse(params)?;
            let request = Request::ProjectPrProvider(ProjectParams { project: p.project });
            match app.service.send(request).await? {
                Reply::PrProvider(provider) => reply(&vm::provider(Some(&provider))),
                other => Err(unexpected("project.pr_provider", &other)),
            }
        }
        "project_settings.remove_preview" => {
            let p: ProjectSettingsOpen = parse(params)?;
            remove_preview(app, &p.project).await
        }
        "project_settings.remove" => {
            let p: ProjectSettingsOpen = parse(params)?;
            app.service
                .send(Request::ProjectRemove(KeyParams { key: p.project }))
                .await?;
            reply(&Value::Null)
        }
        _ => Err(unknown(method)),
    }
}

/// `{"project": "SHOP"}`.
#[derive(Debug, Clone, Deserialize, TS)]
pub struct ProjectSettingsOpen {
    pub project: String,
}

/// One edit on the project's page.
#[derive(Debug, Clone, Deserialize, TS)]
pub struct ProjectSettingsEdit {
    pub project: String,
    pub field: String,
    pub value: page::Value,
}

/// The types the page reads and sends, for `generated.rs`.
#[cfg(test)]
pub fn export_types(cfg: &ts_rs::Config) -> Result<(), ts_rs::ExportError> {
    vm::View::export_all(cfg)?;
    vm::ProviderView::export_all(cfg)?;
    ProjectSettingsOpen::export_all(cfg)?;
    ProjectSettingsEdit::export_all(cfg)
}

/// The project's flow as it is now; `None` when it doesn't read.
async fn current_delivery(app: &App, project: &str) -> Option<Delivery> {
    let request = Request::FlowCurrent(ProjectParams {
        project: project.to_owned(),
    });
    match app.service.send(request).await {
        Ok(Reply::FlowVersion(v)) => v.file().ok().map(|f| f.flow.delivery),
        _ => None,
    }
}

/// The page from the project, its settings and its flow's delivery.
async fn view(app: &App, key: &str) -> Result<vm::View, RequestError> {
    let info = projects(app)
        .await?
        .into_iter()
        .find(|p| p.key == key)
        .ok_or_else(|| RequestError::plain(format!("no project {key}")))?;
    let (global, _) = global(app).await?;
    let (settings, problems) = project_settings(app, key).await?;
    // The warm build switch (T-0301): whether the disk can clone, and the project's warm build.
    let warm = super::recovery::warm_list(app).await.ok();
    // Run command's Ask the agent names Noyan's agent (T-0342).
    let project_agent = otaq_view::view_models::agent_panel::new_agent_name(
        super::agent_panel::project_agent(app).await?.as_ref(),
        &super::agent_panel::tools(app).await,
    );
    // Ask the agent is hidden only when no agent tool is connected (RUN-02, d0086).
    let tool_connected = match super::agent_tools::list(app, Request::AgentToolsList).await {
        Ok(list) => list.tools.iter().any(|t| t.state.is_connected()),
        Err(_) => true,
    };
    let delivery = current_delivery(app, key).await;
    Ok(vm::view(&ProjectInputs {
        info: &info,
        settings: &settings,
        problems: &problems,
        global: &global,
        warm: warm.as_ref().map(|list| WarmInputs {
            list,
            now: otaq_core::store::now_ms(),
        }),
        project_agent: &project_agent,
        tool_connected,
        delivery: delivery.as_ref(),
    }))
}

/// Applies one edit. A refused value comes back as the request's error.
async fn edit(app: &App, edit: &ProjectSettingsEdit) -> Result<(), RequestError> {
    let key = &edit.project;
    if is_delivery_field(&edit.field) {
        let value = match &edit.value {
            page::Value::Text(text) => text.as_str(),
            _ => return Err(RequestError::plain("Deliver as takes a word")),
        };
        return deliver(app, key, &edit.field, value).await;
    }
    let (mut settings, _) = project_settings(app, key).await?;
    // Turning the warm build switch off clears the project's warm build after saving (the service does it; the page
    // asked first). Its toast comes once it is gone, after any copy it waits for (WRM-06).
    let clears = if edit.field == "warm_builds" && edit.value == page::Value::Flag(false) {
        warm_build_of(app, key).await
    } else {
        None
    };
    match page::edit_project(&mut settings, &edit.field, &edit.value).map_err(RequestError::plain)? {
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
    Ok(())
}

/// Deliver as: the flow with the new delivery becomes the project's draft and is applied at once, a new flow version
/// as the flow editor's Save makes. A change the user is still making in the flow editor is theirs: the page then
/// says to finish it there first, and changes nothing.
async fn deliver(app: &App, project: &str, field: &str, value: &str) -> Result<(), RequestError> {
    let open = draft(app, Request::FlowDraft(ProjectParams {
        project: project.to_owned(),
    }))
    .await?;
    if open.proposal.id != 0 {
        return Err(RequestError::plain(
            "The flow has a change that isn\u{2019}t saved yet: save or cancel it in Edit flow first.",
        ));
    }
    let mut file = yaml::read(&open.proposal.yaml)
        .map_err(|e| RequestError::plain(format!("The flow doesn\u{2019}t read: {e}")))?;
    let delivery = deliver_as(&file.flow.delivery, field, value).map_err(RequestError::plain)?;
    if delivery == file.flow.delivery {
        return Ok(());
    }
    file.flow.delivery = delivery;
    let drafted = draft(
        app,
        Request::FlowDraftUpdate(DraftUpdateParams {
            project: project.to_owned(),
            edit: DraftEdit::Document {
                yaml: yaml::write(&file),
            },
        }),
    )
    .await?;
    let apply = Request::FlowApply(ApplyParams {
        project: project.to_owned(),
        proposal: drafted.proposal.id,
        choices: Default::default(),
    });
    match app.service.send(apply).await {
        Ok(Reply::FlowApplied(_)) => Ok(()),
        Ok(other) => Err(unexpected("flow.apply", &other)),
        Err(e) => {
            // Nothing is left behind in Edit flow's draft.
            let _ = app
                .service
                .send(Request::FlowCancelProposal(ProjectParams {
                    project: project.to_owned(),
                }))
                .await;
            Err(e)
        }
    }
}

async fn draft(app: &App, request: Request) -> Result<Draft, RequestError> {
    match app.service.send(request).await? {
        Reply::FlowDraft(draft) => Ok(*draft),
        other => Err(unexpected("flow.draft", &other)),
    }
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

/// The Remove project confirmation, with the project's ticket count.
async fn remove_preview(app: &App, key: &str) -> Result<Value, RequestError> {
    let info = projects(app)
        .await?
        .into_iter()
        .find(|i| i.key == key)
        .ok_or_else(|| RequestError::plain(format!("no project {key}")))?;
    let request = Request::BoardSnapshot(ProjectParams {
        project: key.to_owned(),
    });
    let tickets = match app.service.send(request).await? {
        Reply::Board(board) => board.tickets().count(),
        _ => 0,
    };
    reply(&page::remove_dialog(&info, tickets))
}

#[cfg(test)]
mod tests {
    use otaq_core::board::projects::AddProject;
    use otaq_core::flow::model::{Delivers, DeliveryMode};
    use otaq_core::flow::store::FlowVersion;
    use otaq_core::git::testing::TempRepos;
    use serde_json::json;

    use super::*;
    use crate::api::request;
    use crate::service::testing::{Harness, connected};

    async fn versions(h: &Harness, project: &str) -> Vec<FlowVersion> {
        match h
            .app
            .service
            .send(Request::FlowVersions(ProjectParams {
                project: project.to_owned(),
            }))
            .await
            .unwrap()
        {
            Reply::FlowVersions(v) => v,
            other => panic!("{other:?}"),
        }
    }

    /// Deliver as on the project's page changes the flow's file: a new flow version with the new delivery.
    #[tokio::test(flavor = "multi_thread")]
    async fn deliver_as_writes_a_new_flow_version_and_the_page_shows_it() {
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
        let page = request(&h.app, "project_settings.view", json!({ "project": project }))
            .await
            .unwrap();
        assert_eq!(page["nav"][1]["label"], "Repository");
        let before = versions(&h, &project).await.len();

        let edited = request(
            &h.app,
            "project_settings.edit",
            json!({ "project": project, "field": "delivery.delivers", "value": "pr" }),
        )
        .await
        .unwrap();
        let after = versions(&h, &project).await;
        assert_eq!(after.len(), before + 1, "a new flow version");
        let current = after.iter().find(|v| v.current).unwrap().file().unwrap();
        assert_eq!(current.flow.delivery.delivers, Delivers::Pr);
        let delivery = &edited["page"]["sections"][1]["rows"][2];
        assert_eq!(delivery["control"]["kind"], "delivery");
        assert_eq!(delivery["control"]["delivers_value"], "pr");

        request(
            &h.app,
            "project_settings.edit",
            json!({ "project": project, "field": "delivery.mode", "value": "automatic" }),
        )
        .await
        .unwrap();
        let current = versions(&h, &project).await;
        let file = current.iter().find(|v| v.current).unwrap().file().unwrap();
        assert_eq!(
            (file.flow.delivery.delivers, file.flow.delivery.mode),
            (Delivers::Pr, DeliveryMode::Automatic)
        );
        // The same value again writes nothing.
        request(
            &h.app,
            "project_settings.edit",
            json!({ "project": project, "field": "delivery.mode", "value": "automatic" }),
        )
        .await
        .unwrap();
        assert_eq!(versions(&h, &project).await.len(), current.len());

        // A project setting is saved as before.
        let edited = request(
            &h.app,
            "project_settings.edit",
            json!({ "project": project, "field": "setup_command", "value": "pnpm install" }),
        )
        .await
        .unwrap();
        let workspace = &edited["page"]["sections"][3]["rows"][0];
        assert_eq!(workspace["control"]["value"], "pnpm install");

        request(&h.app, "project_settings.remove", json!({ "project": project }))
            .await
            .unwrap();
        assert!(request(&h.app, "project_settings.view", json!({ "project": project }))
            .await
            .is_err());
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
                "project_settings.edit",
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
        edit("env_vars.1.name", json!("DEBUG")).await.unwrap();
        edit("env_vars.0.value", json!("1")).await.unwrap();
        let (stored, problems) = super::project_settings(&h.app, &project).await.unwrap();
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
        let open = || request(&h.app, "project_settings.view", json!({ "project": project }));
        let edit = |value: bool| {
            request(
                &h.app,
                "project_settings.edit",
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
        let (stored, _) = super::project_settings(&h.app, &project).await.unwrap();
        assert!(!stored.warm_builds);

        let control = switch(&edit(true).await.unwrap());
        assert_eq!(control["on"], true);
        assert_eq!(control["confirm"], Value::Null);
        h.service.stop_all().await;
    }
}
