//! The draft and when an applied change reaches tickets (T-0470; FED-02, FED-08; d0115): a Project agent's proposal
//! lands in the project's draft with its stages marked, Undo takes one back and Undo all every one; Plan added after
//! Grill applies at once, so SHOP-12 in Grill goes to it and SHOP-33 in Dev never meets it; Security moved before
//! Validate waits for Dev to Review only, holds a ticket at Grill's exit and applies once the span empties; a swap to
//! another flow waits for the whole board; Apply now lists only the tickets inside the span. On the example's shop-web
//! (Grill, Dev, Validate, Security, Review, Done as a PR: `tests/flows/every-kind.yaml` with QA named Review).

use std::collections::HashSet;
use std::path::Path;

use otaq_core::agents::FixedModels;
use otaq_core::board::moves::{DropOn, DropTarget};
use otaq_core::board::tickets::Ticket;
use otaq_core::domain::{Actor, Place, Status, TicketKey};
use otaq_core::flow::apply::{ProposeFrom, Wait};
use otaq_core::flow::apply_now::Live;
use otaq_core::flow::draft::{DraftApplies, DraftEdit, Inside, Mark};
use otaq_core::flow::engine::{At, Move, step};
use otaq_core::flow::proposal::{ApplyChoices, ProposedBy};
use otaq_core::flow::propose::Proposed;
use otaq_core::flow::{Flow, FlowFile, yaml};
use otaq_core::scheduler::{Gate, plan_holding};
use otaq_core::service::testing::TestService;

fn me() -> Actor {
    Actor::User("me".into())
}

fn models() -> FixedModels {
    FixedModels::sample()
}

fn noyan() -> ProposedBy {
    ProposedBy::ProjectAgent {
        agent: "pa-1".into(),
        session: "session-1".into(),
        name: "Checkout slowness".into(),
        tool: otaq_core::agent_tools::Tool::Claude,
    }
}

/// shop-web's flow: Grill, Dev, Validate, Security, Review (a check by you), Done (a PR).
fn shop_flow() -> FlowFile {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/flows/every-kind.yaml");
    let mut file = yaml::read(&std::fs::read_to_string(path).unwrap()).unwrap();
    file.flow
        .stages
        .iter_mut()
        .find(|s| s.id == "qa")
        .unwrap()
        .name = "Review".into();
    file
}

/// `edit` made to shop-web's flow, as a document.
fn shop_with(edit: impl FnOnce(&mut FlowFile)) -> String {
    let mut file = shop_flow();
    edit(&mut file);
    yaml::write(&file)
}

/// Security moved before Validate.
fn security_first(file: &mut FlowFile) {
    let at = file.flow.position("security").unwrap();
    let security = file.flow.stages.remove(at);
    let validate = file.flow.position("validate").unwrap();
    file.flow.stages.insert(validate, security);
}

/// Plan, an Agent stage run by Griller, after Grill.
fn plan_after_grill(file: &mut FlowFile) {
    let mut plan = file.flow.stage("grill").unwrap().clone();
    plan.id = "plan".into();
    plan.name = "Plan".into();
    plan.options.clear();
    plan.why = Some("Big tickets in shop-web get a plan first".into());
    file.flow.stages.insert(1, plan);
}

/// shop-web on its flow, its key.
async fn world() -> (TestService, String) {
    let w = TestService::start().await;
    let key = w.add_project("shop-web").await.key;
    w.board.use_flow_for_tests(&key, &shop_flow()).await;
    (w, key)
}

async fn at(w: &TestService, key: &str, title: &str, stage: &str, status: Status) -> Ticket {
    let t = w.create(key, title).await;
    w.force_stage(&t, stage, status).await;
    w.board.ticket(&t.key.to_string()).await.unwrap()
}

async fn ready(w: &TestService, key: &str, title: &str) -> Ticket {
    let t = w.create(key, title).await;
    w.board
        .move_ticket(&t.key.to_string(), DropTarget::ReadyForDev, true, &me())
        .await
        .unwrap()
}

async fn edit(w: &TestService, key: &str, edit: DraftEdit) -> otaq_core::flow::draft::Draft {
    w.board.update_draft(key, &edit, &models()).await.unwrap()
}

/// The project's tickets the scheduler would start now, after a settling pass.
async fn would_start(w: &TestService, key: &str) -> HashSet<TicketKey> {
    let settled = w.board.settle_flow_changes().await.unwrap();
    let tickets = w.board.tickets(key).await.unwrap();
    plan_holding(
        &tickets,
        &HashSet::new(),
        &Gate::default(),
        &settled.held,
        &HashSet::new(),
    )
    .start
    .into_iter()
    .map(|t| t.key)
    .collect()
}

/// Where the agent's "next stage" takes `ticket` in `flow`.
fn next_place(flow: &Flow, ticket: &Ticket) -> Place {
    step(At::new(flow, &ticket.place, ticket.status), Move::NextStage)
        .unwrap()
        .place
}

fn ids(text: &str) -> Vec<String> {
    yaml::read(text)
        .unwrap()
        .flow
        .stages
        .iter()
        .map(|s| s.id.clone())
        .collect()
}

/// FED-02: a Project agent's `flow propose` lands in the draft with each stage it changed marked (Plan added, Visual
/// check new with its new agent, Security changed), the agent the answer goes to; the user's own edit keeps the marks
/// that still hold; Undo takes Plan back, Undo all the rest; Cancel drops the draft.
#[tokio::test(flavor = "multi_thread")]
async fn an_agents_proposal_lands_in_the_draft_with_marks() {
    let (w, key) = world().await;
    let draft = w.board.draft(&key, &models()).await.unwrap();
    assert_eq!(draft.proposal.id, 0, "nothing stored");
    assert_eq!(draft.applies, DraftApplies::Nothing);
    assert_eq!(ids(&draft.proposal.yaml), ids(&yaml::write(&shop_flow())));

    let doc = shop_with(|file| {
        plan_after_grill(file);
        let mut visual = file.flow.stage("security").unwrap().clone();
        visual.id = String::new();
        visual.name = "Visual check".into();
        visual.agents = vec!["Visual checker".into()];
        visual.skill = Some("validate".into());
        let after = file.flow.position("validate").unwrap() + 1;
        file.flow.stages.insert(after, visual);
        file.flow
            .stages
            .iter_mut()
            .find(|s| s.id == "security")
            .unwrap()
            .rounds = Some(3);
        file.agents.push(otaq_core::flow::FlowAgent {
            name: "Visual checker".into(),
            skills: vec!["validate".into()],
            model: "default".into(),
            ..Default::default()
        });
    });
    let proposal = w
        .board
        .propose(
            &key,
            &Proposed::Document {
                yaml: doc,
                file: None,
            },
            &noyan(),
            &models(),
        )
        .await
        .unwrap();
    let draft = w.board.draft(&key, &models()).await.unwrap();
    assert_eq!(draft.proposal.id, proposal.id, "the draft holds it");
    assert_eq!(draft.proposal.proposed_by, noyan(), "the answer goes to it");
    let marks: Vec<(&str, Mark, &str)> = draft
        .marks
        .iter()
        .map(|m| (m.stage.as_str(), m.mark, m.by.as_str()))
        .collect();
    assert_eq!(
        marks,
        [
            ("Plan", Mark::Added, "Checkout slowness"),
            ("Visual check", Mark::New, "Checkout slowness"),
            ("Security", Mark::Changed, "Checkout slowness"),
        ]
    );
    assert_eq!(
        draft.marks[0].why.as_deref(),
        Some("Big tickets in shop-web get a plan first")
    );
    assert!(draft.can_undo_all);
    assert_eq!(draft.applies, DraftApplies::AtOnce, "additions and edits");
    assert_eq!(
        w.board
            .proposals(&key, &models())
            .await
            .unwrap()
            .into_iter()
            .map(|p| p.id)
            .collect::<Vec<_>>(),
        [proposal.id],
        "no separate proposal"
    );

    // The user takes Visual check out in the editor: its mark goes, the others stay.
    let mut file = yaml::read(&draft.proposal.yaml).unwrap();
    file.flow.stages.retain(|s| s.name != "Visual check");
    let draft = edit(
        &w,
        &key,
        DraftEdit::Document {
            yaml: yaml::write(&file),
        },
    )
    .await;
    assert_eq!(
        draft.proposal.id, proposal.id,
        "the same draft, changed in place"
    );
    let marked: Vec<&str> = draft.marks.iter().map(|m| m.stage.as_str()).collect();
    assert_eq!(marked, ["Plan", "Security"]);

    // Undo Plan: it goes; Security stays marked.
    let draft = edit(
        &w,
        &key,
        DraftEdit::UndoStage {
            stage: "plan".into(),
        },
    )
    .await;
    assert!(!ids(&draft.proposal.yaml).contains(&"plan".to_owned()));
    let marked: Vec<&str> = draft.marks.iter().map(|m| m.stage.as_str()).collect();
    assert_eq!(marked, ["Security"]);

    // Undo all: the draft before the agent's proposal, nothing marked, nothing to apply.
    let draft = edit(&w, &key, DraftEdit::UndoAll).await;
    assert!(draft.marks.is_empty());
    assert!(!draft.can_undo_all);
    assert_eq!(
        yaml::read(&draft.proposal.yaml).unwrap().flow,
        shop_flow().flow
    );
    assert_eq!(draft.applies, DraftApplies::Nothing);

    // Cancel drops it.
    w.board.cancel_proposal(&key).await.unwrap();
    assert_eq!(w.board.draft(&key, &models()).await.unwrap().proposal.id, 0);
    w.stop().await;
}

/// FED-08: Plan added after Grill applies at once: SHOP-12 in Grill goes to Plan when it passes Grill, SHOP-33 in Dev
/// goes on to Validate and never meets it.
#[tokio::test(flavor = "multi_thread")]
async fn plan_added_after_grill_reaches_grill_not_dev() {
    let (w, key) = world().await;
    let shop12 = at(&w, &key, "Add dark mode toggle", "grill", Status::Working).await;
    let shop33 = at(&w, &key, "Shipping estimate", "dev", Status::Working).await;
    let draft = edit(
        &w,
        &key,
        DraftEdit::Document {
            yaml: shop_with(plan_after_grill),
        },
    )
    .await;
    assert_eq!(draft.applies, DraftApplies::AtOnce);
    let applied = w
        .board
        .apply_proposal(&key, draft.proposal.id, &ApplyChoices::default(), &models())
        .await
        .unwrap();
    assert!(applied.waiting.is_none());
    assert!(applied.version.is_some());
    let flow = w.board.effective_flow(&key).await.unwrap();
    assert_eq!(
        next_place(&flow, &shop12),
        Place::Waiting(Some("plan".into()))
    );
    assert_eq!(
        next_place(&flow, &shop33),
        Place::InStage("validate".into())
    );
    w.stop().await;
}

/// FED-08, `s27-07`: Security moved before Validate waits for Dev to Review only. SHOP-33 in Dev and SHOP-19 in
/// Security hold it, SHOP-18 waiting in Review doesn't; SHOP-35 in Ready starts (Grill is before the span); SHOP-12
/// leaving Grill waits at the edge, before Dev, and a drag of it past Dev stops there; Apply now lists only SHOP-33 and
/// SHOP-19. Once both are through, the change applies and SHOP-12 starts.
#[tokio::test(flavor = "multi_thread")]
async fn a_reorder_waits_for_its_span_only() {
    let (w, key) = world().await;
    let shop12 = at(&w, &key, "Add dark mode toggle", "grill", Status::Working).await;
    let shop33 = at(&w, &key, "Shipping estimate", "dev", Status::Working).await;
    let shop19 = at(&w, &key, "Price table", "security", Status::Checking).await;
    let shop18 = at(&w, &key, "Checkout API", "qa", Status::WaitingForYou).await;
    let shop35 = ready(&w, &key, "Slow checkout query").await;

    let draft = edit(
        &w,
        &key,
        DraftEdit::Document {
            yaml: shop_with(security_first),
        },
    )
    .await;
    let DraftApplies::Span { span, inside } = &draft.applies else {
        panic!("{:?}", draft.applies)
    };
    assert_eq!((span.first.as_str(), span.last.as_str()), ("Dev", "Review"));
    assert_eq!(span.stages, ["dev", "validate", "security", "qa"]);
    let mut expected = vec![
        Inside {
            ticket: shop33.key.clone(),
            stage: "dev".into(),
        },
        Inside {
            ticket: shop19.key.clone(),
            stage: "security".into(),
        },
    ];
    expected.sort_by(|a, b| a.ticket.cmp(&b.ticket));
    assert_eq!(inside, &expected);

    let applied = w
        .board
        .apply_proposal(&key, draft.proposal.id, &ApplyChoices::default(), &models())
        .await
        .unwrap();
    assert_eq!(applied.version, None);
    let waiting = applied.waiting.unwrap();
    assert!(matches!(&waiting.wait, Wait::Span(s) if s.first == "Dev" && s.last == "Review"));
    let mut holding = vec![shop33.key.clone(), shop19.key.clone()];
    holding.sort();
    assert_eq!(waiting.waiting_for, holding, "not {}", shop18.key);
    assert!(!waiting.hold.ready_for_dev, "Grill comes before the span");
    assert_eq!(waiting.hold.before.as_deref(), Some("dev"));
    assert_eq!(
        would_start(&w, &key).await,
        HashSet::from([shop35.key.clone()])
    );

    // A drag of SHOP-12 past Dev stops at the edge; a drop on Dev stays one.
    assert_eq!(
        w.board
            .span_edge(&shop12, &DropOn::stage("security"))
            .await
            .unwrap(),
        DropOn::stage("dev")
    );
    assert_eq!(
        w.board
            .span_edge(&shop12, &DropOn::stage("dev"))
            .await
            .unwrap(),
        DropOn::stage("dev")
    );

    // SHOP-12 leaves Grill: it waits before Dev, and doesn't start; it doesn't hold the change either.
    let flow = w.board.effective_flow(&key).await.unwrap();
    assert_eq!(
        next_place(&flow, &shop12),
        Place::Waiting(Some("dev".into()))
    );
    w.force_stage(&shop12, "dev", Status::Queued).await;
    assert_eq!(
        w.board.ticket(&shop12.key.to_string()).await.unwrap().place,
        Place::Waiting(Some("dev".into()))
    );
    assert!(
        !would_start(&w, &key).await.contains(&shop12.key),
        "held at the edge"
    );
    let waiting = w.board.waiting_change(&key).await.unwrap().unwrap();
    assert_eq!(waiting.waiting_for, holding);

    // Apply now: only the tickets inside the span keep their place; nothing is removed, so nothing moves.
    let plan = w
        .board
        .apply_now_plan(&key, &Live::default())
        .await
        .unwrap()
        .unwrap();
    assert!(plan.moves.is_empty());
    let kept: Vec<TicketKey> = plan.keep.iter().map(|k| k.ticket.clone()).collect();
    assert_eq!(kept, holding);

    // The span empties: the change applies and SHOP-12 starts.
    w.force_stage(&shop33, "qa", Status::WaitingForYou).await;
    w.force_stage(&shop19, "qa", Status::WaitingForYou).await;
    assert!(would_start(&w, &key).await.contains(&shop12.key));
    assert!(w.board.waiting_change(&key).await.unwrap().is_none());
    let flow = w.board.effective_flow(&key).await.unwrap();
    let order: Vec<&str> = flow.stages.iter().map(|s| s.id.as_str()).collect();
    assert_eq!(
        order,
        ["grill", "dev", "security", "validate", "qa", "ship"]
    );
    w.stop().await;
}

/// FED-08: the same reorder picked from Start from (an imported file) is a swap: it waits for the whole board, SHOP-12
/// in Grill included, and nothing starts from Ready for Dev.
#[tokio::test(flavor = "multi_thread")]
async fn a_swap_waits_for_the_whole_board() {
    let (w, key) = world().await;
    let shop12 = at(&w, &key, "Add dark mode toggle", "grill", Status::Working).await;
    let _shop35 = ready(&w, &key, "Slow checkout query").await;
    let draft = edit(
        &w,
        &key,
        DraftEdit::StartFrom {
            source: ProposeFrom::File {
                name: "security-first.yaml".into(),
                yaml: shop_with(security_first),
            },
        },
    )
    .await;
    assert!(draft.swap);
    assert_eq!(
        draft.applies,
        DraftApplies::WholeBoard {
            inside: vec![Inside {
                ticket: shop12.key.clone(),
                stage: "grill".into(),
            }]
        }
    );
    let applied = w
        .board
        .apply_proposal(&key, draft.proposal.id, &ApplyChoices::default(), &models())
        .await
        .unwrap();
    let waiting = applied.waiting.unwrap();
    assert_eq!(waiting.wait, Wait::WholeBoard);
    assert_eq!(waiting.waiting_for, std::slice::from_ref(&shop12.key));
    assert!(would_start(&w, &key).await.is_empty(), "Ready for Dev held");
    w.stop().await;
}

/// FED-08, APN: Security removed while SHOP-19 checks in it waits for Dev to Review; Apply now moves SHOP-19 and keeps
/// SHOP-33 (in Dev), and doesn't list SHOP-12 in Grill, before the span.
#[tokio::test(flavor = "multi_thread")]
async fn apply_now_plans_only_inside_the_span() {
    let (w, key) = world().await;
    let shop12 = at(&w, &key, "Add dark mode toggle", "grill", Status::Working).await;
    let shop33 = at(&w, &key, "Shipping estimate", "dev", Status::Working).await;
    let shop19 = at(&w, &key, "Price table", "security", Status::Checking).await;
    let draft = edit(
        &w,
        &key,
        DraftEdit::Document {
            yaml: shop_with(|f| f.flow.stages.retain(|s| s.id != "security")),
        },
    )
    .await;
    w.board
        .apply_proposal(&key, draft.proposal.id, &ApplyChoices::default(), &models())
        .await
        .unwrap();
    let plan = w
        .board
        .apply_now_plan(&key, &Live::default())
        .await
        .unwrap()
        .unwrap();
    let moves: Vec<&TicketKey> = plan.moves.iter().map(|m| &m.ticket).collect();
    assert_eq!(moves, [&shop19.key]);
    let kept: Vec<&TicketKey> = plan.keep.iter().map(|k| &k.ticket).collect();
    assert_eq!(kept, [&shop33.key], "not {}", shop12.key);
    w.stop().await;
}
