//! Pinning and column order (TKT-06, SDD §7.5) with the shop-web sample, and the start order of Ready for Dev
//! (SCH-01, TKT-09; d0065, d0066).

use crate::common::{Fixture, me};
use otaq_core::board::order::{blocked_counts, fold_dev, sort_column, start_order};
use otaq_core::board::tickets::{Base, Ticket};
use otaq_core::domain::{Actor, Flags, Priority, Status, StatusSet, TicketKey};

fn t(key: &str, status: Status, created_at: i64) -> Ticket {
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
        created_at,
        updated_at: created_at,
        status_since: None,
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

fn deps(mut ticket: Ticket, parents: &[&str]) -> Ticket {
    ticket.depends_on = parents.iter().map(|p| p.parse().unwrap()).collect();
    ticket
}

fn pinned(mut ticket: Ticket, at: i64) -> Ticket {
    ticket.flags.pinned_at = Some(at);
    ticket
}

fn keys(list: &[TicketKey]) -> Vec<String> {
    list.iter().map(ToString::to_string).collect()
}

fn priority(mut ticket: Ticket, priority: Priority) -> Ticket {
    ticket.priority = priority;
    ticket
}

fn ready_at(mut ticket: Ticket, at: i64) -> Ticket {
    ticket.ready_since = Some(at);
    ticket
}

/// The Dev and Ready for Dev columns of the sample board.
fn sample() -> Vec<Ticket> {
    vec![
        t("SHOP-17", Status::Merged, 1),
        t("SHOP-18", Status::Working, 2),
        pinned(t("SHOP-19", Status::Working, 3), 100),
        deps(t("SHOP-20", Status::Queued, 4), &["SHOP-18", "SHOP-19"]),
        deps(t("SHOP-21", Status::Queued, 5), &["SHOP-20"]),
        t("SHOP-22", Status::NeedsYou, 6),
        t("SHOP-23", Status::AsksYou, 7),
        t("SHOP-24", Status::Backlog, 8),
        t("SHOP-25", Status::WorkingInBackground, 9),
        t("SHOP-26", Status::Interrupted, 10),
        t("SHOP-27", Status::WaitingForLimitReset, 11),
        t("SHOP-28", Status::StartInterrupted, 12),
        t("SHOP-29", Status::Held, 13),
    ]
}

#[test]
fn dev_order() {
    let tickets = sample();
    let order = sort_column(StatusSet::AgentStage, &tickets);
    assert_eq!(
        keys(&order.cards),
        [
            "SHOP-19", // pinned, above turquoise
            "SHOP-22", "SHOP-23", // turquoise
            "SHOP-26", // Interrupted
            "SHOP-18", // working, blocks SHOP-20 and SHOP-21
            "SHOP-25", "SHOP-27", // working, oldest first
        ]
    );
}

/// Ready for Dev: Start interrupted and Parent closed first, then the start order; tickets still waiting for a
/// parent come after the ready ones (they have no ready time yet).
#[test]
fn ready_for_dev_order() {
    let mut tickets = sample();
    tickets.push(t("SHOP-30", Status::ParentClosed, 14));
    assert_eq!(
        keys(&sort_column(StatusSet::Waiting, &tickets).cards),
        ["SHOP-28", "SHOP-30", "SHOP-29", "SHOP-20", "SHOP-21"]
    );
}

#[test]
fn start_order_pins_then_priority_then_ready_the_longest() {
    let tickets = vec![
        ready_at(
            priority(t("SHOP-40", Status::Queued, 1), Priority::Urgent),
            50,
        ),
        ready_at(pinned(t("SHOP-41", Status::Queued, 2), 10), 90),
        ready_at(
            priority(t("SHOP-42", Status::Queued, 3), Priority::High),
            20,
        ),
        ready_at(
            priority(t("SHOP-43", Status::Queued, 4), Priority::High),
            10,
        ),
        ready_at(priority(t("SHOP-44", Status::Queued, 5), Priority::Low), 1),
        ready_at(pinned(t("SHOP-45", Status::Queued, 6), 30), 95),
    ];
    let mut sorted: Vec<&Ticket> = tickets.iter().collect();
    sorted.sort_by_cached_key(|x| start_order(x, &tickets));
    let order: Vec<String> = sorted.iter().map(|x| x.key.to_string()).collect();
    assert_eq!(
        order,
        [
            "SHOP-45", // pinned last: most recently pinned first
            "SHOP-41", // a pinned P3 beats an unpinned P1 (d0065)
            "SHOP-40", // P1
            "SHOP-43", "SHOP-42", // P2: ready the longest first
            "SHOP-44", // P4
        ]
    );
    assert_eq!(
        keys(&sort_column(StatusSet::Waiting, &tickets).cards),
        order,
        "the column shows the start order"
    );
}

#[test]
fn blocks_more_counts_direct_and_indirect_dependents() {
    let counts = blocked_counts(&sample());
    let of = |k: &str| counts[&k.parse::<TicketKey>().unwrap()];
    assert_eq!(
        (of("SHOP-18"), of("SHOP-19"), of("SHOP-20"), of("SHOP-21")),
        (2, 2, 1, 0)
    );
}

#[test]
fn pins_in_every_column_most_recent_first() {
    let tickets = vec![
        t("SHOP-30", Status::WaitingForYou, 1),
        pinned(t("SHOP-31", Status::WaitingForYou, 2), 10),
        pinned(t("SHOP-32", Status::WaitingForYou, 3), 20),
        pinned(t("SHOP-33", Status::Queued, 4), 30),
        t("SHOP-34", Status::Queued, 5),
    ];
    assert_eq!(
        keys(&sort_column(StatusSet::HumanCheck, &tickets).cards),
        ["SHOP-32", "SHOP-31", "SHOP-30"]
    );
    assert!(
        sort_column(StatusSet::AgentStage, &tickets)
            .cards
            .is_empty()
    );
    assert_eq!(
        keys(&sort_column(StatusSet::Waiting, &tickets).cards),
        ["SHOP-33", "SHOP-34"],
        "pins are first in Ready for Dev too"
    );
}

#[test]
fn fold_never_hides_needs_you_cards() {
    let tickets = sample();
    let order = sort_column(StatusSet::AgentStage, &tickets);
    let fold = fold_dev(&order, &tickets, 5);
    // 3 needs-you cards always shown, 2 more slots: SHOP-19 (pinned) and SHOP-18.
    assert_eq!(
        keys(&fold.shown_cards),
        ["SHOP-19", "SHOP-22", "SHOP-23", "SHOP-26", "SHOP-18"]
    );
    assert_eq!(fold.hidden_working, 2);
    assert_eq!(fold.label().as_deref(), Some("2 more · working"));

    let fold = fold_dev(&order, &tickets, 1);
    assert_eq!(
        fold.shown_cards.len(),
        3,
        "needs-you cards stay even past the limit"
    );
    assert!(fold_dev(&order, &tickets, 50).label().is_none());
}

#[tokio::test]
async fn pin_and_unpin() {
    let f = Fixture::shop().await;
    let a = f.create("A").await;
    let b = f.create("B").await;
    let c = f.create("C").await;
    let order = |tickets: &[Ticket]| keys(&sort_column(StatusSet::Backlog, tickets).cards);

    f.board.pin(&b.key.to_string(), &me()).await.unwrap();
    f.board.pin(&c.key.to_string(), &me()).await.unwrap();
    assert_eq!(
        order(&f.board.tickets("SHOP").await.unwrap()),
        ["SHOP-3", "SHOP-2", "SHOP-1"]
    );
    f.board.pin(&b.key.to_string(), &me()).await.unwrap();
    assert_eq!(
        order(&f.board.tickets("SHOP").await.unwrap()),
        ["SHOP-2", "SHOP-3", "SHOP-1"],
        "re-pin bumps"
    );
    let unpinned = f.board.unpin(&b.key.to_string(), &me()).await.unwrap();
    assert!(!unpinned.flags.is_pinned());
    assert_eq!(
        (unpinned.board_group(), unpinned.status),
        (StatusSet::Backlog, Status::Backlog),
        "pinning changes no state"
    );
    assert_eq!(
        order(&f.board.tickets("SHOP").await.unwrap()),
        ["SHOP-3", "SHOP-1", "SHOP-2"]
    );
    let _ = a;

    // The pin follows the ticket across columns.
    f.board
        .move_ticket(
            "SHOP-3",
            otaq_core::board::moves::DropTarget::ReadyForDev,
            true,
            &me(),
        )
        .await
        .unwrap();
    let tickets = f.board.tickets("SHOP").await.unwrap();
    assert_eq!(
        keys(&sort_column(StatusSet::Waiting, &tickets).cards),
        ["SHOP-3"]
    );
    assert!(f.ticket("SHOP-3").await.flags.is_pinned());
}
