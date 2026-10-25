//! The board state machine (SDD §7.3, §7.4; TKT-04): every column pair and the relevant sub-states, then the
//! moves this milestone carries out.

use crate::common::{Fixture, me, shop_input};
use otaq_core::board::Refusal;
use otaq_core::board::moves::{DropTarget, MoveAction, MoveCheck, check_move};
use otaq_core::board::tickets::{Base, NewTicketInput, Ticket};
use otaq_core::domain::{Actor, Flags, Status, StatusSet, TicketKey};

fn ticket(status: Status) -> Ticket {
    Ticket {
        id: "id".into(),
        key: "SHOP-18".parse::<TicketKey>().unwrap(),
        title: "t".into(),
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

fn allowed(action: MoveAction) -> MoveCheck {
    MoveCheck::Allowed { action }
}

fn reason(check: &MoveCheck) -> &str {
    match check {
        MoveCheck::Refused { reason } => reason,
        other => panic!("expected a refusal, got {other:?}"),
    }
}

#[test]
fn allowed_drags() {
    use DropTarget as T;
    let rows = [
        (Status::Backlog, T::ReadyForDev, allowed(MoveAction::Start)),
        // On Dev it lands in Ready for Dev until it can start (d0066).
        (Status::Backlog, T::Dev, allowed(MoveAction::Start)),
        (Status::Parked, T::ReadyForDev, allowed(MoveAction::Start)),
        (Status::Parked, T::Dev, allowed(MoveAction::Start)),
        (Status::Queued, T::Backlog, allowed(MoveAction::Unqueue)),
        (
            Status::WaitingForSlot,
            T::Backlog,
            allowed(MoveAction::Unqueue),
        ),
        (
            Status::ParentClosed,
            T::Backlog,
            allowed(MoveAction::Unqueue),
        ),
        (
            Status::Working,
            T::Backlog,
            MoveCheck::Confirm {
                action: MoveAction::Park,
            },
        ),
        (
            Status::NeedsYou,
            T::Backlog,
            MoveCheck::Confirm {
                action: MoveAction::Park,
            },
        ),
        (
            Status::WaitingForYou,
            T::Approved,
            MoveCheck::Choose {
                action: MoveAction::Approve,
            },
        ),
        (
            Status::PrOpen,
            T::ReadyForReview,
            allowed(MoveAction::PrRework),
        ),
        (
            Status::ClosedNotMerged,
            T::Backlog,
            allowed(MoveAction::Reopen),
        ),
    ];
    for (status, target, expected) in rows {
        assert_eq!(
            check_move(&ticket(status), target),
            expected,
            "{status} → {target}"
        );
    }
}

/// No "Done when" warning any more (TKT-04, d0067): a ticket with an empty description starts like any other.
#[test]
fn an_empty_description_never_warns() {
    let check = check_move(&ticket(Status::Backlog), DropTarget::ReadyForDev);
    assert_eq!(
        check,
        MoveCheck::Allowed {
            action: MoveAction::Start
        }
    );
}

#[test]
fn refused_drags_carry_a_reason() {
    use DropTarget as T;
    let t = ticket;
    for status in [Status::Backlog, Status::ClosedNotMerged] {
        assert_eq!(
            reason(&check_move(&t(status), T::ReadyForReview)),
            "Only the agent or Move to next stage moves a ticket to Ready for Review"
        );
    }
    // A forward drag is the user's Move to next stage (FLW-18): not while the agent is mid-turn.
    assert_eq!(
        reason(&check_move(&t(Status::Working), T::ReadyForReview)),
        "SHOP-18 can't move to the next stage now: its agent is in the middle of a turn"
    );
    for target in [T::Backlog, T::ReadyForDev, T::ReadyForReview, T::Approved] {
        assert_eq!(
            reason(&check_move(&t(Status::Merged), target)),
            "Merged tickets are locked"
        );
    }
    assert_eq!(
        reason(&check_move(&t(Status::Queued), T::Dev)),
        "It starts by itself when it can, in priority order",
        "not into Dev by hand"
    );
    assert_eq!(
        reason(&check_move(&t(Status::Working), T::ReadyForDev)),
        "Drag it to Backlog to park it"
    );
    for (status, target) in [
        (Status::Backlog, T::Approved),
        (Status::Backlog, T::Done),
        (Status::Working, T::Approved),
        (Status::Working, T::Done),
        (Status::WaitingForYou, T::ReadyForDev),
        (Status::WaitingForYou, T::Backlog),
        (Status::WaitingForYou, T::Done),
        (Status::PrOpen, T::ReadyForDev),
        (Status::PrOpen, T::Backlog),
        (Status::PrOpen, T::Done),
        (Status::ClosedNotMerged, T::ReadyForDev),
        (Status::ClosedNotMerged, T::Approved),
    ] {
        let check = check_move(&t(status), target);
        assert!(!reason(&check).is_empty(), "{status} → {target}");
    }
}

/// The board group a drop target is on (the default flow's columns; Approved is the Delivery's section).
fn group(target: DropTarget) -> StatusSet {
    match target {
        DropTarget::Backlog => StatusSet::Backlog,
        DropTarget::ReadyForDev => StatusSet::Waiting,
        DropTarget::Dev => StatusSet::AgentStage,
        DropTarget::ReadyForReview => StatusSet::HumanCheck,
        DropTarget::Approved => StatusSet::Delivery,
        DropTarget::Done => StatusSet::Done,
    }
}

#[test]
fn same_column_is_unchanged_and_every_pair_is_decided() {
    for status in Status::ALL {
        for target in DropTarget::ALL {
            let check = check_move(&ticket(*status), *target);
            // Validate has its own column (FLW-01): a card in it dropped on Dev goes back (FLW-18).
            let check_on_dev = status.set() == StatusSet::AgentCheck && *target == DropTarget::Dev;
            if status.set().board_group() == group(*target) && !check_on_dev {
                assert_eq!(check, MoveCheck::Unchanged, "{status} → {target}");
            }
            if *status == Status::Merged && group(*target) != StatusSet::Done {
                assert!(check.is_refused());
            }
        }
    }
}

#[tokio::test]
async fn start_queues_the_ticket_and_locks_its_base() {
    let f = Fixture::shop().await;
    let t = f
        .create_with(NewTicketInput {
            title: "Checkout".into(),
            ..shop_input()
        })
        .await;
    let moved = f
        .board
        .move_ticket(&t.key.to_string(), DropTarget::ReadyForDev, false, &me())
        .await
        .unwrap();
    assert_eq!(
        (moved.board_group(), moved.status),
        (StatusSet::Waiting, Status::Queued)
    );
    assert!(moved.base_locked && moved.has_started());

    // No description at all: queued without a confirmation (d0067).
    let bare = f.create("Order history").await;
    let moved = f
        .board
        .move_ticket(&bare.key.to_string(), DropTarget::ReadyForDev, false, &me())
        .await
        .unwrap();
    assert_eq!(moved.status, Status::Queued);
}

#[tokio::test]
async fn later_moves_are_allowed_but_not_available_yet() {
    let f = Fixture::shop().await;
    let t = f.create("Checkout").await;
    let key = t.key.to_string();
    // A working ticket: Park needs a confirmation.
    sqlx::query("UPDATE tickets SET state = 'working', stage_id = 'dev' WHERE id = ?")
        .bind(&t.id)
        .execute(f.board.store().pool())
        .await
        .unwrap();
    let err = f
        .board
        .move_ticket(&key, DropTarget::Backlog, false, &me())
        .await
        .unwrap_err();
    assert_eq!(err.refusal(), Some(Refusal::NeedsConfirmation));
    let err = f
        .board
        .move_ticket(&key, DropTarget::Backlog, true, &me())
        .await
        .unwrap_err();
    assert_eq!(err.refusal(), Some(Refusal::NotAvailableYet));
    assert_eq!(err.to_string(), "Park isn't available yet");
    let err = f
        .board
        .move_ticket(&key, DropTarget::ReadyForReview, true, &me())
        .await
        .unwrap_err();
    assert_eq!(err.refusal(), Some(Refusal::MoveRefused));
}

#[tokio::test]
async fn reopen_of_a_never_started_closed_ticket_is_bookkeeping() {
    let f = Fixture::shop().await;
    let never = f.create("Search").await;
    sqlx::query("UPDATE tickets SET stage_id = NULL, state = 'closed_not_merged' WHERE id = ?")
        .bind(&never.id)
        .execute(f.board.store().pool())
        .await
        .unwrap();
    let reopened = f
        .board
        .move_ticket(&never.key.to_string(), DropTarget::Backlog, false, &me())
        .await
        .unwrap();
    assert_eq!(
        (reopened.board_group(), reopened.status),
        (StatusSet::Backlog, Status::Backlog)
    );

    let t = f.create("Started then closed").await;
    f.board
        .move_ticket(&t.key.to_string(), DropTarget::ReadyForDev, true, &me())
        .await
        .unwrap();
    sqlx::query("UPDATE tickets SET stage_id = NULL, state = 'closed_not_merged' WHERE id = ?")
        .bind(&t.id)
        .execute(f.board.store().pool())
        .await
        .unwrap();
    // Started work reopens too (T-0101): its next start rebases the old branch first.
    let reopened = f
        .board
        .move_ticket(&t.key.to_string(), DropTarget::Backlog, false, &me())
        .await
        .unwrap();
    assert_eq!(reopened.status, Status::Backlog);
    let row = f.board.store().ticket(&t.id).await.unwrap().unwrap();
    assert!(row.reopened);
}

/// Ready for Dev → Backlog (d0066): out of the queue; a ticket that never worked is "never started" again, so it
/// can still be deleted (d0060).
#[tokio::test]
async fn unqueue_takes_it_out_of_the_queue() {
    let f = Fixture::shop().await;
    let t = f.create("Checkout").await;
    let key = t.key.to_string();
    for target in [DropTarget::Dev, DropTarget::ReadyForDev] {
        let queued = f
            .board
            .move_ticket(&key, target, false, &me())
            .await
            .unwrap();
        assert_eq!(
            (queued.board_group(), queued.status),
            (StatusSet::Waiting, Status::Queued),
            "{target}"
        );
        let back = f
            .board
            .move_ticket(&key, DropTarget::Backlog, false, &me())
            .await
            .unwrap();
        assert_eq!(
            (back.board_group(), back.status),
            (StatusSet::Backlog, Status::Backlog)
        );
        assert_eq!((back.queued_at, back.ready_since), (None, None));
        assert!(
            !back.has_started() && !back.base_locked,
            "nothing had started"
        );
    }
    f.board.delete_ticket(&key).await.unwrap();
}

/// "Ready to start" (d0066): a Backlog ticket whose parents are all merged; it never starts.
#[tokio::test]
async fn backlog_tickets_with_merged_parents_are_ready_to_start() {
    let f = Fixture::shop().await;
    let parent = f.create("Checkout API").await;
    let child = f.create("Checkout page").await;
    f.board
        .add_dependency(&child.key.to_string(), &parent.key.to_string(), &me())
        .await
        .unwrap();
    let flags = |tickets: &[otaq_core::board::tickets::Ticket], key: &TicketKey| {
        tickets
            .iter()
            .find(|t| &t.key == key)
            .unwrap()
            .flags
            .ready_to_start
    };
    let tickets = f.board.tickets("SHOP").await.unwrap();
    assert!(flags(&tickets, &parent.key), "no parents: unblocked");
    assert!(!flags(&tickets, &child.key), "waits for SHOP's parent");
    sqlx::query("UPDATE tickets SET state = 'merged', stage_id = NULL WHERE id = ?")
        .bind(&parent.id)
        .execute(f.board.store().pool())
        .await
        .unwrap();
    let tickets = f.board.tickets("SHOP").await.unwrap();
    assert!(flags(&tickets, &child.key));
    assert_eq!(
        f.ticket(&child.key.to_string()).await.status,
        Status::Backlog
    );
}
