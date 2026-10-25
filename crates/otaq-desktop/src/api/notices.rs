//! `notices.*`: the banner area and the toasts (T-0231, [`crate::notices`]), what their buttons do on the user's
//! click, and what a notification click opens. Notifications (ATT-13, ATT-14; d0090): the ask's Allow brings up
//! macOS's question in this process (the app, under the bundle's ID), never in the service; Not now only answers the
//! ask; Open System Settings opens Otaq's entry there.

use otaq_core::agent_tools::Tool;
use otaq_core::protocol::{
    ProjectParams, Reply, Request, ResumeAllParams, ToolParams,
};
use otaq_view::view_models::banners::{PERMISSION_DOCS_URL, Situation, SituationKind};
use otaq_view::view_models::limit_banner::{
    FALLBACK_FAILED, RESUME_ALL_FAILED, RESUME_FAILED, fallback_toast, resumed,
};
use otaq_view::view_models::restart::RestartDialog;
use otaq_view::view_models::toasts::{NewToast, ToastAction, ToastLevel};
use serde::Deserialize;
use serde_json::Value;
use ts_rs::TS;

use super::{RequestError, reply, unknown};
use crate::App;
use crate::notices::OpenTarget;

pub async fn request(app: &App, method: &str, params: Value) -> Result<Value, RequestError> {
    match method {
        "notices.view" => reply(&app.notices.view()),
        "notices.dismiss_banner" => dismiss_banner(app, parse(params)?).await,
        "notices.toast" => reply(&app.notices.toast(parse(params)?)),
        "notices.dismiss_toast" => {
            app.notices.dismiss_toast(parse::<ToastId>(params)?.id);
            Ok(Value::Null)
        }
        "notices.hold_toast" => {
            let hold: HoldToast = parse(params)?;
            app.notices.hold_toast(hold.id, hold.held);
            Ok(Value::Null)
        }
        "notices.toast_action" => toast_action(app, parse(params)?).await,
        "notices.restart_dialog" => {
            let (working, waiting) = app.notices.agent_counts();
            reply(&RestartDialog { working, waiting }.view())
        }
        "notices.restart" => restart(app).await,
        "notices.resume_after_limit" => resume_after_limit(app).await,
        "notices.resume_all" => resume_all(app).await,
        "notices.open_docs" => {
            open::that_detached(PERMISSION_DOCS_URL).map_err(|e| {
                RequestError::plain(format!("couldn't open {PERMISSION_DOCS_URL}: {e}"))
            })?;
            Ok(Value::Null)
        }
        "notices.permission_warning" => {
            app.notices.raise(Situation::PermissionWarning);
            Ok(Value::Null)
        }
        "notices.open_at_start" => reply(&app.notices.take_open_at_start()),
        "notices.open_target" => open_target(parse(params)?),
        "notices.notifications_allow" => allow_notifications(app).await,
        "notices.notifications_not_now" => {
            app.service.send(Request::NotificationsAskAnswered).await?;
            Ok(Value::Null)
        }
        "notices.open_notification_settings" => open_notification_settings(),
        // The flow change banner (FLW-06): the project on screen, and Cancel change (no question; starts resume).
        "notices.project" => {
            app.notices.set_project(parse::<NoticesProject>(params)?.project);
            Ok(Value::Null)
        }
        "notices.flow_cancel" => {
            let project = parse::<NoticesProject>(params)?
                .project
                .or_else(|| app.notices.project())
                .ok_or_else(|| RequestError::plain("no project on screen"))?;
            app.service
                .send(Request::FlowCancelWaiting(ProjectParams { project }))
                .await?;
            Ok(Value::Null)
        }
        _ => Err(unknown(method)),
    }
}

fn parse<T: for<'de> Deserialize<'de>>(params: Value) -> Result<T, RequestError> {
    serde_json::from_value(params).map_err(|e| RequestError::plain(e.to_string()))
}

/// `{"kind": "permission_warning"}`: the banner the user closed.
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(rename = "BannerDismiss")]
pub struct DismissBanner {
    pub kind: SituationKind,
}

/// `{"project": "SHOP"}`: the project on screen (`null` before one is added), or the one whose waiting flow change
/// Cancel change drops.
#[derive(Debug, Clone, Default, Deserialize, TS)]
pub struct NoticesProject {
    #[serde(default)]
    pub project: Option<String>,
}

/// `{"id": 3}`: a toast.
#[derive(Debug, Clone, Deserialize, TS)]
pub struct ToastId {
    pub id: u64,
}

/// `{"id": 3, "held": true}`: the pointer came onto a toast (or left it).
#[derive(Debug, Clone, Deserialize, TS)]
pub struct HoldToast {
    pub id: u64,
    pub held: bool,
}

/// `{"id": 3, "label": "Undo", "action": {"kind": "undo_epic_queue", …}}`: the user pressed a toast's button.
#[derive(Debug, Clone, Deserialize, TS)]
pub struct ToastPress {
    pub id: u64,
    pub action: ToastAction,
}

/// `{"target": {"kind": "ticket", "key": "SHOP-22"}}`: an `open_from_notification` event's target, as the service
/// sent it.
#[derive(Debug, Clone, Deserialize, TS)]
pub struct NotificationOpen {
    #[ts(type = "unknown")]
    pub target: otaq_core::attention::notify::Target,
}

/// Closing the permission-mode warning is remembered in the app's database (SET-01), and so is closing the folder
/// trust note (PRJ-08: never shown again).
async fn dismiss_banner(app: &App, params: DismissBanner) -> Result<Value, RequestError> {
    app.notices.dismiss_banner(params.kind);
    match params.kind {
        SituationKind::PermissionWarning => {
            app.service.send(Request::DismissPermissionWarning).await?;
        }
        SituationKind::TrustNote => {
            app.service.send(Request::DismissTrustNote).await?;
        }
        SituationKind::NotificationsBlocked => {
            app.service
                .send(Request::NotificationsDismissBlocked)
                .await?;
        }
        _ => {}
    }
    Ok(Value::Null)
}

/// How long the app waits for the user's answer to macOS's question before reading the permission anyway. macOS's
/// question stays up meanwhile (it goes with the asking process, so never ask from a short-lived one); an answer
/// after this is read when the window next gains focus.
#[cfg(target_os = "macos")]
const MACOS_ANSWER_WAIT: std::time::Duration = std::time::Duration::from_secs(15 * 60);

/// Allow (the ask) and Allow notifications (Settings): macOS's question, from this process under the bundle's ID,
/// then `notifications.ask_answered` at once (the ask goes) and again once macOS answered, so the service reads the
/// permission again. Outside the bundle nothing is asked (the API would end the process there).
pub(super) async fn allow_notifications(app: &App) -> Result<Value, RequestError> {
    #[cfg(target_os = "macos")]
    if let Some(in_bundle) = otaq_core::attention::notification_center::in_bundle() {
        let clients = app.service.clients();
        tokio::spawn(async move {
            let answer = tokio::task::spawn_blocking(move || {
                otaq_core::attention::notification_center::request_permission(
                    &in_bundle,
                    MACOS_ANSWER_WAIT,
                )
            })
            .await;
            if let Ok(Err(e)) = answer {
                eprintln!("macOS didn't answer the notifications question: {e}");
            }
            // Not connected: the next app focus reads the permission again.
            let client = clients.borrow().clone();
            if let Some(client) = client {
                let _ = client.request(Request::NotificationsAskAnswered).await;
            }
        });
    }
    app.service.send(Request::NotificationsAskAnswered).await?;
    Ok(Value::Null)
}

/// Otaq's entry in System Settings → Notifications (the blocked banner, the Settings status line). Where the URL
/// doesn't select the app, macOS opens the Notifications pane.
fn open_notification_settings() -> Result<Value, RequestError> {
    let url = notification_settings_url();
    open::that_detached(&url)
        .map_err(|e| RequestError::plain(format!("couldn't open System Settings: {e}")))?;
    Ok(Value::Null)
}

/// `x-apple.systempreferences:com.apple.Notifications-Settings.extension?id=<bundle ID>`, with the running bundle's
/// ID (Otaq's when outside one).
fn notification_settings_url() -> String {
    #[cfg(target_os = "macos")]
    let id = otaq_core::attention::notification_center::main_bundle_id()
        .unwrap_or_else(|| otaq_core::naming::BUNDLE_ID.to_owned());
    #[cfg(not(target_os = "macos"))]
    let id = otaq_core::naming::BUNDLE_ID.to_owned();
    format!("x-apple.systempreferences:com.apple.Notifications-Settings.extension?id={id}")
}

/// Restarts the service with this build, or starts it when stopped. Sent only after the user's click (confirmed
/// first when agents would be interrupted, d0022); the outcome shows as a toast or the summary banner.
async fn restart(app: &App) -> Result<Value, RequestError> {
    let before = app.notices.begin_restart();
    let outcome = app.service.restart().await;
    let now = app.service.states().borrow().clone();
    app.notices.end_restart(outcome, before, &now);
    Ok(Value::Null)
}

/// Resume on the banner's single button (USG-06, USG-10): the first line with Resume, as `limits.resume_tool`. A
/// line's own Resume sends `limits.resume_tool { tool }`.
async fn resume_after_limit(app: &App) -> Result<Value, RequestError> {
    let Some(tool) = app.notices.limited_tool() else {
        return Ok(Value::Null);
    };
    resume_tool(app, tool).await
}

/// Resume one tool after its limit (USG-10): the strip's limit item, a notice's Resume after reset (`limits.resume_tool`).
pub(super) async fn resume_tool(app: &App, tool: Tool) -> Result<Value, RequestError> {
    let toast = match app
        .service
        .send(Request::LimitsResumeTool(ToolParams { tool }))
        .await
    {
        Ok(Reply::Resumed(keys)) => {
            let (title, text) = resumed(keys.len());
            NewToast::new(ToastLevel::Success, title, Some(text))
        }
        Ok(_) => return Ok(Value::Null),
        Err(e) => NewToast::new(ToastLevel::Error, RESUME_FAILED, Some(e.message)),
    };
    app.notices.toast(toast);
    refresh_limits(app).await;
    Ok(Value::Null)
}

/// Use fallback now (AGM-03): `tool`'s waiting tickets start on its fallback (`limits.use_fallback_now`); a toast
/// says how many ("4 tickets started on Codex").
pub(super) async fn use_fallback_now(app: &App, tool: Tool) -> Result<Value, RequestError> {
    let toast = match app
        .service
        .send(Request::LimitsUseFallbackNow(ToolParams { tool }))
        .await
    {
        Ok(Reply::StartedOnFallback(keys)) => {
            let on = match app.service.send(Request::AgentsList).await {
                Ok(Reply::Agents(list)) => list
                    .fallbacks
                    .iter()
                    .find(|f| f.tool == tool)
                    .map(|f| f.fallback.tool),
                _ => None,
            };
            let title = match on {
                Some(on) => fallback_toast(keys.len(), on),
                None => format!("{} started on the fallback", keys.len()),
            };
            NewToast::new(ToastLevel::Success, title, None)
        }
        Ok(_) => return Ok(Value::Null),
        Err(e) => NewToast::new(ToastLevel::Error, FALLBACK_FAILED, Some(e.message)),
    };
    app.notices.toast(toast);
    refresh_limits(app).await;
    Ok(Value::Null)
}

/// Fetches the banner's lines again, after Resume or a move.
async fn refresh_limits(app: &App) {
    if let Ok(Reply::Usage(summary)) = app
        .service
        .send(Request::UsageSummary(Default::default()))
        .await
    {
        let inputs = crate::notices::limit_inputs(summary.tool_lines, |request| async move {
            app.service.send(request).await.ok()
        })
        .await;
        app.notices.on_usage(inputs);
    }
}

/// The user confirmed Resume all: every interrupted session continues, and the summary banner goes.
async fn resume_all(app: &App) -> Result<Value, RequestError> {
    app.notices.clear(SituationKind::SessionsInterrupted);
    if let Err(e) = app
        .service
        .send(Request::TicketsResumeAll(ResumeAllParams::default()))
        .await
    {
        app.notices.toast(NewToast::new(
            ToastLevel::Error,
            RESUME_ALL_FAILED,
            Some(e.message),
        ));
    }
    Ok(Value::Null)
}

/// A toast's button, on the user's click: the toast goes, and what the action did shows in its place (EPC-15).
async fn toast_action(app: &App, press: ToastPress) -> Result<Value, RequestError> {
    app.notices.dismiss_toast(press.id);
    // No toast offers an action any more (the epic queue's Undo went with epics, d0133).
    match press.action {
        ToastAction::UndoEpicQueue { .. } => {}
    }
    Ok(Value::Null)
}

fn open_target(params: NotificationOpen) -> Result<Value, RequestError> {
    reply(&OpenTarget::from(params.target))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::api::request;
    use crate::service::testing::{Harness, connected};

    /// T-0372: Not now and Allow answer the ask (outside the bundle Allow asks macOS nothing); the blocked banner's
    /// close is the service's; Open System Settings names the app's entry.
    #[tokio::test(flavor = "multi_thread")]
    async fn the_notification_buttons_answer_the_service() {
        let h = Harness::new(None);
        h.wait_for("connected", connected).await;
        for method in ["notices.notifications_not_now", "notices.notifications_allow"] {
            assert_eq!(request(&h.app, method, Value::Null).await.unwrap(), Value::Null);
        }
        request(
            &h.app,
            "notices.dismiss_banner",
            json!({"kind": "notifications_blocked"}),
        )
        .await
        .unwrap();
        let state = h.app.service.send(Request::NotificationsState).await.unwrap();
        let Reply::NotificationsState(state) = state else {
            panic!("{state:?}")
        };
        assert!(!state.ask_pending && !state.blocked_banner);
        assert!(
            notification_settings_url().starts_with(
                "x-apple.systempreferences:com.apple.Notifications-Settings.extension?id="
            )
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn the_banner_area_and_the_toasts_through_the_table() {
        let h = Harness::new(None);
        h.wait_for("connected", connected).await;

        let view = request(&h.app, "notices.view", Value::Null).await.unwrap();
        assert_eq!(view["banners"], json!([]));
        assert_eq!(view["only_restart_allowed"], false);

        let id = request(
            &h.app,
            "notices.toast",
            json!({"level": "error", "title": "gh is not signed in",
                   "link": {"kind": "copy", "label": "Copy command", "text": "gh auth login"}}),
        )
        .await
        .unwrap();
        h.wait_for_event("the toast", |e| {
            e["kind"] == "notices"
                && e["notices"]["toasts"]
                    .as_array()
                    .is_some_and(|t| t.iter().any(|t| t["title"] == "gh is not signed in"))
        })
        .await;
        request(&h.app, "notices.hold_toast", json!({"id": id, "held": true}))
            .await
            .unwrap();
        request(&h.app, "notices.dismiss_toast", json!({"id": id}))
            .await
            .unwrap();
        let view = request(&h.app, "notices.view", Value::Null).await.unwrap();
        assert!(
            view["toasts"]
                .as_array()
                .unwrap()
                .iter()
                .all(|t| t["id"] != id)
        );

        // The permission-mode warning: raised after the first run, closed for good.
        request(&h.app, "notices.permission_warning", Value::Null)
            .await
            .unwrap();
        let view = request(&h.app, "notices.view", Value::Null).await.unwrap();
        assert_eq!(view["banners"][0]["kind"], "permission_warning");
        assert_eq!(view["banners"][0]["buttons"][0]["label"], "Read the docs");
        request(
            &h.app,
            "notices.dismiss_banner",
            json!({"kind": "permission_warning"}),
        )
        .await
        .unwrap();
        let view = request(&h.app, "notices.view", Value::Null).await.unwrap();
        assert_eq!(view["banners"], json!([]));
        match h.app.service.send(Request::OnboardingState).await.unwrap() {
            Reply::OnboardingState(state) => assert!(state.permission_warning_dismissed),
            other => panic!("{other:?}"),
        }

        let dialog = request(&h.app, "notices.restart_dialog", Value::Null)
            .await
            .unwrap();
        assert_eq!(dialog["title"], "Restart the background service?");
        assert_eq!(dialog["confirm"], "Restart service");

        assert_eq!(
            request(
                &h.app,
                "notices.open_target",
                json!({"target": {"kind": "ticket", "key": "SHOP-22"}})
            )
            .await
            .unwrap(),
            json!({"kind": "ticket", "project": "SHOP", "ticket": "SHOP-22"})
        );
        assert_eq!(
            request(
                &h.app,
                "notices.open_target",
                json!({"target": {"kind": "project_agent", "project": "SHOP", "agent": "pa7"}})
            )
            .await
            .unwrap(),
            json!({"kind": "project_agent", "project": "SHOP", "agent": "pa7"})
        );
        assert_eq!(
            request(
                &h.app,
                "notices.open_target",
                json!({"target": {"kind": "needs_you"}})
            )
            .await
            .unwrap(),
            json!({"kind": "needs_you"})
        );
        assert!(
            request(&h.app, "notices.open_target", json!({"key": "SHOP-22"}))
                .await
                .is_err()
        );
        assert_eq!(
            request(&h.app, "notices.open_at_start", Value::Null)
                .await
                .unwrap(),
            Value::Null
        );
        h.service.stop_all().await;
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn restart_now_restarts_only_when_asked_and_says_how_it_went() {
        let h = Harness::new(Some("older.1"));
        h.wait_for("connected", connected).await;
        // The in-process service runs another build: the update-ready banner, and nothing restarts by itself.
        h.wait_for_event("the update banner", |e| {
            e["kind"] == "notices" && e["notices"]["banners"][0]["kind"] == "service_update_ready"
        })
        .await;
        assert_eq!(h.service.started.lock().unwrap().len(), 1);
        request(&h.app, "notices.restart", Value::Null)
            .await
            .unwrap();
        h.wait_for_event("the restart toast", |e| {
            e["kind"] == "notices"
                && e["notices"]["toasts"].as_array().is_some_and(|t| {
                    t.iter()
                        .any(|t| t["title"] == "Background service restarted")
                })
        })
        .await;
        h.wait_for_event("a busy button", |e| {
            e["kind"] == "notices" && e["notices"]["banners"][0]["buttons"][0]["label"] == "Restarting…"
        })
        .await;
        assert_eq!(
            h.service.started.lock().unwrap().len(),
            2,
            "a new service was started"
        );
        h.service.stop_all().await;
    }
}
