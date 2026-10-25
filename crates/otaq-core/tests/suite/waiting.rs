//! Waiting before every Agent stage, and the limit (T-0401; STG-07; d0108, d0016, d0049, d0079): on the test flow
//! (`tests/flows/every-kind.yaml`) a ticket finishing Grill waits before Dev like a queued ticket, through the
//! scheduler and the limit, and starts there in the worktree it has; a skipped Grill goes straight to waiting before
//! Dev. The limit counts every ticket with a live agent session, whatever its stage (an agent check, a PR Delivery
//! session), and no waiting card. `claude` stands in as `sh`; every data folder is temporary.

use otaq_core::service::testing::said;
use std::collections::HashSet;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::common::Fixture;
use otaq_core::agent_tools::LaunchContext;
use otaq_core::agent_tools::claude::ClaudeLaunch;
use otaq_core::board::moves::DropTarget;
use otaq_core::board::settings::{AgentLimits, GlobalSettings};
use otaq_core::board::tickets::{Base, Ticket};
use otaq_core::domain::{Actor, Flags, Place, Status, TicketKey, ValidationTrigger};
use otaq_core::flow::engine::{self, At, Move, RunResult};
use otaq_core::flow::{Flow, FlowFile, yaml};
use otaq_core::git::testing::commit;
use otaq_core::scheduler::{
    Gate, HOLDS_A_SLOT, Scheduler, StartTickets, ToStart, plan, running_count, stock_limit,
};
use otaq_core::service::testing::TestService;
use otaq_core::service::{EventBus, ServiceConfig};
use otaq_core::session::SessionKind;

fn every_kind_file() -> FlowFile {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/flows/every-kind.yaml");
    yaml::read(&std::fs::read_to_string(path).unwrap()).unwrap()
}

fn every_kind() -> Flow {
    every_kind_file().flow
}

fn before(stage: &str) -> Place {
    Place::Waiting(Some(stage.to_owned()))
}

/// A ticket in `place` with `status`, queued at `queued_at`.
fn t(key: &str, place: Place, status: Status, queued_at: i64) -> Ticket {
    Ticket {
        id: key.into(),
        key: key.parse::<TicketKey>().unwrap(),
        title: key.into(),
        body: String::new(),
        plan: String::new(),
        plan_by: None,
        plan_at: None,
        priority: Default::default(),
        place,
        status,
        base: Base::Branch("main".into()),
        base_locked: false,
        external_id: None,
        depends_on: Vec::new(),
        flags: Flags::default(),
        created_by: Actor::User("me".into()),
        updated_by: None,
        started_at: None,
        created_at: 0,
        updated_at: 0,
        status_since: None,
        queued_at: Some(queued_at),
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

fn one_slot() -> Gate {
    Gate {
        limits: AgentLimits {
            enabled: true,
            max_concurrency: 1,
            back_pressure: 3,
        },
        needs_you: 0,
    }
}

fn started(tickets: &[Ticket], gate: &Gate) -> Vec<String> {
    plan(tickets, &HashSet::new(), gate)
        .start
        .iter()
        .map(|s| s.key.to_string())
        .collect()
}

// ---- The engine and the scheduler ----

/// Finishing Grill (the agent's `stage next`, or the user's Move to next stage) waits before Dev: Grill's run is done, the
/// card is queued before Dev, and only the scheduler's start enters Dev.
#[test]
fn finishing_grill_waits_before_dev() {
    let flow = every_kind();
    let grill = Place::stage_of("grill");
    for mv in [Move::NextStage, Move::SubmitByUser] {
        let step = engine::step(At::new(&flow, &grill, Status::Working), mv).unwrap();
        assert_eq!(
            (&step.place, step.status, step.ended, step.stage),
            (&before("dev"), Status::Queued, Some(RunResult::Done), None),
            "{mv:?}"
        );
        assert!(!step.start_check && !step.resume);
    }
    let begin = engine::step(At::new(&flow, &before("dev"), Status::Queued), Move::Begin).unwrap();
    assert_eq!(
        (begin.place, begin.status, begin.ended),
        (Place::stage_of("dev"), Status::Working, None)
    );
    // A Fail still goes straight back into Dev, whose session resumes: no wait.
    let fail = engine::step(
        At::new(&flow, &Place::stage_of("security"), Status::Checking),
        Move::Fail,
    )
    .unwrap();
    assert_eq!((fail.place, fail.resume), (Place::stage_of("dev"), true));
}

/// A skipped Grill goes straight to waiting before Dev: Ready for Dev is the wait before the first Agent stage the
/// ticket runs, and its start enters Dev with a skipped run of Grill.
#[test]
fn a_skipped_grill_waits_before_dev_from_ready_for_dev() {
    let flow = every_kind();
    let skips = ["grill".to_owned()];
    let at = |place: &'static Place, status| At {
        skips: &skips,
        ..At::new(&flow, place, status)
    };
    static BACKLOG: Place = Place::Backlog;
    static READY: Place = Place::Waiting(None);
    let start = engine::step(at(&BACKLOG, Status::Backlog), Move::Start).unwrap();
    assert_eq!(
        (start.place, start.status),
        (Place::Waiting(None), Status::Queued)
    );
    // With the limit full it waits like any queued ticket.
    let tickets = [
        t("NOTE-1", Place::stage_of("dev"), Status::Working, 0),
        t("NOTE-2", Place::Waiting(None), Status::Queued, 1),
    ];
    let pass = plan(&tickets, &HashSet::new(), &one_slot());
    assert!(pass.start.is_empty());
    assert_eq!(pass.changes[0].to, Status::WaitingForSlot);
    let begin = engine::step(at(&READY, Status::WaitingForSlot), Move::Begin).unwrap();
    assert_eq!(begin.place, Place::stage_of("dev"));
    let through: Vec<_> = begin
        .passed_through
        .iter()
        .map(|(s, r)| (s.id.as_str(), *r))
        .collect();
    assert_eq!(through, [("grill", RunResult::Skipped)]);
}

/// The scheduler treats a ticket waiting before Dev as it treats Ready for Dev: it waits for a slot, then starts, in
/// the start order; its dependencies hold it as they would in Ready for Dev.
#[test]
fn the_scheduler_waits_and_starts_before_any_agent_stage() {
    let mut tickets = vec![
        t("NOTE-1", Place::stage_of("dev"), Status::Working, 0),
        t("NOTE-2", before("dev"), Status::Queued, 20),
        t("NOTE-3", Place::Waiting(None), Status::Queued, 10),
    ];
    let pass = plan(&tickets, &HashSet::new(), &one_slot());
    assert!(pass.start.is_empty(), "the limit is full");
    let waiting: Vec<_> = pass
        .changes
        .iter()
        .map(|c| (c.key.to_string(), c.to))
        .collect();
    assert_eq!(
        waiting,
        [
            ("NOTE-3".to_owned(), Status::WaitingForSlot),
            ("NOTE-2".to_owned(), Status::WaitingForSlot),
        ],
        "ready the longest first, wherever it waits"
    );
    tickets[0].status = Status::WaitingForYou;
    tickets[0].place = Place::stage_of("qa");
    tickets[1].status = Status::WaitingForSlot;
    tickets[1].queued_at = Some(5);
    assert_eq!(started(&tickets, &one_slot()), ["NOTE-2"]);

    // A dependency not merged holds it before Dev too.
    tickets[1].depends_on = vec!["NOTE-1".parse().unwrap()];
    let pass = plan(&tickets, &HashSet::new(), &Gate::default());
    assert_eq!(
        pass.start
            .iter()
            .map(|s| s.key.to_string())
            .collect::<Vec<_>>(),
        ["NOTE-3"]
    );
    assert_eq!(
        (pass.changes[0].key.to_string(), pass.changes[0].to),
        ("NOTE-2".to_owned(), Status::Queued)
    );
    // It keeps its worktree: no stock build is kept for it.
    tickets[1].depends_on.clear();
    assert_eq!(stock_limit(&tickets[1..2], &tickets), 1);
}

// ---- The limit ----

/// Every ticket with a live agent session holds a slot, whatever its stage: an Agent stage's, an agent check's (an
/// own-column check such as Security too), a `restack` fix. Waiting cards (before Dev included) and a ticket waiting
/// in Review (or QA, any human check) don't.
#[test]
fn a_checking_ticket_holds_a_slot_and_waiting_cards_do_not() {
    let tickets = [
        t("NOTE-1", Place::stage_of("grill"), Status::Working, 0),
        t("NOTE-2", Place::stage_of("security"), Status::Checking, 0),
        t(
            "NOTE-3",
            Place::stage_of("validate"),
            Status::CheckerNeedsYou,
            0,
        ),
        t(
            "NOTE-4",
            Place::stage_of("dev"),
            Status::MergeConflictFixing,
            0,
        ),
        t(
            "NOTE-5",
            Place::stage_of("review"),
            Status::WaitingForYou,
            0,
        ),
        t("NOTE-6", Place::stage_of("qa"), Status::WaitingForYou, 0),
        t("NOTE-7", before("dev"), Status::WaitingForSlot, 0),
        t("NOTE-8", before("dev"), Status::Held, 0),
        t("NOTE-9", Place::Waiting(None), Status::Queued, 0),
        t("NOTE-10", Place::stage_of("ship"), Status::ToDeliver, 0),
    ];
    assert_eq!(running_count(&tickets, &HashSet::new()), 4);
    for status in [
        Status::Queued,
        Status::WaitingForSlot,
        Status::Held,
        Status::WaitingForYou,
    ] {
        assert!(!HOLDS_A_SLOT.contains(&status), "{status}");
    }
    // One slot per ticket, however it holds it.
    assert_eq!(
        running_count(
            &tickets,
            &HashSet::from(["NOTE-2".to_owned(), "NOTE-5".to_owned()])
        ),
        5
    );
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

async fn set(f: &Fixture, ticket: &Ticket, stage: Option<&str>, status: Status) {
    sqlx::query("UPDATE tickets SET stage_id = ?, state = ? WHERE id = ?")
        .bind(stage)
        .bind(status.as_str())
        .bind(&ticket.id)
        .execute(f.board.store().pool())
        .await
        .unwrap();
}

async fn one_slot_on(board: &otaq_core::board::Board) {
    board
        .set_global_settings(&GlobalSettings {
            agent_limits: one_slot().limits,
            ..GlobalSettings::default()
        })
        .await
        .unwrap();
}

/// A PR Delivery session holds a slot (from the Open PR click until the PR is reported), though its card waits in
/// Review; the same card waiting in Review without one doesn't.
#[tokio::test]
async fn a_pr_delivery_session_holds_a_slot() {
    let f = Fixture::shop().await;
    let recorder = Arc::new(Recorder::default());
    let scheduler = Scheduler::new(
        f.board.clone(),
        recorder.clone(),
        EventBus::new(16),
        Duration::from_secs(3600),
    );
    one_slot_on(&f.board).await;
    let review = f.create("Checkout").await;
    let queued = f.create("Coupons").await;
    set(&f, &review, Some("review"), Status::WaitingForYou).await;
    f.board
        .move_ticket(
            &queued.key.to_string(),
            DropTarget::ReadyForDev,
            false,
            &crate::common::me(),
        )
        .await
        .unwrap();
    assert_eq!(
        scheduler.status().await.unwrap().holding,
        0,
        "waiting in Review"
    );

    sqlx::query(
        "INSERT INTO pr_flows (ticket_id, kind, state, started_at, updated_at) VALUES (?, 'open', 'preparing', 0, 0)",
    )
    .bind(&review.id)
    .execute(f.board.store().pool())
    .await
    .unwrap();
    assert_eq!(scheduler.status().await.unwrap().holding, 1);
    scheduler.pass().await.unwrap();
    assert!(
        recorder.0.lock().unwrap().is_empty(),
        "the PR session holds the one slot"
    );
    assert_eq!(f.ticket("SHOP-2").await.status, Status::WaitingForSlot);

    sqlx::query("DELETE FROM pr_flows WHERE ticket_id = ?")
        .bind(&review.id)
        .execute(f.board.store().pool())
        .await
        .unwrap();
    scheduler.pass().await.unwrap();
    assert_eq!(*recorder.0.lock().unwrap(), ["SHOP-2"]);
}

// ---- On the board ----

/// An ordinary command standing in for `claude`.
fn stand_in() -> LaunchContext {
    LaunchContext {
        bin_dir: "/nonexistent".into(),
        plugin_dir: "/nonexistent/plugin".into(),
        path: std::env::var_os("PATH").unwrap_or_default(),
        claude: ClaudeLaunch {
            program: vec![
                "sh".into(),
                "-c".into(),
                "echo started; exec cat".into(),
                "sh".into(),
            ],
            config_dir: None,
        },
    }
}

async fn until(w: &TestService, key: &str, status: Status) -> Ticket {
    for _ in 0..500 {
        let ticket = w.board.ticket(key).await.unwrap();
        if ticket.status == status {
            return ticket;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("{key} never became {status}");
}

/// pocket-notes on the test flow, with one slot: NOTE-1 runs Grill; while NOTE-2 holds the slot, NOTE-1's submit
/// waits before Dev ("Waiting for a slot", in its worktree); once the slot frees it starts Dev there, in a new
/// session (Grill's stopped), and Grill's run is done.
#[tokio::test(flavor = "multi_thread")]
async fn a_ticket_finishing_grill_waits_for_a_slot_before_dev() {
    let w = TestService::start_with(|config| ServiceConfig {
        launch: Some(stand_in()),
        ..config
    })
    .await;
    let repo = w.repos.repo("pocket-notes", 1);
    w.board
        .add_project(&otaq_core::board::projects::AddProject {
            path: repo.display().to_string(),
            key: Some("NOTE".into()),
            ..Default::default()
        })
        .await
        .unwrap();
    w.board.use_flow_for_tests("NOTE", &every_kind_file()).await;
    one_slot_on(&w.board).await;
    let grilled = w.create("NOTE", "Export as CSV").await;
    let other = w.create("NOTE", "Search notes").await;
    let me = Actor::User("me".into());
    w.board
        .move_ticket("NOTE-1", DropTarget::ReadyForDev, true, &me)
        .await
        .unwrap();
    w.starter
        .start("NOTE-1", &me, otaq_core::session::Start::Auto)
        .await
        .unwrap();
    let in_grill = until(&w, "NOTE-1", Status::Working).await;
    assert_eq!(in_grill.place, Place::stage_of("grill"));
    let grill_session = w
        .sessions
        .running_of(&grilled.id, SessionKind::Agent)
        .expect("Grill's agent runs");
    let worktree = in_grill.work.as_ref().unwrap().worktree.clone().unwrap();

    // NOTE-2 takes the only slot.
    w.force_status(&other, Status::Working).await;
    commit(Path::new(&worktree), "questions.md", "agreed\n");
    let submitted = w
        .validation
        .submit(
            "NOTE-1",
            said(&w.board, "NOTE-1", "Agreed: CSV with a header row.")
                .await
                .as_ref(),
            &Actor::Agent(grilled.key.clone()),
            ValidationTrigger::Agent,
        )
        .await
        .unwrap();
    assert_eq!(submitted.place, before("dev"));
    let scheduler = Scheduler::new(
        w.board.clone(),
        Arc::new(w.starter.clone()),
        EventBus::new(16),
        Duration::from_secs(3600),
    );
    scheduler.pass().await.unwrap();
    let waiting = until(&w, "NOTE-1", Status::WaitingForSlot).await;
    assert_eq!(waiting.place, before("dev"));
    assert_eq!(
        waiting.work.as_ref().unwrap().worktree.as_deref(),
        Some(worktree.as_str()),
        "it keeps its worktree"
    );

    // The slot frees.
    w.force_status(&other, Status::WaitingForYou).await;
    scheduler.pass().await.unwrap();
    let in_dev = until(&w, "NOTE-1", Status::Working).await;
    assert_eq!(in_dev.place, Place::stage_of("dev"));
    assert_eq!(
        in_dev.work.as_ref().unwrap().worktree.as_deref(),
        Some(worktree.as_str())
    );
    let dev_session = w
        .sessions
        .running_of(&grilled.id, SessionKind::Agent)
        .expect("Dev's agent runs");
    assert_ne!(
        dev_session.id, grill_session.id,
        "a new conversation for Dev"
    );
    assert!(
        !w.sessions.info(&grill_session.id).unwrap().is_running(),
        "Grill's session was stopped"
    );
    let runs: Vec<_> = w
        .board
        .stage_runs("NOTE-1")
        .await
        .unwrap()
        .into_iter()
        .map(|r| (r.stage, r.session, r.result))
        .collect();
    assert_eq!(
        runs,
        [
            (
                "grill".to_owned(),
                Some(grill_session.id.clone()),
                Some(RunResult::Done)
            ),
            ("dev".to_owned(), Some(dev_session.id.clone()), None),
        ]
    );
    w.stop().await;
}
