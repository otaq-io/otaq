//! d{RUN} (T4, monorepos): a project's run command is a list of named apps. Older settings read as one app `app` with
//! nothing else changed; each app has its own Run port per ticket (distinct, stable, freed with the ticket's); every
//! app's command gets every app's port as `OTAQ_PORT_<NAME>`; with several apps a request names one; the Shell gets
//! `$PORT` only while there is one app; two tickets run both apps side by side without clashing; an agent adds a
//! missing app with `run set <app>`.

use std::ops::RangeInclusive;
use std::time::Duration;

use otaq_core::board::settings::{DEFAULT_APP, ProjectSettings, RunApp, RunCommandSetBy};
use otaq_core::board::tickets::Ticket;
use otaq_core::domain::{RunningApp, Status};
use otaq_core::ports::PortPool;
use otaq_core::service::ServiceConfig;
use otaq_core::service::testing::TestService;
use otaq_core::terminals::run::{RunAgent, RunState};

fn app(name: &str, command: &str) -> RunApp {
    RunApp {
        name: name.into(),
        command: command.into(),
        open_url: String::new(),
        set_by: None,
    }
}

/// `web` and `api`, each printing its own `$PORT` and the other's `OTAQ_PORT_<NAME>`, then staying up.
fn web_and_api() -> Vec<RunApp> {
    vec![
        RunApp {
            open_url: "http://localhost:$PORT/".into(),
            ..app("web", "echo web port=$PORT api=$OTAQ_PORT_API && sleep 30")
        },
        app("api", "echo api port=$PORT web=$OTAQ_PORT_WEB && sleep 30"),
    ]
}

async fn with_apps(w: &TestService, apps: Vec<RunApp>) {
    let settings = ProjectSettings {
        apps,
        ..Default::default()
    };
    w.board
        .set_project_settings("SHOP", &settings)
        .await
        .unwrap();
}

/// A started ticket with ports for `web` and `api` from `range`: (ticket, web's port, api's port).
async fn started(w: &TestService, title: &str, range: RangeInclusive<u16>) -> (Ticket, u16, u16) {
    let t = w.create("SHOP", title).await;
    w.start_worktree(&t).await;
    let ports = PortPool::with_range(w.board.store().clone(), range)
        .allocate(&t.id, &["web".to_owned(), "api".to_owned()])
        .await
        .unwrap();
    w.force_status(&t, Status::Working).await;
    let t = w.board.ticket(&t.key.to_string()).await.unwrap();
    (t, ports.apps[0].1, ports.apps[1].1)
}

async fn wait_live(w: &TestService, key: &str, app: &str) {
    tokio::time::timeout(Duration::from_secs(10), async {
        while !w.runs.info(key, Some(app)).await.unwrap().state.is_live() {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("{key}'s {app} never started"));
}

#[tokio::test]
async fn older_settings_read_as_one_app_named_app_and_nothing_else_changes() {
    let w = TestService::start().await;
    w.add_project("shop-web").await;
    let legacy = serde_json::json!({
        "setup_command": "npm ci",
        "run_command": " pnpm dev --port $PORT ",
        "open_url": "http://localhost:$PORT",
        "run_command_set_by": {"stage": "Dev", "ticket": "SHOP-27", "at": 5, "open_url": true},
        "env_files": [".env.local"],
        "let_finished_tickets_answer": false
    });
    sqlx::query("UPDATE projects SET settings_json = ? WHERE key = 'SHOP'")
        .bind(legacy.to_string())
        .execute(w.board.store().pool())
        .await
        .unwrap();
    let read = w.board.project_settings("SHOP").await.unwrap();
    let expected = ProjectSettings {
        setup_command: "npm ci".into(),
        apps: vec![RunApp {
            name: DEFAULT_APP.into(),
            command: "pnpm dev --port $PORT".into(),
            open_url: "http://localhost:$PORT".into(),
            set_by: Some(RunCommandSetBy {
                stage: "Dev".into(),
                ticket: "SHOP-27".parse().unwrap(),
                at: 5,
                open_url: true,
            }),
        }],
        env_files: vec![".env.local".into()],
        let_finished_tickets_answer: false,
        ..Default::default()
    };
    assert_eq!(read, expected);

    // Saved once, it is stored as apps and reads the same.
    w.board.save_project_settings("SHOP", &read).await.unwrap();
    assert_eq!(w.board.project_settings("SHOP").await.unwrap(), expected);

    // No run command: no app.
    sqlx::query("UPDATE projects SET settings_json = ? WHERE key = 'SHOP'")
        .bind(r#"{"run_command": "", "open_url": "http://localhost:3000"}"#)
        .execute(w.board.store().pool())
        .await
        .unwrap();
    assert!(
        w.board
            .project_settings("SHOP")
            .await
            .unwrap()
            .apps
            .is_empty()
    );
    w.stop().await;
}

#[tokio::test]
async fn the_page_keeps_set_by_per_app_and_names_problems_per_app() {
    let w = TestService::start().await;
    w.add_project("shop-web").await;
    let by = RunCommandSetBy {
        stage: "Dev".into(),
        ticket: "SHOP-3".parse().unwrap(),
        at: 1,
        open_url: false,
    };
    assert!(
        w.board
            .save_agent_app("SHOP", "api", "node api.js", None, by.clone())
            .await
            .unwrap()
    );
    // The user adds `web` in front and leaves `api` alone: its line stays; an empty row is no app.
    let page = ProjectSettings {
        apps: vec![
            app("web", "pnpm dev"),
            app("api", "node api.js"),
            app("", ""),
        ],
        ..Default::default()
    };
    w.board.save_project_settings("SHOP", &page).await.unwrap();
    let s = w.board.project_settings("SHOP").await.unwrap();
    assert_eq!(
        s.apps
            .iter()
            .map(|a| (a.name.as_str(), a.set_by.is_some()))
            .collect::<Vec<_>>(),
        [("web", false), ("api", true)]
    );
    // An agent never changes an app that has a command.
    assert!(
        !w.board
            .save_agent_app("SHOP", "api", "node other.js", None, by)
            .await
            .unwrap()
    );
    // Two names giving the same variable, a missing command, a bad URL.
    let bad = ProjectSettings {
        apps: vec![
            app("web-admin", "a"),
            app("web_admin", "b"),
            RunApp {
                open_url: "localhost".into(),
                ..app("api", "")
            },
        ],
        ..Default::default()
    };
    w.board.set_project_settings("SHOP", &bad).await.unwrap();
    let problems: Vec<(String, String)> = w
        .board
        .project_setting_problems("SHOP")
        .await
        .unwrap()
        .into_iter()
        .map(|p| (p.field, p.message))
        .collect();
    assert_eq!(
        problems,
        [
            (
                "apps.1.name".to_owned(),
                "web_admin and web-admin both give OTAQ_PORT_WEB_ADMIN; rename one".to_owned()
            ),
            (
                "apps.2.command".to_owned(),
                "Add the command that starts it".to_owned()
            ),
            (
                "apps.2.open_url".to_owned(),
                "localhost isn't a URL; use something like http://localhost:$PORT".to_owned()
            ),
        ]
    );
    w.stop().await;
}

#[tokio::test]
async fn each_app_gets_its_own_port_stable_and_freed_with_the_ticket() {
    let w = TestService::start().await;
    w.add_project("shop-web").await;
    let t = w.create("SHOP", "Checkout").await;
    let u = w.create("SHOP", "Cart").await;
    let pool = PortPool::with_range(w.board.store().clone(), 25_500..=25_599);
    let names = ["web".to_owned(), "api".to_owned(), "worker".to_owned()];
    let a = pool.allocate(&t.id, &names).await.unwrap();
    let b = pool.allocate(&u.id, &names).await.unwrap();
    let mut all: Vec<u16> = [a.agent, b.agent]
        .into_iter()
        .chain(a.apps.iter().chain(&b.apps).map(|(_, p)| *p))
        .collect();
    assert_eq!(
        a.apps.iter().map(|(n, _)| n.as_str()).collect::<Vec<_>>(),
        ["web", "api", "worker"]
    );
    all.sort_unstable();
    all.dedup();
    assert_eq!(all.len(), 8, "every port distinct: {a:?} {b:?}");
    // Asked again (a restart's new pool on the same store): the same ports.
    let again = PortPool::with_range(w.board.store().clone(), 25_500..=25_599);
    assert_eq!(again.allocate(&t.id, &names).await.unwrap(), a);
    // An app added later gets a new port; the others keep theirs.
    let more = again
        .app_ports(&t.id, &["web".to_owned(), "admin".to_owned()])
        .await
        .unwrap();
    assert_eq!(more[0], a.apps[0]);
    assert!(!all.contains(&more[1].1));
    // The ticket's Work shows them, lowest first.
    let work = w.board.ticket("SHOP-1").await.unwrap().work.unwrap();
    assert_eq!(work.app_port("api"), Some(a.apps[1].1));
    assert_eq!(work.app_ports.len(), 4);
    // Freed at Done or Park: every app port goes back.
    again.free(&t.id).await.unwrap();
    assert!(again.held_app_ports(&t.id).await.unwrap().is_empty());
    assert_eq!(again.held_app_ports(&u.id).await.unwrap(), b.apps);
    w.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn each_app_runs_on_its_own_port_with_every_apps_port() {
    let w = TestService::start().await;
    w.add_project("shop-web").await;
    with_apps(&w, web_and_api()).await;
    let (t, web_port, api_port) = started(&w, "Checkout", 25_600..=25_699).await;
    let key = t.key.to_string();

    // Several apps: a request names one.
    let err = w.runs.start(&key, None).await.unwrap_err();
    assert_eq!(
        err.to_string(),
        "shop-web has 2 apps: web, api. Name the one you mean: `otaq run start web`"
    );
    let err = w.runs.start(&key, Some("worker")).await.unwrap_err();
    assert_eq!(
        err.to_string(),
        "shop-web has no app worker; its apps: web, api"
    );

    let web = w.runs.start(&key, Some("web")).await.unwrap();
    let api = w.runs.start(&key, Some("api")).await.unwrap();
    assert_eq!((web.app.as_str(), web.port), ("web", Some(web_port)));
    assert_eq!((api.app.as_str(), api.port), ("api", Some(api_port)));
    assert_eq!(
        web.url().as_deref(),
        Some(format!("http://localhost:{web_port}/").as_str())
    );
    w.wait_for_output(
        web.session.as_deref().unwrap(),
        &format!("web port={web_port} api={api_port}"),
    )
    .await;
    w.wait_for_output(
        api.session.as_deref().unwrap(),
        &format!("api port={api_port} web={web_port}"),
    )
    .await;

    // The card shows both, the Run tab lists both in the project's order.
    let flags = w.board.ticket(&key).await.unwrap().flags;
    assert_eq!(
        flags.run_apps,
        [
            RunningApp {
                app: "web".into(),
                port: web_port,
                started_by: None
            },
            RunningApp {
                app: "api".into(),
                port: api_port,
                started_by: None
            },
        ]
    );
    let apps = w.runs.apps(&key).await.unwrap();
    assert_eq!(
        apps.iter()
            .map(|a| (a.app.as_str(), a.state.is_live()))
            .collect::<Vec<_>>(),
        [("web", true), ("api", true)]
    );

    // Stopping one leaves the other up.
    let stopped = w.runs.stop(&key, Some("web")).await.unwrap();
    assert_eq!(stopped.state, RunState::Stopped);
    assert!(
        w.runs
            .info(&key, Some("api"))
            .await
            .unwrap()
            .state
            .is_live()
    );
    assert_eq!(w.board.ticket(&key).await.unwrap().flags.run_apps.len(), 1);
    w.runs.stop(&key, Some("api")).await.unwrap();
    assert!(
        w.board
            .ticket(&key)
            .await
            .unwrap()
            .flags
            .run_apps
            .is_empty()
    );
    w.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn two_tickets_run_both_apps_side_by_side_without_clashing() {
    let w = TestService::start().await;
    w.add_project("shop-web").await;
    with_apps(&w, web_and_api()).await;
    let (a, a_web, a_api) = started(&w, "Checkout", 25_700..=25_799).await;
    let (b, b_web, b_api) = started(&w, "Cart", 25_700..=25_799).await;
    let mut ports = vec![a_web, a_api, b_web, b_api];
    ports.sort_unstable();
    ports.dedup();
    assert_eq!(ports.len(), 4);
    for (t, web, api) in [(&a, a_web, a_api), (&b, b_web, b_api)] {
        let key = t.key.to_string();
        for (name, line) in [
            ("web", format!("web port={web} api={api}")),
            ("api", format!("api port={api} web={web}")),
        ] {
            let info = w.runs.start(&key, Some(name)).await.unwrap();
            w.wait_for_output(info.session.as_deref().unwrap(), &line)
                .await;
        }
    }
    for t in [&a, &b] {
        let key = t.key.to_string();
        for name in ["web", "api"] {
            wait_live(&w, &key, name).await;
        }
        assert_eq!(w.board.ticket(&key).await.unwrap().flags.run_apps.len(), 2);
    }
    assert_eq!(w.runs.running().await.len(), 4);
    for t in [&a, &b] {
        for name in ["web", "api"] {
            w.runs.stop(&t.key.to_string(), Some(name)).await.unwrap();
        }
    }
    w.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn the_shell_gets_port_only_with_one_app() {
    let plain_sh = |mut c: ServiceConfig| {
        c.terminals.shell = "sh".into();
        c.terminals.shell_args = Vec::new();
        c
    };
    let w = TestService::start_with(plain_sh).await;
    w.add_project("shop-web").await;
    with_apps(&w, web_and_api()).await;
    let (t, web, api) = started(&w, "Checkout", 25_800..=25_899).await;
    let enter = if cfg!(windows) { "\r" } else { "\n" };
    let shell = w
        .shells
        .open_shell(&t.key.to_string(), false)
        .await
        .unwrap();
    w.sessions
        .write(
            &shell.id,
            format!("echo \"port=[$PORT] web=$OTAQ_PORT_WEB api=$OTAQ_PORT_API\"{enter}")
                .as_bytes(),
        )
        .unwrap();
    w.wait_for_output(&shell.id, &format!("port=[] web={web} api={api}"))
        .await;
    w.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn an_agent_adds_a_missing_app_and_never_changes_one() {
    let w = TestService::start().await;
    w.add_project("shop-web").await;
    with_apps(&w, vec![app("web", "echo web && sleep 30")]).await;
    let t = w.create("SHOP", "Checkout").await;
    w.start_worktree(&t).await;
    PortPool::with_range(w.board.store().clone(), 25_900..=25_999)
        .allocate(&t.id, &["web".to_owned(), "api".to_owned()])
        .await
        .unwrap();
    w.force_status(&t, Status::Working).await;
    let key = t.key.to_string();
    let dev = RunAgent {
        session: "dev-session".into(),
        stage: "Dev".into(),
        kind: otaq_core::session::SessionKind::Agent,
    };
    // No name while the project has an app: refused, saying how to add another.
    let err = w
        .runs
        .set_command(&key, dev.clone(), None, "node api.js", None)
        .await
        .unwrap_err();
    assert!(
        err.to_string()
            .ends_with("to add another app, name it: `otaq run set <app> --command …`"),
        "{err}"
    );
    // The existing app: never changed.
    let err = w
        .runs
        .set_command(&key, dev.clone(), Some("web"), "pnpm dev", None)
        .await
        .unwrap_err();
    assert!(
        err.to_string()
            .starts_with("shop-web's app web already has a run command (`echo web && sleep 30`)"),
        "{err}"
    );
    // A new one: tried as this ticket's Run of it, saved once it runs.
    let tried = w
        .runs
        .set_command(
            &key,
            dev,
            Some("api"),
            "echo api on $PORT && sleep 30",
            Some("http://localhost:$PORT/health"),
        )
        .await
        .unwrap();
    assert_eq!(tried.app, "api");
    let port = tried.port.unwrap();
    let listener = std::net::TcpListener::bind(std::net::SocketAddrV4::new(
        std::net::Ipv4Addr::LOCALHOST,
        port,
    ))
    .unwrap();
    tokio::time::timeout(Duration::from_secs(10), async {
        while w.runs.info(&key, Some("api")).await.unwrap().state != RunState::Running {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("running");
    let s = w.board.project_settings("SHOP").await.unwrap();
    let api = s.app("api").expect("added");
    assert_eq!(
        (api.command.as_str(), api.open_url.as_str()),
        (
            "echo api on $PORT && sleep 30",
            "http://localhost:$PORT/health"
        )
    );
    assert_eq!(
        api.set_by.as_ref().map(|b| b.line()).as_deref(),
        Some("Set by Dev on SHOP-1")
    );
    w.runs.stop(&key, Some("api")).await.unwrap();
    drop(listener);
    // Undo removes the app the agent added.
    w.board.undo_agent_app("SHOP", Some("api")).await.unwrap();
    let s = w.board.project_settings("SHOP").await.unwrap();
    assert_eq!(
        s.apps.iter().map(|a| a.name.as_str()).collect::<Vec<_>>(),
        ["web"]
    );
    w.stop().await;
}
