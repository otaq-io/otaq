//! Launch and resume (ADP-07, SES-03, ADP-04; d0086, d0088), the real interactive `codex` in the session's folder:
//!
//! ```text
//! OTAQ_TICKET=SHOP-18  PORT=<agent port>  PATH=<otaq bin dir>:$PATH
//! codex --no-daemon -c check_for_update_on_startup=false [-m <model>] "$otaq-work-ticket"
//! codex --no-daemon -c check_for_update_on_startup=false resume <id> [-m <model>] ["<prompt>"]
//! ```
//!
//! `--no-daemon` keeps the agent's commands under the session's own process (no shared background server, which
//! could own them, or be left behind); the `-c` override skips the update screen that would block the start. **No
//! approval, sandbox or bypass flag** (d0011): the user's own Codex settings apply; the `otaq` pre-approval and the
//! hooks come from the one-time setup (T-0351). The skills are project skills copied into the session's folder
//! (AGM-07: Codex has no launch option for a skill folder, the typed skill lines report of 2026-10-19), so the first
//! prompt only names one (`$otaq-work-ticket`).
//!
//! Codex can't take a chosen id: a new conversation's id is captured after start (its `SessionStart` hook, T-0350)
//! and the session's id stays empty until then; a resume names the stored id.

use std::path::{Path, PathBuf};

use super::{COMMAND_NAME, Codex, CodexContext};
use crate::agent_tools::{
    FirstPrompt, LaunchError, LaunchPlan, LaunchRequest, LaunchRole, ToolIdMode,
};
use crate::naming;

/// The flags of every launch, new or resumed: no background server, no update screen.
pub(super) const COMMON_FLAGS: [&str; 3] =
    ["--no-daemon", "-c", "check_for_update_on_startup=false"];

/// The subcommand that continues a conversation.
const RESUME: &str = "resume";

/// The model option.
const MODEL_FLAG: &str = "-m";

/// What Codex prints when `resume` names an id it has no session file for (`codex exec resume <id>`, 0.157.1:
/// "thread/resume failed: no rollout found for thread id <id>"). The TUI's text is checked by hand (T-0349).
const CONVERSATION_NOT_FOUND: &str = "no rollout found for thread id";

pub(super) fn launch(codex: &Codex, request: &LaunchRequest) -> Result<LaunchPlan, LaunchError> {
    let context = codex.context();
    let Some(launch) = &context.tool.launch else {
        return Err(launch_missing());
    };
    let prompt = request
        .first_prompt
        .as_ref()
        .map(|prompt| prompt_text(codex, prompt));
    let mut args: Vec<String> = COMMON_FLAGS.map(str::to_owned).to_vec();
    let tool_id = match &request.resume {
        Some(id) => {
            args.extend([RESUME.to_owned(), id.clone()]);
            ToolIdMode::ChosenUpFront(id.clone())
        }
        None => ToolIdMode::CapturedAfterStart,
    };
    if let Some(model) = &request.model {
        args.extend([MODEL_FLAG.to_owned(), model.clone()]);
    }
    if let Some(effort) = &request.effort {
        args.extend([
            "-c".to_owned(),
            format!("model_reasoning_effort=\"{effort}\""),
        ]);
    }
    match prompt {
        Some(prompt) => args.push(prompt),
        None if request.resume.is_none() => {
            return Err(LaunchError(
                "a new conversation needs a first prompt".to_owned(),
            ));
        }
        None => {}
    }
    let path = crate::shell::prepend_path(Some(&launch.bin_dir), &launch.path)
        .to_string_lossy()
        .into_owned();
    Ok(LaunchPlan {
        program: COMMAND_NAME.to_owned(),
        args,
        cwd: request.cwd.clone(),
        env: environment(request, path)?,
        tool_id,
    })
}

/// The first prompt as typed: `$otaq-noyan <message>`, or the text.
fn prompt_text(codex: &Codex, prompt: &FirstPrompt) -> String {
    prompt.text(codex)
}

/// Otaq's variables for the role, as every tool gets them: a ticket's agent (and an answer or delivery session)
/// `OTAQ_TICKET`, `PORT` and PATH; a validator the ticket, PATH and its role, no `PORT`; a Project agent PATH and its
/// role.
fn environment(
    request: &LaunchRequest,
    path: String,
) -> Result<Vec<(String, String)>, LaunchError> {
    let ticket = || {
        request
            .ticket
            .as_ref()
            .map(ToString::to_string)
            .ok_or_else(|| LaunchError(format!("{:?} needs a ticket", request.role)))
    };
    Ok(match request.role {
        LaunchRole::TicketAgent | LaunchRole::Answer | LaunchRole::Delivery => vec![
            (naming::ticket_env_var(), ticket()?),
            ("PORT".to_owned(), request.port.to_string()),
            ("PATH".to_owned(), path),
        ],
        LaunchRole::Validator => vec![
            (naming::ticket_env_var(), ticket()?),
            ("PATH".to_owned(), path),
            (naming::role_env_var(), naming::ROLE_VALIDATOR.to_owned()),
        ],
        LaunchRole::ProjectAgent => vec![
            ("PATH".to_owned(), path),
            (
                naming::role_env_var(),
                naming::ROLE_PROJECT_AGENT.to_owned(),
            ),
        ],
    })
}

/// Why no launch can be built: Otaq's own folders (the `otaq` command for the agent's PATH, its plugin) weren't
/// found.
fn launch_missing() -> LaunchError {
    LaunchError(format!(
        "{}'s plugin wasn't found next to the {} command",
        naming::PRODUCT_NAME,
        naming::COMMAND_NAME
    ))
}

/// Otaq's folders not found, or its one-time setup not (or no longer) in Codex's home
/// ([`super::setup::launch_refusal`]: Codex would stop at "Hooks need review").
pub(super) fn unavailable(context: &CodexContext) -> Option<LaunchError> {
    if context.tool.launch.is_none() {
        return Some(launch_missing());
    }
    super::setup::launch_refusal(context)
}

/// Its session file is still in Codex's home folder (`sessions/YYYY/MM/DD/rollout-…-<id>.jsonl`), looked up
/// read-only; without a home folder (tests) the id is enough.
pub(super) fn can_resume(context: &CodexContext, tool_id: &str, _cwd: &Path) -> bool {
    context
        .codex_home
        .as_deref()
        .is_none_or(|home| session_file(home, tool_id).is_some())
}

/// The session file of conversation `tool_id` under `home`: the newest day folder first. `None` when there is
/// none (or the id can't be part of a file name).
pub(crate) fn session_file(home: &Path, tool_id: &str) -> Option<PathBuf> {
    if tool_id.is_empty() || tool_id.contains(['/', '\\']) {
        return None;
    }
    let suffix = format!("-{tool_id}.jsonl");
    let sorted = |dir: &Path| -> Vec<PathBuf> {
        let mut entries: Vec<PathBuf> = std::fs::read_dir(dir)
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.path())
            .collect();
        entries.sort();
        entries.reverse();
        entries
    };
    let sessions = home.join("sessions");
    for year in sorted(&sessions).iter().filter(|p| p.is_dir()) {
        for month in sorted(year).iter().filter(|p| p.is_dir()) {
            for day in sorted(month).iter().filter(|p| p.is_dir()) {
                let found = sorted(day).into_iter().find(|file| {
                    file.file_name()
                        .and_then(|n| n.to_str())
                        .is_some_and(|n| n.starts_with("rollout-") && n.ends_with(&suffix))
                });
                if found.is_some() {
                    return found;
                }
            }
        }
    }
    None
}

pub(super) fn conversation_not_found(output: &[u8]) -> bool {
    String::from_utf8_lossy(output)
        .to_lowercase()
        .contains(CONVERSATION_NOT_FOUND)
}

/// "codex resume <id>".
pub(crate) fn resume_hint(tool_id: &str) -> String {
    format!("{COMMAND_NAME} {RESUME} {tool_id}")
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;

    use super::*;
    use crate::agent_tools::AgentTool;
    use crate::agent_tools::claude::ClaudeLaunch;
    use crate::agent_tools::test_support::only_the_prompt_differs;
    use crate::agent_tools::{
        Adapters, LaunchContext, OneOffRequest, Tool, ToolContext, WORK_TICKET_SKILL,
    };

    /// What Codex is never given (d0011, ADP-07): an approval mode, a sandbox, a bypass of either or of hook trust.
    const FORBIDDEN: &[&str] = &[
        "--dangerously-bypass-approvals-and-sandbox",
        "--dangerously-bypass-hook-trust",
        "--full-auto",
        "--yolo",
        "--approve-for-me",
        "-a",
        "--ask-for-approval",
        "-s",
        "--sandbox",
    ];

    fn platform_path(dirs: &[&str]) -> OsString {
        std::env::join_paths(dirs).unwrap()
    }

    fn launch_context() -> LaunchContext {
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

    fn codex_with(launch: Option<LaunchContext>, codex_home: Option<&Path>) -> Codex {
        let tool = ToolContext {
            launch,
            path: OsString::new(),
            home: None,
            data: crate::data_dir::DataDir::new("/nonexistent/data"),
        };
        Codex::with_context(CodexContext {
            tool,
            codex_home: codex_home.map(Path::to_path_buf),
        })
    }

    fn codex() -> Codex {
        codex_with(Some(launch_context()), None)
    }

    fn request(role: LaunchRole) -> LaunchRequest {
        LaunchRequest {
            role,
            cwd: "/wt/shop-web/SHOP-18".into(),
            project: "SHOP".into(),
            ticket: Some("SHOP-18".parse().unwrap()),
            port: 4012,
            first_prompt: Some(FirstPrompt::Skill {
                name: WORK_TICKET_SKILL.into(),
                message: None,
            }),
            resume: None,
            model: None,
            title: None,
            effort: None,
            theme: crate::board::settings::Theme::OtaqDark,
        }
    }

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|a| (*a).to_owned()).collect()
    }

    fn path_value() -> String {
        platform_path(&["/opt/cb/bin", "/usr/bin", "/bin"])
            .to_string_lossy()
            .into_owned()
    }

    fn assert_no_forbidden_flag(args: &[String]) {
        for flag in FORBIDDEN {
            assert!(!args.iter().any(|a| a == flag), "{flag} in {args:?}");
        }
        assert!(
            !args.iter().any(|a| a.starts_with("--dangerously")),
            "{args:?}"
        );
    }

    /// SDD ADP-07: `codex --no-daemon -c check_for_update_on_startup=false [-m <model>] "$otaq-work-ticket"` in the
    /// worktree, Otaq's environment, the id captured after start.
    #[test]
    fn a_new_session_with_an_exact_model_and_with_none() {
        let skill = format!("${}-work-ticket", naming::COMMAND_NAME);
        let mut with_model = request(LaunchRole::TicketAgent);
        with_model.model = Some("gpt-6-sol".into());
        let plan = codex().launch(&with_model).unwrap();
        assert_eq!(plan.program, "codex");
        assert_eq!(
            plan.args,
            args(&[
                "--no-daemon",
                "-c",
                "check_for_update_on_startup=false",
                "-m",
                "gpt-6-sol",
                &skill,
            ])
        );
        assert_eq!(plan.cwd, Path::new("/wt/shop-web/SHOP-18"));
        assert_eq!(plan.tool_id, ToolIdMode::CapturedAfterStart);
        assert_eq!(
            plan.env,
            [
                (naming::ticket_env_var(), "SHOP-18".to_owned()),
                ("PORT".to_owned(), "4012".to_owned()),
                ("PATH".to_owned(), path_value()),
            ]
        );
        assert_no_forbidden_flag(&plan.args);

        let plain = codex().launch(&request(LaunchRole::TicketAgent)).unwrap();
        assert_eq!(
            plain.args,
            args(&[
                "--no-daemon",
                "-c",
                "check_for_update_on_startup=false",
                &skill
            ]),
            "the tool default passes no model option"
        );
        assert_no_forbidden_flag(&plain.args);
    }

    /// Resume: `codex --no-daemon -c … resume <id> [-m <model>] ["<prompt>"]`, the stored id known up front.
    /// An agent's effort goes to Codex as `-c model_reasoning_effort="<level>"` after the model (AGM-01).
    #[test]
    fn an_effort_is_a_config_override() {
        let mut req = request(LaunchRole::TicketAgent);
        req.model = Some("gpt-6-sol".into());
        req.effort = Some("xhigh".into());
        let plan = codex().launch(&req).unwrap();
        assert_eq!(
            plan.args[3..8],
            args(&[
                "-m",
                "gpt-6-sol",
                "-c",
                "model_reasoning_effort=\"xhigh\"",
                &format!("${}-work-ticket", naming::COMMAND_NAME),
            ])
        );
        assert!(codex().effort_levels().contains(&"minimal"));
    }

    #[test]
    fn a_resume_with_and_without_a_prompt() {
        let mut resume = request(LaunchRole::TicketAgent);
        resume.resume = Some("01a147a1-dd63-7c03-b96e-8f7557586707".into());
        resume.model = Some("gpt-6-sol".into());
        resume.first_prompt = Some(FirstPrompt::Skill {
            name: "address-review".into(),
            message: None,
        });
        let plan = codex().launch(&resume).unwrap();
        assert_eq!(
            plan.args,
            args(&[
                "--no-daemon",
                "-c",
                "check_for_update_on_startup=false",
                "resume",
                "01a147a1-dd63-7c03-b96e-8f7557586707",
                "-m",
                "gpt-6-sol",
                &format!("${}-address-review", naming::COMMAND_NAME),
            ])
        );
        assert_eq!(
            plan.tool_id,
            ToolIdMode::ChosenUpFront("01a147a1-dd63-7c03-b96e-8f7557586707".into())
        );
        assert_no_forbidden_flag(&plan.args);

        resume.first_prompt = None;
        resume.model = None;
        let plan = codex().launch(&resume).unwrap();
        assert_eq!(
            plan.args,
            args(&[
                "--no-daemon",
                "-c",
                "check_for_update_on_startup=false",
                "resume",
                "01a147a1-dd63-7c03-b96e-8f7557586707",
            ])
        );
    }

    /// Each role gets Otaq's variables as on every tool; the Project agent's first message follows its skill.
    #[test]
    fn the_environment_of_each_role() {
        let path = path_value();
        let validator = codex().launch(&request(LaunchRole::Validator)).unwrap();
        assert_eq!(
            validator.env,
            [
                (naming::ticket_env_var(), "SHOP-18".to_owned()),
                ("PATH".to_owned(), path.clone()),
                (naming::role_env_var(), naming::ROLE_VALIDATOR.to_owned()),
            ],
            "no PORT"
        );

        let mut project = request(LaunchRole::ProjectAgent);
        project.ticket = None;
        project.cwd = "/r/shop-web".into();
        let plan = codex().launch(&project).unwrap();
        assert_eq!(plan.cwd, Path::new("/r/shop-web"));
        assert_eq!(
            plan.env,
            [
                ("PATH".to_owned(), path.clone()),
                (
                    naming::role_env_var(),
                    naming::ROLE_PROJECT_AGENT.to_owned()
                ),
            ]
        );

        let mut first = project.clone();
        first.first_prompt = Some(FirstPrompt::Skill {
            name: "noyan".into(),
            message: Some("  Start with the cart  ".into()),
        });
        let plan = codex().launch(&first).unwrap();
        assert_eq!(
            plan.args.last().unwrap(),
            &format!("${}-noyan Start with the cart", naming::COMMAND_NAME)
        );
        let mut no_ticket = request(LaunchRole::TicketAgent);
        no_ticket.ticket = None;
        assert!(codex().launch(&no_ticket).is_err());
        let mut no_prompt = request(LaunchRole::TicketAgent);
        no_prompt.first_prompt = None;
        assert_eq!(
            codex().launch(&no_prompt),
            Err(LaunchError(
                "a new conversation needs a first prompt".to_owned()
            ))
        );
    }

    /// Without Otaq's own folders no launch can be built, with the same reason as the launch's.
    #[test]
    fn without_the_launch_context_codex_is_unavailable() {
        let adapter = codex_with(None, None);
        let reason = LaunchError(format!(
            "{}'s plugin wasn't found next to the {} command",
            naming::PRODUCT_NAME,
            naming::COMMAND_NAME
        ));
        assert_eq!(adapter.unavailable(), Some(reason.clone()));
        assert_eq!(
            adapter.launch(&request(LaunchRole::TicketAgent)),
            Err(reason)
        );
        assert_eq!(codex().unavailable(), None);
    }

    /// Through the registry, a new Codex session stores no tool id yet and the model as launched (ADP-04).
    #[test]
    fn the_registry_stores_no_id_until_it_is_captured() {
        let home = tempfile::tempdir().unwrap();
        let adapters = Adapters::launching_only(
            Some(launch_context()),
            crate::data_dir::DataDir::new(home.path()),
        );
        assert_eq!(adapters.unavailable(Some(Tool::Codex)), Ok(()));
        let mut new = request(LaunchRole::TicketAgent);
        new.model = Some("gpt-6-sol".into());
        let launched = adapters.launch(Some(Tool::Codex), &new).unwrap();
        assert_eq!(launched.tool, Tool::Codex);
        assert_eq!(launched.tool_session_id, None);
        assert_eq!(launched.model.as_deref(), Some("gpt-6-sol"));
        new.resume = Some("01a147a1".into());
        let resumed = adapters.launch(Some(Tool::Codex), &new).unwrap();
        assert_eq!(resumed.tool_session_id.as_deref(), Some("01a147a1"));
        let question = OneOffRequest {
            cwd: "/r".into(),
            prompt: "Suggest".into(),
            model: None,
        };
        assert!(adapters.get(Tool::Codex).one_off(&question).is_ok());
    }

    /// The resume check looks for `sessions/YYYY/MM/DD/rollout-…-<id>.jsonl` in Codex's home folder.
    #[test]
    fn can_resume_with_and_without_the_session_file() {
        let home = tempfile::tempdir().unwrap();
        let id = "01a14bae-dd63-7c03-b96e-8f7557586707";
        let adapter = codex_with(Some(launch_context()), Some(home.path()));
        let cwd = Path::new("/wt/shop-web/SHOP-18");
        assert!(!adapter.can_resume(id, cwd));
        let day = home.path().join("sessions/2026/10/18");
        std::fs::create_dir_all(&day).unwrap();
        std::fs::write(day.join("rollout-2026-10-18T00-05-13-other.jsonl"), "").unwrap();
        assert!(!adapter.can_resume(id, cwd), "another conversation");
        let file = day.join(format!("rollout-2026-10-18T00-05-13-{id}.jsonl"));
        std::fs::write(&file, "{}\n").unwrap();
        assert!(adapter.can_resume(id, cwd));
        assert_eq!(session_file(home.path(), id), Some(file));
        assert!(!adapter.can_resume("", cwd));
        assert!(!adapter.can_resume("../x", cwd));
        assert!(
            codex().can_resume(id, cwd),
            "no home folder: the id is enough"
        );
    }

    /// Written from `codex exec resume <made-up id>` with `CODEX_HOME` in a temporary folder (0.157.1); the TUI's
    /// text is the hand check's (T-0369 replaces it with a capture).
    #[test]
    fn a_missing_conversation_is_read_from_the_resume_output() {
        let output = b"Error: thread/resume: thread/resume failed: no rollout found for thread id \
                       01a147a1-0000-7000-8000-000000000000 (code -32600)\n";
        assert!(codex().conversation_not_found(output));
        assert!(!codex().conversation_not_found(b"Working (12s)"));
    }

    /// A stage's skill as a first prompt (AGM-07): a shipped one and an imported one alike by their copies' names in
    /// Codex's syntax; the launch is otherwise the same, with no flag of [`FORBIDDEN`] and no added folder (d0011).
    #[test]
    fn a_skill_first_prompt() {
        let mut req = request(LaunchRole::Validator);
        req.first_prompt = Some(FirstPrompt::Skill {
            name: "validate".into(),
            message: None,
        });
        let shipped = codex().launch(&req).unwrap();
        assert_eq!(
            shipped.args.last().unwrap(),
            &format!("${}-validate", naming::COMMAND_NAME)
        );
        req.first_prompt = Some(FirstPrompt::Skill {
            name: "team-validate".into(),
            message: None,
        });
        let imported = codex().launch(&req).unwrap();
        assert_eq!(
            imported.args.last().unwrap(),
            &format!("${}-team-validate", naming::COMMAND_NAME)
        );
        only_the_prompt_differs(&shipped, &imported);
        for flag in FORBIDDEN.iter().chain(&["--add-dir"]) {
            assert!(!imported.args.iter().any(|a| a == flag), "{flag}");
        }
    }
}
