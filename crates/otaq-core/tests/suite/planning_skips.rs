//! Planning skips on a flow shaped like the Spec first preset (T-0547): Product spec, Tech spec and Milestone plan
//! (each `skip`) → Spec review (a skippable check by you, its fails to Product spec) → Dev → Validate → Code review →
//! Done. Only the user skips a stage: Noyan passes on the user's skips with `--by-user` and its own are refused (T1).
//! Nothing is skipped on its own: a ticket that skips every planning stage reaches Spec review and waits there (the
//! user skips it too if they want; its choice "required" means it happens as the flow says); a set of choices that
//! leaves a check the flow doesn't mark skippable with nothing before it is refused; a Send back goes to the nearest
//! planning stage the ticket runs.

use otaq_core::board::BoardError;
use otaq_core::board::Refusal;
use otaq_core::board::stage_choices::{ChoicePick, ChoiceWrite, Chooser, NOYAN_USER_CHOICES_ONLY};
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
    agent: Validator
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

/// The user's skips of every planning stage ("1 skips planning").
fn skip_planning() -> Vec<ChoicePick> {
    PLANNING.iter().map(|s| pick(s, "skip", None)).collect()
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
    choose_as(w, ticket, picks, chooser, false).await
}

/// Noyan passing on the user's `picks` (`--by-user`).
async fn passed_on(
    w: &TestService,
    ticket: &Ticket,
    picks: &[ChoicePick],
) -> Result<Vec<ChoiceWrite>, BoardError> {
    choose_as(w, ticket, picks, &noyan(), true).await
}

async fn choose_as(
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

/// Spec first validates (its Spec review is skippable); the same check made unskippable after only skippable stages
/// is refused.
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

/// Only the user skips a stage: Noyan's own skips of the planning stages are refused and change nothing (T1). With
/// the user's skips of the three, passed on by Noyan, nothing else is skipped: Spec review's choice still reads "required"
/// with its picker, the walk from Ready goes past the three into Spec review and waits there for the user, who can
/// Pass it or skip it too.
#[tokio::test(flavor = "multi_thread")]
async fn skipping_every_planning_stage_waits_in_spec_review() {
    let w = world(&spec_first()).await;
    let t = w.create("SHOP", "Fix cart total rounding").await;
    let own: Vec<ChoicePick> = PLANNING
        .iter()
        .map(|s| pick(s, "skip", Some("ordinary work")))
        .collect();
    let refused = choose(&w, &t, &own, &noyan()).await.unwrap_err();
    assert_eq!(refused.refusal(), Some(Refusal::NotAllowed));
    assert_eq!(refused.to_string(), NOYAN_USER_CHOICES_ONLY);
    assert!(skips(&w, &t).await.is_empty());

    let writes = passed_on(&w, &t, &skip_planning()).await.unwrap();
    assert!(writes.iter().all(|w| w.set_by.is_user()), "{writes:?}");

    assert_eq!(
        skips(&w, &t).await,
        ["product-spec", "tech-spec", "milestone-plan"]
    );
    let choices = w.board.ticket_choices("SHOP-1").await.unwrap();
    let review = choices
        .stages
        .iter()
        .find(|s| s.stage_id == "spec-review")
        .unwrap();
    assert_eq!(
        (review.label(), review.words(), review.choice.is_none()),
        (
            "required".to_owned(),
            vec!["required".to_owned(), "skip".to_owned()],
            true
        )
    );

    // Ready → Spec review, the three walked over with skipped runs; the ticket waits there for you.
    let flow = w.board.effective_flow("SHOP").await.unwrap();
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
    assert_eq!(
        (begin.place.clone(), begin.status),
        (Place::stage_of("spec-review"), Status::WaitingForYou)
    );
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
        ]
    );
    // Its Pass goes on to wait before Dev.
    let in_review = Place::stage_of("spec-review");
    let pass = engine::step(
        At {
            skips: &skipping,
            ..At::new(&flow, &in_review, Status::WaitingForYou)
        },
        Move::HumanPass,
    )
    .unwrap();
    assert_eq!(pass.place, Place::Waiting(Some("dev".into())));

    // The user skips Spec review too: allowed, and the walk goes straight to Dev.
    choose(&w, &t, &[pick("Spec review", "skip", None)], &Chooser::User)
        .await
        .unwrap();
    let skipping = skips(&w, &t).await;
    assert_eq!(
        skipping,
        ["product-spec", "tech-spec", "milestone-plan", "spec-review"]
    );
    let begin = engine::step(
        At {
            skips: &skipping,
            ..At::new(&flow, &ready, Status::Queued)
        },
        Move::Begin,
    )
    .unwrap();
    assert_eq!(begin.place, Place::stage_of("dev"));
    // And back: "required" sets it to run again.
    choose(
        &w,
        &t,
        &[pick("Spec review", "required", None)],
        &Chooser::User,
    )
    .await
    .unwrap();
    assert_eq!(
        skips(&w, &t).await,
        ["product-spec", "tech-spec", "milestone-plan"]
    );
    w.stop().await;
}

/// "required" is a human check's choice: `Spec review=required` with every planning stage skipped is taken and
/// skips nothing more (the check happens, as the flow says); "you" is no choice and is refused, nothing written; on an
/// Agent stage "required" is refused too (its usual choice is its agent). A `usual` row on the check (a "required"
/// stored) changes nothing, and the check can still be skipped.
#[tokio::test(flavor = "multi_thread")]
async fn required_keeps_the_check_and_you_is_not_a_choice() {
    let w = world(&spec_first()).await;
    let t = w.create("SHOP", "Fix cart total rounding").await;
    let mut picks = skip_planning();
    picks.push(pick("Spec review", "you", None));
    let refused = choose(&w, &t, &picks, &Chooser::User).await.unwrap_err();
    assert_eq!(refused.refusal(), Some(Refusal::NotAnOption));
    assert_eq!(
        refused.to_string(),
        "Spec review runs no agent: its choices are required (you check it) and skip"
    );
    assert!(skips(&w, &t).await.is_empty());
    let refused = choose(
        &w,
        &t,
        &[pick("Product spec", "required", None)],
        &Chooser::User,
    )
    .await
    .unwrap_err();
    assert_eq!(refused.refusal(), Some(Refusal::NotAnOption));
    let mut picks = skip_planning();
    picks.push(pick("Spec review", "required", None));
    choose(&w, &t, &picks, &Chooser::User).await.unwrap();
    assert_eq!(
        skips(&w, &t).await,
        ["product-spec", "tech-spec", "milestone-plan"]
    );

    let t = w.create("SHOP", "Saved carts").await;
    // A "required" stored (a `usual` row).
    sqlx::query(
        "INSERT INTO ticket_stage_choices (ticket_id, stage_id, choice, set_by, set_at)
         VALUES (?, 'spec-review', 'usual', 'user', 0)",
    )
    .bind(&t.id)
    .execute(w.board.store().pool())
    .await
    .unwrap();
    passed_on(&w, &t, &skip_planning()).await.unwrap();
    assert_eq!(
        skips(&w, &t).await,
        ["product-spec", "tech-spec", "milestone-plan"]
    );
    choose(&w, &t, &[pick("Spec review", "skip", None)], &Chooser::User)
        .await
        .unwrap();
    assert_eq!(
        skips(&w, &t).await,
        ["product-spec", "tech-spec", "milestone-plan", "spec-review"]
    );
    w.stop().await;
}

/// One planning stage left on, Spec review runs; with every planning stage skipped it runs too, before and after a
/// drag back into one of them.
#[tokio::test(flavor = "multi_thread")]
async fn one_planning_stage_left_on_runs_spec_review() {
    let w = world(&spec_first()).await;
    let t = w.create("SHOP", "Wishlist").await;
    let picks = [
        pick("Product spec", "skip", None),
        pick("Tech spec", "skip", None),
    ];
    passed_on(&w, &t, &picks).await.unwrap();
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

    let other = w.create("SHOP", "Saved carts").await;
    passed_on(&w, &other, &skip_planning()).await.unwrap();
    let planning = ["product-spec", "tech-spec", "milestone-plan"];
    assert_eq!(skips(&w, &other).await, planning);
    ran(&w, &other, "product-spec", None).await;
    assert_eq!(skips(&w, &other).await, planning);
    w.stop().await;
}

/// A set of choices that leaves a check the flow doesn't mark skippable with nothing before it is refused, naming the
/// stages, in one request (the ⌘N form's batch, a new ticket) or one picker at a time (the sheet). Nothing is written.
/// A skippable check isn't stranded: skipping every planning stage with Spec review left to run is accepted.
#[tokio::test(flavor = "multi_thread")]
async fn a_set_of_skips_that_strands_a_check_is_refused() {
    let w = world(&spec_first()).await;
    let mut picks = skip_planning();
    picks.push(pick("Spec review", "required", None));
    let accepted = w
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
        .unwrap();
    assert_eq!(accepted.len(), 4);

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
    // The ⌘N form: a ticket about to be created.
    let refused = w
        .board
        .check_choices(
            "SHOP",
            None,
            "the new ticket",
            &skip_planning(),
            &Chooser::User,
            false,
        )
        .await
        .unwrap_err();
    assert_eq!(refused.refusal(), Some(Refusal::StrandsCheck));
    assert_eq!(refused.to_string(), REFUSED);

    // The sheet's pickers, one at a time: the third skip is refused.
    let t = w.create("SHOP", "Fix cart total rounding").await;
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

    let v = w.create("SHOP", "Gift cards").await;
    let refused = passed_on(&w, &v, &skip_planning()).await.unwrap_err();
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
    passed_on(&w, &t, &[pick("Product spec", "skip", None)])
        .await
        .unwrap();
    assert_eq!(send_back(&skips(&w, &t).await), "milestone-plan");

    let u = w.create("SHOP", "Gift cards").await;
    let picks = [
        pick("Product spec", "skip", None),
        pick("Milestone plan", "skip", None),
    ];
    passed_on(&w, &u, &picks).await.unwrap();
    assert_eq!(send_back(&skips(&w, &u).await), "tech-spec");
    w.stop().await;
}
