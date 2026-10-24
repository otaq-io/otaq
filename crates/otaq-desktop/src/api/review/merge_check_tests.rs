//! The Merge check from the window (T-0492; d0120, Q475): shop-web (SHOP) with a Merge check that runs until the test
//! releases it, SHOP-1 in Review and `main` moved since it was built. While **Local merge** runs the check, the app
//! still gets answers (the merge has a connection of its own), the dialog names the command, the sheet offers only
//! Stop, and **Stop** from the card ends it: nothing is merged. Without a Merge check the dialog says so. A click that
//! starts an automatic Delivery (Pass into one) leaves the app free the same way while its check runs.
//!
//! The app talks to a core `TestService` (temporary `OTAQ_HOME` and repositories); the check is a plain `sh` loop. No
//! agent tool runs.

use std::path::{Path, PathBuf};
use std::time::Duration;

use otaq_core::domain::{MergeCheckState, Status};
use otaq_core::flow::default::{DELIVER, REVIEW, VALIDATE};
use otaq_core::flow::model::{Delivers, Delivery, DeliveryMode, FlowFile};
use otaq_core::git::testing::{commit, git};
use otaq_core::service::testing::TestService;
use serde_json::{Value, json};

use crate::App;
use crate::api::request;
use crate::service::testing::{app_on, connected};

async fn app(w: &TestService) -> App {
    let app = app_on(w.data.clone());
    tokio::time::timeout(Duration::from_secs(10), async {
        while !connected(&app.service.connection()) {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("the app connects");
    app
}

async fn set_check(w: &TestService, command: Option<&str>) {
    let mut settings = w.board.project_settings("SHOP").await.unwrap();
    settings.merge_check = command.map(str::to_owned);
    w.board
        .save_project_settings("SHOP", &settings)
        .await
        .unwrap();
}

async fn until(what: &str, done: impl Fn() -> bool) {
    tokio::time::timeout(Duration::from_secs(20), async {
        while !done() {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("{what} never happened"));
}

/// A check that runs until `release` exists, after touching `started`.
fn held_check(started: &Path, release: &Path) -> String {
    format!(
        "touch '{}'; while [ ! -e '{}' ]; do sleep 0.05; done",
        started.display(),
        release.display()
    )
}

fn main_of(repo: &Path) -> String {
    git(repo, &["rev-parse", "main"])
}

/// An answer within 5 s: the shared connection isn't waiting behind the merge.
async fn quick(app: &App, method: &str, params: Value) -> Value {
    tokio::time::timeout(Duration::from_secs(5), request(app, method, params))
        .await
        .unwrap_or_else(|_| panic!("{method} waited for the merge"))
        .unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn a_running_merge_check_leaves_the_app_free_and_stop_merges_nothing() {
    let w = TestService::start().await;
    let project = w.add_project("shop-web").await;
    let repo = PathBuf::from(&project.repo_path);
    let marks = tempfile::tempdir().unwrap();
    let started = marks.path().join("started");
    let release = marks.path().join("release");
    let check = held_check(&started, &release);
    set_check(&w, Some(&check)).await;
    let t = w.create("SHOP", "Card").await;
    let wt = w.start_worktree(&t).await;
    commit(&wt, "card.ts", "card\n");
    w.force_status(&t, Status::WaitingForYou).await;
    commit(&repo, "other.ts", "main moved\n");
    let main = main_of(&repo);
    let app = app(&w).await;

    let approve = quick(&app, "review.approve", json!({ "key": "SHOP-1" })).await;
    assert_eq!(
        approve["confirm"]["merge_check"],
        json!({
            "text": format!("Runs `{check}` first when main has moved."),
            "link": null,
            "project": "SHOP",
        })
    );
    assert_eq!(approve["merging"]["close"], "Close");

    let merge = request(
        &app,
        "review.local_merge",
        json!({ "key": "SHOP-1", "message": "SHOP-1: Card" }),
    );
    let stop = async {
        until("the check started", || started.exists()).await;
        let shown = w.board.ticket("SHOP-1").await.unwrap();
        assert_eq!(shown.flags.merge_check, Some(MergeCheckState::Running));
        // The sheet: Stop is its only button.
        let panel = quick(&app, "panel.view", json!({ "key": "SHOP-1" })).await;
        let buttons: Vec<&Value> = panel["view"]["buttons"]
            .as_array()
            .unwrap_or_else(|| panic!("{panel}"))
            .iter()
            .map(|b| &b["action"])
            .collect();
        assert_eq!(buttons, [&json!("stop_merge_check")]);
        // The card's Stop.
        let sent = quick(
            &app,
            "panel.card",
            json!({ "key": "SHOP-1", "kind": "stop_merge_check" }),
        )
        .await;
        assert_eq!(sent, json!({ "outcome": "done" }));
    };
    let (merged, ()) = tokio::time::timeout(Duration::from_secs(30), async {
        tokio::join!(merge, stop)
    })
    .await
    .expect("the merge ends after Stop");
    assert_eq!(
        merged.unwrap(),
        json!({
            "kind": "stopped",
            "title": "Merge check stopped",
            "message": "The merge check was stopped; nothing was merged into main.",
            "close": "Close",
        })
    );
    assert_eq!(main_of(&repo), main, "nothing merged");
    let after = w.board.ticket("SHOP-1").await.unwrap();
    assert_eq!(after.status, Status::WaitingForYou, "where it was");
    assert_eq!(after.flags.merge_check, None);
    std::fs::write(&release, "").unwrap();
    // Stopping a check that doesn't run is refused (a toast).
    assert!(
        request(
            &app,
            "panel.card",
            json!({ "key": "SHOP-1", "kind": "stop_merge_check" }),
        )
        .await
        .is_err()
    );

    // No Merge check: the dialog says so once, with the link to the row.
    set_check(&w, None).await;
    let approve = quick(&app, "review.approve", json!({ "key": "SHOP-1" })).await;
    assert_eq!(
        approve["confirm"]["merge_check"],
        json!({
            "text": "No merge check \u{b7} ",
            "link": "set one in project settings",
            "project": "SHOP",
        })
    );
    w.stop().await;
}

/// Dev → Review → Done, the Delivery merging locally with no click after Review's Pass (FLW-14, FLW-15).
fn review_then_automatic_merge(file: &mut FlowFile) {
    file.flow.stages.retain(|s| s.id != VALIDATE);
    for stage in file.flow.stages.iter_mut().filter(|s| s.id == DELIVER) {
        stage.agent = None;
        stage.skill = None;
        stage.delivery = Some(Delivery {
            mode: DeliveryMode::Automatic,
            delivers: Delivers::Local,
        });
    }
}

/// Pass into an automatic Delivery runs the merge inside its click: while the Merge check runs, the board still gets
/// its answers, and the merge finishes on its own once the check passes (d0120).
#[tokio::test(flavor = "multi_thread")]
async fn a_click_that_starts_an_automatic_delivery_leaves_the_app_free() {
    let w = TestService::start().await;
    let project = w.add_project("shop-web").await;
    let repo = PathBuf::from(&project.repo_path);
    w.board
        .edit_flow_for_tests("SHOP", review_then_automatic_merge)
        .await;
    let marks = tempfile::tempdir().unwrap();
    let started = marks.path().join("started");
    let release = marks.path().join("release");
    set_check(&w, Some(&held_check(&started, &release))).await;
    let t = w.create("SHOP", "Pricing").await;
    let wt = w.start_worktree(&t).await;
    commit(&wt, "pricing.ts", "pricing\n");
    w.force_stage(&t, REVIEW, Status::WaitingForYou).await;
    commit(&repo, "other.ts", "main moved\n");
    let app = app(&w).await;

    let pass = request(&app, "review.pass", json!({ "key": "SHOP-1" }));
    let meanwhile = async {
        until("the check started", || started.exists()).await;
        let shown = w.board.ticket("SHOP-1").await.unwrap();
        assert_eq!(shown.flags.merge_check, Some(MergeCheckState::Running));
        quick(&app, "panel.view", json!({ "key": "SHOP-1" })).await;
        quick(&app, "review.available", json!({ "key": "SHOP-1" })).await;
        std::fs::write(&release, "").unwrap();
    };
    let (sent, ()) = tokio::time::timeout(Duration::from_secs(30), async {
        tokio::join!(pass, meanwhile)
    })
    .await
    .expect("the merge ends once the check passes");
    assert_eq!(sent.unwrap(), json!("moved"));
    assert_eq!(w.status("SHOP-1").await, Status::Merged);
    assert_eq!(
        git(&repo, &["log", "-1", "--format=%s", "main"]),
        "SHOP-1: Pricing"
    );
    w.stop().await;
}
