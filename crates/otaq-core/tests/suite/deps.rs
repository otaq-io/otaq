//! Dependencies (DEP-01, DEP-02, DEP-04, PRJ-05) with the shop-web sample: SHOP-17 → SHOP-18 → SHOP-20 ← SHOP-19,
//! SHOP-21 depending on SHOP-20, the hotfix SHOP-31 on `release/2.3`.

use crate::common::{Fixture, me, shop_input};
use otaq_core::board::Refusal;
use otaq_core::board::deps::DepNode;
use otaq_core::board::tickets::{NewTicketInput, TicketUpdate};
use otaq_core::domain::Status;

/// Creates SHOP-17…SHOP-21 with the sample links.
async fn sample() -> Fixture {
    let f = Fixture::shop().await;
    f.skip_to(17).await;
    for title in [
        "Cart totals",
        "Checkout API",
        "Payment form",
        "Checkout page",
        "Order confirmation email",
    ] {
        f.create(title).await;
    }
    for (child, parent) in [
        ("SHOP-18", "SHOP-17"),
        ("SHOP-20", "SHOP-18"),
        ("SHOP-20", "SHOP-19"),
        ("SHOP-21", "SHOP-20"),
    ] {
        f.board.add_dependency(child, parent, &me()).await.unwrap();
    }
    f
}

fn keys(nodes: &[DepNode]) -> Vec<String> {
    nodes.iter().map(|n| n.key.to_string()).collect()
}

#[tokio::test]
async fn links_and_tree() {
    let f = sample().await;
    let t20 = f.ticket("SHOP-20").await;
    assert_eq!(
        t20.depends_on
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>(),
        ["SHOP-18", "SHOP-19"]
    );

    let tree = f.board.dependency_tree("SHOP-20").await.unwrap();
    assert_eq!(keys(&tree.parents), ["SHOP-18", "SHOP-19"]);
    assert_eq!(keys(&tree.parents[0].nodes), ["SHOP-17"]);
    assert_eq!(tree.parents[0].status, Status::Backlog);
    assert_eq!(keys(&tree.children), ["SHOP-21"]);

    let tree = f.board.dependency_tree("SHOP-17").await.unwrap();
    assert!(tree.parents.is_empty());
    assert_eq!(keys(&tree.children), ["SHOP-18"]);
    assert_eq!(keys(&tree.children[0].nodes), ["SHOP-20"]);
    assert_eq!(keys(&tree.children[0].nodes[0].nodes), ["SHOP-21"]);

    assert!(f.board.dependency_cycles("SHOP").await.unwrap().is_empty());

    f.board
        .remove_dependency("SHOP-20", "SHOP-19", &me())
        .await
        .unwrap();
    assert_eq!(
        keys(&f.board.dependency_tree("SHOP-20").await.unwrap().parents),
        ["SHOP-18"]
    );
    assert!(
        f.board
            .remove_dependency("SHOP-20", "SHOP-19", &me())
            .await
            .is_err()
    );
}

#[tokio::test]
async fn refusals() {
    let f = sample().await;
    let err = f
        .board
        .add_dependency("SHOP-20", "SHOP-21", &me())
        .await
        .unwrap_err();
    assert_eq!(err.refusal(), Some(Refusal::Cycle));
    assert!(
        err.to_string().ends_with("SHOP-21 → SHOP-20 → SHOP-21"),
        "{err}"
    );

    let err = f
        .board
        .add_dependency("SHOP-17", "SHOP-21", &me())
        .await
        .unwrap_err();
    assert_eq!(err.refusal(), Some(Refusal::Cycle));
    assert!(
        err.to_string()
            .ends_with("SHOP-21 → SHOP-17 → SHOP-18 → SHOP-20 → SHOP-21"),
        "{err}"
    );

    let err = f
        .board
        .add_dependency("SHOP-18", "SHOP-18", &me())
        .await
        .unwrap_err();
    assert_eq!(err.refusal(), Some(Refusal::SelfDependency));
    let err = f
        .board
        .add_dependency("SHOP-20", "SHOP-18", &me())
        .await
        .unwrap_err();
    assert_eq!(err.refusal(), Some(Refusal::DuplicateDependency));

    f.add_project("billing-api").await;
    f.create_with(NewTicketInput {
        project: "BILL".into(),
        title: "Invoices".into(),
        ..Default::default()
    })
    .await;
    let err = f
        .board
        .add_dependency("SHOP-20", "BILL-1", &me())
        .await
        .unwrap_err();
    assert_eq!(err.refusal(), Some(Refusal::OtherProject));

    assert!(f.board.dependency_cycles("SHOP").await.unwrap().is_empty());
}

#[tokio::test]
async fn linked_tickets_share_a_base() {
    let f = sample().await;
    f.skip_to(31).await;
    let hotfix = f
        .create_with(NewTicketInput {
            title: "Hotfix".into(),
            base_branch: Some("release/2.3".into()),
            ..shop_input()
        })
        .await;
    assert_eq!(hotfix.key.to_string(), "SHOP-31");
    let err = f
        .board
        .add_dependency("SHOP-31", "SHOP-20", &me())
        .await
        .unwrap_err();
    assert_eq!(err.refusal(), Some(Refusal::CrossBase));
    assert_eq!(
        err.to_string(),
        "SHOP-31 merges into `release/2.3`, SHOP-20 into `main`. Linked tickets must use the same base branch."
    );

    // Changing a linked ticket's base is refused with the same message.
    let err = f
        .board
        .update_ticket(
            "SHOP-21",
            &TicketUpdate {
                base_branch: Some("release/2.3".into()),
                ..Default::default()
            },
            &me(),
        )
        .await
        .unwrap_err();
    assert_eq!(err.refusal(), Some(Refusal::CrossBase));
    assert!(
        err.to_string()
            .starts_with("SHOP-21 merges into `release/2.3`, SHOP-20 into `main`."),
        "{err}"
    );
    // An unlinked ticket can change its base.
    f.board
        .update_ticket(
            "SHOP-31",
            &TicketUpdate {
                base_branch: Some("main".into()),
                ..Default::default()
            },
            &me(),
        )
        .await
        .unwrap();
    f.board
        .add_dependency("SHOP-31", "SHOP-20", &me())
        .await
        .unwrap();
}

#[tokio::test]
async fn cycles_finds_loops_written_around_the_rules() {
    let f = sample().await;
    let (t17, t21) = (f.ticket("SHOP-17").await, f.ticket("SHOP-21").await);
    sqlx::query("INSERT INTO ticket_deps (ticket_id, depends_on_id) VALUES (?, ?)")
        .bind(&t17.id)
        .bind(&t21.id)
        .execute(f.board.store().pool())
        .await
        .unwrap();
    let cycles = f.board.dependency_cycles("SHOP").await.unwrap();
    assert_eq!(cycles.len(), 1);
    let lap: Vec<_> = cycles[0].iter().map(ToString::to_string).collect();
    assert_eq!(lap.first(), lap.last());
    assert_eq!(lap.len(), 5, "{lap:?}");
    // The tree still terminates.
    f.board.dependency_tree("SHOP-20").await.unwrap();
}

/// DPV-03 (d0049): a drag or Start past Ready for Dev is refused while a dependency isn't merged ("SHOP-20 is
/// blocked"); a drop on Ready for Dev still queues it, and from there it can't be dragged on into Dev either. Once
/// every parent is merged the same drag goes through as before.
#[tokio::test]
async fn a_blocked_ticket_cant_start_past_ready_for_dev() {
    use otaq_core::board::moves::DropTarget;
    let f = sample().await;
    let refused = |r: Result<otaq_core::board::tickets::Ticket, otaq_core::board::BoardError>| {
        let e = r.expect_err("refused");
        assert_eq!(e.refusal(), Some(Refusal::MoveRefused), "{e}");
        e.to_string()
    };
    let into_dev = f
        .board
        .move_ticket("SHOP-20", DropTarget::Dev, false, &me())
        .await;
    assert!(refused(into_dev).contains("SHOP-20 is blocked"));
    assert_eq!(f.ticket("SHOP-20").await.status, Status::Backlog);

    let queued = f
        .board
        .move_ticket("SHOP-20", DropTarget::ReadyForDev, false, &me())
        .await
        .unwrap();
    assert_eq!(queued.status, Status::Queued, "into Ready it queues");
    let again = f
        .board
        .move_ticket("SHOP-20", DropTarget::Dev, false, &me())
        .await;
    assert!(refused(again).contains("SHOP-20 is blocked"));

    // SHOP-17 has no parents: dragged into Dev it queues as before.
    let free = f
        .board
        .move_ticket("SHOP-17", DropTarget::Dev, false, &me())
        .await
        .unwrap();
    assert_eq!(free.status, Status::Queued);

    // Merged parents: SHOP-18 (whose own parent is SHOP-17) and SHOP-19. SHOP-20 is no longer blocked.
    for key in ["SHOP-18", "SHOP-19"] {
        let t = f.ticket(key).await;
        f.force_status(&t, Status::Merged).await;
    }
    let tickets = f.board.tickets("SHOP").await.unwrap();
    let t20 = tickets
        .iter()
        .find(|t| t.key.to_string() == "SHOP-20")
        .unwrap();
    assert!(otaq_core::board::readiness::readiness(t20, &tickets).is_ready());
    let check = f
        .board
        .check_drag(t20, &DropTarget::Dev.into())
        .await
        .unwrap();
    assert_eq!(
        check,
        otaq_core::board::moves::MoveCheck::Refused {
            reason: otaq_core::board::moves::STARTS_BY_ITSELF.into()
        },
        "a ready ticket in Ready for Dev starts by itself, as before"
    );
}
