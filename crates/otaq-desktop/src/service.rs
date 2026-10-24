//! The app's link to the background service (UIT-03), as the GPUI app's `service_link.rs` did it: connects at
//! launch through `otaq_core::client::Client` (starting the service if none answers, d0055), follows the
//! connection state (connected, reconnecting, stopped), keeps the agent counts for the service banners fresh (SVC-03)
//! and restarts or starts the service only when the page asks on the user's click. The app is only a client: closing
//! the window never stops the service (ARC-03), and nothing here restarts it on its own (d0022).
//!
//! Everything the page should hear goes out through the [`EventHub`]: the service's events, a resync after a
//! reconnect or a lag, and every change of the connection state.

use std::path::Path;
use std::sync::{Arc, Mutex, Weak};
use std::time::Duration;

use otaq_core::client::{Client, ClientConfig, ClientError, Compatibility, Launcher};
use otaq_core::data_dir::DataDir;
use otaq_core::protocol::{ErrorKind, Event, Reply, Request};
use otaq_core::service::lock::service_alive;
use otaq_view::view_models::connection::LinkState;
use serde::Serialize;
use tokio::sync::broadcast::error::RecvError;
use ts_rs::TS;

use crate::events::{EventHub, PageEvent};

/// How often the link checks whether a lost service still runs, and refreshes the banner counts.
const POLL: Duration = Duration::from_secs(1);
const COUNTS_POLL: Duration = Duration::from_secs(5);

/// The link to the service as the page reads it (`service.connection`, and the `connection` event).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(tag = "state", rename_all = "snake_case")]
#[ts(rename = "ServiceConnection")]
pub enum Connection {
    /// Connecting, starting the service first if it isn't running.
    Connecting,
    Connected {
        pid: u32,
        build_id: String,
        compatibility: ServiceCompatibility,
        /// For "Restart now (3 agents working, 2 waiting)" (d0022).
        agents_working: u32,
        agents_waiting: u32,
    },
    /// The connection dropped; the client is retrying. Agents keep running (SVC-04).
    Reconnecting { attempt: u32 },
    /// The service process has ended. Nothing starts it until the user clicks.
    Stopped,
    /// The service couldn't be reached or started; `reason` says why (Q40).
    Failed { reason: String },
}

/// How the running service relates to this build (d0022, SVC-03): the states of T-0231's banners.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum ServiceCompatibility {
    Same,
    /// Another build with the same protocol: "Service update ready · Restart now". Everything keeps working.
    UpdateReady,
    /// Another protocol: "Restart required". Only status and restart work.
    RestartRequired,
}

impl From<&LinkState> for Connection {
    fn from(state: &LinkState) -> Self {
        match state {
            LinkState::Connecting => Self::Connecting,
            LinkState::Connected(info) => Self::Connected {
                pid: info.status.pid,
                build_id: info.status.build_id.clone(),
                compatibility: match info.compatibility {
                    Compatibility::Same => ServiceCompatibility::Same,
                    Compatibility::ServiceOlder => ServiceCompatibility::UpdateReady,
                    Compatibility::Incompatible => ServiceCompatibility::RestartRequired,
                },
                agents_working: info.status.agents_working,
                agents_waiting: info.status.agents_waiting,
            },
            LinkState::Reconnecting { attempt } => Self::Reconnecting { attempt: *attempt },
            LinkState::Stopped => Self::Stopped,
            LinkState::Failed(reason) => Self::Failed {
                reason: reason.clone(),
            },
        }
    }
}

/// Why a request failed, as the page shows it (the reply of a rejected `request`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "RequestError")]
pub struct RequestError {
    pub message: String,
    /// A board rule's code (`cycle`, `cross_base`…), when a rule refused the request.
    pub refusal: Option<String>,
    pub not_found: bool,
}

impl RequestError {
    pub fn plain(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            refusal: None,
            not_found: false,
        }
    }
}

impl From<ClientError> for RequestError {
    fn from(e: ClientError) -> Self {
        match e {
            ClientError::Service(body) => Self {
                refusal: body
                    .data
                    .as_ref()
                    .and_then(|d| d.get("refusal"))
                    .and_then(|r| r.as_str())
                    .map(str::to_owned),
                not_found: body.kind == ErrorKind::NotFound,
                message: body.message,
            },
            other => Self::plain(other.to_string()),
        }
    }
}

/// Where the service lives and how to start it.
pub struct Settings {
    pub data: DataDir,
    pub client: ClientConfig,
    pub launcher: Arc<dyn Launcher>,
}

impl Settings {
    pub fn new(data: DataDir, launcher: Arc<dyn Launcher>) -> Self {
        Self {
            client: ClientConfig::new(&data),
            data,
            launcher,
        }
    }
}

pub struct ServiceLink {
    shared: Arc<Shared>,
}

struct Shared {
    /// `Err` when there is no data folder; the link then stays failed with that reason.
    settings: Result<Settings, String>,
    events: Arc<EventHub>,
    link: Mutex<Link>,
    /// The client once the link has one, for the terminals' attach tasks (T-0216).
    clients: tokio::sync::watch::Sender<Option<Client>>,
    /// Every state the page is told about, for the banners and toasts (T-0231).
    states: tokio::sync::watch::Sender<LinkState>,
}

struct Link {
    state: LinkState,
    client: Option<Client>,
    /// A start or restart is running.
    restarting: bool,
}

impl ServiceLink {
    /// Starts connecting (and the checks that tell "reconnecting" from "stopped"). Call inside the app's tokio
    /// runtime; the link's tasks end when it is dropped.
    pub fn start(settings: Result<Settings, String>, events: Arc<EventHub>) -> Self {
        let shared = Arc::new(Shared {
            settings,
            events,
            link: Mutex::new(Link {
                state: LinkState::Connecting,
                client: None,
                restarting: false,
            }),
            clients: tokio::sync::watch::Sender::new(None),
            states: tokio::sync::watch::Sender::new(LinkState::Connecting),
        });
        let weak = Arc::downgrade(&shared);
        tokio::spawn(async move {
            if let Some(shared) = weak.upgrade() {
                let _ = connect(shared).await;
            }
        });
        tokio::spawn(poll(Arc::downgrade(&shared)));
        Self { shared }
    }

    pub fn connection(&self) -> Connection {
        Connection::from(&self.shared.lock().state)
    }

    /// The link's state with the service's status (start time, interrupted sessions, agent counts), and each
    /// change the page is told about.
    pub fn states(&self) -> tokio::sync::watch::Receiver<LinkState> {
        self.shared.states.subscribe()
    }

    /// The data folder the service runs in, if there is one (Settings → Data shows its paths, T-0234).
    pub fn data(&self) -> Option<DataDir> {
        self.shared.settings.as_ref().ok().map(|s| s.data.clone())
    }

    /// The service's client from the moment the link has one: a terminal follows its connection and its events
    /// itself (attach, live output, reattach after a reconnect).
    pub fn clients(&self) -> tokio::sync::watch::Receiver<Option<Client>> {
        self.shared.clients.subscribe()
    }

    /// Sends a request. Fails at once while there is no connection. A click that can start a Local merge goes on a
    /// connection of its own ([`Self::send_alone`], [`may_merge`]).
    pub async fn send(&self, request: Request) -> Result<Reply, RequestError> {
        if may_merge(&request) {
            return self.send_alone(request).await;
        }
        let client = self.shared.lock().client.clone();
        match client {
            Some(client) => Ok(client.request(request).await?),
            None => Err(RequestError::from(ClientError::Disconnected)),
        }
    }

    /// Sends a request on a connection of its own, opened for it. The service answers one request per connection at a
    /// time, so a request that can take minutes (a Local merge running the project's Merge check, d0120) goes here:
    /// the shared connection stays free for the board and for Stop. Fails at once while there is no connection.
    pub async fn send_alone(&self, request: Request) -> Result<Reply, RequestError> {
        let client = self.shared.lock().client.clone();
        let socket = match (&self.shared.settings, client) {
            (_, Some(client)) if client.compatibility() == Some(Compatibility::Incompatible) => {
                return Err(RequestError::from(ClientError::Incompatible));
            }
            (Ok(settings), Some(_)) => settings.client.socket.clone(),
            _ => return Err(RequestError::from(ClientError::Disconnected)),
        };
        Ok(otaq_core::client::request_once(&socket, request).await?)
    }

    /// Restarts the service with this build, or starts it when none runs or the start at launch failed. Only
    /// called on the user's click (confirmed first when agents would be interrupted, T-0231).
    pub async fn restart(&self) -> Result<(), String> {
        let client = {
            let mut link = self.shared.lock();
            if link.restarting {
                return Err("the background service is already restarting".into());
            }
            link.restarting = true;
            link.client.clone()
        };
        let outcome = match (client, &self.shared.settings) {
            (_, Err(reason)) => Err(reason.clone()),
            // Never connected: try the launch-time connect again.
            (None, Ok(_)) => {
                self.shared.set_state(LinkState::Connecting);
                connect(self.shared.clone()).await
            }
            (Some(client), Ok(settings)) => client
                .restart(&*settings.launcher)
                .await
                .map(drop)
                .map_err(|e| e.to_string()),
        };
        self.shared.lock().restarting = false;
        outcome
    }
}

/// The clicks that can run a Local merge before they answer, Merge check included (d0120): Local merge itself, and
/// every move that can put a ticket into an automatic Delivery (Pass, a drag or move, Submit for review, the
/// validation buttons: Move on anyway, Skip this check, Retry merge…, and Apply now). They go on a connection of
/// their own, so the board keeps its answers and Stop reaches the service while the check runs.
pub fn may_merge(request: &Request) -> bool {
    matches!(
        request,
        Request::TicketLocalMerge(_)
            | Request::TicketPass(_)
            | Request::TicketMove(_)
            | Request::TicketDrag(_)
            | Request::TicketSubmitByUser(_)
            | Request::TicketValidationAction(_)
            | Request::FlowApplyNow(_)
    )
}

impl Shared {
    fn lock(&self) -> std::sync::MutexGuard<'_, Link> {
        self.link.lock().expect("service link")
    }

    fn publish_connection(&self, state: &LinkState) {
        self.states.send_replace(state.clone());
        self.events.publish(&PageEvent::Connection {
            connection: Connection::from(state),
        });
    }

    fn set_state(&self, state: LinkState) {
        let mut link = self.lock();
        if link.state != state {
            link.state = state;
            self.publish_connection(&link.state);
        }
    }

    /// While disconnected: "stopped" if no service process is alive any more, else "reconnecting".
    fn check_alive(&self, link: &mut Link) {
        let alive = self
            .settings
            .as_ref()
            .is_ok_and(|s| service_alive(&s.data.service_lock_path()));
        link.state = match std::mem::replace(&mut link.state, LinkState::Connecting) {
            LinkState::Reconnecting { .. } if !alive => LinkState::Stopped,
            LinkState::Stopped if alive => LinkState::Reconnecting { attempt: 0 },
            other => other,
        };
    }
}

/// Connects, starting the service if none answers, then follows the client in the background. On failure the link
/// shows why (Q40) instead of waiting.
async fn connect(shared: Arc<Shared>) -> Result<(), String> {
    let client = match &shared.settings {
        Err(reason) => Err(reason.clone()),
        Ok(settings) => open(settings).await,
    };
    match client {
        Ok(client) => {
            tokio::spawn(follow(Arc::downgrade(&shared), client));
            Ok(())
        }
        Err(reason) => {
            shared.set_state(LinkState::Failed(reason.clone()));
            Err(reason)
        }
    }
}

async fn open(settings: &Settings) -> Result<Client, String> {
    if let Some(problem) = socket_path_problem(&settings.client.socket) {
        return Err(problem);
    }
    Client::connect_or_start(settings.client.clone(), &*settings.launcher)
        .await
        .map_err(|e| match e {
            ClientError::Start(_) | ClientError::NotRunning(_) => format!(
                "{e}; see the logs in {}",
                settings.data.logs_dir().display()
            ),
            other => other.to_string(),
        })
}

/// The longest socket path the OS takes, closing NUL included: `sun_path` holds 104 bytes on macOS and 108 on
/// Linux. A longer data-folder path makes the service fail to start ("path must be shorter than SUN_LEN"), so the
/// link says so at once (Q40). Windows uses a named pipe, which has no such limit.
fn socket_path_problem(socket: &Path) -> Option<String> {
    let limit = if cfg!(windows) {
        return None;
    } else if cfg!(target_os = "macos") {
        104
    } else {
        108
    };
    let len = socket.as_os_str().len();
    (len >= limit).then(|| {
        format!(
            "Couldn't start the background service: the data folder's path is too long for its socket ({len} \
             characters in {}; this system allows {}). Use a shorter {}.",
            socket.display(),
            limit - 1,
            DataDir::env_var()
        )
    })
}

/// Mirrors the client's state into the link, and its events onto the page, until the link is dropped.
async fn follow(weak: Weak<Shared>, client: Client) {
    let mut states = client.state();
    let Some(shared) = weak.upgrade() else {
        return;
    };
    {
        let mut link = shared.lock();
        link.client = Some(client.clone());
        link.state = states.borrow_and_update().clone().into();
        shared.publish_connection(&link.state);
    }
    shared.clients.send_replace(Some(client.clone()));
    // Subscribe before the page refetches: events from here on reach it.
    if let Ok(receiver) = client.subscribe().await {
        tokio::spawn(forward_events(weak.clone(), receiver));
    }
    // A view the page asked for before the link was up failed: it loads now (T-0213: the theme at launch).
    shared.events.publish(&PageEvent::Resync);
    // Only the link holds the client from here on, so dropping the link ends this loop.
    drop((client, shared));
    while states.changed().await.is_ok() {
        let state: LinkState = states.borrow_and_update().clone().into();
        let Some(shared) = weak.upgrade() else {
            return;
        };
        let reconnected = matches!(state, LinkState::Connected(_));
        let mut link = shared.lock();
        link.state = state;
        shared.check_alive(&mut link);
        shared.publish_connection(&link.state);
        if reconnected {
            // Events from the gap are lost (the client resubscribes): the page refetches.
            shared.events.publish(&PageEvent::Resync);
        }
    }
}

/// Passes the service's events on to the page until the link is dropped.
async fn forward_events(weak: Weak<Shared>, mut receiver: tokio::sync::broadcast::Receiver<Event>) {
    loop {
        let event = match receiver.recv().await {
            // Terminal output goes through the terminal's own channel (T-0216), not the page's event stream.
            Ok(Event::SessionOutput { .. }) => continue,
            Ok(event) => PageEvent::service(&event),
            Err(RecvError::Lagged(_)) => PageEvent::Resync,
            Err(RecvError::Closed) => return,
        };
        let Some(shared) = weak.upgrade() else {
            return;
        };
        shared.events.publish(&event);
    }
}

/// Re-checks a lost service every second and refreshes the banner counts every few seconds while a service banner
/// shows.
async fn poll(weak: Weak<Shared>) {
    let mut since_counts = Duration::ZERO;
    loop {
        tokio::time::sleep(POLL).await;
        since_counts += POLL;
        let Some(shared) = weak.upgrade() else {
            return;
        };
        let refresh = {
            let mut link = shared.lock();
            if matches!(
                link.state,
                LinkState::Reconnecting { .. } | LinkState::Stopped
            ) {
                let before = link.state.clone();
                shared.check_alive(&mut link);
                if link.state != before {
                    shared.publish_connection(&link.state);
                }
            }
            match (&link.state, &link.client) {
                (LinkState::Connected(info), Some(client))
                    if info.compatibility != Compatibility::Same && since_counts >= COUNTS_POLL =>
                {
                    Some(client.clone())
                }
                _ => None,
            }
        };
        let Some(client) = refresh else {
            continue;
        };
        since_counts = Duration::ZERO;
        drop(shared);
        if let Ok(Reply::ServiceStatus(status)) = client.request(Request::ServiceStatus).await {
            let Some(shared) = weak.upgrade() else {
                return;
            };
            let mut link = shared.lock();
            if let LinkState::Connected(info) = &mut link.state
                && info.status != status
            {
                info.status = status;
                shared.publish_connection(&link.state);
            }
        }
    }
}

#[cfg(test)]
pub(crate) mod testing {
    //! A real service, run in this process on a temporary data folder, and a launcher that starts it the way
    //! `otaq daemon start` would ("already running" counts as success). No `otaq` process is left behind.

    use std::future::Future;
    use std::io;
    use std::pin::Pin;
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};

    use otaq_core::client::Launcher;
    use otaq_core::data_dir::DataDir;
    use otaq_core::service::{EventBus, Service, ServiceConfig, ServiceError, Stopper};
    use otaq_core::session::Sessions;

    use super::{Connection, ServiceLink, Settings};
    use crate::App;
    use crate::events::EventHub;

    pub struct Running {
        pub bus: EventBus,
        pub sessions: Sessions,
        pub inbox: otaq_core::attention::Inbox,
        /// For tests that feed an agent's hooks and limits as the service would (T-0430).
        pub project_agents: otaq_core::project_agents::ProjectAgents,
        pub signals: otaq_core::signals::Signals,
        pub plan_limit: otaq_core::usage::limit::PlanLimit,
        stopper: Stopper,
        task: tokio::task::JoinHandle<()>,
    }

    impl Running {
        pub async fn stop(self) {
            self.stopper.stop();
            self.task.await.unwrap();
        }
    }

    /// What a test changes in the service's config before it binds.
    pub type Configure = Arc<dyn Fn(&mut ServiceConfig) + Send + Sync>;

    pub struct InProcess {
        pub data: DataDir,
        pub build_id: Option<String>,
        /// A temporary Claude Code config file for the folder trust (PRJ-06); `None` records nothing.
        pub claude: Option<otaq_core::agent_tools::claude::trust::ClaudeConfig>,
        /// Changes the config last, e.g. a stand-in agent launch (`ServiceConfig::launch`); `None` launches nothing.
        pub configure: Option<Configure>,
        pub started: Mutex<Vec<Running>>,
    }

    impl InProcess {
        pub async fn bind(&self) -> Result<(), ServiceError> {
            let mut config = ServiceConfig::new(self.data.clone());
            if let Some(build_id) = &self.build_id {
                config.build_id = build_id.clone();
            }
            config.claude = self.claude.clone();
            // Never the user's own Claude Code folder (their transcripts and settings).
            config.tools.claude_config_dir = Some(self.data.root().join("claude-config"));
            // Terminals run a plain `sh`, not the user's login shell (as the core's tests do).
            config.terminals.shell = "sh".into();
            config.terminals.shell_args = Vec::new();
            if let Some(configure) = &self.configure {
                configure(&mut config);
            }
            let service = Service::bind(config).await?;
            let (bus, sessions, stopper) =
                (service.events(), service.sessions(), service.stopper());
            let inbox = service.inbox();
            let (project_agents, signals, plan_limit) = (
                service.project_agents(),
                service.signals(),
                service.plan_limit(),
            );
            let task = tokio::spawn(service.serve());
            self.started.lock().unwrap().push(Running {
                bus,
                sessions,
                inbox,
                project_agents,
                signals,
                plan_limit,
                stopper,
                task,
            });
            Ok(())
        }

        /// The sessions of the service started last.
        pub fn sessions(&self) -> Sessions {
            self.started
                .lock()
                .unwrap()
                .last()
                .expect("a running service")
                .sessions
                .clone()
        }

        /// The needs-you queue of the service started last.
        pub fn inbox(&self) -> otaq_core::attention::Inbox {
            self.started
                .lock()
                .unwrap()
                .last()
                .expect("a running service")
                .inbox
                .clone()
        }

        /// The Project agents, signals and plan limit of the service started last.
        pub fn agents(
            &self,
        ) -> (
            otaq_core::project_agents::ProjectAgents,
            otaq_core::signals::Signals,
            otaq_core::usage::limit::PlanLimit,
        ) {
            let started = self.started.lock().unwrap();
            let last = started.last().expect("a running service");
            (
                last.project_agents.clone(),
                last.signals.clone(),
                last.plan_limit.clone(),
            )
        }

        /// The service started last.
        pub fn bus(&self) -> EventBus {
            self.started
                .lock()
                .unwrap()
                .last()
                .expect("a running service")
                .bus
                .clone()
        }

        pub async fn stop_all(&self) {
            let running: Vec<Running> = std::mem::take(&mut *self.started.lock().unwrap());
            for r in running {
                r.stop().await;
            }
        }
    }

    impl Launcher for InProcess {
        fn launch(&self) -> Pin<Box<dyn Future<Output = io::Result<()>> + Send + '_>> {
            Box::pin(async move {
                match self.bind().await {
                    Ok(()) | Err(ServiceError::AlreadyRunning { .. }) => Ok(()),
                    Err(e) => Err(io::Error::other(e.to_string())),
                }
            })
        }
    }

    /// A temporary data folder with a short path (the socket path limit), its in-process launcher, and an app
    /// whose page events are recorded.
    pub struct Harness {
        pub _tmp: tempfile::TempDir,
        pub service: Arc<InProcess>,
        pub app: App,
        pub seen: Arc<Mutex<Vec<serde_json::Value>>>,
    }

    impl Harness {
        pub fn new(build_id: Option<&str>) -> Self {
            Self::with_claude(build_id, None)
        }

        /// A harness whose service records the folder trust in `claude` (a temporary file).
        pub fn with_claude(
            build_id: Option<&str>,
            claude: Option<otaq_core::agent_tools::claude::trust::ClaudeConfig>,
        ) -> Self {
            Self::with_config(build_id, claude, None)
        }

        /// A harness whose service config `configure` changes last (a stand-in agent launch).
        pub fn configured(configure: impl Fn(&mut ServiceConfig) + Send + Sync + 'static) -> Self {
            Self::with_config(None, None, Some(Arc::new(configure)))
        }

        fn with_config(
            build_id: Option<&str>,
            claude: Option<otaq_core::agent_tools::claude::trust::ClaudeConfig>,
            configure: Option<Configure>,
        ) -> Self {
            let tmp = tempfile::tempdir().unwrap();
            let data = DataDir::new(tmp.path().join("h"));
            let service = Arc::new(InProcess {
                data: data.clone(),
                build_id: build_id.map(str::to_owned),
                claude,
                configure,
                started: Mutex::new(Vec::new()),
            });
            let (app, seen) = app_with(Settings::new(data, service.clone()));
            Self {
                _tmp: tmp,
                service,
                app,
                seen,
            }
        }

        /// Waits until the link's state matches.
        pub async fn wait_for(&self, what: &str, want: impl Fn(&Connection) -> bool) {
            let deadline = Instant::now() + Duration::from_secs(10);
            loop {
                let now = self.app.service.connection();
                if want(&now) {
                    return;
                }
                assert!(
                    Instant::now() < deadline,
                    "waited for {what}; still {now:?}"
                );
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        }

        /// Waits until a recorded page event matches.
        pub async fn wait_for_event(&self, what: &str, want: impl Fn(&serde_json::Value) -> bool) {
            let deadline = Instant::now() + Duration::from_secs(10);
            loop {
                if self.seen.lock().unwrap().iter().any(&want) {
                    return;
                }
                assert!(
                    Instant::now() < deadline,
                    "waited for the event {what}; saw {:?}",
                    self.seen.lock().unwrap()
                );
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        }
    }

    /// An app linked to the service `settings` names, with its page events recorded.
    fn app_with(mut settings: Settings) -> (App, Arc<Mutex<Vec<serde_json::Value>>>) {
        settings.client.backoff_min = Duration::from_millis(10);
        settings.client.backoff_max = Duration::from_millis(50);
        let events = Arc::new(EventHub::default());
        let seen = Arc::new(Mutex::new(Vec::new()));
        let record = seen.clone();
        events.subscribe(Box::new(move |event| {
            record.lock().unwrap().push(event);
            true
        }));
        let link = ServiceLink::start(Ok(settings), events.clone());
        let app = App {
            notices: crate::notices::Notices::start(&link, events.clone()),
            service: link,
            events,
            switcher: Default::default(),
            drawer_tabs: Default::default(),
            review: Default::default(),
            terminals: crate::terminal::Terminals::default(),
            pr_dialogs: Default::default(),
            cleanup: Default::default(),
            run_commands: Default::default(),
            window_focus: Default::default(),
            reports: Default::default(),
            looked: std::sync::Mutex::new(otaq_view::view_models::ask_bar::Looked::new(
                otaq_core::store::now_ms(),
            )),
            usage: crate::usage::Usage::new(tokio::runtime::Handle::current()),
            // Tests never read the real font folders.
            fonts: crate::fonts::Fonts::in_folders(Vec::new()),
        };
        (app, seen)
    }

    /// An app on a service that is already running on `data` (a core `TestService`); it never starts one.
    pub fn app_on(data: DataDir) -> App {
        struct AlreadyRunning;
        impl Launcher for AlreadyRunning {
            fn launch(&self) -> Pin<Box<dyn Future<Output = io::Result<()>> + Send + '_>> {
                Box::pin(std::future::ready(Ok(())))
            }
        }
        app_with(Settings::new(data, Arc::new(AlreadyRunning))).0
    }

    pub fn connected(c: &Connection) -> bool {
        matches!(c, Connection::Connected { .. })
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Instant;

    use otaq_core::client::ServiceInfo;
    use otaq_core::protocol::{PROTOCOL_VERSION, ServiceStatus};
    use serde_json::json;

    use super::testing::{Harness, connected};
    use super::*;

    fn info(compatibility: Compatibility) -> ServiceInfo {
        ServiceInfo {
            status: ServiceStatus {
                pid: 4120,
                build_id: "abc.1".into(),
                protocol_version: PROTOCOL_VERSION,
                agents_working: 3,
                agents_waiting: 2,
                ..Default::default()
            },
            compatibility,
        }
    }

    #[test]
    fn the_page_reads_the_states_of_the_service_banners() {
        let page = |state: LinkState| serde_json::to_value(Connection::from(&state)).unwrap();
        assert_eq!(
            page(LinkState::Connected(info(Compatibility::ServiceOlder))),
            json!({"state": "connected", "pid": 4120, "build_id": "abc.1", "compatibility": "update_ready",
                   "agents_working": 3, "agents_waiting": 2})
        );
        assert_eq!(
            page(LinkState::Connected(info(Compatibility::Incompatible)))["compatibility"],
            "restart_required"
        );
        assert_eq!(
            page(LinkState::Connected(info(Compatibility::Same)))["compatibility"],
            "same"
        );
        assert_eq!(
            page(LinkState::Reconnecting { attempt: 2 }),
            json!({"state": "reconnecting", "attempt": 2})
        );
        assert_eq!(page(LinkState::Stopped), json!({"state": "stopped"}));
        assert_eq!(page(LinkState::Connecting), json!({"state": "connecting"}));
        assert_eq!(
            page(LinkState::Failed("why".into())),
            json!({"state": "failed", "reason": "why"})
        );
    }

    #[test]
    fn a_socket_path_over_the_limit_is_explained() {
        assert_eq!(socket_path_problem(&PathBuf::from("/tmp/h/x.sock")), None);
        let long = PathBuf::from(format!("/tmp/{}/x.sock", "a".repeat(120)));
        let problem = socket_path_problem(&long);
        if cfg!(windows) {
            assert_eq!(problem, None);
        } else {
            let problem = problem.expect("too long");
            assert!(problem.contains("too long"), "{problem}");
            assert!(problem.contains(&DataDir::env_var()), "{problem}");
        }
    }

    #[test]
    fn service_errors_keep_their_refusal() {
        let body = otaq_core::protocol::ErrorBody {
            kind: ErrorKind::NotFound,
            message: "no ticket SHOP-9".into(),
            data: Some(json!({"refusal": "cycle"})),
        };
        assert_eq!(
            RequestError::from(ClientError::Service(body)),
            RequestError {
                message: "no ticket SHOP-9".into(),
                refusal: Some("cycle".into()),
                not_found: true,
            }
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn starts_the_service_connects_and_leaves_it_running_when_dropped() {
        let h = Harness::new(None);
        h.wait_for("connected", connected).await;
        assert!(matches!(
            h.app.service.connection(),
            Connection::Connected {
                compatibility: ServiceCompatibility::Same,
                ..
            }
        ));
        assert_eq!(h.service.started.lock().unwrap().len(), 1, "started once");
        let reply = h.app.service.send(Request::Ping).await;
        assert!(reply.is_ok(), "{reply:?}");

        // Closing the app drops the link; the service keeps running (ARC-03).
        let Harness { app, service, .. } = h;
        drop(app);
        tokio::time::sleep(Duration::from_millis(100)).await;
        let socket = service.data.socket_path();
        let status = otaq_core::client::request_once(&socket, Request::ServiceStatus).await;
        assert!(matches!(status, Ok(Reply::ServiceStatus(_))), "{status:?}");
        service.stop_all().await;
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn connects_to_a_running_service_without_starting_another() {
        let tmp = tempfile::tempdir().unwrap();
        let data = DataDir::new(tmp.path().join("h"));
        let service = Arc::new(testing::InProcess {
            data: data.clone(),
            build_id: None,
            claude: None,
            configure: None,
            started: Mutex::new(Vec::new()),
        });
        service.bind().await.unwrap();

        struct Counting(Arc<testing::InProcess>, AtomicUsize);
        impl Launcher for Counting {
            fn launch(
                &self,
            ) -> std::pin::Pin<Box<dyn std::future::Future<Output = std::io::Result<()>> + Send + '_>>
            {
                self.1.fetch_add(1, Ordering::SeqCst);
                self.0.launch()
            }
        }
        let launcher = Arc::new(Counting(service.clone(), AtomicUsize::new(0)));
        let link = ServiceLink::start(
            Ok(Settings::new(data, launcher.clone())),
            Arc::new(EventHub::default()),
        );
        let deadline = Instant::now() + Duration::from_secs(10);
        while !connected(&link.connection()) {
            assert!(Instant::now() < deadline, "{:?}", link.connection());
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        assert_eq!(launcher.1.load(Ordering::SeqCst), 0, "nothing launched");
        drop(link);
        service.stop_all().await;
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn an_older_service_build_is_reported_as_update_ready() {
        let h = Harness::new(Some("an-older-build"));
        h.wait_for("connected", connected).await;
        assert!(matches!(
            h.app.service.connection(),
            Connection::Connected {
                compatibility: ServiceCompatibility::UpdateReady,
                ..
            }
        ));
        h.service.stop_all().await;
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn passes_events_on_and_resyncs_after_a_restart_of_the_service() {
        let h = Harness::new(None);
        h.wait_for("connected", connected).await;
        h.wait_for_event("connected", |e| e["kind"] == "connection")
            .await;
        // Views asked for before the link was up load now.
        h.wait_for_event("resync", |e| *e == json!({"kind": "resync"}))
            .await;
        let resyncs = |h: &Harness| {
            h.seen
                .lock()
                .unwrap()
                .iter()
                .filter(|e| **e == json!({"kind": "resync"}))
                .count()
        };
        assert_eq!(resyncs(&h), 1);

        h.service.bus().publish(Event::ProjectsChanged);
        h.wait_for_event("projects_changed", |e| {
            *e == json!({"kind": "service", "name": "projects_changed", "event": {"kind": "projects_changed"}})
        })
        .await;

        // The service ends: the client starts reconnecting, and with the process gone (here: its lock) the link says
        // "stopped". A service whose process still runs shows "reconnecting" instead.
        h.service.stop_all().await;
        h.wait_for("stopped", |c| *c == Connection::Stopped).await;
        h.wait_for_event("stopped", |e| {
            *e == json!({"kind": "connection", "connection": {"state": "stopped"}})
        })
        .await;
        assert!(
            h.app.service.send(Request::Ping).await.is_err(),
            "requests fail at once while disconnected"
        );

        // Started again (here by hand, as `otaq daemon start` would): the link reconnects by itself and tells
        // the page to refetch.
        h.service.bind().await.unwrap();
        h.wait_for("reconnected", connected).await;
        let deadline = Instant::now() + Duration::from_secs(10);
        while resyncs(&h) < 2 {
            assert!(Instant::now() < deadline, "waited for the second resync");
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        h.service.bus().publish(Event::UsageChanged);
        h.wait_for_event("usage_changed after the reconnect", |e| {
            e["name"] == "usage_changed"
        })
        .await;
        h.service.stop_all().await;
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn restart_replaces_the_service_on_request() {
        let h = Harness::new(None);
        h.wait_for("connected", connected).await;
        h.app.service.restart().await.unwrap();
        h.wait_for("connected", connected).await;
        assert_eq!(h.service.started.lock().unwrap().len(), 2, "a new service");
        h.service.stop_all().await;
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_socket_path_too_long_shows_the_start_error_at_once() {
        let tmp = tempfile::tempdir().unwrap();
        let data = DataDir::new(tmp.path().join("a".repeat(120)));
        let service = Arc::new(testing::InProcess {
            data: data.clone(),
            build_id: None,
            claude: None,
            configure: None,
            started: Mutex::new(Vec::new()),
        });
        let link = ServiceLink::start(
            Ok(Settings::new(data, service.clone())),
            Arc::new(EventHub::default()),
        );
        let started = Instant::now();
        while matches!(link.connection(), Connection::Connecting) {
            assert!(started.elapsed() < Duration::from_secs(3), "still waiting");
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        if cfg!(windows) {
            drop(link);
            service.stop_all().await;
            return;
        }
        let Connection::Failed { reason } = link.connection() else {
            panic!("{:?}", link.connection());
        };
        assert!(reason.contains("too long"), "{reason}");
        assert!(
            service.started.lock().unwrap().is_empty(),
            "nothing started"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_failed_start_shows_why_and_a_click_tries_again() {
        struct Failing;
        impl Launcher for Failing {
            fn launch(
                &self,
            ) -> std::pin::Pin<Box<dyn std::future::Future<Output = std::io::Result<()>> + Send + '_>>
            {
                Box::pin(async {
                    Err(std::io::Error::other(
                        "daemon start failed (exit status: 1)",
                    ))
                })
            }
        }
        let tmp = tempfile::tempdir().unwrap();
        let data = DataDir::new(tmp.path().join("h"));
        let link = ServiceLink::start(
            Ok(Settings::new(data, Arc::new(Failing))),
            Arc::new(EventHub::default()),
        );
        let started = Instant::now();
        while matches!(link.connection(), Connection::Connecting) {
            assert!(started.elapsed() < Duration::from_secs(3), "still waiting");
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        let Connection::Failed { reason } = link.connection() else {
            panic!("{:?}", link.connection());
        };
        assert!(reason.contains("exit status: 1"), "{reason}");
        assert!(reason.contains("see the logs"), "{reason}");
        assert!(link.restart().await.is_err(), "fails again");
        assert!(matches!(link.connection(), Connection::Failed { .. }));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn without_a_data_folder_the_link_fails_with_the_reason() {
        let link = ServiceLink::start(Err("no home folder".into()), Arc::new(EventHub::default()));
        let started = Instant::now();
        while matches!(link.connection(), Connection::Connecting) {
            assert!(started.elapsed() < Duration::from_secs(3));
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert_eq!(
            link.connection(),
            Connection::Failed {
                reason: "no home folder".into()
            }
        );
    }
}
