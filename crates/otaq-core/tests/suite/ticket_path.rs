//! The facts behind a ticket's choices, its path and its card (T-0411; AGM-09, AGM-03; d0108) on the test flow
//! `tests/flows/every-kind.yaml`: `ticket.agents` carries every stage with the agent it runs for the ticket, its runs
//! and its choices (with what a changed choice read before); `board.agents` says where a waiting ticket starts. The
//! default flow has the path and no choices.

use std::path::Path;

use otaq_core::agents::facts::{PathKind, board_agents, ticket_agents};
use otaq_core::board::stage_choices::{ChoicePick, Chooser};
use otaq_core::domain::{Actor, Place};
use otaq_core::flow::{FlowFile, yaml};
use otaq_core::service::testing::TestService;

fn every_kind_file() -> FlowFile {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/flows/every-kind.yaml");
    yaml::read(&std::fs::read_to_string(path).unwrap()).unwrap()
}

fn pick(stage: &str, choice: &str, reason: Option<&str>) -> ChoicePick {
    ChoicePick {
        stage: stage.into(),
        choice: choice.into(),
        reason: reason.map(Into::into),
    }
}

/// Writes `picks` on `key` as `chooser`.
async fn choose(w: &TestService, key: &str, picks: &[ChoicePick], chooser: &Chooser) {
    let ticket = w.board.ticket(key).await.unwrap();
    let writes = w
        .board
        .check_choices("SHOP", Some(&ticket), "", picks, chooser, false)
        .await
        .unwrap();
    w.board
        .write_choices(&ticket, &writes, &Actor::User("me".into()))
        .await
        .unwrap();
}

/// AGM-09: the path lists every stage of the flow with the agent it runs for this ticket (its choice, else the
/// stage's agent), the column it shows in, an agent check's rounds, the Delivery's mode, the skipped stages; the
/// choices come with them, and a changed choice says what it read before.
#[tokio::test(flavor = "multi_thread")]
async fn the_panel_facts_carry_the_path_and_the_choices() {
    let w = TestService::start().await;
    w.add_project("shop-web").await;
    w.board.use_flow_for_tests("SHOP", &every_kind_file()).await;
    w.create("SHOP", "Fix README typo").await;
    let planner = Chooser::ProjectAgent {
        name: "Checkout slowness".into(),
    };
    choose(
        &w,
        "SHOP-1",
        &[
            pick("Dev", "Quick developer", Some("typo fix")),
            pick("Product spec", "skip", Some("nothing to ask")),
        ],
        &planner,
    )
    .await;
    choose(
        &w,
        "SHOP-1",
        &[pick("Dev", "Rust developer", None)],
        &Chooser::User,
    )
    .await;

    let t = ticket_agents(&w.board, "SHOP-1").await.unwrap();
    assert_eq!(t.place, Some(Place::Backlog));
    let path: Vec<(&str, PathKind, Option<&str>, bool)> = t
        .path
        .iter()
        .map(|s| {
            (
                s.stage.as_str(),
                s.kind,
                s.agent.as_ref().map(|a| a.name.as_str()),
                s.skipped,
            )
        })
        .collect();
    assert_eq!(
        path,
        [
            ("Product spec", PathKind::Agent, Some("Spec writer"), true),
            ("Dev", PathKind::Agent, Some("Rust developer"), false),
            ("Validate", PathKind::AgentCheck, Some("Validator"), false),
            (
                "Security",
                PathKind::AgentCheck,
                Some("Security reviewer"),
                false
            ),
            ("QA", PathKind::HumanCheck, None, false),
            ("Done", PathKind::Delivery, Some("Shipper"), false),
        ]
    );
    assert_eq!(t.path[2].shown_in.as_deref(), Some("Dev"));
    assert_eq!((t.path[2].rounds, t.path[3].rounds), (Some(3), Some(2)));
    let ship = t.path[5].delivery.as_ref().unwrap();
    assert!(ship.automatic && ship.delivers == "pr");
    assert!(t.runs.is_empty(), "nothing ran yet");
    let dev = t.choices.iter().find(|c| c.stage == "Dev").unwrap();
    assert_eq!(dev.label(), "Rust developer");
    assert_eq!(
        dev.choice.as_ref().unwrap().from.as_deref(),
        Some("Quick developer")
    );
    let product_spec = t
        .choices
        .iter()
        .find(|c| c.stage == "Product spec")
        .unwrap();
    assert_eq!(
        product_spec.choice.as_ref().unwrap().from,
        None,
        "first set"
    );
    w.stop().await;
}

/// AGM-09 (`s03-38`): a ticket waiting in Ready for Dev says where it starts, past the stages it skips, on the agent
/// it chose there; a ticket in Backlog says nothing.
#[tokio::test(flavor = "multi_thread")]
async fn a_waiting_card_says_where_it_starts() {
    let w = TestService::start().await;
    w.add_project("shop-web").await;
    w.board.use_flow_for_tests("SHOP", &every_kind_file()).await;
    let waiting = w.create("SHOP", "Fix README typo").await;
    w.create("SHOP", "Saved carts").await;
    choose(
        &w,
        "SHOP-1",
        &[
            pick("Product spec", "skip", None),
            pick("Dev", "Quick developer", None),
        ],
        &Chooser::User,
    )
    .await;
    sqlx::query("UPDATE tickets SET state = 'queued', stage_id = NULL WHERE id = ?")
        .bind(&waiting.id)
        .execute(w.board.store().pool())
        .await
        .unwrap();
    let board = board_agents(&w.board, "SHOP").await.unwrap();
    let card = |key: &str| {
        board
            .cards
            .iter()
            .find(|c| c.key.to_string() == key)
            .unwrap()
    };
    let start = card("SHOP-1").starts.as_ref().expect("waiting");
    assert_eq!(
        (start.stage.as_str(), start.agent.as_str()),
        ("Dev", "Quick developer")
    );
    assert_eq!(card("SHOP-2").starts, None, "Backlog");
    assert_eq!(card("SHOP-2").refused_agent, None);
    w.stop().await;
}

/// AGM-09: on the default flow no stage has options: the ticket has its path (Dev · Validate · Review · Done) and no
/// choices.
#[tokio::test(flavor = "multi_thread")]
async fn the_default_flow_has_a_path_and_no_choices() {
    let w = TestService::start().await;
    let notes = w.add_project("pocket-notes").await;
    let note = w.create(&notes.key, "Offline mode").await;
    let t = ticket_agents(&w.board, &note.key.to_string())
        .await
        .unwrap();
    assert!(t.choices.is_empty());
    let stages: Vec<&str> = t.path.iter().map(|s| s.stage.as_str()).collect();
    assert_eq!(stages, ["Dev", "Validate", "Review", "Done"]);
    assert_eq!(t.path[0].agent.as_ref().unwrap().name, "Developer");
    assert!(t.path.iter().all(|s| !s.skipped));
    w.stop().await;
}
