//! Skills in the checkout (T2): a Project agent runs in the project's own folder, and Otaq never writes skills into it.
//! Its tool loads Otaq's own skills from Otaq's plugin, and its first prompt names Otaq's `noyan` by the plugin's
//! namespace. Copies an older Otaq left (marked with Otaq's marker, and M27's `otaq-custom-*`) are removed at service
//! start from every checkout and worktree; a user's own skill folder is never touched. Every repository is a temporary one, `claude` is `sh` printing its arguments, and the
//! plugin folder is a temporary one holding the repository's shipped skills: no test starts claude.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use otaq_core::agent_tools::LaunchContext;
use otaq_core::agent_tools::claude::{ClaudeLaunch, find_plugin_dir};
use otaq_core::git::testing::git;
use otaq_core::service::ServiceConfig;
use otaq_core::service::testing::TestService;
use otaq_core::skills::copy::marked_copies;
use otaq_core::skills::testing::{claude_skill, leftover_copy};
use tempfile::TempDir;

/// Prints its arguments and the skills in its folder, then echoes what is typed.
const SCRIPT: &str = "printf 'ARGS %s\\n' \"$*\"; \
                      printf 'CLAUDE %s.\\n' \"$(ls .claude/skills 2>/dev/null | tr '\\n' ' ')\"; exec cat";

/// A plugin folder under `root` with Claude Code's hooks plugin and the repository's shipped skills.
fn plugin(root: &Path) -> PathBuf {
    let plugin = root.join("plugin");
    let manifest = plugin.join("claude").join(".claude-plugin");
    std::fs::create_dir_all(&manifest).unwrap();
    std::fs::write(manifest.join("plugin.json"), "{}").unwrap();
    let skills = find_plugin_dir(Path::new("/nonexistent"))
        .unwrap()
        .join("skills");
    for name in ["noyan", "flow"] {
        let to = plugin.join("skills").join(name);
        std::fs::create_dir_all(&to).unwrap();
        std::fs::copy(skills.join(name).join("SKILL.md"), to.join("SKILL.md")).unwrap();
    }
    plugin
}

fn launch(dir: &Path) -> impl FnOnce(ServiceConfig) -> ServiceConfig {
    let plugin_dir = plugin(dir);
    move |config| ServiceConfig {
        launch: Some(LaunchContext {
            bin_dir: "/nonexistent".into(),
            plugin_dir,
            path: std::env::var_os("PATH").unwrap_or_default(),
            claude: ClaudeLaunch {
                program: vec!["sh".into(), "-c".into(), SCRIPT.into(), "sh".into()],
                config_dir: None,
            },
        }),
        ..config
    }
}

/// Every file of the checkout with its contents (`.git` included, but its index): what must not change.
fn files(repo: &Path) -> BTreeMap<String, Vec<u8>> {
    otaq_core::git::testing::snapshot(repo)
        .into_iter()
        .filter(|rel| repo.join(rel).is_file())
        .filter(|rel| !rel.starts_with(".git/") || !rel.contains("index"))
        .map(|rel| {
            let contents = std::fs::read(repo.join(&rel)).unwrap();
            (rel, contents)
        })
        .collect()
}

fn clean(repo: &Path) -> bool {
    git(repo, &["status", "--porcelain", "--untracked-files=all"]).is_empty()
}

/// A Project agent starts and resumes with nothing written into the checkout: no `.agents` or `.claude` folder, no
/// other file changed, `git status` clean; its first prompt runs Otaq's `noyan` from the plugin.
#[tokio::test(flavor = "multi_thread")]
async fn a_project_agent_writes_nothing_into_the_checkout() {
    let dir = TempDir::new().unwrap();
    let w = TestService::start_with(launch(dir.path())).await;
    let project = w.add_project("shop-web").await;
    let repo = PathBuf::from(&project.repo_path);
    let before = files(&repo);

    let agent = w
        .project_agents
        .open("SHOP", "Why is checkout slow?")
        .await
        .unwrap();
    let session = agent.session.as_ref().unwrap().id.clone();
    let out = w.wait_for_output(&session, "CLAUDE").await;
    let typed = format!("{} Why is checkout slow?", claude_skill("noyan"));
    assert!(out.contains(&typed), "{out}");
    assert!(out.contains("CLAUDE ."), "no skill in the checkout: {out}");
    assert!(!repo.join(".agents").exists());
    assert!(!repo.join(".claude").exists());
    assert!(clean(&repo));
    assert_eq!(files(&repo), before, "nothing changed");

    w.project_agents.close(&agent.id, true).await.unwrap();
    w.project_agents.resume(&agent.id).await.unwrap();
    assert!(!repo.join(".agents").exists());
    assert!(!repo.join(".claude").exists());
    assert_eq!(files(&repo), before, "a resume writes nothing either");
    w.stop().await;
}

/// What an older Otaq left in `root` (two marked copies and M27's `otaq-custom-commit`) beside the user's own: an
/// unmarked `otaq-mine` and Claude Code's settings. Returns the user's skill folder.
fn left_beside_mine(root: &Path) -> PathBuf {
    let cmd = otaq_core::naming::COMMAND_NAME;
    leftover_copy(root, &format!("{cmd}-noyan"));
    leftover_copy(root, &format!("{cmd}-commit"));
    let old = root.join(format!(".claude/skills/{cmd}-custom-commit"));
    std::fs::create_dir_all(&old).unwrap();
    std::fs::write(old.join("SKILL.md"), "old\n").unwrap();
    let mine = root.join(format!(".claude/skills/{cmd}-mine"));
    std::fs::create_dir_all(&mine).unwrap();
    std::fs::write(mine.join("SKILL.md"), "---\nname: mine\n---\nMine.\n").unwrap();
    std::fs::write(root.join(".claude/settings.json"), "{}\n").unwrap();
    assert_eq!(marked_copies(root).len(), 2);
    mine
}

/// Only the user's own folders are left in `root`: its skill and its settings.
fn only_mine_left(root: &Path, mine: &Path) {
    let cmd = otaq_core::naming::COMMAND_NAME;
    assert_eq!(
        marked_copies(root),
        Vec::<String>::new(),
        "{}",
        root.display()
    );
    assert!(!root.join(".agents").exists(), "{}", root.display());
    assert!(
        !root
            .join(format!(".claude/skills/{cmd}-custom-commit"))
            .exists()
    );
    assert!(
        mine.join("SKILL.md").is_file(),
        "the user's own skill stays"
    );
    assert!(
        root.join(".claude/settings.json").is_file(),
        "the user's other files stay"
    );
    let left: Vec<String> = std::fs::read_dir(root.join(".claude/skills"))
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(left, [format!("{cmd}-mine")]);
}

/// Copies an older Otaq left behind: the next service start removes every marked one and M27's `otaq-custom-*` from
/// the project's checkout, a ticket's worktree and the merge worktree, and leaves the user's own `otaq-mine` (no
/// marker) and other files alone.
#[tokio::test(flavor = "multi_thread")]
async fn leftover_copies_go_at_service_start() {
    let dir = TempDir::new().unwrap();
    let w = TestService::start_with(launch(dir.path())).await;
    let project = w.add_project("shop-web").await;
    let repo = PathBuf::from(&project.repo_path);
    let ticket = w.create("SHOP", "Coupons").await;
    let worktree = w.start_worktree(&ticket).await;
    let scratch = w.data.scratch_path("shop-web").unwrap();
    git(
        &repo,
        &[
            "worktree",
            "add",
            "-q",
            "--detach",
            &scratch.display().to_string(),
        ],
    );
    let mine = [&repo, &worktree, &scratch].map(|root| left_beside_mine(root));

    let w = w.restart().await;
    for (root, mine) in [&repo, &worktree, &scratch].into_iter().zip(&mine) {
        only_mine_left(root, mine);
    }
    w.stop().await;
}
