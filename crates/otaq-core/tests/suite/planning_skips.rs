//! Planning skips on a flow shaped like the Spec first preset (T-0547; 0129 row 4 and "Noyan sets the skips", 0108
//! second-gap rule 2, 0131): Product spec, Tech spec and Milestone plan (each `skip`) → Spec review (a skippable
//! check by you, its fails to Product spec) → Dev → Validate → Code review → Done. A ticket that skips every planning
//! stage skips Spec review on its own; one planning stage left on runs it; a set of choices that leaves a check that
//! must run with nothing before it is refused; a Send back goes to the nearest planning stage the ticket runs.

use otaq_core::board::BoardError;
use otaq_core::board::Refusal;
use otaq_core::board::human_check::Arrival;
use otaq_core::board::stage_choices::{ChoicePick, ChoiceWrite, Chooser};
use otaq_core::board::tickets::Ticket;
use otaq_core::domain::{Actor, Place, Status};
use otaq_core::flow::check::{Rule, check};
use otaq_core::flow::engine::{self, At, Move, RunResult};
use otaq_core::flow::{FlowFile, StageOption, yaml};
use otaq_core::service::testing::TestService;

/// The Spec first preset's shape (its file is T-0545's `flows/spec-first.yaml`).
const SPEC_FIRST: &str = "\
agents:
  - name: Spec writer
    skills: [product-spec]
    model: default
  - name: Architect
    skills: [tech-spec]
    model: default
  - name: Milestone planner
    skills: [milestone-plan]
    model: default
  - name: Developer
    skills: [work-ticket, commit]
    model: default
  - name: Validator
    skills: [validate]
    model: default
flow:
  - id: product-spec
    name: Product spec
    kind: agent
    column: own
    agent: Spec writer
    skill: product-spec
    options: [skip]
  - id: tech-spec
    name: Tech spec
    kind: agent
    column: own
    agent: Architect
    skill: tech-spec
    options: [skip]
  - id: milestone-plan
    name: Milestone plan
    kind: agent
    column: own
    agent: Milestone planner
    skill: milestone-plan
    options: [skip]
  - id: spec-review
    name: Spec review
    kind: check
    by: you
    column: own
    options: [skip]
    fail_to: Product spec
  - id: dev
    name: Dev
    kind: agent
    column: own
    agent: Developer
    skill: work-ticket
  - id: validate
    name: Validate
    kind: check
    by: agent
    column: own
    agents: [Validator]
    skill: validate
    rounds: 3
    fail_to: Dev
  - id: code-review
    name: Code review
    kind: check
    by: you
    column: own
    fail_to: Dev
  - id: deliver
    name: Done
    kind: delivery
    column: with-previous
    agent: Developer
    skill: open-pr
    mode: click
    delivers: either
";

const PLANNING: [&str; 3] = ["Product spec", "Tech spec", "Milestone plan"];
const REFUSED: &str = "Spec review needs one of Product spec, Tech spec or Milestone plan to run; it can't be skipped on this flow";

fn spec_first() -> FlowFile {
    yaml::read(SPEC_FIRST).unwrap()
}

fn me() -> Actor {
    Actor::User("me".into())
}

fn noyan() -> Chooser {
    Chooser::ProjectAgent {
        name: "Checkout slowness".into(),
    }
}

fn pick(stage: &str, choice: &str, reason: Option<&str>) -> ChoicePick {
    ChoicePick {
        stage: stage.into(),
        choice: choice.into(),
        reason: reason.map(Into::into),
    }
}

/// Noyan's skips on an ordinary ticket (0129): every planning stage, "ordinary work".
fn skip_planning() -> Vec<ChoicePick> {
    PLANNING
        .iter()
        .map(|s| pick(s, "skip", Some("ordinary work")))
        .collect()
}

async fn world(file: &FlowFile) -> TestService {
    let w = TestService::start().await;
    w.add_project("shop-web").await;
    w.board.use_flow_for_tests("SHOP", file).await;
    w
}

/// Checks and writes `picks` on `ticket` as `chooser` would (the service's `ticket.update`, the sheet's pickers).
async fn choose(
    w: &TestService,
    ticket: &Ticket,
    picks: &[ChoicePick],
    chooser: &Chooser,
) -> Result<Vec<ChoiceWrite>, BoardError> {
    let ticket = w.board.ticket(&ticket.key.to_string()).await?;
    let writes = w
        .board
        .check_choices("SHOP", Some(&ticket), "", picks, chooser, false)
        .await?;
    w.board.write_choices(&ticket, &writes, &me()).await?;
    Ok(writes)
}

async fn skips(w: &TestService, t: &Ticket) -> Vec<String> {
    let flow = w.board.effective_flow("SHOP").await.unwrap();
    w.board.ticket_skips(t, &flow).await.unwrap()
}

/// A run of `stage` on `t`, ended with `result` (`None`: still going), each a millisecond after the one before and all
/// in the past, so a run the `stage_runs_follow_tickets` trigger opens on a move comes after them.
async fn ran(w: &TestService, t: &Ticket, stage: &str, result: Option<RunResult>) {
    let runs: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM stage_runs WHERE ticket_id = ?")
        .bind(&t.id)
        .fetch_one(w.board.store().pool())
        .await
        .unwrap();
    let now = otaq_core::store::now_ms() - 60_000 + runs;
    sqlx::query(
        "INSERT INTO stage_runs (id, ticket_id, stage_id, kind, result, started_at, ended_at)
         VALUES (?, ?, ?, 'agent', ?, ?, ?)",
    )
    .bind(otaq_core::store::new_id())
    .bind(&t.id)
    .bind(stage)
    .bind(result.map(|r| r.as_str()))
    .bind(now)
    .bind(result.map(|_| now))
    .execute(w.board.store().pool())
    .await
    .unwrap();
}

/// 0129 row 4 refines 0108's second-gap rule 2: Spec first validates (its Spec review is skippable); the same check
/// made required after only skippable stages is still refused.
#[test]
fn spec_first_validates_and_a_required_check_after_skippable_stages_does_not() {
    let file = spec_first();
    assert_eq!(check(&file.flow), Ok(()));

    let mut required = file.flow.clone();
    let review = required
        .stages
        .iter_mut()
        .find(|s| s.id == "spec-review")
        .unwrap();
    review.options.retain(|o| *o != StageOption::Skip);
    let problems = check(&required).unwrap_err();
    assert_eq!(problems.len(), 1, "{problems:?}");
    assert_eq!(problems[0].rule, Rule::SkipStrandsHumanCheck);
    assert_eq!(problems[0].stage.as_deref(), Some("spec-review"));
    assert_eq!(
        problems[0].message,
        "If Product spec, Tech spec and Milestone plan are all skipped, a Fail at Spec review has nowhere to go"
    );
}

/// Noyan's ordinary ticket skips the three planning stages: Spec review skips on its own. The walk from Ready goes
/// straight to Dev past all four, the choices say "Spec review: skipped · nothing before it runs", and a card that
/// waits in a check right after it reads "Skipped Spec review · nothing before it runs".
#[tokio::test(flavor = "multi_thread")]
async fn skipping_every_planning_stage_skips_spec_review() {
    let w = world(&spec_first()).await;
    let t = w.create("SHOP", "Fix cart total rounding").await;
    choose(&w, &t, &skip_planning(), &noyan()).await.unwrap();

    assert_eq!(
        skips(&w, &t).await,
        ["product-spec", "tech-spec", "milestone-plan", "spec-review"]
    );
    let flow = w.board.effective_flow("SHOP").await.unwrap();
    assert_eq!(
        w.board.skipped_on_its_own(&t, &flow).await.unwrap(),
        ["spec-review"]
    );
    let choices = w.board.ticket_choices("SHOP-1").await.unwrap();
    let on_its_own: Vec<(&str, bool)> = choices
        .stages
        .iter()
        .map(|s| (s.stage.as_str(), s.on_its_own))
        .collect();
    assert_eq!(
        on_its_own,
        [
            ("Product spec", false),
            ("Tech spec", false),
            ("Milestone plan", false),
            ("Spec review", true)
        ]
    );

    // Ready → Dev, the four walked over with skipped runs.
    let skipping = skips(&w, &t).await;
    let ready = Place::Waiting(None);
    let begin = engine::step(
        At {
            skips: &skipping,
            ..At::new(&flow, &ready, Status::Queued)
        },
        Move::Begin,
    )
    .unwrap();
    assert_eq!(begin.place, Place::stage_of("dev"));
    let through: Vec<(&str, RunResult)> = begin
        .passed_through
        .iter()
        .map(|(s, r)| (s.id.as_str(), *r))
        .collect();
    assert_eq!(
        through,
        [
            ("product-spec", RunResult::Skipped),
            ("tech-spec", RunResult::Skipped),
            ("milestone-plan", RunResult::Skipped),
            ("spec-review", RunResult::Skipped),
        ]
    );

    // The arrival line, on a flow with a second skippable check right after Spec review (a card dragged into it).
    let mut file = spec_first();
    let mut look = file.flow.stage("spec-review").unwrap().clone();
    look.id = "final-look".into();
    look.name = "Final look".into();
    let at = file.flow.position("dev").unwrap();
    file.flow.stages.insert(at, look);
    assert_eq!(check(&file.flow), Ok(()));
    w.board.use_flow_for_tests("SHOP", &file).await;
    for stage in ["product-spec", "tech-spec", "milestone-plan", "spec-review"] {
        ran(&w, &t, stage, Some(RunResult::Skipped)).await;
    }
    // The move opens Final look's run (the `stage_runs_follow_tickets` trigger), after the skipped ones.
    sqlx::query("UPDATE tickets SET stage_id = 'final-look', state = ? WHERE id = ?")
        .bind(Status::WaitingForYou.as_str())
        .bind(&t.id)
        .execute(w.board.store().pool())
        .await
        .unwrap();
    let arrival = w.board.arrival("SHOP-1").await.unwrap().unwrap();
    assert_eq!(
        arrival,
        Arrival::SkippedOnItsOwn {
            stage: "Spec review".into()
        }
    );
    assert_eq!(
        arrival.label(),
        "Skipped Spec review \u{b7} nothing before it runs"
    );
    let listed = w.board.tickets("SHOP").await.unwrap();
    assert_eq!(listed[0].flags.arrival.as_ref(), Some(&arrival));
    w.stop().await;
}

/// One planning stage left on, Spec review runs; a drag back into a skipped planning stage runs it once, and Spec
/// review then runs too.
#[tokio::test(flavor = "multi_thread")]
async fn one_planning_stage_left_on_runs_spec_review() {
    let w = world(&spec_first()).await;
    let t = w.create("SHOP", "Wishlist").await;
    let picks = [
        pick("Product spec", "skip", Some("settled")),
        pick("Tech spec", "skip", Some("settled")),
    ];
    choose(&w, &t, &picks, &noyan()).await.unwrap();
    assert_eq!(skips(&w, &t).await, ["product-spec", "tech-spec"]);
    let flow = w.board.effective_flow("SHOP").await.unwrap();
    let skipping = skips(&w, &t).await;
    let in_plan = Place::stage_of("milestone-plan");
    let next = engine::step(
        At {
            skips: &skipping,
            ..At::new(&flow, &in_plan, Status::Working)
        },
        Move::NextStage,
    )
    .unwrap();
    assert_eq!(
        (next.place, next.status),
        (Place::stage_of("spec-review"), Status::WaitingForYou)
    );

    // Every planning stage skipped, then Product spec run by a drag back: Spec review runs.
    let other = w.create("SHOP", "Saved carts").await;
    choose(&w, &other, &skip_planning(), &noyan())
        .await
        .unwrap();
    assert!(skips(&w, &other).await.contains(&"spec-review".to_owned()));
    ran(&w, &other, "product-spec", None).await;
    assert_eq!(
        skips(&w, &other).await,
        ["product-spec", "tech-spec", "milestone-plan"]
    );
    w.stop().await;
}

/// A set of choices that leaves a check that must run with nothing before it is refused, naming the stages: Spec
/// review chosen to run ("required") with every planning stage skipped, in one request (the ⌘N form's batch, a new
/// ticket) or one picker at a time (the sheet); and a check required by the flow. Nothing is written.
#[tokio::test(flavor = "multi_thread")]
async fn a_set_of_skips_that_strands_a_check_is_refused() {
    let w = world(&spec_first()).await;
    let mut picks = skip_planning();
    picks.push(pick("Spec review", "required", Some("ordinary work")));
    // The ⌘N form: a ticket about to be created.
    let refused = w
        .board
        .check_choices(
            "SHOP",
            None,
            "the new ticket",
            &picks,
            &Chooser::User,
            false,
        )
        .await
        .unwrap_err();
    assert_eq!(refused.refusal(), Some(Refusal::StrandsCheck));
    assert_eq!(refused.to_string(), REFUSED);

    // The sheet's pickers, one at a time: Spec review set to run, then the planning stages skipped.
    let t = w.create("SHOP", "Fix cart total rounding").await;
    choose(
        &w,
        &t,
        &[pick("Spec review", "required", None)],
        &Chooser::User,
    )
    .await
    .unwrap();
    choose(
        &w,
        &t,
        &[pick("Product spec", "skip", None)],
        &Chooser::User,
    )
    .await
    .unwrap();
    choose(&w, &t, &[pick("Tech spec", "skip", None)], &Chooser::User)
        .await
        .unwrap();
    let refused = choose(
        &w,
        &t,
        &[pick("Milestone plan", "skip", None)],
        &Chooser::User,
    )
    .await
    .unwrap_err();
    assert_eq!(refused.to_string(), REFUSED);
    assert_eq!(skips(&w, &t).await, ["product-spec", "tech-spec"]);

    // While Spec review skips on its own, it can't be set to run.
    let u = w.create("SHOP", "Saved carts").await;
    choose(&w, &u, &skip_planning(), &noyan()).await.unwrap();
    let refused = choose(
        &w,
        &u,
        &[pick("Spec review", "required", None)],
        &Chooser::User,
    )
    .await
    .unwrap_err();
    assert_eq!(refused.to_string(), REFUSED);

    // A check the flow requires (Spec review without `skip`; the flow check refuses this flow, so only a stored
    // flow from before gets here).
    let mut file = spec_first();
    for stage in file
        .flow
        .stages
        .iter_mut()
        .filter(|s| s.id == "spec-review")
    {
        stage.options.clear();
    }
    w.board.use_flow_for_tests("SHOP", &file).await;
    let v = w.create("SHOP", "Gift cards").await;
    let refused = choose(&w, &v, &skip_planning(), &noyan())
        .await
        .unwrap_err();
    assert_eq!(refused.to_string(), REFUSED);
    w.stop().await;
}

/// 0131: a Send back at Spec review on a ticket that skips Product spec (its fails-go-to stage) goes to the nearest
/// planning stage before Spec review that the ticket runs.
#[tokio::test(flavor = "multi_thread")]
async fn a_send_back_goes_to_the_nearest_planning_stage_the_ticket_runs() {
    let w = world(&spec_first()).await;
    let flow = w.board.effective_flow("SHOP").await.unwrap();
    let send_back = |skipping: &[String]| {
        let place = Place::stage_of("spec-review");
        let step = engine::step(
            At {
                skips: skipping,
                ..At::new(&flow, &place, Status::WaitingForYou)
            },
            Move::HumanFail,
        )
        .unwrap();
        assert!(step.resume);
        step.place.stage().unwrap().to_owned()
    };
    let all = w.create("SHOP", "Wishlist").await;
    assert_eq!(send_back(&skips(&w, &all).await), "product-spec");

    let t = w.create("SHOP", "Saved carts").await;
    choose(
        &w,
        &t,
        &[pick("Product spec", "skip", Some("settled"))],
        &noyan(),
    )
    .await
    .unwrap();
    assert_eq!(send_back(&skips(&w, &t).await), "milestone-plan");

    let u = w.create("SHOP", "Gift cards").await;
    let picks = [
        pick("Product spec", "skip", Some("settled")),
        pick("Milestone plan", "skip", Some("one piece")),
    ];
    choose(&w, &u, &picks, &noyan()).await.unwrap();
    assert_eq!(send_back(&skips(&w, &u).await), "tech-spec");
    w.stop().await;
}
