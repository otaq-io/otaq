//! A note per answer option (T-0374; ATT-17; d0061): `--note` rides with its `--option` through both routes (a
//! ticket's agent, a Project agent) and is stored whole in `option_notes_json`. Requests are sent over the socket from inside the sessions (an ordinary `sh` as each agent,
//! running `nc`), so each is judged as its caller. No test starts `claude`.

use otaq_core::agent_tools::LaunchContext;
use otaq_core::agent_tools::claude::ClaudeLaunch;
use otaq_core::board::questions::Choice;
use otaq_core::board::tickets::Ticket;
use otaq_core::git::testing::TempRepos;
use otaq_core::service::ServiceConfig;
use otaq_core::service::testing::{AgentShell, TestService, path_without_build_folders};
use serde_json::{Value, json};

const QUESTION: &str = "Which export format?";
const MD: &str = "Opens in any editor; one file per note, no attachments.";
const ZIP: &str = "One download with everything, but needs a ZIP library and an import later.";

/// The sample board (SHOP-42 Working); `claude` is an interactive `sh` in the service's data folder.
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

/// One request from inside the session.
async fn send(w: &TestService, shell: &AgentShell, method: &str, params: Value) -> Value {
    let request =
        json!({"protocol_version": 1, "id": 1, "request": {"method": method, "params": params}});
    let ran = shell
        .send_raw(&w.data.socket_path(), &request.to_string())
        .await;
    serde_json::from_str(ran.stdout.lines().next().unwrap_or_default())
        .unwrap_or_else(|e| panic!("{e}: {ran:?}"))
}

fn ok(answer: &Value) -> &Value {
    assert!(answer.get("error").is_none(), "{answer}");
    &answer["reply"]
}

fn refused(answer: &Value) -> String {
    assert_eq!(answer["error"]["kind"], "refused", "{answer}");
    answer["error"]["message"].as_str().unwrap().to_owned()
}

fn notes(entries: &[Option<&str>]) -> Vec<Option<String>> {
    entries.iter().map(|n| n.map(str::to_owned)).collect()
}

async fn ticket(w: &TestService, key: &str) -> Ticket {
    w.board.ticket(key).await.unwrap()
}

/// The shell of `key`'s running agent session.
async fn running_agent(w: &TestService, key: &str) -> AgentShell {
    w.agent_shell(&ticket(w, key).await).await
}

/// A ticket's agent: the notes pair with the options, null for one without; a 600-character note is stored whole;
/// notes on a free-text question or not one per option are refused. The delivered answer doesn't name the note.
#[tokio::test(flavor = "multi_thread")]
async fn a_ticket_agents_notes_are_stored_whole_with_its_options() {
    let w = world().await;
    let asker = running_agent(&w, "SHOP-42").await;
    let long = "n".repeat(600);
    let free_text = send(
        &w,
        &asker,
        "ticket.ask_human",
        json!({"key": "SHOP-42", "question": QUESTION, "notes": [MD]}),
    )
    .await;
    assert_eq!(
        refused(&free_text),
        "A free-text question takes no --note: a --note describes the --option before it"
    );
    let uneven = send(
        &w,
        &asker,
        "ticket.ask_human",
        json!({"key": "SHOP-42", "question": QUESTION, "options": ["A", "B"], "notes": [MD]}),
    )
    .await;
    assert_eq!(
        refused(&uneven),
        "Give the notes one per option, in the options' order"
    );

    let asked = send(
        &w,
        &asker,
        "ticket.ask_human",
        json!({"key": "SHOP-42", "question": QUESTION, "options": ["Markdown files", "One ZIP", "PDF"],
               "notes": [format!("  {MD}\n"), null, long], "recommended": 1}),
    )
    .await;
    let reply = ok(&asked);
    assert_eq!(
        reply["ticket"]["question"]["notes"],
        json!([MD, null, long])
    );
    let question = ticket(&w, "SHOP-42").await.question.expect("asks you");
    assert_eq!(question.notes, notes(&[Some(MD), None, Some(&long)]));
    let stored: Option<String> =
        sqlx::query_scalar("SELECT option_notes_json FROM human_questions WHERE id = ?")
            .bind(&question.id)
            .fetch_one(w.board.store().pool())
            .await
            .unwrap();
    assert_eq!(stored, Some(json!([MD, null, long]).to_string()));
    assert_eq!(
        otaq_core::board::questions::format_answer(&question, &Choice::Option(1)),
        "Answer to your question: Markdown files (option 1)."
    );
    w.stop().await;
}

/// Without notes nothing is stored and nothing shows; old rows (NULL) read the same.
#[tokio::test(flavor = "multi_thread")]
async fn a_question_without_notes_stores_none() {
    let w = world().await;
    let t = w.create("SHOP", "Order confirmation email").await;
    let q = w
        .board
        .store_question(
            &t.id,
            "Which email provider?",
            &["Postmark".into(), "SES".into()],
            &[None, Some("  ".into())],
            Some(1),
        )
        .await
        .unwrap();
    assert!(q.notes.is_empty());
    let read = w.board.open_question(&t.id).await.unwrap().unwrap();
    assert!(read.notes.is_empty());
    assert!(serde_json::to_value(&read).unwrap().get("notes").is_none());
    let stored: Option<String> =
        sqlx::query_scalar("SELECT option_notes_json FROM human_questions WHERE id = ?")
            .bind(&q.id)
            .fetch_one(w.board.store().pool())
            .await
            .unwrap();
    assert_eq!(stored, None);

    // A round trip with a null entry.
    let t2 = w.create("SHOP", "Export").await;
    w.board
        .store_question(
            &t2.id,
            QUESTION,
            &["Markdown files".into(), "One ZIP".into(), "PDF".into()],
            &notes(&[None, Some(ZIP), None]),
            None,
        )
        .await
        .unwrap();
    let read = w.board.open_question(&t2.id).await.unwrap().unwrap();
    assert_eq!(read.notes, notes(&[None, Some(ZIP), None]));
    w.stop().await;
}

/// A Project agent's `ask human` carries the notes.
#[tokio::test(flavor = "multi_thread")]
async fn a_project_agents_question_carries_the_notes() {
    let w = world().await;
    let agent = w
        .project_agents
        .open("SHOP", "Plan the export")
        .await
        .unwrap();
    let shell = w.shell_in(&agent.session.as_ref().unwrap().id);
    let asked = send(
        &w,
        &shell,
        "agent.ask_human",
        json!({"question": QUESTION, "options": ["Markdown files", "One ZIP"], "notes": [MD, null],
               "recommended": 1}),
    )
    .await;
    assert_eq!(
        ok(&asked)["project_agent"]["question"]["notes"],
        json!([MD, null])
    );
    let question = w
        .project_agents
        .get(&agent.id)
        .await
        .unwrap()
        .question
        .unwrap();
    assert_eq!(question.notes, notes(&[Some(MD), None]));
    w.stop().await;
}
