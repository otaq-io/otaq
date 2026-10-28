//! A Fail goes to the check's stage, and a re-walked Agent stage resumes its session (T-0526; d0131 and its row 8,
//! d0108 sessions rules 2 and 4, d0118, d0122). On the test flow (`tests/flows/every-kind.yaml`: Product spec · Dev ·
//! Validate · Security · QA, whose fails go to Product spec · Done):
//!
//! - QA's Send back stops Dev's live session and resumes Product spec's conversation, on the tool it ran on, with the
//!   plain sentence ("QA sent SHOP-1 back to you with a comment. Run `otaq prime`."), never `address-review`; with line
//!   comments it is a code review and `address-review` goes first.
//! - Product spec finishing walks the card into Dev again: Dev's own conversation resumes, told why ("SHOP-1 is back in
//!   Dev: QA sent it to Product spec, which finished. Run `otaq prime`."); a stage's first run is a fresh session.
//! - Validate's fails set to Product spec: its Fail stops Dev's live session and resumes Product spec's conversation
//!   with "Validate failed round 1. Run `otaq prime`."; History's event names the check and the stage.
//! - A Send back into the live Dev session (QA's fails set to Dev) types the plain sentence with only a comment, and
//!   `/otaq-address-review` with a line comment.
//!
//! Every tool is the stand-in of `tests/fixtures/stand-in` (`crate::common::stand_in`): it prints its arguments at
//! start, runs a skill it loaded and ignores other text. Repos, `OTAQ_HOME` and the tools' home are temporary.
//!
//! Unix only: the stand-in agent tools are `sh` scripts started by name, which Windows can't run (Q598).
#![cfg(unix)]

use std::path::Path;
use std::time::Duration;

use crate::common::stand_in;
use otaq_core::board::comments::{NewComment, Side};
use otaq_core::board::moves::DropTarget;
use otaq_core::board::tickets::{EventKind, Ticket};
use otaq_core::domain::{Actor, Place, Status};
use otaq_core::flow::{FlowFile, yaml};
use otaq_core::service::ServiceConfig;
use otaq_core::service::testing::TestService;
use otaq_core::session::{SessionInfo, SessionKind, Start};
use otaq_core::start::StartOutcome;
use tempfile::TempDir;

fn me() -> Actor {
    Actor::User("me".into())
}

fn every_kind() -> FlowFile {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/flows/every-kind.yaml");
    yaml::read(&std::fs::read_to_string(path).unwrap()).unwrap()
}

/// A service whose four tools are the stand-in, with shop-web (a local origin) on `flow`.
async fn world(dir: &TempDir, flow: &FlowFile) -> TestService {
    let launch = stand_in::launch(&dir.path().join("bin"), dir.path());
    let w = TestService::start_with(move |config| ServiceConfig {
        launch: Some(launch),
        ..config
    })
    .await;
    let repo = w.repos.clone_with_origin("shop-web", "main", 1);
    w.board
        .add_project(&otaq_core::board::projects::AddProject {
            path: repo.display().to_string(),
            ..Default::default()
        })
        .await
        .unwrap();
    w.board.use_flow_for_tests("SHOP", flow).await;
    w
}

/// The ticket's live agent session other than `not`, once the stand-in printed its arguments.
async fn running(w: &TestService, t: &Ticket, not: Option<&str>) -> SessionInfo {
    let session = tokio::time::timeout(Duration::from_secs(20), async {
        loop {
            if let Some(s) = w.sessions.running_of(&t.id, SessionKind::Agent)
                && Some(s.id.as_str()) != not
            {
                return s;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("{}'s agent never ran", t.key));
    w.wait_for_output(&session.id, "ARGS").await;
    session
}

/// The `ARGS …` line the stand-in printed at start.
async fn args(w: &TestService, session: &SessionInfo) -> String {
    let out = w.wait_for_output(&session.id, "ARGS").await;
    out.lines()
        .find(|l| l.starts_with("ARGS"))
        .unwrap()
        .to_owned()
}

/// Starts the ticket where it waits (the scheduler may get there first) and returns its new session.
async fn start(w: &TestService, t: &Ticket, before: Option<&str>) -> SessionInfo {
    let outcome = w
        .starter
        .start(&t.key.to_string(), &me(), Start::Auto)
        .await
        .unwrap();
    assert!(
        matches!(outcome, StartOutcome::Started(_) | StartOutcome::Skipped),
        "{outcome:?}"
    );
    running(w, t, before).await
}

/// The ticket's agent finished its stage (the user's Move to next stage between two turns: no agent check comes
/// next).
async fn finished(w: &TestService, t: &Ticket) {
    let now = w.board.ticket(&t.key.to_string()).await.unwrap();
    place_in(w, t, now.place.stage().unwrap(), Status::Idle).await;
    w.validation
        .submit_by_user(&t.key.to_string(), "", &me())
        .await
        .unwrap();
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

async fn until_in(w: &TestService, key: &str, stage: &str, status: Status) {
    let reached = tokio::time::timeout(Duration::from_secs(20), async {
        loop {
            let t = w.board.ticket(key).await.unwrap();
            if t.place == Place::stage_of(stage) && t.status == status {
                return;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await;
    assert!(reached.is_ok(), "{key} never was {status} in {stage}");
}

async fn line_comment(w: &TestService, key: &str) {
    w.board
        .add_comment(&NewComment {
            key: key.to_owned(),
            file: "notes.txt".into(),
            line: 1,
            side: Side::New,
            body: "Say which prices are old.".into(),
        })
        .await
        .unwrap();
}

/// The newest Sent back row of History.
async fn sent_back_row(w: &TestService, key: &str) -> serde_json::Value {
    w.board
        .history(key)
        .await
        .unwrap()
        .into_iter()
        .rfind(|e| e.kind == EventKind::SentBack)
        .expect("a Sent back event")
        .payload
}

/// QA's fails go to Product spec: its Send back stops Dev and resumes Product spec's conversation with the plain
/// sentence; Product spec finishing walks the card into Dev again, which resumes Dev's own conversation, told why. Each
/// stage's first run was fresh.
#[tokio::test(flavor = "multi_thread")]
async fn a_send_back_resumes_the_checks_stage_and_the_rewalk_resumes_dev() {
    let dir = TempDir::new().unwrap();
    let w = world(&dir, &every_kind()).await;
    let t = w.create("SHOP", "Price table").await;
    let key = t.key.to_string();
    w.board
        .move_ticket(&key, DropTarget::ReadyForDev, true, &me())
        .await
        .unwrap();
    let product_spec = start(&w, &t, None).await;
    let first = args(&w, &product_spec).await;
    assert!(
        first.contains("/otaq-product-spec") && !first.contains("--resume"),
        "{first}"
    );
    finished(&w, &t).await;
    let dev = start(&w, &t, Some(&product_spec.id)).await;
    let first = args(&w, &dev).await;
    assert!(
        first.contains("/otaq-work-ticket") && !first.contains("--resume"),
        "the first entry into Dev is fresh: {first}"
    );
    let t = w.board.ticket(&key).await.unwrap();
    place_in(&w, &t, "qa", Status::WaitingForYou).await;

    w.review
        .fail(&key, "Ask about the old prices too.", None, false, &me())
        .await
        .unwrap();
    let back = running(&w, &t, Some(&dev.id)).await;
    let resumed = args(&w, &back).await;
    let product_spec_id = product_spec.tool_session_id.clone().unwrap();
    assert!(
        resumed.contains(&format!("--resume {product_spec_id}")),
        "{resumed}"
    );
    assert!(
        resumed.contains("QA sent SHOP-1 back to you with a comment. Run `otaq prime`."),
        "{resumed}"
    );
    assert!(!resumed.contains("address-review"), "{resumed}");
    assert!(
        !w.sessions.info(&dev.id).unwrap().is_running(),
        "Dev's session stopped"
    );
    until_in(&w, &key, "product-spec", Status::Working).await;
    assert_eq!(sent_back_row(&w, &key).await["stage"], "Product spec");

    finished(&w, &t).await;
    let again = start(&w, &t, Some(&back.id)).await;
    let rewalk = args(&w, &again).await;
    let dev_id = dev.tool_session_id.clone().unwrap();
    assert!(rewalk.contains(&format!("--resume {dev_id}")), "{rewalk}");
    assert!(
        rewalk.contains(
            "SHOP-1 is back in Dev: QA sent it to Product spec, which finished. Run `otaq prime`."
        ),
        "{rewalk}"
    );
    assert!(
        !rewalk.contains("/otaq-"),
        "never a slash command: {rewalk}"
    );
    until_in(&w, &key, "dev", Status::Working).await;
    w.stop().await;
}

/// A Send back with a line comment is a code review: Product spec's conversation resumes with `address-review` first.
#[tokio::test(flavor = "multi_thread")]
async fn a_send_back_with_line_comments_runs_address_review_in_any_stage() {
    let dir = TempDir::new().unwrap();
    let w = world(&dir, &every_kind()).await;
    let t = w.create("SHOP", "Price table").await;
    let key = t.key.to_string();
    w.board
        .move_ticket(&key, DropTarget::ReadyForDev, true, &me())
        .await
        .unwrap();
    let product_spec = start(&w, &t, None).await;
    finished(&w, &t).await;
    let dev = start(&w, &t, Some(&product_spec.id)).await;
    let t = w.board.ticket(&key).await.unwrap();
    place_in(&w, &t, "qa", Status::WaitingForYou).await;
    line_comment(&w, &key).await;

    w.review.fail(&key, "", None, false, &me()).await.unwrap();
    let back = running(&w, &t, Some(&dev.id)).await;
    let resumed = args(&w, &back).await;
    assert!(
        resumed.contains(&format!(
            "--resume {}",
            product_spec.tool_session_id.clone().unwrap()
        )),
        "{resumed}"
    );
    assert!(
        resumed.contains(
            "/otaq-address-review SHOP-1 was sent back to Product spec with 1 line comment."
        ),
        "{resumed}"
    );
    w.stop().await;
}

/// QA's fails set to Dev: a Send back into the live Dev session types the plain sentence with only a comment, and
/// `/otaq-address-review` (which its tool loaded at start) with a line comment.
#[tokio::test(flavor = "multi_thread")]
async fn a_send_back_into_the_live_session_types_the_skill_only_for_line_comments() {
    let dir = TempDir::new().unwrap();
    let mut flow = every_kind();
    let qa = flow.flow.position("qa").unwrap();
    flow.flow.stages[qa].fail_to = Some("dev".into());
    let w = world(&dir, &flow).await;
    let t = w.create("SHOP", "Price table").await;
    let key = t.key.to_string();
    w.board
        .move_ticket(&key, DropTarget::ReadyForDev, true, &me())
        .await
        .unwrap();
    let product_spec = start(&w, &t, None).await;
    finished(&w, &t).await;
    let dev = start(&w, &t, Some(&product_spec.id)).await;

    let t = w.board.ticket(&key).await.unwrap();
    place_in(&w, &t, "qa", Status::WaitingForYou).await;
    w.review
        .fail(&key, "Round the prices.", None, false, &me())
        .await
        .unwrap();
    let out = w
        .wait_for_output(
            &dev.id,
            "QA sent SHOP-1 back to you with a comment. Run `otaq prime`.",
        )
        .await;
    assert!(!out.contains("RUNS otaq-address-review"), "{out}");
    assert_eq!(
        w.sessions
            .running_of(&t.id, SessionKind::Agent)
            .map(|s| s.id),
        Some(dev.id.clone()),
        "the live session is Dev's own: kept"
    );

    place_in(&w, &t, "qa", Status::WaitingForYou).await;
    line_comment(&w, &key).await;
    w.review.fail(&key, "", None, false, &me()).await.unwrap();
    let out = w.wait_for_output(&dev.id, "RUNS otaq-address-review").await;
    assert!(!out.contains("Unknown command"), "{out}");
    w.stop().await;
}

/// Validate with two check agents, its fails set to Product spec (not the nearest, Dev): one fails and the other
/// passes; the combined Fail stops Dev's live session and resumes Product spec's conversation, told both verdicts; the
/// round's event names the check and where it went.
#[tokio::test(flavor = "multi_thread")]
async fn an_agent_checks_fail_resumes_its_stage() {
    let dir = TempDir::new().unwrap();
    let mut flow = every_kind();
    let validate = flow.flow.position("validate").unwrap();
    flow.flow.stages[validate].fail_to = Some("product-spec".into());
    flow.flow.stages[validate].agents = vec!["Security reviewer".into(), "Validator".into()];
    let w = world(&dir, &flow).await;
    let t = w.create("SHOP", "Price table").await;
    let key = t.key.to_string();
    w.board
        .move_ticket(&key, DropTarget::ReadyForDev, true, &me())
        .await
        .unwrap();
    let product_spec = start(&w, &t, None).await;
    finished(&w, &t).await;
    let dev = start(&w, &t, Some(&product_spec.id)).await;
    finished(&w, &t).await;
    until_in(&w, &key, "validate", Status::Checking).await;
    let checkers = tokio::time::timeout(Duration::from_secs(20), async {
        loop {
            let running = w.sessions.running_all_of(&t.id, SessionKind::Validator);
            if running.len() == 2 {
                return running;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("both check agents run");
    for (checker, pass, text) in [
        (&checkers[0], false, "- prices.ts:3 rounds wrong"),
        (&checkers[1], true, "Prices match."),
    ] {
        let author = w.board.session_author(&checker.id).await.unwrap();
        let now = w.board.ticket(&key).await.unwrap();
        let hand_over = w
            .board
            .stage_end_hand_over(&now, author, text, None, &[])
            .await
            .unwrap();
        w.validation
            .verdict(&key, pass, hand_over.as_ref(), &checker.id)
            .await
            .unwrap();
    }

    let back = running(&w, &t, Some(&dev.id)).await;
    let resumed = args(&w, &back).await;
    assert!(
        resumed.contains(&format!(
            "--resume {}",
            product_spec.tool_session_id.clone().unwrap()
        )),
        "{resumed}"
    );
    assert!(
        resumed.contains(
            "Validate failed round 1: Security reviewer failed, Validator passed. Run `otaq prime`."
        ),
        "{resumed}"
    );
    assert!(
        !w.sessions.info(&dev.id).unwrap().is_running(),
        "Dev stopped"
    );
    until_in(&w, &key, "product-spec", Status::Working).await;
    let ended = w
        .board
        .history(&key)
        .await
        .unwrap()
        .into_iter()
        .rfind(|e| e.kind == EventKind::ValidationEnded)
        .unwrap();
    assert_eq!(
        (&ended.payload["check"], &ended.payload["back_to"]),
        (&"Validate".into(), &"Product spec".into())
    );
    w.stop().await;
}
