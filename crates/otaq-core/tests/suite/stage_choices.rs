//! Stage choices (T-0407; AGM-08, AGM-11, AGM-13; d0108, d0087) on the test flow `tests/flows/every-kind.yaml`: Grill
//! (skip), Dev (Rust developer, or Quick developer), Validate, Security (no options), QA (a check by you, skip) and
//! an automatic PR Delivery. Who may set what, the locks, the skipped stages and the click Delivery after a skipped
//! human check. The default flow has no choices. `claude` is stood in for by an ordinary command.

use std::path::Path;
use std::time::Duration;

use otaq_core::agent_tools::LaunchContext;
use otaq_core::agent_tools::claude::ClaudeLaunch;
use otaq_core::agents::resolve;
use otaq_core::board::moves::DropTarget;
use otaq_core::board::stage_choices::{
    ChoicePick, ChoiceWrite, Chooser, SetBy, TicketChoices, skipped_mark,
};
use otaq_core::board::tickets::Ticket;
use otaq_core::board::{BoardError, Refusal};
use otaq_core::domain::{Actor, Kind, Place, Status};
use otaq_core::flow::engine::{self, At, Move, RunResult};
use otaq_core::flow::{FlowFile, StageOption, yaml};
use otaq_core::service::ServiceConfig;
use otaq_core::service::testing::TestService;

fn me() -> Actor {
    Actor::User("me".into())
}

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

fn planner() -> Chooser {
    Chooser::ProjectAgent {
        name: "Checkout slowness".into(),
    }
}

fn griller() -> Chooser {
    Chooser::TicketAgent {
        name: "Griller".into(),
    }
}

/// A world on the test flow: project SHOP.
async fn world() -> TestService {
    let w = TestService::start().await;
    w.add_project("shop-web").await;
    w.board.use_flow_for_tests("SHOP", &every_kind_file()).await;
    w
}

/// Checks and writes `picks` on `ticket` as `chooser` would (the service's `ticket.update`).
async fn choose(
    w: &TestService,
    ticket: &Ticket,
    picks: &[ChoicePick],
    chooser: &Chooser,
    by_user: bool,
) -> Result<Vec<ChoiceWrite>, BoardError> {
    let ticket = w.board.ticket(&ticket.key.to_string()).await?;
    let writes = w
        .board
        .check_choices(
            "SHOP",
            Some(&ticket),
            (ticket.kind, ""),
            picks,
            chooser,
            by_user,
        )
        .await?;
    w.board.write_choices(&ticket, &writes, &me(), None).await?;
    Ok(writes)
}

fn refusal(result: Result<Vec<ChoiceWrite>, BoardError>) -> (Refusal, String) {
    let e = result.unwrap_err();
    (e.refusal().expect("a refusal"), e.to_string())
}

async fn choices(w: &TestService, ticket: &Ticket) -> TicketChoices {
    w.board
        .ticket_choices(&ticket.key.to_string())
        .await
        .unwrap()
}

/// The ticket's choice for `stage` as it reads, and who set it.
async fn choice_of(w: &TestService, ticket: &Ticket, stage: &str) -> (String, Option<SetBy>) {
    let all = choices(w, ticket).await;
    let stage = all.stages.iter().find(|s| s.stage == stage).unwrap();
    (
        stage.label(),
        stage.choice.as_ref().map(|c| c.set_by.clone()),
    )
}

/// Puts `ticket` at work in `stage`, started, as the start would have (its run of the stage opens).
async fn at_work_in(w: &TestService, ticket: &Ticket, stage: &str) {
    sqlx::query("UPDATE tickets SET state = 'working', stage_id = ?, started_at = 1 WHERE id = ?")
        .bind(stage)
        .bind(&ticket.id)
        .execute(w.board.store().pool())
        .await
        .unwrap();
}

/// AGM-08's rules, sentence by sentence: the Project agent sets Dev: Quick developer at creation; the user changes
/// it and wins; a later agent can't override the user's choice (unless in the user's words, d0087); Griller, at work
/// in Grill, can't choose for Security (no options) and changes Dev with a reason; an option not in the stage's list
/// is refused naming them; a choice locks when Dev starts.
#[tokio::test(flavor = "multi_thread")]
async fn who_may_set_what() {
    let w = world().await;

    // The Project agent at creation (a batch about to create it): with a reason.
    let label = "the new ticket \"Fix README typo\"";
    let writes = w
        .board
        .check_choices(
            "SHOP",
            None,
            (Kind::Ticket, label),
            &[pick("Dev", "quick developer", Some("typo fix"))],
            &planner(),
            false,
        )
        .await
        .unwrap();
    let typo = w.create("SHOP", "Fix README typo").await;
    w.board
        .write_choices(&typo, &writes, &me(), None)
        .await
        .unwrap();
    assert_eq!(
        choice_of(&w, &typo, "Dev").await,
        (
            "Quick developer".to_owned(),
            Some(SetBy::ProjectAgent {
                name: "Checkout slowness".into(),
                reason: "typo fix".into()
            })
        )
    );
    // Before the ticket starts, an agent may set any stage's choice, but always with a reason.
    assert_eq!(
        refusal(choose(&w, &typo, &[pick("Grill", "skip", None)], &planner(), false).await),
        (Refusal::EmptyField, "Say why with --reason".into())
    );
    choose(
        &w,
        &typo,
        &[pick("Grill", "skip", Some("nothing to ask"))],
        &planner(),
        false,
    )
    .await
    .unwrap();

    // The user changes Dev back; their choice wins over any agent's later one.
    choose(
        &w,
        &typo,
        &[pick("dev", "Rust developer", None)],
        &Chooser::User,
        false,
    )
    .await
    .unwrap();
    assert_eq!(
        choice_of(&w, &typo, "Dev").await,
        ("Rust developer".to_owned(), Some(SetBy::User))
    );
    let set_by_you = (
        Refusal::NotAllowed,
        "SHOP-1's Dev choice was set by you; agents can't change it".to_owned(),
    );
    assert_eq!(
        refusal(
            choose(
                &w,
                &typo,
                &[pick("Dev", "Quick developer", Some("typo fix"))],
                &planner(),
                false
            )
            .await
        ),
        set_by_you
    );
    // Griller, at work in Grill, can't override it either.
    at_work_in(&w, &typo, "grill").await;
    assert_eq!(
        refusal(
            choose(
                &w,
                &typo,
                &[pick("Dev", "Quick developer", Some("x"))],
                &griller(),
                false
            )
            .await
        ),
        set_by_you
    );
    // The request comes from the user's words (`--by-user`): it is the user's.
    choose(
        &w,
        &typo,
        &[pick("Dev", "Quick developer", None)],
        &griller(),
        true,
    )
    .await
    .unwrap();
    assert_eq!(
        choice_of(&w, &typo, "Dev").await,
        ("Quick developer".to_owned(), Some(SetBy::User))
    );

    // Another ticket at work in Grill: Griller changes the later Dev with a reason, not Grill (started), and
    // Security has no options.
    let session = w.create("SHOP", "Sessions survive a restart").await;
    choose(
        &w,
        &session,
        &[pick("Dev", "Quick developer", Some("small change"))],
        &planner(),
        false,
    )
    .await
    .unwrap();
    at_work_in(&w, &session, "grill").await;
    assert_eq!(
        refusal(
            choose(
                &w,
                &session,
                &[pick("Security", "Quick developer", Some("x"))],
                &griller(),
                false
            )
            .await
        ),
        (
            Refusal::NotAnOption,
            "Security has no options: every ticket runs it as the flow says".into()
        )
    );
    assert_eq!(
        refusal(
            choose(
                &w,
                &session,
                &[pick("Grill", "skip", Some("x"))],
                &griller(),
                false
            )
            .await
        )
        .0,
        Refusal::ChoiceLocked
    );
    assert_eq!(
        refusal(
            choose(
                &w,
                &session,
                &[pick("Dev", "Rust developer", None)],
                &griller(),
                false
            )
            .await
        ),
        (Refusal::EmptyField, "Say why with --reason".into())
    );
    choose(
        &w,
        &session,
        &[pick(
            "Dev",
            "Rust developer",
            Some("touches session handling"),
        )],
        &griller(),
        false,
    )
    .await
    .unwrap();
    assert_eq!(
        choice_of(&w, &session, "Dev").await,
        (
            "Rust developer".to_owned(),
            Some(SetBy::Agent {
                name: "Griller".into(),
                reason: "touches session handling".into()
            })
        )
    );
    // History names who chose and why.
    let history = w.board.history(&session.key.to_string()).await.unwrap();
    let last = history
        .iter()
        .find(|e| {
            e.kind == otaq_core::board::tickets::EventKind::StageChoiceSet
                && e.payload["by"] == "Griller"
        })
        .unwrap();
    assert_eq!(last.payload["from"], "Quick developer");
    assert_eq!(last.payload["by"], "Griller");
    assert_eq!(last.payload["reason"], "touches session handling");

    // An option not in the stage's list is refused, naming the options; so is an unknown stage.
    let fresh = w.create("SHOP", "Export as CSV").await;
    assert_eq!(
        refusal(
            choose(
                &w,
                &fresh,
                &[pick("Dev", "Griller", None)],
                &Chooser::User,
                false
            )
            .await
        ),
        (
            Refusal::NotAnOption,
            "Griller isn't one of Dev's options: Rust developer, Quick developer".into()
        )
    );
    assert_eq!(
        refusal(
            choose(
                &w,
                &fresh,
                &[pick("QA", "Quick developer", None)],
                &Chooser::User,
                false
            )
            .await
        )
        .1,
        "Quick developer isn't one of QA's options: required, skip"
    );
    let unknown = choose(
        &w,
        &fresh,
        &[pick("Lint", "skip", None)],
        &Chooser::User,
        false,
    )
    .await;
    assert_eq!(
        unknown.unwrap_err().to_string(),
        "SHOP's flow has no stage Lint (stages with options: Grill, Dev, QA)"
    );

    // A choice locks when its stage starts: Dev at work.
    at_work_in(&w, &fresh, "dev").await;
    let locked = choices(&w, &fresh).await;
    let dev = locked.stages.iter().find(|s| s.stage == "Dev").unwrap();
    assert!(dev.locked);
    assert!(
        !locked
            .stages
            .iter()
            .find(|s| s.stage == "QA")
            .unwrap()
            .locked
    );
    assert_eq!(
        refusal(
            choose(
                &w,
                &fresh,
                &[pick("Dev", "Quick developer", None)],
                &Chooser::User,
                false
            )
            .await
        ),
        (
            Refusal::ChoiceLocked,
            "Dev has started on SHOP-3: its choice is locked".into()
        )
    );
    // The user can still skip the QA ahead.
    choose(
        &w,
        &fresh,
        &[pick("QA", "skip", None)],
        &Chooser::User,
        false,
    )
    .await
    .unwrap();
    w.stop().await;
}

/// AGM-05 and AGM-08: the running agent is the choice, else the stage's agent; a choice for an agent the stage no
/// longer offers, or for a stage the flow no longer has, is ignored. Epics have no choices.
#[tokio::test(flavor = "multi_thread")]
async fn the_running_agent_and_choices_the_flow_dropped() {
    let w = world().await;
    let t = w.create("SHOP", "Fix README typo").await;
    let running = |t: Ticket, stage: &'static str| {
        let w = &w;
        async move {
            resolve::running_agent(&w.board, &t, stage)
                .await
                .unwrap()
                .name
        }
    };
    assert_eq!(running(t.clone(), "dev").await, "Rust developer");
    choose(
        &w,
        &t,
        &[
            pick("Dev", "Quick developer", None),
            pick("Grill", "skip", None),
        ],
        &Chooser::User,
        false,
    )
    .await
    .unwrap();
    assert_eq!(running(t.clone(), "dev").await, "Quick developer");
    let flow = w.board.effective_flow("SHOP").await.unwrap();
    assert_eq!(w.board.ticket_skips(&t, &flow).await.unwrap(), ["grill"]);
    let agents = otaq_core::agents::facts::ticket_agents(&w.board, "SHOP-1")
        .await
        .unwrap();
    let dev = agents.stages.iter().find(|s| s.stage_id == "dev").unwrap();
    assert_eq!(dev.agent.as_ref().unwrap().name, "Quick developer");

    // The flow drops Dev's option and Grill's skip: the choices are ignored, the usual agents run.
    let mut file = every_kind_file();
    for stage in &mut file.flow.stages {
        stage.options.clear();
    }
    w.board.use_flow_for_tests("SHOP", &file).await;
    assert_eq!(running(t.clone(), "dev").await, "Rust developer");
    let flow = w.board.effective_flow("SHOP").await.unwrap();
    assert!(w.board.ticket_skips(&t, &flow).await.unwrap().is_empty());
    assert!(choices(&w, &t).await.stages.is_empty());
    // And a flow without Grill: its choice is ignored too.
    let mut file = every_kind_file();
    file.flow.stages.retain(|s| s.id != "grill");
    w.board.use_flow_for_tests("SHOP", &file).await;
    let left = choices(&w, &t).await;
    assert_eq!(
        left.stages
            .iter()
            .map(|s| s.stage.as_str())
            .collect::<Vec<_>>(),
        ["Dev", "QA"]
    );
    assert_eq!(left.stages[0].label(), "Quick developer");
    let flow = w.board.effective_flow("SHOP").await.unwrap();
    assert!(w.board.ticket_skips(&t, &flow).await.unwrap().is_empty());

    // Epics have none.
    let epic = w
        .board
        .create_ticket(
            &otaq_core::board::tickets::NewTicketInput {
                project: "SHOP".into(),
                title: "Wishlist".into(),
                kind: Kind::Epic,
                ..Default::default()
            },
            &me(),
        )
        .await
        .unwrap();
    assert!(choices(&w, &epic).await.stages.is_empty());
    assert_eq!(
        refusal(
            choose(
                &w,
                &epic,
                &[pick("Dev", "Quick developer", None)],
                &Chooser::User,
                false
            )
            .await
        ),
        (
            Refusal::NotAnOption,
            "SHOP-2 is an epic: epics have no stage choices".into()
        )
    );
    w.stop().await;
}

/// Keeps running, printing its arguments.
const KEEPS_RUNNING: &str = "printf 'ARGS %s\\n' \"$*\"; exec sleep 30";

fn launch(config: ServiceConfig) -> ServiceConfig {
    ServiceConfig {
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
    }
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

/// Grill skipped and Dev on Quick developer: the start waits in Ready for Dev like any, then goes straight into
/// Dev, where Quick developer runs; Grill has a run of result `skipped`; both choices are locked now. The card's mark
/// reads "Skipped Grill · this ticket's choice".
#[tokio::test(flavor = "multi_thread")]
async fn a_skipped_grill_starts_in_dev_on_the_chosen_agent() {
    let w = TestService::start_with(launch).await;
    w.add_project("shop-web").await;
    w.board.use_flow_for_tests("SHOP", &every_kind_file()).await;
    let t = w.create("SHOP", "Fix README typo").await;
    choose(
        &w,
        &t,
        &[
            pick("Grill", "skip", None),
            pick("Dev", "Quick developer", None),
        ],
        &Chooser::User,
        false,
    )
    .await
    .unwrap();
    w.board
        .move_ticket("SHOP-1", DropTarget::ReadyForDev, true, &me())
        .await
        .unwrap();
    assert_eq!(
        w.board.ticket("SHOP-1").await.unwrap().place,
        Place::Waiting(None),
        "waiting before the first stage it runs"
    );
    w.starter
        .start("SHOP-1", &me(), otaq_core::session::Start::Auto)
        .await
        .unwrap();
    until_status(&w, "SHOP-1", Status::Working).await;
    assert_eq!(
        w.board.ticket("SHOP-1").await.unwrap().place,
        Place::stage_of("dev")
    );
    let runs = w.board.stage_runs("SHOP-1").await.unwrap();
    let summary: Vec<(&str, Option<RunResult>)> =
        runs.iter().map(|r| (r.stage.as_str(), r.result)).collect();
    assert_eq!(
        summary,
        [("grill", Some(RunResult::Skipped)), ("dev", None)]
    );
    let agent: Option<String> = sqlx::query_scalar(
        "SELECT agent_name FROM sessions WHERE ticket_id = ? AND kind = 'agent' ORDER BY started_at DESC",
    )
    .bind(&t.id)
    .fetch_one(w.board.store().pool())
    .await
    .unwrap();
    assert_eq!(agent.as_deref(), Some("Quick developer"));
    let now = choices(&w, &t).await;
    let locked: Vec<(&str, bool)> = now
        .stages
        .iter()
        .map(|s| (s.stage.as_str(), s.locked))
        .collect();
    assert_eq!(locked, [("Grill", true), ("Dev", true), ("QA", false)]);
    assert!(now.stages[0].skipped());
    assert_eq!(
        skipped_mark("Grill"),
        "Skipped Grill · this ticket's choice"
    );
    w.stop().await;
}

/// QA (a human check) skipped by the ticket's choice: Security's Pass walks past QA (a skipped run) into the
/// automatic PR Delivery, which then waits for a click (d0108: a ticket that skipped a human check always reaches
/// Delivery in click mode). Without the skip the same Pass enters QA, and QA's Pass delivers with no click.
#[tokio::test(flavor = "multi_thread")]
async fn a_skipped_qa_reaches_done_in_click_mode() {
    let w = world().await;
    let t = w.create("SHOP", "Fix README typo").await;
    choose(&w, &t, &[pick("QA", "skip", None)], &Chooser::User, false)
        .await
        .unwrap();
    let flow = w.board.effective_flow("SHOP").await.unwrap();
    let ship = flow.stage("ship").unwrap().delivery.unwrap();
    assert_eq!(
        ship.mode,
        otaq_core::flow::DeliveryMode::Automatic,
        "the flow's Delivery is automatic"
    );
    let skips = w.board.ticket_skips(&t, &flow).await.unwrap();
    assert_eq!(skips, ["qa"]);
    let in_security = Place::stage_of("security");
    let pass = engine::step(
        At {
            skips: &skips,
            ..At::new(&flow, &in_security, Status::Checking)
        },
        Move::Pass,
    )
    .unwrap();
    assert_eq!(
        (pass.place.clone(), pass.status, pass.automatic),
        (Place::stage_of("ship"), Status::ToDeliver, false),
        "Done in click mode"
    );
    let through: Vec<(&str, RunResult)> = pass
        .passed_through
        .iter()
        .map(|(s, r)| (s.id.as_str(), *r))
        .collect();
    assert_eq!(through, [("qa", RunResult::Skipped)]);

    // Not skipped: QA, then an automatic Delivery.
    let into_qa = engine::step(At::new(&flow, &in_security, Status::Checking), Move::Pass).unwrap();
    assert_eq!(into_qa.place, Place::stage_of("qa"));
    let in_qa = Place::stage_of("qa");
    let delivered = engine::step(
        At::new(&flow, &in_qa, Status::WaitingForYou),
        Move::HumanPass,
    )
    .unwrap();
    assert!(delivered.automatic);
    w.stop().await;
}

/// The default flow has no options: no choices, and a `--stage` is refused.
#[tokio::test(flavor = "multi_thread")]
async fn the_default_flow_has_no_choices() {
    let w = TestService::start().await;
    w.add_project("pocket-notes").await;
    let project = w.board.projects().await.unwrap()[0].key.clone();
    let flow = w.board.effective_flow(&project).await.unwrap();
    assert!(
        flow.stages
            .iter()
            .all(|s| s.options.iter().all(|o| *o != StageOption::Skip) && s.options.is_empty())
    );
    let t = w.create(&project, "Sort notes").await;
    assert!(choices(&w, &t).await.stages.is_empty());
    assert!(
        w.board
            .new_ticket_choices(&project)
            .await
            .unwrap()
            .stages
            .is_empty()
    );
    let refused = w
        .board
        .check_choices(
            &project,
            Some(&t),
            (Kind::Ticket, ""),
            &[pick("Dev", "skip", None)],
            &Chooser::User,
            false,
        )
        .await
        .unwrap_err();
    assert_eq!(
        refused.to_string(),
        "Dev has no options; add `skip` to its options in the flow to allow it."
    );
    w.stop().await;
}
