//! The uninstall's removal (T-0555, UNI-05; d0134, d0087, d0098, Q564) against temporary repositories and a temporary
//! data folder: refused before anything changes while worktrees weren't confirmed; once confirmed, every session stops
//! and none can start, each ticket's resource Remove runs (a failure is reported, not fatal), every worktree and the
//! merge worktree go with `git worktree remove --force` (a dirty one too), a worktree whose repository is gone and a
//! folder git doesn't know are left for the data folder, Otaq's marked skill copies go and the user's folders stay,
//! and every `otaq/` branch keeps its commits.

use std::path::{Path, PathBuf};

use crate::common::resources::by_os;
use otaq_core::agent_tools::LaunchContext;
use otaq_core::agent_tools::claude::ClaudeLaunch;
use otaq_core::board::Refusal;
use otaq_core::board::settings::ProjectSettings;
use otaq_core::domain::Status;
use otaq_core::git::testing::{commit, git};
use otaq_core::service::ServiceConfig;
use otaq_core::service::testing::TestService;
use otaq_core::session::SessionKind;
use otaq_core::skills::copy::{Marker, write_copy};
use otaq_core::uninstall::{NOT_CONFIRMED, StepKind, UninstallOutcome};
use tempfile::TempDir;

/// Every file and folder under `dir` with its size and modification time (symlinks not followed).
fn tree(dir: &Path) -> Vec<(String, u64, Option<std::time::SystemTime>)> {
    let mut all = Vec::new();
    let mut stack = vec![dir.to_owned()];
    while let Some(at) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&at) else {
            continue;
        };
        for entry in entries.flatten() {
            let meta = entry.path().symlink_metadata().unwrap();
            if meta.is_dir() {
                stack.push(entry.path());
            }
            all.push((
                entry.path().display().to_string(),
                meta.len(),
                meta.modified().ok(),
            ));
        }
    }
    all.sort();
    all
}

/// Agents and the Noyan stand-in are `sh` sleeping; never a real agent tool.
fn sh_launch(config: ServiceConfig) -> ServiceConfig {
    ServiceConfig {
        launch: Some(LaunchContext {
            bin_dir: "/nonexistent".into(),
            plugin_dir: "/nonexistent/plugin".into(),
            path: std::env::var_os("PATH").unwrap_or_default(),
            claude: ClaudeLaunch {
                program: vec![
                    "sh".into(),
                    "-c".into(),
                    "exec sleep 60".into(),
                    "sh".into(),
                ],
                config_dir: None,
            },
        }),
        ..config
    }
}

/// The folder's name of each step of `kind` in `steps`, with its message.
fn names(
    steps: &[otaq_core::uninstall::UninstallStep],
    kind: StepKind,
) -> Vec<(String, Option<String>)> {
    let mut names: Vec<(String, Option<String>)> = steps
        .iter()
        .filter(|s| s.kind == kind)
        .map(|s| {
            let name = Path::new(&s.path)
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| s.path.clone());
            (name, s.message.clone())
        })
        .collect();
    names.sort();
    names
}

fn worktree_list(repo: &Path) -> Vec<PathBuf> {
    git(repo, &["worktree", "list", "--porcelain"])
        .lines()
        .filter_map(|l| l.strip_prefix("worktree "))
        .map(PathBuf::from)
        .collect()
}

#[tokio::test(flavor = "multi_thread")]
async fn confirmed_removal_takes_the_worktrees_and_copies_and_keeps_branches() {
    let w = TestService::start_with(sh_launch).await;
    let shop = w.add_project("shop-web").await;
    let pay = w.add_project("pay-api").await;
    let repo = PathBuf::from(&shop.repo_path);
    let removed_log = w.home.path().join("removed.txt");
    w.board
        .set_project_settings(
            "SHOP",
            &ProjectSettings {
                // Writes which ticket it removed; SHOP-12's Remove fails.
                resource_remove: by_os(
                    &format!(
                        "echo \"$OTAQ_TICKET\" >> '{log}'; test \"$OTAQ_TICKET\" != SHOP-12",
                        log = removed_log.display()
                    ),
                    &format!(
                        "echo %OTAQ_TICKET%>> \"{log}\" & if \"%OTAQ_TICKET%\"==\"SHOP-12\" exit 1",
                        log = removed_log.display()
                    ),
                ),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    let mut tickets = Vec::new();
    for n in 1..=14 {
        tickets.push(w.create("SHOP", &format!("ticket {n}")).await);
    }
    let (shop_12, shop_14) = (&tickets[11], &tickets[13]);
    let wt_12 = w.start_worktree(shop_12).await;
    commit(&wt_12, "done.rs", "fn done() {}\n");
    w.force_status(shop_12, Status::WaitingForYou).await;
    let wt_14 = w.start_worktree(shop_14).await;
    commit(&wt_14, "app.rs", "fn app() {}\n");
    w.force_status(shop_14, Status::Working).await;
    std::fs::write(wt_14.join("app.rs"), "fn app() { todo!() }\n").unwrap();
    std::fs::write(wt_14.join("new.rs"), "fn new() {}\n").unwrap();
    for (t, name) in [(shop_12, "shop_12"), (shop_14, "shop_14")] {
        sqlx::query(
            "UPDATE tickets SET resource_name = ?, resource_state = 'created' WHERE id = ?",
        )
        .bind(name)
        .bind(&t.id)
        .execute(w.board.store().pool())
        .await
        .unwrap();
    }
    let agent = w.agent_shell(shop_14).await;
    let noyan = w.shell(shop_12, SessionKind::ProjectAgent).await;
    // A folder git doesn't know, PAY-1 of a repository deleted since, and the merge worktree.
    let stray = w.data.worktrees_dir().join("shop-web").join("notes");
    std::fs::create_dir_all(&stray).unwrap();
    std::fs::write(stray.join("todo.txt"), "x\n").unwrap();
    let pay_1 = w.create(&pay.key, "Refunds").await;
    let wt_pay = w.start_worktree(&pay_1).await;
    std::fs::remove_dir_all(&pay.repo_path).unwrap();
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
    // Otaq's marked copy in the checkout, and the user's own folders (one named like a copy, without the marker).
    let source = TempDir::new().unwrap();
    std::fs::write(source.path().join("SKILL.md"), "---\nname: noyan\n---\n").unwrap();
    write_copy(&repo, "otaq-noyan", source.path(), Marker::Checkout).unwrap();
    for own in ["my-skill", "otaq-x"] {
        let folder = repo.join(".claude/skills").join(own);
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join("SKILL.md"), "mine\n").unwrap();
    }
    let heads = |branches: &[&str]| -> Vec<String> {
        branches
            .iter()
            .map(|b| git(&repo, &["rev-parse", b]))
            .collect()
    };
    let before_heads = heads(&["otaq/SHOP-12", "otaq/SHOP-14"]);

    // Not confirmed: refused, and nothing changed.
    let before = (tree(w.data.root()), tree(&repo));
    let err = w.uninstall.remove(false).await.unwrap_err();
    assert_eq!(err.refusal(), Some(Refusal::NeedsConfirmation));
    assert_eq!(err.to_string(), NOT_CONFIRMED);
    assert!(w.sessions.info(&agent.session).unwrap().is_running());
    assert!(!w.sessions.is_closed());
    let after = (tree(w.data.root()), tree(&repo));
    // The database and logs change on their own; the worktrees, the checkout and its `.git` don't.
    let skip_own = |t: &[(String, u64, Option<std::time::SystemTime>)]| -> Vec<(String, u64, Option<std::time::SystemTime>)> {
        t.iter()
            .filter(|(p, ..)| !p.contains("otaq.db") && !p.contains("/logs"))
            .cloned()
            .collect()
    };
    assert_eq!(skip_own(&after.0), skip_own(&before.0));
    assert_eq!(after.1, before.1);

    // Confirmed.
    let outcome: UninstallOutcome = w.uninstall.remove(true).await.unwrap();

    // Sessions stopped, and nothing starts again: no session, no worktree.
    assert!(!w.sessions.info(&agent.session).unwrap().is_running());
    assert!(!w.sessions.info(&noyan.session).unwrap().is_running());
    assert!(w.sessions.is_closed());
    let err = otaq_core::start::prepare_worktree(&w.board, &w.data, "SHOP-3")
        .await
        .unwrap_err();
    assert!(err.to_string().contains("is being uninstalled"), "{err}");

    // Resources: SHOP-12's Remove failed and was reported; SHOP-14's ran; both were tried.
    assert_eq!(
        names(&outcome.removed, StepKind::Resources),
        [("shop_14".into(), None)]
    );
    let failed = names(&outcome.failed, StepKind::Resources);
    assert_eq!(failed.len(), 1);
    assert_eq!(failed[0].0, "shop_12");
    let mut tried: Vec<String> = std::fs::read_to_string(&removed_log)
        .unwrap()
        .lines()
        .map(|l| l.trim().to_owned())
        .collect();
    tried.sort();
    assert_eq!(tried, ["SHOP-12", "SHOP-14"]);

    // Worktrees: SHOP-12's and SHOP-14's (dirty) went through git; PAY-1 and the unknown folder are left.
    assert_eq!(
        names(&outcome.removed, StepKind::Worktree),
        [("SHOP-12".into(), None), ("SHOP-14".into(), None)]
    );
    assert_eq!(
        names(&outcome.skipped, StepKind::Worktree),
        [
            (
                "PAY-1".into(),
                Some("its repository is gone; it goes with the data folder".into())
            ),
            (
                "notes".into(),
                Some("git doesn't know it as a worktree; it goes with the data folder".into())
            ),
        ]
    );
    assert_eq!(
        names(&outcome.removed, StepKind::MergeWorktree),
        [("shop-web".into(), None)]
    );
    assert!(!wt_12.exists() && !wt_14.exists() && !scratch.exists());
    assert!(
        wt_pay.exists() && stray.exists(),
        "left for the data folder"
    );
    assert_eq!(
        worktree_list(&repo)
            .iter()
            .map(|p| otaq_core::paths::canonical(p).unwrap())
            .collect::<Vec<_>>(),
        [otaq_core::paths::canonical(&repo).unwrap()]
    );

    // Branches stay with their commits.
    assert_eq!(heads(&["otaq/SHOP-12", "otaq/SHOP-14"]), before_heads);

    // The marked copy went from both roots; the user's folders stay.
    assert_eq!(
        names(&outcome.removed, StepKind::SkillCopy),
        [("otaq-noyan".into(), None)]
    );
    assert!(!repo.join(".claude/skills/otaq-noyan").exists());
    assert!(!repo.join(".agents/skills/otaq-noyan").exists());
    assert!(repo.join(".claude/skills/my-skill/SKILL.md").is_file());
    assert!(repo.join(".claude/skills/otaq-x/SKILL.md").is_file());
    assert!(outcome.failed.iter().all(|s| s.kind == StepKind::Resources));
}

/// Without worktrees, no question is needed: a clean merge worktree goes, nothing is refused.
#[tokio::test(flavor = "multi_thread")]
async fn a_clean_merge_worktree_needs_no_confirmation() {
    let w = TestService::start().await;
    let shop = w.add_project("shop-web").await;
    let repo = PathBuf::from(&shop.repo_path);
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
    let outcome = w.uninstall.remove(false).await.unwrap();
    assert_eq!(
        names(&outcome.removed, StepKind::MergeWorktree),
        [("shop-web".into(), None)]
    );
    assert!(outcome.failed.is_empty() && outcome.skipped.is_empty());
    assert_eq!(worktree_list(&repo).len(), 1);
}

/// A ✎ merge worktree (a crash during a merge) needs the worktrees' confirmation like a ticket's.
#[tokio::test(flavor = "multi_thread")]
async fn a_dirty_merge_worktree_needs_the_confirmation() {
    let w = TestService::start().await;
    let shop = w.add_project("shop-web").await;
    let repo = PathBuf::from(&shop.repo_path);
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
    std::fs::write(scratch.join("conflict.txt"), "<<<<<<<\n").unwrap();
    let err = w.uninstall.remove(false).await.unwrap_err();
    assert_eq!(err.refusal(), Some(Refusal::NeedsConfirmation));
    assert!(scratch.join("conflict.txt").exists());
    let outcome = w.uninstall.remove(true).await.unwrap();
    assert_eq!(
        names(&outcome.removed, StepKind::MergeWorktree),
        [("shop-web".into(), None)]
    );
    assert!(!scratch.exists());
}
