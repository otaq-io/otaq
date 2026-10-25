//! Close ticket (T-0100, CLS-01): from any state, the agent stopped after a
//! confirmation, the branch kept, the worktree removed unless it has uncommitted changes, the ticket's resources
//! removed (M19-d).

use crate::common::resources as resources_common;

use otaq_core::board::Refusal;
use otaq_core::board::settings::ProjectSettings;
use otaq_core::board::tickets::EventKind;
use otaq_core::domain::{Actor, Status};
use otaq_core::git::testing::{commit, git};
use otaq_core::naming;
use otaq_core::service::testing::TestService;
use resources_common::{append, created, lines, resource_row, var};

fn me() -> Actor {
    Actor::User("me".into())
}

fn branch_exists(w: &TestService, key: &str) -> bool {
    let repo = w.repos.root().join("shop-web");
    !git(
        &repo,
        &["branch", "--list", &otaq_core::naming::branch(key)],
    )
    .is_empty()
}

#[tokio::test]
async fn close_from_backlog_dev_and_review() {
    let w = TestService::start().await;
    w.add_project("shop-web").await;
    // Backlog: nothing to stop or remove.
    w.create("SHOP", "Won't do").await;
    let closed = w.review.close("SHOP-1", false, &me()).await.unwrap();
    assert_eq!(closed.status, Status::ClosedNotMerged);
    let history = w.board.history("SHOP-1").await.unwrap();
    assert!(history.iter().any(|e| e.kind == EventKind::Closed));

    // Dev with a running agent: asks first, then stops it.
    let dev = w.create("SHOP", "Working").await;
    let wt = w.start_worktree(&dev).await;
    commit(&wt, "a.ts", "a\n");
    w.force_status(&dev, Status::Working).await;
    let agent = w.agent_shell(&dev).await;
    let err = w.review.close("SHOP-2", false, &me()).await.unwrap_err();
    assert_eq!(err.refusal(), Some(Refusal::NeedsConfirmation));
    assert_eq!(
        err.to_string(),
        "SHOP-2's agent is working. It will be stopped first."
    );
    assert_eq!(w.status("SHOP-2").await, Status::Working);
    let closed = w.review.close("SHOP-2", true, &me()).await.unwrap();
    assert_eq!(closed.status, Status::ClosedNotMerged);
    assert!(!w.sessions.info(&agent.session).unwrap().is_running());
    assert!(!wt.exists(), "clean worktree removed");
    assert!(branch_exists(&w, "SHOP-2"), "branch kept");
    let row = w.board.store().ticket(&dev.id).await.unwrap().unwrap();
    assert_eq!(row.outcome.as_deref(), Some("closed"));

    // Ready for Review with uncommitted files: the worktree is kept and flagged.
    let review = w.create("SHOP", "In review").await;
    let wt = w.start_worktree(&review).await;
    std::fs::write(wt.join("scratch.md"), "x\n").unwrap();
    w.force_status(&review, Status::WaitingForYou).await;
    w.review.close("SHOP-3", false, &me()).await.unwrap();
    assert!(wt.join("scratch.md").is_file());
    let row = w.board.store().ticket(&review.id).await.unwrap().unwrap();
    assert!(row.worktree_kept);

    // Merged tickets are locked.
    let merged = w.create("SHOP", "Merged").await;
    w.force_status(&merged, Status::Merged).await;
    let err = w.review.close("SHOP-4", true, &me()).await.unwrap_err();
    assert_eq!(err.refusal(), Some(Refusal::MoveRefused));
    w.stop().await;
}

/// T-0445 (CMT-09, d0109): with no ticket resources, Close still trashes Otaq's resource folder of the ticket
/// (`OTAQ_RESOURCE_DIR`, where its agents saved long output); runs nothing and records nothing.
#[tokio::test]
async fn close_trashes_the_resource_folder_without_resources() {
    let w = TestService::start().await;
    w.add_project("shop-web").await;
    let t = w.create("SHOP", "Won't do").await;
    let wt = w.start_worktree(&t).await;
    commit(&wt, "a.ts", "a\n");
    let dir = w.data.resource_path("shop-web", &t.key).unwrap();
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("test-run.txt"), "ok\n").unwrap();
    w.force_status(&t, Status::WaitingForYou).await;

    let closed = w.review.close("SHOP-1", true, &me()).await.unwrap();
    assert_eq!(closed.status, Status::ClosedNotMerged);
    assert!(!dir.exists(), "the resource folder went to the trash");
    assert_eq!(resource_row(&w, &t).await, (None, "none".into(), None));
    w.stop().await;
}

/// Close is a Release too (M19-d): the ticket's recorded resource is removed, but the template is refreshed only
/// after a merge, never at Close.
#[tokio::test]
async fn close_removes_but_does_not_refresh() {
    let w = TestService::start().await;
    w.add_project("shop-web").await;
    let markers = w.home.path().join("markers.txt");
    let t = w.create("SHOP", "Won't do").await;
    let wt = w.start_worktree(&t).await;
    commit(&wt, "a.ts", "a\n");
    let dir = created(&w, &t, "shop_db_1").await;
    w.force_status(&t, Status::WaitingForYou).await;
    w.board
        .set_project_settings(
            "SHOP",
            &ProjectSettings {
                resource_create: "echo create".into(),
                resource_remove: append(
                    &markers,
                    &format!("remove {}", var(&naming::db_name_env_var())),
                ),
                resource_template_refresh: append(&markers, "refresh"),
                ..Default::default()
            },
        )
        .await
        .unwrap();

    let closed = w.review.close("SHOP-1", true, &me()).await.unwrap();
    assert_eq!(closed.status, Status::ClosedNotMerged);
    assert_eq!(lines(&markers), ["remove shop_db_1"], "no refresh");
    assert_eq!(
        resource_row(&w, &t).await,
        (Some("shop_db_1".into()), "removed".into(), None)
    );
    assert!(!dir.exists());
    let history = w.board.history("SHOP-1").await.unwrap();
    assert!(
        !history
            .iter()
            .any(|e| e.kind == EventKind::TemplateRefreshFailed)
    );
    w.stop().await;
}
