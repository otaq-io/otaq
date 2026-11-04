//! The Shell tab (T-0137, RUN-05): a plain shell in the ticket's worktree with `$PORT` = the Run port and the
//! ticket's env, owned by the service; it starts on first use, is shown again (not restarted) when reopened,
//! closes at Done or Close, and after a restart stays "stopped by restart" until the user's Reopen. `sh` stands in
//! for the user's login shell. A new Shell of a ticket whose build was freed at Review brings it back; one there
//! already never does (T-0309, M19-c).

use crate::common::freed;

use std::time::Duration;

use otaq_core::board::Refusal;
use otaq_core::domain::{Actor, BuildState, Status};
use otaq_core::ports::PortPool;
use otaq_core::service::ServiceConfig;
use otaq_core::service::testing::TestService;
use otaq_core::session::{END_STOPPED_BY_RESTART, SessionKind};

fn plain_sh(mut c: ServiceConfig) -> ServiceConfig {
    c.terminals.shell = "sh".into();
    c.terminals.shell_args = Vec::new();
    c
}

fn me() -> Actor {
    Actor::User("me".into())
}

#[tokio::test(flavor = "multi_thread")]
async fn the_shell_runs_in_the_worktree_with_the_run_port_and_closes_at_close() {
    let w = TestService::start_with(plain_sh).await;
    w.add_project("shop-web").await;
    let backlog = w.create("SHOP", "Not started").await;
    let err = w
        .shells
        .open_shell(&backlog.key.to_string(), false)
        .await
        .unwrap_err();
    assert_eq!(err.refusal(), Some(Refusal::NoWorktree));

    let t = w.create("SHOP", "Invoice PDF export").await;
    let worktree = w.start_worktree(&t).await;
    let ports = PortPool::with_range(w.board.store().clone(), 24_000..=24_099)
        .allocate(&t.id, &[otaq_core::board::settings::DEFAULT_APP.to_owned()])
        .await
        .unwrap();
    w.force_status(&t, Status::Working).await;
    let key = t.key.to_string();

    let shell = w.shells.open_shell(&key, false).await.unwrap();
    assert_eq!(shell.kind, SessionKind::Shell);
    assert!(shell.is_running());
    // A Windows console passes Return on as `\r`; Git for Windows' sh prints its own form of the folder
    // (`/c/Users/…`), so only the worktree's own part is compared there.
    let enter = if cfg!(windows) { "\r" } else { "\n" };
    w.sessions
        .write(
            &shell.id,
            format!("echo \"port=$PORT ticket=$OTAQ_TICKET\"; pwd{enter}").as_bytes(),
        )
        .unwrap();
    w.wait_for_output(&shell.id, &format!("port={} ticket={key}", ports.apps[0].1))
        .await;
    if cfg!(windows) {
        w.wait_for_output(&shell.id, &format!("/wt/shop-web/{key}"))
            .await;
    } else {
        let canonical = otaq_core::paths::canonical(&worktree).unwrap();
        w.wait_for_output(&shell.id, canonical.to_str().unwrap())
            .await;
    }

    // Opening the tab again shows the same shell.
    assert_eq!(w.shells.open_shell(&key, false).await.unwrap().id, shell.id);

    // Close ends it.
    w.review.close(&key, true, &me()).await.unwrap();
    tokio::time::timeout(Duration::from_secs(5), async {
        while w.sessions.info(&shell.id).unwrap().is_running() {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("the shell closed");
    w.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn after_a_restart_it_stays_stopped_until_reopen() {
    let w = TestService::start_with(plain_sh).await;
    w.add_project("shop-web").await;
    let t = w.create("SHOP", "Price table").await;
    w.start_worktree(&t).await;
    PortPool::with_range(w.board.store().clone(), 24_100..=24_199)
        .allocate(&t.id, &[otaq_core::board::settings::DEFAULT_APP.to_owned()])
        .await
        .unwrap();
    w.force_status(&t, Status::Working).await;
    let key = t.key.to_string();
    let first = w.shells.open_shell(&key, false).await.unwrap();

    let w = w.restart_with(plain_sh).await;
    let shown = w.shells.open_shell(&key, false).await.unwrap();
    assert_eq!(shown.id, first.id, "the stopped one is shown");
    assert!(!shown.is_running());
    assert_eq!(shown.end_reason.as_deref(), Some(END_STOPPED_BY_RESTART));
    assert!(
        w.sessions.running_of(&t.id, SessionKind::Shell).is_none(),
        "nothing restarted it"
    );

    let reopened = w.shells.open_shell(&key, true).await.unwrap();
    assert_ne!(reopened.id, first.id);
    assert!(reopened.is_running());
    w.stop().await;
}

/// A new Shell of a ticket in Review brings its freed build back before it opens; when it ends the build is freed
/// again.
#[tokio::test(flavor = "multi_thread")]
async fn a_new_shell_in_review_brings_the_build_back() {
    let w = freed::world_with(plain_sh).await;
    let project = freed::shop_web(&w, |_| {}).await;
    let (t, wt) = freed::freed(&w, "Invoice PDF", "invoice.ts").await;
    let key = t.key.to_string();

    let shell = w.shells.open_shell(&key, false).await.unwrap();

    assert!(shell.is_running());
    assert_eq!(freed::app_bin(&wt).as_deref(), Some("Invoice PDF"));
    assert_eq!(freed::state(&w, &key).await, BuildState::Present);
    assert_eq!(freed::entries(&w, &project), Vec::<u64>::new());

    w.sessions.stop(&shell.id).await.unwrap();
    freed::until_state(&w, &key, BuildState::Freed).await;
    assert_eq!(freed::app_bin(&wt), None);
    w.stop().await;
}

/// Opening the Shell again shows the ended one: nothing is brought back for it.
#[tokio::test(flavor = "multi_thread")]
async fn reopening_an_ended_shell_does_not_fetch() {
    let w = freed::world_with(plain_sh).await;
    let project = freed::shop_web(&w, |_| {}).await;
    let (t, wt) = freed::in_review(&w, "Invoice PDF", "invoice.ts").await;
    let key = t.key.to_string();
    // The Shell opens while the build is still in the worktree, and ends; then the build is freed.
    let first = w.shells.open_shell(&key, false).await.unwrap();
    w.sessions.stop(&first.id).await.unwrap();
    freed::until_state(&w, &key, BuildState::Freed).await;
    assert_eq!(freed::app_bin(&wt), None);
    assert_eq!(freed::entries(&w, &project).len(), 1);

    let again = w.shells.open_shell(&key, false).await.unwrap();

    assert_eq!(again.id, first.id, "the ended Shell");
    assert!(!again.is_running());
    assert_eq!(freed::state(&w, &key).await, BuildState::Freed);
    assert_eq!(freed::app_bin(&wt), None, "nothing came back");
    assert_eq!(freed::entries(&w, &project).len(), 1);
    w.stop().await;
}
