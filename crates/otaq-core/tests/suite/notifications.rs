//! OS notifications in the running service (T-0117, ATT-11; d0016, d0042, d0090): once per blocking item, never for
//! Ready for Review or a focused ticket, never while an app window is focused, at most one pop-up per quiet period,
//! and the setting applies at once. A recording notifier stands in for the OS and a fake clock for the time: nothing
//! is posted for real and nothing sleeps for 2 minutes.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use otaq_core::attention::notify::pace::{FakeClock, QUIET};
use otaq_core::attention::notify::{
    Notifier, OnClick, OsNotification, PermissionChanged, PermissionState, Target,
};
use otaq_core::attention::os_permission::NotificationsState;
use otaq_core::board::settings::GlobalSettings;
use otaq_core::caller::Caller;
use otaq_core::client::{Client, ClientConfig};
use otaq_core::domain::Status;
use otaq_core::protocol::{AppFocusParams, Event, FocusParams, Reply, Request};
use otaq_core::service::ServiceConfig;
use otaq_core::service::testing::TestService;
use otaq_core::session::SessionKind;

/// Records what is posted, and keeps each notification's click so a test can click it.
#[derive(Default)]
struct Recorder(Mutex<Vec<OsNotification>>, Mutex<Vec<OnClick>>);

impl Notifier for Recorder {
    fn post(&self, n: &OsNotification, on_click: OnClick) {
        self.0.lock().unwrap().push(n.clone());
        self.1.lock().unwrap().push(on_click);
    }
}

impl Recorder {
    fn titles(&self) -> Vec<String> {
        self.0
            .lock()
            .unwrap()
            .iter()
            .map(|n| n.title.clone())
            .collect()
    }
}

async fn settle() {
    tokio::time::sleep(Duration::from_millis(300)).await;
}

async fn until_titles(recorder: &Recorder, n: usize) -> Vec<String> {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let titles = recorder.titles();
            if titles.len() >= n {
                return titles;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("{n} notifications never came: {:?}", recorder.titles()))
}

/// A service whose notifications go to a recorder, on a fake clock.
async fn world() -> (TestService, Arc<Recorder>, Arc<FakeClock>) {
    let recorder = Arc::new(Recorder::default());
    let clock = Arc::new(FakeClock::new());
    let (notifier, time): (Arc<dyn Notifier>, Arc<FakeClock>) = (recorder.clone(), clock.clone());
    let w = TestService::start_with(move |config| ServiceConfig {
        notifier,
        clock: time,
        ..config
    })
    .await;
    w.add_project("shop-web").await;
    (w, recorder, clock)
}

/// An agent session for a new Working ticket, and the caller its hooks come from.
async fn working(
    w: &TestService,
    title: &str,
) -> (otaq_core::board::tickets::Ticket, Caller, String) {
    let t = w.create("SHOP", title).await;
    w.force_status(&t, Status::Working).await;
    let shell = w.agent_shell(&t).await;
    let agent = Caller::Agent {
        ticket: t.key.clone(),
        ticket_id: t.id.clone(),
        session: shell.session.clone(),
    };
    (t, agent, shell.session.clone())
}

#[tokio::test(flavor = "multi_thread")]
async fn blocking_items_notify_once_and_never_while_focused() {
    let (w, recorder, clock) = world().await;
    let review = w.create("SHOP", "Checkout API").await;
    let (t, agent, session) = working(&w, "Split user full name").await;
    settle().await;

    w.signals
        .ask_human(&agent, &t.id, "Should fullName be split everywhere?")
        .await
        .unwrap();
    let titles = until_titles(&recorder, 1).await;
    assert_eq!(titles, ["SHOP-2 asks a question"]);
    {
        let posted = recorder.0.lock().unwrap();
        assert_eq!(posted[0].subtitle, "shop-web · Split user full name");
        assert_eq!(posted[0].body, "Should fullName be split everywhere?");
        assert_eq!(posted[0].thread.as_deref(), Some("SHOP"));
        assert!(posted[0].sound);
    }
    // Ready for Review never notifies; other events don't repeat the question.
    w.force_status(&review, Status::WaitingForYou).await;
    w.board
        .pin("SHOP-1", &otaq_core::domain::Actor::User("me".into()))
        .await
        .unwrap();
    w.inbox.refresh().await.unwrap();
    settle().await;
    assert_eq!(recorder.titles().len(), 1);

    // Past the quiet period. Answered in the terminal, then a new question while the terminal has focus: nothing
    // until it loses focus.
    clock.advance(QUIET);
    w.signals.enter(&t.id, None).await.unwrap();
    let app = Client::connect(ClientConfig::new(&w.data)).await.unwrap();
    app.request(Request::SessionFocus(FocusParams {
        session: session.clone(),
        focused: true,
    }))
    .await
    .unwrap();
    w.signals
        .ask_human(&agent, &t.id, "And the sender address?")
        .await
        .unwrap();
    w.inbox.refresh().await.unwrap();
    settle().await;
    assert_eq!(recorder.titles().len(), 1, "never for the focused ticket");
    app.request(Request::SessionFocus(FocusParams {
        session: session.clone(),
        focused: false,
    }))
    .await
    .unwrap();
    let titles = until_titles(&recorder, 2).await;
    assert_eq!(titles[1], "SHOP-2 asks a question");

    // Turned off in Settings: respected at once.
    clock.advance(QUIET);
    let mut settings = w.board.global_settings().await.unwrap();
    settings.notifications = false;
    w.board.set_global_settings(&settings).await.unwrap();
    w.signals.enter(&t.id, None).await.unwrap();
    w.signals
        .ask_human(&agent, &t.id, "Third question?")
        .await
        .unwrap();
    w.inbox.refresh().await.unwrap();
    settle().await;
    assert_eq!(recorder.titles().len(), 2);
    let _: GlobalSettings = settings;
    w.stop().await;
}

/// A click on a notification brings a connected app forward on the ticket (T-0159): the service tells every
/// subscribed client to open its target.
#[tokio::test(flavor = "multi_thread")]
async fn a_click_opens_the_ticket_in_the_connected_app() {
    let (w, recorder, _clock) = world().await;
    let (t, agent, _) = working(&w, "Split user full name").await;
    settle().await;
    w.signals
        .ask_human(&agent, &t.id, "Should fullName be split everywhere?")
        .await
        .unwrap();
    until_titles(&recorder, 1).await;

    let app = Client::connect(ClientConfig::new(&w.data)).await.unwrap();
    let mut events = app.subscribe().await.unwrap();
    let click = recorder.1.lock().unwrap().pop().unwrap();
    click();
    let opened = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let Ok(Event::OpenFromNotification { target }) = events.recv().await {
                return target;
            }
        }
    })
    .await
    .expect("the app was told to open the ticket");
    assert_eq!(opened, Target::Ticket { key: t.key.clone() });
    w.stop().await;
}

/// T-0371 (d0090): while an app window is focused nothing is posted, and those items never notify later; a closed
/// connection stops counting; `otaq attach` (a terminal's focus only) doesn't count.
#[tokio::test(flavor = "multi_thread")]
async fn nothing_is_posted_while_an_app_window_is_focused() {
    let (w, recorder, clock) = world().await;
    let (one, first, _) = working(&w, "Add a search box").await;
    let (two, second, _) = working(&w, "Fix the date format").await;
    settle().await;

    let app = Client::connect(ClientConfig::new(&w.data)).await.unwrap();
    app.request(Request::AppFocus(AppFocusParams { focused: true }))
        .await
        .unwrap();
    assert!(w.inbox.looking());
    w.signals
        .ask_human(&first, &one.id, "Fuzzy search?")
        .await
        .unwrap();
    w.inbox.refresh().await.unwrap();
    settle().await;
    assert!(recorder.titles().is_empty(), "looking at Otaq");

    // Switching away doesn't bring it up, not even after the quiet period.
    app.request(Request::AppFocus(AppFocusParams { focused: false }))
        .await
        .unwrap();
    assert!(!w.inbox.looking());
    clock.advance(QUIET);
    w.inbox.refresh().await.unwrap();
    settle().await;
    assert!(recorder.titles().is_empty(), "never later");

    // Focused again, then the window's connection closes: it doesn't count any more.
    app.request(Request::AppFocus(AppFocusParams { focused: true }))
        .await
        .unwrap();
    drop(app);
    tokio::time::timeout(Duration::from_secs(5), async {
        while w.inbox.looking() {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("a closed connection stops looking");
    w.signals
        .ask_human(&second, &two.id, "Which date format?")
        .await
        .unwrap();
    assert_eq!(until_titles(&recorder, 1).await, ["SHOP-2 asks a question"]);
    w.stop().await;
}

/// T-0371 (d0090): the first pop-up goes out at once; the next item is held and goes out by itself when the quiet
/// period ends (the inbox wakes for it); one held while the service stops is never posted.
#[tokio::test(flavor = "multi_thread")]
async fn the_quiet_period_holds_and_the_service_posts_at_its_end() {
    let (w, recorder, clock) = world().await;
    let (one, first, _) = working(&w, "Add a search box").await;
    let (two, second, _) = working(&w, "Fix the date format").await;
    let (three, third, _) = working(&w, "Sort notes by date").await;
    settle().await;

    w.signals
        .ask_human(&first, &one.id, "Fuzzy search?")
        .await
        .unwrap();
    assert_eq!(until_titles(&recorder, 1).await, ["SHOP-1 asks a question"]);
    clock.advance(Duration::from_secs(30));
    w.signals
        .ask_human(&second, &two.id, "Which date format?")
        .await
        .unwrap();
    w.inbox.refresh().await.unwrap();
    settle().await;
    assert_eq!(recorder.titles().len(), 1, "held");

    // The period ends: the service posts the held item without any other change.
    clock.advance(QUIET - Duration::from_secs(30));
    assert_eq!(
        until_titles(&recorder, 2).await,
        ["SHOP-1 asks a question", "SHOP-2 asks a question"]
    );

    // Held in the new period, then the service stops: nothing is posted afterwards.
    w.signals
        .ask_human(&third, &three.id, "Newest first?")
        .await
        .unwrap();
    w.inbox.refresh().await.unwrap();
    settle().await;
    assert_eq!(recorder.titles().len(), 2, "held");
    w.stop().await;
    clock.advance(QUIET);
    settle().await;
    assert_eq!(recorder.titles().len(), 2, "no timer posts after the stop");
}

/// A notifier with a permission the test sets, like macOS's: a change tells the service (T-0372). Posts nothing.
struct Permissions {
    state: Mutex<PermissionState>,
    changed: Mutex<Option<PermissionChanged>>,
    refreshed: std::sync::atomic::AtomicUsize,
}

impl Permissions {
    fn new(state: PermissionState) -> Arc<Self> {
        Arc::new(Self {
            state: Mutex::new(state),
            changed: Mutex::new(None),
            refreshed: Default::default(),
        })
    }

    /// The user changed System Settings and a read found it.
    fn set(&self, state: PermissionState) {
        *self.state.lock().unwrap() = state;
        if let Some(changed) = self.changed.lock().unwrap().clone() {
            changed();
        }
    }
}

impl Notifier for Permissions {
    fn post(&self, _n: &OsNotification, _on_click: OnClick) {}

    fn permission(&self) -> PermissionState {
        *self.state.lock().unwrap()
    }

    fn refresh_permission(&self) {
        self.refreshed
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    }

    fn on_permission(&self, changed: PermissionChanged) {
        *self.changed.lock().unwrap() = Some(changed);
    }
}

async fn permission_world(state: PermissionState) -> (TestService, Arc<Permissions>) {
    let permissions = Permissions::new(state);
    let notifier: Arc<dyn Notifier> = permissions.clone();
    let w = TestService::start_with(move |config| ServiceConfig { notifier, ..config }).await;
    w.add_project("shop-web").await;
    (w, permissions)
}

async fn notifications_state(client: &Client) -> NotificationsState {
    match client.request(Request::NotificationsState).await.unwrap() {
        Reply::NotificationsState(state) => state,
        other => panic!("{other:?}"),
    }
}

/// Waits until `notifications.state` satisfies `wanted`.
async fn until_state(
    client: &Client,
    wanted: impl Fn(&NotificationsState) -> bool,
) -> NotificationsState {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let state = notifications_state(client).await;
            if wanted(&state) {
                return state;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("the notifications state never got there")
}

/// T-0372 (ATT-13): the first agent session while macOS hasn't been asked sets the ask pending, once (a Shell
/// doesn't); answered (Allow or Not now), it is never pending again; the change comes as `notifications_changed`.
#[tokio::test(flavor = "multi_thread")]
async fn the_first_agent_start_sets_the_ask_pending_once() {
    let (w, _permissions) = permission_world(PermissionState::NotAsked).await;
    let app = Client::connect(ClientConfig::new(&w.data)).await.unwrap();
    let mut events = app.subscribe().await.unwrap();
    let t = w.create("SHOP", "Add a search box").await;
    let _shell = w.shell(&t, SessionKind::Shell).await;
    settle().await;
    let state = notifications_state(&app).await;
    assert_eq!(state.permission, PermissionState::NotAsked);
    assert!(!state.ask_pending, "a Shell isn't an agent");

    let _agent = w.agent_shell(&t).await;
    until_state(&app, |s| s.ask_pending).await;
    let changed = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let Ok(Event::NotificationsChanged { state }) = events.recv().await {
                return state;
            }
        }
    })
    .await
    .expect("notifications_changed");
    assert!(changed.ask_pending);

    // Not now: answered for good; later agents (a Project agent too) never set it again.
    app.request(Request::NotificationsAskAnswered)
        .await
        .unwrap();
    assert!(!notifications_state(&app).await.ask_pending);
    let other = w.create("SHOP", "Fix the date format").await;
    let _second = w.agent_shell(&other).await;
    let _project_agent = w.shell(&other, SessionKind::ProjectAgent).await;
    settle().await;
    assert!(!notifications_state(&app).await.ask_pending);
    w.stop().await;
}

/// T-0372 (ATT-13): with macOS's permission already given, or outside the bundle, an agent start never asks.
#[tokio::test(flavor = "multi_thread")]
async fn allowed_or_outside_the_bundle_never_asks() {
    for permission in [PermissionState::Allowed, PermissionState::OutsideBundle] {
        let (w, _permissions) = permission_world(permission).await;
        let app = Client::connect(ClientConfig::new(&w.data)).await.unwrap();
        let t = w.create("SHOP", "Add a search box").await;
        let _agent = w.agent_shell(&t).await;
        settle().await;
        let state = notifications_state(&app).await;
        assert_eq!(state.permission, permission);
        assert!(!state.ask_pending, "{permission:?}");
        assert!(!state.blocked_banner);
        w.stop().await;
    }
}

/// T-0372 (ATT-14): a notification blocked while macOS has notifications off shows the banner; closed, it stays
/// closed while still off; allowed and then off again, it shows once more at the next blocked notification; never
/// while Otaq's switch is off. A focused app window reads the permission again.
#[tokio::test(flavor = "multi_thread")]
async fn the_blocked_banner_shows_once_per_turn_off() {
    let (w, permissions) = permission_world(PermissionState::TurnedOff).await;
    let app = Client::connect(ClientConfig::new(&w.data)).await.unwrap();
    let (one, first, _) = working(&w, "Add a search box").await;
    let (two, second, _) = working(&w, "Fix the date format").await;
    let (three, third, _) = working(&w, "Sort notes by date").await;
    settle().await;
    assert!(
        !notifications_state(&app).await.blocked_banner,
        "nothing blocked yet"
    );

    w.signals
        .ask_human(&first, &one.id, "Fuzzy search?")
        .await
        .unwrap();
    until_state(&app, |s| s.blocked_banner).await;
    assert!(w.inbox.notification_blocked());

    // Closed: hidden, also after the next blocked notification while still off.
    app.request(Request::NotificationsDismissBlocked)
        .await
        .unwrap();
    assert!(!notifications_state(&app).await.blocked_banner);
    w.signals
        .ask_human(&second, &two.id, "Which date format?")
        .await
        .unwrap();
    settle().await;
    assert!(!notifications_state(&app).await.blocked_banner, "dismissed");

    // Allowed in System Settings: the closing and the blocked notification are forgotten.
    let before = permissions
        .refreshed
        .load(std::sync::atomic::Ordering::SeqCst);
    app.request(Request::AppFocus(AppFocusParams { focused: true }))
        .await
        .unwrap();
    assert!(
        permissions
            .refreshed
            .load(std::sync::atomic::Ordering::SeqCst)
            > before,
        "a focused window reads the permission again"
    );
    app.request(Request::AppFocus(AppFocusParams { focused: false }))
        .await
        .unwrap();
    permissions.set(PermissionState::Allowed);
    until_state(&app, |s| s.permission == PermissionState::Allowed).await;
    assert!(!w.inbox.notification_blocked());

    // Off again: nothing until a notification is blocked, then the banner once more.
    permissions.set(PermissionState::TurnedOff);
    let state = until_state(&app, |s| s.permission == PermissionState::TurnedOff).await;
    assert!(!state.blocked_banner);
    w.signals
        .ask_human(&third, &three.id, "Newest first?")
        .await
        .unwrap();
    until_state(&app, |s| s.blocked_banner).await;

    // Otaq's own switch off: no banner.
    let mut settings = w.board.global_settings().await.unwrap();
    settings.notifications = false;
    app.request(Request::SettingsSet(settings)).await.unwrap();
    assert!(!notifications_state(&app).await.blocked_banner);
    w.stop().await;
}
