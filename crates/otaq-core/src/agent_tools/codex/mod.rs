//! The Codex adapter (ADP-07; d0086, d0088). Each group of the interface lives in its own file, so the lane
//! tickets never edit the same lines:
//! - [`detect`]: found on PATH and the version (T-0347), signed in (T-0351);
//! - [`launch`]: launch and resume, the resume check, the conversation-not-found text (T-0349); [`oneoff`]: "Ask
//!   the agent" (T-0349); [`history`]: "Before the restart" (T-0349);
//! - [`hooks`]: its hooks (T-0350); [`session_file`]: the watched session file (T-0350); [`usage`]: tokens and
//!   limit windows (T-0350);
//! - [`setup`]: the one-time setup in `~/.codex` (T-0351), [`config_toml`]: editing its `config.toml` (T-0351);
//!   [`models`]: its model list (T-0351); [`permissions`]: the permission warning (T-0351).
//!
//! The names, the input quirks and the skills are here (T-0347).

mod app_server;
pub mod config_toml;
pub mod detect;
pub mod history;
pub mod hooks;
pub mod launch;
pub mod models;
pub mod oneoff;
pub mod permissions;
pub mod session_file;
pub mod setup;
pub mod usage;

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde_json::Value;

use super::{
    AgentTool, BoxFuture, Detection, InputQuirks, LaunchError, LaunchPlan, LaunchRequest,
    ModelList, ModelListError, OneOffCommand, OneOffLine, OneOffRequest, Os, SetupEntry,
    SetupOutcome, SetupReport, SignalContext, TokenSource, Tool, ToolContext, ToolEvent,
    TranscriptEntry, WindowsSource,
};
use crate::agent_tools::events::LimitWindow;
use crate::naming;
use crate::usage::limit::LimitKind;

/// The name shown to people.
pub(super) const DISPLAY_NAME: &str = "Codex";

/// Codex's reasoning effort levels (AGM-01): its config key `model_reasoning_effort` ("minimal | low | medium | high |
/// xhigh", the Codex configuration reference, https://developers.openai.com/codex/config-reference), set for one
/// launch with `-c model_reasoning_effort="<level>"` (`codex --help`: "-c, --config <key=value> Override a
/// configuration value", 0.157.1, read with a throwaway HOME).
pub(super) const EFFORT_LEVELS: &[&str] = &["minimal", "low", "medium", "high", "xhigh"];

/// The short name used in sentences ("Codex's saved conversation").
pub(super) const SHORT_NAME: &str = "Codex";

/// The command name.
pub(super) const COMMAND_NAME: &str = "codex";

/// Ctrl+E (end of line) then Ctrl+U (delete to the start of the line). Never Ctrl+C: it quits Codex on an empty
/// line (ADP-06).
const CLEAR_LINE: &[u8] = b"\x05\x15";

/// Codex takes text typed at once as a paste; Enter right after it is part of the paste, so it waits (ADP-06).
const ENTER_DELAY: Duration = Duration::from_millis(100);

/// How Codex is installed (the same npm package on every OS).
const INSTALL: &str = "npm install -g @openai/codex";

/// Where Codex finds things on this machine.
#[derive(Debug, Clone)]
pub struct CodexContext {
    pub tool: ToolContext,
    /// Codex's home folder: `$CODEX_HOME`, else `~/.codex`; `None` without a home folder (tests).
    pub codex_home: Option<PathBuf>,
}

impl CodexContext {
    pub fn new(tool: ToolContext) -> Self {
        let codex_home = tool.home.as_ref().map(|home| {
            std::env::var_os("CODEX_HOME")
                .filter(|dir| !dir.is_empty())
                .map_or_else(|| home.join(".codex"), PathBuf::from)
        });
        Self { tool, codex_home }
    }
}

/// What a running Codex session sets on the commands it runs (seen in `codex exec`, 0.157.1), dropped by the service
/// at start ([`crate::agent_tools::parent_session_vars`]): that session's ids and its CI marker. None of them stops
/// a new session saving or resuming (checked by hand); they would only name a session that isn't the agent's.
/// Codex's sandbox variables stay: a service started inside Codex's sandbox inherits the sandbox itself.
pub(super) const PARENT_SESSION_VARS: &[&str] =
    &["CODEX_THREAD_ID", "CODEX_SESSION_ID", "CODEX_CI"];

/// The Codex adapter.
#[derive(Debug, Clone)]
pub struct Codex {
    context: CodexContext,
}

impl Codex {
    pub fn new(tool: ToolContext) -> Self {
        Self {
            context: CodexContext::new(tool),
        }
    }

    pub fn with_context(context: CodexContext) -> Self {
        Self { context }
    }

    pub fn context(&self) -> &CodexContext {
        &self.context
    }
}

impl AgentTool for Codex {
    fn tool(&self) -> Tool {
        Tool::Codex
    }

    fn display_name(&self) -> &'static str {
        DISPLAY_NAME
    }

    fn short_name(&self) -> &'static str {
        SHORT_NAME
    }

    fn command_name(&self) -> &'static str {
        COMMAND_NAME
    }

    fn detect(&self) -> BoxFuture<'_, Detection> {
        Box::pin(detect::detect(&self.context))
    }

    fn login_command(&self) -> Option<Vec<String>> {
        detect::login_command()
    }

    fn install_command(&self, _os: Os) -> &'static str {
        INSTALL
    }

    fn permission_warning(&self) -> Option<String> {
        permissions::warning(&self.context)
    }

    fn launch(&self, request: &LaunchRequest) -> Result<LaunchPlan, LaunchError> {
        launch::launch(self, request)
    }

    fn unavailable(&self) -> Option<LaunchError> {
        launch::unavailable(&self.context)
    }

    fn can_resume(&self, tool_id: &str, cwd: &Path) -> bool {
        launch::can_resume(&self.context, tool_id, cwd)
    }

    fn conversation_not_found(&self, output: &[u8]) -> bool {
        launch::conversation_not_found(output)
    }

    fn resume_hint(&self, tool_id: &str) -> String {
        launch::resume_hint(tool_id)
    }

    fn list_models(&self) -> BoxFuture<'_, Result<ModelList, ModelListError>> {
        Box::pin(models::read_list(&self.context))
    }

    fn one_off(&self, request: &OneOffRequest) -> Result<OneOffCommand, LaunchError> {
        oneoff::command(&self.context, request)
    }

    fn one_off_line(&self, line: &str) -> Option<OneOffLine> {
        oneoff::read_line(line)
    }

    fn record_project(&self, repo: &Path) -> Option<SetupOutcome> {
        setup::record_project(&self.context, repo)
    }

    fn set_up_tool(&self) -> BoxFuture<'_, SetupReport> {
        Box::pin(setup::set_up_tool(&self.context))
    }

    fn setup_entries(&self) -> Vec<SetupEntry> {
        setup::setup_entries()
    }

    fn remove_setup(&self) -> SetupReport {
        setup::remove_setup(&self.context)
    }

    fn setup_needed(&self) -> bool {
        setup::setup_needed(&self.context)
    }

    fn map_hook(&self, event: &str, payload: &Value, context: SignalContext<'_>) -> Vec<ToolEvent> {
        hooks::map_hook(event, payload, context)
    }

    fn watched_file(&self, tool_id: &str, cwd: &Path) -> Option<PathBuf> {
        session_file::watched_file(&self.context, tool_id, cwd)
    }

    fn map_watched_lines(&self, lines: &str, context: SignalContext<'_>) -> Vec<ToolEvent> {
        session_file::map_lines(lines, context)
    }

    fn token_source(&self, tool_id: &str, cwd: &Path) -> TokenSource {
        usage::token_source(&self.context, tool_id, cwd)
    }

    fn last_entries(
        &self,
        tool_id: &str,
        cwd: &Path,
        limit: usize,
    ) -> Option<Vec<TranscriptEntry>> {
        history::last_entries(&self.context, tool_id, cwd, limit)
    }

    fn windows_source(&self) -> WindowsSource {
        usage::windows_source()
    }

    fn windows_from_report(&self, report: &Value) -> Option<Vec<LimitWindow>> {
        usage::windows_from_report(report)
    }

    fn limit_window_key(&self, kind: LimitKind) -> Option<&'static str> {
        usage::limit_window_key(kind)
    }

    fn limit_reset_from_text(&self, text: &str, now: i64) -> Option<i64> {
        usage::limit_reset_from_text(text, now)
    }

    fn input(&self) -> InputQuirks {
        InputQuirks {
            clear_line: CLEAR_LINE,
            enter_delay: ENTER_DELAY,
            multi_line_clearable: false,
        }
    }

    fn typed_key_answers_prompt(&self) -> bool {
        false
    }

    fn turn_end_reports_background(&self) -> bool {
        false
    }

    fn swaps_refused_model(&self) -> bool {
        false
    }

    fn effort_levels(&self) -> &'static [&'static str] {
        EFFORT_LEVELS
    }

    /// `$otaq-commit`: Codex reads the project skills in `.agents/skills` "in every directory from your current
    /// working directory up to the repository root" and a skill is mentioned with `$` (its skills page,
    /// learn.chatgpt.com/docs/build-skills, formerly developers.openai.com/codex/skills).
    fn skill_reference(&self, skill: &str) -> String {
        format!("${}", naming::copy_name(skill))
    }

    /// Codex's ids are models.dev's `openai` ids exactly (d0086).
    fn family_source(&self) -> Option<super::catalog::FamilySource> {
        Some(super::catalog::FamilySource::ModelsDev { provider: "openai" })
    }
}

#[cfg(test)]
mod tests;
