//! Apply now on a waiting flow change (T-0438; APN-02…APN-05, FLW-06, FLW-18; d0108 "A flow change waits for a quiet
//! board" with Apply now, "Dragging a card back…", "Other drags", "After a Fail"; d0045), on the test flow
//! (`tests/flows/every-kind.yaml`: Product spec · Dev · Validate · Security · QA · Done): the plan (moves with their
//! choices, defaults and hints, the tickets that keep their place, the stamp) for one ticket in a removed human check,
//! four tickets in two removed stages, a merged change and no change; applying (the flow, the moves and the stopped
//! sessions together, a failure leaving nothing done, a pick back resuming Dev's conversation, a forward pick marking
//! skips, a stale stamp or a bad pick refused with the fresh plan, "already applied"); and the example's run with
//! stand-in sessions. `claude` is stood in for by an ordinary command.

use std::path::Path;
use std::time::Duration;

use otaq_core::agent_tools::LaunchContext;
use otaq_core::agent_tools::claude::ClaudeLaunch;
use otaq_core::agents::FixedModels;
use otaq_core::board::moves::{DropOn, DropTarget};
use otaq_core::board::tickets::{EventKind, HistoryEvent, Ticket};
use otaq_core::domain::{Actor, Place, Status};
use otaq_core::flow::apply_now::{
    ApplyNowAnswer, ApplyNowParams, ApplyNowPlan, FORWARD_HINT, Pick, PlannedMove, Refreshed, Way,
};
use otaq_core::flow::engine::RunResult;
use otaq_core::flow::proposal::{ApplyChoices, NewProposal, ProposedBy, Source};
use otaq_core::flow::{FlowFile, yaml};
use otaq_core::git::testing::commit;
use otaq_core::service::ServiceConfig;
use otaq_core::service::testing::TestService;
use otaq_core::session::SessionKind;

fn me() -> Actor {
    Actor::User("me".into())
}

/// Keeps running, printing its arguments.
const KEEPS_RUNNING: &str = "printf 'ARGS %s\\n' \"$*\"; exec sleep 30";

/// shop-web (SHOP) on the test flow, with a stand-in `claude` that keeps running.
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
    w.board.use_flow_for_tests("SHOP", &every_kind()).await;
    w
}

fn every_kind() -> FlowFile {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/flows/every-kind.yaml");
    yaml::read(&std::fs::read_to_string(path).unwrap()).unwrap()
}

/// Proposes the current flow without the stages `gone` and applies it: a change that waits while tickets work.
async fn remove(w: &TestService, gone: &[&str]) {
    let mut file = w.board.current_flow_file("SHOP").await.unwrap();
    if let Some(waiting) = w.board.waiting_change("SHOP").await.unwrap() {
        file = yaml::read(&waiting.yaml).unwrap();
    }
    file.flow.stages.retain(|s| !gone.contains(&s.id.as_str()));
    let proposal = w
        .board
        .open_proposal(
            "SHOP",
            &NewProposal {
                yaml: yaml::write(&file),
                source: Source::Document { file: None },
                proposed_by: ProposedBy::User,
            },
            &FixedModels::sample(),
        )
        .await
        .unwrap();
    let applied = w
        .board
        .apply_proposal(
            "SHOP",
            proposal.id,
            &ApplyChoices::default(),
            &FixedModels::sample(),
        )
        .await
        .unwrap();
    assert!(applied.waiting.is_some(), "the change waits");
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

/// The ticket ran Dev in the conversation `<key>-dev`, then moved on to `stage`.
async fn ran_dev_then(w: &TestService, title: &str, stage: &str, status: Status) -> Ticket {
    let t = at(w, title, "dev", Status::Working).await;
    w.record_ended_agent(&t, &format!("{}-dev", t.key)).await;
    place_in(w, &t, stage, status).await;
    w.board.ticket(&t.key.to_string()).await.unwrap()
}

async fn plan(w: &TestService) -> ApplyNowPlan {
    w.review
        .apply_now_plan("SHOP")
        .await
        .unwrap()
        .expect("a plan")
}

fn move_of<'p>(plan: &'p ApplyNowPlan, key: &str) -> &'p PlannedMove {
    plan.moves
        .iter()
        .find(|m| m.ticket.to_string() == key)
        .unwrap_or_else(|| panic!("no move for {key}"))
}

fn choices(m: &PlannedMove) -> Vec<(String, Way)> {
    m.choices.iter().map(|c| (c.name.clone(), c.way)).collect()
}

fn pick(key: &str, to: impl Into<DropOn>) -> Pick {
    Pick {
        ticket: key.parse().unwrap(),
        to: to.into(),
    }
}

async fn apply(w: &TestService, stamp: &str, picks: Vec<Pick>) -> ApplyNowAnswer {
    w.review
        .apply_now(
            &ApplyNowParams {
                project: "SHOP".into(),
                stamp: stamp.into(),
                picks,
            },
            &w.validation,
            &me(),
        )
        .await
        .unwrap()
}

async fn place_and_status(w: &TestService, key: &str) -> (Place, Status) {
    let t = w.board.ticket(key).await.unwrap();
    (t.place, t.status)
}

async fn last_event(w: &TestService, key: &str, kind: EventKind) -> HistoryEvent {
    w.board
        .history(key)
        .await
        .unwrap()
        .into_iter()
        .rfind(|e| e.kind == kind)
        .unwrap_or_else(|| panic!("{key} has no {kind:?} event"))
}

async fn until(w: &TestService, key: &str, want: impl Fn(&Ticket) -> bool, what: &str) {
    let reached = tokio::time::timeout(Duration::from_secs(20), async {
        while !want(&w.board.ticket(key).await.unwrap()) {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await;
    assert!(reached.is_ok(), "{key} never {what}");
}

fn names(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| (*s).to_owned()).collect()
}

/// s11-17: one ticket waiting in QA, a removed human check; SHOP-2 working in Dev holds the change and keeps its
/// place. No session runs: nothing stops. QA's place is before Done, so Done is the default; every earlier stage is
/// back.
#[tokio::test(flavor = "multi_thread")]
async fn the_plan_for_one_ticket_in_a_removed_human_check() {
    let w = world().await;
    assert!(
        w.review.apply_now_plan("SHOP").await.unwrap().is_none(),
        "no change waits"
    );
    let qa = ran_dev_then(&w, "Price table", "qa", Status::WaitingForYou).await;
    at(&w, "Coupons", "dev", Status::Working).await;
    remove(&w, &["qa"]).await;

    let plan = plan(&w).await;
    assert_eq!(
        plan.summary,
        "Removes QA (your check) from shop-web\u{2019}s flow"
    );
    assert_eq!(plan.removed, names(&["QA"]));
    assert_eq!(plan.moves.len(), 1);
    let m = &plan.moves[0];
    assert_eq!(m.ticket, qa.key);
    assert_eq!((m.stage.as_str(), m.stage_name.as_str()), ("qa", "QA"));
    assert!(!m.agent_will_stop && !m.waiting);
    assert_eq!(m.state, None);
    assert_eq!(
        choices(m),
        [
            ("Backlog".to_owned(), Way::Backlog),
            ("Ready for Dev".to_owned(), Way::ReadyForDev),
            ("Product spec".to_owned(), Way::Back),
            ("Dev".to_owned(), Way::Back),
            ("Validate".to_owned(), Way::Back),
            ("Security".to_owned(), Way::Back),
            ("Done".to_owned(), Way::Next),
        ]
    );
    assert_eq!(m.default, DropOn::stage("ship"));
    let hint = |stage: &str| m.choice(&DropOn::stage(stage)).unwrap().hint.clone();
    assert_eq!(
        hint("dev").as_deref(),
        Some("Dev resumes SHOP-1\u{2019}s earlier session")
    );
    assert_eq!(
        hint("product-spec").as_deref(),
        Some("Product spec starts a new session for SHOP-1")
    );
    assert_eq!(
        hint("security").as_deref(),
        Some("Security runs again on SHOP-1")
    );
    assert_eq!(hint("ship"), None);
    assert_eq!(plan.keep.len(), 1);
    let kept = &plan.keep[0];
    assert_eq!(
        (
            kept.ticket.to_string(),
            kept.stage_name.as_str(),
            kept.keeps_running
        ),
        ("SHOP-2".to_owned(), "Dev", false),
        "keeps its place"
    );
    assert!(!plan.stamp.is_empty());
    assert_eq!(
        plan.stamp,
        self::plan(&w).await.stamp,
        "the same board, the same stamp"
    );
    w.stop().await;
}

/// s11-18: four tickets in two removed stages, Product spec and Security: two with running sessions (an agent in
/// Product spec, a checker in Security), one failed ×3, one idle with no session. Product spec's place is before Dev,
/// Security's before QA; moving on from Security to Done skips QA. SHOP-5's agent keeps running in Dev; SHOP-6 keeps
/// its place waiting before Dev.
#[tokio::test(flavor = "multi_thread")]
async fn the_plan_for_four_tickets_in_two_removed_stages() {
    let w = world().await;
    let planning = at(&w, "Price table", "product-spec", Status::Working).await;
    let _product_spec_agent = w.agent_shell(&planning).await;
    let checking = ran_dev_then(&w, "Coupons", "security", Status::Checking).await;
    let _checker = w.shell(&checking, SessionKind::Validator).await;
    let failed = ran_dev_then(&w, "Wishlist", "security", Status::CheckFailed).await;
    sqlx::query(
        "INSERT INTO check_counters (ticket_id, stage_id, fails, updated_at) VALUES (?, 'security', 3, 1)",
    )
    .bind(&failed.id)
    .execute(w.board.store().pool())
    .await
    .unwrap();
    at(&w, "Search", "product-spec", Status::Idle).await;
    let working = at(&w, "Checkout", "dev", Status::Working).await;
    let _dev_agent = w.agent_shell(&working).await;
    at(&w, "Reviews", "dev", Status::Queued).await;
    remove(&w, &["product-spec", "security"]).await;

    let plan = plan(&w).await;
    assert_eq!(
        plan.summary,
        "Removes Product spec and Security from shop-web\u{2019}s flow"
    );
    let keys: Vec<String> = plan.moves.iter().map(|m| m.ticket.to_string()).collect();
    assert_eq!(keys, ["SHOP-1", "SHOP-2", "SHOP-3", "SHOP-4"]);
    let stops: Vec<bool> = plan.moves.iter().map(|m| m.agent_will_stop).collect();
    assert_eq!(stops, [true, true, false, false]);
    let states: Vec<Option<&str>> = plan.moves.iter().map(|m| m.state.as_deref()).collect();
    assert_eq!(states, [None, None, Some("failed \u{d7}3"), Some("idle")]);

    let in_product_spec = move_of(&plan, "SHOP-1");
    assert_eq!(in_product_spec.stage_name, "Product spec");
    assert_eq!(in_product_spec.default, DropOn::stage("dev"));
    assert_eq!(
        choices(in_product_spec)[2..],
        [
            ("Dev".to_owned(), Way::Next),
            ("Validate".to_owned(), Way::Forward),
            ("QA".to_owned(), Way::Forward),
            ("Done".to_owned(), Way::Forward),
        ]
    );
    let in_security = move_of(&plan, "SHOP-2");
    assert_eq!(in_security.default, DropOn::stage("qa"));
    assert_eq!(
        choices(in_security)[2..],
        [
            ("Dev".to_owned(), Way::Back),
            ("Validate".to_owned(), Way::Back),
            ("QA".to_owned(), Way::Next),
            ("Done".to_owned(), Way::Forward),
        ]
    );
    assert_eq!(
        in_security
            .choice(&DropOn::stage("ship"))
            .unwrap()
            .hint
            .as_deref(),
        Some(FORWARD_HINT)
    );
    let kept: Vec<(String, &str, bool)> = plan
        .keep
        .iter()
        .map(|k| (k.ticket.to_string(), k.stage_name.as_str(), k.keeps_running))
        .collect();
    assert_eq!(
        kept,
        [
            ("SHOP-5".to_owned(), "Dev", true),
            ("SHOP-6".to_owned(), "Dev", false)
        ]
    );
    w.stop().await;
}

/// A change while another waits merges into it (FLW-06): the plan is for the merged change as a whole.
#[tokio::test(flavor = "multi_thread")]
async fn a_merged_waiting_change_counts_whole() {
    let w = world().await;
    at(&w, "Price table", "product-spec", Status::Working).await;
    at(&w, "Coupons", "security", Status::Checking).await;
    remove(&w, &["product-spec"]).await;
    remove(&w, &["security"]).await;

    let plan = plan(&w).await;
    assert_eq!(plan.removed, names(&["Product spec", "Security"]));
    // QA's fails went to Product spec: with Product spec gone they go to the nearest Agent stage (d0131), and the
    // preview says so.
    assert_eq!(
        plan.changes,
        [
            "QA's fails go to the nearest Agent stage now",
            "Product spec removed",
            "Security removed"
        ],
    );
    let keys: Vec<String> = plan.moves.iter().map(|m| m.ticket.to_string()).collect();
    assert_eq!(keys, ["SHOP-1", "SHOP-2"]);
    w.stop().await;
}

/// The example's Apply now with stand-in sessions (APN-04, APN-05): SHOP-1 runs in Product spec and goes on to Dev (the
/// default), SHOP-2's checker runs in Security and SHOP-2 is picked back to Dev, SHOP-3 runs in Dev, SHOP-4 waits in
/// QA. Apply now stores the flow and both moves at once; Product spec's agent and Security's checker stop (their runs
/// end "stopped · flow change"); SHOP-1 starts Dev in a new session; SHOP-2's Dev conversation resumes with Send back's
/// skill (a human Fail: its counters restart); SHOP-3's agent keeps running and SHOP-4 is untouched.
#[tokio::test(flavor = "multi_thread")]
async fn apply_now_stops_the_removed_stages_and_moves_every_ticket_together() {
    let w = world().await;
    // SHOP-1 starts in Product spec, the flow's first stage, with the stand-in `claude`.
    let planning = w.create("SHOP", "Price table").await;
    w.board
        .move_ticket("SHOP-1", DropTarget::ReadyForDev, true, &me())
        .await
        .unwrap();
    let started = w
        .starter
        .start("SHOP-1", &me(), otaq_core::session::Start::Auto)
        .await
        .unwrap();
    assert!(
        matches!(started, otaq_core::start::StartOutcome::Started(_)),
        "{started:?}"
    );
    assert_eq!(
        w.board.ticket("SHOP-1").await.unwrap().place,
        Place::stage_of("product-spec")
    );
    let product_spec_agent = w
        .sessions
        .running_of(&planning.id, SessionKind::Agent)
        .unwrap()
        .id;
    let checking = ran_dev_then(&w, "Coupons", "security", Status::Checking).await;
    let checker = w.shell(&checking, SessionKind::Validator).await;
    sqlx::query(
        "INSERT INTO check_counters (ticket_id, stage_id, fails, updated_at) VALUES (?, 'validate', 1, 1)",
    )
    .bind(&checking.id)
    .execute(w.board.store().pool())
    .await
    .unwrap();
    let working = at(&w, "Checkout", "dev", Status::Working).await;
    let dev_agent = w.agent_shell(&working).await;
    let waiting = ran_dev_then(&w, "Reviews", "qa", Status::WaitingForYou).await;
    remove(&w, &["product-spec", "security"]).await;
    let before = w.board.current_flow("SHOP").await.unwrap().version;

    let plan = plan(&w).await;
    let answer = apply(&w, &plan.stamp, vec![pick("SHOP-2", DropOn::stage("dev"))]).await;
    let ApplyNowAnswer::Applied {
        version,
        moved,
        stopped,
    } = answer
    else {
        panic!("not applied: {answer:?}");
    };
    assert_eq!(version.version, before + 1);
    let keys = |list: &[otaq_core::domain::TicketKey]| -> Vec<String> {
        list.iter().map(ToString::to_string).collect()
    };
    assert_eq!(keys(&moved), ["SHOP-1", "SHOP-2"]);
    assert_eq!(keys(&stopped), ["SHOP-1", "SHOP-2"]);
    assert!(w.board.waiting_change("SHOP").await.unwrap().is_none());
    let flow = w.board.effective_flow("SHOP").await.unwrap();
    assert!(flow.stage("product-spec").is_none() && flow.stage("security").is_none());

    // The sessions in removed stages stopped; the agent in Dev didn't.
    let ended = |id: &str| w.sessions.info(id).map_or(true, |s| !s.is_running());
    for id in [product_spec_agent.as_str(), checker.session.as_str()] {
        let gone = tokio::time::timeout(Duration::from_secs(10), async {
            while !ended(id) {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await;
        assert!(gone.is_ok(), "{id} still runs");
    }
    assert!(
        !ended(dev_agent.session.as_str()),
        "SHOP-3's agent keeps running"
    );
    assert_eq!(
        w.sessions
            .running_of(&working.id, SessionKind::Agent)
            .unwrap()
            .id,
        dev_agent.session.as_str()
    );
    assert_eq!(
        place_and_status(&w, "SHOP-3").await,
        (Place::stage_of("dev"), Status::Working)
    );
    assert_eq!(
        place_and_status(&w, &waiting.key.to_string()).await,
        (Place::stage_of("qa"), Status::WaitingForYou)
    );

    // The removed stages' runs ended stopped by the flow change.
    for (key, stage) in [("SHOP-1", "product-spec"), ("SHOP-2", "security")] {
        let runs = w.board.stage_runs(key).await.unwrap();
        let run = runs.iter().rfind(|r| r.stage == stage).unwrap();
        assert_eq!(run.result, Some(RunResult::Cancelled), "{key}");
    }
    let validation: Option<String> = sqlx::query_scalar(
        "SELECT outcome FROM validations WHERE ticket_id = ? ORDER BY started_at DESC",
    )
    .bind(&checking.id)
    .fetch_optional(w.board.store().pool())
    .await
    .unwrap()
    .flatten();
    assert!(
        validation.as_deref().is_none_or(|o| o == "stopped"),
        "{validation:?}"
    );

    // SHOP-1 starts Dev in a new conversation.
    until(&w, "SHOP-1", |t| t.status == Status::Working, "started Dev").await;
    assert_eq!(
        w.board.ticket("SHOP-1").await.unwrap().place,
        Place::stage_of("dev")
    );
    let fresh = w
        .sessions
        .running_of(&planning.id, SessionKind::Agent)
        .unwrap();
    assert_ne!(fresh.id, product_spec_agent);
    let out = w.wait_for_output(&fresh.id, "ARGS").await;
    assert!(!out.contains("--resume"), "{out}");

    // SHOP-2's Dev conversation resumes with Send back's skill; a human Fail resets its counters.
    until(&w, "SHOP-2", |t| t.status == Status::Working, "resumed Dev").await;
    let resumed = w
        .sessions
        .running_of(&checking.id, SessionKind::Agent)
        .unwrap();
    assert_eq!(resumed.tool_session_id.as_deref(), Some("SHOP-2-dev"));
    let out = w.wait_for_output(&resumed.id, "ARGS").await;
    assert!(out.contains("--resume SHOP-2-dev"), "{out}");
    assert!(out.contains("address-review"), "{out}");
    assert!(
        w.board
            .check_counts(&checking.id)
            .await
            .unwrap()
            .0
            .is_empty()
    );

    // History names the stage that went.
    let event = last_event(&w, "SHOP-1", EventKind::Dragged).await;
    assert_eq!(
        (
            &event.payload["stage"],
            &event.payload["removed"],
            &event.payload["back"]
        ),
        (&"Dev".into(), &"Product spec".into(), &false.into())
    );
    let event = last_event(&w, "SHOP-2", EventKind::Dragged).await;
    assert_eq!(
        (&event.payload["removed"], &event.payload["back"]),
        (&"Security".into(), &true.into())
    );
    w.stop().await;
}

/// A forward pick past stages marks them skipped by you (FLW-18), and Backlog parks with the work kept.
#[tokio::test(flavor = "multi_thread")]
async fn a_forward_pick_marks_skips_and_backlog_parks() {
    let w = world().await;
    at(&w, "Price table", "product-spec", Status::Idle).await;
    at(&w, "Coupons", "product-spec", Status::Idle).await;
    remove(&w, &["product-spec"]).await;

    let plan = plan(&w).await;
    let answer = apply(
        &w,
        &plan.stamp,
        vec![
            pick("SHOP-1", DropOn::stage("qa")),
            pick("SHOP-2", DropTarget::Backlog),
        ],
    )
    .await;
    assert!(
        matches!(answer, ApplyNowAnswer::Applied { .. }),
        "{answer:?}"
    );
    assert_eq!(
        place_and_status(&w, "SHOP-1").await,
        (Place::stage_of("qa"), Status::WaitingForYou)
    );
    let runs = w.board.stage_runs("SHOP-1").await.unwrap();
    for stage in ["dev", "validate", "security"] {
        assert_eq!(
            runs.iter().rfind(|r| r.stage == stage).unwrap().result,
            Some(RunResult::Skipped),
            "{stage}"
        );
    }
    let event = last_event(&w, "SHOP-1", EventKind::Dragged).await;
    assert_eq!(
        event.payload["skipped"],
        serde_json::json!(["Dev", "Validate", "Security"])
    );
    assert_eq!(event.payload["removed"], "Product spec");
    assert_eq!(
        place_and_status(&w, "SHOP-2").await,
        (Place::Backlog, Status::Parked)
    );
    assert_eq!(
        last_event(&w, "SHOP-2", EventKind::Parked).await.payload["removed"],
        "Product spec"
    );
    w.stop().await;
}

/// The board changed since the plan (a ticket entered a removed stage): nothing applies and the answer carries the
/// fresh plan. A pick that isn't one of its ticket's choices is refused the same way.
#[tokio::test(flavor = "multi_thread")]
async fn a_stale_stamp_or_a_bad_pick_is_refused_with_the_fresh_plan() {
    let w = world().await;
    at(&w, "Price table", "product-spec", Status::Idle).await;
    let later = at(&w, "Coupons", "dev", Status::Working).await;
    remove(&w, &["product-spec"]).await;
    let before = w.board.current_flow("SHOP").await.unwrap().version;
    let old = plan(&w).await;

    place_in(&w, &later, "product-spec", Status::Working).await;
    let answer = apply(&w, &old.stamp, Vec::new()).await;
    let ApplyNowAnswer::Refreshed { plan: fresh, why } = answer else {
        panic!("not refreshed: {answer:?}");
    };
    assert_eq!(why, Refreshed::BoardChanged);
    assert_ne!(fresh.stamp, old.stamp);
    assert_eq!(fresh.moves.len(), 2, "SHOP-2 entered Product spec");
    assert!(w.board.waiting_change("SHOP").await.unwrap().is_some());
    assert_eq!(w.board.current_flow("SHOP").await.unwrap().version, before);
    assert_eq!(
        place_and_status(&w, "SHOP-1").await,
        (Place::stage_of("product-spec"), Status::Idle)
    );

    // Product spec is no choice: it is the stage that goes.
    let answer = apply(
        &w,
        &fresh.stamp,
        vec![pick("SHOP-1", DropOn::stage("product-spec"))],
    )
    .await;
    let ApplyNowAnswer::Refreshed { why, .. } = answer else {
        panic!("not refreshed: {answer:?}");
    };
    assert_eq!(
        why,
        Refreshed::NotAChoice {
            ticket: "SHOP-1".parse().unwrap()
        }
    );
    assert_eq!(w.board.current_flow("SHOP").await.unwrap().version, before);
    w.stop().await;
}

/// The change applied on its own meanwhile (the board went quiet): "already applied". Cancelled: nothing waits.
#[tokio::test(flavor = "multi_thread")]
async fn already_applied_or_cancelled_meanwhile() {
    let w = world().await;
    let working = at(&w, "Price table", "dev", Status::Working).await;
    remove(&w, &["product-spec"]).await;
    let old = plan(&w).await;
    assert!(old.moves.is_empty());

    w.force_status(&working, Status::Backlog).await;
    let settled = w.board.settle_flow_change("SHOP").await.unwrap().unwrap();
    let answer = apply(&w, &old.stamp, Vec::new()).await;
    assert_eq!(
        answer,
        ApplyNowAnswer::AlreadyApplied {
            version: Box::new(settled)
        }
    );

    place_in(&w, &working, "dev", Status::Working).await;
    remove(&w, &["security"]).await;
    let old = plan(&w).await;
    assert!(w.board.cancel_waiting("SHOP").await.unwrap());
    assert_eq!(
        apply(&w, &old.stamp, Vec::new()).await,
        ApplyNowAnswer::NothingWaits
    );
    w.stop().await;
}

/// A failure in any move leaves nothing half done (APN-04): the flow, the waiting change, every ticket and every
/// session stay as they were. (A trigger makes SHOP-2's write fail.)
#[tokio::test(flavor = "multi_thread")]
async fn a_failure_in_a_move_leaves_nothing_half_done() {
    let w = world().await;
    let first = at(&w, "Price table", "product-spec", Status::Working).await;
    let agent = w.agent_shell(&first).await;
    let second = at(&w, "Coupons", "product-spec", Status::Idle).await;
    remove(&w, &["product-spec"]).await;
    let before = w.board.current_flow("SHOP").await.unwrap().version;
    let history = w.board.history("SHOP-1").await.unwrap().len();
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
        "CREATE TRIGGER fail_the_move BEFORE UPDATE OF state ON tickets WHEN NEW.id = '{}'
         BEGIN SELECT RAISE(ABORT, 'the move failed'); END",
        second.id
    )))
    .execute(w.board.store().pool())
    .await
    .unwrap();

    let plan = plan(&w).await;
    let failed = w
        .review
        .apply_now(
            &ApplyNowParams {
                project: "SHOP".into(),
                stamp: plan.stamp.clone(),
                picks: Vec::new(),
            },
            &w.validation,
            &me(),
        )
        .await;
    assert!(failed.is_err(), "{failed:?}");
    assert_eq!(w.board.current_flow("SHOP").await.unwrap().version, before);
    assert!(w.board.waiting_change("SHOP").await.unwrap().is_some());
    assert_eq!(
        place_and_status(&w, "SHOP-1").await,
        (Place::stage_of("product-spec"), Status::Working)
    );
    assert_eq!(w.board.history("SHOP-1").await.unwrap().len(), history);
    assert!(
        w.sessions
            .info(agent.session.as_str())
            .is_ok_and(|s| s.is_running()),
        "SHOP-1's agent still runs"
    );
    let runs = w.board.stage_runs("SHOP-1").await.unwrap();
    assert_eq!(
        runs.iter()
            .rfind(|r| r.stage == "product-spec")
            .unwrap()
            .ended_at,
        None
    );
    w.stop().await;
}
