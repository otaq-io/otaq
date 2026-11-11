//! `otaq prime` from inside an agent session (T-0071): the current ticket's Markdown, `--json`. It is a read: it
//! clears no needs-you label and moves no last signal (d0119, T-0489).

use otaq_core::board::settings::WorkflowStep;
use otaq_core::domain::Status;
use otaq_core::service::testing::TestService;
use otaq_core::skills::testing::skill_folder;
use serde_json::Value;

const CB: &str = env!("CARGO_BIN_EXE_otaq");

#[tokio::test(flavor = "multi_thread")]
async fn prime_prints_the_context_and_changes_no_status() {
    let w = TestService::start().await;
    w.shop_sample().await;
    assert_eq!(w.status("SHOP-12").await, Status::AsksYou);
    let shop12 = w.board.ticket("SHOP-12").await.unwrap();
    let agent = w.agent_shell(&shop12).await;

    let ran = agent.run(&format!("'{CB}' prime")).await;
    assert_eq!(ran.status, 0, "{ran:?}");
    assert!(
        ran.stdout.starts_with("# SHOP-12 · Add dark mode toggle\n"),
        "{}",
        ran.stdout
    );
    assert!(ran.stdout.contains("stage next --stdin"));
    assert_eq!(
        w.status("SHOP-12").await,
        Status::AsksYou,
        "prime is a read"
    );

    let ran = agent.run(&format!("'{CB}' prime --json")).await;
    let context: Value = serde_json::from_str(&ran.stdout).unwrap();
    assert_eq!(context["ticket"]["key"], "SHOP-12");
    assert_eq!(context["commit_skill"], "/otaq:commit");
    w.stop().await;
}

/// A skill folder named `commit` of the user's doesn't replace Otaq's: agents have no skills, so prime names Otaq's
/// own Commit skill in the caller's tool's syntax.
#[tokio::test(flavor = "multi_thread")]
async fn prime_names_otaqs_commit_skill_not_a_users_folder() {
    let w = TestService::start().await;
    w.shop_sample().await;
    let folder = skill_folder(w.data.root(), "commit");
    w.use_step_skill(WorkflowStep::Commit, &folder).await;
    let shop12 = w.board.ticket("SHOP-12").await.unwrap();
    w.give_worktree(&shop12).await;
    let agent = w.agent_shell(&shop12).await;
    let ran = agent.run(&format!("'{CB}' prime")).await;
    assert_eq!(ran.status, 0, "{ran:?}");
    let rule = ran
        .stdout
        .lines()
        .find(|l| l.contains("before you end your stage"))
        .expect("the commit rule");
    assert_eq!(
        rule,
        "2. Commit everything with `/otaq:commit` before you end your stage. Don't push unless the user asks; when \
         you do, wait for the push to finish, however long a pre-push hook takes, and never use `--no-verify`. Never \
         open a PR: the user does that."
    );
    let ran = agent.run(&format!("'{CB}' prime --json")).await;
    let context: Value = serde_json::from_str(&ran.stdout).unwrap();
    assert_eq!(context["commit_skill"], "/otaq:commit");
    w.stop().await;
}

/// No-skill-copies F11: plain `otaq prime` in the ticket's validator's session prints the validator's prime (the same
/// as `prime --validate`, its stage named) and clears the refused verdict; the working session's prime is unchanged.
#[tokio::test(flavor = "multi_thread")]
async fn plain_prime_in_a_validators_session_is_the_validate_prime() {
    let w = TestService::start().await;
    w.add_project("shop-web").await;
    let t = w.create("SHOP", "Export orders as CSV").await;
    w.give_worktree(&t).await;
    w.board
        .post_ticket_comment(
            &t.key.to_string(),
            &otaq_core::board::ticket_comments::CommentAuthor::User { name: "me".into() },
            "Done. Excel not added.",
            Vec::new(),
            &otaq_core::domain::Actor::User("me".into()),
        )
        .await
        .unwrap();
    let dev = w.agent_shell(&t).await;
    let working = dev.run(&format!("'{CB}' prime")).await;
    assert_eq!(working.status, 0, "{working:?}");
    assert!(
        working
            .stdout
            .starts_with("# SHOP-1 · Export orders as CSV\n"),
        "{}",
        working.stdout
    );
    assert!(
        working.stdout.contains("## Your stage\n\nDev, as "),
        "{}",
        working.stdout
    );

    let checker = w
        .shell(&t, otaq_core::session::SessionKind::Validator)
        .await;
    w.validating_with(&t, &checker.session).await;
    let verdict = checker
        .run_with_stdin(&format!("'{CB}' check pass --stdin"), "All good.\n")
        .await;
    assert_ne!(verdict.status, 0, "{verdict:?}");
    assert!(
        format!("{}{}", verdict.stdout, verdict.stderr)
            .contains("Run `otaq prime` first: SHOP-1 has 1 comment you haven't read."),
        "{verdict:?}"
    );

    let plain = checker.run(&format!("'{CB}' prime")).await;
    assert_eq!(plain.status, 0, "{plain:?}");
    assert!(
        plain
            .stdout
            .starts_with("# Validate SHOP-1 · Export orders as CSV\n"),
        "{}",
        plain.stdout
    );
    assert!(
        plain
            .stdout
            .contains("## Your stage\n\nValidate, as Checker, with the skill `/otaq:validate`."),
        "{}",
        plain.stdout
    );
    let flagged = checker.run(&format!("'{CB}' prime --validate")).await;
    assert_eq!(flagged.stdout, plain.stdout);
    let json = checker.run(&format!("'{CB}' prime --json")).await;
    let context: Value = serde_json::from_str(&json.stdout).unwrap();
    assert_eq!(context["ticket"]["key"], "SHOP-1");
    assert!(context.get("round").is_some(), "{context}");

    let verdict = checker
        .run_with_stdin(&format!("'{CB}' check pass --stdin"), "All good.\n")
        .await;
    assert_eq!(verdict.status, 0, "{verdict:?}");
    w.stop().await;
}

/// A reader that goes away before the answer is printed (`otaq prime | head`, or a closed pipe on Windows) ends the
/// command quietly: no panic on stderr, exit 0.
#[tokio::test(flavor = "multi_thread")]
async fn prime_into_a_closed_pipe_ends_quietly() {
    let w = TestService::start().await;
    w.shop_sample().await;
    let shop12 = w.board.ticket("SHOP-12").await.unwrap();
    let agent = w.agent_shell(&shop12).await;

    // `exec 0<&-` closes the reader's end before prime answers, so its write meets a broken pipe.
    let ran = agent
        .run(&format!(
            "( '{CB}' prime; echo \"exit=$?\" >&2 ) | ( exec 0<&- ; sleep 1 )"
        ))
        .await;
    assert!(!ran.stderr.contains("panicked"), "{ran:?}");
    assert!(ran.stderr.contains("exit=0"), "{ran:?}");
    w.stop().await;
}
