//! Needs you, Delivery and cleanup on stages (T-0403; STG-11; d0108, d0016, d0018, d0038, d0064, d0079): needs-you
//! group 3 is "waiting in a human check or a click Delivery" (a ticket in the test flow's QA is in it), builds are
//! freed in human checks and Delivery stages, a ticket keeps its worktree in its Delivery stage until Merged or Closed,
//! and a Delivery's conflict is fixed in the Agent stage. `claude` stands in as `sh`; every data folder is temporary.

use otaq_core::service::testing::said;
use std::path::{Path, PathBuf};

use otaq_core::agent_tools::LaunchContext;
use otaq_core::agent_tools::claude::ClaudeLaunch;
use otaq_core::board::needs_you::{NeedsYouGroup, ProjectTickets, group_of, needs_you_items};
use otaq_core::board::projects::AddProject;
use otaq_core::board::tickets::{Base, Ticket};
use otaq_core::domain::{Actor, AutoMergeStop, Flags, Place, Status, TicketKey, ValidationTrigger};
use otaq_core::flow::default::{DELIVER, DEV, REVIEW};
use otaq_core::flow::{Delivers, Delivery, DeliveryMode, Flow, FlowFile, yaml};
use otaq_core::git::testing::commit;
use otaq_core::service::ServiceConfig;
use otaq_core::service::testing::TestService;
use otaq_core::session::SessionKind;
use otaq_core::store::projects::Project;
use otaq_core::ticket_env::free::freed_in;

const NOW: i64 = 1_791_600_000_000;
const MIN: i64 = 60_000;

fn me() -> Actor {
    Actor::User("me".into())
}

fn every_kind_file() -> FlowFile {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/flows/every-kind.yaml");
    yaml::read(&std::fs::read_to_string(path).unwrap()).unwrap()
}

fn every_kind() -> Flow {
    every_kind_file().flow
}

fn ticket(key: &str, place: Place, status: Status, minutes_ago: i64) -> Ticket {
    Ticket {
        id: key.into(),
        key: key.parse::<TicketKey>().unwrap(),
        title: key.into(),
        body: String::new(),
        priority: Default::default(),
        place,
        status,
        base: Base::Branch("main".into()),
        base_locked: false,
        external_id: None,
        depends_on: Vec::new(),
        flags: Flags::default(),
        created_by: me(),
        updated_by: None,
        started_at: None,
        created_at: 0,
        updated_at: 0,
        status_since: Some(NOW - minutes_ago * MIN),
        queued_at: None,
        ready_since: None,
        validation_round: 0,
        auto_merge_conflicts: 0,
        auto_merge_stop: None,
        work: None,
        start_problem: None,
        interrupted_from: None,
        signal: None,
        question: None,
        pushed_differs: None,
    }
}

// ---- Needs you ----

/// Group 3 is every human check's wait and a click Delivery's To deliver; PR open waits for nobody's click in the
/// Needs you, as today. A ticket in the test flow's QA is in group 3 (its Delivery in click mode here); one waiting at
/// its Delivery comes first (closest to Done), whatever its age; inside the human checks the older comes first, as
/// today.
#[test]
fn group_three_is_waiting_in_a_human_check_or_a_click_delivery() {
    let mut flow = every_kind();
    for stage in flow.stages.iter_mut().filter(|s| s.id == "ship") {
        stage.delivery = Some(Delivery {
            mode: DeliveryMode::Click,
            delivers: Delivers::Pr,
        });
    }
    assert_eq!(group_of(Status::WaitingForYou), Some(NeedsYouGroup::Review));
    assert_eq!(group_of(Status::ToDeliver), Some(NeedsYouGroup::Review));
    for status in [Status::PrOpen, Status::Merged, Status::Checking] {
        assert_eq!(group_of(status), None, "{status}");
    }

    let tickets = vec![
        ticket("NOTE-1", Place::stage_of("qa"), Status::WaitingForYou, 30),
        ticket("NOTE-2", Place::stage_of("qa"), Status::WaitingForYou, 60),
        ticket("NOTE-3", Place::stage_of("ship"), Status::ToDeliver, 5),
        ticket("NOTE-4", Place::stage_of("security"), Status::Checking, 90),
        ticket("NOTE-5", Place::stage_of("ship"), Status::PrOpen, 90),
    ];
    let items = needs_you_items(
        &[ProjectTickets {
            key: "NOTE",
            name: "pocket-notes",
            tickets: &tickets,
            flow: &flow,
        }],
        NOW,
    );
    let keys: Vec<_> = items.iter().map(|i| i.key.to_string()).collect();
    assert_eq!(keys, ["NOTE-3", "NOTE-2", "NOTE-1"]);
    assert!(items.iter().all(|i| i.group == NeedsYouGroup::Review));
    assert_eq!(items[1].reason, "ready for QA");
}

// ---- Builds and worktrees ----

/// Builds are freed in every human check and Delivery status, nowhere else (STG-11).
#[test]
fn builds_are_freed_in_human_checks_and_deliveries() {
    for status in Status::ALL {
        let expected = matches!(
            status,
            Status::WaitingForYou | Status::ToDeliver | Status::PrOpen
        );
        assert_eq!(freed_in(*status), expected, "{status}");
    }
}

async fn world() -> TestService {
    TestService::start_with(|config| ServiceConfig {
        launch: Some(LaunchContext {
            bin_dir: "/nonexistent".into(),
            plugin_dir: "/nonexistent/plugin".into(),
            path: std::env::var_os("PATH").unwrap_or_default(),
            claude: ClaudeLaunch {
                program: vec![
                    "sh".into(),
                    "-c".into(),
                    "exec sleep 30".into(),
                    "sh".into(),
                ],
                config_dir: None,
            },
        }),
        ..config
    })
    .await
}

/// The example's pocket-notes, key NOTE.
async fn pocket_notes(w: &TestService) -> Project {
    let repo = w.repos.repo("pocket-notes", 1);
    w.board
        .add_project(&AddProject {
            path: repo.display().to_string(),
            key: Some("NOTE".into()),
            ..Default::default()
        })
        .await
        .unwrap()
}

/// A ticket of NOTE with one commit in its worktree, waiting in Review.
async fn in_review(w: &TestService, title: &str, file: &str) -> (Ticket, PathBuf) {
    let t = w.create("NOTE", title).await;
    let wt = w.start_worktree(&t).await;
    commit(&wt, file, &format!("{title}\n"));
    w.force_status(&t, Status::WaitingForYou).await;
    (w.board.ticket(&t.key.to_string()).await.unwrap(), wt)
}

/// PR open is the Delivery stage: the ticket stays there with its worktree until Mark merged or Mark closed, which
/// remove it as today.
#[tokio::test(flavor = "multi_thread")]
async fn a_pr_open_ticket_keeps_its_worktree_until_merged_or_closed() {
    let w = TestService::start().await;
    pocket_notes(&w).await;
    for (n, merged) in [(481, true), (482, false)] {
        let (t, wt) = in_review(&w, &format!("Export {n}"), &format!("{n}.rs")).await;
        assert!(t.place.is_in(REVIEW));
        let key = t.key.to_string();
        let open = w
            .prs
            .opened(
                &key,
                &format!("https://github.com/acme/pocket-notes/pull/{n}"),
                &me(),
            )
            .await
            .unwrap();
        let open = w.board.ticket(&open.key.to_string()).await.unwrap();
        assert_eq!(open.status, Status::PrOpen);
        assert_eq!(
            open.place,
            Place::stage_of(DELIVER),
            "in the Delivery stage"
        );
        assert!(wt.is_dir(), "PR open keeps the worktree");

        let done = if merged {
            *w.review.mark_merged(&key, &me()).await.unwrap().ticket
        } else {
            w.review.mark_closed(&key, true, &me()).await.unwrap()
        };
        assert!(done.place.is_done(), "{:?}", done.place);
        assert!(!wt.exists(), "removed at {}", done.status);
    }
    w.stop().await;
}

/// An automatic Delivery's conflict is fixed in the Agent stage before it (the card is in Dev, "Merge conflict ·
/// fixing"), the third stops there (FLW-17, d0108 second gap review rule 12).
#[tokio::test(flavor = "multi_thread")]
async fn a_delivery_conflict_is_fixed_in_the_agent_stage() {
    let w = world().await;
    let project = pocket_notes(&w).await;
    let repo = PathBuf::from(&project.repo_path);
    w.board
        .edit_flow_for_tests(
            "NOTE",
            otaq_core::flow::default::testing::automatic_local_delivery,
        )
        .await;
    let t = w.create("NOTE", "Sort notes by date").await;
    let wt = w.start_worktree(&t).await;
    commit(&wt, "sort.rs", "by date\n");
    w.force_status(&t, Status::Working).await;
    w.agent_shell(&t).await;
    commit(&repo, "sort.rs", "main\n");

    for n in 1..=3 {
        let key = t.key.to_string();
        w.validation
            .submit(
                &key,
                said(&w.board, &key, "done").await.as_ref(),
                &Actor::Agent(t.key.clone()),
                ValidationTrigger::Agent,
            )
            .await
            .unwrap();
        let checker = w
            .sessions
            .running_of(&t.id, SessionKind::Validator)
            .unwrap()
            .id;
        w.validation
            .verdict(
                &key,
                true,
                said(&w.board, &key, "ok").await.as_ref(),
                &checker,
            )
            .await
            .unwrap();
        let after = w.board.ticket(&key).await.unwrap();
        assert_eq!(after.place, Place::stage_of(DEV), "conflict {n}");
        if n < 3 {
            assert_eq!(after.status, Status::MergeConflictFixing);
        } else {
            assert_eq!(after.status, Status::DeliveryStopped);
            assert_eq!(after.auto_merge_stop, Some(AutoMergeStop::KeepsConflicting));
        }
    }
    w.stop().await;
}
