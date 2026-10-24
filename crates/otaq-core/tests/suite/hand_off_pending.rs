//! "Waiting for the agent to pick it up" (T-0488; d0118, d0048, d0086, d0119): a hand-off typed into a live session
//! marks its ticket until the agent's tool reports activity. A hook that says where the turn is (the prompt submitted,
//! a turn ending) or an `otaq` command of the agent's own (a read too) picks it up; the status line's model and
//! `usage.record_limits`, and Claude Code's idle reminder, don't. A hand-off queued behind a busy agent ("1 message
//! waiting") isn't pending until it is typed. Send again types it once more as the user's click.
//!
//! Each agent is an `sh` (`TestService::agent_shell`) that ignores what it is typed, as an agent whose tool didn't
//! load the skill would; requests go over the socket from inside it, so each is judged as its caller. Hooks are the
//! tools' real-shaped JSON (`tests/fixtures/<tool>/hooks`). No test starts an agent tool.

use std::path::Path;
use std::time::Duration;

use otaq_core::agent_tools::Tool;
use otaq_core::board::tickets::Ticket;
use otaq_core::caller::Caller;
use otaq_core::client::{Client, ClientConfig};
use otaq_core::domain::{Actor, HandOffKind, Status};
use otaq_core::git::testing::commit;
use otaq_core::protocol::{
    DeliverNowParams, InputParams, Reply, Request, SessionTarget, encode_bytes,
};
use otaq_core::review::LocalMergeOutcome;
use otaq_core::service::testing::{AgentShell, TestService, said};
use otaq_core::session::deliveries::Delivery;
use otaq_core::session::hand_offs::HandOff;
use serde_json::{Value, json};

fn me() -> Actor {
    Actor::User("me".into())
}

/// A Working ticket with a commit of its own and its agent's session.
async fn working(w: &TestService, title: &str) -> (Ticket, AgentShell) {
    let t = w.create("SHOP", title).await;
    let worktree = w.start_worktree(&t).await;
    commit(&worktree, "login.ts", "redirect\n");
    w.force_status(&t, Status::Working).await;
    let shell = w.agent_shell(&t).await;
    (w.board.ticket(&t.key.to_string()).await.unwrap(), shell)
}

/// `t` submitted and sent back: Send back's line is typed into the live session.
async fn sent_back(w: &TestService, t: &Ticket) {
    let key = t.key.to_string();
    w.board
        .submit_for_review(
            &key,
            said(&w.board, &key, "Redirects after login.")
                .await
                .as_ref(),
            &me(),
        )
        .await
        .unwrap();
    w.review
        .fail(&key, "Keep the old URL working.", None, false, &me())
        .await
        .unwrap();
}

fn caller(t: &Ticket, shell: &AgentShell) -> Caller {
    Caller::Agent {
        ticket: t.key.clone(),
        ticket_id: t.id.clone(),
        session: shell.session.clone(),
    }
}

/// The ticket's pending hand-off, as its card reads it.
async fn pending(w: &TestService, t: &Ticket) -> Option<HandOffKind> {
    w.board
        .ticket(&t.key.to_string())
        .await
        .unwrap()
        .flags
        .hand_off
}

/// One request from inside the session.
async fn send(w: &TestService, shell: &AgentShell, method: &str, params: Value) {
    let request =
        json!({"protocol_version": 1, "id": 1, "request": {"method": method, "params": params}});
    let ran = shell
        .send_raw(&w.data.socket_path(), &request.to_string())
        .await;
    let answer: Value = serde_json::from_str(ran.stdout.lines().next().unwrap_or_default())
        .unwrap_or_else(|e| panic!("{e}: {ran:?}"));
    assert!(answer.get("error").is_none(), "{method}: {answer}");
}

/// A tool's hook in its real shape, from `tests/fixtures/<tool>/hooks/<file>.json` (OpenCode's `events`).
fn fixture(tool: &str, file: &str) -> Value {
    let folder = if tool == "opencode" {
        "events"
    } else {
        "hooks"
    };
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(tool)
        .join(folder)
        .join(format!("{file}.json"));
    serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap()
}

async fn hook(w: &TestService, who: &Caller, tool: Tool, event: &str, payload: &Value) {
    w.signals.hook(who, tool, event, payload).await.unwrap();
}

async fn eventually(what: &str, mut check: impl AsyncFnMut() -> bool) {
    tokio::time::timeout(Duration::from_secs(10), async {
        while !check().await {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("{what} never happened"));
}

fn output(w: &TestService, session: &str) -> String {
    String::from_utf8_lossy(&w.sessions.attach(session).unwrap().scrollback).into_owned()
}

/// The dogfood run's lost Send back (#19): typed, it waits for the agent. The status line (its model through `otaq
/// hook` and its `usage.record_limits`) and Claude Code's idle reminder leave it; the prompt submitted picks it up.
#[tokio::test(flavor = "multi_thread")]
async fn send_back_waits_for_the_agent_until_its_prompt_is_submitted() {
    let w = TestService::start().await;
    w.add_project("shop-web").await;
    let (t, shell) = working(&w, "Login redirect").await;
    sent_back(&w, &t).await;
    assert_eq!(w.status("SHOP-1").await, Status::Working);
    assert_eq!(pending(&w, &t).await, Some(HandOffKind::SendBack));

    // The status line's requests, from inside the session.
    send(
        &w,
        &shell,
        "usage.record_limits",
        json!({"tool": "claude", "windows": [
            {"key": "five_hour", "length_minutes": 300, "used_percent": 23.5},
        ]}),
    )
    .await;
    send(
        &w,
        &shell,
        "hook",
        json!({"tool": "claude", "event": "Status",
               "payload": {"hook_event_name": "Status", "model": {"id": "claude-opus-5-5"}}}),
    )
    .await;
    let who = caller(&t, &shell);
    hook(
        &w,
        &who,
        Tool::Claude,
        "Notification",
        &fixture("claude", "Notification-idle_prompt"),
    )
    .await;
    assert_eq!(
        pending(&w, &t).await,
        Some(HandOffKind::SendBack),
        "the status line and the idle reminder aren't the agent picking it up"
    );

    hook(
        &w,
        &who,
        Tool::Claude,
        "UserPromptSubmit",
        &fixture("claude", "UserPromptSubmit"),
    )
    .await;
    assert_eq!(pending(&w, &t).await, None);
    w.stop().await;
}

/// The agent running `otaq prime` itself (a read) picked the hand-off up.
#[tokio::test(flavor = "multi_thread")]
async fn the_agents_own_prime_picks_it_up() {
    let w = TestService::start().await;
    w.add_project("shop-web").await;
    let (t, shell) = working(&w, "Login redirect").await;
    sent_back(&w, &t).await;
    assert_eq!(pending(&w, &t).await, Some(HandOffKind::SendBack));
    send(&w, &shell, "ticket.prime", json!({"key": "SHOP-1"})).await;
    assert_eq!(pending(&w, &t).await, None);
    w.stop().await;
}

/// A merge conflict handed back while the agent is busy waits as "1 message waiting", not pending; once its turn
/// ends (its Stop through `otaq hook`) the restack line is typed and the ticket waits for the agent to pick it up.
#[tokio::test(flavor = "multi_thread")]
async fn a_conflict_queued_behind_a_busy_agent_is_pending_once_typed() {
    let w = TestService::start().await;
    w.add_project("shop-web").await;
    let (t, shell) = working(&w, "Pricing").await;
    let key = t.key.to_string();
    w.board
        .submit_for_review(&key, said(&w.board, &key, "Prices.").await.as_ref(), &me())
        .await
        .unwrap();
    let repo = w.board.project("SHOP").await.unwrap().repo_path;
    commit(Path::new(&repo), "login.ts", "main\n");
    let merged = w.review.local_merge(&key, "m", false, &me()).await.unwrap();
    assert!(
        matches!(merged, LocalMergeOutcome::Conflict { .. }),
        "{merged:?}"
    );
    let ticket = w.board.ticket(&key).await.unwrap();
    assert_eq!(ticket.status, Status::MergeConflictFixing);
    assert_eq!(ticket.flags.messages_waiting, 1);
    assert_eq!(ticket.flags.hand_off, None, "not typed yet");

    send(
        &w,
        &shell,
        "hook",
        json!({"tool": "claude", "event": "Stop", "payload": fixture("claude", "Stop-idle")}),
    )
    .await;
    eventually("the restack line typed", async || {
        pending(&w, &t).await == Some(HandOffKind::MergeConflict)
    })
    .await;
    eventually("the waiting message gone", async || {
        w.board.ticket(&key).await.unwrap().flags.messages_waiting == 0
    })
    .await;
    assert_eq!(pending(&w, &t).await, Some(HandOffKind::MergeConflict));
    w.stop().await;
}

/// Send again types the same line once more, and over unsent text in the terminal it asks first (d0048): Keep it
/// sends nothing, Clear it and send types it. The ticket keeps waiting for the agent throughout.
#[tokio::test(flavor = "multi_thread")]
async fn send_again_types_it_once_more_and_asks_over_unsent_text() {
    let w = TestService::start().await;
    w.add_project("shop-web").await;
    let (t, shell) = working(&w, "Login redirect").await;
    sent_back(&w, &t).await;
    let line = w
        .board
        .hand_offs()
        .pending(&t.id)
        .expect("pending")
        .hand_off
        .text;
    let typed = |w: &TestService| output(w, &shell.session).matches(line.as_str()).count();
    eventually("Send back typed", async || typed(&w) >= 1).await;
    let before = typed(&w);

    let client = Client::connect(ClientConfig::new(&w.data)).await.unwrap();
    let again = |clear_first| {
        Request::SessionSendAgain(DeliverNowParams {
            target: SessionTarget {
                key: Some("SHOP-1".into()),
                ..Default::default()
            },
            clear_first,
        })
    };
    assert_eq!(
        client.request(again(false)).await.unwrap(),
        Reply::Delivery(Delivery::Sent { count: 1 })
    );
    eventually("typed again", async || typed(&w) == before + 1).await;
    assert_eq!(pending(&w, &t).await, Some(HandOffKind::SendBack));

    // The user's half-typed line.
    client
        .request(Request::SessionInput(InputParams {
            session: shell.session.clone(),
            data: encode_bytes(b"also rename the"),
            reply: false,
        }))
        .await
        .unwrap();
    assert!(matches!(
        client.request(again(false)).await.unwrap(),
        Reply::Delivery(Delivery::NeedsConfirmation { .. })
    ));
    assert_eq!(pending(&w, &t).await, Some(HandOffKind::SendBack));
    assert_eq!(
        client.request(again(true)).await.unwrap(),
        Reply::Delivery(Delivery::Sent { count: 1 })
    );
    eventually("typed after the line was cleared", async || {
        typed(&w) == before + 2
    })
    .await;
    assert_eq!(pending(&w, &t).await, Some(HandOffKind::SendBack));

    // Once picked up, there is nothing to send again.
    hook(
        &w,
        &caller(&t, &shell),
        Tool::Claude,
        "UserPromptSubmit",
        &fixture("claude", "UserPromptSubmit"),
    )
    .await;
    assert_eq!(
        client.request(again(false)).await.unwrap(),
        Reply::Delivery(Delivery::Sent { count: 0 })
    );
    w.stop().await;
}

/// Every tool's own "the prompt was submitted" picks it up (d0086: waits come from each tool's hooks); OpenCode's
/// usage report doesn't.
#[tokio::test(flavor = "multi_thread")]
async fn each_tools_prompt_submitted_picks_it_up() {
    let w = TestService::start().await;
    w.add_project("shop-web").await;
    for (n, (tool, name, event, file)) in [
        (
            Tool::Claude,
            "claude",
            "UserPromptSubmit",
            "UserPromptSubmit",
        ),
        (Tool::Codex, "codex", "UserPromptSubmit", "UserPromptSubmit"),
        (
            Tool::Copilot,
            "copilot",
            "UserPromptSubmit",
            "UserPromptSubmit",
        ),
        (
            Tool::OpenCode,
            "opencode",
            "session.execution.started",
            "session.execution.started",
        ),
    ]
    .into_iter()
    .enumerate()
    {
        let (t, shell) = working(&w, &format!("Login redirect {n}")).await;
        sqlx::query("UPDATE sessions SET tool = ? WHERE id = ?")
            .bind(tool.as_str())
            .bind(&shell.session)
            .execute(w.board.store().pool())
            .await
            .unwrap();
        let typed = w
            .deliveries
            .hand_off(
                &shell.session,
                HandOff::new(HandOffKind::OpenPr, "/otaq-open-pr"),
                false,
            )
            .await
            .unwrap();
        assert_eq!(typed, Delivery::Sent { count: 1 }, "{tool:?}");
        assert_eq!(pending(&w, &t).await, Some(HandOffKind::OpenPr), "{tool:?}");
        let who = caller(&t, &shell);
        if tool == Tool::OpenCode {
            hook(
                &w,
                &who,
                tool,
                "session.usage.updated",
                &fixture(name, "session.usage.updated"),
            )
            .await;
            assert_eq!(pending(&w, &t).await, Some(HandOffKind::OpenPr), "usage");
        }
        hook(&w, &who, tool, event, &fixture(name, file)).await;
        assert_eq!(pending(&w, &t).await, None, "{tool:?}");
    }
    w.stop().await;
}
