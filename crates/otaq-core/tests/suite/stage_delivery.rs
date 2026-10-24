//! Needs you, Delivery, cleanup and epics on stages (T-0403; STG-11; d0108, d0016, d0018, d0038, d0059, d0064, d0080,
//! d0079): needs-you group 3 is "waiting in a human check or a click Delivery" (a ticket in the test flow's QA is in
//! it), builds are freed in human checks and Delivery stages, a ticket keeps its worktree in its Delivery stage
//! until Merged or Closed, a Delivery's conflict is fixed in the Agent stage, and an epic stops at its flow's last
//! human check, or at the implicit Review epic when the flow has none, where it counts as quiet. A child's Delivery
//! always merges into the epic's branch. `claude` stands in as `sh`; every data folder is temporary.

use otaq_core::service::testing::said;
use std::path::{Path, PathBuf};

use otaq_core::agent_tools::LaunchContext;
use otaq_core::agent_tools::claude::ClaudeLaunch;
use otaq_core::board::epic_flight::{
    EpicPlace, REVIEW_EPIC, REVIEW_EPIC_NAME, delivery_of, epic_flow, epic_is_quiet, epic_place,
    epic_stop,
};
use otaq_core::board::needs_you::{NeedsYouGroup, ProjectTickets, group_of, needs_you_items};
use otaq_core::board::projects::AddProject;
use otaq_core::board::tickets::{Base, NewTicketInput, Ticket};
use otaq_core::domain::{
    Actor, AutoMergeStop, Flags, Kind, Place, Status, TicketKey, ValidationTrigger,
};
use otaq_core::flow::default::{DELIVER, DEV, REVIEW};
use otaq_core::flow::engine::{self, At, Move, RunResult};
use otaq_core::flow::{Delivers, Delivery, DeliveryMode, Flow, FlowFile, default_flow, yaml};
use otaq_core::git::testing::{commit, git};
use otaq_core::review::LocalMergeOutcome;
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

/// The test flow without its human check (QA): grill → dev → validate → security → ship.
fn no_human_check() -> Flow {
    let mut flow = every_kind();
    flow.stages.retain(|s| !s.is_human_check());
    flow
}

fn ticket(key: &str, place: Place, status: Status, minutes_ago: i64) -> Ticket {
    Ticket {
        id: key.into(),
        key: key.parse::<TicketKey>().unwrap(),
        kind: Kind::Ticket,
        title: key.into(),
        body: String::new(),
        priority: Default::default(),
        priority_inherited: false,
        epic: None,
        place,
        status,
        base: Base::Branch("main".into()),
        base_locked: false,
        external_id: None,
        depends_on: Vec::new(),
        aliases: Vec::new(),
        next_child_number: None,
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
        epic_queue_mode: Default::default(),
        joined_queue_by: None,
    }
}

fn epic_at(place: Place, status: Status) -> Ticket {
    Ticket {
        kind: Kind::Epic,
        ..ticket("NOTE-10", place, status, 1)
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

// ---- Epics ----

/// The epic's stop: the flow's last human check (the default flow's Review, the test flow's QA, with its `skip`
/// gone), or the implicit Review epic when the flow has none.
#[test]
fn an_epic_stops_at_the_last_human_check_or_at_review_epic() {
    assert_eq!(epic_stop(&default_flow()).id, REVIEW);
    let qa = epic_stop(&every_kind());
    assert_eq!(qa.id, "qa");
    assert!(!qa.can_skip(), "an epic never skips its review");

    let stop = epic_stop(&no_human_check());
    assert_eq!(stop.id, REVIEW_EPIC);
    assert_eq!(stop.name, REVIEW_EPIC_NAME);
    assert!(stop.is_human_check());

    // Its flow: the stop, then the Delivery.
    let ids = |flow: &Flow| -> Vec<String> { flow.stages.iter().map(|s| s.id.clone()).collect() };
    assert_eq!(ids(&epic_flow(&default_flow())), [REVIEW, DELIVER]);
    assert_eq!(ids(&epic_flow(&no_human_check())), [REVIEW_EPIC, "ship"]);
}

/// At Review epic the epic is in group 3, quiet, a delivery card; its Pass delivers as the flow's Delivery says
/// (the test flow's automatic PR), and there is no Fail on an epic. On the default flow its Pass offers the Local
/// merge / Open PR choice, as today.
#[test]
fn an_epic_at_review_epic_waits_quietly_for_its_pass() {
    let flow = no_human_check();
    let epic = epic_at(Place::stage_of(REVIEW_EPIC), Status::WaitingForYou);
    assert_eq!(epic_place(&epic, &[]), EpicPlace::Delivery);
    assert!(epic_is_quiet(&epic, &flow));
    assert!(!epic_is_quiet(&epic, &default_flow()), "not its stop there");
    assert!(!epic_is_quiet(
        &epic_at(Place::Backlog, Status::Backlog),
        &flow
    ));
    let items = needs_you_items(
        &[ProjectTickets {
            key: "NOTE",
            name: "pocket-notes",
            tickets: std::slice::from_ref(&epic),
            flow: &flow,
        }],
        NOW,
    );
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].group, NeedsYouGroup::Review);

    let walked = epic_flow(&flow);
    let at = At::new(&walked, &epic.place, Status::WaitingForYou);
    let pass = engine::step(at, Move::HumanPass).unwrap();
    assert_eq!(pass.place, Place::stage_of("ship"));
    assert!(pass.automatic);
    assert!(
        engine::step(at, Move::HumanFail).is_err(),
        "no Fail on an epic"
    );

    let walked = epic_flow(&default_flow());
    let place = Place::stage_of(REVIEW);
    let pass = engine::step(
        At::new(&walked, &place, Status::WaitingForYou),
        Move::HumanPass,
    )
    .unwrap();
    assert_eq!(pass.choose, Some(Delivers::Either));
    assert!(
        engine::step(
            At::new(&walked, &place, Status::WaitingForYou),
            Move::HumanFail
        )
        .is_err()
    );
}

/// A child's Delivery always Local-merges into the epic's branch, in the stage's mode: on the test flow's automatic
/// PR Delivery a child merges locally, a standalone ticket opens its PR.
#[test]
fn a_childs_delivery_merges_into_the_epic_branch() {
    let flow = every_kind();
    let standalone = ticket("NOTE-4", Place::stage_of("ship"), Status::ToDeliver, 1);
    let child = Ticket {
        epic: Some("NOTE-10".parse().unwrap()),
        ..ticket("NOTE-10.2", Place::stage_of("ship"), Status::ToDeliver, 1)
    };
    assert_eq!(
        delivery_of(&flow, &standalone),
        Some(Delivery {
            mode: DeliveryMode::Automatic,
            delivers: Delivers::Pr
        })
    );
    assert_eq!(
        delivery_of(&flow, &child),
        Some(Delivery {
            mode: DeliveryMode::Automatic,
            delivers: Delivers::Local
        })
    );
    assert_eq!(
        delivery_of(&default_flow(), &child).map(|d| (d.mode, d.delivers)),
        Some((DeliveryMode::Click, Delivers::Local))
    );
}

async fn create(w: &TestService, input: NewTicketInput) -> Ticket {
    w.board.create_ticket(&input, &me()).await.unwrap()
}

/// NOTE-10 "Sharing": its children merge into the epic's branch and reach Done; with every child done the epic
/// stops at Review (the default flow's last human check), quiet, and `main` hasn't moved.
#[tokio::test(flavor = "multi_thread")]
async fn epic_children_merge_into_the_epic_branch_and_the_epic_stops_at_review() {
    let w = TestService::start().await;
    let project = pocket_notes(&w).await;
    let repo = PathBuf::from(&project.repo_path);
    let main_before = git(&repo, &["rev-parse", "main"]);
    let epic = create(
        &w,
        NewTicketInput {
            project: "NOTE".into(),
            title: "Sharing".into(),
            kind: Kind::Epic,
            ..Default::default()
        },
    )
    .await;
    let mut children = Vec::new();
    for title in ["Share link", "Share by email"] {
        children.push(
            create(
                &w,
                NewTicketInput {
                    project: "NOTE".into(),
                    title: title.into(),
                    epic: Some(epic.key.to_string()),
                    ..Default::default()
                },
            )
            .await,
        );
    }
    for (child, file) in children.iter().zip(["link.rs", "email.rs"]) {
        let wt = w.start_worktree(child).await;
        commit(&wt, file, "share\n");
        w.force_status(child, Status::WaitingForYou).await;
        let key = child.key.to_string();
        let proposal = w.board.merge_proposal(&key).await.unwrap();
        assert!(proposal.into_epic, "{key} merges into the epic branch");
        let outcome = w
            .review
            .local_merge(&key, &proposal.message, false, &me())
            .await
            .unwrap();
        assert!(
            matches!(outcome, LocalMergeOutcome::Merged { .. }),
            "{outcome:?}"
        );
        assert_eq!(w.board.ticket(&key).await.unwrap().place, Place::Done);
    }

    let epic = w.board.ticket(&epic.key.to_string()).await.unwrap();
    assert_eq!(epic.status, Status::WaitingForYou);
    assert_eq!(epic.place, Place::stage_of(REVIEW));
    assert!(epic_is_quiet(&epic, &default_flow()));
    let branch = epic.work.as_ref().and_then(|w| w.branch.clone()).unwrap();
    let files = git(&repo, &["ls-tree", "--name-only", &branch]);
    assert!(
        files.contains("link.rs") && files.contains("email.rs"),
        "{files}"
    );
    assert_eq!(
        git(&repo, &["rev-parse", "main"]),
        main_before,
        "main untouched"
    );
    w.stop().await;
}

/// On the test flow without QA (no human check) NOTE-10 stops at Review epic once its child is merged into the
/// epic's branch: in group 3, quiet, a delivery card. Its Local merge is the Pass and the Delivery: Done, with a
/// passed run of Review epic and a done run of the Delivery, and `main` gets the epic's squash.
#[tokio::test(flavor = "multi_thread")]
async fn on_a_flow_without_a_human_check_an_epic_is_approved_and_delivered_at_review_epic() {
    let w = TestService::start().await;
    let project = pocket_notes(&w).await;
    let repo = PathBuf::from(&project.repo_path);
    let flow = no_human_check();
    w.board
        .use_flow_for_tests(
            "NOTE",
            &FlowFile {
                agents: every_kind_file().agents,
                flow: flow.clone(),
            },
        )
        .await;
    let main_before = git(&repo, &["rev-parse", "main"]);
    let epic = create(
        &w,
        NewTicketInput {
            project: "NOTE".into(),
            title: "Sharing".into(),
            kind: Kind::Epic,
            ..Default::default()
        },
    )
    .await;
    let child = create(
        &w,
        NewTicketInput {
            project: "NOTE".into(),
            title: "Share link".into(),
            epic: Some(epic.key.to_string()),
            ..Default::default()
        },
    )
    .await;
    let wt = w.start_worktree(&child).await;
    commit(&wt, "link.rs", "share\n");
    w.force_status(&child, Status::WaitingForYou).await;
    let key = child.key.to_string();
    let proposal = w.board.merge_proposal(&key).await.unwrap();
    assert!(proposal.into_epic);
    let merged = w
        .review
        .local_merge(&key, &proposal.message, false, &me())
        .await
        .unwrap();
    assert!(
        matches!(merged, LocalMergeOutcome::Merged { .. }),
        "{merged:?}"
    );

    let epic_key = epic.key.to_string();
    let at_stop = w.board.ticket(&epic_key).await.unwrap();
    assert_eq!(at_stop.status, Status::WaitingForYou);
    assert_eq!(at_stop.place, Place::stage_of(REVIEW_EPIC));
    assert!(epic_is_quiet(&at_stop, &flow));
    let tickets = w.board.tickets("NOTE").await.unwrap();
    assert_eq!(epic_place(&at_stop, &tickets), EpicPlace::Delivery);
    let items = w.board.needs_you_items(NOW).await.unwrap();
    assert!(
        items
            .iter()
            .any(|i| i.key == at_stop.key && i.group == NeedsYouGroup::Review),
        "{items:?}"
    );
    assert_eq!(git(&repo, &["rev-parse", "main"]), main_before);

    let proposal = w.board.merge_proposal(&epic_key).await.unwrap();
    assert!(!proposal.into_epic);
    let delivered = w
        .review
        .local_merge(&epic_key, &proposal.message, false, &me())
        .await
        .unwrap();
    assert!(
        matches!(delivered, LocalMergeOutcome::Merged { .. }),
        "{delivered:?}"
    );
    let done = w.board.ticket(&epic_key).await.unwrap();
    assert_eq!((done.status, done.place), (Status::Merged, Place::Done));
    let runs: Vec<_> = w
        .board
        .stage_runs(&epic_key)
        .await
        .unwrap()
        .into_iter()
        .map(|r| (r.stage, r.kind, r.result))
        .collect();
    assert_eq!(
        runs,
        [
            (
                REVIEW_EPIC.to_owned(),
                "you".to_owned(),
                Some(RunResult::Passed)
            ),
            (
                "ship".to_owned(),
                "delivery".to_owned(),
                Some(RunResult::Done)
            ),
        ]
    );
    assert!(
        repo.join("link.rs").is_file(),
        "the epic squashed into main"
    );
    w.stop().await;
}
