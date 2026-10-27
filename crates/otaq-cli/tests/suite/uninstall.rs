//! `otaq uninstall` (T-0556, UNI-01…UNI-06; d0134) run inside a PTY against a service running in the test, with a
//! temporary `OTAQ_HOME`, `HOME` and repositories, the answers typed at its questions: no worktrees → one question;
//! worktrees → N changes nothing; y then N at the data folder changes nothing (nothing runs before both answers);
//! y, y removes the worktrees and the folder and keeps the branches; `--keep-data` keeps the database and the reports
//! and a later `otaq daemon start` opens them; the refusals (the app open, no terminal, `--json`) change nothing; and
//! with `OTAQ_HOME` unset under the test runner it refuses before anything.
//!
//! The sessions are `sh` sleeping; never a real agent tool. Unix only: a Windows pseudo console needs the cursor
//! handshake `attach`'s test does, and Windows is checked with its build.
#![cfg(unix)]

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use otaq_core::agent_tools::LaunchContext;
use otaq_core::agent_tools::claude::ClaudeLaunch;
use otaq_core::agent_tools::claude::trust::ClaudeConfig;
use otaq_core::client::{Client, ClientConfig};
use otaq_core::data_dir::DataDir;
use otaq_core::domain::Status;
use otaq_core::git::testing::{commit, git};
use otaq_core::naming;
use otaq_core::service::ServiceConfig;
use otaq_core::service::testing::{AgentShell, TestService};
use portable_pty::{CommandBuilder, PtySize, native_pty_system};
use tempfile::TempDir;

type Tree = Vec<(String, u64, Option<std::time::SystemTime>)>;

/// Every file and folder under `dir` with its size and modification time (symlinks not followed), without the
/// database and logs, which the service writes on its own.
fn tree(dir: &Path) -> Tree {
    let mut all = Vec::new();
    let mut stack = vec![dir.to_owned()];
    while let Some(at) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&at) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path().display().to_string();
            if path.contains("otaq.db") || path.contains("/logs") || path.ends_with("otaq.sock") {
                continue;
            }
            let meta = entry.path().symlink_metadata().unwrap();
            if meta.is_dir() {
                stack.push(entry.path());
            }
            all.push((path, meta.len(), meta.modified().ok()));
        }
    }
    all.sort();
    all
}

/// A service with shop-web (SHOP), Claude Code's trust recorded in a temporary config, and with `worktrees`:
/// SHOP-12 in Review (committed) and SHOP-14 in Dev with uncommitted edits and its agent at work.
struct World {
    w: TestService,
    repo: PathBuf,
    /// HOME for the command: the data folder sits under it, so it is shown as `~/h`.
    _claude: TempDir,
    agent: Option<AgentShell>,
}

impl World {
    async fn new(worktrees: bool) -> Self {
        let claude_dir = TempDir::new().unwrap();
        let claude = ClaudeConfig::locate(Some(claude_dir.path().into()), None).unwrap();
        let w = TestService::start_with(move |config| ServiceConfig {
            launch: Some(LaunchContext {
                bin_dir: "/nonexistent".into(),
                plugin_dir: "/nonexistent/plugin".into(),
                path: std::env::var_os("PATH").unwrap_or_default(),
                claude: ClaudeLaunch {
                    program: vec![
                        "sh".into(),
                        "-c".into(),
                        "exec sleep 60".into(),
                        "sh".into(),
                    ],
                    config_dir: None,
                },
            }),
            claude: Some(claude),
            ..config
        })
        .await;
        let shop = w.add_project("shop-web").await;
        let repo = PathBuf::from(&shop.repo_path);
        let mut agent = None;
        if worktrees {
            let mut tickets = Vec::new();
            for n in 1..=14 {
                tickets.push(w.create("SHOP", &format!("ticket {n}")).await);
            }
            let wt_12 = w.start_worktree(&tickets[11]).await;
            commit(&wt_12, "done.rs", "fn done() {}\n");
            w.force_status(&tickets[11], Status::WaitingForYou).await;
            let wt_14 = w.start_worktree(&tickets[13]).await;
            w.force_status(&tickets[13], Status::Working).await;
            std::fs::write(wt_14.join("new.rs"), "fn new() {}\n").unwrap();
            agent = Some(w.agent_shell(&tickets[13]).await);
        } else {
            git(&repo, &["branch", "otaq/OLD-1"]);
        }
        Self {
            w,
            repo,
            _claude: claude_dir,
            agent,
        }
    }

    fn root(&self) -> &Path {
        self.w.data.root()
    }

    fn home(&self) -> &Path {
        self.w.home.path()
    }

    /// The command's environment: this data folder, a temporary HOME and the agent tools' folders in it.
    fn env(&self) -> Vec<(String, PathBuf)> {
        let home = self.home();
        vec![
            (DataDir::env_var(), self.root().to_owned()),
            ("HOME".into(), home.to_owned()),
            ("CLAUDE_CONFIG_DIR".into(), home.join("claude-config")),
            ("CODEX_HOME".into(), home.join("codex")),
            ("COPILOT_HOME".into(), home.join("copilot")),
            ("XDG_CONFIG_HOME".into(), home.join("config")),
            ("XDG_DATA_HOME".into(), home.join("share")),
        ]
    }

    fn snapshot(&self) -> (Tree, Tree) {
        (tree(self.root()), tree(&self.repo))
    }

    /// `otaq uninstall <args>` in a PTY, typing `answers` one per question; its output and exit code.
    async fn run(&self, args: &[&str], answers: &[&str]) -> (String, u32) {
        let (env, args, answers) = (
            self.env(),
            args.iter().map(|a| a.to_string()).collect::<Vec<_>>(),
            answers.iter().map(|a| a.to_string()).collect::<Vec<_>>(),
        );
        tokio::task::spawn_blocking(move || in_pty(&env, &args, &answers))
            .await
            .unwrap()
    }

    fn worktrees(&self) -> Vec<String> {
        git(&self.repo, &["worktree", "list", "--porcelain"])
            .lines()
            .filter_map(|l| l.strip_prefix("worktree "))
            .map(str::to_owned)
            .collect()
    }
}

/// Runs the command in a PTY; answers each `[y/N]` with the next of `answers`.
fn in_pty(env: &[(String, PathBuf)], args: &[String], answers: &[String]) -> (String, u32) {
    let pty = native_pty_system()
        .openpty(PtySize {
            rows: 50,
            cols: 250,
            pixel_width: 0,
            pixel_height: 0,
        })
        .unwrap();
    let mut cmd = CommandBuilder::new(env!("CARGO_BIN_EXE_otaq"));
    cmd.arg("uninstall");
    cmd.args(args);
    for (name, value) in env {
        cmd.env(name, value);
    }
    cmd.env_remove(naming::ticket_env_var());
    let mut child = pty.slave.spawn_command(cmd).unwrap();
    drop(pty.slave);
    let mut reader = pty.master.try_clone_reader().unwrap();
    let mut writer = pty.master.take_writer().unwrap();
    let seen = Arc::new(Mutex::new(Vec::new()));
    let sink = seen.clone();
    std::thread::spawn(move || {
        let mut buf = [0u8; 4096];
        while let Ok(n) = reader.read(&mut buf) {
            if n == 0 {
                break;
            }
            sink.lock().unwrap().extend_from_slice(&buf[..n]);
        }
    });
    let text = || String::from_utf8_lossy(&seen.lock().unwrap()).into_owned();
    let deadline = Instant::now() + Duration::from_secs(60);
    let mut answered = 0;
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        let asked = text().matches("[y/N]").count();
        if asked > answered {
            let answer = answers
                .get(answered)
                .unwrap_or_else(|| panic!("an unexpected question:\n{}", text()));
            writer.write_all(format!("{answer}\n").as_bytes()).unwrap();
            answered += 1;
        }
        if Instant::now() > deadline {
            let _ = child.kill();
            panic!("uninstall didn't end:\n{}", text());
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    // What it printed last.
    std::thread::sleep(Duration::from_millis(200));
    let out = text().replace("\r\n", "\n");
    println!("{out}");
    (out, status.exit_code())
}

/// No worktrees: one question, then the folder is gone; the summary names the branches and the trust.
#[tokio::test(flavor = "multi_thread")]
async fn without_worktrees_one_question_and_the_folder_is_gone() {
    let world = World::new(false).await;
    let (out, code) = world.run(&[], &["y"]).await;
    assert_eq!(code, 0, "{out}");
    assert_eq!(out.matches("[y/N]").count(), 1, "{out}");
    assert!(
        out.contains("Delete ~/h (") && out.contains(": 1 project, 0 tickets)? [y/N]"),
        "{out}"
    );
    assert!(!world.root().exists());
    assert!(out.contains("  Claude Code: SHOP stays trusted"), "{out}");
    assert!(
        out.contains("  shop-web: 1 otaq/ branch kept · git branch --list 'otaq/*'"),
        "{out}"
    );
    assert!(out.contains("  ~/h ("), "{out}");
    assert_eq!(
        out.trim_end().lines().last(),
        Some("Drag Otaq.app to the Trash")
    );
    assert!(git(&world.repo, &["branch", "--list"]).contains("otaq/OLD-1"));
}

/// Worktrees, N: nothing changed, the agent still runs.
#[tokio::test(flavor = "multi_thread")]
async fn worktrees_answered_no_change_nothing() {
    let world = World::new(true).await;
    let before = world.snapshot();
    let (out, code) = world.run(&[], &["n"]).await;
    assert_eq!(code, 1, "{out}");
    assert!(
        out.contains("✎ SHOP-14 · shop-web · ") && out.contains("  SHOP-12 · shop-web · "),
        "{out}"
    );
    assert!(
        out.contains(
            "Delete these 2 worktrees? Uncommitted changes in the 1 marked ✎ are lost. Their branches are kept. \
             [y/N]"
        ),
        "{out}"
    );
    assert!(
        out.contains(
            "Nothing changed. Move them on or clear them first (Done, Close, the Cleanup view)"
        ),
        "{out}"
    );
    assert_eq!(out.matches("[y/N]").count(), 1);
    assert_eq!(world.snapshot(), before);
    let agent = world.agent.as_ref().unwrap();
    assert!(world.w.sessions.info(&agent.session).unwrap().is_running());
}

/// y, then Enter at the data folder: nothing ran before both answers.
#[tokio::test(flavor = "multi_thread")]
async fn no_at_the_data_folder_changes_nothing() {
    let world = World::new(true).await;
    let before = world.snapshot();
    let (out, code) = world.run(&[], &["y", ""]).await;
    assert_eq!(code, 1, "{out}");
    assert!(
        out.contains("Delete ~/h (")
            && out.contains(": 1 project, 14 tickets)? 1 agent ends. [y/N]"),
        "{out}"
    );
    assert!(out.trim_end().ends_with("Nothing changed."), "{out}");
    assert_eq!(world.snapshot(), before);
    assert_eq!(world.worktrees().len(), 3);
    let agent = world.agent.as_ref().unwrap();
    assert!(world.w.sessions.info(&agent.session).unwrap().is_running());
    assert!(!world.w.sessions.is_closed());
}

/// y, y: the worktrees and the folder are gone, the branches stay.
#[tokio::test(flavor = "multi_thread")]
async fn yes_yes_removes_the_worktrees_and_the_folder() {
    let world = World::new(true).await;
    let heads = |w: &World| {
        ["otaq/SHOP-12", "otaq/SHOP-14"]
            .map(|b| git(&w.repo, &["rev-parse", b]))
            .to_vec()
    };
    let before = heads(&world);
    let (out, code) = world.run(&[], &["Y", "yes"]).await;
    assert_eq!(code, 0, "{out}");
    assert!(!world.root().exists());
    assert_eq!(world.worktrees().len(), 1, "only the main checkout");
    assert_eq!(heads(&world), before);
    assert!(out.contains("Removed:\n  2 worktrees\n  ~/h ("), "{out}");
    assert!(out.contains("  Claude Code: SHOP stays trusted"), "{out}");
    assert!(
        out.contains("  shop-web: 2 otaq/ branches kept · git branch --list 'otaq/*'"),
        "{out}"
    );
    assert!(!out.contains("Failed:"), "{out}");
    assert_eq!(
        out.trim_end().lines().last(),
        Some("Drag Otaq.app to the Trash")
    );
}

/// Stops a real service started on `root` when the test ends, even when it fails.
struct Daemon<'a>(&'a [(String, PathBuf)]);

impl Daemon<'_> {
    fn run(&self, args: &[&str]) -> std::process::Output {
        let mut command = Command::new(env!("CARGO_BIN_EXE_otaq"));
        command.args(args);
        for (name, value) in self.0 {
            command.env(name, value);
        }
        // The service detects the tools on an empty PATH: no agent tool is found or run.
        command
            .env("PATH", "/nonexistent")
            .env_remove(naming::ticket_env_var())
            .output()
            .unwrap()
    }
}

impl Drop for Daemon<'_> {
    fn drop(&mut self) {
        let _ = self.run(&["daemon", "stop"]);
    }
}

/// `--keep-data`: only the database files, the `before-` copy (d0140) and the reports stay; a later `otaq daemon
/// start` opens the board.
#[tokio::test(flavor = "multi_thread")]
async fn keep_data_keeps_the_database_and_reports() {
    let world = World::new(true).await;
    let report = world.w.data.reports_dir().join("shop-web").join("note.txt");
    std::fs::create_dir_all(report.parent().unwrap()).unwrap();
    std::fs::write(&report, "kept\n").unwrap();
    let copy = world.root().join("otaq.db.before-0.0.9");
    std::fs::write(&copy, "the copy\n").unwrap();
    let (out, code) = world.run(&["--keep-data"], &["y", "y"]).await;
    assert_eq!(code, 0, "{out}");
    assert!(
        out.contains("Delete everything in ~/h but the database and comment reports ("),
        "{out}"
    );
    assert!(
        out.contains("  The database and comment reports in ~/h"),
        "{out}"
    );
    let mut left: Vec<String> = std::fs::read_dir(world.root())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    left.sort();
    for name in &left {
        assert!(
            [
                "otaq.db",
                "otaq.db-wal",
                "otaq.db-shm",
                "otaq.db.before-0.0.9",
                "reports"
            ]
            .contains(&name.as_str()),
            "{name} left: {left:?}"
        );
    }
    assert!(
        left.contains(&"otaq.db".to_owned()) && report.is_file() && copy.is_file(),
        "{left:?}"
    );
    assert_eq!(world.worktrees().len(), 1);

    // The board opens again from what was kept.
    let env = world.env();
    let daemon = Daemon(&env);
    let started = daemon.run(&["daemon", "start"]);
    assert!(started.status.success(), "{started:?}");
    let projects = daemon.run(&["project", "list", "--json"]);
    assert!(projects.status.success(), "{projects:?}");
    assert!(
        String::from_utf8_lossy(&projects.stdout).contains("\"SHOP\""),
        "{projects:?}"
    );
}

/// The app open, no terminal, `--json`: refused, nothing changed.
#[tokio::test(flavor = "multi_thread")]
async fn refusals_change_nothing() {
    let world = World::new(true).await;
    let before = world.snapshot();

    // No terminal: stdin isn't a TTY.
    let mut command = Command::new(env!("CARGO_BIN_EXE_otaq"));
    command.arg("uninstall").stdin(Stdio::null());
    for (name, value) in world.env() {
        command.env(name, value);
    }
    let piped = command.output().unwrap();
    assert_eq!(piped.status.code(), Some(1));
    assert_eq!(
        String::from_utf8_lossy(&piped.stderr).trim(),
        "error: Run otaq uninstall in a terminal"
    );
    assert_eq!(world.snapshot(), before);

    // --json, in a terminal.
    let (out, code) = world.run(&["--json"], &[]).await;
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("Run otaq uninstall in a terminal"), "{out}");
    assert!(out.contains("\"error\""), "{out}");
    assert_eq!(world.snapshot(), before);

    // The app open (its subscription).
    let app = Client::connect(ClientConfig {
        app: true,
        ..ClientConfig::new(&world.w.data)
    })
    .await
    .unwrap();
    let _events = app.subscribe().await.unwrap();
    let (out, code) = world.run(&[], &[]).await;
    assert_eq!(code, 1, "{out}");
    assert!(
        out.contains(
            "error: Quit Otaq from its menu bar / tray icon first: the app would start the service again"
        ),
        "{out}"
    );
    assert_eq!(world.snapshot(), before);
    let agent = world.agent.as_ref().unwrap();
    assert!(world.w.sessions.info(&agent.session).unwrap().is_running());
}

/// The suite's guard: under the test runner with `OTAQ_HOME` unset it refuses before anything, so no test can reach
/// a real `~/.otaq`.
#[test]
fn without_otaq_home_under_the_test_runner_it_refuses() {
    let home = TempDir::new().unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_otaq"))
        .arg("uninstall")
        .env_remove(DataDir::env_var())
        .env("HOME", home.path())
        .env("CARGO_MANIFEST_DIR", env!("CARGO_MANIFEST_DIR"))
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1), "{out:?}");
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("uninstall needs OTAQ_HOME set"),
        "{out:?}"
    );
    assert_eq!(
        std::fs::read_dir(home.path()).unwrap().count(),
        0,
        "nothing made in HOME"
    );
}
