//! A checklist row's **Install** and **Log in** (git, gh, az; SET-02, PLT-03 revised): a plain terminal of no
//! project, kept in memory only like an agent tool's Sign in… (d0086), running exactly the command the row shows
//! (`brew install gh`, `gh auth login`) after a `$ …` line, so the user sees its output and answers its prompts (a
//! sudo password, a device code). It starts only on the user's click; the command comes from [`crate::tools`],
//! never from the page. The app checks the tools again when it ends.
//!
//! One terminal per tool and action: opening one that still runs returns it.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use super::TerminalEnv;
use crate::board::{BoardError, Refusal};
use crate::data_dir::DataDir;
use crate::session::{SessionInfo, SessionKind, Sessions, SpawnSpec, Start, TerminalSize};
use crate::shell::ShellCommand;
use crate::tools::{self, CheckEnv, Tool, ToolAction};

/// The terminal's starting size: short, as the checklist shows it; the app resizes it.
const START_SIZE: TerminalSize = TerminalSize {
    cols: 100,
    rows: 10,
};

#[derive(Clone)]
pub struct ToolTerminals {
    sessions: Sessions,
    data: DataDir,
    env: TerminalEnv,
    tools: CheckEnv,
    /// The newest session of each tool and action.
    open: Arc<Mutex<HashMap<(Tool, ToolAction), String>>>,
    /// The newest session of each plugin command (its plugin and command line).
    open_commands: Arc<Mutex<HashMap<String, String>>>,
}

impl ToolTerminals {
    pub fn new(sessions: Sessions, data: DataDir, env: TerminalEnv, tools: CheckEnv) -> Self {
        Self {
            sessions,
            data,
            env,
            tools,
            open: Arc::default(),
            open_commands: Arc::default(),
        }
    }

    /// `tools.terminal`: runs `action` for `tool`, or answers the terminal still running it. Refused when the
    /// action doesn't apply now (installed already, logged in already, or the package manager is missing).
    pub async fn open(&self, tool: Tool, action: ToolAction) -> Result<SessionInfo, BoardError> {
        let running = self
            .open
            .lock()
            .expect("tool terminals")
            .get(&(tool, action))
            .cloned();
        if let Some(id) = running
            && let Ok(info) = self.sessions.info(&id)
            && info.is_running()
        {
            return Ok(info);
        }
        let status = tools::check_one(tool, &self.tools).await;
        let Some(command) = status.command_for(action) else {
            let why = match (action, &status.missing_package_manager) {
                (ToolAction::Install, Some(manager)) => {
                    format!("its install command needs {manager}")
                }
                (ToolAction::Install, None) => "has nothing to install".to_owned(),
                (ToolAction::Login, _) => "needs no login".to_owned(),
            };
            return Err(BoardError::refused(
                Refusal::NotAvailableYet,
                format!("{} {why}", tool.command()),
            ));
        };
        let run = command_line(command, &self.env);
        let mut env = self.env.env(&self.data, None, None);
        env.extend(run.env);
        let info = self
            .sessions
            .spawn(SpawnSpec {
                kind: SessionKind::Project,
                start: Start::User,
                // No project: the terminal belongs to the checklist, and isn't stored.
                project_id: String::new(),
                project_key: String::new(),
                ticket: None,
                program: run.program,
                args: run.args,
                cwd: std::env::home_dir().unwrap_or_else(|| PathBuf::from(self.data.root())),
                env,
                size: START_SIZE,
                tool: None,
                tool_session_id: None,
                model: None,
                agent: None,
                effort: None,
                fallback: None,
                theme: None,
            })
            .await
            .map_err(|e| BoardError::refused(Refusal::SessionEnded, e.to_string()))?;
        tracing::info!(tool = tool.command(), ?action, session = %info.id, "tool terminal opened");
        self.open
            .lock()
            .expect("tool terminals")
            .insert((tool, action), info.id.clone());
        Ok(info)
    }
}

impl ToolTerminals {
    /// A plugin's **Log in** or **Add extension** (the otaq-marketplace README): the same kind of terminal, running one of
    /// the plugin's own command lines, which the user agreed to on its consent screen. The caller found `argv` in
    /// the installed manifest; nothing comes from the page.
    pub async fn open_command(
        &self,
        plugin: &str,
        argv: &[String],
    ) -> Result<SessionInfo, BoardError> {
        let shown = quoted(argv);
        let key = format!("{plugin}\0{shown}");
        let running = self
            .open_commands
            .lock()
            .expect("tool terminals")
            .get(&key)
            .cloned();
        if let Some(id) = running
            && let Ok(info) = self.sessions.info(&id)
            && info.is_running()
        {
            return Ok(info);
        }
        let run = command_line(&shown, &self.env);
        let mut env = self.env.env(&self.data, None, None);
        env.extend(run.env);
        let info = self
            .sessions
            .spawn(SpawnSpec {
                kind: SessionKind::Project,
                start: Start::User,
                project_id: String::new(),
                project_key: String::new(),
                ticket: None,
                program: run.program,
                args: run.args,
                cwd: std::env::home_dir().unwrap_or_else(|| PathBuf::from(self.data.root())),
                env,
                size: START_SIZE,
                tool: None,
                tool_session_id: None,
                model: None,
                agent: None,
                effort: None,
                fallback: None,
                theme: None,
            })
            .await
            .map_err(|e| BoardError::refused(Refusal::SessionEnded, e.to_string()))?;
        tracing::info!(plugin, session = %info.id, "plugin terminal opened");
        self.open_commands
            .lock()
            .expect("tool terminals")
            .insert(key, info.id.clone());
        Ok(info)
    }
}

/// A command line as one shell line: each argument quoted where it needs it (PowerShell's quotes on Windows).
pub fn quoted(argv: &[String]) -> String {
    let safe = |a: &str| {
        !a.is_empty()
            && a.chars().all(|c| {
                c.is_ascii_alphanumeric()
                    || matches!(c, '-' | '_' | '.' | '/' | ':' | '=' | '@' | ',')
            })
    };
    argv.iter()
        .map(|a| {
            if safe(a) {
                a.clone()
            } else if cfg!(windows) {
                format!("'{}'", a.replace('\'', "''"))
            } else {
                format!("'{}'", a.replace('\'', "'\\''"))
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// How an install or login command runs: `/bin/sh -c` after a `$ <command>` line on macOS and Linux; PowerShell
/// on Windows, where the install commands are PowerShell's (`irm … | iex`, `winget …`).
pub fn command_line(command: &str, env: &TerminalEnv) -> ShellCommand {
    if cfg!(windows) {
        ShellCommand {
            program: env.shell.clone(),
            args: vec![
                "-NoLogo".into(),
                "-NoProfile".into(),
                "-Command".into(),
                format!("Write-Host '$ {}'; {command}", command.replace('\'', "''")),
            ],
            env: vec![],
        }
    } else {
        crate::shell::one_line_shown(command, command)
    }
}
