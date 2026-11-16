//! Stage choices (T-0407; AGM-08, AGM-11, AGM-13; d0087, UX-72) on the test flow `tests/flows/every-kind.yaml`:
//! Product spec (skippable), Dev, Validate, Security, QA (a check by you, skippable) and an automatic PR Delivery. The
//! only choice is skipping a stage the flow marks skippable: a ticket can't pick its own model, every stage runs on its
//! own. Who may set what (only the user, or Noyan passing on the user's words), the locks, the skipped stages and the
//! Delivery after a skipped human check. `claude` is stood in for by an ordinary command.

use std::path::Path;
use std::time::Duration;

use otaq_core::agent_tools::LaunchContext;
use otaq_core::agent_tools::claude::ClaudeLaunch;
use otaq_core::agents::resolve;
use otaq_core::board::moves::DropTarget;
use otaq_core::board::stage_choices::{
    ChoicePick, ChoiceWrite, Chooser, NOYAN_USER_CHOICES_ONLY, ONLY_NOYAN_PASSES_ON,
    ONLY_THE_USER_PICKS, ONLY_THE_USER_SKIPS, SetBy, TicketChoices, skipped_mark,
};
use otaq_core::board::tickets::Ticket;
use otaq_core::board::{BoardError, Refusal};
use otaq_core::domain::{Actor, Place, Status};
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

fn architect() -> Chooser {
    Chooser::TicketAgent {
        name: "Architect".into(),
    }
}

fn spec_writer() -> Chooser {
    Chooser::TicketAgent {
        name: "Spec writer".into(),
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
        .check_choices("SHOP", Some(&ticket), "", picks, chooser, by_user)
        .await?;
    w.board.write_choices(&ticket, &writes, &me()).await?;
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

/// AGM-08's rules, sentence by sentence: Noyan sets only the choices the user names (`--by-user`), its own are
/// refused; a ticket's own agent sets none (only the user skips a stage), and only Noyan passes on the user's words; a
/// ticket can't pick a model (UX-72): a stage that can't be skipped has no choice, and anything but `skip` or
/// `required` is refused; a choice locks when its stage starts.
#[tokio::test(flavor = "multi_thread")]
async fn who_may_set_what() {
    let w = world().await;

    // Noyan at creation (a batch about to create it): its own choice is refused, with a reason or not, `required`
    // and `skip` too (T1).
    let label = "the new ticket \"Fix README typo\"";
    for own in [
        pick("QA", "required", Some("risky")),
        pick("Product spec", "skip", Some("ordinary work")),
    ] {
        let refused = w
            .board
            .check_choices("SHOP", None, label, &[own], &planner(), false)
            .await;
        assert_eq!(
            refusal(refused),
            (Refusal::NotAllowed, NOYAN_USER_CHOICES_ONLY.into())
        );
    }
    // The user's choice, passed on with `--by-user`: accepted, and the user's.
    let writes = w
        .board
        .check_choices(
            "SHOP",
            None,
            label,
            &[pick("Product spec", "skip", None)],
            &planner(),
            true,
        )
        .await
        .unwrap();
    let typo = w.create("SHOP", "Fix README typo").await;
    w.board.write_choices(&typo, &writes, &me()).await.unwrap();
    assert_eq!(
        choice_of(&w, &typo, "Product spec").await,
        ("skip".to_owned(), Some(SetBy::User))
    );

    // A ticket's own agent sets no choice, before the ticket starts or after, with a reason or not. Nothing is
    // written.
    at_work_in(&w, &typo, "dev").await;
    for chooser in [architect(), spec_writer()] {
        for (picked, refused) in [
            (
                pick("QA", "required", Some("touches payments")),
                ONLY_THE_USER_PICKS,
            ),
            (pick("QA", "skip", Some("docs only")), ONLY_THE_USER_SKIPS),
        ] {
            assert_eq!(
                refusal(choose(&w, &typo, &[picked], &chooser, false).await),
                (Refusal::NotAllowed, refused.into())
            );
        }
        // Only Noyan passes on the user's choices: a ticket agent's `--by-user` is refused (T9).
        for words in [None, Some("the user asked")] {
            assert_eq!(
                refusal(choose(&w, &typo, &[pick("QA", "skip", words)], &chooser, true).await),
                (Refusal::NotAllowed, ONLY_NOYAN_PASSES_ON.into())
            );
        }
    }
    assert_eq!(choice_of(&w, &typo, "QA").await.1, None);

    // The user skips QA; History says who chose.
    let fresh = w.create("SHOP", "Export as CSV").await;
    choose(
        &w,
        &fresh,
        &[pick("QA", "skip", None)],
        &Chooser::User,
        false,
    )
    .await
    .unwrap();
    assert_eq!(
        choice_of(&w, &fresh, "QA").await,
        ("skip".to_owned(), Some(SetBy::User))
    );
    let history = w.board.history(&fresh.key.to_string()).await.unwrap();
    let set = history
        .iter()
        .find(|e| e.kind == otaq_core::board::tickets::EventKind::StageChoiceSet)
        .unwrap();
    assert_eq!(set.payload["by"], "you");

    // A ticket can't pick a model (UX-72): Dev has no choice, a word that isn't one is refused naming them, Dev isn't
    // skippable, and an unknown stage lists the stages with choices.
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
            Refusal::NotAnOption,
            "Dev has no choices: every ticket runs it as the flow says".into()
        )
    );
    assert_eq!(
        refusal(
            choose(
                &w,
                &fresh,
                &[pick("Product spec", "Spec writer", None)],
                &Chooser::User,
                false
            )
            .await
        ),
        (
            Refusal::NotAnOption,
            "Spec writer isn't one of Product spec's choices: required, skip".into()
        )
    );
    assert_eq!(
        refusal(
            choose(
                &w,
                &fresh,
                &[pick("Dev", "skip", None)],
                &Chooser::User,
                false
            )
            .await
        ),
        (
            Refusal::NotAnOption,
            "Dev can't be skipped; add `skip` to its options in the flow to allow it.".into()
        )
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
        "SHOP's flow has no stage Lint (stages with choices: Product spec, QA)"
    );

    // A choice locks when its stage starts: Product spec at work.
    at_work_in(&w, &fresh, "product-spec").await;
    let locked = choices(&w, &fresh).await;
    let spec = locked
        .stages
        .iter()
        .find(|s| s.stage == "Product spec")
        .unwrap();
    assert!(spec.locked);
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
                &[pick("Product spec", "skip", None)],
                &Chooser::User,
                false
            )
            .await
        ),
        (
            Refusal::ChoiceLocked,
            "Product spec has started on SHOP-2: its choice is locked".into()
        )
    );
    // The user can still change the QA ahead.
    choose(
        &w,
        &fresh,
        &[pick("QA", "required", None)],
        &Chooser::User,
        false,
    )
    .await
    .unwrap();

    // Noyan passes on the user's skips: stored as the user's.
    let noyan = w.create("SHOP", "Rename the export button").await;
    choose(
        &w,
        &noyan,
        &[pick("QA", "skip", None), pick("Product spec", "skip", None)],
        &planner(),
        true,
    )
    .await
    .unwrap();
    assert_eq!(
        choice_of(&w, &noyan, "QA").await,
        ("skip".to_owned(), Some(SetBy::User))
    );
    assert_eq!(
        choice_of(&w, &noyan, "Product spec").await,
        ("skip".to_owned(), Some(SetBy::User))
    );
    w.stop().await;
}

/// AGM-05 and AGM-08: a stage runs on its own model, whatever the ticket chose; a skip of a stage no longer
/// skippable, or a choice for a stage the flow no longer has, is ignored.
#[tokio::test(flavor = "multi_thread")]
async fn the_stages_own_model_and_choices_the_flow_dropped() {
    let w = world().await;
    let t = w.create("SHOP", "Fix README typo").await;
    let running = |t: Ticket, stage: &'static str| {
        let w = &w;
        async move {
            resolve::running_who(&w.board, &t, stage)
                .await
                .unwrap()
                .instructions
        }
    };
    assert_eq!(
        running(t.clone(), "dev").await,
        "Follow our error-handling rules: no unwrap in library code."
    );
    choose(
        &w,
        &t,
        &[pick("Product spec", "skip", None)],
        &Chooser::User,
        false,
    )
    .await
    .unwrap();
    let flow = w.board.effective_flow("SHOP").await.unwrap();
    assert_eq!(
        w.board.ticket_skips(&t, &flow).await.unwrap(),
        ["product-spec"]
    );
    let agents = otaq_core::agents::facts::ticket_agents(&w.board, "SHOP-1")
        .await
        .unwrap();
    let dev = agents.stages.iter().find(|s| s.stage_id == "dev").unwrap();
    assert_eq!(
        dev.who.as_ref().unwrap().model,
        otaq_core::agents::AgentModel::Default
    );

    // The flow drops Product spec's skip: the skip is ignored.
    let mut file = every_kind_file();
    for stage in &mut file.flow.stages {
        stage.options.clear();
    }
    w.board.use_flow_for_tests("SHOP", &file).await;
    let flow = w.board.effective_flow("SHOP").await.unwrap();
    assert!(w.board.ticket_skips(&t, &flow).await.unwrap().is_empty());
    assert!(choices(&w, &t).await.stages.is_empty(), "nothing to skip");
    // And a flow without Product spec: its choice is ignored too.
    let mut file = every_kind_file();
    file.flow.stages.retain(|s| s.id != "product-spec");
    w.board.use_flow_for_tests("SHOP", &file).await;
    let left = choices(&w, &t).await;
    assert_eq!(
        left.stages
            .iter()
            .map(|s| s.stage.as_str())
            .collect::<Vec<_>>(),
        ["QA"]
    );
    let flow = w.board.effective_flow("SHOP").await.unwrap();
    assert!(w.board.ticket_skips(&t, &flow).await.unwrap().is_empty());

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

/// Product spec skipped: the start waits waiting to begin like any, then goes straight into Dev, on Dev's own model
/// (its session named by its tool); Product spec has a run of result `skipped`, and its choice is locked now. The
/// card's mark reads "Skipped Product spec · this ticket's choice".
#[tokio::test(flavor = "multi_thread")]
async fn a_skipped_product_spec_starts_in_dev() {
    let w = TestService::start_with(launch).await;
    w.add_project("shop-web").await;
    w.board.use_flow_for_tests("SHOP", &every_kind_file()).await;
    let t = w.create("SHOP", "Fix README typo").await;
    choose(
        &w,
        &t,
        &[pick("Product spec", "skip", None)],
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
        [("product-spec", Some(RunResult::Skipped)), ("dev", None)]
    );
    let agent: Option<String> = sqlx::query_scalar(
        "SELECT agent_name FROM sessions WHERE ticket_id = ? AND kind = 'agent' ORDER BY started_at DESC",
    )
    .bind(&t.id)
    .fetch_one(w.board.store().pool())
    .await
    .unwrap();
    assert_eq!(agent.as_deref(), Some("Claude Code"));
    let now = choices(&w, &t).await;
    let locked: Vec<(&str, bool)> = now
        .stages
        .iter()
        .map(|s| (s.stage.as_str(), s.locked))
        .collect();
    assert_eq!(locked, [("Product spec", true), ("QA", false)]);
    assert!(now.stages[0].skipped());
    assert_eq!(
        skipped_mark("Product spec"),
        "Skipped Product spec · this ticket's choice"
    );
    w.stop().await;
}

/// QA (a human check) skipped by the ticket's choice: Security's Pass walks past QA (a skipped run) into the
/// automatic PR Delivery, which delivers with no click, as the flow says. Without the skip the same Pass enters QA,
/// and QA's Pass delivers with no click too.
#[tokio::test(flavor = "multi_thread")]
async fn a_skipped_qa_reaches_done_as_the_flow_says() {
    let w = world().await;
    let t = w.create("SHOP", "Fix README typo").await;
    choose(&w, &t, &[pick("QA", "skip", None)], &Chooser::User, false)
        .await
        .unwrap();
    let flow = w.board.effective_flow("SHOP").await.unwrap();
    let ship = &flow.delivery;
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
        (Place::stage_of("qa"), Status::ToDeliver, true),
        "delivered automatically, as the flow says, in the last stage"
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

/// On the default flow (no stage lists options, UX-72) a ticket has no choice: picking another agent or model for Dev
/// is refused, as is skipping it; every ticket runs Dev on its own model.
#[tokio::test(flavor = "multi_thread")]
async fn on_the_default_flow_every_ticket_runs_dev_on_its_model() {
    let w = TestService::start_with(launch).await;
    let project = w.add_project("pocket-notes").await.key;
    let flow = w.board.effective_flow(&project).await.unwrap();
    assert!(flow.stages.iter().all(|s| s.options.is_empty()));
    assert!(
        w.board
            .new_ticket_choices(&project)
            .await
            .unwrap()
            .stages
            .is_empty()
    );
    let one = w.create(&project, "Sort notes").await;
    let other = w.create(&project, "Tag notes").await;
    for (word, refused) in [
        (
            "Strong",
            "Dev has no choices: every ticket runs it as the flow says",
        ),
        (
            "skip",
            "Dev can't be skipped; add `skip` to its options in the flow to allow it.",
        ),
    ] {
        let e = w
            .board
            .check_choices(
                &project,
                Some(&one),
                "",
                &[pick("Dev", word, None)],
                &Chooser::User,
                false,
            )
            .await
            .unwrap_err();
        assert_eq!(e.to_string(), refused);
    }
    for t in [&one, &other] {
        let key = t.key.to_string();
        w.board
            .move_ticket(&key, DropTarget::ReadyForDev, true, &me())
            .await
            .unwrap();
        w.starter
            .start(&key, &me(), otaq_core::session::Start::Auto)
            .await
            .unwrap();
        until_status(&w, &key, Status::Working).await;
        let ran: (Option<String>, Option<String>) = sqlx::query_as(
            "SELECT agent_name, runs_for FROM sessions WHERE ticket_id = ? AND kind = 'agent' ORDER BY started_at DESC",
        )
        .bind(&t.id)
        .fetch_one(w.board.store().pool())
        .await
        .unwrap();
        assert_eq!(
            ran,
            (
                Some("Claude Code".to_owned()),
                Some(format!("stage/{project}/dev"))
            ),
            "{key}"
        );
    }
    w.stop().await;
}

/// An older flow whose check lists several agents (`tests/flows/older/several-check-agents.yaml`, Security here skippable)
/// splits when it is read: the project's board runs Security and "Security · Validator" one after the other. The
/// tickets carry over once: a skip of Security skips both, a move past Security is past both, and Security's fails
/// in a row and extra rounds start at 0. The stored version is never rewritten; the user's next change saves the
/// split. Back to the previous flow on the old version gives the same split.
#[tokio::test(flavor = "multi_thread")]
async fn an_older_flow_splits_its_check_and_carries_the_tickets_over() {
    use otaq_core::domain::validation::MovedPast;
    use otaq_core::flow::Flow;
    use otaq_core::flow::apply::ProposeFrom;
    use otaq_core::flow::proposal::ApplyChoices;

    let w = TestService::start().await;
    w.add_project("shop-web").await;
    // Before the upgrade: Security is one stage, skippable.
    let mut file = every_kind_file();
    let skippable = |file: &mut FlowFile| {
        let security = file
            .flow
            .stages
            .iter_mut()
            .find(|s| s.id == "security")
            .unwrap();
        security.options = vec![StageOption::Skip];
    };
    skippable(&mut file);
    w.board.use_flow_for_tests("SHOP", &file).await;
    let skipped = w.create("SHOP", "Skips Security").await;
    choose(
        &w,
        &skipped,
        &[pick("Security", "skip", None)],
        &Chooser::User,
        false,
    )
    .await
    .unwrap();
    let moved = w.create("SHOP", "Moved past Security").await;
    let failing = w.create("SHOP", "Failed Security twice").await;
    let pool = w.board.store().pool();
    sqlx::query(
        "INSERT INTO check_counters (ticket_id, stage_id, fails, moved_past, updated_at) VALUES (?, 'security', 1, 'skipped', 1)",
    )
    .bind(&moved.id)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO check_counters (ticket_id, stage_id, fails, extra, updated_at) VALUES (?, 'security', 2, 1, 1)",
    )
    .bind(&failing.id)
    .execute(pool)
    .await
    .unwrap();
    // The flow as an older build stored it: Security lists two agents.
    let old = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/flows/older/several-check-agents.yaml"),
    )
    .unwrap()
    .replace(
        "    agents: [Security reviewer, Validator]\n",
        "    agents: [Security reviewer, Validator]\n    options: [skip]\n",
    );
    let old_version = w.board.current_flow("SHOP").await.unwrap().version;
    sqlx::query("UPDATE project_flows SET yaml = ? WHERE current")
        .bind(&old)
        .execute(pool)
        .await
        .unwrap();
    // The service starts; a second start carries nothing again (the failing ticket's new fail stays).
    w.board.carry_split_checks().await.unwrap();
    sqlx::query(
        "UPDATE check_counters SET fails = 1 WHERE ticket_id = ? AND stage_id = 'security'",
    )
    .bind(&failing.id)
    .execute(pool)
    .await
    .unwrap();
    w.board.carry_split_checks().await.unwrap();
    let count = w
        .board
        .check_counts(&failing.id)
        .await
        .unwrap()
        .of("security");
    assert_eq!(count.fails, 1);
    sqlx::query(
        "UPDATE check_counters SET fails = 0 WHERE ticket_id = ? AND stage_id = 'security'",
    )
    .bind(&failing.id)
    .execute(pool)
    .await
    .unwrap();

    let split = yaml::read(&old).unwrap().flow;
    let flow = w.board.effective_flow("SHOP").await.unwrap();
    assert_eq!(flow, split);
    let security = |flow: &Flow| -> Vec<(String, String)> {
        flow.stages
            .iter()
            .filter(|s| s.id.starts_with("security"))
            .map(|s| (s.id.clone(), s.name.clone()))
            .collect()
    };
    // Each copy is its own column, as every stage is.
    assert_eq!(
        security(&flow),
        [
            ("security".to_owned(), "Security".to_owned()),
            (
                "security-validator".to_owned(),
                "Security · Validator".to_owned(),
            ),
        ]
    );
    let skipped = w.board.ticket(&skipped.key.to_string()).await.unwrap();
    assert_eq!(
        w.board.ticket_skips(&skipped, &flow).await.unwrap(),
        ["security", "security-validator"]
    );
    let counts = w.board.check_counts(&moved.id).await.unwrap();
    assert_eq!(
        counts.of("security-validator").moved_past,
        Some(MovedPast::Skipped)
    );
    assert_eq!(counts.of("security").moved_past, Some(MovedPast::Skipped));
    let counts = w.board.check_counts(&failing.id).await.unwrap();
    for stage in ["security", "security-validator"] {
        let count = counts.of(stage);
        assert_eq!((count.fails, count.extra), (0, 0), "{stage}");
    }
    // Stored versions are never rewritten by reading.
    assert_eq!(w.board.current_flow("SHOP").await.unwrap().yaml, old);

    // The user's next change saves the split.
    w.board
        .edit_flow_for_tests("SHOP", |file| {
            file.flow
                .stages
                .iter_mut()
                .find(|s| s.id == "validate")
                .unwrap()
                .rounds = Some(2);
        })
        .await;
    let saved = w.board.current_flow("SHOP").await.unwrap().yaml;
    assert!(!saved.contains("agents: ["), "{saved}");
    assert!(saved.contains("id: security-validator"), "{saved}");
    assert_eq!(
        security(&w.board.effective_flow("SHOP").await.unwrap()),
        security(&split)
    );

    // Back to the previous flow on the old version: the same split.
    let back = w
        .board
        .propose_from(
            "SHOP",
            &ProposeFrom::Previous {
                version: Some(old_version),
            },
            &otaq_core::agents::FixedModels::sample(),
        )
        .await
        .unwrap();
    assert_eq!(yaml::read(&back.yaml).unwrap().flow, split);
    w.board
        .apply_proposal(
            "SHOP",
            back.id,
            &ApplyChoices::default(),
            &otaq_core::agents::FixedModels::sample(),
        )
        .await
        .unwrap();
    assert_eq!(w.board.effective_flow("SHOP").await.unwrap(), split);
    let skipped = w.board.ticket(&skipped.key.to_string()).await.unwrap();
    assert_eq!(
        w.board.ticket_skips(&skipped, &split).await.unwrap(),
        ["security", "security-validator"]
    );
    w.stop().await;
}

/// What the full new-ticket form sends, in its order (the ticket with its attachments as its first comment, then its
/// Waits for, then the stages you skip): the ticket shows each. Its choices are only the stages the flow lets tickets
/// skip (Product spec, never Dev), with what each stage is for.
#[tokio::test(flavor = "multi_thread")]
async fn a_ticket_from_the_new_ticket_form_shows_each_of_its_settings() {
    use otaq_core::board::reports::ReportSource;
    use otaq_core::board::ticket_comments::FirstComment;
    use otaq_core::board::tickets::NewTicketInput;

    let w = world().await;
    let parent = w.create("SHOP", "Wishlist share link").await;
    let shot = tempfile::NamedTempFile::with_suffix(".png").unwrap();
    std::fs::write(shot.path(), [0x89, b'P', b'N', b'G']).unwrap();
    let t = w
        .board
        .create_ticket(
            &NewTicketInput {
                project: "SHOP".into(),
                title: "Coupon field rejects lowercase codes".into(),
                first_comment: Some(FirstComment {
                    attachments: vec![ReportSource::Path(shot.path().to_path_buf())],
                    ..FirstComment::default()
                }),
                ..Default::default()
            },
            &me(),
        )
        .await
        .unwrap();
    let key = t.key.to_string();
    w.board
        .add_dependency(&key, &parent.key.to_string(), &me())
        .await
        .unwrap();
    choose(
        &w,
        &t,
        &[pick("Product spec", "skip", None)],
        &Chooser::User,
        false,
    )
    .await
    .unwrap();

    let first = w.board.ticket_comment(&key, 1).await.unwrap();
    assert!(first.at_creation);
    assert_eq!(first.attachments.len(), 1);
    let tree = w.board.dependency_tree(&key).await.unwrap();
    assert_eq!(
        tree.parents
            .iter()
            .map(|p| p.key.to_string())
            .collect::<Vec<_>>(),
        [parent.key.to_string()]
    );
    assert_eq!(choice_of(&w, &t, "Product spec").await.0, "skip");

    let offered = w.board.new_ticket_choices("SHOP").await.unwrap();
    let spec = offered
        .stages
        .iter()
        .find(|s| s.stage == "Product spec")
        .unwrap();
    assert_eq!(spec.words(), ["required", "skip"]);
    assert!(
        !offered.stages.iter().any(|s| s.stage == "Dev"),
        "Dev can't be skipped: no choice"
    );
    w.stop().await;
}
