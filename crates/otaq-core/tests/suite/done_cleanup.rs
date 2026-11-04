//! Done cleanup (T-0099, REC-05): on Done the agent session ends, the ports return, the worktree is removed when
//! clean (the branch stays) and kept and flagged when it has uncommitted changes; the finished hook runs. Then the
//! ticket's resources are removed and, after a merge, the template is refreshed (M19-d).

use crate::common::resources as resources_common;

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use otaq_core::board::settings::ProjectSettings;
use otaq_core::board::tickets::Ticket;
use otaq_core::domain::{Actor, Status};
use otaq_core::git::testing::{commit, git};
use otaq_core::naming;
use otaq_core::ports::PortPool;
use otaq_core::recovery::cleanup::CleanupGroup;
use otaq_core::review::LocalMergeOutcome;
use otaq_core::service::testing::TestService;
use otaq_core::session::SessionKind;
use otaq_core::ticket_env::resources;
use resources_common::{append, by_os, created, lines, resource_row, then, var};

fn me() -> Actor {
    Actor::User("me".into())
}

#[tokio::test]
async fn a_local_merge_cleans_up_a_clean_worktree_and_keeps_the_branch() {
    let w = TestService::start().await;
    let project = w.add_project("shop-web").await;
    let t = w.create("SHOP", "Checkout API").await;
    let wt = w.start_worktree(&t).await;
    commit(&wt, "api.ts", "api\n");
    PortPool::with_range(w.board.store().clone(), 21_000..=21_999)
        .allocate(&t.id, &[otaq_core::board::settings::DEFAULT_APP.to_owned()])
        .await
        .unwrap();
    w.force_status(&t, Status::WaitingForYou).await;
    let agent = w.agent_shell(&t).await;
    let finished = Arc::new(AtomicUsize::new(0));
    let seen = finished.clone();
    w.review.on_ticket_finished(Arc::new(move |_| {
        seen.fetch_add(1, Ordering::SeqCst);
        Box::pin(async {})
    }));

    let outcome = w
        .review
        .local_merge("SHOP-1", "m", false, &me())
        .await
        .unwrap();
    assert!(
        matches!(outcome, LocalMergeOutcome::Merged { .. }),
        "{outcome:?}"
    );

    assert!(
        !w.sessions.info(&agent.session).unwrap().is_running(),
        "session ended"
    );
    assert!(w.sessions.running_of(&t.id, SessionKind::Agent).is_none());
    assert!(!wt.exists(), "worktree removed");
    let branch = otaq_core::naming::branch("SHOP-1");
    assert_eq!(
        git(
            std::path::Path::new(&project.repo_path),
            &["branch", "--list", &branch]
        )
        .trim_start_matches(['*', '+', ' ']),
        branch,
        "branch kept"
    );
    let row = w.board.store().ticket(&t.id).await.unwrap().unwrap();
    assert_eq!((row.agent_port, row.run_port), (None, None), "ports freed");
    assert_eq!(row.worktree_path, None);
    assert!(!row.worktree_kept);
    assert_eq!(finished.load(Ordering::SeqCst), 1);
    // The Done ticket's diff still reads from the branch.
    assert_eq!(w.board.diff_summary("SHOP-1").await.unwrap().files.len(), 1);
    w.stop().await;
}

#[tokio::test]
async fn a_dirty_worktree_is_kept_and_flagged() {
    let w = TestService::start().await;
    w.add_project("shop-web").await;
    let t = w.create("SHOP", "Price table").await;
    let wt = w.start_worktree(&t).await;
    std::fs::write(wt.join("notes.md"), "not committed\n").unwrap();
    let t = w.board.ticket("SHOP-1").await.unwrap();

    let cleanup = w.review.finish(&t).await.unwrap();
    assert!(!cleanup.worktree_removed);
    assert_eq!(cleanup.kept_dirty, ["notes.md"]);
    assert!(wt.join("notes.md").is_file(), "never removed");
    let row = w.board.store().ticket(&t.id).await.unwrap().unwrap();
    assert!(row.worktree_kept);
    assert!(row.worktree_path.is_some());
    w.stop().await;
}

/// A ticket in Ready for Review with a commit, its resource recorded as created.
async fn merged_with_resources(w: &TestService, name: &str) -> (Ticket, PathBuf, PathBuf) {
    let t = w.create("SHOP", "Checkout API").await;
    let wt = w.start_worktree(&t).await;
    commit(&wt, "api.ts", "api\n");
    let dir = created(w, &t, name).await;
    w.force_status(&t, Status::WaitingForYou).await;
    (t, wt, dir)
}

/// Done runs Remove for the recorded name in the main checkout once the build is kept and the worktree gone, then
/// the project's Template refresh; Otaq's resource folder goes to the trash.
#[tokio::test]
async fn done_runs_remove_then_refreshes_the_template() {
    let w = TestService::start().await;
    let project = w.add_project("shop-web").await;
    let markers = w.home.path().join("markers.txt");
    let (t, wt, dir) = merged_with_resources(&w, "shop_db_1").await;
    let wt_there = by_os(
        &format!(
            "test -d '{}' && echo wt-there >> '{}'",
            wt.display(),
            markers.display()
        ),
        &format!(
            "(if exist \"{}\" echo wt-there>> \"{}\")",
            wt.display(),
            markers.display()
        ),
    );
    w.board
        .set_project_settings(
            "SHOP",
            &ProjectSettings {
                resource_create: "echo create".into(),
                resource_remove: then(&[
                    wt_there,
                    append(
                        &markers,
                        &format!(
                            "remove {} {} {}",
                            var(&naming::db_name_env_var()),
                            var(&naming::ticket_env_var()),
                            var(&naming::db_template_env_var())
                        ),
                    ),
                    append(&markers, "remove-in-checkout"),
                ]),
                resource_template_refresh: append(
                    &markers,
                    &format!("refresh {}", var(&naming::db_template_env_var())),
                ),
                ..Default::default()
            },
        )
        .await
        .unwrap();

    let outcome = w
        .review
        .local_merge("SHOP-1", "m", false, &me())
        .await
        .unwrap();
    assert!(
        matches!(outcome, LocalMergeOutcome::Merged { .. }),
        "{outcome:?}"
    );
    let template = resources::db_template("SHOP");
    assert_eq!(
        lines(&markers),
        [
            format!("remove shop_db_1 SHOP-1 {template}"),
            "remove-in-checkout".to_owned(),
            format!("refresh {template}"),
        ],
        "the worktree was gone before Remove; Refresh came after it"
    );
    assert!(!wt.exists());
    assert_eq!(
        resource_row(&w, &t).await,
        (Some("shop_db_1".into()), "removed".into(), None)
    );
    assert!(!dir.exists(), "the resource folder went to the trash");
    let log = w.data.resource_log_path("shop-web").unwrap();
    let log = std::fs::read_to_string(log).unwrap();
    assert!(log.contains("remove-in-checkout"), "{log}");
    assert!(Path::new(&project.repo_path).is_dir());
    w.stop().await;
}

/// **The M19-d acceptance**: a failed Remove never fails Done; it is listed in Cleanup with its problem, and
/// Cleanup's Delete runs it again.
#[tokio::test]
async fn a_failed_remove_shows_in_cleanup_and_cleanup_retries_it() {
    let w = TestService::start().await;
    w.add_project("shop-web").await;
    let markers = w.home.path().join("markers.txt");
    let (t, wt, dir) = merged_with_resources(&w, "shop_db_1").await;
    let settings = ProjectSettings {
        resource_create: "echo create".into(),
        resource_remove: by_os(
            "echo database is being accessed >&2; exit 4",
            "1>&2 echo database is being accessed& exit 4",
        ),
        ..Default::default()
    };
    w.board
        .set_project_settings("SHOP", &settings)
        .await
        .unwrap();
    let outcome = w
        .review
        .local_merge("SHOP-1", "m", false, &me())
        .await
        .unwrap();
    assert!(
        matches!(outcome, LocalMergeOutcome::Merged { .. }),
        "{outcome:?}"
    );
    assert_eq!(w.status("SHOP-1").await, Status::Merged, "Done went on");
    assert!(!wt.exists());
    let (name, state, problem) = resource_row(&w, &t).await;
    assert_eq!(
        (name.as_deref(), state.as_str()),
        (Some("shop_db_1"), "failed")
    );
    let problem = problem.unwrap();
    assert!(
        problem.starts_with("remove command failed (exit 4):"),
        "{problem}"
    );
    assert!(problem.contains("database is being accessed"), "{problem}");
    assert!(dir.is_dir(), "the folder stays while the resource does");

    let items = w.cleanup.scan().await.unwrap();
    let item = items
        .iter()
        .find(|i| i.group == CleanupGroup::TicketResources)
        .expect("listed in Cleanup")
        .clone();
    assert_eq!(item.label, "shop_db_1");
    assert_eq!(
        item.ticket.as_ref().map(ToString::to_string).as_deref(),
        Some("SHOP-1")
    );
    assert_eq!(item.why, problem);

    // Still failing: the retry reports it and the item stays.
    let outcome = w.cleanup.delete(&[item.target()], true).await.unwrap();
    assert_eq!(outcome.failed.len(), 1, "{outcome:?}");
    assert!(
        w.cleanup
            .scan()
            .await
            .unwrap()
            .iter()
            .any(|i| i.group == CleanupGroup::TicketResources)
    );

    w.board
        .set_project_settings(
            "SHOP",
            &ProjectSettings {
                resource_remove: append(
                    &markers,
                    &format!("remove {}", var(&naming::db_name_env_var())),
                ),
                ..settings
            },
        )
        .await
        .unwrap();
    let outcome = w.cleanup.delete(&[item.target()], true).await.unwrap();
    assert_eq!(outcome.deleted, [item.target()], "{outcome:?}");
    assert_eq!(lines(&markers), ["remove shop_db_1"]);
    assert_eq!(
        resource_row(&w, &t).await,
        (Some("shop_db_1".into()), "removed".into(), None)
    );
    assert!(!dir.exists());
    assert!(
        !w.cleanup
            .scan()
            .await
            .unwrap()
            .iter()
            .any(|i| i.group == CleanupGroup::TicketResources)
    );
    w.stop().await;
}

/// **The M19-d acceptance**: Remove gets the name recorded at Create, never one computed from today's project or
/// settings; a ticket with no recorded name never runs Remove.
#[tokio::test]
async fn remove_uses_only_the_recorded_name() {
    let w = TestService::start().await;
    w.add_project("shop-web").await;
    let markers = w.home.path().join("markers.txt");
    let (t, _wt, _dir) = merged_with_resources(&w, "named_at_create").await;
    let untouched = w.create("SHOP", "Never started").await;
    // The settings changed after Create; the computed name differs from the recorded one too.
    assert_ne!(resources::db_name("SHOP", &t.key), "named_at_create");
    w.board
        .set_project_settings(
            "SHOP",
            &ProjectSettings {
                resource_create: "echo another create".into(),
                resource_remove: append(
                    &markers,
                    &format!(
                        "remove {} {}",
                        var(&naming::db_name_env_var()),
                        var(&naming::ticket_env_var())
                    ),
                ),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    w.review
        .local_merge("SHOP-1", "m", false, &me())
        .await
        .unwrap();
    w.review.close("SHOP-2", true, &me()).await.unwrap();
    assert_eq!(lines(&markers), ["remove named_at_create SHOP-1"]);
    assert_eq!(
        resource_row(&w, &untouched).await,
        (None, "none".into(), None)
    );
    w.stop().await;
}

/// A worktree Done can't handle (git can't read it) is no reason to leave the ticket's resources: Remove still
/// runs, and the cleanup reports the worktree's error after it.
#[tokio::test]
async fn a_worktree_error_still_removes_the_resources() {
    let w = TestService::start().await;
    w.add_project("shop-web").await;
    let markers = w.home.path().join("markers.txt");
    let (t, wt, _dir) = merged_with_resources(&w, "shop_db_1").await;
    w.board
        .set_project_settings(
            "SHOP",
            &ProjectSettings {
                resource_create: "echo create".into(),
                resource_remove: append(
                    &markers,
                    &format!("remove {}", var(&naming::db_name_env_var())),
                ),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    // The worktree's link to its repository is broken: git refuses to read it.
    std::fs::remove_file(wt.join(".git")).unwrap();
    std::fs::write(wt.join(".git"), "gitdir: nowhere\n").unwrap();
    w.force_status(&t, Status::Merged).await;
    let merged = w.board.ticket("SHOP-1").await.unwrap();

    assert!(
        w.review.finish(&merged).await.is_err(),
        "the worktree's error"
    );
    assert_eq!(lines(&markers), ["remove shop_db_1"]);
    assert_eq!(resource_row(&w, &t).await.1, "removed");
    w.stop().await;
}
