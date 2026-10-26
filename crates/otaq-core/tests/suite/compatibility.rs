//! Build-ID and protocol check (SVC-03) and the user-clicked restart. Services run in-process on temporary data
//! folders; the "other protocol" service is a small stand-in that speaks only the version-independent requests.

use std::future::Future;
use std::io;
use std::pin::Pin;
use std::sync::Mutex;
use std::time::Duration;

use otaq_core::client::{
    Client, ClientConfig, ClientError, Compatibility, ConnectionState, Launcher,
};
use otaq_core::data_dir::DataDir;
use otaq_core::protocol::{FrameReader, Reply, Request};
use otaq_core::service::{Service, ServiceConfig, ServiceError, Stopper};
use serde_json::{Value, json};
use tokio::io::{AsyncWriteExt, BufReader};

struct Running {
    stopper: Stopper,
    task: tokio::task::JoinHandle<()>,
}

async fn bind(data: &DataDir, build_id: &str) -> Result<Running, ServiceError> {
    let mut config = ServiceConfig::new(data.clone());
    config.build_id = build_id.into();
    let service = Service::bind(config).await?;
    let stopper = service.stopper();
    let task = tokio::spawn(service.serve());
    Ok(Running { stopper, task })
}

/// Starts this build's service in-process, like `otaq daemon start` next to the app would.
struct StartBuild {
    data: DataDir,
    build_id: String,
    started: Mutex<Vec<Running>>,
}

impl Launcher for StartBuild {
    fn launch(&self) -> Pin<Box<dyn Future<Output = io::Result<()>> + Send + '_>> {
        Box::pin(async move {
            let running = bind(&self.data, &self.build_id)
                .await
                .map_err(|e| io::Error::other(e.to_string()))?;
            self.started.lock().unwrap().push(running);
            Ok(())
        })
    }
}

fn setup(own_build: &str) -> (tempfile::TempDir, DataDir, ClientConfig) {
    let tmp = tempfile::tempdir().unwrap();
    let data = DataDir::new(tmp.path().join("home"));
    let mut config = ClientConfig::new(&data);
    config.build_id = own_build.into();
    config.backoff_min = Duration::from_millis(10);
    config.backoff_max = Duration::from_millis(50);
    (tmp, data, config)
}

#[tokio::test]
async fn same_build_is_same() {
    let (_tmp, data, config) = setup("b.2");
    let running = bind(&data, "b.2").await.unwrap();
    let client = Client::connect(config).await.unwrap();
    assert_eq!(client.compatibility(), Some(Compatibility::Same));
    drop(client);
    running.stopper.stop();
    running.task.await.unwrap();
}

#[tokio::test]
async fn an_older_build_is_reported_and_restart_replaces_it() {
    let (_tmp, data, config) = setup("new.2");
    let old = bind(&data, "old.1").await.unwrap();
    let client = Client::connect(config).await.unwrap();
    assert_eq!(client.compatibility(), Some(Compatibility::ServiceOlder));
    // An older service with the same protocol keeps working until the user restarts.
    assert_eq!(client.request(Request::Ping).await.unwrap(), Reply::Pong);
    let ConnectionState::Connected(before) = client.state().borrow().clone() else {
        panic!()
    };

    let launcher = StartBuild {
        data: data.clone(),
        build_id: "new.2".into(),
        started: Mutex::new(Vec::new()),
    };
    let after = client.restart(&launcher).await.unwrap();
    assert_eq!(after.compatibility, Compatibility::Same);
    assert_eq!(after.status.build_id, "new.2");
    assert_ne!(after.status.started_at, before.status.started_at);
    // The old service has ended.
    tokio::time::timeout(Duration::from_secs(5), old.task)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(client.request(Request::Ping).await.unwrap(), Reply::Pong);

    drop(client);
    let new = launcher.started.lock().unwrap().pop().unwrap();
    new.stopper.stop();
    new.task.await.unwrap();
}

/// A service from a future protocol version: answers status and stop (the version-independent requests) in
/// its own envelope, refuses everything else, and ends after stop.
async fn other_protocol_service(data: &DataDir) -> tokio::task::JoinHandle<()> {
    DataDir::ensure_dir(data.root()).unwrap();
    let mut listener = otaq_core::ipc::Listener::bind(&data.socket_path()).unwrap();
    tokio::spawn(async move {
        loop {
            let stream = listener.accept().await.unwrap();
            let (read, mut write) = otaq_core::ipc::split(stream);
            let mut reader = FrameReader::new(BufReader::new(read));
            while let Ok(Some(frame)) = reader.next_frame().await {
                let request: Value = serde_json::from_slice(frame).unwrap();
                let id = request["id"].clone();
                let method = request["request"]["method"]
                    .as_str()
                    .unwrap_or_default()
                    .to_owned();
                let answer = match method.as_str() {
                    "service.status" => {
                        json!({"protocol_version": 99, "id": id, "reply": {"service_status": {
                        "pid": 1, "build_id": "future.9", "protocol_version": 99, "started_at": 5,
                        "uptime_secs": 1, "agents_working": 3, "agents_waiting": 2, "new_field": true}}})
                    }
                    "service.stop" => json!({"protocol_version": 99, "id": id, "reply": "ok"}),
                    _ => json!({"protocol_version": 99, "id": id,
                                "error": {"kind": "incompatible_protocol", "message": "no"}}),
                };
                let mut line = serde_json::to_vec(&answer).unwrap();
                line.push(b'\n');
                write.write_all(&line).await.unwrap();
                if method == "service.stop" {
                    return;
                }
            }
        }
    })
}

#[tokio::test]
async fn another_protocol_is_incompatible_and_only_status_and_restart_work() {
    let (_tmp, data, config) = setup("new.2");
    let stand_in = other_protocol_service(&data).await;
    let client = Client::connect(config).await.unwrap();
    assert_eq!(client.compatibility(), Some(Compatibility::Incompatible));
    let ConnectionState::Connected(info) = client.state().borrow().clone() else {
        panic!()
    };
    // The counts for "Restart required (3 agents working, 2 waiting)" still arrive.
    assert_eq!(
        (info.status.agents_working, info.status.agents_waiting),
        (3, 2)
    );

    // Refused client-side, without asking the service.
    assert!(matches!(
        client.request(Request::Subscribe(Default::default())).await,
        Err(ClientError::Incompatible)
    ));
    assert!(matches!(
        client.request(Request::ServiceStatus).await,
        Ok(Reply::ServiceStatus(_))
    ));

    let launcher = StartBuild {
        data: data.clone(),
        build_id: "new.2".into(),
        started: Mutex::new(Vec::new()),
    };
    let after = client.restart(&launcher).await.unwrap();
    assert_eq!(after.compatibility, Compatibility::Same);
    stand_in.await.unwrap();
    assert!(
        client
            .request(Request::Subscribe(Default::default()))
            .await
            .is_ok()
    );

    drop(client);
    let new = launcher.started.lock().unwrap().pop().unwrap();
    new.stopper.stop();
    new.task.await.unwrap();
}

#[test]
fn compatibility_rules() {
    use otaq_core::protocol::{PROTOCOL_VERSION, ServiceStatus};
    let status = |build: &str, protocol| ServiceStatus {
        pid: 1,
        build_id: build.into(),
        protocol_version: protocol,
        started_at: 0,
        uptime_secs: 0,
        agents_working: 0,
        agents_waiting: 0,
        ..Default::default()
    };
    assert_eq!(
        Compatibility::of(&status("a", PROTOCOL_VERSION), "a"),
        Compatibility::Same
    );
    assert_eq!(
        Compatibility::of(&status("a", PROTOCOL_VERSION), "b"),
        Compatibility::ServiceOlder
    );
    assert_eq!(
        Compatibility::of(&status("a", PROTOCOL_VERSION + 1), "a"),
        Compatibility::Incompatible
    );
}
