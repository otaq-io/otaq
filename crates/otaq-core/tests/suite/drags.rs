//! Drags to any stage (T-0421; FLW-18; d0108 "Dragging a card back to any earlier stage", "Other drags", second gap
//! review rule 2; d0052, d0059): on the test flow (`tests/flows/every-kind.yaml`: Product spec · Dev · Validate ·
//! Security · QA · Done) and the default flow, each drop the board takes: back to an Agent stage with instructions (its
//! session resumes; a skipped one runs once in a fresh session), stopping a running checker; back to an agent check
//! with a note; back onto a human check with no dialog; forward to the next stage (Move to next stage) and past stages
//! (they are skipped by you and the Delivery waits for a click); a parked card back to the stage it left. Every drag
//! back resets the agent checks' counters. `claude` is stood in for by ordinary commands.

use otaq_core::service::testing::said;
use std::path::Path;
use std::time::Duration;

use otaq_core::agent_tools::LaunchContext;
use otaq_core::agent_tools::claude::ClaudeLaunch;
use otaq_core::board::Refusal;
use otaq_core::board::moves::{DropOn, DropTarget};
use otaq_core::board::tickets::{EventKind, HistoryEvent, Ticket};
use otaq_core::domain::{Actor, Place, Status, ValidationTrigger};
use otaq_core::flow::engine::RunResult;
use otaq_core::flow::{
    CheckBy, Column, Delivers, Delivery, DeliveryMode, FlowFile, Stage, StageKind, yaml,
};
use otaq_core::git::testing::commit;
use otaq_core::review::ReturnOutcome;
use otaq_core::review::drag::DragAnswers;
use otaq_core::service::ServiceConfig;
use otaq_core::service::testing::TestService;
use otaq_core::session::SessionKind;

fn me() -> Actor {
    Actor::User("me".into())
}

/// Keeps running, printing its arguments.
const KEEPS_RUNNING: &str = "printf 'ARGS %s\\n' \"$*\"; exec sleep 30";

async fn world() -> TestService {
    let w = TestService::start_with(|config| ServiceConfig {
        launch: Some(LaunchContext {
            bin_dir: "/nonexistent".into(),
            plugin_dir: "/nonexistent/plugin".into(),
            path: std::env::var_os("PATH").unwrap_or_default(),
            claude: ClaudeLaunch {
                program: vec!["sh".into(), "-c".into(), KEEPS_RUNNING.into(), "sh".into()],
                config_dir: None,
            },
        }),
        ..config
    })
    .await;
    w.add_project("shop-web").await;
    w
}

fn every_kind() -> FlowFile {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/flows/every-kind.yaml");
    yaml::read(&std::fs::read_to_string(path).unwrap()).unwrap()
}

/// The test flow with a Review human check between QA and the Delivery, which delivers a PR on a click.
fn with_review() -> FlowFile {
    let mut file = every_kind();
    let at = file.flow.position("ship").unwrap();
    file.flow.stages.insert(
        at,
        Stage {
            id: "review".into(),
            name: "Review".into(),
            kind: StageKind::Check { by: CheckBy::You },
            column: Column::Own,
            agent: None,
            agents: Vec::new(),
            skill: None,
            options: Vec::new(),
            rounds: None,
            fail_to: None,
            delivery: None,
            why: None,
        },
    );
    for stage in file.flow.stages.iter_mut().filter(|s| s.is_delivery()) {
        stage.delivery = Some(Delivery {
            mode: DeliveryMode::Click,
            delivers: Delivers::Pr,
        });
    }
    file
}

/// Puts the ticket in `stage` with `status` in one write.
async fn place_in(w: &TestService, t: &Ticket, stage: &str, status: Status) {
    sqlx::query("UPDATE tickets SET stage_id = ?, state = ?, state_since = ? WHERE id = ?")
        .bind(stage)
        .bind(status.as_str())
        .bind(otaq_core::store::now_ms())
        .bind(&t.id)
        .execute(w.board.store().pool())
        .await
        .unwrap();
}

/// A ticket with a worktree holding one commit, in `stage` with `status`.
async fn at(w: &TestService, title: &str, stage: &str, status: Status) -> Ticket {
    let t = w.create("SHOP", title).await;
    let wt = w.start_worktree(&t).await;
    commit(&wt, "notes.txt", &format!("{title}\n"));
    place_in(w, &t, stage, status).await;
    w.board.ticket(&t.key.to_string()).await.unwrap()
}

/// The ticket ran Dev in the conversation `conversation`, then moved on to `stage`.
async fn ran_dev_then(w: &TestService, title: &str, stage: &str, status: Status) -> Ticket {
    let t = at(w, title, "dev", Status::Working).await;
    w.record_ended_agent(&t, &format!("{title}-dev")).await;
    place_in(w, &t, stage, status).await;
    w.board.ticket(&t.key.to_string()).await.unwrap()
}

async fn counted(w: &TestService, t: &Ticket) {
    sqlx::query(
        "INSERT INTO check_counters (ticket_id, stage_id, fails, updated_at)
         VALUES (?1, 'validate', 2, 1), (?1, 'security', 1, 2)",
    )
    .bind(&t.id)
    .execute(w.board.store().pool())
    .await
    .unwrap();
}

async fn drag(
    w: &TestService,
    key: &str,
    target: impl Into<DropOn>,
    note: &str,
) -> Result<ReturnOutcome, otaq_core::board::BoardError> {
    let answers = DragAnswers {
        note,
        report: None,
        confirmed: true,
        clear_first: false,
    };
    w.review
        .drag(key, &target.into(), answers, &w.validation, &me())
        .await
}

async fn until_status(w: &TestService, key: &str, want: Status) {
    let reached = tokio::time::timeout(Duration::from_secs(20), async {
        while w.status(key).await != want {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await;
    assert!(reached.is_ok(), "{key} never became {want}");
}

async fn dragged(w: &TestService, key: &str) -> HistoryEvent {
    w.board
        .history(key)
        .await
        .unwrap()
        .into_iter()
        .rfind(|e| e.kind == EventKind::Dragged)
        .expect("a Dragged event")
}

async fn arrival(w: &TestService, key: &str) -> Option<String> {
    w.board.arrival(key).await.unwrap().map(|a| a.label())
}

/// Back to Dev from QA (s03-50): asked first; then Dev's conversation resumes with the instructions (Send back's
/// skill and the note), every agent check's counter restarts and QA's run ends as a Fail.
#[tokio::test(flavor = "multi_thread")]
async fn back_to_dev_resumes_its_session_with_the_instructions() {
    let w = world().await;
    w.board.use_flow_for_tests("SHOP", &every_kind()).await;
    let t = ran_dev_then(&w, "Price table", "qa", Status::WaitingForYou).await;
    counted(&w, &t).await;

    let unconfirmed = w
        .review
        .drag(
            "SHOP-1",
            &DropOn::stage("dev"),
            DragAnswers::default(),
            &w.validation,
            &me(),
        )
        .await
        .unwrap_err();
    assert_eq!(unconfirmed.refusal(), Some(Refusal::NeedsConfirmation));
    assert_eq!(
        w.status("SHOP-1").await,
        Status::WaitingForYou,
        "Cancel leaves it"
    );

    let note = "Keep the old price column until the migration has run.";
    assert!(matches!(
        drag(&w, "SHOP-1", DropOn::stage("dev"), note)
            .await
            .unwrap(),
        ReturnOutcome::Done { .. }
    ));
    until_status(&w, "SHOP-1", Status::Working).await;
    let back = w.board.ticket("SHOP-1").await.unwrap();
    assert_eq!(back.place, Place::stage_of("dev"));
    assert!(w.board.check_counts(&t.id).await.unwrap().0.is_empty());
    let session = w.sessions.running_of(&t.id, SessionKind::Agent).unwrap();
    let out = w.wait_for_output(&session.id, "ARGS").await;
    assert!(out.contains("--resume Price table-dev"), "{out}");
    // Only instructions, no line comments: the plain sentence, no `address-review` (d0131 Q514).
    assert!(
        out.contains(
            "--resume Price table-dev SHOP-1 was sent back to Dev by you with a comment. Run `otaq prime`."
        ),
        "{out}"
    );
    assert!(!out.contains("address-review"), "{out}");
    assert!(
        !out.contains(note),
        "the instructions are a comment, not typed: {out}"
    );
    assert_eq!(
        crate::common::last_comment(&w.board, "SHOP-1")
            .await
            .map(|c| c.body)
            .as_deref(),
        Some(note)
    );
    let runs = w.board.stage_runs("SHOP-1").await.unwrap();
    assert_eq!(
        runs.iter().rfind(|r| r.stage == "qa").unwrap().result,
        Some(RunResult::Failed)
    );
    let event = dragged(&w, "SHOP-1").await;
    assert_eq!(
        (
            &event.payload["stage"],
            &event.payload["back"],
            &event.payload["note"]
        ),
        (&"Dev".into(), &true.into(), &note.into())
    );
    w.stop().await;
}

/// Back to Dev while Security's checker runs (s03-50, "Stop and send to Dev"): the checker stops, its round ends
/// stopped, and Dev resumes with the instructions.
#[tokio::test(flavor = "multi_thread")]
async fn back_from_a_running_check_stops_the_checker() {
    let w = world().await;
    let mut file = every_kind();
    file.flow.stages.retain(|s| s.id != "validate");
    w.board.use_flow_for_tests("SHOP", &file).await;
    let t = ran_dev_then(&w, "Price table", "dev", Status::Working).await;
    w.validation
        .submit(
            "SHOP-1",
            said(&w.board, "SHOP-1", "Done").await.as_ref(),
            &Actor::Agent(t.key.clone()),
            ValidationTrigger::Agent,
        )
        .await
        .unwrap();
    assert_eq!(
        w.board.ticket("SHOP-1").await.unwrap().place,
        Place::stage_of("security")
    );
    assert!(
        w.sessions
            .running_of(&t.id, SessionKind::Validator)
            .is_some()
    );

    drag(&w, "SHOP-1", DropOn::stage("dev"), "Escape the SQL")
        .await
        .unwrap();
    until_status(&w, "SHOP-1", Status::Working).await;
    assert!(
        w.sessions
            .running_of(&t.id, SessionKind::Validator)
            .is_none(),
        "the checker stopped"
    );
    let outcome: Option<String> = sqlx::query_scalar(
        "SELECT outcome FROM validations WHERE ticket_id = ? ORDER BY started_at DESC",
    )
    .bind(&t.id)
    .fetch_one(w.board.store().pool())
    .await
    .unwrap();
    assert_eq!(outcome.as_deref(), Some("stopped"));
    assert_eq!(
        w.board.ticket("SHOP-1").await.unwrap().place,
        Place::stage_of("dev")
    );
    w.stop().await;
}

/// Back to Security from QA (s03-50, "Run Security again on SHOP-18?"): a fresh round with the note, the counters
/// reset.
#[tokio::test(flavor = "multi_thread")]
async fn back_to_a_check_runs_it_again_with_the_note() {
    let w = world().await;
    w.board.use_flow_for_tests("SHOP", &every_kind()).await;
    let t = ran_dev_then(&w, "Coupons", "qa", Status::WaitingForYou).await;
    counted(&w, &t).await;

    let note = "Look again at the new coupon endpoint.";
    drag(&w, "SHOP-1", DropOn::stage("security"), note)
        .await
        .unwrap();
    let after = w.board.ticket("SHOP-1").await.unwrap();
    assert_eq!(after.place, Place::stage_of("security"));
    assert!(
        matches!(
            after.status,
            Status::Checking | Status::WaitingForLimitReset
        ),
        "{}",
        after.status
    );
    let counts = w.board.check_counts(&t.id).await.unwrap();
    assert!(counts.0.iter().all(|c| c.fails == 0), "{counts:?}");
    let runs = w.board.stage_runs("SHOP-1").await.unwrap();
    // The note is the user's comment, which the QA run ended with (CMT-05).
    let qa = runs.iter().rfind(|r| r.stage == "qa").unwrap();
    assert_eq!(
        (qa.result, qa.summary.as_deref()),
        (Some(RunResult::Cancelled), Some(note))
    );
    let comment = crate::common::last_comment(&w.board, "SHOP-1")
        .await
        .unwrap();
    assert_eq!(comment.author_label(), "You");
    let marked: Option<i64> = sqlx::query_scalar(
        "SELECT handed_over_at FROM stage_runs WHERE ticket_id = ? AND stage_id = 'security' AND ended_at IS NULL",
    )
    .bind(&t.id)
    .fetch_one(w.board.store().pool())
    .await
    .unwrap();
    assert!(marked.is_some(), "the check runs again with a new comment");
    let security = runs.iter().rfind(|r| r.stage == "security").unwrap();
    assert_eq!((security.round, security.result), (Some(1), None));
    assert_eq!(dragged(&w, "SHOP-1").await.payload["stage"], "Security");
    w.stop().await;
}

/// Back onto a human check (s03-54): no dialog; the card waits there, "Moved back by you".
#[tokio::test(flavor = "multi_thread")]
async fn back_onto_a_human_check_needs_no_dialog() {
    let w = world().await;
    w.board.use_flow_for_tests("SHOP", &with_review()).await;
    let t = ran_dev_then(&w, "Price table", "review", Status::WaitingForYou).await;
    counted(&w, &t).await;

    let check = w.board.check_drag(&t, &DropOn::stage("qa")).await.unwrap();
    assert_eq!(
        check.action(),
        Some(otaq_core::board::moves::MoveAction::MoveBack)
    );
    assert!(!check.is_refused());
    let answers = DragAnswers::default();
    w.review
        .drag(
            "SHOP-1",
            &DropOn::stage("qa"),
            answers,
            &w.validation,
            &me(),
        )
        .await
        .unwrap();
    let after = w.board.ticket("SHOP-1").await.unwrap();
    assert_eq!(
        (after.place.clone(), after.status),
        (Place::stage_of("qa"), Status::WaitingForYou)
    );
    assert_eq!(
        arrival(&w, "SHOP-1").await.as_deref(),
        Some("Moved back by you")
    );
    assert!(w.board.check_counts(&t.id).await.unwrap().0.is_empty());
    w.stop().await;
}

/// Forward from Dev past Validate and Security (s03-54, "Drag forward"): both get skipped runs and are marked, the
/// card waits in QA ("Skipped Security · by you"), and its Delivery will wait for a click.
#[tokio::test(flavor = "multi_thread")]
async fn forward_past_checks_marks_them_and_delivers_on_a_click() {
    let w = world().await;
    w.board.use_flow_for_tests("SHOP", &every_kind()).await;
    ran_dev_then(&w, "Split user full name", "dev", Status::Idle).await;
    drag(&w, "SHOP-1", DropTarget::ReadyForReview, "")
        .await
        .unwrap();
    let after = w.board.ticket("SHOP-1").await.unwrap();
    assert_eq!(
        (after.place.clone(), after.status),
        (Place::stage_of("qa"), Status::WaitingForYou)
    );
    assert_eq!(
        arrival(&w, "SHOP-1").await.as_deref(),
        Some("Skipped Security \u{b7} by you")
    );
    let runs = w.board.stage_runs("SHOP-1").await.unwrap();
    for stage in ["validate", "security"] {
        assert_eq!(
            runs.iter().rfind(|r| r.stage == stage).unwrap().result,
            Some(RunResult::Skipped),
            "{stage}"
        );
    }
    assert_eq!(
        w.board
            .forced_click_reason(&after)
            .await
            .unwrap()
            .as_deref(),
        Some("Skipped Validate \u{b7} by you \u{b7} Skipped Security \u{b7} by you")
    );
    let event = dragged(&w, "SHOP-1").await;
    assert_eq!(
        event.payload["skipped"],
        serde_json::json!(["Validate", "Security"])
    );

    // Straight to the automatic Delivery, past QA too: it waits for the click.
    ran_dev_then(&w, "Wishlist sort", "dev", Status::Idle).await;
    drag(&w, "SHOP-2", DropOn::stage("ship"), "").await.unwrap();
    tokio::time::sleep(Duration::from_millis(300)).await;
    let delivered = w.board.ticket("SHOP-2").await.unwrap();
    assert_eq!(
        (delivered.place.clone(), delivered.status),
        (Place::stage_of("ship"), Status::ToDeliver),
        "an automatic Delivery waits for the click after a forward drag"
    );
    assert!(
        w.board
            .forced_click_reason(&delivered)
            .await
            .unwrap()
            .is_some_and(|r| r.contains("Skipped QA")),
    );
    w.stop().await;
}

/// The very next stage is Move to next stage: from Dev onto Validate, a checker starts.
#[tokio::test(flavor = "multi_thread")]
async fn the_next_stage_is_move_to_next_stage() {
    let w = world().await;
    ran_dev_then(&w, "Search notes", "dev", Status::Idle).await;
    let working = w.create("SHOP", "Busy").await;
    place_in(&w, &working, "dev", Status::Working).await;
    let refused = drag(&w, "SHOP-2", DropOn::stage("validate"), "")
        .await
        .unwrap_err();
    assert_eq!(
        refused.to_string(),
        "SHOP-2 can't move to the next stage now: its agent is in the middle of a turn"
    );

    drag(&w, "SHOP-1", DropOn::stage("validate"), "")
        .await
        .unwrap();
    let after = w.board.ticket("SHOP-1").await.unwrap();
    assert_eq!(after.place, Place::stage_of("validate"));
    assert!(
        matches!(
            after.status,
            Status::Checking | Status::WaitingForLimitReset
        ),
        "{}",
        after.status
    );
    assert!(
        w.board.forced_click_reason(&after).await.unwrap().is_none(),
        "nothing skipped"
    );
    w.stop().await;
}

/// A drag back into a stage the ticket skips by its choice runs it once (second gap review rule 2): a fresh session
/// in Product spec with its skill and the instructions; History says "Product spec · run by you".
#[tokio::test(flavor = "multi_thread")]
async fn back_into_a_skipped_stage_runs_it_once() {
    let w = world().await;
    w.board.use_flow_for_tests("SHOP", &every_kind()).await;
    let t = ran_dev_then(&w, "Saved carts", "qa", Status::WaitingForYou).await;
    sqlx::query(
        "INSERT INTO ticket_stage_choices (ticket_id, stage_id, choice, set_by, set_at)
         VALUES (?, 'product-spec', 'skip', 'user', 1)",
    )
    .bind(&t.id)
    .execute(w.board.store().pool())
    .await
    .unwrap();

    let note = "Ask about carts shared between devices.";
    drag(&w, "SHOP-1", DropOn::stage("product-spec"), note)
        .await
        .unwrap();
    until_status(&w, "SHOP-1", Status::Working).await;
    let after = w.board.ticket("SHOP-1").await.unwrap();
    assert_eq!(after.place, Place::stage_of("product-spec"));
    let session = w.sessions.running_of(&t.id, SessionKind::Agent).unwrap();
    let out = w.wait_for_output(&session.id, "ARGS").await;
    assert!(!out.contains("--resume"), "a fresh session: {out}");
    // The stage's skill, then what came back (CMT-05): the instructions are the user's comment, not typed.
    assert!(
        out.contains(
            "/otaq-product-spec SHOP-1 was sent back to Product spec by you with a comment. Run `otaq prime`."
        ),
        "{out}"
    );
    assert!(!out.contains(note), "{out}");
    assert_eq!(
        crate::common::last_comment(&w.board, "SHOP-1")
            .await
            .map(|c| c.body)
            .as_deref(),
        Some(note)
    );
    let event = dragged(&w, "SHOP-1").await;
    assert_eq!(event.payload["run_by_you"], true);
    w.stop().await;
}

/// A parked card dropped anywhere goes back to the stage it left (s03-54), through the usual waiting.
#[tokio::test(flavor = "multi_thread")]
async fn a_parked_card_returns_to_the_stage_it_left() {
    let w = world().await;
    w.board.use_flow_for_tests("SHOP", &every_kind()).await;
    let t = ran_dev_then(&w, "Cache product prices", "dev", Status::Working).await;
    sqlx::query(
        "UPDATE tickets SET stage_id = NULL, state = 'parked', parked = TRUE, branch_name = 'otaq/SHOP-1'
         WHERE id = ?",
    )
    .bind(&t.id)
    .execute(w.board.store().pool())
    .await
    .unwrap();
    let parked = w.board.ticket("SHOP-1").await.unwrap();
    assert!(parked.place.is_backlog());
    w.board
        .move_ticket("SHOP-1", DropOn::stage("security"), false, &me())
        .await
        .unwrap();
    let after = w.board.ticket("SHOP-1").await.unwrap();
    assert_eq!(after.place, Place::Waiting(Some("dev".into())));
    assert!(after.status.is_queued(), "{}", after.status);
    w.stop().await;
}
