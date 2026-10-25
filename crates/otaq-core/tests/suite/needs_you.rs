//! Needs-you items (ATT-06, SCH-03) with the shared sample data across shop-web and billing-api.

use crate::common::{Fixture, me};
use otaq_core::board::moves::DropTarget;
use otaq_core::board::needs_you::{
    NeedsYouGroup, ProjectTickets, group_of, needs_you_count, needs_you_items,
};
use otaq_core::board::questions::HumanQuestion;
use otaq_core::board::tickets::{AgentSignal, Base, NewTicketInput, StartProblem, Ticket};
use otaq_core::domain::{Actor, Flags, StartStep, Status, TicketKey};
use otaq_core::store::now_ms;

const NOW: i64 = 1_791_600_000_000;
const MIN: i64 = 60_000;

fn t(key: &str, title: &str, status: Status, minutes_ago: i64) -> Ticket {
    Ticket {
        id: key.into(),
        key: key.parse::<TicketKey>().unwrap(),
        title: title.into(),
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

fn deps(mut ticket: Ticket, parents: &[&str]) -> Ticket {
    ticket.depends_on = parents.iter().map(|p| p.parse().unwrap()).collect();
    ticket
}

/// A needs-you ticket whose signal began `minutes_ago`.
fn needs_you(key: &str, title: &str, status: Status, text: &str, minutes_ago: i64) -> Ticket {
    let mut ticket = t(key, title, status, 999);
    ticket.signal = Some(AgentSignal {
        text: Some(text.into()),
        since: Some(NOW - minutes_ago * MIN),
        wakeup_at: None,
    });
    ticket
}

fn shop_web() -> Vec<Ticket> {
    let mut asks = needs_you(
        "SHOP-23",
        "Split user full name",
        Status::AsksYou,
        "signal text",
        6,
    );
    asks.question = Some(HumanQuestion {
        id: "q".into(),
        text: "Should fullName be split everywhere, or only in the profile form?".into(),
        options: Vec::new(),
        recommended: None,
        notes: Vec::new(),
        asked_at: NOW - 6 * MIN,
    });
    let mut start = t("SHOP-28", "Cart badge count", Status::StartInterrupted, 20);
    start.start_problem = Some(StartProblem {
        step: StartStep::Setup,
        message: "setup command failed (exit 1)".into(),
    });
    vec![
        needs_you(
            "SHOP-12",
            "Add dark mode toggle",
            Status::AsksYou,
            "Should dark mode follow the system setting?",
            8,
        ),
        t("SHOP-17", "Add Stripe client", Status::Merged, 600),
        deps(
            t("SHOP-18", "Checkout API", Status::WaitingForYou, 25),
            &["SHOP-17"],
        ),
        t("SHOP-19", "Price table", Status::Working, 30),
        deps(
            t("SHOP-20", "Checkout page", Status::Queued, 40),
            &["SHOP-18", "SHOP-19"],
        ),
        deps(
            t("SHOP-21", "Order confirmation email", Status::Queued, 40),
            &["SHOP-20"],
        ),
        needs_you(
            "SHOP-22",
            "Fix flaky login redirect",
            Status::NeedsYou,
            "Allow rm -rf .next?",
            2,
        ),
        asks,
        t("SHOP-24", "Update footer links", Status::Backlog, 100),
        t("SHOP-25", "Upgrade to Next.js 15", Status::Interrupted, 180),
        t("SHOP-26", "Add sitemap", Status::ClosedNotMerged, 50),
        t("SHOP-27", "Search results paging", Status::Working, 5),
        start,
        deps(
            t("SHOP-30", "Sitemap for blog posts", Status::ParentClosed, 4),
            &["SHOP-26"],
        ),
    ]
}

fn billing_api() -> Vec<Ticket> {
    vec![
        t("BILL-4", "Invoice PDF export", Status::Working, 10),
        t("BILL-5", "VAT rounding fix", Status::WaitingForYou, 60),
        t("BILL-6", "Late fee rule", Status::Held, 15),
    ]
}

fn items(shop: &[Ticket], bill: &[Ticket]) -> Vec<(String, NeedsYouGroup, String, u32)> {
    let projects = [
        ProjectTickets {
            key: "SHOP",
            name: "shop-web",
            tickets: shop,
            flow: &otaq_core::flow::default_file().flow,
        },
        ProjectTickets {
            key: "BILL",
            name: "billing-api",
            tickets: bill,
            flow: &otaq_core::flow::default_file().flow,
        },
    ];
    needs_you_items(&projects, NOW)
        .into_iter()
        .map(|i| (i.key.to_string(), i.group, i.reason, i.blocked))
        .collect()
}

#[test]
fn groups_in_needs_you_order_across_projects() {
    let list = items(&shop_web(), &billing_api());
    let keys: Vec<&str> = list.iter().map(|(k, ..)| k.as_str()).collect();
    assert_eq!(
        keys,
        [
            // 1 · turquoise, oldest first
            "SHOP-12", "SHOP-23", "SHOP-22", //
            // 2 · interrupted, start interrupted, parent closed
            "SHOP-25", "SHOP-28", "SHOP-30", //
            // 3 · review: SHOP-18 blocks two tickets, so it comes before the older BILL-5
            "SHOP-18", "BILL-5",
        ]
    );
    let reasons: Vec<&str> = list.iter().map(|(_, _, r, _)| r.as_str()).collect();
    assert_eq!(
        reasons,
        [
            "asks: Should dark mode follow the system setting?",
            "asks: Should fullName be split everywhere, or only in the profile form?",
            "permission: Allow rm -rf .next?",
            "session interrupted",
            "start failed · setup command failed",
            "parent SHOP-26 was closed without merging",
            "ready for review",
            "ready for review",
        ]
    );
    assert_eq!(list[6].3, 2, "SHOP-18 blocks SHOP-20 and SHOP-21");
    assert!(
        list[6..]
            .iter()
            .all(|(_, g, ..)| *g == NeedsYouGroup::Review)
    );
}

#[test]
fn blocks_more_first_inside_a_group() {
    let mut shop = shop_web();
    // SHOP-22's fix blocks SHOP-24 now: it moves ahead of the older needs-you items.
    let i = shop
        .iter()
        .position(|t| t.key.to_string() == "SHOP-24")
        .unwrap();
    shop[i].depends_on = vec!["SHOP-22".parse().unwrap()];
    let list = items(&shop, &billing_api());
    let needs_you: Vec<&str> = list
        .iter()
        .filter(|(_, g, ..)| *g == NeedsYouGroup::Family)
        .map(|(k, ..)| k.as_str())
        .collect();
    assert_eq!(needs_you, ["SHOP-22", "SHOP-12", "SHOP-23"]);
}

#[test]
fn every_field_of_an_item() {
    let shop = shop_web();
    let projects = [ProjectTickets {
        key: "SHOP",
        name: "shop-web",
        tickets: &shop,
        flow: &otaq_core::flow::default_file().flow,
    }];
    let list = needs_you_items(&projects, NOW);
    let first = &list[0];
    assert_eq!(first.project, "SHOP");
    assert_eq!(first.project_name, "shop-web");
    assert_eq!(first.key.to_string(), "SHOP-12");
    assert_eq!(first.title, "Add dark mode toggle");
    assert_eq!(first.status, Status::AsksYou);
    assert_eq!(
        first.age_ms,
        8 * MIN,
        "from the signal, not the status time"
    );
    assert_eq!(first.since, NOW - 8 * MIN);
    assert_eq!(
        list.iter()
            .find(|i| i.key.to_string() == "SHOP-25")
            .unwrap()
            .age_ms,
        180 * MIN
    );
}

#[test]
fn back_pressure_counts_every_group() {
    let shop = shop_web();
    let bill = billing_api();
    let projects = [
        ProjectTickets {
            key: "SHOP",
            name: "shop-web",
            tickets: &shop,
            flow: &otaq_core::flow::default_file().flow,
        },
        ProjectTickets {
            key: "BILL",
            name: "billing-api",
            tickets: &bill,
            flow: &otaq_core::flow::default_file().flow,
        },
    ];
    let list = needs_you_items(&projects, NOW);
    assert_eq!(needs_you_count(&list), 8);
}

#[test]
fn states_that_arrive_later_slot_in_by_status() {
    for status in [Status::Interrupted, Status::ResumeFailed] {
        assert_eq!(group_of(status), Some(NeedsYouGroup::Stopped));
    }
    for status in [
        Status::Working,
        Status::WorkingInBackground,
        Status::WaitingForLimitReset,
        Status::Queued,
        Status::WaitingForSlot,
        Status::Held,
        Status::PrOpen,
        Status::Merged,
    ] {
        assert_eq!(group_of(status), None, "{status}");
    }
    let mut shop = shop_web();
    shop.push(t("SHOP-29", "Resume me", Status::ResumeFailed, 300));
    let list = items(&shop, &[]);
    assert_eq!(list[3].0, "SHOP-29", "the oldest of group 2");
    assert_eq!(list[3].2, "resume failed");
}

/// T-0196 (ATT-06, d0063, d0064): the M12 items' lines of frame *10 → New item types · group 1 (M12)*, and
/// "validated · round 2" on group 3.
#[test]
fn the_m12_items_read_as_the_frame() {
    use otaq_core::domain::{AutoMergeStop, ReviewEntry, ReviewEntryFlag};
    let validator = needs_you(
        "SHOP-33",
        "Shipping estimate",
        Status::CheckerNeedsYou,
        "Allow Bash(pnpm test)?",
        1,
    );
    let mut failed = t("SHOP-32", "Address autocomplete", Status::CheckFailed, 12);
    failed.validation_round = 3;
    failed.flags.verdict =
        Some("The description asks for keyboard navigation; arrow keys do nothing".into());
    let no_result = t("SHOP-34", "Tax display", Status::CheckNoResult, 4);
    let mut stopped = needs_you(
        "SHOP-40",
        "Wishlist share link",
        Status::DeliveryStopped,
        "Keeps conflicting with `wishlist`",
        7,
    );
    stopped.auto_merge_stop = Some(AutoMergeStop::KeepsConflicting);
    let mut review = t("SHOP-18", "Checkout API", Status::WaitingForYou, 25);
    review.flags.review_entry = Some(ReviewEntryFlag {
        entry: ReviewEntry::Validated,
        round: 2,
    });
    let shop = vec![validator, failed, no_result, stopped, review];
    let lines: Vec<(String, NeedsYouGroup, String)> = items(&shop, &[])
        .into_iter()
        .map(|(key, group, reason, _)| (key, group, reason))
        .collect();
    use NeedsYouGroup::*;
    assert_eq!(
        lines,
        [
            (
                "SHOP-32".into(),
                Family,
                "validation failed 3 times · “The description asks for keyboard navigation; arrow keys do nothing”"
                    .into()
            ),
            (
                "SHOP-40".into(),
                Family,
                "Delivery stopped · keeps conflicting with wishlist after 3 tries in a row".into()
            ),
            ("SHOP-34".into(), Family, "validation ended without a result".into()),
            (
                "SHOP-33".into(),
                Family,
                "validator permission: Allow Bash(pnpm test)?".into()
            ),
            (
                "SHOP-18".into(),
                Review,
                "ready for review · validated · round 2".into()
            ),
        ]
    );
}

#[tokio::test]
async fn the_board_derives_items_across_projects_with_status_times() {
    let f = Fixture::shop().await;
    f.add_project("billing-api").await;
    let ask = f.create("Split user full name").await;
    let review = f
        .create_with(NewTicketInput {
            project: "BILL".into(),
            title: "VAT rounding fix".into(),
            ..Default::default()
        })
        .await;
    let before = now_ms();
    let queued = f
        .board
        .move_ticket(
            &review.key.to_string(),
            DropTarget::ReadyForDev,
            false,
            &me(),
        )
        .await
        .unwrap();
    assert!(queued.status_since.unwrap() >= before, "set by the move");

    let pool = f.board.store().pool();
    for (ticket, status) in [(&ask, Status::AsksYou), (&review, Status::WaitingForYou)] {
        sqlx::query("UPDATE tickets SET stage_id = ?, state = ?, state_since = ? WHERE id = ?")
            .bind(otaq_core::flow::engine::default_stage(status, None))
            .bind(status.as_str())
            .bind(NOW - 5 * MIN)
            .bind(&ticket.id)
            .execute(pool)
            .await
            .unwrap();
    }
    let list = f.board.needs_you_items(NOW).await.unwrap();
    let keys: Vec<String> = list.iter().map(|i| i.key.to_string()).collect();
    assert_eq!(keys, [ask.key.to_string(), review.key.to_string()]);
    assert_eq!(list[1].project_name, "billing-api");
    assert_eq!(list[1].age_ms, 5 * MIN);
    assert_eq!(list[0].reason, "asks you");
}
