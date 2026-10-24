//! Automatic and click deliveries (T-0054) with an injected "idle" signal and `cat` in a PTY in place of
//! `claude`: held while busy or while the line has unsent text, delivered when both clear, Deliver now, and the
//! Keep / Clear prompt outcomes.

use crate::common;

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use crate::common::Fixture;
use otaq_core::agent_tools::Adapters;
use otaq_core::data_dir::DataDir;
use otaq_core::protocol::Event;
use otaq_core::service::EventBus;
use otaq_core::session::deliveries::{Deliveries, Delivery, DeliveryKind, IdleSignal};
use otaq_core::session::{SessionKind, SessionTicket, Sessions, SpawnSpec, Start, TerminalSize};
use tokio::sync::broadcast;

#[derive(Default)]
struct Idle(AtomicBool);

impl IdleSignal for Idle {
    fn is_idle(&self, _session: &str) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}

struct Setup {
    f: Fixture,
    sessions: Sessions,
    deliveries: Deliveries,
    idle: Arc<Idle>,
    events: broadcast::Receiver<Event>,
    session: String,
    ticket_id: String,
}

async fn setup() -> Setup {
    let f = Fixture::shop().await;
    let t = f.create("Split user full name").await;
    let bus = EventBus::new(256);
    let events = bus.subscribe();
    let sessions = Sessions::new(f.board.store().clone(), bus.clone());
    let idle = Arc::new(Idle::default());
    let adapters = Adapters::launching_only(None, DataDir::new(f.home.path().join("home")));
    let deliveries = Deliveries::new(
        f.board.store().clone(),
        sessions.clone(),
        bus,
        idle.clone(),
        adapters,
    );
    let info = sessions
        .spawn(SpawnSpec {
            kind: SessionKind::Agent,
            start: Start::User,
            project_id: f.board.project("SHOP").await.unwrap().id,
            project_key: "SHOP".into(),
            ticket: Some(SessionTicket {
                id: t.id.clone(),
                key: t.key.clone(),
            }),
            program: "cat".into(),
            args: vec![],
            cwd: f.repos.root(),
            env: vec![],
            size: TerminalSize::default(),
            tool: None,
            tool_session_id: None,
            model: None,
            agent: None,
            effort: None,
            fallback: None,
            theme: None,
        })
        .await
        .unwrap();
    Setup {
        session: info.id,
        ticket_id: t.id,
        f,
        sessions,
        deliveries,
        idle,
        events,
    }
}

impl Setup {
    /// Everything the session printed so far, once `needle` shows up.
    async fn output_with(&self, needle: &str) -> String {
        tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                let out = String::from_utf8_lossy(
                    &self.sessions.attach(&self.session).unwrap().scrollback,
                )
                .into_owned();
                if out.contains(needle) {
                    return out;
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .unwrap_or_else(|_| panic!("{needle:?} never printed"))
    }

    fn output(&self) -> String {
        String::from_utf8_lossy(&self.sessions.attach(&self.session).unwrap().scrollback)
            .into_owned()
    }

    async fn waiting(&self) -> usize {
        self.deliveries.pending(&self.session).await.unwrap().len()
    }

    fn last_count(&mut self) -> Option<u32> {
        let mut count = None;
        while let Ok(event) = self.events.try_recv() {
            if let Event::MessagesWaiting {
                count: c, ticket, ..
            } = event
            {
                assert_eq!(ticket.unwrap().to_string(), "SHOP-1");
                count = Some(c);
            }
        }
        count
    }
}

#[tokio::test]
async fn held_while_busy_then_delivered_in_order_when_idle() {
    let mut s = setup().await;
    s.deliveries
        .queue_delivery(&s.session, DeliveryKind::AgentMessage, "first message")
        .await
        .unwrap();
    s.deliveries
        .queue_delivery(&s.session, DeliveryKind::TicketChange, "second message")
        .await
        .unwrap();
    assert_eq!(s.waiting().await, 2);
    assert_eq!(s.last_count(), Some(2));
    assert_eq!(s.f.ticket("SHOP-1").await.flags.messages_waiting, 2);
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert!(!s.output().contains("first"), "nothing typed while busy");

    s.idle.0.store(true, Ordering::SeqCst);
    assert_eq!(s.deliveries.deliver_if_ready(&s.session).await.unwrap(), 2);
    let out = s.output_with("second message\r\n").await;
    let first = out.find("first message").unwrap();
    let second = out.find("second message").unwrap();
    assert!(first < second, "in order: {out:?}");
    assert_eq!(s.waiting().await, 0);
    assert_eq!(s.last_count(), Some(0));
    assert_eq!(s.f.ticket("SHOP-1").await.flags.messages_waiting, 0);
    let _ = &s.ticket_id;
    s.sessions.shutdown().await;
}

#[tokio::test]
async fn held_while_the_line_has_unsent_text() {
    let s = setup().await;
    s.idle.0.store(true, Ordering::SeqCst);
    s.sessions.input(&s.session, b"my half-typed").unwrap();
    s.deliveries
        .queue_delivery(&s.session, DeliveryKind::AgentMessage, "from SHOP-18")
        .await
        .unwrap();
    assert_eq!(s.waiting().await, 1, "held: the user is typing");
    assert_eq!(s.deliveries.deliver_if_ready(&s.session).await.unwrap(), 0);

    // The user sends their line; now both conditions hold.
    s.sessions.input(&s.session, b"\r").unwrap();
    assert_eq!(s.deliveries.deliver_if_ready(&s.session).await.unwrap(), 1);
    s.output_with("from SHOP-18\r\n").await;
    s.sessions.shutdown().await;
}

#[tokio::test]
async fn delivered_at_once_when_idle_and_empty() {
    let s = setup().await;
    s.idle.0.store(true, Ordering::SeqCst);
    s.deliveries
        .queue_delivery(&s.session, DeliveryKind::ContinueAfterLimit, "continue")
        .await
        .unwrap();
    assert_eq!(s.waiting().await, 0);
    s.output_with("continue\r\n").await;
    s.sessions.shutdown().await;
}

#[tokio::test]
async fn deliver_now_sends_while_busy_but_asks_about_unsent_text() {
    let s = setup().await;
    s.deliveries
        .queue_delivery(&s.session, DeliveryKind::AgentMessage, "question one")
        .await
        .unwrap();
    // Busy agent, empty line: the click sends it.
    assert_eq!(
        s.deliveries.deliver_now(&s.session).await.unwrap(),
        Delivery::Sent { count: 1 }
    );
    s.output_with("question one\r\n").await;

    s.deliveries
        .queue_delivery(&s.session, DeliveryKind::AgentMessage, "question two")
        .await
        .unwrap();
    s.sessions.input(&s.session, b"draft").unwrap();
    let answer = s.deliveries.deliver_now(&s.session).await.unwrap();
    assert_eq!(
        answer,
        Delivery::NeedsConfirmation {
            ticket: Some("SHOP-1".parse().unwrap()),
            unclearable_by: None
        }
    );
    assert_eq!(s.waiting().await, 1, "nothing sent");

    // "Clear it and send": the line is cleared, then the waiting message goes.
    assert_eq!(
        s.deliveries.clear_and_send(&s.session, None).await.unwrap(),
        Delivery::Sent { count: 1 }
    );
    let out = s.output_with("question two\r\n").await;
    assert!(
        !out.contains("draftquestion two"),
        "the draft was cleared: {out:?}"
    );
    assert!(!s.sessions.has_unsent_input(&s.session).unwrap());
    assert_eq!(s.waiting().await, 0);
    s.sessions.shutdown().await;
}

#[tokio::test]
async fn clicks_are_never_held_but_never_clear_silently() {
    let s = setup().await;
    // Busy agent: a click still types at once.
    assert_eq!(
        s.deliveries
            .deliver_from_click(&s.session, "Send back: fix the tests")
            .await
            .unwrap(),
        Delivery::Sent { count: 1 }
    );
    s.output_with("Send back: fix the tests\r\n").await;

    s.sessions.input(&s.session, b"wip").unwrap();
    let answer = s
        .deliveries
        .deliver_from_click(&s.session, "needs-you answer")
        .await
        .unwrap();
    assert!(matches!(answer, Delivery::NeedsConfirmation { .. }));
    // "Keep it and cancel": nothing sent, the text stays.
    assert_eq!(s.deliveries.keep(&s.session), Delivery::Sent { count: 0 });
    assert!(s.sessions.has_unsent_input(&s.session).unwrap());
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert!(!s.output().contains("needs-you answer"));

    // "Clear it and send".
    s.deliveries
        .clear_and_send(&s.session, Some("needs-you answer"))
        .await
        .unwrap();
    s.output_with("needs-you answer\r\n").await;

    // Multi-line text arrives as one message.
    s.deliveries
        .deliver_from_click(&s.session, "line one\nline two")
        .await
        .unwrap();
    s.output_with("line two").await;
    s.sessions.shutdown().await;
}

#[tokio::test]
async fn deliver_now_over_the_socket() {
    use otaq_core::client::{Client, ClientConfig};
    use otaq_core::data_dir::DataDir;
    use otaq_core::protocol::{DeliverNowParams, Reply, Request, SessionTarget};
    use otaq_core::service::{Service, ServiceConfig};

    let tmp = tempfile::tempdir().unwrap();
    let data = DataDir::new(tmp.path().join("home"));
    let service = Service::bind(ServiceConfig::new(data.clone()))
        .await
        .unwrap();
    let (stopper, board, sessions, deliveries) = (
        service.stopper(),
        service.board(),
        service.sessions(),
        service.deliveries(),
    );
    let task = tokio::spawn(service.serve());
    let repos = otaq_core::git::testing::TempRepos::new();
    let repo = repos.repo("shop-web", 1);
    let project = board
        .add_project(&otaq_core::board::projects::AddProject {
            path: repo.display().to_string(),
            ..Default::default()
        })
        .await
        .unwrap();
    let t = board
        .create_ticket(
            &otaq_core::board::tickets::NewTicketInput {
                project: "SHOP".into(),
                title: "t".into(),
                ..Default::default()
            },
            &common::me(),
        )
        .await
        .unwrap();
    let info = sessions
        .spawn(SpawnSpec {
            kind: SessionKind::Agent,
            start: Start::User,
            project_id: project.id,
            project_key: "SHOP".into(),
            ticket: Some(SessionTicket {
                id: t.id,
                key: t.key,
            }),
            program: "cat".into(),
            args: vec![],
            cwd: repo,
            env: vec![],
            size: TerminalSize::default(),
            tool: None,
            tool_session_id: None,
            model: None,
            agent: None,
            effort: None,
            fallback: None,
            theme: None,
        })
        .await
        .unwrap();
    deliveries
        .queue_delivery(&info.id, DeliveryKind::AgentMessage, "held")
        .await
        .unwrap();
    let client = Client::connect(ClientConfig::new(&data)).await.unwrap();
    sessions.input(&info.id, b"typing").unwrap();
    let request = |clear_first| {
        Request::SessionDeliverNow(DeliverNowParams {
            target: SessionTarget {
                session: None,
                key: Some("SHOP-1".into()),
                ..Default::default()
            },
            clear_first,
        })
    };
    let Reply::Delivery(answer) = client.request(request(false)).await.unwrap() else {
        panic!()
    };
    assert!(matches!(answer, Delivery::NeedsConfirmation { .. }));
    let Reply::Delivery(answer) = client.request(request(true)).await.unwrap() else {
        panic!()
    };
    assert_eq!(answer, Delivery::Sent { count: 1 });
    stopper.stop();
    task.await.unwrap();
}
