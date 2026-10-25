//! `recovery.*`: Recovery and Cleanup (T-0235, T-0244; REC-02, REC-03, REC-04, REC-06, SES-02; d0018, d0022, d0045; *16 ·
//! Recovery & cleanup*). `recovery.summary` draws the after-restart sheet from the service's startup report, and
//! `recovery.resume` / `recovery.retry` are its buttons; `recovery.scratch` words Start from scratch's confirmation
//! and `recovery.start_from_scratch` is its confirm. The Cleanup overlay keeps its scan and selection here between
//! requests (`cleanup.rs`'s model): `recovery.cleanup` scans or redraws, the toggles tick, and only
//! `recovery.cleanup_delete`, the confirmation's button, deletes. The warm builds' section (WRM-07, d0082) comes
//! from `warm.list`, not the scan, so they are never leftovers; `recovery.warm_clear` is a row's confirmed Clear….
//! Every text is `otaq-view`'s.

use std::collections::{BTreeMap, BTreeSet};

use otaq_core::board::tickets::Ticket;
use otaq_core::protocol::{
    CleanupDeleteParams, CloseParams, IdParams, KeyParams, ProjectParams, Reply, Request,
    SessionListParams, WarmClearParams,
};
use otaq_core::session::SessionKind;
use otaq_core::recovery::StartupReport;
use otaq_core::warm::store::{ClearOutcome, WarmList};
use otaq_view::view_models::after_restart::{self, RanOn, Runs, SummaryPage};
use otaq_view::view_models::cleanup::{self, Page as CleanupPage, TargetPage};
use otaq_view::view_models::recovery::{self, ScratchConfirm};
use otaq_view::view_models::toasts::{NewToast, ToastLevel};
use otaq_view::view_models::warm_build;
use serde::Deserialize;
use serde_json::Value;
use ts_rs::TS;

use super::{RequestError, reply, unknown};
use crate::App;

pub async fn request(app: &App, method: &str, params: Value) -> Result<Value, RequestError> {
    match method {
        "recovery.summary" => reply(&summary(app, parse(params)?).await?),
        "recovery.resume" => reply(&resume(app, parse(params)?).await?),
        "recovery.retry" => reply(&retry(app, parse(params)?).await?),
        "recovery.resume_agent" => reply(&resume_agent(app, parse(params)?).await?),
        "recovery.scratch" => reply(&scratch(app, parse(params)?).await?),
        "recovery.start_from_scratch" => reply(&start_from_scratch(app, parse(params)?).await?),
        "recovery.cleanup" => reply(&cleanup(app, parse(params)?).await?),
        "recovery.cleanup_toggle" => reply(&cleanup_toggle(app, parse(params)?)?),
        "recovery.cleanup_toggle_group" => reply(&cleanup_toggle_group(app, parse(params)?)?),
        "recovery.cleanup_delete" => reply(&cleanup_delete(app).await?),
        "recovery.warm_clear" => reply(&warm_clear(app, parse(params)?).await?),
        _ => Err(unknown(method)),
    }
}

fn parse<T: serde::de::DeserializeOwned>(params: Value) -> Result<T, RequestError> {
    serde_json::from_value(params).map_err(|e| RequestError::plain(e.to_string()))
}

fn unexpected(what: &str, other: &Reply) -> RequestError {
    RequestError::plain(format!("unexpected answer to {what}: {other:?}"))
}

/// `{"unchecked": ["SHOP-19"], "unfolded": false}`: what the user unticked and whether "+3" was opened.
#[derive(Debug, Clone, Default, Deserialize, TS)]
#[ts(rename = "AfterRestartParams")]
pub struct SummaryParams {
    #[serde(default)]
    pub unchecked: Vec<String>,
    #[serde(default)]
    pub unfolded: bool,
}

/// `{"keys": ["SHOP-25", "SHOP-19"]}`: the tickets to resume, in order.
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(rename = "RecoveryResumeParams")]
pub struct ResumeParams {
    pub keys: Vec<String>,
}

/// `{"scan": true}` scans again (and lists the warm builds); `{"warm": true}` lists only the warm builds again (on
/// `warm_build_changed`); otherwise the last scan is drawn.
#[derive(Debug, Clone, Default, Deserialize, TS)]
#[ts(rename = "CleanupParams")]
pub struct CleanupParams {
    #[serde(default)]
    pub scan: bool,
    #[serde(default)]
    #[ts(optional)]
    pub warm: Option<bool>,
}

/// `{"repo": "shop-web"}`: the warm build's folder, as its row names it.
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(rename = "CleanupWarmClearParams")]
pub struct WarmParams {
    pub repo: String,
}

/// `{"target": {"group": "worktrees", "id": "…"}}`.
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(rename = "CleanupToggleParams")]
pub struct ToggleParams {
    pub target: TargetPage,
}

/// `{"group": "worktrees"}`.
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(rename = "CleanupGroupParams")]
pub struct GroupParams {
    pub group: String,
}

async fn ticket(app: &App, key: &str) -> Result<Option<Ticket>, RequestError> {
    match app
        .service
        .send(Request::TicketGet(KeyParams {
            key: key.to_owned(),
        }))
        .await
    {
        Ok(Reply::Ticket(t)) => Ok(Some(*t)),
        Ok(other) => Err(unexpected("ticket.get", &other)),
        Err(e) if e.not_found => Ok(None),
        Err(e) => Err(e),
    }
}

/// The after-restart sheet, or `null` when this service start interrupted nothing.
pub async fn summary(app: &App, p: SummaryParams) -> Result<Option<SummaryPage>, RequestError> {
    // A paused project's tickets wait for its Resume, not this sheet (PAU-07).
    let report = match app.service.send(Request::StartupReport).await? {
        Reply::StartupReport(r) => after_restart::outside_pause(&r),
        other => return Err(unexpected("service.startup_report", &other)),
    };
    let StartupReport {
        interrupted,
        session_ended,
        ..
    } = &report;
    let mut tickets = Vec::new();
    for key in interrupted.iter().chain(session_ended) {
        if let Some(t) = ticket(app, &key.to_string()).await? {
            tickets.push(t);
        }
    }
    let projects: BTreeMap<String, String> = match app.service.send(Request::ProjectList).await? {
        Reply::Projects(list) => list.into_iter().map(|p| (p.key, p.name)).collect(),
        other => return Err(unexpected("project.list", &other)),
    };
    // The Project agents the restart ended, as they are now (T-0244).
    let mut agents = Vec::new();
    if !report.project_agents_ended.is_empty() {
        for project in projects.keys() {
            if let Reply::ProjectAgents(list) = app
                .service
                .send(Request::ProjectAgentList(ProjectParams {
                    project: project.clone(),
                }))
                .await?
            {
                agents.extend(
                    list.open
                        .into_iter()
                        .chain(list.recent)
                        .filter(|a| report.project_agents_ended.contains(&a.id)),
                );
            }
        }
    }
    let unchecked: BTreeSet<String> = p.unchecked.into_iter().collect();
    // What each interrupted ticket ran on, and which tools can resume now (M23, d0086).
    let mut runs = Runs {
        tools: super::editor::agent_tools(app).await,
        tickets: BTreeMap::new(),
    };
    for key in interrupted {
        if let Some(ran) = ran_on(app, &key.to_string()).await {
            runs.tickets.insert(key.to_string(), ran);
        }
    }
    Ok(after_restart::summary_with(
        &report,
        &tickets,
        &agents,
        &projects,
        &unchecked,
        p.unfolded,
        &runs,
    ))
}

/// The tool and named agent of a ticket's newest agent session (its own, a validator's, an answer's or a PR's).
async fn ran_on(app: &App, key: &str) -> Option<RanOn> {
    let Ok(Reply::Sessions(sessions)) = app
        .service
        .send(Request::SessionList(SessionListParams {
            key: Some(key.to_owned()),
        }))
        .await
    else {
        return None;
    };
    sessions
        .into_iter()
        .filter(|s| {
            matches!(
                s.kind,
                SessionKind::Agent | SessionKind::Validator | SessionKind::Answer | SessionKind::Delivery
            )
        })
        .max_by_key(|s| s.started_at)
        .and_then(|s| {
            Some(RanOn {
                tool: s.tool?,
                agent: s.agent.map(|a| a.name),
            })
        })
}

/// Resume, and Resume selected: each ticket in turn (`ticket.resume`, the user's click, d0045). One that can't be
/// resumed any more is skipped with an error toast; the rest still go.
pub async fn resume(app: &App, p: ResumeParams) -> Result<u32, RequestError> {
    let mut resumed = 0;
    for key in p.keys {
        match app
            .service
            .send(Request::TicketResume(KeyParams { key: key.clone() }))
            .await
        {
            Ok(_) => resumed += 1,
            Err(e) => {
                app.notices.toast(NewToast::new(
                    ToastLevel::Error,
                    format!("Couldn’t resume {key}"),
                    Some(e.message),
                ));
            }
        }
    }
    Ok(resumed)
}

/// Retry after Resume failed: the same resume again (REC-03). A refusal is an error toast.
pub async fn retry(app: &App, p: KeyParams) -> Result<(), RequestError> {
    let key = p.key.clone();
    if let Err(e) = app.service.send(Request::TicketRetryResume(p)).await {
        app.notices.toast(NewToast::new(
            ToastLevel::Error,
            format!("Couldn’t resume {key}"),
            Some(e.message),
        ));
    }
    Ok(())
}

/// A Project agent's Resume in the sheet: the same conversation, on the user's click (d0068, d0045).
pub async fn resume_agent(app: &App, p: IdParams) -> Result<(), RequestError> {
    if let Err(e) = app.service.send(Request::ProjectAgentResume(p)).await {
        app.notices.toast(NewToast::new(
            ToastLevel::Error,
            "Noyan didn\u{2019}t resume",
            Some(e.message),
        ));
    }
    Ok(())
}

/// Start from scratch's confirmation: what is kept and what is lost.
pub async fn scratch(app: &App, p: KeyParams) -> Result<ScratchConfirm, RequestError> {
    match ticket(app, &p.key).await? {
        Some(t) => Ok(recovery::scratch_confirm(&t)),
        None => Err(RequestError::plain(format!("{} no longer exists", p.key))),
    }
}

/// The confirmation's button: a new conversation in the same worktree (REC-03).
pub async fn start_from_scratch(app: &App, p: KeyParams) -> Result<(), RequestError> {
    app.service
        .send(Request::TicketStartFromScratch(CloseParams {
            key: p.key,
            confirm: true,
        }))
        .await?;
    Ok(())
}

async fn scan(app: &App) -> Result<(), RequestError> {
    let items = match app.service.send(Request::CleanupScan).await? {
        Reply::CleanupScan(items) => items,
        other => return Err(unexpected("cleanup.scan", &other)),
    };
    app.cleanup.lock().expect("cleanup").set_items(items);
    Ok(())
}

/// The warm builds (`warm.list`) and the names of their projects, for the section.
async fn list_warm(app: &App) -> Result<(), RequestError> {
    let list = warm_list(app).await?;
    let names: BTreeMap<String, String> = match app.service.send(Request::ProjectList).await? {
        Reply::Projects(list) => list.into_iter().map(|p| (p.key, p.name)).collect(),
        other => return Err(unexpected("project.list", &other)),
    };
    app.cleanup
        .lock()
        .expect("cleanup")
        .set_warm(list.builds, names, otaq_core::store::now_ms());
    Ok(())
}

/// `warm.list`, as the service answers it. T-0301's switch reads the same.
pub async fn warm_list(app: &App) -> Result<WarmList, RequestError> {
    match app.service.send(Request::WarmList).await? {
        Reply::WarmList(list) => Ok(*list),
        other => Err(unexpected("warm.list", &other)),
    }
}

/// The Cleanup overlay; a scan first when asked (the overlay opens with one; nothing is ticked, d0018).
pub async fn cleanup(app: &App, p: CleanupParams) -> Result<CleanupPage, RequestError> {
    if p.scan {
        scan(app).await?;
    }
    if p.scan || p.warm == Some(true) {
        list_warm(app).await?;
    }
    Ok(app.cleanup.lock().expect("cleanup").page())
}

/// A warm build's confirmed Clear… (WRM-06, WRM-07). The service answers at once: cleared (the toast now), or
/// waiting for the copies running from it (the row shows it; the toast comes when it is gone). Never turns the
/// switch off.
pub async fn warm_clear(app: &App, p: WarmParams) -> Result<CleanupPage, RequestError> {
    let (params, name) = {
        let mut model = app.cleanup.lock().expect("cleanup");
        let Some(build) = model.warm_build(&p.repo) else {
            return Ok(model.page());
        };
        let params = match &build.project {
            Some(project) => WarmClearParams {
                project: Some(project.clone()),
                repo: None,
            },
            None => WarmClearParams {
                project: None,
                repo: Some(build.repo.clone()),
            },
        };
        let name = model.warm_name(build);
        model.clearing(&p.repo);
        (params, name)
    };
    if let Err(e) = clear_warm(app, params, &p.repo, &name).await {
        let mut toast = warm_build::not_cleared_toast(&name);
        toast.text = Some(e.message);
        app.notices.toast(toast);
    }
    list_warm(app).await?;
    Ok(app.cleanup.lock().expect("cleanup").page())
}

/// Sends `warm.clear` and gives the toast: at once when it cleared, or once the warm build is gone when it waits
/// for a copy (followed on `warm_build_changed`, so the toast comes even with Cleanup closed). T-0301's switch can
/// call it after `project.settings.set` too, with the same wording.
pub async fn clear_warm(
    app: &App,
    params: WarmClearParams,
    repo: &str,
    name: &str,
) -> Result<(), RequestError> {
    match app.service.send(Request::WarmClear(params)).await? {
        Reply::WarmClear(ClearOutcome::Cleared) => {
            app.notices.toast(warm_build::cleared_toast(name));
        }
        Reply::WarmClear(ClearOutcome::Waiting { .. }) => {
            follow_clear(app, repo, name, warm_build::cleared_toast(name))
        }
        Reply::WarmClear(ClearOutcome::Nothing) => {}
        other => return Err(unexpected("warm.clear", &other)),
    }
    Ok(())
}

/// Waits for a waiting Clear to end, on the service's `warm_build_changed` for `repo`: gone is the `cleared` toast,
/// still there and no longer clearing is the error toast. Settings' switch (T-0301) follows its Clear the same way.
pub fn follow_clear(app: &App, repo: &str, name: &str, cleared: NewToast) {
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<()>();
    // A first look, in case it ended before the subscription.
    let _ = tx.send(());
    let wanted = repo.to_owned();
    app.events.subscribe(Box::new(move |event| {
        if event["kind"] == "service"
            && event["name"] == "warm_build_changed"
            && event["event"]["repo"] == wanted.as_str()
        {
            let _ = tx.send(());
        }
        !tx.is_closed()
    }));
    let clients = app.service.clients();
    let notices = app.notices.clone();
    let (repo, name) = (repo.to_owned(), name.to_owned());
    tokio::spawn(async move {
        while rx.recv().await.is_some() {
            let Some(client) = clients.borrow().clone() else {
                continue;
            };
            let Ok(Reply::WarmList(list)) = client.request(Request::WarmList).await else {
                continue;
            };
            match list.builds.iter().find(|b| b.repo == repo) {
                None => notices.toast(cleared),
                Some(b) if !b.status.clearing => notices.toast(warm_build::not_cleared_toast(&name)),
                Some(_) => continue,
            };
            return;
        }
    });
}

fn cleanup_toggle(app: &App, p: ToggleParams) -> Result<CleanupPage, RequestError> {
    let target = p
        .target
        .target()
        .ok_or_else(|| RequestError::plain(format!("no such group: {}", p.target.group)))?;
    let mut model = app.cleanup.lock().expect("cleanup");
    model.toggle(&target);
    Ok(model.page())
}

fn cleanup_toggle_group(app: &App, p: GroupParams) -> Result<CleanupPage, RequestError> {
    let target = TargetPage {
        group: p.group.clone(),
        id: String::new(),
    }
    .target()
    .ok_or_else(|| RequestError::plain(format!("no such group: {}", p.group)))?;
    let mut model = app.cleanup.lock().expect("cleanup");
    model.toggle_group(target.group);
    Ok(model.page())
}

/// The confirmation's Delete: exactly what is ticked, then the success toast and a new scan.
pub async fn cleanup_delete(app: &App) -> Result<CleanupPage, RequestError> {
    let (items, bytes, before) = {
        let model = app.cleanup.lock().expect("cleanup");
        (model.selected(), model.selected_bytes(), model.len())
    };
    if items.is_empty() {
        return Ok(app.cleanup.lock().expect("cleanup").page());
    }
    let outcome = match app
        .service
        .send(Request::CleanupDelete(CleanupDeleteParams {
            items,
            confirm: true,
        }))
        .await?
    {
        Reply::CleanupOutcome(outcome) => outcome,
        other => return Err(unexpected("cleanup.delete", &other)),
    };
    let left = before.saturating_sub(outcome.deleted.len());
    let (title, text) = cleanup::success(&outcome, bytes, left);
    app.notices.toast(NewToast::new(ToastLevel::Success, title, text));
    scan(app).await?;
    Ok(app.cleanup.lock().expect("cleanup").page())
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::api::request as table;
    use crate::service::testing::{Harness, connected};

    #[tokio::test(flavor = "multi_thread")]
    async fn cleanup_lists_leftovers_and_deletes_only_what_is_ticked() {
        let h = Harness::new(None);
        h.wait_for("connected", connected).await;
        let data = h.app.service.data().unwrap();
        let leftover = data.worktrees_dir().join("shop-web").join("SHOP-99");
        std::fs::create_dir_all(&leftover).unwrap();
        std::fs::write(leftover.join("notes.txt"), "left behind").unwrap();

        let page = table(&h.app, "recovery.cleanup", json!({ "scan": true }))
            .await
            .unwrap();
        assert_eq!(page["count"], "1 item");
        assert_eq!(page["groups"][0]["group"], "worktrees");
        let row = &page["groups"][0]["rows"][0];
        assert_eq!(row["checked"], false, "nothing is ticked at first");
        assert_eq!(page["confirm"], Value::Null);

        // Delete with nothing ticked deletes nothing.
        table(&h.app, "recovery.cleanup_delete", Value::Null)
            .await
            .unwrap();
        assert!(leftover.exists());

        let page = table(
            &h.app,
            "recovery.cleanup_toggle",
            json!({ "target": row["target"] }),
        )
        .await
        .unwrap();
        assert_eq!(page["groups"][0]["rows"][0]["checked"], true);
        assert_eq!(page["confirm"]["title"], "Delete 1 item?");
        let page = table(
            &h.app,
            "recovery.cleanup_toggle_group",
            json!({ "group": "worktrees" }),
        )
        .await
        .unwrap();
        assert_eq!(page["groups"][0]["all_checked"], false);
        table(
            &h.app,
            "recovery.cleanup_toggle_group",
            json!({ "group": "worktrees" }),
        )
        .await
        .unwrap();

        let page = table(&h.app, "recovery.cleanup_delete", Value::Null)
            .await
            .unwrap();
        assert!(!leftover.exists());
        assert_eq!(page["empty"]["title"], "Nothing to clean up");
        let toasts = h.app.notices.view().toasts;
        assert_eq!(toasts.last().unwrap().title, "Cleaned up 1 item");

        assert!(
            table(
                &h.app,
                "recovery.cleanup_toggle_group",
                json!({ "group": "nonsense" })
            )
            .await
            .is_err()
        );
        h.service.stop_all().await;
    }

    /// Waits until a toast with `title` shows.
    async fn toast_titled(h: &Harness, title: &str) {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while !h.app.notices.view().toasts.iter().any(|t| t.title == title) {
            assert!(std::time::Instant::now() < deadline, "no toast {title}");
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn warm_builds_are_a_section_of_their_own_with_a_clear() {
        let h = Harness::new(None);
        h.wait_for("connected", connected).await;
        let data = h.app.service.data().unwrap();
        let leftover = data.worktrees_dir().join("shop-web").join("SHOP-99");
        std::fs::create_dir_all(&leftover).unwrap();
        // Two warm builds no project owns (Remove project leaves them), the bigger one first.
        for (repo, bytes) in [("old-app", 3000), ("legacy", 9000)] {
            let folder = data.warm_entry_path(repo, 0).unwrap();
            std::fs::create_dir_all(folder.join("out")).unwrap();
            std::fs::write(folder.join("out").join("built.bin"), vec![0u8; bytes]).unwrap();
        }

        let page = table(&h.app, "recovery.cleanup", json!({ "scan": true }))
            .await
            .unwrap();
        assert_eq!(page["count"], "1 item", "warm builds aren't leftovers");
        let warm = &page["warm"];
        assert_eq!(warm["count"], "2");
        assert_eq!(warm["rows"][0]["name"], "legacy");
        assert_eq!(warm["rows"][1]["name"], "old-app");
        assert_eq!(warm["rows"][0]["built_at"], Value::Null);
        assert_eq!(
            warm["rows"][0]["confirm"]["confirm"],
            format!("Clear up to {}", cleanup::size(9000))
        );
        // The rail's broom counts the leftover only.
        let broom = table(&h.app, "shell.cleanup", Value::Null).await.unwrap();
        assert_eq!(broom["count"], 1);
        // Delete selected… never takes a warm build.
        table(&h.app, "recovery.cleanup_toggle_group", json!({ "group": "worktrees" }))
            .await
            .unwrap();
        let page = table(&h.app, "recovery.cleanup_delete", Value::Null)
            .await
            .unwrap();
        assert!(!leftover.exists());
        assert_eq!(page["empty"]["title"], "Nothing to clean up");
        assert_eq!(page["warm"]["count"], "2");
        assert!(data.warm_path("legacy").unwrap().exists());

        // Clear… on legacy: cleared at once (nothing copies from it), the toast, focus on the next row.
        let page = table(&h.app, "recovery.warm_clear", json!({ "repo": "legacy" }))
            .await
            .unwrap();
        assert_eq!(page["warm"]["count"], "1");
        assert_eq!(page["warm"]["focus"]["repo"], "old-app");
        assert_eq!(page["warm"]["total"], format!("{} on disk", cleanup::size(3000)));
        toast_titled(&h, "Cleared legacy’s warm build").await;
        // Re-listed on warm_build_changed: the same section.
        let page = table(&h.app, "recovery.cleanup", json!({ "warm": true }))
            .await
            .unwrap();
        assert_eq!(page["warm"]["rows"][0]["repo"], "old-app");
        assert_eq!(page["warm"]["focus"], Value::Null);

        // A Clear that waited: the toast once it is gone, even with Cleanup closed. (Gone before the follow
        // subscribed: its first look finds it.)
        std::fs::remove_dir_all(data.warm_path("old-app").unwrap()).unwrap();
        follow_clear(&h.app, "old-app", "old-app", warm_build::cleared_toast("old-app"));
        toast_titled(&h, "Cleared old-app’s warm build").await;
        let page = table(&h.app, "recovery.cleanup", json!({ "warm": true }))
            .await
            .unwrap();
        assert_eq!(page["warm"], Value::Null, "none left: the section is hidden");

        // One that ended without clearing (still there, no longer clearing) says so.
        std::fs::create_dir_all(data.warm_entry_path("kept", 0).unwrap()).unwrap();
        follow_clear(&h.app, "kept", "kept", warm_build::cleared_toast("kept"));
        toast_titled(&h, "Couldn’t clear kept’s warm build").await;
        h.service.stop_all().await;
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_start_that_interrupted_nothing_has_no_sheet() {
        let h = Harness::new(None);
        h.wait_for("connected", connected).await;
        assert_eq!(
            table(&h.app, "recovery.summary", json!({})).await.unwrap(),
            Value::Null
        );
        let missing = table(&h.app, "recovery.scratch", json!({ "key": "SHOP-1" })).await;
        assert!(missing.is_err());
        // Resuming a ticket that doesn't exist says so in a toast; nothing else happens.
        assert_eq!(
            table(&h.app, "recovery.resume", json!({ "keys": ["SHOP-1"] }))
                .await
                .unwrap(),
            0
        );
        assert!(
            h.app
                .notices
                .view()
                .toasts
                .iter()
                .any(|t| t.title == "Couldn’t resume SHOP-1")
        );
        h.service.stop_all().await;
    }
}
