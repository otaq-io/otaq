//! `otaq create` / `otaq update` with `--stage "<stage>=<agent|skip>"`, `--reason` and `--by-user`, `-f` lines, and
//! `show` / `--json` printing the choices (T-0407; AGM-11, AGM-08, CLI-06, CLI-07; d0108, d0087), on the test flow
//! `otaq-core/tests/flows/every-kind.yaml`. The old `--developer`, `--validator` and `--lead` are unknown flags.
//! `claude` stands in as an interactive `sh`; no test starts claude.

use std::path::Path;
use std::process::{Command, Output};

use otaq_core::agent_tools::LaunchContext;
use otaq_core::agent_tools::claude::ClaudeLaunch;
use otaq_core::board::tickets::{NewTicketInput, Ticket};
use otaq_core::data_dir::DataDir;
use otaq_core::domain::{Actor, Kind, Status};
use otaq_core::flow::yaml;
use otaq_core::git::testing::TempRepos;
use otaq_core::naming;
use otaq_core::service::ServiceConfig;
use otaq_core::service::testing::TestService;
use serde_json::{Value, json};

const CB: &str = env!("CARGO_BIN_EXE_otaq");

/// shop-web on the test flow; `claude` is an interactive `sh` in the data folder.
async fn world() -> TestService {
    let home = tempfile::TempDir::new().unwrap();
    let data = home.path().join("h");
    let script = format!(
        "export {}='{}'; export PS1=''; exec sh",
        DataDir::env_var(),
        data.display()
    );
    let w = TestService::start_in(home, TempRepos::new(), |config| ServiceConfig {
        launch: Some(LaunchContext {
            bin_dir: "/nonexistent".into(),
            plugin_dir: "/nonexistent/plugin".into(),
            path: std::env::var_os("PATH").unwrap_or_default(),
            claude: ClaudeLaunch {
                program: vec!["sh".into(), "-c".into(), script, "sh".into()],
                config_dir: None,
            },
        }),
        ..config
    })
    .await;
    w.add_project("shop-web").await;
    let path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../otaq-core/tests/flows/every-kind.yaml");
    let file = yaml::read(&std::fs::read_to_string(path).unwrap()).unwrap();
    w.board.use_flow_for_tests("SHOP", &file).await;
    w
}

fn cb(w: &TestService, args: &[&str]) -> Output {
    Command::new(CB)
        .args(args)
        .env(DataDir::env_var(), w.data.root())
        .env_remove(naming::ticket_env_var())
        .current_dir(w.home.path())
        .output()
        .unwrap()
}

fn ok(out: &Output) -> String {
    assert!(
        out.status.success(),
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout.clone()).unwrap()
}

fn json_of(out: &Output) -> Value {
    serde_json::from_str(&ok(out)).unwrap()
}

/// The error of a failed `--json` command: (kind, message).
fn error(out: &Output) -> (String, String) {
    assert!(!out.status.success());
    let e = &serde_json::from_slice::<Value>(&out.stdout).unwrap()["error"];
    (
        e["kind"].as_str().unwrap().to_owned(),
        e["message"].as_str().unwrap().to_owned(),
    )
}

fn refused(w: &TestService, args: &[&str]) -> String {
    let mut args = args.to_vec();
    args.push("--json");
    let (kind, message) = error(&cb(w, &args));
    assert_eq!(kind, "refused", "{message}");
    message
}

fn usage(w: &TestService, args: &[&str]) -> String {
    let mut args = args.to_vec();
    args.push("--json");
    let (kind, message) = error(&cb(w, &args));
    assert_eq!(kind, "usage", "{message}");
    message
}

/// The "Choices:" block of `show`.
fn choices_block(shown: &str) -> String {
    let start = shown.find("Choices:").unwrap_or(shown.len());
    let rest = &shown[start..];
    let end = rest.find("\n\n").unwrap_or(rest.len());
    rest[..end].trim_end().to_owned()
}

#[tokio::test(flavor = "multi_thread")]
async fn the_users_choices_round_trip() {
    let w = world().await;

    // create: names ignoring case, a stage by name or id; every stage with options in --json.
    let created = json_of(&cb(
        &w,
        &[
            "-P",
            "SHOP",
            "create",
            "Fix README typo",
            "--stage",
            "dev=quick developer",
            "--stage",
            "Grill=skip",
            "--json",
        ],
    ));
    assert_eq!(created["key"], "SHOP-1");
    assert_eq!(
        created["choices"],
        json!([
            {"stage": "Grill", "choice": "skip", "set_by": "user", "by": "you", "locked": false,
             "options": ["Griller", "skip"]},
            {"stage": "Dev", "choice": "Quick developer", "set_by": "user", "by": "you", "locked": false,
             "options": ["Rust developer", "Quick developer"]},
            {"stage": "QA", "choice": "required", "locked": false, "options": ["required", "skip"]},
        ])
    );
    let dev = created["agents"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["stage"] == "Dev")
        .unwrap()
        .clone();
    assert_eq!(dev, json!({"stage": "Dev", "agent": "Quick developer"}));
    let shown = ok(&cb(&w, &["show", "SHOP-1"]));
    assert_eq!(
        choices_block(&shown),
        "Choices:     Grill: skip · set by you\n             Dev: Quick developer · set by you"
    );

    // update: Grill back to required, QA skipped.
    assert_eq!(
        ok(&cb(
            &w,
            &[
                "update",
                "SHOP-1",
                "--stage",
                "Grill=required",
                "--stage",
                "qa=skip"
            ]
        )),
        "Updated SHOP-1 · Fix README typo\n"
    );
    let shown = json_of(&cb(&w, &["show", "SHOP-1", "--json"]));
    let choices: Vec<(String, String)> = shown["choices"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| {
            (
                c["stage"].as_str().unwrap().to_owned(),
                c["choice"].as_str().unwrap().to_owned(),
            )
        })
        .collect();
    assert_eq!(
        choices,
        [
            ("Grill".to_owned(), "Griller".to_owned()),
            ("Dev".to_owned(), "Quick developer".to_owned()),
            ("QA".to_owned(), "skip".to_owned())
        ]
    );

    // Refusals name the options; nothing changes.
    assert_eq!(
        refused(&w, &["update", "SHOP-1", "--stage", "Dev=Lite"]),
        "Lite isn't one of Dev's options: Rust developer, Quick developer"
    );
    assert_eq!(
        refused(
            &w,
            &["update", "SHOP-1", "--stage", "Security=Quick developer"]
        ),
        "Security has no options: every ticket runs it as the flow says"
    );
    // #62: a skip of a stage without options says how the flow allows it, by the stage's name.
    assert_eq!(
        refused(&w, &["update", "SHOP-1", "--stage", "Security=skip"]),
        "Security has no options; add `skip` to its options in the flow to allow it."
    );
    let count = w.board.tickets("SHOP").await.unwrap().len();
    error(&cb(
        &w,
        &[
            "-P", "SHOP", "create", "Nope", "--stage", "Dev=Lite", "--json",
        ],
    ));
    assert_eq!(w.board.tickets("SHOP").await.unwrap().len(), count);

    // Usage errors.
    assert_eq!(
        usage(&w, &["update", "SHOP-1", "--reason", "x"]),
        "--reason goes with --stage"
    );
    assert_eq!(
        usage(&w, &["update", "SHOP-1", "--by-user"]),
        "--by-user goes with --stage (or -f)"
    );
    assert!(
        usage(&w, &["update", "SHOP-1", "--stage", "Dev"]).starts_with("give a stage choice as")
    );

    // A file: "- **Stage:**" lines; `--stage` doesn't go with -f.
    let plan = w.home.path().join("plan.md");
    std::fs::write(
        &plan,
        "### Export as CSV\n- **Stage:** Dev=Quick developer\n- **Stage:** QA=skip\n\nCSV.\n",
    )
    .unwrap();
    ok(&cb(
        &w,
        &["-P", "SHOP", "create", "-f", plan.to_str().unwrap()],
    ));
    let shown = ok(&cb(&w, &["show", "SHOP-2"]));
    assert_eq!(
        choices_block(&shown),
        "Choices:     Dev: Quick developer · set by you\n             QA: skip · set by you"
    );
    assert!(
        usage(
            &w,
            &[
                "-P",
                "SHOP",
                "create",
                "-f",
                plan.to_str().unwrap(),
                "--stage",
                "Grill=skip"
            ]
        )
        .starts_with("-f takes everything from the file")
    );
    w.stop().await;
}

/// The old agent-row flags are gone: clap doesn't know them.
#[tokio::test(flavor = "multi_thread")]
async fn the_old_flags_are_unknown() {
    let w = world().await;
    w.create("SHOP", "Fix README typo").await;
    for flag in ["--developer", "--validator", "--lead"] {
        for args in [
            vec!["-P", "SHOP", "create", "T", flag, "Light"],
            vec!["update", "SHOP-1", flag, "Light"],
        ] {
            let out = cb(&w, &args);
            assert_eq!(out.status.code(), Some(2), "{args:?}");
            let stderr = String::from_utf8_lossy(&out.stderr);
            assert!(
                stderr.contains(&format!("unexpected argument '{flag}'")),
                "{stderr}"
            );
        }
    }
    w.stop().await;
}

/// A Project agent sets choices with a reason ("set by the Project agent · typo fix"); never over the user's, except
/// with `--by-user`; only on Backlog tickets (CLI-06).
#[tokio::test(flavor = "multi_thread")]
async fn a_project_agents_choices_and_the_users_words() {
    let w = world().await;
    let opened = w
        .project_agents
        .open("SHOP", "Look around the docs")
        .await
        .unwrap();
    let agent = w.shell_in(&opened.session.as_ref().unwrap().id);

    let ran = agent
        .run(&format!(
            "'{CB}' -P SHOP create 'Fix README typo' --stage 'Dev=Quick developer' --reason 'typo fix'"
        ))
        .await;
    assert_eq!(ran.status, 0, "{ran:?}");
    let shown = ok(&cb(&w, &["show", "SHOP-1"]));
    assert_eq!(
        choices_block(&shown),
        "Choices:     Dev: Quick developer · set by the Project agent · typo fix"
    );
    let shown = json_of(&cb(&w, &["show", "SHOP-1", "--json"]));
    assert_eq!(
        shown["choices"][1],
        json!({"stage": "Dev", "choice": "Quick developer", "set_by": "project_agent",
               "by": "the Project agent", "reason": "typo fix", "locked": false,
               "options": ["Rust developer", "Quick developer"]})
    );
    // Without a reason: refused, nothing created.
    let ran = agent
        .run(&format!(
            "'{CB}' -P SHOP create 'Other' --stage 'Grill=skip'"
        ))
        .await;
    assert_ne!(ran.status, 0);
    assert!(ran.stderr.contains("Say why with --reason"), "{ran:?}");
    assert_eq!(w.board.tickets("SHOP").await.unwrap().len(), 1);

    // The user's choice wins.
    ok(&cb(
        &w,
        &["update", "SHOP-1", "--stage", "Dev=Rust developer"],
    ));
    let ran = agent
        .run(&format!(
            "'{CB}' update SHOP-1 --stage 'Dev=Quick developer' --reason 'typo fix'"
        ))
        .await;
    assert_ne!(ran.status, 0);
    assert!(
        ran.stderr
            .contains("SHOP-1's Dev choice was set by you; agents can't change it"),
        "{ran:?}"
    );
    // The user's own words, passed on: theirs.
    let ran = agent
        .run(&format!(
            "'{CB}' update SHOP-1 --stage 'Dev=Quick developer' --by-user"
        ))
        .await;
    assert_eq!(ran.status, 0, "{ran:?}");
    let shown = ok(&cb(&w, &["show", "SHOP-1"]));
    assert_eq!(
        choices_block(&shown),
        "Choices:     Dev: Quick developer · set by you"
    );

    // Only Backlog tickets (CLI-06).
    ok(&cb(&w, &["-P", "SHOP", "create", "Queued one"]));
    w.board
        .move_ticket(
            "SHOP-2",
            otaq_core::board::moves::DropTarget::ReadyForDev,
            true,
            &Actor::User("me".into()),
        )
        .await
        .unwrap();
    let ran = agent
        .run(&format!(
            "'{CB}' update SHOP-2 --stage 'QA=skip' --reason 'docs only'"
        ))
        .await;
    assert_ne!(ran.status, 0);
    assert!(
        ran.stderr
            .contains("Project agents can only change Backlog tickets"),
        "{ran:?}"
    );
    w.stop().await;
}

/// A ticket's agent sets its own ticket's later stages with a reason (AGM-11), not a stage that started, nor another
/// ticket; an epic lead its unstarted children (CLI-07), logged on the epic too.
#[tokio::test(flavor = "multi_thread")]
async fn a_ticket_agent_and_an_epic_lead() {
    let w = world().await;
    let me = Actor::User("me".into());
    let other = w.create("SHOP", "Other ticket").await; // SHOP-1
    let own = w.create("SHOP", "Sessions survive a restart").await; // SHOP-2
    // Started: at work in Grill, where Griller runs (the stand-in `sh`).
    w.board
        .move_ticket(
            "SHOP-2",
            otaq_core::board::moves::DropTarget::ReadyForDev,
            true,
            &me,
        )
        .await
        .unwrap();
    w.starter
        .start("SHOP-2", &me, otaq_core::session::Start::Auto)
        .await
        .unwrap();
    for _ in 0..500 {
        if w.status("SHOP-2").await == Status::Working {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    assert_eq!(
        w.board.ticket("SHOP-2").await.unwrap().place,
        otaq_core::domain::Place::stage_of("grill")
    );
    let session = w
        .sessions
        .running_of(&own.id, otaq_core::session::SessionKind::Agent)
        .expect("Griller runs");
    let agent = w.shell_in(&session.id);
    let ran = agent
        .run(&format!(
            "'{CB}' update SHOP-2 --stage 'Dev=Quick developer' --reason 'small change'"
        ))
        .await;
    assert_eq!(ran.status, 0, "{ran:?}");
    let shown = ok(&cb(&w, &["show", "SHOP-2"]));
    assert_eq!(
        choices_block(&shown),
        "Choices:     Dev: Quick developer · set by Griller · small change"
    );
    let ran = agent
        .run(&format!(
            "'{CB}' update SHOP-2 --stage 'Grill=skip' --reason 'x'"
        ))
        .await;
    assert!(
        ran.stderr
            .contains("Grill has started on SHOP-2: its choice is locked"),
        "{ran:?}"
    );
    let ran = agent
        .run(&format!(
            "'{CB}' update SHOP-1 --stage 'QA=skip' --reason 'x'"
        ))
        .await;
    assert!(
        ran.stderr.contains("SHOP-2's agent can't change SHOP-1"),
        "{ran:?}"
    );
    let _ = other;

    // An epic lead: its unstarted child, with a reason; logged on the epic too.
    let epic = w
        .board
        .create_ticket(
            &NewTicketInput {
                project: "SHOP".into(),
                title: "Wishlist".into(),
                kind: Kind::Epic,
                ..Default::default()
            },
            &me,
        )
        .await
        .unwrap();
    let child: Ticket = w
        .board
        .create_ticket(
            &NewTicketInput {
                project: "SHOP".into(),
                title: "Wishlist API".into(),
                epic: Some(epic.key.to_string()),
                ..Default::default()
            },
            &me,
        )
        .await
        .unwrap();
    let lead = w
        .project_agents
        .leads()
        .open(&epic.key.to_string())
        .await
        .unwrap();
    let shell = w.shell_in(&lead.agent.session.as_ref().unwrap().id);
    let ran = shell
        .run(&format!(
            "'{CB}' update {} --stage 'Grill=skip' --reason 'the API is agreed'",
            child.key
        ))
        .await;
    assert_eq!(ran.status, 0, "{ran:?}");
    let shown = ok(&cb(&w, &["show", &child.key.to_string()]));
    assert_eq!(
        choices_block(&shown),
        "Choices:     Grill: skip · set by Wishlist lead · the API is agreed"
    );
    let history = w.board.history(&epic.key.to_string()).await.unwrap();
    assert!(
        history.iter().any(|e| {
            e.kind == otaq_core::board::tickets::EventKind::StageChoiceSet
                && e.payload["child"] == child.key.to_string()
        }),
        "{history:?}"
    );
    w.stop().await;
}
