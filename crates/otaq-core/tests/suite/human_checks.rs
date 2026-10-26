//! Human checks (T-0418; FLW-14, FLW-15, STG-06; d0108 rule 6, "After a Fail", smaller rule 13): a Pass on any human
//! check moves the card on, or, directly before a click Delivery, offers the delivery choices; a Fail resumes the
//! nearest earlier Agent stage's session with the comments, every agent check's counter restarts and the card walks
//! every later stage again; a card in a human check says how it got there; needs-you group 3 holds human checks and
//! click-mode Deliveries only, closest to Done first by the stage's position in the flow. On the test flow
//! (`tests/flows/every-kind.yaml`) and the default flow; `claude` is stood in for by ordinary commands.

use otaq_core::service::testing::said;
use std::path::Path;
use std::time::Duration;

use otaq_core::agent_tools::LaunchContext;
use otaq_core::agent_tools::claude::ClaudeLaunch;
use otaq_core::board::human_check::Arrival;
use otaq_core::board::needs_you::NeedsYouGroup;
use otaq_core::board::tickets::{EventKind, Ticket};
use otaq_core::domain::{Actor, Place, Status, ValidationTrigger};
use otaq_core::flow::engine::RunResult;
use otaq_core::flow::{
    CheckBy, Column, Delivers, Delivery, DeliveryMode, FlowFile, Stage, StageKind, yaml,
};
use otaq_core::git::testing::commit;
use otaq_core::review::{PassOutcome, ReturnOutcome};
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

fn human_check(id: &str, name: &str) -> Stage {
    Stage {
        id: id.into(),
        name: name.into(),
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
    }
}

/// The test flow with a Review human check inserted before the stage `before`, and its Delivery in click mode
/// (`delivers: pr`).
fn with_review_before(before: &str) -> FlowFile {
    let mut file = every_kind();
    let at = file.flow.position(before).unwrap();
    file.flow.stages.insert(at, human_check("review", "Review"));
    for stage in file.flow.stages.iter_mut().filter(|s| s.is_delivery()) {
        stage.delivery = Some(Delivery {
            mode: DeliveryMode::Click,
            delivers: Delivers::Pr,
        });
    }
    file
}

/// Puts the ticket in `stage` with `status` in one write, so no other stage gets a run on the way.
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

/// A ticket with a worktree holding one commit, waiting in `stage` with `status`.
async fn at(w: &TestService, title: &str, stage: &str, status: Status) -> Ticket {
    let t = w.create("SHOP", title).await;
    let wt = w.start_worktree(&t).await;
    commit(&wt, "notes.txt", &format!("{title}\n"));
    place_in(w, &t, stage, status).await;
    w.board.ticket(&t.key.to_string()).await.unwrap()
}

/// A finished run of `stage` recorded `ago` ms before now.
async fn ran(w: &TestService, t: &Ticket, stage: &str, result: RunResult, round: Option<u32>) {
    let now = otaq_core::store::now_ms();
    sqlx::query(
        "INSERT INTO stage_runs (id, ticket_id, stage_id, kind, round, result, started_at, ended_at)
         VALUES (?, ?, ?, 'check', ?, ?, ?, ?)",
    )
    .bind(otaq_core::store::new_id())
    .bind(&t.id)
    .bind(stage)
    .bind(round.map(i64::from))
    .bind(result.as_str())
    .bind(now - 60_000)
    .bind(now - 50_000)
    .execute(w.board.store().pool())
    .await
    .unwrap();
}

async fn ended_session(w: &TestService, ticket: &Ticket, conversation: &str) {
    let project = w.board.project("SHOP").await.unwrap();
    let at = otaq_core::store::now_ms() - 5_000;
    sqlx::query(
        "INSERT INTO sessions (id, project_id, ticket_id, kind, tool, tool_session_id, state, started_at, ended_at)
         VALUES (?, ?, ?, 'agent', 'claude', ?, 'ended', ?, ?)",
    )
    .bind(otaq_core::store::new_id())
    .bind(&project.id)
    .bind(&ticket.id)
    .bind(conversation)
    .bind(at)
    .bind(at + 10)
    .execute(w.board.store().pool())
    .await
    .unwrap();
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

fn moved(outcome: PassOutcome) -> Ticket {
    match outcome {
        PassOutcome::Moved { ticket } => *ticket,
        other => panic!("expected a move, got {other:?}"),
    }
}

async fn arrival(w: &TestService, key: &str) -> Option<String> {
    w.board
        .arrival(key)
        .await
        .unwrap()
        .map(|a: Arrival| a.label())
}

/// QA in the middle of the flow: its Pass moves the card on to Review (no click Delivery right after QA), the run
/// ends Passed with a History line, and the card says how it got there; Review's Pass, right before the click
/// Delivery, offers the PR and changes nothing.
#[tokio::test(flavor = "multi_thread")]
async fn a_mid_flow_pass_moves_on_and_the_last_offers_the_delivery() {
    let w = world().await;
    w.board
        .use_flow_for_tests("SHOP", &with_review_before("ship"))
        .await;
    let t = at(&w, "Export as CSV", "qa", Status::WaitingForYou).await;
    ran(&w, &t, "security", RunResult::Passed, Some(2)).await;
    assert_eq!(
        arrival(&w, "SHOP-1").await.as_deref(),
        Some("Passed Security \u{b7} round 2")
    );

    let after = moved(w.review.pass("SHOP-1", &w.validation, &me()).await.unwrap());
    assert_eq!(after.place, Place::stage_of("review"));
    assert_eq!(after.status, Status::WaitingForYou);
    assert_eq!(arrival(&w, "SHOP-1").await.as_deref(), Some("Passed QA"));
    let runs = w.board.stage_runs("SHOP-1").await.unwrap();
    let qa = runs.iter().rfind(|r| r.stage == "qa").unwrap();
    assert_eq!(qa.result, Some(RunResult::Passed));
    let history = w.board.history("SHOP-1").await.unwrap();
    let passed = history
        .iter()
        .find(|e| e.kind == EventKind::Passed)
        .unwrap();
    assert_eq!(passed.payload["stage"], "QA");

    let choose = w.review.pass("SHOP-1", &w.validation, &me()).await.unwrap();
    assert_eq!(
        choose,
        PassOutcome::Choose {
            delivers: Delivers::Pr
        }
    );
    let still = w.board.ticket("SHOP-1").await.unwrap();
    assert_eq!(
        (still.place, still.status),
        (Place::stage_of("review"), Status::WaitingForYou)
    );
    w.stop().await;
}

/// The board's list carries how each card came to wait in a human check or at its Delivery (T-0424, FLW-14, FLW-15),
/// as `Board::arrival` says it; a skip the ticket's own choice made names the agent that chose it (Q305).
#[tokio::test(flavor = "multi_thread")]
async fn the_list_says_how_each_card_got_into_its_check() {
    let w = world().await;
    w.board.use_flow_for_tests("SHOP", &every_kind()).await;
    let passed = at(&w, "Export as CSV", "qa", Status::WaitingForYou).await;
    ran(&w, &passed, "security", RunResult::Passed, Some(2)).await;
    let skipped = at(&w, "Fix README typo", "qa", Status::WaitingForYou).await;
    ran(&w, &skipped, "product-spec", RunResult::Skipped, None).await;
    sqlx::query(
        "INSERT INTO ticket_stage_choices (ticket_id, stage_id, choice, set_by, by_name, reason, set_at)
         VALUES (?, 'product-spec', 'skip', 'project_agent', 'Checkout slowness', 'a typo', 0)",
    )
    .bind(&skipped.id)
    .execute(w.board.store().pool())
    .await
    .unwrap();
    let delivering = at(&w, "Gift cards", "ship", Status::ToDeliver).await;
    ran(&w, &delivering, "qa", RunResult::Passed, None).await;
    let working = at(&w, "Saved carts", "dev", Status::Working).await;
    ran(&w, &working, "product-spec", RunResult::Passed, None).await;

    let tickets = w.board.tickets("SHOP").await.unwrap();
    let label = |key: &str| {
        tickets
            .iter()
            .find(|t| t.key.to_string() == key)
            .and_then(|t| t.flags.arrival.as_ref())
            .map(Arrival::label)
    };
    assert_eq!(
        label("SHOP-1").as_deref(),
        Some("Passed Security \u{b7} round 2")
    );
    assert_eq!(
        label("SHOP-2").as_deref(),
        Some("Skipped Product spec \u{b7} by Checkout slowness")
    );
    assert_eq!(
        arrival(&w, "SHOP-2").await.as_deref(),
        Some("Skipped Product spec \u{b7} by Checkout slowness")
    );
    assert_eq!(label("SHOP-3").as_deref(), Some("Passed QA"));
    assert_eq!(
        label("SHOP-4"),
        None,
        "only cards waiting in a check or at the Delivery"
    );
    w.stop().await;
}

/// A Pass works only on a ticket waiting in a human check.
#[tokio::test(flavor = "multi_thread")]
async fn pass_is_refused_outside_a_human_check() {
    let w = world().await;
    w.board.use_flow_for_tests("SHOP", &every_kind()).await;
    at(&w, "Export as CSV", "dev", Status::Working).await;
    let err = w
        .review
        .pass("SHOP-1", &w.validation, &me())
        .await
        .unwrap_err();
    assert_eq!(
        err.to_string(),
        "SHOP-1 is Working; Pass works on a ticket waiting in a human check"
    );
    w.stop().await;
}

/// A Pass into an agent check opens its round and starts its checker, as a submit does.
#[tokio::test(flavor = "multi_thread")]
async fn a_pass_into_an_agent_check_starts_its_checker() {
    let w = world().await;
    let mut file = every_kind();
    // Dev → Validate → QA → Security → Done.
    let qa = file.flow.stages.remove(file.flow.position("qa").unwrap());
    let security = file.flow.position("security").unwrap();
    file.flow.stages.insert(security, qa);
    w.board.use_flow_for_tests("SHOP", &file).await;
    let t = at(&w, "Export as CSV", "qa", Status::WaitingForYou).await;

    let after = moved(w.review.pass("SHOP-1", &w.validation, &me()).await.unwrap());
    assert_eq!(after.place, Place::stage_of("security"));
    assert!(
        w.sessions
            .running_of(&t.id, SessionKind::Validator)
            .is_some(),
        "the checker runs ({})",
        after.status
    );
    let runs = w.board.stage_runs("SHOP-1").await.unwrap();
    assert_eq!(
        runs.iter().rfind(|r| r.stage == "qa").unwrap().result,
        Some(RunResult::Passed)
    );
    w.stop().await;
}

/// Fail at QA of a ticket that skips Product spec (QA's fails go to Product spec): back to the nearest Agent stage it
/// runs, Dev, whose conversation resumes with the address-review skill (a line comment goes with it) and the comments;
/// the check counter restarts, and the card walks Validate and Security again before QA.
#[tokio::test(flavor = "multi_thread")]
async fn a_fail_at_qa_resumes_dev_and_walks_the_checks_again() {
    let w = world().await;
    w.board.use_flow_for_tests("SHOP", &every_kind()).await;
    let t = at(&w, "Export as CSV", "dev", Status::Working).await;
    sqlx::query(
        "INSERT INTO ticket_stage_choices (ticket_id, stage_id, choice, set_by, set_at)
         VALUES (?, 'product-spec', 'skip', 'user', 1)",
    )
    .bind(&t.id)
    .execute(w.board.store().pool())
    .await
    .unwrap();
    ended_session(&w, &t, "dev-conversation").await;
    place_in(&w, &t, "qa", Status::WaitingForYou).await;
    crate::common::line_comment(&w.board, "SHOP-1").await;
    // Both checks counted fails (each its own, T-0417).
    sqlx::query(
        "INSERT INTO check_counters (ticket_id, stage_id, fails, updated_at)
         VALUES (?1, 'validate', 2, 1), (?1, 'security', 1, 2)",
    )
    .bind(&t.id)
    .execute(w.board.store().pool())
    .await
    .unwrap();

    let note = "The CSV needs a header row.";
    match w
        .review
        .fail("SHOP-1", note, None, false, &me())
        .await
        .unwrap()
    {
        ReturnOutcome::Done { .. } => {}
        other => panic!("{other:?}"),
    }
    until_status(&w, "SHOP-1", Status::Working).await;
    let back = w.board.ticket("SHOP-1").await.unwrap();
    assert_eq!(back.place, Place::stage_of("dev"));
    assert_eq!(back.validation_round, 0, "a human Fail resets the counters");
    assert!(
        w.board.check_counts(&t.id).await.unwrap().0.is_empty(),
        "both checks' counters restart"
    );
    let session = w.sessions.running_of(&t.id, SessionKind::Agent).unwrap();
    let out = w.wait_for_output(&session.id, "ARGS").await;
    assert!(out.contains("--resume dev-conversation"), "{out}");
    assert!(out.contains("address-review"), "{out}");
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

    // The agent's next stage: Validate runs again, and its pass takes the card into Security again. (A pass from one
    // agent check into the next opens no round yet: per-check rounds are T-0417's, so the walk is checked up to
    // Security here and through QA in the engine's tests.)
    let agent = Actor::Agent(t.key.clone());
    w.validation
        .submit(
            "SHOP-1",
            said(&w.board, "SHOP-1", "Added the header").await.as_ref(),
            &agent,
            ValidationTrigger::Agent,
        )
        .await
        .unwrap();
    assert_eq!(
        w.board.ticket("SHOP-1").await.unwrap().place,
        Place::stage_of("validate")
    );
    let checker = w
        .sessions
        .running_of(&t.id, SessionKind::Validator)
        .expect("Validate's checker runs")
        .id;
    let after = w
        .validation
        .verdict(
            "SHOP-1",
            true,
            said(&w.board, "SHOP-1", "ok").await.as_ref(),
            &checker,
        )
        .await
        .unwrap();
    assert_eq!(
        (after.place, after.status),
        (Place::stage_of("security"), Status::Checking)
    );
    w.sessions.stop(&session.id).await.unwrap();
    w.stop().await;
}

/// The default flow: Review's Pass offers Local merge and Open PR, as Approve does today; nothing moves.
#[tokio::test(flavor = "multi_thread")]
async fn on_the_default_flow_reviews_pass_offers_the_delivery_choices() {
    let w = world().await;
    let t = w.create("SHOP", "Export as CSV").await;
    w.force_status(&t, Status::WaitingForYou).await;
    assert_eq!(
        w.review.pass("SHOP-1", &w.validation, &me()).await.unwrap(),
        PassOutcome::Choose {
            delivers: Delivers::Either
        }
    );
    assert_eq!(w.status("SHOP-1").await, Status::WaitingForYou);

    w.stop().await;
}

/// Needs-you group 3 (FLW-15, Q265): a card that a Pass moved into an automatic Delivery delivers by itself (its
/// delivery session opens the PR) and isn't in group 3; in QA it was, naming its stage. A card whose Security check
/// failed and was moved on reaches the same Delivery in click mode and waits in group 3 (T-0419).
#[tokio::test(flavor = "multi_thread")]
async fn an_automatic_deliverys_card_is_not_in_group_three() {
    let w = world().await;
    w.board.use_flow_for_tests("SHOP", &every_kind()).await;
    // GitHub's built-in Open PR skill: the automatic PR starts (nothing is pushed or called before the proposal).
    let repo = w.board.project("SHOP").await.unwrap().repo_path;
    otaq_core::git::testing::git(
        Path::new(&repo),
        &[
            "remote",
            "add",
            "origin",
            "git@github.com:acme/shop-web.git",
        ],
    );
    let t = at(&w, "Export as CSV", "qa", Status::WaitingForYou).await;
    ran(&w, &t, "validate", RunResult::Passed, Some(1)).await;
    ran(&w, &t, "security", RunResult::Passed, Some(1)).await;
    let items = w.board.needs_you_items(i64::MAX / 2).await.unwrap();
    let item = items.iter().find(|i| i.id == t.id).unwrap();
    assert_eq!(item.group, NeedsYouGroup::Review);
    assert_eq!(item.waiting_in.as_ref().unwrap().name, "QA");
    assert_eq!(item.reason, "ready for QA");

    let after = moved(w.review.pass("SHOP-1", &w.validation, &me()).await.unwrap());
    assert_eq!(
        (after.place, after.status),
        (Place::stage_of("ship"), Status::ToDeliver)
    );
    let after = w.board.ticket("SHOP-1").await.unwrap();
    assert!(after.flags.delivering_automatically);
    assert!(
        w.sessions
            .running_of(&t.id, SessionKind::Delivery)
            .is_some(),
        "Shipper's delivery session"
    );
    let items = w.board.needs_you_items(i64::MAX / 2).await.unwrap();
    assert!(items.iter().all(|i| i.id != t.id), "{items:?}");

    let moved_on = at(&w, "Import CSV", "qa", Status::WaitingForYou).await;
    ran(&w, &moved_on, "validate", RunResult::Passed, Some(1)).await;
    ran(&w, &moved_on, "security", RunResult::Failed, Some(2)).await;
    let key = moved_on.key.to_string();
    let after = moved(w.review.pass(&key, &w.validation, &me()).await.unwrap());
    assert_eq!(after.status, Status::ToDeliver);
    assert!(
        !w.board
            .ticket(&key)
            .await
            .unwrap()
            .flags
            .delivering_automatically
    );
    let items = w.board.needs_you_items(i64::MAX / 2).await.unwrap();
    let item = items.iter().find(|i| i.id == moved_on.id).unwrap();
    assert_eq!(item.group, NeedsYouGroup::Review, "click mode: the user's");
    w.stop().await;
}

/// Needs-you group 3's order (Q264): closest to Done by the stage's position in the flow. With QA after Review, a QA
/// row comes before an older Review row, and a card at the click Delivery before both.
#[tokio::test(flavor = "multi_thread")]
async fn qa_rows_sort_before_review_rows_when_qa_is_later() {
    let w = world().await;
    w.board
        .use_flow_for_tests("SHOP", &with_review_before("qa"))
        .await;
    let review = at(&w, "Search notes", "review", Status::WaitingForYou).await;
    let qa = at(&w, "Export as CSV", "qa", Status::WaitingForYou).await;
    let deliver = at(&w, "Sort by date", "ship", Status::ToDeliver).await;
    sqlx::query("UPDATE tickets SET state_since = state_since - 60000 WHERE id = ?")
        .bind(&review.id)
        .execute(w.board.store().pool())
        .await
        .unwrap();
    let items = w.board.needs_you_items(i64::MAX / 2).await.unwrap();
    let group: Vec<_> = items
        .iter()
        .filter(|i| i.group == NeedsYouGroup::Review)
        .map(|i| {
            (
                i.key.to_string(),
                i.waiting_in.as_ref().unwrap().name.clone(),
            )
        })
        .collect();
    assert_eq!(
        group,
        [
            (deliver.key.to_string(), "Done".to_owned()),
            (qa.key.to_string(), "QA".to_owned()),
            (review.key.to_string(), "Review".to_owned()),
        ]
    );
    w.stop().await;
}
