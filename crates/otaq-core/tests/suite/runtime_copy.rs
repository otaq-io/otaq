//! The runtime copy (T-0574, UPD-02; d0140): with it on, the service copies the installed `otaq` and plugin into
//! `<data>/runtime/<build>/`, and agents and terminals get that folder: PATH's first entry, Claude Code's plugin
//! folder and status line. Replacing the installed `otaq` afterwards changes nothing for a running agent. The
//! installed `otaq` is a script printing its version; `claude` stands in as `sh`: no test starts claude or a real
//! `otaq`.

#![cfg(unix)]

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::common::stand_in;

use otaq_core::agent_tools::LaunchContext;
use otaq_core::agent_tools::claude::ClaudeLaunch;
use otaq_core::board::moves::DropTarget;
use otaq_core::domain::{Actor, Status};
use otaq_core::service::ServiceConfig;
use otaq_core::service::testing::{TestService, path_without_build_folders};
use otaq_core::session::SessionKind;
use tempfile::TempDir;

/// Prints its arguments and PATH's first folder, runs `otaq`, then reads commands typed into it.
const SCRIPT: &str =
    "printf 'ARGS %s\\n' \"$*\"; printf 'FIRST %s\\n' \"${PATH%%:*}\"; otaq; exec sh";

fn me() -> Actor {
    Actor::User("me".into())
}

/// Writes the installed app's `otaq`, printing `version` (an update or a rebuild replaces it).
fn write_otaq(bin: &Path, version: &str) {
    let program = bin.join("otaq");
    let _ = std::fs::remove_file(&program);
    std::fs::write(&program, format!("#!/bin/sh\necho otaq-{version}\n")).unwrap();
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
}

/// The runtime folders in the data folder (Codex's fixed folder left out).
fn runtime_folders(w: &TestService) -> Vec<PathBuf> {
    std::fs::read_dir(w.data.runtime_dir())
        .map(|entries| {
            entries
                .map(|e| e.unwrap().path())
                .filter(|p| *p != w.data.codex_plugin_dir())
                .collect()
        })
        .unwrap_or_default()
}

#[tokio::test(flavor = "multi_thread")]
async fn agents_and_terminals_run_the_runtime_copy_and_a_replaced_install_changes_nothing() {
    let installed = TempDir::new().unwrap();
    let bin = installed.path().join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    write_otaq(&bin, "v1");
    let plugin = stand_in::plugin(&bin);
    let claude_home = TempDir::new().unwrap();
    let w = {
        let (bin, claude_home) = (bin.clone(), claude_home.path().to_owned());
        TestService::start_with(move |mut c: ServiceConfig| {
            c.launch = Some(LaunchContext {
                bin_dir: bin,
                plugin_dir: plugin,
                path: path_without_build_folders().into(),
                claude: ClaudeLaunch {
                    program: vec!["sh".into(), "-c".into(), SCRIPT.into(), "sh".into()],
                    config_dir: None,
                },
            });
            c.tools.claude_config_dir = Some(claude_home);
            c.terminals.shell = "sh".into();
            c.terminals.shell_args = Vec::new();
            c.runtime_copy = true;
            c
        })
        .await
    };
    let folders = runtime_folders(&w);
    assert_eq!(folders.len(), 1, "{folders:?}");
    let runtime = otaq_core::paths::canonical(&folders[0]).unwrap();
    assert!(
        runtime
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with(otaq_core::protocol::build_id())
    );

    // SHOP-12's agent: `otaq` from the runtime folder, first on PATH; Claude Code's plugin from it.
    w.add_project("shop-web").await;
    let t = w.create("SHOP", "Invoice PDF export").await;
    let key = t.key.to_string();
    w.board
        .move_ticket(&key, DropTarget::ReadyForDev, true, &me())
        .await
        .unwrap();
    w.starter
        .start(&key, &me(), otaq_core::session::Start::Auto)
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(20), async {
        while w.status(&key).await != Status::Working {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("the agent works");
    let agent = w
        .sessions
        .running_of(&t.id, SessionKind::Agent)
        .expect("the agent runs")
        .id;
    let out = w.wait_for_output(&agent, "otaq-v1").await;
    let first = out
        .lines()
        .find_map(|l| l.trim_end().strip_prefix("FIRST "))
        .expect("PATH's first folder");
    assert_eq!(
        otaq_core::paths::canonical(first).unwrap(),
        runtime,
        "{out}"
    );
    let args = out.lines().find(|l| l.contains("ARGS ")).unwrap();
    assert!(
        args.contains(&format!(
            "--plugin-dir {}",
            folders[0].join("plugin/claude").display()
        )),
        "{args}"
    );
    let settings = args
        .split_whitespace()
        .skip_while(|a| *a != "--settings")
        .nth(1)
        .expect("the status line settings");
    let settings = std::fs::read_to_string(settings).unwrap();
    assert!(
        settings.contains(&folders[0].join("otaq").display().to_string()),
        "the status line runs the copy: {settings}"
    );

    // The installed app is replaced (an update): the running agent still runs the copy.
    write_otaq(&bin, "v2");
    w.sessions
        .write(&agent, b"otaq; echo done-$((1 + 1))\n")
        .unwrap();
    // The echoed input never says `done-2`: only the shell's output after `otaq` ran.
    let out = w.wait_for_output(&agent, "done-2").await;
    assert_eq!(out.matches("otaq-v1").count(), 2, "{out}");
    assert!(!out.contains("otaq-v2"), "{out}");

    // A Project terminal: the same folder first on PATH.
    let terminal = w.shells.open_project("SHOP", false).await.unwrap();
    w.sessions
        .write(&terminal.session.id, b"echo \"FIRST ${PATH%%:*}\"; otaq\n")
        .unwrap();
    let out = w.wait_for_output(&terminal.session.id, "otaq-v1").await;
    assert!(
        out.contains(&format!("FIRST {}", folders[0].display()))
            || out.contains(&format!("FIRST {}", runtime.display())),
        "{out}"
    );
}

/// Off unless the daemon (or a test) turns it on: no runtime folder.
#[tokio::test(flavor = "multi_thread")]
async fn off_by_default() {
    let w = TestService::start().await;
    assert!(runtime_folders(&w).is_empty());
}
