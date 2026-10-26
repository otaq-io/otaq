//! `pause.*`: the title bar's Pause and Resume (T-0457; PAU-08; d0112, d0097; `pause.rs` in `otaq-view`).
//! `pause.view` answers what the button and its scope menu draw for the open project; `pause.run` is a pick in the menu
//! or the palette: it pauses or resumes at once, with no confirmation (nothing is lost), and answers the new view.
//! The counts are the service's (`agents.pause_counts`); the pause itself is the service's (`agents.pause`,
//! `agents.resume`), which agents can't send.

use otaq_core::agents::pause::{PauseCounts, PauseScope};
use otaq_core::protocol::{Reply, Request};
use otaq_view::view_models::pause::{self as vm, Scope};
use serde::Deserialize;
use serde_json::Value;
use ts_rs::TS;

use super::{RequestError, reply, unknown};
use crate::App;

pub async fn request(app: &App, method: &str, params: Value) -> Result<Value, RequestError> {
    match method {
        "pause.view" => reply(&view(app, parse(params)?).await?),
        "pause.run" => reply(&run(app, parse(params)?).await?),
        _ => Err(unknown(method)),
    }
}

fn parse<T: serde::de::DeserializeOwned>(params: Value) -> Result<T, RequestError> {
    serde_json::from_value(params).map_err(|e| RequestError::plain(e.to_string()))
}

/// What `pause.view` takes: the open project, if any.
#[derive(Debug, Clone, Default, Deserialize, TS)]
#[ts(rename = "PauseViewParams")]
pub struct ViewParams {
    #[serde(default)]
    #[ts(optional)]
    pub project: Option<String>,
}

/// What `pause.run` takes: the scope picked, the open project for This project, and whether it resumes.
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(rename = "PauseRunParams")]
pub struct RunParams {
    pub scope: Scope,
    #[serde(default)]
    #[ts(optional)]
    pub project: Option<String>,
    pub resume: bool,
}

/// Every project's counts (`agents.pause_counts` for All projects).
pub async fn counts(app: &App) -> Result<PauseCounts, RequestError> {
    match app
        .service
        .send(Request::AgentsPauseCounts(PauseScope::All))
        .await?
    {
        Reply::PauseCounts(counts) => Ok(*counts),
        other => Err(RequestError::plain(format!(
            "unexpected answer to agents.pause_counts: {other:?}"
        ))),
    }
}

pub async fn view(app: &App, params: ViewParams) -> Result<vm::View, RequestError> {
    let (counts, keys) = tokio::join!(counts(app), super::keymap::labels(app));
    Ok(vm::view(params.project.as_deref(), &counts?, &keys))
}

/// Pauses or resumes the scope at once (no confirmation, d0112), then the new view for the open project.
pub async fn run(app: &App, params: RunParams) -> Result<vm::View, RequestError> {
    let scope = match (params.scope, &params.project) {
        (Scope::Project, Some(project)) => PauseScope::Project {
            project: project.clone(),
        },
        (Scope::Project, None) => {
            return Err(RequestError::plain("This project needs the open project"));
        }
        (Scope::All, _) => PauseScope::All,
    };
    let request = if params.resume {
        Request::AgentsResume(scope)
    } else {
        Request::AgentsPause(scope)
    };
    app.service.send(request).await?;
    view(
        app,
        ViewParams {
            project: params.project,
        },
    )
    .await
}

#[cfg(test)]
pub fn export_types(cfg: &ts_rs::Config) -> Result<(), ts_rs::ExportError> {
    ViewParams::export_all(cfg)?;
    RunParams::export_all(cfg)?;
    vm::View::export_all(cfg)?;
    vm::Gallery::export_all(cfg)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use otaq_core::board::projects::AddProject;
    use otaq_core::git::testing::TempRepos;
    use serde_json::json;

    use super::*;
    use crate::api::request as table;
    use crate::service::testing::{Harness, connected};

    async fn add(h: &Harness, repos: &TempRepos, name: &str) -> String {
        let path = repos.repo(name, 1);
        match h
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
        }
    }

    /// PAU-08 against the real service: Pause on This project pauses at once (no confirmation) and the button turns
    /// into Resume; All projects pauses the other one too; Resume all lifts both.
    #[tokio::test(flavor = "multi_thread")]
    async fn pause_and_resume_from_the_title_bar() {
        let h = Harness::new(None);
        h.wait_for("connected", connected).await;
        let repos = TempRepos::new();
        let shop = add(&h, &repos, "shop-web").await;
        let bill = add(&h, &repos, "billing-api").await;

        let v = table(&h.app, "pause.view", json!({ "project": shop }))
            .await
            .unwrap();
        assert_eq!(v["button"]["label"], "Pause");
        assert_eq!(v["menu"]["rows"][0]["detail"], "shop-web \u{b7} 0 working");

        let v = table(
            &h.app,
            "pause.run",
            json!({ "scope": "project", "project": shop, "resume": false }),
        )
        .await
        .unwrap();
        assert_eq!(v["button"]["label"], "Resume");
        assert_eq!(v["menu"]["title"], "Resume agents");
        let other = table(&h.app, "pause.view", json!({ "project": bill }))
            .await
            .unwrap();
        assert_eq!(other["button"]["label"], "Pause", "billing-api keeps working");

        table(
            &h.app,
            "pause.run",
            json!({ "scope": "all", "project": bill, "resume": false }),
        )
        .await
        .unwrap();
        let all = counts(&h.app).await.unwrap();
        assert!(all.projects.iter().all(|p| p.paused_since.is_some()));
        // shop-web was paused on its own: Resume's All projects row says it stays paused (d0112 2026-10-27).
        let v = table(&h.app, "pause.view", json!({ "project": bill }))
            .await
            .unwrap();
        assert_eq!(
            v["menu"]["rows"][1]["detail"],
            "0 paused in 0 projects \u{b7} shop-web stays paused"
        );

        table(
            &h.app,
            "pause.run",
            json!({ "scope": "all", "resume": true }),
        )
        .await
        .unwrap();
        // Resume all lifts only what the All projects pause set; shop-web's own Resume lifts it.
        let all = counts(&h.app).await.unwrap();
        let paused: Vec<&str> = all
            .projects
            .iter()
            .filter(|p| p.paused_since.is_some())
            .map(|p| p.name.as_str())
            .collect();
        assert_eq!(paused, ["shop-web"]);
        table(
            &h.app,
            "pause.run",
            json!({ "scope": "project", "project": shop, "resume": true }),
        )
        .await
        .unwrap();
        let all = counts(&h.app).await.unwrap();
        assert!(all.projects.iter().all(|p| p.paused_since.is_none()));
        assert!(
            table(&h.app, "pause.run", json!({ "scope": "project", "resume": true }))
                .await
                .is_err(),
            "This project needs the open project"
        );
        h.service.stop_all().await;
    }
}
