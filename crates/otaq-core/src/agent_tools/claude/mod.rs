//! The Claude Code adapter (ADP-01; d0088). It answers the names, detect, input, skills and signals ([`hooks`])
//! itself; its launch and resume are [`launch`] with Otaq's status line ([`statusline`]). Its one-time setup is
//! folder trust ([`trust`]); its tokens and "Before the restart" come from its saved conversations
//! ([`transcript`]).

pub mod hooks;
mod launch;
pub mod limits;
pub mod models;
pub mod oneoff;
pub mod statusline;
pub mod transcript;
pub mod trust;

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde_json::Value;

use self::launch::AgentCommand;
pub use self::launch::{ClaudeLaunch, find_plugin_dir};
use self::trust::ClaudeConfig;
use super::{
    AgentTool, BoxFuture, Detection, FirstPrompt, InputQuirks, LaunchContext, LaunchError,
    LaunchPlan, LaunchRequest, LaunchRole, ModelList, ModelListError, OneOffCommand, OneOffLine,
    OneOffRequest, Os, SetupEntry, SetupOutcome, SetupReport, SignalContext, TokenSource, Tool,
    ToolEvent, ToolIdMode, TranscriptEntry, WindowsSource,
};
use crate::agent_tools::events::LimitWindow;
use crate::data_dir::DataDir;
use crate::naming;
use crate::tools::{self, CheckEnv, PermissionVerdict};
use crate::usage::limit::LimitKind;
use transcript::{Transcript, find_transcript, read_transcript};

/// What clears Claude Code's input line before Otaq types (SES-07 "Clear it and send"): Ctrl+E (end of line) then
/// Ctrl+U (delete to the start of the line). Implementation choice; verified by hand against the real `claude`.
const CLEAR_LINE: &[u8] = b"\x05\x15";

/// Where Claude Code and Otaq's files are on this machine.
#[derive(Debug, Clone)]
pub struct ClaudeContext {
    /// The program, plugin folder and PATH for launches. `None` when Otaq's plugin wasn't found: every launch
    /// fails with that reason.
    pub launch: Option<LaunchContext>,
    /// The PATH searched for `claude`, and the user's Claude Code folder (saved conversations).
    pub check: CheckEnv,
    /// Otaq's data folder, where the status-line settings files go.
    pub data: DataDir,
    /// Claude Code's config file for folder trust; `None` when it can't be located.
    pub trust: Option<ClaudeConfig>,
}

/// The name shown to people.
pub(super) const DISPLAY_NAME: &str = "Claude Code";

/// Claude Code's effort levels (AGM-01): its `--effort <level>` flag, "Effort level for the current session (low,
/// medium, high, xhigh, max)" (`claude --help`, 2.1.304, read with a throwaway HOME). Passed on every launch and
/// resume that has one, like the model.
pub(super) const EFFORT_LEVELS: &[&str] = &["low", "medium", "high", "xhigh", "max"];

/// The flag [`EFFORT_LEVELS`] go with.
const EFFORT_FLAG: &str = "--effort";

/// The short name used in sentences ("Claude's saved conversation").
pub(super) const SHORT_NAME: &str = "Claude";

/// The command name (History's "claude 3be94c10…").
pub(super) fn command_name() -> &'static str {
    tools::Tool::Claude.command()
}

/// How the user resumes conversation `tool_id` by hand: `claude --resume <id>`.
pub(super) fn resume_hint(tool_id: &str) -> String {
    format!("{} --resume {tool_id}", command_name())
}

/// What `claude --resume <id>` prints when it can't find the conversation, compared without case ("No
/// conversation found with session ID: …"). Checked by hand against the installed CLI (SDD §26).
const CONVERSATION_NOT_FOUND: &str = "no conversation found";

/// The Claude Code adapter.
#[derive(Debug, Clone)]
pub struct ClaudeCode {
    context: ClaudeContext,
}

impl ClaudeCode {
    pub fn new(context: ClaudeContext) -> Self {
        Self { context }
    }

    fn claude_dir(&self) -> Option<&Path> {
        self.context.check.claude_config_dir.as_deref()
    }

    /// The prompt as typed: `/otaq-epic-lead <message>`, or the text.
    fn prompt_text(&self, prompt: &FirstPrompt) -> String {
        prompt.text(self)
    }

    /// Otaq's settings file for the session: its status line (USG-04) and the theme's kind (d0117 point 4). Without
    /// it the agent still starts; the limit windows in Activity miss data, and the theme is left to Claude Code.
    fn settings(&self, launch: &LaunchContext, request: &LaunchRequest) -> Option<PathBuf> {
        let data = &self.context.data;
        let path = match (request.role, &request.ticket) {
            (LaunchRole::ProjectAgent | LaunchRole::EpicLead, _) | (_, None) => {
                statusline::project_agent_settings_path(
                    data,
                    &request.project,
                    request.theme.kind(),
                )
            }
            (_, Some(ticket)) => statusline::settings_path(data, ticket, request.theme.kind()),
        };
        statusline::write_settings_at(
            path,
            &request.cwd,
            launch.claude.config_dir.as_deref(),
            &launch.bin_dir,
            request.theme.kind(),
        )
        .inspect_err(|e| tracing::warn!(project = %request.project, error = %e, "can't write the status-line settings"))
        .ok()
    }

    fn command(
        &self,
        launch: &LaunchContext,
        request: &LaunchRequest,
        prompt: Option<String>,
    ) -> Result<AgentCommand, LaunchError> {
        let settings = self.settings(launch, request);
        let settings = settings.as_deref();
        let cwd = request.cwd.as_path();
        let needs_ticket = || {
            request
                .ticket
                .as_ref()
                .ok_or_else(|| LaunchError(format!("{:?} needs a ticket", request.role)))
        };
        let needs_epic = || {
            request
                .epic
                .as_ref()
                .ok_or_else(|| LaunchError("an epic lead needs its epic".to_owned()))
        };
        let command = match (&request.resume, request.role) {
            (Some(id), LaunchRole::ProjectAgent) => {
                let mut command = launch::resume_project_agent(launch, cwd, id, settings);
                command.args.extend(prompt);
                command
            }
            (Some(id), LaunchRole::EpicLead) => {
                let mut command =
                    launch::resume_epic_lead(launch, cwd, needs_epic()?, id, settings);
                command.args.extend(prompt);
                command
            }
            (Some(id), role) => {
                let mut command = launch::resume_agent_with(
                    launch,
                    needs_ticket()?,
                    cwd,
                    request.port,
                    id,
                    prompt,
                    settings,
                );
                if role == LaunchRole::Validator {
                    validator_env(&mut command);
                }
                command
            }
            (None, role) => {
                let prompt = prompt.ok_or_else(|| {
                    LaunchError("a new conversation needs a first prompt".to_owned())
                })?;
                match role {
                    LaunchRole::ProjectAgent => {
                        with_last_arg(launch::launch_project_agent(launch, cwd, settings), prompt)
                    }
                    LaunchRole::EpicLead => with_last_arg(
                        launch::launch_epic_lead(launch, cwd, needs_epic()?, None, settings),
                        prompt,
                    ),
                    LaunchRole::Validator => {
                        launch::launch_validator(launch, needs_ticket()?, cwd, &prompt, settings)
                    }
                    LaunchRole::TicketAgent | LaunchRole::Answer | LaunchRole::Delivery => {
                        launch::launch_with_prompt(
                            launch,
                            needs_ticket()?,
                            cwd,
                            request.port,
                            prompt,
                            settings,
                        )
                    }
                }
            }
        };
        Ok(command)
    }
}

/// Why no launch can be built: the plugin folder wasn't found.
fn plugin_missing() -> LaunchError {
    LaunchError(format!(
        "{}'s Claude Code plugin wasn't found next to the {} command",
        naming::PRODUCT_NAME,
        naming::COMMAND_NAME
    ))
}

/// `command` with its last argument (the first prompt) replaced.
fn with_last_arg(mut command: AgentCommand, prompt: String) -> AgentCommand {
    if let Some(last) = command.args.last_mut() {
        *last = prompt;
    }
    command
}

/// A validator's environment: no `PORT`, the validator role (as [`launch::launch_validator`]).
fn validator_env(command: &mut AgentCommand) {
    command.env.retain(|(name, _)| name != "PORT");
    command
        .env
        .push((naming::role_env_var(), naming::ROLE_VALIDATOR.to_owned()));
}

impl AgentTool for ClaudeCode {
    fn tool(&self) -> Tool {
        Tool::Claude
    }

    fn display_name(&self) -> &'static str {
        DISPLAY_NAME
    }

    fn short_name(&self) -> &'static str {
        SHORT_NAME
    }

    fn command_name(&self) -> &'static str {
        command_name()
    }

    fn detect(&self) -> BoxFuture<'_, Detection> {
        Box::pin(async move {
            let status = tools::check_one(tools::Tool::Claude, &self.context.check).await;
            Detection {
                installed: status.found,
                signed_in: status.logged_in,
                version: status.version,
                ..Detection::default()
            }
        })
    }

    fn install_command(&self, os: Os) -> &'static str {
        tools::Tool::Claude.install_command_on(os)
    }

    /// The Tools checklist's evaluation of `permissions.defaultMode` (d0011, d0016), unchanged.
    fn permission_warning(&self) -> Option<String> {
        (tools::check_permission_mode(&self.context.check).verdict == PermissionVerdict::Prompting)
            .then(|| format!("Your {DISPLAY_NAME} settings ask for permission prompts"))
    }

    fn launch(&self, request: &LaunchRequest) -> Result<LaunchPlan, LaunchError> {
        let Some(launch) = &self.context.launch else {
            return Err(plugin_missing());
        };
        let prompt = request.first_prompt.as_ref().map(|p| self.prompt_text(p));
        let has_prompt = prompt.is_some();
        let mut command = self.command(launch, request, prompt)?;
        if let Some(model) = &request.model {
            // Before the prompt, which is always last.
            let at = command.args.len() - usize::from(has_prompt);
            command
                .args
                .splice(at..at, ["--model".to_owned(), model.clone()]);
        }
        if let Some(effort) = &request.effort {
            let at = command.args.len() - usize::from(has_prompt);
            command
                .args
                .splice(at..at, [EFFORT_FLAG.to_owned(), effort.clone()]);
        }
        Ok(LaunchPlan {
            program: command.program,
            args: command.args,
            cwd: command.cwd,
            env: command.env,
            tool_id: ToolIdMode::ChosenUpFront(command.tool_session_id),
        })
    }

    fn effort_levels(&self) -> &'static [&'static str] {
        EFFORT_LEVELS
    }

    fn unavailable(&self) -> Option<LaunchError> {
        self.context.launch.is_none().then(plugin_missing)
    }

    /// Its saved conversation is still on disk (looked up read-only); without a Claude folder the id is
    /// enough.
    fn can_resume(&self, tool_id: &str, cwd: &Path) -> bool {
        self.claude_dir()
            .is_none_or(|dir| find_transcript(dir, cwd, tool_id).is_some())
    }

    fn conversation_not_found(&self, output: &[u8]) -> bool {
        String::from_utf8_lossy(output)
            .to_lowercase()
            .contains(CONVERSATION_NOT_FOUND)
    }

    fn resume_hint(&self, tool_id: &str) -> String {
        resume_hint(tool_id)
    }

    /// The `initialize` request ([`models`]) with the launch's program and PATH, in Otaq's data folder.
    fn list_models(&self) -> BoxFuture<'_, Result<ModelList, ModelListError>> {
        Box::pin(async move {
            let Some(launch) = &self.context.launch else {
                return Err(ModelListError {
                    message: "Claude Code didn't list its models".to_owned(),
                    reason: "no launch context".to_owned(),
                });
            };
            models::read_list(
                &launch.claude.program,
                &launch.path,
                self.context.data.root(),
            )
            .await
        })
    }

    /// [`oneoff`] with the launch's program. Without a launch context nothing can run it.
    fn one_off(&self, request: &OneOffRequest) -> Result<OneOffCommand, LaunchError> {
        let launch = self.context.launch.as_ref().ok_or_else(|| {
            LaunchError(format!(
                "{} isn't available to the background service",
                self.display_name()
            ))
        })?;
        Ok(oneoff::command(&launch.claude.program, request))
    }

    fn one_off_line(&self, line: &str) -> Option<OneOffLine> {
        oneoff::read_line(line)
    }

    /// Folder trust in Claude Code's config file ([`trust`]); `None` when the file couldn't be located.
    fn record_project(&self, repo: &Path) -> Option<SetupOutcome> {
        Some(self.context.trust.as_ref()?.record_trust(repo))
    }

    fn set_up_tool(&self) -> BoxFuture<'_, SetupReport> {
        Box::pin(std::future::ready(SetupReport::default()))
    }

    fn setup_entries(&self) -> Vec<SetupEntry> {
        vec![SetupEntry {
            text: "trust for your projects".to_owned(),
            removed_on_disconnect: false,
        }]
    }

    /// Claude Code has no Otaq-only entries: its hooks, pre-approval and skills come with every launch, and
    /// folder trust stays when a tool is disconnected (d0086).
    fn remove_setup(&self) -> SetupReport {
        SetupReport::default()
    }

    /// Claude Code's hooks carry everything in their payload ([`hooks`]); the context isn't needed.
    fn map_hook(
        &self,
        event: &str,
        payload: &Value,
        _context: SignalContext<'_>,
    ) -> Vec<ToolEvent> {
        hooks::map_hook(event, payload)
    }

    fn watched_file(&self, _tool_id: &str, _cwd: &Path) -> Option<PathBuf> {
        None
    }

    fn map_watched_lines(&self, _lines: &str, _context: SignalContext<'_>) -> Vec<ToolEvent> {
        Vec::new()
    }

    fn token_source(&self, tool_id: &str, cwd: &Path) -> TokenSource {
        TokenSource::Reader(Box::new(Transcript::new(
            self.claude_dir().map(Path::to_path_buf),
            cwd.to_path_buf(),
            tool_id.to_owned(),
        )))
    }

    fn last_entries(
        &self,
        tool_id: &str,
        cwd: &Path,
        limit: usize,
    ) -> Option<Vec<TranscriptEntry>> {
        find_transcript(self.claude_dir()?, cwd, tool_id)
            .and_then(|path| read_transcript(&path, limit))
    }

    fn windows_source(&self) -> WindowsSource {
        WindowsSource::Report
    }

    /// The status line's windows ([`limits`]).
    fn windows_from_report(&self, report: &Value) -> Option<Vec<LimitWindow>> {
        limits::windows_from_status_line(report)
    }

    fn limit_window_key(&self, kind: LimitKind) -> Option<&'static str> {
        limits::window_key(kind)
    }

    /// The time in Claude Code's limit text ([`limits`]).
    fn limit_reset_from_text(&self, text: &str, now: i64) -> Option<i64> {
        limits::reset_from_text(text, now)
    }

    fn input(&self) -> InputQuirks {
        InputQuirks {
            clear_line: CLEAR_LINE,
            enter_delay: Duration::ZERO,
            multi_line_clearable: true,
        }
    }

    /// `/otaq-commit`: Claude Code runs a project skill of `.claude/skills/` by `/<name>` (its skills page,
    /// code.claude.com/docs/en/skills: "invoke one directly with `/skill-name`").
    fn skill_reference(&self, skill: &str) -> String {
        launch::skill_reference(skill)
    }

    /// `.claude/skills`: the only one of the two copies Claude Code reads.
    fn skill_root(&self) -> &'static str {
        crate::skills::copy::SKILL_ROOTS[1]
    }
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;

    use serde_json::json;

    use super::*;
    use crate::agent_tools::{Adapters, TokensUnknown};
    use crate::domain::TicketKey;
    use crate::signals::background::BackgroundWork;

    struct Fixture {
        _data: tempfile::TempDir,
        adapter: ClaudeCode,
    }

    fn launch_context() -> LaunchContext {
        LaunchContext {
            bin_dir: "/opt/cb/bin".into(),
            plugin_dir: "/opt/cb/plugin".into(),
            path: std::env::join_paths(["/usr/bin", "/bin"]).unwrap(),
            claude: ClaudeLaunch {
                program: vec!["claude".into()],
                config_dir: None,
            },
        }
    }

    fn fixture() -> Fixture {
        let data = tempfile::tempdir().unwrap();
        let adapter = ClaudeCode::new(ClaudeContext {
            launch: Some(launch_context()),
            check: CheckEnv {
                path: OsString::new(),
                claude_config_dir: None,
                managed_settings_dir: data.path().join("managed"),
            },
            data: DataDir::new(data.path()),
            trust: None,
        });
        Fixture {
            _data: data,
            adapter,
        }
    }

    fn key() -> TicketKey {
        "SHOP-18".parse().unwrap()
    }

    fn request(role: LaunchRole) -> LaunchRequest {
        LaunchRequest {
            role,
            cwd: "/wt/shop-web/SHOP-18".into(),
            project: "SHOP".into(),
            ticket: Some(key()),
            epic: None,
            port: 4012,
            first_prompt: Some(FirstPrompt::Skill {
                name: crate::agent_tools::WORK_TICKET_SKILL.into(),
                message: None,
            }),
            resume: None,
            model: None,
            title: None,
            effort: None,
            theme: crate::board::settings::Theme::OtaqDark,
        }
    }

    fn chosen(plan: &LaunchPlan) -> &str {
        match &plan.tool_id {
            ToolIdMode::ChosenUpFront(id) => id,
            other => panic!("not chosen up front: {other:?}"),
        }
    }

    #[test]
    fn names_keep_todays_wording() {
        let f = fixture();
        assert_eq!(f.adapter.tool(), Tool::Claude);
        assert_eq!(f.adapter.display_name(), "Claude Code");
        assert_eq!(f.adapter.short_name(), "Claude");
        assert_eq!(f.adapter.command_name(), "claude");
        assert_eq!(
            f.adapter.resume_hint("3be94c10"),
            "claude --resume 3be94c10"
        );
    }

    #[test]
    fn a_missing_conversation_is_read_from_the_resume_output() {
        let f = fixture();
        assert!(f.adapter.conversation_not_found(
            b"\x1b[31mNo conversation found with session ID: 3be94c10\x1b[0m\r\n"
        ));
        assert!(
            !f.adapter
                .conversation_not_found(b"Resuming conversation\r\n")
        );
    }

    #[test]
    fn input_quirks() {
        let quirks = fixture().adapter.input();
        assert_eq!(quirks.clear_line, b"\x05\x15");
        assert_eq!(quirks.enter_delay, Duration::ZERO);
        assert!(quirks.multi_line_clearable);
    }

    /// AGM-07: a skill by its copy's name in Claude Code's syntax for a project skill, the same for a shipped and
    /// an imported one; no plugin namespace.
    #[test]
    fn skills_by_their_copys_name() {
        let f = fixture();
        assert_eq!(
            f.adapter.skill_reference("commit"),
            format!("/{}-commit", naming::COMMAND_NAME)
        );
        assert_eq!(
            f.adapter.skill_reference("team-validate"),
            format!("/{}-team-validate", naming::COMMAND_NAME)
        );
        let mut request = request(LaunchRole::Validator);
        request.first_prompt = Some(FirstPrompt::Skill {
            name: "validate".into(),
            message: None,
        });
        let plan = f.adapter.launch(&request).unwrap();
        assert_eq!(
            plan.args.last(),
            Some(&format!("/{}-validate", naming::COMMAND_NAME))
        );
        assert!(!plan.args.iter().any(|a| a == "--add-dir"));
    }

    #[test]
    fn the_tool_id_is_chosen_up_front_a_fresh_uuid_v7_each_time() {
        let f = fixture();
        let first = f.adapter.launch(&request(LaunchRole::TicketAgent)).unwrap();
        let second = f.adapter.launch(&request(LaunchRole::TicketAgent)).unwrap();
        let (a, b) = (chosen(&first), chosen(&second));
        assert_ne!(a, b);
        for id in [a, b] {
            assert_eq!(uuid::Uuid::parse_str(id).unwrap().get_version_num(), 7);
        }
        let mut resume = request(LaunchRole::TicketAgent);
        resume.resume = Some("3be94c10-0000-7000-8000-000000000000".into());
        resume.first_prompt = None;
        let resumed = f.adapter.launch(&resume).unwrap();
        assert_eq!(chosen(&resumed), "3be94c10-0000-7000-8000-000000000000");
    }

    /// The ticket agent's launch is today's `launch_agent`, with the status-line file written into the data
    /// folder.
    #[test]
    fn a_ticket_agent_launches_as_today() {
        let f = fixture();
        let plan = f.adapter.launch(&request(LaunchRole::TicketAgent)).unwrap();
        let settings = statusline::settings_path(
            &f.adapter.context.data,
            &key(),
            crate::agent_tools::ThemeKind::Dark,
        );
        assert!(settings.is_file());
        let today = launch::launch_agent(
            &launch_context(),
            &key(),
            Path::new("/wt/shop-web/SHOP-18"),
            4012,
            Some(&settings),
        );
        assert_eq!(plan.program, today.program);
        assert_eq!(plan.cwd, today.cwd);
        assert_eq!(plan.env, today.env);
        let mut expected = today.args.clone();
        let id_at = expected.len() - 2;
        expected[id_at] = chosen(&plan).to_owned();
        assert_eq!(plan.args, expected);
    }

    #[test]
    fn a_model_goes_before_the_prompt() {
        let f = fixture();
        let mut req = request(LaunchRole::TicketAgent);
        req.model = Some("opus".into());
        let plan = f.adapter.launch(&req).unwrap();
        let n = plan.args.len();
        assert_eq!(
            plan.args[n - 3..],
            [
                "--model".to_owned(),
                "opus".to_owned(),
                launch::skill_reference(crate::agent_tools::WORK_TICKET_SKILL)
            ]
        );
    }

    #[test]
    fn without_the_plugin_a_launch_fails_with_todays_reason() {
        let mut f = fixture();
        assert_eq!(f.adapter.unavailable(), None);
        f.adapter.context.launch = None;
        assert_eq!(f.adapter.unavailable(), Some(plugin_missing()));
        let error = f
            .adapter
            .launch(&request(LaunchRole::TicketAgent))
            .unwrap_err();
        assert!(
            error.0.contains("Claude Code plugin wasn't found"),
            "{error}"
        );
    }

    #[test]
    fn the_one_off_question_runs_the_launch_program() {
        let mut f = fixture();
        let question = OneOffRequest {
            cwd: "/r/shop-web".into(),
            prompt: "Suggest".into(),
            model: Some("opus".into()),
        };
        let command = f.adapter.one_off(&question).unwrap();
        assert_eq!(command.program, "claude");
        assert_eq!(
            command.args[command.args.len() - 3..],
            ["--model", "opus", "Suggest"]
        );
        f.adapter.context.launch = None;
        assert_eq!(
            f.adapter.one_off(&question).unwrap_err().0,
            "Claude Code isn't available to the background service"
        );
    }

    #[test]
    fn hooks_become_otaqs_events() {
        let f = fixture();
        let map = |event: &str, payload: Value| {
            f.adapter
                .map_hook(event, &payload, SignalContext::default())
        };
        assert_eq!(
            map(
                "Notification",
                json!({"notification_type": "permission_prompt", "message": "Run command: ls", "agent_id": "a1"})
            ),
            [ToolEvent::NeedsYou {
                prompt: "Run command: ls".into(),
                helper_agent: true
            }]
        );
        assert_eq!(
            map("Notification", json!({"notification_type": "idle_prompt"})),
            []
        );
        assert_eq!(
            map(
                "Stop",
                json!({"last_assistant_message": "Done.\n- all green"})
            ),
            [ToolEvent::TurnEnded {
                background: BackgroundWork::default(),
                last_message: Some("Done.\n- all green".into())
            }]
        );
        assert_eq!(
            map("UserPromptSubmit", json!({"prompt": "hi"})),
            [ToolEvent::NewTurn {
                prompt: Some("hi".into())
            }]
        );
        assert_eq!(
            map("SessionStart", json!({})),
            [ToolEvent::Unrecognised {
                event: "SessionStart".into()
            }]
        );
    }

    #[test]
    fn status_line_windows() {
        let f = fixture();
        let windows = f
            .adapter
            .windows_from_report(&json!({"rate_limits": {
                "five_hour": {"used_percentage": 82.0, "resets_at": 1_760_000_000},
                "seven_day": {"used_percentage": 40.5, "resets_at": 1_760_500_000}
            }}))
            .unwrap();
        assert_eq!(
            windows,
            [
                LimitWindow {
                    key: "five_hour".into(),
                    length_minutes: Some(300),
                    used_percent: 82.0,
                    resets_at: Some(1_760_000_000_000),
                    credits: None
                },
                LimitWindow {
                    key: "seven_day".into(),
                    length_minutes: Some(10_080),
                    used_percent: 40.5,
                    resets_at: Some(1_760_500_000_000),
                    credits: None
                },
            ]
        );
        assert_eq!(f.adapter.windows_from_report(&json!({"model": {}})), None);
        assert_eq!(
            f.adapter.limit_window_key(LimitKind::Weekly),
            Some("seven_day")
        );
    }

    /// Claude Code has no Otaq-only entries: removal removes nothing (d0086).
    #[test]
    fn setup_entries_and_removal() {
        let f = fixture();
        assert_eq!(
            f.adapter.setup_entries(),
            [SetupEntry {
                text: "trust for your projects".into(),
                removed_on_disconnect: false
            }]
        );
        assert_eq!(f.adapter.remove_setup(), SetupReport::default());
        assert_eq!(f.adapter.record_project(Path::new("/repo")), None);
    }

    /// With a (temporary) config file, the adapter records the trust; removal leaves the file as it is.
    #[test]
    fn the_setup_records_the_trust_and_removal_touches_no_file() {
        let claude = tempfile::tempdir().unwrap();
        let config = ClaudeConfig::locate(Some(claude.path().into()), None).unwrap();
        let mut f = fixture();
        f.adapter.context.trust = Some(config.clone());
        let repo = Path::new("/Users/me/pocket-notes");

        assert_eq!(f.adapter.record_project(repo), Some(SetupOutcome::Recorded));
        assert_eq!(config.is_trusted(&trust::trust_key(repo)), Ok(true));
        assert_eq!(
            f.adapter.record_project(repo),
            Some(SetupOutcome::AlreadyTrusted)
        );

        let bytes = std::fs::read(&config.file).unwrap();
        let modified = std::fs::metadata(&config.file).unwrap().modified().unwrap();
        assert_eq!(f.adapter.remove_setup(), SetupReport::default());
        assert_eq!(std::fs::read(&config.file).unwrap(), bytes);
        assert_eq!(
            std::fs::metadata(&config.file).unwrap().modified().unwrap(),
            modified
        );
        assert_eq!(std::fs::read_dir(claude.path()).unwrap().count(), 1);
    }

    #[test]
    fn without_a_claude_folder_tokens_are_unknown() {
        let f = fixture();
        let TokenSource::Reader(mut reader) = f.adapter.token_source("id", Path::new("/wt")) else {
            panic!("Claude Code's tokens come from its transcript");
        };
        assert_eq!(reader.read(), Err(TokensUnknown::Missing));
        assert!(reader.steps().is_empty());
        // As before M20: without a Claude folder the stored id is enough to resume.
        assert!(f.adapter.can_resume("id", Path::new("/wt")));
    }

    /// With a Claude folder, only a conversation whose transcript is there can be resumed.
    #[test]
    fn a_conversation_can_be_resumed_while_its_transcript_is_there() {
        let claude = tempfile::tempdir().unwrap();
        let mut f = fixture();
        f.adapter.context.check.claude_config_dir = Some(claude.path().into());
        let cwd = Path::new("/wt/shop-web/SHOP-18");
        assert!(!f.adapter.can_resume("c-1", cwd));
        let dir = claude.path().join("projects").join("-wt-shop-web-SHOP-18");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("c-1.jsonl"), "{}\n").unwrap();
        assert!(f.adapter.can_resume("c-1", cwd));
        assert!(!f.adapter.can_resume("c-2", cwd));
    }

    /// With a (temporary) Claude Code folder, the adapter reads a conversation's tokens with Claude's mapping and
    /// its last entries for "Before the restart".
    #[test]
    fn tokens_and_last_entries_come_from_the_saved_conversation() {
        let claude = tempfile::tempdir().unwrap();
        let mut f = fixture();
        f.adapter.context.check.claude_config_dir = Some(claude.path().to_owned());
        let (cwd, id) = (Path::new("/wt/shop-web/SHOP-18"), "0199a1b2-3c4d-7e5f");
        let file = transcript::transcript_path(claude.path(), cwd, id);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(
            &file,
            concat!(
                r#"{"type":"user","message":{"content":"Add a README line"}}"#,
                "\n",
                r#"{"type":"assistant","message":{"id":"msg_1","content":[{"type":"text","text":"Done."}],"usage":{"input_tokens":2,"cache_creation_input_tokens":500,"cache_read_input_tokens":9000,"output_tokens":40}}}"#,
                "\n"
            ),
        )
        .unwrap();

        let TokenSource::Reader(mut reader) = f.adapter.token_source(id, cwd) else {
            panic!("Claude Code's tokens come from its transcript");
        };
        assert_eq!(reader.read(), Ok(true));
        let steps = reader.steps();
        assert_eq!(steps.len(), 1);
        assert_eq!(steps[0].id, "msg_1");
        assert_eq!(
            steps[0].tokens,
            crate::usage::Tokens {
                input: 502,
                cached: 9000,
                output: 40,
                reasoning: None
            }
        );
        assert_eq!(reader.read(), Ok(false));

        assert_eq!(
            f.adapter.last_entries(id, cwd, 10),
            Some(vec![
                TranscriptEntry::User {
                    text: "Add a README line".into(),
                    at: None
                },
                TranscriptEntry::Assistant {
                    text: "Done.".into(),
                    at: None
                },
            ])
        );
        assert!(f.adapter.can_resume(id, cwd));
        assert_eq!(f.adapter.last_entries("0199ffff", cwd, 10), None);
    }

    #[tokio::test]
    async fn detect_without_claude_on_path() {
        let detection = fixture().adapter.detect().await;
        assert_eq!(detection, Detection::default());
    }

    #[test]
    fn the_registry_returns_the_claude_adapter() {
        let data = tempfile::tempdir().unwrap();
        let tools = crate::agent_tools::ToolContext {
            launch: None,
            path: OsString::new(),
            home: None,
            data: DataDir::new(data.path()),
        };
        let adapters = Adapters::new(fixture().adapter, tools);
        let copy = adapters.clone();
        assert_eq!(copy.get(Tool::Claude).tool(), Tool::Claude);
        assert_eq!(adapters.get(Tool::Claude).display_name(), "Claude Code");
    }
}
