//! Board requests over the socket against a running service in a temporary data folder: add a project, create
//! SHOP-1 and SHOP-2, link them, and receive the events; errors carry their refusal code.

use std::time::Duration;

use otaq_core::board::moves::DropTarget;
use otaq_core::board::projects::AddProject;
use otaq_core::board::tickets::{NewTicketInput, TicketUpdate};
use otaq_core::client::{Client, ClientConfig, ClientError};
use otaq_core::data_dir::DataDir;
use otaq_core::domain::{Priority, Status, StatusSet};
use otaq_core::git::testing::TempRepos;
use otaq_core::protocol::{
    DepParams, ErrorKind, Event, KeyParams, MoveParams, ProjectParams, Reply, Request,
    TicketDeleteParams, TicketUpdateParams,
};
use otaq_core::service::{Service, ServiceConfig};
use tokio::sync::broadcast;

async fn next_event(events: &mut broadcast::Receiver<Event>) -> Event {
    tokio::time::timeout(Duration::from_secs(5), events.recv())
        .await
        .expect("an event in time")
        .unwrap()
}

/// Events until (and including) the first one matching `done`.
async fn events_until(
    events: &mut broadcast::Receiver<Event>,
    done: impl Fn(&Event) -> bool,
) -> Vec<Event> {
    let mut seen = Vec::new();
    loop {
        let event = next_event(events).await;
        let stop = done(&event);
        seen.push(event);
        if stop {
            return seen;
        }
    }
}

fn ticket(reply: Reply) -> otaq_core::board::tickets::Ticket {
    match reply {
        Reply::Ticket(t) => *t,
        other => panic!("{other:?}"),
    }
}

#[tokio::test]
async fn projects_tickets_deps_and_events() {
    let tmp = tempfile::tempdir().unwrap();
    let data = DataDir::new(tmp.path().join("home"));
    let service = Service::bind(ServiceConfig::new(data.clone()))
        .await
        .unwrap();
    let stopper = service.stopper();
    let task = tokio::spawn(service.serve());
    let client = Client::connect(ClientConfig::new(&data)).await.unwrap();
    let mut events = client.subscribe().await.unwrap();
    let repos = TempRepos::new();
    let repo = repos.clone_with_origin("shop-web", "develop", 1);

    let Reply::FolderProbe(probe) = client
        .request(Request::ProjectProbe(otaq_core::protocol::PathParams {
            path: repo.display().to_string(),
        }))
        .await
        .unwrap()
    else {
        panic!()
    };
    assert_eq!(
        (probe.suggested_key.as_str(), probe.base_branch.as_str()),
        ("SHOP", "develop")
    );
    let Reply::ProjectAdded { project, trust } = client
        .request(Request::ProjectAdd(AddProject {
            path: repo.display().to_string(),
            ..Default::default()
        }))
        .await
        .unwrap()
    else {
        panic!()
    };
    assert_eq!(
        (
            project.key.as_str(),
            project.agents_working,
            project.needs_you
        ),
        ("SHOP", 0, 0)
    );
    // No Claude Code config file in this service: nothing recorded, nothing said.
    assert_eq!(trust, None);
    assert_eq!(next_event(&mut events).await, Event::ProjectsChanged);
    let Reply::Projects(list) = client.request(Request::ProjectList).await.unwrap() else {
        panic!()
    };
    assert_eq!(list.len(), 1);

    let create = |title: &str, body: &str| {
        Request::TicketCreate(NewTicketInput {
            project: "SHOP".into(),
            title: title.into(),
            body: body.into(),
            ..Default::default()
        })
    };
    let t1 = ticket(
        client
            .request(create("Cart totals", "Totals add up"))
            .await
            .unwrap(),
    );
    let t2 = ticket(client.request(create("Checkout page", "")).await.unwrap());
    assert_eq!(
        (t1.key.to_string(), t2.key.to_string()),
        ("SHOP-1".into(), "SHOP-2".into())
    );
    let seen = events_until(
        &mut events,
        |e| matches!(e, Event::CardStatusChanged { key, .. } if key.to_string() == "SHOP-2"),
    )
    .await;
    assert!(seen.contains(&Event::TicketChanged {
        project: "SHOP".into(),
        id: t1.id.clone(),
        key: t1.key.clone()
    }));

    let linked = ticket(
        client
            .request(Request::DepAdd(DepParams {
                child: "SHOP-2".into(),
                parent: "SHOP-1".into(),
            }))
            .await
            .unwrap(),
    );
    assert_eq!(linked.depends_on, vec![t1.key.clone()]);
    let event = next_event(&mut events).await;
    assert_eq!(
        event,
        Event::TicketChanged {
            project: "SHOP".into(),
            id: t2.id.clone(),
            key: t2.key.clone()
        }
    );

    // A loop is refused with its path and the refusal code.
    let err = client
        .request(Request::DepAdd(DepParams {
            child: "SHOP-1".into(),
            parent: "SHOP-2".into(),
        }))
        .await
        .unwrap_err();
    let ClientError::Service(body) = err else {
        panic!("{err:?}")
    };
    assert_eq!(body.kind, ErrorKind::Refused);
    assert!(
        body.message.ends_with("SHOP-2 → SHOP-1 → SHOP-2"),
        "{}",
        body.message
    );
    assert_eq!(body.data.unwrap()["refusal"], "cycle");

    // Moving publishes a status change; it needs no confirmation (no "Done when" warning, d0067).
    let moved = ticket(
        client
            .request(Request::TicketMove(MoveParams {
                key: "SHOP-1".into(),
                target: DropTarget::ReadyForDev,
                confirmed: false,
            }))
            .await
            .unwrap(),
    );
    assert_eq!(moved.status, Status::Queued);
    let seen = events_until(&mut events, |e| {
        matches!(e, Event::CardStatusChanged { .. })
    })
    .await;
    assert_eq!(
        seen.last().unwrap(),
        &Event::CardStatusChanged {
            project: "SHOP".into(),
            id: t1.id.clone(),
            key: t1.key.clone(),
            status: Status::Queued
        }
    );

    // The snapshot has sorted columns.
    let Reply::Board(board) = client
        .request(Request::BoardSnapshot(ProjectParams {
            project: "SHOP".into(),
        }))
        .await
        .unwrap()
    else {
        panic!()
    };
    assert_eq!(board.columns.len(), 6);
    let ready_for_dev = board
        .columns
        .iter()
        .find(|c| c.column == StatusSet::Waiting)
        .unwrap();
    assert_eq!(
        ready_for_dev
            .cards
            .iter()
            .map(|t| t.key.to_string())
            .collect::<Vec<_>>(),
        ["SHOP-1"]
    );
    let backlog = board
        .columns
        .iter()
        .find(|c| c.column == StatusSet::Backlog)
        .unwrap();
    assert_eq!(backlog.cards.len(), 1);

    // Updates name their actor; unknown tickets are NotFound; key changes are refused.
    let updated = ticket(
        client
            .request(Request::TicketUpdate(TicketUpdateParams {
                key: "SHOP-2".into(),
                update: TicketUpdate {
                    title: Some("Checkout".into()),
                    priority: Some(Priority::High),
                    ..Default::default()
                },
            }))
            .await
            .unwrap(),
    );
    assert_eq!(updated.title, "Checkout");
    // The priority travels as its number (TKT-09).
    assert_eq!(updated.priority, Priority::High);
    assert_eq!(serde_json::to_value(&updated).unwrap()["priority"], 2);
    let err = client
        .request(Request::TicketGet(KeyParams {
            key: "SHOP-99".into(),
        }))
        .await
        .unwrap_err();
    let ClientError::Service(body) = err else {
        panic!()
    };
    assert_eq!(body.kind, ErrorKind::NotFound);
    let err = client
        .request(Request::ProjectUpdate(
            otaq_core::protocol::ProjectUpdateParams {
                project: "SHOP".into(),
                update: otaq_core::board::projects::ProjectUpdate {
                    key: Some("SHP".into()),
                    ..Default::default()
                },
            },
        ))
        .await
        .unwrap_err();
    let ClientError::Service(body) = err else {
        panic!()
    };
    assert_eq!(body.data.unwrap()["refusal"], "key_locked");

    let Reply::Cycles(cycles) = client
        .request(Request::DepCycles(ProjectParams {
            project: "SHOP".into(),
        }))
        .await
        .unwrap()
    else {
        panic!()
    };
    assert!(cycles.is_empty());
    let Reply::GlobalSettings(settings) = client.request(Request::SettingsGet).await.unwrap()
    else {
        panic!()
    };
    assert!(settings.settings.notifications && settings.problems.is_empty());

    // Removing the project deletes its tickets (events) and never the repo.
    client
        .request(Request::ProjectRemove(KeyParams { key: "SHOP".into() }))
        .await
        .unwrap();
    let seen = events_until(&mut events, |e| *e == Event::ProjectsChanged).await;
    assert_eq!(
        seen.iter()
            .filter(|e| matches!(e, Event::TicketDeleted { .. }))
            .count(),
        2
    );
    assert!(repo.join(".git").is_dir());

    drop(client);
    stopper.stop();
    task.await.unwrap();
}

#[tokio::test]
async fn themes_travel_in_the_settings_requests() {
    use otaq_core::board::settings::{Theme, ThemeSetting};
    let tmp = tempfile::tempdir().unwrap();
    let data = DataDir::new(tmp.path().join("home"));
    let service = Service::bind(ServiceConfig::new(data.clone()))
        .await
        .unwrap();
    let stopper = service.stopper();
    let task = tokio::spawn(service.serve());
    let client = Client::connect(ClientConfig::new(&data)).await.unwrap();
    let mut events = client.subscribe().await.unwrap();
    let Reply::GlobalSettings(reply) = client.request(Request::SettingsGet).await.unwrap() else {
        panic!()
    };
    let mut settings = reply.settings;
    assert_eq!(settings.theme, ThemeSetting::default());
    settings.theme = ThemeSetting::Picked(Theme::Dracula);
    settings.terminal_font_size = 15;
    client
        .request(Request::SettingsSet(settings.clone()))
        .await
        .unwrap();
    // Every window hears of it, to apply the theme at once (UIT-07).
    assert_eq!(next_event(&mut events).await, Event::SettingsChanged);
    let Reply::GlobalSettings(reply) = client.request(Request::SettingsGet).await.unwrap() else {
        panic!()
    };
    assert_eq!(reply.settings, settings);
    drop(client);
    stopper.stop();
    task.await.unwrap();
}

#[tokio::test]
async fn the_user_deletes_tickets() {
    let tmp = tempfile::tempdir().unwrap();
    let data = DataDir::new(tmp.path().join("home"));
    let service = Service::bind(ServiceConfig::new(data.clone()))
        .await
        .unwrap();
    let stopper = service.stopper();
    let task = tokio::spawn(service.serve());
    let repos = TempRepos::new();
    let repo = repos.repo("shop-web", 1);
    let client = Client::connect(ClientConfig::new(&data)).await.unwrap();
    client
        .request(Request::ProjectAdd(AddProject {
            path: repo.display().to_string(),
            ..Default::default()
        }))
        .await
        .unwrap();
    client
        .request(Request::TicketCreate(NewTicketInput {
            project: "SHOP".into(),
            title: "x".into(),
            ..Default::default()
        }))
        .await
        .unwrap();
    // Agent callers are refused by the permission policy (tests/suite/permissions.rs).
    // The user can.
    client
        .request(Request::TicketDelete(TicketDeleteParams {
            key: "SHOP-1".into(),
            reason: None,
        }))
        .await
        .unwrap();
    drop(client);
    stopper.stop();
    task.await.unwrap();
}
