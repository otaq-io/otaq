//! Who does the work (UX-65, UX-70; SCH-04): the holds' store, and "Noyan starts on" (an app setting, the user's own
//! requests: checked, it changes the model the next Noyan conversation starts on).

use otaq_core::agent_tools::Tool;
use otaq_core::agents::store::HoldScope;
use otaq_core::agents::{AgentHold, AgentModel, HoldReason, NoyanStartsOn, SessionRole, resolve};
use otaq_core::client::{Client, ClientConfig, ClientError};
use otaq_core::protocol::{Event, Reply, Request};
use otaq_core::service::testing::TestService;
use serde_json::{Value, json};

use crate::common::Fixture;

/// One request over the socket, as the app sends it.
async fn ask(client: &Client, method: &str, params: Value) -> Result<Reply, ClientError> {
    let request: Request = serde_json::from_value(json!({"method": method, "params": params}))
        .unwrap_or_else(|e| panic!("{method}: {e}"));
    client.request(request).await
}

/// UX-70, UX-74: "Noyan starts on" is `default` until set; the user sets it (a model that reads, an effort of its
/// tool), which changes the next Noyan conversation's model and publishes `agents_changed`; a bad model or effort is
/// refused; a ticket's agent is refused setting it.
#[tokio::test(flavor = "multi_thread")]
async fn noyan_starts_on_is_the_users_setting() {
    let w = TestService::start().await;
    w.add_project("shop-web").await;
    let ticket = w.create("SHOP", "Price table").await;
    let client = Client::connect(ClientConfig::new(&w.data)).await.unwrap();
    let mut events = client.subscribe().await.unwrap();

    let Reply::NoyanStartsOn(now) = client.request(Request::NoyanStartsOn).await.unwrap() else {
        panic!("noyan.starts_on")
    };
    assert_eq!(*now, NoyanStartsOn::default());
    assert_eq!(
        resolve::noyan_who(w.board.store()).await.unwrap().model,
        AgentModel::Default
    );

    let Reply::NoyanStartsOn(set) = ask(
        &client,
        "noyan.set_starts_on",
        json!({"model": "codex:gpt-5.5", "effort": "high"}),
    )
    .await
    .unwrap() else {
        panic!("noyan.set_starts_on")
    };
    assert_eq!(set.model.as_deref(), Some("codex:gpt-5.5"));
    loop {
        if events.recv().await.unwrap() == Event::AgentsChanged {
            break;
        }
    }
    let who = resolve::noyan_who(w.board.store()).await.unwrap();
    assert_eq!(
        (who.model, who.effort.as_deref(), who.instructions.as_str()),
        (
            AgentModel::Tool {
                tool: Tool::Codex,
                model: Some("gpt-5.5".into())
            },
            Some("high"),
            ""
        )
    );

    for (params, refusal) in [
        (json!({"model": "gpt-9"}), "model_not_listed"),
        (
            json!({"model": "claude", "effort": "ludicrous"}),
            "bad_effort",
        ),
        (json!({"model": "opencode", "effort": "high"}), "bad_effort"),
    ] {
        let Err(ClientError::Service(refused)) =
            ask(&client, "noyan.set_starts_on", params.clone()).await
        else {
            panic!("refused: {params}")
        };
        assert_eq!(
            refused.data.as_ref().unwrap()["refusal"],
            refusal,
            "{params}"
        );
    }

    // A ticket's agent may read it, never set it.
    let agent = w.agent_shell(&ticket).await;
    let line = json!({"protocol_version": otaq_core::protocol::PROTOCOL_VERSION, "id": 1,
        "request": {"method": "noyan.set_starts_on", "params": {"model": "claude"}}})
    .to_string();
    let ran = agent.send_raw(&w.data.socket_path(), &line).await;
    let answer: Value = serde_json::from_str(ran.stdout.lines().next().unwrap_or_default())
        .unwrap_or_else(|e| panic!("{e}: {ran:?}"));
    assert_eq!(
        answer["error"]["data"]["refusal"], "not_allowed",
        "{answer}"
    );
    w.stop().await;
}

fn hold(
    ticket: Option<&str>,
    project: Option<&str>,
    role: SessionRole,
    reason: HoldReason,
) -> AgentHold {
    AgentHold {
        id: uuid::Uuid::now_v7().to_string(),
        ticket_id: ticket.map(str::to_owned),
        project_id: project.map(str::to_owned),
        role,
        reason,
        tool: Tool::ALL[0],
        model: Some("opus".into()),
        skill: None,
        since: 100,
    }
}

/// SCH-04 `[M23]`: one hold per ticket (or project) and session role; setting it again replaces it and keeps when it
/// started while the reason stays; queries by ticket, tool and reason; clearing
/// lifts it.
#[tokio::test]
async fn holds_are_one_per_scope_and_role() {
    let f = Fixture::shop().await;
    let store = f.board.store();
    let shop31 = f.create("Checkout flow").await;
    let shop24 = f.create("Update footer links").await;
    let project = f.board.project("SHOP").await.unwrap();

    let mut developer = hold(
        Some(&shop31.id),
        None,
        SessionRole::Developer,
        HoldReason::ToolLimit,
    );
    store.set_agent_hold(&developer).await.unwrap();
    store
        .set_agent_hold(&hold(
            Some(&shop31.id),
            None,
            SessionRole::Validator,
            HoldReason::ToolLimit,
        ))
        .await
        .unwrap();
    store
        .set_agent_hold(&hold(
            Some(&shop24.id),
            None,
            SessionRole::Developer,
            HoldReason::ModelUnavailable,
        ))
        .await
        .unwrap();
    store
        .set_agent_hold(&hold(
            None,
            Some(&project.id),
            SessionRole::ProjectAgent,
            HoldReason::ToolUnavailable,
        ))
        .await
        .unwrap();

    // The same role again: replaced, the start kept while the reason stays.
    developer.id = "another".into();
    developer.since = 900;
    developer.model = Some("sonnet".into());
    store.set_agent_hold(&developer).await.unwrap();
    let held = store.ticket_holds(&shop31.id).await.unwrap();
    assert_eq!(held.len(), 2);
    let dev = held
        .iter()
        .find(|h| h.role == SessionRole::Developer)
        .unwrap();
    assert_eq!((dev.since, dev.model.as_deref()), (100, Some("sonnet")));
    developer.reason = HoldReason::ToolUnavailable;
    store.set_agent_hold(&developer).await.unwrap();
    let held = store.ticket_holds(&shop31.id).await.unwrap();
    let dev = held
        .iter()
        .find(|h| h.role == SessionRole::Developer)
        .unwrap();
    assert_eq!((dev.reason, dev.since), (HoldReason::ToolUnavailable, 900));

    // A second row for the same scope and role can't be written by hand either.
    let twice = sqlx::query(
        "INSERT INTO agent_holds (id, ticket_id, role, reason, tool, since)
         VALUES ('x', ?, 'validator', 'tool_limit', 'claude', 1)",
    )
    .bind(&shop31.id)
    .execute(store.pool())
    .await;
    assert!(twice.is_err(), "one hold per ticket and role");
    let both = sqlx::query(
        "INSERT INTO agent_holds (id, ticket_id, project_id, role, reason, tool, since)
         VALUES ('y', ?, ?, 'project_agent', 'tool_limit', 'claude', 1)",
    )
    .bind(&shop24.id)
    .bind(&project.id)
    .execute(store.pool())
    .await;
    assert!(both.is_err(), "a ticket's or a project's, not both");

    assert_eq!(store.holds_on_tool(Tool::ALL[0]).await.unwrap().len(), 4);
    assert!(store.holds_on_tool(Tool::ALL[1]).await.unwrap().is_empty());
    let unavailable = store.holds_for(HoldReason::ModelUnavailable).await.unwrap();
    assert_eq!(unavailable.len(), 1);
    assert_eq!(
        unavailable[0].ticket_id.as_deref(),
        Some(shop24.id.as_str())
    );
    assert_eq!(store.project_holds(&project.id).await.unwrap().len(), 1);

    assert!(
        store
            .clear_agent_hold(HoldScope::Ticket(&shop31.id), SessionRole::Validator)
            .await
            .unwrap()
    );
    assert!(
        !store
            .clear_agent_hold(HoldScope::Ticket(&shop31.id), SessionRole::Validator)
            .await
            .unwrap()
    );
    assert!(
        store
            .clear_agent_hold(HoldScope::Project(&project.id), SessionRole::ProjectAgent)
            .await
            .unwrap()
    );

    // A check's hold is the ticket's, one per role: a second replaces the first.
    let first = hold(
        Some(&shop31.id),
        None,
        SessionRole::Validator,
        HoldReason::ToolLimit,
    );
    let second = hold(
        Some(&shop31.id),
        None,
        SessionRole::Validator,
        HoldReason::ModelUnavailable,
    );
    store.set_agent_hold(&first).await.unwrap();
    store.set_agent_hold(&second).await.unwrap();
    let checks: Vec<HoldReason> = store
        .ticket_holds(&shop31.id)
        .await
        .unwrap()
        .into_iter()
        .filter(|h| h.role == SessionRole::Validator)
        .map(|h| h.reason)
        .collect();
    assert_eq!(checks, [HoldReason::ModelUnavailable]);
    assert!(
        store
            .clear_agent_hold(HoldScope::Ticket(&shop31.id), SessionRole::Validator)
            .await
            .unwrap()
    );
    assert!(
        store
            .agent_hold(HoldScope::Ticket(&shop31.id), SessionRole::Validator)
            .await
            .unwrap()
            .is_none()
    );
    // A deleted ticket takes its holds with it.
    sqlx::query("DELETE FROM tickets WHERE id = ?")
        .bind(&shop24.id)
        .execute(store.pool())
        .await
        .unwrap();
    assert!(
        store
            .holds_for(HoldReason::ModelUnavailable)
            .await
            .unwrap()
            .is_empty()
    );
}
