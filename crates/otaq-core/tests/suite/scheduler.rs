//! The scheduler (SCH-01, SCH-02; T-0079): pure pass decisions, and the loop with an injected start function:
//! the storyboard chain, oldest first, event and tick triggers, overlapping passes, and what it never touches.

use crate::common;

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::common::{Fixture, me};
use otaq_core::board::moves::DropTarget;
use otaq_core::board::tickets::{Base, NewTicketInput, StartProblem, Ticket};
use otaq_core::domain::{Actor, Flags, Kind, Priority, StartStep, Status, TicketKey};
use otaq_core::epic_lead::wake::LeadWait;
use otaq_core::protocol::Event;
use otaq_core::scheduler::{
    Gate, Holds, Plan, Scheduler, StartTickets, StatusChange, ToStart, plan, plan_holding,
    plan_with, stock_limit, waiting_for_lead,
};
use otaq_core::service::EventBus;
use otaq_core::start::Starter;
use tokio::sync::watch;

fn t(key: &str, status: Status, queued_at: i64) -> Ticket {
    Ticket {
        id: key.into(),
        key: key.parse::<TicketKey>().unwrap(),
        kind: Kind::Ticket,
        title: key.into(),
        body: String::new(),
        priority: Default::default(),
        priority_inherited: false,
        epic: None,
        place: otaq_core::domain::Place::of(status, None, None),
        status,
        base: Base::Branch("main".into()),
        base_locked: false,
        external_id: None,
        depends_on: Vec::new(),
        aliases: Vec::new(),
        next_child_number: None,
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
        epic_queue_mode: Default::default(),
        joined_queue_by: None,
    }
}

fn deps(mut ticket: Ticket, parents: &[&str]) -> Ticket {
    ticket.depends_on = parents.iter().map(|p| p.parse().unwrap()).collect();
    ticket
}

fn keys(list: &[ToStart]) -> Vec<String> {
    list.iter().map(|t| t.key.to_string()).collect()
}

fn held(key: &str, step: StartStep) -> Ticket {
    let mut ticket = t(key, Status::Held, 0);
    ticket.start_problem = Some(StartProblem {
        step,
        message: "Claude Code limit reached".into(),
    });
    ticket
}

/// The stock limit (M19-a, plan §4.2): only tickets that wait for a slot or their tool's limit count.
#[test]
fn stock_limit_counts_waiting_for_slot_and_limit_held_only() {
    let board = [
        t("SHOP-1", Status::WaitingForSlot, 0),
        t("SHOP-2", Status::WaitingForSlot, 0),
        held("SHOP-3", StartStep::ToolLimit),
        t("SHOP-4", Status::Queued, 0),
        t("SHOP-5", Status::Working, 0),
        t("SHOP-6", Status::Backlog, 0),
        t("SHOP-7", Status::WaitingForYou, 0),
        t("SHOP-8", Status::StartInterrupted, 0),
    ];
    assert_eq!(stock_limit(&board, &board), 3);
    // Only the repository's tickets count; the rest are there for readiness.
    assert_eq!(stock_limit(&board[..1], &board), 1);
    assert_eq!(stock_limit(&board[1..3], &board), 2);
}

#[test]
fn stock_limit_leaves_out_tickets_waiting_for_dependencies() {
    let board = [
        t("SHOP-1", Status::Working, 0),
        // A child of an unmerged parent: back in Queued, and a slot-waiter whose parent isn't merged either.
        deps(t("SHOP-2", Status::Queued, 0), &["SHOP-1"]),
        deps(t("SHOP-3", Status::WaitingForSlot, 0), &["SHOP-1"]),
        t("SHOP-4", Status::ClosedNotMerged, 0),
        deps(t("SHOP-5", Status::ParentClosed, 0), &["SHOP-4"]),
        deps(t("SHOP-6", Status::WaitingForSlot, 0), &["SHOP-4"]),
        t("SHOP-7", Status::Merged, 0),
        deps(t("SHOP-8", Status::WaitingForSlot, 0), &["SHOP-7"]),
        deps(t("SHOP-9", Status::WaitingForSlot, 0), &["SHOP-7"]),
    ];
    // Only SHOP-8 and SHOP-9: their parent is merged.
    assert_eq!(stock_limit(&board, &board), 2);
}

#[test]
fn stock_limit_is_one_when_nothing_waits() {
    assert_eq!(stock_limit(&[], &[]), 1);
    let board = [
        t("SHOP-1", Status::Working, 0),
        t("SHOP-2", Status::Queued, 0),
        t("SHOP-3", Status::Backlog, 0),
    ];
    assert_eq!(stock_limit(&board, &board), 1);
}

#[test]
fn stock_limit_ignores_held_for_a_diverged_base() {
    let board = [
        held("SHOP-1", StartStep::SyncBase),
        t("SHOP-2", Status::Held, 0),
        held("SHOP-3", StartStep::ToolLimit),
        held("SHOP-4", StartStep::ToolLimit),
    ];
    assert_eq!(stock_limit(&board, &board), 2);
    assert_eq!(stock_limit(&board[..2], &board), 1);
}

#[test]
fn ready_tickets_start_oldest_first_by_queue_time() {
    let board = [
        t("SHOP-1", Status::Queued, 30),
        t("SHOP-2", Status::Queued, 10),
        t("BILL-1", Status::Queued, 20),
    ];
    let pass = plan(&board, &HashSet::new(), &Gate::default());
    assert_eq!(keys(&pass.start), ["SHOP-2", "BILL-1", "SHOP-1"]);
    assert!(pass.changes.is_empty());
}

/// The start order (SCH-01, d0065): a pinned P3 starts before an unpinned P1; among equal pins and priorities,
/// the one ready the longest starts first, and a ticket whose parent merged later counts from that merge.
#[test]
fn start_order_pins_then_priority_then_ready_the_longest() {
    let mut pinned = t("SHOP-1", Status::Queued, 50);
    pinned.flags.pinned_at = Some(70);
    let mut urgent = t("SHOP-2", Status::Queued, 40);
    urgent.priority = Priority::Urgent;
    let older = t("SHOP-3", Status::Queued, 10);
    // Queued at 5, but its parent merged at 30: ready since 30.
    let mut parent = t("SHOP-9", Status::Merged, 0);
    parent.status_since = Some(30);
    let late = deps(t("SHOP-4", Status::Queued, 5), &["SHOP-9"]);
    let newer = t("SHOP-5", Status::Queued, 20);
    let board = [pinned, urgent, older, parent, late, newer];
    let pass = plan(&board, &HashSet::new(), &Gate::default());
    assert_eq!(
        keys(&pass.start),
        ["SHOP-1", "SHOP-2", "SHOP-3", "SHOP-5", "SHOP-4"]
    );
    // Each ready ticket gets its ready time stored.
    let mut ready: Vec<(String, Option<i64>)> = pass.ready_since.clone();
    ready.sort();
    assert_eq!(
        ready,
        [
            ("SHOP-1".into(), Some(50)),
            ("SHOP-2".into(), Some(40)),
            ("SHOP-3".into(), Some(10)),
            ("SHOP-4".into(), Some(30)),
            ("SHOP-5".into(), Some(20)),
        ]
    );
    // A stored time wins over the derived one, and a waiting ticket loses it.
    let mut stored = t("SHOP-6", Status::Queued, 90);
    stored.ready_since = Some(1);
    let mut waiting = deps(t("SHOP-7", Status::Queued, 1), &["SHOP-8"]);
    waiting.ready_since = Some(2);
    let board = [
        t("SHOP-3", Status::Queued, 10),
        stored,
        waiting,
        t("SHOP-8", Status::Working, 0),
    ];
    let pass = plan(&board, &HashSet::new(), &Gate::default());
    assert_eq!(keys(&pass.start), ["SHOP-6", "SHOP-3"]);
    assert!(pass.ready_since.contains(&("SHOP-7".into(), None)));
}

/// EPC-17 (d0080): a pinned epic's children start first, as if each were pinned. SHOP-40 pinned, SHOP-51 not,
/// both P3 and SHOP-51 queued earlier: SHOP-40.1 starts first; SHOP-51 pinned later than SHOP-40: SHOP-51 first.
#[test]
fn a_pinned_epics_children_start_first() {
    let mut epic = t("SHOP-40", Status::Backlog, 0);
    epic.kind = Kind::Epic;
    epic.flags.pinned_at = Some(100);
    let mut child = t("SHOP-40.1", Status::Queued, 50);
    child.epic = Some(epic.key.clone());
    let shop51 = t("SHOP-51", Status::Queued, 10);
    let mut board = [epic, child, shop51];
    let pass = plan(&board, &HashSet::new(), &Gate::default());
    assert_eq!(keys(&pass.start), ["SHOP-40.1", "SHOP-51"]);
    board[2].flags.pinned_at = Some(200);
    let pass = plan(&board, &HashSet::new(), &Gate::default());
    assert_eq!(keys(&pass.start), ["SHOP-51", "SHOP-40.1"]);
}

#[test]
fn the_scheduler_never_queues_retries_or_touches_other_states() {
    let mut board: Vec<Ticket> = [
        Status::Backlog,
        Status::Parked,
        Status::Held,
        Status::StartInterrupted,
        Status::Working,
        Status::Idle,
        Status::WaitingForYou,
        Status::PrOpen,
        Status::Merged,
        Status::ClosedNotMerged,
    ]
    .iter()
    .enumerate()
    .map(|(i, s)| t(&format!("SHOP-{}", i + 1), *s, 0))
    .collect();
    assert_eq!(
        plan(&board, &HashSet::new(), &Gate::default()),
        Plan::default()
    );
    // Even with every dependency merged.
    for ticket in &mut board {
        ticket.depends_on.clear();
    }
    assert_eq!(
        plan(&board, &HashSet::new(), &Gate::default()),
        Plan::default()
    );
}

/// PAU-04 (d0112): a paused project starts nothing, from Ready for Dev or between two Agent stages; a waiting flow
/// change holds only Ready for Dev; other projects start.
#[test]
fn a_paused_project_starts_nothing_and_a_waiting_flow_change_holds_only_ready_for_dev() {
    let mut between = t("SHOP-2", Status::Queued, 2);
    between.place = otaq_core::domain::Place::Waiting(Some("dev".into()));
    let mut bill_between = t("BILL-2", Status::Queued, 4);
    bill_between.place = otaq_core::domain::Place::Waiting(Some("dev".into()));
    let board = [
        t("SHOP-1", Status::Queued, 1),
        between,
        t("BILL-1", Status::Queued, 3),
        bill_between,
    ];
    let none = HashSet::new();
    let no_holds = HashMap::new();
    let shop = HashSet::from(["SHOP".to_owned()]);
    let bill = HashMap::from([(
        "BILL".to_owned(),
        otaq_core::flow::apply::Hold::READY_FOR_DEV,
    )]);
    let paused = plan_holding(&board, &none, &Gate::default(), &no_holds, &shop);
    assert_eq!(keys(&paused.start), ["BILL-1", "BILL-2"]);
    assert!(paused.changes.is_empty(), "they wait as they are");
    let flow_change = plan_holding(&board, &none, &Gate::default(), &bill, &none);
    assert_eq!(keys(&flow_change.start), ["SHOP-1", "SHOP-2", "BILL-2"]);
}

/// d0124: with a live lead, a child that a sibling finished since the lead's session started waits for the lead's
/// turn about it; a sibling outside the epic, or one that finished before the lead started, holds nothing; a turn
/// that ended releases it until the sibling finishes again; `freed` holds a child whose link the lead removed.
#[test]
fn freed_children_wait_for_the_leads_turn() {
    let child = |key: &str, status: Status, at: i64| {
        let mut c = t(key, status, 0);
        c.epic = Some("SHOP-40".parse().unwrap());
        c.status_since = Some(at);
        c
    };
    let api = child("SHOP-40.1", Status::Merged, 200);
    let mut other = t("SHOP-7", Status::Merged, 0);
    other.status_since = Some(200);
    let old = child("SHOP-40.2", Status::Merged, 50);
    let page = deps(child("SHOP-40.3", Status::Queued, 0), &["SHOP-40.1"]);
    let outside = deps(
        child("SHOP-40.4", Status::Queued, 0),
        &["SHOP-7", "SHOP-40.2"],
    );
    let unlinked = child("SHOP-40.5", Status::Queued, 0);
    let board = [api, other, old, page, outside, unlinked];
    let mut wait = LeadWait {
        epic: "SHOP-40".parse().unwrap(),
        since: 100,
        released: HashMap::new(),
        freed: HashSet::from(["SHOP-40.5".to_owned()]),
    };
    let ids = |w: &LeadWait| {
        let mut ids: Vec<String> = waiting_for_lead(&board, std::slice::from_ref(w))
            .into_iter()
            .collect();
        ids.sort();
        ids
    };
    assert_eq!(ids(&wait), ["SHOP-40.3", "SHOP-40.5"]);
    assert!(waiting_for_lead(&board, &[]).is_empty(), "no live lead");

    // They don't start, and wait in Queued.
    let lead: HashSet<String> = waiting_for_lead(&board, std::slice::from_ref(&wait));
    let none = HashSet::new();
    let pass = plan_with(
        &board,
        &none,
        &Gate::default(),
        &Holds {
            flow: &HashMap::new(),
            paused: &none,
            lead: &lead,
        },
    );
    assert_eq!(keys(&pass.start), ["SHOP-40.4"]);

    // The turn about 40.1 ended; the lead's turn about the close of 40.5's old parent too.
    wait.released.insert("SHOP-40.1".into(), 300);
    wait.freed.clear();
    assert!(ids(&wait).is_empty());
    // 40.1 finished again (reopened, merged) after that: 40.3 waits again.
    wait.released.insert("SHOP-40.1".into(), 150);
    assert_eq!(ids(&wait), ["SHOP-40.3"]);
}

#[test]
fn starting_tickets_are_skipped() {
    let board = [
        t("SHOP-1", Status::Queued, 1),
        t("SHOP-2", Status::Queued, 2),
    ];
    let pass = plan(
        &board,
        &HashSet::from(["SHOP-1".to_owned()]),
        &Gate::default(),
    );
    assert_eq!(keys(&pass.start), ["SHOP-2"]);
}

#[test]
fn a_closed_parent_moves_the_child_to_parent_closed_and_back() {
    let board = [
        t("SHOP-19", Status::ClosedNotMerged, 0),
        deps(t("SHOP-20", Status::Queued, 1), &["SHOP-19"]),
    ];
    let pass = plan(&board, &HashSet::new(), &Gate::default());
    assert!(pass.start.is_empty());
    assert_eq!(
        pass.changes,
        [StatusChange {
            id: "SHOP-20".into(),
            key: "SHOP-20".parse().unwrap(),
            from: Status::Queued,
            to: Status::ParentClosed,
        }]
    );
    // The parent is reopened: SHOP-20 waits again.
    let board = [
        t("SHOP-19", Status::Backlog, 0),
        deps(t("SHOP-20", Status::ParentClosed, 1), &["SHOP-19"]),
    ];
    let pass = plan(&board, &HashSet::new(), &Gate::default());
    assert_eq!(pass.changes[0].to, Status::Queued);
    assert!(pass.start.is_empty());
    // The dependency is removed instead: back to Queued and started.
    let board = [
        t("SHOP-19", Status::ClosedNotMerged, 0),
        t("SHOP-20", Status::ParentClosed, 1),
    ];
    let pass = plan(&board, &HashSet::new(), &Gate::default());
    assert_eq!(pass.changes[0].to, Status::Queued);
    assert_eq!(keys(&pass.start), ["SHOP-20"]);
}

/// A start function that records what it was asked to start and keeps those tickets "starting" for good.
#[derive(Default)]
struct Recorder {
    started: Mutex<Vec<String>>,
    starting: Mutex<HashSet<String>>,
}

impl Recorder {
    fn started(&self) -> Vec<String> {
        self.started.lock().unwrap().clone()
    }
}

impl StartTickets for Recorder {
    fn spawn_start(&self, ticket: &ToStart) {
        self.started.lock().unwrap().push(ticket.key.to_string());
        self.starting.lock().unwrap().insert(ticket.id.clone());
    }

    fn starting(&self) -> HashSet<String> {
        self.starting.lock().unwrap().clone()
    }
}

struct World {
    f: Fixture,
    recorder: Arc<Recorder>,
    events: EventBus,
    scheduler: Scheduler,
}

async fn world(tick: Duration) -> World {
    let f = Fixture::shop().await;
    let recorder = Arc::new(Recorder::default());
    let events = EventBus::new(64);
    let scheduler = Scheduler::new(f.board.clone(), recorder.clone(), events.clone(), tick);
    World {
        f,
        recorder,
        events,
        scheduler,
    }
}

async fn create(f: &Fixture, title: &str, parents: &[&str]) -> Ticket {
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

async fn queue(f: &Fixture, key: &str) {
    f.board
        .move_ticket(key, DropTarget::ReadyForDev, false, &me())
        .await
        .unwrap();
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

#[tokio::test]
async fn storyboard_chain_shop_20_starts_only_after_both_parents_merged() {
    let w = world(Duration::from_secs(3600)).await;
    let f = &w.f;
    // SHOP-1 stands in for SHOP-17 (merged), SHOP-2 for SHOP-18, SHOP-3 for SHOP-19, SHOP-4 for SHOP-20.
    create(f, "Add Stripe client", &[]).await;
    set(f, "SHOP-1", Status::Merged).await;
    create(f, "Checkout API", &["SHOP-1"]).await;
    create(f, "Price table", &[]).await;
    create(f, "Checkout page", &["SHOP-2", "SHOP-3"]).await;
    for key in ["SHOP-2", "SHOP-3", "SHOP-4"] {
        queue(f, key).await;
    }

    // 2 · the chain runs: SHOP-2 and SHOP-3 start in parallel, SHOP-4 waits.
    w.scheduler.pass().await.unwrap();
    assert_eq!(w.recorder.started(), ["SHOP-2", "SHOP-3"]);
    set(f, "SHOP-2", Status::WaitingForYou).await;
    set(f, "SHOP-3", Status::Working).await;
    w.scheduler.pass().await.unwrap();
    assert_eq!(w.recorder.started().len(), 2);

    // 3 · one parent merged: still waiting.
    set(f, "SHOP-2", Status::Merged).await;
    w.scheduler.pass().await.unwrap();
    assert_eq!(w.recorder.started().len(), 2);
    assert_eq!(f.ticket("SHOP-4").await.status, Status::Queued);

    // 4 · all parents merged: SHOP-4 starts.
    set(f, "SHOP-3", Status::Merged).await;
    let pass = w.scheduler.pass().await.unwrap();
    assert_eq!(keys(&pass.start), ["SHOP-4"]);
    assert_eq!(w.recorder.started(), ["SHOP-2", "SHOP-3", "SHOP-4"]);
}

#[tokio::test]
async fn oldest_first_by_the_time_they_entered_queued() {
    let w = world(Duration::from_secs(3600)).await;
    let f = &w.f;
    for title in ["a", "b", "c"] {
        create(f, title, &[]).await;
    }
    // Queued in the order 3, 1, 2 (created 1, 2, 3).
    for key in ["SHOP-3", "SHOP-1", "SHOP-2"] {
        queue(f, key).await;
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    w.scheduler.pass().await.unwrap();
    assert_eq!(w.recorder.started(), ["SHOP-3", "SHOP-1", "SHOP-2"]);
}

#[tokio::test]
async fn two_passes_at_once_start_a_ticket_once() {
    let w = world(Duration::from_secs(3600)).await;
    create(&w.f, "a", &[]).await;
    queue(&w.f, "SHOP-1").await;
    let (a, b) = tokio::join!(w.scheduler.pass(), w.scheduler.pass());
    a.unwrap();
    b.unwrap();
    assert_eq!(w.recorder.started(), ["SHOP-1"]);
}

#[tokio::test]
async fn a_closed_parent_is_recorded_and_the_card_says_so() {
    let w = world(Duration::from_secs(3600)).await;
    let f = &w.f;
    create(f, "parent", &[]).await;
    create(f, "child", &["SHOP-1"]).await;
    queue(f, "SHOP-2").await;
    set(f, "SHOP-1", Status::ClosedNotMerged).await;
    w.scheduler.pass().await.unwrap();
    assert_eq!(f.ticket("SHOP-2").await.status, Status::ParentClosed);
    assert!(w.recorder.started().is_empty());
    let history = f.board.history("SHOP-2").await.unwrap();
    assert!(
        history.iter().any(|e| e.payload["to"] == "parent_closed"),
        "{history:?}"
    );
}

async fn wait_until(what: &str, check: impl Fn() -> bool) {
    tokio::time::timeout(Duration::from_secs(5), async {
        while !check() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("{what}"));
}

#[tokio::test]
async fn a_state_change_event_triggers_a_pass() {
    let w = world(Duration::from_secs(3600)).await;
    let (stop, stopped) = watch::channel(false);
    let task = tokio::spawn(w.scheduler.clone().run(stopped));
    let f = &w.f;
    create(f, "parent", &[]).await;
    create(f, "child", &["SHOP-1"]).await;
    queue(f, "SHOP-2").await;
    // The first tick passes at once; SHOP-2 waits for SHOP-1.
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert!(w.recorder.started().is_empty());

    // SHOP-1 merged: one event, one pass.
    set(f, "SHOP-1", Status::Merged).await;
    let parent = f.ticket("SHOP-1").await;
    w.events.publish(Event::CardStatusChanged {
        project: "SHOP".into(),
        id: parent.id.clone(),
        key: parent.key.clone(),
        status: Status::Merged,
    });
    let recorder = w.recorder.clone();
    wait_until("SHOP-2 never started", move || {
        recorder.started() == ["SHOP-2"]
    })
    .await;

    stop.send_replace(true);
    task.await.unwrap();
}

#[tokio::test]
async fn the_slow_tick_catches_a_change_without_an_event() {
    let w = world(Duration::from_millis(100)).await;
    let (stop, stopped) = watch::channel(false);
    let task = tokio::spawn(w.scheduler.clone().run(stopped));
    let f = &w.f;
    create(f, "parent", &[]).await;
    create(f, "child", &["SHOP-1"]).await;
    queue(f, "SHOP-2").await;
    set(f, "SHOP-1", Status::Merged).await; // no event published
    let recorder = w.recorder.clone();
    wait_until("the tick never started SHOP-2", move || {
        recorder.started() == ["SHOP-2"]
    })
    .await;
    stop.send_replace(true);
    task.await.unwrap();
}

#[tokio::test]
async fn the_start_sequence_runs_a_ticket_once_when_called_twice_at_once() {
    let f = Fixture::shop().await;
    let data = otaq_core::data_dir::DataDir::new(f.home.path().join("home"));
    let events = EventBus::new(64);
    let sessions = otaq_core::session::Sessions::new(f.board.store().clone(), events.clone());
    let starter = Starter::new(
        f.board.clone(),
        data.clone(),
        otaq_core::ports::PortPool::new(f.board.store().clone()),
        sessions,
        otaq_core::agent_tools::Adapters::launching_only(None, data),
        events,
    );
    create(&f, "a", &[]).await;
    queue(&f, "SHOP-1").await;
    let actor = me();
    let (a, b) = tokio::join!(
        starter.start("SHOP-1", &actor, otaq_core::session::Start::Auto),
        starter.start("SHOP-1", &actor, otaq_core::session::Start::Auto)
    );
    let outcomes = [a.unwrap(), b.unwrap()];
    let skipped = outcomes
        .iter()
        .filter(|o| matches!(o, otaq_core::start::StartOutcome::Skipped))
        .count();
    assert_eq!(skipped, 1, "{outcomes:?}");
    // Without a launch context the one start stops at its launch step.
    assert_eq!(f.ticket("SHOP-1").await.status, Status::StartInterrupted);
    let started: Vec<(String,)> =
        sqlx::query_as("SELECT kind FROM events WHERE kind = 'start_interrupted'")
            .fetch_all(f.board.store().pool())
            .await
            .unwrap();
    assert_eq!(started.len(), 1);
}

#[tokio::test]
async fn a_start_cut_short_by_the_service_is_shown_not_rerun() {
    let f = Fixture::shop().await;
    create(&f, "a", &[]).await;
    queue(&f, "SHOP-1").await;
    // The service stopped while the start was at its setup step.
    let t = f.ticket("SHOP-1").await;
    sqlx::query("UPDATE tickets SET start_problem_step = 'setup' WHERE id = ?")
        .bind(&t.id)
        .execute(f.board.store().pool())
        .await
        .unwrap();
    assert_eq!(
        f.ticket("SHOP-1").await.start_problem,
        None,
        "a running step isn't a problem"
    );
    assert_eq!(Starter::interrupt_cut_short(&f.board).await.unwrap(), 1);
    let t = f.ticket("SHOP-1").await;
    assert_eq!(t.status, Status::StartInterrupted);
    let problem = t.start_problem.unwrap();
    assert_eq!(problem.step.as_str(), "setup");
    assert!(problem.message.contains("stopped"));
    // The scheduler leaves it alone.
    assert_eq!(
        plan(
            &f.board.tickets("SHOP").await.unwrap(),
            &HashSet::new(),
            &Gate::default()
        ),
        Plan::default()
    );
}
