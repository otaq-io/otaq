//! What an epic lead may change, through the socket (T-0280; CLI-07, LEAD-08, DEC-04; d0083, d0084, d0068, d0044,
//! d0060, d0016): requests sent from inside its session (an ordinary `sh` started as the lead, running `nc`) are
//! judged as that lead. It edits, links and prioritises its epic's never-started children and creates children,
//! with History on the child and the epic; everything else is refused with the one text. `suggest close|delete`
//! is a needs-you item after group 1 that the user confirms (the usual close or delete) or dismisses. `decision mark`
//! is the lead's alone.

use otaq_core::agent_tools::LaunchContext;
use otaq_core::agent_tools::claude::ClaudeLaunch;
use otaq_core::board::needs_you::NeedsYouGroup;
use otaq_core::board::tickets::{EventKind, NewTicketInput, Ticket};
use otaq_core::client::{Client, ClientConfig, ClientError};
use otaq_core::decisions::NewDecision;
use otaq_core::domain::{Actor, Kind, Status};
use otaq_core::epic_lead::suggest::{self, SuggestionKind, SuggestionState};
use otaq_core::git::testing::TempRepos;
use otaq_core::policy::{ONLY_A_LEAD_MARKS, ONLY_A_LEAD_SUGGESTS};
use otaq_core::project_agents::lead::Lead;
use otaq_core::protocol::Request;
use otaq_core::service::ServiceConfig;
use otaq_core::service::testing::{AgentShell, TestService, path_without_build_folders};
use otaq_core::store::now_ms;
use serde_json::{Value, json};

const REFUSAL: &str = "Wishlist lead can only change SHOP-40's children that haven't started";

/// The sample board (SHOP-40 Wishlist: 40.1 Merged, 40.2 Working, 40.3 Backlog) plus 40.4 Wishlist share link
/// (Backlog) and the epic SHOP-41 Search with SHOP-41.1; `claude` is an interactive `sh` in the service's data
/// folder.
async fn world() -> TestService {
    let home = tempfile::TempDir::new().unwrap();
    let data = home.path().join("h");
    let script = format!(
        "export {}='{}'; export PS1=''; exec sh",
        otaq_core::data_dir::DataDir::env_var(),
        data.display()
    );
    let w = TestService::start_in(home, TempRepos::new(), |config| ServiceConfig {
        launch: Some(LaunchContext {
            bin_dir: "/nonexistent".into(),
            plugin_dir: "/nonexistent/plugin".into(),
            path: path_without_build_folders().into(),
            claude: ClaudeLaunch {
                program: vec!["sh".into(), "-c".into(), script, "sh".into()],
                config_dir: None,
            },
        }),
        ..config
    })
    .await;
    w.shop_sample().await;
    child(&w, "SHOP-40", "Wishlist share link").await;
    let search = w
        .board
        .create_ticket(
            &NewTicketInput {
                project: "SHOP".into(),
                title: "Search".into(),
                kind: Kind::Epic,
                ..Default::default()
            },
            &me(),
        )
        .await
        .unwrap();
    child(&w, &search.key.to_string(), "Search index API").await;
    w
}

fn me() -> Actor {
    Actor::User("me".into())
}

async fn child(w: &TestService, epic: &str, title: &str) -> Ticket {
    w.board
        .create_ticket(
            &NewTicketInput {
                project: "SHOP".into(),
                title: title.into(),
                epic: Some(epic.into()),
                ..Default::default()
            },
            &me(),
        )
        .await
        .unwrap()
}

async fn open(w: &TestService, epic: &str) -> (Lead, AgentShell) {
    let lead = w.project_agents.leads().open(epic).await.unwrap();
    let shell = w.shell_in(&lead.agent.session.as_ref().unwrap().id);
    (lead, shell)
}

async fn send(w: &TestService, shell: &AgentShell, method: &str, params: Value) -> Value {
    let request =
        json!({"protocol_version": 1, "id": 1, "request": {"method": method, "params": params}});
    let ran = shell
        .send_raw(&w.data.socket_path(), &request.to_string())
        .await;
    serde_json::from_str(ran.stdout.lines().next().unwrap_or_default())
        .unwrap_or_else(|e| panic!("{e}: {ran:?}"))
}

/// The same request from the user (the app), answered in the socket's shape.
async fn send_as_user(w: &TestService, method: &str, params: Value) -> Value {
    let request: Request =
        serde_json::from_value(json!({"method": method, "params": params})).unwrap();
    let client = Client::connect(ClientConfig::new(&w.data)).await.unwrap();
    match client.request(request).await {
        Ok(reply) => json!({"reply": serde_json::to_value(reply).unwrap()}),
        Err(ClientError::Service(e)) => json!({"error": serde_json::to_value(e).unwrap()}),
        Err(e) => panic!("{method}: {e}"),
    }
}

fn ok(answer: &Value) -> &Value {
    assert!(answer.get("error").is_none(), "{answer}");
    &answer["reply"]
}

fn refused(answer: &Value) -> &str {
    assert_eq!(answer["error"]["kind"], "refused", "{answer}");
    answer["error"]["message"].as_str().unwrap()
}

/// The newest event of `kind` on `key`.
async fn last_event(w: &TestService, key: &str, kind: EventKind) -> (Actor, Value) {
    let event = w
        .board
        .history(key)
        .await
        .unwrap()
        .into_iter()
        .find(|e| e.kind == kind)
        .unwrap_or_else(|| panic!("no {kind} on {key}"));
    (event.actor, event.payload)
}

#[tokio::test(flavor = "multi_thread")]
async fn a_lead_changes_its_never_started_children_with_history_on_both() {
    let w = world().await;
    let (lead, shell) = open(&w, "SHOP-40").await;
    let actor = Actor::ProjectAgent(lead.agent.id.clone());
    assert_eq!(lead.name, "Wishlist lead");

    // Edit a never-started child: History on the child and the epic, and "Updated by agent" naming the lead.
    let edited = send(
        &w,
        &shell,
        "ticket.update",
        json!({"key": "SHOP-40.3", "body": "40.2 returns `items[]`, not `list`.", "priority": 2}),
    )
    .await;
    assert_eq!(ok(&edited)["ticket"]["updated_by"], actor.to_string());
    let (by, payload) = last_event(&w, "SHOP-40.3", EventKind::Updated).await;
    assert_eq!(
        (by.clone(), payload["lead"].clone()),
        (actor.clone(), json!("Wishlist lead"))
    );
    let (by, payload) = last_event(&w, "SHOP-40", EventKind::LeadChanged).await;
    assert_eq!(by, actor);
    assert_eq!(
        payload,
        json!({"lead": "Wishlist lead", "child": "SHOP-40.3", "change": "updated",
               "fields": ["description", "priority"]})
    );
    let edits = w.board.agent_edits("SHOP-40.3").await.unwrap();
    assert!(edits.updated_by_agent);
    assert_eq!(edits.last_agent, Some(actor.clone()));
    assert!(
        w.board
            .ticket("SHOP-40.3")
            .await
            .unwrap()
            .flags
            .updated_by_agent
    );

    // "SHOP-40.3 waits for SHOP-40.4", then the wait for the running 40.2; and removed again.
    for (method, parent, change) in [
        ("dep.add", "SHOP-40.4", "dep_added"),
        ("dep.add", "SHOP-40.2", "dep_added"),
        ("dep.remove", "SHOP-40.4", "dep_removed"),
    ] {
        let linked = send(
            &w,
            &shell,
            method,
            json!({"child": "SHOP-40.3", "parent": parent}),
        )
        .await;
        ok(&linked);
        let (by, payload) = last_event(&w, "SHOP-40", EventKind::LeadChanged).await;
        assert_eq!(by, actor);
        assert_eq!(
            payload,
            json!({"lead": "Wishlist lead", "child": "SHOP-40.3", "change": change, "parent": parent}),
            "{method} {parent}"
        );
    }
    let kind = EventKind::DependencyAdded;
    assert_eq!(
        last_event(&w, "SHOP-40.3", kind).await.1["lead"],
        "Wishlist lead"
    );
    assert_eq!(
        w.board.ticket("SHOP-40.3").await.unwrap().depends_on,
        ["SHOP-40.2".parse().unwrap()]
    );
    // The card's "Updated by lead" (T-0291, LEAD-08): the fields and the parents its links added, until the user
    // looks at the changes.
    let by_lead = |t: Ticket| t.flags.by_lead.expect("by lead");
    let flag = by_lead(w.board.ticket("SHOP-40.3").await.unwrap());
    assert_eq!(
        (flag.lead.as_str(), flag.added, flag.updated),
        ("Wishlist lead", false, true)
    );
    assert_eq!(flag.fields, ["description", "priority"]);
    assert_eq!(flag.waits_for, ["SHOP-40.2".parse().unwrap()]);

    // A new child of its epic: created by the lead, on the epic's History.
    let created = send(
        &w,
        &shell,
        "ticket.create",
        json!({"project": "SHOP", "title": "Wishlist empty state", "epic": "SHOP-40"}),
    )
    .await;
    let new = &ok(&created)["ticket"];
    assert_eq!(new["key"], "SHOP-40.5");
    assert_eq!(new["created_by"], actor.to_string());
    let (_, payload) = last_event(&w, "SHOP-40", EventKind::LeadChanged).await;
    assert_eq!(
        payload,
        json!({"lead": "Wishlist lead", "child": "SHOP-40.5", "change": "created",
               "title": "Wishlist empty state"})
    );
    assert_eq!(
        last_event(&w, "SHOP-40.5", EventKind::Created).await.1["lead"],
        "Wishlist lead"
    );
    let flag = by_lead(w.board.ticket("SHOP-40.5").await.unwrap());
    assert!(flag.added && !flag.updated, "{flag:?}");
    // Looking at the changes clears "updated"; "added" stays.
    w.board
        .mark_agent_edits_seen("SHOP-40.3", &me())
        .await
        .unwrap();
    assert_eq!(
        w.board.ticket("SHOP-40.3").await.unwrap().flags.by_lead,
        None
    );
    assert_eq!(
        w.board.ticket("SHOP-40.4").await.unwrap().flags.by_lead,
        None
    );

    // Everything else is refused with the one text, and changes nothing.
    for (method, params) in [
        ("ticket.update", json!({"key": "SHOP-40.2", "title": "x"})),
        ("ticket.update", json!({"key": "SHOP-40", "title": "x"})),
        ("ticket.update", json!({"key": "SHOP-41.1", "title": "x"})),
        ("ticket.update", json!({"key": "SHOP-13", "title": "x"})),
        (
            "dep.add",
            json!({"child": "SHOP-40.2", "parent": "SHOP-40.4"}),
        ),
        (
            "dep.add",
            json!({"child": "SHOP-40.4", "parent": "SHOP-41.1"}),
        ),
        (
            "dep.add",
            json!({"child": "SHOP-40.4", "parent": "SHOP-13"}),
        ),
        ("ticket.create", json!({"project": "SHOP", "title": "x"})),
        (
            "ticket.create",
            json!({"project": "SHOP", "title": "x", "epic": "SHOP-41"}),
        ),
        (
            "ticket.move",
            json!({"key": "SHOP-40.3", "target": "ready_for_dev"}),
        ),
        ("epic.queue", json!({"key": "SHOP-40"})),
        ("epic.take_back", json!({"key": "SHOP-40"})),
        ("ticket.park", json!({"key": "SHOP-40.2"})),
        ("ticket.close", json!({"key": "SHOP-40.3", "confirm": true})),
        ("ticket.delete", json!({"key": "SHOP-40.4"})),
        (
            "ticket.local_merge",
            json!({"key": "SHOP-40.2", "message": "m"}),
        ),
        (
            "ticket.stage_next",
            json!({"key": "SHOP-40.3", "summary": "done"}),
        ),
        (
            "ticket.stage_done",
            json!({"key": "SHOP-40.3", "summary": "nothing to do"}),
        ),
        // Never on its own epic (AGM-10).
        (
            "ticket.stage_done",
            json!({"key": "SHOP-40", "summary": "nothing to do"}),
        ),
        ("agent.name", json!({"name": "Wishlist planner"})),
    ] {
        let answer = send(&w, &shell, method, params.clone()).await;
        assert_eq!(refused(&answer), REFUSAL, "{method} {params}");
    }
    assert_eq!(
        w.board.ticket("SHOP-40.2").await.unwrap().title,
        "Wishlist button"
    );
    assert_eq!(w.status("SHOP-40.3").await, Status::Backlog);
    assert_eq!(w.status("SHOP-40.4").await, Status::Backlog);
    w.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn suggestions_wait_after_group_one_and_the_user_confirms_or_dismisses_them() {
    let w = world().await;
    let (lead, shell) = open(&w, "SHOP-40").await;
    let status_before = w.project_agents.leads().status("SHOP-40").await.unwrap();
    let export = child(&w, "SHOP-40", "Wishlist export (CSV)").await; // SHOP-40.5

    // Suggest closing 40.5 and deleting 40.4.
    let closing = send(
        &w,
        &shell,
        "lead.suggest",
        json!({"key": "SHOP-40.5", "kind": "close", "reason": "PAY-123 asks for no export."}),
    )
    .await;
    let closing = ok(&closing)["suggestion"].clone();
    assert_eq!(closing["state"], "open");
    assert_eq!(closing["child"], "SHOP-40.5");
    assert_eq!(
        closing["suggested_by"],
        format!("project-agent:{}", lead.agent.id)
    );
    let deleting = send(
        &w,
        &shell,
        "lead.suggest",
        json!({"key": "SHOP-40.4", "kind": "delete", "reason": "40.2 already shares the list."}),
    )
    .await;
    let deleting = ok(&deleting)["suggestion"].clone();

    // One open suggestion per child and kind; a started child, another epic's child, an empty reason are refused.
    let again = send(
        &w,
        &shell,
        "lead.suggest",
        json!({"key": "SHOP-40.5", "kind": "close", "reason": "again"}),
    )
    .await;
    assert_eq!(
        refused(&again),
        "You already suggested closing SHOP-40.5; the user hasn't decided yet"
    );
    for key in ["SHOP-40.2", "SHOP-41.1", "SHOP-40"] {
        let answer = send(
            &w,
            &shell,
            "lead.suggest",
            json!({"key": key, "kind": "close", "reason": "r"}),
        )
        .await;
        assert_eq!(refused(&answer), REFUSAL, "{key}");
    }
    let empty = send(
        &w,
        &shell,
        "lead.suggest",
        json!({"key": "SHOP-40.3", "kind": "close", "reason": "  "}),
    )
    .await;
    assert_eq!(refused(&empty), suggest::MISSING_REASON);
    // Only a lead suggests: a ticket's agent and the user are refused.
    let agent = w
        .agent_shell(&w.board.ticket("SHOP-40.2").await.unwrap())
        .await;
    let params = json!({"key": "SHOP-40.3", "kind": "close", "reason": "r"});
    let by_agent = send(&w, &agent, "lead.suggest", params.clone()).await;
    assert_eq!(refused(&by_agent), ONLY_A_LEAD_SUGGESTS);
    assert_eq!(
        refused(&send_as_user(&w, "lead.suggest", params).await),
        ONLY_A_LEAD_SUGGESTS
    );

    // Needs you: their own group right after group 1, with the headline and the reason; no status change.
    let items = w.board.needs_you_items(now_ms()).await.unwrap();
    let groups: Vec<NeedsYouGroup> = items.iter().map(|i| i.group).collect();
    let first = groups
        .iter()
        .position(|g| *g == NeedsYouGroup::Suggestions)
        .unwrap();
    assert!(
        groups[..first].iter().all(|g| *g == NeedsYouGroup::Family),
        "{groups:?}"
    );
    assert!(
        groups[first..]
            .iter()
            .all(|g| *g >= NeedsYouGroup::Suggestions)
    );
    assert!(groups.contains(&NeedsYouGroup::Family) && groups.contains(&NeedsYouGroup::Review));
    let shown: Vec<_> = items
        .iter()
        .filter(|i| i.group == NeedsYouGroup::Suggestions)
        .collect();
    assert_eq!(shown.len(), 2);
    let csv = shown.iter().find(|i| i.key == export.key).unwrap();
    assert_eq!(
        csv.reason,
        "Wishlist lead suggests closing SHOP-40.5 Wishlist export (CSV)"
    );
    let item = csv.suggestion.as_ref().unwrap();
    assert_eq!(item.reason, "PAY-123 asks for no export.");
    assert_eq!(item.lead, "Wishlist lead");
    assert_eq!(item.kind, SuggestionKind::Close);
    assert_eq!(csv.id, closing["id"].as_str().unwrap());
    assert!(!otaq_core::attention::notify::should_notify(csv));
    assert_eq!(
        w.project_agents.leads().status("SHOP-40").await.unwrap(),
        status_before
    );
    // Confirming and dismissing are the user's.
    let id = closing["id"].as_str().unwrap();
    assert_eq!(
        refused(&send(&w, &shell, "suggestion.confirm", json!({"id": id})).await),
        REFUSAL
    );

    // Confirm the close: the usual close, as the user; History on the child and the epic.
    let confirmed = send_as_user(&w, "suggestion.confirm", json!({"id": id})).await;
    assert_eq!(ok(&confirmed)["suggestion"]["state"], "confirmed");
    assert_eq!(w.status("SHOP-40.5").await, Status::ClosedNotMerged);
    for key in ["SHOP-40.5", "SHOP-40"] {
        let (by, payload) = last_event(&w, key, EventKind::LeadSuggestion).await;
        assert!(matches!(by, Actor::User(_)), "{key}: the user, {by}");
        assert_eq!(payload["outcome"], "closed", "{key}");
        assert_eq!(payload["child"], "SHOP-40.5", "{key}");
    }
    let (by, _) = last_event(&w, "SHOP-40.5", EventKind::Closed).await;
    assert!(matches!(by, Actor::User(_)), "{by}");

    // Confirm the delete: the usual delete; the epic keeps the line.
    let id = deleting["id"].as_str().unwrap();
    let confirmed = send_as_user(&w, "suggestion.confirm", json!({"id": id})).await;
    assert_eq!(ok(&confirmed)["suggestion"]["state"], "confirmed");
    assert!(w.board.ticket("SHOP-40.4").await.is_err(), "deleted");
    let (_, payload) = last_event(&w, "SHOP-40", EventKind::LeadSuggestion).await;
    assert_eq!(
        (payload["child"].clone(), payload["outcome"].clone()),
        (json!("SHOP-40.4"), json!("deleted"))
    );

    // Dismiss: marked dismissed, listed for `prime --epic`; nothing else changes.
    let dismissed = send(
        &w,
        &shell,
        "lead.suggest",
        json!({"key": "SHOP-40.3", "kind": "close", "reason": "covered by 40.2"}),
    )
    .await;
    let id = ok(&dismissed)["suggestion"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let answer = send_as_user(&w, "suggestion.dismiss", json!({"id": id})).await;
    assert_eq!(ok(&answer)["suggestion"]["state"], "dismissed");
    assert_eq!(w.status("SHOP-40.3").await, Status::Backlog);
    let (_, payload) = last_event(&w, "SHOP-40.3", EventKind::LeadSuggestion).await;
    assert_eq!(payload["outcome"], "dismissed");
    let all = suggest::list_for_epic(&w.board, "SHOP-40").await.unwrap();
    assert_eq!(
        all.iter()
            .map(|s| (s.child.to_string(), s.state))
            .collect::<Vec<_>>(),
        [
            ("SHOP-40.5".to_owned(), SuggestionState::Confirmed),
            ("SHOP-40.3".to_owned(), SuggestionState::Dismissed),
        ]
    );
    let again = send_as_user(&w, "suggestion.dismiss", json!({"id": id})).await;
    assert_eq!(
        refused(&again),
        "The suggestion to close SHOP-40.3 was already dismissed"
    );
    assert!(
        w.board
            .needs_you_items(now_ms())
            .await
            .unwrap()
            .iter()
            .all(|i| i.group != NeedsYouGroup::Suggestions)
    );
    w.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_suggestion_disappears_when_its_child_starts() {
    let w = world().await;
    let (_, shell) = open(&w, "SHOP-40").await;
    let answer = send(
        &w,
        &shell,
        "lead.suggest",
        json!({"key": "SHOP-40.3", "kind": "close", "reason": "covered by 40.2"}),
    )
    .await;
    let id = ok(&answer)["suggestion"]["id"].as_str().unwrap().to_owned();
    let suggested = |items: &[otaq_core::board::needs_you::NeedsYouItem]| {
        items.iter().any(|i| i.group == NeedsYouGroup::Suggestions)
    };
    assert!(suggested(&w.board.needs_you_items(now_ms()).await.unwrap()));
    // Queued (Ready for Dev, never been to Dev): still shown.
    let page = w.board.ticket("SHOP-40.3").await.unwrap();
    w.force_status(&page, Status::Queued).await;
    assert!(suggested(&w.board.needs_you_items(now_ms()).await.unwrap()));
    // In Dev: gone, and it can't be confirmed any more.
    w.force_status(&page, Status::Working).await;
    assert!(!suggested(
        &w.board.needs_you_items(now_ms()).await.unwrap()
    ));
    let answer = send_as_user(&w, "suggestion.confirm", json!({"id": id})).await;
    assert_eq!(
        refused(&answer),
        "SHOP-40.3 has started or closed meanwhile"
    );
    assert_eq!(w.status("SHOP-40.3").await, Status::Working);
    w.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn only_the_epics_lead_marks_a_childs_decision_and_a_second_mark_replaces_the_first() {
    let w = world().await;
    let decision = NewDecision {
        question: "Order of saved items?".into(),
        built: "newest first".into(),
        other_options: vec!["oldest first".into()],
        why: "most shops list newest first".into(),
    };
    let api = w.board.ticket("SHOP-40.2").await.unwrap();
    otaq_core::decisions::record(
        &w.board,
        "SHOP-40.2",
        &decision,
        &Actor::Agent(api.key.clone()),
    )
    .await
    .unwrap();
    let (lead, shell) = open(&w, "SHOP-40").await;

    let marked = send(
        &w,
        &shell,
        "decision.mark",
        json!({"id": "SHOP-40.2#1", "source": "PAY-123",
               "text": "items are listed\n  in the order they were added"}),
    )
    .await;
    let mark = &ok(&marked)["decision"]["mark"];
    assert_eq!(
        mark["text"],
        "items are listed in the order they were added"
    );
    assert_eq!(mark["source"], "PAY-123");
    assert_eq!(mark["by"], format!("project-agent:{}", lead.agent.id));
    let (by, payload) = last_event(&w, "SHOP-40.2", EventKind::DecisionMarked).await;
    assert_eq!(by, Actor::ProjectAgent(lead.agent.id.clone()));
    assert_eq!(
        payload,
        json!({"ordinal": 1, "lead": "Wishlist lead", "source": "PAY-123",
               "text": "items are listed in the order they were added"})
    );

    // A second mark replaces the first.
    let again = send(
        &w,
        &shell,
        "decision.mark",
        json!({"id": "SHOP-40.2#1", "source": "the epic plan", "text": "the plan says oldest first"}),
    )
    .await;
    ok(&again);
    let stored = otaq_core::decisions::get(&w.board, "SHOP-40.2#1")
        .await
        .unwrap();
    let mark = stored.mark.unwrap();
    assert_eq!(
        (mark.text.as_str(), mark.source.as_deref()),
        ("the plan says oldest first", Some("the epic plan"))
    );

    // A missing source or text is refused.
    for (source, text, refusal) in [
        ("", "t", otaq_core::decisions::mark::MISSING_SOURCE),
        ("PAY-123", " \n", otaq_core::decisions::mark::MISSING_TEXT),
    ] {
        let answer = send(
            &w,
            &shell,
            "decision.mark",
            json!({"id": "SHOP-40.2#1", "source": source, "text": text}),
        )
        .await;
        assert_eq!(refused(&answer), refusal);
    }

    // The ticket's agent, the user and another epic's lead are refused.
    let params = json!({"id": "SHOP-40.2#1", "source": "PAY-123", "text": "t"});
    let agent = w.agent_shell(&api).await;
    assert_eq!(
        refused(&send(&w, &agent, "decision.mark", params.clone()).await),
        ONLY_A_LEAD_MARKS
    );
    assert_eq!(
        refused(&send_as_user(&w, "decision.mark", params.clone()).await),
        ONLY_A_LEAD_MARKS
    );
    let (_, search) = open(&w, "SHOP-41").await;
    assert_eq!(
        refused(&send(&w, &search, "decision.mark", params).await),
        "Search lead can only change SHOP-41's children that haven't started"
    );
    let stored = otaq_core::decisions::get(&w.board, "SHOP-40.2#1")
        .await
        .unwrap();
    assert_eq!(stored.mark.unwrap().text, "the plan says oldest first");
    w.stop().await;
}
