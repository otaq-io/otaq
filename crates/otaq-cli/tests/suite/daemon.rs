//! `otaq daemon start | stop | status` against a temporary data folder. A guard stops the service even
//! when a test fails, so no background process is left behind.

use std::process::{Command, Output};

use otaq_core::data_dir::DataDir;
use serde_json::Value;

struct Home {
    tmp: tempfile::TempDir,
}

impl Home {
    fn new() -> Self {
        Self {
            tmp: tempfile::tempdir().unwrap(),
        }
    }

    fn data(&self) -> DataDir {
        DataDir::new(self.tmp.path())
    }

    fn run(&self, args: &[&str]) -> Output {
        self.run_with(&[], args)
    }

    fn run_with(&self, env: &[(String, &str)], args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_otaq"))
            .args(args)
            .envs(env.iter().map(|(k, v)| (k, v)))
            .env(DataDir::env_var(), self.tmp.path())
            // The real service locates Claude Code's config file: never the user's (PRJ-06).
            .env("CLAUDE_CONFIG_DIR", self.tmp.path())
            // Nor any agent tool's: the service detects the tools on an empty test PATH, with a test home.
            .env("PATH", self.tmp.path().join("no-tools"))
            .env("HOME", self.tmp.path())
            .output()
            .unwrap()
    }

    fn json(&self, args: &[&str]) -> Value {
        self.json_with(&[], args)
    }

    fn json_with(&self, env: &[(String, &str)], args: &[&str]) -> Value {
        let out = self.run_with(env, args);
        assert!(out.status.success(), "{args:?}: {out:?}");
        serde_json::from_slice(&out.stdout).unwrap_or_else(|e| panic!("{e}: {out:?}"))
    }
}

impl Drop for Home {
    fn drop(&mut self) {
        let _ = self.run(&["daemon", "stop"]);
    }
}

fn pid_alive(pid: u64) -> bool {
    u32::try_from(pid).is_ok_and(otaq_core::process::alive)
}

#[test]
fn start_status_stop() {
    let home = Home::new();
    assert_eq!(
        home.json(&["daemon", "status", "--json"]),
        serde_json::json!({"running": false})
    );

    let started = home.json(&["daemon", "start", "--json"]);
    assert_eq!(started["running"], true);
    assert_eq!(started["already_running"], false);
    let pid = started["pid"].as_u64().unwrap();
    assert!(pid_alive(pid));

    let status = home.json(&["daemon", "status", "--json"]);
    assert_eq!(status["running"], true);
    assert_eq!(status["pid"], pid);
    assert_eq!(status["build_id"], otaq_core::protocol::build_id());
    assert_eq!(
        status["protocol_version"],
        otaq_core::protocol::PROTOCOL_VERSION
    );
    assert!(status["uptime_secs"].is_u64());

    let human = home.run(&["daemon", "status"]);
    let text = String::from_utf8_lossy(&human.stdout);
    assert!(
        text.contains("running") && text.contains(&format!("PID: {pid}")),
        "{text}"
    );

    // A second start finds the running service and exits 0.
    let again = home.json(&["daemon", "start", "--json"]);
    assert_eq!(again["already_running"], true);
    assert_eq!(again["pid"], pid);

    let stopped = home.json(&["daemon", "stop", "--json"]);
    assert_eq!(stopped["stopped"], true);
    let data = home.data();
    assert!(!data.socket_path().exists());
    assert!(!data.service_lock_path().exists());
    assert_eq!(home.json(&["daemon", "status", "--json"])["running"], false);
    wait_for_exit(pid);

    // Stopping again is fine.
    assert_eq!(home.json(&["daemon", "stop", "--json"])["stopped"], false);
}

fn wait_for_exit(pid: u64) {
    for _ in 0..100 {
        if !pid_alive(pid) {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    panic!("service process {pid} still running");
}

#[test]
fn start_replaces_a_stale_socket_and_lock() {
    let home = Home::new();
    let data = home.data();
    // A Windows pipe ends with its process, so only the lock file is left behind there.
    #[cfg(unix)]
    {
        drop(std::os::unix::net::UnixListener::bind(data.socket_path()).unwrap());
        assert!(data.socket_path().exists());
    }
    std::fs::write(data.service_lock_path(), "999999\n").unwrap();

    let started = home.json(&["daemon", "start", "--json"]);
    assert_eq!(started["already_running"], false);
    assert_eq!(home.json(&["daemon", "status", "--json"])["running"], true);
}

#[test]
fn silent_prints_only_the_pid() {
    let home = Home::new();
    let out = home.run(&["daemon", "start", "--silent"]);
    assert!(out.status.success());
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.trim().parse::<u32>().is_ok(), "{text:?}");
}

/// The program a running process runs: `/proc/<pid>/exe` on Linux, `ps` elsewhere.
#[cfg(unix)]
fn program_of(pid: u64) -> std::path::PathBuf {
    if cfg!(target_os = "linux") {
        return std::fs::read_link(format!("/proc/{pid}/exe")).unwrap();
    }
    let out = Command::new("ps")
        .args(["-o", "comm=", "-p", &pid.to_string()])
        .output()
        .unwrap();
    std::path::PathBuf::from(String::from_utf8_lossy(&out.stdout).trim())
}

/// Windows and Linux (T-0574, UPD-02; the debug seam makes any platform one): `daemon start` copies `otaq` and the
/// plugin into `runtime/<build>/` first, and the service runs from the copy, so the installed program is never in use.
/// macOS keeps the service in place (`0`, d0090).
#[cfg(unix)]
#[test]
fn the_service_runs_from_the_runtime_copy_where_the_platform_says() {
    let seam = otaq_core::runtime::service_from_copy_var();
    let home = Home::new();
    let data = home.data();
    let started = home.json_with(&[(seam.clone(), "1")], &["daemon", "start", "--json"]);
    let pid = started["pid"].as_u64().unwrap();
    let program = std::fs::canonicalize(program_of(pid)).unwrap();
    let runtime = std::fs::canonicalize(data.runtime_dir()).unwrap();
    assert!(program.starts_with(&runtime), "{program:?}");
    let folder = program.parent().unwrap();
    assert!(
        folder
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with(otaq_core::protocol::build_id())
    );
    assert!(
        folder
            .join("plugin/claude/.claude-plugin/plugin.json")
            .is_file()
    );
    assert_eq!(
        std::fs::read_to_string(folder.join("service.pid")).unwrap(),
        pid.to_string()
    );
    // Codex's fixed folder sits beside it.
    assert!(data.codex_plugin_dir().join(".codex-plugin").is_dir());
    home.json(&["daemon", "stop", "--json"]);
    wait_for_exit(pid);

    // Started the macOS way: the installed program runs, and agents still get the copy it made.
    let started = home.json_with(&[(seam, "0")], &["daemon", "start", "--json"]);
    let pid = started["pid"].as_u64().unwrap();
    assert_eq!(
        std::fs::canonicalize(program_of(pid)).unwrap(),
        std::fs::canonicalize(env!("CARGO_BIN_EXE_otaq")).unwrap()
    );
    assert_eq!(
        std::fs::read_to_string(folder.join("service.pid")).unwrap(),
        pid.to_string()
    );
}

/// T-0574's question QB: on Windows and Linux (the seam stands in on any platform) a service started through
/// `<data>/bin/otaq`, the copy the app keeps there with the plugin folder beside it, runs from a runtime copy made
/// from `bin/`, and that copy has the plugin, so agents can start; `bin/otaq` itself is never the service's program.
#[cfg(unix)]
#[test]
fn a_service_started_from_bin_otaq_runs_from_a_runtime_copy_with_the_plugin() {
    use otaq_core::shell_path::{BinKind, refresh_bin};
    let home = Home::new();
    let data = home.data();
    let own = std::path::Path::new(env!("CARGO_BIN_EXE_otaq"));
    let plugin = otaq_core::agent_tools::plugin_dir_next_to(own.parent().unwrap())
        .expect("the repository's plugin");
    refresh_bin(&data, own, BinKind::Copy, Some(&plugin)).unwrap();
    let bin_otaq = data.bin_dir().join(otaq_core::runtime::program_name());

    let out = Command::new(&bin_otaq)
        .args(["daemon", "start", "--json"])
        .env(DataDir::env_var(), data.root())
        .env(otaq_core::runtime::service_from_copy_var(), "1")
        .env("CLAUDE_CONFIG_DIR", data.root())
        .env("PATH", data.root().join("no-tools"))
        .env("HOME", data.root())
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    let started: Value = serde_json::from_slice(&out.stdout).unwrap();
    let pid = started["pid"].as_u64().unwrap();
    let program = std::fs::canonicalize(program_of(pid)).unwrap();
    let runtime = std::fs::canonicalize(data.runtime_dir()).unwrap();
    assert!(program.starts_with(&runtime), "{program:?}");
    assert!(
        program
            .parent()
            .unwrap()
            .join("plugin/claude/.claude-plugin/plugin.json")
            .is_file()
    );
    assert_ne!(program, std::fs::canonicalize(&bin_otaq).unwrap());
}
