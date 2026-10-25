//! What a Project agent may do, through the socket (T-0200; CLI-06, PAG-02, PAG-03, PAG-05, TKT-08, CLI-04, MSG-01;
//! d0068, d0031, d0030, d0060): requests sent from inside its session (an ordinary `sh` started as a Project agent,
//! running `nc`) are judged as that Project agent. It creates, edits, links and prioritises Backlog tickets only;
//! its tickets name it; `prime --project`; Created and changed with Delete; `ask <KEY>` and `ask human`, with the
//! answers typed into its session.

use otaq_core::agent_tools::LaunchContext;
use otaq_core::agent_tools::claude::ClaudeLaunch;
use otaq_core::board::questions::Choice;
use otaq_core::domain::{Actor, Status};
use otaq_core::git::testing::TempRepos;
use otaq_core::policy::BACKLOG_ONLY;
use otaq_core::project_agents::ProjectAgent;
use otaq_core::service::ServiceConfig;
use otaq_core::service::testing::{AgentShell, TestService, path_without_build_folders};
use otaq_core::session::deliveries::Delivery;
use serde_json::{Value, json};

/// The sample board, with `claude` an interactive `sh` in the service's data folder.
async fn world() -> TestService {
    let home = tempfile::TempDir::new().unwrap();
    let data = home.path().join("h");
    let script = format!(
        "export {}='{}'; export PS1=''; exec sh",
        otaq_core::data_dir::DataDir::env_var(),
        data.display()
    );
    let w = TestService::start_in(home, TempRepos::new(), |config| ServiceConfig {
        launch: Some(LaunchContext {
            bin_dir: "/nonexistent".into(),
            plugin_dir: "/nonexistent/plugin".into(),
            path: path_without_build_folders().into(),
            claude: ClaudeLaunch {
                program: vec!["sh".into(), "-c".into(), script, "sh".into()],
                config_dir: None,
            },
        }),
        ..config
    })
    .await;
    w.shop_sample().await;
    w
}

async fn open(w: &TestService) -> (ProjectAgent, AgentShell) {
    let agent = w
        .project_agents
        .open("SHOP", "Look around the checkout code")
        .await
        .unwrap();
    let shell = w.shell_in(&agent.session.as_ref().unwrap().id);
    (agent, shell)
}

async fn send(w: &TestService, shell: &AgentShell, method: &str, params: Value) -> Value {
    let request =
        json!({"protocol_version": 1, "id": 1, "request": {"method": method, "params": params}});
    let ran = shell
        .send_raw(&w.data.socket_path(), &request.to_string())
        .await;
    serde_json::from_str(ran.stdout.lines().next().unwrap_or_default())
        .unwrap_or_else(|e| panic!("{e}: {ran:?}"))
}

fn refused(answer: &Value) -> &str {
    assert_eq!(answer["error"]["kind"], "refused", "{answer}");
    answer["error"]["message"].as_str().unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn a_project_agent_changes_backlog_tickets_only_and_its_tickets_name_it() {
    let w = world().await;
    let (agent, shell) = open(&w).await;
    let me = format!("project-agent:{}", agent.id);

    // Create (Backlog, with priority and External ID), then edit and link it.
    let created = send(
        &w,
        &shell,
        "ticket.create",
        json!({"project": "SHOP", "title": "Slow checkout query", "priority": 2, "external_id": "GH-212"}),
    )
    .await;
    let ticket = &created["reply"]["ticket"];
    assert_eq!(ticket["key"], "SHOP-44", "{created}");
    assert_eq!(ticket["place"], "backlog");
    assert_eq!(ticket["created_by"], me.as_str());
    let edited = send(
        &w,
        &shell,
        "ticket.update",
        json!({"key": "SHOP-44", "title": "Slow checkout queries", "priority": 1}),
    )
    .await;
    assert_eq!(
        edited["reply"]["ticket"]["updated_by"],
        me.as_str(),
        "{edited}"
    );
    let linked = send(
        &w,
        &shell,
        "dep.add",
        json!({"child": "SHOP-44", "parent": "SHOP-19"}),
    )
    .await;
    assert!(linked.get("error").is_none(), "{linked}");
    // A user's Backlog ticket may be edited too.
    let other = send(
        &w,
        &shell,
        "ticket.update",
        json!({"key": "SHOP-13", "priority": 2}),
    )
    .await;
    assert!(other.get("error").is_none(), "{other}");

    // Everything else is refused with the one text.
    for (method, params) in [
        ("ticket.update", json!({"key": "SHOP-19", "title": "x"})),
        ("dep.add", json!({"child": "SHOP-19", "parent": "SHOP-44"})),
        (
            "ticket.move",
            json!({"key": "SHOP-44", "target": "ready_for_dev"}),
        ),
        ("ticket.delete", json!({"key": "SHOP-44"})),
        ("ticket.close", json!({"key": "SHOP-19", "confirm": true})),
        (
            "ticket.stage_next",
            json!({"key": "SHOP-44", "summary": "done"}),
        ),
        (
            "ticket.stage_done",
            json!({"key": "SHOP-44", "summary": "nothing to do"}),
        ),
        (
            "ticket.local_merge",
            json!({"key": "SHOP-18", "message": "m"}),
        ),
        (
            "project_agent.open",
            json!({"project": "SHOP", "message": "m"}),
        ),
    ] {
        let answer = send(&w, &shell, method, params).await;
        assert_eq!(refused(&answer), BACKLOG_ONLY, "{method}");
    }
    assert_eq!(
        w.board.ticket("SHOP-19").await.unwrap().title,
        "Price table"
    );
    assert_eq!(w.status("SHOP-44").await, Status::Backlog);
    w.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn created_and_changed_lists_its_work_and_delete_removes_a_created_ticket() {
    let w = world().await;
    let (agent, shell) = open(&w).await;
    let empty = w
        .project_agents
        .created_and_changed(&agent.id)
        .await
        .unwrap();
    assert_eq!(empty.summary, "Nothing created or changed yet");
    for title in ["Slow checkout query", "Cache price table"] {
        let created = send(
            &w,
            &shell,
            "ticket.create",
            json!({"project": "SHOP", "title": title}),
        )
        .await;
        assert!(created.get("error").is_none(), "{created}");
    }
    send(
        &w,
        &shell,
        "ticket.update",
        json!({"key": "SHOP-13", "priority": 2}),
    )
    .await;
    send(
        &w,
        &shell,
        "ticket.update",
        json!({"key": "SHOP-44", "priority": 1}),
    )
    .await;
    send(
        &w,
        &shell,
        "dep.add",
        json!({"child": "SHOP-44", "parent": "SHOP-19"}),
    )
    .await;
    // The user's own changes aren't the agent's.
    w.board
        .update_ticket(
            "SHOP-14",
            &otaq_core::board::tickets::TicketUpdate {
                title: Some("Fix the cart rounding".into()),
                ..Default::default()
            },
            &Actor::User("me".into()),
        )
        .await
        .unwrap();

    let summary = w
        .project_agents
        .created_and_changed(&agent.id)
        .await
        .unwrap();
    assert_eq!(
        summary.summary,
        "Created SHOP-44, SHOP-45 · edited SHOP-13 · 1 dependency"
    );
    assert!(summary.created.iter().all(|t| t.deletable));

    // Delete (the user's, after the 0060 confirmation) removes a never-started created ticket.
    w.board.delete_ticket("SHOP-45").await.unwrap();
    let summary = w
        .project_agents
        .created_and_changed(&agent.id)
        .await
        .unwrap();
    assert_eq!(
        summary.summary,
        "Created SHOP-44 · edited SHOP-13 · 1 dependency"
    );
    w.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn prime_project_prints_every_ticket_worktrees_and_the_instructions() {
    let w = world().await;
    let shop19 = w.board.ticket("SHOP-19").await.unwrap();
    let worktree = w.give_worktree(&shop19).await;
    sqlx::query("UPDATE tickets SET started_at = 1, branch_name = 'otaq-x/SHOP-19' WHERE id = ?")
        .bind(&shop19.id)
        .execute(w.board.store().pool())
        .await
        .unwrap();
    // AGM-02: Noyan’s instructions are the Project agent role's agent's own.
    w.edit_agent("Noyan", |a| {
        a.instructions = "Split into tickets of about one day.".into();
    })
    .await;
    let (_, shell) = open(&w).await;
    let answer = send(
        &w,
        &shell,
        "project_agent.prime",
        json!({"project": "SHOP"}),
    )
    .await;
    let prime: otaq_core::project_agents::prime::ProjectPrime =
        serde_json::from_value(answer["reply"]["project_prime"].clone())
            .unwrap_or_else(|e| panic!("{e}: {answer}"));
    let text = otaq_core::project_agents::prime::render(&prime);
    assert!(text.starts_with("# Noyan · shop-web (SHOP)\n"), "{text}");
    assert_eq!(prime.tickets.len(), 43, "every ticket");
    assert!(
        text.contains(
            "- SHOP-20 · Checkout page (Ready for Dev · P3 · Normal) · depends on SHOP-18, SHOP-19\n"
        ),
        "{text}"
    );
    assert!(
        text.contains("- SHOP-42 · Wishlist button (Working · P3 · Normal) · depends on SHOP-41\n"),
        "{text}"
    );
    assert!(
        text.contains(&format!(
            "  worktree {} · branch `otaq-x/SHOP-19`\n",
            worktree.display()
        )),
        "{text}"
    );
    assert!(text.contains(BACKLOG_ONLY), "{text}");
    assert!(
        text.ends_with("## Noyan’s instructions (Noyan)\n\nSplit into tickets of about one day.\n"),
        "{text}"
    );
    w.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn its_questions_reach_a_ticket_and_the_user_and_the_answers_come_back() {
    let w = world().await;
    let (agent, shell) = open(&w).await;
    let shop19 = w.board.ticket("SHOP-19").await.unwrap();
    let ticket_agent = w.agent_shell(&shop19).await;

    // ask <KEY>: to any ticket (no link needed), typed into its agent's session; the answer comes back here.
    let asked = send(
        &w,
        &shell,
        "agent.ask",
        json!({"target": "SHOP-19", "question": "Which query builds the price table?"}),
    )
    .await;
    let message = &asked["reply"]["message"];
    assert_eq!(message["to"], "SHOP-19", "{asked}");
    assert!(message.get("from").is_none());
    assert_eq!(message["from_agent"]["id"], agent.id.as_str());
    let id = message["id"].as_str().unwrap().to_owned();
    w.deliveries
        .deliver_now(&ticket_agent.session)
        .await
        .unwrap();
    w.wait_for_output(
        &ticket_agent.session,
        "Noyan · New conversation asks you (question",
    )
    .await;
    let answered = send(
        &w,
        &ticket_agent,
        "message.answer",
        json!({"id": id, "answer": "getPrices in lib/prices.ts"}),
    )
    .await;
    assert!(answered.get("error").is_none(), "{answered}");
    let session = agent.session.as_ref().unwrap().id.clone();
    w.deliveries.deliver_now(&session).await.unwrap();
    w.wait_for_output(&session, "getPrices in lib/prices.ts")
        .await;

    // ask human: waits on the Project agent; the user's answer is typed into its session.
    let asked = send(
        &w,
        &shell,
        "agent.ask_human",
        json!({"question": "Import closed issues too?", "options": ["Yes", "No"], "recommended": 2}),
    )
    .await;
    assert_eq!(
        asked["reply"]["project_agent"]["question"]["text"], "Import closed issues too?",
        "{asked}"
    );
    let delivery = w
        .project_agents
        .answer(&agent.id, &Choice::Option(1), false)
        .await
        .unwrap();
    assert!(matches!(delivery, Delivery::Sent { .. }), "{delivery:?}");
    w.wait_for_output(&session, "Answer to your question: Yes (option 1).")
        .await;
    assert_eq!(
        w.project_agents.get(&agent.id).await.unwrap().question,
        None
    );

    // Only a Project agent asks this way.
    let answer = send(
        &w,
        &ticket_agent,
        "agent.ask_human",
        json!({"question": "q"}),
    )
    .await;
    assert_eq!(
        refused(&answer),
        otaq_core::project_agents::ONLY_A_PROJECT_AGENT
    );
    w.stop().await;
}
