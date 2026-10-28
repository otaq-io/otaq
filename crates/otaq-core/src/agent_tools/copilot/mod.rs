//! The GitHub Copilot adapter (ADP-08; d0086, d0088). Each group of the interface lives in its own file, so the
//! lane tickets never edit the same lines:
//! - [`detect`]: found on PATH, the version and "update needed" before 1.0.98 (T-0347), signed in and Sign in…
//!   (T-0353);
//! - [`launch`]: launch and resume, the resume check, the conversation-not-found text (T-0352); [`hooks`]: its
//!   hooks file's events (T-0352); [`events_file`]: its `events.jsonl` (errors, the model that ran, "Before the
//!   restart"; T-0352); [`usage`]: tokens and the monthly window (T-0352);
//! - [`models`]: its model list and Auto tiers (T-0353); [`trust`]: folder trust, its only setup (T-0353);
//!   [`oneoff`]: "Ask the agent" (T-0353); [`permissions`]: the permission warning (T-0353).
//!
//! The names, the input quirks and the skills are here (T-0347).

pub mod detect;
pub mod events_file;
pub mod hooks;
pub mod launch;
pub mod models;
pub mod oneoff;
pub mod permissions;
pub mod trust;
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

use crate::usage::limit::LimitKind;

/// The name shown to people.
pub(super) const DISPLAY_NAME: &str = "GitHub Copilot";

/// Copilot's reasoning effort levels (AGM-01): its `--reasoning-effort <level>` flag, "Set the reasoning effort level
/// [possible values: none, minimal, low, medium, high, xhigh, max]" (`copilot --help`, GitHub Copilot CLI 1.0.98,
/// read with a throwaway HOME).
pub(super) const EFFORT_LEVELS: &[&str] =
    &["none", "minimal", "low", "medium", "high", "xhigh", "max"];

/// The short name used in sentences ("Copilot's saved conversation").
pub(super) const SHORT_NAME: &str = "Copilot";

/// The command name.
pub(super) const COMMAND_NAME: &str = "copilot";

/// Ctrl+E (end of line) then Ctrl+U (delete to the start of the line); it clears a multi-line draft too. Never
/// Ctrl+C: it arms Copilot's exit (ADP-06).
const CLEAR_LINE: &[u8] = b"\x05\x15";

/// How Copilot is installed (the same npm package on every OS; Homebrew's is older than Otaq can run).
const INSTALL: &str = "npm install -g @github/copilot";

/// Where Copilot finds things on this machine.
#[derive(Debug, Clone)]
pub struct CopilotContext {
    pub tool: ToolContext,
    /// Copilot's home folder: `$COPILOT_HOME`, else `~/.copilot`; `None` without a home folder (tests).
    pub copilot_home: Option<PathBuf>,
}

impl CopilotContext {
    pub fn new(tool: ToolContext) -> Self {
        let copilot_home = tool.home.as_ref().map(|home| {
            std::env::var_os("COPILOT_HOME")
                .filter(|dir| !dir.is_empty())
                .map_or_else(|| home.join(".copilot"), PathBuf::from)
        });
        Self { tool, copilot_home }
    }
}

/// What a running GitHub Copilot session sets on the commands it runs (1.0.105), dropped by the service at start
/// ([`crate::agent_tools::parent_session_vars`]): `COPILOT_AGENT_SESSION_ID` scopes MCP sign-ins to that session,
/// and a detached helper's `COPILOT_DETACHED_*` skip the end-of-session memory step and link a new session to its
/// parent. A new session still saves and resumes with them (checked by hand). The user's own `COPILOT_HOME` and
/// `COPILOT_MODEL` stay.
pub(super) const PARENT_SESSION_VARS: &[&str] = &[
    "COPILOT_CLI",
    "COPILOT_AGENT_SESSION_ID",
    "COPILOT_DETACHED_SESSION",
    "COPILOT_DETACHED_PARENT_SESSION_ID",
    "COPILOT_DETACHED_PARENT_ENGAGEMENT_ID",
    "COPILOT_RUN_APP",
];

/// The GitHub Copilot adapter.
#[derive(Debug, Clone)]
pub struct Copilot {
    context: CopilotContext,
}

impl Copilot {
    pub fn new(tool: ToolContext) -> Self {
        Self {
            context: CopilotContext::new(tool),
        }
    }

    pub fn with_context(context: CopilotContext) -> Self {
        Self { context }
    }

    pub fn context(&self) -> &CopilotContext {
        &self.context
    }
}

impl AgentTool for Copilot {
    fn tool(&self) -> Tool {
        Tool::Copilot
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
        detect::login_command(&self.context)
    }

    fn install_command(&self, _os: Os) -> &'static str {
        INSTALL
    }

    fn permission_warning(&self) -> Option<String> {
        permissions::warning(&self.context)
    }

    fn launch(&self, request: &LaunchRequest) -> Result<LaunchPlan, LaunchError> {
        launch::launch(&self.context, request)
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
        trust::record_project(&self.context, repo)
    }

    fn set_up_tool(&self) -> BoxFuture<'_, SetupReport> {
        Box::pin(std::future::ready(trust::set_up_tool()))
    }

    fn setup_entries(&self) -> Vec<SetupEntry> {
        trust::setup_entries()
    }

    fn remove_setup(&self) -> SetupReport {
        trust::remove_setup()
    }

    fn map_hook(&self, event: &str, payload: &Value, context: SignalContext<'_>) -> Vec<ToolEvent> {
        hooks::map_hook(&self.context, event, payload, context)
    }

    fn watched_file(&self, tool_id: &str, cwd: &Path) -> Option<PathBuf> {
        hooks::watched_file(tool_id, cwd)
    }

    fn map_watched_lines(&self, lines: &str, context: SignalContext<'_>) -> Vec<ToolEvent> {
        hooks::map_watched_lines(lines, context)
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
        events_file::last_entries(&self.context, tool_id, cwd, limit)
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
            enter_delay: Duration::ZERO,
            multi_line_clearable: true,
        }
    }

    /// Esc at Copilot's permission prompt ends the turn without a hook (d0086).
    fn typed_key_answers_prompt(&self) -> bool {
        true
    }

    fn turn_end_reports_background(&self) -> bool {
        false
    }

    /// An unavailable `--model` starts on Auto instead (ADP-08).
    fn swaps_refused_model(&self) -> bool {
        true
    }

    fn effort_levels(&self) -> &'static [&'static str] {
        EFFORT_LEVELS
    }

    /// `/otaq-commit` ([`launch::skill_reference`]).
    fn skill_reference(&self, skill: &str) -> String {
        launch::skill_reference(skill)
    }
}

#[cfg(test)]
mod tests;
