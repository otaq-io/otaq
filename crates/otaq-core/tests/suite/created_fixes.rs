//! T-0542 (d0129): a ticket's agent fixes the never-started tickets it created, through the socket: it edits them,
//! links them (also to its own ticket), deletes them; each change is a History line on both tickets, a deleted one
//! on the creating ticket; `prime` lists them. Started ones, the user's and other agents' are refused.

use otaq_core::board::tickets::EventKind;
use otaq_core::domain::Status;
use otaq_core::service::testing::{AgentShell, TestService};
use serde_json::{Value, json};

async fn send(world: &TestService, agent: &AgentShell, method: &str, params: Value) -> Value {
    let request =
        json!({"protocol_version": 1, "id": 1, "request": {"method": method, "params": params}});
    let ran = agent
        .send_raw(&world.data.socket_path(), &request.to_string())
        .await;
    serde_json::from_str(ran.stdout.lines().next().unwrap_or_default())
        .unwrap_or_else(|e| panic!("{e}: {ran:?}"))
}

fn refused(answer: &Value) -> &str {
    assert_eq!(answer["error"]["kind"], "refused", "{answer}");
    answer["error"]["message"].as_str().unwrap()
}

fn ok(answer: &Value) {
    assert!(answer.get("reply").is_some(), "{answer}");
}

#[tokio::test(flavor = "multi_thread")]
async fn an_agent_fixes_the_never_started_tickets_it_created() {
    let w = TestService::start().await;
    w.add_project("shop-web").await;
    let shop1 = w.create("SHOP", "Checkout redesign").await;
    let users = w.create("SHOP", "Add wishlist").await;
    let agent = w.agent_shell(&shop1).await;
    for title in ["Cart page", "Payment page", "Receipt email", "Started one"] {
        ok(&send(
            &w,
            &agent,
            "ticket.create",
            json!({"project": "SHOP", "title": title}),
        )
        .await);
    }
    // SHOP-3..SHOP-6 are SHOP-1's; SHOP-6 has started.
    let started = w.board.ticket("SHOP-6").await.unwrap();
    w.force_status(&started, Status::Working).await;

    // Edit, link to its own ticket ("SHOP-3 waits for SHOP-1") and to another of its own, unlink.
    ok(&send(
        &w,
        &agent,
        "ticket.update",
        json!({"key": "SHOP-3", "title": "Cart page, new layout", "priority": 2}),
    )
    .await);
    ok(&send(
        &w,
        &agent,
        "dep.add",
        json!({"child": "SHOP-3", "parent": "SHOP-1"}),
    )
    .await);
    ok(&send(
        &w,
        &agent,
        "dep.add",
        json!({"child": "SHOP-4", "parent": "SHOP-3"}),
    )
    .await);
    ok(&send(
        &w,
        &agent,
        "dep.remove",
        json!({"child": "SHOP-4", "parent": "SHOP-3"}),
    )
    .await);
    let shop3 = w.board.ticket("SHOP-3").await.unwrap();
    assert_eq!(shop3.title, "Cart page, new layout");
    assert_eq!(shop3.depends_on, vec!["SHOP-1".parse().unwrap()]);
    assert!(
        w.board
            .ticket("SHOP-4")
            .await
            .unwrap()
            .depends_on
            .is_empty()
    );

    // Its prime lists what it may still change.
    let primed = send(&w, &agent, "ticket.prime", json!({"key": "SHOP-1"})).await;
    assert_eq!(
        primed["reply"]["prime"]["created_not_started"],
        json!(["SHOP-3", "SHOP-4", "SHOP-5"]),
        "{primed}"
    );
    let text = otaq_core::prime::render(
        &serde_json::from_value(primed["reply"]["prime"].clone()).unwrap(),
    );
    assert!(
        text.contains("Tickets you created, not started: SHOP-3, SHOP-4, SHOP-5."),
        "{text}"
    );

    // Delete, with its reason in the creating ticket's History.
    ok(&send(
        &w,
        &agent,
        "ticket.delete",
        json!({"key": "SHOP-5", "reason": "merged into SHOP-4"}),
    )
    .await);
    assert!(w.board.ticket("SHOP-5").await.is_err());

    // Refused: the started one, the user's, its own ticket's delete.
    let started_text = "SHOP-6 has started; SHOP-1's agent can only change tickets it created that haven't started";
    for (method, params) in [
        ("ticket.update", json!({"key": "SHOP-6", "title": "x"})),
        ("dep.add", json!({"child": "SHOP-6", "parent": "SHOP-1"})),
        ("ticket.delete", json!({"key": "SHOP-6"})),
    ] {
        assert_eq!(
            refused(&send(&w, &agent, method, params).await),
            started_text,
            "{method}"
        );
    }
    let users_text = format!(
        "{} was created by you; SHOP-1's agent can only change tickets it created",
        users.key
    );
    for (method, params) in [
        ("ticket.update", json!({"key": "SHOP-2", "title": "x"})),
        ("dep.add", json!({"child": "SHOP-2", "parent": "SHOP-1"})),
        ("ticket.delete", json!({"key": "SHOP-2"})),
    ] {
        assert_eq!(
            refused(&send(&w, &agent, method, params).await),
            users_text,
            "{method}"
        );
    }
    assert_eq!(
        refused(&send(&w, &agent, "ticket.delete", json!({"key": "SHOP-1"})).await),
        "SHOP-1's agent can't delete its own ticket; ask the user"
    );
    assert_eq!(
        w.board.ticket("SHOP-2").await.unwrap().title,
        "Add wishlist"
    );

    // Another agent's ticket is refused too.
    let other = w.agent_shell(&users).await;
    assert_eq!(
        refused(
            &send(
                &w,
                &other,
                "ticket.update",
                json!({"key": "SHOP-4", "title": "x"})
            )
            .await
        ),
        "SHOP-4 was created by SHOP-1's agent; SHOP-2's agent can only change tickets it created"
    );

    // History: on the creating ticket, one line per change; on the changed ticket, its own events by SHOP-1.
    let fixes: Vec<Value> = w
        .board
        .history("SHOP-1")
        .await
        .unwrap()
        .into_iter()
        .filter(|e| e.kind == EventKind::FixedCreatedTicket)
        .map(|e| e.payload)
        .rev()
        .collect();
    assert_eq!(
        fixes,
        vec![
            json!({"ticket": "SHOP-3", "change": "edited", "fields": ["title", "priority"]}),
            json!({"ticket": "SHOP-3", "change": "dependency_added", "parent": "SHOP-1"}),
            json!({"ticket": "SHOP-4", "change": "dependency_added", "parent": "SHOP-3"}),
            json!({"ticket": "SHOP-4", "change": "dependency_removed", "parent": "SHOP-3"}),
            json!({"ticket": "SHOP-5", "change": "deleted", "title": "Receipt email",
                   "reason": "merged into SHOP-4"}),
        ]
    );
    let on_target = w.board.history("SHOP-3").await.unwrap();
    for kind in [EventKind::Updated, EventKind::DependencyAdded] {
        let event = on_target.iter().find(|e| e.kind == kind).unwrap();
        assert_eq!(event.actor.to_string(), "agent:SHOP-1", "{kind:?}");
    }
    // The user's own edits and its own ticket's log nothing more.
    assert!(
        w.board
            .history("SHOP-2")
            .await
            .unwrap()
            .iter()
            .all(|e| e.kind != EventKind::FixedCreatedTicket)
    );
    w.stop().await;
}

/// A Plan edit by the agent that created the ticket is logged on the creating ticket as "Edited SHOP-2 · plan" (d0133).
#[tokio::test(flavor = "multi_thread")]
async fn a_plan_fix_is_logged_on_the_creating_ticket() {
    let w = TestService::start().await;
    w.add_project("shop-web").await;
    let shop1 = w.create("SHOP", "Checkout redesign").await;
    let agent = w.agent_shell(&shop1).await;
    ok(&send(
        &w,
        &agent,
        "ticket.create",
        json!({"project": "SHOP", "title": "Cart page"}),
    )
    .await);
    ok(&send(
        &w,
        &agent,
        "ticket.update",
        json!({"key": "SHOP-2", "plan": "## Pieces\n1. Cart API\n2. Cart page — waits for 1"}),
    )
    .await);
    let fixes: Vec<Value> = w
        .board
        .history("SHOP-1")
        .await
        .unwrap()
        .into_iter()
        .filter(|e| e.kind == EventKind::FixedCreatedTicket)
        .map(|e| e.payload)
        .collect();
    assert_eq!(
        fixes,
        [json!({"ticket": "SHOP-2", "change": "edited", "fields": ["plan"]})]
    );
    w.stop().await;
}
