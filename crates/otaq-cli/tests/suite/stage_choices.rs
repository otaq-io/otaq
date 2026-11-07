//! `otaq create` / `otaq update` with `--stage "<stage>=<agent|skip>"` and `--by-user`, `-f` lines, and `show` /
//! `--json` printing the choices (T-0407; AGM-11, AGM-08, CLI-06, CLI-07; d0087), on the test flow
//! `otaq-core/tests/flows/every-kind.yaml`: any stage an agent runs takes any agent whose tool has its skill, and only
//! the user picks (or Noyan passing on the user's words). The old `--developer`, `--validator`, `--lead` and `--reason`
//! are unknown flags.
//! `claude` stands in as an interactive `sh`; no test starts claude.

use std::path::Path;
use std::process::{Command, Output};

use otaq_core::agent_tools::LaunchContext;
use otaq_core::agent_tools::claude::ClaudeLaunch;
use otaq_core::board::stage_choices::{
    NOYAN_USER_CHOICES_ONLY, ONLY_NOYAN_PASSES_ON, ONLY_THE_USER_PICKS, ONLY_THE_USER_SKIPS,
};
use otaq_core::data_dir::DataDir;
use otaq_core::domain::{Actor, Status};
use otaq_core::flow::{FlowFile, yaml};
use otaq_core::git::testing::TempRepos;
use otaq_core::naming;
use otaq_core::service::ServiceConfig;
use otaq_core::service::testing::TestService;
use serde_json::{Value, json};

const CB: &str = env!("CARGO_BIN_EXE_otaq");

/// shop-web on the test flow; `claude` is an interactive `sh` in the data folder.
async fn world() -> TestService {
    let path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../otaq-core/tests/flows/every-kind.yaml");
    world_on(&yaml::read(&std::fs::read_to_string(path).unwrap()).unwrap()).await
}

/// shop-web on `file`'s flow; `claude` is an interactive `sh` in the data folder.
async fn world_on(file: &FlowFile) -> TestService {
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
    w.board.use_flow_for_tests("SHOP", file).await;
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

    // create: names ignoring case, a stage by name or id; every stage with choices in --json, each agent stage with
    // its own agent first, then the other agents (no stage lists them).
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
            "Product spec=skip",
            "--json",
        ],
    ));
    assert_eq!(created["key"], "SHOP-1");
    let choices = created["choices"].as_array().unwrap();
    let read: Vec<(&str, &str, Option<&str>)> = choices
        .iter()
        .map(|c| {
            (
                c["stage"].as_str().unwrap(),
                c["choice"].as_str().unwrap(),
                c["set_by"].as_str(),
            )
        })
        .collect();
    assert_eq!(
        read,
        [
            ("Product spec", "skip", Some("user")),
            ("Dev", "Quick developer", Some("user")),
            ("Validate", "Validator", None),
            ("Security", "Security reviewer", None),
            ("QA", "required", None),
        ]
    );
    let options = |stage: usize| -> Vec<&str> {
        choices[stage]["options"]
            .as_array()
            .unwrap()
            .iter()
            .map(|o| o.as_str().unwrap())
            .collect()
    };
    assert_eq!(options(0).first(), Some(&"Spec writer"));
    assert_eq!(options(0).last(), Some(&"skip"));
    assert_eq!(options(1).first(), Some(&"Rust developer"));
    assert!(options(1).contains(&"Quick developer") && !options(1).contains(&"skip"));
    assert!(
        !options(1).contains(&"Noyan"),
        "Noyan's own agent isn't offered"
    );
    assert_eq!(options(4), ["required", "skip"]);
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
        "Choices:     Product spec: skip · set by you\n             Dev: Quick developer · set by you"
    );

    // update: Product spec back to its own agent, QA skipped.
    assert_eq!(
        ok(&cb(
            &w,
            &[
                "update",
                "SHOP-1",
                "--stage",
                "Product spec=Spec writer",
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
            ("Product spec".to_owned(), "Spec writer".to_owned()),
            ("Dev".to_owned(), "Quick developer".to_owned()),
            ("Validate".to_owned(), "Validator".to_owned()),
            ("Security".to_owned(), "Security reviewer".to_owned()),
            ("QA".to_owned(), "skip".to_owned())
        ]
    );
    // Security lists no agents in the flow: the user picks another one all the same.
    ok(&cb(
        &w,
        &["update", "SHOP-1", "--stage", "Security=Quick developer"],
    ));

    // Refusals name the choices; nothing changes.
    assert!(
        refused(&w, &["update", "SHOP-1", "--stage", "Dev=Lite"])
            .starts_with("Lite isn't one of Dev's choices: Rust developer, ")
    );
    assert_eq!(
        refused(&w, &["update", "SHOP-1", "--stage", "QA=Quick developer"]),
        "QA runs no agent: its choices are required (you check it) and skip"
    );
    // #62: a skip of a stage the flow doesn't mark skippable says how the flow allows it, by the stage's name.
    assert_eq!(
        refused(&w, &["update", "SHOP-1", "--stage", "Security=skip"]),
        "Security can't be skipped; add `skip` to its options in the flow to allow it."
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
                "Product spec=skip"
            ]
        )
        .starts_with("-f takes everything from the file")
    );
    w.stop().await;
}

/// The old agent-row flags are gone, and so is `--reason` (only the user picks, and the user needn't say why): clap
/// doesn't know them.
#[tokio::test(flavor = "multi_thread")]
async fn the_old_flags_are_unknown() {
    let w = world().await;
    w.create("SHOP", "Fix README typo").await;
    for flag in ["--developer", "--validator", "--lead", "--reason"] {
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

/// Noyan sets only the choices the user names (T1): its own `--stage` is refused on
/// `create`, `update` and a `-f` file's Stage lines, and nothing is created; with `--by-user` the choice is the user's;
/// never over the user's; only on Backlog tickets (CLI-06).
#[tokio::test(flavor = "multi_thread")]
async fn a_project_agents_choices_and_the_users_words() {
    let w = world().await;
    let opened = w
        .project_agents
        .open("SHOP", "Look around the docs")
        .await
        .unwrap();
    let agent = w.shell_in(&opened.session.as_ref().unwrap().id);

    for own in [
        "--stage 'Dev=Quick developer'",
        "--stage 'QA=required'",
        "--stage 'Product spec=skip'",
    ] {
        let ran = agent
            .run(&format!("'{CB}' -P SHOP create 'Fix README typo' {own}"))
            .await;
        assert_ne!(ran.status, 0, "{own}");
        assert!(
            ran.stderr.contains(NOYAN_USER_CHOICES_ONLY),
            "{own}: {ran:?}"
        );
    }
    // A file with a Stage line and no --by-user: the whole batch is refused.
    let plan = w.home.path().join("plan.md");
    std::fs::write(
        &plan,
        "### Export as CSV\n\nCSV.\n\n### Fix README typo\n- **Stage:** Dev=Quick developer\n\nTypo.\n",
    )
    .unwrap();
    let ran = agent
        .run(&format!("'{CB}' -P SHOP create -f '{}'", plan.display()))
        .await;
    assert_ne!(ran.status, 0);
    assert!(ran.stderr.contains(NOYAN_USER_CHOICES_ONLY), "{ran:?}");
    assert!(w.board.tickets("SHOP").await.unwrap().is_empty());

    // The user's choice, passed on: the user's.
    let ran = agent
        .run(&format!(
            "'{CB}' -P SHOP create 'Fix README typo' --stage 'Dev=Quick developer' --by-user"
        ))
        .await;
    assert_eq!(ran.status, 0, "{ran:?}");
    let shown = ok(&cb(&w, &["show", "SHOP-1"]));
    assert_eq!(
        choices_block(&shown),
        "Choices:     Dev: Quick developer · set by you"
    );
    let shown = json_of(&cb(&w, &["show", "SHOP-1", "--json"]));
    assert_eq!(shown["choices"][1]["set_by"], "user", "{shown}");

    // The user's choice wins.
    ok(&cb(
        &w,
        &["update", "SHOP-1", "--stage", "Dev=Rust developer"],
    ));
    let ran = agent
        .run(&format!(
            "'{CB}' update SHOP-1 --stage 'Dev=Quick developer'"
        ))
        .await;
    assert_ne!(ran.status, 0);
    assert!(ran.stderr.contains(NOYAN_USER_CHOICES_ONLY), "{ran:?}");
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
    // The user's skip, passed on: theirs too.
    let ran = agent
        .run(&format!("'{CB}' update SHOP-1 --stage 'QA=skip' --by-user"))
        .await;
    assert_eq!(ran.status, 0, "{ran:?}");
    let shown = json_of(&cb(&w, &["show", "SHOP-1", "--json"]));
    let qa = shown["choices"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["stage"] == "QA")
        .unwrap();
    assert_eq!(
        (&qa["choice"], &qa["set_by"]),
        (&json!("skip"), &json!("user")),
        "{shown}"
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
        .run(&format!("'{CB}' update SHOP-2 --stage 'QA=skip' --by-user"))
        .await;
    assert_ne!(ran.status, 0);
    assert!(
        ran.stderr.contains("Noyan can only change Backlog tickets"),
        "{ran:?}"
    );
    w.stop().await;
}

/// Only the user picks a stage's agent: a ticket's own agent's `--stage` is refused, for a later stage of its own
/// ticket (`--stage Validate=Strong`) and for the tickets it created that never started; a stage that started is
/// locked first, another ticket isn't its own (CLI-06). Only the user skips a stage, and only Noyan passes on the
/// user's choices: its `--by-user` is refused (T9). Nothing is written.
#[tokio::test(flavor = "multi_thread")]
async fn a_ticket_agent_sets_no_stage_choice() {
    let w = world().await;
    let me = Actor::User("me".into());
    w.board
        .store()
        .insert_agent(
            &otaq_core::agents::store::NewAgent {
                name: "Strong".into(),
                instructions: String::new(),
                model: otaq_core::agents::AgentModel::Default,
                effort: None,
                role: None,
            },
            &otaq_core::agents::CameFrom::You,
            None,
        )
        .await
        .unwrap();
    w.create("SHOP", "Other ticket").await; // SHOP-1
    let own = w.create("SHOP", "Sessions survive a restart").await; // SHOP-2
    // Started: at work in Product spec, where Spec writer runs (the stand-in `sh`).
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
        otaq_core::domain::Place::stage_of("product-spec")
    );
    let session = w
        .sessions
        .running_of(&own.id, otaq_core::session::SessionKind::Agent)
        .expect("Spec writer runs");
    let agent = w.shell_in(&session.id);
    // The user may pick Strong for Validate; the ticket's agent may not.
    for (words, refusal) in [
        ("--stage 'Validate=Strong'", ONLY_THE_USER_PICKS),
        ("--stage 'Dev=Quick developer'", ONLY_THE_USER_PICKS),
        ("--stage 'QA=required'", ONLY_THE_USER_PICKS),
        ("--stage 'QA=skip'", ONLY_THE_USER_SKIPS),
        ("--stage 'Validate=Strong' --by-user", ONLY_NOYAN_PASSES_ON),
        ("--stage 'QA=skip' --by-user", ONLY_NOYAN_PASSES_ON),
    ] {
        let ran = agent.run(&format!("'{CB}' update SHOP-2 {words}")).await;
        assert_ne!(ran.status, 0, "{words}");
        assert!(ran.stderr.contains(refusal), "{words}: {ran:?}");
    }
    let ran = agent
        .run(&format!("'{CB}' update SHOP-2 --stage 'Product spec=skip'"))
        .await;
    assert!(
        ran.stderr
            .contains("Product spec has started on SHOP-2: its choice is locked"),
        "{ran:?}"
    );
    let ran = agent
        .run(&format!("'{CB}' update SHOP-1 --stage 'QA=skip'"))
        .await;
    assert!(
        ran.stderr.contains(
            "SHOP-1 was created by you; SHOP-2's agent can only change tickets it created"
        ),
        "{ran:?}"
    );
    // The tickets it creates are its to fix while they haven't started (d0129), but not their stage choices.
    let ran = agent
        .run(&format!("'{CB}' create 'Cart badge' --json"))
        .await;
    assert_eq!(ran.status, 0, "{ran:?}");
    for (words, refusal) in [
        ("--stage 'Dev=Strong'", ONLY_THE_USER_PICKS),
        ("--stage 'Product spec=skip'", ONLY_THE_USER_SKIPS),
    ] {
        let ran = agent.run(&format!("'{CB}' update SHOP-3 {words}")).await;
        assert_ne!(ran.status, 0, "{words}");
        assert!(ran.stderr.contains(refusal), "{words}: {ran:?}");
    }
    let ran = agent
        .run(&format!("'{CB}' create 'Wishlist' --stage 'Dev=Strong'"))
        .await;
    assert_ne!(ran.status, 0);
    assert!(ran.stderr.contains(ONLY_THE_USER_PICKS), "{ran:?}");
    for key in ["SHOP-2", "SHOP-3"] {
        let shown = ok(&cb(&w, &["show", key]));
        assert_eq!(choices_block(&shown), "", "{key}");
    }
    // The user's pick for the same stage is taken.
    ok(&cb(&w, &["update", "SHOP-2", "--stage", "Validate=Strong"]));
    let shown = ok(&cb(&w, &["show", "SHOP-2"]));
    assert_eq!(
        choices_block(&shown),
        "Choices:     Validate: Strong · set by you"
    );
    w.stop().await;
}

/// The Spec first preset's shape (its file is `flows/spec-first.yaml`): three skippable planning stages, then Spec
/// review (a skippable check by you), Dev, Validate, Code review and Done.
const SPEC_FIRST: &str = "\
agents:
  - name: Spec writer
    skills: [product-spec]
    model: default
  - name: Architect
    skills: [tech-spec]
    model: default
  - name: Milestone planner
    skills: [milestone-plan]
    model: default
  - name: Developer
    skills: [work-ticket, commit]
    model: default
  - name: Validator
    skills: [validate]
    model: default
flow:
  - {id: product-spec, name: Product spec, kind: agent, column: own, agent: Spec writer, skill: product-spec,
     options: [skip]}
  - {id: tech-spec, name: Tech spec, kind: agent, column: own, agent: Architect, skill: tech-spec, options: [skip]}
  - {id: milestone-plan, name: Milestone plan, kind: agent, column: own, agent: Milestone planner,
     skill: milestone-plan, options: [skip]}
  - {id: spec-review, name: Spec review, kind: check, by: you, column: own, options: [skip], fail_to: Product spec}
  - {id: dev, name: Dev, kind: agent, column: own, agent: Developer, skill: work-ticket}
  - {id: validate, name: Validate, kind: check, by: agent, column: own, agent: Validator, skill: validate,
     rounds: 3, fail_to: Dev}
  - {id: code-review, name: Code review, kind: check, by: you, column: own, fail_to: Dev}
  - {id: deliver, name: Done, kind: delivery, column: with-previous, agent: Developer, skill: open-pr, mode: click,
     delivers: either}
";

/// On Spec first, nothing is skipped on its own: `--stage` skips the three planning stages and Spec review still
/// reads "required" (`show` lists only the choices made, `--json` all), and the ticket reaches Spec review and waits
/// there; "you" is no choice: `--stage "Spec review=you"` is refused as an unknown choice, on `create` (nothing is
/// created) and on `update`, while `--stage "Spec review=required"` is taken.
#[tokio::test(flavor = "multi_thread")]
async fn planning_skips_on_spec_first() {
    let w = world_on(&yaml::read(SPEC_FIRST).unwrap()).await;
    let planning = [
        "--stage",
        "Product spec=skip",
        "--stage",
        "Tech spec=skip",
        "--stage",
        "Milestone plan=skip",
    ];
    let mut args = vec!["-P", "SHOP", "create", "Fix cart total rounding"];
    args.extend(planning);
    args.push("--json");
    let created = json_of(&cb(&w, &args));
    let choices: Vec<(&str, &str)> = created["choices"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| (c["stage"].as_str().unwrap(), c["choice"].as_str().unwrap()))
        .collect();
    assert_eq!(
        choices,
        [
            ("Product spec", "skip"),
            ("Tech spec", "skip"),
            ("Milestone plan", "skip"),
            ("Spec review", "required"),
            ("Dev", "Developer"),
            ("Validate", "Validator"),
        ]
    );
    let shown = ok(&cb(&w, &["show", "SHOP-1"]));
    assert_eq!(
        choices_block(&shown),
        "Choices:     Product spec: skip · set by you\n             Tech spec: skip · set by you\n             \
         Milestone plan: skip · set by you"
    );

    // Started, it walks past the three into Spec review and waits there for you.
    let me = Actor::User("me".into());
    w.board
        .move_ticket(
            "SHOP-1",
            otaq_core::board::moves::DropTarget::Dev,
            true,
            &me,
        )
        .await
        .unwrap();
    let started = w
        .starter
        .start("SHOP-1", &me, otaq_core::session::Start::Auto)
        .await
        .unwrap();
    assert_eq!(started, otaq_core::start::StartOutcome::InCheck);
    let ticket = w.board.ticket("SHOP-1").await.unwrap();
    assert_eq!(
        (ticket.place, ticket.status),
        (
            otaq_core::domain::Place::stage_of("spec-review"),
            Status::WaitingForYou
        )
    );

    // "you" is an unknown choice: refused, nothing created or changed.
    let unknown = "Spec review runs no agent: its choices are required (you check it) and skip";
    let mut args = vec!["-P", "SHOP", "create", "Saved carts"];
    args.extend(planning);
    args.extend(["--stage", "Spec review=you"]);
    assert_eq!(refused(&w, &args), unknown);
    assert_eq!(w.board.tickets("SHOP").await.unwrap().len(), 1);
    assert_eq!(
        refused(&w, &["update", "SHOP-1", "--stage", "Spec review=you"]),
        unknown
    );

    // Waiting in the waiting column: Spec first names it "Ready" (d0093, Q515). A parent that hasn't merged keeps it
    // from starting.
    let mut args = vec!["-P", "SHOP", "create", "Saved carts"];
    args.extend(planning);
    args.extend(["--stage", "Spec review=required"]);
    ok(&cb(&w, &args));
    let waiting = w
        .board
        .add_dependency("SHOP-2", "SHOP-1", &Actor::User("me".into()))
        .await
        .unwrap();
    w.force_status(&waiting, Status::Queued).await;
    let shown = ok(&cb(&w, &["show", "SHOP-2"]));
    assert!(shown.contains("Status:      Ready\n"), "{shown}");
    w.stop().await;
}
