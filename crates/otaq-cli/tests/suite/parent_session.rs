//! A service started from inside an agent's session (a Claude Code terminal, or another tool's) inherits what that session puts on
//! its programs; the service drops it, so what it starts isn't taken for part of that session (Claude Code would
//! stop saving the conversations, and resuming a ticket would find nothing). Uses a temporary data folder and a
//! stand-in `codex` that writes down its environment; the service is stopped at the end even if the test fails.
#![cfg(unix)]

use std::os::unix::fs::PermissionsExt;
use std::process::Command;
use std::time::Duration;

use otaq_core::agent_tools::Tool;
use otaq_core::client::{Client, ClientConfig, CommandLauncher};
use otaq_core::data_dir::DataDir;
use otaq_core::protocol::{Reply, Request, ToolParams};

struct StopOnDrop(std::path::PathBuf);

impl Drop for StopOnDrop {
    fn drop(&mut self) {
        let _ = Command::new(env!("CARGO_BIN_EXE_otaq"))
            .args(["daemon", "stop"])
            .env(DataDir::env_var(), &self.0)
            .env("PATH", self.0.join("bin"))
            .output();
    }
}

#[tokio::test]
async fn what_the_service_starts_doesnt_inherit_an_agents_session() {
    let tmp = tempfile::tempdir().unwrap();
    let _guard = StopOnDrop(tmp.path().to_owned());
    let bin = tmp.path().join("bin");
    std::fs::create_dir(&bin).unwrap();
    let seen = tmp.path().join("seen-env");
    let codex = bin.join("codex");
    std::fs::write(
        &codex,
        format!(
            "#!/bin/sh\nenv > '{}.tmp'\nmv '{0}.tmp' '{0}'\n",
            seen.display()
        ),
    )
    .unwrap();
    std::fs::set_permissions(&codex, std::fs::Permissions::from_mode(0o755)).unwrap();

    let data = DataDir::new(tmp.path());
    let launcher = CommandLauncher::new(env!("CARGO_BIN_EXE_otaq"))
        .with_env(DataDir::env_var(), tmp.path())
        .with_env("CLAUDE_CONFIG_DIR", tmp.path())
        .with_env("PATH", &bin)
        .with_env("HOME", tmp.path())
        // What a Claude Code terminal puts on the programs it starts…
        .with_env("CLAUDECODE", "1")
        .with_env("CLAUDE_CODE_CHILD_SESSION", "1")
        .with_env(
            "CLAUDE_CODE_SESSION_ID",
            "12814b17-29cd-4b33-8945-943ca1b558f1",
        )
        .with_env("CLAUDE_CODE_MESSAGING_SOCKET", "/tmp/cc-socks/3158.sock")
        .with_env("CLAUDE_PID", "3158")
        .with_env("CLAUDE_EFFORT", "medium")
        // …and what the other tools' sessions do (one each)…
        .with_env("CODEX_THREAD_ID", "01a18333-0dd6-7000-8000-000000000000")
        .with_env(
            "COPILOT_AGENT_SESSION_ID",
            "ffecbb79-0000-4000-8000-000000000000",
        )
        .with_env("OPENCODE_TERMINAL", "1")
        // …and a variable of the user's own, which stays.
        .with_env("CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS", "1");
    let client = Client::connect_or_start(ClientConfig::new(&data), &launcher)
        .await
        .unwrap();

    // The sign-in terminal runs `codex login` from the service, as an agent's terminal runs its tool.
    let reply = client
        .request(Request::AgentToolsSignIn(ToolParams { tool: Tool::Codex }))
        .await
        .unwrap();
    assert!(matches!(reply, Reply::ProjectTerminal(_)), "{reply:?}");
    let env = tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            if let Ok(text) = std::fs::read_to_string(&seen) {
                return text;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .expect("the stand-in codex ran");

    let names: Vec<&str> = env
        .lines()
        .filter_map(|l| l.split_once('='))
        .map(|(n, _)| n)
        .collect();
    for dropped in [
        "CLAUDECODE",
        "CLAUDE_CODE_CHILD_SESSION",
        "CLAUDE_CODE_SESSION_ID",
        "CLAUDE_CODE_MESSAGING_SOCKET",
        "CLAUDE_PID",
        "CLAUDE_EFFORT",
        "CODEX_THREAD_ID",
        "COPILOT_AGENT_SESSION_ID",
        "OPENCODE_TERMINAL",
    ] {
        assert!(!names.contains(&dropped), "{dropped} reached codex:\n{env}");
    }
    assert!(
        names.contains(&"CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS"),
        "the user's own variable stays:\n{env}"
    );
}
