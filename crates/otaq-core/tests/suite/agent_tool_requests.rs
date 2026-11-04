//! M23's requests (T-0347; d0086, d0088): `agent_tools.list` and `agent_tools.set_hidden` on the service's tool
//! states (Hidden kept in settings), the M23 requests not built yet answering so until their ticket lands, and
//! none of them open to agents. What rechecks, Sign in… and Disconnect do is in `connected_tools`.

use otaq_core::agent_tools::Tool;
use otaq_core::agent_tools::connected::{AgentToolState, hidden_tools};
use otaq_core::client::{Client, ClientConfig, ClientError};
use otaq_core::protocol::{ErrorKind, Reply, Request};
use otaq_core::service::testing::TestService;
use serde_json::{Value, json};

async fn ask(client: &Client, method: &str, params: Value) -> Result<Reply, ClientError> {
    let mut request = json!({ "method": method });
    if !params.is_null() {
        request["params"] = params;
    }
    let request: Request =
        serde_json::from_value(request).unwrap_or_else(|e| panic!("{method}: {e}"));
    client.request(request).await
}

#[tokio::test(flavor = "multi_thread")]
async fn agent_tools_list_and_hidden_and_the_rest_not_built_yet() {
    let w = TestService::start().await;
    w.add_project("shop-web").await;
    let ticket = w.create("SHOP", "Checkout flow").await;
    let client = Client::connect(ClientConfig::new(&w.data)).await.unwrap();

    // Nothing is detected in tests: every tool is unchecked, in d0086's order.
    let Reply::AgentTools(list) = ask(&client, "agent_tools.list", Value::Null).await.unwrap()
    else {
        panic!("agent_tools.list")
    };
    let tools: Vec<Tool> = list.tools.iter().map(|t| t.tool).collect();
    assert_eq!(tools, Tool::ALL);
    assert!(
        list.tools
            .iter()
            .all(|t| t.state == AgentToolState::NotInstalled)
    );

    let Reply::AgentTools(_) = ask(
        &client,
        "agent_tools.set_hidden",
        json!({"tool": "copilot", "hidden": true}),
    )
    .await
    .unwrap() else {
        panic!("agent_tools.set_hidden")
    };
    assert_eq!(
        hidden_tools(w.board.store()).await.unwrap(),
        [Tool::Copilot]
    );
    ask(
        &client,
        "agent_tools.set_hidden",
        json!({"tool": "opencode", "hidden": true}),
    )
    .await
    .unwrap();
    ask(
        &client,
        "agent_tools.set_hidden",
        json!({"tool": "copilot", "hidden": false}),
    )
    .await
    .unwrap();
    assert_eq!(
        hidden_tools(w.board.store()).await.unwrap(),
        [Tool::OpenCode]
    );

    let key = ticket.key.to_string();
    // Use fallback with nothing held is refused (T-0357, AGM-03, `launch_gate.rs`).
    let Err(ClientError::Service(e)) =
        ask(&client, "ticket.use_fallback", json!({"key": key})).await
    else {
        panic!("ticket.use_fallback answered")
    };
    assert_eq!(e.message, format!("{key} isn't waiting"));
    // Resume and Use fallback now with nothing waiting (T-0358, `limits_per_tool.rs`): no fallback set.
    let resumed = ask(&client, "limits.resume_tool", json!({"tool": "claude"}))
        .await
        .unwrap();
    assert_eq!(resumed, Reply::Resumed(Vec::new()));
    let Err(ClientError::Service(e)) = ask(
        &client,
        "limits.use_fallback_now",
        json!({"tool": "claude"}),
    )
    .await
    else {
        panic!("limits.use_fallback_now answered")
    };
    assert_eq!(e.kind, ErrorKind::Refused);

    // An agent may not call any of them.
    let agent = w.agent_shell(&ticket).await;
    let mut all = vec![
        ("agent_tools.list", Value::Null),
        ("agent_tools.recheck", json!({})),
        ("agent_tools.sign_in", json!({"tool": "codex"})),
        ("agent_tools.disconnect", json!({"tool": "codex"})),
        (
            "agent_tools.set_hidden",
            json!({"tool": "codex", "hidden": true}),
        ),
    ];
    all.extend([
        ("limits.resume_tool", json!({"tool": "claude"})),
        ("limits.use_fallback_now", json!({"tool": "claude"})),
    ]);
    all.push(("ticket.use_fallback", json!({"key": key})));
    for (method, params) in all {
        let mut request = json!({"method": method});
        if !params.is_null() {
            request["params"] = params;
        }
        let line = json!({"protocol_version": otaq_core::protocol::PROTOCOL_VERSION, "id": 1, "request": request}).to_string();
        let ran = agent.send_raw(&w.data.socket_path(), &line).await;
        let answer: Value = serde_json::from_str(ran.stdout.lines().next().unwrap_or_default())
            .unwrap_or_else(|e| panic!("{e}: {ran:?}"));
        assert_eq!(
            answer["error"]["data"]["refusal"], "not_allowed",
            "{method}: {answer}"
        );
    }
    assert_eq!(
        hidden_tools(w.board.store()).await.unwrap(),
        [Tool::OpenCode]
    );
    w.stop().await;
}
