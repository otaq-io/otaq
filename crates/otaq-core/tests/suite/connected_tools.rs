//! Connected tools (T-0348; ADP-10, SET-02, AGT-01, PRJ-08; d0086, d0085): detection with stand-in programs on a
//! test PATH, Hidden, the rechecks (start, `agent_tools.recheck`, after a Sign in… terminal, one tool after a failed
//! launch), what a tool becoming Connected does (setup once, its entry, its folder trust with the note once),
//! Disconnect, and the first-run rule "at least one agent tool connected".
//!
//! No real tool runs: every program is a stand-in shell script, and Claude Code's config is a temporary file.

use std::path::{Path, PathBuf};
use std::time::Duration;

use otaq_core::agent_tools::claude::trust::{ClaudeConfig, TRUST_FIELD, trust_key};
use otaq_core::agent_tools::connected::{AgentToolState, AgentToolsList, SetupAction, ToolStatus};
use otaq_core::agent_tools::{Tool, WindowsSource};
use otaq_core::client::{Client, ClientConfig, ClientError};
use otaq_core::protocol::{ErrorKind, Event, OnboardingState, Reply, Request};
use otaq_core::service::ServiceConfig;
use otaq_core::service::testing::TestService;
use serde_json::{Value, json};
use tempfile::TempDir;

/// A test PATH of stand-in programs, Claude Code's settings and config in a temporary folder.
struct Tools {
    dir: TempDir,
}

impl Tools {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("bin")).unwrap();
        std::fs::create_dir_all(dir.path().join("claude")).unwrap();
        Self { dir }
    }

    fn bin(&self) -> PathBuf {
        self.dir.path().join("bin")
    }

    fn stub(&self, name: &str, script: &str) {
        otaq_core::process::testing::stub_command(&self.bin(), name, script);
    }

    fn remove(&self, name: &str) {
        std::fs::remove_file(otaq_core::process::testing::stub_file(&self.bin(), name)).unwrap();
    }

    fn claude(&self) {
        self.stub("claude", r#"echo "2.1.4 (Claude Code)""#);
    }

    fn codex(&self) {
        self.stub("codex", r#"echo "codex-cli 0.157.1""#);
    }

    fn claude_config(&self) -> ClaudeConfig {
        ClaudeConfig::locate(Some(self.dir.path().join("claude").into()), None).unwrap()
    }

    /// The service's configuration: this PATH, these Claude Code settings and config file.
    fn with(&self) -> impl FnOnce(ServiceConfig) -> ServiceConfig + use<> {
        let (bin, root, claude) = (
            self.bin().into_os_string(),
            self.dir.path().to_path_buf(),
            self.claude_config(),
        );
        move |mut config| {
            config.tools.path = bin;
            config.tools.claude_config_dir = Some(root.join("claude"));
            config.tools.managed_settings_dir = root.join("managed");
            config.claude = Some(claude);
            config
        }
    }

    fn trusted(&self, repo_path: &str) -> Option<bool> {
        let config: Value =
            serde_json::from_slice(&std::fs::read(self.claude_config().file).unwrap()).unwrap();
        config["projects"][trust_key(Path::new(repo_path))][TRUST_FIELD].as_bool()
    }

    fn set_trusted(&self, repo_path: &str, trusted: bool) {
        let file = self.claude_config().file;
        let mut config: Value = serde_json::from_slice(&std::fs::read(&file).unwrap()).unwrap();
        config["projects"][trust_key(Path::new(repo_path))][TRUST_FIELD] = json!(trusted);
        std::fs::write(file, serde_json::to_string(&config).unwrap()).unwrap();
    }
}

async fn ask(client: &Client, method: &str, params: Value) -> Result<Reply, ClientError> {
    let mut request = json!({ "method": method });
    if !params.is_null() {
        request["params"] = params;
    }
    client
        .request(serde_json::from_value(request).unwrap())
        .await
}

fn list(reply: Reply) -> AgentToolsList {
    match reply {
        Reply::AgentTools(list) => *list,
        other => panic!("{other:?}"),
    }
}

/// `agent_tools.recheck` for every tool (it waits for the start's check), then the rows.
async fn recheck(client: &Client) -> AgentToolsList {
    list(ask(client, "agent_tools.recheck", json!({})).await.unwrap())
}

fn row(list: &AgentToolsList, tool: Tool) -> &ToolStatus {
    list.tools.iter().find(|t| t.tool == tool).unwrap()
}

fn connected(version: &str) -> AgentToolState {
    AgentToolState::Connected {
        version: Some(version.into()),
        account: None,
    }
}

async fn onboarding(client: &Client) -> OnboardingState {
    match client.request(Request::OnboardingState).await.unwrap() {
        Reply::OnboardingState(state) => state,
        other => panic!("{other:?}"),
    }
}

/// The default model for new agents' tool (AGM-03), with no model of its own.
async fn default_tool(w: &TestService) -> Option<Tool> {
    let model = w.board.store().default_model().await.unwrap()?;
    assert_eq!(model.model, None, "the tool's own default");
    Some(model.tool)
}

#[tokio::test(flavor = "multi_thread")]
async fn each_state_from_detect_and_hidden_wins() {
    let tools = Tools::new();
    tools.claude();
    tools.codex();
    // The Homebrew Copilot: too old to run (Q-G).
    tools.stub("copilot", r#"echo "GitHub Copilot CLI 0.0.404.""#);
    let w = TestService::start_with(tools.with()).await;
    let client = Client::connect(ClientConfig::new(&w.data)).await.unwrap();

    let found = recheck(&client).await;
    assert_eq!(
        row(&found, Tool::Claude).state,
        connected("Claude Code 2.1.4")
    );
    assert_eq!(row(&found, Tool::Codex).state, connected("0.157.1"));
    assert_eq!(
        row(&found, Tool::Copilot).state,
        AgentToolState::UpdateNeeded {
            version: Some("0.0.404".into())
        }
    );
    assert_eq!(
        row(&found, Tool::OpenCode).state,
        AgentToolState::NotInstalled
    );
    // What the page shows beside each state.
    let codex = row(&found, Tool::Codex);
    assert!(codex.checked_at.is_some());
    assert!(codex.install_command.is_some());
    assert!(codex.can_disconnect && !codex.can_sign_in);
    assert!(!row(&found, Tool::Copilot).can_disconnect);

    // Hidden wins over what was found, and stops the tool counting; a recheck keeps it hidden.
    let hidden = list(
        ask(
            &client,
            "agent_tools.set_hidden",
            json!({"tool": "codex", "hidden": true}),
        )
        .await
        .unwrap(),
    );
    assert_eq!(
        row(&hidden, Tool::Codex).state,
        AgentToolState::Hidden {
            version: Some("0.157.1".into())
        }
    );
    let again = recheck(&client).await;
    assert!(matches!(
        row(&again, Tool::Codex).state,
        AgentToolState::Hidden { .. }
    ));
    let counting: Vec<Tool> = again
        .tools
        .iter()
        .filter(|t| t.state.is_connected())
        .map(|t| t.tool)
        .collect();
    assert_eq!(counting, [Tool::Claude]);
    w.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_failed_launch_rechecks_only_its_tool() {
    let tools = Tools::new();
    tools.claude();
    tools.codex();
    let w = TestService::start_with(tools.with()).await;
    let client = Client::connect(ClientConfig::new(&w.data)).await.unwrap();
    let before = recheck(&client).await;
    assert!(row(&before, Tool::Codex).state.is_connected());

    // Codex went away; a launch failing because of it rechecks Codex alone (T-0357 calls the same method).
    tools.remove("codex");
    tools.remove("claude");
    let after = list(
        ask(&client, "agent_tools.recheck", json!({"tool": "codex"}))
            .await
            .unwrap(),
    );
    assert_eq!(row(&after, Tool::Codex).state, AgentToolState::NotInstalled);
    let claude = row(&after, Tool::Claude);
    assert!(claude.state.is_connected(), "not checked again");
    assert_eq!(claude.checked_at, row(&before, Tool::Claude).checked_at);
    w.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn sign_in_runs_the_login_in_a_terminal_then_rechecks() {
    let tools = Tools::new();
    // A stand-in whose `--version` answers only once `codex login` ran.
    let marker = tools.dir.path().join("signed-in");
    tools.stub(
        "codex",
        &format!(
            r#"case "$1" in --version) [ -f "{m}" ] && echo "codex-cli 0.157.1";; login) touch "{m}";; esac"#,
            m = marker.display()
        ),
    );
    let w = TestService::start_with(tools.with()).await;
    let client = Client::connect(ClientConfig::new(&w.data)).await.unwrap();
    assert_eq!(
        row(&recheck(&client).await, Tool::Codex).state,
        AgentToolState::NotInstalled
    );

    let Reply::ProjectTerminal(terminal) =
        ask(&client, "agent_tools.sign_in", json!({"tool": "codex"}))
            .await
            .unwrap()
    else {
        panic!("a terminal")
    };
    assert_eq!(terminal.name, "Codex sign in");
    tokio::time::timeout(Duration::from_secs(20), async {
        loop {
            let rows = list(ask(&client, "agent_tools.list", Value::Null).await.unwrap());
            if row(&rows, Tool::Codex).state.is_connected() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .expect("rechecked once the sign-in terminal ended");
    assert!(marker.exists());

    // A tool without its own sign-in command is refused.
    let Err(ClientError::Service(e)) =
        ask(&client, "agent_tools.sign_in", json!({"tool": "claude"})).await
    else {
        panic!("refused")
    };
    assert_eq!(e.kind, ErrorKind::Refused);
    w.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
/// AGM-02 (fresh install): the default model for new agents is the first connected tool with no model.
async fn the_first_connected_tool_is_the_default_model() {
    let tools = Tools::new();
    tools.codex();
    tools.claude();
    let w = TestService::start_with(tools.with()).await;
    let client = Client::connect(ClientConfig::new(&w.data)).await.unwrap();
    recheck(&client).await;
    assert_eq!(default_tool(&w).await, Some(Tool::Claude));
    w.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn connecting_a_tool_sets_it_up_once_and_records_its_trust() {
    let tools = Tools::new();
    tools.claude();
    let w = TestService::start_with(tools.with()).await;
    let client = Client::connect(ClientConfig::new(&w.data)).await.unwrap();
    let first = recheck(&client).await;
    let setup = row(&first, Tool::Claude).setup.clone().expect("set up");
    assert_eq!(setup.action, SetupAction::SetUp);
    assert_eq!(default_tool(&w).await, Some(Tool::Claude));

    // Two projects; Claude Code records their trust at Add project.
    let shop = w.add_project("shop-web").await;
    let bill = w.add_project("billing-api").await;
    assert_eq!(tools.trusted(&shop.repo_path), Some(true));

    // Checked again: still connected, so nothing runs again.
    let again = recheck(&client).await;
    assert_eq!(row(&again, Tool::Claude).setup.as_ref(), Some(&setup));

    // Codex connects later: the default model unchanged, and Claude Code's config untouched (trust for that
    // tool only).
    tools.set_trusted(&shop.repo_path, false);
    tools.codex();
    let with_codex = recheck(&client).await;
    assert_eq!(
        row(&with_codex, Tool::Codex)
            .setup
            .as_ref()
            .map(|s| s.action),
        Some(SetupAction::SetUp)
    );
    assert_eq!(default_tool(&w).await, Some(Tool::Claude));
    assert_eq!(tools.trusted(&shop.repo_path), Some(false));
    assert!(!onboarding(&client).await.trust_note);

    // Claude Code hidden, then shown again: it becomes Connected again, records the trust for every existing
    // project and raises the one-time note naming it.
    let hide = |hidden: bool| {
        ask(
            &client,
            "agent_tools.set_hidden",
            json!({"tool": "claude", "hidden": hidden}),
        )
    };
    hide(true).await.unwrap();
    hide(false).await.unwrap();
    assert_eq!(tools.trusted(&shop.repo_path), Some(true));
    assert_eq!(tools.trusted(&bill.repo_path), Some(true));
    let state = onboarding(&client).await;
    assert!(state.trust_note);
    assert_eq!(state.trust_note_tools, [Tool::Claude]);

    // Dismissed, then the same again: the trust is recorded, but the note is raised once per tool.
    assert_eq!(
        client.request(Request::DismissTrustNote).await.unwrap(),
        Reply::Ok
    );
    tools.set_trusted(&shop.repo_path, false);
    hide(true).await.unwrap();
    hide(false).await.unwrap();
    assert_eq!(tools.trusted(&shop.repo_path), Some(true));
    assert!(!onboarding(&client).await.trust_note);

    // A restart connects nothing again (the tools that counted are stored).
    drop(client);
    let w = w.restart_with(tools.with()).await;
    let client = Client::connect(ClientConfig::new(&w.data)).await.unwrap();
    let restarted = recheck(&client).await;
    assert!(row(&restarted, Tool::Codex).state.is_connected());
    assert_eq!(row(&restarted, Tool::Codex).setup, None);
    w.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn disconnect_removes_the_setup_and_hides_the_tool_unless_agents_run() {
    let tools = Tools::new();
    tools.claude();
    tools.codex();
    let w = TestService::start_with(tools.with()).await;
    let client = Client::connect(ClientConfig::new(&w.data)).await.unwrap();
    recheck(&client).await;
    let shop = w.add_project("shop-web").await;

    // Two Codex agents running.
    for id in ["s1", "s2"] {
        sqlx::query(
            "INSERT INTO sessions (id, project_id, kind, state, started_at, tool)
             VALUES (?, ?, 'agent', 'running', 1, 'codex')",
        )
        .bind(id)
        .bind(&shop.id)
        .execute(w.board.store().pool())
        .await
        .unwrap();
    }
    let Err(ClientError::Service(e)) =
        ask(&client, "agent_tools.disconnect", json!({"tool": "codex"})).await
    else {
        panic!("refused")
    };
    assert_eq!(e.message, "Codex has 2 agents running. End them first.");
    assert_eq!(e.data.unwrap()["refusal"], "agent_in_use");

    sqlx::query("UPDATE sessions SET ended_at = 2")
        .execute(w.board.store().pool())
        .await
        .unwrap();
    let rows = list(
        ask(&client, "agent_tools.disconnect", json!({"tool": "codex"}))
            .await
            .unwrap(),
    );
    let codex = row(&rows, Tool::Codex);
    assert!(matches!(codex.state, AgentToolState::Hidden { .. }));
    assert_eq!(
        codex.setup.as_ref().map(|s| s.action),
        Some(SetupAction::Removed)
    );
    // It stays hidden at the next check.
    assert!(matches!(
        row(&recheck(&client).await, Tool::Codex).state,
        AgentToolState::Hidden { .. }
    ));
    w.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn the_first_run_needs_one_connected_agent_tool() {
    let tools = Tools::new();
    tools.stub("git", r#"echo "git version 2.46.0""#);
    let w = TestService::start_with(tools.with()).await;
    let client = Client::connect(ClientConfig::new(&w.data)).await.unwrap();
    let check = || async {
        match client.request(Request::ToolsCheck).await.unwrap() {
            Reply::ToolsCheck(report) => report,
            other => panic!("{other:?}"),
        }
    };

    // No agent tool: blocked, and `claude` is no row of the checklist.
    let none = check().await;
    assert!(!none.agent_tool_connected());
    assert!(!none.required_found());
    assert!(
        none.tools
            .iter()
            .all(|t| t.tool != otaq_core::tools::Tool::Claude)
    );
    assert_eq!(none.agent_tools.len(), Tool::ALL.len());

    // Codex alone is enough: `claude` missing no longer blocks.
    tools.codex();
    let codex = check().await;
    assert!(codex.agent_tool_connected());
    assert!(codex.required_found());
    w.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn opencode_with_free_models_only_is_connected_and_still_offers_sign_in() {
    let tools = Tools::new();
    // No provider signed in: `opencode auth list` answers an empty list (Q-K, Q148).
    tools.stub(
        "opencode",
        r#"case "$1" in --version) echo "2.0.17";; auth) echo "[]";; esac"#,
    );
    let w = TestService::start_with(tools.with()).await;
    let client = Client::connect(ClientConfig::new(&w.data)).await.unwrap();
    let mut events = w.events.subscribe();
    let found = recheck(&client).await;
    let opencode = row(&found, Tool::OpenCode);
    assert_eq!(
        opencode.state,
        AgentToolState::Connected {
            version: Some("2.0.17".into()),
            account: Some("free models only".into())
        }
    );
    assert!(opencode.can_sign_in, "Sign in… still offered");
    assert!(opencode.can_disconnect);
    assert_eq!(opencode.limits, Some(WindowsSource::NotReported));
    // Claude Code isn't installed: no Sign in…; its limits come from its report.
    let claude = row(&found, Tool::Claude);
    assert!(!claude.can_sign_in);
    assert_eq!(claude.limits, Some(WindowsSource::Report));
    // The page hears each change.
    tokio::time::timeout(Duration::from_secs(5), async {
        while events.recv().await.unwrap() != Event::AgentToolsChanged {}
    })
    .await
    .expect("agent_tools_changed");

    // A provider signed in: Connected without Sign in….
    tools.stub(
        "opencode",
        r#"case "$1" in --version) echo "2.0.17";; auth) echo '[{"name":"Anthropic"}]';; esac"#,
    );
    let signed_in = recheck(&client).await;
    assert!(!row(&signed_in, Tool::OpenCode).can_sign_in);
    w.stop().await;
}
