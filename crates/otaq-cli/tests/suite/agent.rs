//! `otaq ask human`, `otaq stage next` / `review submit` and `otaq stage done` (T-0072, T-0406; ATT-01, REV-07,
//! AGM-10) from inside an agent session, with a temporary git repository as the ticket's worktree.

use std::path::Path;
use std::process::Command as Process;

use otaq_core::domain::Status;
use otaq_core::service::testing::{AgentShell, TestService};
use serde_json::{Value, json};

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

/// The pre-M12 path: shop-web's flow has no Validate, so a submit goes straight to Ready for Review (d0063).
async fn validation_off(w: &TestService) {
    w.board
        .edit_flow_for_tests("SHOP", otaq_core::flow::default::testing::without_validate)
        .await;
}

/// The agent's `otaq prime`, which ending its stage waits for (CMT-08).
async fn prime(agent: &AgentShell) {
    let ran = agent.run(&format!("'{CB}' prime")).await;
    assert_eq!(ran.status, 0, "{ran:?}");
}

async fn stop_hook(agent: &AgentShell) {
    let payload = json!({"hook_event_name": "Stop", "background_tasks": [], "session_crons": []});
    let ran = agent
        .run_with_stdin(
            &format!("'{CB}' hook --tool claude Stop"),
            &payload.to_string(),
        )
        .await;
    assert_eq!(ran.status, 0);
}

#[tokio::test(flavor = "multi_thread")]
async fn ask_human_shows_asks_you_with_the_question() {
    let w = TestService::start().await;
    w.shop_sample().await;
    let shop19 = w.board.ticket("SHOP-19").await.unwrap();
    let agent = w.agent_shell(&shop19).await;

    let question = "Should `fullName` be split into first and last name everywhere, or only in the profile form?";
    let ran = agent
        .run_with_stdin(&format!("'{CB}' ask human --stdin --json"), question)
        .await;
    assert_eq!(ran.status, 0, "{ran:?}");
    let ticket: Value = serde_json::from_str(&ran.stdout).unwrap();
    assert_eq!(ticket["status"], "asks_you");
    assert_eq!(ticket["signal"]["text"], question);

    // The turn's Stop keeps Asks you (not Idle).
    stop_hook(&agent).await;
    assert_eq!(w.status("SHOP-19").await, Status::AsksYou);

    // An empty question or a missing --stdin is refused.
    let ran = agent
        .run_with_stdin(&format!("'{CB}' ask human --stdin --json"), "  \n")
        .await;
    assert_ne!(ran.status, 0);
    let ran = agent.run(&format!("'{CB}' ask human")).await;
    assert_ne!(ran.status, 0);
    assert!(ran.stderr.contains("--stdin"), "{ran:?}");
    let ran = agent
        .run_with_stdin(&format!("'{CB}' ask SHOP-18 --stdin"), "x")
        .await;
    assert_ne!(ran.status, 0);
    w.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn review_submit_refuses_uncommitted_files_then_submits() {
    let w = TestService::start().await;
    w.shop_sample().await;
    validation_off(&w).await;
    let shop19 = w.board.ticket("SHOP-19").await.unwrap();
    let worktree = w.give_worktree(&shop19).await;
    let agent = w.agent_shell(&shop19).await;
    prime(&agent).await;
    let submit = format!("'{CB}' review submit --stdin");

    // A modified tracked file.
    std::fs::write(worktree.join("file1.txt"), "changed\n").unwrap();
    let ran = agent.run_with_stdin(&submit, "Price table done.").await;
    assert_ne!(ran.status, 0);
    assert_eq!(
        ran.stderr.trim(),
        "error: 1 file is not committed. Commit it, or discard it if it's a scratch file"
    );
    // …and an untracked one.
    std::fs::write(worktree.join("notes.tmp"), "scratch\n").unwrap();
    let ran = agent
        .run_with_stdin(&format!("{submit} --json"), "Price table done.")
        .await;
    let e: Value = serde_json::from_str(&ran.stdout).unwrap();
    assert_eq!(
        e["error"]["message"],
        "2 files are not committed. Commit them, or discard them if they're scratch files"
    );
    assert_eq!(e["error"]["data"]["refusal"], "uncommitted");
    assert_eq!(w.status("SHOP-19").await, Status::Working, "stays in Dev");

    // Committed: Ready for Review, with the summary and the review head.
    std::fs::remove_file(worktree.join("notes.tmp")).unwrap();
    git(&worktree, &["commit", "-qam", "SHOP-19: price table"]);
    let ran = agent
        .run_with_stdin(&submit, "Price table with sorting.\nPrices align right.")
        .await;
    assert_eq!(
        (ran.status, ran.stdout.as_str()),
        (0, "Finished Dev. SHOP-19 is Ready for Review.\n"),
        "{ran:?}"
    );
    let row = w.board.store().ticket(&shop19.id).await.unwrap().unwrap();
    assert_eq!(row.state, "ready_for_review");
    let head = String::from_utf8(
        Process::new("git")
            .args(["rev-parse", "HEAD"])
            .current_dir(&worktree)
            .output()
            .unwrap()
            .stdout,
    )
    .unwrap();
    assert_eq!(row.review_head_sha.as_deref(), Some(head.trim()));
    assert!(row.review_entered_at.is_some());
    assert_eq!(
        w.board.review_summary(&shop19.id).await.unwrap().as_deref(),
        Some("Price table with sorting.\nPrices align right.")
    );
    // The summary is SHOP-19's agent's comment (CMT-05): SHOP-20's prime shows it under "Depends on" (CMT-07).
    let prime = otaq_core::prime::render(
        &w.board
            .prime_context("SHOP-20", w.adapters.for_new_session())
            .await
            .unwrap(),
    );
    assert!(
        prime.contains("  > Price table with sorting.\n  > Prices align right.\n"),
        "{prime}"
    );
    // The turn's Stop doesn't touch it.
    stop_hook(&agent).await;
    assert_eq!(w.status("SHOP-19").await, Status::WaitingForYou);
    w.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn the_submit_fact_keeps_stop_from_turning_idle() {
    let w = TestService::start().await;
    w.shop_sample().await;
    validation_off(&w).await;
    let shop19 = w.board.ticket("SHOP-19").await.unwrap();
    w.give_worktree(&shop19).await;
    let agent = w.agent_shell(&shop19).await;
    prime(&agent).await;
    let ran = agent
        .run_with_stdin(&format!("'{CB}' review submit --stdin"), "done")
        .await;
    assert_eq!(ran.status, 0, "{ran:?}");
    assert!(w.signals.turn(&agent.session).submitted_or_asked);
    w.stop().await;
}

/// T-0163: `ask human` with answer options (d0061): stored with the question, refused when malformed.
#[tokio::test(flavor = "multi_thread")]
async fn ask_human_with_answer_options() {
    let w = TestService::start().await;
    w.shop_sample().await;
    let shop21 = w.board.ticket("SHOP-21").await.unwrap();
    w.force_status(&shop21, Status::Working).await;
    let agent = w.agent_shell(&shop21).await;
    let ask = |flags: &str| format!("'{CB}' ask human {flags} --stdin --json");
    let question = "Which email provider should I use?";

    let ran = agent
        .run_with_stdin(
            &ask("--option Postmark --option SES --option Resend --recommended 1"),
            question,
        )
        .await;
    assert_eq!(ran.status, 0, "{ran:?}");
    let ticket: Value = serde_json::from_str(&ran.stdout).unwrap();
    assert_eq!(
        ticket["status"], "asks_you",
        "shows Asks you exactly as without options"
    );
    assert_eq!(ticket["signal"]["text"], question);
    let q = &ticket["question"];
    assert!(q["id"].as_str().is_some_and(|id| !id.is_empty()));
    assert_eq!(q["options"], json!(["Postmark", "SES", "Resend"]));
    assert_eq!(q["recommended"], 1);
    // Stored: the same question reads back.
    let stored = w.board.open_question(&shop21.id).await.unwrap().unwrap();
    assert_eq!(stored.id, q["id"].as_str().unwrap());
    assert_eq!(
        (stored.text.as_str(), stored.recommended),
        (question, Some(1))
    );

    // Free text still works, without options.
    let ran = agent.run_with_stdin(&ask(""), "Anything else?").await;
    let ticket: Value = serde_json::from_str(&ran.stdout).unwrap();
    assert!(ticket["question"].get("options").is_none());

    let long = "x".repeat(41);
    for (flags, message) in [
        (
            "--option Postmark",
            "Give 2–4 options, or none for a free-text question",
        ),
        (
            "--option a --option b --option c --option d --option e",
            "Give 2–4 options, or none for a free-text question",
        ),
        (
            "--option a --option b --recommended 3",
            "--recommended must be between 1 and 2",
        ),
        (
            "--recommended 1",
            "--recommended needs options to choose from",
        ),
        (
            &format!("--option a --option {long}"),
            "Option labels can be at most 40 characters",
        ),
    ] {
        let ran = agent.run_with_stdin(&ask(flags), question).await;
        assert_ne!(ran.status, 0, "{flags}");
        let e: Value = serde_json::from_str(&ran.stdout).unwrap();
        assert!(
            e["error"]["message"].as_str().unwrap().starts_with(message),
            "{flags}: {e}"
        );
    }
    let ran = agent
        .run_with_stdin(
            &format!("'{CB}' ask SHOP-18 --option a --option b --stdin --json"),
            question,
        )
        .await;
    let e: Value = serde_json::from_str(&ran.stdout).unwrap();
    assert_eq!(
        e["error"]["message"],
        "Options are only for questions to the human"
    );
    w.stop().await;
}

/// T-0087 (REV-05): `review feedback` prints the latest Send back's comments by file and the note, and only
/// reads.
#[tokio::test(flavor = "multi_thread")]
async fn review_feedback_prints_the_latest_round() {
    use otaq_core::board::comments::{NewComment, Side};
    use otaq_core::domain::Actor;

    let w = TestService::start().await;
    w.add_project("shop-web").await;
    let t = w.create("SHOP", "Search results paging").await;
    let agent = w.agent_shell(&t).await;

    let none = agent.run(&format!("'{CB}' review feedback")).await;
    assert_eq!(none.status, 0, "{none:?}");
    assert_eq!(
        none.stdout.trim(),
        "SHOP-1 has no review feedback: it was never sent back."
    );

    w.force_status(&t, Status::WaitingForYou).await;
    for (file, line, side, body) in [
        ("lib/search.ts", 40, Side::Old, "Why was this removed?"),
        (
            "lib/search.ts",
            12,
            Side::New,
            "Use the page size from settings.",
        ),
        ("app/page.tsx", 3, Side::New, "Show the total."),
    ] {
        w.board
            .add_comment(&NewComment {
                key: "SHOP-1".into(),
                file: file.into(),
                line,
                side,
                body: body.into(),
            })
            .await
            .unwrap();
    }
    w.review
        .fail(
            "SHOP-1",
            "Also check page 0.",
            None,
            false,
            &Actor::User("me".into()),
        )
        .await
        .unwrap();
    // The typed skill reaches this shell as a command line; wait until it has run before typing more.
    w.wait_for_output(&agent.session, "address-review").await;
    let status_before = w.status("SHOP-1").await;

    let human = agent.run(&format!("'{CB}' review feedback")).await;
    assert_eq!(human.status, 0, "{human:?}");
    assert_eq!(
        human.stdout.trim(),
        "SHOP-1 · feedback from round 1\n\napp/page.tsx\n  3: Show the total.\n\nlib/search.ts\n  12: Use the page size from settings.\n  40 (removed line): Why was this removed?"
    );
    let json = agent
        .run(&format!("'{CB}' review feedback SHOP-1 --json"))
        .await;
    assert_eq!(json.status, 0, "{json:?}");
    let feedback: Value = serde_json::from_str(&json.stdout).unwrap();
    assert_eq!(feedback["key"], "SHOP-1");
    assert_eq!(feedback["round"], 1);
    // The note is the user's ticket comment, read in `prime` (CMT-05).
    assert_eq!(feedback.get("note"), None);
    assert_eq!(feedback["comments"].as_array().unwrap().len(), 3);
    assert_eq!(
        feedback["comments"][2],
        json!({"file": "lib/search.ts", "line": 40, "side": "old", "body": "Why was this removed?"})
    );
    assert_eq!(
        w.status("SHOP-1").await,
        status_before,
        "reading moves nothing"
    );
    w.stop().await;
}

/// T-0183 (VAL-01): with validation on, `review submit` answers that validation runs and the ticket validates;
/// the validator is an ordinary `sh` here.
#[tokio::test(flavor = "multi_thread")]
async fn submit_with_validation_on_says_validation_runs() {
    use otaq_core::agent_tools::LaunchContext;
    use otaq_core::agent_tools::claude::ClaudeLaunch;
    use otaq_core::service::ServiceConfig;
    let w = TestService::start_with(|config| ServiceConfig {
        launch: Some(LaunchContext {
            bin_dir: "/nonexistent".into(),
            plugin_dir: "/nonexistent/plugin".into(),
            path: std::env::var_os("PATH").unwrap_or_default(),
            claude: ClaudeLaunch {
                program: vec![
                    "sh".into(),
                    "-c".into(),
                    "exec sleep 30".into(),
                    "sh".into(),
                ],
                config_dir: None,
            },
        }),
        ..config
    })
    .await;
    w.shop_sample().await;
    let shop19 = w.board.ticket("SHOP-19").await.unwrap();
    w.give_worktree(&shop19).await;
    let agent = w.agent_shell(&shop19).await;
    prime(&agent).await;
    let ran = agent
        .run_with_stdin(&format!("'{CB}' review submit --stdin"), "done")
        .await;
    assert_eq!(
        (ran.status, ran.stdout.trim()),
        (
            0,
            "Finished Dev. Validate is running; the result will come back to you here. End your turn."
        ),
        "{ran:?}"
    );
    assert_eq!(w.status("SHOP-19").await, Status::Checking);
    w.stop().await;
}

/// T-0406 (AGM-10): `otaq stage next` and `otaq review submit` are one request: both refuse uncommitted files and
/// move the ticket on, naming the stage it finished.
#[tokio::test(flavor = "multi_thread")]
async fn stage_next_and_review_submit_both_move_the_ticket_on() {
    let w = TestService::start().await;
    w.shop_sample().await;
    validation_off(&w).await;
    for (key, command) in [("SHOP-19", "stage next"), ("SHOP-27", "review submit")] {
        let ticket = w.board.ticket(key).await.unwrap();
        let worktree = w.give_worktree(&ticket).await;
        let agent = w.agent_shell(&ticket).await;
        prime(&agent).await;
        let run = format!("'{CB}' {command} --stdin");
        std::fs::write(worktree.join("file1.txt"), "changed\n").unwrap();
        let ran = agent.run_with_stdin(&run, "Done.").await;
        assert_eq!(
            ran.stderr.trim(),
            "error: 1 file is not committed. Commit it, or discard it if it's a scratch file",
            "{command}"
        );
        assert_eq!(w.status(key).await, Status::Working, "{command}");
        git(&worktree, &["commit", "-qam", "work"]);
        let ran = agent.run_with_stdin(&run, "Done.").await;
        assert_eq!(
            (ran.status, ran.stdout.trim().to_owned()),
            (0, format!("Finished Dev. {key} is Ready for Review.")),
            "{command}: {ran:?}"
        );
        assert_eq!(w.status(key).await, Status::WaitingForYou, "{command}");
        assert!(w.signals.turn(&agent.session).submitted_or_asked);
    }
    w.stop().await;
}

/// T-0498 (d0125): `otaq stage next --manual-check …` lists what the agent couldn't check for the user, one line
/// each; `prime` shows them; a line break is refused and nothing moves.
#[tokio::test(flavor = "multi_thread")]
async fn stage_next_lists_manual_checks() {
    let w = TestService::start().await;
    w.shop_sample().await;
    validation_off(&w).await;
    let ticket = w.board.ticket("SHOP-19").await.unwrap();
    w.give_worktree(&ticket).await;
    let agent = w.agent_shell(&ticket).await;
    prime(&agent).await;

    let refused = agent
        .run_with_stdin(
            &format!("'{CB}' stage next --stdin --manual-check \"$(printf 'Reload offline\\nOn a phone')\""),
            "Done.",
        )
        .await;
    assert_eq!(
        (refused.status, refused.stderr.trim()),
        (
            1,
            "error: A manual check is one line: give each check its own `--manual-check`"
        ),
        "{refused:?}"
    );
    assert_eq!(w.status("SHOP-19").await, Status::Working);

    let ran = agent
        .run_with_stdin(
            &format!(
                "'{CB}' stage next --stdin --manual-check 'Production build: reload a visited page offline' \
                 --manual-check 'Open a never-visited page offline'"
            ),
            "Pages cache for offline use.",
        )
        .await;
    assert_eq!(ran.status, 0, "{ran:?}");
    assert_eq!(w.status("SHOP-19").await, Status::WaitingForYou);
    let checks = w.board.manual_checks("SHOP-19").await.unwrap();
    assert_eq!(
        checks
            .items
            .iter()
            .map(|i| i.text.as_str())
            .collect::<Vec<_>>(),
        [
            "Production build: reload a visited page offline",
            "Open a never-visited page offline"
        ]
    );
    assert_eq!(checks.unticked, 2);
    let primed = agent.run(&format!("'{CB}' prime")).await;
    assert!(
        primed
            .stdout
            .contains("- [ ] Open a never-visited page offline \u{b7} "),
        "{primed:?}"
    );
    w.stop().await;
}

/// T-0406 (AGM-10, d0033): `otaq stage done` with no commits beyond the base closes the ticket as Closed · nothing
/// to merge with the reason; the session ends after its turn, and Reopen brings the ticket back. With commits it is
/// refused.
#[tokio::test(flavor = "multi_thread")]
async fn stage_done_closes_a_ticket_with_nothing_to_merge() {
    let w = TestService::start().await;
    w.shop_sample().await;
    let shop27 = w.board.ticket("SHOP-27").await.unwrap();
    w.give_worktree_at_base(&shop27).await;
    let agent = w.agent_shell(&shop27).await;
    prime(&agent).await;
    let ran = agent
        .run_with_stdin(
            &format!("'{CB}' stage done --stdin --json"),
            "Already fixed in SHOP-12.\nNothing left to change.",
        )
        .await;
    assert_eq!(ran.status, 0, "{ran:?}");
    let ticket: Value = serde_json::from_str(&ran.stdout).unwrap();
    assert_eq!(ticket["status"], "closed_not_merged");
    let ran = agent
        .run_with_stdin(&format!("'{CB}' stage done --stdin"), "again")
        .await;
    assert_ne!(
        ran.status, 0,
        "a closed ticket is no longer in an Agent stage"
    );
    let history = w.board.history("SHOP-27").await.unwrap();
    let closed = history
        .iter()
        .find(|e| e.kind == otaq_core::board::tickets::EventKind::Closed)
        .unwrap();
    assert_eq!(closed.payload["nothing_to_merge"], true);
    assert_eq!(
        closed.payload["reason"],
        "Already fixed in SHOP-12.\nNothing left to change."
    );
    // The card says so too (Q276): the reason is on the ticket while it is Closed.
    let closed_ticket = w.board.ticket("SHOP-27").await.unwrap();
    assert_eq!(
        closed_ticket.flags.nothing_to_merge.as_deref(),
        Some("Already fixed in SHOP-12.\nNothing left to change.")
    );
    let runs = w.board.stage_runs("SHOP-27").await.unwrap();
    let dev = runs.iter().rev().find(|r| r.stage == "dev").unwrap();
    assert_eq!(
        (dev.result, dev.summary.as_deref()),
        (
            Some(otaq_core::flow::engine::RunResult::Done),
            Some("Already fixed in SHOP-12.\nNothing left to change.")
        )
    );

    // Its turn ends: the Done cleanup ends the session. Typed, not awaited: the session ends under the command.
    assert!(
        w.sessions
            .running_of(&shop27.id, otaq_core::session::SessionKind::Agent)
            .is_some()
    );
    let payload = json!({"hook_event_name": "Stop", "background_tasks": [], "session_crons": []});
    let line = format!("printf '%s' '{payload}' | '{CB}' hook --tool claude Stop\r");
    w.sessions.input(&agent.session, line.as_bytes()).unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(10), async {
        while w
            .sessions
            .running_of(&shop27.id, otaq_core::session::SessionKind::Agent)
            .is_some()
        {
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("the agent ends after its turn");
    assert_eq!(w.status("SHOP-27").await, Status::ClosedNotMerged);

    // Reopen (dragging it out of Done) works as for any closed ticket (d0033).
    let reopened = w
        .board
        .reopen("SHOP-27", &otaq_core::domain::Actor::User("me".into()))
        .await
        .unwrap();
    assert_eq!(reopened.status, Status::Backlog);
    assert_eq!(reopened.flags.nothing_to_merge, None);

    // With commits: refused, named with the count.
    let shop19 = w.board.ticket("SHOP-19").await.unwrap();
    let worktree = w.give_worktree_at_base(&shop19).await;
    for n in 1..=2 {
        std::fs::write(worktree.join(format!("f{n}.txt")), "x\n").unwrap();
        git(&worktree, &["add", "."]);
        git(&worktree, &["commit", "-qm", &format!("SHOP-19: step {n}")]);
    }
    let agent = w.agent_shell(&shop19).await;
    prime(&agent).await;
    let ran = agent
        .run_with_stdin(&format!("'{CB}' stage done --stdin"), "nothing to do")
        .await;
    assert_ne!(ran.status, 0);
    assert_eq!(
        ran.stderr.trim(),
        "error: SHOP-19 has 2 commits. Use `otaq stage next` so they're checked and delivered."
    );
    assert_eq!(w.status("SHOP-19").await, Status::Working);
    w.stop().await;
}

/// T-0406 (AGM-10): `otaq validate pass|fail` is gone, with no other name.
#[tokio::test(flavor = "multi_thread")]
async fn validate_is_an_unknown_command() {
    let out = Process::new(CB)
        .args(["validate", "pass", "--stdin"])
        .output()
        .unwrap();
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("unrecognized subcommand 'validate'"),
        "{stderr}"
    );
}
