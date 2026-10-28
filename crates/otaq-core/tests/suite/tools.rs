//! `tools.check` through the service, with a fake PATH of stub scripts (ordinary shell scripts named git and gh;
//! no claude is ever run) and Claude Code settings from fixture files.

use std::path::Path;

use otaq_core::client::request_once;
use otaq_core::data_dir::DataDir;
use otaq_core::protocol::{Reply, Request};
use otaq_core::service::{Service, ServiceConfig};
use otaq_core::tools::{self, CheckEnv, PermissionVerdict, Tool, ToolStatus, ToolsReport};

fn stub(dir: &Path, name: &str, script: &str) {
    otaq_core::process::testing::stub_command(dir, name, script);
}

struct Fixture {
    _tmp: tempfile::TempDir,
    env: CheckEnv,
    bin: std::path::PathBuf,
    claude_dir: std::path::PathBuf,
}

fn fixture() -> Fixture {
    let tmp = tempfile::tempdir().unwrap();
    let bin = tmp.path().join("bin");
    let claude_dir = tmp.path().join("claude-config");
    std::fs::create_dir_all(&bin).unwrap();
    std::fs::create_dir_all(&claude_dir).unwrap();
    let env = CheckEnv {
        path: bin.clone().into_os_string(),
        claude_config_dir: Some(claude_dir.clone()),
        managed_settings_dir: tmp.path().join("managed"),
    };
    Fixture {
        _tmp: tmp,
        env,
        bin,
        claude_dir,
    }
}

fn row(report: &ToolsReport, tool: Tool) -> &ToolStatus {
    report.tools.iter().find(|t| t.tool == tool).unwrap()
}

#[tokio::test]
async fn no_agent_tool_gh_logged_in() {
    let f = fixture();
    stub(&f.bin, "git", r#"echo "git version 2.46.0""#);
    stub(
        &f.bin,
        "gh",
        r#"case "$1" in --version) echo "gh version 2.62.0 (2024-11-14)";; auth) exit 0;; esac"#,
    );
    std::fs::write(
        f.claude_dir.join("settings.json"),
        r#"{"permissions": {"defaultMode": "default"}}"#,
    )
    .unwrap();

    // Through the service, as the app asks for it.
    let tmp = tempfile::tempdir().unwrap();
    let data = DataDir::new(tmp.path().join("home"));
    let mut config = ServiceConfig::new(data.clone());
    config.tools = f.env.clone();
    let service = Service::bind(config).await.unwrap();
    let stopper = service.stopper();
    let task = tokio::spawn(service.serve());
    let Reply::ToolsCheck(report) = request_once(&data.socket_path(), Request::ToolsCheck)
        .await
        .unwrap()
    else {
        panic!()
    };
    stopper.stop();
    task.await.unwrap();

    let git = row(&report, Tool::Git);
    assert!(git.found && git.required);
    assert_eq!(git.version.as_deref(), Some("2.46.0"));
    assert_eq!(git.install_command, None);

    // `claude` is no row (SET-02 `[M23]`): with no agent tool connected, the report blocks.
    assert!(report.tools.iter().all(|t| t.tool != Tool::Claude));
    assert!(!report.agent_tool_connected());
    assert!(!report.required_found(), "no connected agent tool blocks");

    let gh = row(&report, Tool::Gh);
    assert!(gh.found && !gh.required);
    assert_eq!(gh.version.as_deref(), Some("2.62.0"));
    assert_eq!(gh.logged_in, Some(true));
    assert_eq!(gh.login_command, None);

    let az = row(&report, Tool::Az);
    assert!(!az.found && !az.required);
    assert_eq!(
        az.install_command.as_deref(),
        Some(Tool::Az.install_command())
    );

    assert_eq!(report.permission.verdict, PermissionVerdict::Prompting);
    assert_eq!(report.permission.mode.as_deref(), Some("default"));
}

#[tokio::test]
async fn logged_out_gh_and_az_without_the_extension() {
    let f = fixture();
    stub(&f.bin, "git", r#"echo "git version 2.46.0""#);
    stub(
        &f.bin,
        "gh",
        r#"case "$1" in --version) echo "gh version 2.62.0 (2024-11-14)";; auth) exit 1;; esac"#,
    );
    stub(
        &f.bin,
        "az",
        r#"case "$1" in version) echo '{"azure-cli": "2.67.0", "extensions": {}}';; account) exit 1;; esac"#,
    );
    let report = tools::check(&f.env).await;

    let gh = row(&report, Tool::Gh);
    assert_eq!(gh.logged_in, Some(false));
    assert_eq!(gh.login_command.as_deref(), Some("gh auth login"));

    let az = row(&report, Tool::Az);
    assert!(az.found);
    assert_eq!(az.version.as_deref(), Some("2.67.0"));
    assert_eq!(az.extension_installed, Some(false));
    assert_eq!(
        az.install_command.as_deref(),
        Some("az extension add --name azure-devops")
    );
    assert_eq!(az.logged_in, Some(false));

    // No settings file at all: unknown, so no warning.
    assert_eq!(report.permission.verdict, PermissionVerdict::Unknown);
}

#[tokio::test]
async fn az_with_the_extension_logged_in_and_auto_mode() {
    let f = fixture();
    stub(
        &f.bin,
        "az",
        r#"case "$1" in version) echo '{"azure-cli": "2.67.0", "extensions": {"azure-devops": "1.0.1"}}';; account) exit 0;; esac"#,
    );
    std::fs::write(
        f.claude_dir.join("settings.json"),
        r#"{"permissions": {"defaultMode": "auto", "allow": ["Bash(npm test)"]}}"#,
    )
    .unwrap();
    let report = tools::check(&f.env).await;
    let az = row(&report, Tool::Az);
    assert_eq!(az.extension_installed, Some(true));
    assert_eq!(az.logged_in, Some(true));
    assert_eq!(az.install_command, None);
    assert_eq!(report.permission.verdict, PermissionVerdict::NotPrompting);
}

#[tokio::test]
async fn unreadable_settings_give_unknown_and_managed_drop_ins_count() {
    let f = fixture();
    std::fs::write(f.claude_dir.join("settings.json"), "{ not json").unwrap();
    assert_eq!(
        tools::check(&f.env).await.permission.verdict,
        PermissionVerdict::Unknown
    );

    let drop_ins = f.env.managed_settings_dir.join("managed-settings.d");
    std::fs::create_dir_all(&drop_ins).unwrap();
    std::fs::write(
        drop_ins.join("10-policy.json"),
        r#"{"permissions": {"defaultMode": "acceptEdits"}}"#,
    )
    .unwrap();
    let check = tools::check(&f.env).await.permission;
    assert_eq!(check.verdict, PermissionVerdict::Prompting);
    assert!(check.source.unwrap().ends_with("10-policy.json"));
}

/// macOS and Linux need the executable bit; Windows needs an extension from `PATHEXT` (`git.cmd`).
#[test]
fn path_lookup_needs_an_executable() {
    let f = fixture();
    std::fs::write(f.bin.join("gh"), "not executable").unwrap();
    assert_eq!(tools::find_on_path("gh", f.env.path.as_os_str()), None);
    let git = otaq_core::process::testing::stub_command(&f.bin, "git", "true");
    assert_eq!(
        tools::find_on_path("git", f.env.path.as_os_str()),
        Some(git)
    );
}

/// A program started outside a terminal (a model list, Codex's app server, a one-off question) through an npm
/// launcher on Windows gets its arguments as written (d0088 point 2).
#[cfg(windows)]
#[test]
fn an_npm_tool_started_outside_a_terminal_gets_its_arguments_as_written() {
    use crate::common::npm_install;
    let dir = tempfile::tempdir().unwrap();
    let npm = npm_install::install(dir.path());
    let path = npm_install::path(&npm);
    let out = dir.path().join("argv.json");
    let status = tools::cmd_shim::command("codex", &path)
        .unwrap()
        .args([npm_install::MESSAGE, npm_install::MORE])
        .env("ARGV_OUT", &out)
        .status()
        .unwrap();
    assert!(status.success());
    assert_eq!(
        npm_install::arguments(&out),
        vec![npm_install::MESSAGE, npm_install::MORE]
    );
}
