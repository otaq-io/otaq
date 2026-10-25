//! The read commands (T-0068): `show`, `list`, `ready`, `blocked`, `project list | show` against a service in a
//! temporary data folder with the sample board (SHOP-12…SHOP-43).

use std::process::{Command, Output};

use otaq_core::data_dir::DataDir;
use otaq_core::domain::Status;
use otaq_core::naming;
use otaq_core::service::testing::TestService;
use otaq_core::session::{SessionKind, SessionTicket, SpawnSpec, Start, TerminalSize};
use serde_json::Value;

const CB: &str = env!("CARGO_BIN_EXE_otaq");

fn cb(w: &TestService, args: &[&str]) -> Output {
    Command::new(CB)
        .args(args)
        .env(DataDir::env_var(), w.data.root())
        .env_remove(naming::ticket_env_var())
        .current_dir(w.home.path())
        .output()
        .unwrap()
}

fn stdout(out: &Output) -> String {
    assert!(
        out.status.success(),
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout.clone()).unwrap()
}

fn json(out: &Output) -> Value {
    serde_json::from_str(&stdout(out)).unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn show_reads_a_ticket_by_key_number_and_variable() {
    let w = TestService::start().await;
    w.shop_sample().await;

    let text = stdout(&cb(&w, &["show", "SHOP-20"]));
    assert!(text.starts_with("SHOP-20 · Checkout page\n"), "{text}");
    assert!(text.contains("Status:      Ready for Dev\n"), "{text}");
    assert!(
        text.contains("Depends on:  SHOP-18 (Ready for Review), SHOP-19 (Working)"),
        "{text}"
    );
    assert_eq!(stdout(&cb(&w, &["view", "SHOP-20"])), text, "view is show");

    let shown = json(&cb(&w, &["show", "SHOP-20", "--json"]));
    assert_eq!(shown["ticket"]["key"], "SHOP-20");
    assert_eq!(shown["ticket"]["depends_on"][1], "SHOP-19");
    assert!(shown.get("history").is_none());

    // A bare number in the current project; the ticket variable when the key is omitted.
    let by_number = json(&cb(&w, &["-P", "SHOP", "show", "18", "--json"]));
    assert_eq!(by_number["ticket"]["title"], "Checkout API");
    let out = Command::new(CB)
        .args(["show", "--json"])
        .env(DataDir::env_var(), w.data.root())
        .env(naming::ticket_env_var(), "SHOP-23")
        .output()
        .unwrap();
    assert_eq!(json(&out)["ticket"]["title"], "Split user full name");

    // --long adds the history.
    let long = json(&cb(&w, &["show", "SHOP-20", "--long", "--json"]));
    assert!(
        long["history"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["kind"] == "dependency_added")
    );
    assert!(stdout(&cb(&w, &["show", "SHOP-20", "--long"])).contains("History (newest first)"));
    w.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn list_filters_and_readiness() {
    let w = TestService::start().await;
    w.shop_sample().await;

    let working = json(&cb(
        &w,
        &["-P", "SHOP", "list", "--status", "working", "--json"],
    ));
    let keys: Vec<&str> = working
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["key"].as_str().unwrap())
        .collect();
    assert_eq!(keys, ["SHOP-19", "SHOP-27", "SHOP-42"]);
    let dev = json(&cb(
        &w,
        &["--project", "SHOP", "list", "--status", "dev", "--json"],
    ));
    assert!(
        dev.as_array()
            .unwrap()
            .iter()
            .all(|t| t["stage"] == "dev" || t["stage"] == "validate")
    );

    // Inside the repository, the project is found from the folder.
    let repo = w.board.project("SHOP").await.unwrap().repo_path;
    let out = Command::new(CB)
        .args(["list", "--silent"])
        .env(DataDir::env_var(), w.data.root())
        .env_remove(naming::ticket_env_var())
        .current_dir(&repo)
        .output()
        .unwrap();
    assert!(stdout(&out).lines().any(|l| l == "SHOP-20"));

    let blocked = stdout(&cb(&w, &["-P", "SHOP", "blocked"]));
    assert!(
        blocked.contains("SHOP-20")
            && blocked.contains("waits for SHOP-18, SHOP-19 to merge (SHOP-18 in Review)"),
        "{blocked}"
    );
    assert!(blocked.contains("SHOP-21") && blocked.contains("waits for SHOP-20 to merge"));
    let blocked_json = json(&cb(&w, &["-P", "SHOP", "blocked", "--json"]));
    let shop20 = blocked_json
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["key"] == "SHOP-20")
        .unwrap();
    assert_eq!(
        shop20["reason"],
        "waits for SHOP-18, SHOP-19 to merge (SHOP-18 in Review)"
    );
    assert_eq!(shop20["waiting_for"][0]["status"], "ready_for_review");
    let ready = json(&cb(&w, &["-P", "SHOP", "ready", "--json"]));
    let ready: Vec<&str> = ready
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["key"].as_str().unwrap())
        .collect();
    assert!(ready.contains(&"SHOP-42"), "its parent SHOP-41 is merged");
    assert!(
        !ready.contains(&"SHOP-20") && !ready.contains(&"SHOP-17"),
        "{ready:?}"
    );
    w.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn projects_and_errors() {
    let w = TestService::start().await;
    w.shop_sample().await;

    let projects = json(&cb(&w, &["project", "list", "--json"]));
    assert_eq!(projects[0]["key"], "SHOP");
    assert!(stdout(&cb(&w, &["project", "list"])).starts_with("SHOP   shop-web"));
    let shop = json(&cb(&w, &["project", "show", "SHOP", "--json"]));
    assert_eq!(shop["base_branch"], "main");

    // Unknown keys: {"error": …} and a non-zero exit.
    let out = cb(&w, &["show", "SHOP-99", "--json"]);
    assert!(!out.status.success());
    let error: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(error["error"]["kind"], "not_found");
    let out = cb(&w, &["project", "show", "NOPE", "--json"]);
    assert!(!out.status.success());
    // A bare number without a current project.
    let out = cb(&w, &["show", "18", "--json"]);
    assert!(!out.status.success());
    let error: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(error["error"]["kind"], "usage");
    w.stop().await;
}

/// Agents may read anything (CLI-02): the same commands from inside SHOP-19's agent session.
#[tokio::test(flavor = "multi_thread")]
async fn an_agent_reads_the_whole_project() {
    let w = TestService::start().await;
    w.shop_sample().await;
    let shop19 = w.board.ticket("SHOP-19").await.unwrap();
    let agent = w.agent_shell(&shop19).await;
    let own = agent.run(&format!("'{CB}' show --json")).await;
    assert_eq!(own.status, 0, "{own:?}");
    let own: Value = serde_json::from_str(&own.stdout).unwrap();
    assert_eq!(own["ticket"]["key"], "SHOP-19");
    let other = agent.run(&format!("'{CB}' show 20")).await;
    assert!(
        other.stdout.starts_with("SHOP-20 · Checkout page"),
        "{other:?}"
    );
    let blocked = agent.run(&format!("'{CB}' blocked --silent")).await;
    assert!(
        blocked.stdout.lines().any(|l| l == "SHOP-21"),
        "{blocked:?}"
    );
    w.stop().await;
}

/// `show --terminal` (T-0317, SHL-10): the ticket's terminal as plain text, `--json` with the session; usage errors
/// with `--long`; a ticket whose agent never started is an error with a non-zero exit.
#[tokio::test(flavor = "multi_thread")]
async fn show_terminal_prints_the_screen_as_plain_text() {
    let w = TestService::start().await;
    w.shop_sample().await;
    let ticket = w.board.ticket("SHOP-22").await.unwrap();
    let project = w.board.project("SHOP").await.unwrap();
    let session = w
        .sessions
        .spawn(SpawnSpec {
            kind: SessionKind::Agent,
            start: Start::User,
            project_id: project.id.clone(),
            project_key: project.key.clone(),
            ticket: Some(SessionTicket {
                id: ticket.id.clone(),
                key: ticket.key.clone(),
            }),
            program: "sh".into(),
            args: vec![
                "-c".into(),
                "printf '\\033[2J\\033[H\\033[1;31mAllow\\033[0m rm -rf .next?   \\r\\n\\033[3;3H1. Yes'; exec cat"
                    .into(),
            ],
            cwd: project.repo_path.clone().into(),
            env: vec![],
            size: TerminalSize { cols: 40, rows: 8 },
            tool: None,
            tool_session_id: None,
            model: None,
            agent: None,
            effort: None,
            fallback: None,
            theme: None,
        })
        .await
        .unwrap();
    w.wait_for_output(&session.id, "1. Yes").await;

    assert_eq!(
        stdout(&cb(&w, &["show", "SHOP-22", "--terminal"])),
        "Allow rm -rf .next?\n\n  1. Yes\n"
    );
    let shown = json(&cb(&w, &["show", "SHOP-22", "--terminal", "--json"]));
    assert_eq!(
        shown,
        serde_json::json!({"key": "SHOP-22", "session": session.id, "text": "Allow rm -rf .next?\n\n  1. Yes"})
    );

    // Not with --long or --children: a usage error.
    for extra in ["--long", "--children"] {
        let out = cb(&w, &["show", "SHOP-22", "--terminal", extra, "--json"]);
        assert_eq!(out.status.code(), Some(2), "{extra}");
        let error: Value = serde_json::from_slice(&out.stdout).unwrap();
        assert_eq!(error["error"]["kind"], "usage", "{extra}");
    }

    // No session: the sentence, a non-zero exit, {"error": …} with --json.
    let out = cb(&w, &["show", "SHOP-13", "--terminal"]);
    assert_eq!(out.status.code(), Some(1));
    assert_eq!(
        String::from_utf8_lossy(&out.stderr),
        "error: SHOP-13 has no terminal output: its agent hasn't started.\n"
    );
    let out = cb(&w, &["show", "SHOP-13", "--terminal", "--json"]);
    assert!(!out.status.success());
    let error: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(
        error["error"]["message"],
        "SHOP-13 has no terminal output: its agent hasn't started."
    );
    w.stop().await;
}

/// `show --terminal --stage <name>` (T-0416, FLW-20) on the test flow: refused while the ticket is in Dev; in
/// Validate (one check agent), the check's terminal, and the plain form's first line names the stage; in Security (two
/// check agents, d0130 row 17) `--stage` alone is refused with the count and `--agent` picks one; `--validator` is
/// unknown.
#[tokio::test(flavor = "multi_thread")]
async fn show_terminal_stage_reads_the_check_running_it() {
    let w = TestService::start().await;
    let project = w.add_project("shop-web").await;
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../otaq-core/tests/flows/every-kind.yaml");
    let file = otaq_core::flow::yaml::read(&std::fs::read_to_string(path).unwrap()).unwrap();
    w.board.use_flow_for_tests("SHOP", &file).await;
    let ticket = w.create("SHOP", "Escape the search query").await;
    w.force_status(&ticket, Status::Working).await;
    let print = |kind, text: &str| SpawnSpec {
        kind,
        start: Start::User,
        project_id: project.id.clone(),
        project_key: project.key.clone(),
        ticket: Some(SessionTicket {
            id: ticket.id.clone(),
            key: ticket.key.clone(),
        }),
        program: "sh".into(),
        args: vec!["-c".into(), format!("printf '{text}'; exec cat")],
        cwd: project.repo_path.clone().into(),
        env: vec![],
        size: TerminalSize { cols: 40, rows: 8 },
        tool: None,
        tool_session_id: None,
        model: None,
        agent: None,
        effort: None,
        fallback: None,
        theme: None,
    };
    let agent = w
        .sessions
        .spawn(print(SessionKind::Agent, "the agent<end>"))
        .await
        .unwrap();
    w.wait_for_output(&agent.id, "<end>").await;

    // In Dev: refused, a non-zero exit.
    let out = cb(&w, &["show", "SHOP-1", "--terminal", "--stage", "security"]);
    assert_eq!(out.status.code(), Some(1));
    assert_eq!(
        String::from_utf8_lossy(&out.stderr),
        "error: SHOP-1 isn't in Security\n"
    );
    assert_eq!(
        stdout(&cb(&w, &["show", "SHOP-1", "--terminal", "--stage", "dev"])),
        "the agent<end>\n"
    );

    // In Validate while its check runs.
    w.force_stage(&ticket, "validate", Status::Checking).await;
    let check = w
        .sessions
        .spawn(print(SessionKind::Validator, "the check<end>"))
        .await
        .unwrap();
    w.wait_for_output(&check.id, "<end>").await;
    assert_eq!(
        stdout(&cb(
            &w,
            &["show", "SHOP-1", "--terminal", "--stage", "validate"]
        )),
        "the check<end>\n"
    );
    let shown = json(&cb(
        &w,
        &[
            "show",
            "SHOP-1",
            "--terminal",
            "--stage",
            "Validate",
            "--json",
        ],
    ));
    assert_eq!(shown["session"], check.id);
    assert_eq!(
        stdout(&cb(&w, &["show", "SHOP-1", "--terminal"])),
        "SHOP-1 is in Validate \u{b7} its terminal: --stage validate\nthe agent<end>\n"
    );
    let out = cb(&w, &["show", "SHOP-1", "--terminal", "--stage", "dev"]);
    assert_eq!(
        String::from_utf8_lossy(&out.stderr),
        "error: SHOP-1 isn't in Dev\n"
    );

    // In Security: two check agents, one terminal each.
    let reviewer = w
        .sessions
        .spawn(print(SessionKind::Validator, "the reviewer<end>"))
        .await
        .unwrap();
    w.wait_for_output(&reviewer.id, "<end>").await;
    w.validating_with_agents(
        &ticket,
        &[
            ("Security reviewer", &reviewer.id),
            ("Validator", &check.id),
        ],
    )
    .await;
    w.force_stage(&ticket, "security", Status::Checking).await;
    let out = cb(&w, &["show", "SHOP-1", "--terminal", "--stage", "security"]);
    assert_eq!(out.status.code(), Some(1));
    assert_eq!(
        String::from_utf8_lossy(&out.stderr),
        "error: Security has 2 check agents; add --agent\n"
    );
    assert_eq!(
        stdout(&cb(
            &w,
            &[
                "show",
                "SHOP-1",
                "--terminal",
                "--stage",
                "security",
                "--agent",
                "Security reviewer"
            ]
        )),
        "the reviewer<end>\n"
    );
    assert_eq!(
        stdout(&cb(
            &w,
            &[
                "show",
                "SHOP-1",
                "--terminal",
                "--stage",
                "security",
                "--agent",
                "validator"
            ]
        )),
        "the check<end>\n"
    );

    // `--validator` is gone: a usage error.
    let out = cb(&w, &["show", "SHOP-1", "--terminal", "--validator"]);
    assert_eq!(out.status.code(), Some(2));
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("unexpected argument '--validator'"),
        "{out:?}"
    );
    w.stop().await;
}
