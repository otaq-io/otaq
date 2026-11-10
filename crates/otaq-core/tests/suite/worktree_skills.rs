//! Sessions start without skill copies (T2): Otaq never writes skills into a worktree. A session's tool loads Otaq's
//! own skills from Otaq's plugin and the user's where they are; the first prompt and every hand-off type the skill as
//! the tool names it: Otaq's own by the plugin's namespace (`/otaq:work-ticket`, `$otaq:work-ticket`, OpenCode's
//! plain `work-ticket` inside its "Load the skill …" instruction), and a skill a stage names that isn't Otaq's by its own name (`/team-validate`). A stage's
//! `skill:` is the only place a skill is picked.
//!
//! Every tool is a stand-in: `claude` is `sh` and `codex`, `copilot` and `opencode` are `sh` scripts in Otaq's bin
//! folder, each printing its arguments and the skill folders in the folder it starts in. The plugin folder is a
//! temporary one holding the repository's shipped skills. Repos, `OTAQ_HOME` and the tools' home are temporary: no
//! test starts a real tool.
//!
//! The sessions' tests are Unix only: their stand-in tools are `sh` scripts started by name, which Windows can't run
//! (Q598).
#![cfg_attr(not(unix), allow(dead_code, unused_imports))]

#[cfg(unix)]
use crate::common::stand_in::plugin;
use otaq_core::service::testing::said;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::Duration;

use otaq_core::agent_tools::claude::ClaudeLaunch;
use otaq_core::agent_tools::{LaunchContext, Tool};
use otaq_core::agents::{AgentModel, HoldReason, SessionRole};
use otaq_core::board::moves::DropTarget;
use otaq_core::board::settings::WorkflowStep;
use otaq_core::board::tickets::Ticket;
use otaq_core::domain::{Actor, MissingSkill, StartStep, Status, ValidationTrigger};
use otaq_core::flow::{FlowFile, yaml};
use otaq_core::git::testing::{commit, git};
use otaq_core::naming::PLUGIN_NAMESPACE;
use otaq_core::pr::service::PrOutcome;
use otaq_core::service::ServiceConfig;
use otaq_core::service::testing::TestService;
use otaq_core::session::SessionKind;
use otaq_core::skills::testing::claude_skill;
use tempfile::TempDir;

/// Prints its arguments, OpenCode's launch config and the skill folders of the launch folder, then echoes what is
/// typed.
const SCRIPT: &str = "printf 'ARGS %s\\n' \"$*\"; \
                      printf 'CONFIG %s\\n' \"${OPENCODE_CONFIG_CONTENT-none}\"; \
                      printf 'AGENTS %s.\\n' \"$(ls .agents/skills 2>/dev/null | tr '\\n' ' ')\"; \
                      printf 'CLAUDE %s.\\n' \"$(ls .claude/skills 2>/dev/null | tr '\\n' ' ')\"; exec cat";

fn me() -> Actor {
    Actor::User("me".into())
}

/// An executable `name` in `bin` running [`SCRIPT`]; OpenCode's also answers the conversation it is asked to make
/// before the launch (`opencode api --standalone POST /api/session …`).
#[cfg(unix)]
fn stand_in(bin: &Path, name: &str) {
    let file = bin.join(name);
    std::fs::write(
        &file,
        format!("#!/bin/sh\ncase \"$1\" in api) exit 0 ;; esac\n{SCRIPT}\n"),
    )
    .unwrap();
    std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o755)).unwrap();
}

struct World {
    w: TestService,
    /// The tools' home: the finder reads the personal skill folders here (`.claude/skills`, `.codex/skills`…).
    home: PathBuf,
    _dir: TempDir,
}

/// A service whose four tools are stand-ins and whose plugin folder is [`plugin`]'s, with shop-web (a local origin:
/// Open PR needs a skill of the user's, REV-11).
#[cfg(unix)]
async fn world() -> World {
    let dir = TempDir::new().unwrap();
    let bin = dir.path().join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    for tool in [Tool::Codex, Tool::Copilot, Tool::OpenCode] {
        stand_in(&bin, tool.command_name());
    }
    let plugin_dir = plugin(dir.path());
    let home = dir.path().join("home");
    std::fs::create_dir_all(&home).unwrap();
    let tool_home = home.clone();
    let w = TestService::start_with(move |config| ServiceConfig {
        tool_home: Some(tool_home),
        launch: Some(LaunchContext {
            bin_dir: bin,
            plugin_dir,
            path: std::env::var_os("PATH").unwrap_or_default(),
            claude: ClaudeLaunch {
                program: vec!["sh".into(), "-c".into(), SCRIPT.into(), "sh".into()],
                config_dir: None,
            },
        }),
        ..config
    })
    .await;
    let repo = w.repos.clone_with_origin("shop-web", "main", 1);
    w.board
        .add_project(&otaq_core::board::projects::AddProject {
            path: repo.display().to_string(),
            ..Default::default()
        })
        .await
        .unwrap();
    World { w, home, _dir: dir }
}

fn worktree_of(t: &Ticket) -> PathBuf {
    PathBuf::from(t.work.as_ref().unwrap().worktree.clone().unwrap())
}

/// Starts `t` while waiting to begin and returns its agent session's output once it printed the skill folders.
async fn started(w: &TestService, t: &Ticket) -> String {
    let key = t.key.to_string();
    w.board
        .move_ticket(&key, DropTarget::ReadyForDev, true, &me())
        .await
        .unwrap();
    w.starter
        .start(&key, &me(), otaq_core::session::Start::Auto)
        .await
        .unwrap();
    let agent = tokio::time::timeout(Duration::from_secs(20), async {
        loop {
            if let Some(s) = w.sessions.running_of(&t.id, SessionKind::Agent) {
                return s;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap_or_else(|_| {
        panic!("{key}'s agent never ran");
    });
    w.wait_for_output(&agent.id, "CLAUDE").await
}

/// A ticket in Ready for Review with a started (linked) worktree and one commit of its own.
async fn in_review(w: &TestService, title: &str) -> (Ticket, PathBuf) {
    let t = w.create("SHOP", title).await;
    let worktree = w.start_worktree(&t).await;
    commit(&worktree, "login.ts", "redirect\n");
    w.force_status(&t, Status::Working).await;
    let key = t.key.to_string();
    w.board
        .submit_for_review(
            &key,
            said(&w.board, &key, "Redirects after login.")
                .await
                .as_ref(),
            &me(),
        )
        .await
        .unwrap();
    (w.board.ticket(&key).await.unwrap(), worktree)
}

/// No skill folder in `worktree`, and git sees nothing new.
fn nothing_written(worktree: &Path) {
    assert!(!worktree.join(".agents").exists(), "{}", worktree.display());
    assert!(!worktree.join(".claude").exists(), "{}", worktree.display());
    assert_eq!(
        git(
            worktree,
            &["status", "--porcelain", "--untracked-files=all"]
        ),
        ""
    );
}

fn every_kind_file() -> FlowFile {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/flows/every-kind.yaml");
    yaml::read(&std::fs::read_to_string(path).unwrap()).unwrap()
}

/// On every tool, a Dev session starts with nothing written into its worktree, and its first prompt runs Otaq's
/// `work-ticket` from the plugin: Claude Code `/otaq:work-ticket`, Codex `$otaq:work-ticket`, Copilot
/// `-i /otaq:work-ticket`, OpenCode `--prompt Load the skill `work-ticket` with your skill tool…`.
#[cfg(unix)]
#[tokio::test(flavor = "multi_thread")]
async fn a_dev_session_on_each_tool_starts_with_nothing_written() {
    let World { w, _dir, .. } = world().await;
    let ns = PLUGIN_NAMESPACE;
    for (n, (tool, prompt)) in [
        (Tool::Claude, format!("/{ns}:work-ticket")),
        (Tool::Codex, format!("${ns}:work-ticket")),
        (Tool::Copilot, format!("-i /{ns}:work-ticket")),
        (
            Tool::OpenCode,
            "--prompt Load the skill `work-ticket` with your skill tool and follow it. If you don't have a skill with \
             that name, say so and stop."
                .to_owned(),
        ),
    ]
    .into_iter()
    .enumerate()
    {
        w.set_agent_model("Developer", AgentModel::Tool { tool, model: None })
            .await;
        let t = w.create("SHOP", &format!("Ticket {n}")).await;
        let out = started(&w, &t).await;
        let args = out.lines().find(|l| l.starts_with("ARGS")).unwrap();
        assert!(args.ends_with(&prompt), "{tool:?}: {args}");
        assert!(out.contains("AGENTS ."), "{tool:?}: {out}");
        assert!(out.contains("CLAUDE ."), "{tool:?}: {out}");
        let worktree = worktree_of(&w.board.ticket(&t.key.to_string()).await.unwrap());
        nothing_written(&worktree);
        let config = out.lines().find(|l| l.starts_with("CONFIG")).unwrap();
        // OpenCode's config names Otaq's own skills folder in its plugin (T1), never a folder in the worktree.
        assert!(
            !config.contains(".claude/skills") && !config.contains(".agents/skills"),
            "{tool:?}: {config}"
        );
    }
    w.stop().await;
}

/// The test flow's Product spec (Q261): the ticket's first session runs Otaq's `product-spec` from the plugin.
#[cfg(unix)]
#[tokio::test(flavor = "multi_thread")]
async fn the_product_spec_stage_starts_with_otaqs_product_spec() {
    let World { w, _dir, .. } = world().await;
    w.board.use_flow_for_tests("SHOP", &every_kind_file()).await;
    let t = w.create("SHOP", "Coupon codes").await;
    let out = started(&w, &t).await;
    let args = out.lines().find(|l| l.starts_with("ARGS")).unwrap();
    assert!(args.ends_with(&claude_skill("product-spec")), "{args}");
    assert!(out.contains("AGENTS ."), "{out}");
    w.stop().await;
}

/// A stage's `skill:` is the only place a skill is picked. The validator of a Validate stage that names no skill of
/// its own runs Otaq's (`/otaq:validate`); one whose stage names the user's `team-validate` runs it by its own name
/// (`/team-validate`), which the tool finds itself (here in `~/.claude/skills`). Nothing is written into the
/// worktree, so the verdict counts (VAL-02, d0063).
#[cfg(unix)]
#[tokio::test(flavor = "multi_thread")]
async fn a_stage_names_the_users_skill_by_its_own_name() {
    let World { w, home, _dir } = world().await;
    skill_in(&home.join(".claude/skills"), "team-validate");
    for (n, named) in [None, Some("team-validate")].into_iter().enumerate() {
        w.name_stage_skill(WorkflowStep::Validate, named).await;
        let t = w.create("SHOP", &format!("Cart totals {n}")).await;
        let worktree = w.give_worktree(&t).await;
        w.force_status(&t, Status::Working).await;
        let key = t.key.to_string();
        w.validation
            .submit(
                &key,
                said(&w.board, &key, "Cart totals add up.").await.as_ref(),
                &Actor::Agent(t.key.clone()),
                ValidationTrigger::Agent,
            )
            .await
            .unwrap();
        let validator = w
            .sessions
            .running_of(&t.id, SessionKind::Validator)
            .unwrap();
        let out = w.wait_for_output(&validator.id, "CLAUDE").await;
        let typed = match named {
            Some(skill) => format!("/{skill}"),
            None => format!("/{PLUGIN_NAMESPACE}:validate"),
        };
        assert_eq!(typed, claude_skill(named.unwrap_or("validate")));
        let args = out.lines().find(|l| l.starts_with("ARGS")).unwrap();
        assert!(args.ends_with(&typed), "{args}");
        assert!(out.contains("AGENTS ."), "{out}");
        nothing_written(&worktree);
        let after = w
            .validation
            .verdict(
                &key,
                true,
                said(&w.board, &key, "Totals are right.").await.as_ref(),
                &validator.id,
            )
            .await
            .unwrap();
        assert_eq!(after.status, Status::WaitingForYou, "not discarded");
    }
    w.stop().await;
}

/// Open PR typed into a live session Otaq didn't launch (an `sh`): the Delivery stage's `skill: my-pr` is typed by
/// its own name, as a command, since every session has the skills it can run from its start; Push to PR types Otaq's
/// `push`. Nothing is written into the worktree. The flow shows the skill's own name (WSK-08).
#[cfg(unix)]
#[tokio::test(flavor = "multi_thread")]
async fn open_pr_and_push_are_typed_by_name_in_a_live_session() {
    let World { w, _dir, .. } = world().await;
    w.name_stage_skill(WorkflowStep::OpenPr, Some("my-pr"))
        .await;
    let (t, worktree) = in_review(&w, "Login redirect").await;
    let agent = w.agent_shell(&t).await;
    // The dialog opens on the prepared text; the skill is typed once the user confirms.
    let outcome = w.prs.open("SHOP-1", false, false, &me()).await.unwrap();
    assert!(matches!(outcome, PrOutcome::Started { .. }), "{outcome:?}");
    assert_eq!(w.board.pr_flow_view("SHOP-1").await.unwrap().skill, "my-pr");
    let proposal = w
        .board
        .pr_flow(&t.id)
        .await
        .unwrap()
        .unwrap()
        .proposal
        .unwrap();
    let outcome = w
        .prs
        .confirm("SHOP-1", &proposal, false, &me())
        .await
        .unwrap();
    assert!(matches!(outcome, PrOutcome::Started { .. }), "{outcome:?}");
    w.wait_for_output(&agent.session, &claude_skill("my-pr"))
        .await;
    sqlx::query("DELETE FROM pr_flows WHERE ticket_id = ?")
        .bind(&t.id)
        .execute(w.board.store().pool())
        .await
        .unwrap();

    sqlx::query("UPDATE tickets SET pr_number = 481 WHERE id = ?")
        .bind(&t.id)
        .execute(w.board.store().pool())
        .await
        .unwrap();
    let outcome = w.prs.open("SHOP-1", true, false, &me()).await.unwrap();
    assert!(matches!(outcome, PrOutcome::Started { .. }), "{outcome:?}");
    w.wait_for_output(&agent.session, &claude_skill("push"))
        .await;
    assert_eq!(w.board.pr_flow_view("SHOP-1").await.unwrap().skill, "push");
    nothing_written(&worktree);
    w.stop().await;
}

/// Open PR with no live session resumes the conversation with the flow's Open PR skill as its first prompt, by its
/// own name, and nothing written into the worktree. The resumed tool (Claude Code) has the skill in
/// `~/.claude/skills`.
#[cfg(unix)]
#[tokio::test(flavor = "multi_thread")]
async fn open_pr_resumes_with_the_skill_by_name() {
    let World { w, home, _dir } = world().await;
    skill_in(&home.join(".claude/skills"), "my-pr");
    w.name_stage_skill(WorkflowStep::OpenPr, Some("my-pr"))
        .await;
    let (t, worktree) = in_review(&w, "Login redirect").await;
    w.record_ended_agent(&t, "3be94c10-0000-7000-8000-000000000000")
        .await;
    let outcome = w.prs.open("SHOP-1", true, false, &me()).await.unwrap();
    assert!(matches!(outcome, PrOutcome::Started { .. }), "{outcome:?}");
    let proposal = w
        .board
        .pr_flow(&t.id)
        .await
        .unwrap()
        .unwrap()
        .proposal
        .unwrap();
    // The confirm resumes the conversation with the skill, told the user confirmed.
    let outcome = w
        .prs
        .confirm("SHOP-1", &proposal, false, &me())
        .await
        .unwrap();
    assert!(matches!(outcome, PrOutcome::Started { .. }), "{outcome:?}");
    let session = w
        .sessions
        .running_of(&t.id, SessionKind::Agent)
        .expect("resumed");
    let out = w.wait_for_output(&session.id, "CLAUDE").await;
    let args = out.lines().find(|l| l.starts_with("ARGS")).unwrap();
    assert!(args.contains(&claude_skill("my-pr")), "{args}");
    assert!(
        args.contains("The user confirmed the pull request"),
        "{args}"
    );
    assert!(out.contains("AGENTS ."), "{out}");
    nothing_written(&worktree);
    w.stop().await;
}

/// No code in the core names an M27 copy (`otaq-custom-<step>`): only the leftover tidy knows those names, to remove
/// them.
#[test]
fn no_old_skill_names_in_the_core() {
    fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                rust_files(&path, out);
            } else if path.extension().is_some_and(|e| e == "rs") {
                out.push(path);
            }
        }
    }
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    rust_files(&src, &mut files);
    assert!(files.len() > 100);
    for file in files {
        let rel = file
            .strip_prefix(&src)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        let text = std::fs::read_to_string(&file).unwrap();
        if rel != "skills/copy.rs" {
            assert!(!text.contains("}-custom-"), "{rel} names an M27 copy");
            assert!(!text.contains("custom_skill"), "{rel}");
        }
    }
}

/// Names `skill` on the default flow's Dev stage of SHOP.
async fn name_dev_skill(w: &TestService, skill: &str) {
    w.board
        .edit_flow_for_tests("SHOP", |file| {
            for stage in &mut file.flow.stages {
                if stage.id == otaq_core::flow::default::DEV {
                    stage.skill = Some(skill.to_owned());
                }
            }
        })
        .await;
}

/// A skill's folder holding its SKILL.md under `skills`.
fn skill_in(skills: &Path, name: &str) -> PathBuf {
    let folder = skills.join(name);
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::write(
        folder.join("SKILL.md"),
        format!("---\nname: {name}\ndescription: The team's way.\n---\n\nDo it.\n"),
    )
    .unwrap();
    folder
}

/// Codex's home set up as Otaq's check wants it before a launch: Otaq's plugin installed from Otaq's plugin folder and
/// its five hooks trusted (what a Re-check writes).
fn codex_set_up(w: &TestService, home: &Path) {
    let ns = PLUGIN_NAMESPACE;
    let id = format!("{ns}@{ns}-local");
    let mut config = format!(
        "[marketplaces.{ns}-local]\nsource_type = \"local\"\nsource = {:?}\n\n[plugins.\"{id}\"]\nenabled = true\n",
        w.data.codex_plugin_dir().display().to_string()
    );
    for event in [
        "session_start",
        "permission_request",
        "stop",
        "interrupt",
        "user_prompt_submit",
    ] {
        config.push_str(&format!(
            "\n[hooks.state.\"{id}:plugin.json#hooks[0]:{event}:0:0\"]\ntrusted_hash = \"sha256:{event}\"\n"
        ));
    }
    let codex = home.join(".codex");
    std::fs::create_dir_all(&codex).unwrap();
    std::fs::write(codex.join("config.toml"), config).unwrap();
}

/// The ticket's hold of `role`: its reason, tool and skill.
async fn hold_of(
    w: &TestService,
    t: &Ticket,
    role: SessionRole,
) -> Option<(HoldReason, Tool, Option<String>)> {
    w.board
        .store()
        .ticket_holds(&t.id)
        .await
        .unwrap()
        .into_iter()
        .find(|h| h.role == role)
        .map(|h| (h.reason, h.tool, h.skill))
}

/// The ticket's Needs you line, if it has an item.
async fn needs_you_line(w: &TestService, t: &Ticket) -> Option<String> {
    w.needs_you
        .list()
        .await
        .unwrap()
        .items
        .into_iter()
        .find(|i| i.id == t.id)
        .map(|i| i.reason)
}

/// A stage runs only on a tool that has its skill: a Validate stage naming `team-validate`, run on a Codex agent with
/// the skill only in `~/.claude/skills` (Claude Code's, not Codex's), doesn't start. Its hold says why, the ticket waits
/// in Needs you with "Codex doesn't have the skill `/team-validate`", and nothing is typed into any session. Check
/// again before the skill reaches Codex keeps it held; once it is linked into `~/.codex/skills`, Check again starts
/// the check on Codex with the skill by its own name.
#[cfg(unix)]
#[tokio::test(flavor = "multi_thread")]
async fn a_check_whose_tool_lacks_its_skill_waits_until_check_again_finds_it() {
    let World { w, home, _dir } = world().await;
    codex_set_up(&w, &home);
    let claude_copy = skill_in(&home.join(".claude/skills"), "team-validate");
    w.name_stage_skill(WorkflowStep::Validate, Some("team-validate"))
        .await;
    w.set_agent_model(
        "Checker",
        AgentModel::Tool {
            tool: Tool::Codex,
            model: None,
        },
    )
    .await;
    let t = w.create("SHOP", "Cart totals").await;
    w.give_worktree(&t).await;
    w.force_status(&t, Status::Working).await;
    let key = t.key.to_string();
    w.validation
        .submit(
            &key,
            said(&w.board, &key, "Cart totals add up.").await.as_ref(),
            &Actor::Agent(t.key.clone()),
            ValidationTrigger::Agent,
        )
        .await
        .unwrap();
    let held = Some((
        HoldReason::MissingSkill,
        Tool::Codex,
        Some("team-validate".to_owned()),
    ));
    assert_eq!(hold_of(&w, &t, SessionRole::Validator).await, held);
    let ticket = w.board.ticket(&key).await.unwrap();
    assert_eq!(ticket.status, Status::Checking);
    let missing = MissingSkill {
        tool: Tool::Codex,
        skill: "team-validate".into(),
    };
    assert_eq!(ticket.flags.missing_skill.as_ref(), Some(&missing));
    let text = "Codex doesn\u{2019}t have the skill `/team-validate`";
    assert_eq!(missing.text(), text);
    assert_eq!(needs_you_line(&w, &t).await.as_deref(), Some(text));
    let typed_nowhere = |w: &TestService| {
        w.sessions
            .running_all_of(&t.id, SessionKind::Validator)
            .is_empty()
    };
    assert!(typed_nowhere(&w), "no checker started");

    // Check again before the skill reaches Codex: still held, still nothing typed.
    assert!(!w.gate.check_again(&t.id).await.unwrap());
    assert_eq!(hold_of(&w, &t, SessionRole::Validator).await, held);
    assert!(typed_nowhere(&w));

    // Linked into Codex's own folder: Check again finds it and the check starts on Codex.
    let codex = home.join(".codex/skills");
    std::fs::create_dir_all(&codex).unwrap();
    std::os::unix::fs::symlink(&claude_copy, codex.join("team-validate")).unwrap();
    assert!(w.gate.check_again(&t.id).await.unwrap());
    let checker = tokio::time::timeout(Duration::from_secs(20), async {
        loop {
            if let Some(s) = w.sessions.running_of(&t.id, SessionKind::Validator) {
                return s;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("the check started");
    assert_eq!(checker.tool, Some(Tool::Codex));
    let out = w.wait_for_output(&checker.id, "CLAUDE").await;
    let typed = w.adapters.get(Tool::Codex).skill_reference("team-validate");
    let args = out.lines().find(|l| l.starts_with("ARGS")).unwrap();
    assert!(args.contains(&typed), "{args}");
    assert_eq!(hold_of(&w, &t, SessionRole::Validator).await, None);
    let ticket = w.board.ticket(&key).await.unwrap();
    assert_eq!(ticket.flags.missing_skill, None);
    assert_eq!(needs_you_line(&w, &t).await, None);
    w.stop().await;
}

/// A round held for its agent's tool and then moved by Change agent… to an agent whose tool has the skill is recorded
/// under the agent that runs it, not the held one: History and the next stage's agent read the round's agent.
#[cfg(unix)]
#[tokio::test(flavor = "multi_thread")]
async fn a_held_round_moved_to_another_agent_is_recorded_under_that_agent() {
    use otaq_core::client::{Client, ClientConfig};
    use otaq_core::protocol::{Request, SetChoiceParams};
    let World { w, home, _dir } = world().await;
    codex_set_up(&w, &home);
    skill_in(&home.join(".claude/skills"), "team-validate");
    w.name_stage_skill(WorkflowStep::Validate, Some("team-validate"))
        .await;
    w.set_agent_model(
        "Checker",
        AgentModel::Tool {
            tool: Tool::Codex,
            model: None,
        },
    )
    .await;
    let t = w.create("SHOP", "Cart totals").await;
    w.give_worktree(&t).await;
    w.force_status(&t, Status::Working).await;
    let key = t.key.to_string();
    w.validation
        .submit(
            &key,
            said(&w.board, &key, "Cart totals add up.").await.as_ref(),
            &Actor::Agent(t.key.clone()),
            ValidationTrigger::Agent,
        )
        .await
        .unwrap();
    assert!(hold_of(&w, &t, SessionRole::Validator).await.is_some());

    // Change agent…: the Developer, on Claude Code, which has the skill.
    let client = Client::connect(ClientConfig::new(&w.data)).await.unwrap();
    client
        .request(Request::TicketSetChoice(SetChoiceParams {
            key: key.clone(),
            stage: "Validate".into(),
            choice: "Developer".into(),
        }))
        .await
        .unwrap();
    let checker = tokio::time::timeout(Duration::from_secs(20), async {
        loop {
            if let Some(s) = w.sessions.running_of(&t.id, SessionKind::Validator) {
                return s;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("the check started on the chosen agent");
    assert_eq!(checker.tool, Some(Tool::Claude));
    // The session shows as running a moment before its start is stored on the round's row.
    let rounds = tokio::time::timeout(Duration::from_secs(20), async {
        loop {
            let rounds: Vec<(Option<String>, Option<String>)> =
                sqlx::query_as("SELECT agent, session_id FROM validations WHERE ticket_id = ?")
                    .bind(&t.id)
                    .fetch_all(w.board.store().pool())
                    .await
                    .unwrap();
            if rounds.iter().all(|(_, session)| session.is_some()) {
                return rounds;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("the checker's start is stored on its round");
    assert_eq!(
        rounds,
        [(Some("Developer".to_owned()), Some(checker.id.clone()))]
    );
    w.stop().await;
}

/// An Agent stage whose tool doesn't have its skill is held before it starts, like a tool that can't run: the ticket
/// is Held with the reason as its start problem, no session starts, and it waits in Needs you. Once the skill is
/// committed to the project, Check again sends it back to Queued and it starts with the skill by its own name.
#[cfg(unix)]
#[tokio::test(flavor = "multi_thread")]
async fn an_agent_stage_whose_tool_lacks_its_skill_is_held_until_check_again() {
    let World { w, _dir, .. } = world().await;
    name_dev_skill(&w, "team-dev").await;
    let t = w.create("SHOP", "Cart totals").await;
    let key = t.key.to_string();
    w.board
        .move_ticket(&key, DropTarget::ReadyForDev, true, &me())
        .await
        .unwrap();
    let outcome = w
        .starter
        .start(&key, &me(), otaq_core::session::Start::Auto)
        .await
        .unwrap();
    let text = "Claude Code doesn\u{2019}t have the skill `/team-dev`";
    assert!(
        matches!(&outcome, otaq_core::start::StartOutcome::Held { message } if message == text),
        "{outcome:?}"
    );
    let ticket = w.board.ticket(&key).await.unwrap();
    assert_eq!(ticket.status, Status::Held);
    let problem = ticket.start_problem.clone().unwrap();
    assert_eq!(
        (problem.step, problem.message.as_str()),
        (StartStep::MissingSkill, text)
    );
    assert_eq!(
        hold_of(&w, &t, SessionRole::Developer).await,
        Some((
            HoldReason::MissingSkill,
            Tool::Claude,
            Some("team-dev".to_owned())
        ))
    );
    assert!(w.sessions.running_of(&t.id, SessionKind::Agent).is_none());
    assert_eq!(needs_you_line(&w, &t).await.as_deref(), Some(text));
    assert!(!w.gate.check_again(&t.id).await.unwrap(), "still missing");

    // Committed to the project: the worktree it starts in will have it.
    let checkout = PathBuf::from(w.board.project("SHOP").await.unwrap().repo_path);
    skill_in(&checkout.join(".claude/skills"), "team-dev");
    commit(
        &checkout,
        ".claude/skills/team-dev/SKILL.md",
        &std::fs::read_to_string(checkout.join(".claude/skills/team-dev/SKILL.md")).unwrap(),
    );
    git(&checkout, &["push", "-q", "origin", "main"]);
    assert!(w.gate.check_again(&t.id).await.unwrap());
    assert_ne!(w.status(&key).await, Status::Held, "back to Queued");
    assert_eq!(needs_you_line(&w, &t).await, None);
    let _ = w
        .starter
        .start(&key, &me(), otaq_core::session::Start::Auto)
        .await;
    let agent = tokio::time::timeout(Duration::from_secs(20), async {
        loop {
            if let Some(s) = w.sessions.running_of(&t.id, SessionKind::Agent) {
                return s;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("started");
    let out = w.wait_for_output(&agent.id, "CLAUDE").await;
    let args = out.lines().find(|l| l.starts_with("ARGS")).unwrap();
    assert!(args.ends_with(&claude_skill("team-dev")), "{args}");
    assert_eq!(hold_of(&w, &t, SessionRole::Developer).await, None);
    w.stop().await;
}

/// Puts Codex's variable for SHOP's sessions empty, which Codex takes as unset: the finder then reads Codex's own
/// `~/.codex/skills` whatever the test's own environment says.
async fn codex_home_unset(w: &TestService) {
    let mut settings = w.board.project_settings("SHOP").await.unwrap();
    settings.env_vars = vec![otaq_core::board::settings::EnvVar::new("CODEX_HOME", "")];
    w.board
        .set_project_settings("SHOP", &settings)
        .await
        .unwrap();
}

/// SHOP's stage warnings for the skills its flow names, as the flow editor shows them.
async fn stage_warnings(w: &TestService) -> Vec<(Option<String>, String)> {
    let file = w.board.current_flow_file("SHOP").await.unwrap();
    let stored = w.board.store().agents().await.unwrap();
    w.board
        .skill_warnings("SHOP", &file, &stored)
        .await
        .into_iter()
        .map(|w| (w.stage, w.text))
        .collect()
}

/// Names `reltest-docs` (in Codex's own `~/.codex/skills`) on Dev and puts Developer on Codex, `CODEX_HOME` unset.
async fn dev_on_codex_with_personal_skill(w: &TestService, home: &Path) -> PathBuf {
    codex_set_up(w, home);
    codex_home_unset(w).await;
    let folder = skill_in(&home.join(".codex/skills"), "reltest-docs");
    name_dev_skill(w, "reltest-docs").await;
    w.set_agent_model(
        "Developer",
        AgentModel::Tool {
            tool: Tool::Codex,
            model: None,
        },
    )
    .await;
    folder
}

/// A personal Codex skill (`~/.codex/skills/<name>/SKILL.md`, `CODEX_HOME` unset) is Codex's: Pick a skill… lists it
/// as Personal with Codex, a Dev stage naming it on a Codex agent has no warning, and a ticket starts there with the
/// skill typed as Codex names it.
#[cfg(unix)]
#[tokio::test(flavor = "multi_thread")]
async fn a_personal_codex_skill_is_codexs_and_its_stage_starts() {
    let World { w, home, _dir } = world().await;
    dev_on_codex_with_personal_skill(&w, &home).await;

    let found = w.board.found_skills("SHOP").await.unwrap();
    let docs = found.iter().find(|s| s.name == "reltest-docs").unwrap();
    assert_eq!(docs.tools, [Tool::Codex]);
    assert!(docs.skipped.is_empty());
    assert_eq!(
        docs.places[0].scope,
        otaq_core::skills::found::SkillScope::Personal
    );
    assert_eq!(stage_warnings(&w).await, []);

    let t = w.create("SHOP", "Update the docs").await;
    let out = started(&w, &t).await;
    let typed = w.adapters.get(Tool::Codex).skill_reference("reltest-docs");
    let args = out.lines().find(|l| l.starts_with("ARGS")).unwrap();
    assert!(args.contains(&typed), "{args}");
    assert_eq!(hold_of(&w, &t, SessionRole::Developer).await, None);
    w.stop().await;
}

/// The same skill saved with a UTF-8 byte order mark (as Windows PowerShell 5 writes it) isn't loaded by Codex, so
/// the stage is still held, but the finder says why instead of "you don't have it": Pick a skill… has Codex skipping
/// it and the stage warns "Codex doesn’t load the skill `reltest-docs`: its SKILL.md starts with a byte order mark".
#[cfg(unix)]
#[tokio::test(flavor = "multi_thread")]
async fn a_codex_skill_with_a_byte_order_mark_says_why_it_is_held() {
    let World { w, home, _dir } = world().await;
    let folder = dev_on_codex_with_personal_skill(&w, &home).await;
    let text = std::fs::read_to_string(folder.join("SKILL.md")).unwrap();
    std::fs::write(folder.join("SKILL.md"), format!("\u{feff}{text}")).unwrap();

    let found = w.board.found_skills("SHOP").await.unwrap();
    let docs = found.iter().find(|s| s.name == "reltest-docs").unwrap();
    assert!(docs.tools.is_empty());
    assert_eq!(docs.skipped, [Tool::Codex]);
    assert_eq!(
        stage_warnings(&w).await,
        [(
            Some("Dev".to_owned()),
            "Codex doesn\u{2019}t load the skill `reltest-docs`: its SKILL.md starts with a byte order mark"
                .to_owned()
        )]
    );

    let t = w.create("SHOP", "Update the docs").await;
    let key = t.key.to_string();
    w.board
        .move_ticket(&key, DropTarget::ReadyForDev, true, &me())
        .await
        .unwrap();
    let outcome = w
        .starter
        .start(&key, &me(), otaq_core::session::Start::Auto)
        .await
        .unwrap();
    assert!(
        matches!(&outcome, otaq_core::start::StartOutcome::Held { .. }),
        "{outcome:?}"
    );
    assert!(w.sessions.running_of(&t.id, SessionKind::Agent).is_none());
    w.stop().await;
}
