//! Optional limits (SCH-03; d0016, d0042; T-0080): counting rules, max_concurrency, back-pressure, and that a
//! limit only holds back starts.

use crate::common;

use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::common::{Fixture, me};
use otaq_core::board::moves::DropTarget;
use otaq_core::board::settings::{AgentLimits, GlobalSettings};
use otaq_core::board::tickets::{Base, NewTicketInput, Ticket};
use otaq_core::domain::{Actor, Flags, Status, TicketKey};
use otaq_core::scheduler::{
    Gate, HOLDS_A_SLOT, Scheduler, StartTickets, ToStart, plan, running_count,
};
use otaq_core::service::EventBus;

fn t(key: &str, status: Status, queued_at: i64) -> Ticket {
    Ticket {
        id: key.into(),
        key: key.parse::<TicketKey>().unwrap(),
        title: key.into(),
        body: String::new(),
        priority: Default::default(),
        place: otaq_core::domain::Place::of(status, None, None),
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

fn on(max: u32, back_pressure: u32, needs_you: usize) -> Gate {
    Gate {
        limits: AgentLimits {
            enabled: true,
            max_concurrency: max,
            back_pressure,
        },
        needs_you,
    }
}

fn started(tickets: &[Ticket], gate: &Gate) -> Vec<String> {
    plan(tickets, &HashSet::new(), gate)
        .start
        .iter()
        .map(|s| s.key.to_string())
        .collect()
}

#[test]
fn needs_you_and_background_count_review_and_parked_do_not() {
    let tickets = [
        t("SHOP-1", Status::Working, 0),
        t("SHOP-2", Status::WorkingInBackground, 0),
        t("SHOP-3", Status::AsksYou, 0),
        t("SHOP-4", Status::NeedsYou, 0),
        t("SHOP-5", Status::Idle, 0),
        t("SHOP-6", Status::WaitingForYou, 0),
        t("SHOP-7", Status::Parked, 0),
        t("SHOP-8", Status::PrOpen, 0),
        t("SHOP-9", Status::Interrupted, 0),
        t("SHOP-10", Status::Queued, 0),
        t("SHOP-11", Status::WaitingForSlot, 0),
        t("BILL-1", Status::Working, 0),
    ];
    assert_eq!(running_count(&tickets, &HashSet::new()), 6);
    // A ticket whose start runs holds a slot too.
    assert_eq!(
        running_count(&tickets, &HashSet::from(["SHOP-10".to_owned()])),
        7
    );
    assert!(HOLDS_A_SLOT.contains(&Status::WorkingInBackground));
    assert!(!HOLDS_A_SLOT.contains(&Status::WaitingForYou));
}

#[test]
fn with_limits_on_at_most_max_minus_running_start_oldest_first() {
    let tickets = [
        t("SHOP-1", Status::Working, 0),
        t("SHOP-2", Status::Idle, 0),
        t("SHOP-3", Status::WorkingInBackground, 0),
        t("SHOP-4", Status::Queued, 40),
        t("SHOP-5", Status::Queued, 10),
        t("SHOP-6", Status::Queued, 30),
        t("BILL-1", Status::Queued, 20),
    ];
    let gate = on(5, 3, 0);
    let pass = plan(&tickets, &HashSet::new(), &gate);
    let keys: Vec<String> = pass.start.iter().map(|s| s.key.to_string()).collect();
    assert_eq!(keys, ["SHOP-5", "BILL-1"], "5 − 3 running = 2 slots");
    let waiting: Vec<(String, Status)> = pass
        .changes
        .iter()
        .map(|c| (c.key.to_string(), c.to))
        .collect();
    assert_eq!(
        waiting,
        [
            ("SHOP-6".into(), Status::WaitingForSlot),
            ("SHOP-4".into(), Status::WaitingForSlot),
        ]
    );
    // Limits off (the default): every ready ticket starts.
    assert_eq!(started(&tickets, &Gate::default()).len(), 4);
}

#[test]
fn a_waiting_ticket_starts_when_a_slot_frees_up() {
    let mut tickets = vec![
        t("SHOP-1", Status::Working, 0),
        t("SHOP-2", Status::WaitingForSlot, 10),
        t("SHOP-3", Status::WaitingForSlot, 5),
    ];
    assert!(started(&tickets, &on(1, 3, 0)).is_empty());
    tickets[0].status = Status::WaitingForYou;
    assert_eq!(started(&tickets, &on(1, 3, 0)), ["SHOP-3"]);
}

#[test]
fn back_pressure_stops_new_starts_until_items_clear() {
    let tickets = [
        t("SHOP-1", Status::Queued, 1),
        t("SHOP-2", Status::Queued, 2),
    ];
    let pass = plan(&tickets, &HashSet::new(), &on(5, 3, 3));
    assert!(pass.start.is_empty());
    assert!(pass.changes.iter().all(|c| c.to == Status::WaitingForSlot));
    assert_eq!(started(&tickets, &on(5, 3, 2)), ["SHOP-1", "SHOP-2"]);
}

#[test]
fn nothing_running_is_touched_by_a_limit() {
    let tickets: Vec<Ticket> = HOLDS_A_SLOT
        .iter()
        .chain([Status::WaitingForYou, Status::Interrupted].iter())
        .enumerate()
        .map(|(i, s)| t(&format!("SHOP-{}", i + 1), *s, 0))
        .collect();
    // Far over the limit, and back-pressure reached: no change to any of them.
    let pass = plan(&tickets, &HashSet::new(), &on(1, 1, 9));
    assert!(pass.changes.is_empty() && pass.start.is_empty());
}

#[test]
fn flagged_zero_values_act_as_one() {
    let tickets = [t("SHOP-1", Status::Queued, 1)];
    assert_eq!(started(&tickets, &on(0, 0, 0)), ["SHOP-1"]);
    assert!(started(&tickets, &on(0, 0, 1)).is_empty());
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

async fn queued(f: &Fixture, title: &str) -> Ticket {
    let t = f
        .create_with(NewTicketInput {
            title: title.into(),
            ..common::shop_input()
        })
        .await;
    f.board
        .move_ticket(&t.key.to_string(), DropTarget::ReadyForDev, false, &me())
        .await
        .unwrap()
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

async fn limits(f: &Fixture, limits: AgentLimits) {
    f.board
        .set_global_settings(&GlobalSettings {
            agent_limits: limits,
            ..GlobalSettings::default()
        })
        .await
        .unwrap();
}

#[tokio::test]
async fn the_scheduler_reads_the_settings_and_counts_needs_you_across_projects() {
    let f = Fixture::shop().await;
    f.add_project("billing-api").await;
    let recorder = Arc::new(Recorder::default());
    let scheduler = Scheduler::new(
        f.board.clone(),
        recorder.clone(),
        EventBus::new(16),
        Duration::from_secs(3600),
    );
    // Defaults: off / 5 / 3.
    assert_eq!(
        f.board.global_settings().await.unwrap().agent_limits,
        AgentLimits {
            enabled: false,
            max_concurrency: 5,
            back_pressure: 3
        }
    );
    queued(&f, "a").await;
    queued(&f, "b").await;
    limits(
        &f,
        AgentLimits {
            enabled: true,
            max_concurrency: 1,
            back_pressure: 3,
        },
    )
    .await;
    scheduler.pass().await.unwrap();
    assert_eq!(*recorder.0.lock().unwrap(), ["SHOP-1"]);
    assert_eq!(f.ticket("SHOP-2").await.status, Status::WaitingForSlot);
    set(&f, "SHOP-1", Status::Working).await;

    // Back-pressure: one item waits in billing-api; with the threshold at 1, nothing new starts.
    let bill = f
        .create_with(NewTicketInput {
            project: "BILL".into(),
            title: "VAT rounding fix".into(),
            ..Default::default()
        })
        .await;
    set(&f, &bill.key.to_string(), Status::WaitingForYou).await;
    limits(
        &f,
        AgentLimits {
            enabled: true,
            max_concurrency: 5,
            back_pressure: 1,
        },
    )
    .await;
    scheduler.pass().await.unwrap();
    assert_eq!(recorder.0.lock().unwrap().len(), 1);
    assert_eq!(
        f.ticket("SHOP-1").await.status,
        Status::Working,
        "never stopped"
    );
    let status = scheduler.status().await.unwrap();
    assert_eq!(
        (
            status.holding,
            status.waiting_for_slot,
            status.needs_you,
            status.starts_paused
        ),
        (1, 1, 1, true)
    );

    // The item is cleared: starts resume.
    set(&f, &bill.key.to_string(), Status::Merged).await;
    scheduler.pass().await.unwrap();
    assert_eq!(*recorder.0.lock().unwrap(), ["SHOP-1", "SHOP-2"]);

    // Limits off again: nothing waits for a slot.
    limits(&f, AgentLimits::default()).await;
    queued(&f, "c").await;
    scheduler.pass().await.unwrap();
    assert_eq!(recorder.0.lock().unwrap().last().unwrap(), "SHOP-3");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_settings_change_over_the_socket_reaches_the_scheduler_at_once() {
    use otaq_core::client::{Client, ClientConfig};
    use otaq_core::protocol::{Reply, Request};
    use otaq_core::service::testing::TestService;

    let w = TestService::start().await;
    w.add_project("shop-web").await;
    let client = Client::connect(ClientConfig::new(&w.data)).await.unwrap();
    let mut settings = GlobalSettings {
        agent_limits: AgentLimits {
            enabled: true,
            max_concurrency: 1,
            back_pressure: 3,
        },
        ..GlobalSettings::default()
    };
    client
        .request(Request::SettingsSet(settings.clone()))
        .await
        .unwrap();
    let running = w.create("SHOP", "running").await;
    w.force_status(&running, Status::Working).await;
    let queued = w
        .board
        .create_ticket(
            &NewTicketInput {
                project: "SHOP".into(),
                title: "waits".into(),
                ..Default::default()
            },
            &me(),
        )
        .await
        .unwrap();
    client
        .request(Request::TicketMove(otaq_core::protocol::MoveParams {
            key: queued.key.to_string(),
            target: DropTarget::ReadyForDev,
            confirmed: false,
        }))
        .await
        .unwrap();
    wait_for(&w, &queued.key.to_string(), Status::WaitingForSlot).await;
    let Reply::SchedulerStatus(status) = client.request(Request::SchedulerStatus).await.unwrap()
    else {
        panic!()
    };
    assert_eq!((status.holding, status.waiting_for_slot), (1, 1));

    // Two agents allowed now: it starts (and, with no claude in tests, stops at the launch step). The running
    // ticket is untouched.
    settings.agent_limits.max_concurrency = 2;
    client
        .request(Request::SettingsSet(settings))
        .await
        .unwrap();
    wait_for(&w, &queued.key.to_string(), Status::StartInterrupted).await;
    assert_eq!(w.status(&running.key.to_string()).await, Status::Working);
    drop(client);
    w.stop().await;
}

async fn wait_for(w: &otaq_core::service::testing::TestService, key: &str, status: Status) {
    tokio::time::timeout(Duration::from_secs(20), async {
        while w.status(key).await != status {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("{key} never reached {status}"));
}
