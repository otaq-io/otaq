//! The client starts the real service through `otaq daemon start` when none runs, and never a second one.
//! Uses a temporary data folder; the service is stopped at the end even if the test fails.

use std::future::Future;
use std::path::Path;
use std::pin::Pin;
use std::process::Command;

use otaq_core::client::{Client, ClientConfig, CommandLauncher, Launcher};
use otaq_core::data_dir::DataDir;
use otaq_core::protocol::{Reply, Request};

/// How long a test waits for the service to stop, end or answer before calling it stuck, also as the client's start
/// timeout: far longer than it takes, even with every test of the workspace running at once. Each wait ends as soon
/// as the service gets there.
const SLOW_MACHINE: std::time::Duration = std::time::Duration::from_secs(60);

struct StopOnDrop(std::path::PathBuf);

impl Drop for StopOnDrop {
    fn drop(&mut self) {
        let _ = Command::new(env!("CARGO_BIN_EXE_otaq"))
            .args(["daemon", "stop"])
            .env(DataDir::env_var(), &self.0)
            .env("PATH", self.0.join("no-tools"))
            .output();
    }
}

/// `otaq daemon start` for the test's data folder, the test then waiting for the service itself rather than for that
/// command's verdict. The command gives up after its own few seconds, but the service it started keeps starting: on
/// a machine busy with the whole test run, a debug build's start (its runtime copy, the store) can take longer than
/// that. A start that really failed still fails the test, when the client finds no service within [`SLOW_MACHINE`].
struct UntilItAnswers(CommandLauncher);

impl Launcher for UntilItAnswers {
    fn launch(&self) -> Pin<Box<dyn Future<Output = std::io::Result<()>> + Send + '_>> {
        Box::pin(async move {
            if let Err(e) = self.0.launch().await {
                eprintln!("{e}; waiting for the service to answer");
            }
            Ok(())
        })
    }
}

fn launcher(data: &Path) -> UntilItAnswers {
    UntilItAnswers(
        CommandLauncher::new(env!("CARGO_BIN_EXE_otaq"))
            .with_env(DataDir::env_var(), data)
            // The real service locates Claude Code's config file: never the user's (PRJ-06).
            .with_env("CLAUDE_CONFIG_DIR", data)
            // Nor any agent tool's: the service detects the tools on an empty test PATH, with a test home.
            .with_env("PATH", data.join("no-tools"))
            .with_env("HOME", data),
    )
}

#[tokio::test]
async fn connect_or_start_launches_the_daemon_once() {
    let tmp = tempfile::tempdir().unwrap();
    let _guard = StopOnDrop(tmp.path().to_owned());
    let data = DataDir::new(tmp.path());
    let launcher = launcher(tmp.path());
    let mut config = ClientConfig::new(&data);
    config.start_timeout = SLOW_MACHINE;

    let client = Client::connect_or_start(config.clone(), &launcher)
        .await
        .unwrap();
    let Reply::ServiceStatus(first) = client.request(Request::ServiceStatus).await.unwrap() else {
        panic!()
    };
    assert_ne!(first.pid, std::process::id(), "a separate service process");

    // A second client (like a second app window or launch) finds the same service.
    let second = Client::connect_or_start(config, &launcher).await.unwrap();
    let Reply::ServiceStatus(again) = second.request(Request::ServiceStatus).await.unwrap() else {
        panic!()
    };
    assert_eq!(again.pid, first.pid);

    // Dropping the clients (closing the app) leaves the service running.
    drop((client, second));
    let out = Command::new(env!("CARGO_BIN_EXE_otaq"))
        .args(["daemon", "status", "--json"])
        .env(DataDir::env_var(), tmp.path())
        .output()
        .unwrap();
    let status: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(status["running"], true);
    assert_eq!(status["pid"], first.pid);
}

#[tokio::test]
async fn restart_replaces_the_service_process() {
    let tmp = tempfile::tempdir().unwrap();
    let _guard = StopOnDrop(tmp.path().to_owned());
    let data = DataDir::new(tmp.path());
    let launcher = launcher(tmp.path());
    let mut config = ClientConfig::new(&data);
    // Pretend this program was rebuilt after the service started.
    config.build_id = "rebuilt.1".into();
    config.start_timeout = SLOW_MACHINE;

    let client = Client::connect_or_start(config, &launcher).await.unwrap();
    assert_eq!(
        client.compatibility(),
        Some(otaq_core::client::Compatibility::ServiceOlder)
    );
    let Reply::ServiceStatus(old) = client.request(Request::ServiceStatus).await.unwrap() else {
        panic!()
    };

    let new = client.restart(&launcher).await.unwrap();
    assert_ne!(new.status.pid, old.pid, "a new service process");
    let alive = || {
        Command::new("kill")
            .args(["-0", &old.pid.to_string()])
            .output()
            .unwrap()
            .status
            .success()
    };
    // The old service removes its lock file before its process has finished exiting (its runtime still winds down),
    // so the process may outlive the restart for a while.
    let deadline = std::time::Instant::now() + SLOW_MACHINE;
    while alive() && std::time::Instant::now() < deadline {
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    assert!(!alive(), "the old service process ended");
    assert_eq!(client.request(Request::Ping).await.unwrap(), Reply::Pong);
}
