//! Launch and resume (ADP-08, SES-03; T-0352). A new conversation:
//!
//! ```text
//! OTAQ_TICKET=SHOP-29  PATH=<otaq bin dir>:$PATH  [PORT, OTAQ_ROLE as for every tool]
//! copilot --session-id <uuid> [--model <model> | --model auto --auto-tier <tier>]
//!         --plugin-dir <otaq plugin>/copilot --allow-tool 'shell(otaq:*)' -i "/otaq-work-ticket"
//! ```
//!
//! and a resume `copilot --resume <id> [--model …] --plugin-dir … --allow-tool … [-i "<prompt>"]` (the model is
//! passed again: whether a resume keeps it isn't verified, the models report, row 6). **No permission flag**
//! (d0011): never `--allow-all`, `--yolo`, `--allow-all-tools` or `COPILOT_ALLOW_ALL`; the only pre-approval is
//! Otaq's own `otaq` commands. Otaq doesn't touch the user's Copilot settings or status line (d0088).
//!
//! **Copilot's plugin folder** (ADP-03): `copilot/` inside Otaq's plugin folder (found where
//! [`crate::agent_tools::claude::find_plugin_dir`] looks, `Otaq.app/Contents/Resources` among them), holds a manifest
//! and a `hooks/hooks.json` whose commands pass `--tool copilot`, and **no skills** (AGM-07): the skills reach Copilot
//! as project skills copied into the session's folder (`.agents/skills/otaq-<name>/` and `.claude/skills/`, both read
//! by Copilot, `copilot skill --help`: "Project .github/skills/, .agents/skills/, or .claude/skills/"), run as
//! `/otaq-<name>` (docs.github.com, "Adding agent skills for GitHub Copilot CLI": the skill name "preceded by a forward
//! slash"). Checked with Copilot 1.0.98 on 2026-10-20 with a throwaway HOME: `copilot --plugin-dir plugin/copilot
//! skill list` in a folder holding one copy in both places lists it once, under "Project skills", and lists no
//! plugin skills.

use std::path::{Path, PathBuf};

use super::{COMMAND_NAME, Copilot, CopilotContext};
use crate::agent_tools::follow::FollowChoice;
use crate::agent_tools::{
    FirstPrompt, LaunchContext, LaunchError, LaunchPlan, LaunchRequest, LaunchRole, ToolIdMode,
};
use crate::naming;

/// Copilot's plugin folder inside Otaq's plugin folder.
const PLUGIN_FOLDER: &str = "copilot";

/// What `copilot --resume <id>` prints for an id it doesn't have ("Error: No session, task, or name matched
/// '<id>'."), compared without case. Captured from 1.0.98 on 2026-10-18.
const CONVERSATION_NOT_FOUND: &str = "no session, task, or name matched";

/// Copilot's plugin folder for `launch`: `copilot/` in its `plugin_dir`, when it has its manifest.
pub fn find_plugin_dir(launch: &LaunchContext) -> Option<PathBuf> {
    let dir = launch.plugin_dir.join(PLUGIN_FOLDER);
    dir.join(".claude-plugin")
        .join("plugin.json")
        .is_file()
        .then_some(dir)
}

/// Why no launch can be built: Otaq's plugin, or Copilot's folder beside it, wasn't found.
fn plugin_missing() -> LaunchError {
    LaunchError(format!(
        "{}'s Copilot plugin wasn't found next to the {} command",
        naming::PRODUCT_NAME,
        naming::COMMAND_NAME
    ))
}

/// The launch context and Copilot's plugin folder.
fn launch_parts(context: &CopilotContext) -> Result<(&LaunchContext, PathBuf), LaunchError> {
    let launch = context.tool.launch.as_ref().ok_or_else(plugin_missing)?;
    let plugin = find_plugin_dir(launch).ok_or_else(plugin_missing)?;
    Ok((launch, plugin))
}

pub(super) fn launch(
    context: &CopilotContext,
    request: &LaunchRequest,
) -> Result<LaunchPlan, LaunchError> {
    let (launch, plugin) = launch_parts(context)?;
    let adapter = Copilot::with_context(context.clone());
    let prompt = request
        .first_prompt
        .as_ref()
        .map(|p| prompt_text(&adapter, p));
    let (mut args, tool_id) = match &request.resume {
        Some(id) => (vec!["--resume".to_owned(), id.clone()], id.clone()),
        None => {
            if prompt.is_none() {
                return Err(LaunchError(
                    "a new conversation needs a first prompt".to_owned(),
                ));
            }
            let id = uuid::Uuid::now_v7().to_string();
            (vec!["--session-id".to_owned(), id.clone()], id)
        }
    };
    args.extend(model_flags(request.model.as_deref()));
    if let Some(effort) = &request.effort {
        args.extend(["--reasoning-effort".to_owned(), effort.clone()]);
    }
    args.extend([
        "--plugin-dir".to_owned(),
        plugin.display().to_string(),
        "--allow-tool".to_owned(),
        format!("shell({}:*)", naming::COMMAND_NAME),
    ]);
    if let Some(prompt) = prompt {
        args.extend(["-i".to_owned(), prompt]);
    }
    Ok(LaunchPlan {
        program: COMMAND_NAME.to_owned(),
        args,
        cwd: request.cwd.clone(),
        env: environment(launch, request)?,
        tool_id: ToolIdMode::ChosenUpFront(tool_id),
    })
}

/// The model as Copilot takes it (d0088: unchanged): an exact id → `--model <id>`; Auto with a tier →
/// `--model auto --auto-tier <tier>` (a tier without `--model auto` stops the launch, the models report, row 3);
/// none → nothing, Copilot's own default.
pub(super) fn model_flags(model: Option<&str>) -> Vec<String> {
    let Some(model) = model.map(str::trim).filter(|m| !m.is_empty()) else {
        return Vec::new();
    };
    match FollowChoice::parse(model) {
        Some(FollowChoice::Auto { tier }) => vec![
            "--model".to_owned(),
            super::events_file::AUTO.to_owned(),
            "--auto-tier".to_owned(),
            tier.to_string(),
        ],
        _ => vec!["--model".to_owned(), model.to_owned()],
    }
}

/// The first prompt as typed: `/otaq-noyan <message>`, or the text.
fn prompt_text(adapter: &Copilot, prompt: &FirstPrompt) -> String {
    prompt.text(adapter)
}

/// A skill as Copilot runs a project skill: `/otaq-work-ticket` (AGM-07).
pub(super) fn skill_reference(skill: &str) -> String {
    format!("/{}", naming::copy_name(skill))
}

/// Otaq's variables for the role, as for every tool: the ticket and `PORT` for a ticket's agent (an answer, a
/// delivery), the ticket and the validator role without `PORT` for a validator, the role for a Project agent; PATH
/// with the `otaq` folder first.
fn environment(
    launch: &LaunchContext,
    request: &LaunchRequest,
) -> Result<Vec<(String, String)>, LaunchError> {
    let path = crate::shell::prepend_path(Some(&launch.bin_dir), &launch.path)
        .to_string_lossy()
        .into_owned();
    let ticket = || {
        request
            .ticket
            .as_ref()
            .map(ToString::to_string)
            .ok_or_else(|| LaunchError(format!("{:?} needs a ticket", request.role)))
    };
    let role = |role: &str| (naming::role_env_var(), role.to_owned());
    Ok(match request.role {
        LaunchRole::TicketAgent | LaunchRole::Answer | LaunchRole::Delivery => vec![
            (naming::ticket_env_var(), ticket()?),
            ("PORT".to_owned(), request.port.to_string()),
            ("PATH".to_owned(), path),
        ],
        LaunchRole::Validator => vec![
            (naming::ticket_env_var(), ticket()?),
            ("PATH".to_owned(), path),
            role(naming::ROLE_VALIDATOR),
        ],
        LaunchRole::ProjectAgent => {
            vec![("PATH".to_owned(), path), role(naming::ROLE_PROJECT_AGENT)]
        }
    })
}

pub(super) fn unavailable(context: &CopilotContext) -> Option<LaunchError> {
    launch_parts(context).err()
}

/// Its folder `session-state/<id>/` is still in Copilot's home; without a home folder (tests) the id is enough.
pub(super) fn can_resume(context: &CopilotContext, tool_id: &str, _cwd: &Path) -> bool {
    if context.copilot_home.is_none() {
        return true;
    }
    super::events_file::session_dir(context, tool_id).is_some_and(|dir| dir.is_dir())
}

/// Copilot's text for an unknown id.
pub(super) fn conversation_not_found(output: &[u8]) -> bool {
    String::from_utf8_lossy(output)
        .to_lowercase()
        .contains(CONVERSATION_NOT_FOUND)
}

/// "copilot --resume <id>".
pub(crate) fn resume_hint(tool_id: &str) -> String {
    format!("{COMMAND_NAME} --resume {tool_id}")
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;

    use super::*;
    use crate::agent_tools::test_support;
    use crate::agent_tools::{Adapters, WORK_TICKET_SKILL};

    /// Otaq's plugin folder with Copilot's inside it in a temporary install, and a context that launches with them.
    struct Install {
        dir: tempfile::TempDir,
        copilot: CopilotContext,
    }

    impl Install {
        fn plugin(&self) -> String {
            self.dir
                .path()
                .join("plugin")
                .join(PLUGIN_FOLDER)
                .display()
                .to_string()
        }
    }

    fn install() -> Install {
        let dir = tempfile::tempdir().unwrap();
        let manifest = dir
            .path()
            .join("plugin")
            .join(PLUGIN_FOLDER)
            .join(".claude-plugin");
        std::fs::create_dir_all(&manifest).unwrap();
        std::fs::write(manifest.join("plugin.json"), "{}").unwrap();
        let mut tool = test_support::context(dir.path(), Some(dir.path()));
        tool.launch = Some(LaunchContext {
            bin_dir: "/opt/cb/bin".into(),
            plugin_dir: dir.path().join("plugin"),
            path: std::env::join_paths(["/usr/bin", "/bin"]).unwrap(),
            claude: crate::agent_tools::claude::ClaudeLaunch {
                program: vec!["claude".into()],
                config_dir: None,
            },
        });
        Install {
            copilot: CopilotContext {
                copilot_home: Some(dir.path().join(".copilot")),
                tool,
            },
            dir,
        }
    }

    fn request(role: LaunchRole) -> LaunchRequest {
        LaunchRequest {
            role,
            cwd: "/wt/shop-web/SHOP-29".into(),
            project: "SHOP".into(),
            ticket: Some("SHOP-29".parse().unwrap()),
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

    fn path() -> String {
        std::env::join_paths(["/opt/cb/bin", "/usr/bin", "/bin"])
            .unwrap()
            .to_string_lossy()
            .into_owned()
    }

    fn allow() -> String {
        format!("shell({}:*)", naming::COMMAND_NAME)
    }

    fn chosen(plan: &LaunchPlan) -> String {
        match &plan.tool_id {
            ToolIdMode::ChosenUpFront(id) => id.clone(),
            other => panic!("{other:?}"),
        }
    }

    /// ADP-08: a ticket's agent with an exact model; the id is chosen up front, a fresh UUID each time.
    #[test]
    fn an_exact_model() {
        let i = install();
        let mut req = request(LaunchRole::TicketAgent);
        req.model = Some("gpt-5.5".into());
        let plan = launch(&i.copilot, &req).unwrap();
        let id = chosen(&plan);
        assert!(uuid::Uuid::parse_str(&id).is_ok());
        assert_eq!(plan.program, "copilot");
        assert_eq!(
            plan.args,
            [
                "--session-id".to_owned(),
                id,
                "--model".into(),
                "gpt-5.5".into(),
                "--plugin-dir".into(),
                i.plugin(),
                "--allow-tool".into(),
                allow(),
                "-i".into(),
                format!("/{}-work-ticket", naming::COMMAND_NAME),
            ]
        );
        assert_eq!(plan.cwd, Path::new("/wt/shop-web/SHOP-29"));
        assert_eq!(
            plan.env,
            [
                (naming::ticket_env_var(), "SHOP-29".to_owned()),
                ("PORT".to_owned(), "4012".to_owned()),
                ("PATH".to_owned(), path()),
            ]
        );
        assert_ne!(chosen(&launch(&i.copilot, &req).unwrap()), chosen(&plan));
    }

    /// `auto@intelligence` → `--model auto --auto-tier intelligence`; no model → no model flag.
    #[test]
    fn auto_with_a_tier_and_no_model() {
        let i = install();
        let mut req = request(LaunchRole::TicketAgent);
        req.model = Some("auto@intelligence".into());
        let args = launch(&i.copilot, &req).unwrap().args;
        assert_eq!(
            args[2..6],
            ["--model", "auto", "--auto-tier", "intelligence"]
        );
        for model in [None, Some(""), Some("  ")] {
            req.model = model.map(str::to_owned);
            let args = launch(&i.copilot, &req).unwrap().args;
            assert!(!args.iter().any(|a| a == "--model"), "{args:?}");
            assert_eq!(args[2], "--plugin-dir");
        }
        req.model = Some("auto".into());
        assert_eq!(
            launch(&i.copilot, &req).unwrap().args[2..4],
            ["--model", "auto"]
        );
    }

    /// A resume passes the id and the model again; a prompt only when there is one.
    /// An agent's effort goes to Copilot as `--reasoning-effort <level>` (AGM-01).
    #[test]
    fn an_effort_is_its_own_flag() {
        let i = install();
        let mut req = request(LaunchRole::TicketAgent);
        req.model = Some("gpt-5.5".into());
        req.effort = Some("max".into());
        let args = launch(&i.copilot, &req).unwrap().args;
        let at = args.iter().position(|a| a == "--reasoning-effort").unwrap();
        assert_eq!(args[at + 1], "max");
    }

    #[test]
    fn resume() {
        let i = install();
        let mut req = request(LaunchRole::TicketAgent);
        req.resume = Some("5f1c0a2e-0000-7000-8000-000000000000".into());
        req.model = Some("gpt-5.5".into());
        req.first_prompt = None;
        let plan = launch(&i.copilot, &req).unwrap();
        assert_eq!(
            plan.tool_id,
            ToolIdMode::ChosenUpFront("5f1c0a2e-0000-7000-8000-000000000000".into())
        );
        assert_eq!(
            plan.args,
            [
                "--resume".to_owned(),
                "5f1c0a2e-0000-7000-8000-000000000000".into(),
                "--model".into(),
                "gpt-5.5".into(),
                "--plugin-dir".into(),
                i.plugin(),
                "--allow-tool".into(),
                allow(),
            ]
        );
        req.first_prompt = Some(FirstPrompt::Text("push now".into()));
        let args = launch(&i.copilot, &req).unwrap().args;
        assert_eq!(args[args.len() - 2..], ["-i", "push now"]);
        // A new conversation needs its prompt.
        req.resume = None;
        req.first_prompt = None;
        assert!(launch(&i.copilot, &req).is_err());
    }

    /// d0011: never a permission or bypass flag, nor `COPILOT_ALLOW_ALL`, on any role.
    #[test]
    fn the_forbidden_flags_never_appear() {
        let i = install();
        for role in [
            LaunchRole::TicketAgent,
            LaunchRole::Validator,
            LaunchRole::ProjectAgent,
            LaunchRole::Answer,
            LaunchRole::Delivery,
        ] {
            let mut req = request(role);
            for model in [None, Some("auto@balance".to_owned())] {
                req.model = model;
                let plan = launch(&i.copilot, &req).unwrap();
                for arg in &plan.args {
                    for forbidden in [
                        "--allow-all",
                        "--yolo",
                        "--allow-all-tools",
                        "--allow-all-paths",
                        "--allow-all-urls",
                        "--autopilot",
                        "--assisted-approval",
                    ] {
                        assert_ne!(arg, forbidden, "{role:?}");
                    }
                }
                assert!(
                    plan.env.iter().all(|(k, _)| !k.starts_with("COPILOT_")),
                    "{role:?}"
                );
                let allowed: Vec<_> = plan
                    .args
                    .iter()
                    .enumerate()
                    .filter(|(_, a)| a.starts_with("--allow"))
                    .map(|(n, _)| plan.args[n + 1].clone())
                    .collect();
                assert_eq!(allowed, [allow()], "only the command is pre-approved");
            }
        }
    }

    /// Every role's variables (as Claude Code's), and its prompt.
    #[test]
    fn roles() {
        let i = install();
        let mut req = request(LaunchRole::Validator);
        req.first_prompt = Some(FirstPrompt::Skill {
            name: "validate".into(),
            message: None,
        });
        let plan = launch(&i.copilot, &req).unwrap();
        assert_eq!(
            plan.env,
            [
                (naming::ticket_env_var(), "SHOP-29".to_owned()),
                ("PATH".to_owned(), path()),
                (naming::role_env_var(), naming::ROLE_VALIDATOR.to_owned()),
            ]
        );
        // A project skill by its copy's name, shipped or imported alike (AGM-07); no `--add-dir` (d0011).
        assert_eq!(
            plan.args.last().unwrap(),
            &format!("/{}-validate", naming::COMMAND_NAME)
        );
        assert!(!plan.args.iter().any(|a| a == "--add-dir"));

        let mut req = request(LaunchRole::ProjectAgent);
        req.ticket = None;
        let plan = launch(&i.copilot, &req).unwrap();
        assert_eq!(
            plan.env,
            [
                ("PATH".to_owned(), path()),
                (
                    naming::role_env_var(),
                    naming::ROLE_PROJECT_AGENT.to_owned()
                ),
            ]
        );
        let mut req = request(LaunchRole::TicketAgent);
        req.ticket = None;
        assert!(launch(&i.copilot, &req).is_err());
    }

    /// Without Otaq's plugin, or without Copilot's folder in it, nothing launches; through the registry too.
    #[test]
    fn the_copilot_plugin_folder_is_needed() {
        let i = install();
        assert_eq!(unavailable(&i.copilot), None);
        assert_eq!(
            find_plugin_dir(i.copilot.tool.launch.as_ref().unwrap()),
            Some(i.dir.path().join("plugin").join(PLUGIN_FOLDER))
        );
        let launch_context = i.copilot.tool.launch.clone();
        let adapters = Adapters::launching_only(
            launch_context.clone(),
            crate::data_dir::DataDir::new(i.dir.path()),
        );
        let launched = adapters
            .launch(
                Some(crate::agent_tools::Tool::Copilot),
                &request(LaunchRole::TicketAgent),
            )
            .unwrap();
        assert_eq!(launched.tool, crate::agent_tools::Tool::Copilot);
        assert_eq!(launched.tool_session_id, Some(chosen(&launched.plan)));

        std::fs::remove_dir_all(i.dir.path().join("plugin").join(PLUGIN_FOLDER)).unwrap();
        let missing = plugin_missing();
        assert_eq!(unavailable(&i.copilot), Some(missing.clone()));
        assert_eq!(
            launch(&i.copilot, &request(LaunchRole::TicketAgent)),
            Err(missing.clone())
        );
        let none = CopilotContext {
            tool: crate::agent_tools::ToolContext {
                launch: None,
                path: OsString::new(),
                ..i.copilot.tool.clone()
            },
            ..i.copilot.clone()
        };
        assert_eq!(unavailable(&none), Some(missing));
    }

    /// In `Otaq.app` (d0072), `copilot/` is in `plugin/` in `Contents/Resources`.
    #[test]
    fn the_bundle_plugin_folder() {
        let dir = tempfile::tempdir().unwrap();
        let contents = dir
            .path()
            .join(naming::bundle_folder_name())
            .join("Contents");
        let macos = contents.join("MacOS");
        std::fs::create_dir_all(&macos).unwrap();
        for folder in ["claude", PLUGIN_FOLDER] {
            let manifest = contents
                .join("Resources")
                .join("plugin")
                .join(folder)
                .join(".claude-plugin");
            std::fs::create_dir_all(&manifest).unwrap();
            std::fs::write(manifest.join("plugin.json"), "{}").unwrap();
        }
        let launch = LaunchContext {
            bin_dir: macos.clone(),
            plugin_dir: crate::agent_tools::claude::find_plugin_dir(&macos).unwrap(),
            path: OsString::new(),
            claude: crate::agent_tools::claude::ClaudeLaunch {
                program: vec![],
                config_dir: None,
            },
        };
        assert_eq!(
            find_plugin_dir(&launch),
            Some(
                contents
                    .join("Resources")
                    .join("plugin")
                    .join(PLUGIN_FOLDER)
            )
        );
    }

    /// The repository's `plugin/copilot/` is found, it has no skills (AGM-07: hooks only), and every hook passes `--tool copilot` behind the PLG-04 guard, for an event the adapter maps.
    #[test]
    fn the_repository_plugin_folder() {
        let plugin =
            crate::agent_tools::claude::find_plugin_dir(Path::new("/nonexistent")).unwrap();
        let launch = LaunchContext {
            bin_dir: "/nonexistent".into(),
            plugin_dir: plugin.clone(),
            path: OsString::new(),
            claude: crate::agent_tools::claude::ClaudeLaunch {
                program: vec![],
                config_dir: None,
            },
        };
        let dir = find_plugin_dir(&launch).expect("plugin/copilot");
        let manifest: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(dir.join(".claude-plugin/plugin.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(manifest["name"], naming::PLUGIN_NAMESPACE);
        assert!(!dir.join("skills").exists(), "hooks only");
        assert!(manifest.get("skills").is_none());
        let hooks: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(dir.join("hooks/hooks.json")).unwrap())
                .unwrap();
        let events: Vec<&String> = hooks["hooks"].as_object().unwrap().keys().collect();
        assert_eq!(
            events,
            [
                "Notification",
                "SessionStart",
                "Stop",
                "UserPromptSubmit",
                "errorOccurred"
            ]
        );
        let name = naming::COMMAND_NAME;
        for (event, groups) in hooks["hooks"].as_object().unwrap() {
            let command = groups[0]["hooks"][0]["command"].as_str().unwrap();
            assert_eq!(
                command,
                format!(
                    "command -v {name} >/dev/null 2>&1 || exit 0; {name} hook --tool copilot {event}"
                )
            );
        }
    }

    #[test]
    fn resume_checks_and_texts() {
        let i = install();
        assert!(!can_resume(&i.copilot, "5f1c", Path::new("/wt")));
        let dir = i.dir.path().join(".copilot/session-state/5f1c");
        std::fs::create_dir_all(&dir).unwrap();
        assert!(can_resume(&i.copilot, "5f1c", Path::new("/wt")));
        let no_home = CopilotContext {
            copilot_home: None,
            ..i.copilot.clone()
        };
        assert!(can_resume(&no_home, "anything", Path::new("/wt")));
        // Captured from 1.0.98.
        assert!(conversation_not_found(
            b"Error: No session, task, or name matched '99999999-2222-4333-8444-555555555555'.\n\n\
              To resume by session or task ID:  copilot --resume=<id>\n"
        ));
        assert!(!conversation_not_found(b"Resume     copilot --resume=5f1c"));
        assert_eq!(resume_hint("5f1c"), "copilot --resume 5f1c");
    }
}
