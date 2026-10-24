//! Skills by one name in the worktree (T-0408; AGM-07, WSK-03, WSK-08, ADP-05; d0108, d0098, d0092, d0023, d0011,
//! d0063): before a session starts, every skill of its agent and the skill it runs first are copied into the
//! worktree as `.agents/skills/otaq-<name>/` and `.claude/skills/otaq-<name>/`, with a `.gitignore` holding `*` and
//! Otaq's marker; shipped skills come from the app's plugin folder, imported ones from Otaq's copy with their own
//! files; the first prompt runs the stage's own skill as each tool names a project skill. Git never sees the copies.
//!
//! Every tool is a stand-in: `claude` is `sh` and `codex`, `copilot` and `opencode` are `sh` scripts in Otaq's bin
//! folder, each printing its arguments and the copies in the folder it starts in. The plugin folder is a temporary
//! one holding the repository's shipped skills. Skill folders, repos, `OTAQ_HOME` and the tools' home are temporary:
//! no test starts a real tool.

use crate::common::stand_in::plugin;
use otaq_core::service::testing::said;
use std::collections::BTreeMap;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::Duration;

use otaq_core::agent_tools::claude::{ClaudeLaunch, find_plugin_dir};
use otaq_core::agent_tools::{LaunchContext, Tool};
use otaq_core::agents::AgentModel;
use otaq_core::board::moves::DropTarget;
use otaq_core::board::settings::WorkflowStep;
use otaq_core::board::tickets::Ticket;
use otaq_core::domain::{Actor, Status, ValidationTrigger};
use otaq_core::flow::{FlowFile, yaml};
use otaq_core::git::testing::{commit, git};
use otaq_core::pr::service::PrOutcome;
use otaq_core::review::LocalMergeOutcome;
use otaq_core::service::ServiceConfig;
use otaq_core::service::testing::TestService;
use otaq_core::session::SessionKind;
use otaq_core::session::deliveries::handoff_sentence;
use otaq_core::skills::copy::{Marker, SKILL_ROOTS, is_marked, marked_copies};
use otaq_core::skills::testing::skill_folder;
use otaq_core::skills::{Source, write_session};
use tempfile::TempDir;

/// Prints its arguments, OpenCode's launch config and the copies in the launch folder's two skill roots, then echoes
/// what is typed.
const SCRIPT: &str = "printf 'ARGS %s\\n' \"$*\"; \
                      printf 'CONFIG %s\\n' \"${OPENCODE_CONFIG_CONTENT-none}\"; \
                      printf 'AGENTS %s.\\n' \"$(ls .agents/skills 2>/dev/null | tr '\\n' ' ')\"; \
                      printf 'CLAUDE %s.\\n' \"$(ls .claude/skills 2>/dev/null | tr '\\n' ' ')\"; exec cat";

fn me() -> Actor {
    Actor::User("me".into())
}

/// An executable `name` in `bin` running [`SCRIPT`]; OpenCode's also answers the conversation it is asked to make
/// before the launch (`opencode api --standalone POST /api/session …`).
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
    _dir: TempDir,
}

/// A service whose four tools are stand-ins and whose plugin folder is [`plugin`]'s, with shop-web (a local origin:
/// Open PR needs an imported skill, REV-11).
async fn world() -> World {
    let dir = TempDir::new().unwrap();
    let bin = dir.path().join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    for tool in [Tool::Codex, Tool::Copilot, Tool::OpenCode] {
        stand_in(&bin, tool.command_name());
    }
    let plugin_dir = plugin(dir.path());
    let w = TestService::start_with(move |config| ServiceConfig {
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
    World { w, _dir: dir }
}

fn worktree_of(t: &Ticket) -> PathBuf {
    PathBuf::from(t.work.as_ref().unwrap().worktree.clone().unwrap())
}

/// Starts `t` from Ready for Dev and returns its agent session's output once it printed the copies.
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

/// Whether both copies `name` are in `folder`, each marked.
fn copied(folder: &Path, name: &str) -> bool {
    SKILL_ROOTS.iter().all(|root| {
        let copy = folder.join(root).join(name);
        copy.join("SKILL.md").is_file() && is_marked(&copy)
    })
}

/// Every file under `path` with its contents: "nothing in `.git` was written".
fn files(path: &Path) -> BTreeMap<String, Vec<u8>> {
    otaq_core::git::testing::snapshot(path)
        .into_iter()
        .filter(|rel| path.join(rel).is_file())
        .map(|rel| {
            let contents = std::fs::read(path.join(&rel)).unwrap();
            (rel, contents)
        })
        .collect()
}

fn mode(path: &Path) -> u32 {
    std::fs::metadata(path).unwrap().permissions().mode() & 0o777
}

/// The example's `my-commit` as a team folder: SKILL.md (CRLF line ends, `disable-model-invocation`, a second
/// `name:` in the body), reference.md, an executable script and a `.gitignore` of its own.
fn my_commit(root: &Path) -> PathBuf {
    let folder = root.join("team").join("my-commit");
    std::fs::create_dir_all(folder.join("scripts")).unwrap();
    std::fs::write(
        folder.join("SKILL.md"),
        "---\r\nname: my-commit\r\ndescription: Commits in the team's style.\r\ndisable-model-invocation: true\r\n\
         ---\r\nname: stays\r\nRun `bash scripts/check-staged.sh`.\r\n",
    )
    .unwrap();
    std::fs::write(folder.join("reference.md"), "notes: sort by date\n").unwrap();
    std::fs::write(folder.join(".gitignore"), "build/\n").unwrap();
    let script = folder.join("scripts/check-staged.sh");
    std::fs::write(&script, "#!/bin/sh\necho CHECK-STAGED-OK\n").unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    folder
}

fn every_kind_file() -> FlowFile {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/flows/every-kind.yaml");
    yaml::read(&std::fs::read_to_string(path).unwrap()).unwrap()
}

/// The writer in a linked worktree (`git worktree add`): both roots with the name set and the flag gone, the rest
/// byte for byte, modes kept, `.gitignore` holding `*` and the marker; git status and `git add -A` see nothing;
/// nothing in `.git` changes; M27's `otaq-custom-commit` goes; a user's own committed `otaq-mine` stays.
#[test]
fn the_copies_in_a_linked_worktree() {
    let dir = tempfile::tempdir().unwrap();
    let repos = otaq_core::git::testing::TempRepos::new();
    let repo = repos.repo("pocket-notes", 1);
    let mine = repo.join(".claude/skills/otaq-mine");
    std::fs::create_dir_all(&mine).unwrap();
    std::fs::write(mine.join("SKILL.md"), "---\nname: otaq-mine\n---\nMine.\n").unwrap();
    git(&repo, &["add", "-A"]);
    git(&repo, &["commit", "-q", "-m", "my own skill"]);
    let wt = dir.path().join("NOTE-1");
    git(
        &repo,
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            "otaq/NOTE-1",
            wt.to_str().unwrap(),
        ],
    );
    let old = wt.join(".agents/skills/otaq-custom-commit");
    std::fs::create_dir_all(&old).unwrap();
    std::fs::write(old.join("SKILL.md"), "old").unwrap();
    std::fs::write(old.join(".gitignore"), "*\n").unwrap();
    let source = my_commit(dir.path());
    let git_dir = files(&repo.join(".git"));

    let shipped = find_plugin_dir(Path::new("/nonexistent"))
        .unwrap()
        .join("skills");
    let written = write_session(
        &wt,
        Some(Source {
            own: "work-ticket".into(),
            folder: shipped.join("work-ticket"),
        }),
        vec![
            Source {
                own: "work-ticket".into(),
                folder: shipped.join("work-ticket"),
            },
            Source {
                own: "my-commit".into(),
                folder: source.clone(),
            },
        ],
        Marker::Worktree,
    );
    assert_eq!(written.failures, []);
    assert_eq!(written.runs.as_deref(), Some("work-ticket"));
    assert_eq!(written.names, ["otaq-work-ticket", "otaq-my-commit"]);
    for root in SKILL_ROOTS {
        let folder = wt.join(root).join("otaq-my-commit");
        assert_eq!(
            std::fs::read_to_string(folder.join("SKILL.md")).unwrap(),
            "---\r\nname: otaq-my-commit\r\ndescription: Commits in the team's style.\r\n---\r\nname: \
             stays\r\nRun `bash scripts/check-staged.sh`.\r\n",
            "the name set, the flag gone, the rest as it was"
        );
        assert_eq!(
            std::fs::read_to_string(folder.join("reference.md")).unwrap(),
            "notes: sort by date\n"
        );
        assert_eq!(
            std::fs::read_to_string(folder.join(".gitignore")).unwrap(),
            "*\n"
        );
        assert!(is_marked(&folder));
        assert_eq!(mode(&folder.join("scripts/check-staged.sh")), 0o755);
        assert!(
            std::fs::read_to_string(wt.join(root).join("otaq-work-ticket/SKILL.md"))
                .unwrap()
                .starts_with("---\nname: otaq-work-ticket\n")
        );
    }
    assert!(!old.exists(), "M27's copy is gone");
    assert!(
        wt.join(".claude/skills/otaq-mine/SKILL.md").is_file(),
        "the user's own skill stays"
    );
    assert_eq!(marked_copies(&wt), ["otaq-my-commit", "otaq-work-ticket"]);
    assert_eq!(files(&repo.join(".git")), git_dir, "nothing in .git");
    assert_eq!(
        git(&wt, &["status", "--porcelain", "--untracked-files=all"]),
        ""
    );
    git(&wt, &["add", "-A"]);
    assert_eq!(git(&wt, &["diff", "--cached", "--name-only"]), "");

    // A folder of the copy's name that isn't Otaq's is never replaced: that copy fails, the others are written.
    let theirs = wt.join(".agents/skills/otaq-push");
    std::fs::create_dir_all(&theirs).unwrap();
    std::fs::write(theirs.join("SKILL.md"), "theirs").unwrap();
    let written = write_session(
        &wt,
        None,
        vec![Source {
            own: "push".into(),
            folder: shipped.join("push"),
        }],
        Marker::Worktree,
    );
    assert_eq!(written.failures.len(), 1, "{:?}", written.failures);
    assert_eq!(
        std::fs::read_to_string(theirs.join("SKILL.md")).unwrap(),
        "theirs"
    );
    assert!(!wt.join(".claude/skills/otaq-push").exists());
}

/// AGM-07 on every tool: a Dev session finds its agent's `otaq-work-ticket` and `otaq-commit` in its worktree, once
/// in each root, with every hand-off skill (`otaq-address-review`, `otaq-restack`, the Delivery's `otaq-open-pr` and
/// `otaq-push`, d0118), and its first prompt runs the stage's skill as the tool names a project skill: Claude Code
/// `/otaq-work-ticket`, Codex `$otaq-work-ticket`, Copilot `-i /otaq-work-ticket`, OpenCode `--prompt
/// otaq-work-ticket`. No `--plugin-dir` carries skills (Claude Code's and Copilot's point at their hooks plugins),
/// OpenCode's config names no skills folder, and git sees nothing.
#[tokio::test(flavor = "multi_thread")]
async fn a_dev_session_on_each_tool_gets_its_agents_skills() {
    let World { w, _dir } = world().await;
    for (n, (tool, prompt)) in [
        (Tool::Claude, "/otaq-work-ticket"),
        (Tool::Codex, "$otaq-work-ticket"),
        (Tool::Copilot, "-i /otaq-work-ticket"),
        (Tool::OpenCode, "--prompt otaq-work-ticket"),
    ]
    .into_iter()
    .enumerate()
    {
        w.set_agent_model("Developer", AgentModel::Tool { tool, model: None })
            .await;
        let t = w.create("SHOP", &format!("Ticket {n}")).await;
        let out = started(&w, &t).await;
        let args = out.lines().find(|l| l.starts_with("ARGS")).unwrap();
        assert!(args.ends_with(prompt), "{tool:?}: {args}");
        assert!(
            !args.contains("/otaq:") && !args.contains("$otaq:"),
            "{args}"
        );
        // Its agent's own skills and every hand-off skill, from its first launch (d0118).
        assert!(out.contains("AGENTS otaq-address-review otaq-commit otaq-open-pr otaq-push otaq-restack otaq-work-ticket ."), "{tool:?}: {out}");
        assert!(out.contains("CLAUDE otaq-address-review otaq-commit otaq-open-pr otaq-push otaq-restack otaq-work-ticket ."), "{tool:?}: {out}");
        match tool {
            Tool::Claude | Tool::Copilot => {
                let plugin = args
                    .split_whitespace()
                    .skip_while(|a| *a != "--plugin-dir")
                    .nth(1)
                    .unwrap();
                assert!(
                    !Path::new(plugin).join("skills").exists(),
                    "{tool:?}: the hooks plugin carries no skills"
                );
            }
            _ => assert!(!args.contains("--plugin-dir"), "{args}"),
        }
        let worktree = worktree_of(&w.board.ticket(&t.key.to_string()).await.unwrap());
        for copy in [
            "otaq-work-ticket",
            "otaq-commit",
            "otaq-address-review",
            "otaq-restack",
            "otaq-open-pr",
            "otaq-push",
        ] {
            assert!(copied(&worktree, copy), "{tool:?}: {copy}");
        }
        assert_eq!(
            git(
                &worktree,
                &["status", "--porcelain", "--untracked-files=all"]
            ),
            ""
        );
        let config = out.lines().find(|l| l.starts_with("CONFIG")).unwrap();
        assert!(!config.contains("skills"), "{tool:?}: {config}");
    }
    w.stop().await;
}

/// An imported skill keeps its own folder's contents in its copy, by its own name; named `commit`, it is the
/// agent's commit skill and `prime` names it as each tool does.
#[tokio::test(flavor = "multi_thread")]
async fn an_imported_skill_keeps_its_own_files() {
    let World { w, _dir } = world().await;
    let folder = my_commit(w.data.root());
    let skill = folder.join("SKILL.md");
    let text = std::fs::read_to_string(&skill).unwrap();
    std::fs::write(&skill, text.replacen("name: my-commit", "name: commit", 1)).unwrap();
    w.import_step_skill(WorkflowStep::Commit, &folder).await;
    let t = w.create("SHOP", "Sort notes by date").await;
    let out = started(&w, &t).await;
    assert!(out.contains("AGENTS otaq-address-review otaq-commit otaq-open-pr otaq-push otaq-restack otaq-work-ticket ."), "{out}");
    let worktree = worktree_of(&w.board.ticket("SHOP-1").await.unwrap());
    for root in SKILL_ROOTS {
        let copy = worktree.join(root).join("otaq-commit");
        assert_eq!(
            std::fs::read_to_string(copy.join("reference.md")).unwrap(),
            "notes: sort by date\n"
        );
        assert_eq!(mode(&copy.join("scripts/check-staged.sh")), 0o755);
    }
    for (tool, named) in [
        (Tool::Claude, "/otaq-commit"),
        (Tool::Codex, "$otaq-commit"),
        (Tool::Copilot, "/otaq-commit"),
        (Tool::OpenCode, "otaq-commit"),
    ] {
        let primed = w
            .board
            .prime_context("SHOP-1", w.adapters.get(tool))
            .await
            .unwrap();
        assert_eq!(primed.commit_skill, named, "{tool:?}");
    }
    w.stop().await;
}

/// The test flow's Grill (Q261): the ticket's first session runs Grill's own skill, `/otaq-grill`, with Griller's
/// skills copied (Otaq's shipped `grill`).
#[tokio::test(flavor = "multi_thread")]
async fn the_grill_stage_starts_with_otaq_grill() {
    let World { w, _dir } = world().await;
    w.board.use_flow_for_tests("SHOP", &every_kind_file()).await;
    let t = w.create("SHOP", "Coupon codes").await;
    let out = started(&w, &t).await;
    let args = out.lines().find(|l| l.starts_with("ARGS")).unwrap();
    assert!(args.ends_with("/otaq-grill"), "{args}");
    assert!(
        out.contains("AGENTS otaq-address-review otaq-grill otaq-open-pr otaq-push otaq-restack ."),
        "{out}"
    );
    w.stop().await;
}

/// The validator: its check's own skill and its agent's skills are copied before its launch and its first prompt
/// runs `/otaq-validate`. With Validator holding the imported `team-validate` in place of `validate` and the flow's
/// Validate naming it (`skill: team-validate`, Q296), it runs `/otaq-team-validate` and `otaq-validate` isn't
/// copied: the agent's skills are. Only ignored paths change, so its verdict counts (VAL-02, d0063).
#[tokio::test(flavor = "multi_thread")]
async fn the_validator_runs_its_checks_skill_and_stays_read_only() {
    let World { w, _dir } = world().await;
    for (n, imported) in [false, true].into_iter().enumerate() {
        if imported {
            let folder = skill_folder(w.data.root(), "team-validate");
            w.import_step_skill(WorkflowStep::Validate, &folder).await;
        }
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
        let name = if imported {
            "otaq-team-validate"
        } else {
            "otaq-validate"
        };
        let args = out.lines().find(|l| l.starts_with("ARGS")).unwrap();
        assert!(args.ends_with(&format!("/{name}")), "{args}");
        assert!(out.contains(&format!("AGENTS {name} .")), "{out}");
        assert!(copied(&worktree, name));
        if imported {
            assert!(!worktree.join(".agents/skills/otaq-validate").exists());
            assert!(!worktree.join(".claude/skills/otaq-validate").exists());
        }
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

/// The copies in a worktree are no uncommitted files: the submit and Local merge go through (REV-07, REV-08).
#[tokio::test(flavor = "multi_thread")]
async fn submit_and_local_merge_ignore_the_copies() {
    let World { w, _dir } = world().await;
    let t = w.create("SHOP", "Login redirect").await;
    let worktree = w.start_worktree(&t).await;
    commit(&worktree, "login.ts", "redirect\n");
    let developer = w
        .board
        .store()
        .agent_named("Developer")
        .await
        .unwrap()
        .unwrap();
    let written = w
        .board
        .copy_skills(
            &worktree,
            w.adapters.shipped_skills(),
            Some(&developer),
            Some("validate"),
            Marker::Worktree,
        )
        .await;
    assert_eq!(written.failures, []);
    assert_eq!(
        written.names,
        ["otaq-validate", "otaq-work-ticket", "otaq-commit"]
    );
    w.force_status(&t, Status::Working).await;
    w.board
        .submit_for_review(
            "SHOP-1",
            said(&w.board, "SHOP-1", "Redirects after login.")
                .await
                .as_ref(),
            &me(),
        )
        .await
        .expect("nothing uncommitted");
    let merged = w
        .review
        .local_merge("SHOP-1", "SHOP-1: Login redirect", false, &me())
        .await
        .unwrap();
    assert!(
        matches!(merged, LocalMergeOutcome::Merged { .. }),
        "{merged:?}"
    );
    w.stop().await;
}

/// Open PR typed into a live session that didn't start with the skill (an `sh` Otaq didn't launch): the Delivery
/// stage's agent's Open PR skill is copied first and named by its file, never as a command (d0118); Push to PR
/// likewise. The flow shows the imported skill's own name (WSK-08).
#[tokio::test(flavor = "multi_thread")]
async fn open_pr_and_push_name_the_copy_in_a_session_that_didnt_start_with_it() {
    let World { w, _dir } = world().await;
    let folder = skill_folder(w.data.root(), "my-pr");
    w.import_step_skill(WorkflowStep::OpenPr, &folder).await;
    let (t, worktree) = in_review(&w, "Login redirect").await;
    let agent = w.agent_shell(&t).await;
    let outcome = w.prs.open("SHOP-1", false, false, &me()).await.unwrap();
    assert!(matches!(outcome, PrOutcome::Started { .. }), "{outcome:?}");
    let root = w.adapters.of_session(None).skill_root();
    w.wait_for_output(&agent.session, &handoff_sentence(root, "my-pr", None))
        .await;
    assert!(copied(&worktree, "otaq-my-pr"));
    assert_eq!(w.board.pr_flow_view("SHOP-1").await.unwrap().skill, "my-pr");
    w.prs.cancel("SHOP-1", &me()).await.unwrap();

    sqlx::query("UPDATE tickets SET pr_number = 481 WHERE id = ?")
        .bind(&t.id)
        .execute(w.board.store().pool())
        .await
        .unwrap();
    let outcome = w.prs.open("SHOP-1", true, false, &me()).await.unwrap();
    assert!(matches!(outcome, PrOutcome::Started { .. }), "{outcome:?}");
    w.wait_for_output(&agent.session, &handoff_sentence(root, "push", None))
        .await;
    assert!(copied(&worktree, "otaq-push"));
    assert_eq!(w.board.pr_flow_view("SHOP-1").await.unwrap().skill, "push");
    w.stop().await;
}

/// Open PR with no live session resumes the conversation: the copy is in the worktree at the launch, and the first
/// prompt names it.
#[tokio::test(flavor = "multi_thread")]
async fn open_pr_resumes_with_the_copy_in_place() {
    let World { w, _dir } = world().await;
    let folder = skill_folder(w.data.root(), "my-pr");
    w.import_step_skill(WorkflowStep::OpenPr, &folder).await;
    let (t, worktree) = in_review(&w, "Login redirect").await;
    w.record_ended_agent(&t, "3be94c10-0000-7000-8000-000000000000")
        .await;
    let outcome = w.prs.open("SHOP-1", true, false, &me()).await.unwrap();
    assert!(matches!(outcome, PrOutcome::Started { .. }), "{outcome:?}");
    let session = w
        .sessions
        .running_of(&t.id, SessionKind::Agent)
        .expect("resumed");
    let out = w.wait_for_output(&session.id, "CLAUDE").await;
    let args = out.lines().find(|l| l.starts_with("ARGS")).unwrap();
    assert!(args.ends_with("/otaq-my-pr"), "{args}");
    assert!(out.contains("otaq-my-pr"), "{out}");
    assert!(copied(&worktree, "otaq-my-pr"));
    w.stop().await;
}

/// AGM-07: no code in the core names an M27 copy (`otaq-custom-<step>`) or a skill through a plugin's namespace
/// (`/otaq:commit`, `$otaq:commit`): every skill is a copy named `otaq-<name>`. Only the writer knows M27's names, to
/// remove them; Copilot's saved events file of 2026-10-17 still holds the line typed then.
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
            .into_owned();
        let text = std::fs::read_to_string(&file).unwrap();
        if rel != "skills/copy.rs" {
            assert!(!text.contains("}-custom-"), "{rel} names an M27 copy");
            assert!(!text.contains("custom_skill"), "{rel}");
        }
        if rel != "agent_tools/copilot/events_file.rs" {
            for namespaced in ["\"/{}:", "\"${}:", "/{NS}:", "${NS}:"] {
                assert!(!text.contains(namespaced), "{rel}: {namespaced}");
            }
        }
    }
}
