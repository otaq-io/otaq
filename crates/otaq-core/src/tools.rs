//! The first-run tools checklist (SET-02, d0021, d0017) and the permission-mode warning (d0011, d0016).
//!
//! `git` is required, and **at least one agent tool connected** (SET-02 `[M23]`, d0086: Claude Code, Codex,
//! Copilot or OpenCode, from [`crate::agent_tools::connected::ConnectedTools`], which the service adds to the
//! report); `gh` and `az` (with the `azure-devops` extension) are optional and only matter for opening PRs. `claude`
//! is no longer a row: it is an agent tool like the others, detected through its adapter. Every tool is looked up on `PATH` and asked for its version, for display only:
//! Otaq never updates or version-gates a tool, and never installs one on its own (PLT-03, revised). A missing tool
//! comes with the command that installs it on the current OS, and a logged-out `gh` / `az` with its login command:
//! the user copies it, or clicks **Install** / **Log in**, which runs exactly that command in a visible terminal
//! ([`crate::terminals::tool`]) where they answer its prompts.
//!
//! The daemon searches the user's full `PATH` ([`daemon_path`]): an app opened from Finder passes on launchd's
//! short one, which misses `/opt/homebrew/bin` and `~/.local/bin`.
//!
//! The permission-mode check reads the user's Claude Code settings **read-only** and reports whether new sessions
//! will stop on permission prompts. Source: Claude Code docs, "Choose a permission mode → Which mode a session
//! starts in" and "Deploy managed settings" (code.claude.com/docs/en/permission-modes and /managed-settings,
//! read 2026-10-09):
//! - the starting mode is `permissions.defaultMode` from the settings files (managed settings win over the
//!   user's `~/.claude/settings.json`), else a built-in default that depends on the Claude Code version and plan;
//! - modes `default` (Manual; alias `manual`), `acceptEdits` and `plan` prompt; `auto`, `dontAsk` and
//!   `bypassPermissions` don't;
//! - `disableAutoMode: "disable"` in any settings file turns `auto` (and the built-in default) into Manual;
//! - managed settings live in `/Library/Application Support/ClaudeCode/` on macOS, `/etc/claude-code/` on Linux
//!   and `C:\Program Files\ClaudeCode\` on Windows (`managed-settings.json` plus `managed-settings.d/*.json`);
//!   `CLAUDE_CONFIG_DIR` moves `~/.claude`.
//!
//! Anything unreadable, an unknown mode, or no mode set at all (the built-in default can't be known without a
//! version check) gives [`PermissionVerdict::Unknown`], which shows no warning. Project settings are not read:
//! the check runs before any project exists. MDM profiles and server-managed settings are out of reach.

use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub mod cmd_shim;

/// How long one tool may take to answer (`az` is slow).
const COMMAND_TIMEOUT: Duration = Duration::from_secs(15);
/// How long the login shell may take to print its `PATH` for the app's own checks ([`login_path`]).
const LOGIN_PATH_TIMEOUT: Duration = Duration::from_secs(5);
/// How long the service waits for the login shell's `PATH` ([`daemon_path`]). It waits once, before its runtime
/// starts, and a profile that loads nvm, conda or similar can take 6–7 s (OTQ-10), so it waits much longer than
/// [`LOGIN_PATH_TIMEOUT`].
pub const DAEMON_LOGIN_PATH_TIMEOUT: Duration = Duration::from_secs(30);
/// Printed before the login shell's `PATH`, so whatever the profile prints first is skipped.
const PATH_MARKER: &str = "__login_path__";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Tool {
    Git,
    Claude,
    Gh,
    Az,
}

impl Tool {
    /// The checklist's rows. [`Tool::Claude`] isn't one: its adapter's detect checks it ([`check_one`]).
    pub const ALL: [Tool; 3] = [Tool::Git, Tool::Gh, Tool::Az];

    /// The executable's name.
    pub fn command(self) -> &'static str {
        match self {
            Self::Git => "git",
            Self::Claude => "claude",
            Self::Gh => "gh",
            Self::Az => "az",
        }
    }

    pub fn required(self) -> bool {
        matches!(self, Self::Git)
    }

    /// The command that installs the tool on this OS, shown for copying (SET-02) and run by **Install**.
    pub fn install_command(self) -> &'static str {
        self.install_command_on(Os::current())
    }

    /// The install command on `os`: Homebrew on macOS (git too: Apple's own installer only opens a window),
    /// winget on Windows (frame *14 → gh and az missing · non-blocking (Windows)*), apt on Linux (Debian and
    /// Ubuntu; a hint, other distributions use their own). Claude Code uses its native installer
    /// (code.claude.com/docs/en/setup, read 2026-10-17): no Node needed. On Windows each command runs in PowerShell,
    /// which may have no `&&`, so az's extension is a second step (Install again once az is there).
    pub fn install_command_on(self, os: Os) -> &'static str {
        match (os, self) {
            (Os::Windows, Self::Claude) => "irm https://claude.ai/install.ps1 | iex",
            (_, Self::Claude) => "curl -fsSL https://claude.ai/install.sh | bash",
            (Os::Mac, Self::Git) => "brew install git",
            (Os::Mac, Self::Gh) => "brew install gh",
            (Os::Mac, Self::Az) => "brew install azure-cli && az extension add --name azure-devops",
            (Os::Windows, Self::Git) => "winget install Git.Git",
            (Os::Windows, Self::Gh) => "winget install GitHub.cli",
            (Os::Windows, Self::Az) => "winget install Microsoft.AzureCLI",
            (Os::Linux, Self::Git) => "sudo apt install git",
            (Os::Linux, Self::Gh) => "sudo apt install gh",
            (Os::Linux, Self::Az) => {
                "curl -sL https://aka.ms/InstallAzureCLIDeb | sudo bash && az extension add --name azure-devops"
            }
        }
    }

    /// The command that logs the tool in (`gh` and `az`; `git` and `claude` have none here).
    pub fn login_command(self) -> Option<&'static str> {
        match self {
            Self::Gh => Some("gh auth login"),
            Self::Az => Some("az login"),
            Self::Git | Self::Claude => None,
        }
    }
}

/// The program an install command starts with when the user may not have it: Homebrew on macOS, `npm` for the
/// agent tools installed with it. `None` for a self-contained command (`curl … | bash`, `winget`, `apt`).
pub fn package_manager_of(command: &str) -> Option<&'static str> {
    match command.split_whitespace().next()? {
        "brew" => Some("brew"),
        "npm" => Some("npm"),
        _ => None,
    }
}

/// What a checklist terminal runs for a tool: its install command or its login command.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolAction {
    Install,
    Login,
}

/// The operating systems with their own install commands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Os {
    Mac,
    Windows,
    Linux,
}

impl Os {
    pub fn current() -> Self {
        if cfg!(target_os = "macos") {
            Self::Mac
        } else if cfg!(windows) {
            Self::Windows
        } else {
            Self::Linux
        }
    }
}

/// One row of the checklist.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolStatus {
    pub tool: Tool,
    pub required: bool,
    pub found: bool,
    /// The version as the frame shows it (`2.46.0`, `Claude Code 2.1.4`); display only.
    pub version: Option<String>,
    /// gh and az only: whether `gh auth status` / `az account show` succeed.
    pub logged_in: Option<bool>,
    /// az only: whether the `azure-devops` extension is installed.
    pub extension_installed: Option<bool>,
    /// What to run to fix what is missing (the tool, or az's extension); `None` when nothing is missing.
    pub install_command: Option<String>,
    /// What to run to log in, when the tool is found but not logged in.
    pub login_command: Option<String>,
    /// The package manager `install_command` needs and `PATH` lacks (`brew`): Install isn't offered.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub missing_package_manager: Option<String>,
}

impl ToolStatus {
    /// The command `action` runs for this row now, if it applies: Install while something is missing and its
    /// package manager is there, Log in while logged out.
    pub fn command_for(&self, action: ToolAction) -> Option<&str> {
        match action {
            ToolAction::Install if self.missing_package_manager.is_none() => {
                self.install_command.as_deref()
            }
            ToolAction::Install => None,
            ToolAction::Login => self.login_command.as_deref(),
        }
    }
}

/// Whether new Claude Code sessions will stop on permission prompts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionVerdict {
    Prompting,
    NotPrompting,
    /// Unreadable or undecidable: no warning.
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PermissionCheck {
    pub verdict: PermissionVerdict,
    /// The `permissions.defaultMode` that decided it, if one is set.
    pub mode: Option<String>,
    /// The settings file it came from.
    pub source: Option<String>,
}

/// The answer to `tools.check`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolsReport {
    pub tools: Vec<ToolStatus>,
    /// Claude Code's permission mode, as the checklist evaluated it (d0011, d0016).
    pub permission: PermissionCheck,
    /// Every agent tool's row, checked again for the report, each with its own permission warning (SET-02,
    /// d0086). Empty from [`check`] alone; the service fills it.
    #[serde(default)]
    pub agent_tools: Vec<crate::agent_tools::connected::ToolStatus>,
}

impl ToolsReport {
    /// Whether onboarding can go on: every required tool found and at least one agent tool connected (SET-02).
    pub fn required_found(&self) -> bool {
        self.tools.iter().all(|t| t.found || !t.required) && self.agent_tool_connected()
    }

    /// At least one agent tool is connected (d0086: "at least one connected tool instead of Claude Code").
    pub fn agent_tool_connected(&self) -> bool {
        self.agent_tools.iter().any(|t| t.state.is_connected())
    }
}

/// Where to look. [`CheckEnv::from_process`] uses this process's environment; tests point it at fixtures.
#[derive(Debug, Clone)]
pub struct CheckEnv {
    /// The `PATH` to search, also passed to the tools.
    pub path: OsString,
    /// The user's Claude Code folder (`$CLAUDE_CONFIG_DIR` or `~/.claude`).
    pub claude_config_dir: Option<PathBuf>,
    /// The managed settings folder.
    pub managed_settings_dir: PathBuf,
}

impl CheckEnv {
    pub fn from_process() -> Self {
        let claude_config_dir = std::env::var_os("CLAUDE_CONFIG_DIR")
            .filter(|d| !d.is_empty())
            .map(PathBuf::from)
            .or_else(|| std::env::home_dir().map(|h| h.join(".claude")));
        Self {
            path: std::env::var_os("PATH").unwrap_or_default(),
            claude_config_dir,
            managed_settings_dir: managed_settings_dir(),
        }
    }
}

/// The `PATH` the daemon runs with, and why the login shell's part is missing when it is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DaemonPath {
    pub path: OsString,
    /// Set when the login shell gave no `PATH`, so the service runs with the fallback folders only.
    pub warning: Option<String>,
}

impl DaemonPath {
    /// Logs [`DaemonPath::warning`], if any. The daemon calls it once its log is open.
    pub fn log_warning(&self) {
        if let Some(warning) = &self.warning {
            tracing::warn!(path = %self.path.to_string_lossy(), "{warning}");
        }
    }
}

/// The `PATH` the daemon runs with: the user's login shell's (`.zprofile` and `.zshrc` both read), then the folders
/// installers write to (`~/.local/bin` for Claude Code's native installer, Homebrew's), then `inherited`. Missing
/// folders are harmless. Unchanged on Windows, where apps get the user's full `PATH`. Blocking, for up to `timeout`
/// ([`DAEMON_LOGIN_PATH_TIMEOUT`]): the daemon calls it once, before its runtime starts.
pub fn daemon_path(
    inherited: &OsStr,
    shell: Option<&Path>,
    home: Option<&Path>,
    timeout: Duration,
) -> DaemonPath {
    if cfg!(windows) {
        return DaemonPath {
            path: inherited.to_owned(),
            warning: None,
        };
    }
    let (login, warning) = match shell {
        None => (
            OsString::new(),
            Some(
                "no login shell (SHELL is not set): the service runs with the fallback PATH only"
                    .to_owned(),
            ),
        ),
        Some(shell) => match login_path_within(shell, timeout) {
            Ok(path) => (path, None),
            Err(failure) => (
                OsString::new(),
                Some(format!(
                    "couldn't read the login shell's PATH ({}): {failure}; the service runs with the fallback \
                     PATH only, so tools the shell profile adds (nvm's npm, for example) aren't found",
                    shell.display()
                )),
            ),
        },
    };
    let mut known: Vec<PathBuf> = Vec::new();
    if let Some(home) = home {
        known.push(home.join(".local").join("bin"));
    }
    known.extend(["/opt/homebrew/bin", "/usr/local/bin"].map(PathBuf::from));
    let known = std::env::join_paths(known).unwrap_or_default();
    DaemonPath {
        path: merge_paths(&merge_paths(&login, &known), inherited),
        warning,
    }
}

/// Why the login shell gave no `PATH`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoginPathFailure {
    /// The shell couldn't be started.
    Spawn(String),
    /// It didn't finish within the limit and was stopped.
    TimedOut(Duration),
    /// It finished without printing a `PATH`; `status` is its exit status.
    NoPath { status: String, elapsed: Duration },
}

impl std::fmt::Display for LoginPathFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Spawn(e) => write!(f, "it couldn't be started: {e}"),
            Self::TimedOut(limit) => write!(
                f,
                "it didn't finish within {} s and was stopped",
                limit.as_secs_f32()
            ),
            Self::NoPath { status, elapsed } => write!(
                f,
                "it printed no PATH ({status}) after {:.1} s",
                elapsed.as_secs_f32()
            ),
        }
    }
}

/// The `PATH` an interactive login shell ends up with; `None` when it fails or takes longer than
/// [`LOGIN_PATH_TIMEOUT`].
pub fn login_path(shell: &Path) -> Option<OsString> {
    login_path_within(shell, LOGIN_PATH_TIMEOUT).ok()
}

/// The `PATH` an interactive login shell ends up with, waiting at most `timeout`. `printenv` prints it
/// colon-separated whatever the shell (fish keeps `$PATH` as a list). Its output is read on a thread, so a profile
/// that prints a lot can't block on a full pipe.
pub fn login_path_within(shell: &Path, timeout: Duration) -> Result<OsString, LoginPathFailure> {
    use std::io::Read;
    let mut child = std::process::Command::new(shell)
        .args([
            "-l",
            "-i",
            "-c",
            &format!("printf '{PATH_MARKER}'; printenv PATH"),
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| LoginPathFailure::Spawn(e.to_string()))?;
    let mut stdout = child.stdout.take();
    let reader = std::thread::spawn(move || {
        let mut out = Vec::new();
        if let Some(stdout) = stdout.as_mut() {
            let _ = stdout.read_to_end(&mut out);
        }
        out
    });
    let started = std::time::Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) if started.elapsed() > timeout => {
                let _ = child.kill();
                let _ = child.wait();
                // A background child of the profile may hold the pipe open: don't wait for the reader.
                return Err(LoginPathFailure::TimedOut(timeout));
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(20)),
            Err(_) => break None,
        }
    };
    let elapsed = started.elapsed();
    let output = reader.join().unwrap_or_default();
    parse_login_path(&String::from_utf8_lossy(&output))
        .map(OsString::from)
        .ok_or_else(|| LoginPathFailure::NoPath {
            status: status.map_or_else(|| "exit status unknown".to_owned(), |s| s.to_string()),
            elapsed,
        })
}

/// The `PATH` after the marker, skipping whatever the profile printed before it.
pub fn parse_login_path(output: &str) -> Option<String> {
    let path = output.rsplit_once(PATH_MARKER)?.1.trim();
    (!path.is_empty()).then(|| path.to_owned())
}

/// `first`'s folders, then those of `rest` it doesn't have, in order.
pub fn merge_paths(first: &OsStr, rest: &OsStr) -> OsString {
    let mut parts: Vec<PathBuf> = Vec::new();
    for dir in std::env::split_paths(first).chain(std::env::split_paths(rest)) {
        if !dir.as_os_str().is_empty() && !parts.contains(&dir) {
            parts.push(dir);
        }
    }
    std::env::join_paths(parts).unwrap_or_else(|_| first.to_owned())
}

/// Runs the checklist's rows (git, gh, az). Only ever reads: runs `--version`, `auth status`, `account show` and
/// reads settings. The agent tools' rows are the service's ([`ToolsReport::agent_tools`]).
pub async fn check(env: &CheckEnv) -> ToolsReport {
    let (git, gh, az) = tokio::join!(
        check_tool(Tool::Git, env),
        check_tool(Tool::Gh, env),
        check_tool(Tool::Az, env),
    );
    ToolsReport {
        tools: vec![git, gh, az],
        permission: check_permission_mode(env),
        agent_tools: Vec::new(),
    }
}

/// Claude Code's managed settings folder on this OS (its docs, "Deploy managed settings").
fn managed_settings_dir() -> PathBuf {
    if cfg!(windows) {
        PathBuf::from(r"C:\Program Files\ClaudeCode")
    } else if cfg!(target_os = "linux") {
        PathBuf::from("/etc/claude-code")
    } else {
        PathBuf::from("/Library/Application Support/ClaudeCode")
    }
}

/// Finds `name` in the directories of `path`, like the shell would. On Windows a name without an extension
/// also matches `name` plus each extension of `PATHEXT` (`gh.exe`, `az.cmd`).
pub fn find_on_path(name: &str, path: &OsStr) -> Option<PathBuf> {
    let names = candidate_names(name);
    std::env::split_paths(path)
        .flat_map(|dir| names.iter().map(move |n| dir.join(n)))
        .find(|candidate| is_executable(candidate))
}

fn candidate_names(name: &str) -> Vec<String> {
    if !cfg!(windows) || Path::new(name).extension().is_some() {
        return vec![name.to_owned()];
    }
    let pathext = std::env::var("PATHEXT").unwrap_or_else(|_| ".COM;.EXE;.BAT;.CMD".to_owned());
    pathext
        .split(';')
        .filter(|ext| !ext.is_empty())
        .map(|ext| format!("{name}{}", ext.to_ascii_lowercase()))
        .collect()
}

#[cfg(unix)]
fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
}

/// Windows has no executable bit: the extension (from `PATHEXT`) decides.
#[cfg(windows)]
fn is_executable(path: &Path) -> bool {
    path.is_file()
}

struct Run {
    success: bool,
    stdout: String,
}

/// Runs a tool with no input and a time limit; `None` if it couldn't run or timed out.
async fn run(program: &Path, args: &[&str], env: &CheckEnv) -> Option<Run> {
    let mut command = tokio::process::Command::new(program);
    command
        .args(args)
        .env("PATH", &env.path)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    let output = tokio::time::timeout(COMMAND_TIMEOUT, command.output())
        .await
        .ok()?
        .ok()?;
    Some(Run {
        success: output.status.success(),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
    })
}

/// Checks one tool, as the whole check would (`--version`, and the login for `gh` / `az`).
pub async fn check_one(tool: Tool, env: &CheckEnv) -> ToolStatus {
    check_tool(tool, env).await
}

async fn check_tool(tool: Tool, env: &CheckEnv) -> ToolStatus {
    let mut status = ToolStatus {
        tool,
        required: tool.required(),
        found: false,
        version: None,
        logged_in: None,
        extension_installed: None,
        install_command: Some(tool.install_command().to_owned()),
        login_command: None,
        missing_package_manager: None,
    };
    let Some(program) = find_on_path(tool.command(), &env.path) else {
        status.missing_package_manager = package_manager_of(tool.install_command())
            .filter(|manager| find_on_path(manager, &env.path).is_none())
            .map(str::to_owned);
        return status;
    };
    status.found = true;
    status.install_command = None;
    match tool {
        Tool::Git | Tool::Claude | Tool::Gh => {
            if let Some(out) = run(&program, &["--version"], env).await {
                status.version = parse_version(tool, &out.stdout);
            }
        }
        Tool::Az => {
            let versions = run(&program, &["version", "--output", "json"], env).await;
            let parsed: Option<Value> = versions.and_then(|o| serde_json::from_str(&o.stdout).ok());
            status.version = parsed
                .as_ref()
                .and_then(|v| v.get("azure-cli")?.as_str().map(str::to_owned));
            let extension = parsed
                .as_ref()
                .and_then(|v| v.get("extensions")?.get("azure-devops"))
                .is_some();
            status.extension_installed = Some(extension);
            if !extension {
                status.install_command = Some("az extension add --name azure-devops".to_owned());
            }
        }
    }
    let status_args: Option<&[&str]> = match tool {
        Tool::Gh => Some(&["auth", "status"]),
        Tool::Az => Some(&["account", "show"]),
        Tool::Git | Tool::Claude => None,
    };
    if let (Some(args), Some(login_command)) = (status_args, tool.login_command()) {
        let logged_in = run(&program, args, env).await.is_some_and(|o| o.success);
        status.logged_in = Some(logged_in);
        if !logged_in {
            status.login_command = Some(login_command.to_owned());
        }
    }
    status
}

/// The version as shown in the checklist, from the tool's `--version` output.
pub fn parse_version(tool: Tool, output: &str) -> Option<String> {
    let first = output.lines().next()?.trim();
    let token = match tool {
        // "git version 2.46.0"
        Tool::Git => first
            .strip_prefix("git version ")?
            .split_whitespace()
            .next()?,
        // "2.1.4 (Claude Code)"
        Tool::Claude => return Some(format!("Claude Code {}", first.split_whitespace().next()?)),
        // "gh version 2.62.0 (2024-11-14)"
        Tool::Gh => first
            .strip_prefix("gh version ")?
            .split_whitespace()
            .next()?,
        Tool::Az => first.split_whitespace().last()?,
    };
    Some(token.to_owned())
}

/// A settings file as read for the check.
#[derive(Debug, Clone, PartialEq)]
pub enum SettingsFile {
    Missing,
    Unreadable,
    Parsed(Value),
}

impl SettingsFile {
    pub fn read(path: &Path) -> Self {
        match std::fs::read_to_string(path) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Self::Missing,
            Err(_) => Self::Unreadable,
            Ok(text) => match serde_json::from_str::<Value>(&text) {
                Ok(value) if value.is_object() => Self::Parsed(value),
                _ => Self::Unreadable,
            },
        }
    }

    fn value(&self) -> Option<&Value> {
        match self {
            Self::Parsed(v) => Some(v),
            _ => None,
        }
    }
}

/// Whether new Claude Code sessions will stop on permission prompts, from its settings files (read-only).
pub fn check_permission_mode(env: &CheckEnv) -> PermissionCheck {
    let mut managed = vec![env.managed_settings_dir.join("managed-settings.json")];
    if let Ok(entries) = std::fs::read_dir(env.managed_settings_dir.join("managed-settings.d")) {
        let mut drop_ins: Vec<PathBuf> = entries
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().is_some_and(|x| x == "json"))
            .collect();
        drop_ins.sort();
        managed.extend(drop_ins);
    }
    let mut files: Vec<(String, SettingsFile)> = managed
        .into_iter()
        .map(|p| (p.display().to_string(), SettingsFile::read(&p)))
        .collect();
    if let Some(dir) = &env.claude_config_dir {
        let user = dir.join("settings.json");
        files.push((user.display().to_string(), SettingsFile::read(&user)));
    }
    evaluate_permission_mode(&files)
}

/// Decides the verdict from settings files in precedence order (highest first).
pub fn evaluate_permission_mode(files: &[(String, SettingsFile)]) -> PermissionCheck {
    let unknown = |mode: Option<String>, source: Option<String>| PermissionCheck {
        verdict: PermissionVerdict::Unknown,
        mode,
        source,
    };
    let mut decided = None;
    for (source, file) in files {
        match file {
            // An unreadable file could hold the deciding value.
            SettingsFile::Unreadable if decided.is_none() => {
                return unknown(None, Some(source.clone()));
            }
            _ => {}
        }
        let mode = file
            .value()
            .and_then(|v| v.get("permissions")?.get("defaultMode")?.as_str());
        if let (None, Some(mode)) = (&decided, mode) {
            decided = Some((mode.to_owned(), source.clone()));
        }
    }
    let auto_disabled = files.iter().any(|(_, f)| {
        f.value()
            .and_then(|v| v.get("disableAutoMode")?.as_str())
            .is_some_and(|v| v == "disable")
    });
    let Some((mode, source)) = decided else {
        // No mode set: the built-in default depends on the claude version and plan, except that disabling auto
        // mode makes it Manual.
        return if auto_disabled {
            PermissionCheck {
                verdict: PermissionVerdict::Prompting,
                mode: None,
                source: None,
            }
        } else {
            unknown(None, None)
        };
    };
    let verdict = match mode.as_str() {
        "default" | "manual" | "acceptEdits" | "plan" => PermissionVerdict::Prompting,
        "auto" if auto_disabled => PermissionVerdict::Prompting,
        "auto" | "dontAsk" | "bypassPermissions" => PermissionVerdict::NotPrompting,
        _ => PermissionVerdict::Unknown,
    };
    PermissionCheck {
        verdict,
        mode: Some(mode),
        source: Some(source),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    /// A fake login shell: a script that ignores its arguments, sleeps `sleep` seconds and prints `printed`.
    #[cfg(unix)]
    fn fake_shell(dir: &Path, sleep: &str, printed: &str) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let shell = dir.join("fake-shell");
        std::fs::write(
            &shell,
            format!("#!/bin/sh\nsleep {sleep}\nprintf '%s' '{printed}'\n"),
        )
        .unwrap();
        std::fs::set_permissions(&shell, std::fs::Permissions::from_mode(0o755)).unwrap();
        shell
    }

    /// OTQ-10: a login shell slower than the old 5 s limit (but under the service's) still gives its PATH.
    #[cfg(unix)]
    #[test]
    fn a_slow_login_shell_still_gives_its_path() {
        let dir = tempfile::tempdir().unwrap();
        let shell = fake_shell(
            dir.path(),
            "6",
            &format!("{PATH_MARKER}/Users/me/.nvm/versions/node/v20.20.0/bin:/usr/bin"),
        );
        let path = daemon_path(
            OsStr::new("/usr/bin:/bin"),
            Some(&shell),
            Some(Path::new("/Users/me")),
            DAEMON_LOGIN_PATH_TIMEOUT,
        );
        assert_eq!(path.warning, None);
        assert_eq!(
            path.path,
            OsString::from(
                "/Users/me/.nvm/versions/node/v20.20.0/bin:/usr/bin:/Users/me/.local/bin:/opt/homebrew/bin:\
                 /usr/local/bin:/bin"
            )
        );
    }

    /// OTQ-10: a login shell that never finishes is stopped; the service gets the fallback PATH and logs why.
    #[cfg(unix)]
    #[test]
    fn a_login_shell_that_never_finishes_gives_the_fallback_and_a_warning() {
        let dir = tempfile::tempdir().unwrap();
        let shell = fake_shell(dir.path(), "1000", &format!("{PATH_MARKER}/never"));
        let path = daemon_path(
            OsStr::new("/usr/bin:/bin"),
            Some(&shell),
            Some(Path::new("/Users/me")),
            Duration::from_millis(300),
        );
        assert_eq!(
            path.path,
            OsString::from("/Users/me/.local/bin:/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin")
        );
        let warning = path.warning.clone().expect("a warning");
        assert!(warning.contains("didn't finish within 0.3 s"), "{warning}");

        #[derive(Clone, Default)]
        struct Log(std::sync::Arc<std::sync::Mutex<Vec<u8>>>);
        impl std::io::Write for Log {
            fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
                self.0.lock().unwrap().extend_from_slice(bytes);
                Ok(bytes.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        let log = Log::default();
        let writer = log.clone();
        let subscriber = tracing_subscriber::fmt()
            .with_ansi(false)
            .with_writer(move || writer.clone())
            .finish();
        tracing::subscriber::with_default(subscriber, || path.log_warning());
        let text = String::from_utf8(log.0.lock().unwrap().clone()).unwrap();
        assert!(text.contains("WARN"), "{text}");
        assert!(
            text.contains("couldn't read the login shell's PATH"),
            "{text}"
        );
        assert!(text.contains("didn't finish within 0.3 s"), "{text}");
    }

    /// A login shell that exits without printing a PATH is a failure too, with its exit status in the warning.
    #[cfg(unix)]
    #[test]
    fn a_login_shell_that_prints_no_path_gives_a_warning() {
        let dir = tempfile::tempdir().unwrap();
        let shell = fake_shell(dir.path(), "0", "no marker here");
        let path = daemon_path(
            OsStr::new("/usr/bin"),
            Some(&shell),
            None,
            DAEMON_LOGIN_PATH_TIMEOUT,
        );
        assert_eq!(
            path.path,
            OsString::from("/opt/homebrew/bin:/usr/local/bin:/usr/bin")
        );
        let warning = path.warning.expect("a warning");
        assert!(
            warning.contains("printed no PATH (exit status: 0)"),
            "{warning}"
        );
    }

    #[test]
    fn versions_as_the_checklist_shows_them() {
        assert_eq!(
            parse_version(Tool::Git, "git version 2.46.0\n").as_deref(),
            Some("2.46.0")
        );
        assert_eq!(
            parse_version(Tool::Git, "git version 2.39.5 (Apple Git-154)\n").as_deref(),
            Some("2.39.5")
        );
        assert_eq!(
            parse_version(Tool::Claude, "2.1.4 (Claude Code)\n").as_deref(),
            Some("Claude Code 2.1.4")
        );
        assert_eq!(
            parse_version(
                Tool::Gh,
                "gh version 2.62.0 (2024-11-14)\nhttps://github.com/cli/cli/releases/tag/v2.62.0\n"
            )
            .as_deref(),
            Some("2.62.0")
        );
        assert_eq!(parse_version(Tool::Git, ""), None);
        assert_eq!(parse_version(Tool::Gh, "garbage"), None);
    }

    fn parsed(v: Value) -> SettingsFile {
        SettingsFile::Parsed(v)
    }

    fn verdict(files: &[(&str, SettingsFile)]) -> PermissionVerdict {
        let files: Vec<_> = files
            .iter()
            .map(|(s, f)| (s.to_string(), f.clone()))
            .collect();
        evaluate_permission_mode(&files).verdict
    }

    #[test]
    fn prompting_modes() {
        for mode in ["default", "manual", "acceptEdits", "plan"] {
            let user = parsed(json!({"permissions": {"defaultMode": mode}}));
            assert_eq!(
                verdict(&[("user", user)]),
                PermissionVerdict::Prompting,
                "{mode}"
            );
        }
    }

    #[test]
    fn non_prompting_modes() {
        for mode in ["auto", "dontAsk", "bypassPermissions"] {
            let user = parsed(json!({"permissions": {"defaultMode": mode}}));
            assert_eq!(
                verdict(&[("user", user)]),
                PermissionVerdict::NotPrompting,
                "{mode}"
            );
        }
    }

    #[test]
    fn managed_settings_win() {
        let managed = parsed(json!({"permissions": {"defaultMode": "default"}}));
        let user = parsed(json!({"permissions": {"defaultMode": "auto"}}));
        let files = [("managed".to_owned(), managed), ("user".to_owned(), user)];
        let check = evaluate_permission_mode(&files);
        assert_eq!(check.verdict, PermissionVerdict::Prompting);
        assert_eq!(check.source.as_deref(), Some("managed"));
        assert_eq!(check.mode.as_deref(), Some("default"));
    }

    #[test]
    fn disabling_auto_mode_means_manual() {
        let managed = parsed(json!({"disableAutoMode": "disable"}));
        let user = parsed(json!({"permissions": {"defaultMode": "auto"}}));
        assert_eq!(
            verdict(&[("managed", managed.clone()), ("user", user)]),
            PermissionVerdict::Prompting
        );
        assert_eq!(
            verdict(&[("managed", managed), ("user", SettingsFile::Missing)]),
            PermissionVerdict::Prompting
        );
    }

    #[test]
    fn unknown_cases_show_no_warning() {
        // Nothing set: the built-in default depends on the version and plan.
        assert_eq!(
            verdict(&[
                ("managed", SettingsFile::Missing),
                ("user", parsed(json!({})))
            ]),
            PermissionVerdict::Unknown
        );
        assert_eq!(verdict(&[]), PermissionVerdict::Unknown);
        // Unreadable file before any decision.
        assert_eq!(
            verdict(&[("user", SettingsFile::Unreadable)]),
            PermissionVerdict::Unknown
        );
        // A mode this build doesn't know.
        let user = parsed(json!({"permissions": {"defaultMode": "telepathy"}}));
        assert_eq!(verdict(&[("user", user)]), PermissionVerdict::Unknown);
        // Wrong shapes are ignored.
        let user = parsed(json!({"permissions": {"defaultMode": 3}}));
        assert_eq!(verdict(&[("user", user)]), PermissionVerdict::Unknown);
    }

    #[test]
    fn an_unreadable_lower_file_does_not_matter_once_decided() {
        let managed = parsed(json!({"permissions": {"defaultMode": "acceptEdits"}}));
        assert_eq!(
            verdict(&[("managed", managed), ("user", SettingsFile::Unreadable)]),
            PermissionVerdict::Prompting
        );
    }

    #[test]
    fn install_commands_per_os() {
        assert_eq!(Tool::Gh.install_command_on(Os::Mac), "brew install gh");
        assert_eq!(
            Tool::Gh.install_command_on(Os::Windows),
            "winget install GitHub.cli"
        );
        assert_eq!(
            Tool::Az.install_command_on(Os::Windows),
            "winget install Microsoft.AzureCLI"
        );
        assert_eq!(
            Tool::Git.install_command_on(Os::Linux),
            "sudo apt install git"
        );
        assert_eq!(
            Tool::Gh.install_command_on(Os::Linux),
            "sudo apt install gh"
        );
        for os in [Os::Mac, Os::Windows, Os::Linux] {
            assert!(
                Tool::ALL
                    .iter()
                    .all(|t| !t.install_command_on(os).is_empty())
            );
            // The extension az needs comes with az's install, except in PowerShell (maybe no `&&`): a second step.
            assert_eq!(
                Tool::Az
                    .install_command_on(os)
                    .ends_with("az extension add --name azure-devops"),
                os != Os::Windows
            );
        }
        assert_eq!(
            Tool::Gh.install_command(),
            Tool::Gh.install_command_on(Os::current())
        );
        // `claude` is an agent tool now: one connected agent tool is the requirement (SET-02, d0086).
        assert!(Tool::Git.required() && !Tool::Claude.required());
        assert!(!Tool::Gh.required() && !Tool::Az.required());
        assert!(!Tool::ALL.contains(&Tool::Claude));
    }
}
