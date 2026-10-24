//! Validation before Review (T-0183; VAL-01, VAL-13, SCH-04; d0063): the agent's submit opens a round and
//! starts a validator in the ticket's worktree; validation off keeps the old path; uncommitted files refuse it
//! first; a project's Validate skill is the prompt; under the usage limit it waits for its tool's Resume.
//! `claude` stands in as `sh` printing its arguments and environment: no test starts claude. (On Windows Git's `sh`
//! prints the folder with `pwd -W`, as Windows writes it.)

use otaq_core::agent_tools::claude::ClaudeLaunch;
use otaq_core::agent_tools::{LaunchContext, Tool};
use otaq_core::board::Refusal;
use otaq_core::board::settings::WorkflowStep;
use otaq_core::board::tickets::Ticket;
use otaq_core::client::{Client, ClientConfig};
use otaq_core::domain::{Actor, Status, ValidationTrigger};
use otaq_core::naming;
use otaq_core::protocol::{Reply, Request, ToolParams};
use otaq_core::service::ServiceConfig;
use otaq_core::service::testing::TestService;
use otaq_core::service::testing::said;
use otaq_core::session::SessionKind;
use otaq_core::store::now_ms;
use otaq_core::usage::limit::{LimitHit, LimitKind};

/// A service whose `claude` prints its arguments, its role, ticket and PORT, then waits.
async fn world() -> TestService {
    let script = fake_script();
    TestService::start_with(|config| ServiceConfig {
        launch: Some(fake_launch(script)),
        ..config
    })
    .await
}

fn fake_script() -> String {
    format!(
        "printf 'ARGS %s\\n' \"$*\"; printf 'ROLE=%s TICKET=%s PORT=%s\\n' \"${role}\" \"${ticket}\" \
         \"${{PORT-none}}\"; printf 'CWD=%s\\n' \"$(pwd -W 2>/dev/null || pwd -P)\"; exec sleep 30",
        role = naming::role_env_var(),
        ticket = naming::ticket_env_var(),
    )
}

fn fake_launch(script: String) -> LaunchContext {
    LaunchContext {
        bin_dir: "/nonexistent".into(),
        plugin_dir: "/nonexistent/plugin".into(),
        path: std::env::var_os("PATH").unwrap_or_default(),
        claude: ClaudeLaunch {
            program: vec!["sh".into(), "-c".into(), script, "sh".into()],
            config_dir: None,
        },
    }
}

/// A folder as the fake `claude` prints it: on Windows without the `\\?\` prefix and with forward slashes, as
/// `pwd -W` writes it.
fn folder_text(folder: &std::path::Path) -> String {
    let text = folder.display().to_string();
    if cfg!(windows) {
        text.trim_start_matches(r"\\?\").replace('\\', "/")
    } else {
        text
    }
}

/// A working ticket with a committed worktree.
async fn working(w: &TestService, title: &str) -> Ticket {
    let t = w.create("SHOP", title).await;
    w.give_worktree(&t).await;
    w.force_status(&t, Status::Working).await;
    w.board.ticket(&t.key.to_string()).await.unwrap()
}

fn agent(t: &Ticket) -> Actor {
    Actor::Agent(t.key.clone())
}

async fn submit(w: &TestService, t: &Ticket) -> Result<Ticket, otaq_core::board::BoardError> {
    w.validation
        .submit(
            &t.key.to_string(),
            said(&w.board, &t.key.to_string(), "Cart totals add up.")
                .await
                .as_ref(),
            &agent(t),
            ValidationTrigger::Agent,
        )
        .await
}

async fn rounds(w: &TestService, t: &Ticket) -> Vec<(i64, String, Option<String>, Option<String>)> {
    sqlx::query_as(
        "SELECT round, trigger, session_id, head_sha FROM validations WHERE ticket_id = ? ORDER BY started_at",
    )
    .bind(&t.id)
    .fetch_all(w.board.store().pool())
    .await
    .unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn submit_starts_a_validator_in_the_worktree() {
    let w = world().await;
    w.add_project("shop-web").await;
    let t = working(&w, "Cart totals").await;

    let after = submit(&w, &t).await.unwrap();
    assert_eq!(
        (after.board_group(), after.status),
        (t.board_group(), Status::Checking)
    );
    let validator = w
        .sessions
        .running_of(&t.id, SessionKind::Validator)
        .expect("a validator runs");
    // The stand-in prints its arguments, its environment and then its folder: wait for the last line.
    let out = w.wait_for_output(&validator.id, "CWD=").await;
    assert!(
        out.contains(&format!("/{}", naming::copy_name("validate"))),
        "{out}"
    );
    assert!(
        out.contains(&format!(
            "ROLE={} TICKET={} PORT=none",
            naming::ROLE_VALIDATOR,
            t.key
        )),
        "{out}"
    );
    assert!(!out.contains("work-ticket"), "{out}");
    let worktree = t.work.as_ref().unwrap().worktree.clone().unwrap();
    let worktree = folder_text(&std::fs::canonicalize(worktree).unwrap());
    assert!(
        out.contains(&format!("CWD={worktree}")),
        "runs in the worktree: {out}"
    );
    assert!(
        validator.tool_session_id.is_some(),
        "stored like every Claude session"
    );
    assert_eq!(
        (validator.tool, validator.model.as_deref()),
        (Some(otaq_core::agent_tools::Tool::Claude), None)
    );

    // The round: 1, started by the agent, with the validator's session and the head.
    let rows = rounds(&w, &t).await;
    assert_eq!(rows.len(), 1);
    let (round, trigger, session, head) = &rows[0];
    assert_eq!((*round, trigger.as_str()), (1, "agent"));
    assert_eq!(session.as_deref(), Some(validator.id.as_str()));
    assert_eq!(head.as_ref().map(String::len), Some(40));
    // The summary is the ticket's latest submit, for prime and the dependents.
    let history = w.board.history(&t.key.to_string()).await.unwrap();
    assert!(
        history
            .iter()
            .any(|e| e.payload["summary"] == "Cart totals add up.")
    );
    w.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn validation_off_goes_straight_to_review() {
    let w = world().await;
    w.add_project("shop-web").await;
    w.board
        .edit_flow_for_tests("SHOP", otaq_core::flow::default::testing::without_validate)
        .await;
    let t = working(&w, "Cart totals").await;
    let after = submit(&w, &t).await.unwrap();
    assert_eq!(after.status, Status::WaitingForYou);
    assert!(
        w.sessions
            .running_of(&t.id, SessionKind::Validator)
            .is_none()
    );
    assert!(rounds(&w, &t).await.is_empty());
    w.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn uncommitted_files_refuse_before_any_validator() {
    let w = world().await;
    w.add_project("shop-web").await;
    let t = working(&w, "Cart totals").await;
    let worktree = t.work.as_ref().unwrap().worktree.clone().unwrap();
    std::fs::write(std::path::Path::new(&worktree).join("scratch.txt"), "x").unwrap();
    let err = submit(&w, &t).await.unwrap_err();
    assert_eq!(err.refusal(), Some(Refusal::Uncommitted));
    assert_eq!(w.status(&t.key.to_string()).await, Status::Working);
    assert!(
        w.sessions
            .running_of(&t.id, SessionKind::Validator)
            .is_none()
    );
    assert!(rounds(&w, &t).await.is_empty());
    w.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn the_validators_imported_skill_is_the_prompt() {
    let w = world().await;
    w.add_project("shop-web").await;
    // An imported skill runs by its copy's name (AGM-07): Validator's, in place of `validate`.
    let folder = otaq_core::skills::testing::skill_folder(w.data.root(), "my-validate");
    w.import_step_skill(WorkflowStep::Validate, &folder).await;
    let t = working(&w, "Cart totals").await;
    submit(&w, &t).await.unwrap();
    let validator = w
        .sessions
        .running_of(&t.id, SessionKind::Validator)
        .unwrap();
    let named = format!("/{}", naming::copy_name("my-validate"));
    let out = w.wait_for_output(&validator.id, &named).await;
    assert!(
        !out.contains(&format!("/{} ", naming::copy_name("validate"))),
        "{out}"
    );
    w.stop().await;
}

/// SCH-04: while the usage limit holds, the round opens but no validator starts; its tool's Resume starts it.
#[tokio::test(flavor = "multi_thread")]
async fn under_the_usage_limit_the_validator_waits_for_resume_all() {
    let w = world().await;
    w.add_project("shop-web").await;
    let other = working(&w, "Search").await;
    let hit = LimitHit {
        kind: LimitKind::Session,
        text: "You've hit your session limit · resets 3:45pm".into(),
    };
    w.plan_limit
        .hit(Tool::Claude, &hit, None, &other.id, now_ms())
        .await
        .unwrap();

    let t = working(&w, "Cart totals").await;
    let after = submit(&w, &t).await.unwrap();
    assert_eq!(after.status, Status::WaitingForLimitReset);
    assert!(
        w.sessions
            .running_of(&t.id, SessionKind::Validator)
            .is_none()
    );
    assert_eq!(rounds(&w, &t).await.len(), 1, "the round is open");

    let client = Client::connect(ClientConfig::new(&w.data)).await.unwrap();
    let reply = client
        .request(Request::LimitsResumeTool(ToolParams { tool: Tool::Claude }))
        .await
        .unwrap();
    let Reply::Resumed(keys) = reply else {
        panic!("{reply:?}")
    };
    assert!(keys.contains(&t.key), "{keys:?}");
    assert_eq!(w.status(&t.key.to_string()).await, Status::Checking);
    let validator = w
        .sessions
        .running_of(&t.id, SessionKind::Validator)
        .expect("started on Resume");
    w.wait_for_output(
        &validator.id,
        &format!("/{}", naming::copy_name("validate")),
    )
    .await;
    assert_eq!(
        rounds(&w, &t).await[0].2.as_deref(),
        Some(validator.id.as_str())
    );
    w.stop().await;
}

/// The running validator's session.
fn validator_of(w: &TestService, t: &Ticket) -> String {
    w.sessions
        .running_of(&t.id, SessionKind::Validator)
        .expect("a validator runs")
        .id
}

async fn verdict(w: &TestService, t: &Ticket, pass: bool, report: &str) -> Ticket {
    let session = validator_of(w, t);
    w.validation
        .verdict(
            &t.key.to_string(),
            pass,
            said(&w.board, &t.key.to_string(), report).await.as_ref(),
            &session,
        )
        .await
        .unwrap()
}

async fn set_rounds(w: &TestService, rounds: u32) {
    w.board
        .edit_flow_for_tests(
            "SHOP",
            otaq_core::flow::default::testing::with_rounds(rounds),
        )
        .await;
}

async fn outcomes(
    w: &TestService,
    t: &Ticket,
) -> Vec<(i64, Option<String>, Option<String>, Option<String>)> {
    sqlx::query_as(
        "SELECT v.round, v.outcome, c.body, v.notes_json FROM validations v
         LEFT JOIN stage_runs r ON r.validation_id = v.id
         LEFT JOIN ticket_comments c ON c.id = r.comment_id
         WHERE v.ticket_id = ? ORDER BY v.started_at",
    )
    .bind(&t.id)
    .fetch_all(w.board.store().pool())
    .await
    .unwrap()
}

const FINDINGS: &str =
    "src/cart.ts:12 the total ignores the discount\n- Note: covered by SHOP-40.2";

/// Pass: Ready for Review "Validated · round 1", the round's head is the reviewed head, the report is stored.
#[tokio::test(flavor = "multi_thread")]
async fn a_pass_enters_review_validated() {
    let w = world().await;
    w.add_project("shop-web").await;
    let t = working(&w, "Cart totals").await;
    submit(&w, &t).await.unwrap();
    let head = rounds(&w, &t).await[0].3.clone();
    let after = verdict(
        &w,
        &t,
        true,
        "Checked the totals.\nNote: covered by SHOP-40.2",
    )
    .await;
    assert_eq!(after.status, Status::WaitingForYou);
    let entry = after.flags.review_entry.unwrap();
    assert_eq!(entry.to_string(), "Validated · round 1");
    let row = w.board.store().ticket(&t.id).await.unwrap().unwrap();
    assert_eq!(row.review_head_sha, head);
    assert!(row.review_entered_at.is_some());
    assert_eq!(
        outcomes(&w, &t).await,
        [(
            1,
            Some("pass".into()),
            Some("Checked the totals.\nNote: covered by SHOP-40.2".into()),
            Some(r#"["covered by SHOP-40.2"]"#.into())
        )]
    );
    w.stop().await;
}

/// Fail with rounds left: the findings go to the agent with "round N of M" and it works again; the last round
/// turns turquoise and delivers nothing.
#[tokio::test(flavor = "multi_thread")]
async fn a_fail_goes_back_to_the_agent_until_the_last_round() {
    let w = world().await;
    w.add_project("shop-web").await;
    let t = working(&w, "Cart totals").await;
    let agent = w.agent_shell(&t).await;

    submit(&w, &t).await.unwrap();
    let after = verdict(&w, &t, false, FINDINGS).await;
    assert_eq!(after.status, Status::Working);
    assert_eq!(
        after.flags.validation_round.unwrap().to_string(),
        "Validation failed · round 1 of 3"
    );
    let pending = w.deliveries.pending(&agent.session).await.unwrap();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].kind, "validation");
    // The findings are the checker's comment: the agent is told only the round (CMT-05).
    assert_eq!(
        pending[0].text,
        "Validate failed round 1. Run `otaq prime`."
    );

    for round in 2..=3 {
        let t = w.board.ticket(&t.key.to_string()).await.unwrap();
        submit(&w, &t).await.unwrap();
        assert_eq!(rounds(&w, &t).await.len(), round, "round {round} opens");
        verdict(&w, &t, false, FINDINGS).await;
    }
    let last = w.board.ticket(&t.key.to_string()).await.unwrap();
    assert_eq!(last.status, Status::CheckFailed);
    assert_eq!(last.validation_round, 3);
    assert_eq!(
        w.deliveries.pending(&agent.session).await.unwrap().len(),
        2,
        "rounds 1 and 2 were delivered, the last one isn't"
    );
    let all = outcomes(&w, &t).await;
    assert_eq!(all.len(), 3);
    assert!(
        all.iter()
            .all(|(_, o, r, _)| o.as_deref() == Some("fail") && r.is_some())
    );
    w.stop().await;
}

/// One round: the first failure is the last. Five rounds: four go back to the agent.
#[tokio::test(flavor = "multi_thread")]
async fn rounds_one_and_five() {
    let w = world().await;
    w.add_project("shop-web").await;
    set_rounds(&w, 1).await;
    let one = working(&w, "Cart totals").await;
    submit(&w, &one).await.unwrap();
    assert_eq!(
        verdict(&w, &one, false, FINDINGS).await.status,
        Status::CheckFailed
    );

    set_rounds(&w, 5).await;
    let five = working(&w, "Checkout").await;
    for round in 1..=5 {
        let t = w.board.ticket(&five.key.to_string()).await.unwrap();
        submit(&w, &t).await.unwrap();
        let after = verdict(&w, &t, false, FINDINGS).await;
        let expected = if round < 5 {
            Status::Working
        } else {
            Status::CheckFailed
        };
        assert_eq!(after.status, expected, "round {round}");
    }
    w.stop().await;
}

/// Only the round's own validator session reports; the round's verdict ends it once its turn ends.
#[tokio::test(flavor = "multi_thread")]
async fn only_the_running_validator_reports_and_it_ends_after_its_turn() {
    let w = world().await;
    w.add_project("shop-web").await;
    let t = working(&w, "Cart totals").await;
    submit(&w, &t).await.unwrap();
    let err = w
        .validation
        .verdict(
            &t.key.to_string(),
            true,
            said(&w.board, &t.key.to_string(), "").await.as_ref(),
            "another-session",
        )
        .await
        .unwrap_err();
    assert_eq!(err.refusal(), Some(Refusal::NotAllowed));
    let session = validator_of(&w, &t);
    verdict(&w, &t, true, "ok").await;
    assert!(
        w.sessions.info(&session).unwrap().is_running(),
        "its turn isn't over yet"
    );
    w.validation.after_validator_turn(&session).await.unwrap();
    assert!(
        w.sessions
            .running_of(&t.id, SessionKind::Validator)
            .is_none()
    );
    w.stop().await;
}

/// After Send back the counter restarts at 0, and `prime --validate` lists the review comments to check.
#[tokio::test(flavor = "multi_thread")]
async fn send_back_restarts_the_counter_and_prime_shows_the_comments() {
    use otaq_core::board::comments::{NewComment, Side};
    let w = world().await;
    w.add_project("shop-web").await;
    let t = working(&w, "Cart totals").await;
    let _agent = w.agent_shell(&t).await;
    submit(&w, &t).await.unwrap();
    verdict(&w, &t, false, FINDINGS).await;
    let t = w.board.ticket(&t.key.to_string()).await.unwrap();
    submit(&w, &t).await.unwrap();
    verdict(&w, &t, true, "ok").await;
    let t = w.board.ticket(&t.key.to_string()).await.unwrap();
    assert_eq!(
        t.flags.review_entry.unwrap().to_string(),
        "Validated · round 2"
    );

    w.board
        .add_comment(&NewComment {
            key: t.key.to_string(),
            file: "src/cart.ts".into(),
            line: 12,
            side: Side::New,
            body: "Round the total".into(),
        })
        .await
        .unwrap();
    w.review
        .fail(
            &t.key.to_string(),
            "Also the tax",
            None,
            false,
            &Actor::User("me".into()),
        )
        .await
        .unwrap();
    let back = w.board.ticket(&t.key.to_string()).await.unwrap();
    assert_eq!(back.validation_round, 0);
    assert!(back.flags.review_entry.is_none());

    submit(&w, &back).await.unwrap();
    let prime = w.board.validate_prime(&t.key.to_string()).await.unwrap();
    assert_eq!((prime.round, prime.rounds), (1, 3));
    let text = otaq_core::prime::render_validate(&prime);
    assert!(
        text.contains("## The user's review (Send back, round 1)"),
        "{text}"
    );
    // Only the line comments (CMT-07): the note is the user's comment (CMT-05), under "## Comments".
    assert!(!text.contains("Note: Also the tax"), "{text}");
    let comments = &text[text.find("\n## Comments\n").expect("a Comments section")..];
    assert!(comments.contains("\n\nAlso the tax\n"), "{text}");
    assert!(text.contains("- src/cart.ts:12: Round the total"), "{text}");
    assert!(
        !text.contains("Earlier rounds"),
        "the counter restarted: {text}"
    );
    w.stop().await;
}

/// Runs git in `dir` with a fixed identity.
fn git(dir: &std::path::Path, args: &[&str]) {
    let ok = std::process::Command::new("git")
        .args(["-c", "user.name=t", "-c", "user.email=t@t"])
        .args(args)
        .current_dir(dir)
        .status()
        .unwrap()
        .success();
    assert!(ok, "git {args:?}");
}

fn worktree(t: &Ticket) -> std::path::PathBuf {
    t.work.as_ref().unwrap().worktree.clone().unwrap().into()
}

async fn last_outcome(w: &TestService, t: &Ticket) -> Option<String> {
    outcomes(w, t).await.last().and_then(|o| o.1.clone())
}

/// VAL-02: the validator edits a tracked file, or commits, without the agent taking a turn: its verdict is
/// discarded, with the message, and the round doesn't count.
#[tokio::test(flavor = "multi_thread")]
async fn a_validator_that_changes_files_gets_its_verdict_discarded() {
    let w = world().await;
    w.add_project("shop-web").await;
    for commit in [false, true] {
        let t = working(&w, if commit { "Commits" } else { "Edits" }).await;
        submit(&w, &t).await.unwrap();
        let dir = worktree(&t);
        std::fs::write(dir.join("file1.txt"), "changed by the validator\n").unwrap();
        if commit {
            git(&dir, &["commit", "-qam", "validator commit"]);
        }
        let session = validator_of(&w, &t);
        let err = w
            .validation
            .verdict(
                &t.key.to_string(),
                true,
                said(&w.board, &t.key.to_string(), "fine").await.as_ref(),
                &session,
            )
            .await
            .unwrap_err();
        assert!(
            err.to_string()
                .ends_with(otaq_core::domain::validation::DISCARDED),
            "{err}"
        );
        let after = w.board.ticket(&t.key.to_string()).await.unwrap();
        assert_eq!(after.status, Status::CheckNoResult);
        assert_eq!(
            after.signal.as_ref().and_then(|s| s.text.as_deref()),
            Some(otaq_core::domain::validation::DISCARDED)
        );
        assert_eq!(after.validation_round, 0, "not a round");
        assert_eq!(last_outcome(&w, &t).await.as_deref(), Some("discarded"));
        assert!(
            w.sessions
                .running_of(&t.id, SessionKind::Validator)
                .is_none()
        );
    }
    w.stop().await;
}

/// Files ignored by `.gitignore` (test caches, coverage output) and untracked scratch don't count.
#[tokio::test(flavor = "multi_thread")]
async fn ignored_files_change_nothing() {
    let w = world().await;
    w.add_project("shop-web").await;
    let t = working(&w, "Cart totals").await;
    let dir = worktree(&t);
    std::fs::write(dir.join(".gitignore"), "coverage/\n").unwrap();
    git(&dir, &["add", ".gitignore"]);
    git(&dir, &["commit", "-qm", "ignore coverage"]);
    submit(&w, &t).await.unwrap();
    std::fs::create_dir_all(dir.join("coverage")).unwrap();
    std::fs::write(dir.join("coverage/lcov.info"), "TN:\n").unwrap();
    let after = verdict(&w, &t, true, "ok").await;
    assert_eq!(after.status, Status::WaitingForYou);
    w.stop().await;
}

/// VAL-08: the agent takes a turn and commits: the validation is cancelled, not counted, the validator stops,
/// and the agent is told.
#[tokio::test(flavor = "multi_thread")]
async fn an_agent_commit_during_validation_cancels_it() {
    let w = world().await;
    w.add_project("shop-web").await;
    let t = working(&w, "Cart totals").await;
    let agent = w.agent_shell(&t).await;
    submit(&w, &t).await.unwrap();

    // A turn that changes nothing cancels nothing (typing to the agent is allowed).
    w.validation.agent_working(&t.id).await;
    w.validation.agent_turn_ended(&t.id).await.unwrap();
    assert_eq!(w.status(&t.key.to_string()).await, Status::Checking);

    w.validation.agent_working(&t.id).await;
    let dir = worktree(&t);
    std::fs::write(dir.join("file1.txt"), "the agent's fix\n").unwrap();
    git(&dir, &["commit", "-qam", "agent fix"]);
    w.validation.agent_turn_ended(&t.id).await.unwrap();
    let after = w.board.ticket(&t.key.to_string()).await.unwrap();
    assert_eq!((after.status, after.validation_round), (Status::Working, 0));
    assert_eq!(last_outcome(&w, &t).await.as_deref(), Some("cancelled"));
    assert!(
        w.sessions
            .running_of(&t.id, SessionKind::Validator)
            .is_none()
    );
    let pending = w.deliveries.pending(&agent.session).await.unwrap();
    assert_eq!(
        pending.last().map(|p| p.text.as_str()),
        Some(
            "Validate was cancelled because the code changed. Run `otaq stage next` when you're done."
        )
    );
    w.stop().await;
}

/// The user's description edit cancels it and the agent also gets the change note; a title-only edit only
/// sends the note.
#[tokio::test(flavor = "multi_thread")]
async fn a_description_edit_cancels_a_title_edit_does_not() {
    use otaq_core::board::tickets::TicketUpdate;
    use otaq_core::protocol::TicketUpdateParams;
    let w = world().await;
    w.add_project("shop-web").await;
    let t = working(&w, "Cart totals").await;
    let agent = w.agent_shell(&t).await;
    submit(&w, &t).await.unwrap();
    let client = Client::connect(ClientConfig::new(&w.data)).await.unwrap();
    let update = |update: TicketUpdate| {
        Request::TicketUpdate(TicketUpdateParams {
            key: t.key.to_string(),
            update,
        })
    };

    client
        .request(update(TicketUpdate {
            title: Some("Cart totals v2".into()),
            ..TicketUpdate::default()
        }))
        .await
        .unwrap();
    assert_eq!(w.status(&t.key.to_string()).await, Status::Checking);
    let pending = w.deliveries.pending(&agent.session).await.unwrap();
    assert_eq!(pending.len(), 1);
    assert!(
        pending[0]
            .text
            .contains("The title is now 'Cart totals v2'.")
    );

    let mut events = client.subscribe().await.unwrap();
    client
        .request(update(TicketUpdate {
            body: Some("Totals include the discount.".into()),
            ..TicketUpdate::default()
        }))
        .await
        .unwrap();
    assert_eq!(w.status(&t.key.to_string()).await, Status::Working);
    let texts: Vec<String> = w
        .deliveries
        .pending(&agent.session)
        .await
        .unwrap()
        .into_iter()
        .map(|p| p.text)
        .collect();
    assert_eq!(texts.len(), 3, "{texts:?}");
    assert_eq!(
        texts[1],
        otaq_core::validation::cancelled_message("Validate")
    );
    assert!(
        texts[2].starts_with("The ticket was updated by the user. The description changed"),
        "{texts:?}"
    );
    // The cancel toast (T-0196, UI-07).
    let toast = tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            if let Ok(otaq_core::protocol::Event::ValidationCancelled { title, message, .. }) =
                events.recv().await
            {
                return (title, message);
            }
        }
    })
    .await
    .expect("the cancel toast");
    assert_eq!(
        toast,
        (
            format!("Validation cancelled: {}’s description changed", t.key),
            "Round not counted · the agent was told to submit again".to_owned()
        )
    );
    w.stop().await;
}

async fn act(
    w: &TestService,
    t: &Ticket,
    action: otaq_core::domain::ValidationAction,
    note: &str,
) -> Result<Ticket, otaq_core::board::BoardError> {
    w.validation
        .user_action(&t.key.to_string(), action, note, &Actor::User("me".into()))
        .await
}

/// T-0406 (d0108 rule 10): the user's Move to next stage counts as the agent's `stage next`: with Validation on the
/// check runs (it used to skip validation; Validate and submit is gone), also from Interrupted; with Validation off
/// the ticket is Ready for Review, "Submitted by you". Refused while validating.
#[tokio::test(flavor = "multi_thread")]
async fn the_users_move_to_next_stage() {
    use otaq_core::domain::ReviewEntry;
    let w = world().await;
    w.add_project("shop-web").await;
    let me = Actor::User("me".into());

    for status in [Status::Idle, Status::Interrupted] {
        let t = working(&w, "Checkout").await;
        w.force_status(&t, status).await;
        let after = w
            .validation
            .submit_by_user(&t.key.to_string(), "please check", &me)
            .await
            .unwrap();
        assert_eq!(after.status, Status::Checking, "{status}");
        assert_eq!(rounds(&w, &t).await[0].1, "user");
        assert!(
            w.sessions
                .running_of(&t.id, SessionKind::Validator)
                .is_some()
        );
        let refused = w
            .validation
            .submit_by_user(&t.key.to_string(), "", &me)
            .await
            .err();
        assert!(refused.is_some(), "refused while validating");
    }

    w.board
        .edit_flow_for_tests("SHOP", otaq_core::flow::default::testing::without_validate)
        .await;
    let t = working(&w, "Cart totals").await;
    w.force_status(&t, Status::Idle).await;
    let after = w
        .validation
        .submit_by_user(&t.key.to_string(), "", &me)
        .await
        .unwrap();
    assert_eq!(
        (after.status, after.flags.review_entry.map(|e| e.entry)),
        (Status::WaitingForYou, Some(ReviewEntry::SubmittedByUser))
    );
    w.stop().await;
}

/// Stop this check and Skip this check, only while validating.
#[tokio::test(flavor = "multi_thread")]
async fn stop_and_skip() {
    use otaq_core::domain::{ReviewEntry, ValidationAction as A};
    let w = world().await;
    w.add_project("shop-web").await;
    let t = working(&w, "Cart totals").await;
    for action in [A::Stop, A::Skip] {
        assert!(
            act(&w, &t, action, "").await.is_err(),
            "{action}: not validating"
        );
    }
    submit(&w, &t).await.unwrap();
    let after = act(&w, &t, A::Stop, "").await.unwrap();
    assert_eq!((after.status, after.validation_round), (Status::Working, 0));
    assert_eq!(last_outcome(&w, &t).await.as_deref(), Some("stopped"));
    assert!(
        w.sessions
            .running_of(&t.id, SessionKind::Validator)
            .is_none()
    );

    let t = w.board.ticket(&t.key.to_string()).await.unwrap();
    submit(&w, &t).await.unwrap();
    let after = act(&w, &t, A::Skip, "").await.unwrap();
    assert_eq!(
        (after.status, after.flags.review_entry.map(|e| e.entry)),
        (Status::WaitingForYou, Some(ReviewEntry::Skipped))
    );
    assert_eq!(last_outcome(&w, &t).await.as_deref(), Some("skipped"));
    w.stop().await;
}

/// After the last failed round: Move on anyway, Give it another round, Answer the agent. After no
/// result: Retry validation and Move on anyway ("Validation skipped by you").
#[tokio::test(flavor = "multi_thread")]
async fn after_a_failed_or_empty_validation() {
    use otaq_core::domain::{ReviewEntry, ValidationAction as A};
    let w = world().await;
    w.add_project("shop-web").await;
    set_rounds(&w, 1).await;

    // Failed 1 time → moved by the user.
    let t = working(&w, "Moved").await;
    submit(&w, &t).await.unwrap();
    verdict(&w, &t, false, FINDINGS).await;
    let after = act(&w, &t, A::MoveOnAnyway, "").await.unwrap();
    let entry = after.flags.review_entry.unwrap();
    assert_eq!(
        (after.status, entry.entry, entry.to_string().as_str()),
        (
            Status::WaitingForYou,
            ReviewEntry::FailedMoved,
            "Failed 1 time · moved by you"
        )
    );

    // Another round: the findings go back, one more round is allowed.
    let t = working(&w, "Another").await;
    let agent = w.agent_shell(&t).await;
    submit(&w, &t).await.unwrap();
    verdict(&w, &t, false, FINDINGS).await;
    let after = act(&w, &t, A::AnotherRound, "").await.unwrap();
    assert_eq!(after.status, Status::Working);
    assert_eq!(
        after.flags.validation_round.unwrap().to_string(),
        "Validation failed · round 1 of 2"
    );
    let text = w.deliveries.pending(&agent.session).await.unwrap()[0]
        .text
        .clone();
    assert!(
        text.contains("another round") && text.ends_with(FINDINGS),
        "{text}"
    );

    // Answer the agent: the note with the findings, the counter back at 0.
    let t = working(&w, "Answered").await;
    let agent = w.agent_shell(&t).await;
    submit(&w, &t).await.unwrap();
    verdict(&w, &t, false, FINDINGS).await;
    let after = act(&w, &t, A::AnswerAgent, "The discount is per line.")
        .await
        .unwrap();
    assert_eq!((after.status, after.validation_round), (Status::Working, 0));
    let text = w.deliveries.pending(&agent.session).await.unwrap()[0]
        .text
        .clone();
    assert!(
        text.starts_with("The user answered: The discount is per line.")
            && text.ends_with(FINDINGS),
        "{text}"
    );

    // No result: Retry validation starts a fresh validator for the same round.
    let t = working(&w, "Retried").await;
    submit(&w, &t).await.unwrap();
    w.validation
        .validator_signal(
            &t.id,
            &validator_of(&w, &t),
            &otaq_core::signals::Signal::Idle { last_line: None },
        )
        .await
        .unwrap();
    assert_eq!(w.status(&t.key.to_string()).await, Status::CheckNoResult);
    let after = act(&w, &t, A::Retry, "").await.unwrap();
    assert_eq!(after.status, Status::Checking);
    let all = rounds(&w, &t).await;
    assert_eq!(
        all.iter().map(|r| r.0).collect::<Vec<_>>(),
        [1, 1],
        "not a round"
    );
    assert!(
        w.sessions
            .running_of(&t.id, SessionKind::Validator)
            .is_some()
    );
    // …or Move on anyway: "Validation skipped by you".
    w.validation
        .validator_signal(
            &t.id,
            &validator_of(&w, &t),
            &otaq_core::signals::Signal::Idle { last_line: None },
        )
        .await
        .unwrap();
    let after = act(&w, &t, A::MoveOnAnyway, "").await.unwrap();
    assert_eq!(
        after.flags.review_entry.map(|e| e.entry),
        Some(ReviewEntry::Skipped)
    );
    w.stop().await;
}

/// Park and Close stop the agent and the validator; the validation is recorded as stopped.
#[tokio::test(flavor = "multi_thread")]
async fn park_and_close_stop_both_sessions() {
    let w = world().await;
    w.add_project("shop-web").await;
    let me = Actor::User("me".into());
    for close in [false, true] {
        let t = working(&w, if close { "Closed" } else { "Parked" }).await;
        let _agent = w.agent_shell(&t).await;
        submit(&w, &t).await.unwrap();
        if close {
            w.review.close(&t.key.to_string(), true, &me).await.unwrap();
        } else {
            w.review.park(&t.key.to_string(), true, &me).await.unwrap();
        }
        assert!(
            w.sessions
                .running_of(&t.id, SessionKind::Validator)
                .is_none()
        );
        assert!(w.sessions.running_of(&t.id, SessionKind::Agent).is_none());
        assert_eq!(last_outcome(&w, &t).await.as_deref(), Some("stopped"));
    }
    w.stop().await;
}

/// T-0490 (#58; d0045, d0119): a ticket whose agent asked the user during its check is Interrupted after a
/// restart and keeps its question, in Needs you's turquoise items; the round isn't counted. A ticket interrupted
/// mid-check is in the Interrupted group.
#[tokio::test(flavor = "multi_thread")]
async fn a_question_asked_during_a_check_is_kept_after_a_restart() {
    use otaq_core::board::needs_you::NeedsYouGroup;
    let w = world().await;
    w.add_project("shop-web").await;
    let asks = working(&w, "Cart totals").await;
    let checking = working(&w, "Coupon field").await;
    for t in [&asks, &checking] {
        submit(&w, t).await.unwrap();
    }
    let question = "Should totals round per line or per order?";
    w.board
        .store_question(&asks.id, question, &[], &[], None)
        .await
        .unwrap();
    let caller = otaq_core::caller::Caller::Agent {
        ticket: asks.key.clone(),
        ticket_id: asks.id.clone(),
        session: validator_of(&w, &asks),
    };
    w.signals
        .ask_human(&caller, &asks.id, question)
        .await
        .unwrap();
    assert_eq!(w.status(&asks.key.to_string()).await, Status::AsksYou);

    let w = w
        .restart_with(|config| ServiceConfig {
            launch: Some(fake_launch(fake_script())),
            ..config
        })
        .await;
    let report = w.recovery.startup_report();
    assert_eq!(report.was_validating, std::slice::from_ref(&checking.key));
    assert!(report.interrupted.contains(&asks.key));
    let after = w.board.ticket(&asks.key.to_string()).await.unwrap();
    assert_eq!(
        (after.status, after.interrupted_from),
        (Status::Interrupted, Some(Status::AsksYou))
    );
    assert_eq!(after.question.map(|q| q.text).as_deref(), Some(question));
    assert_eq!(
        last_outcome(&w, &asks).await.as_deref(),
        Some("interrupted")
    );
    let items = w.board.needs_you_items(now_ms()).await.unwrap();
    let item = |t: &Ticket| items.iter().find(|i| i.key == t.key).unwrap();
    assert_eq!(item(&asks).group, NeedsYouGroup::Family);
    assert_eq!(item(&asks).reason, format!("asks: {question}"));
    assert_eq!(item(&checking).group, NeedsYouGroup::Stopped);
    assert_eq!(item(&checking).reason, "session interrupted");
    w.stop().await;
}

/// T-0188 (REC-01, VAL-07): after a restart a validating ticket is Interrupted from Validating, in Resume all;
/// Resume starts a fresh validator with the same round number and leaves the agent alone; the agent is resumed
/// only when the findings must reach it.
#[tokio::test(flavor = "multi_thread")]
async fn after_a_restart_resume_validates_again() {
    let w = world().await;
    w.add_project("shop-web").await;
    let t = working(&w, "Cart totals").await;
    w.record_ended_agent(&t, "0199d2a4-3b1c-7c3e-9f00-123456789abc")
        .await;
    submit(&w, &t).await.unwrap();
    let w = w
        .restart_with(|config| ServiceConfig {
            launch: Some(fake_launch(fake_script())),
            ..config
        })
        .await;
    let key = t.key.to_string();
    let after = w.board.ticket(&key).await.unwrap();
    assert_eq!(after.status, Status::Interrupted);
    assert_eq!(after.interrupted_from, Some(Status::Checking));
    assert_eq!(last_outcome(&w, &t).await.as_deref(), Some("interrupted"));
    let tickets = w.board.tickets("SHOP").await.unwrap();
    assert!(otaq_core::recovery::resume_all_eligible(&tickets).contains(&t.key));

    let resumed = w
        .review
        .resume(&key, &Actor::User("me".into()))
        .await
        .unwrap();
    assert_eq!(resumed.status, Status::Checking);
    assert!(
        w.sessions
            .running_of(&t.id, SessionKind::Validator)
            .is_some()
    );
    assert!(
        w.sessions.running_of(&t.id, SessionKind::Agent).is_none(),
        "the agent isn't resumed for a validation"
    );
    let all = rounds(&w, &t).await;
    assert_eq!(all.iter().map(|r| r.0).collect::<Vec<_>>(), [1, 1]);

    // A failed round must reach the agent: its conversation is resumed with what came back.
    let session = validator_of(&w, &t);
    w.validation
        .verdict(
            &key,
            false,
            said(&w.board, &key, FINDINGS).await.as_ref(),
            &session,
        )
        .await
        .unwrap();
    let start = std::time::Instant::now();
    let agent = loop {
        if let Some(agent) = w.sessions.running_of(&t.id, SessionKind::Agent) {
            break agent;
        }
        assert!(
            start.elapsed() < std::time::Duration::from_secs(10),
            "no agent"
        );
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    };
    let out = w.wait_for_output(&agent.id, "--resume").await;
    assert!(
        out.contains("0199d2a4-3b1c-7c3e-9f00-123456789abc"),
        "{out}"
    );
    // Resumed with what came back, never the findings (the checker's comment, CMT-05).
    let out = w
        .wait_for_output(&agent.id, "Validate failed round 1. Run `otaq prime`.")
        .await;
    assert!(!out.contains("src/cart.ts:12"), "{out}");
    w.stop().await;
}
