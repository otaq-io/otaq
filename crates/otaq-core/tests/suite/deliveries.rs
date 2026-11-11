//! Automatic and click deliveries (T-0054) with an injected "idle" signal and `cat` in a PTY in place of
//! `claude`: held while busy or while the line has unsent text, delivered when both clear, Deliver now, and the
//! Keep / Clear prompt outcomes.

use crate::common;

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use crate::common::Fixture;
use otaq_core::agent_tools::{Adapters, Tool};
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
    setup_with("cat", vec![], TerminalSize::default()).await
}

async fn setup_with(program: &str, args: Vec<String>, size: TerminalSize) -> Setup {
    setup_on(None, program, args, size).await
}

/// `program` as a session on `tool` (`None`: a new session's tool).
async fn setup_on(
    tool: Option<Tool>,
    program: &str,
    args: Vec<String>,
    size: TerminalSize,
) -> Setup {
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
            program: program.into(),
            args,
            cwd: f.repos.root(),
            env: vec![],
            size,
            tool,
            tool_session_id: None,
            model: None,
            agent: None,
            effort: None,
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

/// Two callers at once (a hook's delivery and a queued one's) type each waiting message once: the second finds the
/// queue empty. `cat` echoes a typed line and then prints it, so a message typed once shows up twice.
#[tokio::test(flavor = "multi_thread")]
async fn two_deliveries_at_once_type_each_message_once() {
    let s = setup().await;
    for text in ["first message", "second message"] {
        s.deliveries
            .queue_delivery(&s.session, DeliveryKind::AgentMessage, text)
            .await
            .unwrap();
    }
    s.idle.0.store(true, Ordering::SeqCst);
    let (a, b) = tokio::join!(
        s.deliveries.deliver_if_ready(&s.session),
        s.deliveries.deliver_if_ready(&s.session)
    );
    assert_eq!(
        a.unwrap() + b.unwrap(),
        2,
        "each message sent by one caller"
    );
    s.output_with("second message\r\n").await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    let out = s.output();
    for text in ["first message", "second message"] {
        assert_eq!(out.matches(text).count(), 2, "{text:?} typed once: {out:?}");
    }
    assert_eq!(s.waiting().await, 0);
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

/// A session that shows a tool's screen as it was captured in Windows' ConPTY (`tests/fixtures/input-line`) and
/// then reads nothing and echoes nothing, at the capture's size.
async fn showing(capture: &str, shown: &str) -> Setup {
    let path = format!(
        "{}/tests/fixtures/input-line/{capture}.bin",
        env!("CARGO_MANIFEST_DIR")
    );
    let script = r#"stty -echo 2>/dev/null; cat "$1"; exec sleep 60"#;
    let s = setup_with(
        "sh",
        vec!["-c".into(), script.into(), "sh".into(), path],
        TerminalSize {
            cols: 120,
            rows: 30,
        },
    )
    .await;
    s.output_with(shown).await;
    s
}

/// The release test's case: Codex asked "Would you like to run the following command?", the user pressed `y` in
/// its terminal, and Codex went back to its empty input line ("› Ask Codex to do anything", dim). The key counts as
/// typed, but the screen shows only the placeholder, so a told comment reaches the idle agent by itself.
#[tokio::test]
async fn a_told_comment_reaches_an_idle_codex_after_a_key_its_menu_took() {
    let s = showing("codex-after-approval", "Ask Codex to do anything").await;
    s.sessions.input(&s.session, b"y").unwrap();
    assert!(
        s.sessions.has_unsent_input(&s.session).unwrap(),
        "just typed: the screen may not show the key yet"
    );
    tokio::time::sleep(Duration::from_millis(700)).await;
    assert!(!s.sessions.has_unsent_input(&s.session).unwrap());
    assert!(!s.sessions.has_multi_line_input(&s.session).unwrap());

    s.idle.0.store(true, Ordering::SeqCst);
    s.deliveries
        .queue_delivery(
            &s.session,
            DeliveryKind::AgentMessage,
            "The user added comment 3. Run 'otaq prime' to read it.",
        )
        .await
        .unwrap();
    assert_eq!(s.waiting().await, 0, "delivered without Deliver now");
    s.sessions.shutdown().await;
}

/// Text the screen shows in the input line still holds the note.
#[tokio::test]
async fn text_codex_shows_in_its_input_line_still_holds_the_note() {
    let s = showing("codex-typed", "abc").await;
    s.sessions.input(&s.session, b"abc").unwrap();
    tokio::time::sleep(Duration::from_millis(700)).await;
    assert!(s.sessions.has_unsent_input(&s.session).unwrap());
    s.idle.0.store(true, Ordering::SeqCst);
    s.deliveries
        .queue_delivery(&s.session, DeliveryKind::AgentMessage, "from SHOP-18")
        .await
        .unwrap();
    assert_eq!(s.waiting().await, 1, "held: the user's text is there");
    s.sessions.shutdown().await;
}

/// The bytes Otaq types for a message, per tool, as the program reads them: `sh` with raw input running `cat -v`,
/// which shows Esc as `^[` (the terminal turns Enter into a line end). Codex gets the text, a space in the same
/// write and (on Windows) End, then one Enter: a skill typed alone (`$otaq:push`) leaves Codex's skill popup open
/// and Enter there only inserts the item, while text after the name closes it first (real Codex 0.166.0 in Windows'
/// ConPTY; `codex_typing` runs it against the real tool). A multi-line message is a bracketed paste with the space
/// after it. Claude Code gets the text and Enter alone.
#[tokio::test]
async fn the_bytes_typed_for_each_tool() {
    let end = if cfg!(windows) { "^[[F" } else { "" };
    let cases = [
        (Tool::Codex, "$otaq:push", format!("$otaq:push {end}\r\n")),
        (
            Tool::Codex,
            "line one\nline two",
            format!("^[[200~line one\r\nline two^[[201~ {end}\r\n"),
        ),
        (Tool::Claude, "/otaq:push", "/otaq:push\r\n".to_owned()),
        (
            Tool::Claude,
            "line one\nline two",
            "^[[200~line one\r\nline two^[[201~\r\n".to_owned(),
        ),
    ];
    for (tool, text, read) in cases {
        let script = "stty -icanon -echo; echo 'REA''DY'; exec cat -v";
        let s = setup_on(
            Some(tool),
            "sh",
            vec!["-c".into(), script.into()],
            TerminalSize::default(),
        )
        .await;
        s.output_with("READY").await;
        s.idle.0.store(true, Ordering::SeqCst);
        s.deliveries
            .queue_delivery(&s.session, DeliveryKind::AgentMessage, text)
            .await
            .unwrap();
        assert_eq!(s.waiting().await, 0, "{tool:?}: typed at once");
        let out = s.output_with(&read).await;
        // Nothing after the one Enter.
        tokio::time::sleep(Duration::from_millis(300)).await;
        let after = s.output();
        let typed = &after[after.find("READY").unwrap() + "READY".len()..];
        assert_eq!(
            typed.trim_start_matches(['\r', '\n']),
            read,
            "{tool:?}: {out:?}"
        );
        s.sessions.shutdown().await;
    }
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

/// While a note waits for a busy agent, the Session tab is told each time the input line fills or empties under a
/// person's keys, and only then: its "Unsent text in the input line" follows the line, not only the notes.
#[tokio::test]
async fn a_waiting_note_tells_the_tab_when_the_line_fills_or_empties() {
    let mut s = setup().await;
    s.deliveries.line_settled(&s.session, false).await.unwrap();
    assert_eq!(s.last_count(), None, "nothing waits");
    s.deliveries
        .queue_delivery(&s.session, DeliveryKind::AgentMessage, "from SHOP-18")
        .await
        .unwrap();
    assert_eq!(s.last_count(), Some(1));
    s.deliveries.line_settled(&s.session, true).await.unwrap();
    assert_eq!(s.last_count(), Some(1), "the line filled");
    s.deliveries.line_settled(&s.session, true).await.unwrap();
    assert_eq!(s.last_count(), None, "no change");
    s.deliveries.line_settled(&s.session, false).await.unwrap();
    assert_eq!(s.last_count(), Some(1), "the line emptied");
    assert_eq!(s.waiting().await, 1, "busy: the note still waits");
    s.sessions.shutdown().await;
}

async fn wait_until(what: &str, check: impl Fn() -> bool) {
    tokio::time::timeout(Duration::from_secs(10), async {
        while !check() {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("timed out waiting: {what}"));
}

/// Re-test 5's footer: "Unsent text in the input line · Otaq's note waits until it's empty" showed over Codex's
/// empty line. `session.unsent`, which the Session tab's line goes by, is the same answer the notes wait by: a `y` a
/// prompt took as it closed leaves the line empty, so the waiting note is typed and the line reads empty; a `y` typed
/// after the prompt closed is text in the line until it is cleared, and then the note goes and the line reads empty
/// again. When the answer changes with a note waiting, `messages_waiting` goes out so the tab looks again.
#[tokio::test(flavor = "multi_thread")]
async fn a_key_a_closing_prompt_took_leaves_no_unsent_text_and_the_note_goes() {
    use otaq_core::caller::Caller;
    use otaq_core::client::{Client, ClientConfig};
    use otaq_core::protocol::{InputParams, Reply, Request, SessionIdParams, encode_bytes};
    use otaq_core::service::testing::TestService;

    let w = TestService::start().await;
    w.add_project("shop-web").await;
    let t = w.create("SHOP", "Split user full name").await;
    let agent = w.agent_shell(&t).await;
    let mut events = w.events.subscribe();
    let shown = || {
        String::from_utf8_lossy(&w.sessions.attach(&agent.session).unwrap().scrollback).into_owned()
    };
    // The row the cursor is on, as the screen shows it.
    let input_row = || {
        let mut screen = vt100::Parser::new(24, 80, 0);
        screen.process(shown().as_bytes());
        let (row, _) = screen.screen().cursor_position();
        screen
            .screen()
            .rows(0, 80)
            .nth(usize::from(row))
            .unwrap_or_default()
    };
    // An agent tool's input line starts with its prompt marker; its turn has ended.
    w.sessions.write(&agent.session, b"PS1='> '\r").unwrap();
    wait_until("the prompt marker shows", || input_row().trim_end() == ">").await;
    let caller = Caller::Agent {
        ticket: t.key.clone(),
        ticket_id: t.id.clone(),
        session: agent.session.clone(),
    };
    let turn_ends = async || {
        w.signals
            .hook(
                &caller,
                Tool::Claude,
                "Stop",
                &serde_json::json!({"background_tasks": [], "session_crons": []}),
            )
            .await
            .unwrap();
    };
    turn_ends().await;
    assert!(w.deliveries.is_ready(&agent.session).unwrap());

    let client = Client::connect(ClientConfig::new(&w.data)).await.unwrap();
    let typing = |bytes: &[u8]| {
        Request::SessionInput(InputParams {
            session: agent.session.clone(),
            data: encode_bytes(bytes),
            reply: false,
            if_empty: false,
        })
    };
    let unsent_request = || {
        Request::SessionUnsent(SessionIdParams {
            session: agent.session.clone(),
        })
    };
    let unsent = |reply: Reply| match reply {
        Reply::Unsent(unsent) => unsent,
        other => panic!("{other:?}"),
    };
    let mut waiting_sent = || {
        let mut sent = false;
        while let Ok(event) = events.try_recv() {
            sent |= matches!(event, Event::MessagesWaiting { .. });
        }
        sent
    };
    let pending = async || w.deliveries.pending(&agent.session).await.unwrap().len();

    // A prompt that takes one key without showing it, then closes back to the empty line.
    w.sessions
        .write(
            &agent.session,
            b"stty -echo -icanon min 1; dd bs=1 count=1 >/dev/null 2>&1; stty echo icanon\r",
        )
        .unwrap();
    wait_until("the prompt is open", || input_row().trim_end().is_empty()).await;
    client.request(typing(b"y")).await.unwrap();
    // Just typed, the screen may not show what became of the key yet: a note waits.
    let note = "The user added comment 2. Run otaq prime to read it.";
    w.deliveries
        .queue_delivery(&agent.session, DeliveryKind::AgentMessage, note)
        .await
        .unwrap();
    assert_eq!(pending().await, 1);
    // Once the program has drawn its empty line the note is typed, and the line reads empty.
    wait_until("the note is typed", || shown().contains(note)).await;
    wait_until("the empty line shows again", || {
        input_row().trim_end() == ">"
    })
    .await;
    assert_eq!(pending().await, 0);
    assert!(
        !unsent(client.request(unsent_request()).await.unwrap()),
        "the line is empty"
    );

    // The turn the note started ends. A `y` typed after the prompt closed is text in the line: a note waits behind
    // it, and the line says so.
    turn_ends().await;
    waiting_sent();
    client.request(typing(b"y")).await.unwrap();
    wait_until("the key shows", || input_row().trim_end() == "> y").await;
    tokio::time::sleep(otaq_core::session::KEY_SETTLE * 2).await;
    assert!(unsent(client.request(unsent_request()).await.unwrap()));
    let again = "The user added comment 3. Run otaq prime to read it.";
    w.deliveries
        .queue_delivery(&agent.session, DeliveryKind::AgentMessage, again)
        .await
        .unwrap();
    assert_eq!(pending().await, 1);

    // Cleared: the note goes, and the line reads empty again.
    client.request(typing(&[0x7f])).await.unwrap();
    wait_until("the note is typed", || shown().contains(again)).await;
    wait_until("the empty line shows again", || {
        input_row().trim_end() == ">"
    })
    .await;
    assert!(
        !unsent(client.request(unsent_request()).await.unwrap()),
        "the line is empty"
    );
    assert!(waiting_sent(), "the tab was told the note went");
    drop(client);
    w.stop().await;
}
