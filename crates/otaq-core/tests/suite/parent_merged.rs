//! A parent merged (SCH-05; d0049; T-0081): the base is fetched before the children that became ready start,
//! so they start from the merged code. A bare repository plays the remote; nothing goes over the network.

use crate::common;

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::common::{Fixture, me};
use otaq_core::board::moves::DropTarget;
use otaq_core::board::projects::AddProject;
use otaq_core::board::tickets::{NewTicketInput, Ticket};
use otaq_core::data_dir::DataDir;
use otaq_core::domain::{StartStep, Status};
use otaq_core::flow::Delivers;
use otaq_core::flow::default::testing::delivering;
use otaq_core::git::testing::{commit, git};
use otaq_core::ports::PortPool;
use otaq_core::scheduler::{Scheduler, StartTickets, ToStart};
use otaq_core::service::EventBus;
use otaq_core::session::Sessions;
use otaq_core::start::Starter;

struct World {
    f: Fixture,
    clone: PathBuf,
    other: PathBuf,
}

/// shop-web as a clone of a bare origin (`main`), plus another clone that pushes "the merged PR".
async fn world() -> World {
    let f = Fixture::new().await;
    let clone = f.repos.clone_with_origin("shop-web", "main", 1);
    let other = f.repos.root().join("shop-web-seed");
    f.board
        .add_project(&AddProject {
            path: clone.display().to_string(),
            ..Default::default()
        })
        .await
        .unwrap();
    World { f, clone, other }
}

async fn ticket(f: &Fixture, title: &str, parents: &[&str]) -> Ticket {
    let t = f
        .create_with(NewTicketInput {
            title: title.into(),
            ..common::shop_input()
        })
        .await;
    for parent in parents {
        f.board
            .add_dependency(&t.key.to_string(), parent, &me())
            .await
            .unwrap();
    }
    t
}

async fn set(f: &Fixture, key: &str, status: Status) {
    let t = f.ticket(key).await;
    sqlx::query("UPDATE tickets SET stage_id = ?, state = ? WHERE id = ?")
        .bind(otaq_core::flow::engine::default_stage(status, None))
        .bind(status.as_str())
        .bind(&t.id)
        .execute(f.board.store().pool())
        .await
        .unwrap();
}

async fn queue(f: &Fixture, key: &str) {
    f.board
        .move_ticket(key, DropTarget::ReadyForDev, true, &me())
        .await
        .unwrap();
}

/// SHOP-1 (the parent, approved with Open PR) and SHOP-2 waiting for it in Queued.
async fn chain(w: &World) {
    ticket(&w.f, "Checkout API", &[]).await;
    ticket(&w.f, "Checkout page", &["SHOP-1"]).await;
    queue(&w.f, "SHOP-2").await;
    set(&w.f, "SHOP-1", Status::PrOpen).await;
}

/// The PR lands on the remote, then the user clicks Mark merged.
async fn pr_lands(w: &World) -> String {
    commit(&w.other, "checkout.rs", "merged\n");
    git(&w.other, &["push", "--quiet", "origin", "main"]);
    set(&w.f, "SHOP-1", Status::Merged).await;
    git(&w.other, &["rev-parse", "HEAD"])
}

#[derive(Default)]
struct Recorder(Mutex<Vec<String>>);

impl StartTickets for Recorder {
    fn spawn_start(&self, ticket: &ToStart) {
        self.0.lock().unwrap().push(ticket.key.to_string());
    }

    fn starting(&self) -> HashSet<String> {
        HashSet::new()
    }
}

fn recording(w: &World) -> (Arc<Recorder>, Scheduler) {
    let recorder = Arc::new(Recorder::default());
    let scheduler = Scheduler::new(
        w.f.board.clone(),
        recorder.clone(),
        EventBus::new(16),
        Duration::from_secs(3600),
    );
    (recorder, scheduler)
}

#[tokio::test]
async fn children_of_a_mark_merged_parent_start_at_the_new_remote_head() {
    let w = world().await;
    chain(&w).await;
    let events = EventBus::new(64);
    let data = DataDir::new(w.f.home.path().join("home"));
    let starter = Starter::new(
        w.f.board.clone(),
        data.clone(),
        PortPool::new(w.f.board.store().clone()),
        Sessions::new(w.f.board.store().clone(), events.clone()),
        otaq_core::agent_tools::Adapters::launching_only(None, data),
        events.clone(),
    );
    let scheduler = Scheduler::new(
        w.f.board.clone(),
        Arc::new(starter),
        events,
        Duration::from_secs(3600),
    );
    let head = pr_lands(&w).await;
    assert_ne!(
        git(&w.clone, &["rev-parse", "main"]),
        head,
        "not fetched yet"
    );

    let plan = scheduler.on_parent_merged("SHOP-1").await.unwrap();
    assert_eq!(plan.start.len(), 1);
    assert_eq!(
        git(&w.clone, &["rev-parse", "main"]),
        head,
        "fetched and fast-forwarded"
    );

    // The start runs in the background; without a launch context it stops at its launch step, after the
    // worktree was made from the base.
    let started = tokio::time::timeout(Duration::from_secs(20), async {
        loop {
            let t = w.f.ticket("SHOP-2").await;
            if t.status == Status::StartInterrupted {
                return t;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("the start finished");
    assert_eq!(started.start_problem.unwrap().step, StartStep::Launch);
    assert_eq!(started.work.unwrap().base_sha.unwrap(), head);
}

#[tokio::test]
async fn a_diverged_base_holds_the_children_with_the_message() {
    let w = world().await;
    chain(&w).await;
    commit(&w.clone, "local.rs", "mine\n"); // local main has a commit origin lacks
    pr_lands(&w).await;
    let (recorder, scheduler) = recording(&w);

    scheduler.on_parent_merged("SHOP-1").await.unwrap();
    assert!(recorder.0.lock().unwrap().is_empty());
    let child = w.f.ticket("SHOP-2").await;
    assert_eq!(child.status, Status::Held);
    let problem = child.start_problem.unwrap();
    assert_eq!(problem.step, StartStep::SyncBase);
    assert!(
        problem.message.contains("have diverged"),
        "{}",
        problem.message
    );

    // Later passes leave it held: only the user's Retry start runs it again.
    scheduler.pass().await.unwrap();
    assert_eq!(w.f.ticket("SHOP-2").await.status, Status::Held);
}

/// A flow that delivers by PR needs the remote (d0121).
#[tokio::test]
async fn a_failed_fetch_holds_the_children_and_nothing_retries() {
    let w = world().await;
    w.f.board
        .edit_flow_for_tests("SHOP", delivering(Delivers::Pr))
        .await;
    chain(&w).await;
    set(&w.f, "SHOP-1", Status::Merged).await;
    git(
        &w.clone,
        &[
            "remote",
            "set-url",
            "origin",
            "/nonexistent/shop-web-origin.git",
        ],
    );
    let (recorder, scheduler) = recording(&w);

    scheduler.on_parent_merged("SHOP-1").await.unwrap();
    assert!(recorder.0.lock().unwrap().is_empty());
    let child = w.f.ticket("SHOP-2").await;
    assert_eq!(child.status, Status::Held);
    let message = child.start_problem.unwrap().message;
    assert!(message.starts_with("Fetching `main` failed"), "{message}");
    for _ in 0..3 {
        scheduler.pass().await.unwrap();
    }
    assert!(recorder.0.lock().unwrap().is_empty());
}

/// d0121: on a flow that doesn't deliver by PR (the default lets the user pick), a failed fetch starts the freed
/// children from the local base; each start records the warning.
#[tokio::test]
async fn a_failed_fetch_on_a_local_flow_starts_the_children() {
    let w = world().await;
    chain(&w).await;
    set(&w.f, "SHOP-1", Status::Merged).await;
    git(
        &w.clone,
        &[
            "remote",
            "set-url",
            "origin",
            "/nonexistent/shop-web-origin.git",
        ],
    );
    let (recorder, scheduler) = recording(&w);

    scheduler.on_parent_merged("SHOP-1").await.unwrap();
    assert_eq!(*recorder.0.lock().unwrap(), ["SHOP-2"]);
    assert_eq!(w.f.ticket("SHOP-2").await.status, Status::Queued);
}

#[tokio::test]
async fn an_epic_branch_is_local_and_not_fetched() {
    let w = world().await;
    let epic =
        w.f.create_with(NewTicketInput {
            title: "Wishlist".into(),
            kind: otaq_core::domain::Kind::Epic,
            ..common::shop_input()
        })
        .await;
    for title in ["Wishlist API", "Wishlist button"] {
        w.f.child(&epic, title).await;
    }
    w.f.board
        .add_dependency("SHOP-1.2", "SHOP-1.1", &me())
        .await
        .unwrap();
    queue(&w.f, "SHOP-1.2").await;
    set(&w.f, "SHOP-1.1", Status::Merged).await;
    // Even with an unreachable remote, a sibling merged into the epic branch starts the child.
    git(
        &w.clone,
        &[
            "remote",
            "set-url",
            "origin",
            "/nonexistent/shop-web-origin.git",
        ],
    );
    let (recorder, scheduler) = recording(&w);
    scheduler.on_parent_merged("SHOP-1.1").await.unwrap();
    assert_eq!(*recorder.0.lock().unwrap(), ["SHOP-1.2"]);
}
