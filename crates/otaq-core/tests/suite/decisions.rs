//! Recorded decisions (T-0276; DEC-01, DEC-02; d0084): `decision.record` stores a numbered decision on the ticket
//! with its History line and changes nothing else; an agent records on its own ticket only, a validator not at all;
//! `prime` lists the ticket's own.

use otaq_core::board::tickets::EventKind;
use otaq_core::decisions::{self, NewDecision, Outcome};
use otaq_core::domain::Actor;
use otaq_core::prime::render;
use otaq_core::service::testing::{AgentShell, TestService};
use otaq_core::session::SessionKind;
use serde_json::{Value, json};

fn split_name() -> NewDecision {
    NewDecision {
        question: "Split fullName everywhere, or only in the profile form?".into(),
        built: "only in the profile form".into(),
        other_options: vec!["everywhere".into()],
        why: "the description only mentions the profile form".into(),
    }
}

fn me() -> Actor {
    Actor::User("nevzat".into())
}

/// Sends one raw request from inside a session and returns the answer.
async fn send(w: &TestService, shell: &AgentShell, method: &str, params: Value) -> Value {
    let request =
        json!({"protocol_version": 1, "id": 1, "request": {"method": method, "params": params}});
    let ran = shell
        .send_raw(&w.data.socket_path(), &request.to_string())
        .await;
    serde_json::from_str(ran.stdout.lines().next().unwrap_or_default())
        .unwrap_or_else(|e| panic!("{e}: {ran:?}"))
}

fn decision_params(key: &str) -> Value {
    json!({
        "key": key,
        "question": "Order of saved items?",
        "built": "newest first",
        "other_options": ["in the order they were added"],
        "why": "most shops do it",
    })
}

#[tokio::test(flavor = "multi_thread")]
async fn record_numbers_per_ticket_logs_history_and_changes_nothing_else() {
    let w = TestService::start().await;
    w.add_project("shop-web").await;
    let shop1 = w.create("SHOP", "Split user full name").await;
    let shop2 = w.create("SHOP", "Add wishlist").await;
    let before = w.board.ticket("SHOP-1").await.unwrap();

    let first = decisions::record(&w.board, "SHOP-1", &split_name(), &me())
        .await
        .unwrap();
    assert_eq!(first.id.to_string(), "SHOP-1#1");
    assert_eq!(first.ticket_id, shop1.id);
    assert_eq!(
        first.question,
        "Split fullName everywhere, or only in the profile form?"
    );
    assert_eq!(first.built, "only in the profile form");
    assert_eq!(first.other_options, ["everywhere"]);
    assert_eq!(first.why, "the description only mentions the profile form");
    assert_eq!(first.recorded_by, me());
    assert_eq!(first.outcome, Outcome::Open);

    // Several other options; fields are trimmed and empty options dropped.
    let second = decisions::record(
        &w.board,
        "SHOP-1",
        &NewDecision {
            question: " Order of saved items? ".into(),
            built: "newest first".into(),
            other_options: vec!["oldest first".into(), " ".into(), "by price".into()],
            why: "w".into(),
        },
        &me(),
    )
    .await
    .unwrap();
    assert_eq!(second.id.to_string(), "SHOP-1#2");
    assert_eq!(second.question, "Order of saved items?");
    assert_eq!(second.other_options, ["oldest first", "by price"]);
    // Numbering is per ticket.
    let other = decisions::record(&w.board, "SHOP-2", &split_name(), &me())
        .await
        .unwrap();
    assert_eq!(other.id.to_string(), "SHOP-2#1");
    assert_eq!(other.ticket_id, shop2.id);

    let listed = decisions::list(&w.board, "SHOP-1").await.unwrap();
    let ids: Vec<String> = listed.iter().map(|d| d.id.to_string()).collect();
    assert_eq!(ids, ["SHOP-1#1", "SHOP-1#2"]);
    assert_eq!(decisions::get(&w.board, "SHOP-1#2").await.unwrap(), second);
    assert!(decisions::get(&w.board, "SHOP-1#3").await.is_err());
    assert!(decisions::get(&w.board, "SHOP-1").await.is_err());

    // The History line; the ticket's state and fields are as before.
    let history = w.board.history("SHOP-1").await.unwrap();
    let recorded: Vec<_> = history
        .iter()
        .filter(|e| e.kind == EventKind::DecisionRecorded)
        .collect();
    assert_eq!(recorded.len(), 2);
    assert_eq!(recorded[1].actor, me());
    assert_eq!(
        recorded[1].payload,
        json!({
            "ordinal": 1,
            "question": "Split fullName everywhere, or only in the profile form?",
            "built": "only in the profile form",
            "other_options": ["everywhere"],
            "why": "the description only mentions the profile form",
        })
    );
    let after = w.board.ticket("SHOP-1").await.unwrap();
    assert_eq!(
        (after.status, after.board_group()),
        (before.status, before.board_group())
    );
    assert_eq!(after.flags, before.flags);
    w.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_missing_field_is_refused_with_what_is_missing() {
    let w = TestService::start().await;
    w.add_project("shop-web").await;
    w.create("SHOP", "Split user full name").await;
    let missing_why = NewDecision {
        why: "  ".into(),
        ..split_name()
    };
    let error = decisions::record(&w.board, "SHOP-1", &missing_why, &me())
        .await
        .unwrap_err();
    assert_eq!(
        error.to_string(),
        "Recorded decisions need Question, Built, at least one Other option, and Why. Missing: Why."
    );
    let none = NewDecision::default();
    let error = decisions::record(&w.board, "SHOP-1", &none, &me())
        .await
        .unwrap_err();
    assert!(
        error
            .to_string()
            .ends_with("Missing: Question, Built, Other option, Why."),
        "{error}"
    );
    assert!(
        decisions::list(&w.board, "SHOP-1")
            .await
            .unwrap()
            .is_empty()
    );
    w.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn an_agent_records_on_its_own_ticket_only_and_a_validator_not_at_all() {
    let w = TestService::start().await;
    w.add_project("shop-web").await;
    let shop1 = w.create("SHOP", "Split user full name").await;
    w.create("SHOP", "Add wishlist").await;
    let agent = w.agent_shell(&shop1).await;

    let own = send(&w, &agent, "decision.record", decision_params("SHOP-1")).await;
    assert_eq!(own["reply"]["decision"]["id"], "SHOP-1#1", "{own}");
    assert_eq!(own["reply"]["decision"]["recorded_by"], "agent:SHOP-1");

    let other = send(&w, &agent, "decision.record", decision_params("SHOP-2")).await;
    assert_eq!(other["error"]["kind"], "refused", "{other}");
    assert_eq!(
        other["error"]["message"],
        "SHOP-1's agent can't change SHOP-2"
    );

    // Anyone may read them.
    let listed = send(&w, &agent, "decision.list", json!({"key": "SHOP-1"})).await;
    assert_eq!(listed["reply"]["decisions"][0]["id"], "SHOP-1#1");

    let validator = w.shell(&shop1, SessionKind::Validator).await;
    let refused = send(&w, &validator, "decision.record", decision_params("SHOP-1")).await;
    assert_eq!(refused["error"]["kind"], "refused", "{refused}");
    assert_eq!(
        refused["error"]["message"],
        "SHOP-1's check can only give its verdict"
    );
    assert_eq!(decisions::list(&w.board, "SHOP-1").await.unwrap().len(), 1);
    assert!(
        decisions::list(&w.board, "SHOP-2")
            .await
            .unwrap()
            .is_empty()
    );
    w.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn prime_lists_the_tickets_own_decisions() {
    let w = TestService::start().await;
    w.add_project("shop-web").await;
    w.create("SHOP", "Split user full name").await;
    w.create("SHOP", "Add wishlist").await;
    let context = w
        .board
        .prime_context("SHOP-1", w.adapters.for_new_session())
        .await
        .unwrap();
    assert!(context.decisions.is_empty());
    assert!(!render(&context).contains("Decisions you recorded"));

    decisions::record(&w.board, "SHOP-1", &split_name(), &me())
        .await
        .unwrap();
    decisions::record(&w.board, "SHOP-2", &split_name(), &me())
        .await
        .unwrap();
    let context = w
        .board
        .prime_context("SHOP-1", w.adapters.for_new_session())
        .await
        .unwrap();
    assert_eq!(context.decisions.len(), 1);
    let text = render(&context);
    assert!(
        text.contains(
            "\n## Decisions you recorded\n\nThe user sees these in the review; don't record them again.\n\n\
             - SHOP-1#1 Split fullName everywhere, or only in the profile form?\n  \
             Built: only in the profile form\n  Other option: everywhere\n  \
             Why: the description only mentions the profile form\n"
        ),
        "{text}"
    );
    assert!(!text.contains("SHOP-2#1"));
    w.stop().await;
}
