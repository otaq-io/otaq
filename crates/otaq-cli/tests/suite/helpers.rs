//! `otaq helper start|list|diff|back|merge|done` and a helper's `otaq prime` (T-0537; d0133): each command's reply
//! and refusal, from the agent in charge's session, a helper's session and the user's own terminal. A helper's tool is
//! a plain `sh`, so commands typed into it run as the helper; repositories and `OTAQ_HOME` are temporary.

use std::path::Path;
use std::process::Command as Process;

use otaq_core::agent_tools::LaunchContext;
use otaq_core::agent_tools::claude::ClaudeLaunch;
use otaq_core::board::tickets::Ticket;
use otaq_core::domain::Status;
use otaq_core::service::ServiceConfig;
use otaq_core::service::testing::{AgentShell, TestService, path_without_build_folders};
use otaq_core::session::SessionKind;
use serde_json::Value;

const CB: &str = env!("CARGO_BIN_EXE_otaq");

fn git(dir: &Path, args: &[&str]) {
    let ok = Process::new("git")
        .args([
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@t",
            "-c",
            "commit.gpgsign=false",
        ])
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap()
        .status
        .success();
    assert!(ok, "git {args:?}");
}

fn commit(dir: &Path, file: &str, content: &str) {
    std::fs::write(dir.join(file), content).unwrap();
    git(dir, &["add", "--", file]);
    git(dir, &["commit", "--quiet", "-m", &format!("edit {file}")]);
}

/// A helper's tool is an interactive `sh`: what is typed into its session runs as the helper.
fn helpers_are_shells(config: ServiceConfig) -> ServiceConfig {
    ServiceConfig {
        launch: Some(LaunchContext {
            bin_dir: "/nonexistent".into(),
            plugin_dir: "/nonexistent/plugin".into(),
            path: std::env::var_os("PATH").unwrap_or_default(),
            claude: ClaudeLaunch {
                program: vec!["sh".into(), "-c".into(), "exec sh".into(), "sh".into()],
                config_dir: None,
            },
        }),
        ..config
    }
}

/// SHOP-1 at work in Dev with its branch and worktree, and its agent in charge's shell.
async fn at_work(w: &TestService) -> (Ticket, std::path::PathBuf, AgentShell) {
    w.add_project("shop-web").await;
    let t = w.create("SHOP", "Wishlist").await;
    let worktree = w.start_worktree(&t).await;
    w.force_status(&t, Status::Working).await;
    let agent = w.agent_shell(&t).await;
    (t, worktree, agent)
}

/// The helper's own session as a shell, told where the data folder is.
async fn helper_shell(w: &TestService, session: &str) -> AgentShell {
    let shell = w.shell_in(session);
    let ran = shell
        .run(&format!(
            "export {}='{}' PATH='{}'",
            otaq_core::data_dir::DataDir::env_var(),
            w.data.root().display(),
            path_without_build_folders()
        ))
        .await;
    assert_eq!(ran.status, 0, "{ran:?}");
    shell
}

#[tokio::test(flavor = "multi_thread")]
async fn the_agent_in_charge_starts_lists_reads_sends_back_and_merges() {
    let w = TestService::start_with(helpers_are_shells).await;
    let (t, worktree, agent) = at_work(&w).await;

    let ran = agent
        .run_with_stdin(
            &format!("'{CB}' helper start 1 --title 'Wishlist API' --stdin"),
            "Endpoints only; the page is piece 2.",
        )
        .await;
    assert_eq!(ran.status, 0, "{ran:?}");
    assert!(
        ran.stdout
            .starts_with("Started 1.1's helper (Wishlist API) on otaq/SHOP-1.1, in "),
        "{ran:?}"
    );
    let ran = agent
        .run(&format!(
            "'{CB}' helper start 2 --title 'Wishlist page' --json"
        ))
        .await;
    let started: Value = serde_json::from_str(&ran.stdout).unwrap();
    assert_eq!(started["helper"]["piece"], 2);
    assert_eq!(started["helper"]["status"], "working");
    assert_eq!(started["helper"]["branch"], "otaq/SHOP-1.2");
    // No title: clap refuses it.
    let ran = agent.run(&format!("'{CB}' helper start 3")).await;
    assert_ne!(ran.status, 0);

    let ran = agent.run(&format!("'{CB}' helper list")).await;
    assert_eq!(ran.status, 0, "{ran:?}");
    let lines: Vec<&str> = ran.stdout.lines().collect();
    assert_eq!(lines.len(), 2, "{ran:?}");
    assert!(lines[0].starts_with("1.1  Wishlist API "), "{ran:?}");
    assert!(
        lines[0].contains(" working ") && lines[0].contains("otaq/SHOP-1.1"),
        "{ran:?}"
    );
    let helpers = w.helpers.of_ticket(&t.id).await.unwrap();
    // The worktree last: the agent in charge runs the project's tests there (T-0539).
    assert!(
        lines[0].ends_with(&format!("just now  {}", helpers[0].worktree)),
        "{ran:?}"
    );
    let one = Path::new(&helpers[0].worktree);
    commit(one, "wishlist.rs", "pub fn add() {}\n");
    let ran = agent.run(&format!("'{CB}' helper diff 1")).await;
    assert_eq!(ran.status, 0, "{ran:?}");
    assert!(ran.stdout.contains("+pub fn add() {}"), "{ran:?}");
    let ran = agent.run(&format!("'{CB}' helper diff 2")).await;
    assert_eq!(ran.stdout.trim(), "1.2 has no changes against otaq/SHOP-1.");

    let ran = agent.run(&format!("'{CB}' helper back 1")).await;
    assert_ne!(ran.status, 0);
    assert!(ran.stderr.contains("helper back 1 --stdin"), "{ran:?}");
    let ran = agent
        .run_with_stdin(&format!("'{CB}' helper back 1 --stdin"), "Add a test.")
        .await;
    assert_eq!(ran.status, 0, "{ran:?}");
    assert_eq!(
        ran.stdout.trim(),
        "Sent 1.1 back: your note is typed into its session as soon as it is idle."
    );

    std::fs::write(worktree.join("notes.tmp"), "x").unwrap();
    let ran = agent.run(&format!("'{CB}' helper merge 1")).await;
    assert_ne!(ran.status, 0);
    assert_eq!(ran.stderr.trim(), "error: SHOP-1 has 1 uncommitted file");
    std::fs::remove_file(worktree.join("notes.tmp")).unwrap();
    let ran = agent.run(&format!("'{CB}' helper merge 1")).await;
    assert_eq!(ran.status, 0, "{ran:?}");
    assert_eq!(
        ran.stdout.trim(),
        "Merged 1.1 (Wishlist API) into otaq/SHOP-1: 1 file. Its worktree is removed; its branch stays until SHOP-1 is Done."
    );
    let ran = agent.run(&format!("'{CB}' helper merge 1")).await;
    assert_ne!(ran.status, 0);
    assert_eq!(ran.stderr.trim(), "error: 1.1 is merged already");
    w.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_helper_hands_back_reads_its_piece_and_moves_nothing() {
    let w = TestService::start_with(helpers_are_shells).await;
    let (t, _, agent) = at_work(&w).await;
    let ran = agent
        .run(&format!("'{CB}' helper start 1 --title 'Wishlist API'"))
        .await;
    assert_eq!(ran.status, 0, "{ran:?}");
    let helper = w.helpers.of_ticket(&t.id).await.unwrap().remove(0);
    let me = helper_shell(&w, helper.session.as_deref().unwrap()).await;

    let ran = me.run("printf '%s' \"$OTAQ_PIECE\"").await;
    assert_eq!(ran.stdout, "1");
    let ran = me.run(&format!("'{CB}' prime")).await;
    assert_eq!(ran.status, 0, "{ran:?}");
    assert!(ran.stdout.starts_with("# 1.1 · Wishlist API"), "{ran:?}");
    assert!(
        ran.stdout
            .contains("You are the helper for piece 1 of SHOP-1 (Wishlist)."),
        "{ran:?}"
    );

    let refused = "error: a helper works on its piece; the agent in charge moves the ticket";
    for command in [
        "stage next --stdin",
        "stage done --stdin",
        "check pass --stdin",
        "helper start 2 --title 'Wishlist page'",
        "update --title Renamed",
    ] {
        let ran = me
            .run_with_stdin(&format!("'{CB}' {command}"), "Done.")
            .await;
        assert_ne!(ran.status, 0, "{command}");
        assert_eq!(ran.stderr.trim(), refused, "{command}: {ran:?}");
    }

    let ran = me.run(&format!("'{CB}' helper done")).await;
    assert_ne!(ran.status, 0);
    assert!(ran.stderr.contains("helper done --stdin"), "{ran:?}");
    std::fs::write(Path::new(&helper.worktree).join("wip.rs"), "x").unwrap();
    let ran = me
        .run_with_stdin(&format!("'{CB}' helper done --stdin"), "API done.")
        .await;
    assert_ne!(ran.status, 0);
    assert_eq!(
        ran.stderr.trim(),
        "error: 1 file is not committed. Commit it, or discard it if it's a scratch file"
    );
    commit(Path::new(&helper.worktree), "wip.rs", "x");
    let ran = me
        .run_with_stdin(&format!("'{CB}' helper done --stdin"), "API done.")
        .await;
    assert_eq!(ran.status, 0, "{ran:?}");
    assert!(
        ran.stdout
            .starts_with("Handed 1.1 back to the agent in charge."),
        "{ran:?}"
    );
    // The agent in charge can't hand back.
    let ran = agent
        .run_with_stdin(&format!("'{CB}' helper done --stdin"), "x")
        .await;
    assert_ne!(ran.status, 0);
    assert_eq!(
        ran.stderr.trim(),
        "error: Only a helper hands back its piece"
    );
    w.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn the_user_and_other_sessions_are_refused() {
    let w = TestService::start_with(helpers_are_shells).await;
    let (t, _, _agent) = at_work(&w).await;
    let in_charge =
        "error: Only the agent in charge of SHOP-1 starts, sends back and merges its helpers";
    // The user's own terminal (the ticket's Shell).
    let user = w.shell(&t, SessionKind::Shell).await;
    for command in ["helper list", "helper start 1 --title x", "helper merge 1"] {
        let ran = user.run(&format!("'{CB}' {command}")).await;
        assert_ne!(ran.status, 0, "{command}");
        assert_eq!(ran.stderr.trim(), in_charge, "{command}: {ran:?}");
    }
    // Another ticket's agent.
    let other = w.create("SHOP", "Coupons").await;
    let theirs = w.agent_shell(&other).await;
    let ran = theirs
        .run(&format!("OTAQ_TICKET=SHOP-1 '{CB}' helper list"))
        .await;
    assert_ne!(ran.status, 0);
    assert_eq!(
        ran.stderr.trim(),
        "error: SHOP-2's agent can't run SHOP-1's helpers"
    );
    w.stop().await;
}

/// T-0538 (d0133 audit 3): a helper's `otaq ask human` goes to the agent in charge, who passes one question on with
/// `helper pass-on <id> --note` (the card turns turquoise) and answers the next with `helper answer <n> --stdin`.
#[tokio::test(flavor = "multi_thread")]
async fn a_helpers_question_goes_to_the_agent_in_charge() {
    let w = TestService::start_with(helpers_are_shells).await;
    let (t, _, agent) = at_work(&w).await;
    let ran = agent
        .run(&format!("'{CB}' helper start 2 --title 'Wishlist button'"))
        .await;
    assert_eq!(ran.status, 0, "{ran:?}");
    let helper = w.helpers.of_ticket(&t.id).await.unwrap().remove(0);
    let me = helper_shell(&w, helper.session.as_deref().unwrap()).await;

    let ran = me
        .run_with_stdin(
            &format!(
                "'{CB}' ask human --option 'Logged-in users only' --option 'Guests too' --stdin"
            ),
            "Can guests save items?",
        )
        .await;
    assert_eq!(ran.status, 0, "{ran:?}");
    assert!(
        ran.stdout
            .starts_with("Asked the agent in charge (1.2's question)"),
        "{ran:?}"
    );
    assert_eq!(w.status("SHOP-1").await, Status::Working);
    let ran = agent.run(&format!("'{CB}' helper list")).await;
    assert!(
        ran.stdout.contains("asked \u{b7} in charge is answering"),
        "{ran:?}"
    );

    let id = otaq_core::helpers::questions::open_of(w.board.store().pool(), &helper.id)
        .await
        .unwrap()
        .unwrap()
        .question
        .id;
    let ran = agent
        .run(&format!(
            "'{CB}' helper pass-on {id} --note \"it's a product question the plan doesn't answer\""
        ))
        .await;
    assert_eq!(ran.status, 0, "{ran:?}");
    assert!(
        ran.stdout
            .starts_with("Passed 1.2's question on to the user."),
        "{ran:?}"
    );
    let card = w.board.ticket("SHOP-1").await.unwrap();
    assert_eq!(card.status, Status::AsksYou);
    assert_eq!(
        card.flags.piece_wait.unwrap().note.as_deref(),
        Some("passed this to you: it's a product question the plan doesn't answer")
    );
    // A helper can't pass its own question on.
    let ran = me.run(&format!("'{CB}' helper pass-on {id}")).await;
    assert_ne!(ran.status, 0, "{ran:?}");

    let ran = me
        .run_with_stdin(&format!("'{CB}' ask human --stdin"), "Which icon?")
        .await;
    assert_eq!(ran.status, 0, "{ran:?}");
    let ran = agent.run(&format!("'{CB}' helper answer 2")).await;
    assert_ne!(ran.status, 0);
    assert!(ran.stderr.contains("helper answer 2 --stdin"), "{ran:?}");
    let ran = agent
        .run_with_stdin(&format!("'{CB}' helper answer 2 --stdin"), "A heart.")
        .await;
    assert_eq!(ran.status, 0, "{ran:?}");
    assert!(ran.stdout.starts_with("Answered 1.2's question"), "{ran:?}");
    w.stop().await;
}
