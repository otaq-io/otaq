//! The agent writing the delivery's message while the ticket waits in Review, on its own delivery session (the
//! delivery's agent isn't Dev's): when its tool stops at a permission prompt, the ticket in Review stays calm (no
//! Needs you), and Approve & merge / Approve & open PR are told so, with the writer's session for Answer in terminal.
//! The writer works only while the ticket waits: merged with the plain message, sent back or closed, its session
//! stops, the Running list no longer names the ticket, and no worktree is left. An `sh` stands in for the agent.
#![cfg(unix)]

use otaq_core::agent_tools::claude::ClaudeLaunch;
use otaq_core::agent_tools::{LaunchContext, Tool, ToolEvent};
use otaq_core::board::projects::AddProject;
use otaq_core::board::tickets::{NewTicketInput, Ticket};
use otaq_core::caller::Caller;
use otaq_core::client::{Client, ClientConfig};
use otaq_core::domain::{Actor, Status, ValidationTrigger};
use otaq_core::git::testing::commit;
use otaq_core::pr::message::MessageState;
use otaq_core::protocol::{InputParams, KeyParams, Reply, Request, encode_bytes};
use otaq_core::service::ServiceConfig;
use otaq_core::service::testing::{TestService, said};
use otaq_core::session::{SessionInfo, SessionKind};
use std::time::Duration;

fn me() -> Actor {
    Actor::User("me".into())
}

async fn world() -> TestService {
    TestService::start_with(|config| ServiceConfig {
        launch: Some(LaunchContext {
            bin_dir: "/nonexistent".into(),
            plugin_dir: "/nonexistent/plugin".into(),
            path: std::env::var_os("PATH").unwrap_or_default(),
            claude: ClaudeLaunch {
                program: vec![
                    "sh".into(),
                    "-c".into(),
                    "exec sleep 60".into(),
                    "sh".into(),
                ],
                config_dir: None,
            },
        }),
        ..config
    })
    .await
}

struct Setup {
    w: TestService,
    ticket: Ticket,
    /// The writer's own delivery session.
    writer: SessionInfo,
}

/// SHOP-1 waiting in Review with one commit; Checker (not Dev's Developer) is the delivery's agent and writes the
/// message in a delivery session of its own.
async fn setup() -> Setup {
    let w = world().await;
    let repo = w.repos.clone_with_origin("shop-web", "main", 1);
    w.board
        .add_project(&AddProject {
            path: repo.display().to_string(),
            ..Default::default()
        })
        .await
        .unwrap();
    w.board
        .edit_flow_for_tests("SHOP", |file| {
            file.flow.delivery.agent = Some("Checker".into());
        })
        .await;
    let t = w
        .board
        .create_ticket(
            &NewTicketInput {
                project: "SHOP".into(),
                title: "Checkout timeout".into(),
                ..Default::default()
            },
            &me(),
        )
        .await
        .unwrap();
    let worktree = w.start_worktree(&t).await;
    commit(&worktree, "api.ts", "retry\n");
    w.force_status(&t, Status::Working).await;
    let key = t.key.to_string();
    w.validation
        .submit(
            &key,
            said(&w.board, &key, "done").await.as_ref(),
            &Actor::Agent(t.key.clone()),
            ValidationTrigger::Agent,
        )
        .await
        .unwrap();
    let validator = w
        .sessions
        .running_of(&t.id, SessionKind::Validator)
        .unwrap()
        .id;
    w.validation
        .verdict(
            &key,
            true,
            said(&w.board, &key, "ok").await.as_ref(),
            &validator,
        )
        .await
        .unwrap();
    let ticket = w.board.ticket(&key).await.unwrap();
    assert_eq!(ticket.status, Status::WaitingForYou);
    if w.sessions
        .running_of(&ticket.id, SessionKind::Delivery)
        .is_none()
    {
        assert!(w.prs.prepare_message(&ticket.id).await.unwrap());
    }
    let writer = w
        .sessions
        .running_of(&ticket.id, SessionKind::Delivery)
        .expect("the writer's own delivery session");
    let message = w.board.delivery_message(&ticket.id).await.unwrap().unwrap();
    assert_eq!(message.state, MessageState::Writing);
    assert_eq!(message.agent, "Checker");
    Setup { w, ticket, writer }
}

impl Setup {
    fn caller(&self) -> Caller {
        Caller::Agent {
            ticket: self.ticket.key.clone(),
            ticket_id: self.ticket.id.clone(),
            session: self.writer.id.clone(),
        }
    }

    /// The writer's tool stops at a permission prompt.
    async fn asks(&self) {
        self.w
            .signals
            .apply_events(
                &self.caller(),
                Tool::Claude,
                &[ToolEvent::NeedsYou {
                    prompt: "Claude needs your permission to use Bash".into(),
                    helper_agent: false,
                }],
            )
            .await
            .unwrap();
    }

    async fn client(&self) -> Client {
        Client::connect(ClientConfig::new(&self.w.data))
            .await
            .unwrap()
    }

    /// `ticket.merge_proposal`, as Approve & merge asks it.
    async fn proposal(&self) -> otaq_core::board::merge::MergeProposal {
        let key = KeyParams {
            key: self.ticket.key.to_string(),
        };
        match self
            .client()
            .await
            .request(Request::TicketMergeProposal(key))
            .await
            .unwrap()
        {
            Reply::MergeProposal(p) => p,
            other => panic!("{other:?}"),
        }
    }

    /// `ticket.pr_flow`, as Approve & open PR asks it.
    async fn pr_flow(&self) -> otaq_core::pr::flow::PrFlowView {
        let key = KeyParams {
            key: self.ticket.key.to_string(),
        };
        match self
            .client()
            .await
            .request(Request::TicketPrFlow(key))
            .await
            .unwrap()
        {
            Reply::PrFlow(v) => *v,
            other => panic!("{other:?}"),
        }
    }

    async fn running_keys(&self) -> Vec<String> {
        otaq_core::usage::activity::working(self.w.board.store())
            .await
            .unwrap()
            .into_iter()
            .map(|a| a.key)
            .collect()
    }

    /// Waits until the writer's session has ended.
    async fn writer_ends(&self) {
        let ended = tokio::time::timeout(Duration::from_secs(20), async {
            while self.w.sessions.info(&self.writer.id).unwrap().is_running() {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await;
        assert!(ended.is_ok(), "the writer's session stops");
    }
}

/// A writer waiting on a prompt keeps Review calm and shows in Approve's dialogs with its session; any other signal of
/// its tool (here its turn going on) clears it.
#[tokio::test(flavor = "multi_thread")]
async fn a_writer_waiting_for_permission_shows_in_the_approve_dialogs_only() {
    let s = setup().await;
    assert_eq!(s.proposal().await.asks_in, None, "still writing, no prompt");

    let review_items = || async {
        s.w.board
            .needs_you_items(0)
            .await
            .unwrap()
            .into_iter()
            .filter(|i| i.key == s.ticket.key)
            .map(|i| (i.group, i.status, i.reason))
            .collect::<Vec<_>>()
    };
    let calm = review_items().await;

    s.asks().await;
    assert_eq!(
        s.w.signals.open_prompt(&s.writer.id).as_deref(),
        Some("Claude needs your permission to use Bash")
    );
    // Review stays calm: the ticket keeps its status and its usual Review item, and nothing else is added.
    let ticket = s.w.board.ticket(&s.ticket.key.to_string()).await.unwrap();
    assert_eq!(ticket.status, Status::WaitingForYou);
    assert_eq!(review_items().await, calm);
    // Approve & merge and Approve & open PR are told, with the session whose terminal answers it.
    let proposal = s.proposal().await;
    assert_eq!(proposal.writing.as_deref(), Some("Checker"));
    assert_eq!(proposal.asks_in.as_deref(), Some(s.writer.id.as_str()));
    assert!(proposal.message.starts_with("SHOP-1: Checkout timeout"));
    assert_eq!(
        s.pr_flow().await.asks_in.as_deref(),
        Some(s.writer.id.as_str())
    );

    // Answered: the tool goes on, and the dialogs say it still writes.
    s.w.signals
        .apply_events(
            &s.caller(),
            Tool::Claude,
            &[ToolEvent::NewTurn { prompt: None }],
        )
        .await
        .unwrap();
    assert_eq!(s.w.signals.open_prompt(&s.writer.id), None);
    assert_eq!(s.proposal().await.asks_in, None);
    assert_eq!(s.pr_flow().await.asks_in, None);

    // Asked again and answered in its terminal (Answer in terminal): the user's key there closes the prompt, though
    // the tool may send nothing until its turn ends. An arrow key (an escape sequence) doesn't.
    s.asks().await;
    let key = |bytes: &'static [u8]| {
        Request::SessionInput(InputParams {
            session: s.writer.id.clone(),
            data: encode_bytes(bytes),
            reply: false,
            if_empty: false,
        })
    };
    let client = s.client().await;
    client.request(key(b"\x1b[B")).await.unwrap();
    assert!(s.w.signals.open_prompt(&s.writer.id).is_some());
    client.request(key(b"y")).await.unwrap();
    assert_eq!(s.w.signals.open_prompt(&s.writer.id), None);
    assert_eq!(s.proposal().await.asks_in, None);
    s.w.stop().await;
}

/// Merging with the plain message while the writer waits on its prompt ends its session: Running no longer lists the
/// ticket and its worktree is removed.
#[tokio::test(flavor = "multi_thread")]
async fn merging_with_the_plain_message_ends_the_writer_and_leaves_no_worktree() {
    let s = setup().await;
    s.asks().await;
    assert!(s.running_keys().await.contains(&"SHOP-1".to_owned()));
    let worktree = s.ticket.work.as_ref().unwrap().worktree.clone().unwrap();

    let plain = s.proposal().await;
    let merged =
        s.w.review
            .local_merge("SHOP-1", &plain.message, true, &me())
            .await
            .unwrap();
    assert!(
        matches!(merged, otaq_core::review::LocalMergeOutcome::Merged { .. }),
        "{merged:?}"
    );
    assert!(
        !s.w.sessions.info(&s.writer.id).unwrap().is_running(),
        "the writer stops with the merge"
    );
    assert!(
        !s.running_keys().await.contains(&"SHOP-1".to_owned()),
        "Running no longer lists it"
    );
    assert!(
        !std::path::Path::new(&worktree).exists(),
        "no worktree is left"
    );
    s.w.stop().await;
}

/// Sent back to Dev, the ticket no longer waits to be delivered: the writer's session stops (Dev's agent does the
/// work).
#[tokio::test(flavor = "multi_thread")]
async fn sending_the_ticket_back_ends_the_writer() {
    let s = setup().await;
    s.asks().await;
    let fail = Request::TicketFail(otaq_core::protocol::FailParams {
        key: "SHOP-1".into(),
        note: "Keep the old timeout.".into(),
        clear_first: true,
        attachments: Vec::new(),
    });
    s.client().await.request(fail).await.unwrap();
    s.writer_ends().await;
    let ticket = s.w.board.ticket("SHOP-1").await.unwrap();
    assert_ne!(ticket.status, Status::WaitingForYou);
    s.w.stop().await;
}

/// Closed without merging: the writer's session stops with the Done cleanup.
#[tokio::test(flavor = "multi_thread")]
async fn closing_the_ticket_ends_the_writer() {
    let s = setup().await;
    let close = Request::TicketClose(otaq_core::protocol::CloseParams {
        key: "SHOP-1".into(),
        confirm: true,
    });
    s.client().await.request(close).await.unwrap();
    s.writer_ends().await;
    s.w.stop().await;
}

impl Setup {
    /// The writer was asked for the branch's head by an earlier run of Otaq (an update with the ticket in Review):
    /// the request is recorded, "writing", and no session writes it.
    async fn leftover_request(&self) {
        self.w.sessions.stop(&self.writer.id).await.unwrap();
        self.writer_ends().await;
        let head = otaq_core::git::testing::git(&self.worktree(), &["rev-parse", "HEAD"]);
        let head = head.trim().to_owned();
        sqlx::query(
            "INSERT OR REPLACE INTO delivery_messages
                 (ticket_id, state, agent, head, title, description, branch, updated_at)
             VALUES (?, 'writing', 'Checker', ?, '', '', NULL, 0)",
        )
        .bind(&self.ticket.id)
        .bind(head)
        .execute(self.w.board.store().pool())
        .await
        .unwrap();
        assert!(
            [SessionKind::Delivery, SessionKind::Agent]
                .into_iter()
                .all(|kind| self.w.sessions.running_of(&self.ticket.id, kind).is_none())
        );
    }

    fn worktree(&self) -> std::path::PathBuf {
        self.ticket
            .work
            .as_ref()
            .unwrap()
            .worktree
            .clone()
            .unwrap()
            .into()
    }
}

/// A writer asked by an earlier run of Otaq, with no session writing any more: opening Approve & merge asks it again,
/// though the branch head is the same, so the dialog never says it writes when no one does. A writer that runs is
/// left alone.
#[tokio::test(flavor = "multi_thread")]
async fn approve_asks_a_writer_that_no_longer_runs_again() {
    let s = setup().await;
    s.leftover_request().await;

    let proposal = s.proposal().await;
    assert_eq!(proposal.writing.as_deref(), Some("Checker"));
    let again =
        s.w.sessions
            .running_of(&s.ticket.id, SessionKind::Delivery)
            .expect("the writer is asked again in a new session");
    assert_ne!(again.id, s.writer.id);
    let message =
        s.w.board
            .delivery_message(&s.ticket.id)
            .await
            .unwrap()
            .unwrap();
    assert_eq!(message.state, MessageState::Writing);

    // Approve & open PR on the running writer asks nobody new.
    assert_eq!(
        s.pr_flow().await.message.map(|m| m.state),
        Some(MessageState::Writing)
    );
    assert_eq!(
        s.w.sessions
            .running_of(&s.ticket.id, SessionKind::Delivery)
            .unwrap()
            .id,
        again.id
    );
    s.w.stop().await;
}

/// A writer asked by an earlier run that can't be asked again (here its project is paused): the dialogs open on the
/// plain message and say nobody is writing it.
#[tokio::test(flavor = "multi_thread")]
async fn approve_offers_the_plain_message_when_the_writer_cant_be_asked_again() {
    let s = setup().await;
    s.w.pause
        .pause(&otaq_core::agents::pause::PauseScope::Project {
            project: "SHOP".into(),
        })
        .await
        .unwrap();
    s.leftover_request().await;

    let proposal = s.proposal().await;
    assert_eq!((proposal.writing, proposal.prepared_by), (None, None));
    assert!(proposal.message.starts_with("SHOP-1: Checkout timeout"));
    assert_eq!(s.pr_flow().await.message, None);
    assert!(
        s.w.sessions
            .running_of(&s.ticket.id, SessionKind::Delivery)
            .is_none()
    );
    s.w.stop().await;
}
