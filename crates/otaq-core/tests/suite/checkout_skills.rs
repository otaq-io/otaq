//! Skills in the checkout (T-0408; AGM-07; d0108, d0098's copy and marker, an exception to d0023): while a Project
//! agent runs in the project's own folder, its agent's skills are copied there as `otaq-<name>` with
//! Otaq's marker and a `.gitignore` holding `*`, and removed when it ends (unless another running one holds the same
//! copy); copies a crash left are removed at service start; `git status` stays clean throughout and nothing else in
//! the repository changes. Every repository is a temporary one, `claude` is `sh` printing its arguments, and the
//! plugin folder is a temporary one holding the repository's shipped skills: no test starts claude.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use otaq_core::agent_tools::LaunchContext;
use otaq_core::agent_tools::claude::{ClaudeLaunch, find_plugin_dir};
use otaq_core::git::testing::git;
use otaq_core::service::ServiceConfig;
use otaq_core::service::testing::TestService;
use otaq_core::skills::copy::{Marker, SKILL_ROOTS, is_marked, marked_copies, write_copy};
use tempfile::TempDir;

/// Prints its arguments and the copies in its folder, then echoes what is typed.
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

/// Every file of the checkout outside Otaq's copies, with its contents (`.git` included): what must not change.
fn files_but_copies(repo: &Path) -> BTreeMap<String, Vec<u8>> {
    otaq_core::git::testing::snapshot(repo)
        .into_iter()
        .filter(|rel| !rel.contains("/otaq-") && repo.join(rel).is_file())
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

fn copied(repo: &Path, name: &str) -> bool {
    SKILL_ROOTS.iter().all(|root| {
        let copy = repo.join(root).join(name);
        copy.join("SKILL.md").is_file() && is_marked(&copy)
    })
}

/// Waits until `repo` holds exactly the marked copies `names` (their removal follows the session's end).
async fn until_copies(repo: &Path, names: &[&str]) {
    let reached = tokio::time::timeout(Duration::from_secs(20), async {
        while marked_copies(repo) != names {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await;
    assert!(
        reached.is_ok(),
        "{} holds {:?}, not {names:?}",
        repo.display(),
        marked_copies(repo)
    );
}

/// Waits until `path` no longer exists.
async fn until_gone(path: &Path) {
    let gone = tokio::time::timeout(Duration::from_secs(20), async {
        while path.exists() {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await;
    assert!(gone.is_ok(), "{} is still there", path.display());
}

/// A Project agent: its agent's skills (`otaq-noyan`, `otaq-flow`) are in the checkout before it starts and
/// its first prompt runs `otaq-noyan`; a second agent shares them; the copies stay while either runs and go
/// when the last ends, with the folders they made; `git status` is clean throughout and nothing else in the checkout or its `.git` changes.
#[tokio::test(flavor = "multi_thread")]
async fn a_project_agents_skills_stay_while_it_runs() {
    let dir = TempDir::new().unwrap();
    let w = TestService::start_with(launch(dir.path())).await;
    let project = w.add_project("shop-web").await;
    let repo = PathBuf::from(&project.repo_path);
    let before = files_but_copies(&repo);

    let first = w
        .project_agents
        .open("SHOP", "Why is checkout slow?")
        .await
        .unwrap();
    let session = first.session.as_ref().unwrap().id.clone();
    let out = w.wait_for_output(&session, "CLAUDE").await;
    assert!(out.contains("/otaq-noyan Why is checkout slow?"), "{out}");
    // Noyan's skills: `noyan` and `flow` (FLW-11).
    assert!(out.contains("CLAUDE otaq-flow otaq-noyan ."), "{out}");
    assert!(copied(&repo, "otaq-flow"));
    assert!(copied(&repo, "otaq-noyan"));
    assert!(clean(&repo));

    let second = w
        .project_agents
        .open("SHOP", "List the open bugs")
        .await
        .unwrap();
    w.project_agents.close(&first.id, true).await.unwrap();
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(copied(&repo, "otaq-noyan"), "the second agent holds it");
    assert!(clean(&repo));

    w.project_agents.close(&second.id, true).await.unwrap();
    until_copies(&repo, &[]).await;
    // The folders go right after the last copy (two copies now: `otaq-flow` too).
    until_gone(&repo.join(".agents")).await;
    until_gone(&repo.join(".claude")).await;
    assert!(clean(&repo));
    assert_eq!(files_but_copies(&repo), before, "nothing else changed");

    // Resume writes it again.
    w.project_agents.resume(&second.id).await.unwrap();
    until_copies(&repo, &["otaq-flow", "otaq-noyan"]).await;
    assert!(clean(&repo));
    w.stop().await;
}

/// A crash leaves copies behind: the next service start removes every marked one, and leaves the user's own
/// `otaq-mine` (no marker) alone.
#[tokio::test(flavor = "multi_thread")]
async fn a_crash_leftover_goes_at_service_start() {
    let dir = TempDir::new().unwrap();
    let w = TestService::start_with(launch(dir.path())).await;
    let project = w.add_project("shop-web").await;
    let repo = PathBuf::from(&project.repo_path);
    let mine = repo.join(".claude/skills/otaq-mine");
    std::fs::create_dir_all(&mine).unwrap();
    std::fs::write(mine.join("SKILL.md"), "---\nname: otaq-mine\n---\nMine.\n").unwrap();
    git(&repo, &["add", "-A"]);
    git(&repo, &["commit", "-q", "-m", "my own skill"]);
    let source = dir.path().join("plugin/skills/noyan");
    write_copy(&repo, "otaq-noyan", &source, Marker::Checkout).unwrap();
    assert!(clean(&repo));
    assert_eq!(marked_copies(&repo), ["otaq-noyan"]);

    let w = w.restart().await;
    assert_eq!(marked_copies(&repo), Vec::<String>::new());
    assert!(!repo.join(".agents").exists());
    assert!(
        mine.join("SKILL.md").is_file(),
        "the user's own skill stays"
    );
    assert!(clean(&repo));
    w.stop().await;
}
