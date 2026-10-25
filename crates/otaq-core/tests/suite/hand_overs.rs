//! Hand-overs become comments (T-0443; CMT-05, CMT-06; d0109, d0108, d0034, d0063): `stage next` / `stage done`
//! post the agent's comment with `--report-file`, a verdict the checker's, and the user's Move to next stage and Send
//! back their note, each in the move's transaction and before any agent is told. A text over the limit or a missing
//! report refuses the whole hand-over: nothing moves. What the agent is typed names the hand-over, never its text.
//! Sessions are an ordinary `sh` (requests through the socket from inside them); no agent tool runs.

use otaq_core::board::Refusal;
use otaq_core::board::comments::{NewComment, Side};
use otaq_core::board::hand_over::{SentBack, check_failed_text, sent_back_text};
use otaq_core::board::reports::ReportSource;
use otaq_core::board::ticket_comments::{CommentAuthor, TicketComment};
use otaq_core::board::tickets::{EventKind, Ticket};
use otaq_core::domain::{Actor, Place, Status};
use otaq_core::review::ReturnOutcome;
use otaq_core::service::testing::{AgentShell, TestService};
use otaq_core::session::SessionKind;
use serde_json::{Value, json};

fn me() -> Actor {
    Actor::User("nevzat".into())
}

async fn world() -> TestService {
    let w = TestService::start().await;
    w.add_project("shop-web").await;
    w
}

/// SHOP on the default flow without Validate: `stage next` from Dev goes to Review.
async fn without_validate(w: &TestService) {
    w.board
        .edit_flow_for_tests("SHOP", otaq_core::flow::default::testing::without_validate)
        .await;
}

/// A ticket working in Dev with a committed worktree, and its agent's session running "Rust developer".
async fn in_dev(w: &TestService, title: &str) -> (Ticket, AgentShell) {
    let t = w.create("SHOP", title).await;
    w.give_worktree(&t).await;
    w.force_status(&t, Status::Working).await;
    let dev = w.agent_shell(&t).await;
    w.run_as_agent(&dev, "rust-dev", "Rust developer").await;
    (w.board.ticket(&t.key.to_string()).await.unwrap(), dev)
}

/// One raw request from inside a session (no `'` in it: it goes through `sh`).
async fn send(w: &TestService, shell: &AgentShell, method: &str, params: Value) -> Value {
    let request =
        json!({"protocol_version": 1, "id": 1, "request": {"method": method, "params": params}});
    let ran = shell
        .send_raw(&w.data.socket_path(), &request.to_string())
        .await;
    serde_json::from_str(ran.stdout.lines().next().unwrap_or_default())
        .unwrap_or_else(|e| panic!("{e}: {ran:?}"))
}

/// `prime` first, as every stage end needs (CMT-08), then `method`.
async fn end_stage(w: &TestService, shell: &AgentShell, method: &str, params: Value) -> Value {
    let key = params["key"].clone();
    let prime = match method {
        "validation.verdict" => "ticket.prime_validate",
        _ => "ticket.prime",
    };
    send(w, shell, prime, json!({ "key": key })).await;
    send(w, shell, method, params).await
}

fn refusal(answer: &Value) -> (&str, &str) {
    (
        answer["error"]["data"]["refusal"]
            .as_str()
            .unwrap_or_else(|| panic!("not refused: {answer}")),
        answer["error"]["message"].as_str().unwrap_or_default(),
    )
}

async fn comments(w: &TestService, key: &str) -> Vec<TicketComment> {
    w.board.ticket_comments(key).await.unwrap().comments
}

fn rust_developer(stage: &str) -> CommentAuthor {
    CommentAuthor::Agent {
        id: Some("rust-dev".into()),
        name: Some("Rust developer".into()),
        tool: Some(otaq_core::agent_tools::Tool::Claude),
        stage: Some(stage.into()),
        round: None,
    }
}

fn you() -> CommentAuthor {
    CommentAuthor::User {
        name: me().to_string(),
    }
}

/// A file outside the worktree holding `text` (a test run saved straight from the tool).
fn saved_output(dir: &tempfile::TempDir, text: &str) -> std::path::PathBuf {
    let path = dir.path().join("test-run.txt");
    std::fs::write(&path, text).unwrap();
    path
}

/// The open stage run's `handed_over_at` (CMT-08 reads it).
async fn handed_over(w: &TestService, t: &Ticket) -> Option<i64> {
    sqlx::query_scalar(
        "SELECT handed_over_at FROM stage_runs WHERE ticket_id = ? AND ended_at IS NULL ORDER BY started_at DESC",
    )
    .bind(&t.id)
    .fetch_optional(w.board.store().pool())
    .await
    .unwrap()
    .flatten()
}

/// `stage next`: the summary is the agent's comment, with its stage, posted with the move; its run ends with it. Too
/// long, or a missing report: refused, nothing moves and nothing is posted.
#[tokio::test(flavor = "multi_thread")]
async fn stage_next_posts_the_summary_as_the_agents_comment() {
    let w = world().await;
    without_validate(&w).await;
    let (t, dev) = in_dev(&w, "Export orders as CSV").await;

    let long = end_stage(
        &w,
        &dev,
        "ticket.stage_next",
        json!({"key": "SHOP-1", "summary": "x".repeat(2_001)}),
    )
    .await;
    assert_eq!(
        refusal(&long),
        (
            "too_long",
            "Comment is 2,001 characters; the limit is 2,000. Put the full text in a report with `--report-file` \
             and keep the comment to what the next agent needs."
        )
    );
    let missing = end_stage(
        &w,
        &dev,
        "ticket.stage_next",
        json!({"key": "SHOP-1", "summary": "Done.", "report_file": {"path": "/nonexistent/run.txt"}}),
    )
    .await;
    assert_eq!(refusal(&missing).0, "bad_report");
    assert_eq!(w.status("SHOP-1").await, Status::Working, "nothing moved");
    assert!(comments(&w, "SHOP-1").await.is_empty(), "nothing posted");

    let dir = tempfile::TempDir::new().unwrap();
    let run = saved_output(&dir, "test a ... ok\ntest b ... ok\n");
    let summary = "Reused the CSV writer in export.rs from SHOP-12.";
    let ended = end_stage(
        &w,
        &dev,
        "ticket.stage_next",
        json!({"key": "SHOP-1", "summary": summary, "report_file": {"path": run}}),
    )
    .await;
    assert!(ended["reply"]["stage_ended"].is_object(), "{ended}");
    assert_eq!(w.status("SHOP-1").await, Status::WaitingForYou);
    let posted = comments(&w, "SHOP-1").await;
    assert_eq!(posted.len(), 1);
    assert_eq!(
        (&posted[0].author, posted[0].body.as_str()),
        (&rust_developer("dev"), summary)
    );
    assert_eq!(posted[0].report.map(|r| (r.bytes, r.lines)), Some((28, 2)));
    assert!(run.exists(), "the original stays");
    let runs = w.board.stage_runs("SHOP-1").await.unwrap();
    assert_eq!(
        runs.iter()
            .rfind(|r| r.stage == "dev")
            .unwrap()
            .summary
            .as_deref(),
        Some(summary),
        "the Dev run ended with the comment"
    );
    let _ = t;
    w.stop().await;
}

/// `stage done`: the reason is the agent's comment; the ticket is Closed · nothing to merge.
#[tokio::test(flavor = "multi_thread")]
async fn stage_done_posts_the_reason() {
    let w = world().await;
    let t = w.create("SHOP", "Remove the old export").await;
    w.give_worktree_at_base(&t).await;
    w.force_status(&t, Status::Working).await;
    let dev = w.agent_shell(&t).await;
    w.run_as_agent(&dev, "rust-dev", "Rust developer").await;

    let reason = "The old export was already removed in SHOP-12.";
    let ended = end_stage(
        &w,
        &dev,
        "ticket.stage_done",
        json!({"key": "SHOP-1", "summary": reason}),
    )
    .await;
    assert!(ended["reply"]["stage_ended"].is_object(), "{ended}");
    assert_eq!(w.status("SHOP-1").await, Status::ClosedNotMerged);
    let posted = comments(&w, "SHOP-1").await;
    assert_eq!(
        posted
            .iter()
            .map(|c| (&c.author, c.body.as_str()))
            .collect::<Vec<_>>(),
        [(&rust_developer("dev"), reason)]
    );
    w.stop().await;
}

/// The user's Move to next stage: the note is the user's comment, without "Submitted by you"; no note, no comment.
#[tokio::test(flavor = "multi_thread")]
async fn move_to_next_stage_posts_the_users_note() {
    let w = world().await;
    without_validate(&w).await;
    let (_, _) = in_dev(&w, "Export orders as CSV").await;
    let (_, _) = in_dev(&w, "Order list paging").await;
    for key in ["SHOP-1", "SHOP-2"] {
        w.force_status(&w.board.ticket(key).await.unwrap(), Status::Idle)
            .await;
    }

    let note = "Tests pass locally; the paging needs your eye.";
    w.validation
        .submit_by_user("SHOP-1", note, &me())
        .await
        .unwrap();
    w.validation
        .submit_by_user("SHOP-2", "  ", &me())
        .await
        .unwrap();
    assert_eq!(w.status("SHOP-1").await, Status::WaitingForYou);
    let posted = comments(&w, "SHOP-1").await;
    assert_eq!(
        posted
            .iter()
            .map(|c| (&c.author, c.body.as_str()))
            .collect::<Vec<_>>(),
        [(&you(), note)]
    );
    assert!(
        comments(&w, "SHOP-2").await.is_empty(),
        "an empty note posts nothing"
    );
    let submitted = w.board.history("SHOP-1").await.unwrap();
    let event = submitted
        .iter()
        .find(|e| e.kind == EventKind::ReviewSubmitted)
        .unwrap();
    assert_eq!(event.payload["summary"], "Submitted by you");
    w.stop().await;
}

/// A check's verdict is the checker's comment (its stage and round) with `--report-file`; "Note:" lines still become
/// the round's notes. A fail tells Dev "Validate failed round 1. Run `otaq prime`." without the findings, and Dev's
/// run is marked handed over. Too long: refused, the round stays open.
#[tokio::test(flavor = "multi_thread")]
async fn a_verdict_is_the_checkers_comment() {
    let w = world().await;
    let (t, dev) = in_dev(&w, "Export orders as CSV").await;
    let checker = w.shell(&t, SessionKind::Validator).await;
    w.run_as_agent(&checker, "validator", "Validator").await;
    w.validating_with(&t, &checker.session).await;

    let long = end_stage(
        &w,
        &checker,
        "validation.verdict",
        json!({"key": "SHOP-1", "pass": false, "report": "y".repeat(2_001)}),
    )
    .await;
    assert_eq!(refusal(&long).0, "too_long");
    assert_eq!(
        w.status("SHOP-1").await,
        Status::Checking,
        "the round stays open"
    );

    let dir = tempfile::TempDir::new().unwrap();
    let run = saved_output(&dir, "FAILED export_tests::row_limit\n");
    let findings = "Fail: the 10,000 row limit is not enforced; 3 tests fail in export_tests.rs.\n\
                    Note: Not covered by any ticket: streaming the export";
    let failed = end_stage(
        &w,
        &checker,
        "validation.verdict",
        json!({"key": "SHOP-1", "pass": false, "report": findings, "report_file": {"path": run}}),
    )
    .await;
    assert!(failed["reply"]["ticket"].is_object(), "{failed}");
    let after = w.board.ticket("SHOP-1").await.unwrap();
    assert_eq!(
        (after.place.clone(), after.status),
        (Place::stage_of("dev"), Status::Working)
    );
    let posted = comments(&w, "SHOP-1").await;
    assert_eq!(posted.len(), 1);
    assert_eq!(
        posted[0].author,
        CommentAuthor::Agent {
            id: Some("validator".into()),
            name: Some("Validator".into()),
            tool: Some(otaq_core::agent_tools::Tool::Claude),
            stage: Some("validate".into()),
            round: Some(1),
        }
    );
    assert_eq!(posted[0].body, findings);
    assert!(posted[0].report.is_some());
    let rounds = w.board.validations("SHOP-1").await.unwrap();
    assert_eq!(rounds[0].report.as_deref(), Some(findings));
    assert_eq!(
        rounds[0].notes,
        ["Not covered by any ticket: streaming the export"]
    );
    // What Dev is told names the round, never the findings.
    let told: Vec<String> = w
        .deliveries
        .pending(&dev.session)
        .await
        .unwrap()
        .into_iter()
        .map(|p| p.text)
        .collect();
    assert_eq!(told, ["Validate failed round 1. Run `otaq prime`."]);
    assert!(told.iter().all(|t| !t.contains("10,000")));
    assert!(handed_over(&w, &after).await.is_some(), "Dev primes again");
    w.stop().await;
}

async fn in_review_with_line_comments(w: &TestService, title: &str, n: u32) -> Ticket {
    let t = w.create("SHOP", title).await;
    w.give_worktree(&t).await;
    w.force_status(&t, Status::WaitingForYou).await;
    for line in 1..=n {
        w.board
            .add_comment(&NewComment {
                key: t.key.to_string(),
                file: "lib/export.rs".into(),
                line,
                side: Side::New,
                body: format!("line comment {line}"),
            })
            .await
            .unwrap();
    }
    w.board.ticket(&t.key.to_string()).await.unwrap()
}

/// Send back (a human check's Fail): the note is the user's comment, with a pasted report, posted before the live
/// session is typed `/otaq-address-review` and what came back, never the note. An empty note posts nothing; a note
/// over the limit refuses the whole Send back.
#[tokio::test(flavor = "multi_thread")]
async fn send_back_posts_the_note_before_the_agent_is_told() {
    let w = world().await;
    let t = in_review_with_line_comments(&w, "Export orders as CSV", 2).await;
    let dev = w.agent_shell(&t).await;

    let refused = w
        .review
        .fail("SHOP-1", &"z".repeat(2_001), None, false, &me())
        .await
        .unwrap_err();
    assert_eq!(refused.refusal(), Some(Refusal::TooLong));
    assert_eq!(
        w.status("SHOP-1").await,
        Status::WaitingForYou,
        "nothing moved"
    );
    assert_eq!(w.board.comments("SHOP-1", None).await.unwrap().pending, 2);

    let note = "Keep the column order the same as the table on screen.";
    let log = "thread main panicked at export.rs:40\n".repeat(100);
    let outcome = w
        .review
        .fail(
            "SHOP-1",
            note,
            Some(ReportSource::Text(log.clone())),
            false,
            &me(),
        )
        .await
        .unwrap();
    assert!(matches!(outcome, ReturnOutcome::Done { .. }));
    let posted = comments(&w, "SHOP-1").await;
    assert_eq!((&posted[0].author, posted[0].body.as_str()), (&you(), note));
    assert_eq!(posted[0].report.map(|r| r.lines), Some(100));
    let out = w
        .wait_for_output(&dev.session, "otaq review feedback")
        .await;
    assert!(
        out.contains(
            "SHOP-1 was sent back to Dev with 2 line comments and a comment. Run `otaq prime`, then `otaq review \
             feedback`."
        ),
        "{out}"
    );
    assert!(!out.contains(note), "the note's text isn't typed: {out}");
    let after = w.board.ticket("SHOP-1").await.unwrap();
    assert!(handed_over(&w, &after).await.is_some());
    let runs = w.board.stage_runs("SHOP-1").await.unwrap();
    assert_eq!(
        runs.iter()
            .rfind(|r| r.stage == "review")
            .unwrap()
            .summary
            .as_deref(),
        Some(note),
        "the Review run ended with the comment"
    );

    // An empty note: nothing posted, and the agent hears only of the line comments.
    let t2 = in_review_with_line_comments(&w, "Order list paging", 1).await;
    let dev2 = w.agent_shell(&t2).await;
    w.review
        .fail("SHOP-2", " ", None, false, &me())
        .await
        .unwrap();
    assert!(comments(&w, "SHOP-2").await.is_empty());
    w.wait_for_output(
        &dev2.session,
        "SHOP-2 was sent back to Dev with 1 line comment. Run `otaq prime`, then `otaq review feedback`.",
    )
    .await;
    // `otaq review feedback` reads the line comments only.
    let feedback = w.board.review_feedback("SHOP-1").await.unwrap();
    assert_eq!(feedback.comments.len(), 2);
    w.stop().await;
}

/// A Send back while the agent limit is full: the note is posted now, and the queued ticket keeps what its agent is
/// told after the skill (`resume_prompt`), never the note.
#[tokio::test(flavor = "multi_thread")]
async fn a_queued_send_back_keeps_what_the_agent_is_told() {
    use otaq_core::board::settings::{AgentLimits, GlobalSettings};
    let w = world().await;
    w.board
        .set_global_settings(&GlobalSettings {
            agent_limits: AgentLimits {
                enabled: true,
                max_concurrency: 1,
                back_pressure: 3,
            },
            ..Default::default()
        })
        .await
        .unwrap();
    let busy = w.create("SHOP", "Keeps the slot").await;
    w.force_status(&busy, Status::Working).await;
    let t = in_review_with_line_comments(&w, "Export orders as CSV", 0).await;

    let note = "Use the CSV writer from SHOP-12.";
    w.review
        .fail(&t.key.to_string(), note, None, false, &me())
        .await
        .unwrap();
    let row = w.board.store().ticket(&t.id).await.unwrap().unwrap();
    assert!(w.status(&t.key.to_string()).await.is_queued());
    // No line comments: no skill, the plain sentence alone (d0131 Q514).
    assert_eq!(
        (row.resume_skill.as_deref(), row.resume_prompt.as_deref()),
        (
            None,
            Some("Review sent SHOP-2 back to you with a comment. Run `otaq prime`.")
        )
    );
    assert_eq!(
        comments(&w, &t.key.to_string()).await[0].body,
        note,
        "posted before the agent comes back"
    );
    w.stop().await;
}

fn back() -> SentBack<'static> {
    SentBack {
        key: "SHOP-18",
        stage: "Dev",
        line_comments: 0,
        pr_comments: false,
        comment: false,
        by_you: false,
        from: None,
    }
}

/// T-0443 (CMT-05): every variant of what a returning agent is told.
#[test]
fn the_sent_back_texts() {
    assert_eq!(
        sent_back_text(&SentBack {
            line_comments: 2,
            comment: true,
            ..back()
        }),
        "SHOP-18 was sent back to Dev with 2 line comments and a comment. Run `otaq prime`, then `otaq review \
         feedback`."
    );
    assert_eq!(
        sent_back_text(&SentBack {
            line_comments: 1,
            ..back()
        }),
        "SHOP-18 was sent back to Dev with 1 line comment. Run `otaq prime`, then `otaq review feedback`."
    );
    assert_eq!(
        sent_back_text(&SentBack {
            comment: true,
            ..back()
        }),
        "SHOP-18 was sent back to Dev with a comment. Run `otaq prime`."
    );
    assert_eq!(
        sent_back_text(&back()),
        "SHOP-18 was sent back to Dev. Run `otaq prime`."
    );
    assert_eq!(
        sent_back_text(&SentBack {
            pr_comments: true,
            comment: true,
            ..back()
        }),
        "SHOP-18 was sent back to Dev with PR comments and a comment. Run `otaq prime`, then `otaq review \
         feedback`."
    );
    assert_eq!(
        sent_back_text(&SentBack {
            line_comments: 3,
            pr_comments: true,
            comment: true,
            ..back()
        }),
        "SHOP-18 was sent back to Dev with 3 line comments, PR comments and a comment. Run `otaq prime`, then \
         `otaq review feedback`."
    );
    assert_eq!(
        sent_back_text(&SentBack {
            key: "SHOP-19",
            comment: true,
            by_you: true,
            ..back()
        }),
        "SHOP-19 was sent back to Dev by you with a comment. Run `otaq prime`."
    );
    // A human check's Send back with no code review: the plain sentence, whatever the stage (d0131 Q514).
    assert_eq!(
        sent_back_text(&SentBack {
            key: "SHOP-40",
            stage: "Product spec",
            comment: true,
            from: Some("Spec review"),
            ..back()
        }),
        "Spec review sent SHOP-40 back to you with a comment. Run `otaq prime`."
    );
    assert_eq!(
        sent_back_text(&SentBack {
            from: Some("Review"),
            ..back()
        }),
        "Review sent SHOP-18 back to you. Run `otaq prime`."
    );
    assert_eq!(
        sent_back_text(&SentBack {
            line_comments: 2,
            from: Some("Review"),
            ..back()
        }),
        "SHOP-18 was sent back to Dev with 2 line comments. Run `otaq prime`, then `otaq review feedback`.",
        "a code review keeps its form, after address-review"
    );
    assert_eq!(
        check_failed_text("Validate", 2),
        "Validate failed round 2. Run `otaq prime`."
    );
}

// ---- The read gate re-armed by every real resume (CMT-08 with T-0444's gate) ----

/// The test flow `tests/flows/every-kind.yaml`: Grill (skippable) → Dev → Validate → Security → QA (yours) → Done.
fn every_kind() -> otaq_core::flow::FlowFile {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/flows/every-kind.yaml");
    otaq_core::flow::yaml::read(&std::fs::read_to_string(path).unwrap()).unwrap()
}

/// A service whose `claude` keeps running, printing its arguments (a fresh start or a checker needs one).
async fn launching_world() -> TestService {
    use otaq_core::agent_tools::LaunchContext;
    use otaq_core::agent_tools::claude::ClaudeLaunch;
    let w = TestService::start_with(|config| otaq_core::service::ServiceConfig {
        launch: Some(LaunchContext {
            bin_dir: "/nonexistent".into(),
            plugin_dir: "/nonexistent/plugin".into(),
            path: std::env::var_os("PATH").unwrap_or_default(),
            claude: ClaudeLaunch {
                program: vec![
                    "sh".into(),
                    "-c".into(),
                    "printf 'ARGS %s\\n' \"$*\"; exec sleep 30".into(),
                    "sh".into(),
                ],
                config_dir: None,
            },
        }),
        ..config
    })
    .await;
    w.add_project("shop-web").await;
    w
}

/// Puts the ticket in `stage` with `status` in one write.
async fn place_in(w: &TestService, t: &Ticket, stage: &str, status: Status) {
    sqlx::query("UPDATE tickets SET stage_id = ?, state = ? WHERE id = ?")
        .bind(stage)
        .bind(status.as_str())
        .bind(&t.id)
        .execute(w.board.store().pool())
        .await
        .unwrap();
}

/// Lets the clock move on, so a comment is posted after the prime before it.
async fn tick() {
    tokio::time::sleep(std::time::Duration::from_millis(5)).await;
}

const ONE_UNREAD: &str = "Run `otaq prime` first: SHOP-1 has 1 comment you haven't read.";

/// The agent's live session after a hand-over: `stage next` refused with the one new comment, then past the gate once
/// it ran `prime` (the stage end's own checks decide the rest).
async fn gate_rearmed(w: &TestService, dev: &AgentShell) {
    let next = json!({"key": "SHOP-1", "summary": "Done."});
    let refused = send(w, dev, "ticket.stage_next", next.clone()).await;
    assert_eq!(refusal(&refused), ("move_refused", ONE_UNREAD));
    tick().await;
    send(w, dev, "ticket.prime", json!({"key": "SHOP-1"})).await;
    let after = send(w, dev, "ticket.stage_next", next).await;
    if let Some(message) = after["error"]["message"].as_str() {
        assert!(!message.contains("prime"), "{after}");
    }
}

/// A session Otaq started or resumed (no shell to type in): the same gate through the board.
async fn gate_rearmed_for(w: &TestService, t: &Ticket, session: &str) {
    let refused = w
        .board
        .refuse_unprimed("SHOP-1", session)
        .await
        .unwrap_err();
    assert_eq!(refused.to_string(), ONE_UNREAD);
    tick().await;
    w.board.record_prime(&t.id, session).await.unwrap();
    w.board.refuse_unprimed("SHOP-1", session).await.unwrap();
}

/// Send back on the default flow and a human Fail on a custom one (QA): the live Dev session primed before; the
/// note re-arms its gate.
#[tokio::test(flavor = "multi_thread")]
async fn send_back_and_a_human_fail_rearm_the_gate() {
    for flow in ["default", "every-kind"] {
        let w = world().await;
        if flow == "every-kind" {
            // QA's fails to the nearest Agent stage, the live Dev (its own `fail_to` is Grill).
            let mut file = every_kind();
            let qa = file.flow.position("qa").unwrap();
            file.flow.stages[qa].fail_to = None;
            w.board.use_flow_for_tests("SHOP", &file).await;
        }
        let t = w.create("SHOP", "Export orders as CSV").await;
        w.give_worktree(&t).await;
        w.force_status(&t, Status::Working).await;
        if flow == "every-kind" {
            place_in(&w, &t, "dev", Status::Working).await;
        }
        let dev = w.agent_shell(&t).await;
        send(&w, &dev, "ticket.prime", json!({"key": "SHOP-1"})).await;
        match flow {
            "default" => w.force_status(&t, Status::WaitingForYou).await,
            _ => place_in(&w, &t, "qa", Status::WaitingForYou).await,
        }
        tick().await;

        w.review
            .fail("SHOP-1", "Keep the column order.", None, false, &me())
            .await
            .unwrap();
        w.wait_for_output(&dev.session, "with a comment. Run").await;
        assert_eq!(w.status("SHOP-1").await, Status::Working, "{flow}");
        gate_rearmed(&w, &dev).await;
        w.stop().await;
    }
}

/// Send back with PR comments: the note re-arms the live Dev session's gate.
#[tokio::test(flavor = "multi_thread")]
async fn a_pr_send_back_rearms_the_gate() {
    let w = world().await;
    let t = w.create("SHOP", "Export orders as CSV").await;
    w.give_worktree(&t).await;
    w.force_status(&t, Status::Working).await;
    let dev = w.agent_shell(&t).await;
    send(&w, &dev, "ticket.prime", json!({"key": "SHOP-1"})).await;
    w.force_status(&t, Status::PrOpen).await;
    tick().await;

    w.review
        .send_back_pr("SHOP-1", "Keep the old route working.", None, false, &me())
        .await
        .unwrap();
    w.wait_for_output(&dev.session, "with PR comments and a comment")
        .await;
    gate_rearmed(&w, &dev).await;
    w.stop().await;
}

/// A failed check resuming Dev: the checker's comment re-arms the live Dev session's gate.
#[tokio::test(flavor = "multi_thread")]
async fn a_failed_check_rearms_devs_gate() {
    let w = world().await;
    let (t, dev) = in_dev(&w, "Export orders as CSV").await;
    send(&w, &dev, "ticket.prime", json!({"key": "SHOP-1"})).await;
    let checker = w.shell(&t, SessionKind::Validator).await;
    w.validating_with(&t, &checker.session).await;
    tick().await;

    let failed = end_stage(
        &w,
        &checker,
        "validation.verdict",
        json!({"key": "SHOP-1", "pass": false, "report": "Fail: no row limit."}),
    )
    .await;
    assert!(failed.get("reply").is_some(), "{failed}");
    assert_eq!(w.status("SHOP-1").await, Status::Working);
    gate_rearmed(&w, &dev).await;
    w.stop().await;
}

/// Drag back to Dev, which ran in the live session: the instructions re-arm its gate.
#[tokio::test(flavor = "multi_thread")]
async fn a_drag_back_resume_rearms_the_gate() {
    let w = world().await;
    w.board.use_flow_for_tests("SHOP", &every_kind()).await;
    let t = w.create("SHOP", "Export orders as CSV").await;
    w.give_worktree(&t).await;
    place_in(&w, &t, "dev", Status::Working).await;
    let dev = w.agent_shell(&t).await;
    send(&w, &dev, "ticket.prime", json!({"key": "SHOP-1"})).await;
    place_in(&w, &t, "qa", Status::WaitingForYou).await;
    tick().await;

    let answers = otaq_core::review::drag::DragAnswers {
        note: "Keep the old price column.",
        report: None,
        confirmed: true,
        clear_first: false,
    };
    w.review
        .drag(
            "SHOP-1",
            &otaq_core::board::moves::DropOn::stage("dev"),
            answers,
            &w.validation,
            &me(),
        )
        .await
        .unwrap();
    w.wait_for_output(&dev.session, "by you with a comment")
        .await;
    gate_rearmed(&w, &dev).await;
    w.stop().await;
}

/// Drag back into Grill, which the ticket skipped (a fresh session there), and back to Security (its check runs
/// again): each new session is told of the comment and has to prime first.
#[tokio::test(flavor = "multi_thread")]
async fn a_fresh_start_and_a_check_run_again_wait_for_prime() {
    for (to, kind) in [
        ("grill", SessionKind::Agent),
        ("security", SessionKind::Validator),
    ] {
        let w = launching_world().await;
        w.board.use_flow_for_tests("SHOP", &every_kind()).await;
        let t = w.create("SHOP", "Saved carts").await;
        let worktree = w.start_worktree(&t).await;
        otaq_core::git::testing::commit(&worktree, "notes.txt", "carts\n");
        sqlx::query(
            "INSERT INTO ticket_stage_choices (ticket_id, stage_id, choice, set_by, set_at)
             VALUES (?, 'grill', 'skip', 'user', 1)",
        )
        .bind(&t.id)
        .execute(w.board.store().pool())
        .await
        .unwrap();
        place_in(&w, &t, "qa", Status::WaitingForYou).await;

        let answers = otaq_core::review::drag::DragAnswers {
            note: "Ask about carts shared between devices.",
            report: None,
            confirmed: true,
            clear_first: false,
        };
        w.review
            .drag(
                "SHOP-1",
                &otaq_core::board::moves::DropOn::stage(to),
                answers,
                &w.validation,
                &me(),
            )
            .await
            .unwrap();
        let start = std::time::Instant::now();
        let session = loop {
            if let Some(s) = w.sessions.running_of(&t.id, kind) {
                break s;
            }
            assert!(start.elapsed().as_secs() < 20, "{to}: no session started");
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        };
        gate_rearmed_for(&w, &t, &session.id).await;
        w.stop().await;
    }
}
