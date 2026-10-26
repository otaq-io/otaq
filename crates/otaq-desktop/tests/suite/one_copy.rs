//! T-0565 (d0139 "Only one copy of Otaq runs"): `otaq-app` started while a copy runs for the same data folder hands its
//! `OTAQ_OPEN` value to that copy and exits at once, before any window or service. The running copy is played by the
//! test: it holds the app's lock and listens where the app does, so no real window opens.

use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use otaq_core::attention::notify::open_env_var;
use otaq_core::data_dir::DataDir;
use otaq_core::ipc;
use otaq_core::service::lock::ServiceLock;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt};

#[test]
fn a_second_start_hands_over_and_exits() {
    let scratch = tempfile::tempdir().unwrap();
    let data = DataDir::new(scratch.path().join("otaq"));
    DataDir::ensure_dir(data.root()).unwrap();
    let home = scratch.path().join("home");
    std::fs::create_dir_all(&home).unwrap();

    let runtime = tokio::runtime::Runtime::new().unwrap();
    let _lock = ServiceLock::acquire(&data.app_lock_path()).unwrap();
    let mut listener = {
        let _inside = runtime.enter();
        ipc::Listener::bind(&data.app_socket_path()).unwrap()
    };
    let handed = runtime.spawn(async move {
        let stream = listener.accept().await.unwrap();
        let (read, mut write) = ipc::split(stream);
        let mut line = String::new();
        tokio::io::BufReader::new(read)
            .read_line(&mut line)
            .await
            .unwrap();
        write.write_all(b"shown\n").await.unwrap();
        write.flush().await.unwrap();
        line
    });

    let mut app = Command::new(env!("CARGO_BIN_EXE_otaq-app"))
        .env(DataDir::env_var(), data.root())
        .env(open_env_var(), "SHOP-22")
        .env("HOME", &home)
        .env("XDG_CONFIG_HOME", home.join(".config"))
        .env("XDG_DATA_HOME", home.join(".local/share"))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let started = Instant::now();
    let status = loop {
        if let Some(status) = app.try_wait().unwrap() {
            break status;
        }
        if started.elapsed() > Duration::from_secs(20) {
            let _ = app.kill();
            let _ = app.wait();
            panic!("the second start didn't exit");
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    let mut stderr = String::new();
    for line in BufReader::new(app.stderr.take().unwrap()).lines() {
        stderr.push_str(&line.unwrap());
    }
    assert!(status.success(), "{status}: {stderr}");
    assert_eq!(runtime.block_on(handed).unwrap(), "SHOP-22\n");
    assert!(
        !data.socket_path().exists() && !data.logs_dir().exists(),
        "no service started"
    );
}
