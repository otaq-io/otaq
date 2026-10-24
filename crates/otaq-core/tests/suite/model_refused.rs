//! A model the tool refused (T-0337; AGT-05, SDD §7.2 `[M22]` rows, §11.4 `StopFailure`; d0088, d0086, d0018), fed
//! the saved `StopFailure` payload as `otaq hook --tool claude StopFailure` hands it to the service: a Developer's
//! first turn is Start failed and counts as not started, a Validator round is No result and not a round, a resumed
//! session or a later turn is a failed turn, a lead's first turn leaves it not started; the named agent's amber mark,
//! and what clears it. Nothing starts again by itself.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use otaq_core::agent_tools::claude::ClaudeLaunch;
use otaq_core::agent_tools::{LaunchContext, Tool};
use otaq_core::agents::{Agent, AgentFields, AgentModel, Agents, FixedModels, SessionRole};
use otaq_core::attention::notify::{Notifier, OnClick, OsNotification};
use otaq_core::board::needs_you::NeedsYouGroup;
use otaq_core::board::tickets::{NewTicketInput, Ticket};
use otaq_core::caller::Caller;
use otaq_core::domain::{Actor, Kind, StartStep, Status, StatusSet, ValidationAction};
use otaq_core::service::ServiceConfig;
use otaq_core::service::testing::TestService;
use otaq_core::session::{END_MODEL_REFUSED, SessionKind, StoredConversation};
use otaq_core::signals::Signal;
use otaq_core::signals::model_refused::MODEL_REFUSED;
use otaq_core::start::StartOutcome;
use otaq_core::validation::refused_its_model;
use serde_json::{Value, json};

/// The saved payload (T-0335's fixture; T-0346 swaps in a real capture).
fn refused_payload() -> Value {
    serde_json::from_str(include_str!(
        "../fixtures/claude/hooks/StopFailure-model_not_found.json"
    ))
    .unwrap()
}

const TOOL_MESSAGE: &str = "There's an issue with the selected model (claude-fable-5-1). It may not exist or you \
                            may not have access to it. Run /model to pick a different model.";

/// An ordinary command standing in for `claude`: it waits like an interactive program.
fn stand_in() -> LaunchContext {
    LaunchContext {
        bin_dir: "/nonexistent".into(),
        plugin_dir: "/nonexistent/plugin".into(),
        path: std::env::var_os("PATH").unwrap_or_default(),
        claude: ClaudeLaunch {
            program: vec![
                "sh".into(),
                "-c".into(),
                "echo agent started; exec cat".into(),
                "sh".into(),
            ],
            config_dir: None,
        },
    }
}

/// Records what is posted.
#[derive(Default)]
struct Recorder(Mutex<Vec<String>>);

impl Notifier for Recorder {
    fn post(&self, n: &OsNotification, _on_click: OnClick) {
        self.0.lock().unwrap().push(n.title.clone());
    }
}

struct World {
    w: TestService,
    agents: Agents,
    fable: Agent,
    light: Agent,
    recorder: Arc<Recorder>,
}

/// shop-web with Developer on `fable` and Light (`haiku`); a scheduler that looks every 50 ms, so a loop would show
/// at once.
async fn world() -> World {
    let recorder = Arc::new(Recorder::default());
    let notifier: Arc<dyn Notifier> = recorder.clone();
    let w = TestService::start_with(move |config| ServiceConfig {
        launch: Some(stand_in()),
        scheduler_tick: Duration::from_millis(50),
        notifier,
        ..config
    })
    .await;
    w.add_project("shop-web").await;
    let agents = Agents::new(w.board.clone(), Arc::new(FixedModels::sample()));
    let fable = w.set_agent_model("Developer", claude(Some("fable"))).await;
    let light = agents
        .add(&page("Light", Some("haiku"), Vec::new()))
        .await
        .unwrap();
    World {
        w,
        agents,
        fable,
        light,
        recorder,
    }
}

fn claude(model: Option<&str>) -> AgentModel {
    AgentModel::Tool {
        tool: Tool::Claude,
        model: model.map(str::to_owned),
    }
}

/// An agent's page on Claude Code with `model`.
fn page(name: &str, model: Option<&str>, skills: Vec<otaq_core::agents::SkillRef>) -> AgentFields {
    AgentFields {
        name: name.into(),
        instructions: String::new(),
        skills,
        model: claude(model),
        effort: None,
    }
}

/// The model text an agent passes.
fn model_of(agent: &Agent) -> Option<String> {
    match &agent.model {
        AgentModel::Tool { model, .. } => model.clone(),
        AgentModel::Default => None,
    }
}

/// Waits (10 s at most) until `done` holds.
async fn until<F: std::future::Future<Output = bool>>(what: &str, mut done: impl FnMut() -> F) {
    tokio::time::timeout(Duration::from_secs(10), async {
        while !done().await {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("{what} never happened"));
}

/// The session's agent and model as the start stores them (AGM-05).
async fn on_agent(w: &TestService, session: &str, agent: &Agent, model_ran: Option<&str>) {
    sqlx::query(
        "UPDATE sessions SET agent_id = ?, agent_name = ?, model = ?, model_ran = ? WHERE id = ?",
    )
    .bind(&agent.id)
    .bind(&agent.name)
    .bind(model_of(agent))
    .bind(model_ran)
    .bind(session)
    .execute(w.board.store().pool())
    .await
    .unwrap();
}

fn agent_caller(ticket: &Ticket, session: &str) -> Caller {
    Caller::Agent {
        ticket: ticket.key.clone(),
        ticket_id: ticket.id.clone(),
        session: session.to_owned(),
    }
}

async fn hook(w: &TestService, caller: &Caller, event: &str, payload: &Value) -> Vec<Signal> {
    w.signals
        .hook(caller, Tool::Claude, event, payload)
        .await
        .unwrap()
}

async fn agent_sessions(w: &TestService, ticket: &Ticket) -> Vec<(String, Option<String>)> {
    sqlx::query_as(
        "SELECT id, end_reason FROM sessions WHERE ticket_id = ? AND kind = 'agent' ORDER BY started_at, id",
    )
    .bind(&ticket.id)
    .fetch_all(w.board.store().pool())
    .await
    .unwrap()
}

async fn mark_text(w: &World, agent: &Agent) -> Option<String> {
    let mark = w.agents.agent(&agent.id).await.unwrap().mark?;
    // The tool's message is kept for the edit dialog.
    assert_eq!(mark.message.as_deref(), Some(TOOL_MESSAGE));
    Some(mark.text)
}

/// SHOP-1 with Developer on `fable`, started by the scheduler: the ticket and its agent's session.
async fn started_on_fable(w: &World) -> (Ticket, String) {
    let ticket = w.w.create("SHOP", "Add gift wrap option").await;
    w.w.board
        .move_ticket(
            "SHOP-1",
            otaq_core::board::moves::DropTarget::ReadyForDev,
            false,
            &Actor::User("me".into()),
        )
        .await
        .unwrap();
    until("the start", || async {
        w.w.status("SHOP-1").await == Status::Working
    })
    .await;
    let session =
        w.w.sessions
            .running_of(&ticket.id, SessionKind::Agent)
            .expect("the agent runs")
            .id;
    on_agent(&w.w, &session, &w.fable, None).await;
    (w.w.board.ticket("SHOP-1").await.unwrap(), session)
}

#[tokio::test(flavor = "multi_thread")]
async fn a_developers_first_turn_refused_is_start_failed_not_started_and_never_restarts_by_itself()
{
    let w = world().await;
    let (ticket, session) = started_on_fable(&w).await;
    let caller = agent_caller(&ticket, &session);
    hook(
        &w.w,
        &caller,
        "UserPromptSubmit",
        &json!({"prompt": "/otaq:work-ticket"}),
    )
    .await;

    let signals = hook(&w.w, &caller, "StopFailure", &refused_payload()).await;
    assert_eq!(signals, []);

    // Back in Ready for Dev as Start failed, with the step `model` and the tool's message; not started.
    let after = w.w.board.ticket("SHOP-1").await.unwrap();
    assert_eq!(
        (after.status, after.board_group()),
        (Status::StartInterrupted, StatusSet::Waiting)
    );
    let problem = after.start_problem.clone().unwrap();
    assert_eq!(problem.step, StartStep::Model);
    assert_eq!(problem.message, TOOL_MESSAGE);
    assert_eq!(after.started_at, None);
    // The session ended "not started": its conversation isn't kept for Resume.
    let info = w.w.sessions.wait_ended(&session).await.unwrap();
    assert_eq!(info.end_reason.as_deref(), Some(END_MODEL_REFUSED));
    let store = w.w.board.store();
    assert!(!store.developer_started(&ticket.id).await.unwrap());
    assert_eq!(
        StoredConversation::latest_agent(store.pool(), &ticket.id)
            .await
            .unwrap(),
        None
    );
    // The worktree stays for the retry.
    assert!(std::path::Path::new(after.work.as_ref().unwrap().worktree.as_ref().unwrap()).is_dir());
    // The agent is marked with the model passed (no exact model was reported).
    assert_eq!(
        mark_text(&w, &w.fable).await.as_deref(),
        Some("Claude Code couldn't run fable on SHOP-1")
    );

    // The needs-you item is Start interrupted's (group 2), offering Open agent…; like Start interrupted it posts no OS
    // notification (ATT-11: only items that block an agent).
    until("the needs_you item", || async {
        w.w.needs_you
            .list()
            .await
            .unwrap()
            .items
            .iter()
            .any(|i| i.key == ticket.key)
    })
    .await;
    let items = w.w.needs_you.list().await.unwrap().items;
    let item = items.iter().find(|i| i.key == ticket.key).unwrap();
    assert_eq!(
        (item.group, item.status, item.change_agent),
        (
            NeedsYouGroup::Stopped,
            Status::StartInterrupted,
            Some(SessionRole::Developer)
        )
    );
    assert_eq!(item.reason, "start failed · couldn't run the model");

    // The scheduler looks many times (every 50 ms) and starts nothing.
    tokio::time::sleep(Duration::from_millis(1500)).await;
    assert_eq!(w.w.status("SHOP-1").await, Status::StartInterrupted);
    assert_eq!(agent_sessions(&w.w, &ticket).await.len(), 1);
    let posted = w.recorder.0.lock().unwrap().clone();
    assert_eq!(posted, Vec::<String>::new());

    // Retry start launches a fresh session; the ticket has started again.
    w.w.starter
        .retry("SHOP-1", &Actor::User("me".into()))
        .await
        .unwrap();
    until("the retried start", || async {
        w.w.status("SHOP-1").await == Status::Working
    })
    .await;
    until("the ticket counted as started again", || async {
        w.w.board
            .ticket("SHOP-1")
            .await
            .unwrap()
            .started_at
            .is_some()
    })
    .await;
    let after = w.w.board.ticket("SHOP-1").await.unwrap();
    assert_eq!(after.start_problem, None);
    let sessions = agent_sessions(&w.w, &ticket).await;
    assert_eq!(sessions.len(), 2);
    assert_ne!(sessions[1].0, session);
    w.w.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_new_model_alone_does_not_start_it_again() {
    let w = world().await;
    let (ticket, session) = started_on_fable(&w).await;
    hook(
        &w.w,
        &agent_caller(&ticket, &session),
        "StopFailure",
        &refused_payload(),
    )
    .await;
    assert_eq!(w.w.status("SHOP-1").await, Status::StartInterrupted);

    // Saving a new model on the agent starts nothing (and clears the mark); Retry start is the click.
    let developer = w.agents.agent(&w.fable.id).await.unwrap();
    w.agents
        .edit(
            &w.fable.id,
            &page(&developer.name, Some("sonnet"), developer.skills.clone()),
        )
        .await
        .unwrap();
    assert_eq!(mark_text(&w, &w.fable).await, None);
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert_eq!(w.w.status("SHOP-1").await, Status::StartInterrupted);
    w.w.starter
        .retry("SHOP-1", &Actor::User("me".into()))
        .await
        .unwrap();
    until("the start on the new model", || async {
        w.w.status("SHOP-1").await == Status::Working
    })
    .await;
    // It runs the agent's new model (resolution at start, AGM-05).
    let (agent, model): (Option<String>, Option<String>) = sqlx::query_as(
        "SELECT agent_id, model FROM sessions WHERE ticket_id = ? AND kind = 'agent'
         ORDER BY started_at DESC, id DESC LIMIT 1",
    )
    .bind(&ticket.id)
    .fetch_one(w.w.board.store().pool())
    .await
    .unwrap();
    assert_eq!(
        (agent.as_deref(), model.as_deref()),
        (Some(w.fable.id.as_str()), Some("sonnet"))
    );
    w.w.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_validator_round_refused_is_no_result_and_never_counts() {
    let w = world().await;
    let ticket = w.w.create("SHOP", "Coupon field").await;
    w.w.give_worktree(&ticket).await;
    let validator = w.w.shell(&ticket, SessionKind::Validator).await;
    w.w.validating_with(&ticket, &validator.session).await;
    on_agent(&w.w, &validator.session, &w.fable, Some("claude-fable-5-1")).await;
    let caller = Caller::Validator {
        ticket: ticket.key.clone(),
        ticket_id: ticket.id.clone(),
        session: validator.session.clone(),
    };

    assert_eq!(
        hook(&w.w, &caller, "StopFailure", &refused_payload()).await,
        []
    );
    let after = w.w.board.ticket("SHOP-1").await.unwrap();
    assert_eq!(
        (after.status, after.board_group(), after.validation_round),
        (Status::CheckNoResult, StatusSet::AgentStage, 0)
    );
    let text = after.signal.as_ref().and_then(|s| s.text.clone()).unwrap();
    assert!(refused_its_model(&text), "{text}");
    assert!(text.contains(TOOL_MESSAGE), "{text}");
    assert_eq!(
        w.w.sessions
            .wait_ended(&validator.session)
            .await
            .unwrap()
            .end_reason
            .as_deref(),
        Some(END_MODEL_REFUSED)
    );
    // The mark names the exact model the session reported.
    assert_eq!(
        mark_text(&w, &w.fable).await.as_deref(),
        Some("Claude Code couldn't run claude-fable-5-1 on SHOP-1")
    );
    until("the needs_you item", || async {
        w.w.needs_you
            .list()
            .await
            .unwrap()
            .items
            .iter()
            .any(|i| i.key == ticket.key)
    })
    .await;
    let items = w.w.needs_you.list().await.unwrap().items;
    let item = items.iter().find(|i| i.key == ticket.key).unwrap();
    assert_eq!(
        (item.group, item.change_agent),
        (NeedsYouGroup::Family, Some(SessionRole::Validator))
    );
    // Notified once, as today's No result.
    tokio::time::sleep(Duration::from_millis(500)).await;
    let posted = w.recorder.0.lock().unwrap().clone();
    assert_eq!(posted.len(), 1, "{posted:?}");

    // Retry validation: a fresh validator, refused again; still no round counted.
    w.w.validation
        .user_action(
            "SHOP-1",
            ValidationAction::Retry,
            "",
            &Actor::User("me".into()),
        )
        .await
        .unwrap();
    let second =
        w.w.sessions
            .running_of(&ticket.id, SessionKind::Validator)
            .expect("a fresh validator")
            .id;
    let caller = Caller::Validator {
        ticket: ticket.key.clone(),
        ticket_id: ticket.id.clone(),
        session: second,
    };
    hook(&w.w, &caller, "StopFailure", &refused_payload()).await;
    let after = w.w.board.ticket("SHOP-1").await.unwrap();
    assert_eq!(
        (after.status, after.validation_round),
        (Status::CheckNoResult, 0)
    );
    let rounds: Vec<(i64, String)> = sqlx::query_as(
        "SELECT round, outcome FROM validations WHERE ticket_id = ? ORDER BY started_at, id",
    )
    .bind(&ticket.id)
    .fetch_all(w.w.board.store().pool())
    .await
    .unwrap();
    assert_eq!(
        rounds,
        [(1, "no_result".to_owned()), (1, "no_result".to_owned())]
    );

    w.w.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_refused_resume_or_later_turn_is_a_failed_turn_with_the_mark() {
    let w = world().await;
    // A resumed session: the ticket ran an earlier session.
    let resumed = w.w.create("SHOP", "Price table").await;
    w.w.record_ended_agent(&resumed, "0199d2a4-earlier").await;
    w.w.force_status(&resumed, Status::Working).await;
    let shell = w.w.agent_shell(&resumed).await;
    on_agent(&w.w, &shell.session, &w.fable, None).await;
    let failed = Signal::Failed {
        error: MODEL_REFUSED.into(),
        message: Some(TOOL_MESSAGE.into()),
    };
    let caller = agent_caller(&resumed, &shell.session);
    assert_eq!(
        hook(&w.w, &caller, "StopFailure", &refused_payload()).await,
        std::slice::from_ref(&failed)
    );
    assert_eq!(w.w.status("SHOP-1").await, Status::Working);
    assert!(w.w.sessions.info(&shell.session).unwrap().is_running());
    assert_eq!(
        mark_text(&w, &w.fable).await.as_deref(),
        Some("Claude Code couldn't run fable on SHOP-1")
    );

    // A later turn: a first turn got through, then the next one is refused.
    let later = w.w.create("SHOP", "Fix flaky login redirect").await;
    w.w.force_status(&later, Status::Working).await;
    let shell = w.w.agent_shell(&later).await;
    on_agent(&w.w, &shell.session, &w.light, None).await;
    let caller = agent_caller(&later, &shell.session);
    hook(
        &w.w,
        &caller,
        "Stop",
        &json!({"last_assistant_message": "Done."}),
    )
    .await;
    hook(
        &w.w,
        &caller,
        "UserPromptSubmit",
        &json!({"prompt": "go on"}),
    )
    .await;
    assert_eq!(
        hook(&w.w, &caller, "StopFailure", &refused_payload()).await,
        [failed]
    );
    assert_eq!(w.w.status("SHOP-2").await, Status::Working);
    assert!(
        w.w.board
            .store()
            .developer_started(&later.id)
            .await
            .unwrap()
    );
    assert_eq!(
        mark_text(&w, &w.light).await.as_deref(),
        Some("Claude Code couldn't run haiku on SHOP-2")
    );
    w.w.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_leads_first_turn_refused_leaves_it_not_started_and_open_lead_starts_afresh() {
    let w = world().await;
    let epic =
        w.w.board
            .create_ticket(
                &NewTicketInput {
                    project: "SHOP".into(),
                    title: "Wishlist".into(),
                    kind: Kind::Epic,
                    ..Default::default()
                },
                &Actor::User("me".into()),
            )
            .await
            .unwrap();
    let leads = w.w.project_agents.leads();
    let lead = leads.open(&epic.key.to_string()).await.unwrap();
    let session = lead.agent.session.clone().unwrap().id;
    on_agent(&w.w, &session, &w.fable, None).await;
    assert!(w.w.board.store().lead_started(&epic.id).await.unwrap());
    let caller = Caller::EpicLead {
        project: lead.agent.project.clone(),
        agent: lead.agent.id.clone(),
        epic: lead.epic.clone(),
        epic_id: lead.epic_id.clone(),
        session: session.clone(),
    };
    hook(
        &w.w,
        &caller,
        "UserPromptSubmit",
        &json!({"prompt": "plan"}),
    )
    .await;
    assert_eq!(
        hook(&w.w, &caller, "StopFailure", &refused_payload()).await,
        []
    );

    let info = w.w.sessions.wait_ended(&session).await.unwrap();
    assert_eq!(info.end_reason.as_deref(), Some(END_MODEL_REFUSED));
    // Its terminal keeps the tool's message (the stand-in printed its start; the tool would print the error).
    assert!(w.w.sessions.attach(&session).is_ok());
    assert!(!w.w.board.store().lead_started(&epic.id).await.unwrap());
    assert_eq!(
        mark_text(&w, &w.fable).await.as_deref(),
        Some("Claude Code couldn't run fable on SHOP-1")
    );

    // Open lead starts a new lead (nothing to resume), and that one has started.
    let again = leads.open(&epic.key.to_string()).await.unwrap();
    assert_ne!(again.agent.id, lead.agent.id);
    assert!(w.w.board.store().lead_started(&epic.id).await.unwrap());
    w.w.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_project_agents_first_turn_refused_ends_its_session_and_marks_the_entry() {
    let w = world().await;
    let agent =
        w.w.project_agents
            .open("SHOP", "Why is checkout slow?")
            .await
            .unwrap();
    let session = agent.session.clone().unwrap().id;
    on_agent(&w.w, &session, &w.light, None).await;
    let caller = Caller::ProjectAgent {
        project: agent.project.clone(),
        agent: agent.id.clone(),
        session: session.clone(),
    };
    assert_eq!(
        hook(&w.w, &caller, "StopFailure", &refused_payload()).await,
        []
    );
    let info = w.w.sessions.wait_ended(&session).await.unwrap();
    assert_eq!(info.end_reason.as_deref(), Some(END_MODEL_REFUSED));
    assert_eq!(
        mark_text(&w, &w.light).await.as_deref(),
        Some("Claude Code couldn't run haiku on shop-web")
    );
    w.w.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn the_mark_clears_on_a_model_edit_or_a_later_first_turn_on_its_model_only() {
    let w = world().await;
    let first = w.w.create("SHOP", "Add gift wrap option").await;
    w.w.record_ended_agent(&first, "0199d2a4-earlier").await;
    w.w.force_status(&first, Status::Working).await;
    let shell = w.w.agent_shell(&first).await;
    on_agent(&w.w, &shell.session, &w.fable, None).await;
    hook(
        &w.w,
        &agent_caller(&first, &shell.session),
        "StopFailure",
        &refused_payload(),
    )
    .await;
    let marked = "Claude Code couldn't run fable on SHOP-1";
    assert_eq!(mark_text(&w, &w.fable).await.as_deref(), Some(marked));

    // A rename keeps it.
    let skills = w.agents.agent(&w.fable.id).await.unwrap().skills;
    let renamed = page("Fable trial", Some("fable"), skills);
    w.agents.edit(&w.fable.id, &renamed).await.unwrap();
    assert_eq!(mark_text(&w, &w.fable).await.as_deref(), Some(marked));

    // A first turn on another agent keeps it.
    let other = w.w.create("SHOP", "Fix README typo").await;
    w.w.force_status(&other, Status::Working).await;
    let shell = w.w.agent_shell(&other).await;
    on_agent(&w.w, &shell.session, &w.light, None).await;
    let stop = json!({"last_assistant_message": "Done."});
    hook(&w.w, &agent_caller(&other, &shell.session), "Stop", &stop).await;
    assert_eq!(mark_text(&w, &w.fable).await.as_deref(), Some(marked));

    // A later session of the agent whose first turn gets through on the agent's model clears it.
    let later = w.w.create("SHOP", "Update footer links").await;
    w.w.force_status(&later, Status::Working).await;
    let shell = w.w.agent_shell(&later).await;
    let fable = w.agents.agent(&w.fable.id).await.unwrap();
    on_agent(&w.w, &shell.session, &fable, None).await;
    hook(&w.w, &agent_caller(&later, &shell.session), "Stop", &stop).await;
    assert_eq!(mark_text(&w, &w.fable).await, None);

    // Marked again, a saved model change clears it.
    let again = w.w.create("SHOP", "Coupon field").await;
    w.w.record_ended_agent(&again, "0199d2a4-earlier-2").await;
    w.w.force_status(&again, Status::Working).await;
    let shell = w.w.agent_shell(&again).await;
    on_agent(&w.w, &shell.session, &fable, None).await;
    hook(
        &w.w,
        &agent_caller(&again, &shell.session),
        "StopFailure",
        &refused_payload(),
    )
    .await;
    assert!(mark_text(&w, &w.fable).await.is_some());
    w.agents
        .edit(
            &w.fable.id,
            &AgentFields {
                model: claude(Some("opus")),
                ..renamed
            },
        )
        .await
        .unwrap();
    assert_eq!(mark_text(&w, &w.fable).await, None);
    w.w.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_start_that_isnt_refused_is_untouched() {
    let w = world().await;
    let (ticket, session) = started_on_fable(&w).await;
    hook(
        &w.w,
        &agent_caller(&ticket, &session),
        "Stop",
        &json!({"last_assistant_message": "Done."}),
    )
    .await;
    assert_eq!(w.w.status("SHOP-1").await, Status::Idle);
    assert!(
        w.w.board
            .store()
            .developer_started(&ticket.id)
            .await
            .unwrap()
    );
    let started =
        w.w.starter
            .start(
                "SHOP-1",
                &Actor::User("me".into()),
                otaq_core::session::Start::Auto,
            )
            .await;
    assert_eq!(started.unwrap(), StartOutcome::Skipped);
    w.w.stop().await;
}
