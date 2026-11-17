//! The facts behind a ticket's choices, its path and its card (T-0411; AGM-09, AGM-03; d0108, UX-65) on the test flow
//! `tests/flows/every-kind.yaml`: `ticket.agents` carries every stage with who does it (the stage's own model), its
//! runs and its choices (skips, with what a changed choice read before); `board.agents` says where a waiting ticket
//! starts and the tool of each card's stage. On the default flow no stage can be skipped, so a ticket has no choice.

use std::path::Path;

use otaq_core::agents::facts::{PathKind, board_agents, ticket_agents};
use otaq_core::board::stage_choices::{ChoicePick, Chooser};
use otaq_core::domain::{Actor, Place};
use otaq_core::flow::{FlowFile, WhoDoesIt, yaml};
use otaq_core::service::testing::TestService;

fn every_kind_file() -> FlowFile {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/flows/every-kind.yaml");
    yaml::read(&std::fs::read_to_string(path).unwrap()).unwrap()
}

/// The test flow with Dev on Codex and Validate on Claude Code's Opus: stages each say who does them.
fn on_tools() -> FlowFile {
    let mut file = every_kind_file();
    for stage in &mut file.flow.stages {
        match stage.id.as_str() {
            "dev" => stage.who.model = Some("codex:gpt-5.5".into()),
            "validate" => stage.who.model = Some("claude:opus".into()),
            _ => {}
        }
    }
    file
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

/// AGM-09: the path lists every stage of the flow with who does it (the stage's own model), an agent check's rounds,
/// the Delivery's mode, the skipped stages; the choices come with them (only the stages that may be skipped), and a
/// changed choice says what it read before.
#[tokio::test(flavor = "multi_thread")]
async fn the_panel_facts_carry_the_path_and_the_choices() {
    let w = TestService::start().await;
    w.add_project("shop-web").await;
    w.board.use_flow_for_tests("SHOP", &on_tools()).await;
    w.create("SHOP", "Fix README typo").await;
    choose(
        &w,
        "SHOP-1",
        &[pick("Product spec", "skip", None)],
        &Chooser::User,
    )
    .await;
    choose(
        &w,
        "SHOP-1",
        &[
            pick("Product spec", "required", None),
            pick("QA", "skip", None),
        ],
        &Chooser::User,
    )
    .await;

    let t = ticket_agents(&w.board, "SHOP-1").await.unwrap();
    assert_eq!(t.place, Some(Place::Backlog));
    let path: Vec<(&str, PathKind, Option<String>, bool)> = t
        .path
        .iter()
        .map(|s| {
            (
                s.stage.as_str(),
                s.kind,
                s.who.as_ref().map(|w| w.model.text()),
                s.skipped,
            )
        })
        .collect();
    let some = |m: &str| Some(m.to_owned());
    assert_eq!(
        path,
        [
            ("Product spec", PathKind::Agent, some("default"), false),
            ("Dev", PathKind::Agent, some("codex:gpt-5.5"), false),
            ("Validate", PathKind::AgentCheck, some("claude:opus"), false),
            ("Security", PathKind::AgentCheck, some("default"), false),
            ("QA", PathKind::HumanCheck, None, true),
            ("Delivery", PathKind::Delivery, some("default"), false),
        ]
    );
    // A stage's instructions come with who does it (UX-67).
    assert_eq!(
        t.path[3].who.as_ref().unwrap().instructions,
        "Never edit files.\nFail on unescaped SQL."
    );

    assert_eq!((t.path[2].rounds, t.path[3].rounds), (Some(3), Some(2)));
    let ship = t.path[5].delivery.as_ref().unwrap();
    assert!(ship.automatic && ship.delivers == "pr");
    assert!(t.runs.is_empty(), "nothing ran yet");
    let stages: Vec<&str> = t.choices.iter().map(|c| c.stage.as_str()).collect();
    assert_eq!(stages, ["Product spec", "QA"], "only the skippable stages");
    let spec = t
        .choices
        .iter()
        .find(|c| c.stage == "Product spec")
        .unwrap();
    assert_eq!(spec.label(), "required");
    assert_eq!(spec.choice.as_ref().unwrap().from.as_deref(), Some("skip"));
    let qa = t.choices.iter().find(|c| c.stage == "QA").unwrap();
    assert_eq!(qa.choice.as_ref().unwrap().from, None, "first set");
    w.stop().await;
}

/// AGM-09 (`s03-38`): a ticket waiting to begin says where it starts, past the stages it skips, on the tool of that
/// stage's model; a ticket in Backlog says nothing.
#[tokio::test(flavor = "multi_thread")]
async fn a_waiting_card_says_where_it_starts() {
    let w = TestService::start().await;
    w.add_project("shop-web").await;
    w.board.use_flow_for_tests("SHOP", &on_tools()).await;
    let waiting = w.create("SHOP", "Fix README typo").await;
    w.create("SHOP", "Saved carts").await;
    choose(
        &w,
        "SHOP-1",
        &[pick("Product spec", "skip", None)],
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
        ("Dev", "Codex")
    );
    assert_eq!(
        start.model.as_ref().and_then(|m| m.model.as_deref()),
        Some("gpt-5.5")
    );
    assert_eq!(card("SHOP-2").starts, None, "Backlog");
    assert_eq!(card("SHOP-2").refused_stage, None);
    w.stop().await;
}

/// AGM-09, UX-72: on the default flow no stage can be skipped, so a ticket has no choice; its path is Dev · Validate ·
/// Review · Delivery, each agent stage on the flow's own model.
#[tokio::test(flavor = "multi_thread")]
async fn the_default_flow_has_a_path_and_no_choice() {
    let w = TestService::start().await;
    let notes = w.add_project("pocket-notes").await;
    let note = w.create(&notes.key, "Offline mode").await;
    let t = ticket_agents(&w.board, &note.key.to_string())
        .await
        .unwrap();
    assert!(t.choices.is_empty(), "{:?}", t.choices);
    let stages: Vec<&str> = t.path.iter().map(|s| s.stage.as_str()).collect();
    assert_eq!(stages, ["Dev", "Validate", "Review", "Delivery"]);
    let dev = t.path[0].who.as_ref().unwrap();
    let stored = w
        .board
        .current_flow_file(&notes.key)
        .await
        .unwrap()
        .flow
        .stage("dev")
        .unwrap()
        .who
        .clone();
    assert_eq!(
        Some(dev.model.text()),
        stored.model,
        "the stage's own model"
    );
    assert_ne!(stored, WhoDoesIt::default());
    assert!(t.path.iter().all(|s| !s.skipped));
    w.stop().await;
}

/// The search's filter reads each card's stage agent: the tool of the stage the ticket is in; none in a check by you
/// or in Backlog.
#[tokio::test(flavor = "multi_thread")]
async fn each_card_names_the_tool_of_its_stage() {
    let w = TestService::start().await;
    w.add_project("shop-web").await;
    w.board.use_flow_for_tests("SHOP", &on_tools()).await;
    let working = w.create("SHOP", "Fix README typo").await;
    let validating = w.create("SHOP", "Saved carts").await;
    let in_qa = w.create("SHOP", "Gift cards").await;
    w.create("SHOP", "Wishlist").await;
    for (ticket, state, stage) in [
        (&working, "working", "dev"),
        (&validating, "validating", "validate"),
        (&in_qa, "ready_for_review", "qa"),
    ] {
        sqlx::query("UPDATE tickets SET state = ?, stage_id = ? WHERE id = ?")
            .bind(state)
            .bind(stage)
            .bind(&ticket.id)
            .execute(w.board.store().pool())
            .await
            .unwrap();
    }
    let board = board_agents(&w.board, "SHOP").await.unwrap();
    let agent = |key: &str| {
        board
            .cards
            .iter()
            .find(|c| c.key.to_string() == key)
            .unwrap()
            .stage_agent
            .clone()
    };
    assert_eq!(agent("SHOP-1").as_deref(), Some("Codex"));
    assert_eq!(agent("SHOP-2").as_deref(), Some("Claude Code"));
    assert_eq!(agent("SHOP-3"), None, "a check by you");
    assert_eq!(agent("SHOP-4"), None, "Backlog");
    w.stop().await;
}
