//! `updates.*`: Settings → About's update parts and the rail's dot (UPD-04, UPD-05, d0140; page 32 items 1–2). The
//! page asks for the view (`updates.view`) and gets each change as an `updates` page event; Check for updates, the
//! switch and Skip this version answer with the view. What's new and the `.deb`'s Download open only the URL the
//! notice shows. Restart now is T-0581's.

use std::sync::Arc;

use otaq_view::view_models::connection::LinkState;
use otaq_view::view_models::updates::{self as vm, Context, NoticeAction, ServiceSide, View};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;

use super::settings::parse;
use super::{RequestError, reply, unknown};
use crate::App;
use crate::events::PageEvent;

/// `updates.auto`: Check for updates automatically.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(rename = "UpdatesAutoParams")]
pub struct AutoParams {
    pub on: bool,
}

/// `updates.open`: the notice's What's new, or its Download (the `.deb`'s release page).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename = "UpdatesOpen")]
pub enum Open {
    WhatsNew,
    Download,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(rename = "UpdatesOpenParams")]
pub struct OpenParams {
    pub what: Open,
}

pub async fn request(app: &App, method: &str, params: Value) -> Result<Value, RequestError> {
    match method {
        "updates.view" => reply(&view(app)),
        "updates.check" => {
            // Runs on its own; the page follows it through `updates` events.
            if let Some(source) = app.update_source.get().cloned() {
                let updates = app.updates.clone();
                tokio::spawn(async move { updates.check_now(source.as_ref()).await });
            }
            reply(&view(app))
        }
        "updates.auto" => {
            let p: AutoParams = parse(params)?;
            app.updates.set_auto(p.on).await;
            reply(&view(app))
        }
        "updates.skip" => {
            app.updates.skip().await;
            reply(&view(app))
        }
        "updates.open" => {
            let p: OpenParams = parse(params)?;
            let notice = view(app)
                .notice
                .ok_or_else(|| RequestError::plain("no update to open"))?;
            // What's new is the release notes; Download only on a `.deb`'s notice, the same page.
            if matches!(p.what, Open::Download) && notice.action != NoticeAction::Download {
                return Err(RequestError::plain("this update downloads by itself"));
            }
            open::that_detached(&notice.notes_url).map_err(|e| RequestError::plain(e.to_string()))?;
            reply(&Value::Null)
        }
        _ => Err(unknown(method)),
    }
}

/// The view now: the updater's state and preferences, this build, and the running service.
pub fn view(app: &App) -> View {
    let snapshot = app.updates.snapshot();
    let build = crate::about::build();
    let service = match &*app.service.states().borrow() {
        LinkState::Connected(info) => Some(ServiceSide {
            protocol_version: info.status.protocol_version,
            agents_working: info.status.agents_working,
        }),
        _ => None,
    };
    vm::view(
        &snapshot.state,
        &Context {
            current_version: build.version.clone(),
            commit: build.commit.clone(),
            now: otaq_core::store::now_ms(),
            auto_check: snapshot.prefs.auto_check,
            skipped: snapshot.prefs.skipped.clone(),
            service,
            app_protocol: otaq_core::protocol::PROTOCOL_VERSION,
        },
    )
}

/// Starts the updater on `source` and tells the page each change of the view: the updater's, and the service's
/// (its protocol number and agents working are in the notice).
pub fn start(
    app: &Arc<App>,
    source: Arc<dyn crate::updates::Source>,
    runtime: &tokio::runtime::Handle,
) {
    if app.update_source.set(source.clone()).is_err() {
        return;
    }
    runtime.spawn(app.updates.clone().run(source));
    let app = app.clone();
    runtime.spawn(async move {
        let mut changes = app.updates.changes();
        let mut states = app.service.states();
        let mut last = None;
        loop {
            let now = view(&app);
            if last.as_ref() != Some(&now) {
                app.events.publish(&PageEvent::Updates {
                    updates: Box::new(now.clone()),
                });
                last = Some(now);
            }
            tokio::select! {
                changed = changes.changed() => if changed.is_err() { return },
                changed = states.changed() => if changed.is_err() { return },
            }
        }
    });
}

#[cfg(test)]
pub fn export_types(cfg: &ts_rs::Config) -> Result<(), ts_rs::ExportError> {
    View::export_all(cfg)?;
    AutoParams::export_all(cfg)?;
    OpenParams::export_all(cfg)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::service::testing::Harness;

    #[tokio::test]
    async fn a_build_from_source_shows_its_line_and_no_dot() {
        let h = Harness::new(None);
        let v = crate::api::request(&h.app, "updates.view", Value::Null)
            .await
            .unwrap();
        assert!(v["row"]["plain"].as_str().unwrap().starts_with("Built from source · "));
        assert!(v["auto"].is_null() && v["notice"].is_null() && v["dot"].is_null(), "{v}");
        // Nothing to open, and no check runs.
        assert!(
            crate::api::request(&h.app, "updates.open", serde_json::json!({"what": "whats_new"}))
                .await
                .is_err()
        );
        assert!(
            crate::api::request(&h.app, "updates.check", Value::Null)
                .await
                .is_ok()
        );
    }
}
