//! Talking to the background service, shared by the app and the CLI (SVC-04, ARC-03).
//!
//! [`Client`] keeps one connection open and repairs it: when the service goes away the client reports
//! [`ConnectionState::Reconnecting`], retries with backoff, and on success reports
//! [`ConnectionState::Connected`] again and re-subscribes to events. Requests made meanwhile fail at once with
//! [`ClientError::Disconnected`], so the UI can disable input instead of waiting. Reconnecting never starts a
//! service; only [`Client::connect_or_start`] does, once, at launch (d0055). The app is only a client: dropping
//! a `Client` never stops the service (d0022).
//!
//! [`request_once`] is the one-shot form the CLI uses for single commands.

mod connection;
mod launcher;

use std::fmt;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::io::BufReader;
use tokio::sync::{Notify, broadcast, watch};

use crate::data_dir::DataDir;
use crate::protocol::{
    self, Body, DecodeError, ErrorBody, Event, FrameReader, Message, PROTOCOL_VERSION, Reply,
    Request, ServiceStatus, VERSION_INDEPENDENT_METHODS, decode, write_message,
};
use connection::Connection;
pub use launcher::{CommandLauncher, Launcher};

/// Connection settings. [`ClientConfig::new`] has the defaults.
#[derive(Debug, Clone)]
pub struct ClientConfig {
    pub socket: PathBuf,
    /// The service's lock file; it disappears when a stopped service has fully ended.
    pub lock: PathBuf,
    /// This program's build ID, compared with the service's (d0022). Tests override it.
    pub build_id: String,
    /// First wait before reconnecting; doubled after every failed attempt up to `backoff_max`.
    pub backoff_min: Duration,
    pub backoff_max: Duration,
    /// How long starting or restarting waits for the service to stop or answer.
    pub start_timeout: Duration,
    /// The actor requests ask to be recorded as (`--actor`, CLI-04). The service decides: an agent caller always
    /// acts as itself.
    pub actor: Option<String>,
    /// This client is the desktop app: its event subscription tells the service the app is open (Q565).
    pub app: bool,
}

impl ClientConfig {
    pub fn new(data: &DataDir) -> Self {
        Self {
            socket: data.socket_path(),
            lock: data.service_lock_path(),
            build_id: protocol::build_id().to_owned(),
            backoff_min: Duration::from_millis(100),
            backoff_max: Duration::from_secs(2),
            start_timeout: Duration::from_secs(10),
            actor: None,
            app: false,
        }
    }
}

/// How the running service relates to this program (d0022, SVC-03).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Compatibility {
    /// Same build.
    Same,
    /// Another build with the same protocol: "Service update ready · Restart now". Everything keeps working.
    ServiceOlder,
    /// Another protocol: "Restart required". Only status and restart work.
    Incompatible,
}

impl Compatibility {
    /// Compares the service's status with this program's build. Any other build ID counts as older, since the
    /// usual cause is a rebuild of Otaq while the service kept running (d0022: "if they differ").
    pub fn of(service: &ServiceStatus, own_build_id: &str) -> Self {
        if service.protocol_version != PROTOCOL_VERSION {
            Self::Incompatible
        } else if service.build_id != own_build_id {
            Self::ServiceOlder
        } else {
            Self::Same
        }
    }
}

/// What the client learned about the service when the connection opened.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServiceInfo {
    pub status: ServiceStatus,
    pub compatibility: Compatibility,
}

/// What the app shows about its link to the service.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConnectionState {
    Connected(ServiceInfo),
    /// The connection dropped; `attempt` counts the tries so far ("Reconnecting… agents keep running").
    Reconnecting {
        attempt: u32,
    },
}

/// A self-repairing connection to the service. Cheap to clone; the connection closes when the last clone is
/// dropped.
#[derive(Clone)]
pub struct Client {
    inner: Arc<Inner>,
}

struct Inner {
    config: ClientConfig,
    state: watch::Sender<ConnectionState>,
    connection: Mutex<Option<Arc<Connection>>>,
    events: broadcast::Sender<Event>,
    subscribed: AtomicBool,
    /// Wakes the reconnect loop before its backoff ends (after a restart launched a new service).
    retry_now: Notify,
    supervisor: Mutex<Option<tokio::task::JoinHandle<()>>>,
}

impl Drop for Inner {
    fn drop(&mut self) {
        if let Some(task) = self.supervisor.lock().expect("supervisor").take() {
            task.abort();
        }
    }
}

/// Events buffered for the app's subscribers before the oldest are dropped.
const EVENT_BUFFER: usize = 1024;

impl Client {
    /// Connects to a running service. Fails with [`ClientError::NotRunning`] if none answers.
    pub async fn connect(config: ClientConfig) -> Result<Self, ClientError> {
        let (events, _) = broadcast::channel(EVENT_BUFFER);
        let (connection, info) = establish(&config, &events, false).await?;
        let inner = Arc::new(Inner {
            config,
            state: watch::Sender::new(ConnectionState::Connected(info)),
            connection: Mutex::new(Some(Arc::new(connection))),
            events,
            subscribed: AtomicBool::new(false),
            retry_now: Notify::new(),
            supervisor: Mutex::new(None),
        });
        let supervisor = tokio::spawn(supervise(Arc::downgrade(&inner)));
        *inner.supervisor.lock().expect("supervisor") = Some(supervisor);
        Ok(Self { inner })
    }

    /// Connects, starting the service with `launcher` first if none answers (d0055). The launcher runs at most
    /// once; the service's lock file makes sure only one service ever runs.
    pub async fn connect_or_start(
        config: ClientConfig,
        launcher: &dyn Launcher,
    ) -> Result<Self, ClientError> {
        match Self::connect(config.clone()).await {
            Err(ClientError::NotRunning(_)) => {}
            other => return other,
        }
        launcher.launch().await.map_err(ClientError::Start)?;
        let deadline = tokio::time::Instant::now() + config.start_timeout;
        loop {
            match Self::connect(config.clone()).await {
                Err(ClientError::NotRunning(e)) if tokio::time::Instant::now() >= deadline => {
                    return Err(ClientError::NotRunning(e));
                }
                Err(ClientError::NotRunning(_)) => {
                    tokio::time::sleep(Duration::from_millis(50)).await;
                }
                other => return other,
            }
        }
    }

    /// The connection state now and as it changes.
    pub fn state(&self) -> watch::Receiver<ConnectionState> {
        self.inner.state.subscribe()
    }

    pub fn is_connected(&self) -> bool {
        matches!(*self.inner.state.borrow(), ConnectionState::Connected(_))
    }

    /// The compatibility of the connected service, or `None` while reconnecting.
    pub fn compatibility(&self) -> Option<Compatibility> {
        match &*self.inner.state.borrow() {
            ConnectionState::Connected(info) => Some(info.compatibility),
            ConnectionState::Reconnecting { .. } => None,
        }
    }

    /// Sends a request. Fails at once with [`ClientError::Disconnected`] while reconnecting, and with
    /// [`ClientError::Incompatible`] for anything but status and stop when the protocols differ.
    pub async fn request(&self, request: Request) -> Result<Reply, ClientError> {
        if self.compatibility() == Some(Compatibility::Incompatible)
            && !VERSION_INDEPENDENT_METHODS.contains(&request.method())
        {
            return Err(ClientError::Incompatible);
        }
        let connection = self
            .inner
            .connection
            .lock()
            .expect("connection")
            .clone()
            .filter(|c| !c.is_closed())
            .ok_or(ClientError::Disconnected)?;
        connection.request(request).await
    }

    /// Receives the service's events from now on, also across reconnects (the subscription is renewed on
    /// every new connection). After a reconnect, events from the gap are lost: refetch what is shown.
    pub async fn subscribe(&self) -> Result<broadcast::Receiver<Event>, ClientError> {
        let receiver = self.inner.events.subscribe();
        if !self.inner.subscribed.swap(true, Ordering::SeqCst) {
            match self.request(subscribe_request(&self.inner.config)).await {
                Ok(_) | Err(ClientError::Disconnected) | Err(ClientError::Incompatible) => {}
                Err(e) => return Err(e),
            }
        }
        Ok(receiver)
    }

    /// Replaces the running service with this build's: asks the old one to stop, waits until it has ended,
    /// starts the new one with `launcher` and waits until the client is connected to it. Also starts a service
    /// when none runs. Only ever called on the user's click; the service never restarts itself (d0022). A
    /// restart ends the old service's sessions like a reboot (d0045); reconcile handles them from T-0111.
    pub async fn restart(&self, launcher: &dyn Launcher) -> Result<ServiceInfo, ClientError> {
        let config = &self.inner.config;
        let old = match &*self.inner.state.borrow() {
            ConnectionState::Connected(info) => Some(info.status.clone()),
            ConnectionState::Reconnecting { .. } => None,
        };
        match self.request(Request::ServiceStop).await {
            Ok(Reply::Ok) | Err(ClientError::Disconnected) => {}
            Ok(other) => return Err(ClientError::Unexpected(Box::new(other))),
            Err(e) => return Err(e),
        }
        // A stopping service removes its lock file last, after closing the store. A crashed one leaves the file
        // behind with a PID that no longer runs: nothing to wait for then (the new service takes the file over).
        let deadline = tokio::time::Instant::now() + config.start_timeout;
        while config.lock.exists() && crate::service::lock::service_alive(&config.lock) {
            if tokio::time::Instant::now() >= deadline {
                return Err(ClientError::Start(io::Error::other(
                    "the old background service didn't stop in time",
                )));
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
        launcher.launch().await.map_err(ClientError::Start)?;
        self.inner.retry_now.notify_one();

        let mut state = self.state();
        let is_new = |s: &ConnectionState| match s {
            ConnectionState::Connected(info) => old.as_ref().is_none_or(|old| {
                (old.pid, old.started_at) != (info.status.pid, info.status.started_at)
            }),
            ConnectionState::Reconnecting { .. } => false,
        };
        let connected = tokio::time::timeout(config.start_timeout, state.wait_for(is_new))
            .await
            .map_err(|_| ClientError::Disconnected)?
            .map_err(|_| ClientError::Disconnected)?;
        match &*connected {
            ConnectionState::Connected(info) => Ok(info.clone()),
            ConnectionState::Reconnecting { .. } => Err(ClientError::Disconnected),
        }
    }
}

/// The event subscription, as the app's when this client is the app.
fn subscribe_request(config: &ClientConfig) -> Request {
    Request::Subscribe(protocol::SubscribeParams { app: config.app })
}

/// Opens a connection, reads the service's status, and subscribes to events if asked (and possible).
async fn establish(
    config: &ClientConfig,
    events: &broadcast::Sender<Event>,
    subscribe: bool,
) -> Result<(Connection, ServiceInfo), ClientError> {
    let connection = Connection::open(&config.socket, events.clone(), config.actor.clone()).await?;
    let status = match connection.request(Request::ServiceStatus).await? {
        Reply::ServiceStatus(status) => status,
        other => return Err(ClientError::Unexpected(Box::new(other))),
    };
    let compatibility = Compatibility::of(&status, &config.build_id);
    if subscribe && compatibility != Compatibility::Incompatible {
        connection.request(subscribe_request(config)).await?;
    }
    Ok((
        connection,
        ServiceInfo {
            status,
            compatibility,
        },
    ))
}

/// Waits for the connection to drop, then reconnects with backoff. Ends when the client is dropped.
async fn supervise(inner: std::sync::Weak<Inner>) {
    loop {
        let Some(current) = inner
            .upgrade()
            .and_then(|i| i.connection.lock().expect("connection").clone())
        else {
            return;
        };
        current.closed().await;
        drop(current);

        let mut attempt = 0;
        let mut backoff = match inner.upgrade() {
            Some(i) => i.config.backoff_min,
            None => return,
        };
        loop {
            attempt += 1;
            let Some(client) = inner.upgrade() else {
                return;
            };
            *client.connection.lock().expect("connection") = None;
            client
                .state
                .send_replace(ConnectionState::Reconnecting { attempt });
            let max = client.config.backoff_max;
            tokio::select! {
                _ = tokio::time::sleep(backoff) => {}
                _ = client.retry_now.notified() => {}
            }
            backoff = (backoff * 2).min(max);

            let subscribe = client.subscribed.load(Ordering::SeqCst);
            match establish(&client.config, &client.events, subscribe).await {
                Ok((connection, info)) => {
                    tracing::info!(attempt, "reconnected to the background service");
                    let compatible = info.compatibility != Compatibility::Incompatible;
                    let connection = Arc::new(connection);
                    *client.connection.lock().expect("connection") = Some(connection.clone());
                    // `subscribe()` may have been called while this connection was being set up.
                    if compatible && !subscribe && client.subscribed.load(Ordering::SeqCst) {
                        let _ = connection.request(subscribe_request(&client.config)).await;
                    }
                    client.state.send_replace(ConnectionState::Connected(info));
                    break;
                }
                Err(e) => tracing::debug!(attempt, error = %e, "reconnect failed"),
            }
        }
    }
}

/// Sends one request on a fresh connection and waits for its answer. Version-independent requests
/// (`ping`, `service.status`, `service.stop`) also work against a service with another protocol version.
pub async fn request_once(socket: &Path, request: Request) -> Result<Reply, ClientError> {
    let stream = crate::ipc::connect(socket)
        .await
        .map_err(ClientError::NotRunning)?;
    let (read, mut write) = crate::ipc::split(stream);
    const ID: u64 = 1;
    write_message(&mut write, &Message::request(ID, request))
        .await
        .map_err(ClientError::Io)?;
    let mut reader = FrameReader::new(BufReader::new(read));
    loop {
        let frame = reader
            .next_frame()
            .await
            .map_err(ClientError::Io)?
            .ok_or(ClientError::Closed)?;
        let message = match decode(frame) {
            Ok(message) => message,
            Err(DecodeError::IncompatibleVersion(foreign)) => foreign.read_as_current().ok_or(
                ClientError::Protocol(DecodeError::IncompatibleVersion(foreign)),
            )?,
            Err(e) => return Err(ClientError::Protocol(e)),
        };
        if message.id != Some(ID) {
            continue;
        }
        return match message.body {
            Body::Reply(reply) => Ok(reply),
            Body::Error(error) => Err(ClientError::Service(error)),
            _ => Err(ClientError::Closed),
        };
    }
}

#[derive(Debug)]
pub enum ClientError {
    /// No service is listening on the socket.
    NotRunning(io::Error),
    /// The client is reconnecting; the request was not sent.
    Disconnected,
    /// The service speaks another protocol; only status and restart work ("Restart required").
    Incompatible,
    /// The service couldn't be started.
    Start(io::Error),
    /// The service answered with a reply that doesn't fit the request.
    Unexpected(Box<Reply>),
    /// The connection ended before the answer arrived.
    Closed,
    Io(io::Error),
    /// The service sent something this build can't read.
    Protocol(DecodeError),
    /// The service answered with an error.
    Service(ErrorBody),
}

impl fmt::Display for ClientError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotRunning(_) => write!(f, "the background service isn't running"),
            Self::Disconnected => write!(f, "not connected to the background service"),
            Self::Incompatible => write!(
                f,
                "the background service is from another version; restart it first"
            ),
            Self::Start(e) => write!(f, "can't start the background service: {e}"),
            Self::Unexpected(reply) => write!(
                f,
                "unexpected answer from the background service: {reply:?}"
            ),
            Self::Closed => write!(f, "the background service closed the connection"),
            Self::Io(e) => write!(f, "connection to the background service failed: {e}"),
            Self::Protocol(e) => write!(f, "{e}"),
            Self::Service(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for ClientError {}
