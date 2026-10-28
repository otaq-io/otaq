//! Launch and resume (ADP-09, SES-03; T-0354). A new conversation is created first by [`super::prepare`], then:
//!
//! ```text
//! OTAQ_TICKET=SHOP-18  PORT=<agent port>  PATH=<otaq bin dir>:$PATH
//! OPENCODE_CONFIG_CONTENT={"plugins":["<otaq plugin>/opencode"],
//!                          "permissions":[{"action":"shell","resource":"otaq *","effect":"allow"}],"model":"<m>"}
//! OPENCODE_CLI_CONFIG_CONTENT={"theme":{"mode":"light"|"dark"}}
//! opencode --standalone --session <ses_…> --prompt "<first prompt>"
//! ```
//!
//! Always `--standalone`: the default mode attaches to the user's shared OpenCode service, which ignores the
//! launch's environment and config (hands-on check). Everything per launch goes in `OPENCODE_CONFIG_CONTENT`, which
//! adds to the user's own config: Otaq's plugin folder (its hooks), the one rule pre-approving `otaq` commands, and
//! the model when one is set. No `skills` entry: the skills reach OpenCode as project skills copied into the session's
//! folder (AGM-07; OpenCode reads `.claude/skills/<name>/SKILL.md` and `.agents/skills/<name>/SKILL.md`, walking up to
//! the git worktree, its skills page opencode.ai/docs/skills/). Never `OPENCODE_CONFIG_DIR` (it replaces the user's config folder) and
//! never `--auto` (it approves every prompt; d0011). The user's own data folder is used (no `XDG_*` change), so
//! the session shows in their OpenCode session list. Resume is `--session <id>` with an optional prompt; the
//! session's stored model wins over the config there.
//!
//! **Light or dark** (d0117 point 4). Checked with OpenCode 2.0.17 (its binary and embedded guide read with a scratch
//! HOME and XDG folders, no provider, no model call): its terminal settings live apart from `opencode.json`, in
//! `cli.json`, and `OPENCODE_CLI_CONFIG_CONTENT` is merged over them for that process only ("Set
//! `OPENCODE_CLI_CONFIG_CONTENT` to merge inline JSON over the global settings"). `theme.mode` `light` or `dark`
//! fixes the mode; anything else (`system`) takes the mode OpenCode detected from the terminal's answer to its colour
//! questions, after a timeout. Otaq sets the theme's kind explicitly, so the mode doesn't hang on that answer's
//! timing; the user's own theme name (`theme.name`) is kept.

use serde_json::{Value, json};

use super::{COMMAND_NAME, DISPLAY_NAME, OpenCode, OpenCodeContext, prepare};
use crate::agent_tools::{
    FirstPrompt, LaunchContext, LaunchError, LaunchPlan, LaunchRequest, LaunchRole, ThemeKind,
    ToolIdMode,
};
use crate::naming;

/// The variable OpenCode reads its extra config from (not in `--help`; checked on 2.0.17).
const CONFIG_CONTENT: &str = "OPENCODE_CONFIG_CONTENT";

/// The variable OpenCode merges over its terminal settings (`cli.json`) for this run (checked on 2.0.17).
const CLI_CONFIG_CONTENT: &str = "OPENCODE_CLI_CONFIG_CONTENT";

/// The folder of Otaq's OpenCode plugin inside Otaq's plugin folder (`package.json` and `index.js`).
const PLUGIN_FOLDER: &str = "opencode";

pub(super) fn launch(
    adapter: &OpenCode,
    request: &LaunchRequest,
) -> Result<LaunchPlan, LaunchError> {
    let launch = adapter
        .context()
        .tool
        .launch
        .as_ref()
        .ok_or_else(plugin_missing)?;
    let model = request
        .model
        .as_deref()
        .map(|m| {
            prepare::model_ref(m).map(|r| (m, r)).ok_or_else(|| {
                LaunchError(format!(
                    "{DISPLAY_NAME} can't run the model {m}: it names no provider"
                ))
            })
        })
        .transpose()?;
    let path = crate::shell::prepend_path(Some(&launch.bin_dir), &launch.path)
        .to_string_lossy()
        .into_owned();
    let prompt = request
        .first_prompt
        .as_ref()
        .map(|p| prompt_text(adapter, p));
    let (id, tool_id) = match &request.resume {
        Some(id) => (id.clone(), ToolIdMode::ChosenUpFront(id.clone())),
        None => {
            if prompt.is_none() {
                return Err(LaunchError(
                    "a new conversation needs a first prompt".to_owned(),
                ));
            }
            let id = prepare::new_session_id(crate::store::now_ms());
            let step = prepare::create_session(
                &id,
                &session_title(request, request.title.as_deref()),
                model.as_ref().map(|(_, r)| r.clone()),
                &request.cwd,
                path.clone(),
            );
            (id.clone(), ToolIdMode::CreatedBeforeLaunch { id, step })
        }
    };
    let mut args = vec!["--standalone".to_owned(), "--session".to_owned(), id];
    if let Some(prompt) = prompt {
        args.extend(["--prompt".to_owned(), prompt]);
    }
    let mut env = otaq_env(request, path)?;
    env.push((
        CONFIG_CONTENT.to_owned(),
        config(launch, model.map(|(m, _)| m)).to_string(),
    ));
    env.push((
        CLI_CONFIG_CONTENT.to_owned(),
        cli_config(request.theme.kind()).to_string(),
    ));
    Ok(LaunchPlan {
        program: COMMAND_NAME.to_owned(),
        args,
        cwd: request.cwd.clone(),
        env,
        tool_id,
    })
}

/// Launches can be built once Otaq's plugin folder (its OpenCode plugin) was found.
pub(super) fn unavailable(context: &OpenCodeContext) -> Option<LaunchError> {
    context.tool.launch.is_none().then(plugin_missing)
}

/// "opencode --session <id>".
pub(crate) fn resume_hint(tool_id: &str) -> String {
    format!("{COMMAND_NAME} --session {tool_id}")
}

/// A skill named for the agent: its copy's name, `otaq-work-ticket`. OpenCode lists the skills it found to the model
/// by name (`<available_skills>`) and its skill tool loads one by name ("`skill({ name: "git-release" })`", its skills
/// page); it has no syntax of its own for running a skill from typed text (the TUI's `@<skill>` is an attachment
/// made by its picker, not parsed from text; checked on 2.0.17, and the typed skill lines report of 2026-10-19: the
/// line goes in as text and the model loads the skill).
pub(super) fn skill_reference(skill: &str) -> String {
    naming::copy_name(skill)
}

/// Why no launch can be built: the plugin folder wasn't found.
fn plugin_missing() -> LaunchError {
    LaunchError(format!(
        "{}'s plugin wasn't found next to the {} command",
        naming::PRODUCT_NAME,
        naming::COMMAND_NAME
    ))
}

/// The session's title in the user's OpenCode session list: "Otaq · SHOP-18 · Add coupon codes" with the ticket's
/// title, else "Otaq · SHOP-18"; "Otaq · SHOP" for a Project agent.
pub(super) fn session_title(request: &LaunchRequest, ticket_title: Option<&str>) -> String {
    let what = request
        .ticket
        .as_ref()
        .map_or_else(|| request.project.clone(), ToString::to_string);
    match ticket_title.map(str::trim).filter(|t| !t.is_empty()) {
        Some(title) => format!("{} · {what} · {title}", naming::PRODUCT_NAME),
        None => format!("{} · {what}", naming::PRODUCT_NAME),
    }
}

/// The prompt as sent: a skill by its copy's name (with a message after it), or the text.
fn prompt_text(adapter: &OpenCode, prompt: &FirstPrompt) -> String {
    prompt.text(adapter)
}

/// Otaq's own variables for the role, as every tool gets them: the ticket and its port (a validator has no port),
/// the role, and PATH with the `otaq` folder first.
fn otaq_env(request: &LaunchRequest, path: String) -> Result<Vec<(String, String)>, LaunchError> {
    let ticket = || {
        request
            .ticket
            .as_ref()
            .map(ToString::to_string)
            .ok_or_else(|| LaunchError(format!("{:?} needs a ticket", request.role)))
    };
    let mut env = Vec::new();
    match request.role {
        LaunchRole::TicketAgent | LaunchRole::Answer | LaunchRole::Delivery => {
            env.push((naming::ticket_env_var(), ticket()?));
            env.push(("PORT".to_owned(), request.port.to_string()));
        }
        LaunchRole::Validator => {
            env.push((naming::ticket_env_var(), ticket()?));
            env.push((naming::role_env_var(), naming::ROLE_VALIDATOR.to_owned()));
        }
        LaunchRole::ProjectAgent => {
            env.push((
                naming::role_env_var(),
                naming::ROLE_PROJECT_AGENT.to_owned(),
            ));
        }
    }
    env.push(("PATH".to_owned(), path));
    Ok(env)
}

/// `OPENCODE_CLI_CONFIG_CONTENT`: the theme's kind as OpenCode's theme mode.
fn cli_config(theme: ThemeKind) -> Value {
    let mode = match theme {
        ThemeKind::Light => "light",
        ThemeKind::Dark => "dark",
    };
    json!({ "theme": { "mode": mode } })
}

/// `OPENCODE_CONFIG_CONTENT`: Otaq's plugin, the one `otaq *` shell rule, and `model` when set.
fn config(launch: &LaunchContext, model: Option<&str>) -> Value {
    let mut config = json!({
        "plugins": [launch.plugin_dir.join(PLUGIN_FOLDER).display().to_string()],
        "permissions": [{
            "action": "shell",
            "resource": format!("{} *", naming::COMMAND_NAME),
            "effect": "allow",
        }],
    });
    if let Some(model) = model {
        config["model"] = json!(model);
    }
    config
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;
    use crate::agent_tools::claude::ClaudeLaunch;
    use crate::agent_tools::test_support::{context, only_the_prompt_differs};
    use crate::agent_tools::{PrepareStep, WORK_TICKET_SKILL};

    fn adapter() -> OpenCode {
        let mut tools = context(Path::new("/nowhere"), None);
        tools.launch = Some(LaunchContext {
            bin_dir: "/opt/cb/bin".into(),
            plugin_dir: "/opt/cb/plugin".into(),
            path: std::env::join_paths(["/usr/bin", "/bin"]).unwrap(),
            claude: ClaudeLaunch {
                program: vec!["claude".into()],
                config_dir: None,
            },
        });
        OpenCode::new(tools)
    }

    fn path() -> String {
        std::env::join_paths(["/opt/cb/bin", "/usr/bin", "/bin"])
            .unwrap()
            .into_string()
            .unwrap()
    }

    fn request(role: LaunchRole, model: Option<&str>) -> LaunchRequest {
        LaunchRequest {
            role,
            cwd: "/wt/shop-web/SHOP-50".into(),
            project: "SHOP".into(),
            ticket: Some("SHOP-50".parse().unwrap()),
            port: 4012,
            first_prompt: Some(FirstPrompt::Skill {
                name: WORK_TICKET_SKILL.into(),
                message: None,
            }),
            resume: None,
            model: model.map(Into::into),
            title: None,
            effort: None,
            theme: crate::board::settings::Theme::OtaqDark,
        }
    }

    fn env<'a>(plan: &'a LaunchPlan, name: &str) -> Option<&'a str> {
        plan.env
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, v)| v.as_str())
    }

    fn config(plan: &LaunchPlan) -> Value {
        serde_json::from_str(env(plan, CONFIG_CONTENT).expect("config")).unwrap()
    }

    /// ADP-09, ADP-04: the session is made first (its id, title and model), then OpenCode opens it with the first
    /// prompt; the config carries the plugin, the one `otaq *` rule and the model.
    #[test]
    fn a_new_conversation_is_made_before_launch() {
        let plan = launch(
            &adapter(),
            &request(LaunchRole::TicketAgent, Some("opencode/big-pickle")),
        )
        .unwrap();
        let ToolIdMode::CreatedBeforeLaunch { id, step } = &plan.tool_id else {
            panic!("{:?}", plan.tool_id)
        };
        assert!(id.starts_with("ses_") && id.len() == 30, "{id}");
        assert!(id[4..16].bytes().all(|b| b.is_ascii_hexdigit()), "{id}");
        assert!(id[16..].bytes().all(|b| b.is_ascii_alphanumeric()), "{id}");
        let body: Value = serde_json::from_str(step.args.last().unwrap()).unwrap();
        assert_eq!(
            body,
            json!({
                "id": id,
                "title": format!("{} · SHOP-50", naming::PRODUCT_NAME),
                "model": {"providerID": "opencode", "id": "big-pickle"}
            })
        );
        assert_eq!(
            step,
            &PrepareStep {
                program: "opencode".into(),
                args: vec![
                    "api".into(),
                    "--standalone".into(),
                    "POST".into(),
                    "/api/session".into(),
                    "-d".into(),
                    step.args.last().unwrap().clone(),
                ],
                cwd: "/wt/shop-web/SHOP-50".into(),
                env: vec![("PATH".into(), path())],
            }
        );
        assert_eq!(plan.program, "opencode");
        assert_eq!(
            plan.args,
            [
                "--standalone",
                "--session",
                id.as_str(),
                "--prompt",
                format!("{}-work-ticket", naming::COMMAND_NAME).as_str()
            ]
        );
        assert_eq!(plan.cwd, Path::new("/wt/shop-web/SHOP-50"));
        assert_eq!(
            config(&plan),
            json!({
                "plugins": [Path::new("/opt/cb/plugin").join("opencode").display().to_string()],
                "permissions": [{
                    "action": "shell",
                    "resource": format!("{} *", naming::COMMAND_NAME),
                    "effect": "allow"
                }],
                "model": "opencode/big-pickle"
            })
        );
        assert_eq!(env(&plan, &naming::ticket_env_var()), Some("SHOP-50"));
        assert_eq!(env(&plan, "PORT"), Some("4012"));
        assert_eq!(env(&plan, "PATH"), Some(path().as_str()));
        assert_eq!(
            env(&plan, CLI_CONFIG_CONTENT),
            Some(r#"{"theme":{"mode":"dark"}}"#)
        );
        assert_eq!(plan.env.len(), 5);
    }

    /// No model: no `model` key and no model in the session made first (OpenCode's own default).
    #[test]
    fn no_model_no_key() {
        let plan = launch(&adapter(), &request(LaunchRole::TicketAgent, None)).unwrap();
        assert!(config(&plan).get("model").is_none());
        let ToolIdMode::CreatedBeforeLaunch { step, .. } = &plan.tool_id else {
            panic!()
        };
        let body: Value = serde_json::from_str(step.args.last().unwrap()).unwrap();
        assert!(body.get("model").is_none());
        // A variant goes with the session's model; a name without a provider is refused.
        let plan = launch(
            &adapter(),
            &request(
                LaunchRole::TicketAgent,
                Some("anthropic/claude-sonnet-5-5#high"),
            ),
        )
        .unwrap();
        let ToolIdMode::CreatedBeforeLaunch { step, .. } = &plan.tool_id else {
            panic!()
        };
        let body: Value = serde_json::from_str(step.args.last().unwrap()).unwrap();
        assert_eq!(
            body["model"],
            json!({"providerID": "anthropic", "id": "claude-sonnet-5-5", "variant": "high"})
        );
        assert!(
            launch(
                &adapter(),
                &request(LaunchRole::TicketAgent, Some("big-pickle"))
            )
            .is_err()
        );
    }

    /// Resume opens the stored session, with a prompt when there is one; nothing is made first.
    #[test]
    fn resume() {
        let mut resumed = request(LaunchRole::TicketAgent, Some("opencode/big-pickle"));
        resumed.resume = Some("ses_eb4534bbfffee5hwyyyzWkBMXI".into());
        resumed.first_prompt = Some(FirstPrompt::Text("address-review".into()));
        let plan = launch(&adapter(), &resumed).unwrap();
        assert_eq!(
            plan.tool_id,
            ToolIdMode::ChosenUpFront("ses_eb4534bbfffee5hwyyyzWkBMXI".into())
        );
        assert_eq!(
            plan.args,
            [
                "--standalone",
                "--session",
                "ses_eb4534bbfffee5hwyyyzWkBMXI",
                "--prompt",
                "address-review"
            ]
        );
        resumed.first_prompt = None;
        let plan = launch(&adapter(), &resumed).unwrap();
        assert_eq!(
            plan.args,
            [
                "--standalone",
                "--session",
                "ses_eb4534bbfffee5hwyyyzWkBMXI"
            ]
        );
        assert_eq!(
            resume_hint("ses_eb4534bb"),
            "opencode --session ses_eb4534bb"
        );
    }

    /// Each role's own variables, as Claude Code's launches set them.
    #[test]
    fn roles() {
        let validator = launch(&adapter(), &request(LaunchRole::Validator, None)).unwrap();
        assert_eq!(env(&validator, "PORT"), None);
        assert_eq!(
            env(&validator, &naming::role_env_var()),
            Some(naming::ROLE_VALIDATOR)
        );
        let mut project = request(LaunchRole::ProjectAgent, None);
        project.ticket = None;
        project.cwd = "/repos/shop-web".into();
        let plan = launch(&adapter(), &project).unwrap();
        assert_eq!(env(&plan, &naming::ticket_env_var()), None);
        assert_eq!(
            env(&plan, &naming::role_env_var()),
            Some(naming::ROLE_PROJECT_AGENT)
        );
        let ToolIdMode::CreatedBeforeLaunch { step, .. } = &plan.tool_id else {
            panic!()
        };
        assert!(step.args.last().unwrap().contains(" · SHOP\""));
        let mut first = project.clone();
        first.first_prompt = Some(FirstPrompt::Skill {
            name: "noyan".into(),
            message: Some("Start with the cart".into()),
        });
        let plan = launch(&adapter(), &first).unwrap();
        assert_eq!(
            plan.args.last().unwrap(),
            &format!("{}-noyan Start with the cart", naming::COMMAND_NAME)
        );
    }

    /// d0011: never a permission or bypass flag, never a config folder that replaces the user's.
    #[test]
    fn never_auto_or_a_config_folder() {
        let plan = launch(&adapter(), &request(LaunchRole::TicketAgent, None)).unwrap();
        assert!(
            !plan
                .args
                .iter()
                .any(|a| a == "--auto" || a.contains("yolo"))
        );
        assert!(
            !plan
                .env
                .iter()
                .any(|(n, _)| n == "OPENCODE_CONFIG_DIR" || n.starts_with("XDG_"))
        );
    }

    /// Without Otaq's plugin folder no launch can be built.
    #[test]
    fn without_the_plugin_folder() {
        let adapter = OpenCode::new(context(Path::new("/nowhere"), None));
        let refusal = unavailable(adapter.context()).unwrap();
        assert!(refusal.0.contains("plugin wasn't found"), "{refusal}");
        assert_eq!(
            launch(&adapter, &request(LaunchRole::TicketAgent, None)),
            Err(refusal)
        );
        assert_eq!(unavailable(self::adapter().context()), None);
    }

    /// The title with and without the ticket's own title.
    #[test]
    fn session_titles() {
        let request = request(LaunchRole::TicketAgent, None);
        let name = naming::PRODUCT_NAME;
        assert_eq!(session_title(&request, None), format!("{name} · SHOP-50"));
        assert_eq!(
            session_title(&request, Some("Order totals rounding")),
            format!("{name} · SHOP-50 · Order totals rounding")
        );
    }

    /// Two ids made at once differ, and a later one sorts before an earlier one (OpenCode's own order).
    #[test]
    fn session_ids_sort_newest_first() {
        let a = prepare::new_session_id(1_792_270_972_289);
        let b = prepare::new_session_id(1_792_270_972_289);
        let later = prepare::new_session_id(1_792_270_999_000);
        assert_ne!(a, b);
        assert!(later < a && later < b);
        assert_eq!(&a[..10], "ses_eb4534");
    }

    /// A stage's skill as a first prompt (AGM-07): shipped and imported alike by their copies' names; the launch is
    /// otherwise the same, and the config names no skill folder and no outside folder.
    #[test]
    fn a_skill_first_prompt() {
        let mut req = request(LaunchRole::Validator, None);
        req.first_prompt = Some(FirstPrompt::Skill {
            name: "validate".into(),
            message: None,
        });
        let shipped = launch(&adapter(), &req).unwrap();
        assert_eq!(
            shipped.args.last().unwrap(),
            &format!("{}-validate", naming::COMMAND_NAME)
        );
        req.first_prompt = Some(FirstPrompt::Skill {
            name: "team-validate".into(),
            message: None,
        });
        let imported = launch(&adapter(), &req).unwrap();
        assert_eq!(
            imported.args.last().unwrap(),
            &format!("{}-team-validate", naming::COMMAND_NAME)
        );
        only_the_prompt_differs(&shipped, &imported);
        let config = config(&imported).to_string();
        assert!(!config.contains("external_directory") && !config.contains("skills"));
    }
}
