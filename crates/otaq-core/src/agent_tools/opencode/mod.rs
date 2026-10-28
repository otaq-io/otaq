//! The OpenCode adapter (ADP-09; d0086, d0088). Each group of the interface lives in its own file, so the lane
//! tickets never edit the same lines:
//! - [`detect`]: found on PATH and the version (T-0347), signed in and Sign in… (T-0355);
//! - [`launch`]: launch and resume and the skill reference (T-0354); [`prepare`]: the session made before launch
//!   (T-0354); [`hooks`]: Otaq's plugin's events (T-0354); [`usage`]: tokens, no limit windows (T-0354);
//! - [`models`]: its model list from a short-lived server (T-0355); [`sessions`]: the resume check, the
//!   conversation-not-found text, "Before the restart" (T-0355); [`oneoff`]: "Ask the agent" (T-0355);
//!   [`permissions`]: the permission warning (T-0355).
//!
//! The names, the input quirks, the skills' delivery and folders are here (T-0347), and the one-time setup:
//! OpenCode needs none (no folder trust exists; everything comes with each launch, d0086).

pub mod detect;
pub mod hooks;
pub mod launch;
pub mod models;
pub mod oneoff;
pub mod permissions;
pub mod prepare;
pub mod sessions;
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
use crate::agent_tools::catalog::FamilySource;
use crate::agent_tools::events::LimitWindow;
use crate::usage::limit::LimitKind;

/// The name shown to people.
pub(super) const DISPLAY_NAME: &str = "OpenCode";

/// OpenCode has no effort setting of its own (AGM-01): `opencode run --help` (v2.0.17, read with a throwaway HOME)
/// lists none, and a model's reasoning level is one of its **variants**, part of the model itself
/// (`--model provider/model#variant`, https://opencode.ai/docs/models/#variants), which the model picker already
/// offers. So an OpenCode agent gets no Effort field.
pub(super) const EFFORT_LEVELS: &[&str] = &[];

/// The short name used in sentences ("OpenCode's saved conversation").
pub(super) const SHORT_NAME: &str = "OpenCode";

/// The command name.
pub(super) const COMMAND_NAME: &str = "opencode";

/// Ctrl+E (end of line) then Ctrl+U (delete to the start of the line); only the current line of a multi-line
/// draft clears. Never Ctrl+C: it quits OpenCode on an empty line (ADP-06).
const CLEAR_LINE: &[u8] = b"\x05\x15";

/// How OpenCode is installed (the same npm package on every OS).
const INSTALL: &str = "npm install -g opencode-ai";

/// Where OpenCode finds things on this machine. Agents use the user's own OpenCode data folder (ADP-09).
#[derive(Debug, Clone)]
pub struct OpenCodeContext {
    pub tool: ToolContext,
    /// OpenCode's config folder: `$XDG_CONFIG_HOME/opencode`, else `~/.config/opencode`; `None` without a home
    /// folder (tests). Read only (the permission warning).
    pub config_dir: Option<PathBuf>,
}

impl OpenCodeContext {
    pub fn new(tool: ToolContext) -> Self {
        let config_dir = tool.home.as_ref().map(|home| {
            std::env::var_os("XDG_CONFIG_HOME")
                .filter(|dir| !dir.is_empty())
                .map_or_else(|| home.join(".config"), PathBuf::from)
                .join(COMMAND_NAME)
        });
        Self { tool, config_dir }
    }
}

/// What a running OpenCode session sets on the commands it runs (2.0.17), dropped by the service at start
/// ([`crate::agent_tools::parent_session_vars`]): its terminal marker and the handoff it passes while its own
/// service is replaced. A new session still saves and resumes with them (checked by hand).
pub(super) const PARENT_SESSION_VARS: &[&str] = &["OPENCODE_TERMINAL", "OPENCODE_PTY_HANDOFF"];

/// The OpenCode adapter.
#[derive(Debug, Clone)]
pub struct OpenCode {
    context: OpenCodeContext,
    /// The plugin events already handled, by their `id` (ADP-09).
    seen: std::sync::Arc<hooks::Seen>,
}

impl OpenCode {
    pub fn new(tool: ToolContext) -> Self {
        Self::with_context(OpenCodeContext::new(tool))
    }

    pub fn with_context(context: OpenCodeContext) -> Self {
        Self {
            context,
            seen: std::sync::Arc::default(),
        }
    }

    pub fn context(&self) -> &OpenCodeContext {
        &self.context
    }
}

impl AgentTool for OpenCode {
    fn tool(&self) -> Tool {
        Tool::OpenCode
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
        launch::launch(self, request)
    }

    fn unavailable(&self) -> Option<LaunchError> {
        launch::unavailable(&self.context)
    }

    fn can_resume(&self, tool_id: &str, cwd: &Path) -> bool {
        sessions::can_resume(&self.context, tool_id, cwd)
    }

    fn conversation_not_found(&self, output: &[u8]) -> bool {
        sessions::conversation_not_found(output)
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

    /// OpenCode keeps no folder trust (ADP-09).
    fn record_project(&self, _repo: &Path) -> Option<SetupOutcome> {
        None
    }

    fn set_up_tool(&self) -> BoxFuture<'_, SetupReport> {
        Box::pin(std::future::ready(SetupReport::default()))
    }

    /// Nothing: its plugin, skills and the `otaq` rule come with every launch.
    fn setup_entries(&self) -> Vec<SetupEntry> {
        Vec::new()
    }

    fn remove_setup(&self) -> SetupReport {
        SetupReport::default()
    }

    fn map_hook(&self, event: &str, payload: &Value, context: SignalContext<'_>) -> Vec<ToolEvent> {
        hooks::map_hook(&self.seen, event, payload, context)
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
        sessions::last_entries(&self.context, tool_id, cwd, limit)
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
            multi_line_clearable: false,
        }
    }

    fn typed_key_answers_prompt(&self) -> bool {
        false
    }

    fn turn_end_reports_background(&self) -> bool {
        false
    }

    /// OpenCode rewrites a refused model quietly (AGT-05).
    fn swaps_refused_model(&self) -> bool {
        true
    }

    fn skill_reference(&self, skill: &str) -> String {
        launch::skill_reference(skill)
    }

    fn family_source(&self) -> Option<FamilySource> {
        Some(models::family_source())
    }
}

#[cfg(test)]
mod tests;
