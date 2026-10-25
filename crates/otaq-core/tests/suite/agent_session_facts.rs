//! What the sessions say about a ticket's agents (T-0336, T-0405; ADP-04, AGM-05; d0086, d0088): the "moved on"
//! query (the sheet's "Sonnet → claude-sonnet-5-6") and the tokens by session role and agent ("Strong 61k · Light
//! 23k (1 round)"). Sessions are stored rows; nothing runs.

use otaq_core::agent_tools::{Adapters, Tool};
use otaq_core::agents::SessionRole;
use otaq_core::board::tickets::Ticket;
use otaq_core::data_dir::DataDir;
use otaq_core::service::EventBus;
use otaq_core::session::{MovedOn, moved_on};
use otaq_core::store::new_id;
use otaq_core::usage::{AgentTokens, Tokens, Usage};

use crate::common::Fixture;

/// One stored session of `t`: its kind, the named agent it ran on (id, name), the exact model that ran, and when it
/// started.
async fn session(
    f: &Fixture,
    t: &Ticket,
    kind: &str,
    agent: Option<(&str, &str)>,
    model_ran: Option<&str>,
    started_at: i64,
) -> String {
    let project = f.board.project("SHOP").await.unwrap();
    let id = new_id();
    sqlx::query(
        "INSERT INTO sessions (id, project_id, ticket_id, kind, tool, tool_session_id, agent_id,
                               agent_name, model_ran, state, started_at, ended_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, 'ended', ?, ?)",
    )
    .bind(&id)
    .bind(&project.id)
    .bind(&t.id)
    .bind(kind)
    .bind(Tool::Claude.as_str())
    .bind(format!("conversation-{id}"))
    .bind(agent.map(|(id, _)| id))
    .bind(agent.map(|(_, name)| name))
    .bind(model_ran)
    .bind(started_at)
    .bind(started_at + 1)
    .execute(f.board.store().pool())
    .await
    .unwrap();
    id
}

/// "Moved on": a session's exact model against the previous session of the same ticket, role and named agent.
#[tokio::test]
async fn moved_on_compares_the_previous_run_of_the_same_agent_and_role() {
    let f = Fixture::shop().await;
    let t = f.create("Fix flaky login redirect").await;
    let sonnet = Some(("sonnet", "Sonnet"));
    let pool = f.board.store().pool();
    let first = session(&f, &t, "agent", sonnet, Some("claude-sonnet-5-5"), 10).await;
    let validator = session(&f, &t, "validator", sonnet, Some("claude-sonnet-5-7"), 15).await;
    let other = session(
        &f,
        &t,
        "agent",
        Some(("light", "Light")),
        Some("claude-haiku-5-5"),
        18,
    )
    .await;
    let unknown = session(&f, &t, "agent", sonnet, None, 19).await;
    let moved = session(&f, &t, "agent", sonnet, Some("claude-sonnet-5-6"), 20).await;
    let same = session(&f, &t, "agent", sonnet, Some("claude-sonnet-5-6"), 30).await;

    assert_eq!(
        moved_on(pool, &first).await.unwrap(),
        None,
        "no earlier run"
    );
    assert_eq!(
        moved_on(pool, &moved).await.unwrap(),
        Some(MovedOn {
            before: "claude-sonnet-5-5".into(),
            now: "claude-sonnet-5-6".into(),
        }),
        "a different agent's or role's sessions and an unknown model in between don't count"
    );
    assert_eq!(moved_on(pool, &same).await.unwrap(), None, "same model");
    assert_eq!(
        moved_on(pool, &validator).await.unwrap(),
        None,
        "the validator's own role"
    );
    assert_eq!(moved_on(pool, &other).await.unwrap(), None);
    assert_eq!(
        moved_on(pool, &unknown).await.unwrap(),
        None,
        "not known yet"
    );
}

/// Tokens by role and named agent: the developer's sessions (resumed and answering ones too) on Strong, the
/// validator's on Light.
#[tokio::test]
async fn tokens_are_split_by_role_and_named_agent() {
    let f = Fixture::shop().await;
    let t = f.create("Price table").await;
    let strong = Some(("strong", "Strong"));
    let light = Some(("light", "Light"));
    let pool = f.board.store().pool();
    let mut used = Vec::new();
    for (kind, agent, at, input) in [
        ("validator", light, 15, 20_000),
        ("agent", strong, 10, 40_000),
        ("agent", strong, 20, 20_000),
        ("answer", strong, 30, 1_000),
        ("validator", light, 40, 3_000),
    ] {
        used.push((session(&f, &t, kind, agent, None, at).await, input));
    }
    for (id, input) in &used {
        sqlx::query("INSERT INTO usage (session_id, day, input, cached, output) VALUES (?, '2026-10-17', ?, 9, 0)")
            .bind(id)
            .bind(input)
            .execute(pool)
            .await
            .unwrap();
    }
    let home = tempfile::tempdir().unwrap();
    let data = DataDir::new(home.path());
    let usage = Usage::new(
        f.board.store().clone(),
        EventBus::new(8),
        Adapters::launching_only(None, data),
        std::time::Duration::from_secs(3600),
    );
    let tokens = |input: u64, sessions: u64| {
        Some(Tokens {
            input,
            cached: 9 * sessions,
            output: 0,
            reasoning: None,
        })
    };
    assert_eq!(
        usage.agents_of(&t.id).await.unwrap(),
        [
            AgentTokens {
                role: SessionRole::Developer,
                agent: Some("Strong".into()),
                tool: Some(Tool::Claude),
                tokens: tokens(61_000, 3),
            },
            AgentTokens {
                role: SessionRole::Validator,
                agent: Some("Light".into()),
                tool: Some(Tool::Claude),
                tokens: tokens(23_000, 2),
            },
        ]
    );
}
