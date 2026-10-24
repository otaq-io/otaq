//! The epic lead's session (T-0278; LEAD-01, LEAD-02, LEAD-11, LEAD-12; d0083): Open lead starts `claude` with the
//! epic lead role, the epic and the skill in the project's folder; Resume continues the conversation; the
//! refusals (a live lead, an epic in review, the usage limit); a drag's start checks the plan; leads are left out
//! of every Project agent list and can't be renamed; every status from the hooks and the turn reasons; tokens count
//! on the epic and today, never toward the agent limit.
//! `claude` stands in as `sh` printing its arguments and environment, then `cat`: no test starts claude.

use otaq_core::agent_tools::LaunchContext;
use otaq_core::agent_tools::Tool;
use otaq_core::agent_tools::claude::ClaudeLaunch;
use otaq_core::board::Refusal;
use otaq_core::board::tickets::{NewTicketInput, Ticket};
use otaq_core::caller::Caller;
use otaq_core::client::{Client, ClientConfig};
use otaq_core::domain::{Actor, Kind, Status};
use otaq_core::naming;
use otaq_core::project_agents::lead::{
    LEAD_LIMIT_REFUSAL, Lead, LeadStatus, NAMED_AFTER_EPIC, TurnReason, Wake,
};
use otaq_core::project_agents::{AgentState, ProjectAgent};
use otaq_core::protocol::{Event, LeadParams, Reply, Request};
use otaq_core::service::ServiceConfig;
use otaq_core::service::testing::TestService;
use otaq_core::session::SessionKind;
use otaq_core::store::now_ms;
use otaq_core::usage::limit::{LimitHit, LimitKind};
use serde_json::{Value, json};

fn fake_launch() -> LaunchContext {
    let script = format!(
        "printf 'ARGS %s\\n' \"$*\"; printf 'ROLE=%s EPIC=%s TICKET=%s\\n' \"${role}\" \"${epic}\" \
         \"${{{ticket}-none}}\"; printf 'CWD=%s\\n' \"$(pwd -W 2>/dev/null || pwd -P)\"; exec cat",
        role = naming::role_env_var(),
        epic = naming::epic_env_var(),
        ticket = naming::ticket_env_var(),
    );
    LaunchContext {
        bin_dir: "/nonexistent".into(),
        plugin_dir: "/nonexistent/plugin".into(),
        path: std::env::var_os("PATH").unwrap_or_default(),
        claude: ClaudeLaunch {
            program: vec!["sh".into(), "-c".into(), script, "sh".into()],
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

async fn epic(w: &TestService, title: &str) -> Ticket {
    w.board
        .create_ticket(
            &NewTicketInput {
                project: "SHOP".into(),
                title: title.into(),
                kind: Kind::Epic,
                ..Default::default()
            },
            &Actor::User("me".into()),
        )
        .await
        .unwrap()
}

/// shop-web with the epic SHOP-1 "Wishlist".
async fn world() -> (TestService, Ticket) {
    let w = TestService::start_with(with_fake_claude).await;
    w.add_project("shop-web").await;
    let wishlist = epic(&w, "Wishlist").await;
    (w, wishlist)
}

fn key(epic: &Ticket) -> String {
    epic.key.to_string()
}

fn caller(lead: &Lead) -> Caller {
    Caller::EpicLead {
        project: lead.agent.project.clone(),
        agent: lead.agent.id.clone(),
        epic: lead.epic.clone(),
        epic_id: lead.epic_id.clone(),
        session: session(&lead.agent),
    }
}

fn session(agent: &ProjectAgent) -> String {
    agent.session.as_ref().expect("a session").id.clone()
}

/// A hook of the lead, as the service handles it: the signals map it, the Project agents apply it.
async fn hook(w: &TestService, lead: &Lead, event: &str, payload: Value) {
    let signals = w
        .signals
        .hook(&caller(lead), Tool::Claude, event, &payload)
        .await
        .unwrap();
    for signal in &signals {
        w.project_agents.hook(&lead.agent.id, signal).await.unwrap();
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

async fn status(w: &TestService, epic: &Ticket) -> LeadStatus {
    w.project_agents.leads().status(&key(epic)).await.unwrap()
}

fn refusal(e: otaq_core::board::BoardError) -> (Option<Refusal>, String) {
    (e.refusal(), e.to_string())
}

async fn wait_ended(w: &TestService, epic: &Ticket) -> Lead {
    tokio::time::timeout(std::time::Duration::from_secs(10), async {
        loop {
            let lead = w.project_agents.leads().get(&key(epic)).await.unwrap();
            if let Some(lead) = lead.filter(|l| l.agent.state == AgentState::Ended) {
                return lead;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("the lead ended")
}

#[tokio::test(flavor = "multi_thread")]
async fn open_lead_starts_claude_with_the_role_the_epic_and_the_skill() {
    let (w, wishlist) = world().await;
    let mut events = w.events.subscribe();
    assert_eq!(status(&w, &wishlist).await, LeadStatus::None);
    assert!(
        w.project_agents
            .leads()
            .get(&key(&wishlist))
            .await
            .unwrap()
            .is_none()
    );
    let repo = std::fs::canonicalize(w.board.project("SHOP").await.unwrap().repo_path).unwrap();

    let lead = w
        .project_agents
        .leads()
        .open(&key(&wishlist))
        .await
        .unwrap();
    assert_eq!(lead.name, "Wishlist lead");
    assert_eq!(lead.agent.name, "Wishlist lead");
    assert_eq!(lead.agent.name_source, None);
    assert_eq!(lead.epic, wishlist.key);
    assert_eq!(lead.agent.epic.as_ref(), Some(&wishlist.key));
    assert_eq!(lead.status, LeadStatus::Idle, "no first message: idle");
    let session = lead.agent.session.as_ref().unwrap();
    assert_eq!(session.kind, SessionKind::ProjectAgent);
    assert_eq!(session.ticket, None);
    assert_eq!(
        (session.tool, session.model.as_deref()),
        (Some(otaq_core::agent_tools::Tool::Claude), None)
    );
    assert_eq!(session.tool_session_id, lead.agent.tool_session_id);
    assert_eq!(
        (lead.agent.tool, lead.agent.model.as_deref()),
        (otaq_core::agent_tools::Tool::Claude, None)
    );
    let out = w.wait_for_output(&session.id, "CWD=").await;
    assert!(
        out.contains(&format!(
            "--session-id {} /{}-epic-lead\r\n",
            lead.agent.tool_session_id.as_deref().unwrap(),
            naming::COMMAND_NAME
        )) || out.contains(&format!(
            "--session-id {} /{}-epic-lead\n",
            lead.agent.tool_session_id.as_deref().unwrap(),
            naming::COMMAND_NAME
        )),
        "the skill and no message: {out}"
    );
    assert!(
        out.contains(&format!("--allowedTools Bash({}:*)", naming::COMMAND_NAME)),
        "{out}"
    );
    assert!(!out.contains("permission"), "{out}");
    assert!(
        out.contains(&format!(
            "ROLE={} EPIC={} TICKET=none",
            naming::ROLE_EPIC_LEAD,
            wishlist.key
        )),
        "{out}"
    );
    let repo_text = repo.display().to_string();
    let repo_text = if cfg!(windows) {
        repo_text.trim_start_matches(r"\\?\").replace('\\', "/")
    } else {
        repo_text
    };
    assert!(out.contains(&format!("CWD={repo_text}")), "{out}");

    let mut changed = false;
    while let Ok(event) = events.try_recv() {
        changed |= matches!(&event, Event::LeadChanged { project, epic }
            if project == "SHOP" && *epic == wishlist.key);
    }
    assert!(changed, "LeadChanged fired");

    // Over the socket: the user's click.
    let client = Client::connect(ClientConfig::new(&w.data)).await.unwrap();
    let other = epic(&w, "Search").await;
    match client
        .request(Request::LeadOpen(LeadParams { epic: key(&other) }))
        .await
        .unwrap()
    {
        Reply::Lead(lead) => assert_eq!(lead.name, "Search lead"),
        reply => panic!("{reply:?}"),
    }
    w.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn open_is_refused_for_a_live_lead_an_epic_in_review_and_a_plain_ticket() {
    let (w, wishlist) = world().await;
    let leads = w.project_agents.leads();
    leads.open(&key(&wishlist)).await.unwrap();
    let (code, text) = refusal(leads.open(&key(&wishlist)).await.unwrap_err());
    assert_eq!(code, Some(Refusal::AlreadyRunning));
    assert_eq!(text, "Wishlist lead is already open");
    let (code, _) = refusal(leads.start_for_queue(&key(&wishlist)).await.unwrap_err());
    assert_eq!(code, Some(Refusal::AlreadyRunning), "one lead per epic");
    let rows: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM project_agents WHERE epic_id = ?")
        .bind(&wishlist.id)
        .fetch_one(w.board.store().pool())
        .await
        .unwrap();
    assert_eq!(rows, 1);

    for status in [Status::WaitingForYou, Status::PrOpen, Status::Merged] {
        let search = epic(&w, "Search").await;
        w.force_status(&search, status).await;
        for result in [
            leads.open(&key(&search)).await,
            leads.resume(&key(&search)).await,
        ] {
            let (code, text) = refusal(result.unwrap_err());
            assert_eq!(code, Some(Refusal::MoveRefused), "{status:?}");
            assert_eq!(
                text,
                format!("{} is in review; its lead has ended", search.key)
            );
        }
    }
    let plain = w.create("SHOP", "Checkout API").await;
    let (code, _) = refusal(leads.open(&key(&plain)).await.unwrap_err());
    assert_eq!(code, Some(Refusal::NotAnEpic));
    w.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn an_ended_lead_is_resumed_not_started_twice_and_after_a_restart_waits_for_resume() {
    let (w, wishlist) = world().await;
    let leads = w.project_agents.leads();
    let first = leads.open(&key(&wishlist)).await.unwrap();
    w.sessions.stop(&session(&first.agent)).await.unwrap();
    let ended = wait_ended(&w, &wishlist).await;
    assert_eq!(ended.status, LeadStatus::Ended);

    // Open lead on an ended lead resumes the same conversation.
    let resumed = leads.open(&key(&wishlist)).await.unwrap();
    assert_eq!(resumed.agent.id, first.agent.id);
    let resumed_session = resumed.agent.session.as_ref().unwrap();
    assert_eq!(resumed_session.tool_session_id, first.agent.tool_session_id);
    assert_eq!(
        (resumed_session.tool, resumed_session.model.as_deref()),
        (Some(otaq_core::agent_tools::Tool::Claude), None)
    );
    assert_eq!(resumed.status, LeadStatus::Idle);
    let out = w.wait_for_output(&session(&resumed.agent), "CWD=").await;
    assert!(
        out.contains(&format!(
            "--resume {}",
            first.agent.tool_session_id.as_deref().unwrap()
        )),
        "{out}"
    );
    assert!(
        !out.contains(&format!("/{}", naming::copy_name("epic-lead"))),
        "no skill on resume: {out}"
    );
    assert!(
        out.contains(&format!("ROLE={}", naming::ROLE_EPIC_LEAD)),
        "{out}"
    );
    // Resume on a live lead changes nothing.
    let again = leads.resume(&key(&wishlist)).await.unwrap();
    assert_eq!(session(&again.agent), session(&resumed.agent));

    // A restart: "ended by restart", kept although it never got a request, and not a Project agent in the report.
    let w = w.restart_with(with_fake_claude).await;
    let leads = w.project_agents.leads();
    let after = leads.get(&key(&wishlist)).await.unwrap().expect("kept");
    assert_eq!(after.agent.state, AgentState::EndedByRestart);
    assert_eq!(after.status, LeadStatus::EndedByRestart);
    let client = Client::connect(ClientConfig::new(&w.data)).await.unwrap();
    match client.request(Request::StartupReport).await.unwrap() {
        Reply::StartupReport(r) => assert!(r.project_agents_ended.is_empty(), "{r:?}"),
        reply => panic!("{reply:?}"),
    }
    // Resume clears it.
    let back = leads.resume(&key(&wishlist)).await.unwrap();
    assert_eq!(back.agent.state, AgentState::Running);
    assert_eq!(back.status, LeadStatus::Idle);
    let out = w.wait_for_output(&session(&back.agent), "CWD=").await;
    assert!(
        out.contains(&format!("ROLE={}", naming::ROLE_EPIC_LEAD)),
        "{out}"
    );
    w.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn open_and_resume_are_refused_during_the_usage_limit() {
    let (w, wishlist) = world().await;
    let search = epic(&w, "Search").await;
    let leads = w.project_agents.leads();
    let lead = leads.open(&key(&search)).await.unwrap();
    w.sessions.stop(&session(&lead.agent)).await.unwrap();
    wait_ended(&w, &search).await;
    let hit = LimitHit {
        kind: LimitKind::Session,
        text: "You've hit your session limit · resets 3:45pm".into(),
    };
    w.plan_limit
        .record_hit(Tool::Claude, &hit, now_ms())
        .await
        .unwrap();
    // A new lead is held by the check before every launch (its tool is at its limit, T-0357); a resume is refused.
    let (code, text) = refusal(leads.open(&key(&wishlist)).await.unwrap_err());
    assert_eq!(code, Some(Refusal::MoveRefused));
    assert_eq!(
        text,
        format!("{} limit reached", Tool::Claude.display_name())
    );
    for result in [
        leads.open(&key(&search)).await,
        leads.resume(&key(&search)).await,
    ] {
        let (code, text) = refusal(result.unwrap_err());
        assert_eq!(code, Some(Refusal::MoveRefused));
        assert_eq!(text, LEAD_LIMIT_REFUSAL);
    }
    w.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_queue_start_checks_the_plan_and_every_status_follows_the_turns() {
    let (w, wishlist) = world().await;
    let leads = w.project_agents.leads();
    let lead = leads.start_for_queue(&key(&wishlist)).await.unwrap();
    assert_eq!(lead.status, LeadStatus::CheckingPlan);
    let out = w.wait_for_output(&session(&lead.agent), "CWD=").await;
    assert!(
        out.contains(&format!(
            "/{}-epic-lead {} was queued. Check the plan before the first child starts: `{} prime --epic`.",
            naming::COMMAND_NAME,
            wishlist.key,
            naming::COMMAND_NAME
        )),
        "{out}"
    );
    prompt(&w, &lead, &format!("/{}", naming::copy_name("epic-lead"))).await;
    assert_eq!(status(&w, &wishlist).await, LeadStatus::CheckingPlan);
    stop(&w, &lead).await;
    assert_eq!(status(&w, &wishlist).await, LeadStatus::Idle);

    // The user types in the Lead tab: re-planning, as a wake-up (d0091: the lead never plans from scratch).
    w.project_agents
        .enter(&session(&lead.agent), None)
        .await
        .unwrap();
    prompt(&w, &lead, "Split this into tickets of about a day").await;
    assert_eq!(status(&w, &wishlist).await, LeadStatus::Replanning);
    stop(&w, &lead).await;

    // Wake-ups: re-planning, answering SHOP-1.2.
    let child: otaq_core::domain::TicketKey = format!("{}.2", wishlist.key).parse().unwrap();
    for (wake, expected) in [
        (Wake::Merge, LeadStatus::Replanning),
        (Wake::Close, LeadStatus::Replanning),
        (Wake::Stuck, LeadStatus::Replanning),
        (
            Wake::Question {
                ticket: child.clone(),
            },
            LeadStatus::Answering {
                ticket: child.clone(),
            },
        ),
    ] {
        leads
            .set_turn_reason(&key(&wishlist), TurnReason::Wake { wake })
            .await
            .unwrap();
        assert_eq!(
            status(&w, &wishlist).await,
            LeadStatus::Idle,
            "until it's typed"
        );
        prompt(&w, &lead, "SHOP-1.1 merged: Wishlist API").await;
        assert_eq!(status(&w, &wishlist).await, expected);
        stop(&w, &lead).await;
        assert_eq!(status(&w, &wishlist).await, LeadStatus::Idle);
    }

    // A reason set mid-turn waits for the next turn.
    prompt(&w, &lead, "Look at the cart too").await;
    leads
        .set_turn_reason(
            &key(&wishlist),
            TurnReason::Wake {
                wake: Wake::Question {
                    ticket: child.clone(),
                },
            },
        )
        .await
        .unwrap();
    assert_eq!(status(&w, &wishlist).await, LeadStatus::Replanning);
    stop(&w, &lead).await;
    prompt(&w, &lead, "SHOP-1.2 asks: Can guests save?").await;
    assert_eq!(
        status(&w, &wishlist).await,
        LeadStatus::Answering {
            ticket: child.clone()
        }
    );

    // Its own question: asks you, a group 1 item naming the epic.
    w.project_agents
        .ask_human(
            &lead.agent.id,
            "Ship the share link in this epic?",
            &[],
            &[],
            None,
        )
        .await
        .unwrap();
    assert_eq!(status(&w, &wishlist).await, LeadStatus::AsksYou);
    let waiting = w.project_agents.needs_you().await.unwrap();
    assert_eq!(waiting.len(), 1);
    assert_eq!(waiting[0].name, "Wishlist lead");
    assert_eq!(waiting[0].epic.as_ref(), Some(&wishlist.key));
    assert_eq!(waiting[0].status, Status::AsksYou);
    w.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_permission_prompt_asks_you() {
    let (w, wishlist) = world().await;
    let lead = w
        .project_agents
        .leads()
        .open(&key(&wishlist))
        .await
        .unwrap();
    prompt(&w, &lead, "Plan it").await;
    hook(
        &w,
        &lead,
        "Notification",
        json!({ "notification_type": "permission_prompt", "message": "Allow gh issue list?" }),
    )
    .await;
    assert_eq!(status(&w, &wishlist).await, LeadStatus::AsksYou);
    let waiting = w.project_agents.needs_you().await.unwrap();
    assert_eq!(waiting.len(), 1);
    assert_eq!(waiting[0].status, Status::NeedsYou);
    assert_eq!(waiting[0].epic.as_ref(), Some(&wishlist.key));
    w.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn leads_are_absent_from_every_project_agent_list_and_named_after_their_epic() {
    let (w, wishlist) = world().await;
    let agent = w
        .project_agents
        .open("SHOP", "Look around the checkout code")
        .await
        .unwrap();
    let lead = w
        .project_agents
        .leads()
        .open(&key(&wishlist))
        .await
        .unwrap();
    prompt(&w, &lead, "Plan the wishlist").await;
    stop(&w, &lead).await;
    let list = w.project_agents.list("SHOP").await.unwrap();
    assert_eq!(
        list.open.iter().map(|a| a.id.as_str()).collect::<Vec<_>>(),
        [agent.id.as_str()]
    );
    assert_eq!(status(&w, &wishlist).await, LeadStatus::Idle);
    assert_eq!(
        w.project_agents.get(&lead.agent.id).await.unwrap().name,
        "Wishlist lead",
        "no fallback name"
    );
    // Ended, it isn't among the recent ones either.
    w.sessions.stop(&session(&lead.agent)).await.unwrap();
    wait_ended(&w, &wishlist).await;
    let list = w.project_agents.list("SHOP").await.unwrap();
    assert!(list.recent.iter().all(|a| a.id != lead.agent.id));
    assert!(list.open.iter().all(|a| a.id != lead.agent.id));
    // `project_agent.resume` by id resumes it as a lead: its role and its epic.
    let back = w.project_agents.resume(&lead.agent.id).await.unwrap();
    assert_eq!(back.epic.as_ref(), Some(&wishlist.key));
    let out = w.wait_for_output(&session(&back), "CWD=").await;
    assert!(
        out.contains(&format!(
            "ROLE={} EPIC={}",
            naming::ROLE_EPIC_LEAD,
            wishlist.key
        )),
        "{out}"
    );

    // `agent name` and the user's rename are refused.
    for result in [
        w.project_agents
            .name_by_agent(&lead.agent.id, "Wishlist planner", false)
            .await,
        w.project_agents
            .name_by_agent(&lead.agent.id, "Wishlist planner", true)
            .await,
        w.project_agents.rename(&lead.agent.id, "Mine").await,
    ] {
        let (code, text) = refusal(result.unwrap_err());
        assert_eq!(code, Some(Refusal::NotAllowed));
        assert_eq!(text, NAMED_AFTER_EPIC);
    }
    // Renaming the epic renames the lead.
    w.board
        .update_ticket(
            &key(&wishlist),
            &otaq_core::board::tickets::TicketUpdate {
                title: Some("Saved items".into()),
                ..Default::default()
            },
            &Actor::User("me".into()),
        )
        .await
        .unwrap();
    let renamed = w
        .project_agents
        .leads()
        .get(&key(&wishlist))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(renamed.name, "Saved items lead");
    w.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn tokens_count_on_the_epic_and_today_and_not_toward_the_limit() {
    let (w, wishlist) = world().await;
    let lead = w
        .project_agents
        .leads()
        .open(&key(&wishlist))
        .await
        .unwrap();
    let day = w.usage.summary(None).await.unwrap().day;
    sqlx::query(
        "INSERT INTO usage (session_id, day, input, cached, output) VALUES (?, ?, 1000 + 100, 5000, 300)",
    )
    .bind(session(&lead.agent))
    .bind(&day)
    .execute(w.board.store().pool())
    .await
    .unwrap();
    let tokens = w
        .project_agents
        .leads()
        .lead_tokens(&key(&wishlist))
        .await
        .unwrap();
    assert_eq!(tokens.shown(), 1400);
    let summary = w.usage.summary(Some("SHOP")).await.unwrap();
    assert_eq!(summary.today.shown(), 1400);
    let on_epic = summary
        .tickets
        .iter()
        .find(|t| t.id == wishlist.id)
        .expect("the epic's tokens");
    assert_eq!(on_epic.tokens.map(|t| t.shown()), Some(1400));
    assert_eq!(on_epic.lead.map(|t| t.shown()), Some(1400));
    assert_eq!(on_epic.agent().map(|t| t.shown()), Some(0));
    let other = epic(&w, "Search").await;
    assert_eq!(
        w.project_agents
            .leads()
            .lead_tokens(&key(&other))
            .await
            .unwrap()
            .shown(),
        0
    );

    let client = Client::connect(ClientConfig::new(&w.data)).await.unwrap();
    match client.request(Request::SchedulerStatus).await.unwrap() {
        Reply::SchedulerStatus(s) => assert_eq!(s.holding, 0),
        reply => panic!("{reply:?}"),
    }
    match client.request(Request::ServiceStatus).await.unwrap() {
        Reply::ServiceStatus(s) => assert_eq!((s.agents_working, s.agents_waiting), (0, 0)),
        reply => panic!("{reply:?}"),
    }
    w.stop().await;
}

/// Changes the lead makes are recorded as it (CLI-04): `created_and_changed` reads them unchanged.
#[tokio::test(flavor = "multi_thread")]
async fn the_leads_changes_carry_its_actor() {
    let (w, wishlist) = world().await;
    let lead = w
        .project_agents
        .leads()
        .open(&key(&wishlist))
        .await
        .unwrap();
    let actor = caller(&lead).actor(Some("me"));
    assert_eq!(actor, Actor::ProjectAgent(lead.agent.id.clone()));
    w.board
        .create_ticket(
            &NewTicketInput {
                project: "SHOP".into(),
                title: "Wishlist share link".into(),
                epic: Some(key(&wishlist)),
                ..Default::default()
            },
            &actor,
        )
        .await
        .unwrap();
    let changes = w
        .project_agents
        .created_and_changed(&lead.agent.id)
        .await
        .unwrap();
    assert_eq!(changes.created.len(), 1);
    assert_eq!(changes.created[0].title, "Wishlist share link");
    w.stop().await;
}
