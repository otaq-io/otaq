//! Stage choices (T-0407; AGM-08, AGM-11, AGM-13; d0087) on the test flow `tests/flows/every-kind.yaml`: Product spec
//! (skippable), Dev, Validate, Security, QA (a check by you, skippable) and an automatic PR Delivery. A stage lists no
//! agents: any stage an agent runs may run any agent whose tool has its skill. Who may set what (only the user, or
//! Noyan passing on the user's words), the locks, the skipped stages and the Delivery after a skipped human check. On
//! the default flow too the user picks Dev's agent. `claude` is stood in for by an ordinary command.

use std::path::Path;
use std::time::Duration;

use otaq_core::agent_tools::LaunchContext;
use otaq_core::agent_tools::Tool;
use otaq_core::agent_tools::claude::ClaudeLaunch;
use otaq_core::agents::AgentModel;
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
/// refused; a ticket's own agent sets none (only the user picks an agent or skips a stage), and only Noyan passes on
/// the user's words; the user picks any agent whose tool has the stage's skill for any stage an agent runs, even one
/// the flow lists nothing for (Security), and an agent whose tool lacks the skill is refused saying so; a skip of a
/// stage the flow doesn't mark skippable is refused; a choice locks when its stage starts.
#[tokio::test(flavor = "multi_thread")]
async fn who_may_set_what() {
    let w = world().await;

    // Noyan at creation (a batch about to create it): its own choice is refused, with a reason or not, `required`
    // and `skip` too (T1).
    let label = "the new ticket \"Fix README typo\"";
    for own in [
        pick("Dev", "quick developer", Some("typo fix")),
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
            &[pick("Dev", "quick developer", None)],
            &planner(),
            true,
        )
        .await
        .unwrap();
    let typo = w.create("SHOP", "Fix README typo").await;
    w.board.write_choices(&typo, &writes, &me()).await.unwrap();
    assert_eq!(
        choice_of(&w, &typo, "Dev").await,
        ("Quick developer".to_owned(), Some(SetBy::User))
    );

    // A ticket's own agent sets no choice, before the ticket starts or after, with a reason or not: picking an agent
    // for a later stage would move work to a tool the user didn't choose. Nothing is written.
    at_work_in(&w, &typo, "product-spec").await;
    for chooser in [architect(), spec_writer()] {
        for (picked, refused) in [
            (
                pick("Validate", "Quick developer", Some("risky change")),
                ONLY_THE_USER_PICKS,
            ),
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
                refusal(
                    choose(
                        &w,
                        &typo,
                        &[pick("Validate", "Quick developer", words)],
                        &chooser,
                        true,
                    )
                    .await
                ),
                (Refusal::NotAllowed, ONLY_NOYAN_PASSES_ON.into())
            );
        }
    }
    assert_eq!(choice_of(&w, &typo, "Validate").await.1, None);
    assert_eq!(choice_of(&w, &typo, "QA").await.1, None);

    // The user picks any agent whose tool has the stage's skill, for any stage an agent runs: Validate and Security
    // list no agents in the flow. History says who chose.
    let fresh = w.create("SHOP", "Export as CSV").await;
    choose(
        &w,
        &fresh,
        &[
            pick("Validate", "Security reviewer", None),
            pick("Security", "Validator", None),
        ],
        &Chooser::User,
        false,
    )
    .await
    .unwrap();
    assert_eq!(
        choice_of(&w, &fresh, "Security").await,
        ("Validator".to_owned(), Some(SetBy::User))
    );
    let history = w.board.history(&fresh.key.to_string()).await.unwrap();
    let set = history
        .iter()
        .find(|e| e.kind == otaq_core::board::tickets::EventKind::StageChoiceSet)
        .unwrap();
    assert_eq!(set.payload["by"], "you");
    // An agent whose tool lacks the stage's skill isn't offered, and picking it is refused saying so: nobody has
    // `/team-security-review`, and Quick developer now runs on Codex.
    w.set_agent_model(
        "Quick developer",
        AgentModel::Tool {
            tool: Tool::Codex,
            model: None,
        },
    )
    .await;
    let offered = choices(&w, &fresh).await;
    let security = offered
        .stages
        .iter()
        .find(|s| s.stage == "Security")
        .unwrap();
    assert!(
        !security.agents.iter().any(|a| a.name == "Quick developer"),
        "{security:?}"
    );
    let dev = offered.stages.iter().find(|s| s.stage == "Dev").unwrap();
    assert!(
        dev.agents
            .iter()
            .any(|a| a.name == "Quick developer" && a.tool == Some(Tool::Codex)),
        "Dev runs one of Otaq's own skills, which every tool has: {dev:?}"
    );
    assert_eq!(
        refusal(
            choose(
                &w,
                &fresh,
                &[pick("Security", "Quick developer", None)],
                &Chooser::User,
                false
            )
            .await
        ),
        (
            Refusal::NotAnOption,
            "Quick developer runs on Codex, which doesn't have the skill `/team-security-review` for Security".into()
        )
    );
    // Not a choice of the stage: refused naming them; a stage that runs no agent offers only its own; Dev isn't
    // skippable; an unknown stage lists the stages with choices.
    let dev_refused = refusal(
        choose(
            &w,
            &fresh,
            &[pick("Dev", "Spec typist", None)],
            &Chooser::User,
            false,
        )
        .await,
    );
    assert_eq!(dev_refused.0, Refusal::NotAnOption);
    assert!(
        dev_refused
            .1
            .starts_with("Spec typist isn't one of Dev's choices: Rust developer, "),
        "{}",
        dev_refused.1
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
        "QA runs no agent: its choices are required (you check it) and skip"
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
        "SHOP's flow has no stage Lint (stages with choices: Product spec, Dev, Validate, Security, QA)"
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
                &[pick("Dev", "Validator", None)],
                &Chooser::User,
                false
            )
            .await
        ),
        (
            Refusal::ChoiceLocked,
            "Dev has started on SHOP-2: its choice is locked".into()
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

    // Noyan passes on the user's skip and agent pick: both stored as the user's.
    let noyan = w.create("SHOP", "Rename the export button").await;
    choose(
        &w,
        &noyan,
        &[pick("QA", "skip", None), pick("Dev", "Validator", None)],
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
        choice_of(&w, &noyan, "Dev").await,
        ("Validator".to_owned(), Some(SetBy::User))
    );
    w.stop().await;
}

/// AGM-05 and AGM-08: the running agent is the choice, else the stage's agent; a skip of a stage no longer
/// skippable, or a choice for a stage the flow no longer has, is ignored.
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
            pick("Product spec", "skip", None),
        ],
        &Chooser::User,
        false,
    )
    .await
    .unwrap();
    assert_eq!(running(t.clone(), "dev").await, "Quick developer");
    let flow = w.board.effective_flow("SHOP").await.unwrap();
    assert_eq!(
        w.board.ticket_skips(&t, &flow).await.unwrap(),
        ["product-spec"]
    );
    let agents = otaq_core::agents::facts::ticket_agents(&w.board, "SHOP-1")
        .await
        .unwrap();
    let dev = agents.stages.iter().find(|s| s.stage_id == "dev").unwrap();
    assert_eq!(dev.agent.as_ref().unwrap().name, "Quick developer");

    // The flow drops Product spec's skip: the skip is ignored; Dev's pick stays (a stage lists no agents).
    let mut file = every_kind_file();
    for stage in &mut file.flow.stages {
        stage.options.clear();
    }
    w.board.use_flow_for_tests("SHOP", &file).await;
    assert_eq!(running(t.clone(), "dev").await, "Quick developer");
    let flow = w.board.effective_flow("SHOP").await.unwrap();
    assert!(w.board.ticket_skips(&t, &flow).await.unwrap().is_empty());
    let left = choices(&w, &t).await;
    let spec = left
        .stages
        .iter()
        .find(|s| s.stage == "Product spec")
        .unwrap();
    assert!(!spec.can_skip && spec.choice.is_none(), "{spec:?}");
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
        ["Dev", "Validate", "Security", "QA"]
    );
    assert_eq!(left.stages[0].label(), "Quick developer");
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

/// Product spec skipped and Dev on Quick developer: the start waits waiting to begin like any, then goes straight into
/// Dev, where Quick developer runs; Product spec has a run of result `skipped`; both choices are locked now. The card's
/// mark reads "Skipped Product spec · this ticket's choice".
#[tokio::test(flavor = "multi_thread")]
async fn a_skipped_product_spec_starts_in_dev_on_the_chosen_agent() {
    let w = TestService::start_with(launch).await;
    w.add_project("shop-web").await;
    w.board.use_flow_for_tests("SHOP", &every_kind_file()).await;
    let t = w.create("SHOP", "Fix README typo").await;
    choose(
        &w,
        &t,
        &[
            pick("Product spec", "skip", None),
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
        [("product-spec", Some(RunResult::Skipped)), ("dev", None)]
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
    assert_eq!(
        locked,
        [
            ("Product spec", true),
            ("Dev", true),
            ("Validate", false),
            ("Security", false),
            ("QA", false)
        ]
    );
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

/// On the default flow (no stage lists options) the user sets Dev=Strong for one ticket and it runs on Strong; another
/// ticket keeps Dev's own agent. Dev isn't skippable, so its skip is refused.
#[tokio::test(flavor = "multi_thread")]
async fn on_the_default_flow_the_user_sets_dev_to_another_agent() {
    let w = TestService::start_with(launch).await;
    let project = w.add_project("pocket-notes").await.key;
    let flow = w.board.effective_flow(&project).await.unwrap();
    assert!(flow.stages.iter().all(|s| s.options.is_empty()));
    w.board
        .store()
        .insert_agent(
            &otaq_core::agents::store::NewAgent {
                name: "Strong".into(),
                instructions: String::new(),
                model: AgentModel::Default,
                effort: None,
            },
            &otaq_core::agents::CameFrom::You,
            None,
        )
        .await
        .unwrap();
    let new = w.board.new_ticket_choices(&project).await.unwrap();
    let dev = new.stages.iter().find(|s| s.stage_id == "dev").unwrap();
    assert!(dev.agents.iter().any(|a| a.name == "Strong"), "{dev:?}");
    let strong = w.create(&project, "Sort notes").await;
    let plain = w.create(&project, "Tag notes").await;
    let writes = w
        .board
        .check_choices(
            &project,
            Some(&strong),
            "",
            &[pick("Dev", "Strong", None)],
            &Chooser::User,
            false,
        )
        .await
        .unwrap();
    w.board
        .write_choices(&strong, &writes, &me())
        .await
        .unwrap();
    assert_eq!(choice_of(&w, &strong, "Dev").await.0, "Strong");
    let refused = w
        .board
        .check_choices(
            &project,
            Some(&plain),
            "",
            &[pick("Dev", "skip", None)],
            &Chooser::User,
            false,
        )
        .await
        .unwrap_err();
    assert_eq!(
        refused.to_string(),
        "Dev can't be skipped; add `skip` to its options in the flow to allow it."
    );
    for (t, runs) in [(&strong, "Strong"), (&plain, "Developer")] {
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
        let agent: Option<String> = sqlx::query_scalar(
            "SELECT agent_name FROM sessions WHERE ticket_id = ? AND kind = 'agent' ORDER BY started_at DESC",
        )
        .bind(&t.id)
        .fetch_one(w.board.store().pool())
        .await
        .unwrap();
        assert_eq!(agent.as_deref(), Some(runs), "{key}");
    }
    w.stop().await;
}

/// An older flow whose check lists several agents (`tests/flows/several-check-agents.yaml`, Security here skippable)
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
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/flows/several-check-agents.yaml"),
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
/// Waits for, then the stages you set): the ticket shows each. Its choices name every stage's own agent and offer
/// "skip" only where the flow lets tickets skip it (Product spec, never Dev), with what each stage is for.
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
        &[
            pick("Dev", "Quick developer", None),
            pick("Product spec", "skip", None),
        ],
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
    assert_eq!(choice_of(&w, &t, "Dev").await.0, "Quick developer");
    assert_eq!(choice_of(&w, &t, "Product spec").await.0, "skip");

    let offered = w.board.new_ticket_choices("SHOP").await.unwrap();
    let words = |stage: &str| {
        offered
            .stages
            .iter()
            .find(|s| s.stage == stage)
            .unwrap()
            .words()
    };
    assert!(words("Product spec").contains(&"skip".to_owned()));
    assert!(
        !words("Dev").contains(&"skip".to_owned()),
        "Dev can't be skipped"
    );
    w.stop().await;
}
