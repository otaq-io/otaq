//! `strip.*`: the Needs you strip and Next (T-0459; NYS-01, NYS-02, NYS-03; d0114, d0016, d0083; `needs_you_strip.rs`
//! in `otaq-view`). `strip.view` answers the open project's strip with what the page shows now; `strip.next` is ⌘J
//! (and the strip's Next): where it goes from there, in this project or the next one with items, or nothing. Next only
//! answers where to go; the page opens it, so nothing moves on by itself (d0016). `strip.resume_all_confirm` words
//! Resume all's confirmation and `strip.resume_all` is its confirm (a project's, or every project's for the banner).
//! The items, their order and the back-pressure count are the service's (`needs_you.list`).

use otaq_core::attention::needs_you::NeedsYouList;
use otaq_core::board::snapshot::ProjectInfo;
use otaq_core::protocol::{Reply, Request, ResumeAllParams};
use otaq_view::keymap::KeyAction;
use otaq_view::view_models::needs_you_strip::{
    self as vm, Item, Open, ResumeAllDialog, Target, View,
};
use otaq_view::view_models::toasts::{NewToast, ToastLevel};
use serde::Deserialize;
use serde_json::Value;
use ts_rs::TS;

use super::{RequestError, reply, unknown};
use crate::App;

pub async fn request(app: &App, method: &str, params: Value) -> Result<Value, RequestError> {
    match method {
        "strip.view" => reply(&view(app, parse(params)?).await?),
        "strip.next" => reply(&next(app, parse(params)?).await?),
        "strip.resume_all_confirm" => reply(&resume_all_confirm(app, parse(params)?).await?),
        "strip.resume_all" => {
            resume_all(app, parse(params)?).await;
            Ok(Value::Null)
        }
        _ => Err(unknown(method)),
    }
}

fn parse<T: serde::de::DeserializeOwned>(params: Value) -> Result<T, RequestError> {
    serde_json::from_value(params).map_err(|e| RequestError::plain(e.to_string()))
}

/// What `strip.view` and `strip.next` take: the open project and what the page shows (the ticket open, or the
/// Project agent in the panel), which Next moves on from.
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(rename = "StripParams")]
pub struct Params {
    pub project: String,
    #[serde(default)]
    #[ts(optional)]
    pub open: Option<Open>,
}

async fn list(app: &App) -> Result<NeedsYouList, RequestError> {
    match app.service.send(Request::NeedsYouList).await? {
        Reply::NeedsYou(list) => Ok(*list),
        other => Err(RequestError::plain(format!(
            "unexpected answer to needs_you.list: {other:?}"
        ))),
    }
}

/// The projects in the rail's order.
async fn projects(app: &App) -> Result<Vec<ProjectInfo>, RequestError> {
    match app.service.send(Request::ProjectList).await? {
        Reply::Projects(list) => Ok(list),
        other => Err(RequestError::plain(format!(
            "unexpected answer to project.list: {other:?}"
        ))),
    }
}

/// Every item and the projects' keys in the rail's order.
async fn items(app: &App) -> Result<(Vec<Item>, Vec<ProjectInfo>), RequestError> {
    let (list, projects) = tokio::join!(list(app), projects(app));
    let projects = projects?;
    // What a pause stopped waits for its Resume, not the strip (PAU-08).
    let items = otaq_view::view_models::pause::strip_items(vm::items(&list?), &projects);
    Ok((items, projects))
}

fn keys_of(projects: &[ProjectInfo]) -> Vec<String> {
    projects.iter().map(|p| p.key.clone()).collect()
}

pub async fn view(app: &App, params: Params) -> Result<View, RequestError> {
    let (items, projects) = items(app).await?;
    let name = projects
        .iter()
        .find(|p| p.key == params.project)
        .map_or(params.project.as_str(), |p| p.name.as_str());
    let keys = super::keymap::labels(app)
        .await
        .label(KeyAction::NextNeedsYouItem);
    Ok(vm::view(
        &items,
        &params.project,
        name,
        params.open.as_ref(),
        &keys_of(&projects),
        keys.as_deref(),
    ))
}

/// ⌘J and the strip's Next (NYS-02): what to open, `null` when nothing waits anywhere.
pub async fn next(app: &App, params: Params) -> Result<Option<Target>, RequestError> {
    let (items, projects) = items(app).await?;
    Ok(vm::next(
        &items,
        &params.project,
        params.open.as_ref(),
        &keys_of(&projects),
    )
    .map(|step| step.item().target()))
}

/// What `strip.resume_all_confirm` and `strip.resume_all` take: the strip's project, or none for every project (the
/// "sessions were interrupted" banner).
#[derive(Debug, Clone, Default, Deserialize, TS)]
#[ts(rename = "StripResumeAllParams")]
pub struct ResumeAllRequest {
    #[serde(default)]
    #[ts(optional)]
    pub project: Option<String>,
}

/// Resume all's confirmation (*10 → Resume all · confirm*); `null` when nothing is left to resume.
pub async fn resume_all_confirm(
    app: &App,
    params: ResumeAllRequest,
) -> Result<Option<ResumeAllDialog>, RequestError> {
    let items = vm::items(&list(app).await?);
    Ok(vm::resume_all_dialog(&items, params.project.as_deref()))
}

/// The confirm of Resume all for one project (d0045): its interrupted sessions continue. A refusal toasts.
async fn resume_all(app: &App, params: ResumeAllRequest) {
    let request = Request::TicketsResumeAll(ResumeAllParams {
        project: params.project,
    });
    if let Err(e) = app.service.send(request).await {
        app.notices.toast(NewToast::new(
            ToastLevel::Error,
            "Couldn\u{2019}t resume the interrupted sessions",
            Some(e.message),
        ));
    }
}

#[cfg(test)]
pub fn export_types(cfg: &ts_rs::Config) -> Result<(), ts_rs::ExportError> {
    Params::export_all(cfg)?;
    View::export_all(cfg)?;
    Target::export_all(cfg)?;
    ResumeAllRequest::export_all(cfg)?;
    ResumeAllDialog::export_all(cfg)?;
    vm::Gallery::export_all(cfg)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::api::request as table;
    use crate::service::testing::{Harness, connected};

    #[tokio::test(flavor = "multi_thread")]
    async fn the_strip_with_a_real_service() {
        let h = Harness::new(None);
        h.wait_for("connected", connected).await;
        let view = table(&h.app, "strip.view", json!({ "project": "SHOP" }))
            .await
            .unwrap();
        assert_eq!(view["empty"]["title"], "Needs you \u{b7} 0 here");
        assert_eq!(view["empty"]["text"], Value::Null);
        assert_eq!(view["groups"], json!([]));
        assert_eq!(view["next"], Value::Null);
        // Nothing waits anywhere: Next opens nothing.
        let next = table(
            &h.app,
            "strip.next",
            json!({ "project": "SHOP", "open": { "kind": "ticket", "key": "SHOP-1" } }),
        )
        .await
        .unwrap();
        assert_eq!(next, Value::Null);
        let confirm = table(
            &h.app,
            "strip.resume_all_confirm",
            json!({ "project": "SHOP" }),
        )
        .await
        .unwrap();
        assert_eq!(confirm, Value::Null);
        let none = table(&h.app, "strip.resume_all_confirm", json!({}))
            .await
            .unwrap();
        assert_eq!(none, Value::Null);
        h.service.stop_all().await;
    }
}
