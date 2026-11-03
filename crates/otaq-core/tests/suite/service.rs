//! The background service in-process: lock, stale socket, requests, version-independent requests and stop.
//! Every test uses a temporary data folder.

use std::time::Duration;

use otaq_core::client::{ClientError, request_once};
use otaq_core::data_dir::DataDir;
use otaq_core::ipc::{self, ServerStream, UserId};
use otaq_core::protocol::{ErrorKind, FrameReader, PROTOCOL_VERSION, Reply, Request, decode};
use otaq_core::service::{Service, ServiceConfig, ServiceError};
use tokio::io::{AsyncWriteExt, BufReader};

struct Running {
    data: DataDir,
    stopper: otaq_core::service::Stopper,
    task: tokio::task::JoinHandle<()>,
    _tmp: tempfile::TempDir,
}

async fn start() -> Running {
    let tmp = tempfile::tempdir().unwrap();
    let data = DataDir::new(tmp.path().join("home"));
    let mut config = ServiceConfig::new(data.clone());
    config.build_id = "test-build.1".into();
    let service = Service::bind(config).await.unwrap();
    let stopper = service.stopper();
    let task = tokio::spawn(service.serve());
    Running {
        data,
        stopper,
        task,
        _tmp: tmp,
    }
}

#[tokio::test]
async fn answers_ping_and_status() {
    let s = start().await;
    let socket = s.data.socket_path();
    assert_eq!(
        request_once(&socket, Request::Ping).await.unwrap(),
        Reply::Pong
    );
    let Reply::ServiceStatus(status) = request_once(&socket, Request::ServiceStatus).await.unwrap()
    else {
        panic!()
    };
    assert_eq!(status.pid, std::process::id());
    assert_eq!(status.build_id, "test-build.1");
    assert_eq!(status.protocol_version, PROTOCOL_VERSION);
    assert_eq!((status.agents_working, status.agents_waiting), (0, 0));
    s.stopper.stop();
    s.task.await.unwrap();
}

#[tokio::test]
async fn stop_request_cleans_up() {
    let s = start().await;
    let socket = s.data.socket_path();
    assert!(s.data.service_lock_path().exists());
    assert_eq!(
        request_once(&socket, Request::ServiceStop).await.unwrap(),
        Reply::Ok
    );
    tokio::time::timeout(Duration::from_secs(5), s.task)
        .await
        .unwrap()
        .unwrap();
    assert!(!socket.exists(), "socket removed (never a file on Windows)");
    assert!(!s.data.service_lock_path().exists(), "lock file removed");
    assert!(matches!(
        request_once(&socket, Request::Ping).await,
        Err(ClientError::NotRunning(_))
    ));
}

#[tokio::test]
async fn only_one_service_per_data_folder() {
    let s = start().await;
    match Service::bind(ServiceConfig::new(s.data.clone())).await {
        Err(ServiceError::AlreadyRunning { pid }) => assert_eq!(pid, Some(std::process::id())),
        Err(e) => panic!("{e}"),
        Ok(_) => panic!("a second service started"),
    }
    // The first one still answers.
    assert_eq!(
        request_once(&s.data.socket_path(), Request::Ping)
            .await
            .unwrap(),
        Reply::Pong
    );
    s.stopper.stop();
    s.task.await.unwrap();
}

#[tokio::test]
async fn stale_socket_and_lock_from_a_crash_are_replaced() {
    let tmp = tempfile::tempdir().unwrap();
    let data = DataDir::new(tmp.path().join("home"));
    DataDir::ensure_dir(data.root()).unwrap();
    // A crashed service leaves its socket file and its lock file (with a dead PID) behind. (A Windows pipe ends
    // with its process, so only the lock file is left there.)
    #[cfg(unix)]
    drop(std::os::unix::net::UnixListener::bind(data.socket_path()).unwrap());
    std::fs::write(data.service_lock_path(), "999999\n").unwrap();
    assert!(matches!(
        request_once(&data.socket_path(), Request::Ping).await,
        Err(ClientError::NotRunning(_))
    ));

    let service = Service::bind(ServiceConfig::new(data.clone()))
        .await
        .unwrap();
    let stopper = service.stopper();
    let task = tokio::spawn(service.serve());
    assert_eq!(
        request_once(&data.socket_path(), Request::Ping)
            .await
            .unwrap(),
        Reply::Pong
    );
    stopper.stop();
    task.await.unwrap();
}

/// Sends raw frames and returns the decoded-or-foreign answers.
async fn raw_exchange(socket: &std::path::Path, frame: &str) -> serde_json::Value {
    let stream = ipc::connect(socket).await.unwrap();
    let (read, mut write) = ipc::split(stream);
    write.write_all(frame.as_bytes()).await.unwrap();
    write.write_all(b"\n").await.unwrap();
    let mut reader = FrameReader::new(BufReader::new(read));
    let frame = reader.next_frame().await.unwrap().unwrap();
    serde_json::from_slice(frame).unwrap()
}

#[tokio::test]
async fn other_protocol_versions() {
    let s = start().await;
    let socket = s.data.socket_path();

    // Status works whatever version the caller speaks.
    let answer = raw_exchange(
        &socket,
        r#"{"protocol_version":99,"id":4,"request":{"method":"service.status"}}"#,
    )
    .await;
    assert_eq!(answer["id"], 4);
    assert_eq!(answer["protocol_version"], PROTOCOL_VERSION);
    assert_eq!(
        answer["reply"]["service_status"]["build_id"],
        "test-build.1"
    );

    // Anything else is refused with a distinct error kind.
    let answer = raw_exchange(
        &socket,
        r#"{"protocol_version":99,"id":5,"request":{"method":"tickets.list"}}"#,
    )
    .await;
    assert_eq!(answer["id"], 5);
    assert_eq!(answer["error"]["kind"], "incompatible_protocol");
    assert_eq!(
        answer["error"]["data"]["protocol_version"],
        PROTOCOL_VERSION
    );

    // Garbage gets a bad_request error, and the connection stays usable for the next frame.
    let answer = raw_exchange(&socket, "not json").await;
    let message = decode(&serde_json::to_vec(&answer).unwrap()).unwrap();
    let otaq_core::protocol::Body::Error(error) = message.body else {
        panic!("{answer}")
    };
    assert_eq!(error.kind, ErrorKind::BadRequest);

    s.stopper.stop();
    s.task.await.unwrap();
}

#[cfg(unix)]
#[tokio::test]
async fn socket_and_folder_are_user_only_even_when_they_were_looser() {
    use std::os::unix::fs::PermissionsExt;
    let tmp = tempfile::tempdir().unwrap();
    let data = DataDir::new(tmp.path().join("home"));
    std::fs::create_dir(data.root()).unwrap();
    std::fs::set_permissions(data.root(), std::fs::Permissions::from_mode(0o755)).unwrap();

    let service = Service::bind(ServiceConfig::new(data.clone()))
        .await
        .unwrap();
    let mode = |p: &std::path::Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode(data.root()), 0o700);
    assert_eq!(mode(&data.socket_path()), 0o600);
    let stopper = service.stopper();
    let task = tokio::spawn(service.serve());
    stopper.stop();
    task.await.unwrap();
}

/// The pipe carries an access list with one entry: full access for this user, nobody else (not even
/// Administrators or SYSTEM).
#[cfg(windows)]
#[tokio::test]
async fn the_pipe_grants_access_only_to_the_current_user() {
    use otaq_core::service::access::current_user;
    let tmp = tempfile::tempdir().unwrap();
    let data = DataDir::new(tmp.path().join("home"));
    DataDir::ensure_dir(data.root()).unwrap();
    let listener = ipc::Listener::bind(&data.socket_path()).unwrap();
    let sddl = listener.dacl_sddl().unwrap();
    // `P`: protected, nothing inherited; one allow entry, for the current user's SID, with full access. As Windows
    // writes it: the built-in Administrator (GitHub's Windows runner) comes back as `LA`, not its SID.
    let want = ipc::dacl_sddl_as_written(&format!("D:P(A;;FA;;;{})", current_user())).unwrap();
    assert_eq!(sddl, want, "{sddl}");
    // A second listener on the same name is refused, so nobody can sit on a running service's pipe.
    assert!(ipc::Listener::bind(&data.socket_path()).is_err());
}

/// Poses as another user: every peer looks like account `.0`.
struct FixedPeer(UserId);

impl otaq_core::service::access::PeerCredentials for FixedPeer {
    // The account is a UID (Copy) on Unix and a SID string on Windows.
    #[cfg_attr(unix, allow(clippy::clone_on_copy))]
    fn peer_user(&self, _stream: &ServerStream) -> std::io::Result<UserId> {
        Ok(self.0.clone())
    }
}

#[cfg(unix)]
fn another_user() -> UserId {
    otaq_core::service::access::current_user() + 1
}

#[cfg(windows)]
fn another_user() -> UserId {
    format!("{}0", otaq_core::service::access::current_user())
}

#[tokio::test]
async fn connections_from_another_user_are_closed() {
    let tmp = tempfile::tempdir().unwrap();
    let data = DataDir::new(tmp.path().join("home"));
    let mut config = ServiceConfig::new(data.clone());
    config.peer_credentials = std::sync::Arc::new(FixedPeer(another_user()));
    let service = Service::bind(config).await.unwrap();
    let stopper = service.stopper();
    let task = tokio::spawn(service.serve());

    match request_once(&data.socket_path(), Request::Ping).await {
        Err(ClientError::Closed) | Err(ClientError::Io(_)) => {}
        other => panic!("another user's request was answered: {other:?}"),
    }
    stopper.stop();
    task.await.unwrap();

    // The real credential source accepts this test process (same user).
    let service = Service::bind(ServiceConfig::new(data.clone()))
        .await
        .unwrap();
    let stopper = service.stopper();
    let task = tokio::spawn(service.serve());
    assert_eq!(
        request_once(&data.socket_path(), Request::Ping)
            .await
            .unwrap(),
        Reply::Pong
    );
    stopper.stop();
    task.await.unwrap();
}

#[tokio::test]
async fn onboarding_state_and_remembered_dismissal() {
    use otaq_core::protocol::OnboardingState;
    let tmp = tempfile::tempdir().unwrap();
    let data = DataDir::new(tmp.path().join("home"));
    let socket = data.socket_path();
    let run = |data: DataDir| async move {
        let service = Service::bind(ServiceConfig::new(data)).await.unwrap();
        (service.stopper(), tokio::spawn(service.serve()))
    };

    let (stopper, task) = run(data.clone()).await;
    let state = |reply| match reply {
        Reply::OnboardingState(state) => state,
        other => panic!("{other:?}"),
    };
    assert_eq!(
        state(
            request_once(&socket, Request::OnboardingState)
                .await
                .unwrap()
        ),
        OnboardingState {
            has_projects: false,
            permission_warning_dismissed: false,
            trust_note: false,
            trust_note_tools: Vec::new(),
            trust_failed: Vec::new(),
        }
    );
    assert_eq!(
        request_once(&socket, Request::DismissPermissionWarning)
            .await
            .unwrap(),
        Reply::Ok
    );
    stopper.stop();
    task.await.unwrap();

    // The dismissal survives a restart of the service; a stored project ends the first run.
    let store = otaq_core::store::Store::open_in(&data).await.unwrap();
    store
        .insert_project(&otaq_core::store::projects::NewProject {
            key: "SHOP".into(),
            name: "shop-web".into(),
            repo_path: "/tmp/shop-web".into(),
            base_branch: "main".into(),
            flow_setup: false,
        })
        .await
        .unwrap();
    store.close().await;
    let (stopper, task) = run(data.clone()).await;
    assert_eq!(
        state(
            request_once(&socket, Request::OnboardingState)
                .await
                .unwrap()
        ),
        OnboardingState {
            has_projects: true,
            permission_warning_dismissed: true,
            trust_note: false,
            trust_note_tools: Vec::new(),
            trust_failed: Vec::new(),
        }
    );
    stopper.stop();
    task.await.unwrap();
}
