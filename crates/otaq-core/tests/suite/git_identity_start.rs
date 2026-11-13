//! An agent never commits under a borrowed name: on a machine where git has no name and email, a start waits (Held,
//! with the reason in plain words) instead of launching, `git.missing_identity` tells the app so it can ask first,
//! and once the user saves them (`tools.git_identity`, only on that click) every held start goes on by itself.

use std::path::PathBuf;
use std::time::Duration;

use otaq_core::board::moves::DropTarget;
use otaq_core::client::{Client, ClientConfig};
use otaq_core::domain::{Actor, StartStep, Status};
use otaq_core::git::Identity;
use otaq_core::git::testing::git;
use otaq_core::protocol::{GitIdentityParams, ProjectParams, Reply, Request};
use otaq_core::service::testing::TestService;
use otaq_core::start::StartOutcome;
use otaq_core::start::sequence::NEEDS_IDENTITY;

fn me() -> Actor {
    Actor::User("me".into())
}

async fn missing(client: &Client) -> Option<Identity> {
    match client
        .request(Request::GitMissingIdentity(ProjectParams {
            project: "SHOP".into(),
        }))
        .await
        .unwrap()
    {
        Reply::GitIdentity(missing) => missing,
        other => panic!("{other:?}"),
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_start_without_a_git_identity_waits_until_the_user_saves_one() {
    // No name or email anywhere git looks: an empty global file of this test's own (each test is its own process),
    // no system file, and no guessing them from the machine's name.
    let home = tempfile::tempdir().unwrap();
    let global = home.path().join("gitconfig");
    std::fs::write(&global, "").unwrap();
    // SAFETY: set before the service starts any thread that reads the environment; the test runs in its own process.
    unsafe {
        std::env::set_var("GIT_CONFIG_GLOBAL", &global);
        std::env::set_var("GIT_CONFIG_NOSYSTEM", "1");
    }
    let w = TestService::start().await;
    let project = w.add_project("shop-web").await;
    let repo = PathBuf::from(&project.repo_path);
    git(&repo, &["config", "--unset", "user.name"]);
    git(&repo, &["config", "--unset", "user.email"]);
    git(&repo, &["config", "user.useConfigOnly", "true"]);
    let client = Client::connect(ClientConfig::new(&w.data)).await.unwrap();
    assert_eq!(
        missing(&client).await,
        Some(Identity::default()),
        "the app asks first"
    );

    let t = w.create("SHOP", "Tidy the README").await;
    let key = t.key.to_string();
    w.board
        .move_ticket(&key, DropTarget::ReadyForDev, true, &me())
        .await
        .unwrap();
    // The scheduler may have taken it first: either way it ends Held, never started.
    let outcome = w
        .starter
        .start(&key, &me(), otaq_core::session::Start::Auto)
        .await
        .unwrap();
    assert!(
        matches!(&outcome, StartOutcome::Held { message } if message == NEEDS_IDENTITY)
            || outcome == StartOutcome::Skipped,
        "{outcome:?}"
    );
    let held = tokio::time::timeout(Duration::from_secs(20), async {
        loop {
            let ticket = w.board.ticket(&key).await.unwrap();
            if ticket.status == Status::Held {
                return ticket;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("held");
    let problem = held.start_problem.clone().unwrap();
    assert_eq!(
        (problem.step, problem.message.as_str()),
        (StartStep::GitIdentity, NEEDS_IDENTITY)
    );
    assert!(
        held.work
            .as_ref()
            .and_then(|w| w.worktree.as_ref())
            .is_none(),
        "nothing was created"
    );
    assert!(
        std::fs::read_to_string(&global).unwrap().is_empty(),
        "nothing written"
    );

    // The user's Save: written where every repository reads it, and the held start goes on.
    let Reply::Ok = client
        .request(Request::GitSetIdentity(GitIdentityParams {
            name: "Ada Lovelace".into(),
            email: "ada@example.com".into(),
        }))
        .await
        .unwrap()
    else {
        panic!("saved");
    };
    let saved = std::fs::read_to_string(&global).unwrap();
    assert!(
        saved.contains("Ada Lovelace") && saved.contains("ada@example.com"),
        "{saved}"
    );
    assert_eq!(missing(&client).await, None);
    let after = tokio::time::timeout(Duration::from_secs(20), async {
        loop {
            let ticket = w.board.ticket(&key).await.unwrap();
            if ticket.status != Status::Held {
                return ticket;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("the start goes on");
    assert!(
        after
            .start_problem
            .as_ref()
            .is_none_or(|p| p.step != StartStep::GitIdentity),
        "{:?}",
        after.start_problem
    );
    w.stop().await;
}

/// A resumed conversation commits as much as a new one: Resume of an interrupted ticket waits Held for the name and
/// email too, launching nothing.
#[tokio::test(flavor = "multi_thread")]
async fn a_resume_without_a_git_identity_waits_too() {
    let home = tempfile::tempdir().unwrap();
    let global = home.path().join("gitconfig");
    std::fs::write(&global, "").unwrap();
    // SAFETY: set before the service starts any thread that reads the environment; the test runs in its own process.
    unsafe {
        std::env::set_var("GIT_CONFIG_GLOBAL", &global);
        std::env::set_var("GIT_CONFIG_NOSYSTEM", "1");
    }
    let w = TestService::start().await;
    let project = w.add_project("shop-web").await;
    let repo = PathBuf::from(&project.repo_path);
    let t = w.create("SHOP", "Price table").await;
    let key = t.key.to_string();
    // An earlier start's work and conversation, interrupted by a restart.
    w.start_worktree(&t).await;
    w.record_ended_agent(&t, "0199d2a4-3b1c-7c3e-9f00-123456789abc")
        .await;
    w.force_status(&t, Status::Interrupted).await;
    git(&repo, &["config", "--unset", "user.name"]);
    git(&repo, &["config", "--unset", "user.email"]);
    git(&repo, &["config", "user.useConfigOnly", "true"]);

    let _ = w.review.resume(&key, &me()).await;
    let held = tokio::time::timeout(Duration::from_secs(20), async {
        loop {
            let ticket = w.board.ticket(&key).await.unwrap();
            if ticket.status == Status::Held {
                return ticket;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("held");
    assert_eq!(
        held.start_problem.as_ref().map(|p| p.step),
        Some(StartStep::GitIdentity)
    );
    assert!(
        w.sessions
            .running_of(&t.id, otaq_core::session::SessionKind::Agent)
            .is_none(),
        "nothing launched"
    );
    w.stop().await;
}
