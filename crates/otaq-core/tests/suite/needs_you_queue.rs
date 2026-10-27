//! Needs you over the protocol (T-0115, ATT-06, ATT-09, ATT-10; d0016, d0024): items across projects with counts
//! in total and per project, `needs_you_changed` only when the set changes, items resolved elsewhere gone at the next
//! derivation, focused tickets left out, and the plan-limit switch.

use std::time::Duration;

use otaq_core::agent_tools::Tool;
use otaq_core::attention::needs_you::{NeedsYouCounts, NeedsYouList};
use otaq_core::board::needs_you::NeedsYouGroup;
use otaq_core::caller::Caller;
use otaq_core::client::{Client, ClientConfig};
use otaq_core::domain::Status;
use otaq_core::protocol::{
    Event, FocusParams, InputParams, KeyParams, Reply, Request, encode_bytes,
};
use otaq_core::service::testing::TestService;
use tokio::sync::broadcast;

async fn list(client: &Client) -> NeedsYouList {
    let Reply::NeedsYou(list) = client.request(Request::NeedsYouList).await.unwrap() else {
        panic!("needs you")
    };
    *list
}

async fn counts(client: &Client) -> NeedsYouCounts {
    let Reply::NeedsYouCounts(counts) = client.request(Request::NeedsYouCounts).await.unwrap()
    else {
        panic!("counts")
    };
    counts
}

/// The next `needs_you_changed` within `wait`, if any.
async fn next_change(
    events: &mut broadcast::Receiver<Event>,
    wait: Duration,
) -> Option<NeedsYouCounts> {
    tokio::time::timeout(wait, async {
        loop {
            if let Ok(Event::NeedsYouChanged { counts }) = events.recv().await {
                return counts;
            }
        }
    })
    .await
    .ok()
}

/// The first `needs_you_changed` with `total` items within `wait`, skipping any earlier ones (a slow machine can
/// publish the watcher's first look late, after the test has moved on).
async fn change_to(
    events: &mut broadcast::Receiver<Event>,
    total: u32,
    wait: Duration,
) -> Option<NeedsYouCounts> {
    tokio::time::timeout(wait, async {
        loop {
            if let Ok(Event::NeedsYouChanged { counts }) = events.recv().await
                && counts.total == total
            {
                return counts;
            }
        }
    })
    .await
    .ok()
}

#[tokio::test(flavor = "multi_thread")]
async fn items_and_counts_across_projects() {
    let w = TestService::start().await;
    w.shop_sample().await;
    w.add_project("billing-api").await;
    let bill = w.create("BILL", "VAT rounding fix").await;
    w.force_status(&bill, Status::WaitingForYou).await;
    w.add_project("otaq").await;
    let client = Client::connect(ClientConfig::new(&w.data)).await.unwrap();

    let needs_you = list(&client).await;
    let keys: Vec<(String, NeedsYouGroup)> = needs_you
        .items
        .iter()
        .map(|i| (i.key.to_string(), i.group))
        .collect();
    // Turquoise, then stopped, then review; inside a group, blocks-more first, then oldest.
    assert_eq!(
        keys,
        [
            ("SHOP-12".into(), NeedsYouGroup::Family),
            ("SHOP-22".into(), NeedsYouGroup::Family),
            ("SHOP-23".into(), NeedsYouGroup::Family),
            ("SHOP-25".into(), NeedsYouGroup::Stopped),
            ("SHOP-28".into(), NeedsYouGroup::Stopped),
            ("SHOP-18".into(), NeedsYouGroup::Review),
            ("BILL-1".into(), NeedsYouGroup::Review),
        ]
    );
    let shop18 = needs_you
        .items
        .iter()
        .find(|i| i.key.to_string() == "SHOP-18")
        .unwrap();
    assert_eq!(
        (
            shop18.project.as_str(),
            shop18.title.as_str(),
            shop18.blocked
        ),
        ("SHOP", "Checkout API", 2)
    );
    assert_eq!(needs_you.counts.total, 7);
    assert_eq!(
        (
            needs_you.counts.of_project("SHOP"),
            needs_you.counts.of_project("BILL"),
            needs_you.counts.of_project("CREW")
        ),
        (6, 1, 0)
    );
    assert_eq!(
        needs_you.counts.projects.len(),
        3,
        "every project is listed"
    );
    assert_eq!(counts(&client).await, needs_you.counts);
    // The project switcher shows the same numbers (ATT-10).
    for info in w.board.project_infos().await.unwrap() {
        assert_eq!(
            info.needs_you,
            needs_you.counts.of_project(&info.key),
            "{}",
            info.key
        );
    }
    w.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn changes_are_published_only_when_the_set_changes() {
    let w = TestService::start().await;
    w.add_project("shop-web").await;
    let t = w.create("SHOP", "Order confirmation email").await;
    let other = w.create("SHOP", "Footer links").await;
    w.force_status(&t, Status::Working).await;
    let shell = w.agent_shell(&t).await;
    let client = Client::connect(ClientConfig::new(&w.data)).await.unwrap();
    let mut events = client.subscribe().await.unwrap();
    let agent = Caller::Agent {
        ticket: t.key.clone(),
        ticket_id: t.id.clone(),
        session: shell.session.clone(),
    };

    // A change that doesn't touch the set: no event.
    client
        .request(Request::TicketPin(KeyParams {
            key: other.key.to_string(),
        }))
        .await
        .unwrap();
    assert_eq!(
        next_change(&mut events, Duration::from_millis(400)).await,
        None
    );

    // The agent asks: one event, with the counts.
    w.signals
        .ask_human(&agent, &t.id, "Which email provider should I use?")
        .await
        .unwrap();
    let counts = next_change(&mut events, Duration::from_secs(5))
        .await
        .unwrap();
    assert_eq!((counts.total, counts.of_project("SHOP")), (1, 1));
    assert_eq!(
        next_change(&mut events, Duration::from_millis(300)).await,
        None
    );

    // Answered elsewhere (Enter in the terminal, as through `otaq attach`): the item is gone.
    client
        .request(Request::SessionInput(InputParams {
            session: shell.session.clone(),
            data: encode_bytes(b"Postmark\r"),
            reply: false,
            if_empty: false,
        }))
        .await
        .unwrap();
    let counts = next_change(&mut events, Duration::from_secs(5))
        .await
        .unwrap();
    assert_eq!(counts.total, 0);
    assert!(list(&client).await.items.is_empty());

    // Focused tickets are left out (ATT-03): asking while focused changes nothing; leaving brings it.
    client
        .request(Request::SessionFocus(FocusParams {
            session: shell.session.clone(),
            focused: true,
        }))
        .await
        .unwrap();
    w.signals
        .ask_human(&agent, &t.id, "And the sender address?")
        .await
        .unwrap();
    assert_eq!(
        next_change(&mut events, Duration::from_millis(400)).await,
        None
    );
    assert!(list(&client).await.items.is_empty());
    client
        .request(Request::SessionFocus(FocusParams {
            session: shell.session.clone(),
            focused: false,
        }))
        .await
        .unwrap();
    assert_eq!(
        next_change(&mut events, Duration::from_secs(5))
            .await
            .unwrap()
            .total,
        1
    );
    w.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn limit_stopped_agents_raise_no_items_while_the_switch_is_on() {
    let w = TestService::start().await;
    w.add_project("shop-web").await;
    let stopped = w.create("SHOP", "Stopped by the limit").await;
    let other = w.create("SHOP", "Asks something").await;
    w.force_status(&stopped, Status::Idle).await;
    w.force_status(&other, Status::AsksYou).await;
    let client = Client::connect(ClientConfig::new(&w.data)).await.unwrap();
    let mut events = client.subscribe().await.unwrap();
    // Let the watcher take its first look.
    tokio::time::sleep(Duration::from_millis(100)).await;

    w.needs_you
        .mark_limit_stopped(&stopped.id, Some(Tool::Claude));
    // Marked, but the switch is off: still an item.
    assert_eq!(list(&client).await.counts.total, 2);
    w.needs_you.set_suppress_limit_stopped(Tool::Claude, true);
    let counts = change_to(&mut events, 1, Duration::from_secs(5))
        .await
        .unwrap();
    assert_eq!(counts.of_project("SHOP"), 1);
    let keys: Vec<String> = list(&client)
        .await
        .items
        .iter()
        .map(|i| i.key.to_string())
        .collect();
    assert_eq!(keys, ["SHOP-2"]);
    // Back-pressure counts what Needs you shows.
    let Reply::SchedulerStatus(status) = client.request(Request::SchedulerStatus).await.unwrap()
    else {
        panic!("scheduler status")
    };
    assert_eq!(status.needs_you, 1);

    w.needs_you.set_suppress_limit_stopped(Tool::Claude, false);
    assert!(
        change_to(&mut events, 2, Duration::from_secs(5))
            .await
            .is_some()
    );
    w.stop().await;
}

/// d0139 (T-0567): the list carries the tickets an agent works on now, across projects, with their stage, for the
/// status icon's Running list; they aren't needs-you items and don't count.
#[tokio::test(flavor = "multi_thread")]
async fn the_list_carries_the_running_tickets_of_every_project() {
    let w = TestService::start().await;
    w.add_project("billing-api").await;
    w.add_project("web-app").await;
    let bill1 = w.create("BILL", "VAT rounding fix").await;
    let bill2 = w.create("BILL", "Invoice template").await;
    let web1 = w.create("WEB", "Landing").await;
    let web2 = w.create("WEB", "Pricing").await;
    let web3 = w.create("WEB", "Footer").await;
    w.force_status(&bill1, Status::Working).await;
    w.force_status(&bill2, Status::WaitingForYou).await;
    w.force_status(&web1, Status::Checking).await;
    w.force_status(&web2, Status::AsksYou).await;
    w.force_status(&web3, Status::WorkingInBackground).await;
    let client = Client::connect(ClientConfig::new(&w.data)).await.unwrap();

    let needs_you = list(&client).await;
    let running: Vec<(String, String, Option<String>)> = needs_you
        .running
        .iter()
        .map(|r| (r.key.to_string(), r.project_name.clone(), r.stage.clone()))
        .collect();
    assert_eq!(
        running,
        [
            ("BILL-1".into(), "billing-api".into(), Some("Dev".into())),
            ("WEB-1".into(), "web-app".into(), Some("Validate".into())),
            ("WEB-3".into(), "web-app".into(), Some("Dev".into())),
        ]
    );
    assert_eq!(
        needs_you.counts.total, 2,
        "WEB-2 asks, BILL-2 waits for its check"
    );
    w.stop().await;
}
