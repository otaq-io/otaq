//! The epic lead's lifecycle (T-0279; LEAD-02, LEAD-04, LEAD-12; d0083): queueing a whole epic without a lead
//! starts one that checks the plan; a child merged or closed wakes it, typed only when it is idle; it ends at Ready
//! for Review and resumes with no message when a child is added or reopened; a restart ends it until the user's
//! click; wake-ups wait during the usage limit; handovers are handed back at the end of the lead's turn and when it
//! ends. `claude` stands in as `sh` printing its arguments, then `cat`: no test starts claude.

use otaq_core::service::testing::said;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use otaq_core::agent_tools::LaunchContext;
use otaq_core::agent_tools::Tool;
use otaq_core::agent_tools::claude::ClaudeLaunch;
use otaq_core::board::tickets::{EventKind, NewTicketInput, Ticket};
use otaq_core::caller::Caller;
use otaq_core::client::{Client, ClientConfig};
use otaq_core::domain::validation::ValidationTrigger;
use otaq_core::domain::{Actor, Kind, Status, TicketKey};
use otaq_core::epic_lead::wake::Handover;
use otaq_core::git::testing::commit;
use otaq_core::naming;
use otaq_core::project_agents::AgentState;
use otaq_core::project_agents::lead::{Lead, LeadStatus, Wake};
use otaq_core::protocol::{Reply, Request};
use otaq_core::review::LocalMergeOutcome;
use otaq_core::service::ServiceConfig;
use otaq_core::service::testing::TestService;
use otaq_core::store::now_ms;
use otaq_core::usage::limit::{LimitHit, LimitKind};
use serde_json::{Value, json};

fn fake_launch() -> LaunchContext {
    LaunchContext {
        bin_dir: "/nonexistent".into(),
        plugin_dir: "/nonexistent/plugin".into(),
        path: std::env::var_os("PATH").unwrap_or_default(),
        claude: ClaudeLaunch {
            program: vec![
                "sh".into(),
                "-c".into(),
                "printf 'ARGS %s\\n' \"$*\"; exec cat".into(),
                "sh".into(),
            ],
            config_dir: None,
        },
    }
}

fn with_fake_claude(config: ServiceConfig) -> ServiceConfig {
    ServiceConfig {
        launch: Some(fake_launch()),
        ..config
    }
}

fn me() -> Actor {
    Actor::User("me".into())
}

async fn world() -> TestService {
    let w = TestService::start_with(with_fake_claude).await;
    w.add_project("shop-web").await;
    w
}

async fn epic(w: &TestService, title: &str) -> Ticket {
    w.board
        .create_ticket(
            &NewTicketInput {
                project: "SHOP".into(),
                title: title.into(),
                kind: Kind::Epic,
                ..Default::default()
            },
            &me(),
        )
        .await
        .unwrap()
}

async fn child(w: &TestService, epic: &Ticket, title: &str) -> Ticket {
    w.board
        .create_ticket(
            &NewTicketInput {
                project: "SHOP".into(),
                title: title.into(),
                epic: Some(epic.key.to_string()),
                ..Default::default()
            },
            &me(),
        )
        .await
        .unwrap()
}

/// Works on a child (one file) and Local-merges it into the epic branch.
async fn merge_child(w: &TestService, child: &Ticket, file: &str) {
    let wt = w.start_worktree(child).await;
    commit(&wt, file, &format!("{file}\n"));
    w.force_status(child, Status::WaitingForYou).await;
    let outcome = w
        .review
        .local_merge(
            &child.key.to_string(),
            &format!("{}: {}", child.key, child.title),
            false,
            &me(),
        )
        .await
        .unwrap();
    assert!(
        matches!(outcome, LocalMergeOutcome::Merged { .. }),
        "{outcome:?}"
    );
}

fn key(t: &Ticket) -> String {
    t.key.to_string()
}

async fn lead(w: &TestService, epic: &Ticket) -> Option<Lead> {
    w.project_agents.leads().get(&key(epic)).await.unwrap()
}

fn session(lead: &Lead) -> String {
    lead.agent.session.as_ref().expect("a session").id.clone()
}

fn caller(lead: &Lead) -> Caller {
    Caller::EpicLead {
        project: lead.agent.project.clone(),
        agent: lead.agent.id.clone(),
        epic: lead.epic.clone(),
        epic_id: lead.epic_id.clone(),
        session: session(lead),
    }
}

/// A hook of the lead as the service handles it: the signals map it, the Project agents apply it, and a turn that
/// ended lets waiting deliveries go.
async fn hook(w: &TestService, lead: &Lead, event: &str, payload: Value) {
    let signals = w
        .signals
        .hook(&caller(lead), Tool::Claude, event, &payload)
        .await
        .unwrap();
    for signal in &signals {
        w.project_agents.hook(&lead.agent.id, signal).await.unwrap();
        if signal.ends_turn() {
            w.deliveries.deliver_if_ready(&session(lead)).await.unwrap();
        }
    }
}

async fn prompt(w: &TestService, lead: &Lead, text: &str) {
    hook(w, lead, "UserPromptSubmit", json!({ "prompt": text })).await;
}

async fn stop(w: &TestService, lead: &Lead) {
    hook(
        w,
        lead,
        "Stop",
        json!({ "last_assistant_message": "Done." }),
    )
    .await;
}

/// The wake-ups waiting for the lead's session.
async fn pending(w: &TestService, lead: &Lead) -> Vec<String> {
    w.deliveries
        .pending(&session(lead))
        .await
        .unwrap()
        .into_iter()
        .map(|d| {
            assert_eq!(d.kind, "lead_wake");
            d.text
        })
        .collect()
}

async fn wait_until<F, Fut>(what: &str, mut check: F)
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = bool>,
{
    tokio::time::timeout(Duration::from_secs(10), async {
        while !check().await {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("timed out waiting: {what}"));
}

async fn wait_state(w: &TestService, epic: &Ticket, state: AgentState) -> Lead {
    wait_until("the lead's state", || async {
        lead(w, epic).await.is_some_and(|l| l.agent.state == state)
    })
    .await;
    lead(w, epic).await.unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn queueing_a_whole_epic_starts_its_lead_once() {
    let w = world().await;
    let search = epic(&w, "Search").await;
    child(&w, &search, "Search index API").await;
    child(&w, &search, "Search results page").await;

    let outcome = w.board.queue_epic(&key(&search), &me()).await.unwrap();
    assert!(outcome.lead_started);
    assert_eq!(outcome.queued.len(), 2);
    let started = lead(&w, &search).await.expect("a lead");
    assert_eq!(started.status, LeadStatus::CheckingPlan);
    assert_eq!(started.name, "Search lead");
    let out = w.wait_for_output(&session(&started), "ARGS").await;
    assert!(
        out.contains(&format!(
            "{} was queued. Check the plan before the first child starts: `{} prime --epic`.",
            search.key,
            naming::COMMAND_NAME
        )),
        "{out}"
    );

    // Queueing one child row starts no lead.
    let wishlist = epic(&w, "Wishlist").await;
    let api = child(&w, &wishlist, "Wishlist API").await;
    child(&w, &wishlist, "Wishlist button").await;
    let single = w.board.queue_epic_child(&key(&api), &me()).await.unwrap();
    assert!(!single.lead_started);
    assert!(lead(&w, &wishlist).await.is_none());

    // An ended lead isn't restarted by a drag: the user resumes it.
    let leads = w.project_agents.leads();
    let opened = leads.open(&key(&wishlist)).await.unwrap();
    w.sessions.stop(&session(&opened)).await.unwrap();
    wait_state(&w, &wishlist, AgentState::Ended).await;
    let whole = w.board.queue_epic(&key(&wishlist), &me()).await.unwrap();
    assert!(!whole.lead_started);
    let after = lead(&w, &wishlist).await.unwrap();
    assert_eq!(after.agent.id, opened.agent.id);
    assert_eq!(after.agent.state, AgentState::Ended);
    w.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_merge_and_a_close_wake_the_lead_once_each_typed_only_when_idle() {
    let w = world().await;
    let wishlist = epic(&w, "Wishlist").await;
    let api = child(&w, &wishlist, "Wishlist API").await;
    let export = child(&w, &wishlist, "Wishlist export (CSV)").await;
    let page = child(&w, &wishlist, "Wishlist page").await;
    let open = w
        .project_agents
        .leads()
        .open(&key(&wishlist))
        .await
        .unwrap();
    w.wait_for_output(&session(&open), "ARGS").await;

    // Mid-turn: the merge's wake-up waits.
    prompt(&w, &open, "Plan it").await;
    merge_child(&w, &api, "api.ts").await;
    let merged = format!("{} merged: Wishlist API", api.key);
    assert_eq!(pending(&w, &open).await, std::slice::from_ref(&merged));
    assert_eq!(
        w.project_agents
            .leads()
            .status(&key(&wishlist))
            .await
            .unwrap(),
        LeadStatus::Replanning
    );
    // Its turn ends: typed, and the next turn is a re-plan.
    stop(&w, &open).await;
    assert!(pending(&w, &open).await.is_empty());
    w.wait_for_output(&session(&open), &merged).await;
    prompt(&w, &open, &merged).await;
    assert_eq!(
        w.project_agents
            .leads()
            .status(&key(&wishlist))
            .await
            .unwrap(),
        LeadStatus::Replanning
    );
    // The board's Lead Status reads what woke it (T-0289), from `lead.list`.
    let listed = w
        .project_agents
        .leads()
        .list(&open.agent.project)
        .await
        .unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].epic, wishlist.key);
    assert_eq!(listed[0].woke_by.as_deref(), Some(merged.as_str()));
    stop(&w, &open).await;
    assert_eq!(lead(&w, &wishlist).await.unwrap().woke_by, None);

    // Idle: the close's wake-up is typed at once.
    w.review.close(&key(&export), true, &me()).await.unwrap();
    let closed = format!(
        "{} closed without merging: Wishlist export (CSV)",
        export.key
    );
    assert!(pending(&w, &open).await.is_empty());
    w.wait_for_output(&session(&open), &closed).await;
    prompt(&w, &open, &closed).await;
    stop(&w, &open).await;
    tokio::time::sleep(Duration::from_millis(200)).await;
    let typed = w.wait_for_output(&session(&open), &closed).await;

    // Nothing else wakes it: an added child, a queued child, a reopened child, a submit and its validation.
    let share = child(&w, &wishlist, "Wishlist share link").await;
    w.board.queue_epic_child(&key(&share), &me()).await.unwrap();
    w.board.reopen(&key(&export), &me()).await.unwrap();
    let wt = w.start_worktree(&page).await;
    commit(&wt, "page.ts", "page\n");
    w.force_status(&page, Status::Working).await;
    w.validation
        .submit(
            &key(&page),
            said(&w.board, &key(&page), "The page lists the items.")
                .await
                .as_ref(),
            &Actor::Agent(page.key.clone()),
            ValidationTrigger::Agent,
        )
        .await
        .unwrap();
    assert_eq!(w.status(&key(&page)).await, Status::Checking);
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert!(pending(&w, &open).await.is_empty());
    let out = w.wait_for_output(&session(&open), &closed).await;
    assert_eq!(out, typed, "nothing more was typed");

    // With no live lead nothing is queued.
    w.sessions.stop(&session(&open)).await.unwrap();
    wait_state(&w, &wishlist, AgentState::Ended).await;
    assert!(
        !w.project_agents
            .leads()
            .wake(&key(&wishlist), Wake::Merge, "dropped")
            .await
            .unwrap()
    );
    assert!(pending(&w, &open).await.is_empty());
    w.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn the_lead_ends_at_review_and_resumes_quietly_when_a_child_is_added_or_reopened() {
    let w = world().await;
    let wishlist = epic(&w, "Wishlist").await;
    let api = child(&w, &wishlist, "Wishlist API").await;
    let export = child(&w, &wishlist, "Wishlist export (CSV)").await;
    let leads = w.project_agents.leads();
    let open = leads.open(&key(&wishlist)).await.unwrap();
    w.wait_for_output(&session(&open), "ARGS").await;

    w.review.close(&key(&export), true, &me()).await.unwrap();
    merge_child(&w, &api, "api.ts").await;
    assert_eq!(w.status(&key(&wishlist)).await, Status::WaitingForYou);
    let ended = lead(&w, &wishlist).await.unwrap();
    assert_eq!(ended.agent.state, AgentState::Ended);
    assert_eq!(ended.status, LeadStatus::EndedInReview);
    wait_until("the lead's session ends", || async {
        !lead(&w, &wishlist)
            .await
            .unwrap()
            .agent
            .session
            .unwrap()
            .is_running()
    })
    .await;
    let history = w.board.history(&key(&wishlist)).await.unwrap();
    let row = history
        .iter()
        .find(|e| e.kind == EventKind::LeadEnded)
        .expect("Lead ended in History");
    assert_eq!(row.payload["lead"], "Wishlist lead");
    assert!(
        !leads.wake(&key(&wishlist), Wake::Merge, "x").await.unwrap(),
        "no live lead"
    );

    // Reopening the closed child makes the epic a track again: the same conversation, no message.
    w.board.reopen(&key(&export), &me()).await.unwrap();
    let resumed = wait_state(&w, &wishlist, AgentState::Running).await;
    assert_eq!(resumed.agent.id, open.agent.id);
    assert_eq!(resumed.status, LeadStatus::Idle);
    let out = w.wait_for_output(&session(&resumed), "ARGS").await;
    assert!(
        out.contains(&format!(
            "--resume {}",
            open.agent.tool_session_id.as_deref().unwrap()
        )),
        "{out}"
    );
    assert!(!out.contains("was queued"), "no message: {out}");

    // Back in review, then a new child: resumed again.
    w.review.close(&key(&export), true, &me()).await.unwrap();
    assert_eq!(w.status(&key(&wishlist)).await, Status::WaitingForYou);
    wait_state(&w, &wishlist, AgentState::Ended).await;
    child(&w, &wishlist, "Wishlist share link").await;
    assert_eq!(w.status(&key(&wishlist)).await, Status::Backlog);
    let again = wait_state(&w, &wishlist, AgentState::Running).await;
    assert_eq!(again.agent.id, open.agent.id);
    assert_eq!(again.status, LeadStatus::Idle);
    w.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_fix_child_from_the_epics_review_resumes_its_lead() {
    let w = world().await;
    let wishlist = epic(&w, "Wishlist").await;
    let api = child(&w, &wishlist, "Wishlist API").await;
    let leads = w.project_agents.leads();
    let open = leads.open(&key(&wishlist)).await.unwrap();
    w.wait_for_output(&session(&open), "ARGS").await;
    let decision = otaq_core::decisions::record(
        &w.board,
        &key(&api),
        &otaq_core::decisions::NewDecision {
            question: "Keep items in the order they were added?".into(),
            built: "newest first".into(),
            other_options: vec!["oldest first".into()],
            why: "the plan doesn't say".into(),
        },
        &Actor::Agent(api.key.clone()),
    )
    .await
    .unwrap();
    merge_child(&w, &api, "api.ts").await;
    assert_eq!(w.status(&key(&wishlist)).await, Status::WaitingForYou);
    wait_state(&w, &wishlist, AgentState::Ended).await;

    // Change… → Add a fix child, in the epic's review: the epic is a track again and its lead resumes quietly.
    otaq_core::decisions::outcome::change_with_fix_child(
        &w.board,
        &decision.id.to_string(),
        "Change: oldest first",
        "List the items oldest first.",
        &me(),
    )
    .await
    .unwrap();
    assert_eq!(w.status(&key(&wishlist)).await, Status::Backlog);
    let resumed = wait_state(&w, &wishlist, AgentState::Running).await;
    assert_eq!(resumed.agent.id, open.agent.id);
    assert_eq!(resumed.status, LeadStatus::Idle);
    let out = w.wait_for_output(&session(&resumed), "ARGS").await;
    assert!(
        out.contains(&format!(
            "--resume {}",
            open.agent.tool_session_id.as_deref().unwrap()
        )),
        "{out}"
    );
    assert!(!out.contains("was queued"), "no message: {out}");
    assert!(pending(&w, &resumed).await.is_empty());
    w.stop().await;
}

/// T-0456 (PAU-04; d0112): a child added while the project is paused doesn't resume the lead that ended in review:
/// it becomes Paused, and the project's Resume resumes it on its conversation.
#[tokio::test(flavor = "multi_thread")]
async fn a_child_added_while_paused_leaves_the_lead_paused_until_resume() {
    let w = world().await;
    let wishlist = epic(&w, "Wishlist").await;
    let api = child(&w, &wishlist, "Wishlist API").await;
    let open = w
        .project_agents
        .leads()
        .open(&key(&wishlist))
        .await
        .unwrap();
    w.wait_for_output(&session(&open), "ARGS").await;
    merge_child(&w, &api, "api.ts").await;
    wait_state(&w, &wishlist, AgentState::Ended).await;
    let shop = otaq_core::agents::pause::PauseScope::Project {
        project: "SHOP".into(),
    };
    w.pause.pause(&shop).await.unwrap();

    child(&w, &wishlist, "Wishlist share link").await;
    let kept = wait_state(&w, &wishlist, AgentState::Paused).await;
    assert!(!kept.agent.session.is_some_and(|s| s.is_running()));

    w.pause.resume(&shop, &me()).await.unwrap();
    let resumed = wait_state(&w, &wishlist, AgentState::Running).await;
    assert_eq!(resumed.agent.id, open.agent.id);
    let out = w.wait_for_output(&session(&resumed), "ARGS").await;
    assert!(
        out.contains(&format!(
            "--resume {}",
            open.agent.tool_session_id.as_deref().unwrap()
        )),
        "{out}"
    );
    for s in w
        .sessions
        .in_memory()
        .into_iter()
        .filter(|s| s.is_running())
    {
        let _ = w.sessions.stop(&s.id).await;
    }
    w.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn after_a_restart_the_lead_waits_for_the_users_click_and_is_counted_on_its_own() {
    let w = world().await;
    let wishlist = epic(&w, "Wishlist").await;
    let api = child(&w, &wishlist, "Wishlist API").await;
    let open = w
        .project_agents
        .leads()
        .open(&key(&wishlist))
        .await
        .unwrap();

    let w = w.restart_with(with_fake_claude).await;
    let after = lead(&w, &wishlist).await.unwrap();
    assert_eq!(after.agent.state, AgentState::EndedByRestart);
    assert_eq!(after.status, LeadStatus::EndedByRestart);
    let client = Client::connect(ClientConfig::new(&w.data)).await.unwrap();
    match client.request(Request::StartupReport).await.unwrap() {
        Reply::StartupReport(r) => {
            assert_eq!(r.leads_ended, std::slice::from_ref(&open.agent.id));
            assert!(r.project_agents_ended.is_empty(), "{r:?}");
            assert!(r.interrupted.is_empty(), "not part of Resume all: {r:?}");
        }
        reply => panic!("{reply:?}"),
    }
    // No live lead: nothing wakes it, and a new child doesn't resume it.
    assert!(
        !w.project_agents
            .leads()
            .wake(&key(&wishlist), Wake::Close, "x")
            .await
            .unwrap()
    );
    child(&w, &wishlist, "Wishlist share link").await;
    let still = lead(&w, &wishlist).await.unwrap();
    assert_eq!(still.agent.state, AgentState::EndedByRestart);
    let _ = api;
    // The user's Resume continues its conversation, told to carry on (d0122).
    let resumed = w
        .project_agents
        .leads()
        .resume(&key(&wishlist))
        .await
        .unwrap();
    let out = w.wait_for_output(&session(&resumed), "ARGS").await;
    assert!(
        out.contains(&format!(
            "--resume {} {}",
            open.agent.tool_session_id.as_deref().unwrap(),
            otaq_core::recovery::CARRY_ON_PROJECT_AGENT
        )),
        "{out}"
    );
    w.sessions.stop(&session(&resumed)).await.unwrap();
    w.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn wake_ups_wait_only_during_the_leads_tools_limit_and_its_resume_types_them() {
    let w = world().await;
    let wishlist = epic(&w, "Wishlist").await;
    let open = w
        .project_agents
        .leads()
        .open(&key(&wishlist))
        .await
        .unwrap();
    w.wait_for_output(&session(&open), "ARGS").await;
    prompt(&w, &open, "Plan it").await;
    stop(&w, &open).await;

    let hit = LimitHit {
        kind: LimitKind::Session,
        text: "You've hit your session limit · resets 3:45pm".into(),
    };
    let lead_tool = open.agent.session.as_ref().and_then(|s| s.tool).unwrap();
    let other = *Tool::ALL.iter().find(|&&t| t != lead_tool).unwrap();
    let leads = w.project_agents.leads();
    // Another tool at its limit: the lead's wake-up goes through (USG-10).
    w.plan_limit
        .record_hit(other, &hit, now_ms())
        .await
        .unwrap();
    assert!(
        leads
            .wake(&key(&wishlist), Wake::Merge, "SHOP-1.2 merged: Wishlist UI")
            .await
            .unwrap()
    );
    w.wait_for_output(&session(&open), "SHOP-1.2 merged: Wishlist UI")
        .await;
    assert!(pending(&w, &open).await.is_empty());
    stop(&w, &open).await;

    // Its own tool at its limit: the wake-up waits; the other tool's Resume doesn't type it, its own does.
    w.plan_limit
        .record_hit(lead_tool, &hit, now_ms())
        .await
        .unwrap();
    assert!(
        leads
            .wake(
                &key(&wishlist),
                Wake::Merge,
                "SHOP-1.1 merged: Wishlist API"
            )
            .await
            .unwrap()
    );
    assert_eq!(pending(&w, &open).await, ["SHOP-1.1 merged: Wishlist API"]);
    w.plan_limit.resume_tool(other).await.unwrap();
    assert_eq!(pending(&w, &open).await, ["SHOP-1.1 merged: Wishlist API"]);
    w.plan_limit.resume_tool(lead_tool).await.unwrap();
    assert!(pending(&w, &open).await.is_empty());
    w.wait_for_output(&session(&open), "SHOP-1.1 merged: Wishlist API")
        .await;
    w.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_dummy_handover_is_handed_back_at_turn_end_and_when_the_lead_ends() {
    let w = world().await;
    let wishlist = epic(&w, "Wishlist").await;
    let leads = w.project_agents.leads();
    let back: Arc<Mutex<Vec<Handover>>> = Arc::default();
    {
        let back = back.clone();
        leads.on_hand_back(
            "dummy",
            Arc::new(move |h| {
                let back = back.clone();
                Box::pin(async move { back.lock().unwrap().push(h) })
            }),
        );
    }
    let child_key: TicketKey = format!("{}.2", wishlist.key).parse().unwrap();
    let handed = || {
        back.lock()
            .unwrap()
            .iter()
            .map(|h| h.reference.clone())
            .collect::<Vec<_>>()
    };

    // No live lead: nothing registered.
    assert_eq!(
        leads
            .wake_with_handover(
                &key(&wishlist),
                Wake::Question {
                    ticket: child_key.clone()
                },
                "asks 6",
                "dummy",
                &child_key,
                "6",
            )
            .await
            .unwrap(),
        None
    );

    let open = leads.open(&key(&wishlist)).await.unwrap();
    w.wait_for_output(&session(&open), "ARGS").await;
    prompt(&w, &open, "Plan it").await;
    let question = Wake::Question {
        ticket: child_key.clone(),
    };
    // Sent mid-turn: that turn's end doesn't hand it back; the turn it is typed in does.
    leads
        .wake_with_handover(
            &key(&wishlist),
            question.clone(),
            "asks 7",
            "dummy",
            &child_key,
            "7",
        )
        .await
        .unwrap()
        .expect("registered");
    stop(&w, &open).await;
    assert!(handed().is_empty());
    assert_eq!(leads.handovers_of(&key(&wishlist)).await.unwrap().len(), 1);
    prompt(&w, &open, "asks 7").await;
    assert_eq!(
        leads.status(&key(&wishlist)).await.unwrap(),
        LeadStatus::Answering {
            ticket: child_key.clone()
        }
    );
    stop(&w, &open).await;
    assert_eq!(handed(), ["7"]);
    assert!(
        leads
            .handovers_of(&key(&wishlist))
            .await
            .unwrap()
            .is_empty()
    );

    // Settled in its turn: not handed back.
    leads
        .wake_with_handover(
            &key(&wishlist),
            question.clone(),
            "asks 8",
            "dummy",
            &child_key,
            "8",
        )
        .await
        .unwrap()
        .expect("registered");
    prompt(&w, &open, "asks 8").await;
    assert!(leads.settle_handover("dummy", "8"));
    stop(&w, &open).await;
    assert_eq!(handed(), ["7"]);

    // The lead ends while one is open: handed back.
    leads
        .wake_with_handover(
            &key(&wishlist),
            question,
            "asks 9",
            "dummy",
            &child_key,
            "9",
        )
        .await
        .unwrap()
        .expect("registered");
    w.sessions.stop(&session(&open)).await.unwrap();
    wait_until("the hand-back", || async { handed().len() == 2 }).await;
    assert_eq!(handed(), ["7", "9"]);
    w.stop().await;
}
