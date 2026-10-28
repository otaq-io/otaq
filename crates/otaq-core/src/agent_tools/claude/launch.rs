//! Claude Code's command line (SES-03, SES-04; d0009, d0011, d0031), the Claude Code adapter's launch and resume.
//!
//! A ticket's agent is the real interactive `claude` CLI, started in the ticket's worktree:
//!
//! ```text
//! OTAQ_TICKET=SHOP-18  PORT=<agent port>  PATH=<otaq bin dir>:$PATH
//! claude --plugin-dir <otaq plugin>/claude --allowedTools "Bash(otaq:*)" --settings <status-line settings>
//!        --session-id <uuid> "/otaq-work-ticket"
//! ```
//!
//! `<otaq plugin>/claude` is Otaq's Claude Code plugin with its hooks only: Claude Code always loads a plugin's
//! `skills/` folder too (the plugin manifest reference, code.claude.com/docs/en/plugins-reference: "`skills` … Adds
//! to the default `skills/` scan"), so the shipped skills live outside it and reach the agent as copies in its
//! working folder, `.claude/skills/otaq-<name>/`, run as `/otaq-<name>` (AGM-07).
//!
//! **No permission mode or other permission flag**: the user's own Claude Code settings apply (d0011). The only
//! pre-approval is `Bash(otaq:*)`. `--settings` points at Otaq's file in its data folder whose only key is
//! `statusLine` ([`super::statusline`], USG-04); it is left out when that file couldn't be written.
//!
//! The Claude session ID (the tool id) is chosen by Otaq and passed with `--session-id` (the installed CLI
//! supports it, checked with `claude --help`, 2.1.296), so it is known and stored before the session starts;
//! Resume, Park and "Before the restart" use it (d0045). [`resume_agent`] builds `claude --resume <id>` with the
//! same environment and flags.

use std::path::{Path, PathBuf};

#[cfg(test)]
use crate::agent_tools::WORK_TICKET_SKILL;
use crate::agent_tools::{LaunchContext, PROJECT_AGENT_SKILL};
use crate::domain::TicketKey;
use crate::naming;

/// The Claude Code adapter's own part of the [`LaunchContext`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClaudeLaunch {
    /// The program and any leading arguments that stand for `claude` (`["claude"]`; tests substitute an
    /// ordinary command).
    pub program: Vec<String>,
    /// The user's Claude Code folder (`$CLAUDE_CONFIG_DIR` or `~/.claude`), read only to find their status line
    /// (USG-04). `None` reads no user settings (tests).
    pub config_dir: Option<PathBuf>,
}

impl ClaudeLaunch {
    /// `claude` from PATH and the user's Claude Code folder.
    pub fn from_process() -> Self {
        Self {
            program: vec![super::command_name().to_owned()],
            config_dir: crate::tools::CheckEnv::from_process().claude_config_dir,
        }
    }
}

/// The folder of Otaq's Claude Code plugin (its hooks) inside Otaq's plugin folder.
const PLUGIN_FOLDER: &str = "claude";

/// The plugin folder: `<bin>/plugin` (a runtime copy, UPD-02), `<bin>/../share/otaq/plugin`,
/// `<bin>/../Resources/plugin` (`Otaq.app`, d0072), `<bin>/../lib/Otaq/plugin` (where Tauri puts a Linux `.deb`'s and
/// AppImage's resources, d0140 gap review C2), or the repository's `plugin/` (dev builds run from `target/`). A
/// folder counts when it has `claude/.claude-plugin/plugin.json`. The repository comes last, so an installed build
/// never uses it.
pub fn find_plugin_dir(bin_dir: &Path) -> Option<PathBuf> {
    let repo_plugin = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .map(|root| root.join("plugin"));
    [
        Some(bin_dir.join("plugin")),
        bin_dir.parent().map(|prefix| {
            prefix
                .join("share")
                .join(naming::COMMAND_NAME)
                .join("plugin")
        }),
        bin_dir
            .parent()
            .map(|contents| contents.join("Resources").join("plugin")),
        bin_dir
            .parent()
            .map(|prefix| prefix.join("lib").join(naming::PRODUCT_NAME).join("plugin")),
        repo_plugin,
    ]
    .into_iter()
    .flatten()
    .find(|dir| {
        dir.join(PLUGIN_FOLDER)
            .join(".claude-plugin")
            .join("plugin.json")
            .is_file()
    })
}

/// A skill as Claude Code runs a project skill: `/otaq-work-ticket` (AGM-07).
pub(super) fn skill_reference(skill: &str) -> String {
    format!("/{}", naming::copy_name(skill))
}

/// A command line, which the adapter turns into its [`crate::agent_tools::LaunchPlan`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct AgentCommand {
    pub program: String,
    pub args: Vec<String>,
    pub cwd: PathBuf,
    pub env: Vec<(String, String)>,
    /// The tool session id the conversation uses (`--session-id` or `--resume`).
    pub tool_session_id: String,
}

/// A new agent session for `ticket` in `worktree`, with a fresh Claude session ID and the work-ticket skill:
/// [`launch_with_prompt`], as the adapter builds it. `settings` is Otaq's status-line settings file.
#[cfg(test)]
pub(super) fn launch_agent(
    ctx: &LaunchContext,
    ticket: &TicketKey,
    worktree: &Path,
    agent_port: u16,
    settings: Option<&Path>,
) -> AgentCommand {
    launch_with_prompt(
        ctx,
        ticket,
        worktree,
        agent_port,
        skill_reference(WORK_TICKET_SKILL),
        settings,
    )
}

/// A new conversation for `ticket` in `cwd` with `prompt` as its first prompt: a delivery session runs the Open PR
/// or Push skill this way, an answer session's fallback too.
pub(super) fn launch_with_prompt(
    ctx: &LaunchContext,
    ticket: &TicketKey,
    cwd: &Path,
    port: u16,
    prompt: String,
    settings: Option<&Path>,
) -> AgentCommand {
    let session_id = uuid::Uuid::now_v7().to_string();
    let mut args = common_flags(ctx, settings);
    args.extend(["--session-id".to_owned(), session_id.clone(), prompt]);
    command(ctx, ticket, cwd, port, args, session_id)
}

/// A ticket's validator (VAL-01, d0063): a new conversation in the ticket's worktree with the project's Validate
/// skill as its first prompt, the ticket and the validator role in its environment, and no `PORT` (it runs no
/// dev server). The same plugin, pre-approval, status line and no permission flags as the agent (d0011).
pub(super) fn launch_validator(
    ctx: &LaunchContext,
    ticket: &TicketKey,
    worktree: &Path,
    skill: &str,
    settings: Option<&Path>,
) -> AgentCommand {
    let session_id = uuid::Uuid::now_v7().to_string();
    let mut args = common_flags(ctx, settings);
    args.extend([
        "--session-id".to_owned(),
        session_id.clone(),
        skill.to_owned(),
    ]);
    let mut command = command(ctx, ticket, worktree, 0, args, session_id);
    command.env.retain(|(name, _)| name != "PORT");
    command
        .env
        .push((naming::role_env_var(), naming::ROLE_VALIDATOR.to_owned()));
    command
}

/// A new Project agent (PAG-01, d0068): a conversation in the project's own folder (the user's checkout) with
/// `/otaq-noyan` as its first prompt and the Project agent role in its environment. No ticket, no
/// `PORT`. The same plugin, pre-approval, status line and no permission flags as the agent (d0011). The adapter
/// puts the user's first message after the skill (`/otaq-noyan <message>`, d0087).
pub(super) fn launch_project_agent(
    ctx: &LaunchContext,
    repo: &Path,
    settings: Option<&Path>,
) -> AgentCommand {
    let session_id = uuid::Uuid::now_v7().to_string();
    let mut args = common_flags(ctx, settings);
    args.extend([
        "--session-id".to_owned(),
        session_id.clone(),
        skill_reference(PROJECT_AGENT_SKILL),
    ]);
    project_agent_command(ctx, repo, args, session_id)
}

/// Continues a Project agent's conversation `session_id` (Resume after it ended, d0068) with the same
/// environment and flags.
pub(super) fn resume_project_agent(
    ctx: &LaunchContext,
    repo: &Path,
    session_id: &str,
    settings: Option<&Path>,
) -> AgentCommand {
    let mut args = common_flags(ctx, settings);
    args.extend(["--resume".to_owned(), session_id.to_owned()]);
    project_agent_command(ctx, repo, args, session_id.to_owned())
}

fn project_agent_command(
    ctx: &LaunchContext,
    repo: &Path,
    flags: Vec<String>,
    tool_session_id: String,
) -> AgentCommand {
    let (program, leading, path) = program_and_path(ctx);
    AgentCommand {
        program,
        args: leading.into_iter().chain(flags).collect(),
        cwd: repo.to_path_buf(),
        env: vec![
            ("PATH".to_owned(), path),
            (
                naming::role_env_var(),
                naming::ROLE_PROJECT_AGENT.to_owned(),
            ),
        ],
        tool_session_id,
    }
}

/// Continues the stored conversation `session_id` (Resume, Park, d0045, d0052) with the same environment and
/// flags.
#[cfg(test)]
pub(super) fn resume_agent(
    ctx: &LaunchContext,
    ticket: &TicketKey,
    worktree: &Path,
    agent_port: u16,
    session_id: &str,
) -> AgentCommand {
    resume_agent_with(ctx, ticket, worktree, agent_port, session_id, None, None)
}

/// Like [`resume_agent`], with a first prompt for the resumed conversation: the skill a Send back or Send to
/// agent to fix asked for (`claude --resume <id> "/otaq-address-review"`), so it runs as soon as the
/// conversation is loaded instead of being typed into a terminal that may not be ready yet.
pub(super) fn resume_agent_with(
    ctx: &LaunchContext,
    ticket: &TicketKey,
    worktree: &Path,
    agent_port: u16,
    session_id: &str,
    prompt: Option<String>,
    settings: Option<&Path>,
) -> AgentCommand {
    let mut args = common_flags(ctx, settings);
    args.extend(["--resume".to_owned(), session_id.to_owned()]);
    args.extend(prompt);
    command(
        ctx,
        ticket,
        worktree,
        agent_port,
        args,
        session_id.to_owned(),
    )
}

/// `--plugin-dir <plugin>/claude --allowedTools "Bash(otaq:*)" [--settings <file>]`: Otaq's hooks plugin, the one
/// pre-approval (d0011) and Otaq's status line (USG-04).
fn common_flags(ctx: &LaunchContext, settings: Option<&Path>) -> Vec<String> {
    let mut flags = vec![
        "--plugin-dir".to_owned(),
        ctx.plugin_dir.join(PLUGIN_FOLDER).display().to_string(),
        "--allowedTools".to_owned(),
        format!("Bash({}:*)", naming::COMMAND_NAME),
    ];
    if let Some(settings) = settings {
        flags.extend(["--settings".to_owned(), settings.display().to_string()]);
    }
    flags
}

fn command(
    ctx: &LaunchContext,
    ticket: &TicketKey,
    worktree: &Path,
    agent_port: u16,
    flags: Vec<String>,
    tool_session_id: String,
) -> AgentCommand {
    let (program, leading, path) = program_and_path(ctx);
    AgentCommand {
        program,
        args: leading.into_iter().chain(flags).collect(),
        cwd: worktree.to_path_buf(),
        env: vec![
            (naming::ticket_env_var(), ticket.to_string()),
            ("PORT".to_owned(), agent_port.to_string()),
            ("PATH".to_owned(), path),
        ],
        tool_session_id,
    }
}

/// The program standing for `claude`, its leading arguments, and the PATH with the `otaq` folder first.
fn program_and_path(ctx: &LaunchContext) -> (String, Vec<String>, String) {
    let (program, leading) = ctx
        .claude
        .program
        .split_first()
        .map(|(p, rest)| (p.clone(), rest.to_vec()))
        .unwrap_or_else(|| (super::command_name().to_owned(), Vec::new()));
    let path = crate::shell::prepend_path(Some(&ctx.bin_dir), &ctx.path);
    (program, leading, path.to_string_lossy().into_owned())
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;

    use super::*;

    fn ctx() -> LaunchContext {
        LaunchContext {
            bin_dir: "/opt/cb/bin".into(),
            plugin_dir: "/opt/cb/plugin".into(),
            path: platform_path(&["/usr/bin", "/bin"]),
            claude: ClaudeLaunch {
                program: vec!["claude".into()],
                config_dir: None,
            },
        }
    }

    /// A PATH value with the platform's separator.
    fn platform_path(dirs: &[&str]) -> OsString {
        std::env::join_paths(dirs).unwrap()
    }

    fn key() -> TicketKey {
        "SHOP-18".parse().unwrap()
    }

    /// The validator (T-0183, VAL-01): the Validate skill as its prompt, the role, the ticket, no `PORT`, no
    /// permission flags.
    #[test]
    fn validator_launch() {
        let cmd = launch_validator(
            &ctx(),
            &key(),
            Path::new("/wt/shop-web/SHOP-18"),
            "/my-validate",
            Some(Path::new("/home/me/.data/claude/SHOP-18.settings.json")),
        );
        let name = naming::COMMAND_NAME;
        assert_eq!(
            cmd.args,
            [
                "--plugin-dir".to_owned(),
                "/opt/cb/plugin/claude".to_owned(),
                "--allowedTools".to_owned(),
                format!("Bash({name}:*)"),
                "--settings".to_owned(),
                "/home/me/.data/claude/SHOP-18.settings.json".to_owned(),
                "--session-id".to_owned(),
                cmd.tool_session_id.clone(),
                "/my-validate".to_owned(),
            ]
        );
        assert!(
            !cmd.args.iter().any(|a| a.contains("permission")),
            "no permission flags (d0011)"
        );
        assert_eq!(cmd.cwd, Path::new("/wt/shop-web/SHOP-18"));
        assert_eq!(
            cmd.env,
            [
                (naming::ticket_env_var(), "SHOP-18".to_owned()),
                (
                    "PATH".to_owned(),
                    platform_path(&["/opt/cb/bin", "/usr/bin", "/bin"])
                        .to_string_lossy()
                        .into_owned()
                ),
                (naming::role_env_var(), naming::ROLE_VALIDATOR.to_owned()),
            ]
        );
    }

    /// A Project agent (T-0199, PAG-01): in the project's folder, the role, the skill, no ticket and no `PORT`,
    /// no permission flags; Resume continues the same conversation with the same environment.
    #[test]
    fn project_agent_launch() {
        let settings = Path::new("/home/me/.data/claude/SHOP.project-agent.light.settings.json");
        let cmd = launch_project_agent(&ctx(), Path::new("/src/shop-web"), Some(settings));
        let name = naming::COMMAND_NAME;
        assert_eq!(cmd.program, "claude");
        assert_eq!(
            cmd.args,
            [
                "--plugin-dir".to_owned(),
                "/opt/cb/plugin/claude".to_owned(),
                "--allowedTools".to_owned(),
                format!("Bash({name}:*)"),
                "--settings".to_owned(),
                settings.display().to_string(),
                "--session-id".to_owned(),
                cmd.tool_session_id.clone(),
                format!("/{}-noyan", naming::COMMAND_NAME),
            ]
        );
        assert!(!cmd.args.iter().any(|a| a.contains("permission")));
        assert_eq!(cmd.cwd, Path::new("/src/shop-web"));
        let path = platform_path(&["/opt/cb/bin", "/usr/bin", "/bin"])
            .to_string_lossy()
            .into_owned();
        assert_eq!(
            cmd.env,
            [
                ("PATH".to_owned(), path),
                (
                    naming::role_env_var(),
                    naming::ROLE_PROJECT_AGENT.to_owned()
                ),
            ]
        );
        assert!(
            !cmd.env
                .iter()
                .any(|(k, _)| *k == naming::ticket_env_var() || k == "PORT"),
            "no ticket, no port"
        );

        let resume =
            resume_project_agent(&ctx(), Path::new("/src/shop-web"), "c-1", Some(settings));
        assert_eq!(resume.env, cmd.env);
        assert_eq!(resume.args[..6], cmd.args[..6]);
        assert_eq!(resume.args[6..], ["--resume".to_owned(), "c-1".to_owned()]);
        assert_eq!(resume.tool_session_id, "c-1");
    }

    #[test]
    fn launch_is_exactly_the_documented_command() {
        let cmd = launch_agent(
            &ctx(),
            &key(),
            Path::new("/wt/shop-web/SHOP-18"),
            4019,
            Some(Path::new("/home/me/.data/claude/SHOP-18.settings.json")),
        );
        assert_eq!(cmd.program, "claude");
        let session = cmd.tool_session_id.clone();
        assert!(uuid::Uuid::parse_str(&session).is_ok());
        let name = naming::COMMAND_NAME;
        assert_eq!(
            cmd.args,
            [
                "--plugin-dir".to_owned(),
                "/opt/cb/plugin/claude".to_owned(),
                "--allowedTools".to_owned(),
                format!("Bash({name}:*)"),
                "--settings".to_owned(),
                "/home/me/.data/claude/SHOP-18.settings.json".to_owned(),
                "--session-id".to_owned(),
                session,
                format!("/{}-work-ticket", naming::COMMAND_NAME),
            ]
        );
        assert_eq!(cmd.cwd, Path::new("/wt/shop-web/SHOP-18"));
        assert_eq!(
            cmd.env,
            [
                (naming::ticket_env_var(), "SHOP-18".to_owned()),
                ("PORT".to_owned(), "4019".to_owned()),
                (
                    "PATH".to_owned(),
                    platform_path(&["/opt/cb/bin", "/usr/bin", "/bin"])
                        .to_string_lossy()
                        .into_owned()
                ),
            ]
        );
    }

    #[test]
    fn each_launch_gets_its_own_session_id() {
        let a = launch_agent(&ctx(), &key(), Path::new("/wt"), 4019, None);
        let b = launch_agent(&ctx(), &key(), Path::new("/wt"), 4019, None);
        assert_ne!(a.tool_session_id, b.tool_session_id);
    }

    #[test]
    fn resume_uses_the_stored_session_with_the_same_env_and_flags() {
        let id = "0199d2a4-3b1c-7c3e-9f00-123456789abc";
        let launch = launch_agent(&ctx(), &key(), Path::new("/wt"), 4019, None);
        let resume = resume_agent(&ctx(), &key(), Path::new("/wt"), 4019, id);
        assert_eq!(resume.env, launch.env);
        assert_eq!(resume.args[..4], launch.args[..4]);
        assert_eq!(resume.args[4..], ["--resume".to_owned(), id.to_owned()]);
        assert_eq!(resume.tool_session_id, id);
    }

    #[test]
    fn no_permission_flags_ever() {
        let forbidden = [
            "--permission-mode",
            "--dangerously-skip-permissions",
            "--allow-dangerously-skip-permissions",
            "--permission-prompt-tool",
            "--disallowedTools",
        ];
        let settings = Path::new("/data/claude/SHOP-18.settings.json");
        let launch = launch_agent(&ctx(), &key(), Path::new("/wt"), 4019, Some(settings));
        let resume = resume_agent_with(
            &ctx(),
            &key(),
            Path::new("/wt"),
            4019,
            "id",
            None,
            Some(settings),
        );
        for cmd in [launch, resume] {
            // `--settings` only ever carries Otaq's status-line file (its content is tested in
            // `statusline`: one key, `statusLine`).
            let passed: Vec<_> = cmd
                .args
                .windows(2)
                .filter(|w| w[0] == "--settings")
                .map(|w| w[1].clone())
                .collect();
            assert_eq!(passed, [settings.display().to_string()]);
            for arg in &cmd.args {
                assert!(
                    !forbidden.iter().any(|f| arg.starts_with(f)),
                    "{arg} must never be passed (d0011)"
                );
            }
            // The only pre-approval is Otaq's own command.
            let allowed: Vec<_> = cmd
                .args
                .windows(2)
                .filter(|w| w[0] == "--allowedTools")
                .map(|w| w[1].clone())
                .collect();
            assert_eq!(allowed, [format!("Bash({}:*)", naming::COMMAND_NAME)]);
        }
    }

    #[test]
    fn a_substituted_program_keeps_its_leading_arguments() {
        let mut ctx = ctx();
        ctx.claude.program = vec!["sh".into(), "-c".into(), "echo".into(), "sh".into()];
        ctx.path = OsString::new();
        let cmd = launch_agent(&ctx, &key(), Path::new("/wt"), 1, None);
        assert_eq!(cmd.program, "sh");
        assert_eq!(cmd.args[..3], ["-c", "echo", "sh"]);
        assert_eq!(cmd.args[3], "--plugin-dir");
        assert_eq!(cmd.env[2].1, "/opt/cb/bin", "no trailing colon");
    }

    #[test]
    fn the_plugin_is_found_in_the_bundle_before_the_repository() {
        let dir = tempfile::tempdir().unwrap();
        let contents = dir
            .path()
            .join(naming::bundle_folder_name())
            .join("Contents");
        let macos = contents.join("MacOS");
        let plugin = contents.join("Resources").join("plugin");
        std::fs::create_dir_all(&macos).unwrap();
        let manifest = plugin.join(PLUGIN_FOLDER).join(".claude-plugin");
        std::fs::create_dir_all(&manifest).unwrap();
        std::fs::write(manifest.join("plugin.json"), "{}").unwrap();
        assert_eq!(find_plugin_dir(&macos), Some(plugin));
    }

    /// A Linux `.deb` or AppImage: `/usr/bin/otaq` with Tauri's resources in `/usr/lib/Otaq/plugin`.
    #[test]
    fn the_plugin_is_found_in_the_linux_resource_folder() {
        let dir = tempfile::tempdir().unwrap();
        let usr = dir.path().join("usr");
        let bin = usr.join("bin");
        let plugin = usr.join("lib").join(naming::PRODUCT_NAME).join("plugin");
        std::fs::create_dir_all(&bin).unwrap();
        let manifest = plugin.join(PLUGIN_FOLDER).join(".claude-plugin");
        std::fs::create_dir_all(&manifest).unwrap();
        std::fs::write(manifest.join("plugin.json"), "{}").unwrap();
        assert_eq!(find_plugin_dir(&bin), Some(plugin));
    }

    #[test]
    fn the_repository_plugin_is_found_in_dev_builds() {
        let dir = find_plugin_dir(Path::new("/nonexistent/bin")).expect("repo plugin");
        assert!(dir.ends_with("plugin"));
        assert!(dir.join("claude/.claude-plugin/plugin.json").is_file());
        assert!(dir.join("skills/work-ticket/SKILL.md").is_file());
    }

    /// Every role through the adapter (0088 launch tests): the exact program, arguments, folder and environment.
    mod through_the_adapter {
        use super::*;
        use crate::agent_tools::claude::{ClaudeCode, ClaudeContext, statusline};
        use crate::agent_tools::{
            AgentTool, FirstPrompt, LaunchPlan, LaunchRequest, LaunchRole, ToolIdMode,
        };
        use crate::data_dir::DataDir;

        const WORKTREE: &str = "/wt/shop-web/SHOP-18";
        const REPO: &str = "/src/shop-web";
        const RESUMED: &str = "0199d2a4-3b1c-7c3e-9f00-123456789abc";

        struct Fixture {
            _home: tempfile::TempDir,
            data: DataDir,
            adapter: ClaudeCode,
        }

        fn fixture() -> Fixture {
            let home = tempfile::tempdir().unwrap();
            let data = DataDir::new(home.path().join("data"));
            let adapter = ClaudeCode::new(ClaudeContext {
                launch: Some(ctx()),
                check: crate::tools::CheckEnv {
                    path: OsString::new(),
                    claude_config_dir: None,
                    managed_settings_dir: home.path().join("managed"),
                },
                data: data.clone(),
                trust: None,
            });
            Fixture {
                _home: home,
                data,
                adapter,
            }
        }

        fn skill(name: &str, message: Option<&str>) -> Option<FirstPrompt> {
            Some(FirstPrompt::Skill {
                name: name.to_owned(),
                message: message.map(str::to_owned),
            })
        }

        fn ticket_request(role: LaunchRole, first_prompt: Option<FirstPrompt>) -> LaunchRequest {
            LaunchRequest {
                role,
                cwd: WORKTREE.into(),
                project: "SHOP".into(),
                ticket: Some(key()),
                port: 4012,
                first_prompt,
                resume: None,
                model: None,
                title: None,
                effort: None,
                theme: crate::board::settings::Theme::OtaqDark,
            }
        }

        fn project_request(role: LaunchRole, first_prompt: Option<FirstPrompt>) -> LaunchRequest {
            LaunchRequest {
                role,
                cwd: REPO.into(),
                project: "SHOP".into(),
                ticket: None,
                port: 0,
                first_prompt,
                resume: None,
                model: None,
                title: None,
                effort: None,
                theme: crate::board::settings::Theme::OtaqDark,
            }
        }

        fn path() -> String {
            platform_path(&["/opt/cb/bin", "/usr/bin", "/bin"])
                .to_string_lossy()
                .into_owned()
        }

        /// A plan's environment: `pairs`, then the variable that keeps Claude saving the conversation, which every
        /// launch and resume ends with ([`crate::agent_tools::claude::KEEP_TRANSCRIPTS`]).
        fn env(pairs: &[(String, &str)]) -> Vec<(String, String)> {
            let (name, value) = crate::agent_tools::claude::KEEP_TRANSCRIPTS;
            pairs
                .iter()
                .map(|(k, v)| (k.clone(), (*v).to_owned()))
                .chain([(name.to_owned(), value.to_owned())])
                .collect()
        }

        /// `--plugin-dir … --allowedTools "Bash(otaq:*)" --settings <file>` and then `rest`.
        fn args(settings: &Path, rest: &[&str]) -> Vec<String> {
            [
                "--plugin-dir".to_owned(),
                "/opt/cb/plugin/claude".to_owned(),
                "--allowedTools".to_owned(),
                format!("Bash({}:*)", naming::COMMAND_NAME),
                "--settings".to_owned(),
                settings.display().to_string(),
            ]
            .into_iter()
            .chain(rest.iter().map(|a| (*a).to_owned()))
            .collect()
        }

        fn new_id(plan: &LaunchPlan) -> String {
            let ToolIdMode::ChosenUpFront(id) = &plan.tool_id else {
                panic!(
                    "Claude Code's tool id is chosen up front: {:?}",
                    plan.tool_id
                );
            };
            assert_eq!(uuid::Uuid::parse_str(id).unwrap().get_version_num(), 7);
            id.clone()
        }

        fn otaq_skill(name: &str) -> String {
            format!("/{}", naming::copy_name(name))
        }

        #[test]
        fn a_ticket_agent() {
            let f = fixture();
            let plan = f
                .adapter
                .launch(&ticket_request(
                    LaunchRole::TicketAgent,
                    skill(WORK_TICKET_SKILL, None),
                ))
                .unwrap();
            let settings =
                statusline::settings_path(&f.data, &key(), crate::agent_tools::ThemeKind::Dark);
            assert!(settings.is_file());
            let id = new_id(&plan);
            assert_eq!(plan.program, "claude");
            assert_eq!(
                plan.args,
                args(
                    &settings,
                    &["--session-id", &id, &otaq_skill("work-ticket")]
                )
            );
            assert_eq!(plan.cwd, Path::new(WORKTREE));
            assert_eq!(
                plan.env,
                env(&[
                    (naming::ticket_env_var(), "SHOP-18"),
                    ("PORT".to_owned(), "4012"),
                    ("PATH".to_owned(), &path()),
                ])
            );
        }

        #[test]
        fn a_validator_has_no_port() {
            let f = fixture();
            let mut request = ticket_request(
                LaunchRole::Validator,
                Some(FirstPrompt::Text("/my-validate".into())),
            );
            request.port = 0;
            let plan = f.adapter.launch(&request).unwrap();
            let settings =
                statusline::settings_path(&f.data, &key(), crate::agent_tools::ThemeKind::Dark);
            let id = new_id(&plan);
            assert_eq!(plan.program, "claude");
            assert_eq!(
                plan.args,
                args(&settings, &["--session-id", &id, "/my-validate"])
            );
            assert_eq!(plan.cwd, Path::new(WORKTREE));
            assert_eq!(
                plan.env,
                env(&[
                    (naming::ticket_env_var(), "SHOP-18"),
                    ("PATH".to_owned(), &path()),
                    (naming::role_env_var(), naming::ROLE_VALIDATOR),
                ])
            );
        }

        /// T-0318 (d0087): a Project agent starts on the user's first message, passed with the skill as one first
        /// prompt, never typed afterwards.
        #[test]
        fn a_project_agent_with_its_first_message_and_its_resume() {
            let f = fixture();
            let settings = statusline::project_agent_settings_path(
                &f.data,
                "SHOP",
                crate::agent_tools::ThemeKind::Dark,
            );
            let env = env(&[
                ("PATH".to_owned(), &path()),
                (naming::role_env_var(), naming::ROLE_PROJECT_AGENT),
            ]);
            let plan = f
                .adapter
                .launch(&project_request(
                    LaunchRole::ProjectAgent,
                    skill(
                        PROJECT_AGENT_SKILL,
                        Some("List the tickets that wait on NOTE-5"),
                    ),
                ))
                .unwrap();
            assert!(settings.is_file());
            let id = new_id(&plan);
            let prompt = format!(
                "{} List the tickets that wait on NOTE-5",
                otaq_skill("noyan")
            );
            assert_eq!(plan.program, "claude");
            assert_eq!(plan.args, args(&settings, &["--session-id", &id, &prompt]));
            assert_eq!(plan.cwd, Path::new(REPO));
            assert_eq!(plan.env, env);

            let mut resume = project_request(LaunchRole::ProjectAgent, None);
            resume.resume = Some(RESUMED.into());
            let plan = f.adapter.launch(&resume).unwrap();
            assert_eq!(plan.tool_id, ToolIdMode::ChosenUpFront(RESUMED.into()));
            assert_eq!(plan.args, args(&settings, &["--resume", RESUMED]));
            assert_eq!(plan.cwd, Path::new(REPO));
            assert_eq!(plan.env, env);
        }

        #[test]
        fn an_answer_resumes_with_the_answer_skill_and_no_port() {
            let f = fixture();
            let mut request = ticket_request(LaunchRole::Answer, skill("answer-question", None));
            request.port = 0;
            request.resume = Some(RESUMED.into());
            let plan = f.adapter.launch(&request).unwrap();
            let settings =
                statusline::settings_path(&f.data, &key(), crate::agent_tools::ThemeKind::Dark);
            assert_eq!(plan.tool_id, ToolIdMode::ChosenUpFront(RESUMED.into()));
            assert_eq!(plan.program, "claude");
            assert_eq!(
                plan.args,
                args(
                    &settings,
                    &["--resume", RESUMED, &otaq_skill("answer-question")]
                )
            );
            assert_eq!(plan.cwd, Path::new(WORKTREE));
            assert_eq!(
                plan.env,
                env(&[
                    (naming::ticket_env_var(), "SHOP-18"),
                    ("PORT".to_owned(), "0"),
                    ("PATH".to_owned(), &path()),
                ])
            );
        }

        #[test]
        fn a_delivery_runs_the_skill_as_stored() {
            let f = fixture();
            let ticket: TicketKey = "SHOP-40".parse().unwrap();
            let request = LaunchRequest {
                ticket: Some(ticket.clone()),
                cwd: "/wt/shop-web/SHOP-40".into(),
                port: 0,
                ..ticket_request(
                    LaunchRole::Delivery,
                    Some(FirstPrompt::Text("/my-open-pr".into())),
                )
            };
            let plan = f.adapter.launch(&request).unwrap();
            let settings =
                statusline::settings_path(&f.data, &ticket, crate::agent_tools::ThemeKind::Dark);
            let id = new_id(&plan);
            assert_eq!(plan.program, "claude");
            assert_eq!(
                plan.args,
                args(&settings, &["--session-id", &id, "/my-open-pr"])
            );
            assert_eq!(plan.cwd, Path::new("/wt/shop-web/SHOP-40"));
            assert_eq!(
                plan.env,
                env(&[
                    (naming::ticket_env_var(), "SHOP-40"),
                    ("PORT".to_owned(), "0"),
                    ("PATH".to_owned(), &path()),
                ])
            );
        }

        #[test]
        fn resume_with_and_without_a_prompt() {
            let f = fixture();
            let settings =
                statusline::settings_path(&f.data, &key(), crate::agent_tools::ThemeKind::Dark);
            let env = env(&[
                (naming::ticket_env_var(), "SHOP-18"),
                ("PORT".to_owned(), "4012"),
                ("PATH".to_owned(), &path()),
            ]);
            let mut request = ticket_request(LaunchRole::TicketAgent, None);
            request.resume = Some(RESUMED.into());
            let plan = f.adapter.launch(&request).unwrap();
            assert_eq!(plan.tool_id, ToolIdMode::ChosenUpFront(RESUMED.into()));
            assert_eq!(plan.args, args(&settings, &["--resume", RESUMED]));
            assert_eq!(plan.cwd, Path::new(WORKTREE));
            assert_eq!(plan.env, env);

            request.first_prompt = skill("address-review", None);
            let plan = f.adapter.launch(&request).unwrap();
            assert_eq!(
                plan.args,
                args(
                    &settings,
                    &["--resume", RESUMED, &otaq_skill("address-review")]
                )
            );
            assert_eq!(plan.env, env);

            request.first_prompt = Some(FirstPrompt::Text("Use the staging database.".into()));
            let plan = f.adapter.launch(&request).unwrap();
            assert_eq!(
                plan.args,
                args(
                    &settings,
                    &["--resume", RESUMED, "Use the staging database."]
                )
            );
        }

        /// No permission flag for any role, and the only pre-approval is Otaq's own command (d0011).
        #[test]
        fn no_permission_flag_and_one_pre_approval_for_every_role() {
            let f = fixture();
            let mut requests = vec![
                ticket_request(LaunchRole::TicketAgent, skill(WORK_TICKET_SKILL, None)),
                ticket_request(LaunchRole::Validator, Some(FirstPrompt::Text("/v".into()))),
                ticket_request(LaunchRole::Answer, skill("answer-question", None)),
                ticket_request(LaunchRole::Delivery, Some(FirstPrompt::Text("/p".into()))),
                project_request(LaunchRole::ProjectAgent, skill(PROJECT_AGENT_SKILL, None)),
            ];
            let resumed: Vec<_> = requests
                .iter()
                .cloned()
                .map(|mut r| {
                    r.resume = Some(RESUMED.into());
                    r
                })
                .collect();
            requests.extend(resumed);
            for request in requests {
                let plan = f.adapter.launch(&request).unwrap();
                for arg in &plan.args {
                    assert!(
                        !arg.contains("permission") && !arg.contains("dangerously"),
                        "{arg} must never be passed (d0011)"
                    );
                }
                let allowed: Vec<_> = plan
                    .args
                    .windows(2)
                    .filter(|w| w[0] == "--allowedTools")
                    .map(|w| w[1].clone())
                    .collect();
                assert_eq!(allowed, [format!("Bash({}:*)", naming::COMMAND_NAME)]);
            }
        }

        /// A model given goes to Claude Code as `--model <model>` before the prompt, on a launch and on every resume
        /// (the models report: Claude Code doesn't keep it); the services pass the named agent's or the session's.
        #[test]
        fn a_model_goes_before_the_prompt_or_last() {
            let f = fixture();
            let settings =
                statusline::settings_path(&f.data, &key(), crate::agent_tools::ThemeKind::Dark);
            let mut request =
                ticket_request(LaunchRole::TicketAgent, skill(WORK_TICKET_SKILL, None));
            request.model = Some("opus".into());
            let plan = f.adapter.launch(&request).unwrap();
            let id = new_id(&plan);
            assert_eq!(
                plan.args,
                args(
                    &settings,
                    &[
                        "--session-id",
                        &id,
                        "--model",
                        "opus",
                        &otaq_skill("work-ticket")
                    ]
                )
            );
            request.first_prompt = None;
            request.resume = Some(RESUMED.into());
            let plan = f.adapter.launch(&request).unwrap();
            assert_eq!(
                plan.args,
                args(&settings, &["--resume", RESUMED, "--model", "opus"])
            );
        }

        /// An agent's effort goes to Claude Code as `--effort <level>` after the model, before the prompt (AGM-01).
        #[test]
        fn an_effort_goes_after_the_model() {
            let f = fixture();
            let settings =
                statusline::settings_path(&f.data, &key(), crate::agent_tools::ThemeKind::Dark);
            let mut request =
                ticket_request(LaunchRole::TicketAgent, skill(WORK_TICKET_SKILL, None));
            request.model = Some("opus".into());
            request.effort = Some("high".into());
            let plan = f.adapter.launch(&request).unwrap();
            let id = new_id(&plan);
            assert_eq!(
                plan.args,
                args(
                    &settings,
                    &[
                        "--session-id",
                        &id,
                        "--model",
                        "opus",
                        "--effort",
                        "high",
                        &otaq_skill("work-ticket")
                    ]
                )
            );
            assert!(f.adapter.effort_levels().contains(&"xhigh"));
        }

        #[test]
        fn each_new_launch_gets_its_own_uuid_v7() {
            let f = fixture();
            let request = ticket_request(LaunchRole::TicketAgent, skill(WORK_TICKET_SKILL, None));
            let a = new_id(&f.adapter.launch(&request).unwrap());
            let b = new_id(&f.adapter.launch(&request).unwrap());
            assert_ne!(a, b);
        }
    }
}
