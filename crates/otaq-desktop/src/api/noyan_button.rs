//! `noyan_button.*`: the rail's Noyan button (T-0458; SHB-02, SHB-03; d0113; `noyan_button.rs` in `otaq-view`).
//! `noyan_button.view` answers what the button draws for a project (its tint and its dot); `noyan_button.looked` says
//! the panel shows a conversation, which clears its "New reply". Whether the panel is open and what it shows are the
//! page's own state, passed in.

use otaq_view::view_models::noyan_button::{self as vm, NoyanButton};
use serde::Deserialize;
use serde_json::Value;
use ts_rs::TS;

use super::{RequestError, reply, unknown};
use crate::App;

pub async fn request(app: &App, method: &str, params: Value) -> Result<Value, RequestError> {
    match method {
        "noyan_button.view" => reply(&view(app, parse(params)?).await?),
        "noyan_button.looked" => {
            let params: LookedParams = parse(params)?;
            app.looked
                .lock()
                .expect("looked")
                .look(&params.agent, otaq_core::store::now_ms());
            Ok(Value::Null)
        }
        _ => Err(unknown(method)),
    }
}

fn parse<T: serde::de::DeserializeOwned>(params: Value) -> Result<T, RequestError> {
    serde_json::from_value(params).map_err(|e| RequestError::plain(e.to_string()))
}

/// What `noyan_button.view` takes: the project and the panel's state.
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(rename = "NoyanButtonParams")]
pub struct ViewParams {
    pub project: String,
    /// The panel is open.
    #[serde(default)]
    pub open: bool,
    /// The conversation the panel shows.
    #[serde(default)]
    #[ts(optional)]
    pub shown: Option<String>,
}

pub async fn view(app: &App, params: ViewParams) -> Result<NoyanButton, RequestError> {
    let agents = super::agent_panel::agents(app, &params.project).await?;
    let keys = super::keymap::labels(app).await;
    let looked = app.looked.lock().expect("looked").clone();
    Ok(vm::view(&vm::Facts {
        agents: &agents.open,
        looked: &looked,
        open: params.open,
        shown: params.shown.as_deref(),
        keys: &keys,
    }))
}

/// What `noyan_button.looked` takes: the conversation the open panel shows.
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(rename = "NoyanLookedParams")]
pub struct LookedParams {
    pub agent: String,
}

#[cfg(test)]
pub fn export_types(cfg: &ts_rs::Config) -> Result<(), ts_rs::ExportError> {
    ViewParams::export_all(cfg)?;
    LookedParams::export_all(cfg)?;
    NoyanButton::export_all(cfg)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use otaq_core::board::projects::AddProject;
    use otaq_core::git::testing::TempRepos;
    use otaq_core::protocol::{Reply, Request};
    use serde_json::json;

    use crate::api::request as table;
    use crate::service::testing::{Harness, connected};

    /// The button answers for a project with no conversation, tinted while the panel is open; `looked` is taken.
    #[tokio::test(flavor = "multi_thread")]
    async fn the_button_answers_and_looked_is_kept() {
        let h = Harness::new(None);
        h.wait_for("connected", connected).await;
        let repos = TempRepos::new();
        let path = repos.repo("pocket-notes", 1);
        let project = match h
            .app
            .service
            .send(Request::ProjectAdd(AddProject {
                path: path.display().to_string(),
                key: None,
                base_branch: None,
            }))
            .await
            .unwrap()
        {
            Reply::ProjectAdded { project: p, .. } => p.key,
            other => panic!("{other:?}"),
        };
        let view = table(&h.app, "noyan_button.view", json!({ "project": project }))
            .await
            .unwrap();
        assert_eq!(view["dot"], false);
        assert_eq!(view["tinted"], false);
        let open = table(
            &h.app,
            "noyan_button.view",
            json!({ "project": project, "open": true }),
        )
        .await
        .unwrap();
        assert_eq!(open["tinted"], true);
        table(&h.app, "noyan_button.looked", json!({ "agent": "a1" }))
            .await
            .unwrap();
        assert!(h.app.looked.lock().unwrap().last("a1") > 0);
        h.service.stop_all().await;
    }
}
