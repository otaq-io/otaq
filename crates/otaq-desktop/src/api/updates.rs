//! `updates.*`: Settings → About's update parts and the rail's dot (UPD-04, UPD-05, d0140; page 32 items 1–2). The
//! page asks for the view (`updates.view`) and gets each change as an `updates` page event; Check for updates, the
//! switch and Skip this version answer with the view. What's new and the `.deb`'s Download open only the URL the
//! notice shows. Restart now (`updates.restart`) and the status icon's Restart to update put the kept update in place
//! ([`restart`]); after the reopen a one-time toast says "Updated to Otaq 0.2.0" (UPD-05, page 32 items 3–4).

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
use crate::updates::{Installer, Relaunch};
use otaq_view::view_models::toasts::{NewToast, ToastLevel, ToastLink};

/// `updates.auto`: Check for updates automatically.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(rename = "UpdatesAutoParams")]
pub struct AutoParams {
    pub on: bool,
}

/// `updates.release_notes`: a version's release page (the toast's What's new).
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(rename = "UpdatesNotesParams")]
pub struct NotesParams {
    pub version: String,
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
        "updates.restart" => {
            restart(app, vm::Trigger::About).await;
            reply(&view(app))
        }
        // The toast's What's new (page 32 item 4): that version's release page, nothing else.
        "updates.release_notes" => {
            let p: NotesParams = parse(params)?;
            open::that_detached(vm::release_page(&p.version))
                .map_err(|e| RequestError::plain(e.to_string()))?;
            reply(&Value::Null)
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

/// The running service, as the link last heard: its protocol number and agents working.
fn service_side(app: &App) -> Option<ServiceSide> {
    match &*app.service.states().borrow() {
        LinkState::Connected(info) => Some(ServiceSide {
            protocol_version: info.status.protocol_version,
            agents_working: info.status.agents_working,
        }),
        _ => None,
    }
}

fn context(app: &App) -> Context {
    let snapshot = app.updates.snapshot();
    let build = crate::about::build();
    Context {
        current_version: build.version.clone(),
        commit: build.commit.clone(),
        now: otaq_core::store::now_ms(),
        auto_check: snapshot.prefs.auto_check,
        skipped: snapshot.prefs.skipped.clone(),
        service: service_side(app),
        app_protocol: otaq_core::protocol::PROTOCOL_VERSION,
        installing: snapshot.installing,
        unreachable_at: snapshot
            .prefs
            .last_check
            .filter(|check| !check.reached)
            .map(|check| check.at),
    }
}

/// The view now: the updater's state and preferences, this build, and the running service.
pub fn view(app: &App) -> View {
    vm::view(&app.updates.snapshot().state, &context(app))
}

/// The status icon's Restart to update, while an update is ready and not skipped.
pub fn restart_label(app: &App) -> Option<String> {
    vm::restart_item(&app.updates.snapshot().state, &context(app))
}

/// Restart now (About) or Restart to update (the menu): puts the kept update in place, stopping the service first
/// when it can't work with the new version, then reopens the app. On a failure the app goes on and About says why.
pub async fn restart(app: &App, trigger: vm::Trigger) {
    let Some((installer, relaunch)) = app.update_installer.get().cloned() else {
        return;
    };
    let protocol = service_side(app).map(|s| s.protocol_version);
    let placed = app
        .updates
        .put_in_place(trigger, protocol, &app.service, installer.as_ref())
        .await;
    match placed {
        crate::updates::Placed::Relaunch => relaunch.relaunch(None),
        // The release couldn't confirm it (offline, or a newer version is out): a check now says which, and
        // downloads the newer one.
        crate::updates::Placed::Offline => {
            if let Some(source) = app.update_source.get() {
                app.updates.check_now(source.as_ref()).await;
            }
        }
        _ => {}
    }
}

/// Starts the updater on `source` and tells the page each change of the view: the updater's, and the service's
/// (its protocol number and agents working are in the notice).
pub fn start(
    app: &Arc<App>,
    source: Arc<dyn crate::updates::Source>,
    installer: (Arc<dyn Installer>, Arc<dyn Relaunch>),
    runtime: &tokio::runtime::Handle,
) {
    if app.update_source.set(source.clone()).is_err() {
        return;
    }
    let _ = app.update_installer.set(installer);
    runtime.spawn(app.updates.clone().run(source));
    let app = app.clone();
    runtime.spawn(async move {
        let mut changes = app.updates.changes();
        let mut states = app.service.states();
        let mut last = None;
        loop {
            // Once, after starting on a newer version (page 32 item 4); the Service update ready banner follows as
            // before.
            if let Some((title, text)) = app.updates.take_updated_toast() {
                let mut toast = NewToast::new(ToastLevel::Success, title, Some(text));
                toast.link = Some(ToastLink::ReleaseNotes {
                    label: vm::WHATS_NEW.to_owned(),
                    version: crate::about::build().version.clone(),
                });
                app.notices.toast(toast);
            }
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
    NotesParams::export_all(cfg)?;
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
